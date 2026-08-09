# Phase C: build freestanding Rust probe blob for FASM `file` inclusion.
#
#   powershell -File rust_kernel/kolibri_utils/build-phase-c.ps1
#
# Produces: rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
# Does NOT assemble kernel.mnt (see docs/migration/phase-c-integration.md).

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
$outBin = Join-Path $outDir "rust_phase_c_probe.bin"
$extract = Join-Path $UtilsDir "scripts\extract_phase_c_probe.py"

Write-Host "==> extract reloc-free probe"
python $extract --archive $archive --out $outBin

Write-Host "Repo root: $RepoRoot"
Write-Host "Probe blob: $outBin"
Write-Host "Next: assemble kernel with Phase C include (see docs/migration/phase-c-integration.md)"
