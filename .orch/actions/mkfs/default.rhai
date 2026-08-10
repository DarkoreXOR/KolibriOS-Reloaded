//! @mkfs — ensure persistent filesystem images under `images/` (generation only).
//!
//! Usage:
//!   orch @mkfs              → default exFAT 4M + NTFS 8M
//!   orch @mkfs exfat 4M
//!   orch @mkfs ntfs 8M
//!
//! Workflow `fs_images` is the named equivalent of `@mkfs` with no args.

fn default_size(fs) {
    if fs == "ntfs" { "8M" } else { "4M" }
}

if args.len() == 0 {
    log::info("Creating default exFAT and NTFS images under images/");
    execution::run_action("mkfs:one", ["exfat", "4M"]);
    execution::run_action("mkfs:one", ["ntfs", "8M"]);
} else {
    let fs = args[0];
    let size = if args.len() > 1 { args[1] } else { default_size(fs) };
    execution::run_action("mkfs:one", [fs, size]);
}
