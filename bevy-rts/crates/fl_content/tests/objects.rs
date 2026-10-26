//! Object parsing and inheritance tests for `fl_content`.

use std::any::Any;
use std::path::{Path, PathBuf};

use bevy_ecs::world::EntityWorldMut;
use fl_content::{
    ContentError, DefComponent, DefRegistry, ObjectDef, ResolvedObject, SpawnCtx, parse_object,
    resolve_objects,
};
use fl_math::Fx;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestHull {
    hp: u32,
}

impl DefComponent for TestHull {
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

fn registry() -> DefRegistry {
    let mut reg = DefRegistry::default();
    reg.register_component::<TestHull>("Hull");
    reg.register_component::<TestSpeed>("Speed");
    reg.register_object_kind("unit");
    reg
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/objects")
        .join(name)
}

fn parse_fixture(reg: &DefRegistry, name: &str) -> Result<ObjectDef, ContentError> {
    let path = fixture(name);
    let text = std::fs::read_to_string(&path).expect("fixture readable");
    parse_object(reg, "unit", &path, &text)
}

fn parse_inline(reg: &DefRegistry, text: &str) -> ObjectDef {
    parse_object(reg, "unit", Path::new("inline.unit.ron"), text).expect("inline object parses")
}

fn resolve(texts: &[&str]) -> Result<fl_content::Resolved, fl_content::ContentErrors> {
    let reg = registry();
    resolve_objects(texts.iter().map(|t| parse_inline(&reg, t)).collect())
}

fn concrete<'a>(resolved: &'a fl_content::Resolved, id: &str) -> &'a ResolvedObject {
    resolved
        .concrete
        .iter()
        .find(|o| o.id == id)
        .unwrap_or_else(|| panic!("concrete object {id} present"))
}

fn kmh(v: i32) -> TestSpeed {
    TestSpeed {
        kmh: Fx::from_int(v),
    }
}

#[test]
fn parses_registered_components() {
    let reg = registry();
    let obj = parse_fixture(&reg, "pz3.unit.ron").expect("pz3 parses");
    assert_eq!(obj.kind, "unit");
    assert_eq!(obj.id, "test.de.pz3");
    assert_eq!(obj.inherits.as_deref(), Some("test.abstract.tank"));
    assert!(!obj.is_abstract);
    assert!(!obj.is_override);
    assert_eq!(obj.name.as_deref(), Some("Pz.Kpfw. III"));
    assert_eq!(obj.tags, vec!["armored".to_string(), "tracked".to_string()]);
    assert!(obj.remove.is_empty());
    assert!(obj.file.ends_with("pz3.unit.ron"));
    let hull = obj.components["Hull"]
        .as_any()
        .downcast_ref::<TestHull>()
        .expect("Hull is a TestHull");
    assert_eq!(hull, &TestHull { hp: 160 });
    let speed = obj.components["Speed"]
        .as_any()
        .downcast_ref::<TestSpeed>()
        .expect("Speed is a TestSpeed");
    assert_eq!(speed.kmh, Fx::from_ratio(81, 2));
}

#[test]
fn optional_fields_default() {
    let reg = registry();
    let obj = parse_inline(&reg, r#"Unit(id: "test.min")"#);
    assert_eq!(obj.id, "test.min");
    assert_eq!(obj.inherits, None);
    assert!(!obj.is_abstract);
    assert_eq!(obj.name, None);
    assert!(obj.tags.is_empty());
    assert!(obj.components.is_empty());
    assert!(obj.remove.is_empty());
}

#[test]
fn missing_id_is_error() {
    let reg = registry();
    let err = parse_object(&reg, "unit", Path::new("x.unit.ron"), "Unit(name: \"x\")")
        .expect_err("id is required");
    assert!(err.message.contains("id"), "{err}");
}

#[test]
fn unknown_top_level_field_is_error() {
    let reg = registry();
    let err = parse_object(
        &reg,
        "unit",
        Path::new("x.unit.ron"),
        "Unit(id: \"test.x\", colour: 1)",
    )
    .expect_err("unknown field rejected");
    assert!(err.message.contains("colour"), "{err}");
    assert_eq!(err.path, "test.x/colour");
}

#[test]
fn unknown_component_is_error_with_line() {
    let reg = registry();
    let e = parse_fixture(&reg, "unknown_component.unit.ron").expect_err("Armour is unknown");
    assert!(
        e.message.contains("unknown component 'Armour'"),
        "message: {}",
        e.message
    );
    assert!(e.line_col.is_some(), "{e}");
    assert_eq!(e.line_col.map(|(line, _)| line), Some(5), "{e}");
    assert!(e.file.ends_with("unknown_component.unit.ron"), "{e}");
    assert_eq!(e.path, "test.bad.armour/components.Armour");
    let shown = e.to_string();
    assert!(shown.contains("unknown_component.unit.ron:5:"), "{shown}");
}

#[test]
fn unknown_field_is_error() {
    let reg = registry();
    let e = parse_fixture(&reg, "unknown_field.unit.ron").expect_err("colour is unknown");
    assert!(e.message.contains("colour"), "message: {}", e.message);
    assert_eq!(e.path, "test.bad.colour/components.Hull");
    assert!(e.line_col.is_some());
}

#[test]
fn duplicate_component_is_error() {
    let reg = registry();
    let err = parse_object(
        &reg,
        "unit",
        Path::new("x.unit.ron"),
        r#"Unit(id: "test.x", components: { "Hull": (hp: 1), "Hull": (hp: 2) })"#,
    )
    .expect_err("duplicate component rejected");
    assert!(err.message.contains("duplicate component 'Hull'"), "{err}");
}

#[test]
fn child_replaces_parent_component_wholesale() {
    let resolved = resolve(&[
        r#"Unit(id: "t.parent", components: { "Hull": (hp: 100), "Speed": (kmh: 10) })"#,
        r#"Unit(id: "t.child", inherits: Some("t.parent"), components: { "Hull": (hp: 200) })"#,
    ])
    .expect("resolves");
    let reg = registry();
    let child = concrete(&resolved, "t.child");
    assert_eq!(child.get::<TestHull>(&reg), Some(&TestHull { hp: 200 }));
    assert_eq!(child.get::<TestSpeed>(&reg), Some(&kmh(10)));
    let parent = concrete(&resolved, "t.parent");
    assert_eq!(parent.get::<TestHull>(&reg), Some(&TestHull { hp: 100 }));
}

#[test]
fn remove_drops_inherited_component() {
    let resolved = resolve(&[
        r#"Unit(id: "t.parent", components: { "Hull": (hp: 100), "Speed": (kmh: 10) })"#,
        r#"Unit(id: "t.child", inherits: Some("t.parent"), remove: ["Speed"])"#,
    ])
    .expect("resolves");
    let child = concrete(&resolved, "t.child");
    assert!(child.has("Hull"));
    assert!(!child.has("Speed"));
    assert_eq!(child.get::<TestSpeed>(&registry()), None);
}

#[test]
fn three_level_chain_resolves() {
    let resolved = resolve(&[
        r#"Unit(id: "t.c", inherits: Some("t.b"), components: { "Speed": (kmh: 30) })"#,
        r#"Unit(id: "t.a", name: "Alpha", components: { "Hull": (hp: 1), "Speed": (kmh: 10) })"#,
        r#"Unit(id: "t.b", inherits: Some("t.a"), components: { "Hull": (hp: 2) })"#,
    ])
    .expect("resolves");
    let reg = registry();
    let c = concrete(&resolved, "t.c");
    assert_eq!(c.get::<TestHull>(&reg), Some(&TestHull { hp: 2 }));
    assert_eq!(c.get::<TestSpeed>(&reg), Some(&kmh(30)));
    assert_eq!(c.name, "Alpha");
    let b = concrete(&resolved, "t.b");
    assert_eq!(b.get::<TestSpeed>(&reg), Some(&kmh(10)));
    let ids: Vec<&str> = resolved.concrete.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["t.a", "t.b", "t.c"]);
}

#[test]
fn name_falls_back_to_id() {
    let resolved = resolve(&[r#"Unit(id: "t.nameless")"#]).expect("resolves");
    assert_eq!(concrete(&resolved, "t.nameless").name, "t.nameless");
}

#[test]
fn abstract_objects_are_not_output() {
    let resolved = resolve(&[
        r#"Unit(id: "t.template", abstract: true, components: { "Hull": (hp: 5) })"#,
        r#"Unit(id: "t.real", inherits: Some("t.template"))"#,
    ])
    .expect("resolves");
    let ids: Vec<&str> = resolved.concrete.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["t.real"]);
    assert!(resolved.abstract_ids.contains("t.template"));
    assert_eq!(resolved.abstract_ids.len(), 1);
    assert!(concrete(&resolved, "t.real").has("Hull"));
}

#[test]
fn tags_are_unioned_sorted() {
    let resolved = resolve(&[
        r#"Unit(id: "t.base", abstract: true, tags: ["vehicle", "armored"])"#,
        r#"Unit(id: "t.mid", abstract: true, inherits: Some("t.base"), tags: ["tracked", "armored"])"#,
        r#"Unit(id: "t.leaf", inherits: Some("t.mid"), tags: ["command", "vehicle"])"#,
    ])
    .expect("resolves");
    assert_eq!(
        concrete(&resolved, "t.leaf").tags,
        ["armored", "command", "tracked", "vehicle"]
    );
}

#[test]
fn inheritance_cycle_is_error() {
    let errs = resolve(&[
        r#"Unit(id: "t.a", inherits: Some("t.b"))"#,
        r#"Unit(id: "t.b", inherits: Some("t.a"))"#,
    ])
    .expect_err("cycle rejected");
    assert_eq!(errs.0.len(), 1, "{errs}");
    let msg = &errs.0[0].message;
    assert!(msg.contains("cycle"), "{msg}");
    assert!(msg.contains("t.a") && msg.contains("t.b"), "{msg}");
}

#[test]
fn missing_parent_is_error() {
    let errs = resolve(&[r#"Unit(id: "t.orphan", inherits: Some("t.ghost"))"#])
        .expect_err("missing parent rejected");
    assert_eq!(errs.0.len(), 1, "{errs}");
    let e = &errs.0[0];
    assert!(e.message.contains("t.ghost"), "{e}");
    assert_eq!(e.path, "t.orphan/inherits");
    assert!(e.file.ends_with("inline.unit.ron"));
}

#[test]
fn removing_absent_component_is_error() {
    let errs = resolve(&[
        r#"Unit(id: "t.parent", components: { "Hull": (hp: 100) })"#,
        r#"Unit(id: "t.child", inherits: Some("t.parent"), remove: ["Speed"])"#,
    ])
    .expect_err("removing an absent component rejected");
    assert_eq!(errs.0.len(), 1, "{errs}");
    let e = &errs.0[0];
    assert!(e.message.contains("Speed"), "{e}");
    assert_eq!(e.path, "t.child/remove");
}

#[test]
fn remove_unknown_component_is_error_with_line() {
    let reg = registry();
    let text = "Unit(\n    id: \"t.child\",\n    remove: [\n        \"Speed\",\n        \"Armour\",\n    ],\n)";
    let err = parse_object(&reg, "unit", Path::new("r.unit.ron"), text)
        .expect_err("unknown component in remove rejected");
    assert!(err.message.contains("unknown component 'Armour'"), "{err}");
    assert_eq!(err.line_col.map(|(line, _)| line), Some(5), "{err}");
    assert_eq!(err.path, "t.child/remove");
    assert!(err.to_string().starts_with("r.unit.ron:5:"), "{err}");
}

#[test]
fn parse_object_rejects_unregistered_kind() {
    let reg = registry();
    let err = parse_object(
        &reg,
        "building",
        Path::new("x.building.ron"),
        r#"(id: "t.x")"#,
    )
    .expect_err("unregistered kind rejected");
    assert!(
        err.message.contains("unknown object kind 'building'"),
        "{err}"
    );
    assert!(err.file.ends_with("x.building.ron"));
}

#[test]
fn duplicate_id_is_error() {
    let errs = resolve(&[r#"Unit(id: "t.same")"#, r#"Unit(id: "t.same")"#])
        .expect_err("duplicate rejected");
    assert!(errs.0[0].message.contains("duplicate"), "{errs}");
}

#[test]
fn errors_display_one_per_line() {
    let errs = resolve(&[
        r#"Unit(id: "t.x", inherits: Some("t.ghost1"))"#,
        r#"Unit(id: "t.y", inherits: Some("t.ghost2"))"#,
    ])
    .expect_err("both missing");
    let shown = errs.to_string();
    assert_eq!(shown.lines().count(), 2, "{shown}");
    assert!(
        shown.starts_with("inline.unit.ron: t.x/inherits: "),
        "{shown}"
    );
}

#[test]
fn registry_names_and_fingerprint() {
    let reg = registry();
    assert_eq!(reg.component_name::<TestHull>(), Some("Hull"));
    assert_eq!(reg.component_name::<TestSpeed>(), Some("Speed"));
    assert_eq!(reg.fingerprint(), registry().fingerprint());

    let mut other = DefRegistry::default();
    other.register_component::<TestHull>("Hull");
    other.register_object_kind("unit");
    assert_eq!(other.component_name::<TestSpeed>(), None);
    assert_ne!(other.fingerprint(), reg.fingerprint());
}

#[test]
#[should_panic(expected = "Hull")]
fn duplicate_component_name_panics() {
    let mut reg = registry();
    reg.register_component::<TestSpeed>("Hull");
}

#[test]
#[should_panic(expected = "TestHull")]
fn duplicate_component_type_panics() {
    let mut reg = registry();
    reg.register_component::<TestHull>("Hull2");
}
