#!/usr/bin/env python3
"""Extract a reloc-free ELF .text section from libkolibri_utils.a for FASM `file`.

Usage:
  extract_reloc_free_text.py --archive libkolibri_utils.a \\
      --section .text.rust_crc_32 --symbol rust_crc_32 --out rust_crc_32.bin

Guarantees (else exit nonzero — never silently emit bad machine code):
  * ELF32 / EM_386 only
  * exact section name match (no symbol-based fallback to another section)
  * reject duplicate section names in one object
  * section must be SHT_PROGBITS
  * reject any REL/RELA targeting the section (size > 0)
  * named symbol must exist in that section at offset 0 (not SHN_UNDEF)
  * reject symbol size smaller than section (would leave trailing unrelated bytes)
  * optional --expect-ret-imm checks stdcall epilogue bytes are present

Output is a raw byte-identical dump of the section contents (deterministic
given the same archive member bytes).
"""

from __future__ import annotations

import argparse
import os
import struct
import sys


def parse_archive(path: str) -> list[tuple[str, bytes]]:
    with open(path, "rb") as f:
        magic = f.read(8)
        if magic != b"!<arch>\n":
            raise SystemExit(f"not an archive: {path}")
        string_table = b""
        members: list[tuple[str, bytes]] = []
        while True:
            hdr = f.read(60)
            if len(hdr) < 60:
                break
            name = hdr[0:16].decode("ascii")
            size = int(hdr[48:58].decode("ascii").strip())
            data = f.read(size)
            if size % 2:
                f.read(1)
            n = name.strip()
            if n == "//":
                string_table = data
                continue
            if n.startswith("/") and n[1:].isdigit():
                idx = int(n[1:])
                end = string_table.find(b"\n", idx)
                n = string_table[idx:end].decode("ascii").rstrip("/")
            members.append((n, data))
        return members


def elf_sections(data: bytes):
    if data[:4] != b"\x7fELF":
        return None
    if data[4] != 1:
        raise SystemExit("only ELF32 supported")
    (
        _e_type,
        e_machine,
        _e_version,
        _e_entry,
        _e_phoff,
        e_shoff,
        _e_flags,
        _e_ehsize,
        _e_phentsize,
        _e_phnum,
        e_shentsize,
        e_shnum,
        e_shstrndx,
    ) = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    if e_machine != 3:
        raise SystemExit(f"unexpected e_machine={e_machine} (want EM_386=3)")
    sh = [
        struct.unpack_from("<IIIIIIIIII", data, e_shoff + i * e_shentsize)
        for i in range(e_shnum)
    ]
    strtab = data[sh[e_shstrndx][4] : sh[e_shstrndx][4] + sh[e_shstrndx][5]]

    def sname(i: int) -> str:
        n = sh[i][0]
        return strtab[n : strtab.find(b"\x00", n)].decode()

    return sh, sname


def find_section_blob(
    data: bytes, section: str, symbol: str | None
) -> bytes | None:
    parsed = elf_sections(data)
    if parsed is None:
        return None
    sh, sname = parsed
    SHT_SYMTAB = 2
    SHT_REL = 9
    SHT_RELA = 4
    SHN_UNDEF = 0
    STT_FUNC = 2
    STT_NOTYPE = 0

    text_idx = None
    for i, s in enumerate(sh):
        if sname(i) == section:
            if text_idx is not None:
                raise SystemExit(
                    f"duplicate section name {section!r} — refusing ambiguous extract"
                )
            text_idx = i

    if text_idx is None:
        # Do not fall back to a differently named section via symbol lookup:
        # that would silently extract unrelated machine code.
        return None

    for i, s in enumerate(sh):
        if s[1] in (SHT_REL, SHT_RELA) and s[7] == text_idx and s[5] > 0:
            raise SystemExit(
                f"{section} has relocations in {sname(i)} — "
                "refusing to emit unresolved machine code"
            )

    # Require the named symbol lives at offset 0 in the named section
    # (typical for #[no_mangle] + #[link_section] function-sections).
    #
    # Unresolved dependencies: any REL/RELA targeting this section already
    # fails above. That is the hard guarantee — do not blanket-reject UND
    # symbols elsewhere in the same .o (multi-section objects are normal).
    saw_symbol = False
    for i, s in enumerate(sh):
        if s[1] != SHT_SYMTAB:
            continue
        stab = data[sh[s[6]][4] : sh[s[6]][4] + sh[s[6]][5]]
        entsize = s[9] or 16
        for j in range(s[5] // entsize):
            so = s[4] + j * entsize
            st_name, st_value, st_size, st_info, _st_other, st_shndx = (
                struct.unpack_from("<IIIBBH", data, so)
            )
            name = stab[st_name : stab.find(b"\x00", st_name)].decode()
            if not symbol or name != symbol:
                continue
            saw_symbol = True
            if st_shndx == SHN_UNDEF:
                raise SystemExit(f"symbol {symbol} is undefined (SHN_UNDEF)")
            if st_shndx != text_idx:
                raise SystemExit(
                    f"symbol {symbol} in section index {st_shndx} "
                    f"({sname(st_shndx)!r}), not requested {section!r}"
                )
            st_type = st_info & 0xF
            if st_type not in (STT_FUNC, STT_NOTYPE):
                raise SystemExit(
                    f"symbol {symbol} has unexpected type {st_type}"
                )
            if st_value != 0:
                raise SystemExit(
                    f"symbol {symbol} at offset 0x{st_value:x}; "
                    "expected 0 for section extract"
                )
            if st_size and st_size < sh[text_idx][5]:
                raise SystemExit(
                    f"symbol {symbol} size {st_size} < section "
                    f"{sh[text_idx][5]} — refusing partial extract"
                )

    if symbol and not saw_symbol:
        raise SystemExit(f"symbol {symbol} not found in object containing {section}")

    sec = sh[text_idx]
    # SHF_EXECINSTR | SHF_ALLOC typically; reject non-PROGBITS surprises.
    SHT_PROGBITS = 1
    if sec[1] != SHT_PROGBITS:
        raise SystemExit(
            f"{section} has unexpected type {sec[1]} (want SHT_PROGBITS=1)"
        )

    blob = data[sec[4] : sec[4] + sec[5]]
    if not blob:
        raise SystemExit(f"empty {section}")
    return blob


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive", required=True)
    ap.add_argument("--section", required=True)
    ap.add_argument("--symbol", default=None)
    ap.add_argument("--out", required=True)
    ap.add_argument(
        "--expect-ret-imm",
        type=lambda s: int(s, 0),
        default=None,
        help="optional: require trailing ret imm16 (e.g. 16 for 4 stdcall args)",
    )
    args = ap.parse_args()

    blob = None
    member_name = None
    for name, data in parse_archive(args.archive):
        if data[:4] != b"\x7fELF":
            continue
        found = find_section_blob(data, args.section, args.symbol)
        if found is not None:
            blob = found
            member_name = name
            break
    if blob is None:
        raise SystemExit(f"{args.section} not found in archive")

    if args.expect_ret_imm is not None:
        needle = bytes((0xC2, args.expect_ret_imm & 0xFF, (args.expect_ret_imm >> 8) & 0xFF))
        if needle not in blob:
            raise SystemExit(
                f"expected stdcall ret {args.expect_ret_imm} (bytes {needle.hex()}) "
                f"in blob; last bytes={blob[-4:].hex()}"
            )
        if blob[-3:] != needle:
            # LLVM may end with jmp to a shared internal epilogue that does ret imm.
            print(
                f"note: ret {args.expect_ret_imm} not trailing "
                f"(shared epilogue); last bytes={blob[-4:].hex()}"
            )

    os.makedirs(os.path.dirname(os.path.abspath(args.out)) or ".", exist_ok=True)
    with open(args.out, "wb") as f:
        f.write(blob)
    print(
        f"found {args.section} in {member_name} ({len(blob)} bytes)"
    )
    print(f"wrote {args.out} ({len(blob)} bytes)")
    print(f"hex: {blob.hex()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
