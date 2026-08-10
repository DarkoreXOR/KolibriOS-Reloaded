# KolibriOS kernel (hybrid FASM + Rust)

This repository is a KolibriOS kernel tree with a staged Rust migration.
**Cuts A–AB are complete** and production-enabled. Use this README to build the
current hybrid kernel, put it on a **fresh disposable floppy image**, and
smoke-test it under QEMU before further migration work.

Do **not** modify the reference image at the repository root. Always work on a
CoW / disposable copy (`dev_build/` via the orchestrator).

## Build and QEMU

Preferred one-command developer workflow (from the repository root):

```powershell
cargo run --manifest-path orch/Cargo.toml -- run
```

This orchestrator (`orch`, config in `orch/config.toml`) performs:

```text
Rust blobs (Cuts A–AH) → assemble kernel.mnt → fresh dev_build/test/*.img → QEMU
```

It always rebuilds current Rust components before packaging, refuses to ship a
stale `kernel.mnt` if the kernel stage fails, and never mutates the reference
`kolibrios-*.img`.

| Command | Action |
|---------|--------|
| `run` | Build → fresh image → QEMU (also attaches regression disks via `--disk:TYPE`) |
| `qemu` | Same as `run` |
| `mkfs` | Create/reuse persistent `./images/{fs}-image.img` (`mkfs exfat 4M`, `mkfs ntfs 8M`) |
| `clean` | Remove `./build/` and `./dev_build/` (preserves `./images/`) |
| `help` | Discover commands, scripts, and tools |
| `script` | Run a Rhai workflow from `orch/scripts/` |
| `tool` | Invoke a utility under `./tools/` |
| `image` | Build → fresh temporary `.img` only |
| `build` | Rust blobs + `kernel/bin/kernel.mnt` only |
| `ref` | Boot the **original reference** floppy in QEMU (no rebuild; `-snapshot`) |
| `testdisk` | Legacy exFAT ensure (prefer `mkfs exfat`) |
| `doctor` | Check tools, paths, and migration-gate registry |

Examples:

```powershell
cargo run --manifest-path orch/Cargo.toml -- doctor
cargo run --manifest-path orch/Cargo.toml -- build
cargo run --manifest-path orch/Cargo.toml -- image
cargo run --manifest-path orch/Cargo.toml -- run
cargo run --manifest-path orch/Cargo.toml -- ref
cargo run --manifest-path orch/Cargo.toml -- testdisk --force
```

`run` / `qemu` / `ref` automatically attach
`tools/testdata/kolibrios-exfat-test.img` as a secondary IDE disk (exFAT
filesystem regression fixture). See [`tools/testdata/README.md`](tools/testdata/README.md).

`ref` (alias: `original`) launches QEMU on `kolibrios-*-en_US.img` with `-snapshot`
so the reference file is never written. Use it to compare a known-good stock boot
against a hybrid test image.

Useful flags:

| Flag | Meaning |
|------|---------|
| `--dry-run` | Print commands without executing |
| `--skip-tests` | Skip `cargo test -p kolibri_utils` |
| `--headless` | Add headless/QMP QEMU args from config |

Settings (QEMU path, image dir, blob list, migration gates, etc.) live in
[`orch/config.toml`](orch/config.toml).

## Prerequisites

Tools required by the current Windows workflow (this tree):

| Tool | Role |
|------|------|
| **Rust stable** | Orchestrator, `cargo test`, `tools/kolibri_img` |
| **Rust nightly** + **`rust-src`** | Freestanding `build-std` for target `i686-kolibri-none` |
| **Python 3** | Extracts reloc-free blobs from `libkolibri_utils.a` (invoked by the orchestrator); also builds the exFAT testdisk via `FATtools` |
| **Vendored FASM** (`fasm/FASM.EXE`) | Assembles `kernel.mnt` (do not assume system `fasm` on `PATH`) |
| **QEMU** `qemu-system-i386` | Boot smoke (often `C:\Program Files\qemu\qemu-system-i386.exe` on Windows; may not be on `PATH`) |

Also required in the tree:

* Reference floppy: `kolibrios-0.7.7.0-9160-g944d74f01-en_US.img` (read-only)
* Host image tool sources: `tools/kolibri_img/`
* Orchestrator: `orch/`
* exFAT testdisk generator: `tools/testdata/` (`python -m pip install -r tools/testdata/requirements.txt`)

Install `rust-src` for nightly if needed:

```powershell
rustup component add rust-src --toolchain nightly
```

## Manual workflow (optional)

The steps below are the low-level commands the orchestrator coordinates.
Prefer `cargo run --manifest-path orch/Cargo.toml -- run` unless you are
debugging a single stage.

All commands start from the **repository root** unless noted.

### 1. Build Rust blobs (Cuts A–AB)

Freestanding blobs under `rust_kernel/kolibri_utils/out/` are **generated**
(not committed). The current kernel embeds them via `kernel/rust/*.inc`.

The Cut AB helper rebuilds **all** blobs the hybrid kernel currently needs
(A through AB), runs host tests, and extracts into `out/`:

```powershell
powershell -File rust_kernel/kolibri_utils/build-utf8to16.ps1
```

Expected outputs include (among others):

```text
rust_kernel/kolibri_utils/out/rust_utf8to16.bin
rust_kernel/kolibri_utils/out/rust_pid_to_slot.bin
… (earlier Cut A–AA blobs)
rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
```

This step does **not** assemble `kernel.mnt`. The orchestrator’s `build` /
`run` commands perform the same extract list from `orch/config.toml`.

### 2. Assemble `kernel.mnt`

Canonical development assemble (language `en_US`, Rust cut switches default on):

```powershell
New-Item -ItemType Directory -Force -Path kernel\bin | Out-Null
Set-Content kernel\lang.inc "lang fix en_US`n"
.\fasm\FASM.EXE -m 262144 kernel\kernel.asm kernel\bin\kernel.mnt
Remove-Item kernel\lang.inc
```

Or let the orchestrator sync every `USE_RUST_*` gate from `config.toml` and
assemble:

```powershell
cargo run --manifest-path orch/Cargo.toml -- build --skip-tests
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
.\target\release\kolibri_img.exe cow ..\..\kolibrios-0.7.7.0-9160-g944d74f01-en_US.img ..\..\dev_build\dev-test.img
.\target\release\kolibri_img.exe delete ..\..\dev_build\dev-test.img DOCPACK
.\target\release\kolibri_img.exe delete ..\..\dev_build\dev-test.img DEVELOP/FASM
.\target\release\kolibri_img.exe delete ..\..\dev_build\dev-test.img 3D/VIEW3DS
.\target\release\kolibri_img.exe delete ..\..\dev_build\dev-test.img GAMES/DINO
.\target\release\kolibri_img.exe replace ..\..\dev_build\dev-test.img KERNEL.MNT ..\..\kernel\bin\kernel.mnt
```

| Step | Why |
|------|-----|
| `cow` | Creates disposable `dev_build/dev-test.img` (refuses same-path overwrite) |
| `delete …` | Authorized free-space paths (see `.cursor/rules/image-handling.mdc`); `DOCPACK` alone is no longer enough for current hybrid kernels |
| `replace … KERNEL.MNT` | Puts **your** `kernel/bin/kernel.mnt` onto that image |

**Resulting image:**

```text
dev_build/dev-test.img
```

Choose any other name under `dev_build/` if you prefer; keep the reference
`kolibrios-*.img` untouched. Production cut checkpoints (e.g.
`dev_build/cut-ab-final.img`) are also disposable CoW descendants.

Optional sanity check that the image sees your kernel:

```powershell
.\target\release\kolibri_img.exe inspect ..\..\dev_build\dev-test.img
```

## Run QEMU

Return to the repository root (or adjust `-fda` accordingly).

### Interactive desktop (recommended for manual verification)

```powershell
& "C:\Program Files\qemu\qemu-system-i386.exe" `
  -fda dev_build\dev-test.img `
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
  -fda dev_build\dev-test.img `
  -boot a `
  -m 256 `
  -vga std `
  -display none `
  -no-reboot `
  -no-shutdown `
  -netdev user,id=n0 `
  -device e1000,netdev=n0 `
  -qmp tcp:127.0.0.1:4550,server,nowait
```

Prefer the interactive form above when you need to click the desktop yourself.

## Manual verification

After boot, check at least:

1. Kernel reaches the KolibriOS **desktop** (taskbar / icons / wallpaper).
2. Mouse and basic UI interaction work.
3. Several normal **`/sys` applications** launch and run without an obvious crash.
4. No hang during ordinary desktop use for a short soak.

Launching several `/sys` apps exercises live paths covered by completed cuts
(e.g. MENUET header validation, path UTF-8→UTF-16 decode, process TID lookup).
This is a **smoke** check, not proof that every `/sys` application is compatible.

When finished, quit QEMU. Disposable images under `dev_build/` may be deleted.

## Troubleshooting

| Symptom | Likely cause / fix |
|---------|--------------------|
| FASM error opening `rust_kernel/kolibri_utils/out/*.bin` | Blobs missing — run `build-utf8to16.ps1` or `orch build` first (`out/` is generated, not committed) |
| `cargo +nightly build` / `build-std` fails | Install nightly and `rust-src` (`rustup component add rust-src --toolchain nightly`) |
| `python: command not found` during blob extract | Install Python 3 and ensure `python` is on `PATH` |
| `lang.inc` / assemble errors | Run the assemble sequence from the **repo root**; create ephemeral `kernel/lang.inc` as shown, then remove it — or use `orch build` |
| `kolibri_img` missing | `cd tools\kolibri_img` then `cargo build --release` |
| `replace` fails (not enough space / write refused) | Always `cow` to `dev_build/`/`build/test/`, then delete authorized free-space paths (`DOCPACK`, `DEVELOP/FASM`, `3D/VIEW3DS`, `GAMES/DINO` — see `.cursor/rules/image-handling.mdc`), then `replace`. Never mutate `kolibrios-*.img`. The orchestrator does this via `orch/config.toml` `delete_before_replace`. |
| QEMU not found | Use the full path to `qemu-system-i386.exe`, or add QEMU to `PATH` |
| Boots but looks like an old build | Image was not recreated or still has old `KERNEL.MNT` — rebuild `kernel.mnt`, then new `cow` + `delete` + `replace` |
| Wrong directory | FASM and `powershell -File …` expect repo-root paths; `kolibri_img` commands in the docs use paths relative to `tools/kolibri_img` |
| Stale artifacts | Re-run the blob build script, reassemble `kernel/bin/kernel.mnt`, and create a **new** `dev_build/*.img` rather than reusing an old CoW |
| `doctor` gate mismatch | `config.toml` `[[rust.migrations]].enabled` must match live `USE_RUST_*=0\|1` in each `gate_file` |

## Development verification workflow

```text
1. cargo run --manifest-path orch/Cargo.toml -- run
2. Manually verify the desktop
3. Launch several /sys applications
4. Stop QEMU
5. Report any regression
```

Or, stage by stage: `build` → `image` → open the printed `build/test/*.img` in QEMU
via `run` / `qemu`.

After this manual QEMU verification succeeds, development continues with the
**next migration step** (do not start Cut AC from this README alone — follow
[`docs/migration/migration-plan.md`](docs/migration/migration-plan.md)).

## Further reading

* Layout and image rules: [`docs/_meta/project-structure.md`](docs/_meta/project-structure.md)
* Build system notes: [`docs/architecture/build-system.md`](docs/architecture/build-system.md)
* Migration status: [`docs/migration/migration-plan.md`](docs/migration/migration-plan.md)
* Latest cut: [`docs/migration/cut-ab-implementation.md`](docs/migration/cut-ab-implementation.md)
* Orchestrator: [`orch/README.md`](orch/README.md)
* Image tool: [`tools/kolibri_img/README.md`](tools/kolibri_img/README.md)
