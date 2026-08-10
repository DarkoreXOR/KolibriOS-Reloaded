# `binutil`

Host-side **disassemble / assemble / object inspect** helper for Kolibri kernel
migration work. **Not** linked into the kernel.

Combines what was missing while debugging Rust blobs and COFF/ELF archives:

| Area | Engines / crates |
|------|------------------|
| Disassemble | **iced-x86** (default, pure Rust), **Capstone** (optional) |
| Assemble | **Keystone** (optional), **vendored FASM** (always, when `tools/fasm` is present) |
| Objects | **`object`** crate — ELF / COFF / PE / Mach-O / `ar` archives |

## Build

```text
cd tools/binutil
cargo build --release --target-dir target
```

Binary: `tools/binutil/target/release/binutil.exe`  
(Avoid a leftover `CARGO_TARGET_DIR` pointing at a shared sandbox cache when building this crate.)

### Feature flags

| Feature | Default | Notes |
|---------|---------|-------|
| `iced` | yes | Pure-Rust x86 disassembler |
| `object-parse` | yes | Object / archive parsing |
| `capstone` | no | Needs a C toolchain (`cc` crate / MSVC or gcc) |
| `keystone` | no | Needs **cmake** + C++ (`build-from-src`) |
| `native` | no | `capstone` + `keystone` |
| `full` | no | All of the above |

```text
cargo build --release -F native
cargo build --release -F full
```

On this Windows workspace cmake is often absent; default build (iced + object +
FASM asm) is enough for blob/section work. Capstone/Keystone turn on when the
native toolchain is available.

```text
binutil engines
```

## Commands

```text
binutil engines
binutil disasm <file> [--engine iced|capstone] [--bits 16|32|64]
                      [--syntax intel|nasm|masm|gas]
                      [--offset HEX] [--len N] [--addr HEX]
                      [--section NAME] [--member ARCHIVE_MEMBER]
binutil asm [--backend keystone|fasm] [--bits 16|32|64] [--addr HEX]
            [--out FILE] [--insn "mov eax, 1"] [file|-]
binutil obj <file>
binutil sections <file> [--member NAME]
binutil symbols <file> [--member NAME]
binutil relocs <file> [--member NAME] [--section NAME]
binutil extract-section <file> <section> [--member NAME] [--out FILE]
binutil dump <file> [--offset HEX] [--len N] [--section NAME] [--member NAME]
```

Default `--bits` is **32** (Kolibri i686).

## Examples

```text
# Disassemble a reloc-free Rust blob
binutil disasm rust_kernel/kolibri_utils/out/rust_utf16_to_upper.bin

# Section from freestanding archive
binutil sections rust_kernel/target/i686-kolibri-none/release/libkolibri_utils.a --member kolibri_utils-….rcgu.o
binutil disasm   rust_kernel/target/i686-kolibri-none/release/libkolibri_utils.a ^
                 --member kolibri_utils-….rcgu.o --section .text.rust_utf16_to_upper

# Assemble with vendored FASM
binutil asm --backend fasm --insn "mov eax, 1" --out dev_build/a.bin

# Relocs (Cut AJ / blob extract checks)
binutil relocs path/to/file.o --section .text.rust_iso9660_compare_name
```

## Env

| Variable | Purpose |
|----------|---------|
| `FASM` | Override path to FASM executable for `--backend fasm` |
