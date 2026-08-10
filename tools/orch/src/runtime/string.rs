//! Extra string methods not provided by Rhai's standard library.
//!
//! Rhai 1.25 (`Engine::new()` / `MoreStringPackage`) already provides idiomatic
//! methods such as `starts_with`, `ends_with`, `contains`, `index_of`, `split`,
//! `trim` (in-place), `replace` (in-place), `to_lower`, `to_upper`, `len`,
//! `is_empty`, `sub_string`, etc. Do **not** re-register those here.
//!
//! This module only adds genuinely missing automation helpers, registered as
//! methods on Rhai's string / array types.

use rhai::{Array, Dynamic, Engine, ImmutableString};

/// Register orch extensions that Rhai does not ship.
pub fn register(engine: &mut Engine) {
    // --- mutating trim edges (Rhai only has in-place `trim`) ---
    engine.register_fn("trim_start", |s: &mut ImmutableString| {
        if let Some(inner) = s.get_mut() {
            let trimmed = inner.trim_start();
            if trimmed != inner.as_str() {
                *inner = trimmed.into();
            }
        } else {
            let trimmed = s.trim_start();
            if trimmed != s.as_str() {
                *s = trimmed.into();
            }
        }
    });
    engine.register_fn("trim_end", |s: &mut ImmutableString| {
        if let Some(inner) = s.get_mut() {
            let trimmed = inner.trim_end();
            if trimmed != inner.as_str() {
                *inner = trimmed.into();
            }
        } else {
            let trimmed = s.trim_end();
            if trimmed != s.as_str() {
                *s = trimmed.into();
            }
        }
    });

    // --- strip_* : non-mutating; returns remainder or () if not present ---
    engine.register_fn("strip_prefix", |s: &str, prefix: &str| -> Dynamic {
        match s.strip_prefix(prefix) {
            Some(rest) => Dynamic::from(rest.to_string()),
            None => Dynamic::UNIT,
        }
    });
    engine.register_fn("strip_suffix", |s: &str, suffix: &str| -> Dynamic {
        match s.strip_suffix(suffix) {
            Some(rest) => Dynamic::from(rest.to_string()),
            None => Dynamic::UNIT,
        }
    });

    // --- predicates ---
    engine.register_fn("is_ascii", |s: &str| s.is_ascii());

    // --- lines: split on `\n`, strip trailing `\r` (Windows-friendly) ---
    engine.register_fn("lines", |s: &str| -> Array {
        s.lines()
            .map(|line| Dynamic::from(line.to_string()))
            .collect()
    });

    // --- Array.join(separator) — not in Rhai stdlib ---
    engine.register_fn("join", |arr: Array, sep: &str| -> String {
        arr.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(sep)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhai::Scope;

    fn engine() -> Engine {
        let mut eng = Engine::new();
        register(&mut eng);
        eng
    }

    #[test]
    fn registers_trim_edges_and_strip() {
        let eng = engine();
        let mut scope = Scope::new();
        let v: bool = eng
            .eval_with_scope(
                &mut scope,
                r#"
                let s = "  ab  ";
                s.trim_start();
                s.trim_end();
                s == "ab"
                "#,
            )
            .unwrap();
        assert!(v);

        let v: bool = eng
            .eval_with_scope(
                &mut scope,
                r#"
                let rest = "foo:bar".strip_prefix("foo:");
                rest == "bar" && "foo".strip_prefix("x") == ()
                "#,
            )
            .unwrap();
        assert!(v);
    }

    #[test]
    fn lines_and_join_and_is_ascii() {
        let eng = engine();
        let mut scope = Scope::new();
        let v: bool = eng
            .eval_with_scope(
                &mut scope,
                r#"
                let ls = "a\r\nb\nc".lines();
                ls.len() == 3 && ls[0] == "a" && ls[1] == "b" && ls[2] == "c"
                    && ["x","y"].join(":") == "x:y"
                    && "abc".is_ascii()
                    && !"Привет".is_ascii()
                "#,
            )
            .unwrap();
        assert!(v);
    }

    #[test]
    fn rhai_builtins_still_work() {
        let eng = engine();
        let mut scope = Scope::new();
        let v: bool = eng
            .eval_with_scope(
                &mut scope,
                r#"
                let path = "example/document.md";
                path.starts_with("example")
                    && path.ends_with(".md")
                    && path.contains("document")
                    && "one:two:three".split(":").len() == 3
                    && "Привет".len() == 6
                    && "🙂".len() == 1
                "#,
            )
            .unwrap();
        assert!(v);
    }
}
