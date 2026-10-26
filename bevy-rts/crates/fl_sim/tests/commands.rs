//! Player commands: registry ids, the `(player, seq)` apply order, late, early and duplicate
//! rejection, validation, the core handlers and the `debug.*` handlers.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandKind, CommandPayload, CommandTypeId, DebugInvulnerableCmd, DebugSetHpCmd,
    DebugSetOwnerCmd, DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd, MoveCmd, PlayerCommand,
    PlayerId, SimId, StopCmd, Tick,
};
use fl_sim::{
    CommandCtx, CommandHandler, CommandLog, CommandRegistry, DespawnRequest, Health, Invulnerable,
    MAX_COMMAND_UNITS, MAX_WAYPOINTS, MapInfo, MatchSetup, MoveOrder, Order, Owner, Pose,
    RejectReason, RunConfig, SimBuilder, SimClock, SimEvents, SimFeature, SimIndex, ThinkSet,
    Wander, build_sim_app, compute_state_hash, last_seqs, load_content, setup_scenario, step_tick,
};
use fl_view::{SimEvent, TickInput, TickOutput};
use serde::{Deserialize, Serialize};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// The duel with `features`: #1 (player 0) at (100, 100) heading 0, #2 (player 1) at
/// (900, 900).
fn build_with(features: Vec<Arc<dyn SimFeature>>, cheats_allowed: bool) -> App {
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 1,
        cheats_allowed,
    };
    let mut app =
        build_sim_app(&features, content, setup, RunConfig::default()).expect("the sim builds");
    let ids = setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    assert_eq!(ids, [SimId(1), SimId(2)]);
    app
}

fn build(cheats_allowed: bool) -> App {
    build_with(Vec::new(), cheats_allowed)
}

fn now(app: &App) -> u32 {
    app.world().resource::<SimClock>().tick.0
}

/// Run one tick with `commands` in its input.
fn tick(app: &mut App, commands: Vec<PlayerCommand>) -> TickOutput {
    let input = TickInput {
        tick: Tick(now(app)),
        commands,
    };
    step_tick(app.world_mut(), input)
}

fn step(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        tick(app, Vec::new());
    }
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn command<P: CommandPayload>(
    app: &App,
    tick: u32,
    player: u8,
    seq: u32,
    units: &[u32],
    payload: &P,
    queue: bool,
) -> PlayerCommand {
    PlayerCommand {
        exec_tick: Tick(tick),
        player: PlayerId(player),
        seq,
        units: units.iter().map(|&id| SimId(id)).collect(),
        kind: app.world().resource::<CommandRegistry>().encode(payload),
        queue,
    }
}

/// A non-queued move for the current tick.
fn move_to(app: &App, player: u8, seq: u32, units: &[u32], x: i32, y: i32) -> PlayerCommand {
    let payload = MoveCmd { target: v(x, y) };
    command(app, now(app), player, seq, units, &payload, false)
}

/// A non-queued command for the current tick.
fn cmd_now<P: CommandPayload>(
    app: &App,
    player: u8,
    seq: u32,
    units: &[u32],
    p: &P,
) -> PlayerCommand {
    command(app, now(app), player, seq, units, p, false)
}

fn entity_of(app: &App, id: u32) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .expect("unit exists")
}

fn pose(app: &App, id: u32) -> Pose {
    *app.world()
        .get::<Pose>(entity_of(app, id))
        .expect("unit has a Pose")
}

/// The targets of #`id`'s orders, all moves.
fn waypoints(app: &App, id: u32) -> Vec<FxVec2> {
    app.world()
        .get::<MoveOrder>(entity_of(app, id))
        .expect("unit has a MoveOrder")
        .orders
        .iter()
        .map(|order| match order {
            Order::Move { target } => *target,
            other => panic!("not a move: {other:?}"),
        })
        .collect()
}

/// The rejections of the last tick.
fn rejected(app: &App) -> Vec<RejectReason> {
    app.world()
        .resource::<CommandLog>()
        .rejected
        .iter()
        .map(|(_, reason)| *reason)
        .collect()
}

fn applied(app: &App) -> usize {
    app.world().resource::<CommandLog>().applied.len()
}

fn events(app: &App) -> Vec<SimEvent> {
    app.world()
        .resource::<SimEvents<SimEvent>>()
        .iter()
        .cloned()
        .collect()
}

#[test]
fn move_command_reaches_target() {
    let mut app = build(false);
    let cmd = move_to(&app, 0, 0, &[1], 200, 100);
    tick(&mut app, vec![cmd]);
    assert_eq!(applied(&app), 1);
    step(&mut app, 198);
    assert_ne!(pose(&app, 1).pos, v(200, 100), "0.5 m/tick needs 200 ticks");
    step(&mut app, 1);
    assert_eq!(pose(&app, 1).pos, v(200, 100));
    assert_eq!(pose(&app, 1).heading, Angle::ZERO);
    // The order queue pops the arrived move at the start of the next tick.
    assert_eq!(waypoints(&app, 1), [v(200, 100)]);
    step(&mut app, 1);
    assert!(waypoints(&app, 1).is_empty());
}

#[test]
fn queued_moves_append() {
    let mut app = build(false);
    let first = move_to(&app, 0, 0, &[1], 300, 100);
    let target = v(300, 300);
    let second = command(&app, 0, 0, 1, &[1], &MoveCmd { target }, true);
    tick(&mut app, vec![first, second]);
    assert_eq!(waypoints(&app, 1), [v(300, 100), v(300, 300)]);

    // An unqueued move replaces the whole order.
    let replace = move_to(&app, 0, 2, &[1], 100, 300);
    tick(&mut app, vec![replace]);
    assert_eq!(waypoints(&app, 1), [v(100, 300)]);
}

#[test]
fn stop_clears_the_move_order() {
    let mut app = build(false);
    let cmd = move_to(&app, 0, 0, &[1], 300, 100);
    tick(&mut app, vec![cmd]);
    step(&mut app, 1);
    let stop = cmd_now(&app, 0, 1, &[1], &StopCmd);
    tick(&mut app, vec![stop]);
    assert!(waypoints(&app, 1).is_empty());
    let stopped_at = pose(&app, 1).pos;
    step(&mut app, 5);
    assert_eq!(pose(&app, 1).pos, stopped_at);
}

#[test]
fn foreign_units_rejected() {
    let mut app = build(false);
    let cmd = move_to(&app, 0, 0, &[2], 500, 500);
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::NoValidUnits]);
    assert!(waypoints(&app, 2).is_empty());
    assert_eq!(applied(&app), 0);
}

#[test]
fn duplicate_and_missing_units_are_filtered() {
    let mut app = build(false);
    let cmd = move_to(&app, 0, 0, &[99, 1, 1, 2], 500, 100);
    tick(&mut app, vec![cmd]);
    assert!(rejected(&app).is_empty());
    assert_eq!(waypoints(&app, 1), [v(500, 100)]);
    assert!(waypoints(&app, 2).is_empty());
}

#[test]
fn unknown_player_rejected() {
    let mut app = build(false);
    let cmd = move_to(&app, 7, 0, &[1], 500, 500);
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::UnknownPlayer]);
}

#[test]
fn commands_apply_in_player_seq_order() {
    let mut app = build(false);
    let later = move_to(&app, 0, 1, &[1], 300, 100);
    let earlier = move_to(&app, 0, 0, &[1], 200, 100);
    tick(&mut app, vec![later, earlier]);
    assert_eq!(applied(&app), 2);
    assert_eq!(waypoints(&app, 1), [v(300, 100)], "seq 1 applied last");
}

#[test]
fn duplicate_seq_rejected() {
    let mut app = build(false);
    let first = move_to(&app, 0, 4, &[1], 300, 100);
    let second = move_to(&app, 0, 4, &[1], 200, 100);
    tick(&mut app, vec![first, second]);
    assert_eq!(rejected(&app), [RejectReason::DuplicateSeq]);
    assert_eq!(waypoints(&app, 1), [v(300, 100)], "the first one applied");
}

#[test]
fn seq_must_increase_across_ticks() {
    let mut app = build(false);
    step(&mut app, 1);
    let cmd = move_to(&app, 0, 5, &[1], 300, 100);
    tick(&mut app, vec![cmd]);
    assert!(rejected(&app).is_empty());
    let same = move_to(&app, 0, 5, &[1], 200, 100);
    tick(&mut app, vec![same]);
    assert_eq!(rejected(&app), [RejectReason::DuplicateSeq]);
    let lower = move_to(&app, 0, 3, &[1], 200, 100);
    tick(&mut app, vec![lower]);
    assert_eq!(rejected(&app), [RejectReason::DuplicateSeq]);
    assert_eq!(waypoints(&app, 1), [v(300, 100)]);
    assert_eq!(last_seqs(app.world()), [(PlayerId(0), 5)].into());
}

#[test]
fn late_and_early_rejected() {
    let mut app = build(false);
    step(&mut app, 5);
    let late = command(&app, 4, 0, 0, &[1], &MoveCmd { target: v(9, 9) }, false);
    let early = command(&app, 6, 0, 1, &[1], &MoveCmd { target: v(9, 9) }, false);
    tick(&mut app, vec![late, early]);
    assert_eq!(rejected(&app), [RejectReason::Late, RejectReason::Early]);
    assert!(waypoints(&app, 1).is_empty());
    // Neither counts as taken: seq 0 is still free.
    assert!(last_seqs(app.world()).is_empty());
}

#[test]
fn last_seq_tracks_the_highest_seq_per_player() {
    let mut app = build(false);
    assert!(last_seqs(app.world()).is_empty());
    let commands = vec![
        move_to(&app, 0, 3, &[1], 200, 100),
        move_to(&app, 0, 1, &[1], 200, 100),
        move_to(&app, 1, 0, &[2], 800, 800),
        move_to(&app, 7, 5, &[1], 200, 100),
    ];
    tick(&mut app, commands);
    let expected = [(PlayerId(0), 3), (PlayerId(1), 0), (PlayerId(7), 5)];
    assert_eq!(last_seqs(app.world()), expected.into_iter().collect());
}

#[test]
fn rejections_raise_events_and_the_log_holds_one_tick() {
    let mut app = build(false);
    let cmd = move_to(&app, 0, 0, &[2], 500, 500);
    tick(&mut app, vec![cmd]);
    assert_eq!(
        events(&app),
        [SimEvent::CommandRejected {
            player: PlayerId(0),
            seq: 0,
            reason: "core.no_valid_units".to_owned(),
        }]
    );
    step(&mut app, 1);
    assert!(rejected(&app).is_empty(), "the log is per tick");
    assert!(events(&app).is_empty());
}

fn spawn_payload(unit: &str, owner: u8) -> DebugSpawnCmd {
    DebugSpawnCmd {
        unit: unit.to_owned(),
        owner: PlayerId(owner),
        pos: v(500, 500),
        heading: Angle::QUARTER,
    }
}

#[test]
fn debug_requires_cheats() {
    let mut app = build(false);
    let cmd = cmd_now(&app, 1, 0, &[], &spawn_payload("core_test.tank", 1));
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::DebugDisabled]);
    step(&mut app, 1);
    assert_eq!(app.world().resource::<SimIndex>().len(), 2);
}

#[test]
fn debug_spawn_appears_at_end_of_tick() {
    let mut app = build(true);
    let cmd = cmd_now(&app, 1, 0, &[], &spawn_payload("core_test.tank", 1));
    tick(&mut app, vec![cmd]);
    assert!(rejected(&app).is_empty());
    assert_eq!(applied(&app), 1);
    let ids: Vec<SimId> = app.world().resource::<SimIndex>().ids().collect();
    assert_eq!(ids, [SimId(1), SimId(2), SimId(3)]);
    let tank = app
        .world()
        .resource::<fl_sim::Content>()
        .0
        .unit_id("core_test.tank");
    assert!(events(&app).contains(&SimEvent::Spawned {
        sim_id: SimId(3),
        unit: tank,
        owner: PlayerId(1),
    }));
    let spawned = pose(&app, 3);
    assert_eq!(
        (spawned.pos, spawned.heading),
        (v(500, 500), Angle::QUARTER)
    );
    let owner = app.world().get::<Owner>(entity_of(&app, 3)).expect("owner");
    assert_eq!(owner.0, PlayerId(1));
}

#[test]
fn debug_spawn_unknown_unit_rejected() {
    let mut app = build(true);
    let cmd = cmd_now(&app, 1, 0, &[], &spawn_payload("nope", 1));
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::UnknownContent]);
    assert_eq!(app.world().resource::<SimIndex>().len(), 2);
}

#[test]
fn debug_spawn_unknown_owner_rejected() {
    let mut app = build(true);
    let cmd = cmd_now(&app, 1, 0, &[], &spawn_payload("core_test.tank", 9));
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::UnknownPlayer]);
    assert_eq!(app.world().resource::<SimIndex>().len(), 2);
}

fn hp(app: &App, id: u32) -> u32 {
    app.world()
        .get::<Health>(entity_of(app, id))
        .expect("unit has Health")
        .hp
}

#[test]
fn debug_commands_target_any_player() {
    // R44: the client issues debug commands as its local player; with cheats on they may
    // target any player's units.
    let mut app = build(true);
    let cmd = cmd_now(&app, 0, 0, &[2], &DebugSetHpCmd { hp: 1 });
    tick(&mut app, vec![cmd]);
    assert!(rejected(&app).is_empty());
    assert_eq!(hp(&app, 2), 1);

    // hp is capped at the maximum.
    let cmd = cmd_now(&app, 0, 1, &[2], &DebugSetHpCmd { hp: 1000 });
    tick(&mut app, vec![cmd]);
    assert_eq!(hp(&app, 2), 100);

    // Without cheats the same command is DebugDisabled, not filtered.
    let mut app = build(false);
    let cmd = cmd_now(&app, 0, 0, &[2], &DebugSetHpCmd { hp: 1 });
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::DebugDisabled]);
    assert_eq!(hp(&app, 2), 100);
}

#[test]
fn debug_set_owner_invulnerable_teleport() {
    let mut app = build(true);
    let cmd = move_to(&app, 0, 0, &[1], 300, 100);
    tick(&mut app, vec![cmd]);
    assert!(!waypoints(&app, 1).is_empty());

    let commands = vec![
        cmd_now(&app, 0, 1, &[1], &DebugSetOwnerCmd { owner: PlayerId(1) }),
        cmd_now(&app, 0, 2, &[1], &DebugInvulnerableCmd { on: true }),
        cmd_now(&app, 0, 3, &[1], &DebugTeleportCmd { pos: v(5000, 5000) }),
        cmd_now(&app, 0, 4, &[1], &DebugSetOwnerCmd { owner: PlayerId(9) }),
    ];
    tick(&mut app, commands);
    assert_eq!(rejected(&app), [RejectReason::UnknownPlayer]);
    let unit = entity_of(&app, 1);
    let world = app.world();
    assert_eq!(world.get::<Owner>(unit).expect("owner").0, PlayerId(1));
    assert!(world.get::<Invulnerable>(unit).is_some());
    let map = world.resource::<MapInfo>().clone();
    assert_eq!(pose(&app, 1).pos, map.clamp(v(5000, 5000)));
    assert!(waypoints(&app, 1).is_empty());

    let cmd = cmd_now(&app, 0, 5, &[1], &DebugInvulnerableCmd { on: false });
    tick(&mut app, vec![cmd]);
    assert!(app.world().get::<Invulnerable>(unit).is_none());
}

#[test]
fn debug_wander_inserts_and_removes_wander() {
    let mut app = build(true);
    let radius = Fx::from_int(50);
    let cmd = cmd_now(&app, 0, 0, &[1], &DebugWanderCmd { radius });
    tick(&mut app, vec![cmd]);
    let wander = app
        .world()
        .get::<Wander>(entity_of(&app, 1))
        .expect("wander inserted");
    assert_eq!((wander.home, wander.radius), (v(100, 100), radius));
    assert_eq!(waypoints(&app, 1).len(), 1, "wander picked a waypoint");

    let stop = DebugWanderCmd { radius: Fx::ZERO };
    let cmd = cmd_now(&app, 0, 1, &[1], &stop);
    tick(&mut app, vec![cmd]);
    assert!(app.world().get::<Wander>(entity_of(&app, 1)).is_none());
}

#[test]
fn command_type_ids_sorted_by_name() {
    let app = build(false);
    let registry = app.world().resource::<CommandRegistry>();
    let ids = [
        registry.type_id::<MoveCmd>(),
        registry.type_id::<StopCmd>(),
        registry.type_id::<DebugInvulnerableCmd>(),
        registry.type_id::<DebugSetHpCmd>(),
        registry.type_id::<DebugSetOwnerCmd>(),
        registry.type_id::<DebugSpawnCmd>(),
        registry.type_id::<DebugTeleportCmd>(),
        registry.type_id::<DebugWanderCmd>(),
    ];
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
    let names: Vec<_> = ids.iter().map(|&id| registry.name(id)).collect();
    assert_eq!(
        names,
        [
            Some("core.move"),
            Some("core.stop"),
            Some("debug.invulnerable"),
            Some("debug.set_hp"),
            Some("debug.set_owner"),
            Some("debug.spawn"),
            Some("debug.teleport"),
            Some("debug.wander"),
        ]
    );
    assert_eq!(registry.name(CommandTypeId(999)), None);
}

/// 50 ticks with #1 moving and a valid command (seq 7) at tick 30; `bad` adds seven bad
/// commands at tick 10. Returns the final state hash, the rejections summed over every tick,
/// and the app.
fn run_with(bad: bool) -> (u64, Vec<RejectReason>, App) {
    let mut app = build(true);
    let mut reasons = Vec::new();
    for t in 0..50 {
        let commands = match t {
            0 => vec![move_to(&app, 0, 0, &[1], 400, 300)],
            10 if bad => {
                let move_type = app
                    .world()
                    .resource::<CommandRegistry>()
                    .type_id::<MoveCmd>();
                let target = MoveCmd { target: v(0, 0) };
                let late = command(&app, 9, 0, 1, &[1], &target, false);
                let early = command(&app, 11, 0, 2, &[1], &target, false);
                let mut unknown_type = move_to(&app, 0, 3, &[1], 0, 0);
                unknown_type.kind.type_id = CommandTypeId(999);
                let mut corrupt = move_to(&app, 0, 4, &[1], 0, 0);
                corrupt.kind = CommandKind {
                    type_id: move_type,
                    payload: vec![0xFF; 3],
                };
                let foreign = move_to(&app, 0, 5, &[2], 0, 0);
                let nope = cmd_now(&app, 0, 6, &[], &spawn_payload("nope", 0));
                let duplicate = move_to(&app, 0, 0, &[1], 0, 0);
                vec![late, early, unknown_type, corrupt, foreign, nope, duplicate]
            }
            30 => vec![move_to(&app, 0, 7, &[1], 300, 400)],
            _ => Vec::new(),
        };
        tick(&mut app, commands);
        reasons.extend(rejected(&app));
        if bad && t == 10 {
            // Every command that is not late, early or a duplicate takes its seq.
            assert_eq!(last_seqs(app.world()), [(PlayerId(0), 6)].into());
            let rejections = events(&app)
                .iter()
                .filter(|e| matches!(e, SimEvent::CommandRejected { .. }))
                .count();
            assert_eq!(rejections, 7);
        }
    }
    (compute_state_hash(app.world()), reasons, app)
}

#[test]
fn bad_commands_do_not_change_state() {
    let (clean, clean_reasons, _) = run_with(false);
    let (dirty, reasons, app) = run_with(true);
    assert!(clean_reasons.is_empty());
    assert_eq!(
        reasons,
        [
            RejectReason::DuplicateSeq,
            RejectReason::Late,
            RejectReason::Early,
            RejectReason::UnknownType,
            RejectReason::BadPayload,
            RejectReason::NoValidUnits,
            RejectReason::UnknownContent,
        ]
    );
    assert_eq!(last_seqs(app.world()), [(PlayerId(0), 7)].into());
    assert_eq!(clean, dirty, "the full state hash, LastSeq included");
}

/// Writes `DespawnRequest(#2)` in tick 3, in `Think/Orders`.
struct DespawnTwoFeature;

impl SimFeature for DespawnTwoFeature {
    fn id(&self) -> &'static str {
        "despawn_two"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(
            ThinkSet::Orders,
            |clock: Res<SimClock>, mut despawns: MessageWriter<DespawnRequest>| {
                if clock.tick == Tick(3) {
                    despawns.write(DespawnRequest(SimId(2)));
                }
            },
        );
    }
}

#[test]
fn despawned_unit_command_same_tick_filtered() {
    let mut app = build_with(vec![Arc::new(DespawnTwoFeature)], false);
    step(&mut app, 3);
    let target = MoveCmd {
        target: v(800, 800),
    };
    // In the despawn tick the move still applies: commands run in Input, the despawn at the
    // end of the tick.
    let same_tick = command(&app, 3, 1, 0, &[2], &target, false);
    // Stamped for tick 4 while #2 is still alive at the end of tick 2.
    let next_tick = command(&app, 4, 1, 1, &[2], &target, false);
    tick(&mut app, vec![same_tick]);
    assert!(rejected(&app).is_empty());
    assert_eq!(applied(&app), 1);
    assert!(app.world().resource::<SimIndex>().get(SimId(2)).is_none());
    assert!(events(&app).contains(&SimEvent::Despawned { sim_id: SimId(2) }));
    tick(&mut app, vec![next_tick]);
    assert_eq!(rejected(&app), [RejectReason::NoValidUnits]);
}

#[test]
fn debug_spawn_off_map_is_clamped() {
    let mut app = build(true);
    let payload = DebugSpawnCmd {
        pos: v(5000, -20),
        ..spawn_payload("core_test.tank", 1)
    };
    let cmd = cmd_now(&app, 1, 0, &[], &payload);
    tick(&mut app, vec![cmd]);
    assert!(rejected(&app).is_empty());
    assert_eq!(applied(&app), 1);
    let map = app.world().resource::<MapInfo>().clone();
    assert_eq!(pose(&app, 3).pos, map.clamp(v(5000, -20)));
    let counters = app.world().resource::<fl_sim::SimCounters>();
    assert_eq!(counters.get("spawn.rejected"), 0);
    assert_eq!(counters.get("spawn.entities"), 1);
}

#[test]
fn reject_codes_are_stable() {
    let codes = [
        (RejectReason::Late, "core.late"),
        (RejectReason::Early, "core.early"),
        (RejectReason::DuplicateSeq, "core.duplicate_seq"),
        (RejectReason::UnknownType, "core.unknown_type"),
        (RejectReason::BadPayload, "core.bad_payload"),
        (RejectReason::UnknownPlayer, "core.unknown_player"),
        (RejectReason::DebugDisabled, "core.debug_disabled"),
        (RejectReason::NoValidUnits, "core.no_valid_units"),
        (RejectReason::UnknownContent, "core.unknown_content"),
        (RejectReason::TooManyUnits, "core.too_many_units"),
        (RejectReason::QueueFull, "core.queue_full"),
        (
            RejectReason::Feature("test.building_full"),
            "test.building_full",
        ),
    ];
    for (reason, code) in codes {
        assert_eq!(reason.code(), code, "{reason:?}");
    }
}

/// `"rej.always"`: always rejected by its handler with a feature reason.
#[derive(Debug, Serialize, Deserialize)]
struct AlwaysRejectCmd;

impl CommandPayload for AlwaysRejectCmd {
    const NAME: &'static str = "rej.always";
    const NEEDS_UNITS: bool = false;
}

struct AlwaysReject;

impl CommandHandler<AlwaysRejectCmd> for AlwaysReject {
    fn apply(
        &self,
        _payload: &AlwaysRejectCmd,
        _world: &mut World,
        _ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        Err(RejectReason::Feature("rej.building_full"))
    }
}

struct RejectFeature;

impl SimFeature for RejectFeature {
    fn id(&self) -> &'static str {
        "rej"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.register_command::<AlwaysRejectCmd>(AlwaysReject);
    }
}

#[test]
fn feature_reject_reason_reaches_log_and_events() {
    let mut app = build_with(vec![Arc::new(RejectFeature)], false);
    let cmd = cmd_now(&app, 0, 0, &[], &AlwaysRejectCmd);
    tick(&mut app, vec![cmd]);
    assert_eq!(rejected(&app), [RejectReason::Feature("rej.building_full")]);
    assert_eq!(
        events(&app),
        [SimEvent::CommandRejected {
            player: PlayerId(0),
            seq: 0,
            reason: "rej.building_full".to_owned(),
        }]
    );
}

#[test]
fn units_per_command_are_capped() {
    let mut app = build(false);
    let mut at_cap = move_to(&app, 0, 0, &[1], 200, 100);
    at_cap.units = vec![SimId(1); MAX_COMMAND_UNITS];
    tick(&mut app, vec![at_cap]);
    assert!(rejected(&app).is_empty());
    assert_eq!(applied(&app), 1);

    let mut over = move_to(&app, 0, 1, &[1], 300, 100);
    over.units = vec![SimId(1); MAX_COMMAND_UNITS + 1];
    tick(&mut app, vec![over]);
    assert_eq!(rejected(&app), [RejectReason::TooManyUnits]);
    assert_eq!(waypoints(&app, 1), [v(200, 100)], "unchanged");
}

#[test]
fn queued_waypoints_are_capped() {
    let mut app = build(false);
    let mut commands = vec![move_to(&app, 0, 0, &[1], 200, 100)];
    let cap = u32::try_from(MAX_WAYPOINTS).expect("small");
    for seq in 1..cap {
        let target = v(200, 100 + i32::try_from(seq).expect("small"));
        commands.push(command(&app, 0, 0, seq, &[1], &MoveCmd { target }, true));
    }
    tick(&mut app, commands);
    assert!(rejected(&app).is_empty());
    assert_eq!(waypoints(&app, 1).len(), MAX_WAYPOINTS);

    // The cap counts orders of every kind (R77); a queued move onto a full queue is
    // "core.queue_full" and changes nothing.
    let before = waypoints(&app, 1);
    let full = command(&app, 1, 0, cap, &[1], &MoveCmd { target: v(9, 9) }, true);
    tick(&mut app, vec![full]);
    assert_eq!(rejected(&app), [RejectReason::QueueFull]);
    assert_eq!(RejectReason::QueueFull.code(), "core.queue_full");
    assert_eq!(waypoints(&app, 1), before, "unchanged");
    assert_eq!(last_seqs(app.world()), [(PlayerId(0), cap)].into());

    // An unqueued move still replaces the full queue.
    let replace = move_to(&app, 0, cap + 1, &[1], 100, 300);
    tick(&mut app, vec![replace]);
    assert_eq!(waypoints(&app, 1), [v(100, 300)]);
}
