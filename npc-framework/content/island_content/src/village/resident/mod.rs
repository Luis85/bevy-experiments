//! Data-authored resident behaviour for the regional island profile.
//!
//! The `behavior` block of [`LivingWorldSpec`](super::frontier::LivingWorldSpec)
//! carries the behaviour trees, the trait ranges, the daily schedule and named
//! places. A world file without the block gets the built-in data: the tree in
//! `content/island_content/trees/resident.json` and the defaults below. The
//! block is validated with the world, hashed with it and saved as part of the
//! construction data, so a continued world keeps the trees it was built with.
//!
//! Trees use the engine's flat `TreeDef` format. Every action and condition a
//! tree may name is listed in [`actions::ACTIONS`] and
//! [`conditions::CONDITIONS`]; `docs/island-ai-authoring.md` is the guide.
pub mod actions;
pub mod conditions;
pub mod param;
pub mod record;
pub mod replace;

use super::frontier::economy_spec::EconomySpec;
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::ai::{NodeDef, NodeIdx, NodeKind, TreeDef};
use npc_framework::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;

pub use record::{ConsideredOption, DecisionTrace, DecisionView, decision, explain};

pub const BEHAVIOR_VERSION: u32 = 1;
/// The tree every regional resident runs.
pub const RESIDENT_TREE: &str = super::behavior::TREE;
/// Built-in tree files, embedded like the world file.
const BUILTIN_TREES: [(&str, &str); 1] = [(
    "resident.json",
    include_str!("../../../trees/resident.json"),
)];
const MAX_NODES: usize = 512;

/// One behaviour tree and the file it was authored in (used in messages).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TreeFile {
    pub file: String,
    pub tree: TreeDef,
}

impl<'de> Deserialize<'de> for TreeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            file: String,
            tree: serde_json::Value,
        }
        let raw = Raw::deserialize(deserializer)?;
        let tree = parse_tree(&raw.file, &raw.tree).map_err(serde::de::Error::custom)?;
        Ok(Self {
            file: raw.file,
            tree,
        })
    }
}

/// Parses one tree file, naming the file and the node in every error.
///
/// Two shapes are read. The nested shape, `{"name", "root"}`, writes each
/// node's children inline, so a branch can be moved or inserted without
/// renumbering anything. The flat shape, `{"name", "nodes"}`, is the engine's
/// `TreeDef` itself: nodes in pre-order, children as indices. Both become the
/// same `TreeDef`; the saved construction data always holds the flat shape.
pub fn parse_tree(file: &str, value: &serde_json::Value) -> Result<TreeDef, String> {
    // Guards with parameters or `not` become plain condition names first.
    let mut rewritten = value.clone();
    param::rewrite_guards(&mut rewritten)
        .map_err(|error| format!("behaviour tree file `{file}`: {error}"))?;
    let value = &rewritten;
    let shape = "a tree has `name` and either `root` (nested nodes) or `nodes` (flat list)";
    let object = value
        .as_object()
        .ok_or_else(|| format!("behaviour tree file `{file}`: expected an object; {shape}"))?;
    if let Some(unknown) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "name" | "nodes" | "root"))
    {
        return Err(format!(
            "behaviour tree file `{file}`: unknown field `{unknown}`; {shape}"
        ));
    }
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("behaviour tree file `{file}`: `name` must be a string"))?;
    let mut parsed = Vec::new();
    match (object.get("root"), object.get("nodes")) {
        (Some(root), None) => {
            nested_node(file, root, &mut parsed)?;
        }
        (None, Some(nodes)) => {
            let nodes = nodes
                .as_array()
                .ok_or_else(|| format!("behaviour tree file `{file}`: `nodes` must be a list"))?;
            for (index, node) in nodes.iter().enumerate() {
                parsed.push(node_def(file, index, node, node.get("children").cloned())?);
            }
        }
        _ => {
            return Err(format!(
                "behaviour tree file `{file}`: give exactly one of `root` and `nodes`; {shape}"
            ));
        }
    }
    Ok(TreeDef {
        name: name.to_owned(),
        nodes: parsed,
    })
}

/// One node without its children, which the caller supplies as indices.
fn node_def(
    file: &str,
    index: usize,
    node: &serde_json::Value,
    children: Option<serde_json::Value>,
) -> Result<NodeDef, String> {
    let label = node
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(|name| format!(" `{name}`"))
        .unwrap_or_default();
    let at = format!("behaviour tree file `{file}`, node {index}{label}");
    let object = node
        .as_object()
        .ok_or_else(|| format!("{at}: a node is an object with `name`, `kind` and `children`"))?;
    if let Some(unknown) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "name" | "kind" | "children"))
    {
        return Err(format!(
            "{at}: unknown field `{unknown}`; a node has `name`, `kind` and `children`"
        ));
    }
    let mut flat = object.clone();
    flat.insert(
        "children".into(),
        children.unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
    );
    serde_json::from_value::<NodeDef>(serde_json::Value::Object(flat))
        .map_err(|error| format!("{at}: {error}"))
}

/// Appends a nested node and its subtree in pre-order; returns its index.
fn nested_node(
    file: &str,
    node: &serde_json::Value,
    out: &mut Vec<NodeDef>,
) -> Result<NodeIdx, String> {
    let index = out.len();
    if index >= MAX_NODES {
        return Err(format!(
            "behaviour tree file `{file}`: a tree needs 1..={MAX_NODES} nodes"
        ));
    }
    out.push(node_def(file, index, node, None)?);
    let children = match node.get("children") {
        None => &[][..],
        Some(serde_json::Value::Array(children)) => children.as_slice(),
        Some(_) => {
            return Err(format!(
                "behaviour tree file `{file}`, node {index} `{}`: `children` must be a list of \
                 nodes",
                out[index].name
            ));
        }
    };
    for child in children {
        let child = nested_node(file, child, out)?;
        out[index].children.push(child);
    }
    Ok(index as NodeIdx)
}

fn builtin_trees() -> Vec<TreeFile> {
    BUILTIN_TREES
        .iter()
        .map(|(file, text)| {
            let value = serde_json::from_str(text)
                .unwrap_or_else(|error| panic!("built-in tree file `{file}` is not JSON: {error}"));
            TreeFile {
                file: (*file).to_owned(),
                tree: parse_tree(file, &value).unwrap_or_else(|error| panic!("{error}")),
            }
        })
        .collect()
}

/// Inclusive range a trait is drawn from when a resident is founded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitRange {
    pub min: i32,
    pub max: i32,
}

/// Seeding ranges of the four traits and what one trait point is worth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TraitSpec {
    /// 0..=100. A diligent resident starts leisure later (see the schedule).
    pub diligence: TraitRange,
    /// 0..=100. At or above `sociable_min` a resident takes leisure time.
    pub sociability: TraitRange,
    /// -100..=100. Negative is an early bird; shifts every schedule block.
    pub chronotype: TraitRange,
    /// 0..=100. Coins kept back from stocking provisions ahead of need.
    pub thrift: TraitRange,
    pub sociable_min: i32,
    /// Coins a resident with thrift 100 keeps back; scales linearly.
    pub thrift_reserve_coins: u32,
}

impl Default for TraitSpec {
    fn default() -> Self {
        Self {
            diligence: TraitRange { min: 20, max: 95 },
            sociability: TraitRange { min: 5, max: 95 },
            chronotype: TraitRange {
                min: -100,
                max: 100,
            },
            thrift: TraitRange { min: 0, max: 100 },
            sociable_min: 35,
            thrift_reserve_coins: 80,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Work,
    Meal,
    Leisure,
    Sleep,
}

/// A daily window in permille of the day, `from` inclusive, `to` exclusive.
/// 0 is dawn; with the shipped 30% night, night falls at 700. `from > to`
/// wraps past midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleBlock {
    pub kind: BlockKind,
    pub from: i32,
    pub to: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScheduleSpec {
    pub blocks: Vec<ScheduleBlock>,
    /// Permille of the day every block moves later at chronotype 100.
    pub chronotype_shift: i32,
    /// Permille of the day leisure starts later at diligence 100 (earlier at 0).
    pub diligence_shift: i32,
    /// During a meal block a resident carrying food eats below this Hunger.
    pub meal_hunger_threshold: i32,
    /// Ticks a meeting lasts; it pays its morale and affinity at the end.
    pub chat_ticks: u64,
    /// A meeting ends early, paying nothing, beyond this distance (mm).
    pub chat_break_mm: i32,
}

impl Default for ScheduleSpec {
    fn default() -> Self {
        let block = |kind, from, to| ScheduleBlock { kind, from, to };
        Self {
            blocks: vec![
                block(BlockKind::Work, 0, 700),
                block(BlockKind::Meal, 250, 300),
                block(BlockKind::Meal, 600, 650),
                block(BlockKind::Leisure, 620, 700),
                block(BlockKind::Sleep, 700, 1000),
            ],
            chronotype_shift: 25,
            diligence_shift: 40,
            meal_hunger_threshold: 60_000,
            chat_ticks: 200,
            chat_break_mm: 3000,
        }
    }
}

/// A named position a `life.go_to` leaf can walk to. Several entries may share
/// a key; a resident walks to the nearest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceDef {
    pub key: String,
    pub position: SimPos,
}

/// The `behavior` block of the regional world file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BehaviorSpec {
    pub version: u32,
    /// Behaviour trees in the engine's `TreeDef` format. One must be named
    /// [`RESIDENT_TREE`].
    pub trees: Vec<TreeFile>,
    pub traits: TraitSpec,
    pub schedule: ScheduleSpec,
    pub places: Vec<PlaceDef>,
    /// Trees by life stage (review fix S3.1); a profession may name its own
    /// (`professions[].tree`). Order: child, profession, elder, default.
    pub stage_trees: StageTrees,
}

/// The trees a child and an elder run, when they differ from the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StageTrees {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elder: Option<String>,
}

impl Default for BehaviorSpec {
    fn default() -> Self {
        Self {
            version: BEHAVIOR_VERSION,
            trees: builtin_trees(),
            traits: TraitSpec::default(),
            schedule: ScheduleSpec::default(),
            places: Vec::new(),
            stage_trees: StageTrees::default(),
        }
    }
}

/// Built-in destinations of `life.go_to`.
pub const BUILTIN_PLACES: [&str; 4] = ["home", "work", "centre", "meal"];

impl BehaviorSpec {
    pub fn validate(&self, economy: &EconomySpec, bounds: GridBounds) -> Result<(), String> {
        if self.version != BEHAVIOR_VERSION {
            return Err(format!(
                "behavior.version {} is not supported; this build reads version {BEHAVIOR_VERSION}",
                self.version
            ));
        }
        for (name, range, lowest) in [
            ("diligence", self.traits.diligence, 0),
            ("sociability", self.traits.sociability, 0),
            ("chronotype", self.traits.chronotype, -100),
            ("thrift", self.traits.thrift, 0),
        ] {
            if range.min > range.max || range.min < lowest || range.max > 100 {
                return Err(format!(
                    "behavior.traits.{name} needs {lowest} <= min <= max <= 100"
                ));
            }
        }
        if !(0..=100).contains(&self.traits.sociable_min)
            || self.traits.thrift_reserve_coins > 10_000
        {
            return Err(
                "behavior.traits needs sociable_min 0..=100 and thrift_reserve_coins <= 10000"
                    .into(),
            );
        }
        let schedule = &self.schedule;
        if schedule.blocks.len() > 16
            || schedule.blocks.iter().any(|block| {
                !(0..=1000).contains(&block.from)
                    || !(0..=1000).contains(&block.to)
                    || block.from == block.to
            })
        {
            return Err(
                "behavior.schedule.blocks: at most 16 blocks, each with from and to in 0..=1000 \
                 permille of the day and from != to"
                    .into(),
            );
        }
        for kind in [BlockKind::Work, BlockKind::Sleep] {
            if !schedule.blocks.iter().any(|block| block.kind == kind) {
                return Err(
                    "behavior.schedule.blocks needs at least one work and one sleep block".into(),
                );
            }
        }
        if !(0..=200).contains(&schedule.chronotype_shift)
            || !(0..=200).contains(&schedule.diligence_shift)
            || !(1..=100_000).contains(&schedule.meal_hunger_threshold)
        {
            return Err(
                "behavior.schedule needs chronotype_shift and diligence_shift in 0..=200 permille \
                 and meal_hunger_threshold in 1..=100000"
                    .into(),
            );
        }
        let mut places = BTreeSet::new();
        for place in &self.places {
            let cell = CellCoord::new(
                place.position.x.div_euclid(1000),
                place.position.z.div_euclid(1000),
            );
            if place.key.is_empty()
                || BUILTIN_PLACES.contains(&place.key.as_str())
                || !bounds.contains(cell)
            {
                return Err(format!(
                    "behavior.places: `{}` needs a non-empty key other than home, work or centre \
                     and a position inside the map",
                    place.key
                ));
            }
            places.insert(place.key.as_str());
        }
        if self.places.len() > 64 {
            return Err("behavior.places: at most 64 places".into());
        }
        let mut files = BTreeSet::new();
        let mut names = BTreeSet::new();
        for file in &self.trees {
            if file.file.is_empty() || !files.insert(file.file.as_str()) {
                return Err(format!(
                    "behavior.trees: file name `{}` is empty or used twice",
                    file.file
                ));
            }
            let name = file.tree.name.as_str();
            if !names.insert(name) {
                return Err(format!(
                    "behaviour tree file `{}`: tree name `{name}` is used twice",
                    file.file
                ));
            }
            validate_tree(
                file,
                &Context {
                    economy,
                    places: &places,
                    bounds,
                },
            )?;
        }
        if !names.contains(RESIDENT_TREE) {
            return Err(format!(
                "behavior.trees needs a tree named `{RESIDENT_TREE}`: the tree every resident runs"
            ));
        }
        // Every tree a stage or a trade names exists (S3.1).
        for (what, tree) in [
            ("stage_trees.child", &self.stage_trees.child),
            ("stage_trees.elder", &self.stage_trees.elder),
        ] {
            if let Some(tree) = tree
                && !names.contains(tree.as_str())
            {
                return Err(format!(
                    "behavior.{what} names tree `{tree}`, which `behavior.trees` does not contain"
                ));
            }
        }
        for entry in &economy.professions {
            if let Some(tree) = &entry.tree
                && !names.contains(tree.as_str())
            {
                return Err(format!(
                    "profession `{}` names tree `{tree}`, which `behavior.trees` does not contain",
                    entry.role.key()
                ));
            }
        }
        let params = param::names_in(&self.trees);
        if params.len() > param::SLOTS {
            return Err(format!(
                "behavior.trees use {} conditions with parameters or `not`; at most {}",
                params.len(),
                param::SLOTS
            ));
        }
        Ok(())
    }

    pub fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.version);
        h.u32(self.trees.len() as u32);
        for file in &self.trees {
            h.str(&file.file);
            h.u64(file.tree.content_hash());
        }
        for range in [
            self.traits.diligence,
            self.traits.sociability,
            self.traits.chronotype,
            self.traits.thrift,
        ] {
            h.i32(range.min);
            h.i32(range.max);
        }
        h.i32(self.traits.sociable_min);
        h.u32(self.traits.thrift_reserve_coins);
        h.opt(self.stage_trees.child.as_ref(), |h, tree| h.str(tree));
        h.opt(self.stage_trees.elder.as_ref(), |h, tree| h.str(tree));
        h.u32(self.schedule.blocks.len() as u32);
        for block in &self.schedule.blocks {
            h.u8(block.kind as u8);
            h.i32(block.from);
            h.i32(block.to);
        }
        h.i32(self.schedule.chronotype_shift);
        h.i32(self.schedule.diligence_shift);
        h.i32(self.schedule.meal_hunger_threshold);
        h.u64(self.schedule.chat_ticks);
        h.i32(self.schedule.chat_break_mm);
        h.u32(self.places.len() as u32);
        for place in &self.places {
            h.str(&place.key);
            place.position.sim_hash(h);
        }
    }
}

/// What a leaf parameter may be, checked when the world is validated.
pub(super) struct Context<'a> {
    pub economy: &'a EconomySpec,
    pub places: &'a BTreeSet<&'a str>,
    pub bounds: GridBounds,
}

/// `(action, parameter name)` of a leaf; the parameter name is empty for none.
type LeafKey = (String, String);

pub(super) fn param_name(param: &ParamValue) -> String {
    match param {
        ParamValue::None => String::new(),
        ParamValue::Int(value) => value.to_string(),
        ParamValue::Pos(x, z) => format!("{x},{z}"),
        ParamValue::Name(name) => name.clone(),
    }
}

fn list(names: impl Iterator<Item = impl AsRef<str>>) -> String {
    let mut names: Vec<String> = names.map(|name| name.as_ref().to_owned()).collect();
    names.sort();
    names.join(", ")
}

/// Checks structure, every referenced name, every leaf parameter and every
/// aborting guard. Errors name the file, the node index and the node name.
fn validate_tree(file: &TreeFile, context: &Context) -> Result<(), String> {
    let tree = &file.tree;
    let at = |index: usize| {
        format!(
            "behaviour tree file `{}`, node {index} `{}`",
            file.file, tree.nodes[index].name
        )
    };
    if tree.nodes.is_empty() || tree.nodes.len() > MAX_NODES {
        return Err(format!(
            "behaviour tree file `{}`: a tree needs 1..={MAX_NODES} nodes",
            file.file
        ));
    }
    // Pre-order: the children of node n start at n + 1 and each child follows
    // the previous child's whole subtree.
    let mut end = vec![0_usize; tree.nodes.len()];
    for index in (0..tree.nodes.len()).rev() {
        let node = &tree.nodes[index];
        if node.name.trim().is_empty() {
            return Err(format!(
                "behaviour tree file `{}`, node {index}: a node needs a name",
                file.file
            ));
        }
        let mut expected = index + 1;
        for &child in &node.children {
            let child = child as usize;
            if child >= tree.nodes.len() || child != expected {
                return Err(format!(
                    "{}: children must list the nodes that follow it in order; expected child \
                     index {expected}, found {child}",
                    at(index)
                ));
            }
            expected = end[child];
        }
        end[index] = expected;
        let count = node.children.len();
        match &node.kind {
            NodeKind::Leaf { .. } if count != 0 => {
                return Err(format!("{}: a leaf has no children", at(index)));
            }
            NodeKind::Guard { .. } if count != 1 => {
                return Err(format!(
                    "{}: a guard needs exactly one child, found {count}",
                    at(index)
                ));
            }
            NodeKind::Selector | NodeKind::Sequence if count == 0 => {
                return Err(format!(
                    "{}: a selector or sequence needs at least one child",
                    at(index)
                ));
            }
            NodeKind::UtilitySelector { scorers } if count == 0 || count != scorers.len() => {
                return Err(format!(
                    "{}: a utility selector needs one child per scorer ({} scorers, {count} \
                     children)",
                    at(index),
                    scorers.len()
                ));
            }
            _ => {}
        }
    }
    if end[0] != tree.nodes.len() {
        return Err(format!(
            "behaviour tree file `{}`, node {} `{}`: not reachable from the root (node 0); every \
             node must be listed as a child of an earlier node",
            file.file, end[0], tree.nodes[end[0]].name
        ));
    }
    for (index, node) in tree.nodes.iter().enumerate() {
        match &node.kind {
            NodeKind::Leaf { action, param } => {
                actions::check(&action.name, param, context)
                    .map_err(|error| format!("{}: {error}", at(index)))?;
            }
            NodeKind::Guard { cond, abort } if param::is_param(&cond.name) => {
                param::parse(&cond.name, context.economy)
                    .map_err(|error| format!("{}: {error}", at(index)))?;
                if *abort == Abort::LowerPriority {
                    return Err(if cond.name.starts_with("not:") {
                        format!(
                            "{}: a `not` guard cannot abort lower-priority work",
                            at(index)
                        )
                    } else {
                        format!(
                            "{}: condition `{}` does not promise that its branch can start, so \
                             it must not abort lower-priority work; use \"abort\": \"None\"",
                            at(index),
                            cond.name
                        )
                    });
                }
            }
            NodeKind::Guard { cond, abort } => {
                let Some(info) = conditions::find(&cond.name) else {
                    return Err(format!(
                        "{}: unknown condition `{}`; known conditions: {}",
                        at(index),
                        cond.name,
                        list(conditions::names())
                    ));
                };
                if *abort == Abort::LowerPriority {
                    let mut first = Vec::new();
                    first_leaves(tree, node.children[0], &cond.name, &mut first);
                    let always = first
                        .iter()
                        .any(|(action, param)| actions::always_starts(action, param));
                    let missing: Vec<String> = info
                        .covers
                        .iter()
                        .filter(|(action, param)| {
                            !first.iter().any(|(a, p)| a == action && p == param)
                        })
                        .map(|(action, param)| format!("{action} {param}").trim().to_owned())
                        .collect();
                    if !always && info.covers.is_empty() {
                        return Err(format!(
                            "{}: condition `{}` does not promise that its branch can start, so \
                             it must not abort lower-priority work; use \"abort\": \"None\" or \
                             give the branch a first leaf that always starts ({})",
                            at(index),
                            cond.name,
                            list(actions::always_starting())
                        ));
                    }
                    if !always && !missing.is_empty() {
                        return Err(format!(
                            "{}: condition `{}` only promises that one of its covered leaves can \
                             start, and the branch cannot start `{}` first; add the leaf, or use \
                             \"abort\": \"None\"",
                            at(index),
                            cond.name,
                            missing.join("`, `")
                        ));
                    }
                }
            }
            NodeKind::UtilitySelector { scorers } => {
                for scorer in scorers {
                    if !conditions::SCORERS
                        .iter()
                        .any(|(name, _, _)| *name == scorer.name)
                    {
                        return Err(format!(
                            "{}: unknown scorer `{}`; known scorers: {}",
                            at(index),
                            scorer.name,
                            list(conditions::SCORERS.iter().map(|(name, _, _)| *name))
                        ));
                    }
                }
            }
            NodeKind::Selector | NodeKind::Sequence => {}
        }
    }
    Ok(())
}

/// The leaves a branch may start first when it is entered: every option of a
/// selector, the first step of a sequence. A nested guard with another
/// condition may be closed, so its leaves do not count.
fn first_leaves(tree: &TreeDef, node: NodeIdx, cond: &str, out: &mut Vec<LeafKey>) {
    let def = &tree.nodes[node as usize];
    match &def.kind {
        NodeKind::Leaf { action, param } => out.push((action.name.clone(), param_name(param))),
        NodeKind::Sequence => first_leaves(tree, def.children[0], cond, out),
        NodeKind::Selector | NodeKind::UtilitySelector { .. } => {
            for &child in &def.children {
                first_leaves(tree, child, cond, out);
            }
        }
        NodeKind::Guard { cond: inner, .. } => {
            if inner.name == cond {
                first_leaves(tree, def.children[0], cond, out);
            }
        }
    }
}

/// Per-resident personality, seeded once at founding and never changed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct ResidentTraits {
    pub diligence: i32,
    pub sociability: i32,
    pub chronotype: i32,
    pub thrift: i32,
}

impl ResidentTraits {
    /// Used for an actor that was never given traits.
    pub const NEUTRAL: Self = Self {
        diligence: 50,
        sociability: 50,
        chronotype: 0,
        thrift: 50,
    };
}

impl SimHash for ResidentTraits {
    const NAME: &'static str = "regional_resident_traits_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.diligence);
        h.i32(self.sociability);
        h.i32(self.chronotype);
        h.i32(self.thrift);
    }
}

/// `sim_rand` purposes of the seeded traits (tick 0, per resident).
const DILIGENCE_PURPOSE: u32 = 0x4149_5401;
const SOCIABILITY_PURPOSE: u32 = 0x4149_5402;
const CHRONOTYPE_PURPOSE: u32 = 0x4149_5403;
const THRIFT_PURPOSE: u32 = 0x4149_5404;

/// Deterministic founding traits of resident `id`.
pub fn seed_traits(spec: &TraitSpec, seed: u64, id: SimId) -> ResidentTraits {
    let draw = |range: TraitRange, purpose: u32| {
        let span = (range.max - range.min + 1).max(1) as u64;
        range.min + (npc_framework::kernel::rng::sim_rand(seed, id, 0, purpose) % span) as i32
    };
    ResidentTraits {
        diligence: draw(spec.diligence, DILIGENCE_PURPOSE),
        sociability: draw(spec.sociability, SOCIABILITY_PURPOSE),
        chronotype: draw(spec.chronotype, CHRONOTYPE_PURPOSE),
        thrift: draw(spec.thrift, THRIFT_PURPOSE),
    }
}

/// The active behaviour data of a regional world.
pub fn spec(world: &World) -> Option<&BehaviorSpec> {
    super::frontier::spec(world).map(|spec| &spec.behavior)
}

/// Traits of `actor`; neutral for an actor without any.
pub fn traits(world: &World, actor: Entity) -> ResidentTraits {
    world
        .get::<ResidentTraits>(actor)
        .copied()
        .unwrap_or(ResidentTraits::NEUTRAL)
}

/// Permille of the day elapsed at the current tick; 0 is dawn.
pub fn day_permille(world: &World) -> i32 {
    let day = world.resource::<SimConfig>().day_length.max(1);
    (world.resource::<SimClock>().tick % day * 1000 / day) as i32
}

fn contains(from: i32, to: i32, at: i32) -> bool {
    if from <= to {
        from <= at && at < to
    } else {
        at >= from || at < to
    }
}

/// Whether `actor` is inside a schedule block of `kind` now. Every block moves
/// with the resident's chronotype; leisure also starts later for the diligent.
pub fn in_block(world: &World, actor: Entity, kind: BlockKind) -> bool {
    let Some(spec) = spec(world) else {
        return false;
    };
    let traits = traits(world, actor);
    let schedule = &spec.schedule;
    let at = (day_permille(world) - traits.chronotype * schedule.chronotype_shift / 100)
        .rem_euclid(1000);
    schedule
        .blocks
        .iter()
        .filter(|block| block.kind == kind)
        .any(|block| {
            if kind != BlockKind::Leisure {
                return contains(block.from, block.to, at);
            }
            // `from < to` up to a whole day; `from > to` wraps past dawn.
            let length = if block.to > block.from {
                block.to - block.from
            } else {
                block.to + 1000 - block.from
            };
            let delay = (traits.diligence - 50) * schedule.diligence_shift / 50;
            delay < length && contains((block.from + delay).rem_euclid(1000), block.to, at)
        })
}

/// Coins `actor` keeps back from stocking provisions ahead of need.
pub fn thrift_reserve(world: &World, actor: Entity) -> u32 {
    let Some(spec) = spec(world) else {
        return 0;
    };
    (traits(world, actor).thrift.clamp(0, 100) as u32) * spec.traits.thrift_reserve_coins / 100
}

/// Position of a `life.go_to` destination for `actor`.
pub fn place(world: &World, actor: Entity, param: &ParamValue) -> Option<SimPos> {
    match param {
        ParamValue::Pos(x, z) => Some(SimPos::new(*x, *z)),
        ParamValue::Name(name) => match name.as_str() {
            "home" => world
                .get::<super::Villager>(actor)
                .map(|person| person.home),
            "work" => {
                let workplace = world.get::<super::Villager>(actor)?.workplace;
                let site = world.resource::<SimIndex>().get(workplace)?;
                world.get::<SimPos>(site).copied()
            }
            "centre" => {
                let settlement = super::frontier::settlements::member(world, actor)?;
                super::frontier::settlements::definition(world, settlement)
                    .map(|definition| definition.center)
            }
            // Where a meal is eaten (S3.3): at home, else at the nearest
            // inn, else at the square.
            "meal" => {
                let id = *world.get::<SimId>(actor)?;
                if super::colony::home_of(world, id).is_some() {
                    return world
                        .get::<super::Villager>(actor)
                        .map(|person| person.home);
                }
                let here = *world.get::<SimPos>(actor)?;
                super::indoors::inns(world)
                    .into_iter()
                    .map(|inn| inn.entrance)
                    .min_by_key(|door| (here.dist2(*door), door.x, door.z))
                    .or_else(|| place(world, actor, &ParamValue::Name("centre".into())))
            }
            key => {
                let here = *world.get::<SimPos>(actor)?;
                spec(world)?
                    .places
                    .iter()
                    .filter(|place| place.key == key)
                    .map(|place| place.position)
                    .min_by_key(|position| (here.dist2(*position), position.x, position.z))
            }
        },
        _ => None,
    }
}

/// Registers the resident actions, conditions, scorers, traits and the trees
/// of the world's behaviour data. Regional profile only.
pub(super) fn register(app: &mut App) {
    let spec = app
        .world()
        .resource::<super::frontier::LivingWorldSpec>()
        .behavior
        .clone();
    app.init_resource::<DecisionTrace>()
        .register_sim_component::<ResidentTraits>()
        .register_sim_component::<DayRhythm>()
        .register_sim_component::<super::ai::Errand>()
        .register_command::<replace::ReplaceBehavior>();
    for action in actions::ACTIONS {
        app.register_action(action.name, action.start, super::behavior::stop);
    }
    for condition in conditions::CONDITIONS {
        if !condition.external {
            app.register_cond(condition.name, condition.test);
        }
    }
    for (name, score, _) in conditions::SCORERS {
        app.register_scorer(name, *score);
    }
    // Conditions with parameters or `not`: one slot each (S3.1).
    let economy = app
        .world()
        .resource::<super::frontier::LivingWorldSpec>()
        .economy
        .clone();
    let mut table = param::ParamConds::default();
    for (index, name) in param::names_in(&spec.trees).into_iter().enumerate() {
        let Ok(cond) = param::parse(&name, &economy) else {
            continue;
        };
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        app.register_cond(leaked, param::SLOT_FNS[index]);
        table.slots.push((name, cond));
    }
    app.insert_resource(table);
    for file in spec.trees {
        app.world_mut()
            .get_resource_or_init::<AiRegistry>()
            .add_tree(file.tree);
    }
}

/// The tree `actor` runs: its life stage's (a child's), else its trade's,
/// else an elder's, else [`RESIDENT_TREE`] (S3.1). A working elder keeps the
/// trade's routine.
pub fn tree_for(world: &World, actor: Entity) -> Option<npc_framework::ai::TreeId> {
    let registry = world.get_resource::<AiRegistry>()?;
    let spec = spec(world);
    let stage = super::demography::stage(world, actor);
    let by_name = |name: &Option<String>| name.as_deref().and_then(|name| registry.tree_id(name));
    let trade = world.get::<super::Villager>(actor).and_then(|person| {
        super::frontier::economy_spec::registry(world)?
            .definition(person.role)?
            .tree
            .clone()
    });
    let child = spec
        .filter(|_| stage == super::demography::LifeStage::Child)
        .and_then(|spec| by_name(&spec.stage_trees.child));
    let elder = spec
        .filter(|_| stage == super::demography::LifeStage::Elder)
        .and_then(|spec| by_name(&spec.stage_trees.elder));
    child
        .or_else(|| by_name(&trade))
        .or(elder)
        .or_else(|| registry.tree_id(RESIDENT_TREE))
}

/// Gives `actor` the tree [`tree_for`] names, stopping its running leaf
/// through the one teardown path when the tree changes.
pub fn assign_tree(world: &mut World, actor: Entity) {
    let Some(tree) = tree_for(world, actor) else {
        return;
    };
    if world
        .get::<Brain>(actor)
        .is_some_and(|brain| brain.tree == tree)
    {
        return;
    }
    if world.get::<ActiveLeaf>(actor).is_some() {
        super::behavior::stop(world, actor, npc_framework::ai::StopReason::Aborted);
    }
    world
        .entity_mut(actor)
        .remove::<(BrainState, ActiveLeaf)>()
        .insert(Brain { tree });
}

/// Stops `actor`'s running leaf and gives it a fresh brain on the tree
/// [`tree_for`] names (after the trees were replaced, every id is new).
pub fn reset_brain(world: &mut World, actor: Entity) {
    if world.get::<ActiveLeaf>(actor).is_some() {
        super::behavior::stop(world, actor, npc_framework::ai::StopReason::Aborted);
    }
    let Some(tree) = tree_for(world, actor) else {
        return;
    };
    world
        .entity_mut(actor)
        .remove::<(BrainState, ActiveLeaf)>()
        .insert(Brain { tree });
}

/// The meals a resident has had (S3.3): the day it last sat down to a meal,
/// so the day has one sit-down meal (the first meal block it reaches;
/// hunger is still answered at any hour).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct DayRhythm {
    /// The day of the last sit-down meal; `None` before the first.
    pub last_meal: Option<u64>,
}
impl SimHash for DayRhythm {
    const NAME: &'static str = "island_day_rhythm_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.last_meal.as_ref(), |h, meal| h.u64(*meal));
    }
}

/// The day, while `actor` is in a meal block (none outside one).
pub fn meal_block(world: &World, actor: Entity) -> Option<u64> {
    let spec = spec(world)?;
    let traits = traits(world, actor);
    let schedule = &spec.schedule;
    let at = (day_permille(world) - traits.chronotype * schedule.chronotype_shift / 100)
        .rem_euclid(1000);
    let day = world.resource::<SimClock>().tick / world.resource::<SimConfig>().day_length.max(1);
    schedule
        .blocks
        .iter()
        .any(|block| block.kind == BlockKind::Meal && contains(block.from, block.to, at))
        .then_some(day)
}

/// Marks that the travel of the leaf being started was abandoned for good.
pub(super) fn travel_failed(world: &mut World) {
    if let Some(mut trace) = world.get_resource_mut::<DecisionTrace>() {
        trace.travel_failed = true;
    }
}

/// Short present-tense label of the running resident leaf, for inspectors.
pub fn label(world: &World, actor: Entity) -> Option<&'static str> {
    let leaf = world.get::<ActiveLeaf>(actor)?;
    let brain = world.get::<Brain>(actor)?;
    let registry = world.get_resource::<AiRegistry>()?;
    let NodeKind::Leaf { action, param } = &registry
        .tree(brain.tree)
        .nodes
        .get(leaf.node as usize)?
        .kind
    else {
        return None;
    };
    if super::indoors::asleep(world, actor) {
        // A guest in a rented bed sleeps at the inn, not at home.
        let at_inn = super::indoors::inside_entity(world, actor)
            .is_some_and(|building| super::indoors::is_inn(world, building));
        return Some(if at_inn {
            "asleep at the inn"
        } else {
            "asleep at home"
        });
    }
    // A resident who has reached home is no longer "going home".
    if action.name == "life.go_to"
        && *param == ParamValue::Name("home".into())
        && world.get::<SimPos>(actor).is_some_and(|here| {
            world
                .get::<super::Villager>(actor)
                .is_some_and(|person| here.dist2(person.home) <= 1_500_i64.pow(2))
        })
    {
        return Some(if in_block(world, actor, BlockKind::Sleep) {
            "asleep at home"
        } else {
            "at home"
        });
    }
    // Fishing is a gathering at a fishing ground: say so, at the water and
    // on the way there.
    let fishing = world
        .get::<super::Gathering>(actor)
        .and_then(|gathering| world.resource::<SimIndex>().get(gathering.patch))
        .and_then(|patch| world.get::<super::ResourcePatch>(patch))
        .is_some_and(|patch| patch.kind == super::catalog::FISH)
        || (action.name == "life.gather"
            && world.get::<super::Villager>(actor).is_some_and(|person| {
                super::catalog::outputs_for(world, person.role).contains(&super::catalog::FISH)
            }));
    if fishing && matches!(action.name.as_str(), "life.gather" | "life.continue") {
        return Some("fishing");
    }
    actions::label(&action.name, param)
}
