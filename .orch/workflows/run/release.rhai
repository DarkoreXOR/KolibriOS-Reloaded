//! run:release — build (release) → image → QEMU.
execution::run_action("build:release");
execution::run_action("prepare_image");
execution::run_action("run_qemu");
