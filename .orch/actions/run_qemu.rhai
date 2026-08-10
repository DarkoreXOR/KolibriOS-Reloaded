//! @run_qemu — boot the image recorded by `@prepare_image`.
//!
//! Optional args:
//!   `--disk:TYPE`  `--memory:SIZE`  `--serial`  `--debug`  `--headless`

fn config_path() { "project/build.toml" }
fn last_image_marker() { "dev_build/last_image.txt" }
fn kernel_mnt() { "kernel/bin/kernel.mnt" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn require_file(p, what) {
    if !path::is_file(p) {
        fail(what);
    }
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
    fail("QEMU executable not found (see project/build.toml [qemu].executables)");
}

fn parse_run_flags(args, unit_args) {
    let disks = [];
    let memory = ();
    let serial = unit_args["serial"] == true;
    // `debug` is a Rhai reserved keyword — use enable_debug.
    let enable_debug = false;
    let headless = unit_args["headless"] == true;

    for a in args {
        let disk = a.strip_prefix("--disk:");
        if disk != () {
            disks.push(disk);
            continue;
        }
        let mem = a.strip_prefix("--memory:");
        if mem != () {
            memory = mem;
            continue;
        }
        if a == "--serial" {
            serial = true;
        } else if a == "--debug" {
            enable_debug = true;
        } else if a == "--headless" {
            headless = true;
        }
    }

    #{ disks: disks, memory: memory, serial: serial, enable_debug: enable_debug, headless: headless }
}

fn append_testdisk(qargs, testdisk) {
    let td = testdisk["image"];
    if path::is_file(td) {
        for tmpl in testdisk["drive_args"] {
            let drive = `${tmpl}`;
            drive.replace("{image}", path::resolve(td));
            qargs.push(drive);
        }
    } else {
        log::warn(`testdisk enabled but missing: ${td} — create with @mkfs exfat 128M`);
    }
    qargs
}

fn append_named_disks(qargs, disks) {
    let index = 0;
    for disk in disks {
        let img = `images/${disk}-image.img`;
        if !path::is_file(img) {
            fail(`disk image missing for ${disk}: ${img} — create with @mkfs ${disk} …`);
        }
        qargs.push("-drive");
        qargs.push(`file=${path::resolve(img)},format=raw,if=ide,index=${index},media=disk`);
        index += 1;
    }
    qargs
}

// --- main ---

let marker = last_image_marker();
let mnt = kernel_mnt();
require_file(marker, "no fresh image marker (dev_build/last_image.txt) — run @prepare_image first");
require_file(mnt, "refusing to launch QEMU — kernel.mnt missing; run @build first");

let image_path = file::read_all(marker);
image_path.trim();
require_file(image_path, `recorded image missing: ${image_path}`);

let cfg = require_config();
let qemu_cfg = cfg["qemu"];
let testdisk = cfg["testdisk"];
let qemu = find_qemu(qemu_cfg);
let flags = parse_run_flags(args, unit_args);

let qargs = ["-fda", image_path];
for a in qemu_cfg["args"] {
    qargs.push(a);
}

if flags.memory != () {
    qargs.push("-m");
    qargs.push(flags.memory);
}
if flags.serial {
    qargs.push("-serial");
    qargs.push("stdio");
}
if flags.enable_debug {
    dir::create("dev_build");
    qargs.push("-d");
    qargs.push("int,cpu_reset");
    qargs.push("-D");
    qargs.push("dev_build/qemu-debug.log");
}
if flags.headless {
    for a in qemu_cfg["headless_extra_args"] {
        qargs.push(a);
    }
}

if flags.disks.len() == 0 && testdisk["enabled"] {
    qargs = append_testdisk(qargs, testdisk);
} else {
    qargs = append_named_disks(qargs, flags.disks);
}

log::info("Starting QEMU");
require_success(process::run(qemu, qargs), "QEMU run failed");

if cfg["cleanup"]["delete_image_on_success"] {
    log::info(`Cleanup: removing ${image_path}`);
    file::remove(image_path);
}
