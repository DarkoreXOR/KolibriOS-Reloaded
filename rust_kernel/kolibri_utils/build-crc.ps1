# Phase D / CRC: freestanding build + extract reloc-free Rust CRC (+ keep Phase C probe).
#
#   powershell -File rust_kernel/kolibri_utils/build-crc.ps1
#
# Produces:
#   rust_kernel/kolibri_utils/out/rust_crc_32.bin
#   rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
#
# Does NOT assemble kernel.mnt (see docs/migration/crc32-migration.md).

param(
    [switch]$SkipTest
)

$ErrorActionPreference = "Stop"
$UtilsDir = $PSScriptRoot
$Workspace = Resolve-Path (Join-Path $UtilsDir "..")
$RepoRoot = Resolve-Path (Join-Path $Workspace "..")
Set-Location $Workspace

$env:CARGO_TARGET_DIR = Join-Path $Workspace "target"

if (-not $SkipTest) {
    Write-Host "==> cargo test -p kolibri_utils"
    cargo test -p kolibri_utils
}

$targetJson = Join-Path $UtilsDir "i686-kolibri-none.json"
Write-Host "==> freestanding staticlib"
cargo +nightly build `
    -Z build-std=core,compiler_builtins `
    -Z json-target-spec `
    -p kolibri_utils `
    --release `
    --target $targetJson

$archive = Join-Path $env:CARGO_TARGET_DIR "i686-kolibri-none\release\libkolibri_utils.a"
if (-not (Test-Path $archive)) {
    throw "missing archive: $archive"
}

$outDir = Join-Path $UtilsDir "out"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$extractGeneric = Join-Path $UtilsDir "scripts\extract_reloc_free_text.py"
$extractProbe = Join-Path $UtilsDir "scripts\extract_phase_c_probe.py"

Write-Host "==> extract reloc-free rust_crc_32"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_crc_32" `
    --symbol "rust_crc_32" `
    --expect-ret-imm 16 `
    --out (Join-Path $outDir "rust_crc_32.bin")

Write-Host "==> extract reloc-free Phase C probe"
python $extractProbe --archive $archive --out (Join-Path $outDir "rust_phase_c_probe.bin")

Write-Host "Repo root: $RepoRoot"
Write-Host "CRC blob:  $(Join-Path $outDir 'rust_crc_32.bin')"
Write-Host "Probe blob: $(Join-Path $outDir 'rust_phase_c_probe.bin')"
Write-Host "Next: assemble kernel with USE_RUST_CRC (see docs/migration/crc32-migration.md)"
