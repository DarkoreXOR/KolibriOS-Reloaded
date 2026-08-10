//! KolibriOS disk image helper — inspect FAT metadata and make disposable copies.
//!
//! This tool is intentionally separate from `rust_kernel/`. It never writes the
//! reference image path in-place; mutations always target an explicit copy path.

use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }
    let cmd = args.remove(0);
    let result = match cmd.as_str() {
        "inspect" => cmd_inspect(&args),
        "ls" => cmd_ls(&args),
        "cow" => cmd_cow(&args),
        "extract" => cmd_extract(&args),
        "delete" => cmd_delete(&args),
        "replace" => cmd_replace(&args),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown command '{other}'").into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "\
kolibri_img — KolibriOS .img inspect / CoW helper

Usage:
  kolibri_img inspect <image.img>
  kolibri_img ls <image.img> [--path DIR]
  kolibri_img cow <readonly-source.img> <dest-copy.img>
  kolibri_img extract <image.img> <FAT-NAME> <out-file>
  kolibri_img delete <writable-image.img> <FAT-NAME-OR-PATH>
  kolibri_img replace <writable-image.img> <FAT-NAME> <host-file>

Rules:
  - Treat the original reference image as read-only.
  - Use `cow` (or any explicit copy) before modify/replace/delete.
  - `delete` / `replace` refuse known reference image filenames.
  - Delete disposable copies under dev_build/ when done.
  - `delete` accepts root 8.3 names or nested paths (e.g. DEVELOP/FASM).
"
    );
}

fn refuse_reference_image(path: &str) -> Result<(), BoxError> {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    // Match the immutable reference artifact naming used in this repo.
    if name.starts_with("kolibrios-") && name.ends_with(".img") && !name.contains("tmp") {
        return Err(format!(
            "refusing to mutate apparent reference image '{name}' — cow to dev_build/ first"
        )
        .into());
    }
    Ok(())
}

type BoxError = Box<dyn std::error::Error>;

fn cmd_inspect(args: &[String]) -> Result<(), BoxError> {
    let path = args.first().ok_or("inspect requires <image.img>")?;
    let img = Image::open(path)?;
    let b = &img.bpb;
    println!("path: {}", path);
    println!("size_bytes: {}", img.data.len());
    println!("oem: {:?}", b.oem);
    println!("bytes_per_sector: {}", b.bytes_per_sector);
    println!("sectors_per_cluster: {}", b.sectors_per_cluster);
    println!("reserved_sectors: {}", b.reserved_sectors);
    println!("fat_count: {}", b.fat_count);
    println!("root_entries: {}", b.root_entries);
    println!("total_sectors: {}", b.total_sectors);
    println!("sectors_per_fat: {}", b.sectors_per_fat);
    println!("sectors_per_track: {}", b.sectors_per_track);
    println!("heads: {}", b.heads);
    println!("media: 0x{:02X}", b.media);
    println!("fs_type_field: {:?}", b.fs_type);
    println!("fat_kind: {:?}", b.fat_kind);
    println!("boot_signature: 0x{:02X}{:02X}", img.data[510], img.data[511]);
    println!("fat_region_offset: {}", b.fat_offset());
    println!("root_dir_offset: {}", b.root_offset());
    println!("data_region_offset: {}", b.data_offset());
    if let Some(e) = img.find_root_file("KERNEL  MNT") {
        println!(
            "KERNEL.MNT: cluster={} size={}",
            e.start_cluster, e.size
        );
    } else {
        println!("KERNEL.MNT: not found in root");
    }
    Ok(())
}

fn cmd_ls(args: &[String]) -> Result<(), BoxError> {
    let path = args.first().ok_or("ls requires <image.img>")?;
    let mut dir_path = String::new();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--path" {
            dir_path = args.get(i + 1).cloned().unwrap_or_default();
            i += 2;
        } else {
            return Err(format!("unexpected argument '{}'", args[i]).into());
        }
    }
    let img = Image::open(path)?;
    let entries = if dir_path.is_empty() || dir_path == "/" || dir_path == "\\" {
        img.list_root()?
    } else {
        img.list_path(&dir_path)?
    };
    println!("{:<12} {:<6} {:>8} {:>8}", "NAME", "ATTR", "CLUSTER", "SIZE");
    for e in entries {
        if e.is_volume_label() || e.is_lfn() {
            continue;
        }
        let display = e.display_name();
        println!(
            "{:<12} {:<6} {:>8} {:>8}",
            display,
            format!("0x{:02X}", e.attr),
            e.start_cluster,
            e.size
        );
    }
    Ok(())
}

fn cmd_cow(args: &[String]) -> Result<(), BoxError> {
    let src = args.first().ok_or("cow requires <source.img> <dest.img>")?;
    let dst = args.get(1).ok_or("cow requires <source.img> <dest.img>")?;
    let src_path = Path::new(src);
    let dst_path = Path::new(dst);
    if !src_path.exists() {
        return Err(format!("source not found: {src}").into());
    }
    if same_path(src_path, dst_path)? {
        return Err("refusing to cow onto the same path (reference must stay read-only)".into());
    }
    if let Some(parent) = dst_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::copy(src_path, dst_path)?;
    // Clear read-only attribute on the copy if inherited.
    let mut perms = fs::metadata(dst_path)?.permissions();
    #[cfg(windows)]
    {
        perms.set_readonly(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o644);
    }
    fs::set_permissions(dst_path, perms)?;
    println!("created disposable copy: {} ({} bytes)", dst, fs::metadata(dst)?.len());
    Ok(())
}

fn cmd_extract(args: &[String]) -> Result<(), BoxError> {
    let img_path = args.first().ok_or("extract requires <image.img> <NAME> <out>")?;
    let name = args.get(1).ok_or("extract requires <image.img> <NAME> <out>")?;
    let out = args.get(2).ok_or("extract requires <image.img> <NAME> <out>")?;
    let img = Image::open(img_path)?;
    let fat_name = to_fat_83(name)?;
    let entry = img
        .find_root_file(&fat_name)
        .ok_or_else(|| format!("file not found in root: {name}"))?;
    if entry.is_dir() {
        return Err("refusing to extract a directory".into());
    }
    let bytes = img.read_file(&entry)?;
    if let Some(parent) = Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(out, &bytes)?;
    println!("wrote {} bytes to {out}", bytes.len());
    Ok(())
}

fn cmd_delete(args: &[String]) -> Result<(), BoxError> {
    let img_path = args.first().ok_or("delete requires <image.img> <NAME>")?;
    let name = args.get(1).ok_or("delete requires <image.img> <NAME>")?;
    refuse_reference_image(img_path)?;
    let mut img = Image::open_mut(img_path)?;
    let (offset, entry) = img
        .find_file_path(name)?
        .ok_or_else(|| format!("file not found: {name}"))?;
    if entry.is_dir() {
        return Err("refusing to delete a directory (not implemented)".into());
    }
    let start = entry.start_cluster;
    let size = entry.size;
    img.free_chain(start)?;
    img.mark_dirent_deleted(offset)?;
    img.mirror_fats()?;
    img.save(img_path)?;
    println!("deleted {name} (was cluster={start} size={size})");
    Ok(())
}

fn cmd_replace(args: &[String]) -> Result<(), BoxError> {
    let img_path = args
        .first()
        .ok_or("replace requires <image.img> <NAME> <host-file>")?;
    let name = args
        .get(1)
        .ok_or("replace requires <image.img> <NAME> <host-file>")?;
    let host = args
        .get(2)
        .ok_or("replace requires <image.img> <NAME> <host-file>")?;
    refuse_reference_image(img_path)?;
    let new_bytes = fs::read(host)?;
    let mut img = Image::open_mut(img_path)?;
    let fat_name = to_fat_83(name)?;
    let (offset, entry) = img
        .find_root_file_mut(&fat_name)?
        .ok_or_else(|| format!("file not found in root: {name}"))?;
    if entry.is_dir() {
        return Err("refusing to replace a directory".into());
    }
    let old_start = entry.start_cluster;
    let old_size = entry.size;
    img.free_chain(old_start)?;
    let new_start = if new_bytes.is_empty() {
        0
    } else {
        img.allocate_and_write(&new_bytes)?
    };
    img.update_dirent(offset, new_start, new_bytes.len() as u32)?;
    img.mirror_fats()?;
    img.save(img_path)?;
    println!(
        "replaced {name}: {old_size} bytes @cluster {old_start} -> {} bytes @cluster {new_start}",
        new_bytes.len()
    );
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> Result<bool, BoxError> {
    let a = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b_parent = b
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let b_name = b.file_name().ok_or("dest has no file name")?;
    let b_full = fs::canonicalize(b_parent)
        .unwrap_or_else(|_| b_parent.to_path_buf())
        .join(b_name);
    Ok(a == b_full)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FatKind {
    Fat12,
    Fat16,
}

#[derive(Debug)]
struct Bpb {
    oem: String,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_count: u8,
    root_entries: u16,
    total_sectors: u32,
    media: u8,
    sectors_per_fat: u16,
    sectors_per_track: u16,
    heads: u16,
    fs_type: String,
    fat_kind: FatKind,
}

impl Bpb {
    fn parse(data: &[u8]) -> Result<Self, BoxError> {
        if data.len() < 512 {
            return Err("image shorter than one sector".into());
        }
        if data[510] != 0x55 || data[511] != 0xAA {
            return Err("missing 0x55AA boot signature".into());
        }
        let bytes_per_sector = u16::from_le_bytes([data[11], data[12]]);
        if bytes_per_sector == 0 || !bytes_per_sector.is_power_of_two() {
            return Err("invalid bytes_per_sector".into());
        }
        let sectors_per_cluster = data[13];
        let reserved_sectors = u16::from_le_bytes([data[14], data[15]]);
        let fat_count = data[16];
        let root_entries = u16::from_le_bytes([data[17], data[18]]);
        let total_sectors_16 = u16::from_le_bytes([data[19], data[20]]);
        let media = data[21];
        let sectors_per_fat = u16::from_le_bytes([data[22], data[23]]);
        let sectors_per_track = u16::from_le_bytes([data[24], data[25]]);
        let heads = u16::from_le_bytes([data[26], data[27]]);
        let total_sectors_32 = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);
        let total_sectors = if total_sectors_16 != 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };
        let oem = String::from_utf8_lossy(&data[3..11]).trim_end().to_string();
        let fs_type = String::from_utf8_lossy(&data[54..62]).trim_end().to_string();

        let root_dir_sectors =
            ((root_entries as u32 * 32) + (bytes_per_sector as u32 - 1)) / bytes_per_sector as u32;
        let data_sectors = total_sectors
            .saturating_sub(reserved_sectors as u32)
            .saturating_sub(fat_count as u32 * sectors_per_fat as u32)
            .saturating_sub(root_dir_sectors);
        let clusters = if sectors_per_cluster == 0 {
            0
        } else {
            data_sectors / sectors_per_cluster as u32
        };
        let fat_kind = if clusters < 4085 {
            FatKind::Fat12
        } else {
            FatKind::Fat16
        };

        Ok(Self {
            oem,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            fat_count,
            root_entries,
            total_sectors,
            media,
            sectors_per_fat,
            sectors_per_track,
            heads,
            fs_type,
            fat_kind,
        })
    }

    fn fat_offset(&self) -> usize {
        self.reserved_sectors as usize * self.bytes_per_sector as usize
    }

    fn root_offset(&self) -> usize {
        self.fat_offset()
            + self.fat_count as usize
                * self.sectors_per_fat as usize
                * self.bytes_per_sector as usize
    }

    fn root_size(&self) -> usize {
        self.root_entries as usize * 32
    }

    fn data_offset(&self) -> usize {
        self.root_offset() + self.root_size()
    }

    fn cluster_size(&self) -> usize {
        self.sectors_per_cluster as usize * self.bytes_per_sector as usize
    }
}

#[derive(Debug, Clone)]
struct DirEntry {
    raw_name: [u8; 11],
    attr: u8,
    start_cluster: u16,
    size: u32,
}

impl DirEntry {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 32 || buf[0] == 0x00 {
            return None;
        }
        if buf[0] == 0xE5 {
            return Some(Self {
                raw_name: [0xE5; 11],
                attr: buf[11],
                start_cluster: 0,
                size: 0,
            });
        }
        let mut raw_name = [0u8; 11];
        raw_name.copy_from_slice(&buf[0..11]);
        let attr = buf[11];
        let start_cluster = u16::from_le_bytes([buf[26], buf[27]]);
        let size = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
        Some(Self {
            raw_name,
            attr,
            start_cluster,
            size,
        })
    }

    fn is_deleted(&self) -> bool {
        self.raw_name[0] == 0xE5
    }

    fn is_lfn(&self) -> bool {
        self.attr == 0x0F
    }

    fn is_volume_label(&self) -> bool {
        self.attr & 0x08 != 0 && self.attr & 0x10 == 0
    }

    fn is_dir(&self) -> bool {
        self.attr & 0x10 != 0
    }

    fn fat_name(&self) -> String {
        String::from_utf8_lossy(&self.raw_name).to_string()
    }

    fn display_name(&self) -> String {
        let name = String::from_utf8_lossy(&self.raw_name[0..8])
            .trim_end()
            .to_string();
        let ext = String::from_utf8_lossy(&self.raw_name[8..11])
            .trim_end()
            .to_string();
        if ext.is_empty() {
            name
        } else {
            format!("{name}.{ext}")
        }
    }
}

struct Image {
    data: Vec<u8>,
    bpb: Bpb,
}

impl Image {
    fn open(path: &str) -> Result<Self, BoxError> {
        let mut f = fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        let bpb = Bpb::parse(&data)?;
        Ok(Self { data, bpb })
    }

    fn open_mut(path: &str) -> Result<Self, BoxError> {
        Self::open(path)
    }

    fn save(&self, path: &str) -> Result<(), BoxError> {
        fs::write(path, &self.data)?;
        Ok(())
    }

    fn list_root(&self) -> Result<Vec<DirEntry>, BoxError> {
        self.parse_dir_region(self.bpb.root_offset(), self.bpb.root_size())
    }

    fn find_root_file(&self, fat_name_11: &str) -> Option<DirEntry> {
        self.find_root_file_offset(fat_name_11)
            .map(|(_, e)| e)
    }

    fn find_root_file_mut(&self, fat_name_11: &str) -> Result<Option<(usize, DirEntry)>, BoxError> {
        Ok(self.find_root_file_offset(fat_name_11))
    }

    fn find_root_file_offset(&self, fat_name_11: &str) -> Option<(usize, DirEntry)> {
        let want = fat_name_11.as_bytes();
        let base = self.bpb.root_offset();
        let size = self.bpb.root_size();
        let mut off = 0usize;
        while off + 32 <= size {
            let abs = base + off;
            let chunk = &self.data[abs..abs + 32];
            if chunk[0] == 0x00 {
                break;
            }
            if let Some(e) = DirEntry::parse(chunk) {
                if !e.is_deleted()
                    && !e.is_lfn()
                    && !e.is_volume_label()
                    && e.raw_name.as_ref() == want
                {
                    return Some((abs, e));
                }
            }
            off += 32;
        }
        None
    }

    /// Locate a file by root 8.3 name or nested path (`DEVELOP/FASM`, `/games/dino`).
    /// Returns absolute image offset of the 32-byte dirent plus the parsed entry.
    fn find_file_path(&self, path: &str) -> Result<Option<(usize, DirEntry)>, BoxError> {
        let components: Vec<&str> = path
            .split(|c| c == '/' || c == '\\')
            .filter(|s| !s.is_empty() && *s != ".")
            .collect();
        if components.is_empty() {
            return Err("empty path".into());
        }
        if components.len() == 1 {
            let fat = to_fat_83(components[0])?;
            return Ok(self.find_root_file_offset(&fat));
        }
        // Walk parent directories from root; last component is the file.
        let mut parent_start: Option<u16> = None;
        let mut walk = self.list_root()?;
        for component in &components[..components.len() - 1] {
            let fat = to_fat_83(component)?;
            let dir = walk
                .into_iter()
                .find(|e| !e.is_deleted() && !e.is_lfn() && e.is_dir() && e.fat_name() == fat)
                .ok_or_else(|| format!("directory not found: {component}"))?;
            parent_start = Some(dir.start_cluster);
            let bytes = self.read_clusters(dir.start_cluster, None)?;
            walk = self.parse_dir_region_bytes(&bytes)?;
        }
        let file_fat = to_fat_83(components[components.len() - 1])?;
        let start = parent_start.ok_or("internal: missing parent cluster")?;
        self.find_in_cluster_dir(start, &file_fat)
    }

    fn find_in_cluster_dir(
        &self,
        start: u16,
        fat_name_11: &str,
    ) -> Result<Option<(usize, DirEntry)>, BoxError> {
        let want = fat_name_11.as_bytes();
        let cluster_size = self.bpb.cluster_size();
        let mut cluster = start as u32;
        let mut guard = 0u32;
        while cluster >= 2 && !self.is_eoc(cluster) {
            let base = self.cluster_offset(cluster)?;
            let mut off = 0usize;
            while off + 32 <= cluster_size {
                let abs = base + off;
                let chunk = &self.data[abs..abs + 32];
                if chunk[0] == 0x00 {
                    return Ok(None);
                }
                if let Some(e) = DirEntry::parse(chunk) {
                    if !e.is_deleted()
                        && !e.is_lfn()
                        && !e.is_volume_label()
                        && e.raw_name.as_ref() == want
                    {
                        return Ok(Some((abs, e)));
                    }
                }
                off += 32;
            }
            cluster = self.fat_next(cluster)?;
            guard += 1;
            if guard > 1_000_000 {
                return Err("FAT chain too long / loop detected".into());
            }
        }
        Ok(None)
    }

    fn list_path(&self, path: &str) -> Result<Vec<DirEntry>, BoxError> {
        let mut entries = self.list_root()?;
        for component in path
            .split(|c| c == '/' || c == '\\')
            .filter(|s| !s.is_empty() && *s != ".")
        {
            let fat = to_fat_83(component)?;
            let dir = entries
                .into_iter()
                .find(|e| {
                    !e.is_deleted()
                        && !e.is_lfn()
                        && e.is_dir()
                        && e.fat_name() == fat
                })
                .ok_or_else(|| format!("directory not found: {component}"))?;
            let bytes = self.read_clusters(dir.start_cluster, None)?;
            entries = self.parse_dir_region_bytes(&bytes)?;
        }
        Ok(entries)
    }

    fn parse_dir_region(&self, offset: usize, size: usize) -> Result<Vec<DirEntry>, BoxError> {
        if offset + size > self.data.len() {
            return Err("directory region out of bounds".into());
        }
        self.parse_dir_region_bytes(&self.data[offset..offset + size])
    }

    fn parse_dir_region_bytes(&self, region: &[u8]) -> Result<Vec<DirEntry>, BoxError> {
        let mut out = Vec::new();
        for chunk in region.chunks_exact(32) {
            match DirEntry::parse(chunk) {
                None => break,
                Some(e) if e.is_deleted() => continue,
                Some(e) => out.push(e),
            }
        }
        Ok(out)
    }

    fn read_file(&self, entry: &DirEntry) -> Result<Vec<u8>, BoxError> {
        let mut data = self.read_clusters(entry.start_cluster, Some(entry.size as usize))?;
        data.truncate(entry.size as usize);
        Ok(data)
    }

    fn read_clusters(&self, start: u16, max_bytes: Option<usize>) -> Result<Vec<u8>, BoxError> {
        if start < 2 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let mut cluster = start as u32;
        let mut guard = 0u32;
        let cluster_size = self.bpb.cluster_size();
        while cluster >= 2 && !self.is_eoc(cluster) {
            let off = self.cluster_offset(cluster)?;
            let end = off + cluster_size;
            if end > self.data.len() {
                return Err("cluster points past end of image".into());
            }
            out.extend_from_slice(&self.data[off..end]);
            if let Some(max) = max_bytes {
                if out.len() >= max {
                    break;
                }
            }
            cluster = self.fat_next(cluster)?;
            guard += 1;
            if guard > 1_000_000 {
                return Err("FAT chain too long / loop detected".into());
            }
        }
        Ok(out)
    }

    fn cluster_offset(&self, cluster: u32) -> Result<usize, BoxError> {
        if cluster < 2 {
            return Err("invalid cluster < 2".into());
        }
        Ok(self.bpb.data_offset() + (cluster as usize - 2) * self.bpb.cluster_size())
    }

    fn fat_next(&self, cluster: u32) -> Result<u32, BoxError> {
        self.fat_get(cluster)
    }

    fn fat_get(&self, cluster: u32) -> Result<u32, BoxError> {
        match self.bpb.fat_kind {
            FatKind::Fat12 => {
                let fat = &self.data[self.bpb.fat_offset()..];
                let i = cluster as usize;
                let offset = i + (i / 2);
                if offset + 1 >= self.bpb.sectors_per_fat as usize * self.bpb.bytes_per_sector as usize
                {
                    return Err("FAT12 index out of range".into());
                }
                let val = u16::from_le_bytes([fat[offset], fat[offset + 1]]);
                Ok(if i & 1 == 0 {
                    (val & 0x0FFF) as u32
                } else {
                    (val >> 4) as u32
                })
            }
            FatKind::Fat16 => {
                let fat = &self.data[self.bpb.fat_offset()..];
                let offset = cluster as usize * 2;
                if offset + 1 >= self.bpb.sectors_per_fat as usize * self.bpb.bytes_per_sector as usize
                {
                    return Err("FAT16 index out of range".into());
                }
                Ok(u16::from_le_bytes([fat[offset], fat[offset + 1]]) as u32)
            }
        }
    }

    fn fat_set(&mut self, cluster: u32, value: u32) -> Result<(), BoxError> {
        match self.bpb.fat_kind {
            FatKind::Fat12 => {
                let fat_off = self.bpb.fat_offset();
                let i = cluster as usize;
                let offset = fat_off + i + (i / 2);
                if offset + 1 >= self.data.len() {
                    return Err("FAT12 write out of range".into());
                }
                let mut cur = u16::from_le_bytes([self.data[offset], self.data[offset + 1]]);
                if i & 1 == 0 {
                    cur = (cur & 0xF000) | ((value as u16) & 0x0FFF);
                } else {
                    cur = (cur & 0x000F) | (((value as u16) & 0x0FFF) << 4);
                }
                self.data[offset] = (cur & 0xFF) as u8;
                self.data[offset + 1] = (cur >> 8) as u8;
                Ok(())
            }
            FatKind::Fat16 => {
                let offset = self.bpb.fat_offset() + cluster as usize * 2;
                if offset + 1 >= self.data.len() {
                    return Err("FAT16 write out of range".into());
                }
                let v = (value as u16).to_le_bytes();
                self.data[offset] = v[0];
                self.data[offset + 1] = v[1];
                Ok(())
            }
        }
    }

    fn is_eoc(&self, cluster: u32) -> bool {
        match self.bpb.fat_kind {
            FatKind::Fat12 => cluster >= 0x0FF8,
            FatKind::Fat16 => cluster >= 0xFFF8,
        }
    }

    fn eoc_value(&self) -> u32 {
        match self.bpb.fat_kind {
            FatKind::Fat12 => 0x0FFF,
            FatKind::Fat16 => 0xFFFF,
        }
    }

    fn max_cluster(&self) -> u32 {
        let data_bytes = self.data.len().saturating_sub(self.bpb.data_offset());
        let clusters = data_bytes / self.bpb.cluster_size();
        (clusters as u32).saturating_add(1) // last valid cluster index = clusters+1? clusters are 2..(n+1)
            .max(2)
    }

    fn free_chain(&mut self, start: u16) -> Result<(), BoxError> {
        if start < 2 {
            return Ok(());
        }
        let mut cluster = start as u32;
        let mut guard = 0u32;
        while cluster >= 2 && !self.is_eoc(cluster) {
            let next = self.fat_get(cluster)?;
            self.fat_set(cluster, 0)?;
            cluster = next;
            guard += 1;
            if guard > 1_000_000 {
                return Err("FAT free: chain too long / loop".into());
            }
        }
        Ok(())
    }

    fn alloc_cluster(&mut self) -> Result<u32, BoxError> {
        let max = self.max_cluster();
        for c in 2..=max {
            if self.fat_get(c)? == 0 {
                self.fat_set(c, self.eoc_value())?;
                return Ok(c);
            }
        }
        Err("no free clusters on image".into())
    }

    fn allocate_and_write(&mut self, bytes: &[u8]) -> Result<u16, BoxError> {
        let cluster_size = self.bpb.cluster_size();
        let mut remaining = bytes;
        let first = self.alloc_cluster()?;
        let mut prev = first;
        loop {
            let off = self.cluster_offset(prev)?;
            let end = off + cluster_size;
            if end > self.data.len() {
                return Err("allocated cluster past end of image".into());
            }
            // clear then copy
            for b in &mut self.data[off..end] {
                *b = 0;
            }
            let take = remaining.len().min(cluster_size);
            self.data[off..off + take].copy_from_slice(&remaining[..take]);
            remaining = &remaining[take..];
            if remaining.is_empty() {
                self.fat_set(prev, self.eoc_value())?;
                break;
            }
            let next = self.alloc_cluster()?;
            self.fat_set(prev, next)?;
            prev = next;
        }
        Ok(first as u16)
    }

    fn mark_dirent_deleted(&mut self, offset: usize) -> Result<(), BoxError> {
        if offset >= self.data.len() {
            return Err("dirent offset out of range".into());
        }
        self.data[offset] = 0xE5;
        Ok(())
    }

    fn update_dirent(&mut self, offset: usize, start: u16, size: u32) -> Result<(), BoxError> {
        if offset + 32 > self.data.len() {
            return Err("dirent offset out of range".into());
        }
        let sc = start.to_le_bytes();
        self.data[offset + 26] = sc[0];
        self.data[offset + 27] = sc[1];
        // high word of cluster (FAT16/32) — keep 0 for FAT12
        self.data[offset + 20] = 0;
        self.data[offset + 21] = 0;
        let sz = size.to_le_bytes();
        self.data[offset + 28] = sz[0];
        self.data[offset + 29] = sz[1];
        self.data[offset + 30] = sz[2];
        self.data[offset + 31] = sz[3];
        Ok(())
    }

    fn mirror_fats(&mut self) -> Result<(), BoxError> {
        let fat_size = self.bpb.sectors_per_fat as usize * self.bpb.bytes_per_sector as usize;
        let first = self.bpb.fat_offset();
        for copy in 1..self.bpb.fat_count {
            let dst = first + copy as usize * fat_size;
            if dst + fat_size > self.data.len() {
                return Err("FAT mirror out of range".into());
            }
            self.data.copy_within(first..first + fat_size, dst);
        }
        Ok(())
    }
}

fn to_fat_83(name: &str) -> Result<String, BoxError> {
    let name = name.trim().trim_start_matches(['/', '\\']);
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return Err(format!("invalid 8.3 name: {name}").into());
    }
    if base.contains('.') || base.contains(' ') && base.len() > 8 {
        return Err(format!("invalid 8.3 name: {name}").into());
    }
    let mut out = vec![b' '; 11];
    for (i, b) in base.bytes().take(8).enumerate() {
        out[i] = b.to_ascii_uppercase();
    }
    for (i, b) in ext.bytes().take(3).enumerate() {
        out[8 + i] = b.to_ascii_uppercase();
    }
    Ok(String::from_utf8(out)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat83_kernel() {
        assert_eq!(to_fat_83("KERNEL.MNT").unwrap(), "KERNEL  MNT");
        assert_eq!(to_fat_83("kernel.mnt").unwrap(), "KERNEL  MNT");
    }
}
