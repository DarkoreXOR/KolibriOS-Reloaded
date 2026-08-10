//! @build_rust — freestanding `kolibri_utils` staticlib + reloc-free blob extract.
//! CONFIG_DATA: `project/build.toml`.

fn config_path() { "project/build.toml" }
fn archive_rel() { "i686-kolibri-none/release/libkolibri_utils.a" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn require_which(name) {
    let p = path::which(name);
    if p == () {
        fail(`'${name}' not found on PATH`);
    }
    p
}

fn require_python(preferred) {
    let py = path::which(preferred);
    if py == () {
        py = path::which("python3");
    }
    if py == () {
        fail(`Python not found (configured '${preferred}')`);
    }
    py
}

fn require_success(handle, what) {
    let code = handle.wait();
    if !success(code) {
        fail(`${what}: exit ${code}`);
    }
}

fn parse_cli(args, unit_args) {
    let skip_tests = false;
    let mode = ();
    let i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--skip-tests" || a == "skip-tests" {
            skip_tests = true;
            i += 1;
        } else if a == "--mode" && i + 1 < args.len() {
            mode = args[i + 1];
            i += 2;
        } else if a.starts_with("--mode=") {
            mode = a.strip_prefix("--mode=");
            i += 1;
        } else {
            i += 1;
        }
    }
    if unit_args["skip-tests"] == true {
        skip_tests = true;
    }
    if unit_args["mode"] != () {
        mode = `${unit_args["mode"]}`;
    }
    #{ skip_tests: skip_tests, mode: mode }
}

fn apply_mode_overlay(cfg, rust, mode) {
    let run_host = rust["run_host_tests"];
    let force_recompile = rust["force_recompile_staticlib"];
    if mode != () {
        let modes = cfg["modes"];
        if modes[mode] != () {
            let mc = modes[mode];
            if mc["run_host_tests"] != () {
                run_host = mc["run_host_tests"];
            }
            if mc["force_recompile_staticlib"] != () {
                force_recompile = mc["force_recompile_staticlib"];
            }
            log::info(`build_rust mode=${mode}`);
        }
    }
    #{ run_host: run_host, force_recompile: force_recompile }
}

fn extract_blob(py, kind, generic, probe, archive, out_dir, blob) {
    let out = `${out_dir}/${blob["out"]}`;
    log::info(`extract ${blob["out"]}`);

    if kind == "generic" {
        let p = process::run(py, [
            generic,
            "--archive", archive,
            "--section", blob["section"],
            "--symbol", blob["symbol"],
            "--expect-ret-imm", `${blob["expect_ret_imm"]}`,
            "--out", out
        ]);
        require_success(p, `blob extract failed: ${blob["out"]}`);
    } else if kind == "probe" {
        let p = process::run(py, [probe, "--archive", archive, "--out", out]);
        require_success(p, `probe extract failed: ${blob["out"]}`);
    } else {
        fail(`unknown blob kind: ${kind}`);
    }

    if !path::is_file(out) {
        fail(`expected blob missing after extract: ${out}`);
    }
}

// --- main ---

let cfg = require_config();
let rust = cfg["rust"];
let extract = rust["extract"];

let workspace = rust["workspace"];
let pkg_name = rust["package"];
let target_json = rust["target_json"];
let target_dir = rust["cargo_target_dir"];
let out_dir = rust["out_dir"];
let toolchain = rust["toolchain"];

if !dir::exists(workspace) {
    fail(`rust workspace missing: ${workspace}`);
}
if !path::is_file(target_json) {
    fail(`freestanding target JSON missing: ${target_json}`);
}

let cargo = require_which("cargo");
let cli = parse_cli(args, unit_args);
let overlay = apply_mode_overlay(cfg, rust, cli.mode);
let archive = `${target_dir}/${archive_rel()}`;

log::info("Building Rust components");

if overlay.run_host && !cli.skip_tests {
    log::info(`host tests: cargo test -p ${pkg_name}`);
    env::set("CARGO_TARGET_DIR", path::resolve(target_dir));
    require_success(
        process::run_cwd(cargo, ["test", "-p", pkg_name], workspace),
        "cargo test failed"
    );
} else if cli.skip_tests {
    log::info("host tests: skipped");
} else {
    log::info("host tests: disabled by config/mode");
}

if overlay.force_recompile && path::is_file(archive) {
    log::info(`invalidate ${archive}`);
    file::remove(archive);
}

if rust["clear_rustflags"] {
    env::remove("RUSTFLAGS");
}
env::set("CARGO_TARGET_DIR", path::resolve(target_dir));

log::info(`freestanding staticlib (release, ${toolchain})`);
require_success(
    process::run_cwd(cargo, [
        `+${toolchain}`,
        "build",
        "-Z", "build-std=core,compiler_builtins",
        "-Z", "json-target-spec",
        "-p", pkg_name,
        "--release",
        "--target", path::resolve(target_json)
    ], workspace),
    "freestanding cargo build failed"
);

if !path::is_file(archive) {
    fail(`freestanding archive missing: ${archive}`);
}

dir::create(out_dir);
let py = require_python(extract["python"]);
let generic = extract["generic_script"];
let probe = extract["probe_script"];

for blob in rust["blobs"] {
    extract_blob(py, blob["kind"], generic, probe, archive, out_dir, blob);
}

log::info("Rust blobs ready");
