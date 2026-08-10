//! Hex dump of a file or object section.

use crate::util::{self, BoxError};

pub fn cmd(args_in: &[String]) -> Result<(), BoxError> {
    let mut args = args_in.to_vec();
    let offset = util::take_opt(&mut args, &["--offset"])?
        .map(|s| util::parse_usize_auto(&s))
        .transpose()?
        .unwrap_or(0);
    let len = util::take_opt(&mut args, &["--len"])?
        .map(|s| util::parse_usize_auto(&s))
        .transpose()?;
    let section = util::take_opt(&mut args, &["--section"])?;
    let member = util::take_opt(&mut args, &["--member"])?;
    let path = util::require_path(&args, "input file")?;

    let bytes = if let Some(sec) = section.as_deref() {
        #[cfg(feature = "object-parse")]
        {
            crate::object_file::read_section_bytes(&path, member.as_deref(), sec)?
        }
        #[cfg(not(feature = "object-parse"))]
        {
            let _ = (sec, &member);
            return Err("object section support requires -F object-parse".into());
        }
    } else {
        if member.is_some() {
            return Err("--member requires --section".into());
        }
        util::read_file(&path)?
    };

    let slice = util::slice_bytes(&bytes, offset, len)?;
    for (i, chunk) in slice.chunks(16).enumerate() {
        let base = offset + i * 16;
        let hex: String = chunk
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("{base:08X}  {hex:<48}  |{ascii}|");
    }
    eprintln!("({} bytes)", slice.len());
    Ok(())
}
