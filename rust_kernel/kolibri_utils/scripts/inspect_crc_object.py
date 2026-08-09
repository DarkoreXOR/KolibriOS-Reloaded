#!/usr/bin/env python3
"""Inspect rust_crc_32 ELF member: sections, symbols, relocations."""

from __future__ import annotations

import struct
import sys

archive = sys.argv[1] if len(sys.argv) > 1 else (
    r"F:\osdev\kolibri_kernel\rust_kernel\target\i686-kolibri-none\release\libkolibri_utils.a"
)


def parse_archive(path: str):
    with open(path, "rb") as f:
        magic = f.read(8)
        if magic != b"!<arch>\n":
            raise SystemExit(f"not an archive: {path}")
        string_table = b""
        members = []
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


SHT_SYMTAB = 2
SHT_REL = 9
SHT_RELA = 4


def analyze(data: bytes, member_name: str) -> bool:
    if data[:4] != b"\x7fELF":
        return False
    e_shoff = struct.unpack_from("<I", data, 32)[0]
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", data, 46)
    sh = [
        struct.unpack_from("<IIIIIIIIII", data, e_shoff + i * e_shentsize)
        for i in range(e_shnum)
    ]
    strtab = data[sh[e_shstrndx][4] : sh[e_shstrndx][4] + sh[e_shstrndx][5]]

    def sname(i: int) -> str:
        n = sh[i][0]
        return strtab[n : strtab.find(b"\x00", n)].decode()

    syms = []
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
            if not name:
                continue
            bind = st_info >> 4
            typ = st_info & 0xF
            if st_shndx < len(sh) and st_shndx != 0:
                secn = sname(st_shndx)
            elif st_shndx == 0:
                secn = "UNDEF"
            else:
                secn = f"SHNDX_{st_shndx}"
            syms.append((name, st_value, st_size, bind, typ, st_shndx, secn))

    interesting = any("crc" in sname(i).lower() for i in range(len(sh)))
    interesting = interesting or any("crc" in n.lower() for n, *_ in syms)
    if not interesting:
        return False

    print("=" * 70)
    print("MEMBER:", member_name)
    print("SECTIONS:")
    for i, s in enumerate(sh):
        print(
            f"  [{i:2d}] {sname(i):40s} type={s[1]:3d} size={s[5]:6d} "
            f"flags=0x{s[2]:x} link={s[6]} info={s[7]}"
        )

    print("SYMBOLS:")
    for name, val, sz, bind, typ, shndx, shn in syms:
        if name.startswith("."):
            continue
        print(
            f"  {name:50s} val=0x{val:08x} size={sz:5d} "
            f"bind={bind} typ={typ} sec={shn}"
        )

    print("RELOCATIONS:")
    any_rel = False
    for i, s in enumerate(sh):
        if s[1] not in (SHT_REL, SHT_RELA) or s[5] == 0:
            continue
        any_rel = True
        target = s[7]
        tname = sname(target) if target < len(sh) else "?"
        print(f"  {sname(i)} -> {tname} (bytes={s[5]})")
        symtab_idx = s[6]
        stab_sec = sh[symtab_idx]
        stab = data[sh[stab_sec[6]][4] : sh[stab_sec[6]][4] + sh[stab_sec[6]][5]]
        entsize = stab_sec[9] or 16
        ent = 8 if s[1] == SHT_REL else 12
        for off in range(0, s[5], ent):
            if s[1] == SHT_REL:
                r_offset, r_info = struct.unpack_from("<II", data, s[4] + off)
                r_addend = None
            else:
                r_offset, r_info, r_addend = struct.unpack_from("<IIi", data, s[4] + off)
            r_sym = r_info >> 8
            r_type = r_info & 0xFF
            so = stab_sec[4] + r_sym * entsize
            st_name = struct.unpack_from("<I", data, so)[0]
            sn = (
                stab[st_name : stab.find(b"\x00", st_name)].decode()
                if st_name
                else f"#{r_sym}"
            )
            st_shndx = struct.unpack_from("<H", data, so + 14)[0]
            add = f" addend={r_addend}" if r_addend is not None else ""
            print(
                f"    offset=0x{r_offset:x} type={r_type} sym={sn!r} "
                f"symsec={st_shndx}{add}"
            )
    if not any_rel:
        print("  (none)")

    # Dump CRC text section hex if present
    for i, s in enumerate(sh):
        nm = sname(i)
        if "crc" in nm.lower() and s[1] == 1:  # SHT_PROGBITS
            blob = data[s[4] : s[4] + s[5]]
            print(f"HEX {nm} ({len(blob)} bytes):")
            print(" ", blob.hex())
            # Also try to find rust_crc_32 symbol size
            for name, val, sz, bind, typ, shndx, shn in syms:
                if name == "rust_crc_32" and shndx == i:
                    print(f"  rust_crc_32 at +0x{val:x} size={sz}")
                    print("  body:", blob[val : val + sz].hex())
    return True


def main() -> int:
    found = False
    for name, data in parse_archive(archive):
        if analyze(data, name):
            found = True
    if not found:
        print("No CRC-related members found")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
