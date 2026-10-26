//! Registry of trees, actions, conditions and scorers.
//!
//! Content registers everything **by name**; nothing gets an id until the
//! registry is [frozen](AiRegistry::freeze). Freezing sorts each kind by name,
//! assigns dense ids in that order and resolves every [`NameRef`] in every
//! tree, so ids (and the state hash, which contains them) never depend on the
//! order plugins registered things in. A tree that names an unknown action,
//! condition or scorer panics at freeze time with the list of known names.

use std::collections::BTreeSet;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::tree::{
    ActionId, CondId, NameRef, NodeKind, ParamValue, ScoreId, Status, TreeDef, TreeId,
};

/// Starts an action on `agent` with the leaf's typed `param`. Returns
/// `Running` after inserting its action component, or `Success`/`Failure`
/// immediately.
pub type StartFn = fn(&mut World, Entity, &ParamValue) -> Status;
/// Cleans up an action on `agent`: removes its component and releases its
/// reservations. Called with `Finished` when the think pass consumes a leaf's
/// Success/Failure, and with `Aborted` when its branch is interrupted (even if
/// the leaf had already completed but its result was not consumed yet).
/// Must be idempotent.
pub type StopFn = fn(&mut World, Entity, StopReason);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StopReason {
    Finished,
    Aborted,
}
pub type CondFn = fn(&World, Entity) -> bool;
pub type ScoreFn = fn(&World, Entity) -> i32;

#[derive(Clone, Copy)]
pub struct ActionFns {
    pub name: &'static str,
    pub start: StartFn,
    pub stop: StopFn,
}

/// All behaviour content. Before [`freeze`](Self::freeze) the vectors hold
/// registrations in arrival order; afterwards each is sorted by name and the
/// index is the id.
#[derive(Resource, Default)]
pub struct AiRegistry {
    pub trees: Vec<TreeDef>,
    pub actions: Vec<ActionFns>,
    pub conds: Vec<(&'static str, CondFn)>,
    pub scorers: Vec<(&'static str, ScoreFn)>,
    frozen: bool,
}

/// `u16` id for the `index`-th entry of `kind`, or a clear panic.
fn checked_id(index: usize, kind: &str) -> u16 {
    u16::try_from(index).unwrap_or_else(|_| {
        panic!(
            "too many registered {kind}s: id {index} does not fit in u16 (max {})",
            u16::MAX
        )
    })
}

fn names<'a>(it: impl Iterator<Item = &'a str>) -> String {
    let mut v: Vec<&str> = it.collect();
    v.sort_unstable();
    format!("[{}]", v.join(", "))
}

impl AiRegistry {
    fn assert_open(&self, what: &str, name: &str) {
        assert!(
            !self.frozen,
            "cannot register {what} `{name}`: the AiRegistry is already frozen \
             (register content in Plugin::build, before the app finishes)"
        );
    }

    /// Registers action `name`. Panics after freeze (a duplicate name panics at freeze).
    pub fn add_action(&mut self, name: &'static str, start: StartFn, stop: StopFn) {
        self.assert_open("action", name);
        self.actions.push(ActionFns { name, start, stop });
    }

    /// Registers condition `name`. Panics after freeze (a duplicate name panics at freeze).
    pub fn add_cond(&mut self, name: &'static str, f: CondFn) {
        self.assert_open("condition", name);
        self.conds.push((name, f));
    }

    /// Registers scorer `name`. Panics after freeze (a duplicate name panics at freeze).
    pub fn add_scorer(&mut self, name: &'static str, f: ScoreFn) {
        self.assert_open("scorer", name);
        self.scorers.push((name, f));
    }

    /// Registers a tree under its `TreeDef::name`. Its references are resolved
    /// at freeze time. Panics after freeze or on a duplicate tree name.
    pub fn add_tree(&mut self, tree: TreeDef) {
        self.assert_open("tree", &tree.name);
        assert!(
            !self.trees.iter().any(|t| t.name == tree.name),
            "tree `{}` is registered twice",
            tree.name
        );
        self.trees.push(tree);
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Sorts actions, conditions, scorers and trees by name, assigns ids in
    /// that order and resolves every tree reference. Idempotent.
    ///
    /// # Panics
    /// When a tree references an unknown name (the message lists the known
    /// names of that kind), or when a kind has more than `u16::MAX + 1` entries.
    pub fn freeze(&mut self) {
        if self.frozen {
            return;
        }
        self.actions.sort_by_key(|a| a.name);
        self.conds.sort_by_key(|c| c.0);
        self.scorers.sort_by_key(|s| s.0);
        self.trees.sort_by(|a, b| a.name.cmp(&b.name));
        dup_check(self.actions.iter().map(|a| a.name), "action");
        dup_check(self.conds.iter().map(|c| c.0), "condition");
        dup_check(self.scorers.iter().map(|s| s.0), "scorer");
        checked_id(self.actions.len().saturating_sub(1), "action");
        checked_id(self.conds.len().saturating_sub(1), "condition");
        checked_id(self.scorers.len().saturating_sub(1), "scorer");
        checked_id(self.trees.len().saturating_sub(1), "tree");

        let actions: Vec<&'static str> = self.actions.iter().map(|a| a.name).collect();
        let conds: Vec<&'static str> = self.conds.iter().map(|c| c.0).collect();
        let scorers: Vec<&'static str> = self.scorers.iter().map(|s| s.0).collect();
        for tree in &mut self.trees {
            let tree_name = tree.name.clone();
            for node in &mut tree.nodes {
                let node_name = node.name.clone();
                let ctx = |kind: &str, name: &str, known: &[&'static str]| -> String {
                    format!(
                        "tree `{tree_name}` node `{node_name}` references unknown {kind} `{name}`; \
                         known {kind}s: {}",
                        names(known.iter().copied())
                    )
                };
                match &mut node.kind {
                    NodeKind::Leaf { action, .. } => {
                        resolve(action, &actions, |n| ctx("action", n, &actions), ActionId)
                    }
                    NodeKind::Guard { cond, .. } => {
                        resolve(cond, &conds, |n| ctx("condition", n, &conds), CondId)
                    }
                    NodeKind::UtilitySelector { scorers: refs } => {
                        for s in refs {
                            resolve(s, &scorers, |n| ctx("scorer", n, &scorers), ScoreId);
                        }
                    }
                    NodeKind::Selector | NodeKind::Sequence => {}
                }
            }
        }
        self.frozen = true;
    }

    /// Every registered name per kind (`"action"`, `"cond"`, `"scorer"`,
    /// `"tree"`), sorted: what the content hash covers.
    pub fn manifest(&self) -> Vec<(&'static str, String)> {
        let mut out: BTreeSet<(&'static str, String)> = BTreeSet::new();
        out.extend(self.actions.iter().map(|a| ("action", a.name.to_owned())));
        out.extend(self.conds.iter().map(|c| ("cond", c.0.to_owned())));
        out.extend(self.scorers.iter().map(|s| ("scorer", s.0.to_owned())));
        out.extend(self.trees.iter().map(|t| ("tree", t.name.clone())));
        out.into_iter().collect()
    }

    /// Id of tree `name` (frozen registries only).
    pub fn tree_id(&self, name: &str) -> Option<TreeId> {
        self.assert_frozen();
        self.trees
            .iter()
            .position(|t| t.name == name)
            .map(|i| TreeId(checked_id(i, "tree")))
    }

    /// Id of action `name` (frozen registries only).
    pub fn action_id(&self, name: &str) -> Option<ActionId> {
        self.assert_frozen();
        self.actions
            .iter()
            .position(|a| a.name == name)
            .map(|i| ActionId(checked_id(i, "action")))
    }

    /// Id of condition `name` (frozen registries only).
    pub fn cond_id(&self, name: &str) -> Option<CondId> {
        self.assert_frozen();
        self.conds
            .iter()
            .position(|c| c.0 == name)
            .map(|i| CondId(checked_id(i, "condition")))
    }

    fn assert_frozen(&self) {
        assert!(
            self.frozen,
            "AiRegistry ids exist only after freeze (NpcPlugins freezes in Plugin::finish)"
        );
    }

    pub fn tree(&self, id: TreeId) -> &TreeDef {
        &self.trees[id.0 as usize]
    }

    pub fn action(&self, id: ActionId) -> &ActionFns {
        &self.actions[id.0 as usize]
    }
}

/// Panics on the first name that appears twice in the sorted `names`.
fn dup_check<'a>(names: impl Iterator<Item = &'a str>, kind: &str) {
    let mut prev: Option<&str> = None;
    for name in names {
        assert!(prev != Some(name), "{kind} `{name}` is registered twice");
        prev = Some(name);
    }
}

fn resolve<I: Copy>(
    r: &mut NameRef<I>,
    known: &[&'static str],
    message: impl Fn(&str) -> String,
    make: fn(u16) -> I,
) {
    match known.binary_search(&r.name.as_str()) {
        Ok(i) => r.id = Some(make(checked_id(i, "entry"))),
        Err(_) => panic!("{}", message(&r.name)),
    }
}
