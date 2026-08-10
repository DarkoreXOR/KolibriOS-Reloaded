//! Argument parser for orchestrator CLI.
//!
//! Grammar:
//!   orch [GLOBAL_OPTIONS] [--] [EXECUTION_UNITS...]
//!
//! Execution units: `$src` (inline Action), `@name` (Action), `workflow`
//! Unit terminator: `::`
//! Global-option terminator: `--`
//!
//! Literal `::`: after a within-unit `--`, or via `--key=::`.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct GlobalOptions {
    pub quiet: bool,
    pub verbose: bool,
    pub json: bool,
    pub no_progress: bool,
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitKind {
    /// Anonymous one-off Action (`$` / `$source`).
    InlineAction,
    Action,
    Workflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgValue {
    pub key: String,
    /// `None` for bare flags (`--release`).
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionUnit {
    pub kind: UnitKind,
    /// Name without `@` prefix; for inline, the Rhai source text; for workflow, the name.
    pub name: String,
    /// Positional arguments for the unit.
    pub positionals: Vec<String>,
    /// Flag / key-value arguments.
    pub args: Vec<ArgValue>,
    /// Original CLI token that introduced this unit.
    pub cli_token: String,
}

#[derive(Debug, Clone)]
pub struct ParsedCli {
    pub globals: GlobalOptions,
    pub units: Vec<ExecutionUnit>,
    pub show_help: bool,
}

#[derive(Debug, Clone)]
pub enum CliError {
    HelpRequested,
    VersionRequested,
    Usage(String),
    Config(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HelpRequested => write!(f, "help requested"),
            Self::VersionRequested => write!(f, "version requested"),
            Self::Usage(msg) | Self::Config(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CliError {}

pub fn parse_argv(args: &[String]) -> Result<ParsedCli, CliError> {
    let mut idx = 0;
    if let Some(first) = args.first() {
        let base = std::path::Path::new(first)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if base == "orch" || first.ends_with("orch.exe") || first.ends_with("orch") {
            idx = 1;
        }
    }

    let mut globals = GlobalOptions::default();
    let mut units = Vec::new();
    let mut show_help = false;
    let mut in_globals = true;
    let mut current: Option<ExecutionUnit> = None;

    while idx < args.len() {
        let token = &args[idx];

        if in_globals {
            match token.as_str() {
                "-h" | "--help" | "help" => {
                    show_help = true;
                    idx += 1;
                    continue;
                }
                "--" => {
                    in_globals = false;
                    idx += 1;
                    continue;
                }
                "--quiet" => {
                    globals.quiet = true;
                    idx += 1;
                    continue;
                }
                "--verbose" => {
                    globals.verbose = true;
                    idx += 1;
                    continue;
                }
                "--json" => {
                    globals.json = true;
                    idx += 1;
                    continue;
                }
                "--no-progress" => {
                    globals.no_progress = true;
                    idx += 1;
                    continue;
                }
                "-V" | "--version" => {
                    return Err(CliError::VersionRequested);
                }
                t if t.starts_with("--config=") => {
                    globals.config = Some(PathBuf::from(&t["--config=".len()..]));
                    idx += 1;
                    continue;
                }
                "--config" => {
                    idx += 1;
                    let v = args.get(idx).ok_or_else(|| {
                        CliError::Usage("missing value for global option --config".into())
                    })?;
                    globals.config = Some(PathBuf::from(v));
                    idx += 1;
                    continue;
                }
                t if t.starts_with('-') => {
                    return Err(CliError::Usage(format!(
                        "unknown global option '{t}' (global options must appear before execution units)"
                    )));
                }
                _ => {
                    in_globals = false;
                }
            }
            if in_globals {
                continue;
            }
        }

        if token == "::" && !after_unit_ddash(&current) {
            if let Some(unit) = current.take() {
                units.push(unit);
            }
            idx += 1;
            continue;
        }

        // Reject obsolete `$$` explicitly.
        if token == "$$" || token.starts_with("$$") {
            return Err(CliError::Usage(
                "`$$` is no longer supported; use `$` for inline Rhai (anonymous Action)".into(),
            ));
        }

        // `$` / `@…` always start a new unit. Bare workflow names only start
        // a unit when no unit is currently open (otherwise they are positionals).
        let starts_unit = if token == "$"
            || (token.starts_with('$') && token.len() > 1)
            || (token.starts_with('@') && token.len() > 1)
        {
            try_parse_unit_start(token)?
        } else if current.is_none() {
            try_parse_unit_start(token)?
        } else {
            None
        };

        if let Some(unit) = starts_unit {
            if let Some(prev) = current.take() {
                units.push(prev);
            }
            current = Some(unit);
            idx += 1;
            continue;
        }

        let Some(cur) = current.as_mut() else {
            return Err(CliError::Usage(format!(
                "unexpected token '{token}': expected an execution unit ($, @, or workflow name)"
            )));
        };

        if token == "--" {
            cur.positionals.push(token.clone());
            idx += 1;
            continue;
        }

        if let Some(rest) = token.strip_prefix("--") {
            if rest.is_empty() {
                cur.positionals.push(token.clone());
                idx += 1;
                continue;
            }
            if let Some((k, v)) = rest.split_once('=') {
                cur.args.push(ArgValue {
                    key: k.to_string(),
                    value: Some(v.to_string()),
                });
                idx += 1;
                continue;
            }
            let key = rest.to_string();
            if let Some(next) = args.get(idx + 1) {
                if !next.starts_with('-') && !is_unit_start(next) && next.as_str() != "::" {
                    cur.args.push(ArgValue {
                        key,
                        value: Some(next.clone()),
                    });
                    idx += 2;
                    continue;
                }
            }
            cur.args.push(ArgValue {
                key,
                value: None,
            });
            idx += 1;
            continue;
        }

        cur.positionals.push(token.clone());
        idx += 1;
    }

    if let Some(unit) = current.take() {
        units.push(unit);
    }

    if !show_help && units.is_empty() {
        show_help = true;
    }

    Ok(ParsedCli {
        globals,
        units,
        show_help,
    })
}

fn is_unit_start(token: &str) -> bool {
    token == "$"
        || (token.starts_with('$') && token.len() > 1)
        || (token.starts_with('@') && token.len() > 1)
}

fn after_unit_ddash(current: &Option<ExecutionUnit>) -> bool {
    current
        .as_ref()
        .map(|u| u.positionals.iter().any(|p| p == "--"))
        .unwrap_or(false)
}

fn looks_like_workflow(token: &str) -> bool {
    if token.starts_with('-') || token == "::" {
        return false;
    }
    if token.starts_with('$') || token.starts_with('@') {
        return false;
    }
    true
}

fn try_parse_unit_start(token: &str) -> Result<Option<ExecutionUnit>, CliError> {
    if token == "$$" || token.starts_with("$$") {
        return Err(CliError::Usage(
            "`$$` is no longer supported; use `$` for inline Rhai (anonymous Action)".into(),
        ));
    }
    if let Some(source) = token.strip_prefix('$') {
        return Ok(Some(ExecutionUnit {
            kind: UnitKind::InlineAction,
            name: source.to_string(),
            positionals: Vec::new(),
            args: Vec::new(),
            cli_token: token.to_string(),
        }));
    }
    if let Some(name) = token.strip_prefix('@') {
        if name.is_empty() {
            return Ok(None);
        }
        return Ok(Some(ExecutionUnit {
            kind: UnitKind::Action,
            name: name.to_string(),
            positionals: Vec::new(),
            args: Vec::new(),
            cli_token: token.to_string(),
        }));
    }
    if looks_like_workflow(token) {
        return Ok(Some(ExecutionUnit {
            kind: UnitKind::Workflow,
            name: token.to_string(),
            positionals: Vec::new(),
            args: Vec::new(),
            cli_token: token.to_string(),
        }));
    }
    Ok(None)
}

/// Finalize inline Actions: `orch $ 'code'` → name empty, first positional is source.
pub fn normalize_inline_units(units: &mut [ExecutionUnit]) -> Result<(), CliError> {
    for unit in units.iter_mut() {
        if unit.kind == UnitKind::InlineAction {
            if unit.name.is_empty() {
                if unit.positionals.is_empty() {
                    return Err(CliError::Usage(
                        "inline Action `$` requires Rhai source as the next argument".into(),
                    ));
                }
                unit.name = unit.positionals.remove(0);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn parses_atomic_action_name() {
        let p = parse_argv(&s(&["@build:dev", "--target", "x86"])).unwrap();
        assert_eq!(p.units.len(), 1);
        assert_eq!(p.units[0].kind, UnitKind::Action);
        assert_eq!(p.units[0].name, "build:dev");
        assert_eq!(p.units[0].args[0].key, "target");
        assert_eq!(p.units[0].args[0].value.as_deref(), Some("x86"));
    }

    #[test]
    fn parses_global_then_unit_args() {
        let p = parse_argv(&s(&["--json", "--", "@build", "--verbose"])).unwrap();
        assert!(p.globals.json);
        assert_eq!(p.units[0].name, "build");
        assert_eq!(p.units[0].args[0].key, "verbose");
        assert!(p.units[0].args[0].value.is_none());
    }

    #[test]
    fn parses_double_colon_terminator() {
        let p = parse_argv(&s(&[
            "@build:dev",
            "--target",
            "x86",
            "::",
            "@clean",
        ]))
        .unwrap();
        assert_eq!(p.units.len(), 2);
        assert_eq!(p.units[0].name, "build:dev");
        assert_eq!(p.units[1].name, "clean");
    }

    #[test]
    fn parses_composition() {
        let p = parse_argv(&s(&["run:dev", "@clean"])).unwrap();
        assert_eq!(p.units[0].kind, UnitKind::Workflow);
        assert_eq!(p.units[0].name, "run:dev");
        assert_eq!(p.units[1].kind, UnitKind::Action);
    }

    #[test]
    fn parses_action_positionals() {
        let p = parse_argv(&s(&["@mkfs", "exfat", "4M"])).unwrap();
        assert_eq!(p.units[0].kind, UnitKind::Action);
        assert_eq!(p.units[0].positionals, vec!["exfat", "4M"]);
    }

    #[test]
    fn parses_inline_packed() {
        let p = parse_argv(&s(&["$print(\"hi\")"])).unwrap();
        assert_eq!(p.units[0].kind, UnitKind::InlineAction);
        assert_eq!(p.units[0].name, "print(\"hi\")");
    }

    #[test]
    fn parses_inline_separate() {
        let p = parse_argv(&s(&["$", "print(\"hi\")"])).unwrap();
        let mut units = p.units;
        normalize_inline_units(&mut units).unwrap();
        assert_eq!(units[0].kind, UnitKind::InlineAction);
        assert_eq!(units[0].name, "print(\"hi\")");
    }

    #[test]
    fn rejects_obsolete_double_dollar() {
        let err = parse_argv(&s(&["$$print(\"hi\")"])).unwrap_err();
        assert!(err.to_string().contains("$$"));
    }

    #[test]
    fn key_equals_value() {
        let p = parse_argv(&s(&["@build:dev", "--target=x86"])).unwrap();
        assert_eq!(p.units[0].args[0].value.as_deref(), Some("x86"));
    }

    #[test]
    fn globals_only_before_units() {
        let p = parse_argv(&s(&["--quiet", "@build", "--release"])).unwrap();
        assert!(p.globals.quiet);
        assert_eq!(p.units[0].args[0].key, "release");
    }

    #[test]
    fn bare_double_colon_terminates() {
        let p = parse_argv(&s(&["@build:dev", "arg1", "::", "@clean"])).unwrap();
        assert_eq!(p.units.len(), 2);
        assert_eq!(p.units[0].name, "build:dev");
        assert_eq!(p.units[0].positionals, vec!["arg1"]);
        assert_eq!(p.units[1].name, "clean");
    }

    #[test]
    fn literal_double_colon_after_end_of_options() {
        let p = parse_argv(&s(&["@action", "--", "::", "more"])).unwrap();
        assert_eq!(p.units.len(), 1);
        assert_eq!(p.units[0].positionals, vec!["--", "::", "more"]);
    }

    #[test]
    fn literal_double_colon_via_equals() {
        let p = parse_argv(&s(&["@build:dev", "--marker=::"])).unwrap();
        assert_eq!(p.units.len(), 1);
        assert_eq!(p.units[0].name, "build:dev");
        assert_eq!(p.units[0].args[0].key, "marker");
        assert_eq!(p.units[0].args[0].value.as_deref(), Some("::"));
    }

    #[test]
    fn action_name_with_colon_is_not_an_argument() {
        let p = parse_argv(&s(&["@build:release"])).unwrap();
        assert_eq!(p.units[0].kind, UnitKind::Action);
        assert_eq!(p.units[0].name, "build:release");
        assert!(p.units[0].positionals.is_empty());
        assert!(p.units[0].args.is_empty());
    }

    #[test]
    fn workflow_plus_action_and_action_plus_action() {
        let p = parse_argv(&s(&["run:dev", "@clean"])).unwrap();
        assert_eq!(p.units.len(), 2);
        let p = parse_argv(&s(&["@build", "@clean"])).unwrap();
        assert_eq!(p.units.len(), 2);
        assert_eq!(p.units[0].kind, UnitKind::Action);
        assert_eq!(p.units[1].kind, UnitKind::Action);
    }

    #[test]
    fn global_flags_combinations() {
        let p = parse_argv(&s(&["--quiet", "--verbose", "--json", "--no-progress", "@clean"]))
            .unwrap();
        assert!(p.globals.quiet);
        assert!(p.globals.verbose);
        assert!(p.globals.json);
        assert!(p.globals.no_progress);
        assert_eq!(p.units[0].name, "clean");
    }
}
