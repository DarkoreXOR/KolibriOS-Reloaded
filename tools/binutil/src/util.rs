//! Shared CLI helpers: hex ints, file slices, flag parsing.

use std::fs;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn parse_u64_auto(s: &str) -> Result<u64, BoxError> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Ok(u64::from_str_radix(hex, 16)?)
    } else if t.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        Ok(u64::from_str_radix(t, 16)?)
    } else {
        Ok(t.parse::<u64>()?)
    }
}

pub fn parse_usize_auto(s: &str) -> Result<usize, BoxError> {
    Ok(usize::try_from(parse_u64_auto(s)?)?)
}

pub fn read_file(path: &str) -> Result<Vec<u8>, BoxError> {
    Ok(fs::read(path)?)
}

pub fn slice_bytes(data: &[u8], offset: usize, len: Option<usize>) -> Result<&[u8], BoxError> {
    if offset > data.len() {
        return Err(format!("offset {offset:#x} past end (len {:#x})", data.len()).into());
    }
    let end = match len {
        Some(n) => offset
            .checked_add(n)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| format!("offset+len exceeds file (len {:#x})", data.len()))?,
        None => data.len(),
    };
    Ok(&data[offset..end])
}

/// Take `--flag value` or `--flag=value`; returns (remaining, value).
pub fn take_opt(args: &mut Vec<String>, names: &[&str]) -> Result<Option<String>, BoxError> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        for name in names {
            if a == *name {
                if i + 1 >= args.len() {
                    return Err(format!("missing value after {name}").into());
                }
                let v = args.remove(i + 1);
                args.remove(i);
                return Ok(Some(v));
            }
            let prefix = format!("{name}=");
            if let Some(rest) = a.strip_prefix(&prefix) {
                let v = rest.to_string();
                args.remove(i);
                return Ok(Some(v));
            }
        }
        i += 1;
    }
    Ok(None)
}

pub fn require_path(args: &[String], what: &str) -> Result<String, BoxError> {
    args.first()
        .cloned()
        .ok_or_else(|| format!("missing {what}").into())
}
