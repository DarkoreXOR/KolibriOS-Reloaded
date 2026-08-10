//! fs_images — generate default persistent exFAT + NTFS images under `images/`.
//! Generation only (not a regression check). Equivalent to `@mkfs` with no args.
execution::run_action("mkfs");
