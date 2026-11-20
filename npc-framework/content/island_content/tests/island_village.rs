use island_content::village::{self, Wallet};
use npc_framework::agent_io::api;
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;

fn scene() -> bevy_app::App {
    island_content::new_sim_app(7, "living_world")
}

fn id(app: &bevy_app::App, name: &str) -> SimId {
    app.world().resource::<island_content::IslandIds>().id(name)
}
fn entity(app: &bevy_app::App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

#[test]
fn failed_trade_is_atomic_and_cannot_sell_claimed_or_unowned_items() {
    let mut app = scene();
    let u = id(&app, "human");
    let buyer = entity(&app, u);
    let baker = entity(&app, id(&app, "Mira"));
    let pos = *app.world().get::<SimPos>(baker).unwrap();
    app.world_mut().entity_mut(buyer).insert(pos);
    let before = state_hash(app.world_mut());
    assert!(village::economy::trade(app.world_mut(), buyer, baker, "bread", 1).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    app.world_mut().get_mut::<Wallet>(baker).unwrap().coins -= 20;
    app.world_mut().get_mut::<Wallet>(buyer).unwrap().coins = 20;
    app.world_mut()
        .get_mut::<Inventory>(buyer)
        .unwrap()
        .max_weight_g = 0;
    let before = state_hash(app.world_mut());
    assert!(village::economy::trade(app.world_mut(), buyer, baker, "bread", 1).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(village::economy::consume(app.world_mut(), buyer, "log", 1).is_err());
    assert!(
        village::economy::merchant_check(app.world(), buyer, baker, "village_diary", 1, false)
            .is_err()
    );
}

#[test]
fn economy_and_diary_resume_with_identical_hashes() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 400, 2400, 40);
}

#[test]
fn hungry_baker_and_thirsty_well_keeper_restart_their_own_empty_supply() {
    let mut app = scene();
    for (name, good, need) in [
        ("Mira", "bread", "Hunger"),
        ("Nell", village::catalog::WATER, "Thirst"),
    ] {
        let e = entity(&app, id(&app, name));
        let n = village::economy::count(app.world(), e, good);
        village::economy::consume(app.world_mut(), e, good, n).unwrap();
        let need = app.world().resource::<NeedRegistry>().id(need).unwrap();
        app.world_mut()
            .get_mut::<Needs>(e)
            .unwrap()
            .get_mut(need)
            .value = 20_000;
        if name == "Mira" {
            for (kind, n) in [
                (village::catalog::FLOUR, 2),
                (village::catalog::WATER, 1),
                ("log", 1),
            ] {
                village::economy::grant(app.world_mut(), e, Item::new(kind).stack(n));
            }
        }
    }
    run_ticks(app.world_mut(), 600);
    for (name, need) in [("Mira", "Hunger"), ("Nell", "Thirst")] {
        let e = entity(&app, id(&app, name));
        assert!(
            village::economy::need(app.world(), e, need) > 50_000,
            "{name} waited for its own unavailable output"
        );
    }
}

#[test]
fn supervisor_and_its_status_endpoint_never_advance_or_mutate_authoritative_state() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let before = state_hash(app.world_mut());
    let tick = app.world().resource::<SimClock>().tick;
    npc_framework::agent_io::scheduler::configure_host(
        app.world_mut(),
        std::env::temp_dir()
            .join(format!("npc-empty-scheduler-{}", std::process::id()))
            .join("missing.json"),
    );
    npc_framework::agent_io::scheduler::frame(app.world_mut());
    let _ = api::call(app.world_mut(), "scheduler/status", None).unwrap();
    assert_eq!(state_hash(app.world_mut()), before);
    assert_eq!(app.world().resource::<SimClock>().tick, tick);
}
