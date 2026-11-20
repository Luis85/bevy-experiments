//! `AiRegistry::replace_trees`: a frozen registry takes a new tree set at an
//! explicit call; nothing else about registration or freezing changes.
use bevy_ecs::prelude::*;
use npc_ai::{
    Abort, AiRegistry, NameRef, NodeDef, NodeKind, ParamValue, Status, StopReason, TreeDef,
};

fn start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Success
}
fn stop(_: &mut World, _: Entity, _: StopReason) {}
fn yes(_: &World, _: Entity) -> bool {
    true
}
fn no(_: &World, _: Entity) -> bool {
    false
}

fn tree(name: &str, cond: &str, action: &str) -> TreeDef {
    TreeDef {
        name: name.into(),
        nodes: vec![
            NodeDef {
                name: "guard".into(),
                kind: NodeKind::Guard {
                    cond: NameRef::new(cond),
                    abort: Abort::None,
                },
                children: vec![1],
            },
            NodeDef {
                name: "leaf".into(),
                kind: NodeKind::Leaf {
                    action: NameRef::new(action),
                    param: ParamValue::None,
                },
                children: vec![],
            },
        ],
    }
}

fn frozen() -> AiRegistry {
    let mut registry = AiRegistry::default();
    registry.add_action("act", start, stop);
    registry.add_cond("b_cond", yes);
    registry.add_tree(tree("t1", "b_cond", "act"));
    registry.freeze();
    registry
}

#[test]
fn a_frozen_registry_takes_new_trees_and_conditions() {
    let mut registry = frozen();
    let before = registry
        .tree(registry.tree_id("t1").expect("t1"))
        .content_hash();
    registry
        .replace_trees(
            vec![tree("t2", "a_new", "act"), tree("t1", "b_cond", "act")],
            vec![("a_new", no)],
        )
        .expect("replaced");
    // Conditions are sorted again: the new one comes first.
    assert_eq!(registry.cond_id("a_new").map(|id| id.0), Some(0));
    assert_eq!(registry.cond_id("b_cond").map(|id| id.0), Some(1));
    let t2 = registry.tree(registry.tree_id("t2").expect("t2"));
    match &t2.nodes[0].kind {
        NodeKind::Guard { cond, .. } => assert_eq!(cond.id.map(|id| id.0), Some(0)),
        other => panic!("{other:?}"),
    }
    let t1 = registry.tree(registry.tree_id("t1").expect("t1"));
    match &t1.nodes[0].kind {
        NodeKind::Guard { cond, .. } => assert_eq!(cond.id.map(|id| id.0), Some(1)),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        t1.content_hash(),
        before,
        "a tree's content hash covers names only"
    );
}

#[test]
fn an_unknown_action_is_refused_and_nothing_changes() {
    let mut registry = frozen();
    let error = registry
        .replace_trees(vec![tree("t1", "b_cond", "dance")], vec![("a_new", no)])
        .expect_err("refused");
    assert!(error.contains("unknown action `dance`"), "{error}");
    assert!(registry.cond_id("a_new").is_none());
    assert!(registry.tree_id("t1").is_some());
    let error = registry
        .replace_trees(
            vec![tree("t1", "b_cond", "act"), tree("t1", "b_cond", "act")],
            vec![],
        )
        .expect_err("refused");
    assert!(error.contains("given twice"), "{error}");
    let mut open = AiRegistry::default();
    assert!(
        open.replace_trees(vec![], vec![]).is_err(),
        "only a frozen registry"
    );
}
