#!/bin/sh
# Populate / recreate an XFS regression image inside a privileged Linux container.
# Invoked by tools/mkfs_utils/create_xfs_image.py (Docker backend).
set -eu

IMG="${IMG:-/img/xfs-image.img}"
MNT="${MNT:-/mnt/xfs}"
FORCE="${FORCE:-0}"
SIZE_BYTES="${SIZE_BYTES:-}"

apk add --no-cache xfsprogs xfsprogs-extra python3
export PATH="/usr/sbin:/sbin:/usr/bin:/bin:${PATH}"

if [ ! -f "$IMG" ]; then
  if [ -z "$SIZE_BYTES" ]; then
    echo "ERROR: image missing and SIZE_BYTES not set: $IMG" >&2
    exit 1
  fi
  echo "Creating sparse image $IMG ($SIZE_BYTES bytes)"
  truncate -s "$SIZE_BYTES" "$IMG"
  mkfs.xfs -f -L kolibri -m crc=1,finobt=1 "$IMG"
elif [ "$FORCE" = "1" ]; then
  if [ -n "$SIZE_BYTES" ]; then
    truncate -s "$SIZE_BYTES" "$IMG"
  fi
  echo "Reformatting $IMG (FORCE=1)"
  mkfs.xfs -f -L kolibri -m crc=1,finobt=1 "$IMG"
else
  # Validate existing superblock via magic bytes (no xfs_db required).
  magic="$(dd if="$IMG" bs=4 count=1 2>/dev/null || true)"
  if [ "$magic" != "XFSB" ]; then
    echo "ERROR: not an XFS image: $IMG" >&2
    exit 1
  fi
fi

mkdir -p "$MNT"
# BusyBox losetup has no --show; allocate then attach.
LOOP="$(losetup -f)"
losetup "$LOOP" "$IMG"
echo "LOOP=$LOOP"
cleanup() {
  umount "$MNT" 2>/dev/null || true
  losetup -d "$LOOP" 2>/dev/null || true
}
trap cleanup EXIT

# Match FAT boot floppy label for Eolite volume display (CRC-safe via xfs_admin).
xfs_admin -L kolibri "$LOOP" 2>/dev/null || xfs_admin -L kolibri "$IMG" || true

mount -t xfs "$LOOP" "$MNT"

# Wipe previous fixture contents (keep FS).
find "$MNT" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

# Deterministic regression tree (matches tools/mkfs_utils/test_tree.py).
printf '%s' 'KolibriOS filesystem regression-test fixture
Purpose: deterministic IDE disk for QEMU kernel tests.
Do not use as a boot disk.
' >"$MNT/README.TXT"

printf '%s' 'ROOT.TXT: file in the volume root for enumeration tests.
' >"$MNT/ROOT.TXT"

: >"$MNT/EMPTY.TXT"

# TINY.BIN = (00 01 02 03 ff fe) * 32
python3 - <<'PY'
from pathlib import Path
Path("/mnt/xfs/TINY.BIN").write_bytes(b"\x00\x01\x02\x03\xff\xfe" * 32)
PY

mkdir -p "$MNT/DATA" "$MNT/NESTED/A" "$MNT/NESTED/B" "$MNT/LARGE" "$MNT/FILES WITH SPACES"

printf '%s' 'DATA/ONE.TXT: first file in DATA/.
' >"$MNT/DATA/ONE.TXT"
printf '%s' 'DATA/TWO.TXT: second file in DATA/.
' >"$MNT/DATA/TWO.TXT"
printf '%s' 'DATA/THREE.TXT: third file in DATA/.
' >"$MNT/DATA/THREE.TXT"

printf '%s' 'NESTED/A/FILE_A1.TXT: nested under NESTED/A/.
' >"$MNT/NESTED/A/FILE_A1.TXT"
printf '%s' 'NESTED/A/FILE_A2.TXT: sibling of FILE_A1.TXT.
' >"$MNT/NESTED/A/FILE_A2.TXT"
printf '%s' 'NESTED/B/FILE_B1.TXT: nested under NESTED/B/.
' >"$MNT/NESTED/B/FILE_B1.TXT"
printf '%s' 'NESTED/B/FILE_B2.TXT: sibling of FILE_B1.TXT.
' >"$MNT/NESTED/B/FILE_B2.TXT"

printf '%s' 'HELLO WORLD.TXT: filename contains a space for regression tests.
' >"$MNT/FILES WITH SPACES/HELLO WORLD.TXT"

python3 - <<'PY'
import hashlib
from pathlib import Path
line = (
    "LARGE.TXT line {:05d}: "
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 "
    "kolibrios-fs-regression-fixture\n"
)
body = "".join(line.format(i) for i in range(1024))
digest = hashlib.sha256(body.encode("ascii")).hexdigest()
trailer = f"END LARGE.TXT sha256={digest} lines=1024\n"
Path("/mnt/xfs/LARGE/LARGE.TXT").write_bytes((body + trailer).encode("ascii"))
PY

sync
echo "Populated root:"
ls -la "$MNT"
echo "OK"
