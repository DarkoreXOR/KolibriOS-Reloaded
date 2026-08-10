//! Generic TOML loading for Scripts (project-agnostic config files).

use rhai::{Dynamic, Map};
use std::fs;
use std::path::Path;

pub fn load(path: &Path) -> Result<Dynamic, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    Ok(toml_to_dynamic(&value))
}

fn toml_to_dynamic(value: &toml::Value) -> Dynamic {
    match value {
        toml::Value::String(s) => Dynamic::from(s.clone()),
        toml::Value::Integer(i) => Dynamic::from(*i),
        toml::Value::Float(f) => Dynamic::from(*f),
        toml::Value::Boolean(b) => Dynamic::from(*b),
        toml::Value::Datetime(d) => Dynamic::from(d.to_string()),
        toml::Value::Array(items) => {
            let arr: rhai::Array = items.iter().map(toml_to_dynamic).collect();
            Dynamic::from(arr)
        }
        toml::Value::Table(table) => {
            let mut map = Map::new();
            for (k, v) in table {
                map.insert(k.clone().into(), toml_to_dynamic(v));
            }
            Dynamic::from(map)
        }
    }
}
