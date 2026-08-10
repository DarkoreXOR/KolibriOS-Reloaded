//! Generic filesystem primitives.

use super::path::ScriptCwd;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn exists(cwd: &ScriptCwd, path: &str) -> bool {
    cwd.resolve(path).exists()
}

pub fn read_all(cwd: &ScriptCwd, path: &str) -> std::io::Result<Vec<u8>> {
    fs::read(cwd.resolve(path))
}

pub fn write_all(cwd: &ScriptCwd, path: &str, data: &[u8]) -> std::io::Result<()> {
    let p = cwd.resolve(path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(p, data)
}

pub fn copy(cwd: &ScriptCwd, from: &str, to: &str) -> std::io::Result<u64> {
    let dest = cwd.resolve(to);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(cwd.resolve(from), dest)
}

pub fn rename(cwd: &ScriptCwd, from: &str, to: &str) -> std::io::Result<()> {
    let dest = cwd.resolve(to);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(cwd.resolve(from), dest)
}

pub fn remove_file(cwd: &ScriptCwd, path: &str) -> std::io::Result<()> {
    fs::remove_file(cwd.resolve(path))
}

pub fn dir_create(cwd: &ScriptCwd, path: &str) -> std::io::Result<()> {
    fs::create_dir_all(cwd.resolve(path))
}

pub fn dir_remove(cwd: &ScriptCwd, path: &str) -> std::io::Result<()> {
    fs::remove_dir_all(cwd.resolve(path))
}

pub fn dir_exists(cwd: &ScriptCwd, path: &str) -> bool {
    cwd.resolve(path).is_dir()
}

pub fn dir_list(cwd: &ScriptCwd, path: &str) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(cwd.resolve(path))? {
        let entry = entry?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

pub fn append(cwd: &ScriptCwd, path: &str, data: &[u8]) -> std::io::Result<()> {
    let p = cwd.resolve(path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(p)?;
    f.write_all(data)
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub len: u64,
    pub is_file: bool,
    pub is_dir: bool,
    pub readonly: bool,
}

pub fn metadata(cwd: &ScriptCwd, path: &str) -> std::io::Result<FileMeta> {
    let p = cwd.resolve(path);
    let meta = fs::metadata(&p)?;
    Ok(FileMeta {
        path: p.display().to_string(),
        len: meta.len(),
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        readonly: meta.permissions().readonly(),
    })
}

pub fn walk(cwd: &ScriptCwd, path: &str) -> std::io::Result<Vec<String>> {
    let root = cwd.resolve(path);
    let mut out = Vec::new();
    walk_inner(&root, &root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_inner(base: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let rel = p
            .strip_prefix(base)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(rel);
        if p.is_dir() {
            walk_inner(base, &p, out)?;
        }
    }
    Ok(())
}

pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

pub fn path_join(cwd: &ScriptCwd, parts: &[String]) -> String {
    if parts.is_empty() {
        return cwd.get().display().to_string();
    }
    let mut p = PathBuf::from(&parts[0]);
    for part in &parts[1..] {
        p.push(part);
    }
    if p.is_absolute() {
        p.display().to_string()
    } else {
        cwd.resolve(p.to_string_lossy().as_ref()).display().to_string()
    }
}

#[derive(Clone)]
pub struct FileHandle {
    inner: Arc<Mutex<File>>,
    path: PathBuf,
}

impl FileHandle {
    pub fn open(cwd: &ScriptCwd, path: &str, write: bool, create: bool) -> std::io::Result<Self> {
        let p = cwd.resolve(path);
        if write {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .write(write)
            .create(create)
            .open(&p)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(file)),
            path: p,
        })
    }

    pub fn read(&self, max: i64) -> std::io::Result<Vec<u8>> {
        let mut g = self.inner.lock().unwrap();
        let mut buf = vec![0u8; max.max(0) as usize];
        let n = g.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut g = self.inner.lock().unwrap();
        g.write_all(data)
    }

    pub fn seek(&self, pos: i64) -> std::io::Result<u64> {
        let mut g = self.inner.lock().unwrap();
        g.seek(SeekFrom::Start(pos.max(0) as u64))
    }

    pub fn path(&self) -> String {
        self.path.display().to_string()
    }
}

pub fn remove_path(cwd: &ScriptCwd, path: &str) -> std::io::Result<()> {
    let p = cwd.resolve(path);
    if p.is_dir() {
        fs::remove_dir_all(p)
    } else if p.exists() {
        fs::remove_file(p)
    } else {
        Ok(())
    }
}

pub fn is_file(path: &Path) -> bool {
    path.is_file()
}
