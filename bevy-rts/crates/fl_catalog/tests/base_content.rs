//! The base mod loads cleanly and its testbed scenario runs.

use fl_catalog::{base_mod_dir, features};
use fl_content::{Faction, MapDef, ScenarioDef, TerrainDef};
use fl_protocol::{PlayerId, SimId, Tick, tables};
use fl_sim::{
    MatchSetup, Pose, RunConfig, SimIndex, SimRunner, TestDriver, build_sim_app, load_content,
    setup_scenario,
};

#[test]
fn base_content_loads() {
    let db = load_content(&features(), &[base_mod_dir()]).expect("base content loads");
    assert_eq!(db.units().len(), 10);
    assert_eq!(db.kind::<Faction>().len(), 2);
    assert_eq!(db.kind::<MapDef>().len(), 2);
    assert_eq!(db.kind::<ScenarioDef>().len(), 2); // testbed, testbed_ai
    assert_eq!(db.kind::<TerrainDef>().len(), 8);
    assert!(db.warnings().is_empty(), "warnings: {:?}", db.warnings());
}

#[test]
fn base_testbed_runs() {
    let db = load_content(&features(), &[base_mod_dir()]).expect("base content loads");
    let mut setup = MatchSetup::for_scenario(&db, "base.testbed").expect("scenario exists");
    assert_eq!(setup.seed, 1);
    setup.cheats_allowed = true;
    let def = db
        .kind::<ScenarioDef>()
        .get("base.testbed")
        .cloned()
        .expect("scenario exists");
    let mut app =
        build_sim_app(&features(), db.clone(), setup, RunConfig::default()).expect("sim builds");
    let ids = setup_scenario(app.world_mut(), "base.testbed").expect("scenario sets up");
    assert_eq!(ids, (1..=10).map(SimId).collect::<Vec<_>>());

    let mut runner = SimRunner::new(app);
    let mut driver = TestDriver::new();
    let outputs = runner.run(&mut driver, 100);
    let last = outputs.last().expect("100 outputs");
    assert_eq!(last.tick, Tick(99));
    assert_eq!(runner.world().resource::<SimIndex>().len(), 10, "all alive");

    // Nobody has orders: every unit is where the scenario put it, in the sim and in the
    // last snapshot.
    let snapshot = &last.snapshot;
    assert_eq!(snapshot.tick, Tick(99));
    assert_eq!(snapshot.entities.len(), 10);
    for (id, unit) in ids.iter().zip(&def.units) {
        let record = snapshot
            .get(*id)
            .unwrap_or_else(|| panic!("{id:?} has a record"));
        assert_eq!(
            Some(record.unit),
            db.unit_id(&unit.unit),
            "{id:?} unit type"
        );
        assert_eq!(record.owner, PlayerId(unit.player), "{id:?} owner");
        assert_eq!(record.pos, unit.pos, "{id:?} position");
        assert_eq!(
            record.heading.to_degrees(),
            unit.heading_deg,
            "{id:?} heading"
        );
        assert_eq!(record.flags, 0, "{id:?} is neither moving nor invulnerable");

        let entity = runner
            .world()
            .resource::<SimIndex>()
            .get(*id)
            .expect("alive");
        let pose = runner.world().get::<Pose>(entity).expect("a pose");
        assert_eq!((pose.pos, pose.heading), (record.pos, record.heading));
    }
}

#[test]
fn base_string_tables() {
    let db = load_content(&features(), &[base_mod_dir()]).expect("base content loads");
    let tables = db.string_tables();
    let names = tables.get(tables::UNITS).expect("unit table");
    assert_eq!(names.len(), 10);
    assert!(names.is_sorted(), "unit names are sorted: {names:?}");
}
