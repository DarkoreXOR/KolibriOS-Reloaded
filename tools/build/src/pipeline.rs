//! Build → image → QEMU pipeline stages.

use crate::config::{resolve, BlobKind, Config};
use crate::util::{find_python, format_cmdline, resolve_tool, run_checked, run_inherit, which_on_path};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Pipeline<'a> {
    pub cfg: &'a Config,
    pub root: &'a Path,
    pub dry_run: bool,
    pub skip_tests: bool,
    pub headless: bool,
    /// Set only after a successful kernel assemble in this process.
    pub kernel_built_ok: bool,
    pub last_kernel: Option<PathBuf>,
    pub last_image: Option<PathBuf>,
}

impl<'a> Pipeline<'a> {
    pub fn new(
        cfg: &'a Config,
        root: &'a Path,
        dry_run: bool,
        skip_tests: bool,
        headless: bool,
    ) -> Self {
        Self {
            cfg,
            root,
            dry_run,
            skip_tests,
            headless,
            kernel_built_ok: false,
            last_kernel: None,
            last_image: None,
        }
    }

    pub fn print_summary_paths(&self) {
        eprintln!("Kernel:");
        match &self.last_kernel {
            Some(p) => eprintln!("  {}", p.display()),
            None => eprintln!("  (not built this run)"),
        }
        eprintln!("Test image:");
        match &self.last_image {
            Some(p) => eprintln!("  {}", p.display()),
            None => eprintln!("  (not created this run)"),
        }
        if let Ok(q) = self.resolve_qemu() {
            eprintln!("QEMU:");
            eprintln!("  {}", q.display());
        }
    }

    pub fn build_rust(&mut self) -> Result<()> {
        eprintln!("== Rust ==");
        eprintln!("Building current Rust components...");

        let workspace = resolve(self.root, &self.cfg.rust.workspace);
        let target_json = resolve(self.root, &self.cfg.rust.target_json);
        let target_dir = resolve(self.root, &self.cfg.rust.cargo_target_dir);
        let out_dir = resolve(self.root, &self.cfg.rust.out_dir);
        let package = self.cfg.rust.package.clone();
        let toolchain = self.cfg.rust.toolchain.clone();

        if !workspace.is_dir() {
            bail!(
                "ERROR: rust workspace missing\nExpected: {}",
                workspace.display()
            );
        }
        if !target_json.is_file() {
            bail!(
                "ERROR: freestanding target JSON missing\nExpected: {}",
                target_json.display()
            );
        }

        let cargo = which_on_path("cargo").context("ERROR: `cargo` not found on PATH")?;

        if self.cfg.rust.run_host_tests && !self.skip_tests {
            eprintln!("  host tests: cargo test -p {package}");
            self.run_cargo(
                &cargo,
                &["test".into(), "-p".into(), package.clone()],
                &workspace,
                &target_dir,
            )?;
        } else if self.skip_tests {
            eprintln!("  host tests: skipped (--skip-tests)");
        }

        if self.cfg.rust.force_recompile_staticlib && !self.dry_run {
            self.invalidate_staticlib(&target_dir)?;
        }

        eprintln!("  freestanding staticlib (release, {toolchain})");
        self.run_cargo(
            &cargo,
            &[
                format!("+{toolchain}"),
                "build".into(),
                "-Z".into(),
                "build-std=core,compiler_builtins".into(),
                "-Z".into(),
                "json-target-spec".into(),
                "-p".into(),
                package,
                "--release".into(),
                "--target".into(),
                target_json.to_string_lossy().into_owned(),
            ],
            &workspace,
            &target_dir,
        )?;

        let archive = target_dir
            .join("i686-kolibri-none")
            .join("release")
            .join("libkolibri_utils.a");
        if !self.dry_run && !archive.is_file() {
            bail!(
                "ERROR: freestanding archive missing after build\nExpected: {}",
                archive.display()
            );
        }

        if !self.dry_run {
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("mkdir {}", out_dir.display()))?;
        } else {
            eprintln!("  mkdir {}", out_dir.display());
        }

        let python = find_python(&self.cfg.rust.extract.python).with_context(|| {
            format!(
                "ERROR: Python not found (configured `{}`)",
                self.cfg.rust.extract.python
            )
        })?;
        let generic = resolve(self.root, &self.cfg.rust.extract.generic_script);
        let probe = resolve(self.root, &self.cfg.rust.extract.probe_script);

        for blob in &self.cfg.rust.blobs {
            let out = out_dir.join(&blob.out);
            match blob.kind {
                BlobKind::Generic => {
                    let section = blob.section.as_ref().unwrap();
                    let symbol = blob.symbol.as_ref().unwrap();
                    let ret = blob.expect_ret_imm.unwrap().to_string();
                    eprintln!("  extract {}", blob.out);
                    let args = vec![
                        generic.to_string_lossy().into_owned(),
                        "--archive".into(),
                        archive.to_string_lossy().into_owned(),
                        "--section".into(),
                        section.clone(),
                        "--symbol".into(),
                        symbol.clone(),
                        "--expect-ret-imm".into(),
                        ret,
                        "--out".into(),
                        out.to_string_lossy().into_owned(),
                    ];
                    run_checked(
                        &format!("blob extract ({})", blob.out),
                        &python,
                        &args,
                        self.root,
                        &[],
                        self.dry_run,
                    )?;
                }
                BlobKind::Probe => {
                    eprintln!("  extract {} (phase-c probe)", blob.out);
                    let args = vec![
                        probe.to_string_lossy().into_owned(),
                        "--archive".into(),
                        archive.to_string_lossy().into_owned(),
                        "--out".into(),
                        out.to_string_lossy().into_owned(),
                    ];
                    run_checked(
                        &format!("blob extract ({})", blob.out),
                        &python,
                        &args,
                        self.root,
                        &[],
                        self.dry_run,
                    )?;
                }
            }
            if !self.dry_run && !out.is_file() {
                bail!(
                    "ERROR: expected blob missing after extract\nExpected: {}",
                    out.display()
                );
            }
        }

        Ok(())
    }

    fn invalidate_staticlib(&self, target_dir: &Path) -> Result<()> {
        let fs_out = target_dir.join("i686-kolibri-none").join("release");
        let archive = fs_out.join("libkolibri_utils.a");
        if archive.exists() {
            eprintln!("  invalidate {}", archive.display());
            fs::remove_file(&archive)
                .with_context(|| format!("remove {}", archive.display()))?;
        }
        let deps = fs_out.join("deps");
        if deps.is_dir() {
            for entry in fs::read_dir(&deps).with_context(|| format!("read {}", deps.display()))? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("kolibri_utils-") {
                    let p = entry.path();
                    eprintln!("  invalidate {}", p.display());
                    if p.is_dir() {
                        fs::remove_dir_all(&p)?;
                    } else {
                        fs::remove_file(&p)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn run_cargo(
        &self,
        cargo: &Path,
        args: &[String],
        cwd: &Path,
        target_dir: &Path,
    ) -> Result<()> {
        let cmdline = format_cmdline(cargo.as_os_str(), args);
        let target_dir_s = target_dir.to_string_lossy();
        eprintln!("  $ {cmdline}");
        eprintln!("    cwd: {}", cwd.display());
        eprintln!("    CARGO_TARGET_DIR={target_dir_s}");
        if self.cfg.rust.clear_rustflags {
            eprintln!("    RUSTFLAGS=<cleared>");
        }

        if self.dry_run {
            return Ok(());
        }

        let mut cmd = Command::new(cargo);
        cmd.args(args)
            .current_dir(cwd)
            .env("CARGO_TARGET_DIR", target_dir.as_os_str())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.cfg.rust.clear_rustflags {
            cmd.env_remove("RUSTFLAGS");
        }

        let output = cmd.output().with_context(|| {
            format!(
                "ERROR: cargo failed to start\nCommand: {cmdline}\nWorking directory: {}",
                cwd.display()
            )
        })?;
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        if !output.status.success() {
            let status = output.status.code().unwrap_or(1);
            bail!(
                "ERROR: cargo failed (exit {status})\nCommand: {cmdline}\nWorking directory: {}",
                cwd.display()
            );
        }
        Ok(())
    }

    pub fn build_kernel(&mut self) -> Result<()> {
        eprintln!("== Kernel ==");
        eprintln!("Building kernel...");

        // Never claim success with a pre-existing artifact if assemble fails later.
        self.kernel_built_ok = false;
        self.last_kernel = None;

        let fasm = resolve(self.root, &self.cfg.kernel.fasm);
        let asm = resolve(self.root, &self.cfg.kernel.asm);
        let out = resolve(self.root, &self.cfg.kernel.output);
        let lang_inc = self.root.join("kernel").join("lang.inc");
        let bin_dir = out.parent().context("kernel.output has no parent")?;

        if !asm.is_file() && !self.dry_run {
            bail!("ERROR: kernel asm missing\nExpected: {}", asm.display());
        }

        let out_dir = resolve(self.root, &self.cfg.rust.out_dir);
        for blob in &self.cfg.rust.blobs {
            let p = out_dir.join(&blob.out);
            if !self.dry_run && !p.is_file() {
                bail!(
                    "ERROR: Rust blob missing before kernel assemble\nExpected: {}\nRun the Rust build stage first.",
                    p.display()
                );
            }
        }

        self.apply_migration_gates()?;

        if !self.dry_run {
            fs::create_dir_all(bin_dir)
                .with_context(|| format!("mkdir {}", bin_dir.display()))?;
            // Remove stale output *before* assemble so a failed build cannot leave an
            // old kernel.mnt that looks current on disk.
            if out.exists() {
                fs::remove_file(&out)
                    .with_context(|| format!("remove stale {}", out.display()))?;
            }
        } else {
            eprintln!("  remove stale {}", out.display());
        }

        if !fasm.is_file() && !self.dry_run {
            bail!("ERROR: FASM missing\nExpected: {}", fasm.display());
        }

        if !self.dry_run {
            let lang_body = format!("lang fix {}\n", self.cfg.kernel.lang);
            fs::write(&lang_inc, lang_body)
                .with_context(|| format!("write {}", lang_inc.display()))?;
        } else {
            eprintln!("  write {}", lang_inc.display());
        }

        let mem = self.cfg.kernel.memory_kib.to_string();
        let args = [
            "-m".to_string(),
            mem,
            asm.to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
        ];

        let assemble = run_checked(
            "kernel assemble (FASM)",
            &fasm,
            &args,
            self.root,
            &[],
            self.dry_run,
        );

        if !self.dry_run {
            let _ = fs::remove_file(&lang_inc);
        } else {
            eprintln!("  remove {}", lang_inc.display());
        }

        assemble?;

        if !self.dry_run {
            if !out.is_file() {
                bail!(
                    "ERROR: kernel artifact missing after assemble\nExpected: {}",
                    out.display()
                );
            }
            let len = fs::metadata(&out)?.len();
            eprintln!("  wrote {} ({} bytes)", out.display(), len);
        }

        self.kernel_built_ok = true;
        self.last_kernel = Some(out);
        Ok(())
    }

    pub fn build_all(&mut self) -> Result<()> {
        self.build_rust()?;
        self.build_kernel()?;
        Ok(())
    }

    /// Sync each `USE_RUST_*` assignment in `gate_file` to `migrations[].enabled`.
    /// No-op when already matching (keeps the tree clean for production defaults).
    fn apply_migration_gates(&self) -> Result<()> {
        if self.cfg.rust.migrations.is_empty() {
            return Ok(());
        }
        eprintln!(
            "  migration gates: {} registered",
            self.cfg.rust.migrations.len()
        );
        for m in &self.cfg.rust.migrations {
            let want = if m.enabled { 1u32 } else { 0u32 };
            let path = resolve(self.root, &m.gate_file);
            if self.dry_run {
                eprintln!("  gate {} → {} ({})", m.gate, want, m.gate_file);
                continue;
            }
            if !path.is_file() {
                bail!(
                    "ERROR: migration gate_file missing for {}\nExpected: {}",
                    m.gate,
                    path.display()
                );
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            let mut found = false;
            let mut changed = false;
            let mut out = String::with_capacity(text.len());
            for line in text.lines() {
                let trimmed = line.trim();
                // Match `USE_RUST_FOO = N` optionally followed by a comment.
                let is_assign = trimmed
                    .strip_prefix(&m.gate)
                    .and_then(|rest| rest.trim_start().strip_prefix('='))
                    .is_some()
                    && !trimmed.starts_with(';');
                if is_assign {
                    found = true;
                    let indent_len = line.len() - line.trim_start().len();
                    let indent = &line[..indent_len];
                    let expected = format!("{} = {}", m.gate, want);
                    if trimmed == expected {
                        out.push_str(line);
                    } else {
                        changed = true;
                        out.push_str(indent);
                        out.push_str(&expected);
                    }
                } else {
                    out.push_str(line);
                }
                out.push('\n');
            }
            // Preserve absence of final newline only if original lacked one.
            if !text.ends_with('\n') && out.ends_with('\n') {
                out.pop();
            }
            if !found {
                bail!(
                    "ERROR: gate `{}` not found in {}\nCannot apply migrations[].enabled",
                    m.gate,
                    m.gate_file
                );
            }
            if changed {
                fs::write(&path, out).with_context(|| format!("write {}", path.display()))?;
                eprintln!("  gate {} → {} ({})", m.gate, want, m.gate_file);
            }
        }
        Ok(())
    }

    pub fn ensure_kolibri_img(&self) -> Result<PathBuf> {
        let configured = resolve(self.root, &self.cfg.image.tool_bin);
        let candidates = tool_bin_candidates(&configured);
        for c in &candidates {
            if c.is_file() {
                return Ok(c.clone());
            }
        }

        eprintln!("  building kolibri_img (release)...");
        let manifest = resolve(self.root, &self.cfg.image.tool_manifest);
        let cargo = which_on_path("cargo").context("ERROR: `cargo` not found on PATH")?;
        let args = [
            "build".to_string(),
            "--release".to_string(),
            "--manifest-path".to_string(),
            manifest.to_string_lossy().into_owned(),
        ];
        run_checked(
            "build kolibri_img",
            &cargo,
            &args,
            self.root,
            &[],
            self.dry_run,
        )?;

        for c in &candidates {
            if self.dry_run || c.is_file() {
                return Ok(c.clone());
            }
        }
        bail!(
            "ERROR: kolibri_img binary missing after build\nExpected one of: {candidates:?}"
        );
    }

    pub fn create_image(&mut self) -> Result<PathBuf> {
        eprintln!("== Image ==");

        if !self.kernel_built_ok {
            bail!(
                "ERROR: refusing to create image — kernel was not successfully built in this run\n\
                 (this prevents packaging a stale kernel.mnt)"
            );
        }
        let kernel = self
            .last_kernel
            .clone()
            .context("internal: kernel_built_ok but last_kernel unset")?;

        if !self.dry_run && !kernel.is_file() {
            bail!(
                "ERROR: kernel artifact missing; cannot package image\nExpected: {}",
                kernel.display()
            );
        }

        let base = resolve(self.root, &self.cfg.image.base_image);
        if !self.dry_run && !base.is_file() {
            bail!(
                "ERROR: base/reference image missing\nExpected: {}",
                base.display()
            );
        }

        let out_dir = resolve(self.root, &self.cfg.image.output_dir);
        if !self.dry_run {
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("mkdir {}", out_dir.display()))?;
        }

        let stamp = timestamp_utc();
        let name = self
            .cfg
            .image
            .filename_pattern
            .replace("{timestamp}", &stamp);
        let image_path = out_dir.join(name);

        eprintln!("Creating fresh test image:");
        eprintln!("  {}", image_path.display());
        eprintln!("Installing:");
        eprintln!("  {}", kernel.display());

        let img_tool = self.ensure_kolibri_img()?;

        run_checked(
            "image cow",
            &img_tool,
            &[
                "cow".to_string(),
                base.to_string_lossy().into_owned(),
                image_path.to_string_lossy().into_owned(),
            ],
            self.root,
            &[],
            self.dry_run,
        )?;

        for name in &self.cfg.image.delete_before_replace {
            run_checked(
                &format!("image delete {name}"),
                &img_tool,
                &[
                    "delete".to_string(),
                    image_path.to_string_lossy().into_owned(),
                    name.clone(),
                ],
                self.root,
                &[],
                self.dry_run,
            )?;
        }

        run_checked(
            "image replace KERNEL.MNT",
            &img_tool,
            &[
                "replace".to_string(),
                image_path.to_string_lossy().into_owned(),
                self.cfg.image.kernel_fat_name.clone(),
                kernel.to_string_lossy().into_owned(),
            ],
            self.root,
            &[],
            self.dry_run,
        )?;

        if !self.dry_run && !image_path.is_file() {
            bail!(
                "ERROR: test image missing after packaging\nExpected: {}",
                image_path.display()
            );
        }

        self.last_image = Some(image_path.clone());
        Ok(image_path)
    }

    pub fn resolve_qemu(&self) -> Result<PathBuf> {
        for cand in &self.cfg.qemu.executables {
            if let Some(p) = resolve_tool(cand, self.root) {
                return Ok(p);
            }
        }
        bail!(
            "ERROR: QEMU executable not found\nTried: {:?}\nInstall qemu-system-i386 or set qemu.executables in tools/build/config.toml",
            self.cfg.qemu.executables
        )
    }

    pub fn run_qemu(&mut self) -> Result<i32> {
        eprintln!("== QEMU ==");
        eprintln!("Launching...");

        let image = match &self.last_image {
            Some(p) => p.clone(),
            None => bail!(
                "ERROR: refusing to launch QEMU — no fresh test image from this run\n\
                 (this prevents booting a stale image)"
            ),
        };

        if !self.kernel_built_ok {
            bail!(
                "ERROR: refusing to launch QEMU — kernel was not successfully built in this run"
            );
        }

        let qemu = self.resolve_qemu()?;
        let mut args: Vec<String> = Vec::new();
        args.push("-fda".into());
        args.push(image.to_string_lossy().into_owned());
        args.extend(self.cfg.qemu.args.iter().cloned());
        if self.headless {
            args.extend(self.cfg.qemu.headless_extra_args.iter().cloned());
        }

        self.print_summary_paths();

        let code = run_inherit("QEMU", &qemu, &args, self.root, self.dry_run)?;
        eprintln!("QEMU exited with status {code}");

        if code == 0
            && self.cfg.cleanup.delete_image_on_success
            && !self.dry_run
            && image.is_file()
        {
            eprintln!("Cleanup: removing {}", image.display());
            fs::remove_file(&image)?;
        }

        Ok(code)
    }

    pub fn run_full(&mut self) -> Result<i32> {
        self.build_all()?;
        self.create_image()?;
        self.run_qemu()
    }

    /// Boot the immutable reference floppy (no rebuild / no image mutation).
    /// Uses `-snapshot` (via config) so writes never hit the base image file.
    pub fn run_reference_qemu(&self) -> Result<i32> {
        eprintln!("== QEMU (reference image) ==");
        eprintln!("Launching original/base image (no rebuild)...");

        let image = resolve(self.root, &self.cfg.image.base_image);
        if !self.dry_run && !image.is_file() {
            bail!(
                "ERROR: reference image missing\nExpected: {}",
                image.display()
            );
        }

        let qemu = self.resolve_qemu()?;
        let mut args: Vec<String> = Vec::new();
        args.push("-fda".into());
        args.push(image.to_string_lossy().into_owned());
        args.extend(self.cfg.qemu.args.iter().cloned());
        args.extend(self.cfg.qemu.reference_extra_args.iter().cloned());
        if self.headless {
            args.extend(self.cfg.qemu.headless_extra_args.iter().cloned());
        }

        eprintln!("Reference image:");
        eprintln!("  {}", image.display());
        eprintln!("QEMU:");
        eprintln!("  {}", qemu.display());
        if !self.cfg.qemu.reference_extra_args.is_empty() {
            eprintln!(
                "Note: reference_extra_args = {:?} (keeps base image read-only)",
                self.cfg.qemu.reference_extra_args
            );
        }

        let code = run_inherit("QEMU (reference)", &qemu, &args, self.root, self.dry_run)?;
        eprintln!("QEMU exited with status {code}");
        Ok(code)
    }
}

fn tool_bin_candidates(configured: &Path) -> Vec<PathBuf> {
    let mut out = vec![configured.to_path_buf()];
    if let Some(parent) = configured.parent() {
        out.push(parent.join("kolibri_img"));
        out.push(parent.join("kolibri_img.exe"));
    }
    out.sort();
    out.dedup();
    out
}

fn timestamp_utc() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let sub = dur.subsec_millis();
    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}-{hour:02}{min:02}{sec:02}-{sub:03}")
}

/// Days since Unix epoch → civil date (Howard Hinnant algorithm).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

pub fn doctor(cfg: &Config, root: &Path) -> Result<()> {
    eprintln!("== doctor ==");
    eprintln!("Repo root: {}", root.display());
    let mut errors = 0u32;

    check(
        "kernel/kernel.asm",
        root.join("kernel/kernel.asm").is_file(),
        &mut errors,
    );
    check(
        "base image",
        resolve(root, &cfg.image.base_image).is_file(),
        &mut errors,
    );
    check("FASM", resolve(root, &cfg.kernel.fasm).is_file(), &mut errors);
    check(
        "target JSON",
        resolve(root, &cfg.rust.target_json).is_file(),
        &mut errors,
    );
    check(
        "extract generic script",
        resolve(root, &cfg.rust.extract.generic_script).is_file(),
        &mut errors,
    );
    check(
        "extract probe script",
        resolve(root, &cfg.rust.extract.probe_script).is_file(),
        &mut errors,
    );
    check(
        "kolibri_img manifest",
        resolve(root, &cfg.image.tool_manifest).is_file(),
        &mut errors,
    );

    match which_on_path("cargo") {
        Some(p) => eprintln!("  OK  cargo ({})", p.display()),
        None => {
            eprintln!("  FAIL cargo not on PATH");
            errors += 1;
        }
    }
    match find_python(&cfg.rust.extract.python) {
        Some(p) => eprintln!("  OK  python ({})", p.display()),
        None => {
            eprintln!("  FAIL python not found");
            errors += 1;
        }
    }

    let mut qemu_ok = false;
    for cand in &cfg.qemu.executables {
        if let Some(p) = resolve_tool(cand, root) {
            eprintln!("  OK  qemu ({})", p.display());
            qemu_ok = true;
            break;
        }
    }
    if !qemu_ok {
        eprintln!("  FAIL qemu not found (tried {:?})", cfg.qemu.executables);
        errors += 1;
    }

    if let Some(cargo) = which_on_path("cargo") {
        let args = [format!("+{}", cfg.rust.toolchain), "--version".into()];
        match Command::new(&cargo).args(&args).output() {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout);
                eprintln!("  OK  rust toolchain {} ({})", cfg.rust.toolchain, v.trim());
            }
            _ => {
                eprintln!(
                    "  FAIL rust toolchain `{}` not available (rustup toolchain install {})",
                    cfg.rust.toolchain, cfg.rust.toolchain
                );
                errors += 1;
            }
        }
    }

    doctor_migrations(cfg, root, &mut errors)?;

    if errors > 0 {
        bail!("doctor found {errors} problem(s)");
    }
    eprintln!("doctor: all checks passed");
    Ok(())
}

fn doctor_migrations(cfg: &Config, root: &Path, errors: &mut u32) -> Result<()> {
    eprintln!(
        "== migrations ({} registered) ==",
        cfg.rust.migrations.len()
    );
    for m in &cfg.rust.migrations {
        let want = if m.enabled { 1 } else { 0 };
        let include = resolve(root, &m.include);
        let gate_file = resolve(root, &m.gate_file);
        let label = format!("Cut {} {} ({})", m.cut, m.gate, m.symbol);

        if !include.is_file() {
            eprintln!("  FAIL {label}: include missing ({})", include.display());
            *errors += 1;
            continue;
        }
        if !gate_file.is_file() {
            eprintln!(
                "  FAIL {label}: gate_file missing ({})",
                gate_file.display()
            );
            *errors += 1;
            continue;
        }

        let include_text = fs::read_to_string(&include)
            .with_context(|| format!("read {}", include.display()))?;
        if !include_text.contains(&m.symbol) {
            eprintln!(
                "  FAIL {label}: include does not reference symbol `{}`",
                m.symbol
            );
            *errors += 1;
            continue;
        }
        if !include_text.contains(&m.blob) {
            eprintln!(
                "  FAIL {label}: include does not embed blob `{}`",
                m.blob
            );
            *errors += 1;
            continue;
        }

        let gate_text = fs::read_to_string(&gate_file)
            .with_context(|| format!("read {}", gate_file.display()))?;
        let assign = format!("{} = {}", m.gate, want);
        let other = format!("{} = {}", m.gate, 1 - want);
        if gate_text.lines().any(|l| {
            let t = l.trim();
            t == assign || t.starts_with(&format!("{assign};"))
        }) {
            eprintln!("  OK  {label} = {want}");
        } else if gate_text.contains(&m.gate) {
            eprintln!(
                "  FAIL {label}: expected `{assign}` in {} (found gate, wrong value? e.g. `{other}`)",
                m.gate_file
            );
            *errors += 1;
        } else {
            eprintln!(
                "  FAIL {label}: gate not found in {}",
                m.gate_file
            );
            *errors += 1;
        }
    }

    let generic_n = cfg
        .rust
        .blobs
        .iter()
        .filter(|b| matches!(b.kind, BlobKind::Generic))
        .count();
    eprintln!(
        "  enumerated {} generic blobs + {} migrations (probe excluded from gates)",
        generic_n,
        cfg.rust.migrations.len()
    );
    Ok(())
}

fn check(label: &str, ok: bool, errors: &mut u32) {
    if ok {
        eprintln!("  OK  {label}");
    } else {
        eprintln!("  FAIL {label}");
        *errors += 1;
    }
}
