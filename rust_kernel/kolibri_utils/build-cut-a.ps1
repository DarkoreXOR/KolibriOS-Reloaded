# Cut A build helpers (Windows PowerShell)
#
# Host tests (logic + algorithm differential oracle):
#   powershell -File rust_kernel/kolibri_utils/build-cut-a.ps1 -Test
#
# Freestanding 32-bit staticlib (custom none target, nightly + build-std):
#   powershell -File rust_kernel/kolibri_utils/build-cut-a.ps1 -Freestanding
#
# Optional 32-bit MSVC staticlib (stdcall may be decorated; not for FASM link):
#   powershell -File rust_kernel/kolibri_utils/build-cut-a.ps1 -Msvc32
#
# Does NOT assemble kernel.mnt (requires fasm + uncorrupted kernel/init.inc).
# Workspace root for cargo is rust_kernel/ (sibling of FASM kernel/).

param(
    [switch]$Test,
    [switch]$Freestanding,
    [switch]$Msvc32,
    [switch]$All
)

$ErrorActionPreference = "Stop"
$Workspace = Resolve-Path (Join-Path $PSScriptRoot "..")
$RepoRoot = Resolve-Path (Join-Path $Workspace "..")
Set-Location $Workspace

if ($All) {
    $Test = $true
    $Freestanding = $true
}

if (-not $Test -and -not $Freestanding -and -not $Msvc32) {
    $Test = $true
    $Freestanding = $true
}

if ($Test) {
    Write-Host "==> cargo test -p kolibri_utils (cwd=$Workspace)"
    cargo test -p kolibri_utils
}

if ($Msvc32) {
    Write-Host "==> i686-pc-windows-msvc staticlib"
    cargo build -p kolibri_utils --release --target i686-pc-windows-msvc
}

if ($Freestanding) {
    $targetJson = Join-Path $PSScriptRoot "i686-kolibri-none.json"
    Write-Host "==> freestanding staticlib ($targetJson)"
    cargo +nightly build `
        -Z build-std=core,compiler_builtins `
        -Z json-target-spec `
        -p kolibri_utils `
        --release `
        --target $targetJson
    $outDir = Join-Path $env:CARGO_TARGET_DIR "i686-kolibri-none\release"
    if (-not $env:CARGO_TARGET_DIR) {
        $outDir = Join-Path $Workspace "target\i686-kolibri-none\release"
    }
    Write-Host "Artifacts under: $outDir"
    if (Test-Path $outDir) {
        Get-ChildItem $outDir -Filter "libkolibri_utils*" | Format-Table Name, Length
    }
}

Write-Host "Repo root (FASM kernel/, reference image): $RepoRoot"
