//! Recipes ([`RecipeDef`], registered by stable name), their validation,
//! the tuning and the completion hooks.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_interaction::InteractionRegistry;
use npc_kernel::SimId;
use npc_rules::RulesRegistry;
use serde::{Deserialize, Serialize};

/// `count` units of items matching `key` (an item kind name or a tag, e.g.
/// `"wood"`, `"metal:iron"`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipeInput {
    pub key: String,
    pub count: u32,
}

/// `count` units of registered kind `kind`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipeOutput {
    pub kind: String,
    pub count: u32,
}

/// A recipe, built with [`RecipeDef::new`] and the builder methods.
///
/// - `inputs` come from the crafter's **inventory** (not its equipment) and
///   are consumed when the craft succeeds or fails critically.
/// - `tools` (kind names or tags) must be **carried**, in the inventory or
///   equipped (an equipped match is preferred); they are not consumed but
///   wear ([`CraftingConfig`]).
/// - `stations`: the workstation kinds that can host it
///   ([`Workstation`](crate::Workstation)); empty = crafted anywhere, without a workstation.
/// - `skill` is rolled with `modifier` (plus the tools' quality) when the
///   duration is over; no skill = no roll (always a margin-0 success).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipeDef {
    pub name: String,
    pub inputs: Vec<RecipeInput>,
    pub tools: Vec<String>,
    pub stations: Vec<String>,
    pub skill: Option<String>,
    pub modifier: i32,
    pub duration_ticks: u32,
    pub outputs: Vec<RecipeOutput>,
    /// XP a completion is worth (reported in [`CraftCompleted`]; the
    /// progression glue awards it).
    pub xp: u32,
}

impl RecipeDef {
    /// No inputs, tools, station or skill; instant; no outputs (add one);
    /// 10 XP.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            tools: Vec::new(),
            stations: Vec::new(),
            skill: None,
            modifier: 0,
            duration_ticks: 0,
            outputs: Vec::new(),
            xp: 10,
        }
    }

    pub fn input(mut self, key: impl Into<String>, count: u32) -> Self {
        self.inputs.push(RecipeInput {
            key: key.into(),
            count,
        });
        self
    }

    pub fn tool(mut self, key: impl Into<String>) -> Self {
        self.tools.push(key.into());
        self
    }

    pub fn station(mut self, kind: impl Into<String>) -> Self {
        let kind = kind.into();
        if let Err(i) = self.stations.binary_search(&kind) {
            self.stations.insert(i, kind);
        }
        self
    }

    pub fn skill(mut self, skill: impl Into<String>, modifier: i32) -> Self {
        self.skill = Some(skill.into());
        self.modifier = modifier;
        self
    }

    pub fn duration(mut self, ticks: u32) -> Self {
        self.duration_ticks = ticks;
        self
    }

    pub fn output(mut self, kind: impl Into<String>, count: u32) -> Self {
        self.outputs.push(RecipeOutput {
            kind: kind.into(),
            count,
        });
        self
    }

    pub fn xp(mut self, xp: u32) -> Self {
        self.xp = xp;
        self
    }

    /// Whether a workstation of `kinds` can host it.
    pub fn hosted_by(&self, kinds: &[String]) -> bool {
        self.stations.iter().any(|s| kinds.binary_search(s).is_ok())
    }
}

/// Checks a recipe against the registered item kinds and skills: a name,
/// at least one output, positive counts, outputs that are registered kinds,
/// input and tool keys that match a registered kind (name or tag), a
/// registered skill and non-empty station names. `Err` says what is wrong.
pub fn validate_recipe(
    def: &RecipeDef,
    items: &InteractionRegistry,
    rules: &RulesRegistry,
) -> Result<(), String> {
    let r = &def.name;
    if r.is_empty() {
        return Err("a recipe needs a name".into());
    }
    if def.outputs.is_empty() {
        return Err(format!("recipe `{r}` has no outputs"));
    }
    let matches = |key: &str| items.kinds.values().any(|k| k.matches(key));
    for i in &def.inputs {
        if i.count == 0 {
            return Err(format!("recipe `{r}`: input `{}` needs a count > 0", i.key));
        }
        if !matches(&i.key) {
            return Err(format!(
                "recipe `{r}`: input `{}` matches no registered item kind or tag",
                i.key
            ));
        }
    }
    for t in &def.tools {
        if !matches(t) {
            return Err(format!(
                "recipe `{r}`: tool `{t}` matches no registered item kind or tag"
            ));
        }
    }
    for o in &def.outputs {
        if o.count == 0 {
            return Err(format!(
                "recipe `{r}`: output `{}` needs a count > 0",
                o.kind
            ));
        }
        if !items.kinds.contains_key(&o.kind) {
            return Err(format!(
                "recipe `{r}`: output `{}` is not a registered item kind",
                o.kind
            ));
        }
    }
    if let Some(s) = &def.skill
        && rules.skill(s).is_none()
    {
        return Err(format!(
            "recipe `{r}`: unknown skill `{s}` (known: {})",
            rules.skill_names()
        ));
    }
    if def.stations.iter().any(String::is_empty) {
        return Err(format!("recipe `{r}`: empty workstation kind"));
    }
    Ok(())
}

/// Registered recipes by name (the names join the content hash).
#[derive(Resource, Clone, Debug, Default)]
pub struct RecipeRegistry {
    pub recipes: BTreeMap<String, RecipeDef>,
}

impl RecipeRegistry {
    pub fn get(&self, name: &str) -> Option<&RecipeDef> {
        self.recipes.get(name)
    }

    /// `"a, b"`.
    pub fn names(&self) -> String {
        self.recipes
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// How quality follows from the roll: `min(margin / margin_per_quality,
/// max)` for a success (margin below 0 counts as 0), plus
/// `crit_success_bonus` on a critical success (which may pass `max`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualityRule {
    pub margin_per_quality: i32,
    pub max: i32,
    pub crit_success_bonus: i32,
}

impl Default for QualityRule {
    fn default() -> Self {
        Self {
            margin_per_quality: 2,
            max: 5,
            crit_success_bonus: 2,
        }
    }
}

impl QualityRule {
    pub fn quality(&self, margin: i32, critical_success: bool) -> i32 {
        let q = (margin.max(0) / self.margin_per_quality.max(1)).min(self.max);
        if critical_success {
            q.saturating_add(self.crit_success_bonus)
        } else {
            q
        }
    }
}

/// Crafting tuning. Insert before adding the plugin to change it. Hashed as
/// `crafting_config`.
///
/// **Tool wear.** Every completed attempt (success or failure) takes
/// `wear_per_craft` from each tool's [`Durability`](crate::Durability); a
/// critical failure takes `crit_fail_wear` instead. A tool without
/// `Durability` does not wear, but a critical failure lowers its
/// `Item::quality` by 1. A tool at durability 0 breaks (it is despawned).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CraftingConfig {
    /// The crafter must be within this distance of the workstation, in mm.
    pub reach_mm: i32,
    pub quality: QualityRule,
    pub wear_per_craft: u32,
    pub crit_fail_wear: u32,
}

impl Default for CraftingConfig {
    fn default() -> Self {
        Self {
            reach_mm: 1500,
            quality: QualityRule::default(),
            wear_per_craft: 1,
            crit_fail_wear: 10,
        }
    }
}

/// How a completed craft went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CraftResult {
    Success,
    CriticalSuccess,
    /// The roll failed: time spent, nothing consumed.
    Failure,
    /// Inputs consumed, tools damaged.
    CriticalFailure,
}

impl CraftResult {
    pub fn succeeded(self) -> bool {
        matches!(self, CraftResult::Success | CraftResult::CriticalSuccess)
    }
}

/// A craft attempt that ran its full duration (not rejected, invalid or
/// cancelled), as the completion hooks see it. The crafting × progression
/// glue turns it into XP (`xp` from source `"craft"` on success) and
/// practice (`duration_ticks` on `skill`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CraftCompleted {
    pub unit: SimId,
    pub recipe: String,
    pub workstation: Option<SimId>,
    pub skill: Option<String>,
    pub result: CraftResult,
    /// The roll's margin (0 without a roll).
    pub margin: i32,
    /// Output quality (`None` unless it succeeded).
    pub quality: Option<i32>,
    pub outputs: Vec<SimId>,
    pub duration_ticks: u32,
    pub xp: u32,
}

/// Called for every [`CraftCompleted`], after the craft's effects.
pub type CraftHookFn = fn(&mut World, &CraftCompleted);

/// Completion hooks by name (called in name order; the names join the
/// content hash). Empty by default.
#[derive(Resource, Clone, Debug, Default)]
pub struct CraftingHooks {
    pub on_complete: Vec<(&'static str, CraftHookFn)>,
}

impl CraftingHooks {
    /// Adds (or replaces) hook `name`, keeping name order.
    pub fn add(&mut self, name: &'static str, f: CraftHookFn) {
        match self.on_complete.binary_search_by_key(&name, |(n, _)| n) {
            Ok(i) => self.on_complete[i].1 = f,
            Err(i) => self.on_complete.insert(i, (name, f)),
        }
    }
}
