//! Controlled invocation of reusable utilities under `./tools/`.

use crate::config::resolve;
use crate::util::{find_python, run_checked, CmdResult};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Result of running a project tool.
pub struct ToolRun {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub result: CmdResult,
}

/// Resolve a tool path under `./tools/` (e.g. `mkfs_utils/create_exfat_image.py`).
pub fn resolve_tool_path(root: &Path, tool: &str) -> Result<PathBuf> {
    let normalized = tool.replace('\\', "/");
    if normalized.contains("..") {
        bail!("tool path must not contain `..`: {tool}");
    }
    let path = if normalized.starts_with("tools/") {
        resolve(root, &normalized)
    } else {
        resolve(root, &format!("tools/{normalized}"))
    };
    if !path.is_file() {
        bail!("tool not found: {}\nExpected: {}", tool, path.display());
    }
    Ok(path)
}

/// Discover tools: immediate files and one level of subdirectories under `./tools/`.
pub fn discover_tools(root: &Path) -> Result<Vec<String>> {
    let tools_root = root.join("tools");
    let mut names = Vec::new();
    if !tools_root.is_dir() {
        return Ok(names);
    }
    for entry in fs_read_dir(&tools_root)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            for sub in fs_read_dir(&path)? {
                let sub = sub?;
                let sub_path = sub.path();
                if sub_path.is_file() {
                    let sub_name = sub.file_name().to_string_lossy().into_owned();
                    if is_tool_file(&sub_name) {
                        names.push(format!("{name}/{sub_name}"));
                    }
                }
            }
        } else if is_tool_file(&name) {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn is_tool_file(name: &str) -> bool {
    name.ends_with(".py")
        || name.ends_with(".ps1")
        || name.ends_with(".rs")
        || name == "Cargo.toml"
}

fn fs_read_dir(path: &Path) -> Result<std::fs::ReadDir> {
    std::fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))
}

/// Run a project tool with structured arguments.
pub fn run_tool(
    root: &Path,
    python: &str,
    tool: &str,
    args: &[String],
    dry_run: bool,
) -> Result<ToolRun> {
    let path = resolve_tool_path(root, tool)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let (program, tool_args): (PathBuf, Vec<String>) = match ext.as_str() {
        "py" => {
            let py = find_python(python).context("python not found")?;
            let mut a = vec![path.to_string_lossy().into_owned()];
            a.extend(args.iter().cloned());
            (py, a)
        }
        "ps1" => {
            let mut a = vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                path.to_string_lossy().into_owned(),
            ];
            a.extend(args.iter().cloned());
            (PathBuf::from("powershell"), a)
        }
        "exe" => {
            let a = args.to_vec();
            (path, a)
        }
        _ => bail!(
            "unsupported tool type `{}` for {}\nSupported: .py, .ps1, .exe",
            ext,
            path.display()
        ),
    };

    eprintln!("tool {} {}", tool, args.join(" "));
    let result = run_checked(
        &format!("tool {tool}"),
        &program,
        &tool_args,
        root,
        &[],
        dry_run,
    )?;
    Ok(ToolRun {
        program,
        args: tool_args,
        result,
    })
}

/// Capture stdout/stderr from a tool (non-dry-run only).
pub fn run_tool_capture(
    root: &Path,
    python: &str,
    tool: &str,
    args: &[String],
) -> Result<ToolRun> {
    run_tool(root, python, tool, args, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_tool_under_tools() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let p = resolve_tool_path(&root, "mkfs_utils/create_exfat_image.py").unwrap();
        assert!(p.is_file());
    }

    #[test]
    fn reject_parent_traversal() {
        let root = Path::new("/repo");
        assert!(resolve_tool_path(root, "../evil.py").is_err());
    }

    #[test]
    fn discover_includes_mkfs_utils() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let tools = discover_tools(&root).unwrap();
        assert!(tools.iter().any(|t| t.contains("create_exfat_image.py")));
    }
}
