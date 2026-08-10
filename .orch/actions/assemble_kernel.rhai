//! @assemble_kernel — apply migration gates and assemble `kernel.mnt` with FASM.
//! Requires blobs from `@build_rust`. CONFIG_DATA: `project/build.toml`.

fn config_path() { "project/build.toml" }
fn lang_inc() { "kernel/lang.inc" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn require_python() {
    let py = path::which("python");
    if py == () {
        py = path::which("python3");
    }
    if py == () {
        fail("Python not found for migration_gates");
    }
    py
}

fn require_success(handle, what) {
    let code = handle.wait();
    if !success(code) {
        fail(`${what}: exit ${code}`);
    }
}

fn require_file(p, what) {
    if !path::is_file(p) {
        fail(`${what}: ${p}`);
    }
}

let cfg = require_config();
let rust = cfg["rust"];
let kernel = cfg["kernel"];
let out_dir = rust["out_dir"];

for blob in rust["blobs"] {
    require_file(
        `${out_dir}/${blob["out"]}`,
        `Rust blob missing before kernel assemble — run @build_rust first`
    );
}

log::info("Applying migration gates");
require_success(
    process::run(require_python(), ["tools/migration_gates/apply_gates.py", config_path()]),
    "apply_gates failed"
);

let fasm = kernel["fasm"];
let asm = kernel["asm"];
let out = kernel["output"];
let lang = kernel["lang"];
let mem = kernel["memory_kib"];

require_file(asm, "kernel asm missing");
require_file(fasm, "FASM missing");

dir::create("kernel/bin");
if path::is_file(out) {
    file::remove(out);
}

let lang_path = lang_inc();
// FASM expects exactly: `lang fix en_US` + newline (`fix` defines the `lang` symbol).
file::write_all(lang_path, "lang fix " + lang + "\n");
execution::on_cleanup(`file::remove("${lang_path}");`);

log::info("Assembling kernel with FASM");
let p = process::run(fasm, ["-m", `${mem}`, asm, out]);
let code = p.wait();
if path::is_file(lang_path) {
    file::remove(lang_path);
}
if !success(code) {
    fail(`FASM assemble failed: exit ${code}`);
}
require_file(out, "kernel artifact missing after assemble");

log::info(`Kernel built: ${out}`);
