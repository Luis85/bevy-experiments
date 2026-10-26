//! Task P2: stateful property tests (proptest-state-machine).
//!
//! Random sequences of valid `SimCommand`s, interleaved with random tick
//! batches, run against a village of 1..=4 plots. A small reference model
//! tracks which units are alive (so every generated command is valid when
//! submitted); the sim's invariants are checked after **every tick**:
//!
//! - `SimIndex` matches the `SimId` components, and the model's alive set;
//! - needs are in `0..=NEED_MAX`;
//! - stockpile food is `<= capacity`;
//! - every claimant and target in `Reservations` is alive in `SimIndex`,
//!   claim lists are non-empty and duplicate-free, and only beds, work sites,
//!   wells and stockpiles are ever claimed;
//! - beds and work sites have at most 1 claimant, wells at most their slots;
//! - claims on a stockpile are `<= food` (see [`check_world`] for the one
//!   tolerated exception: a `SetFood` that lowers food below the
//!   outstanding claims);
//! - `ActiveLeaf` exists iff the running path is non-empty;
//! - a replay of the run (its `CommandLog` plus a hash checkpoint after every
//!   tick batch) verifies, so the same commands give the same hashes. Half
//!   the cases run with the journal on, while the replay always runs with
//!   it off: that also checks the journal never changes sim results.
//!
//! Cases: 64 by default; `PROPTEST_CASES` overrides it. Failures shrink to a
//! minimal sequence (persisted under `proptest-regressions/`).

use std::collections::{BTreeMap, BTreeSet};

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::ai::{ActiveLeaf, Brain, BrainState};
use npc_framework::kernel::commands::{CommandLog, POS_LIMIT};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::kernel::registry::content_hash;
use npc_framework::needs::NEED_MAX;
use npc_framework::prelude::{
    Needs, Order, Reservations, SimClock, SimCommand, SimId, SimIndex, run_ticks, submit,
};
use npc_framework::tools::snapshot_check::assert_resume_equivalent;
use village_content::needs::{ENERGY, HUNGER, THIRST, VillagerNeeds};
use village_content::replay::{REPLAY_VERSION, Replay, new_sim_app, replay_app, verify};
use village_content::scenario::{STOCKPILE_CAPACITY, VillageIds};
use village_content::{Bed, SetFood, SpawnVillager, Stockpile, Well, WorkSite};

/// The typed content command, when `cmd` is one of kind `C`.
fn as_content<C: npc_framework::prelude::ContentCommand>(cmd: &SimCommand) -> Option<C> {
    match cmd {
        SimCommand::Content { kind, payload } if kind == C::NAME => payload.to_typed().ok(),
        _ => None,
    }
}
use proptest::prelude::*;
use proptest::test_runner::Config;
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest, prop_state_machine};

/// Kinds of sim entity the model tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    House,
    Bed,
    Stockpile,
    Field,
    Villager,
    Well,
}

/// One step of a generated run.
#[derive(Clone, Debug)]
enum Step {
    /// Run this many ticks, checking the invariants after each.
    Tick(u32),
    /// Submit a command for the next tick.
    Cmd(SimCommand),
}

/// Reference model: just enough to generate valid commands.
#[derive(Clone, Debug)]
struct Model {
    seed: u64,
    /// Plots of the scenario (1 = `village`, else `village_n`).
    plots: u32,
    journal: bool,
    /// Tick of the snapshot save in the resume-equivalence variant.
    resume_at: u64,
    /// Units alive once every submitted command has applied.
    alive: BTreeMap<u32, Kind>,
    /// Spawns submitted since the last tick (they get ids when it runs).
    pending_spawns: u32,
    /// Next `SimId` the sim will allocate.
    next_id: u32,
}

impl Model {
    fn new(seed: u64, plots: u32, journal: bool, resume_at: u64) -> Self {
        let mut alive = BTreeMap::new();
        // Each plot spawns house, bed, stockpile, field, villager in order
        // (`scenario::spawn_plot`), then every plot's well after all plots
        // (`scenario::spawn_well`). A new scenario entity must be added here
        // too, in spawn order (`.claude/rules/content.md`, "SimId stability").
        let kinds = [
            Kind::House,
            Kind::Bed,
            Kind::Stockpile,
            Kind::Field,
            Kind::Villager,
        ];
        for k in 0..plots {
            for (i, kind) in kinds.iter().enumerate() {
                alive.insert(k * 5 + i as u32 + 1, *kind);
            }
        }
        for k in 0..plots {
            alive.insert(plots * 5 + k + 1, Kind::Well);
        }
        Self {
            seed,
            plots,
            journal,
            resume_at,
            alive,
            pending_spawns: 0,
            next_id: plots * 6 + 1,
        }
    }

    fn scenario(&self) -> &'static str {
        if self.plots == 1 {
            "village"
        } else {
            "village_n"
        }
    }

    fn ids(&self, kind: Kind) -> Vec<u32> {
        self.alive
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(&id, _)| id)
            .collect()
    }

    fn is(&self, id: u32, kind: Kind) -> bool {
        self.alive.get(&id) == Some(&kind)
    }

    /// Whether `cmd` is valid against the model (and so against the sim).
    fn valid(&self, cmd: &SimCommand) -> bool {
        if let Some(SpawnVillager { home, .. }) = as_content(cmd) {
            return match home {
                Some(h) => self.is(h, Kind::House),
                None => !self.ids(Kind::House).is_empty(),
            };
        }
        if let Some(SetFood { stockpile, food }) = as_content(cmd) {
            return self.is(stockpile, Kind::Stockpile) && food <= STOCKPILE_CAPACITY;
        }
        match *cmd {
            SimCommand::Despawn { unit } => self.alive.contains_key(&unit),
            SimCommand::SetNeed { unit, .. } | SimCommand::Order { unit, .. } => {
                self.is(unit, Kind::Villager)
            }
            SimCommand::Content { .. } => false,
        }
    }
}

fn pick(ids: Vec<u32>) -> BoxedStrategy<u32> {
    proptest::sample::select(ids).boxed()
}

fn pos(range: i32) -> impl Strategy<Value = (i32, i32)> {
    (-range..=range, -range..=range)
}

struct SimModel;

impl ReferenceStateMachine for SimModel {
    type State = Model;
    type Transition = Step;

    fn init_state() -> BoxedStrategy<Model> {
        (0u64..1000, 1u32..=4, any::<bool>(), 1u64..4000)
            .prop_map(|(seed, plots, journal, at)| Model::new(seed, plots, journal, at))
            .boxed()
    }

    fn transitions(state: &Model) -> BoxedStrategy<Step> {
        let ticks = prop_oneof![4 => 1u32..=40, 1 => 200u32..=1500].prop_map(Step::Tick);
        let mut options: Vec<(u32, BoxedStrategy<Step>)> = vec![(10, ticks.boxed())];

        let villagers = state.ids(Kind::Villager);
        if !villagers.is_empty() {
            let need = prop_oneof![
                Just(HUNGER.into()),
                Just(ENERGY.into()),
                Just(THIRST.into())
            ];
            let value = prop_oneof![Just(0), Just(NEED_MAX), 0..=NEED_MAX];
            options.push((
                3,
                (pick(villagers.clone()), need, value)
                    .prop_map(|(unit, need, value)| {
                        Step::Cmd(SimCommand::SetNeed { unit, need, value })
                    })
                    .boxed(),
            ));
            let order = prop_oneof![
                4 => pos(20_000).prop_map(|pos| Order::GoTo { pos }),
                1 => Just(Order::GoTo { pos: (POS_LIMIT, -POS_LIMIT) }),
                2 => Just(Order::Clear),
            ];
            options.push((
                3,
                (pick(villagers), order)
                    .prop_map(|(unit, order)| Step::Cmd(SimCommand::Order { unit, order }))
                    .boxed(),
            ));
        }
        let stockpiles = state.ids(Kind::Stockpile);
        if !stockpiles.is_empty() {
            options.push((
                2,
                (pick(stockpiles), 0..=STOCKPILE_CAPACITY)
                    .prop_map(|(stockpile, food)| {
                        Step::Cmd(SimCommand::content(&SetFood { stockpile, food }))
                    })
                    .boxed(),
            ));
        }
        let houses = state.ids(Kind::House);
        if !houses.is_empty() {
            let home = prop_oneof![Just(None), pick(houses).prop_map(Some)];
            options.push((
                2,
                (pos(20_000), home)
                    .prop_map(|(pos, home)| {
                        Step::Cmd(SimCommand::content(&SpawnVillager { pos, home }))
                    })
                    .boxed(),
            ));
        }
        let all: Vec<u32> = state.alive.keys().copied().collect();
        if !all.is_empty() {
            options.push((
                1,
                pick(all)
                    .prop_map(|unit| Step::Cmd(SimCommand::Despawn { unit }))
                    .boxed(),
            ));
        }
        proptest::strategy::Union::new_weighted(options).boxed()
    }

    fn apply(mut state: Model, step: &Step) -> Model {
        match step {
            Step::Tick(_) => {
                for _ in 0..state.pending_spawns {
                    state.alive.insert(state.next_id, Kind::Villager);
                    state.next_id += 1;
                }
                state.pending_spawns = 0;
            }
            Step::Cmd(cmd) if as_content::<SpawnVillager>(cmd).is_some() => {
                state.pending_spawns += 1;
            }
            Step::Cmd(SimCommand::Despawn { unit }) => {
                state.alive.remove(unit);
            }
            Step::Cmd(_) => {}
        }
        state
    }

    fn preconditions(state: &Model, step: &Step) -> bool {
        match step {
            Step::Tick(n) => *n > 0,
            Step::Cmd(cmd) => state.valid(cmd),
        }
    }
}

/// The system under test.
struct Sut {
    app: App,
    /// Over-claimed stockpiles and their claimants after the last tick.
    over: BTreeMap<SimId, Vec<SimId>>,
    /// `(tick, state_hash)` after every tick batch.
    checkpoints: Vec<(u64, u64)>,
}

struct SimProps;

impl StateMachineTest for SimProps {
    type SystemUnderTest = Sut;
    type Reference = SimModel;

    fn init_test(model: &Model) -> Sut {
        let mut app = new_sim_app(model.seed, model.scenario(), model.plots);
        if model.journal {
            set_journal(app.world_mut(), true, false);
        }
        Sut {
            app,
            over: BTreeMap::new(),
            checkpoints: Vec::new(),
        }
    }

    fn apply(mut sut: Sut, model: &Model, step: Step) -> Sut {
        let world = sut.app.world_mut();
        match step {
            Step::Tick(n) => {
                for _ in 0..n {
                    run_ticks(world, 1);
                    check_world(world, &mut sut.over);
                }
                let tick = world.resource::<SimClock>().tick;
                sut.checkpoints.push((tick, state_hash(world)));
                // After a tick every submitted command has applied.
                let index: BTreeSet<u32> = world
                    .resource::<SimIndex>()
                    .map
                    .keys()
                    .map(|id| id.0)
                    .collect();
                let expected: BTreeSet<u32> = model.alive.keys().copied().collect();
                assert_eq!(
                    index, expected,
                    "tick {tick}: SimIndex differs from the model"
                );
            }
            Step::Cmd(cmd) => {
                if let Err(e) = submit(world, None, cmd.clone()) {
                    panic!("valid command {cmd:?} rejected: {e}");
                }
            }
        }
        sut
    }

    fn check_invariants(sut: &Sut, _model: &Model) {
        // `check_world` needs `&mut World` for queries; run it on the
        // per-tick path instead. Here only the cheap resource checks.
        let world = sut.app.world();
        let index = world.resource::<SimIndex>();
        let reservations = world.resource::<Reservations>();
        for (target, claimants) in &reservations.claims {
            assert!(
                index.get(*target).is_some(),
                "claim on dead target {target:?}"
            );
            for c in claimants {
                assert!(index.get(*c).is_some(), "dead claimant {c:?} on {target:?}");
            }
        }
    }

    fn teardown(sut: Sut, model: Model) {
        replay_of(sut, &model);
    }
}

/// Replays the finished run (its `CommandLog` plus the checkpoints) and
/// asserts it verifies; returns the replay.
fn replay_of(mut sut: Sut, model: &Model) -> Replay {
    let world = sut.app.world_mut();
    let ticks = world.resource::<SimClock>().tick;
    let replay = Replay {
        version: REPLAY_VERSION,
        content_hash: Some(content_hash(world)),
        seed: model.seed,
        scenario: model.scenario().to_owned(),
        villagers: model.plots,
        ticks,
        commands: world.resource::<CommandLog>().entries.clone(),
        checkpoints: std::mem::take(&mut sut.checkpoints),
    };
    let report = verify(&replay);
    assert!(
        report.ok,
        "replay of the generated run does not verify: {report:?}
replay: {}",
        serde_json::to_string(&replay).unwrap_or_default()
    );
    replay
}

/// The same generated runs, ending in the snapshot invariant: an app built
/// from the run's commands, saved at the model's `resume_at` tick and loaded
/// into a fresh app, runs on exactly like one never saved (ADR 0013).
struct ResumeProps;

impl StateMachineTest for ResumeProps {
    type SystemUnderTest = Sut;
    type Reference = SimModel;

    fn init_test(model: &Model) -> Sut {
        SimProps::init_test(model)
    }

    fn apply(sut: Sut, model: &Model, step: Step) -> Sut {
        SimProps::apply(sut, model, step)
    }

    fn check_invariants(sut: &Sut, model: &Model) {
        SimProps::check_invariants(sut, model);
    }

    fn teardown(sut: Sut, model: Model) {
        let replay = replay_of(sut, &model);
        assert_resume_equivalent(
            || replay_app(&replay).expect("replay app"),
            model.resume_at,
            1200,
            100,
        );
    }
}

/// Asserts every per-tick invariant (see the module docs). `over` carries
/// the over-claimed stockpiles from one tick to the next.
///
/// Claims on a stockpile may exceed its food only after a `SetFood` lowering
/// the food below the outstanding claims (an external override). The
/// stockpile may then stay over-claimed, but only while no new claim is made
/// on it (its claimants are a subset of the last tick's). A finished meal
/// releases its claim in the tick it takes its food (plan Task C2), so there
/// is no release-lag exception any more.
fn check_world(world: &mut World, over: &mut BTreeMap<SimId, Vec<SimId>>) {
    let tick = world.resource::<SimClock>().tick;
    let index = world.resource::<SimIndex>().map.clone();

    let mut ids = world.query::<(Entity, &SimId)>();
    let mut count = 0;
    for (e, id) in ids.iter(world) {
        count += 1;
        assert_eq!(
            index.get(id),
            Some(&e),
            "tick {tick}: SimIndex misses {id:?}"
        );
    }
    assert_eq!(
        count,
        index.len(),
        "tick {tick}: SimIndex has stale entries"
    );

    let mut needs = world.query::<(&SimId, &Needs)>();
    for (id, n) in needs.iter(world) {
        let n = VillagerNeeds::of(world, n).expect("villager needs");
        for (name, need) in [
            ("hunger", n.hunger),
            ("energy", n.energy),
            ("thirst", n.thirst),
        ] {
            assert!(
                (0..=NEED_MAX).contains(&need.value),
                "tick {tick}: {id:?} {name} {} outside 0..={NEED_MAX}",
                need.value
            );
        }
    }

    let mut piles = world.query::<(&SimId, &Stockpile)>();
    for (id, pile) in piles.iter(world) {
        assert!(
            pile.food <= pile.capacity,
            "tick {tick}: stockpile {id:?} food {} > capacity {}",
            pile.food,
            pile.capacity
        );
    }

    let mut brains =
        world.query_filtered::<(&SimId, Option<&BrainState>, Option<&ActiveLeaf>), With<Brain>>();
    for (id, state, leaf) in brains.iter(world) {
        let running = state.is_some_and(|s| !s.running_path.is_empty());
        assert_eq!(
            leaf.is_some(),
            running,
            "tick {tick}: {id:?} ActiveLeaf {leaf:?} vs running path {:?}",
            state.map(|s| &s.running_path)
        );
    }

    // Stockpiles a `SetFood` applied to in the tick that just ran.
    let set_food: BTreeSet<SimId> = world
        .resource::<CommandLog>()
        .entries
        .iter()
        .rev()
        .take_while(|(t, _, _)| *t + 1 == tick)
        .filter_map(|(_, _, cmd)| as_content::<SetFood>(cmd).map(|c| SimId(c.stockpile)))
        .collect();
    let mut now_over = BTreeMap::new();
    let reservations = world.resource::<Reservations>();
    for (target, claimants) in &reservations.claims {
        assert!(
            !claimants.is_empty(),
            "tick {tick}: empty claim list on {target:?}"
        );
        let unique: BTreeSet<_> = claimants.iter().collect();
        assert_eq!(
            unique.len(),
            claimants.len(),
            "tick {tick}: duplicate claimants {claimants:?}"
        );
        let Some(&te) = index.get(target) else {
            panic!("tick {tick}: claim on dead target {target:?} by {claimants:?}");
        };
        for c in claimants {
            assert!(
                index.contains_key(c),
                "tick {tick}: dead claimant {c:?} holds {target:?}"
            );
        }
        let n = claimants.len() as u32;
        if world.get::<Bed>(te).is_some() || world.get::<WorkSite>(te).is_some() {
            assert!(
                n <= 1,
                "tick {tick}: {n} claimants on bed/work site {target:?}"
            );
        } else if let Some(well) = world.get::<Well>(te) {
            assert!(
                n <= well.slots,
                "tick {tick}: {n} claimants on well {target:?} with {} slots",
                well.slots
            );
        } else if let Some(pile) = world.get::<Stockpile>(te) {
            let counted: Vec<SimId> = claimants.clone();
            if counted.len() as u32 > pile.food {
                let set_food_now = set_food.contains(target);
                let shrinking = over
                    .get(target)
                    .is_some_and(|prev| counted.iter().all(|c| prev.contains(c)));
                assert!(
                    set_food_now || shrinking,
                    "tick {tick}: {} claims {counted:?} on stockpile {target:?} with food {} \
                     (not after a SetFood; claimants last tick: {:?})",
                    counted.len(),
                    pile.food,
                    over.get(target)
                );
                now_over.insert(*target, counted);
            }
        } else {
            panic!("tick {tick}: claim on non-reservable {target:?}");
        }
    }
    *over = now_over;
}

fn config() -> Config {
    let mut config = Config::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 64;
    }
    config
}

prop_state_machine! {
    #![proptest_config(config())]
    #[test]
    fn slow_random_commands_keep_invariants(sequential 1..48 => SimProps);
}

prop_state_machine! {
    #![proptest_config(config())]
    #[test]
    fn slow_random_commands_resume_equivalent(sequential 1..48 => ResumeProps);
}

/// Found by `slow_random_commands_keep_invariants`: despawning a claimed
/// place left its claim behind (held by a live villager, on a dead target).
#[test]
fn despawned_target_drops_its_claims() {
    let mut app = new_sim_app(0, "village", 1);
    run_ticks(app.world_mut(), 2);
    let ids = *app.world().resource::<VillageIds>();
    let (field, villager) = (ids.field, ids.villager);
    assert!(
        app.world()
            .resource::<Reservations>()
            .holds(field, villager),
        "precondition: the villager claims the field by tick 2"
    );
    submit(app.world_mut(), None, SimCommand::Despawn { unit: field.0 }).unwrap();
    run_ticks(app.world_mut(), 1);
    let reservations = app.world().resource::<Reservations>();
    assert_eq!(reservations.count(field), 0, "{reservations:?}");
    check_world(app.world_mut(), &mut BTreeMap::new());
}
