# CP866 encode migration: freestanding build + extract reloc-free blobs.
#
#   powershell -File rust_kernel/kolibri_utils/build-cp866.ps1
#
# Produces (kernel currently needs all of these):
#   rust_kernel/kolibri_utils/out/rust_unicode_cp866_encode.bin
#   rust_kernel/kolibri_utils/out/rust_unicode_utf16_encode.bin
#   rust_kernel/kolibri_utils/out/rust_crc_32.bin
#   rust_kernel/kolibri_utils/out/rust_phase_c_probe.bin
#
# Does NOT assemble kernel.mnt (see docs/migration/cp866-migration.md).

param(
    [switch]$SkipTest
)

$ErrorActionPreference = "Stop"
$UtilsDir = $PSScriptRoot
$Workspace = Resolve-Path (Join-Path $UtilsDir "..")
$RepoRoot = Resolve-Path (Join-Path $Workspace "..")
Set-Location $Workspace

$env:CARGO_TARGET_DIR = Join-Path $Workspace "target"
# Clear any prior freestanding RUSTFLAGS (must not lower opt-level globally —
# that reintroduces relocs into CRC/UTF-16).
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue

if (-not $SkipTest) {
    Write-Host "==> cargo test -p kolibri_utils"
    cargo test -p kolibri_utils
}

$targetJson = Join-Path $UtilsDir "i686-kolibri-none.json"
Write-Host "==> freestanding staticlib (force recompile)"
# Remove prior freestanding artifacts so CP866 codegen is not stale.
$fsOut = Join-Path $env:CARGO_TARGET_DIR "i686-kolibri-none\release"
if (Test-Path $fsOut) {
    Remove-Item -Recurse -Force (Join-Path $fsOut "libkolibri_utils.a") -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force (Join-Path $fsOut "deps\kolibri_utils-*") -ErrorAction SilentlyContinue
}
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

Write-Host "==> extract reloc-free rust_unicode_cp866_encode"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_unicode_cp866_encode" `
    --symbol "rust_unicode_cp866_encode" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_unicode_cp866_encode.bin")

Write-Host "==> extract reloc-free rust_unicode_utf16_encode"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_unicode_utf16_encode" `
    --symbol "rust_unicode_utf16_encode" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_unicode_utf16_encode.bin")

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
Write-Host "CP866 blob: $(Join-Path $outDir 'rust_unicode_cp866_encode.bin')"
Write-Host "UTF-16 blob: $(Join-Path $outDir 'rust_unicode_utf16_encode.bin')"
Write-Host "CRC blob:    $(Join-Path $outDir 'rust_crc_32.bin')"
Write-Host "Probe blob:  $(Join-Path $outDir 'rust_phase_c_probe.bin')"
Write-Host "Next: assemble kernel with USE_RUST_CP866 (see docs/migration/cp866-migration.md)"
