//! @mkfs:one — create or reuse one persistent FS image via `tools/mkfs_utils`.
//! Args: `<exfat|ntfs> <size>`

fn require_success(handle, what) {
    let code = handle.wait();
    if !success(code) {
        fail(`${what}: exit ${code}`);
    }
}

fn script_for(fs) {
    if fs == "exfat" {
        "tools/mkfs_utils/create_exfat_image.py"
    } else if fs == "ntfs" {
        "tools/mkfs_utils/create_ntfs_image.py"
    } else {
        fail(`unsupported filesystem '${fs}' (expected exfat or ntfs)`);
    }
}

fn run_mkfs(script, size) {
    let py = path::which("python");
    if py != () {
        let p = process::run(py, [script, "--size", size]);
        let code = p.wait();
        if success(code) {
            return;
        }
    }
    let py3 = path::which("python3");
    if py3 == () {
        fail("Python not found (tried python, python3)");
    }
    require_success(
        process::run(py3, [script, "--size", size]),
        `mkfs failed`
    );
}

if args.len() < 2 {
    fail("@mkfs:one requires filesystem and size arguments (e.g. exfat 4M)");
}

let fs = args[0];
let size = args[1];
log::info(`mkfs ${fs} ${size}`);
run_mkfs(script_for(fs), size);
log::info(`mkfs ${fs} completed`);
