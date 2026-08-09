# Cut G — fsCalculateTime: freestanding build + extract reloc-free blobs.
#
#   powershell -File rust_kernel/kolibri_utils/build-fs-calculate-time.ps1
#
# Produces (kernel currently needs all of these):
#   rust_kernel/kolibri_utils/out/rust_fs_calculate_time.bin
#   (+ all prior Cut A–F blobs)
#
# Does NOT assemble kernel.mnt (see docs/migration/cut-g-implementation.md).

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
# that reintroduces relocs into Cut A–G blobs).
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue

if (-not $SkipTest) {
    Write-Host "==> cargo test -p kolibri_utils"
    cargo test -p kolibri_utils
}

$targetJson = Join-Path $UtilsDir "i686-kolibri-none.json"
Write-Host "==> freestanding staticlib (force recompile)"
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

Write-Host "==> extract reloc-free rust_fs_calculate_time"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_fs_calculate_time" `
    --symbol "rust_fs_calculate_time" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_fs_calculate_time.bin")

Write-Host "==> extract reloc-free rust_checksum_2"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_checksum_2" `
    --symbol "rust_checksum_2" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_checksum_2.bin")

Write-Host "==> extract reloc-free rust_checksum_1"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_checksum_1" `
    --symbol "rust_checksum_1" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_checksum_1.bin")

Write-Host "==> extract reloc-free rust_strncmp"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_strncmp" `
    --symbol "rust_strncmp" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_strncmp.bin")

Write-Host "==> extract reloc-free rust_utf16_to_upper"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_utf16_to_upper" `
    --symbol "rust_utf16_to_upper" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_utf16_to_upper.bin")

Write-Host "==> extract reloc-free rust_cp866_to_upper"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_cp866_to_upper" `
    --symbol "rust_cp866_to_upper" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_cp866_to_upper.bin")

Write-Host "==> extract reloc-free rust_unicode_utf8_decode"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_unicode_utf8_decode" `
    --symbol "rust_unicode_utf8_decode" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_unicode_utf8_decode.bin")

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
Write-Host "fs_calculate_time blob: $(Join-Path $outDir 'rust_fs_calculate_time.bin')"
Write-Host "checksum_2 blob:        $(Join-Path $outDir 'rust_checksum_2.bin')"
Write-Host "checksum_1 blob:        $(Join-Path $outDir 'rust_checksum_1.bin')"
Write-Host "strncmp blob:           $(Join-Path $outDir 'rust_strncmp.bin')"
Write-Host "UTF-16 upper blob:      $(Join-Path $outDir 'rust_utf16_to_upper.bin')"
Write-Host "CP866 upper blob:       $(Join-Path $outDir 'rust_cp866_to_upper.bin')"
Write-Host "UTF-8 blob:             $(Join-Path $outDir 'rust_unicode_utf8_decode.bin')"
Write-Host "CP866 blob:             $(Join-Path $outDir 'rust_unicode_cp866_encode.bin')"
Write-Host "UTF-16 blob:            $(Join-Path $outDir 'rust_unicode_utf16_encode.bin')"
Write-Host "CRC blob:               $(Join-Path $outDir 'rust_crc_32.bin')"
Write-Host "Probe blob:             $(Join-Path $outDir 'rust_phase_c_probe.bin')"
Write-Host "Next: assemble kernel with USE_RUST_FS_CALCULATE_TIME (see docs/migration/cut-g-implementation.md)"
