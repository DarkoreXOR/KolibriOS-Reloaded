//! run — default pipeline: build (dev) → image → QEMU.
execution::run_action("build:dev");
execution::run_action("prepare_image");
execution::run_action("run_qemu");
