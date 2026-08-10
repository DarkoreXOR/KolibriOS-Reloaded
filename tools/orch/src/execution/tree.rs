//! Execution tree: authoritative hierarchy, state, and identity.

use super::events::ExecutionState;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExecId(pub Uuid);

impl ExecId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ExecId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Root,
    Workflow,
    Action,
    /// Anonymous `$` Action.
    InlineAction,
    Process,
    Rollback,
}

#[derive(Debug, Clone)]
pub struct ExecutionNode {
    pub id: ExecId,
    pub parent: Option<ExecId>,
    pub kind: NodeKind,
    pub name: String,
    pub state: ExecutionState,
    pub depth: usize,
    /// Logical identity for deduplication (kind + name + args).
    pub logical_key: String,
    pub source: Option<String>,
    pub children: Vec<ExecId>,
}

#[derive(Debug, Default)]
pub struct ExecutionTree {
    nodes: HashMap<ExecId, ExecutionNode>,
    root: Option<ExecId>,
    /// First-seen logical keys → node id (graph-wide identity dedup).
    seen_keys: HashMap<String, ExecId>,
}

impl ExecutionTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ensure_root(&mut self) -> ExecId {
        if let Some(id) = self.root {
            return id;
        }
        let id = ExecId::new();
        self.nodes.insert(
            id,
            ExecutionNode {
                id,
                parent: None,
                kind: NodeKind::Root,
                name: "root".into(),
                state: ExecutionState::Running,
                depth: 0,
                logical_key: "root".into(),
                source: None,
                children: Vec::new(),
            },
        );
        self.root = Some(id);
        id
    }

    pub fn add_child(
        &mut self,
        parent: ExecId,
        kind: NodeKind,
        name: impl Into<String>,
        logical_key: impl Into<String>,
        source: Option<String>,
    ) -> ExecId {
        let name = name.into();
        let logical_key = logical_key.into();
        let depth = self.nodes.get(&parent).map(|n| n.depth + 1).unwrap_or(1);
        let id = ExecId::new();
        self.nodes.insert(
            id,
            ExecutionNode {
                id,
                parent: Some(parent),
                kind,
                name,
                state: ExecutionState::Pending,
                depth,
                logical_key: logical_key.clone(),
                source,
                children: Vec::new(),
            },
        );
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.push(id);
        }
        id
    }

    /// Claim a logical key for first execution. Returns `None` if already claimed
    /// (caller should skip). Returns `Some(id)` of the existing node when duplicate.
    pub fn claim_or_existing(&mut self, logical_key: &str) -> ClaimResult {
        if let Some(id) = self.seen_keys.get(logical_key).copied() {
            ClaimResult::AlreadySeen(id)
        } else {
            ClaimResult::First
        }
    }

    pub fn mark_seen(&mut self, logical_key: String, id: ExecId) {
        self.seen_keys.entry(logical_key).or_insert(id);
    }

    pub fn set_state(&mut self, id: ExecId, state: ExecutionState) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.state = state;
        }
    }

    pub fn get(&self, id: ExecId) -> Option<&ExecutionNode> {
        self.nodes.get(&id)
    }

    pub fn depth(&self, id: ExecId) -> usize {
        self.nodes.get(&id).map(|n| n.depth).unwrap_or(0)
    }

    pub fn root_id(&self) -> Option<ExecId> {
        self.root
    }

    pub fn children(&self, id: ExecId) -> Vec<ExecId> {
        self.nodes
            .get(&id)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimResult {
    First,
    AlreadySeen(ExecId),
}
