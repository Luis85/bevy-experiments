---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; spec-gap resolutions accepted as R148); M1 final review (final-review-m1.md, focus 2, M4, M5, M11–M14); research: architecture-best-practices.md N14, N15, C21; baseline-recommendations.md C7"
informed: "M2 prelude implementers (Tasks 2, 13, 15)"
---

# Unit behaviour and the entity graph: order queue, activity stack and `SimId` relationships

## Context and Problem Statement

M1's `MoveOrder` v2 (`"core.move_order"`) holds a closed `Order` enum (`Move`, `Rotate`, `Follow`), which R69
declared an interim until C7 (spec §9.2). M2 brings attack, attack-move, deploy, enter and tow orders from different
features, and composite behaviour (move into range, then fire; drive to a gun, then hitch it) that a closed enum in
core cannot hold without core depending on every feature. Squads, garrisons, transports, towing and crews also need
links between units. Bevy 0.20 offers relationships (`ChildOf`, `#[relationship]`) built on `Entity`, with
`linked_spawn` cascades, but `Entity` may not appear in hashed or saved state (spec §5.4), `ChildOf` carries transform
semantics for the client and plugins, and the R73 check already forbids `ChildOf` and `Children` in the sim world.
Saves taken under M1 (`m1_testbed.save`, Task 2) must load and continue with the same poses.

## Decision Drivers

* Features add order kinds and behaviours without core knowing them (ADR-0001).
* Every order and link is Hashed and saved by name and `SimId`, never by dense id or `Entity` (ADR-0004, ADR-0006).
* Order-sensitive work runs in `SimIndex` order (ADR-0005), with state changes as plain writes, not archetype moves
  (spec §5.7 scale).
* M1 saves migrate, and M1 behaviour is kept exactly: the pose trace of Task 2 is the oracle.
* Despawns go through the funnel (ADR-0005); no cascade may bypass it.

## Considered Options

* In-house order queue, activity stack and `SimId` relationships
* Keep extending the closed `Order` enum
* A behaviour-tree or state-machine crate
* Bevy's built-in relationships

## Decision Outcome

Chosen option: "In-house order queue, activity stack and `SimId` relationships", because it is the only
option that lets features register behaviour while every piece of state stays Hashed, named and ordered by `SimId`.

- **Order queue.** `OrderQueue { orders: Vec<QueuedOrder> }` replaces M1's closed `Order` enum without a deprecation
  period, as R69 declared it interim.
  - `QueuedOrder { kind: OrderKindId, target: OrderTarget, param: Vec<u8> }`, with `OrderTarget ∈ {None,
    Point(FxVec2), Unit(SimId), Facing(Angle), UnitOffset { unit, offset }}`. `param` holds the kind's own parameter
    as postcard bytes, at most `MAX_ORDER_PARAM_BYTES` (64), a code constant like `MAX_WAYPOINTS` because it too
    bounds the save decoder. The kind's command handler validates the parameter at handler entry (R104);
    `give_order` rejects a longer `param` with `RejectReason::Feature("core.param_too_large")`, and the codec rejects
    one on load (`LoadError::Codec`).
  - Order kinds are registered with `SimBuilder::register_order(name, behaviour)` (table `"orders"`; a duplicate
    panics naming both features). The dense id is saved, remapped through the saved table on load, and hashed by
    name.
  - It keeps the Hashed state name `"core.move_order"` (renaming would need a codec rename mechanism; the name is only
    a key) and becomes codec v3, a `Codec::custom`, migrated from the frozen `units::v2::MoveOrder` with
    `Codec::migrate_with` (ADR-0006 refinement): `Move` → `"core.move"` `Point`, `Rotate` → `"core.rotate"`
    `Facing`, `Follow` → `"core.follow"` `UnitOffset`.
  - At most `MAX_WAYPOINTS` (32) orders, every kind counted (`core.queue_full`). It stays a code constant: it bounds
    the save decoder, so a content value could make valid saves undecodable.
  - `give_order(world, entity, order, queue)` appends when `queue` is set; otherwise it cancels the current
    activities (below), replaces the orders and begins the new one in the same call, in `Phase::Input`, as M1 did.
    `stop_unit` cancels every frame, clears the orders and resets the path (`PathFollow::reset`, the one copy, M12).
- **Activity stack.** `ActivityStack { frames: Vec<Activity> }` (Hashed `"core.activity_stack"` v1; top = last; at
  most `MAX_ACTIVITY_DEPTH` = 8 frames).
  - `Activity { kind: ActivityKindId, target: OrderTarget, param: Vec<u8>, state: Active | Canceling }`; activity
    kinds are registered with `SimBuilder::register_activity(name, behaviour)` (table `"activities"`). Like
    `OrderKindId`, the dense `ActivityKindId` is saved, remapped through the saved `"activities"` table on load, and
    hashed by name; `param` has the same `MAX_ORDER_PARAM_BYTES` bound.
  - An order becomes current by pushing its root activity (`OrderBehaviour::begin`). `run_activities` replaces M1's
    `advance_orders` in `ThinkSet::Orders`' core slot, right after `recompute_stats` (ADR-0017). It visits units in
    `SimIndex` order and steps each as follows (R151):
    1. If the stack is empty and the unit has a current order, push that order's root activity.
    2. Tick the top frame (`ActivityBehaviour::tick → Continue | Done(Completed | Failed) | Push(child)`).
       - `Continue`: the unit is done for this tick.
       - `Push(child)`: push it; **the child ticks in the same tick** (back to step 2).
       - `Done` of a child frame: pop it; **its parent ticks again on the next tick**, not in this one.
       - `Done` of the root frame: pop it and the current order (journal `core.order_changed`, `current` being the
         kind's name such as `"core.move"`); the next queued order begins in the same tick (back to step 1), as M1's
         `advance_orders` moved straight on to the next order.
    3. **Bounds per unit per tick** (R153). `run_activities` begins at most `MAX_ORDERS_BEGUN_PER_TICK` (32) root
       orders per unit per tick; an order past it begins on the next tick. The bound equals the queue cap
       (`MAX_WAYPOINTS`), so a full queue of instantly finishing orders still runs in one tick, and M1's timing never
       changes. Pushes and pops inside one order's chain are bounded only by the depth cap (step 4): a parent ticks at
       most once per tick, since a child's `Done` ends the unit's turn, so a chain makes at most 8 pushes and one pop
       per tick. Cancel steps are exempt from every bound.
    4. A push past `MAX_ACTIVITY_DEPTH` (8 frames) fails the order: the frames get their cancel steps top-down and the
       order pops `"failed"`. It never panics.
    5. **Loop guard.** A debug assertion fires only on a true loop: the same activity kind pushed onto the same parent
       frame more than `MAX_REPUSH_PER_TICK` (4) times in one tick. The rules above already make that unreachable
       through the behaviour traits, so the assertion guards the implementation of `run_activities`, not feature
       code, and never fires on a valid queue.
  - **Cancelling** (R151): replacing or stopping marks each frame `Canceling` and gives it exactly one cancel step
    (`ActivityBehaviour::cancel`), top-down, child before parent, then removes it; the new order's root activity
    begins in the same tick. No frame stays `Canceling` past its cancel step, so a saved stack holds only `Active`
    frames; the state exists so that a behaviour can tell a cancel from a normal tick.
  - Core registers the orders `"core.move"`, `"core.rotate"` and `"core.follow"` and the activities `"core.drive"`
    (M1's move logic), `"core.turn"` (facing resolved when the order becomes current, R105) and `"core.follow"`
    (R106, R107). Their behaviour is M1's exactly.
  - Observability: journal `core.activity_changed { kind, event: push | pop | cancel, depth, outcome }`, counter
    `orders.activity_ticks`, span `run_activities`, debug query `core.activities`.
  - State lives in these table-stored components, so a transition is a plain write, not an archetype move. SparseSet
    markers are allowed only for rare states that need filtering (`Embarked`, later).
- **Relationships.** A relation is a type `R: Relation { NAME, ONE_TO_ONE }` registered with
  `SimBuilder::register_relation::<R>()`.
  - `RelatedTo<R>(SimId)` on the source is Hashed under `R::NAME` (v1; its codec saves the target `SimId`).
    `Related<R>` on the target is Derived (`"<R::NAME>.sources"`), its sources sorted by `SimId`, maintained by
    `RelatedTo<R>`'s component hooks.
  - Loading spawns entities in `SimId` order, so a source can load before its target; the framework therefore
    rebuilds every `Related<R>` after a load, before the feature rebuilds.
  - `relate::<R>(source, target)` replaces the source's old target; `unrelate::<R>(source)` removes it. Errors are
    `RelationError::{NoSource, NoTarget, SelfRelation, TargetTaken { target, by }}`; a one-to-one relation rejects a
    second source with `TargetTaken`.
  - When a target despawns, features apply their rule in `LifecycleSet::Death` (the crew bails out, the towed gun is
    dropped), and the despawn funnel then removes any `RelatedTo<R>` still pointing at it, in `SimId` order.
  - No `ChildOf`, no `linked_spawn`: whether passengers die or bail out is a rule, not a cascade. Ownership stays the
    `Owner(PlayerId)` component, not a relationship (spec §9.2 C7 listed ownership among the relationships): players
    are not entities.
- **M1 saves.** Core becomes version 3 (Task 13; `SIM_VERSION` 3, MCR-001): `core.move_order` migrates v2 → v3, and
  `core.activity_stack`'s `fill_missing` gives each unit with a current order that order's root activity (or an
  empty stack), so a loaded M1 unit equals a live one.
- **Hashed state that names derived layouts.** A Hashed field that stores a key into a Derived layout, like
  `PathState::Flow { key }` (a `FlowKey` names an HPA* cluster, whose layout `Path.cluster_cells` defines), is valid
  only while the rules that define that layout are unchanged. Every new key of this kind names its layout rules in its
  doc comment, and the units holding it re-plan when those rules change: on a load whose rules hash differs (R129) and
  on a value reload (ADR-0015 refinement, M1).

### Consequences

* Good, because a feature adds an order kind, its activities and its relations by registration, and core never names
  feature types.
* Good, because Shift-queue, interruption and sequential composite orders (drive to the gun, then hitch it) fall
  out of the stack. Behaviour that runs at the same time as the hull's (a turret tracking while the hull drives)
  needs a second stack (see "M2 direct-fire gaps").
* Good, because every link is a `SimId` in Hashed state, so saves, replays and the branch test cover the entity graph.
* Bad, because the closed `Order` enum goes without a deprecation period; every M1 test that built a `MoveOrder`
  is ported in Task 13.
* Bad, because `param` is untyped bytes at the queue level; each kind owns its decoding and its bounds.
* Bad, because the hash layout changes (new Hashed state, codec v3), so both goldens are re-blessed under MCR-001
  even though no pose changes.
* Neutral, because `MAX_ORDERS_BEGUN_PER_TICK` equals the queue cap, so it can never defer an order M1 would have
  begun in the same tick: a full queue of 32 already-satisfied orders still finishes in one tick (R153).

### Confirmation

* Behaviour (Task 13, `fl_sim/tests/behaviour.rs`): `orders_and_activities_are_registered`,
  `move_runs_as_a_drive_activity`, `queued_orders_begin_in_turn`, `replacing_cancels_top_down`,
  `stop_clears_queue_stack_and_path`, `feature_order_kind_runs`, `activity_depth_is_capped`,
  `activity_stack_round_trips`, `no_map_is_its_own_code`, `paths_query_answers_immobile_units`,
  `activities_query_lists_frames`, `max_command_units_lives_in_protocol`; for the stepping rules (R151),
  `pushed_child_ticks_in_the_same_tick`, `parent_resumes_on_the_next_tick`,
  `full_queue_of_instant_orders_finishes_in_one_tick`, `cancel_gets_one_step_then_new_order_begins`,
  `oversized_order_param_is_rejected`, and the loop guard's unit test `repush_guard_counts_per_parent` (R153).
* Relationships (Task 13, `fl_sim/tests/relation.rs`): `relate_maintains_sources`,
  `one_to_one_rejects_second_source`, `self_relation_is_error`, `despawned_target_strips_sources`,
  `relations_survive_save_and_load`, `relations_branch`; the R73 check keeps
  `relationships_are_not_allowed_in_the_sim_world` passing.
* M1 behaviour kept (Review Focus 1): `frozen_v2_reads_current_move_order_bytes` and
  `m1_save_continues_with_m1_pose_trace` (Task 2), `m1_save_migrates_to_order_queue` (Task 13), and
  `testbed_paths_pose_trace_matches` and the Task 11 acceptance tests passing unchanged through Task 13.
* Gate rows: `golden-policy` and `change-class` (class F with `MCR: MCR-001` and a `SIM_VERSION` bump), the
  system-order and `formats__save_codecs` snapshots (`protected`), `api`; the re-bless goes through `fl_cli rebless
  --reason MCR-001`, whose report shows only `core.activity_changed` added to the journal summary.

## Pros and Cons of the Options

### In-house order queue, activity stack and `SimId` relationships

* Good, because it follows OpenRA's activity model, which has held up across many mods (research N15).
* Good, because it is small (a few hundred lines) and every decision in it is ours to keep deterministic.
* Bad, because we maintain it, and features write `tick` and `cancel` by hand.

### Keep extending the closed `Order` enum

* Good, because it needs no new API and no migration now.
* Bad, because core would name every feature's orders, breaking the two-crate rule, and every new variant would bump
  the codec and the hash layout again.
* Bad, because composite orders would need ad hoc sub-state per variant.

### A behaviour-tree or state-machine crate

* Good, because the patterns are known and some tooling exists.
* Bad, because none is built for lockstep: they rely on `Entity`, change detection, observers or `Local` state, and
  `seldom_state` and `big-brain` trail Bevy 0.20 (research N15).
* Bad, because behaviour trees suit AI controllers (spec §8), not per-unit core behaviour.

### Bevy's built-in relationships

* Good, because the target collection is maintained by Bevy, and hooks keep it current.
* Bad, because they store `Entity`, which may not be hashed or saved, and `linked_spawn` despawns sources outside the
  funnel and outside `SimIndex` order.
* Bad, because `ChildOf` carries transform and visibility semantics for the client and plugins, and the R73 check
  forbids it in the sim world.

## M2 direct-fire gaps

M2's direct fire fits this model and ADR-0017 with three extensions, decided here so that M2 needs no new ADR for
them:

* **Per-weapon stats, keyed by weapon slot.** A multi-weapon unit (a tank's main gun and its machine guns) needs
  range, reload and dispersion per weapon. A stat registered as per-slot has one effective value per `(stat, slot)`,
  and a `Modifier` gains `slot: Option<u8>` in `core.modifiers` v2, migrated from v1 with `slot: None`. Slots are the
  indices of the unit def's turrets and guns, so they are stable per unit type. The bounds, order and cache of
  ADR-0017 apply per slot, and `slot` joins the end of the bucket order. The M2 planner implements this (R153):
  - `StatDef.base` and `effective_stat` take a `slot: Option<u8>` parameter, `None` being the hull (the prelude's
    unit-level value); `EffectiveStats` caches one entry per `(stat, slot)`.
  - A modifier with `slot: None` applies to the hull value and to every slot. The 16-modifier cap of ADR-0017
    applies per `(stat, slot)`, counting every modifier that applies there (the `None` ones and that slot's own), so
    the overflow bound holds for each slot.
* **Turret and firing activities run beside the hull's stack, in a second stack per weapon mount.** The weapons
  feature holds a Hashed `TurretActivities { mounts: Vec<ActivityStack> }` (one stack per turret or mount, at most
  `MAX_MOUNTS` = 8, a code constant because it bounds the save decoder, R153), stepped by its own system in
  `ActSet::Fire` with the same registered `ActivityBehaviour`s, step rules and bounds, in `SimIndex` order. A second
  stack wins over a parallel slot inside the hull's `ActivityStack` because the two lifetimes are independent: a turret keeps engaging while a new move order cancels the hull's frames, and a hull
  order never needs to unwind a turret's frames. With one shared stack, every cancel and every depth bound would
  depend on the other's state. An order that commands both (attack: drive into range, then fire) pushes the hull's
  drive activity and sets the turret's target through the weapons feature's own state, not through a shared stack.
* **Per-shot data travels in combat messages, not effects.** Impact point, angle of incidence, shell and
  penetration are a combat feature's own Messages (`ShotFired`, `Hit`, read in `ResolveSet::Projectiles`;
  ADR-0019). Their readers then request effects (ADR-0017) or add modifiers, with only the content arguments those
  need.

## More Information

* Spec §5.4 (hidden state, `Entity`), §5.5 (commands), §5.6 (`ThinkSet::Orders`, `LifecycleSet::Death`), §5.8,
  §7 (orders), §9.2 C7.
* Research: `architecture-best-practices.md` N14 (relationships), N15 (order queue and activity stack), C21.
* Related: ADR-0004, ADR-0005 (spawn and despawn funnel), ADR-0006 (codecs; `migrate_with`, core versions 3 and 4 in
  the M2 prelude refinement), ADR-0007 (`core.queue_full`, `core.no_map`), ADR-0015 (path requests), ADR-0019 (hooks
  maintain `Related<R>`).
* Rulings: R69 (the interim enum), R77 (`MAX_WAYPOINTS`), R105–R107 (rotate and follow), R129 (re-plan on a rules
  change), R148 (`core.move_order` keeps its name; `MAX_WAYPOINTS` stays a constant), R151 (stepping and cancel
  rules), R153 (orders begun per tick, the loop guard, per-slot stats and `MAX_MOUNTS`).
* Revisit: when an order needs typed parameters across features (a shared parameter registry), or when a relation
  needs many-to-many links.
