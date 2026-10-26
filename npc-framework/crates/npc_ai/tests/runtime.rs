//! Behavior-tree IR, builder and think-pass semantics.

use bevy_ecs::prelude::*;
use npc_ai::{
    Abort, ActionId, ActiveLeaf, AiRegistry, AiTrace, AiTraceEvent, AiTraceKind, Brain, BrainState,
    BrainStats, CondId, NodeKind, ParamValue, Status, StopReason, ThinkReason, ThinkRequest,
    TreeBuilder, TreeDef, think,
};

// ---------------------------------------------------------------------------
// Test actions, conditions and scorers, referenced by name. `world_with`
// registers them and freezes the registry, which assigns ids sorted by name:
// instant_fail = 0, instant_ok = 1, run = 2.
// ---------------------------------------------------------------------------

const OK: &str = "instant_ok";
const FAIL: &str = "instant_fail";
const RUN: &str = "run";
const RUN_ID: ActionId = ActionId(2);
const FLAG: &str = "flag";
const FLAG_ID: CondId = CondId(0);
const S0: &str = "s0";
const S1: &str = "s1";
const S2: &str = "s2";

/// Action component inserted by `run`; holds the leaf's param (a name char).
#[derive(Component)]
struct TestRun(i32);

/// Every stop of a `run` action, as `"<name>:<reason>"`.
#[derive(Resource, Default)]
struct Log(Vec<String>);

#[derive(Resource, Default)]
struct Flag(bool);

#[derive(Resource, Default)]
struct Scores([i32; 3]);

fn instant_ok(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Success
}

fn instant_fail(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Failure
}

fn noop_stop(_: &mut World, _: Entity, _: StopReason) {}

fn run_start(world: &mut World, e: Entity, param: &ParamValue) -> Status {
    world.entity_mut(e).insert(TestRun(
        param.as_int().expect("run leaves carry an Int param"),
    ));
    Status::Running
}

fn run_stop(world: &mut World, e: Entity, reason: StopReason) {
    let Some(run) = world.entity_mut(e).take::<TestRun>() else {
        return; // idempotent
    };
    let name = run.0 as u8 as char;
    world
        .resource_mut::<Log>()
        .0
        .push(format!("{name}:{reason:?}"));
}

fn flag(world: &World, _: Entity) -> bool {
    world.resource::<Flag>().0
}

fn s0(world: &World, _: Entity) -> i32 {
    world.resource::<Scores>().0[0]
}
fn s1(world: &World, _: Entity) -> i32 {
    world.resource::<Scores>().0[1]
}
fn s2(world: &World, _: Entity) -> i32 {
    world.resource::<Scores>().0[2]
}

/// `run` leaf whose name (and log name) is the single char `c`.
fn run(b: &mut TreeBuilder, c: char) {
    b.leaf(&c.to_string(), RUN, c as i32);
}

fn world_with(tree: TreeDef) -> (World, Entity) {
    let mut reg = AiRegistry::default();
    reg.add_action(OK, instant_ok, noop_stop);
    reg.add_action(FAIL, instant_fail, noop_stop);
    reg.add_action(RUN, run_start, run_stop);
    reg.add_cond(FLAG, flag);
    reg.add_scorer(S0, s0);
    reg.add_scorer(S1, s1);
    reg.add_scorer(S2, s2);
    let name = tree.name.clone();
    reg.add_tree(tree);
    reg.freeze();
    assert_eq!(reg.action_id(RUN), Some(RUN_ID));
    assert_eq!(reg.cond_id(FLAG), Some(FLAG_ID));
    let tree_id = reg.tree_id(&name).expect("tree registered");

    let mut world = World::new();
    world.insert_resource(reg);
    world.init_resource::<Log>();
    world.init_resource::<Flag>();
    world.init_resource::<Scores>();
    let e = world.spawn(Brain { tree: tree_id }).id();
    (world, e)
}

fn tree(f: impl FnOnce(&mut TreeBuilder)) -> TreeDef {
    let mut b = TreeBuilder::new("test");
    f(&mut b);
    b.build()
}

fn finish(world: &mut World, e: Entity, status: Status) {
    world
        .get_mut::<ActiveLeaf>(e)
        .expect("no active leaf")
        .status = status;
}

fn think1(world: &mut World, e: Entity) {
    think(world, &[e]);
}

/// Name of the active leaf, if any.
fn active(world: &World, e: Entity) -> Option<String> {
    let leaf = world.get::<ActiveLeaf>(e)?;
    let tree_id = world.get::<Brain>(e).unwrap().tree;
    Some(
        world.resource::<AiRegistry>().tree(tree_id).nodes[leaf.node as usize]
            .name
            .clone(),
    )
}

fn log(world: &World) -> Vec<String> {
    world.resource::<Log>().0.clone()
}

fn stats(world: &World, e: Entity) -> BrainStats {
    *world.get::<BrainStats>(e).unwrap()
}

fn state(world: &World, e: Entity) -> BrainState {
    world.get::<BrainState>(e).unwrap().clone()
}

/// `Sel[Guard(flag, abort){run "a"}, run "b"]`
fn guarded(abort: Abort) -> TreeDef {
    tree(|b| {
        b.selector("Root", |b| {
            b.guard("G", FLAG, abort, |b| run(b, 'a'));
            run(b, 'b');
        });
    })
}

// ---------------------------------------------------------------------------
// Builder and IR
// ---------------------------------------------------------------------------

#[test]
fn builder_emits_preorder_ir() {
    let t = tree(|b| {
        b.selector("Root", |b| {
            b.guard("G", FLAG, Abort::LowerPriority, |b| {
                b.leaf("A", OK, 0);
            });
            b.leaf("B", OK, 0);
        });
    });
    let names: Vec<&str> = t.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Root", "G", "A", "B"]);
    assert_eq!(t.name, "test");
    assert_eq!(t.nodes[0].children, vec![1, 3]);
    assert_eq!(t.nodes[1].children, vec![2]);
    assert!(t.nodes[2].children.is_empty());
    assert!(t.nodes[3].children.is_empty());
    assert_eq!(t.nodes[0].kind, NodeKind::Selector);
    assert_eq!(
        t.nodes[1].kind,
        NodeKind::Guard {
            cond: FLAG.into(),
            abort: Abort::LowerPriority
        }
    );
    assert_eq!(
        t.nodes[2].kind,
        NodeKind::Leaf {
            action: OK.into(),
            param: ParamValue::Int(0)
        }
    );
    assert_eq!(t.path_string(&[0, 1, 2]), "Root > G > A");
}

#[test]
#[should_panic(expected = "must have exactly one child")]
fn builder_panics_on_guard_without_child() {
    tree(|b| {
        b.selector("Root", |b| {
            b.guard("G", FLAG, Abort::None, |_| {});
            b.leaf("B", OK, 0);
        });
    });
}

#[test]
#[should_panic(expected = "is empty")]
fn builder_panics_on_empty_tree() {
    tree(|_| {});
}

#[test]
#[should_panic(expected = "one child per scorer")]
fn builder_panics_on_utility_child_count_mismatch() {
    tree(|b| {
        b.utility("U", [S0, S1], |b| {
            b.leaf("A", OK, 0);
        });
    });
}

#[test]
fn content_hash_stable_and_sensitive() {
    let make = |param: i32| {
        tree(|b| {
            b.sequence("Root", |b| {
                b.leaf("A", OK, 1);
                b.leaf("B", RUN, param);
            });
        })
    };
    let h1 = make(7).content_hash();
    let h2 = make(7).content_hash();
    assert_eq!(h1, h2);
    assert_ne!(h1, make(8).content_hash());
    // Pinned: the encoding must not drift across machines or versions.
    // P3 (stable-name registration) re-pinned it once: leaves hash their
    // action *name* and a typed `ParamValue` instead of a registration-order
    // id and an `i32` (was 10_819_025_673_482_893_471).
    assert_eq!(h1, 3_263_589_783_082_370_103);
}

// ---------------------------------------------------------------------------
// Think pass
// ---------------------------------------------------------------------------

#[test]
fn sequence_runs_children_in_order() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| {
            b.leaf("ok", OK, 0);
            run(b, 'b');
            run(b, 'c');
        });
    }));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
    assert!(log(&w).is_empty());

    finish(&mut w, e, Status::Success);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("c"));
    assert_eq!(log(&w), ["b:Finished"]);
    assert_eq!(stats(&w, e).leaf_starts, 2);
}

#[test]
fn mem_resized_to_tree_len() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| {
            run(b, 'a');
            run(b, 'b');
        });
    }));
    think1(&mut w, e);
    assert_eq!(state(&w, e).mem.len(), 3);
}

#[test]
fn running_leaf_is_not_restarted() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| run(b, 'a'));
    }));
    think1(&mut w, e);
    think1(&mut w, e);
    // Even a ThinkRequest must not restart a leaf that is still running.
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(stats(&w, e).leaf_starts, 1);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
    assert!(log(&w).is_empty());
}

#[test]
fn selector_falls_back_on_failure() {
    let (mut w, e) = world_with(tree(|b| {
        b.selector("Sel", |b| {
            b.leaf("fail", FAIL, 0);
            run(b, 'x');
        });
    }));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("x"));
    assert_eq!(state(&w, e).running_path, vec![0, 2]);
}

#[test]
fn abort_guard_preempts_running_lower_branch() {
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));

    w.resource_mut::<Flag>().0 = true;
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(log(&w), ["b:Aborted"]);
    assert_eq!(stats(&w, e).aborts, 1);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
    assert_eq!(state(&w, e).running_path, vec![0, 1, 2]);
    assert!(w.get::<TestRun>(e).is_some_and(|r| r.0 == 'a' as i32));
}

/// A leaf that already completed but whose result was not consumed yet is
/// still stopped with `Aborted` when a reactive guard preempts its branch:
/// the branch as a whole was interrupted, so all of its claims must go.
#[test]
fn abort_of_completed_leaf_passes_aborted() {
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));

    finish(&mut w, e, Status::Success);
    w.resource_mut::<Flag>().0 = true;
    think1(&mut w, e);
    assert_eq!(log(&w), ["b:Aborted"]);
    assert_eq!(stats(&w, e).aborts, 1);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
}

#[test]
fn non_abort_guard_does_not_preempt() {
    let (mut w, e) = world_with(guarded(Abort::None));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));

    w.resource_mut::<Flag>().0 = true;
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
    assert!(log(&w).is_empty());
    assert_eq!(stats(&w, e).aborts, 0);
}

#[test]
fn no_request_no_preemption() {
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));

    w.resource_mut::<Flag>().0 = true;
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
    assert!(log(&w).is_empty());
    assert_eq!(stats(&w, e).aborts, 0);
}

#[test]
fn abort_resets_subtree_memory() {
    // Sel[Guard(flag, LP){run a}, Seq[ok, run b, run c]]
    let (mut w, e) = world_with(tree(|b| {
        b.selector("Root", |b| {
            b.guard("G", FLAG, Abort::LowerPriority, |b| run(b, 'a'));
            b.sequence("Seq", |b| {
                b.leaf("ok", OK, 0);
                run(b, 'b');
                run(b, 'c');
            });
        });
    }));
    think1(&mut w, e);
    finish(&mut w, e, Status::Success);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("c"));
    assert_eq!(state(&w, e).mem[3], 2, "sequence stores its current child");

    w.resource_mut::<Flag>().0 = true;
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
    assert_eq!(log(&w), ["b:Finished", "c:Aborted"]);
    assert!(state(&w, e).mem[3..].iter().all(|&m| m == 0));

    // Guard branch completes; the root finishes and the path clears.
    w.resource_mut::<Flag>().0 = false;
    finish(&mut w, e, Status::Success);
    think1(&mut w, e);
    assert!(state(&w, e).running_path.is_empty());

    // Next think: the aborted sequence restarts fresh at "b", not "c".
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
}

#[test]
fn failed_running_branch_falls_through_in_same_call() {
    let (mut w, e) = world_with(tree(|b| {
        b.selector("Sel", |b| {
            run(b, 'a');
            run(b, 'b');
        });
    }));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
    finish(&mut w, e, Status::Failure);
    think1(&mut w, e);
    assert_eq!(log(&w), ["a:Finished"]);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
}

#[test]
fn utility_picks_highest_score_ties_lower_index() {
    let (mut w, e) = world_with(tree(|b| {
        b.utility("U", [S0, S1, S2], |b| {
            run(b, 'p');
            run(b, 'q');
            run(b, 'r');
        });
    }));
    w.resource_mut::<Scores>().0 = [5, 9, 9];
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("q"));

    // On the running path the running child continues even if scores change.
    w.resource_mut::<Scores>().0 = [5, 9, 99];
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("q"));
    assert_eq!(stats(&w, e).leaf_starts, 1);
}

#[test]
fn think_request_removed() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| run(b, 'a'));
    }));
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert!(w.get::<ThinkRequest>(e).is_none());
    // Also removed when the agent thinks with a running leaf.
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert!(w.get::<ThinkRequest>(e).is_none());
}

#[test]
fn root_completion_clears_path() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| run(b, 'a'));
    }));
    think1(&mut w, e);
    assert_eq!(state(&w, e).running_path, vec![0, 1]);
    finish(&mut w, e, Status::Success);
    think1(&mut w, e);
    assert!(state(&w, e).running_path.is_empty());
    assert!(w.get::<ActiveLeaf>(e).is_none());
    assert_eq!(log(&w), ["a:Finished"]);
    // The root is not re-run in the same call.
    assert_eq!(stats(&w, e).leaf_starts, 1);
}

#[test]
fn instant_leaves_insert_no_active_leaf() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| {
            b.leaf("ok1", OK, 0);
            b.leaf("ok2", OK, 0);
        });
    }));
    think1(&mut w, e);
    assert!(w.get::<ActiveLeaf>(e).is_none());
    assert!(state(&w, e).running_path.is_empty());
    assert_eq!(stats(&w, e).leaf_starts, 0);
}

#[test]
fn agent_listed_twice_thinks_once() {
    let (mut w, e) = world_with(tree(|b| {
        b.sequence("Seq", |b| run(b, 'a'));
    }));
    think1(&mut w, e);
    finish(&mut w, e, Status::Success);
    // First visit consumes "a" and completes the root; the second must not
    // evaluate the tree again (which would start "a" anew).
    think(&mut w, &[e, e]);
    assert!(w.get::<ActiveLeaf>(e).is_none());
    assert_eq!(stats(&w, e).leaf_starts, 1);
}

#[test]
#[should_panic(expected = "exactly one root")]
fn builder_panics_on_two_roots() {
    tree(|b| {
        b.leaf("A", OK, 0);
        b.leaf("B", OK, 0);
    });
}

// ---------------------------------------------------------------------------
// Review cleanup 1: a failed running child re-opens higher siblings
// ---------------------------------------------------------------------------

/// `Sel[Guard "NightSleep"(flag, None){run n}, run w ("Work"), run x ("Wander")]`.
/// Work runs while the flag is false; then night falls (flag true, a
/// non-reactive guard, so Work is not aborted) and Work fails. The selector
/// must reconsider NightSleep (a higher sibling) instead of falling through
/// to Wander.
#[test]
fn selector_reconsiders_higher_siblings_after_running_child_fails() {
    let (mut w, e) = world_with(tree(|b| {
        b.selector("Villager", |b| {
            b.guard("NightSleep", FLAG, Abort::None, |b| run(b, 'n'));
            run(b, 'w');
            run(b, 'x');
        });
    }));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("w"));

    w.resource_mut::<Flag>().0 = true;
    finish(&mut w, e, Status::Failure);
    think1(&mut w, e);
    assert_eq!(log(&w), ["w:Finished"]);
    assert_eq!(
        active(&w, e).as_deref(),
        Some("n"),
        "chose Wander over NightSleep"
    );
    assert_eq!(state(&w, e).running_path, vec![0, 1, 2]);
}

/// The failed child itself is skipped for the rest of the call, even when it
/// would start again (no infinite retry within one think).
#[test]
fn failed_running_child_is_not_retried_in_same_call() {
    let (mut w, e) = world_with(tree(|b| {
        b.selector("Sel", |b| {
            b.guard("G", FLAG, Abort::None, |b| run(b, 'a'));
            run(b, 'b');
            run(b, 'c');
        });
    }));
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("b"));
    finish(&mut w, e, Status::Failure);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("c"));
    assert_eq!(stats(&w, e).leaf_starts, 2);
}

// ---------------------------------------------------------------------------
// AiTrace
// ---------------------------------------------------------------------------

fn kinds(world: &World) -> Vec<AiTraceKind> {
    world
        .resource::<AiTrace>()
        .events
        .iter()
        .map(|ev| ev.kind.clone())
        .collect()
}

#[test]
fn trace_records_leaf_start_stop_abort() {
    // Root = 0, G = 1, a = 2, b = 3.
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    w.insert_resource(AiTrace {
        enabled: true,
        ..Default::default()
    });
    think1(&mut w, e);
    assert_eq!(
        kinds(&w),
        [
            AiTraceKind::ThinkStart {
                reason: ThinkReason::NoLeaf
            },
            AiTraceKind::LeafStart {
                node: 3,
                action: RUN_ID,
                status: Status::Running
            },
        ]
    );
    assert!(
        w.resource::<AiTrace>()
            .events
            .iter()
            .all(|ev| ev.agent == e)
    );

    w.resource_mut::<AiTrace>().events.clear();
    w.resource_mut::<Flag>().0 = true;
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(
        kinds(&w),
        [
            AiTraceKind::ThinkStart {
                reason: ThinkReason::Request
            },
            AiTraceKind::Abort {
                guard: 1,
                aborted: vec![0, 3]
            },
            AiTraceKind::LeafStop {
                node: 3,
                action: RUN_ID,
                reason: StopReason::Aborted
            },
            AiTraceKind::LeafStart {
                node: 2,
                action: RUN_ID,
                status: Status::Running
            },
        ]
    );

    // A finished leaf: ThinkStart(LeafDone), LeafStop(Finished), RootDone.
    w.resource_mut::<AiTrace>().events.clear();
    finish(&mut w, e, Status::Success);
    think1(&mut w, e);
    assert_eq!(
        kinds(&w),
        [
            AiTraceKind::ThinkStart {
                reason: ThinkReason::LeafDone
            },
            AiTraceKind::LeafStop {
                node: 2,
                action: RUN_ID,
                reason: StopReason::Finished
            },
            AiTraceKind::RootDone {
                status: Status::Success
            },
        ]
    );
}

#[test]
fn trace_cond_eval_only_when_verbose() {
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    w.insert_resource(AiTrace {
        enabled: true,
        verbose: false,
        events: Vec::new(),
    });
    think1(&mut w, e);
    assert!(
        !kinds(&w)
            .iter()
            .any(|k| matches!(k, AiTraceKind::CondEval { .. }))
    );

    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    w.insert_resource(AiTrace {
        enabled: true,
        verbose: true,
        events: Vec::new(),
    });
    think1(&mut w, e);
    assert!(kinds(&w).contains(&AiTraceKind::CondEval {
        node: 1,
        cond: FLAG_ID,
        result: false
    }));
}

#[test]
fn no_trace_resource_no_events() {
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    think1(&mut w, e);
    w.resource_mut::<Flag>().0 = true;
    w.entity_mut(e).insert(ThinkRequest);
    think1(&mut w, e);
    assert_eq!(active(&w, e).as_deref(), Some("a"));
    assert!(w.get_resource::<AiTrace>().is_none());

    // Present but disabled: still nothing.
    let (mut w, e) = world_with(guarded(Abort::LowerPriority));
    w.init_resource::<AiTrace>();
    think1(&mut w, e);
    assert!(w.resource::<AiTrace>().events.is_empty());
    let _: Option<&AiTraceEvent> = w.resource::<AiTrace>().events.first();
}

// ---------------------------------------------------------------------------
// Stable-name registration (plan Task P3)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(
    expected = "references unknown action `Teleport`; known actions: [instant_fail, instant_ok, run]"
)]
fn unknown_action_name_panics_helpfully() {
    world_with(tree(|b| {
        b.sequence("Root", |b| {
            b.leaf("A", OK, ());
            b.leaf("B", "Teleport", ());
        });
    }));
}

#[test]
#[should_panic(expected = "references unknown condition `nope`; known conditions: [flag]")]
fn unknown_condition_name_panics_helpfully() {
    world_with(tree(|b| {
        b.guard("G", "nope", Abort::None, |b| {
            b.leaf("A", OK, ());
        });
    }));
}

#[test]
fn ids_are_sorted_by_name_whatever_the_registration_order() {
    let build = |reverse: bool| {
        let mut reg = AiRegistry::default();
        let mut actions: Vec<(&'static str, npc_ai::StartFn)> =
            vec![(OK, instant_ok), (FAIL, instant_fail), (RUN, run_start)];
        if reverse {
            actions.reverse();
        }
        for (name, start) in actions {
            reg.add_action(name, start, noop_stop);
        }
        reg.add_cond(FLAG, flag);
        reg.add_tree(guarded(Abort::None));
        reg.freeze();
        (
            reg.actions.iter().map(|a| a.name).collect::<Vec<_>>(),
            reg.tree(reg.tree_id("test").unwrap()).clone(),
        )
    };
    let (a, ta) = build(false);
    let (b, tb) = build(true);
    assert_eq!(a, ["instant_fail", "instant_ok", "run"]);
    assert_eq!(a, b);
    assert_eq!(ta, tb, "resolved trees (with ids) are identical");
    assert_eq!(ta.content_hash(), tb.content_hash());
}

#[test]
#[should_panic(expected = "already frozen")]
fn registering_after_freeze_panics() {
    let mut reg = AiRegistry::default();
    reg.freeze();
    reg.add_action(OK, instant_ok, noop_stop);
}

#[test]
#[should_panic(expected = "does not fit in u16")]
fn too_many_actions_panic_instead_of_wrapping() {
    let mut reg = AiRegistry::default();
    for i in 0..=u16::MAX as usize + 1 {
        let name: &'static str = Box::leak(format!("a{i:06}").into_boxed_str());
        reg.add_action(name, instant_ok, noop_stop);
    }
    reg.freeze();
}
