//! Target components: [`Interactable`] (what a target advertises) and
//! [`ToolTarget`] (what `use_tool_on` works on).

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::defs::SkillCheck;

/// The verbs a target advertises (sorted, unique). Built-in verbs work on
/// targets without it (an item can be picked up, a door opened); when a
/// target has it, **only** the listed verbs apply to it. Content verbs
/// usually require it in their `offer` ([`Interactable::offers`]).
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Interactable {
    pub verbs: Vec<String>,
}

impl Interactable {
    pub fn new<S: Into<String>>(verbs: impl IntoIterator<Item = S>) -> Self {
        let mut v: Vec<String> = verbs.into_iter().map(Into::into).collect();
        v.sort_unstable();
        v.dedup();
        Self { verbs: v }
    }

    pub fn offers(&self, verb: &str) -> bool {
        self.verbs
            .binary_search_by(|v| v.as_str().cmp(verb))
            .is_ok()
    }
}

/// Items a successful `use_tool_on` produces (spawned at the actor's feet).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct ItemYield {
    pub kind: String,
    pub stack: u32,
    pub quality: i32,
}

impl ItemYield {
    pub fn new(kind: impl Into<String>, stack: u32) -> Self {
        Self {
            kind: kind.into(),
            stack: stack.max(1),
            quality: 0,
        }
    }
}

/// Something worked with a tool (a tree chopped with an axe): the actor
/// needs an equipped item tagged `required_tag`; the work takes
/// `duration_ticks`, may need a skill check, yields items and uses up one
/// of `remaining` uses (`None`: unlimited; at 0 it is depleted).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ToolTarget {
    pub required_tag: String,
    pub yields: Vec<ItemYield>,
    pub duration_ticks: u32,
    pub check: Option<SkillCheck>,
    pub remaining: Option<u32>,
}

impl ToolTarget {
    /// Needs `required_tag`, instant, no yields, unlimited.
    pub fn new(required_tag: impl Into<String>) -> Self {
        Self {
            required_tag: required_tag.into(),
            yields: Vec::new(),
            duration_ticks: 0,
            check: None,
            remaining: None,
        }
    }

    pub fn yields(mut self, y: ItemYield) -> Self {
        self.yields.push(y);
        self
    }

    pub fn duration(mut self, ticks: u32) -> Self {
        self.duration_ticks = ticks;
        self
    }

    pub fn check(mut self, skill: impl Into<String>, modifier: i32) -> Self {
        self.check = Some(SkillCheck::new(skill, modifier));
        self
    }

    pub fn uses(mut self, n: u32) -> Self {
        self.remaining = Some(n);
        self
    }

    pub fn depleted(&self) -> bool {
        self.remaining == Some(0)
    }
}
