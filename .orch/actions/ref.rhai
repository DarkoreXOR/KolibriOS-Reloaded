//! @ref — boot the immutable reference floppy with `-snapshot` (no rebuild).

fn config_path() { "project/build.toml" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn require_success(handle, what) {
    let code = handle.wait();
    if !success(code) {
        fail(`${what}: exit ${code}`);
    }
}

fn find_qemu(qemu_cfg) {
    for cand in qemu_cfg["executables"] {
        if path::is_file(cand) {
            return cand;
        }
        let w = path::which(cand);
        if w != () {
            return w;
        }
    }
    fail("QEMU executable not found");
}

let cfg = require_config();
let base = cfg["image"]["base_image"];
if !path::is_file(base) {
    fail(`reference image missing: ${base}`);
}

let qemu_cfg = cfg["qemu"];
let qemu = find_qemu(qemu_cfg);

let qargs = ["-fda", base];
for a in qemu_cfg["args"] {
    qargs.push(a);
}
for a in qemu_cfg["reference_extra_args"] {
    qargs.push(a);
}

let headless = unit_args["headless"] == true;
for a in args {
    if a == "--headless" {
        headless = true;
    }
}
if headless {
    for a in qemu_cfg["headless_extra_args"] {
        qargs.push(a);
    }
}

log::info(`Starting QEMU (reference snapshot): ${base}`);
require_success(process::run(qemu, qargs), "QEMU ref failed");
