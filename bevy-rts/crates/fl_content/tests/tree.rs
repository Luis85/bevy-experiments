//! The lossless content tree (ADR-0020, R148): parsing, printing, typed reads from trees,
//! the unchanged rules hashes of the tree pipeline, and per-path origins.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy_ecs::world::EntityWorldMut;
use fl_content::{
    ContentDb, ContentTree, DefComponent, DefRegistry, LinkCtx, Node, PathSeg, RulesSection, Span,
    SpawnCtx, from_tree, load_dirs, parse_object, parse_ron, path_key, split_path_key, to_ron,
};
use fl_math::Fx;
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm, RngSeed};
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------ test registry

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

/// The registry of `tests/load.rs`, which the `fl_content` fixture mods are written for.
fn test_registry() -> DefRegistry {
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

fn crate_fixture(krate: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("tests/fixtures")
        .join(name)
}

/// The game's content, loaded with the game's registry (what `data-check` loads).
fn load_game(dirs: &[PathBuf]) -> Result<std::sync::Arc<ContentDb>, fl_sim::BuildError> {
    fl_sim::load_content(&fl_catalog::features(), dirs)
}

fn parse(text: &str) -> Node {
    parse_ron(Path::new("inline.ron"), text).unwrap_or_else(|e| panic!("{text:?} parses: {e}"))
}

fn node(value: ContentTree) -> Node {
    Node {
        value,
        span: Span { line: 0, col: 0 },
    }
}

/// `node` with every span zeroed, so trees compare by value only.
fn strip(n: &Node) -> Node {
    let value = match &n.value {
        ContentTree::Option(Some(inner)) => ContentTree::Option(Some(Box::new(strip(inner)))),
        ContentTree::Seq(items) => ContentTree::Seq(items.iter().map(strip).collect()),
        ContentTree::Tuple(items) => ContentTree::Tuple(items.iter().map(strip).collect()),
        ContentTree::Map(entries) => {
            ContentTree::Map(entries.iter().map(|(k, v)| (strip(k), strip(v))).collect())
        }
        ContentTree::Struct { name, fields } => ContentTree::Struct {
            name: name.clone(),
            fields: fields.iter().map(|(f, v)| (f.clone(), strip(v))).collect(),
        },
        ContentTree::TupleStruct { name, items } => ContentTree::TupleStruct {
            name: name.clone(),
            items: items.iter().map(strip).collect(),
        },
        other => other.clone(),
    };
    node(value)
}

// ------------------------------------------------------------------ parser and printer

#[test]
fn identifiers_survive_parsing() {
    let tree = strip(&parse("(class: Tracked, p: Box(size: (1.0, 2.0)))"));
    let expected = node(ContentTree::Struct {
        name: None,
        fields: vec![
            ("class".into(), node(ContentTree::Ident("Tracked".into()))),
            (
                "p".into(),
                node(ContentTree::Struct {
                    name: Some("Box".into()),
                    fields: vec![(
                        "size".into(),
                        node(ContentTree::Tuple(vec![
                            node(ContentTree::Float(1.0)),
                            node(ContentTree::Float(2.0)),
                        ])),
                    )],
                }),
            ),
        ],
    });
    assert_eq!(tree, expected);
    // The identifier also reads back as the enum variant.
    let class: fl_content::MoveClass = from_tree(&parse("Tracked"), Path::new("inline.ron"))
        .expect("an identifier reads as a unit variant");
    assert_eq!(class, fl_content::MoveClass::Tracked);
}

#[test]
fn floats_parse_like_ron() {
    for text in ["0.05", "2.8", "1e-3", "-4.0", "40", "0.1", "1.5e3", "-0.0"] {
        let tree = parse(text);
        let file = Path::new("inline.ron");
        let fx: Fx = from_tree(&tree, file).expect("an Fx");
        assert_eq!(
            fx,
            ron::from_str::<Fx>(text).expect("ron reads it"),
            "{text}"
        );
        let f32_tree: f32 = from_tree(&tree, file).expect("an f32");
        let f32_ron: f32 = ron::from_str(text).expect("ron reads it as f32");
        assert_eq!(f32_tree.to_bits(), f32_ron.to_bits(), "{text} as f32");
        let f64_tree: f64 = from_tree(&tree, file).expect("an f64");
        let f64_ron: f64 = ron::from_str(text).expect("ron reads it as f64");
        assert_eq!(f64_tree.to_bits(), f64_ron.to_bits(), "{text} as f64");
    }
}

#[test]
fn ambiguous_f32_float_is_rejected() {
    // 1 + 2^-24 (a midpoint between two f32s) plus 1e-18: the f64 parse lands on the
    // midpoint, which narrows to 1.0 (ties to even), while a direct f32 parse rounds up.
    let text = "1.000000059604644776390625";
    assert_ne!(
        text.parse::<f32>().expect("parses").to_bits(),
        (text.parse::<f64>().expect("parses") as f32).to_bits()
    );
    let err = parse_ron(Path::new("x.ron"), text).expect_err("ambiguous float rejected");
    assert!(err.message.contains("rounds differently"), "{err}");
    let err = parse_ron(Path::new("x.ron"), "(a: -0)").expect_err("-0 rejected");
    assert!(err.message.contains("negative zero"), "{err}");
}

/// The proptest policy of `fl_test_utils::proptest_policy` (R140): a fixed ChaCha seed, no
/// failure persistence, `PROPTEST_CASES` (else `default`) capped at `max`. Task 6 adds the
/// shared helper, which this branch predates; this test switches to it when both merge.
fn proptest_config(default: u32, max: u32) -> ProptestConfig {
    #[allow(
        clippy::disallowed_methods,
        reason = "the proptest case count comes from PROPTEST_CASES (R140)"
    )]
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default);
    ProptestConfig {
        cases: cases.min(max),
        rng_algorithm: RngAlgorithm::ChaCha,
        rng_seed: RngSeed::Fixed(0x5EED_F00D_CAFE_0001),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn ident() -> impl Strategy<Value = String> {
    "[A-Za-z_][A-Za-z0-9_]{0,6}".prop_filter("not a keyword", |s| {
        !matches!(s.as_str(), "true" | "false" | "None" | "Some")
    })
}

/// Every tree [`parse_ron`] can produce (see `to_ron`'s documentation for the others).
fn tree() -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
        Just(ContentTree::Unit),
        any::<bool>().prop_map(ContentTree::Bool),
        any::<i128>().prop_map(ContentTree::Int),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(ContentTree::Float),
        any::<String>().prop_map(ContentTree::Str),
        ident().prop_map(ContentTree::Ident),
        Just(ContentTree::Option(None)),
    ];
    leaf.prop_recursive(4, 48, 5, |inner| {
        let child = inner.prop_map(node);
        prop_oneof![
            child
                .clone()
                .prop_map(|n| ContentTree::Option(Some(Box::new(n)))),
            prop::collection::vec(child.clone(), 0..5).prop_map(ContentTree::Seq),
            prop::collection::vec(child.clone(), 1..5).prop_map(ContentTree::Tuple),
            prop::collection::vec((child.clone(), child.clone()), 0..4).prop_map(ContentTree::Map),
            (
                prop::option::of(ident()),
                prop::collection::vec((ident(), child.clone()), 1..4)
            )
                .prop_map(|(name, fields)| ContentTree::Struct { name, fields }),
            ident().prop_map(|name| ContentTree::Struct {
                name: Some(name),
                fields: Vec::new(),
            }),
            (ident(), prop::collection::vec(child, 1..4)).prop_map(|(name, items)| {
                ContentTree::TupleStruct {
                    name: Some(name),
                    items,
                }
            }),
        ]
    })
    .prop_map(node)
}

proptest! {
    #![proptest_config(proptest_config(256, 1024))]

    #[test]
    fn printer_round_trips(original in tree()) {
        let text = to_ron(&original);
        let back = parse_ron(Path::new("printed.ron"), &text)
            .map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
        prop_assert_eq!(strip(&back), original, "{}", text);
    }
}

#[test]
fn unsupported_syntax_names_position() {
    let err = parse_ron(Path::new("x.ron"), "(a: 'c')").expect_err("char literal rejected");
    // Line 1, column 5, shown once: `<file>:<line>:<column>: <message>`.
    assert_eq!(err.line_col, Some((1, 5)));
    assert_eq!(
        err.to_string(),
        "x.ron:1:5: char literals are not supported"
    );
    for (text, needle) in [
        ("(a: 0x10)", "unsupported number literal"),
        ("(a: 1_000)", "underscores"),
        ("(a: r\"x\")", "raw strings"),
        ("#![enable(implicit_some)]\n(a: 1)", "attributes"),
        ("(a: \"\\q\")", "unsupported escape"),
        ("(a: 1) (b: 2)", "one value per file"),
        ("(a: 1", "expected ',' or ')'"),
        ("/* open", "unclosed block comment"),
    ] {
        let err = parse_ron(Path::new("x.ron"), text).expect_err(text);
        assert!(err.message.contains(needle), "{text}: {err}");
        assert!(err.line_col.is_some(), "{text}: {err}");
    }
}

#[test]
fn typed_errors_name_field_path() {
    let text = "Unit(\n    id: \"t.x\",\n    components: {\n        \"Hull\": (hp: -1),\n    },\n)";
    let err = parse_object(&test_registry(), "unit", Path::new("x.unit.ron"), text)
        .expect_err("hp is a u32");
    assert_eq!(err.path, "t.x/components.Hull.hp", "{err}");
    assert_eq!(err.line_col, Some((4, 22)), "{err}");
    assert!(err.message.contains("-1"), "{err}");

    // A rules section's error names the section and field.
    let tree = parse("Rules({\n  \"Test\": (gravity: \"heavy\"),\n})");
    let dir = tempfile::tempdir().expect("a temp dir");
    std::fs::write(dir.path().join("mod.ron"), "Mod(id: \"t\", version: \"1\")").unwrap();
    std::fs::write(dir.path().join("rules.ron"), to_ron(&tree)).unwrap();
    let errs = load_dirs(test_registry(), &[dir.path().to_path_buf()])
        .err()
        .expect("gravity is a number");
    assert!(
        errs.0
            .iter()
            .any(|e| e.path == "Test.gravity" && e.line_col.is_some()),
        "{errs}"
    );

    // A content kind's error names the item by its id field.
    std::fs::write(
        dir.path().join("rules.ron"),
        r#"Rules({ "Test": (gravity: 9.8) })"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("mud.terrain.ron"),
        r#"TerrainDef(id: "t.mud", name: "Mud", pass: 0.5, blocks: [Hover], color: (0.1, 0.1, 0.1))"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("x.visual.ron"),
        r#"VisualDef(unit: "t.x", placeholder: Box(size: (1.0, 1.0, 1.0)), scale: "big", recolor: true)"#,
    )
    .unwrap();
    let errs = load_dirs(test_registry(), &[dir.path().to_path_buf()])
        .err()
        .expect("Hover and \"big\" are invalid");
    let paths: Vec<&str> = errs.0.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"t.mud/blocks.0"), "{errs}");
    assert!(paths.contains(&"t.x/scale"), "{errs}");
}

#[test]
fn overridden_object_is_still_type_checked() {
    // Mod x's unit is invalid; mod y overrides it with a valid one. x stays broken on its own,
    // so loading both still reports x's error.
    let root = tempfile::tempdir().expect("a temp dir");
    let (x, y) = (root.path().join("x"), root.path().join("y"));
    std::fs::create_dir_all(&x).unwrap();
    std::fs::create_dir_all(&y).unwrap();
    std::fs::write(x.join("mod.ron"), r#"Mod(id: "x", version: "1")"#).unwrap();
    std::fs::write(x.join("rules.ron"), r#"Rules({ "Test": (gravity: 9.8) })"#).unwrap();
    std::fs::write(
        x.join("u.unit.ron"),
        r#"Unit(id: "x.u", components: { "Hull": (hp: -1) })"#,
    )
    .unwrap();
    std::fs::write(
        y.join("mod.ron"),
        r#"Mod(id: "y", version: "1", depends: ["x"])"#,
    )
    .unwrap();
    std::fs::write(
        y.join("u.unit.ron"),
        r#"Unit(id: "x.u", override: true, components: { "Hull": (hp: 5) })"#,
    )
    .unwrap();
    let errs = load_dirs(test_registry(), &[x.clone(), y])
        .err()
        .expect("the replaced document is invalid");
    assert_eq!(errs.0.len(), 1, "{errs}");
    assert_eq!(errs.0[0].path, "x.u/components.Hull.hp", "{errs}");
    assert_eq!(errs.0[0].file, x.join("u.unit.ron"), "{errs}");
}

#[test]
fn path_keys_escape_dots_in_map_keys() {
    let path = [
        PathSeg::Field("roster".into()),
        PathSeg::Field("base.de.pz3".into()),
        PathSeg::Field(r"back\slash".into()),
        PathSeg::Index(3),
        PathSeg::Field("plain".into()),
    ];
    let key = path_key("faction:base.de", &path);
    assert_eq!(
        key,
        r"faction:base.de/roster.base\.de\.pz3.back\\slash.3.plain"
    );
    let (doc, segs) = split_path_key(&key).expect("a well-formed key");
    assert_eq!(doc, "faction:base.de");
    assert_eq!(segs, ["roster", "base.de.pz3", r"back\slash", "3", "plain"]);
    assert_eq!(split_path_key("rules"), Some(("rules", Vec::new())));
    assert_eq!(split_path_key(r"rules/a\x"), None);
    assert_eq!(split_path_key(r"rules/a\"), None);

    // Typed-error paths escape map keys the same way.
    let err = from_tree::<BTreeMap<String, Fx>>(&parse(r#"{ "a.b": "fast" }"#), Path::new("m.ron"))
        .expect_err("a string is not an Fx");
    assert_eq!(err.path, r"a\.b", "{err}");
}

// ------------------------------------------------------------------ rules hashes (R148)

#[test]
fn tree_load_matches_typed_load() {
    // Recorded at the task base (1274ec3) with the typed `ron` loader: the base mod with
    // `fl_cli data-check assets/base --json`, the others with `load_dirs` on the same
    // registries.
    let game: [(PathBuf, u64); 4] = [
        (fl_catalog::base_mod_dir(), 0x0f29_a5a5_6c79_3aa7),
        (crate_fixture("fl_sim", "core_mod"), 0x7b1c_2a29_1ae6_ab64),
        (
            crate_fixture("fl_sim", "core_mod_extra"),
            0xe385_d691_8997_747b,
        ),
        (
            crate_fixture("fl_sim", "core_mod_factions"),
            0x707d_5aca_9065_74bb,
        ),
    ];
    for (dir, expected) in game {
        let db = load_game(std::slice::from_ref(&dir))
            .unwrap_or_else(|e| panic!("{} loads: {e}", dir.display()));
        assert_eq!(
            db.rules_hash(),
            expected,
            "{}: 0x{:016x}",
            dir.display(),
            db.rules_hash()
        );
    }
    let test: [(&[&str], u64); 8] = [
        (&["mod_a"], 0x0136_d369_779d_1e7d),
        (&["mod_a", "mod_b"], 0x09e0_d8f0_5c19_f859),
        (&["hash_base"], 0x6a38_c7f8_c48a_c723),
        (&["hash_stat"], 0x4cd6_f2b2_75b0_c6df),
        (&["hash_visual"], 0x6a38_c7f8_c48a_c723),
        (&["hash_faction_b"], 0x74ea_8b7f_fde4_1b95),
        (&["hash_rules_b"], 0xd3a3_18d2_efa4_4edd),
        (&["hash_format"], 0x6a38_c7f8_c48a_c723),
    ];
    for (names, expected) in test {
        let dirs: Vec<PathBuf> = names.iter().map(|n| fixture(n)).collect();
        let db = load_dirs(test_registry(), &dirs).unwrap_or_else(|e| panic!("{names:?}: {e}"));
        assert_eq!(
            db.rules_hash(),
            expected,
            "{names:?}: 0x{:016x}",
            db.rules_hash()
        );
    }
    // The fixtures that failed at the base still fail, first on the same file.
    let failing = [
        (crate_fixture("fl_sim", "absurd_speed"), "tank.unit.ron"),
        (crate_fixture("fl_cli", "bad_mod"), "mod.ron"),
    ];
    for (dir, first_file) in failing {
        let Err(fl_sim::BuildError::Content(errors)) = load_game(std::slice::from_ref(&dir)) else {
            panic!("{} must fail to load as content", dir.display());
        };
        assert!(
            errors.0[0].file.ends_with(first_file),
            "{}: {errors}",
            dir.display()
        );
    }
}

// ------------------------------------------------------------------ origins

/// The path key of every leaf under `n`, as ADR-0020 defines them.
fn leaf_keys(n: &Node, key: String, out: &mut Vec<String>) {
    let join = |seg: &str| {
        if key.contains('/') {
            format!("{key}.{seg}")
        } else {
            format!("{key}/{seg}")
        }
    };
    match &n.value {
        ContentTree::Option(Some(inner)) => leaf_keys(inner, key, out),
        ContentTree::Seq(items)
        | ContentTree::Tuple(items)
        | ContentTree::TupleStruct { items, .. }
            if !items.is_empty() =>
        {
            for (i, item) in items.iter().enumerate() {
                leaf_keys(item, join(&i.to_string()), out);
            }
        }
        ContentTree::Map(entries) if !entries.is_empty() => {
            for (k, v) in entries {
                let seg = match &k.value {
                    ContentTree::Str(s) | ContentTree::Ident(s) => {
                        s.replace('\\', r"\\").replace('.', r"\.")
                    }
                    other => panic!("unexpected map key {other:?}"),
                };
                leaf_keys(v, join(&seg), out);
            }
        }
        ContentTree::Struct { fields, .. } if !fields.is_empty() => {
            for (f, v) in fields {
                leaf_keys(v, join(f), out);
            }
        }
        _ => out.push(key),
    }
}

fn field<'n>(n: &'n Node, name: &str) -> Option<&'n Node> {
    match &n.value {
        ContentTree::Struct { fields, .. } => {
            fields.iter().find(|(f, _)| f == name).map(|(_, v)| v)
        }
        _ => None,
    }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a readable dir") {
        let path = entry.expect("a dir entry").path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn origins_cover_every_leaf() {
    let base = fl_catalog::base_mod_dir();
    let db = load_game(std::slice::from_ref(&base)).expect("base loads");
    let origins = db.origins();

    let pz3 = &origins["unit:base.de.pz3/components.Hull.hp"];
    assert_eq!(pz3.mod_id, "base");
    assert!(pz3.file.ends_with("units/de/pz3.unit.ron"), "{pz3:?}");
    // pz2 inherits Selectable: its origin is under the template's key.
    assert!(!origins.contains_key("unit:base.de.pz2/components.Selectable"));
    let template = &origins["unit:base.abstract.light_tank/components.Selectable"];
    assert!(
        template
            .file
            .ends_with("units/templates/light_tank.unit.ron"),
        "{template:?}"
    );
    assert!(origins.contains_key("rules/Movement.min_speed_frac"));
    assert!(origins.contains_key("rules/Movement.max_slope_m.Tracked"));
    assert!(origins.contains_key("terrain:base.forest/pass"));
    assert!(
        origins.contains_key("terrain:base.forest/blocks"),
        "an empty list is a leaf"
    );
    assert!(origins.contains_key("faction:base.de/color.0"));
    assert!(origins.contains_key("visual:base.de.pz3/placeholder.hull.2"));
    assert!(origins.contains_key("map:base.testbed_1km/terrain.legend.0.0.1"));

    // Every leaf of every document, and nothing else, with its own file.
    let mut files = Vec::new();
    files_under(&base, &mut files);
    let mut expected = BTreeMap::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let Some(stem) = name.strip_suffix(".ron") else {
            continue;
        };
        if matches!(name, "mod.ron" | "credits.ron") {
            continue;
        }
        let tree = parse_ron(&file, &std::fs::read_to_string(&file).unwrap()).unwrap();
        let mut keys = Vec::new();
        if name == "rules.ron" {
            let ContentTree::TupleStruct { items, .. } = &tree.value else {
                panic!("Rules(…)");
            };
            leaf_keys(&items[0], "rules".to_owned(), &mut keys);
        } else {
            let ext = stem.rsplit_once('.').expect("<name>.<kind>.ron").1;
            let id_field = if ext == "visual" { "unit" } else { "id" };
            let Some(ContentTree::Str(id)) = field(&tree, id_field).map(|n| &n.value) else {
                panic!("{} has an id", file.display());
            };
            leaf_keys(&tree, format!("{ext}:{id}"), &mut keys);
        }
        for key in keys {
            assert!(
                expected.insert(key, file.clone()).is_none(),
                "one owner per key"
            );
        }
    }
    assert!(expected.len() > 300, "{} leaves", expected.len());
    let got: BTreeMap<String, PathBuf> = origins
        .iter()
        .map(|(key, origin)| {
            assert_eq!(origin.mod_id, "base", "{key}");
            (key.clone(), origin.file.clone())
        })
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn inherited_leaf_keeps_parent_origin() {
    // `a.alpha` inherits its Hull from `a.abstract.tank`; mod b replaces `a.alpha` whole
    // with an override that still inherits from mod a's template.
    let db = load_dirs(test_registry(), &[fixture("mod_a"), fixture("mod_b")]).expect("loads");
    let alpha = db.unit(db.unit_id("a.alpha").unwrap()).unwrap();
    assert_eq!(
        alpha.get::<TestHull>(db.registry()),
        Some(&TestHull { hp: 100 })
    );
    let origins = db.origins();
    assert!(!origins.contains_key("unit:a.alpha/components.Hull.hp"));
    let hull = &origins["unit:a.abstract.tank/components.Hull.hp"];
    assert_eq!(hull.mod_id, "a");
    assert_eq!(
        hull.file,
        fixture("mod_a").join("units/abstract_tank.unit.ron")
    );
    // The override's own leaves belong to mod b, and mod a's replaced document is gone.
    let speed = &origins["unit:a.alpha/components.Speed.kmh"];
    assert_eq!(speed.mod_id, "b");
    assert!(speed.file.starts_with(fixture("mod_b")), "{speed:?}");
    assert!(
        origins
            .iter()
            .filter(|(key, _)| key.starts_with("unit:a.alpha/"))
            .all(|(_, origin)| origin.mod_id == "b"),
        "{origins:#?}"
    );
}
