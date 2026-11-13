//! Learned island places outlive short-term perception. Their live stock,
//! occupants and ownership remain governed by the ordinary observation checks.
use super::{ResourcePatch, VillagePlace, homestead::HomesteadPlot};
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{AgentCtx, AgentIoAppExt};
use npc_framework::perception::{RevealsFog, SeenStatics};
use npc_framework::prelude::{SimId, SimIndex, SimPos};

pub(super) fn register(app: &mut App) {
    app.register_known("island.discovered_places", known)
        .register_thing_position("island.discovered_places", position);
}

fn immovable(world: &World, id: SimId) -> bool {
    world.resource::<SimIndex>().get(id).is_some_and(|entity| {
        world.get::<VillagePlace>(entity).is_some()
            || (world.get::<super::frontier::Settlement>(entity).is_some()
                || world.get::<super::colony::Household>(entity).is_some())
            || world.get::<ResourcePatch>(entity).is_some()
            || world.get::<HomesteadPlot>(entity).is_some()
    })
}

fn record<'a>(world: &'a World, ctx: &AgentCtx) -> Option<&'a SeenStatics> {
    if world.get::<RevealsFog>(ctx.entity).is_none() {
        return None;
    }
    world.get::<SeenStatics>(ctx.entity)
}

fn known(world: &World, ctx: &AgentCtx) -> Option<Vec<SimId>> {
    Some(
        record(world, ctx)?
            .0
            .keys()
            .copied()
            .filter(|id| immovable(world, *id))
            .collect(),
    )
}

fn position(world: &World, ctx: &AgentCtx, id: SimId) -> Option<SimPos> {
    if !immovable(world, id) {
        return None;
    }
    // Never substitute the current unseen position, even for an object which
    // authored content normally leaves fixed in place.
    record(world, ctx)?.0.get(&id).map(|seen| seen.pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::agent_io::{api, observe, registry::AgentRegistry};
    use npc_framework::kernel::{hash::state_hash, registry::content_hash};
    use npc_framework::perception::{
        Percept, PerceptionConfig, PerceptionMemory, SeenState, SenseKind,
    };
    use npc_framework::prelude::*;

    fn fixture(scenario: &str) -> (App, SimId, SimId, SimId) {
        let mut app = crate::new_sim_app(7, scenario);
        let actor = app.world().resource::<super::super::VillageState>().played;
        if scenario == "living_world" {
            super::super::colony::JoinIsland { unit: actor.0 }
                .perform(app.world_mut())
                .unwrap();
        }
        let station = app
            .world_mut()
            .query::<(&SimId, &VillagePlace)>()
            .iter(app.world())
            .filter(|(_, place)| place.role == super::super::Role::Carpenter)
            .map(|(id, _)| *id)
            .min()
            .unwrap();
        let source = app
            .world_mut()
            .query::<(&SimId, &ResourcePatch)>()
            .iter(app.world())
            .find(|(_, patch)| patch.kind == "log")
            .map(|(id, _)| *id)
            .unwrap();
        (app, actor, station, source)
    }

    fn learn(app: &mut App, actor: SimId, target: SimId, pos: SimPos) {
        let actor = app.world().resource::<SimIndex>().get(actor).unwrap();
        app.world_mut().entity_mut(actor).insert(RevealsFog);
        app.world_mut()
            .get_mut::<SeenStatics>(actor)
            .unwrap()
            .0
            .insert(
                target,
                SeenState {
                    kind: crate::world::kinds::STATION,
                    pos,
                    open: None,
                    felled: None,
                    footprint: None,
                },
            );
    }

    fn observed(app: &App, actor: SimId) -> serde_json::Value {
        observe::observe_with(app.world(), actor, Default::default())
            .unwrap()
            .0
    }

    #[test]
    fn learned_places_survive_perception_expiry_without_unseen_stock_or_moving_identity() {
        let (mut app, actor, station, source) = fixture("living_world");
        let entity = app.world().resource::<SimIndex>().get(actor).unwrap();
        let station_pos = SimPos::new(-7000, -2000);
        learn(&mut app, actor, station, station_pos);
        learn(&mut app, actor, source, SimPos::new(-20000, -2000));
        app.world_mut()
            .get_mut::<PerceptionMemory>(entity)
            .unwrap()
            .percepts
            .insert(
                station,
                Percept {
                    last_seen_tick: 0,
                    pos: station_pos,
                    via: SenseKind::Vision,
                    confidence: 1000,
                    kinds: crate::world::kinds::STATION,
                    sensed: false,
                },
            );
        assert!(
            app.world()
                .get::<PerceptionMemory>(entity)
                .unwrap()
                .get(station)
                .is_some()
        );
        let neighbour = app.world().resource::<crate::IslandIds>().id("Ada");
        learn(&mut app, actor, neighbour, SimPos::new(123, 456));
        app.world_mut()
            .entity_mut(entity)
            .insert(SimPos::new(30000, -30000));
        let expires = app
            .world()
            .resource::<PerceptionConfig>()
            .forget_after_ticks;
        run_ticks(app.world_mut(), expires + 64);
        // The actor has left these places and transient perception has expired.
        assert!(
            app.world()
                .get::<PerceptionMemory>(entity)
                .unwrap()
                .get(station)
                .is_none()
        );
        let before = state_hash(app.world_mut());
        let hash = content_hash(app.world_mut());
        let ctx = observe::ctx(app.world(), actor).unwrap();
        let known = observe::known_targets(app.world(), &ctx).unwrap();
        assert!(known.contains(&station) && known.contains(&source));
        assert!(!known.contains(&neighbour));
        let positions = &app.world().resource::<AgentRegistry>().thing_positions;
        assert_eq!(
            positions
                .values()
                .find_map(|f| f(app.world(), &ctx, station)),
            Some(station_pos)
        );
        let view = observed(&app, actor);
        assert!(view["sections"]["village"]["environment"]["workstations"].as_array().unwrap()
            .iter().any(|row| row["id"] == station.0 && !row["recipes"].as_array().unwrap().is_empty()));
        let patch = view["sections"]["village"]["environment"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == source.0)
            .unwrap();
        assert!(patch["remaining"].is_null() && patch["capacity"].is_null());
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(content_hash(app.world_mut()), hash);
    }

    #[test]
    fn discovered_positions_restore_per_actor_but_never_cross_into_a_fresh_world() {
        let (mut app, actor, station, _) = fixture("living_world");
        let at = SimPos::new(-9876, 5432);
        learn(&mut app, actor, station, at);
        let before = state_hash(app.world_mut());
        let saved = npc_framework::snapshot::save(app.world_mut()).unwrap();
        let old_epoch = api::epoch(app.world());
        let (mut restored, _, _, _) = fixture("living_world");
        npc_framework::snapshot::load(
            restored.world_mut(),
            &saved,
            npc_framework::snapshot::LoadMode::Strict,
        )
        .unwrap();
        assert_ne!(api::epoch(restored.world()), old_epoch);
        assert_eq!(state_hash(restored.world_mut()), before);
        let ctx = observe::ctx(restored.world(), actor).unwrap();
        assert_eq!(position(restored.world(), &ctx, station), Some(at));
        let other = restored.world().resource::<crate::IslandIds>().id("Ada");
        let other_ctx = observe::ctx(restored.world(), other).unwrap();
        assert_eq!(position(restored.world(), &other_ctx, station), None);
        let (fresh, _, _, _) = fixture("living_world");
        let ctx = observe::ctx(fresh.world(), actor).unwrap();
        assert_eq!(position(fresh.world(), &ctx, station), None);
    }
}
