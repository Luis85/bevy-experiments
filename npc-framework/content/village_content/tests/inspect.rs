//! Task G: JSON inspection of units and the world, and the village_n layout.

mod common;

use common::*;
use npc_framework::ai::{BbValue, Status};
use npc_framework::kernel::journal::JournalPlugin;
use village_content::inspect::{WorldSnapshot, snapshot, unit};
use village_content::{Activity, Place, Villager};

#[test]
fn unit_snapshot_has_path_names_and_reservations() {
    let mut app = village(1);
    app.add_plugins(JournalPlugin::default());
    let ids = ids(&app);
    run_until(&mut app, 200, |app| {
        path(app, ids.villager).contains("WorkSeq > MoveToDest")
            && app
                .world()
                .resource::<Reservations>()
                .holds(ids.field, ids.villager)
    })
    .expect("walks to the reserved field");

    let u = unit(app.world(), ids.villager).expect("villager snapshot");
    assert_eq!(u.sim_id, ids.villager);
    assert_eq!(u.kind, "villager");
    assert_eq!(
        u.tree_path,
        ["Villager", "DayWork", "WorkSeq", "MoveToDest"]
    );
    let leaf = u.active_leaf.as_ref().expect("active leaf");
    assert_eq!(
        (leaf.node.as_str(), leaf.action.as_str(), leaf.status),
        ("MoveToDest", "MoveToDest", Status::Running)
    );
    assert_eq!(u.reservations_held, [ids.field]);
    assert_eq!(u.activity, Some(Activity::Walk));
    assert_eq!(u.pos, Some(pos(&app, ids.villager)));
    assert!(u.needs.is_some());
    assert_eq!(u.blackboard.get("target"), Some(&BbValue::U32(ids.field.0)));
    assert_eq!(
        u.blackboard.get("dest"),
        Some(&BbValue::Pos(3000, 2200)),
        "{:?}",
        u.blackboard
    );
    assert_eq!(u.order, None);
    assert!(!u.recent_journal.is_empty());
    assert!(u.recent_journal.len() <= 20);
    assert!(
        u.recent_journal
            .iter()
            .all(|e| e.unit == Some(ids.villager))
    );

    let pile = unit(app.world(), ids.stockpile).unwrap();
    assert_eq!(pile.kind, "stockpile");
    assert!(pile.stockpile.is_some());
    assert!(pile.tree_path.is_empty());
    assert!(unit(app.world(), SimId(999)).is_none());
}

#[test]
fn world_snapshot_serializes_roundtrip() {
    let mut app = village(1);
    app.add_plugins(JournalPlugin::default());
    run_ticks(app.world_mut(), 700);
    let snap = snapshot(app.world());
    assert_eq!(snap.tick, 700);
    assert_eq!(snap.day, 0);
    assert_eq!(snap.time_permille, 291);
    assert!(!snap.night);
    assert_eq!(
        snap.entities.iter().map(|e| e.sim_id.0).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        snap.entities
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        ["house", "bed", "stockpile", "field", "villager", "well"]
    );
    let json = serde_json::to_string(&snap).unwrap();
    let back: WorldSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
    let value = serde_json::to_value(&snap).unwrap();
    assert!(value["reservations"].is_array());
    assert_eq!(value["entities"][4]["sim_id"], 5);
}

#[test]
fn spawn_village_n_layout() {
    let mut app = new_sim_app(1, "village_n", 100);
    let world = app.world_mut();
    let mut villagers = world.query::<(&SimId, &Villager)>();
    assert_eq!(villagers.iter(world).count(), 100);
    let mut all = world.query::<&SimId>();
    let mut ids: Vec<SimId> = all.iter(world).copied().collect();
    ids.sort();
    ids.dedup();
    // Six sim entities per plot (house, bed, stockpile, field, villager, well).
    assert_eq!(ids.len(), 600);
    assert_eq!(world.resource::<SimIndex>().map.len(), 600);

    let mut places = world.query::<(&SimPos, &Place)>();
    let places: Vec<SimPos> = places.iter(world).map(|(p, _)| *p).collect();
    assert_eq!(places.len(), 500);
    for (i, a) in places.iter().enumerate() {
        for b in &places[i + 1..] {
            assert!(a.dist2(*b) >= 1_000 * 1_000, "places overlap: {a:?} {b:?}");
        }
    }
    run_ticks(world, 50);
    let snap = snapshot(world);
    let thinking = snap
        .entities
        .iter()
        .filter(|u| u.kind == "villager" && !u.tree_path.is_empty())
        .count();
    assert_eq!(thinking, 100);
}

#[test]
fn village_n_of_one_matches_village() {
    let mut a = village(7);
    let mut b = new_sim_app(7, "village_n", 1);
    run_ticks(a.world_mut(), 3000);
    run_ticks(b.world_mut(), 3000);
    assert_eq!(
        npc_framework::kernel::hash::state_hash(a.world_mut()),
        npc_framework::kernel::hash::state_hash(b.world_mut())
    );
}

/// The components and resources the remote interface shows are registered
/// with `ReflectComponent` / `ReflectResource` and readable by reflection.
#[test]
fn sim_types_are_reflect_registered() {
    use std::any::TypeId;

    use village_content::Stockpile;

    let app = village(1);
    let ids = ids(&app);
    let world = app.world();
    let registry = world.resource::<AppTypeRegistry>().read();
    for (name, type_id) in [
        ("SimId", TypeId::of::<SimId>()),
        ("SimPos", TypeId::of::<SimPos>()),
        ("Needs", TypeId::of::<Needs>()),
        ("Activity", TypeId::of::<Activity>()),
        ("Stockpile", TypeId::of::<Stockpile>()),
        ("Villager", TypeId::of::<Villager>()),
        ("Place", TypeId::of::<Place>()),
        ("CurrentOrder", TypeId::of::<CurrentOrder>()),
    ] {
        assert!(
            registry
                .get_type_data::<ReflectComponent>(type_id)
                .is_some(),
            "{name} lacks ReflectComponent"
        );
    }
    assert!(
        registry
            .get_type_data::<ReflectResource>(TypeId::of::<SimClock>())
            .is_some()
    );
    let reflect = registry
        .get_type_data::<ReflectComponent>(TypeId::of::<SimPos>())
        .unwrap();
    let value = reflect
        .reflect(world.entity(ent(&app, ids.stockpile)))
        .expect("stockpile has a SimPos");
    assert_eq!(
        value.downcast_ref::<SimPos>(),
        Some(&SimPos::new(5000, -4000))
    );
}
