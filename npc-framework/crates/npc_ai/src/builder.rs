//! Code front-end that emits the flat [`TreeDef`] IR.
//!
//! Leaves, guards and utility selectors name their action, condition and
//! scorers; the names are resolved when the registry is frozen.
//!
//! ```ignore
//! let mut b = TreeBuilder::new("villager");
//! b.selector("Root", |b| {
//!     b.guard("UrgentEat", "hungry", Abort::LowerPriority, |b| {
//!         b.sequence("EatSeq", |b| {
//!             b.leaf("ReserveFood", "ReserveFood", ());
//!             b.leaf("Eat", "EatFood", ());
//!         });
//!     });
//!     b.leaf("Wander", "Wander", 200);
//! });
//! let tree: TreeDef = b.build();
//! ```

use crate::tree::{Abort, NameRef, NodeDef, NodeIdx, NodeKind, ParamValue, TreeDef};

pub struct TreeBuilder {
    tree: TreeDef,
    /// Composite nodes whose children are currently being added.
    open: Vec<NodeIdx>,
    /// Kind of the next node `push_node` creates.
    pending: Option<(String, NodeKind)>,
    roots: usize,
}

impl TreeBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            tree: TreeDef {
                name: name.to_owned(),
                nodes: Vec::new(),
            },
            open: Vec::new(),
            pending: None,
            roots: 0,
        }
    }

    pub fn selector(&mut self, name: &str, children: impl FnOnce(&mut Self)) -> &mut Self {
        self.composite(name, NodeKind::Selector, children)
    }

    pub fn sequence(&mut self, name: &str, children: impl FnOnce(&mut Self)) -> &mut Self {
        self.composite(name, NodeKind::Sequence, children)
    }

    /// `children` must add exactly one child per entry of `scorers` (scorer
    /// names), in order.
    pub fn utility<'a>(
        &mut self,
        name: &str,
        scorers: impl IntoIterator<Item = &'a str>,
        children: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let scorers = scorers.into_iter().map(NameRef::new).collect();
        self.composite(name, NodeKind::UtilitySelector { scorers }, children)
    }

    /// Guard on condition `cond` (a registered condition name). `child`
    /// must add exactly one node.
    pub fn guard(
        &mut self,
        name: &str,
        cond: &str,
        abort: Abort,
        child: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let cond = NameRef::new(cond);
        self.composite(name, NodeKind::Guard { cond, abort }, child)
    }

    /// Leaf running action `action` (a registered action name) with `param`
    /// (`()`, an `i32`, an `(x, z)` pair, a `&str` name or a [`ParamValue`]).
    pub fn leaf(&mut self, name: &str, action: &str, param: impl Into<ParamValue>) -> &mut Self {
        let kind = NodeKind::Leaf {
            action: NameRef::new(action),
            param: param.into(),
        };
        self.pending = Some((name.to_owned(), kind));
        self.push_node();
        self
    }

    /// Panics if the tree is empty, a guard does not have exactly one child,
    /// or a utility selector's child count differs from its scorer count.
    pub fn build(self) -> TreeDef {
        assert!(
            !self.tree.nodes.is_empty(),
            "tree `{}` is empty",
            self.tree.name
        );
        assert_eq!(
            self.roots, 1,
            "tree `{}` must have exactly one root node",
            self.tree.name
        );
        for node in &self.tree.nodes {
            match &node.kind {
                NodeKind::Guard { .. } => assert_eq!(
                    node.children.len(),
                    1,
                    "guard `{}` in tree `{}` must have exactly one child",
                    node.name,
                    self.tree.name
                ),
                NodeKind::UtilitySelector { scorers } => assert_eq!(
                    node.children.len(),
                    scorers.len(),
                    "utility `{}` in tree `{}` needs one child per scorer",
                    node.name,
                    self.tree.name
                ),
                _ => {}
            }
        }
        self.tree
    }

    fn composite(
        &mut self,
        name: &str,
        kind: NodeKind,
        children: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.pending = Some((name.to_owned(), kind));
        let idx = self.push_node();
        self.open.push(idx);
        children(self);
        self.open.pop();
        self
    }

    /// Appends the pending node in pre-order and links it to the innermost
    /// open parent (or makes it the root).
    fn push_node(&mut self) -> NodeIdx {
        let (name, kind) = self
            .pending
            .take()
            .expect("push_node without a pending node");
        let idx =
            NodeIdx::try_from(self.tree.nodes.len()).expect("tree exceeds NodeIdx::MAX nodes");
        self.tree.nodes.push(NodeDef {
            name,
            kind,
            children: Vec::new(),
        });
        match self.open.last() {
            Some(&parent) => self.tree.nodes[parent as usize].children.push(idx),
            None => self.roots += 1,
        }
        idx
    }
}
