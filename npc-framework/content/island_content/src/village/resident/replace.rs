//! Replace the behaviour document by recorded command (review fix S3.2).
//!
//! `{"type": "ReplaceBehavior", "behavior": {..the whole behaviour block..}}`
//!
//! The whole document travels in the command, so a replay needs no file. It
//! is validated like a world's behaviour block and applied at a tick
//! boundary (commands run in `NpcSet::Inputs`): the world's behaviour data
//! and its saved rules are replaced, and every resident, in identity order,
//! stops its running leaf and takes the tree [`super::tree_for`] now names.
//! Traits are not re-seeded: they are the person.
//!
//! Trees may change too (ADR 0045): the engine registry takes the new tree
//! set at this call (`AiRegistry::replace_trees`), the island registers the
//! new conditions with parameters, recomputes the content hash as a fresh
//! build of the new document would have it, and resets every resident's
//! brain in identity order. Actions stay fixed: a tree naming an action the
//! island does not have is refused.
use super::{BehaviorSpec, RESIDENT_TREE};
use bevy_ecs::prelude::*;
use npc_framework::ai::AiRegistry;
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// At most this many bytes of serialized behaviour in one command.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceBehavior {
    pub behavior: BehaviorSpec,
}

/// Whether `behavior` may replace the running document now; the refusal is
/// one sentence starting "Behaviour not changed:". Pure: the inspector's
/// "Reload behaviour from file" checks with it before sending.
pub fn check_behavior(world: &World, behavior: &BehaviorSpec) -> Result<(), String> {
    let refuse = |why: String| format!("Behaviour not changed: {why}");
    let spec = super::super::frontier::spec(world)
        .ok_or_else(|| refuse("this world has no living-world data".into()))?;
    let bytes = serde_json::to_vec(behavior)
        .map(|text| text.len())
        .unwrap_or(usize::MAX);
    if bytes > MAX_DOCUMENT_BYTES || behavior.trees.len() > 32 {
        return Err(refuse(format!(
            "the document is too large (at most 32 trees and {} KiB)",
            MAX_DOCUMENT_BYTES / 1024
        )));
    }
    behavior
        .validate(
            &spec.economy,
            super::super::frontier::terrain::bounds(world),
        )
        .map_err(refuse)?;
    let registry = world
        .get_resource::<AiRegistry>()
        .ok_or_else(|| refuse("the island has no behaviour registry".into()))?;
    // New conditions with parameters take free slots; the old ones keep
    // theirs.
    let known: Vec<&str> = world
        .get_resource::<super::param::ParamConds>()
        .map(|table| table.slots.iter().map(|(name, _)| name.as_str()).collect())
        .unwrap_or_default();
    let fresh = super::param::names_in(&behavior.trees)
        .into_iter()
        .filter(|name| !known.contains(&name.as_str()))
        .count();
    if known.len() + fresh > super::param::SLOTS {
        return Err(refuse(format!(
            "the island has room for {} conditions with parameters or `not` and this document needs {} more",
            super::param::SLOTS,
            fresh
        )));
    }
    let _ = registry;
    if !behavior
        .trees
        .iter()
        .any(|file| file.tree.name == RESIDENT_TREE)
    {
        return Err(refuse(format!("the tree `{RESIDENT_TREE}` is missing")));
    }
    Ok(())
}

/// The running behaviour document, for "Save behaviour to file".
pub fn export_behavior(world: &World) -> Option<BehaviorSpec> {
    super::spec(world).cloned()
}

impl ContentCommand for ReplaceBehavior {
    const NAME: &'static str = "ReplaceBehavior";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        check_behavior(world, &self.behavior).map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        if check_behavior(world, &self.behavior).is_err() {
            return;
        }
        let trees: Vec<String> = self
            .behavior
            .trees
            .iter()
            .map(|file| file.tree.name.clone())
            .collect();
        let document_hash = {
            let mut h = Fnv64::default();
            self.behavior.sim_hash(&mut h);
            h.mark()
        };
        // Every brain's tree by name, before the ids are renumbered.
        let mut brains: Vec<(SimId, Entity, String)> = {
            let names: Vec<String> = world
                .resource::<AiRegistry>()
                .trees
                .iter()
                .map(|tree| tree.name.clone())
                .collect();
            world
                .query::<(Entity, &SimId, &npc_framework::ai::Brain)>()
                .iter(world)
                .map(|(entity, id, brain)| {
                    (
                        *id,
                        entity,
                        names
                            .get(brain.tree.0 as usize)
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
                .collect()
        };
        brains.sort_unstable_by_key(|a| a.0);
        if let Err(reason) = replace_trees(world, &self.behavior) {
            record_effect(
                world,
                None,
                "VillageBehaviourNotReplaced",
                &json!({"reason":reason}),
            );
            return;
        }
        world
            .resource_mut::<super::super::frontier::LivingWorldSpec>()
            .behavior = self.behavior.clone();
        if let Some(mut rules) = world.get_resource_mut::<crate::world_runtime::ActiveWorld>()
            && let Some(living) = rules.0.living_world.as_mut()
        {
            living.behavior = self.behavior;
        }
        // Every brain is reset in identity order: tree and condition ids
        // were all renumbered. A resident takes the tree its trade or stage
        // names; any other agent keeps its tree by name.
        for (_, actor, tree) in brains {
            if world.get::<super::super::Villager>(actor).is_some() {
                super::reset_brain(world, actor);
            } else if let Some(id) = world.resource::<AiRegistry>().tree_id(&tree) {
                if world.get::<npc_framework::ai::ActiveLeaf>(actor).is_some() {
                    super::super::behavior::stop(
                        world,
                        actor,
                        npc_framework::ai::StopReason::Aborted,
                    );
                }
                world
                    .entity_mut(actor)
                    .remove::<(npc_framework::ai::BrainState, npc_framework::ai::ActiveLeaf)>()
                    .insert(npc_framework::ai::Brain { tree: id });
            }
        }
        record_effect(
            world,
            None,
            "VillageBehaviourReplaced",
            &json!({"trees":trees,"document_hash":format!("{document_hash:016x}")}),
        );
    }
}

/// Gives the engine registry the document's trees and the island's new
/// conditions with parameters, then recomputes the content hash.
fn replace_trees(world: &mut World, behavior: &BehaviorSpec) -> Result<(), String> {
    let mut trees: Vec<npc_framework::ai::TreeDef> = behavior
        .trees
        .iter()
        .map(|file| file.tree.clone())
        .collect();
    // Trees that are not the behaviour document's (the played character's
    // idle tree, the remains) stay as they are.
    let old: Vec<String> = super::spec(world)
        .map(|spec| {
            spec.trees
                .iter()
                .map(|file| file.tree.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let kept: Vec<npc_framework::ai::TreeDef> = world
        .resource::<AiRegistry>()
        .trees
        .iter()
        .filter(|tree| !old.contains(&tree.name) && !trees.iter().any(|new| new.name == tree.name))
        .cloned()
        .collect();
    // The transforms every tree got at the freeze.
    if let Some(transforms) =
        world.get_resource::<npc_framework::kernel::registry::TreeTransforms>()
    {
        for (_, transform) in &transforms.transforms {
            for tree in &mut trees {
                transform(tree);
            }
        }
    }
    let economy = super::super::frontier::spec(world)
        .map(|spec| spec.economy.clone())
        .ok_or("this world has no living-world data")?;
    let mut table = world
        .get_resource::<super::param::ParamConds>()
        .cloned()
        .unwrap_or_default();
    let mut added = Vec::new();
    for name in super::param::names_in(&behavior.trees) {
        if table.slots.iter().any(|(known, _)| *known == name) {
            continue;
        }
        let index = table.slots.len();
        if index >= super::param::SLOTS {
            return Err("no free condition slot".into());
        }
        let cond = super::param::parse(&name, &economy)?;
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        added.push((leaked, super::param::SLOT_FNS[index]));
        table.slots.push((name, cond));
    }
    trees.extend(kept);
    world
        .resource_mut::<AiRegistry>()
        .replace_trees(trees, added)?;
    world.insert_resource(table);
    recompute_content_hash(world, behavior);
    Ok(())
}

/// The content hash a fresh build of `behavior` would have: the registry's
/// names, without the conditions with parameters no tree uses any more, and
/// every tree's content hash; every other entry as before.
fn recompute_content_hash(world: &mut World, behavior: &BehaviorSpec) {
    use npc_framework::kernel::registry::ContentHash;
    use std::collections::BTreeSet;
    let used: BTreeSet<String> = super::param::names_in(&behavior.trees)
        .into_iter()
        .collect();
    let params: BTreeSet<String> = world
        .get_resource::<super::param::ParamConds>()
        .map(|table| table.slots.iter().map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    let Some(old) = world.get_resource::<ContentHash>() else {
        return;
    };
    let ai = world.resource::<AiRegistry>();
    let mut manifest: Vec<(String, String)> = old
        .manifest
        .iter()
        .filter(|(category, _)| !matches!(category.as_str(), "tree" | "tree_hash" | "cond"))
        .cloned()
        .collect();
    for (category, name) in ai.manifest() {
        if category == "cond" && params.contains(&name) && !used.contains(&name) {
            continue;
        }
        manifest.push((category.to_owned(), name));
    }
    for tree in &ai.trees {
        manifest.push((
            "tree_hash".to_owned(),
            format!("{}={:016x}", tree.name, tree.content_hash()),
        ));
    }
    manifest.sort();
    manifest.dedup();
    let mut h = Fnv64::default();
    h.u32(u32::try_from(manifest.len()).unwrap_or(u32::MAX));
    for (category, name) in &manifest {
        h.str(category);
        h.str(name);
    }
    world.insert_resource(ContentHash {
        hash: h.mark(),
        manifest,
    });
}
