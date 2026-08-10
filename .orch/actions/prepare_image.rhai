//! @prepare_image — package a disposable CoW boot image for testing.
//! Requires a built `kernel.mnt`. Uses `tools/kolibri_img`.

fn config_path() { "project/build.toml" }
fn tool_alt() { "tools/kolibri_img/target/release/kolibri_img" }
fn last_image_marker() { "dev_build/last_image.txt" }

fn require_config() {
    let cfg_path = config_path();
    if !path::is_file(cfg_path) {
        fail(`missing build config: ${cfg_path}`);
    }
    toml::load(cfg_path)
}

fn require_file(p, what) {
    if !path::is_file(p) {
        fail(`${what}: ${p}`);
    }
}

fn require_success(handle, what) {
    let code = handle.wait();
    if !success(code) {
        fail(`${what}: exit ${code}`);
    }
}

fn ensure_kolibri_img(image) {
    let tool_bin = image["tool_bin"];
    if path::is_file(tool_bin) {
        return tool_bin;
    }
    let alt = tool_alt();
    if path::is_file(alt) {
        return alt;
    }

    log::info("Building kolibri_img (release)");
    let cargo = path::which("cargo");
    if cargo == () {
        fail("`cargo` not found on PATH");
    }
    require_success(
        process::run(cargo, [
            "build", "--release",
            "--manifest-path", image["tool_manifest"]
        ]),
        "kolibri_img build failed"
    );

    if path::is_file(tool_bin) {
        return tool_bin;
    }
    if path::is_file(alt) {
        return alt;
    }
    fail("kolibri_img binary missing after build");
}

fn resolve_output_dir(cfg, image) {
    let out_dir = image["output_dir"];
    let mode = env::get("ORCH_BUILD_MODE");
    if mode == () {
        let marker = "dev_build/orch-mode.txt";
        if path::is_file(marker) {
            mode = file::read_all(marker);
            mode.trim();
        }
    }
    if mode != () {
        let modes = cfg["modes"];
        if modes[mode] != () && modes[mode]["image_output_dir"] != () {
            out_dir = `${modes[mode]["image_output_dir"]}`;
            log::info(`prepare_image mode=${mode} output_dir=${out_dir}`);
        }
    }
    out_dir
}

let cfg = require_config();
let image = cfg["image"];
let kernel = cfg["kernel"]["output"];

require_file(kernel, "kernel artifact missing; cannot package image");
require_file(image["base_image"], "base/reference image missing");

let img_tool = ensure_kolibri_img(image);
let out_dir = resolve_output_dir(cfg, image);
dir::create(out_dir);

let fname = `${image["filename_pattern"]}`;
fname.replace("{timestamp}", time::utc_stamp());
let image_path = `${out_dir}/${fname}`;
let base = image["base_image"];

log::info(`Creating disk image ${image_path}`);
require_success(
    process::run(img_tool, ["cow", base, image_path]),
    "kolibri_img cow failed"
);

for del_path in image["delete_before_replace"] {
    log::info(`delete ${del_path}`);
    require_success(
        process::run(img_tool, ["delete", image_path, del_path]),
        `kolibri_img delete ${del_path} failed`
    );
}

let fat_name = image["kernel_fat_name"];
log::info(`replace ${fat_name}`);
require_success(
    process::run(img_tool, ["replace", image_path, fat_name, kernel]),
    "kolibri_img replace failed"
);

require_file(image_path, "test image missing after packaging");

dir::create("dev_build");
file::write_all(last_image_marker(), image_path);
log::info(`Boot image prepared: ${image_path}`);
