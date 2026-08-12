#!/bin/sh
# Populate / recreate an EXT2 regression image inside a privileged Linux container.
# Invoked by tools/mkfs_utils/create_ext_image.py (Docker backend).
set -eu

IMG="${IMG:-/img/ext-image.img}"
MNT="${MNT:-/mnt/ext}"
FORCE="${FORCE:-0}"
SIZE_BYTES="${SIZE_BYTES:-}"

apk add --no-cache e2fsprogs e2fsprogs-extra python3
export PATH="/usr/sbin:/sbin:/usr/bin:/bin:${PATH}"

need_format=0
if [ ! -f "$IMG" ]; then
  if [ -z "$SIZE_BYTES" ]; then
    echo "ERROR: image missing and SIZE_BYTES not set: $IMG" >&2
    exit 1
  fi
  echo "Creating sparse image $IMG ($SIZE_BYTES bytes)"
  truncate -s "$SIZE_BYTES" "$IMG"
  need_format=1
elif [ "$FORCE" = "1" ]; then
  if [ -n "$SIZE_BYTES" ]; then
    truncate -s "$SIZE_BYTES" "$IMG"
  fi
  echo "Reformatting $IMG (FORCE=1)"
  need_format=1
else
  # Validate EXT magic at classic superblock offset 1024.
  magic="$(dd if="$IMG" bs=1 skip=1080 count=2 2>/dev/null | od -An -tx1 | tr -d ' \n')"
  # little-endian 0xEF53 → bytes 53 ef
  if [ "$magic" != "53ef" ]; then
    echo "NOTE: not an EXT image (magic=$magic); reformatting $IMG" >&2
    need_format=1
  fi
fi

if [ "$need_format" = "1" ]; then
  # Plain EXT2 — Kolibri rejects modern ext4-only incompat features and
  # blocksTotal_hi != 0. Keep the image small and feature-minimal.
  mkfs.ext2 -F -b 1024 -I 128 -L kolibri "$IMG"
fi

mkdir -p "$MNT"
LOOP="$(losetup -f)"
losetup "$LOOP" "$IMG"
echo "LOOP=$LOOP"
cleanup() {
  umount "$MNT" 2>/dev/null || true
  losetup -d "$LOOP" 2>/dev/null || true
}
trap cleanup EXIT

mount -t ext2 "$LOOP" "$MNT"

find "$MNT" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

printf '%s' 'KolibriOS filesystem regression-test fixture
Purpose: deterministic IDE disk for QEMU kernel tests.
Do not use as a boot disk.
' >"$MNT/README.TXT"

printf '%s' 'ROOT.TXT: file in the volume root for enumeration tests.
' >"$MNT/ROOT.TXT"

: >"$MNT/EMPTY.TXT"

python3 - <<'PY'
from pathlib import Path
Path("/mnt/ext/TINY.BIN").write_bytes(b"\x00\x01\x02\x03\xff\xfe" * 32)
PY

mkdir -p "$MNT/DATA" "$MNT/NESTED/A" "$MNT/NESTED/B" "$MNT/LARGE" "$MNT/FILES WITH SPACES"

printf '%s' 'DATA/ONE.TXT: first file in DATA/.
' >"$MNT/DATA/ONE.TXT"
printf '%s' 'DATA/TWO.TXT: second file in DATA/.
' >"$MNT/DATA/TWO.TXT"
printf '%s' 'DATA/THREE.TXT: third file in DATA/.
' >"$MNT/DATA/THREE.TXT"

printf '%s' 'NESTED/A/FILE_A1.TXT
' >"$MNT/NESTED/A/FILE_A1.TXT"
printf '%s' 'NESTED/A/FILE_A2.TXT
' >"$MNT/NESTED/A/FILE_A2.TXT"
printf '%s' 'NESTED/B/FILE_B1.TXT
' >"$MNT/NESTED/B/FILE_B1.TXT"
printf '%s' 'NESTED/B/FILE_B2.TXT
' >"$MNT/NESTED/B/FILE_B2.TXT"

python3 - <<'PY'
from pathlib import Path
Path("/mnt/ext/LARGE/LARGE.TXT").write_bytes(b"LARGE-PAYLOAD-" * 4096)
PY

printf '%s' 'space name content
' >"$MNT/FILES WITH SPACES/HELLO WORLD.TXT"

sync
echo "EXT populate OK: $IMG"
