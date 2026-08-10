//! Object / archive inspection via the `object` crate.

use crate::util::{self, BoxError};

#[cfg(feature = "object-parse")]
use object::{Object, ObjectSection, ObjectSymbol};

pub fn cmd_obj(args: &[String]) -> Result<(), BoxError> {
    let path = util::require_path(args, "object / archive / binary")?;
    summarize(&path)
}

pub fn cmd_sections(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let member = util::take_opt(&mut args, &["--member"])?;
    let path = util::require_path(&args, "object / archive")?;
    list_sections(&path, member.as_deref())
}

pub fn cmd_symbols(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let member = util::take_opt(&mut args, &["--member"])?;
    let path = util::require_path(&args, "object / archive")?;
    list_symbols(&path, member.as_deref())
}

pub fn cmd_relocs(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let member = util::take_opt(&mut args, &["--member"])?;
    let section = util::take_opt(&mut args, &["--section"])?;
    let path = util::require_path(&args, "object / archive")?;
    list_relocs(&path, member.as_deref(), section.as_deref())
}

pub fn cmd_extract_section(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let out = util::take_opt(&mut args, &["--out", "-o"])?;
    let member = util::take_opt(&mut args, &["--member"])?;
    if args.len() < 2 {
        return Err("usage: binutil extract-section <file> <section> [--member NAME] [--out FILE]".into());
    }
    let path = args[0].clone();
    let section = args[1].clone();
    let bytes = read_section_bytes(&path, member.as_deref(), &section)?;
    if let Some(p) = out {
        std::fs::write(&p, &bytes)?;
        eprintln!("wrote {} bytes → {p}", bytes.len());
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&bytes)?;
    }
    Ok(())
}

#[cfg(feature = "object-parse")]
pub fn read_section_bytes(
    path: &str,
    member: Option<&str>,
    section: &str,
) -> Result<Vec<u8>, BoxError> {
    let data = util::read_file(path)?;
    if is_archive(&data) {
        let member_name = member.ok_or("--member is required for archives (.a/.lib)")?;
        let file = object::read::archive::ArchiveFile::parse(&*data)
            .map_err(|e| format!("archive parse: {e}"))?;
        for m in file.members() {
            let m = m.map_err(|e| format!("archive member: {e}"))?;
            let name = String::from_utf8_lossy(m.name());
            if name == member_name || name.ends_with(member_name) {
                let member_data = m
                    .data(&*data)
                    .map_err(|e| format!("archive member data: {e}"))?;
                return section_from_object_bytes(member_data, section);
            }
        }
        return Err(format!("archive member '{member_name}' not found in {path}").into());
    }
    if member.is_some() {
        return Err("--member is only valid for archives".into());
    }
    section_from_object_bytes(&data, section)
}

#[cfg(not(feature = "object-parse"))]
pub fn read_section_bytes(
    _path: &str,
    _member: Option<&str>,
    _section: &str,
) -> Result<Vec<u8>, BoxError> {
    Err("object support requires -F object-parse".into())
}

#[cfg(feature = "object-parse")]
fn is_archive(data: &[u8]) -> bool {
    data.starts_with(b"!<arch>\n")
}

#[cfg(feature = "object-parse")]
fn section_from_object_bytes(data: &[u8], section: &str) -> Result<Vec<u8>, BoxError> {
    let obj = object::File::parse(data).map_err(|e| format!("object parse: {e}"))?;
    for sec in obj.sections() {
        let name = sec.name().unwrap_or("");
        if name == section {
            let data = sec.data().map_err(|e| format!("section data: {e}"))?;
            return Ok(data.to_vec());
        }
    }
    // Helpful listing on miss
    let mut names = Vec::new();
    for sec in obj.sections() {
        if let Ok(n) = sec.name() {
            if !n.is_empty() {
                names.push(n.to_string());
            }
        }
    }
    Err(format!(
        "section '{section}' not found; available: {}",
        names.join(", ")
    )
    .into())
}

#[cfg(feature = "object-parse")]
fn summarize(path: &str) -> Result<(), BoxError> {
    let data = util::read_file(path)?;
    if is_archive(&data) {
        let file = object::read::archive::ArchiveFile::parse(&*data)
            .map_err(|e| format!("archive parse: {e}"))?;
        println!("kind: archive");
        println!("path: {path}");
        println!("members:");
        for m in file.members() {
            let m = m.map_err(|e| format!("archive member: {e}"))?;
            let name = String::from_utf8_lossy(m.name());
            let md = m.data(&*data).map_err(|e| format!("member data: {e}"))?;
            println!("  {name}  ({} bytes)", md.len());
        }
        return Ok(());
    }

    match object::File::parse(&*data) {
        Ok(obj) => {
            println!("path:       {path}");
            println!("format:     {:?}", obj.format());
            println!("architecture: {:?}", obj.architecture());
            println!("endian:     {:?}", obj.endianness());
            println!("entry:      {:#x}", obj.entry());
            println!("is_64:      {}", obj.is_64());
            let mut nsec = 0usize;
            for _ in obj.sections() {
                nsec += 1;
            }
            let mut nsym = 0usize;
            for s in obj.symbols() {
                let _ = s;
                nsym += 1;
            }
            println!("sections:   {nsec}");
            println!("symbols:    {nsym}");
            Ok(())
        }
        Err(e) => {
            // Raw blob fallback
            println!("path:    {path}");
            println!("kind:    raw binary (object parse failed: {e})");
            println!("size:    {} bytes ({:#x})", data.len(), data.len());
            Ok(())
        }
    }
}

#[cfg(not(feature = "object-parse"))]
fn summarize(_path: &str) -> Result<(), BoxError> {
    Err("object support requires -F object-parse".into())
}

#[cfg(feature = "object-parse")]
fn with_object<F>(path: &str, member: Option<&str>, f: F) -> Result<(), BoxError>
where
    F: FnOnce(&object::File<'_>) -> Result<(), BoxError>,
{
    let data = util::read_file(path)?;
    if is_archive(&data) {
        let member_name = member.ok_or("archives require --member NAME")?;
        let file = object::read::archive::ArchiveFile::parse(&*data)
            .map_err(|e| format!("archive parse: {e}"))?;
        for m in file.members() {
            let m = m.map_err(|e| format!("archive member: {e}"))?;
            let name = String::from_utf8_lossy(m.name());
            if name == member_name || name.ends_with(member_name) {
                let member_data = m
                    .data(&*data)
                    .map_err(|e| format!("archive member data: {e}"))?;
                // Leak-free: parse into owned vec then re-parse… keep member_data alive:
                return with_object_bytes(member_data, f);
            }
        }
        return Err(format!("archive member '{member_name}' not found").into());
    }
    if member.is_some() {
        return Err("--member is only valid for archives".into());
    }
    with_object_bytes(&data, f)
}

#[cfg(feature = "object-parse")]
fn with_object_bytes<F>(data: &[u8], f: F) -> Result<(), BoxError>
where
    F: FnOnce(&object::File<'_>) -> Result<(), BoxError>,
{
    let obj = object::File::parse(data).map_err(|e| format!("object parse: {e}"))?;
    f(&obj)
}

#[cfg(feature = "object-parse")]
fn list_sections(path: &str, member: Option<&str>) -> Result<(), BoxError> {
    with_object(path, member, |obj| {
        println!(
            "{:<6} {:<32} {:>10} {:>10} {}",
            "IDX", "NAME", "SIZE", "ADDR", "KIND"
        );
        for (i, sec) in obj.sections().enumerate() {
            let name = sec.name().unwrap_or("<unnamed>");
            let size = sec.size();
            let addr = sec.address();
            let kind = format!("{:?}", sec.kind());
            println!("{i:<6} {name:<32} {size:>10} {addr:>10x} {kind}");
        }
        Ok(())
    })
}

#[cfg(not(feature = "object-parse"))]
fn list_sections(_path: &str, _member: Option<&str>) -> Result<(), BoxError> {
    Err("object support requires -F object-parse".into())
}

#[cfg(feature = "object-parse")]
fn list_symbols(path: &str, member: Option<&str>) -> Result<(), BoxError> {
    with_object(path, member, |obj| {
        println!(
            "{:<10} {:<8} {:<12} {}",
            "ADDR", "SIZE", "KIND", "NAME"
        );
        for sym in obj.symbols() {
            let name = sym.name().unwrap_or("<unnamed>");
            let kind = format!("{:?}", sym.kind());
            println!(
                "{:<10x} {:<8} {:<12} {}",
                sym.address(),
                sym.size(),
                kind,
                name
            );
        }
        Ok(())
    })
}

#[cfg(not(feature = "object-parse"))]
fn list_symbols(_path: &str, _member: Option<&str>) -> Result<(), BoxError> {
    Err("object support requires -F object-parse".into())
}

#[cfg(feature = "object-parse")]
fn list_relocs(
    path: &str,
    member: Option<&str>,
    section_filter: Option<&str>,
) -> Result<(), BoxError> {
    with_object(path, member, |obj| {
        let mut any = false;
        for sec in obj.sections() {
            let name = sec.name().unwrap_or("");
            if let Some(want) = section_filter {
                if name != want {
                    continue;
                }
            }
            let relocs: Vec<_> = sec.relocations().collect();
            if relocs.is_empty() {
                continue;
            }
            any = true;
            println!("section {name}:");
            for (offset, reloc) in relocs {
                println!(
                    "  offset={offset:#x}  kind={:?}  addend={:?}  target={:?}",
                    reloc.kind(),
                    reloc.addend(),
                    reloc.target()
                );
            }
        }
        if !any {
            println!("(no relocations{})", 
                section_filter.map(|s| format!(" in '{s}'")).unwrap_or_default());
        }
        Ok(())
    })
}

#[cfg(not(feature = "object-parse"))]
fn list_relocs(
    _path: &str,
    _member: Option<&str>,
    _section_filter: Option<&str>,
) -> Result<(), BoxError> {
    Err("object support requires -F object-parse".into())
}
