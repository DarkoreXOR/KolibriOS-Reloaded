//! @build:release — release kernel build (`[modes.release]` in project/build.toml).
execution::run_action("build_kernel", ["--mode", "release"]);
