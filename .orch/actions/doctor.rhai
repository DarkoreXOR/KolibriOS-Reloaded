//! @doctor — verify host tools and paths from `project/build.toml`.

fn config_path() { "project/build.toml" }

let cfg_path = config_path();
if !path::is_file(cfg_path) {
    fail(`missing ${cfg_path}`);
}

let cfg = toml::load(cfg_path);
let failed = [];

fn note(label, ok) {
    if ok {
        log::info(`OK  ${label}`);
    } else {
        log::error(`MISSING  ${label}`);
    }
    ok
}

fn check(failed, label, ok) {
    note(label, ok);
    if !ok {
        failed.push(label);
    }
    failed
}

failed = check(failed, "cargo", path::which("cargo") != ());
failed = check(failed, "python", path::which("python") != () || path::which("python3") != ());
failed = check(failed, "kernel asm", path::is_file(cfg["kernel"]["asm"]));
failed = check(failed, "FASM", path::is_file(cfg["kernel"]["fasm"]));
failed = check(failed, "base image", path::is_file(cfg["image"]["base_image"]));
failed = check(failed, "extract generic script", path::is_file(cfg["rust"]["extract"]["generic_script"]));
failed = check(failed, "kolibri_img manifest", path::is_file(cfg["image"]["tool_manifest"]));
failed = check(failed, "migration_gates tool", path::is_file("tools/migration_gates/apply_gates.py"));

let qemu_ok = false;
for cand in cfg["qemu"]["executables"] {
    if path::is_file(cand) || path::which(cand) != () {
        qemu_ok = true;
        break;
    }
}
failed = check(failed, "QEMU", qemu_ok);

if failed.len() > 0 {
    fail(`doctor found missing tools or paths: ${failed.join(", ")}`);
}

log::info("doctor: all checks passed");
