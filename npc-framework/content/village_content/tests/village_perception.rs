//! Plan wave D1: the opt-in `village_perception` scenario. `village_n` plus
//! `PerceptionPlugin`, with `Sensors` on every villager and `Perceivable` on
//! villagers and places. Built here (not in `SCENARIOS`), so the CLI, the
//! corpus and the goldens are untouched. Also the anti-goal check: without
//! the plugin the village is unchanged, and the plugin alone does not change
//! what villagers do.

use std::collections::BTreeSet;

use bevy_app::App;
use bevy_ecs::prelude::With;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{JournalEntry, JournalKind, set_journal};
use npc_framework::kernel::registry::{ContentHash, content_hash};
use npc_framework::perception::{
    Perceivable, PerceptionConfig, PerceptionMemory, PerceptionPlugin, SenseKind, Sensors,
};
use npc_framework::prelude::*;
use village_content::replay::Replay;
use village_content::{Place, VillagePlugin, Villager, new_sim_app, spawn_village_n};

const UNIT: u32 = 1;
const PLACE: u32 = 2;
const PLOTS: u32 = 4;
const TICKS: u64 = 2400;

/// The village (`village_n` layout) with the perception plugin; `sensors`
/// adds sensors and perceivables.
fn village_perception(seed: u64, plots: u32, sensors: bool) -> App {
    let mut app = App::new();
    app.add_plugins((
        NpcPlugins::new().seed(seed).without_driver(),
        VillagePlugin,
        PerceptionPlugin,
    ));
    village_content::finish_content(app.world_mut());
    spawn_village_n(app.world_mut(), plots);
    if sensors {
        let world = app.world_mut();
        let mut q = world.query_filtered::<bevy_ecs::entity::Entity, With<Villager>>();
        let villagers: Vec<_> = q.iter(world).collect();
        for v in villagers {
            world.entity_mut(v).insert((
                Sensors::new()
                    .vision(20_000, 60)
                    .hearing(8_000)
                    .proximity(1_500),
                Perceivable::new(UNIT),
            ));
        }
        let mut q = world.query_filtered::<bevy_ecs::entity::Entity, With<Place>>();
        let places: Vec<_> = q.iter(world).collect();
        for p in places {
            world.entity_mut(p).insert(Perceivable::new(PLACE));
        }
    }
    app
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn is_perception(e: &JournalEntry) -> bool {
    matches!(
        e.kind,
        JournalKind::Perceived { .. } | JournalKind::Lost { .. }
    )
}

#[test]
fn villagers_see_each_other_and_the_places() {
    let mut app = village_perception(1, PLOTS, true);
    set_journal(app.world_mut(), true, false);
    let mut seen_places: BTreeSet<SimId> = BTreeSet::new();
    let mut seen_villagers: BTreeSet<(SimId, SimId)> = BTreeSet::new();
    let villager_ids: BTreeSet<SimId> = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&SimId, With<Villager>>();
        q.iter(world).copied().collect()
    };
    assert_eq!(villager_ids.len(), PLOTS as usize);
    for _ in 0..TICKS / 100 {
        run_ticks(app.world_mut(), 100);
        let world = app.world_mut();
        let mut q = world.query::<(&SimId, &PerceptionMemory)>();
        for (&id, memory) in q.iter(world) {
            for (target, p) in &memory.percepts {
                if p.kinds & PLACE != 0 {
                    seen_places.insert(*target);
                }
                if p.kinds & UNIT != 0 && p.via != SenseKind::Hearing {
                    seen_villagers.insert((id, *target));
                }
            }
        }
    }
    // 4 places per plot.
    assert!(
        seen_places.len() >= PLOTS as usize * 3,
        "places seen: {seen_places:?}"
    );
    assert!(!seen_villagers.is_empty(), "villagers never saw each other");
    assert!(
        seen_villagers
            .iter()
            .all(|(a, b)| a != b && villager_ids.contains(b))
    );
    let j = journal(&app);
    assert!(
        j.iter()
            .any(|e| matches!(&e.kind, JournalKind::Perceived { via, .. } if via == "vision"))
    );
    assert!(j.iter().any(|e| matches!(e.kind, JournalKind::Lost { .. })));
}

#[test]
fn village_perception_is_deterministic() {
    let run = || {
        let mut app = village_perception(7, PLOTS, true);
        set_journal(app.world_mut(), true, false);
        run_ticks(app.world_mut(), 1200);
        (state_hash(app.world_mut()), journal(&app))
    };
    let (h1, j1) = run();
    let (h2, j2) = run();
    assert_eq!(h1, h2);
    assert_eq!(j1, j2);
}

#[test]
fn perception_does_not_change_what_villagers_do() {
    // Plain village vs. the plugin alone vs. the plugin with sensors: the
    // village tree does not read perception, so apart from Perceived/Lost
    // the journals are identical (the hash encoding differs: new names).
    let mut plain = new_sim_app(3, "village_n", PLOTS);
    let mut bare = village_perception(3, PLOTS, false);
    let mut sensing = village_perception(3, PLOTS, true);
    for app in [&mut plain, &mut bare, &mut sensing] {
        set_journal(app.world_mut(), true, false);
        run_ticks(app.world_mut(), TICKS);
    }
    let plain_j = journal(&plain);
    assert!(!plain_j.is_empty());
    assert_eq!(journal(&bare), plain_j, "the plugin alone changes nothing");
    let sensing_j = journal(&sensing);
    assert!(sensing_j.iter().any(is_perception));
    let filtered: Vec<_> = sensing_j
        .into_iter()
        .filter(|e| !is_perception(e))
        .collect();
    // Seqs shift where perception entries were interleaved; compare content.
    let strip = |j: &[JournalEntry]| -> Vec<(u64, Option<SimId>, JournalKind)> {
        j.iter().map(|e| (e.tick, e.unit, e.kind.clone())).collect()
    };
    assert_eq!(strip(&filtered), strip(&plain_j));
}

#[test]
fn without_the_plugin_the_village_is_unchanged() {
    // The anti-goal (spec §3): the perception crate is compiled into this
    // test build, but no app adds it, so the village keeps the content hash
    // of the recorded corpus and registers nothing of perception.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/replays/village_seed1_3days.json"
    );
    let replay: Replay =
        serde_json::from_str(&std::fs::read_to_string(path).expect("corpus replay")).unwrap();
    let mut app = new_sim_app(replay.seed, &replay.scenario, replay.villagers);
    assert_eq!(Some(content_hash(app.world_mut())), replay.content_hash);
    let manifest = &app.world().resource::<ContentHash>().manifest;
    for (_, name) in manifest {
        for banned in [
            "sensors",
            "perceivable",
            "perception",
            "facing",
            "sees_kind",
            "heard_noise",
            "remembers_kind",
            "detection",
        ] {
            assert!(
                !name.contains(banned),
                "{name} registered without the plugin"
            );
        }
    }
    assert!(!app.world().contains_resource::<PerceptionConfig>());
    // And the plugin's names do appear when it is added.
    let mut with = village_perception(replay.seed, 1, false);
    assert_ne!(Some(content_hash(with.world_mut())), replay.content_hash);
}

/// Plan Task 9 (fog of war): the fog record is snapshot-only, so neither
/// the plain village nor the village with perception changes its content or
/// state hash. Constants recorded from the code before fog existed (fe7a91e).
#[test]
fn fog_does_not_change_existing_hashes() {
    const VILLAGE: (u64, u64) = (0x6edcdabf053bdd44, 0xd64407ab0cf1216f);
    const VILLAGE_PERCEPTION: (u64, u64) = (0x722a742acb1e886b, 0x9675c29a9749cb8e);
    let mut plain = new_sim_app(1, "village", 25);
    let mut perceiving = village_perception(1, PLOTS, true);
    let mut got = Vec::new();
    for app in [&mut plain, &mut perceiving] {
        let content = content_hash(app.world_mut());
        run_ticks(app.world_mut(), 600);
        got.push((content, state_hash(app.world_mut())));
    }
    assert_eq!(got, vec![VILLAGE, VILLAGE_PERCEPTION]);
}
