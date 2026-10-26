//! `SimFeature` ordering, the `MatchHeader` a sim is built with, and `StableRegistry`.

use std::path::PathBuf;
use std::sync::Arc;

use fl_math::stream_key;
use fl_protocol::{FORMAT_VERSION, StringTables, tables};
use fl_sim::{
    FeatureError, MatchHeaderRes, MatchSetup, RunConfig, SIM_VERSION, SimBuilder, SimFeature,
    StableRegistry, build_id, build_sim_app, load_content, sort_features,
};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// A feature that registers nothing.
struct Feat {
    id: &'static str,
    version: u32,
    deps: &'static [&'static str],
}

impl SimFeature for Feat {
    fn id(&self) -> &'static str {
        self.id
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn deps(&self) -> &'static [&'static str] {
        self.deps
    }

    fn build(&self, sim: &mut SimBuilder) {
        assert_eq!(sim.feature_id(), self.id);
    }
}

fn feat(id: &'static str, deps: &'static [&'static str]) -> Arc<dyn SimFeature> {
    Arc::new(Feat {
        id,
        version: 1,
        deps,
    })
}

fn sorted_ids(features: &[Arc<dyn SimFeature>]) -> Result<Vec<&'static str>, FeatureError> {
    sort_features(features).map(|sorted| sorted.iter().map(|f| f.id()).collect())
}

#[test]
fn features_sort_topologically_ties_by_id() {
    let features = [feat("c", &[]), feat("b", &["a"]), feat("a", &[])];
    assert_eq!(sorted_ids(&features), Ok(vec!["core", "a", "b", "c"]));
    // A dependency on core is always satisfied.
    let features = [feat("b", &["core"]), feat("a", &["b"])];
    assert_eq!(sorted_ids(&features), Ok(vec!["core", "b", "a"]));
    assert_eq!(sorted_ids(&[]), Ok(vec!["core"]));
}

#[test]
fn feature_cycle_is_error() {
    let features = [feat("a", &["b"]), feat("b", &["a"]), feat("c", &[])];
    assert_eq!(
        sorted_ids(&features),
        Err(FeatureError::Cycle(vec!["a".to_owned(), "b".to_owned()]))
    );
}

#[test]
fn missing_dependency_is_error() {
    let features = [feat("a", &[]), feat("b", &["a", "z"])];
    assert_eq!(
        sorted_ids(&features),
        Err(FeatureError::MissingDep {
            feature: "b".to_owned(),
            dep: "z".to_owned()
        })
    );
}

#[test]
fn duplicate_feature_id_is_error() {
    let features = [feat("a", &[]), feat("b", &[]), feat("a", &[])];
    assert_eq!(
        sorted_ids(&features),
        Err(FeatureError::Duplicate("a".to_owned()))
    );
}

#[test]
fn core_id_is_reserved() {
    let features = [feat("core", &[])];
    assert_eq!(
        sorted_ids(&features),
        Err(FeatureError::Reserved("core".to_owned()))
    );
}

#[test]
fn header_lists_features_mods_and_tables() {
    let features: Vec<Arc<dyn SimFeature>> = vec![Arc::new(Feat {
        id: "test",
        version: 3,
        deps: &[],
    })];
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 42,
        cheats_allowed: true,
    };
    let app = build_sim_app(&features, content.clone(), setup, RunConfig::default())
        .expect("the sim builds");
    let header = &app.world().resource::<MatchHeaderRes>().0;

    assert_eq!(header.format_version, FORMAT_VERSION);
    assert_eq!(header.build_id, build_id());
    assert_eq!(header.sim_version, SIM_VERSION);
    assert_eq!(
        header.features,
        [("core".to_owned(), 2), ("test".to_owned(), 3)]
    );
    assert_eq!(header.mods, [("core_test".to_owned(), "0.1.0".to_owned())]);
    assert_eq!(header.rules_hash, content.rules_hash());
    assert!(header.cheats_allowed);
    assert_eq!(header.seed, 42);

    let names = |table: &str| -> Vec<&str> {
        header
            .string_tables
            .get(table)
            .unwrap_or_else(|| panic!("table {table} is in the header"))
            .iter()
            .map(String::as_str)
            .collect()
    };
    assert_eq!(
        names(tables::COMMANDS),
        [
            "core.follow",
            "core.move",
            "core.rotate",
            "core.stop",
            "debug.invulnerable",
            "debug.set_hp",
            "debug.set_owner",
            "debug.set_terrain",
            "debug.spawn",
            "debug.teleport",
            "debug.wander",
        ]
    );
    assert_eq!(
        names(tables::RNG_STREAMS),
        ["debug.wander", "scale.fire", "setup.scale"]
    );
    assert_eq!(names(tables::UNITS), ["core_test.tank"]);
    assert_eq!(
        names(tables::DEF_COMPONENTS),
        ["Hull", "Mobility", "Selectable"]
    );
    assert_eq!(
        names(tables::RULES_SECTIONS),
        ["Movement", "Path", "Spatial", "StateHash", "Steering"]
    );
}

#[test]
fn build_id_is_dev_without_git_rev() {
    match option_env!("FL_GIT_REV") {
        Some(rev) => assert_eq!(build_id(), format!("{}+{rev}", env!("CARGO_PKG_VERSION"))),
        None => assert_eq!(build_id(), "dev"),
    }
}

#[test]
fn registry_ids_sorted_by_name() {
    let mut registry = StableRegistry::new("test_table");
    registry.register("b", "test", 20);
    registry.register("a", "test", 10);
    registry.register("c", "other", 30);
    assert!(!registry.is_frozen());
    assert_eq!(
        registry.get("c"),
        Some(&30),
        "lookup by name works before freeze"
    );
    registry.freeze();
    assert!(registry.is_frozen());

    assert_eq!(registry.id("a"), Some(0));
    assert_eq!(registry.id("b"), Some(1));
    assert_eq!(registry.id("c"), Some(2));
    assert_eq!(registry.id("d"), None);
    assert_eq!(registry.name(1), Some("b"));
    assert_eq!(registry.name(3), None);
    assert_eq!(registry.name_key(0), Some(stream_key("a")));
    assert_eq!(registry.name_key(3), None);
    assert_eq!(registry.get("b"), Some(&20));
    assert_eq!(registry.get_by_id(2), Some(&30));
    assert_eq!(
        registry.iter().collect::<Vec<_>>(),
        [(0, "a", &10), (1, "b", &20), (2, "c", &30)]
    );

    let mut out = StringTables::default();
    registry.export(&mut out);
    assert_eq!(
        out.get("test_table"),
        Some(&["a".to_owned(), "b".to_owned(), "c".to_owned()][..])
    );
}

#[test]
#[should_panic(expected = "is frozen")]
fn register_after_freeze_panics() {
    let mut registry = StableRegistry::new("test_table");
    registry.register("a", "test", ());
    registry.freeze();
    registry.register("b", "test", ());
}

#[test]
#[should_panic(expected = "registered by 'test' and 'other'")]
fn duplicate_name_panics_naming_both() {
    let mut registry = StableRegistry::new("test_table");
    registry.register("a", "test", ());
    registry.register("a", "other", ());
}
