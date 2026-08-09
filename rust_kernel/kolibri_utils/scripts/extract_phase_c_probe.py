#!/usr/bin/env python3
"""Extract reloc-free rust_phase_c_probe machine code from libkolibri_utils.a.

Phase C uses section extraction (not FASM consuming .a directly):
  1. Build freestanding staticlib for i686-kolibri-none.
  2. Locate the ELF member that defines rust_phase_c_probe.
  3. Require zero relocations against that function's .text section.
  4. Write raw bytes for FASM `file` inclusion.

Fails loudly if relocations appear — do not concatenate unresolved code.
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


def find_probe(data: bytes):
    parsed = elf_sections(data)
    if parsed is None:
        return None
    sh, sname = parsed
    SHT_SYMTAB = 2
    SHT_REL = 9
    SHT_RELA = 4
    text_idx = None
    for i, s in enumerate(sh):
        if sname(i) == ".text.rust_phase_c_probe":
            text_idx = i
            break
    if text_idx is None:
        # Fallback: symbol table points at a section
        for i, s in enumerate(sh):
            if s[1] != SHT_SYMTAB:
                continue
            stab = data[sh[s[6]][4] : sh[s[6]][4] + sh[s[6]][5]]
            entsize = s[9] or 16
            for j in range(s[5] // entsize):
                so = s[4] + j * entsize
                st_name, st_value, st_size, st_info, _st_other, st_shndx = struct.unpack_from(
                    "<IIIBBH", data, so
                )
                name = stab[st_name : stab.find(b"\x00", st_name)].decode()
                if name == "rust_phase_c_probe" and st_shndx != 0:
                    text_idx = st_shndx
                    break
    if text_idx is None:
        return None

    # Reject any relocation section targeting the probe text
    for i, s in enumerate(sh):
        if s[1] in (SHT_REL, SHT_RELA) and s[7] == text_idx and s[5] > 0:
            raise SystemExit(
                f"rust_phase_c_probe has relocations in {sname(i)} — "
                "refusing to emit unresolved machine code"
            )

    sec = sh[text_idx]
    blob = data[sec[4] : sec[4] + sec[5]]
    if not blob:
        raise SystemExit("empty .text.rust_phase_c_probe")
    return blob


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    blob = None
    for name, data in parse_archive(args.archive):
        if data[:4] != b"\x7fELF":
            continue
        found = find_probe(data)
        if found is not None:
            blob = found
            print(f"found rust_phase_c_probe in {name} ({len(found)} bytes)")
            break
    if blob is None:
        raise SystemExit("rust_phase_c_probe not found in archive")

    # Sanity: stdcall 0-arg returning imm32 is typically mov eax,imm32 / ret
    if len(blob) < 6:
        raise SystemExit(f"probe unexpectedly tiny: {len(blob)} bytes")
    magic = 0xC0DEA11C
    if magic.to_bytes(4, "little") not in blob:
        raise SystemExit("magic 0xC0DEA11C not present in probe machine code")

    os.makedirs(os.path.dirname(os.path.abspath(args.out)) or ".", exist_ok=True)
    with open(args.out, "wb") as f:
        f.write(blob)
    print(f"wrote {args.out} ({len(blob)} bytes)")
    print(f"hex: {blob.hex()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
