//! Strict registry for Actions and Workflows.
//!
//! Actions and Workflows occupy separate namespaces by design:
//! `@build` (Action) and `build` (Workflow) are distinct entities.

mod discovery;

pub use discovery::{discover_all, logical_name_from_rel, EntitySource, RegisteredEntity};

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityType {
    Action,
    Workflow,
}

impl EntityType {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Action => "@",
            Self::Workflow => "",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Workflow => "workflow",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub entity_type: EntityType,
    pub source: EntitySource,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Registry {
    actions: BTreeMap<String, RegistryEntry>,
    workflows: BTreeMap<String, RegistryEntry>,
}

#[derive(Debug, Clone)]
pub struct DuplicateError {
    pub entity_type: EntityType,
    pub name: String,
    pub first: EntitySource,
    pub second: EntitySource,
}

impl fmt::Display for DuplicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display = format!("{}{}", self.entity_type.prefix(), self.name);
        writeln!(
            f,
            "error: duplicate {} '{display}'",
            self.entity_type.label()
        )?;
        writeln!(f)?;
        writeln!(f, "first definition:")?;
        writeln!(f, "  {}", self.first.display())?;
        writeln!(f)?;
        writeln!(f, "second definition:")?;
        writeln!(f, "  {}", self.second.display())?;
        writeln!(f)?;
        write!(f, "overriding registrations is not allowed")
    }
}

impl std::error::Error for DuplicateError {}

#[derive(Debug, Clone)]
pub struct NotFoundError {
    pub entity_type: EntityType,
    pub name: String,
    pub cli_token: Option<String>,
}

impl fmt::Display for NotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display = format!("{}{}", self.entity_type.prefix(), self.name);
        match self.entity_type {
            EntityType::Action => write!(f, "Action not found: {display}")?,
            EntityType::Workflow => write!(f, "Workflow not found: {display}")?,
        }
        if let Some(tok) = &self.cli_token {
            write!(f, " (CLI token: {tok})")?;
        }
        Ok(())
    }
}

impl std::error::Error for NotFoundError {}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, entry: RegistryEntry) -> Result<(), DuplicateError> {
        let map = match entry.entity_type {
            EntityType::Action => &mut self.actions,
            EntityType::Workflow => &mut self.workflows,
        };
        if let Some(existing) = map.get(&entry.name) {
            return Err(DuplicateError {
                entity_type: entry.entity_type,
                name: entry.name,
                first: existing.source.clone(),
                second: entry.source,
            });
        }
        map.insert(entry.name.clone(), entry);
        Ok(())
    }

    pub fn get(&self, ty: EntityType, name: &str) -> Option<&RegistryEntry> {
        match ty {
            EntityType::Action => self.actions.get(name),
            EntityType::Workflow => self.workflows.get(name),
        }
    }

    pub fn require(
        &self,
        ty: EntityType,
        name: &str,
        cli_token: Option<&str>,
    ) -> Result<&RegistryEntry, NotFoundError> {
        self.get(ty, name).ok_or_else(|| NotFoundError {
            entity_type: ty,
            name: name.to_string(),
            cli_token: cli_token.map(|s| s.to_string()),
        })
    }

    pub fn actions(&self) -> impl Iterator<Item = &RegistryEntry> {
        self.actions.values()
    }

    pub fn workflows(&self) -> impl Iterator<Item = &RegistryEntry> {
        self.workflows.values()
    }

    pub fn path_for(&self, ty: EntityType, name: &str) -> Option<PathBuf> {
        self.get(ty, name).and_then(|e| match &e.source {
            EntitySource::File { path, .. } => Some(path.clone()),
            EntitySource::Inline { .. } | EntitySource::Builtin { .. } => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_fatal() {
        let mut reg = Registry::new();
        reg.register(RegistryEntry {
            name: "build".into(),
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
                name: "build".into(),
                entity_type: EntityType::Action,
                source: EntitySource::File {
                    path: PathBuf::from("b.rhai"),
                    line: 1,
                },
                metadata: BTreeMap::new(),
            })
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("duplicate action '@build'"));
        assert!(msg.contains("a.rhai"));
        assert!(msg.contains("b.rhai"));
    }

    #[test]
    fn not_found_messages() {
        let err = NotFoundError {
            entity_type: EntityType::Action,
            name: "does_not_exist".into(),
            cli_token: Some("@does_not_exist".into()),
        };
        assert_eq!(
            err.to_string(),
            "Action not found: @does_not_exist (CLI token: @does_not_exist)"
        );
        let err = NotFoundError {
            entity_type: EntityType::Workflow,
            name: "nonexistent_workflow".into(),
            cli_token: None,
        };
        assert_eq!(err.to_string(), "Workflow not found: nonexistent_workflow");
    }
}
