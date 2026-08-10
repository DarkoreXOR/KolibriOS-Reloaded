//! @build_kernel — Rust blobs + FASM assemble → `kernel/bin/kernel.mnt`.
//!
//! Mode selection (default `dev`):
//!   positional / `--mode NAME` / `--mode=NAME` / `--mode` unit arg
//! Mode names must exist under `[modes.<name>]` in `project/build.toml`.
//!
//! Note: Rhai script-level `const` is not visible inside `fn`; use helpers.

fn config_path() { "project/build.toml" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn parse_mode(args, unit_args) {
    let mode = "dev";
    let i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--mode" && i + 1 < args.len() {
            mode = args[i + 1];
            i += 2;
        } else if a.starts_with("--mode=") {
            mode = a.strip_prefix("--mode=");
            i += 1;
        } else {
            i += 1;
        }
    }
    if unit_args["mode"] != () {
        mode = `${unit_args["mode"]}`;
    }
    mode
}

fn persist_mode(mode, mode_cfg) {
    let marker = "dev_build/orch-mode.txt";
    if mode_cfg["mode_marker"] != () {
        marker = `${mode_cfg["mode_marker"]}`;
    }
    dir::create("dev_build");
    file::write_all(marker, mode);
    env::set("ORCH_BUILD_MODE", mode);
}

fn forward_args(args, mode) {
    let forward = ["--mode", mode];
    for a in args {
        if a != "--mode" && a != mode && !a.starts_with("--mode=") {
            forward.push(a);
        }
    }
    forward
}

let mode = parse_mode(args, unit_args);
let cfg = require_config();
let modes = cfg["modes"];
if modes[mode] == () {
    fail(`unknown build mode '${mode}' (expected [modes.${mode}] in ${config_path()})`);
}

persist_mode(mode, modes[mode]);
log::info(`Building kernel (mode=${mode})`);

execution::run_action("build_rust", forward_args(args, mode));
execution::run_action("assemble_kernel");
log::info(`Kernel built successfully (mode=${mode})`);
