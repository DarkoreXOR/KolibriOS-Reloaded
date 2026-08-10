//! Host binary / assembly utility for Kolibri kernel migration work.
//!
//! Subcommands:
//!   engines                         — list compiled backends
//!   disasm <file> […]               — disassemble raw bytes or an object section
//!   asm [--out FILE] <asm|-> […]    — assemble (Keystone and/or vendored FASM)
//!   obj <file>                      — summarize object / archive / PE / ELF
//!   sections <file>                 — list sections
//!   symbols <file>                  — list symbols
//!   relocs <file>                   — list relocations
//!   extract-section <file> <name>   — dump a section to stdout or --out
//!   dump <file> […]                 — hex dump of a file / section slice

mod arch;
mod asm;
mod disasm;
mod dump;
mod object_file;
mod util;

use std::env;
use std::process::ExitCode;
use util::BoxError;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }
    let cmd = args.remove(0);
    let result: Result<(), BoxError> = match cmd.as_str() {
        "engines" => cmd_engines(),
        "disasm" => disasm::cmd(&args),
        "asm" => asm::cmd(&args),
        "obj" => object_file::cmd_obj(&args),
        "sections" => object_file::cmd_sections(&args),
        "symbols" => object_file::cmd_symbols(&args),
        "relocs" => object_file::cmd_relocs(&args),
        "extract-section" => object_file::cmd_extract_section(&args),
        "dump" => dump::cmd(&args),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown command '{other}' (try `binutil help`)").into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_engines() -> Result<(), BoxError> {
    println!("binutil backends:");
    println!(
        "  iced-x86 disasm : {}",
        if cfg!(feature = "iced") {
            "enabled"
        } else {
            "disabled (cargo build -F iced)"
        }
    );
    println!(
        "  capstone disasm : {}",
        if cfg!(feature = "capstone") {
            "enabled"
        } else {
            "disabled (cargo build -F capstone; needs C toolchain)"
        }
    );
    println!(
        "  keystone asm    : {}",
        if cfg!(feature = "keystone") {
            "enabled"
        } else {
            "disabled (cargo build -F keystone; needs cmake + C++)"
        }
    );
    println!("  fasm asm        : always (vendored tools/fasm when present)");
    println!(
        "  object parse    : {}",
        if cfg!(feature = "object-parse") {
            "enabled"
        } else {
            "disabled (cargo build -F object-parse)"
        }
    );
    Ok(())
}

fn print_usage() {
    eprintln!(
        "\
binutil — disassemble / assemble / inspect binaries & objects

Usage:
  binutil engines
  binutil disasm <file> [options]
  binutil asm [--backend keystone|fasm] [--bits 16|32|64] [--addr HEX]
              [--out FILE] [--insn \"mov eax,1\"] [file|-]
  binutil obj <file>
  binutil sections <file>
  binutil symbols <file>
  binutil relocs <file>
  binutil extract-section <file> <section> [--out FILE]
  binutil dump <file> [--offset HEX] [--len N] [--section NAME]

disasm options:
  --engine iced|capstone     disassembler (default: iced if built, else capstone)
  --bits 16|32|64            decode width (default: 32 — Kolibri i686)
  --syntax intel|nasm|masm|gas
  --offset HEX               byte offset into file (raw) or section
  --len N                    max bytes to decode (default: rest of range)
  --addr HEX                 runtime/IP base address printed in listing
  --section NAME             take bytes from this object section (ELF/COFF/…)
  --member NAME              archive member (.a) before --section

Examples:
  binutil disasm rust_kernel/kolibri_utils/out/rust_utf16_to_upper.bin
  binutil disasm lib.a --member foo.o --section .text.rust_crc_32
  binutil asm --insn \"mov eax, 1\" --out /tmp/a.bin
  binutil sections target/i686-kolibri-none/release/libkolibri_utils.a --member …
"
    );
}
