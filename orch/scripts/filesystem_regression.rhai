// Ensure persistent regression images exist, then report paths for manual QEMU testing.
let exfat = mkfs("exfat", "4M");
let ntfs = mkfs("ntfs", "8M");
print(`exfat: ${exfat}`);
print(`ntfs: ${ntfs}`);
