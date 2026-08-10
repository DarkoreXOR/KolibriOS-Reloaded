//! @build:dev — development kernel build (`[modes.dev]` in project/build.toml).
execution::run_action("build_kernel", ["--mode", "dev"]);
