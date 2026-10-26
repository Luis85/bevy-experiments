//! Flat, serializable tree IR shared by all agents running the same tree.
//!
//! Trees reference actions, conditions and scorers **by name** ([`NameRef`]).
//! The ids are resolved once, when the [`AiRegistry`](crate::AiRegistry) is
//! frozen (ids are assigned sorted by name), so registration order never
//! changes a tree, its content hash or a replay.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Index into `AiRegistry` trees (frozen sorted by tree name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TreeId(pub u16);

/// Index into `AiRegistry` actions (frozen sorted by action name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActionId(pub u16);

/// Index into `AiRegistry` conditions (frozen sorted by condition name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CondId(pub u16);

/// Index into `AiRegistry` utility scorers (frozen sorted by scorer name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ScoreId(pub u16);

/// Index of a node inside its `TreeDef::nodes`. Root is always 0.
pub type NodeIdx = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Running,
    Success,
    Failure,
}

/// What a passing guard may interrupt while a sibling branch is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Abort {
    /// Only evaluated when the parent selector reaches it normally.
    None,
    /// When re-checked on a thinking agent and passing, aborts a running
    /// lower-priority (later) sibling branch of the parent selector.
    LowerPriority,
}

/// A typed leaf parameter, interpreted by the action that reads it.
/// JSON: `"None"`, `{"Int":5}`, `{"Pos":[x,z]}`, `{"Name":"Morning"}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParamValue {
    #[default]
    None,
    Int(i32),
    /// A position in sim units (e.g. millimetres).
    Pos(i32, i32),
    /// A symbolic option, e.g. `"Morning"`.
    Name(String),
}

impl ParamValue {
    /// The integer, if this is an `Int`.
    pub fn as_int(&self) -> Option<i32> {
        match *self {
            ParamValue::Int(v) => Some(v),
            _ => None,
        }
    }

    /// The name, if this is a `Name`.
    pub fn as_name(&self) -> Option<&str> {
        match self {
            ParamValue::Name(n) => Some(n),
            _ => None,
        }
    }

    /// The position, if this is a `Pos`.
    pub fn as_pos(&self) -> Option<(i32, i32)> {
        match *self {
            ParamValue::Pos(x, z) => Some((x, z)),
            _ => None,
        }
    }

    fn hash_into(&self, h: &mut Fnv1a) {
        match self {
            ParamValue::None => h.u8(0),
            ParamValue::Int(v) => {
                h.u8(1);
                h.bytes(&v.to_le_bytes());
            }
            ParamValue::Pos(x, z) => {
                h.u8(2);
                h.bytes(&x.to_le_bytes());
                h.bytes(&z.to_le_bytes());
            }
            ParamValue::Name(n) => {
                h.u8(3);
                h.str(n);
            }
        }
    }
}

impl From<i32> for ParamValue {
    fn from(v: i32) -> Self {
        ParamValue::Int(v)
    }
}

impl From<(i32, i32)> for ParamValue {
    fn from((x, z): (i32, i32)) -> Self {
        ParamValue::Pos(x, z)
    }
}

impl From<&str> for ParamValue {
    fn from(name: &str) -> Self {
        ParamValue::Name(name.to_owned())
    }
}

impl From<()> for ParamValue {
    fn from((): ()) -> Self {
        ParamValue::None
    }
}

/// A reference to a registered action, condition or scorer **by name**.
/// `id` is filled in when the registry is frozen and is never serialized
/// (JSON: just the name).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NameRef<I> {
    pub name: String,
    pub id: Option<I>,
}

impl<I> NameRef<I> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: None,
        }
    }
}

impl<I: Copy> NameRef<I> {
    /// The resolved id.
    ///
    /// # Panics
    /// When the registry owning the tree is not frozen yet.
    pub fn id(&self) -> I {
        self.id.unwrap_or_else(|| {
            panic!(
                "`{}` is not resolved: freeze the AiRegistry before running trees \
                 (NpcPlugins does it in Plugin::finish)",
                self.name
            )
        })
    }
}

impl<I> From<&str> for NameRef<I> {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl<I: fmt::Debug> fmt::Debug for NameRef<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.id {
            Some(id) => write!(f, "{:?}@{id:?}", self.name),
            None => write!(f, "{:?}", self.name),
        }
    }
}

impl<I> Serialize for NameRef<I> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.name)
    }
}

impl<'de, I> Deserialize<'de> for NameRef<I> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Self::new)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// Priority fallback: first child that does not fail wins.
    Selector,
    /// Runs children in order; fails on first failure.
    Sequence,
    /// Tries children in descending score order (ties: lower index first).
    /// `scorers[i]` scores `children[i]`.
    UtilitySelector { scorers: Vec<NameRef<ScoreId>> },
    /// Exactly one child. Fails without running the child when `cond` is false.
    Guard { cond: NameRef<CondId>, abort: Abort },
    /// Starts a registered action with a typed parameter.
    Leaf {
        action: NameRef<ActionId>,
        param: ParamValue,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeDef {
    /// Human-readable label shown in debug paths.
    pub name: String,
    pub kind: NodeKind,
    pub children: Vec<NodeIdx>,
}

/// A complete tree. `nodes[0]` is the root; nodes are in pre-order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeDef {
    pub name: String,
    pub nodes: Vec<NodeDef>,
}

impl TreeDef {
    /// Platform-independent FNV-1a 64 hash of the tree structure (node names,
    /// kinds, the *names* of the referenced actions/conditions/scorers,
    /// params, children). Equal trees hash equal on every machine and in
    /// every registration order (ids are never hashed).
    pub fn content_hash(&self) -> u64 {
        let mut h = Fnv1a::new();
        h.str(&self.name);
        h.u32(self.nodes.len() as u32);
        for node in &self.nodes {
            h.str(&node.name);
            match &node.kind {
                NodeKind::Selector => h.u8(0),
                NodeKind::Sequence => h.u8(1),
                NodeKind::UtilitySelector { scorers } => {
                    h.u8(2);
                    h.u32(scorers.len() as u32);
                    for s in scorers {
                        h.str(&s.name);
                    }
                }
                NodeKind::Guard { cond, abort } => {
                    h.u8(3);
                    h.str(&cond.name);
                    h.u8(match abort {
                        Abort::None => 0,
                        Abort::LowerPriority => 1,
                    });
                }
                NodeKind::Leaf { action, param } => {
                    h.u8(4);
                    h.str(&action.name);
                    param.hash_into(&mut h);
                }
            }
            h.u32(node.children.len() as u32);
            for &c in &node.children {
                h.u16(c);
            }
        }
        h.0
    }

    /// Index one past the last node of `node`'s subtree (pre-order makes a
    /// subtree the contiguous range `node..subtree_end(node)`).
    pub fn subtree_end(&self, node: NodeIdx) -> usize {
        let mut n = node as usize;
        while let Some(&last) = self.nodes[n].children.last() {
            n = last as usize;
        }
        n + 1
    }

    /// `Root > Child > Leaf` style path of node names.
    pub fn path_string(&self, path: &[NodeIdx]) -> String {
        path.iter()
            .map(|&i| self.nodes[i as usize].name.as_str())
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// FNV-1a 64 over an explicit little-endian, length-prefixed byte encoding.
struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }
    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.bytes(s.as_bytes());
    }
}
