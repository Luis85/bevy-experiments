//! Mod loading, content kinds, the link pass, rules sections and the rules hash.

use std::any::Any;
use std::path::{Path, PathBuf};

use bevy_ecs::world::EntityWorldMut;
use fl_content::{
    ContentDb, ContentErrors, DefComponent, DefRegistry, Faction, LinkCtx, MapDef, Placeholder,
    RulesSection, ScenarioDef, SpawnCtx, VisualDef, load_dirs,
};
use fl_math::{Fx, FxVec2};
use fl_protocol::{Controller, UnitTypeId, tables};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestHull {
    hp: u32,
}

impl DefComponent for TestHull {
    fn validate(&self, ctx: &mut LinkCtx<'_>) {
        if self.hp == 0 {
            ctx.error("hp", "must be positive");
        }
    }
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestSpeed {
    kmh: Fx,
}

impl DefComponent for TestSpeed {
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, _entity: &mut EntityWorldMut<'_>) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestRules {
    gravity: Fx,
}

impl RulesSection for TestRules {}

fn registry() -> DefRegistry {
    let mut reg = DefRegistry::with_builtins();
    reg.register_component::<TestHull>("Hull");
    reg.register_component::<TestSpeed>("Speed");
    reg.register_rules_section::<TestRules>("Test");
    reg
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load(names: &[&str]) -> Result<ContentDb, ContentErrors> {
    let dirs: Vec<PathBuf> = names.iter().map(|n| fixture(n)).collect();
    load_dirs(registry(), &dirs)
}

fn load_ok(names: &[&str]) -> ContentDb {
    load(names).unwrap_or_else(|errs| panic!("{names:?} should load:\n{errs}"))
}

fn load_err(names: &[&str]) -> ContentErrors {
    match load(names) {
        Ok(_) => panic!("{names:?} should fail to load"),
        Err(errs) => errs,
    }
}

/// Asserts that some error's display contains every one of `needles`.
fn assert_error(errs: &ContentErrors, needles: &[&str]) {
    assert!(
        errs.0.iter().any(|e| {
            let text = e.to_string();
            needles.iter().all(|n| text.contains(n))
        }),
        "no error contains all of {needles:?}; errors:\n{errs}"
    );
}

fn hull_hp(db: &ContentDb, id: &str) -> u32 {
    let unit = db.unit(db.unit_id(id).expect("unit interned")).unwrap();
    unit.get::<TestHull>(db.registry()).expect("has Hull").hp
}

// ---------------------------------------------------------------- mod order and ids

#[test]
fn loads_two_mods_in_dependency_order() {
    let db = load_ok(&["mod_b", "mod_a"]);
    let ids: Vec<&str> = db.mods().iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(db.mods()[1].depends, ["a"]);
    assert_eq!(db.mods()[1].load_after, ["not_installed"]);
    assert_eq!(db.mods()[0].dir, fixture("mod_a"));
}

#[test]
fn override_replaces_unit_from_earlier_mod() {
    let db = load_ok(&["mod_a", "mod_b"]);
    let alpha = db.unit(db.unit_id("a.alpha").unwrap()).unwrap();
    assert_eq!(alpha.name, "Alpha (b)");
    assert!(alpha.file.starts_with(fixture("mod_b")));
    let speed = alpha.get::<TestSpeed>(db.registry()).unwrap();
    assert_eq!(speed.kmh, Fx::from_int(50));
    // Still inherits from mod a's template.
    assert_eq!(hull_hp(&db, "a.alpha"), 100);
}

#[test]
fn duplicate_id_without_override_is_error() {
    let errs = load_err(&["bad_duplicate"]);
    assert_error(&errs, &["d.same", "one.unit.ron", "two.unit.ron"]);
}

#[test]
fn override_without_target_is_error() {
    let errs = load_err(&["mod_a", "bad_override"]);
    assert_error(&errs, &["ghost.unit.ron", "a.ghost", "override"]);
}

#[test]
fn id_must_carry_mod_prefix() {
    let errs = load_err(&["mod_a", "bad_prefix"]);
    assert_error(&errs, &["wrong.unit.ron", "q.wrong", "'p.'"]);
}

#[test]
fn missing_manifest_and_duplicate_mod_are_errors() {
    let errs = load_err(&["mod_a", "mod_a", "bad_no_manifest"]);
    assert_error(&errs, &["bad_no_manifest", "mod.ron"]);
    assert_error(&errs, &["duplicate mod id 'a'"]);
}

// ---------------------------------------------------------------- file-level errors

#[test]
fn unknown_file_kind_is_error() {
    let errs = load_err(&["mod_a", "bad_unknown_kind"]);
    assert_error(&errs, &["foo.weapon.ron", "unknown content file type"]);
}

#[test]
fn mod_errors_are_aggregated() {
    let errs = load_err(&["bad_aggregate"]);
    assert!(errs.0.len() >= 3, "expected at least 3 errors:\n{errs}");
    assert_error(&errs, &["missing"]);
    assert_error(&errs, &["tank.unit.ron:4:", "unknown component 'Armour'"]);
    assert_error(&errs, &["foo.weapon.ron", "unknown content file type"]);
}

#[test]
fn missing_rules_section_is_error() {
    let errs = load_err(&["bad_no_rules"]);
    assert_error(&errs, &["rules section 'Test'"]);
    // The error names the top mod's rules.ron, not a bare file name.
    let missing = errs
        .0
        .iter()
        .find(|e| e.message.contains("rules section 'Test'"))
        .expect("missing-section error");
    assert_eq!(missing.file, fixture("bad_no_rules").join("rules.ron"));
}

#[test]
fn credits_reject_nc_license() {
    let errs = load_err(&["mod_a", "bad_credits"]);
    assert_eq!(errs.0.len(), 1, "{errs}");
    assert_error(&errs, &["credits.ron:3:", "CC-BY-NC-4.0"]);
}

#[test]
fn credits_and_rules_are_loaded() {
    let db = load_ok(&["mod_a"]);
    assert_eq!(db.credits().len(), 1);
    assert_eq!(db.credits()[0].license, "CC-BY-4.0");
    assert_eq!(db.rules::<TestRules>().gravity, Fx::from_ratio(39, 4));
}

// ---------------------------------------------------------------- link pass

#[test]
fn scenario_rejects_abstract_unit() {
    let errs = load_err(&["mod_a", "bad_scenario_abstract"]);
    assert_error(
        &errs,
        &["s.abstract/units[0].unit", "a.abstract.tank", "abstract"],
    );
}

#[test]
fn scenario_rejects_unit_outside_map() {
    let errs = load_err(&["mod_a", "bad_scenario_outside"]);
    assert_error(&errs, &["s.outside/units[0].pos", "outside"]);
}

#[test]
fn faction_roster_must_exist() {
    let errs = load_err(&["mod_a", "bad_faction_roster"]);
    assert_eq!(errs.0.len(), 1, "{errs}");
    assert_error(&errs, &["f.green/roster[1]", "a.nope"]);
}

#[test]
fn map_size_must_be_multiple_of_cell() {
    let errs = load_err(&["mod_a", "bad_map"]);
    assert_error(&errs, &["m.odd/size_m", "4096"]);
    assert_error(&errs, &["m.odd/size_m", "multiple"]);
}

#[test]
fn component_validate_runs_in_link_pass() {
    let errs = load_err(&["mod_a", "bad_validate"]);
    assert_error(
        &errs,
        &[
            "zero.unit.ron",
            "v.zero/components.Hull.hp",
            "must be positive",
        ],
    );
}

#[test]
fn unit_ids_interned_in_sorted_order() {
    let db = load_ok(&["mod_a", "mod_b"]);
    assert_eq!(db.unit_id("a.alpha"), Some(UnitTypeId(0)));
    assert_eq!(db.unit_id("a.beta"), Some(UnitTypeId(1)));
    assert_eq!(db.unit_id("b.gamma"), Some(UnitTypeId(2)));
    assert_eq!(db.unit_id("a.abstract.tank"), None);
    let ids: Vec<&str> = db.units().iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["a.alpha", "a.beta", "b.gamma"]);
}

#[test]
fn kinds_are_loaded_into_sorted_tables() {
    let db = load_ok(&["mod_a", "mod_b"]);
    let factions = db.kind::<Faction>();
    assert_eq!(factions.len(), 2);
    assert!(!factions.is_empty());
    let ids: Vec<&str> = factions.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["a.blue", "a.red"]);
    assert_eq!(factions.index_of("a.red"), Some(1));
    assert_eq!(factions.get("a.red").unwrap().roster, ["a.alpha", "a.beta"]);
    assert!(factions.get("a.green").is_none());

    let map = db.kind::<MapDef>().get("a.flat").unwrap();
    assert_eq!(
        map.size_m,
        FxVec2::new(Fx::from_int(1000), Fx::from_int(1000))
    );

    let duel = db.kind::<ScenarioDef>().get("a.duel").unwrap();
    assert_eq!(duel.seed, 42);
    let controllers: Vec<&Controller> = duel.players.iter().map(|p| &p.controller).collect();
    assert_eq!(
        controllers,
        [
            &Controller::Human,
            &Controller::Ai("default".into()),
            &Controller::None
        ]
    );

    let visuals = db.kind::<VisualDef>();
    assert_eq!(visuals.len(), 3);
    assert!(matches!(
        visuals.get("a.beta").unwrap().placeholder,
        Placeholder::Infantry { .. }
    ));
    assert_eq!(
        visuals.get("a.abstract.tank").unwrap().model.as_deref(),
        Some("models/truck.glb")
    );
}

#[test]
fn load_scenario_file_links_against_db() {
    let db = load_ok(&["mod_a"]);
    let ok = db
        .load_scenario_file(&fixture("standalone/ok.scenario.ron"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ok.id, "agent.ok");
    assert_eq!(ok.players.len(), 2);

    let errs = db
        .load_scenario_file(&fixture("standalone/bad.scenario.ron"))
        .expect_err("bad scenario rejected");
    assert_error(&errs, &["bad.scenario.ron", "agent.bad/map", "a.nowhere"]);
    assert_error(&errs, &["agent.bad/players[0].faction", "a.purple"]);
    assert_error(&errs, &["agent.bad/units[0].player", "3"]);
}

// ---------------------------------------------------------------- rules hash

#[test]
fn rules_hash_stable_across_loads() {
    let first = load_ok(&["mod_a", "mod_b"]).rules_hash();
    let second = load_ok(&["mod_b", "mod_a"]).rules_hash();
    assert_eq!(first, second);
    assert_ne!(first, load_ok(&["mod_a"]).rules_hash());
}

#[test]
fn rules_hash_changes_with_stat() {
    assert_ne!(
        load_ok(&["hash_base"]).rules_hash(),
        load_ok(&["hash_stat"]).rules_hash()
    );
}

#[test]
fn rules_hash_ignores_visuals() {
    assert_eq!(
        load_ok(&["hash_base"]).rules_hash(),
        load_ok(&["hash_visual"]).rules_hash()
    );
}

#[test]
fn rules_hash_changes_with_faction_color() {
    assert_ne!(
        load_ok(&["hash_base"]).rules_hash(),
        load_ok(&["hash_faction_b"]).rules_hash()
    );
}

#[test]
fn rules_hash_changes_with_rules_section() {
    assert_ne!(
        load_ok(&["hash_base"]).rules_hash(),
        load_ok(&["hash_rules_b"]).rules_hash()
    );
}

#[test]
fn rules_hash_ignores_formatting() {
    assert_eq!(
        load_ok(&["hash_base"]).rules_hash(),
        load_ok(&["hash_format"]).rules_hash()
    );
}

// ---------------------------------------------------------------- frozen registry and string tables

#[test]
#[should_panic(expected = "DefRegistry is frozen")]
fn register_after_freeze_panics() {
    let mut reg = registry();
    reg.freeze();
    assert!(reg.is_frozen());
    reg.register_object_kind("building");
}

#[test]
fn load_freezes_the_registry() {
    assert!(!registry().is_frozen());
    assert!(load_ok(&["mod_a"]).registry().is_frozen());
}

#[test]
fn registry_string_tables_sorted() {
    let st = registry().string_tables();
    let names = |table: &str| -> Vec<&str> {
        st.get(table)
            .unwrap_or_else(|| panic!("table {table}"))
            .iter()
            .map(String::as_str)
            .collect()
    };
    assert_eq!(names(tables::DEF_COMPONENTS), ["Hull", "Speed"]);
    assert_eq!(names(tables::OBJECT_KINDS), ["unit"]);
    assert_eq!(
        names(tables::CONTENT_KINDS),
        ["faction", "map", "scenario", "terrain", "visual"]
    );
    assert_eq!(names(tables::RULES_SECTIONS), ["Test"]);
    assert_eq!(st.tables.len(), 4);
}

#[test]
fn content_string_tables_in_id_order() {
    let db = load_ok(&["mod_a", "mod_b"]);
    let st = db.string_tables();
    let units = st.get(tables::UNITS).expect("units table");
    assert_eq!(units.len(), db.units().len());
    for (i, unit) in db.units().iter().enumerate() {
        assert_eq!(units[i], unit.id);
        let id = u16::try_from(i).unwrap();
        assert_eq!(db.unit_id(&units[i]), Some(UnitTypeId(id)));
    }
    let factions = st.get("content.faction").expect("faction table");
    let ids: Vec<&str> = db.kind::<Faction>().iter().map(|f| f.id.as_str()).collect();
    assert_eq!(factions, ids);
    for (i, id) in factions.iter().enumerate() {
        assert_eq!(
            db.kind::<Faction>().index_of(id),
            Some(u32::try_from(i).unwrap())
        );
    }
    let kinds: Vec<&str> = st.tables.keys().map(String::as_str).collect();
    assert_eq!(
        kinds,
        [
            "content.faction",
            "content.map",
            "content.scenario",
            "content.terrain",
            "content.unit",
            "content.visual"
        ]
    );
}

// ---------------------------------------------------------------- link pass after resolution errors

#[test]
fn link_pass_runs_when_an_inheritance_chain_fails() {
    let errs = load_err(&["mod_a", "bad_inherit_link"]);
    // The broken chain is reported...
    assert_error(&errs, &["k.orphan/inherits", "k.ghost"]);
    // ...and the link pass still reports the unrelated roster error,
    assert_error(&errs, &["k.green/roster[1]", "a.nope"]);
    // but not a spurious "does not exist" for the unit whose chain failed.
    assert!(
        !errs.to_string().contains("unit 'k.orphan' does not exist"),
        "{errs}"
    );
}

// ---------------------------------------------------------------- warnings

#[test]
fn missing_visual_is_warning_not_error() {
    assert!(load_ok(&["mod_a"]).warnings().is_empty());
    let db = load_ok(&["mod_a", "mod_b"]);
    assert_eq!(db.warnings().len(), 1, "{:?}", db.warnings());
    assert!(db.warnings()[0].contains("b.gamma"), "{:?}", db.warnings());
}

// ---------------------------------------------------------------- ID sets (hot reload, ADR-0012)

/// A copy of fixture mod `name` in a temporary directory, for tests that edit it.
fn copy_fixture(name: &str) -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("a fixture copy directory");
        for entry in std::fs::read_dir(from).expect("a fixture directory") {
            let entry = entry.expect("a fixture entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("a file type").is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).expect("a fixture file copies");
            }
        }
    }
    let dir = tempfile::tempdir().expect("a temporary directory");
    copy(&fixture(name), dir.path());
    dir
}

fn load_dir_ok(dir: &Path) -> ContentDb {
    load_dirs(registry(), &[dir.to_path_buf()])
        .unwrap_or_else(|errs| panic!("{} should load:\n{errs}", dir.display()))
}

#[test]
fn same_id_sets_true_for_value_change() {
    let base = load_ok(&["hash_base"]);
    let stat = load_ok(&["hash_stat"]);
    assert_ne!(base.rules_hash(), stat.rules_hash(), "the values differ");
    assert!(base.same_id_sets(&stat));
    assert!(stat.same_id_sets(&base));
    assert!(base.id_set_diff(&stat).is_empty());
    assert!(base.same_id_sets(&base));
}

#[test]
fn same_id_sets_false_when_unit_added() {
    let dir = copy_fixture("mod_a");
    std::fs::write(
        dir.path().join("units/gamma.unit.ron"),
        r#"Unit(id: "a.gamma", components: { "Hull": (hp: 70) })"#,
    )
    .expect("a unit file");
    let before = load_ok(&["mod_a"]);
    let after = load_dir_ok(dir.path());
    assert!(!before.same_id_sets(&after));
    assert_eq!(before.id_set_diff(&after), ["content.unit: +a.gamma"]);
    // The other way round, the unit is gone.
    assert_eq!(after.id_set_diff(&before), ["content.unit: -a.gamma"]);
}

#[test]
fn terrain_kind_added_changes_id_set() {
    let dir = copy_fixture("mod_a");
    std::fs::write(
        dir.path().join("sand.terrain.ron"),
        r#"TerrainDef(id: "a.sand", name: "Sand", pass: 0.6, blocks: [], color: (0.8, 0.7, 0.4))"#,
    )
    .expect("a terrain file");
    std::fs::write(
        dir.path().join("units/gamma.unit.ron"),
        r#"Unit(id: "a.gamma", components: { "Hull": (hp: 70) })"#,
    )
    .expect("a unit file");
    let before = load_ok(&["mod_a"]);
    let after = load_dir_ok(dir.path());
    assert!(!before.same_id_sets(&after));
    // Sorted: by table, then by name.
    assert_eq!(
        before.id_set_diff(&after),
        ["content.terrain: +a.sand", "content.unit: +a.gamma"]
    );
}
