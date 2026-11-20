//! Pure explorer construction data carried by a generated scenario. No
//! credentials or filesystem paths: the ordinary content loader can replay
//! the same world without an explorer host or its agent folder.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::scenario::ScenarioDoc;
use npc_framework::perception::{Perceivable, PerceptionConfig, ReachSense, RevealsFog};
use npc_framework::prelude::SimIndex;
use serde::{Deserialize, Serialize};

use crate::TestbedIds;
use crate::actors::ActorPreset;
use crate::world::{Prop, PropKind, kinds};
use crate::worlds::WorldPreset;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorerContext {
    pub actor: String,
    pub preset_id: String,
    pub preset: ActorPreset,
    pub memory: MemoryConfig,
    pub reveals_fog: bool,
    pub perceivable_chests: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSelection {
    pub id: String,
    pub preset: WorldPreset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    pub stagger: u32,
    pub forget_after_ticks: u64,
    pub memory_capacity: u32,
    pub noise_window_ticks: u64,
}

impl MemoryConfig {
    pub fn to_config(&self) -> PerceptionConfig {
        PerceptionConfig {
            stagger: self.stagger,
            forget_after_ticks: self.forget_after_ticks,
            memory_capacity: self.memory_capacity,
            noise_window_ticks: self.noise_window_ticks,
        }
    }
}

impl ExplorerContext {
    pub fn new(
        actor: &str,
        preset_id: &str,
        preset: &ActorPreset,
        memory: PerceptionConfig,
    ) -> Self {
        Self {
            actor: actor.into(),
            preset_id: preset_id.into(),
            preset: preset.clone(),
            memory: MemoryConfig {
                stagger: memory.stagger,
                forget_after_ticks: memory.forget_after_ticks,
                memory_capacity: memory.memory_capacity,
                noise_window_ticks: memory.noise_window_ticks,
            },
            reveals_fog: true,
            perceivable_chests: true,
            world: None,
        }
    }

    pub fn with_world(mut self, id: &str, preset: &WorldPreset) -> Self {
        self.memory.forget_after_ticks = preset.perception.forget_after_ticks;
        self.memory.memory_capacity = preset.perception.memory_capacity;
        self.world = Some(WorldSelection {
            id: id.into(),
            preset: preset.clone(),
        });
        self
    }

    pub fn for_world(
        actor_name: &str,
        actor_preset_id: &str,
        actor: &ActorPreset,
        world_id: &str,
        world: &WorldPreset,
    ) -> Self {
        Self::new(
            actor_name,
            actor_preset_id,
            actor,
            PerceptionConfig::default(),
        )
        .with_world(world_id, world)
    }

    pub fn write_to(&self, doc: &mut ScenarioDoc) -> Result<(), String> {
        if doc.world.is_null() {
            doc.world = serde_json::json!({});
        }
        let world = doc
            .world
            .as_object_mut()
            .ok_or("explorer scenario world must be an object")?;
        world.insert(
            "explorer_context".into(),
            serde_json::to_value(self).map_err(|e| format!("explorer context: {e}"))?,
        );
        Ok(())
    }

    pub fn apply(&self, app: &mut App) -> Result<(), String> {
        if self.memory.stagger == 0 || self.memory.memory_capacity == 0 {
            return Err("explorer context memory requires a positive stagger and capacity".into());
        }
        app.insert_resource(self.memory.to_config());
        if let Some(world) = &self.world {
            app.insert_resource(npc_framework::agent_io::map::MapEnabled(
                world.preset.systems.map,
            ));
        }
        apply_senses(
            app,
            &self.actor,
            &self.preset,
            self.reveals_fog,
            self.perceivable_chests,
        )
    }
}

/// Idempotent world setup shared by host resume and explicit scenario data.
pub fn apply_senses(
    app: &mut App,
    actor: &str,
    preset: &ActorPreset,
    reveals_fog: bool,
    perceivable_chests: bool,
) -> Result<(), String> {
    let world = app.world_mut();
    let entity = world
        .get_resource::<TestbedIds>()
        .and_then(|ids| ids.get(actor))
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .ok_or_else(|| format!("the explorer world has no actor named `{actor}`"))?;

    let mut actor = world.entity_mut(entity);
    if reveals_fog {
        actor.insert(RevealsFog);
    }
    match preset.reach_mm() {
        Some(radius_mm) => {
            actor.insert(ReachSense { radius_mm });
        }
        None => {
            actor.remove::<ReachSense>();
        }
    }
    if perceivable_chests {
        let mut chests = world.query_filtered::<(Entity, &Prop), Without<Perceivable>>();
        let chests: Vec<Entity> = chests
            .iter(world)
            .filter(|(_, p)| p.kind == PropKind::Chest)
            .map(|(e, _)| e)
            .collect();
        for e in chests {
            world
                .entity_mut(e)
                .insert(Perceivable::new(kinds::CONTAINER));
        }
    }
    Ok(())
}
