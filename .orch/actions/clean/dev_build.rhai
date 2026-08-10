//! @clean:dev_build — delete disposable files under `dev_build/*`.
//! Keeps `dev_build/README.md`. Never touches `images/` or `build/`.

fn clean_dev_build() {
    if !dir::exists("dev_build") {
        log::info("dev_build/ absent — nothing to clean");
        return;
    }

    let removed = 0;
    for name in dir::list("dev_build") {
        if name == "README.md" {
            continue;
        }
        let p = `dev_build/${name}`;
        if path::is_file(p) {
            file::remove(p);
            removed += 1;
            log::info(`removed ${p}`);
        } else if dir::exists(p) {
            dir::remove(p);
            removed += 1;
            log::info(`removed ${p}/`);
        }
    }
    log::info(`dev_build clean: ${removed} entries removed (README preserved)`);
}

log::info("Cleaning dev_build/*");
clean_dev_build();
