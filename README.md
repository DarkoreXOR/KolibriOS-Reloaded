# KolibriOS kernel (hybrid FASM + Rust)

This repository is a KolibriOS kernel tree with a staged Rust migration.
**Cuts A–O are complete.** Use this README to build the current hybrid kernel,
put it on a **fresh disposable floppy image**, and smoke-test it under QEMU
before further migration work.

Do **not** modify the reference image at the repository root. Always work on a
CoW / disposable copy (`build/test/` via the orchestrator, or `tmp_images/`).

## Build and QEMU

Preferred one-command developer workflow (from the repository root):

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- run
```

This orchestrator (`tools/build`, config in `tools/build/config.toml`) performs:

```text
Rust blobs (Cuts A–O) → assemble kernel.mnt → fresh build/test/*.img → QEMU
```

It always rebuilds current Rust components before packaging, refuses to ship a
stale `kernel.mnt` if the kernel stage fails, and never mutates the reference
`kolibrios-*.img`.

| Command | Action |
|---------|--------|
| `run` | Build → fresh image → QEMU (recommended) |
| `qemu` | Same as `run` |
| `image` | Build → fresh temporary `.img` only |
| `build` | Rust blobs + `kernel/bin/kernel.mnt` only |
| `ref` | Boot the **original reference** floppy in QEMU (no rebuild; `-snapshot`) |
| `doctor` | Check tools and configured paths |

Examples:

```powershell
cargo run --manifest-path tools/build/Cargo.toml -- doctor
cargo run --manifest-path tools/build/Cargo.toml -- build
cargo run --manifest-path tools/build/Cargo.toml -- image
cargo run --manifest-path tools/build/Cargo.toml -- run
cargo run --manifest-path tools/build/Cargo.toml -- ref
```

`ref` (alias: `original`) launches QEMU on `kolibrios-*-en_US.img` with `-snapshot`
so the reference file is never written. Use it to compare a known-good stock boot
against a hybrid test image.

Useful flags:

| Flag | Meaning |
|------|---------|
| `--dry-run` | Print commands without executing |
| `--skip-tests` | Skip `cargo test -p kolibri_utils` |
| `--headless` | Add headless/QMP QEMU args from config |

Settings (QEMU path, image dir, blob list, etc.) live in
[`tools/build/config.toml`](tools/build/config.toml).

## Prerequisites

Tools required by the current Windows workflow (this tree):

| Tool | Role |
|------|------|
| **Rust stable** | Orchestrator, `cargo test`, `tools/kolibri_img` |
| **Rust nightly** + **`rust-src`** | Freestanding `build-std` for target `i686-kolibri-none` |
| **Python 3** | Extracts reloc-free blobs from `libkolibri_utils.a` (invoked by the orchestrator) |
| **Vendored FASM** (`fasm/FASM.EXE`) | Assembles `kernel.mnt` (do not assume system `fasm` on `PATH`) |
| **QEMU** `qemu-system-i386` | Boot smoke (often `C:\Program Files\qemu\qemu-system-i386.exe` on Windows; may not be on `PATH`) |

Also required in the tree:

* Reference floppy: `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` (read-only)
* Host image tool sources: `tools/kolibri_img/`
* Orchestrator: `tools/build/`

Install `rust-src` for nightly if needed:

```powershell
rustup component add rust-src --toolchain nightly
```

## Manual workflow (optional)

The steps below are the low-level commands the orchestrator coordinates.
Prefer `cargo run --manifest-path tools/build/Cargo.toml -- run` unless you are
debugging a single stage.

All commands start from the **repository root** unless noted.

### 1. Build Rust blobs (Cuts A–O)

Freestanding blobs under `rust_kernel/kolibri_utils/out/` are **generated**
(not committed). The current kernel embeds them via `kernel/rust/*.inc`.

The Cut O helper rebuilds **all** blobs the hybrid kernel currently needs
(A through O), runs host tests, and extracts into `out/`:

```powershell
powershell -File rust_kernel/kolibri_utils/build-test-app-header.ps1
```

Expected outputs include (among others):

```text
rust_kernel/kolibri_utils/out/rust_test_app_header.bin
rust_kernel/kolibri_utils/out/rust_anti_aliasing.bin
… (earlier Cut A–N blobs)
rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
```

This step does **not** assemble `kernel.mnt`.

### 2. Assemble `kernel.mnt`

Canonical development assemble (language `en_US`, Rust cut switches default on):

```powershell
New-Item -ItemType Directory -Force -Path kernel\bin | Out-Null
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc
```

**Artifact:**

```text
kernel/bin/kernel.mnt
```

This is the uncompressed hybrid kernel used for QEMU testing. Always rebuild it
after changing Rust blobs or FASM sources; do not trust a previously checked-in
or leftover `kernel.mnt` as “current.”

## Create a separate test image

Never write into `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img`.
Build (or reuse) the host helper, then CoW → free space → install your kernel.

### 1. Build `kolibri_img` (once per machine / after tool changes)

```powershell
cd tools\kolibri_img
cargo build --release
```

Binary: `tools/kolibri_img/target/release/kolibri_img.exe`

Avoid leaving `CARGO_TARGET_DIR` pointed at `rust_kernel/target` when building
this crate (it has its own `target/`).

### 2. Fresh CoW image + install newly built kernel

Still in `tools/kolibri_img` (or use the same relative paths from that directory):

```powershell
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\tmp_images\dev-test.img
.\target\release\kolibri_img.exe delete ..\..\tmp_images\dev-test.img DOCPACK
.\target\release\kolibri_img.exe delete ..\..\tmp_images\dev-test.img DEVELOP/FASM
.\target\release\kolibri_img.exe delete ..\..\tmp_images\dev-test.img 3D/VIEW3DS
.\target\release\kolibri_img.exe delete ..\..\tmp_images\dev-test.img GAMES/DINO
.\target\release\kolibri_img.exe replace ..\..\tmp_images\dev-test.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

| Step | Why |
|------|-----|
| `cow` | Creates disposable `tmp_images/dev-test.img` (refuses same-path overwrite) |
| `delete …` | Authorized free-space paths (see `.cursor/rules/image-handling.mdc`); `DOCPACK` alone is no longer enough for Cut S–sized kernels |
| `replace … KERNEL.MNT` | Puts **your** `kernel/bin/kernel.mnt` onto that image |

**Resulting image:**

```text
tmp_images/dev-test.img
```

Choose any other name under `tmp_images/` if you prefer; keep the reference
`kolibrios-*.img` untouched.

Optional sanity check that the image sees your kernel:

```powershell
.\target\release\kolibri_img.exe inspect ..\..\tmp_images\dev-test.img
```

## Run QEMU

Return to the repository root (or adjust `-fda` accordingly).

### Interactive desktop (recommended for manual verification)

```powershell
& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda tmp_images\dev-test.img `
  -boot a `
  -m 256 `
  -vga std
```

| Option | Meaning in this workflow |
|--------|--------------------------|
| `-fda …` | Boot the disposable floppy image |
| `-boot a` | Boot from floppy A |
| `-m 256` | 256 MiB guest RAM |
| `-vga std` | Standard VGA (QEMU window) |

Adjust the QEMU executable path if yours differs. QEMU is often **not** on `PATH`.

### Headless / automation variant (optional)

Cut audits sometimes use headless mode plus QMP, for example:

```powershell
& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda tmp_images\dev-test.img `
  -boot a `
  -m 256 `
  -vga std `
  -display none `
  -no-reboot `
  -no-shutdown `
  -qmp tcp:127.0.0.1:4550,server,nowait
```

Prefer the interactive form above when you need to click the desktop yourself.

## Manual verification

After boot, check at least:

1. Kernel reaches the KolibriOS **desktop** (taskbar / icons / wallpaper).
2. Mouse and basic UI interaction work.
3. Several normal **`/sys` applications** launch and run without an obvious crash.
4. No hang during ordinary desktop use for a short soak.

Launching several `/sys` apps is especially useful right now: Cuts through **O**
exercise the live `fs_execute → test_app_header` path used when starting
MENUET-format binaries. This is a **smoke** check, not proof that every `/sys`
application is compatible.

When finished, quit QEMU. Disposable images under `tmp_images/` may be deleted.

## Troubleshooting

| Symptom | Likely cause / fix |
|---------|--------------------|
| FASM error opening `rust_kernel/kolibri_utils/out/*.bin` | Blobs missing — run `build-test-app-header.ps1` first (`out/` is generated, not committed) |
| `cargo +nightly build` / `build-std` fails | Install nightly and `rust-src` (`rustup component add rust-src --toolchain nightly`) |
| `python: command not found` during blob extract | Install Python 3 and ensure `python` is on `PATH` |
| `lang.inc` / assemble errors | Run the assemble sequence from the **repo root**; create ephemeral `kernel/lang.inc` as shown, then remove it |
| `kolibri_img` missing | `cd tools\kolibri_img` then `cargo build --release` |
| `replace` fails (not enough space / write refused) | Always `cow` to `tmp_images/`/`build/test/`, then delete authorized free-space paths (`DOCPACK`, `DEVELOP/FASM`, `3D/VIEW3DS`, `GAMES/DINO` — see `.cursor/rules/image-handling.mdc`), then `replace`. Never mutate `kolibrios-*.img`. The orchestrator does this via `tools/build/config.toml` `delete_before_replace`. |
| QEMU not found | Use the full path to `qemu-system-i386.exe`, or add QEMU to `PATH` |
| Boots but looks like an old build | Image was not recreated or still has old `KERNEL.MNT` — rebuild `kernel.mnt`, then new `cow` + `delete` + `replace` |
| Wrong directory | FASM and `powershell -File …` expect repo-root paths; `kolibri_img` commands in the docs use paths relative to `tools/kolibri_img` |
| Stale artifacts | Re-run the blob build script, reassemble `kernel/bin/kernel.mnt`, and create a **new** `tmp_images/*.img` rather than reusing an old CoW |

## Development verification workflow

```text
1. cargo run --manifest-path tools/build/Cargo.toml -- run
2. Manually verify the desktop
3. Launch several /sys applications
4. Stop QEMU
5. Report any regression
```

Or, stage by stage: `build` → `image` → open the printed `build/test/*.img` in QEMU
via `run` / `qemu`.

After this manual QEMU verification succeeds, development continues with the
**next migration step** (do not start Cut P from this README alone — follow
`docs/migration/`).

## Further reading

* Layout and image rules: [`docs/_meta/project-structure.md`](docs/_meta/project-structure.md)
* Build system notes: [`docs/architecture/build-system.md`](docs/architecture/build-system.md)
* Migration status: [`docs/migration/migration-plan.md`](docs/migration/migration-plan.md)
* Image tool: [`tools/kolibri_img/README.md`](tools/kolibri_img/README.md)
