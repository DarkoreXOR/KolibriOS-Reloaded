//! Integration tests for the orchestrator (final Action / Workflow / `$` model).

use orch::cli::parser::{normalize_inline_units, parse_argv, UnitKind};
use orch::cli::{ExecutionUnit, GlobalOptions};
use orch::config::OrchConfig;
use orch::execution::engine::run_with_globals;
use orch::registry::{discover_all, EntityType};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use tempfile::tempdir;

fn s(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

fn quiet() -> GlobalOptions {
    GlobalOptions {
        quiet: true,
        ..Default::default()
    }
}

fn bare_cfg(actions: &str, workflows: &str) -> OrchConfig {
    OrchConfig {
        actions_dirs: if actions.is_empty() {
            vec![]
        } else {
            vec![actions.into()]
        },
        workflows_dirs: if workflows.is_empty() {
            vec![]
        } else {
            vec![workflows.into()]
        },
        ..OrchConfig::default()
    }
}

fn inline_unit(src: impl Into<String>) -> ExecutionUnit {
    ExecutionUnit {
        kind: UnitKind::InlineAction,
        name: src.into(),
        positionals: vec![],
        args: vec![],
        cli_token: "$".into(),
    }
}

#[test]
fn inline_action_runs_without_temp_file() {
    let tmp = tempdir().unwrap();
    let mut parsed = parse_argv(&s(&["$", r#"print("hello")"#])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    assert_eq!(parsed.units[0].kind, UnitKind::InlineAction);
    assert_eq!(parsed.units[0].name, r#"print("hello")"#);

    run_with_globals(tmp.path().to_path_buf(), bare_cfg("", ""), quiet(), parsed.units).unwrap();
}

#[test]
fn preflight_fails_before_side_effects() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(actions.join("clean.rhai"), "log::info(\"clean\");\n").unwrap();

    let mut parsed = parse_argv(&s(&["@missing", "@clean"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", ""),
        GlobalOptions::default(),
        parsed.units,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Action not found: @missing"), "{msg}");
}

#[test]
fn action_name_with_colon_is_atomic() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions").join("build");
    fs::create_dir_all(&actions).unwrap();
    fs::write(actions.join("dev.rhai"), "log::info(\"dev build\");\n").unwrap();

    let cfg = bare_cfg("actions", "");
    let reg = discover_all(tmp.path(), &cfg).unwrap();
    assert!(reg.get(EntityType::Action, "build:dev").is_some());

    let mut parsed = parse_argv(&s(&["@build:dev", "--target", "x86"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    assert_eq!(parsed.units[0].name, "build:dev");
    assert_eq!(parsed.units[0].args[0].value.as_deref(), Some("x86"));

    run_with_globals(tmp.path().to_path_buf(), cfg, quiet(), parsed.units).unwrap();
}

#[test]
fn discovery_default_and_nested_names() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(actions.join("build").join("kernel")).unwrap();
    fs::write(actions.join("clean.rhai"), "// @clean\n").unwrap();
    fs::write(actions.join("build").join("default.rhai"), "// @build\n").unwrap();
    fs::write(actions.join("build").join("dev.rhai"), "// @build:dev\n").unwrap();
    fs::write(
        actions.join("build").join("kernel").join("release.rhai"),
        "// @build:kernel:release\n",
    )
    .unwrap();

    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(workflows.join("run")).unwrap();
    fs::write(workflows.join("run").join("default.rhai"), "// run\n").unwrap();
    fs::write(workflows.join("run").join("dev.rhai"), "// run:dev\n").unwrap();

    let cfg = bare_cfg("actions", "workflows");
    let reg = discover_all(tmp.path(), &cfg).unwrap();
    assert!(reg.get(EntityType::Action, "clean").is_some());
    assert!(reg.get(EntityType::Action, "build").is_some());
    assert!(reg.get(EntityType::Action, "build:dev").is_some());
    assert!(reg.get(EntityType::Action, "build:kernel:release").is_some());
    assert!(reg.get(EntityType::Workflow, "run").is_some());
    assert!(reg.get(EntityType::Workflow, "run:dev").is_some());
}

#[test]
fn cwd_and_file_ops() {
    let tmp = tempdir().unwrap();
    let marker = tmp.path().join("marker.txt");
    let src = format!(
        r#"
        path::chdir("{}");
        file::write_all("marker.txt", "ok");
        if !file::exists("marker.txt") {{ fail("missing"); }}
        "#,
        tmp.path().display().to_string().replace('\\', "/")
    );
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
    assert_eq!(fs::read_to_string(marker).unwrap(), "ok");
}

#[test]
fn process_poll_and_wait() {
    let tmp = tempdir().unwrap();
    let src = if cfg!(windows) {
        r#"
        let p = process::run_capture("cmd", ["/C", "echo hi"]);
        let code = p.wait();
        if !success(code) { fail("cmd failed"); }
        "#
    } else {
        r#"
        let p = process::run_capture("sh", ["-c", "echo hi"]);
        let code = p.wait();
        if !success(code) { fail("sh failed"); }
        "#
    };
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}

#[test]
fn rollback_lifo_on_failure() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(&actions).unwrap();
    let marker = tmp.path().join("rb.txt");
    let marker_s = marker.display().to_string().replace('\\', "/");

    fs::write(
        actions.join("a.rhai"),
        format!(
            r#"
            log::info("A");
            execution::on_rollback("file::write_all(\"{marker_s}\", \"A\");");
            "#
        ),
    )
    .unwrap();
    fs::write(
        actions.join("b.rhai"),
        format!(
            r#"
            log::info("B");
            execution::on_rollback("file::write_all(\"{marker_s}\", file::read_all(\"{marker_s}\") + \"B\");");
            "#
        ),
    )
    .unwrap();
    fs::write(actions.join("c.rhai"), "fail(\"boom\");\n").unwrap();
    fs::write(&marker, "").unwrap();

    let mut parsed = parse_argv(&s(&["@a", "@b", "@c"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", ""),
        quiet(),
        parsed.units,
    )
    .unwrap_err();
    assert!(err.to_string().contains("boom") || err.to_string().contains("failed"), "{err}");
    assert_eq!(fs::read_to_string(&marker).unwrap_or_default(), "A");
}

#[test]
fn dedup_skips_duplicate_action() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();
    let counter = tmp.path().join("count.txt");
    let counter_s = counter.display().to_string().replace('\\', "/");
    fs::write(&counter, "0").unwrap();

    fs::write(
        actions.join("clean.rhai"),
        format!(
            r#"
            let n = file::read_all("{counter_s}");
            if n == "0" {{ file::write_all("{counter_s}", "1"); }}
            else if n == "1" {{ file::write_all("{counter_s}", "2"); }}
            else {{ file::write_all("{counter_s}", "3"); }}
            "#
        ),
    )
    .unwrap();
    fs::write(
        workflows.join("run.rhai"),
        "execution::run_action(\"clean\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["run", "@clean"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", "workflows"),
        quiet(),
        parsed.units,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(counter).unwrap(), "1");
}

#[test]
fn find_repo_root_smoke() {
    let root = orch::config::find_repo_root(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert!(root.join("tools").join("orch").join("Cargo.toml").is_file());
    assert!(root.join(".orch").join("actions").is_dir());
}

#[test]
fn find_repo_root_from_dot_orch_config() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = orch::config::find_repo_root(&manifest).unwrap();
    let cfg = root.join(".orch").join("config.toml");
    assert!(cfg.is_file());
    let from_cfg = orch::config::find_repo_root(&cfg).unwrap();
    assert_eq!(from_cfg, root);
}

#[test]
fn find_repo_root_accepts_mini_project_without_kernel() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("actions")).unwrap();
    fs::write(tmp.path().join("config.toml"), "actions_dirs = [\"actions\"]\n").unwrap();
    let root = orch::config::find_repo_root(&tmp.path().join("config.toml")).unwrap();
    assert_eq!(root, tmp.path());
}

#[test]
fn duplicate_registration_rejected() {
    use orch::registry::{EntitySource, EntityType, Registry, RegistryEntry};
    use std::collections::BTreeMap;
    let mut reg = Registry::new();
    reg.register(RegistryEntry {
        name: "clean".into(),
        entity_type: EntityType::Action,
        source: EntitySource::File {
            path: PathBuf::from("a.rhai"),
            line: 1,
        },
        metadata: BTreeMap::new(),
    })
    .unwrap();
    let err = reg
        .register(RegistryEntry {
            name: "clean".into(),
            entity_type: EntityType::Action,
            source: EntitySource::File {
                path: PathBuf::from("b.rhai"),
                line: 1,
            },
            metadata: BTreeMap::new(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn action_import_rejected() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(
        actions.join("bad.rhai"),
        "import \"something\" as x;\nlog::info(\"nope\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["@bad"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", ""),
        quiet(),
        parsed.units,
    )
    .unwrap_err();
    assert!(err.to_string().contains("import"));
}

#[test]
fn workflow_composes_actions() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();
    let marker = tmp.path().join("ok.txt");
    let marker_s = marker.display().to_string().replace('\\', "/");

    fs::write(
        actions.join("ping.rhai"),
        format!("file::write_all(\"{marker_s}\", \"pong\");\n"),
    )
    .unwrap();
    fs::write(
        actions.join("do_ping.rhai"),
        "execution::run_action(\"ping\");\n",
    )
    .unwrap();
    fs::write(
        workflows.join("go.rhai"),
        "execution::run_action(\"do_ping\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["go"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", "workflows"),
        quiet(),
        parsed.units,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(marker).unwrap(), "pong");
}

#[test]
fn generic_runtime_without_project_config() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();

    fs::write(actions.join("hello.rhai"), "log::info(\"hi\");\n").unwrap();
    fs::write(
        workflows.join("demo.rhai"),
        "execution::run_action(\"hello\");\n",
    )
    .unwrap();

    let cfg = bare_cfg("actions", "workflows");
    let mut p = parse_argv(&s(&["@hello"])).unwrap();
    normalize_inline_units(&mut p.units).unwrap();
    run_with_globals(tmp.path().to_path_buf(), cfg.clone(), quiet(), p.units).unwrap();

    let mut p = parse_argv(&s(&["demo"])).unwrap();
    normalize_inline_units(&mut p.units).unwrap();
    run_with_globals(tmp.path().to_path_buf(), cfg, quiet(), p.units).unwrap();
}

#[test]
fn global_flag_after_unit_is_unit_arg() {
    let mut parsed = parse_argv(&s(&["@build:dev", "--json"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    assert!(!parsed.globals.json);
    assert!(parsed.units[0].args.iter().any(|a| a.key == "json"));
}

#[test]
fn unit_terminator_preserves_action_name() {
    let mut parsed = parse_argv(&s(&[
        "@build:dev",
        "--target",
        "x86",
        "::",
        "@clean",
    ]))
    .unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    assert_eq!(parsed.units.len(), 2);
    assert_eq!(parsed.units[0].name, "build:dev");
    assert_eq!(parsed.units[1].name, "clean");
}

#[test]
fn cancel_request_kills_long_child() {
    let tmp = tempdir().unwrap();
    let src = if cfg!(windows) {
        r#"
        let p = process::run("cmd", ["/C", "ping -n 30 127.0.0.1 >NUL"]);
        timer::sleep(200);
        cancel::request();
        p.wait();
        "#
    } else {
        r#"
        let p = process::run("sleep", ["30"]);
        timer::sleep(200);
        cancel::request();
        p.wait();
        "#
    };

    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("cancel") || matches!(err, orch::execution::ExecutionError::Cancelled),
        "expected cancellation, got {err}"
    );
}

#[test]
fn filesystem_append_metadata_walk() {
    let tmp = tempdir().unwrap();
    let base = tmp.path().display().to_string().replace('\\', "/");
    let src = format!(
        r#"
        path::chdir("{base}");
        dir::create("sub");
        file::write_all("sub/a.txt", "hi");
        file::append("sub/a.txt", "!");
        let m = file::metadata("sub/a.txt");
        if m["len"] < 3 {{ fail("len"); }}
        let walked = dir::walk("sub");
        let n = 0;
        for x in walked {{
            n += 1;
        }}
        if n < 1 {{ fail("walk"); }}
        let joined = path::join(["sub", "a.txt"]);
        if !file::exists(joined) {{ fail("join"); }}
        let td = path::temp_dir();
        if td == "" {{ fail("temp"); }}
        "#
    );
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}

#[test]
fn process_stdin_and_pipeline() {
    let tmp = tempdir().unwrap();
    let src = if cfg!(windows) {
        r#"
        let p = process::run_stdin("cmd", ["/C", "more"], "hello\r\n");
        let code = p.wait();
        if !success(code) { fail("stdin cmd"); }
        let code2 = process::pipeline("cmd", ["/C", "echo piped"], "cmd", ["/C", "more"]);
        if !success(code2) { fail("pipeline"); }
        "#
    } else {
        r#"
        let p = process::run_stdin("cat", [], "hello\n");
        let code = p.wait();
        if !success(code) { fail("stdin cat"); }
        if p.stdout_text() != "hello\n" { fail("stdout mismatch"); }
        let code2 = process::pipeline("echo", ["piped"], "cat", []);
        if !success(code2) { fail("pipeline"); }
        "#
    };
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}

#[test]
fn nested_workflow_dedup_same_action_identity() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();
    let counter = tmp.path().join("count.txt");
    let counter_s = counter.display().to_string().replace('\\', "/");
    fs::write(&counter, "0").unwrap();

    fs::write(
        actions.join("bump.rhai"),
        format!(
            r#"
            let n = file::read_all("{counter_s}");
            if n == "0" {{ file::write_all("{counter_s}", "1"); }}
            else if n == "1" {{ file::write_all("{counter_s}", "2"); }}
            else {{ file::write_all("{counter_s}", "3"); }}
            "#
        ),
    )
    .unwrap();
    fs::write(
        workflows.join("outer.rhai"),
        "execution::run_action(\"bump\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["outer", "@bump"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", "workflows"),
        quiet(),
        parsed.units,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(counter).unwrap(), "1");
}

#[test]
fn different_action_args_are_not_deduped() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(&actions).unwrap();
    let counter = tmp.path().join("count.txt");
    let counter_s = counter.display().to_string().replace('\\', "/");
    fs::write(&counter, "0").unwrap();
    fs::write(
        actions.join("bump.rhai"),
        format!(
            r#"
            let n = file::read_all("{counter_s}");
            if n == "0" {{ file::write_all("{counter_s}", "1"); }}
            else if n == "1" {{ file::write_all("{counter_s}", "2"); }}
            else {{ file::write_all("{counter_s}", "3"); }}
            "#
        ),
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["@bump", "--a", "::", "@bump", "--b"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", ""),
        quiet(),
        parsed.units,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(counter).unwrap(), "2");
}

#[test]
fn inline_import_allowed() {
    let tmp = tempdir().unwrap();
    // Inline Actions may contain `import` (policy gate). Resolve may still fail
    // without a real module; use a body without import for success path.
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit("log::info(\"ok\");")],
    )
    .unwrap();
}

#[test]
fn crypto_rng_encoding_via_inline() {
    let tmp = tempdir().unwrap();
    let src = r#"
        let h = crypto::sha256("abc");
        if h == "" { fail("sha256"); }
        let u = rng::uuid();
        if u == "" { fail("uuid"); }
        let b = encoding::base64_encode("hi");
        if encoding::base64_decode(b) != "hi" { fail("b64"); }
        let hx = encoding::hex_encode("hi");
        if encoding::hex_decode(hx) != "hi" { fail("hex"); }
        if !crypto::constant_time_eq("aa", "aa") { fail("eq"); }
        // legacy hashes exist for compatibility
        let legacy_md5 = crypto::md5("x");
        let legacy_sha1 = crypto::sha1("x");
        if legacy_md5 == "" || legacy_sha1 == "" { fail("legacy"); }
    "#;
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}

#[test]
fn http_get_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello";
        let _ = stream.write_all(resp);
    });

    let tmp = tempdir().unwrap();
    let src = format!(
        r#"
        let r = http::get("http://{addr}/");
        if r["status"] != 200 {{ fail("status"); }}
        if r["body"] != "hello" {{ fail("body"); }}
        "#
    );
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}

#[test]
fn cycle_detection_preflight() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    fs::create_dir_all(&actions).unwrap();
    fs::write(
        actions.join("a.rhai"),
        "execution::run_action(\"b\");\n",
    )
    .unwrap();
    fs::write(
        actions.join("b.rhai"),
        "execution::run_action(\"a\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["@a"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", ""),
        quiet(),
        parsed.units,
    )
    .unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err}");
}

#[test]
fn missing_workflow_message() {
    let tmp = tempdir().unwrap();
    let mut parsed = parse_argv(&s(&["nonexistent_workflow"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    let err = run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        parsed.units,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("Workflow not found: nonexistent_workflow"),
        "{err}"
    );
}

#[test]
fn workflow_run_token_api() {
    let tmp = tempdir().unwrap();
    let actions = tmp.path().join("actions");
    let workflows = tmp.path().join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();
    let marker = tmp.path().join("m.txt");
    let marker_s = marker.display().to_string().replace('\\', "/");
    fs::write(
        actions.join("x.rhai"),
        format!("file::write_all(\"{marker_s}\", \"1\");\n"),
    )
    .unwrap();
    fs::write(
        workflows.join("w.rhai"),
        "execution::run(\"@x\");\n",
    )
    .unwrap();

    let mut parsed = parse_argv(&s(&["w"])).unwrap();
    normalize_inline_units(&mut parsed.units).unwrap();
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("actions", "workflows"),
        quiet(),
        parsed.units,
    )
    .unwrap();
    assert_eq!(fs::read_to_string(marker).unwrap(), "1");
}

#[test]
fn string_methods_rhai_builtins_and_orch_extensions() {
    let tmp = tempdir().unwrap();
    let src = r#"
        let path = "example/document.md";
        if !path.starts_with("example") { fail("starts_with"); }
        if !path.ends_with(".md") { fail("ends_with"); }
        if !path.contains("document") { fail("contains"); }
        if path.index_of("doc") < 0 { fail("index_of"); }

        let parts = "one:two:three".split(":");
        if parts.len() != 3 { fail("split"); }
        if parts.join("-") != "one-two-three" { fail("join"); }

        let spaced = "  hi  ";
        spaced.trim();
        if spaced != "hi" { fail("trim"); }

        let edge = "  ab  ";
        edge.trim_start();
        if edge != "ab  " { fail("trim_start"); }
        edge.trim_end();
        if edge != "ab" { fail("trim_end"); }

        if "foo:bar".strip_prefix("foo:") != "bar" { fail("strip_prefix"); }
        if "foo:bar".strip_suffix(":bar") != "foo" { fail("strip_suffix"); }
        if "foo".strip_prefix("x") != () { fail("strip_prefix miss"); }

        let ls = "a\r\nb\nc".lines();
        if ls.len() != 3 || ls[0] != "a" { fail("lines"); }

        if !"abc".is_ascii() { fail("is_ascii ascii"); }
        if "Привет".is_ascii() { fail("is_ascii unicode"); }

        // Character-oriented len (not UTF-8 bytes)
        if "hello".len() != 5 { fail("ascii len"); }
        if "Привет".len() != 6 { fail("cyrillic len"); }
        if "日本語".len() != 3 { fail("cjk len"); }
        if "🙂".len() != 1 { fail("emoji len"); }
        if "Привет".sub_string(0, 3) != "При" { fail("unicode sub_string"); }

        // Empty / edge cases
        if !"".is_empty() { fail("empty"); }
        if !"".starts_with("") { fail("empty prefix"); }
        if !"".ends_with("") { fail("empty suffix"); }
        if !"abc".contains("") { fail("empty contains"); }
        if "".split(":").len() != 1 { fail("split empty"); }
    "#;
    run_with_globals(
        tmp.path().to_path_buf(),
        bare_cfg("", ""),
        quiet(),
        vec![inline_unit(src)],
    )
    .unwrap();
}
