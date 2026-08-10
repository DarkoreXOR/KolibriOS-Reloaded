//! @clean — remove build artifacts (`build/`, `dev_build/`).
//! Never touches `images/`. Prefer workflow `clean` / `@clean:dev_build` to
//! clear only `dev_build/*` (README kept); use this action for a full wipe.

fn remove_if_present(dir) {
    if dir::exists(dir) {
        dir::remove(dir);
        log::info(`removed ${dir}/`);
    }
}

log::info("Cleaning build artifacts");
remove_if_present("build");
remove_if_present("dev_build");
log::info("Clean completed (images/ preserved)");
