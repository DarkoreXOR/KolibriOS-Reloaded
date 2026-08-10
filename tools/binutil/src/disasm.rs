//! Disassembly backends: iced-x86 (default) and Capstone (optional).

use crate::arch::Bits;
use crate::util::{self, BoxError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Iced,
    Capstone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Intel,
    Nasm,
    Masm,
    Gas,
}

impl Syntax {
    fn parse(s: &str) -> Result<Self, BoxError> {
        match s.to_ascii_lowercase().as_str() {
            "intel" => Ok(Self::Intel),
            "nasm" => Ok(Self::Nasm),
            "masm" => Ok(Self::Masm),
            "gas" | "att" => Ok(Self::Gas),
            other => Err(format!("invalid --syntax '{other}'").into()),
        }
    }
}

impl Engine {
    fn parse(s: &str) -> Result<Self, BoxError> {
        match s.to_ascii_lowercase().as_str() {
            "iced" | "iced-x86" => Ok(Self::Iced),
            "capstone" | "cs" => Ok(Self::Capstone),
            other => Err(format!("invalid --engine '{other}' (iced|capstone)").into()),
        }
    }

    fn default_available() -> Result<Self, BoxError> {
        if cfg!(feature = "iced") {
            Ok(Self::Iced)
        } else if cfg!(feature = "capstone") {
            Ok(Self::Capstone)
        } else {
            Err(
                "no disasm engine compiled in (build with -F iced and/or -F capstone)".into(),
            )
        }
    }
}

pub fn cmd(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let engine = match util::take_opt(&mut args, &["--engine"])? {
        Some(s) => Engine::parse(&s)?,
        None => Engine::default_available()?,
    };
    let bits = match util::take_opt(&mut args, &["--bits"])? {
        Some(s) => Bits::parse(&s)?,
        None => Bits::default(),
    };
    let syntax = match util::take_opt(&mut args, &["--syntax"])? {
        Some(s) => Syntax::parse(&s)?,
        None => Syntax::Intel,
    };
    let offset = util::take_opt(&mut args, &["--offset"])?
        .map(|s| util::parse_usize_auto(&s))
        .transpose()?
        .unwrap_or(0);
    let len = util::take_opt(&mut args, &["--len"])?
        .map(|s| util::parse_usize_auto(&s))
        .transpose()?;
    let addr = util::take_opt(&mut args, &["--addr"])?
        .map(|s| util::parse_u64_auto(&s))
        .transpose()?
        .unwrap_or(0);
    let section = util::take_opt(&mut args, &["--section"])?;
    let member = util::take_opt(&mut args, &["--member"])?;

    let path = util::require_path(&args, "input file")?;
    let bytes = load_code_bytes(&path, section.as_deref(), member.as_deref())?;
    let code = util::slice_bytes(&bytes, offset, len)?;

    match engine {
        Engine::Iced => disasm_iced(code, bits, syntax, addr)?,
        Engine::Capstone => disasm_capstone(code, bits, syntax, addr)?,
    }
    Ok(())
}

fn load_code_bytes(
    path: &str,
    section: Option<&str>,
    member: Option<&str>,
) -> Result<Vec<u8>, BoxError> {
    if let Some(sec) = section {
        #[cfg(feature = "object-parse")]
        {
            return crate::object_file::read_section_bytes(path, member, sec);
        }
        #[cfg(not(feature = "object-parse"))]
        {
            let _ = (sec, member);
            return Err("object section support requires -F object-parse".into());
        }
    }
    if member.is_some() {
        return Err("--member requires --section".into());
    }
    util::read_file(path)
}

#[cfg(feature = "iced")]
fn disasm_iced(code: &[u8], bits: Bits, syntax: Syntax, ip: u64) -> Result<(), BoxError> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, GasFormatter, Instruction, IntelFormatter, MasmFormatter, NasmFormatter};

    let bitness = bits.as_u32();
    let mut decoder = Decoder::with_ip(bitness, code, ip, DecoderOptions::NONE);
    let mut instruction = Instruction::default();

    let mut intel;
    let mut nasm;
    let mut masm;
    let mut gas;
    let formatter: &mut dyn Formatter = match syntax {
        Syntax::Intel => {
            intel = IntelFormatter::new();
            &mut intel
        }
        Syntax::Nasm => {
            nasm = NasmFormatter::new();
            &mut nasm
        }
        Syntax::Masm => {
            masm = MasmFormatter::new();
            &mut masm
        }
        Syntax::Gas => {
            gas = GasFormatter::new();
            &mut gas
        }
    };

    let mut output = String::new();
    while decoder.can_decode() {
        decoder.decode_out(&mut instruction);
        output.clear();
        formatter.format(&instruction, &mut output);
        let start = (instruction.ip() - ip) as usize;
        let len = instruction.len();
        let end = (start + len).min(code.len());
        let hex: String = code[start..end]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("{:08X}  {:<24} {}", instruction.ip(), hex, output);
        if instruction.is_invalid() {
            // Keep going; iced marks undecodable bytes as invalid.
        }
    }
    Ok(())
}

#[cfg(not(feature = "iced"))]
fn disasm_iced(_code: &[u8], _bits: Bits, _syntax: Syntax, _ip: u64) -> Result<(), BoxError> {
    Err("iced-x86 backend not compiled in (cargo build -F iced)".into())
}

#[cfg(feature = "capstone")]
fn disasm_capstone(code: &[u8], bits: Bits, syntax: Syntax, ip: u64) -> Result<(), BoxError> {
    use capstone::prelude::*;
    use capstone::arch::x86::{ArchMode, ArchSyntax};

    let mode = match bits {
        Bits::B16 => ArchMode::Mode16,
        Bits::B32 => ArchMode::Mode32,
        Bits::B64 => ArchMode::Mode64,
    };
    let syn = match syntax {
        Syntax::Intel | Syntax::Nasm | Syntax::Masm => ArchSyntax::Intel,
        Syntax::Gas => ArchSyntax::Att,
    };
    let cs = Capstone::new()
        .x86()
        .mode(mode)
        .syntax(syn)
        .detail(false)
        .build()
        .map_err(|e| format!("capstone init: {e}"))?;
    let insns = cs
        .disasm_all(code, ip)
        .map_err(|e| format!("capstone disasm: {e}"))?;
    for insn in insns.iter() {
        let hex: String = insn
            .bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mnem = insn.mnemonic().unwrap_or("?");
        let op = insn.op_str().unwrap_or("");
        if op.is_empty() {
            println!("{:08X}  {:<24} {}", insn.address(), hex, mnem);
        } else {
            println!("{:08X}  {:<24} {} {}", insn.address(), hex, mnem, op);
        }
    }
    Ok(())
}

#[cfg(not(feature = "capstone"))]
fn disasm_capstone(_code: &[u8], _bits: Bits, _syntax: Syntax, _ip: u64) -> Result<(), BoxError> {
    Err("capstone backend not compiled in (cargo build -F capstone; needs C toolchain)".into())
}
