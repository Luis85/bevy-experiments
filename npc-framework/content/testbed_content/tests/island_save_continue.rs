//! A save of the shipped living world continues the way the title screen's
//! Continue does (`actor_testbed::island::restore_validated`): the structure
//! check against a freshly built world, a strict load, the colony integrity
//! check, and the same hash as the world that was never saved, after mortal
//! construction identities have gone: an animal hunted, a resident laid to
//! rest, an authored resource patch removed, and a building site cancelled.
//! Tampering with a permanent identity is still refused.
mod life_support;

use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};
use testbed_content::village::{
    self, ResourcePatch, civic,
    demography::Life,
    frontier::{self, PlaceResourcePatch, RemoveResourcePatch, settlements},
    hunting::Wildlife,
    integrity,
};

const DAY: u64 = 2_400;

fn world(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "clear");
}

/// A free spot for a `good` patch near a settlement's centre.
fn spot(world: &World, good: &str, settlement: SimId) -> PlaceResourcePatch {
    let centre = settlements::definition(world, settlement)
        .expect("settlement")
        .center;
    for dz in (-30..=30).step_by(3) {
        for dx in (-30..=30).step_by(3) {
            let command = PlaceResourcePatch {
                good: good.into(),
                x: centre.x + dx * 1000 + 500,
                z: centre.z + dz * 1000 + 500,
                richness: 100,
            };
            if command.check(world).is_ok() {
                return command;
            }
        }
    }
    panic!("no free spot for a {good} patch");
}

#[test]
fn a_save_after_a_hunt_a_death_a_removed_patch_and_a_cancelled_site_continues() {
    let mut app = world_with(world);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");

    // An animal of the founding is hunted (the hunt's own despawn).
    let mut animals: Vec<(SimId, Entity)> = app
        .world_mut()
        .query_filtered::<(&SimId, Entity), With<Wildlife>>()
        .iter(app.world())
        .map(|(id, entity)| (*id, entity))
        .collect();
    animals.sort_unstable();
    let (hunted, prey) = animals[0];
    app.world_mut().despawn(prey);

    // An authored berry patch is replaced by a new one and removed.
    let berries = village::catalog::BERRIES;
    let authored = frontier::shape::patches_of(app.world(), harbour, berries)[0].0;
    spot(app.world(), berries, harbour)
        .perform(app.world_mut())
        .expect("a second berry patch");
    RemoveResourcePatch { patch: authored }
        .perform(app.world_mut())
        .expect("the authored one removed");

    // A building site is placed and cancelled.
    let def = civic::definition(app.world(), "stockpile")
        .expect("stockpile")
        .clone();
    let (x, z, rotation) = civic::find_site(app.world(), &def, harbour, 36).expect("a site");
    let site = civic::PlaceBuilding {
        building: "stockpile".into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed");
    civic::CancelBuilding { site }
        .perform(app.world_mut())
        .expect("cancelled");

    // A founder dies of old age and is laid to rest.
    let (gone, body) = residents(app.world(), harbour)[0];
    let today = village::calendar(app.world()).0;
    app.world_mut()
        .get_mut::<Life>(body)
        .expect("life")
        .death_day = today + 1;
    let laid = until(&mut app, 4 * DAY, 50, |app| {
        app.world().resource::<SimIndex>().get(gone).is_none()
    });
    assert!(laid, "the founder was never laid to rest");
    assert!(app.world().resource::<SimIndex>().get(hunted).is_none());
    assert!(app.world().resource::<SimIndex>().get(authored).is_none());

    // Save, and continue as the title screen does.
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut fresh = world_with(world);
    let expected = snapshot::save(fresh.world_mut()).expect("baseline");
    assert!(
        expected
            .entities
            .iter()
            .any(|entity| entity.id == u64::from(hunted.0))
    );
    assert!(
        expected
            .entities
            .iter()
            .any(|entity| entity.id == u64::from(gone.0))
    );
    integrity::validate_snapshot_structure(&expected, &saved).expect("the save continues");
    let report =
        snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    integrity::validate_colony(fresh.world()).expect("valid after the restore");
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    run_ticks(app.world_mut(), 600);
    run_ticks(fresh.world_mut(), 600);
    assert_eq!(
        state_hash(fresh.world_mut()),
        state_hash(app.world_mut()),
        "the continued world runs on as the original"
    );

    // Tampering is still refused: a founding workplace, a resident who was
    // never laid to rest, the last patch of a good.
    let workplace =
        settlements::workplace(app.world(), harbour, village::Role::Baker).expect("a bakery");
    let alive = residents(app.world(), harbour)
        .into_iter()
        .find(|(id, _)| *id != gone)
        .expect("a living founder")
        .0;
    let last_patch = saved
        .entities
        .iter()
        .find(|entity| {
            entity
                .components
                .get(ResourcePatch::NAME)
                .is_some_and(|patch| patch["kind"] == json!("ore"))
        })
        .expect("an ore deposit")
        .id;
    for id in [u64::from(workplace.0), u64::from(alive.0), last_patch] {
        let mut cut = saved.clone();
        let ore_left = cut
            .entities
            .iter()
            .filter(|entity| {
                entity
                    .components
                    .get(ResourcePatch::NAME)
                    .is_some_and(|patch| patch["kind"] == json!("ore"))
            })
            .count();
        cut.entities.retain(|entity| entity.id != id);
        if id == last_patch && ore_left > 1 {
            // Every ore deposit of that settlement must go for this case.
            let settlement = saved
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .and_then(|entity| entity.components.get("living_world_settlement_member_v1"))
                .cloned();
            cut.entities.retain(|entity| {
                !(entity
                    .components
                    .get(ResourcePatch::NAME)
                    .is_some_and(|patch| patch["kind"] == json!("ore"))
                    && entity
                        .components
                        .get("living_world_settlement_member_v1")
                        .cloned()
                        == settlement)
            });
        }
        let error = integrity::validate_snapshot_structure(&expected, &cut)
            .expect_err("a missing permanent identity is refused");
        assert!(error.contains("is missing"), "{error}");
    }
}
