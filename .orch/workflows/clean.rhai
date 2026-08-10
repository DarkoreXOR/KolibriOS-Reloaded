//! clean — remove disposable files under `dev_build/*` (keeps README).
//! For a full wipe of `build/` + `dev_build/`, use `@clean`.
execution::run_action("clean:dev_build");
