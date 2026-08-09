# Cut V — tcp_set_persist: freestanding build + extract reloc-free blobs.
#
#   powershell -File rust_kernel/kolibri_utils/build-tcp-set-persist.ps1
#
# Produces (kernel currently needs all of these):
#   rust_kernel/kolibri_utils/out/rust_tcp_set_persist.bin
#   (+ all prior Cut A–U blobs)
#
# Does NOT assemble kernel.mnt / trampoline / USE_RUST_* by itself.

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
# that reintroduces relocs into Cut A–Q blobs).
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue

if (-not $SkipTest) {
    Write-Host "==> cargo test -p kolibri_utils"
    cargo test -p kolibri_utils
    if ($LASTEXITCODE -ne 0) {
        throw "cargo test failed with exit code $LASTEXITCODE"
    }
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

Write-Host "==> extract reloc-free rust_tcp_set_persist"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_tcp_set_persist" `
    --symbol "rust_tcp_set_persist" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_tcp_set_persist.bin")

Write-Host "==> extract reloc-free rust_fat_gen_short_name"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_fat_gen_short_name" `
    --symbol "rust_fat_gen_short_name" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_fat_gen_short_name.bin")

Write-Host "==> extract reloc-free rust_fs_time2bdfe"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_fs_time2bdfe" `
    --symbol "rust_fs_time2bdfe" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_fs_time2bdfe.bin")

Write-Host "==> extract reloc-free rust_check_window_position"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_check_window_position" `
    --symbol "rust_check_window_position" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_check_window_position.bin")

Write-Host "==> extract reloc-free rust_xfs_extent_unpack"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_xfs_extent_unpack" `
    --symbol "rust_xfs_extent_unpack" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_xfs_extent_unpack.bin")

Write-Host "==> extract reloc-free rust_utf16_to_8"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_utf16_to_8" `
    --symbol "rust_utf16_to_8" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_utf16_to_8.bin")

Write-Host "==> extract reloc-free rust_is_region_userspace"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_is_region_userspace" `
    --symbol "rust_is_region_userspace" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_is_region_userspace.bin")

Write-Host "==> extract reloc-free rust_test_app_header"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_test_app_header" `
    --symbol "rust_test_app_header" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_test_app_header.bin")

Write-Host "==> extract reloc-free rust_anti_aliasing"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_anti_aliasing" `
    --symbol "rust_anti_aliasing" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_anti_aliasing.bin")

Write-Host "==> extract reloc-free rust_tcp_xmit_timer"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_tcp_xmit_timer" `
    --symbol "rust_tcp_xmit_timer" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_tcp_xmit_timer.bin")

Write-Host "==> extract reloc-free rust_mouse_acceleration"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_mouse_acceleration" `
    --symbol "rust_mouse_acceleration" `
    --expect-ret-imm 12 `
    --out (Join-Path $outDir "rust_mouse_acceleration.bin")

Write-Host "==> extract reloc-free rust_fat_next_short_name"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_fat_next_short_name" `
    --symbol "rust_fat_next_short_name" `
    --expect-ret-imm 4 `
    --out (Join-Path $outDir "rust_fat_next_short_name.bin")

Write-Host "==> extract reloc-free rust_ntfs_restore_usa"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_ntfs_restore_usa" `
    --symbol "rust_ntfs_restore_usa" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_ntfs_restore_usa.bin")

Write-Host "==> extract reloc-free rust_ntfs_decode_mcb_entry"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_ntfs_decode_mcb_entry" `
    --symbol "rust_ntfs_decode_mcb_entry" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_ntfs_decode_mcb_entry.bin")

Write-Host "==> extract reloc-free rust_block_clip"
python $extractGeneric `
    --archive $archive `
    --section ".text.rust_block_clip" `
    --symbol "rust_block_clip" `
    --expect-ret-imm 8 `
    --out (Join-Path $outDir "rust_block_clip.bin")

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

function Show-Blob($name) {
    $p = Join-Path $outDir $name
    $hash = (Get-FileHash -Algorithm SHA256 $p).Hash
    $len = (Get-Item $p).Length
    Write-Host ("{0}: {1} bytes SHA-256={2}" -f $name, $len, $hash)
}

Write-Host "Repo root: $RepoRoot"
Show-Blob "rust_tcp_set_persist.bin"
Show-Blob "rust_fat_gen_short_name.bin"
Show-Blob "rust_fs_time2bdfe.bin"
Show-Blob "rust_check_window_position.bin"
Write-Host "Next: wire USE_RUST_TCP_SET_PERSIST trampoline + smoke; default OFF until gates pass"
