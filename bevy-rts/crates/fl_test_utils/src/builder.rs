//! [`ScenarioBuilder`] and the [`Scenario`] it builds.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use bevy_ecs::world::World;
use fl_content::{ContentDb, Faction};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandKind, CommandPayload, Controller, DebugWanderCmd, FactionId, PlayerId, SimId, TeamId,
    Tick,
};
use fl_sim::{
    CommandRegistry, HashInterval, MapInfo, MatchHeaderRes, MatchSetup, Owner, PlayerInfo, Players,
    Pose, Replay, ReplayRecorder, ReplayStart, RunConfig, ScaleScenario, SimFeature, SimIndex,
    SimRunner, SpawnRequest, TerrainGrid, TestDriver, build_sim_app, load_world, rebuild_spatial,
    save_world, setup_map, setup_scenario, setup_spawn,
};
use fl_view::{CommandDraft, JournalEntry, SimQuery, SimReply, TickDriver, TickOutput};

/// Terrain cell side of a [`ScenarioBuilder::map_flat`] map, in metres.
const FLAT_CELL_M: i32 = 4;

/// The map id of [`ScenarioBuilder::ascii_map`]'s content.
const ASCII_MAP: &str = "t.ascii";

/// The base content with the game's features ([`fl_catalog::features`] and
/// [`fl_catalog::base_mod_dir`]), loaded once per process.
///
/// # Panics
/// If the base content does not load.
pub fn base_content() -> Arc<ContentDb> {
    static CONTENT: OnceLock<Arc<ContentDb>> = OnceLock::new();
    Arc::clone(CONTENT.get_or_init(|| {
        fl_sim::load_content(&fl_catalog::features(), &[fl_catalog::base_mod_dir()])
            .unwrap_or_else(|err| panic!("the base content does not load: {err}"))
    }))
}

/// Encodes a scheduled command's payload once the sim's command registry exists.
type Encode = Box<dyn Fn(&CommandRegistry) -> CommandKind>;

/// A command the builder issues through the driver when its tick comes up.
struct Scheduled {
    tick: u32,
    player: u8,
    units: Vec<u32>,
    encode: Encode,
    queue: bool,
}

/// Sets up a sim for a test, then [`build`](ScenarioBuilder::build)s a [`Scenario`].
///
/// Setup runs in this order: the scenario or the scale scenario (not both), the flat or ASCII
/// map, the players, the units, then the wander orders. Every setup error panics with its
/// text.
pub struct ScenarioBuilder {
    content: Arc<ContentDb>,
    features: Vec<Arc<dyn SimFeature>>,
    seed: Option<u64>,
    cheats: bool,
    hash_interval: u32,
    shuffle_seed: Option<u64>,
    flow_cache_cold: bool,
    scenario: Option<String>,
    scale: Option<ScaleScenario>,
    map: Option<(i32, i32)>,
    ascii: Option<(Vec<String>, i32)>,
    ascii_rules: Option<String>,
    players: Vec<(String, u8)>,
    units: Vec<(String, u8, i32, i32)>,
    wander: Option<i32>,
    commands: Vec<Scheduled>,
}

impl Default for ScenarioBuilder {
    fn default() -> Self {
        ScenarioBuilder::new()
    }
}

impl ScenarioBuilder {
    /// [`base_content`] with the game's features; seed 0 (or the scenario's), a state hash
    /// every tick, `debug.*` commands allowed, no schedule shuffle, nothing set up.
    pub fn new() -> Self {
        ScenarioBuilder::with_content(base_content(), fl_catalog::features())
    }

    /// As [`ScenarioBuilder::new`], with `content` loaded for `features`.
    pub fn with_content(content: Arc<ContentDb>, features: Vec<Arc<dyn SimFeature>>) -> Self {
        ScenarioBuilder {
            content,
            features,
            seed: None,
            cheats: true,
            hash_interval: 1,
            shuffle_seed: None,
            flow_cache_cold: false,
            scenario: None,
            scale: None,
            map: None,
            ascii: None,
            ascii_rules: None,
            players: Vec::new(),
            units: Vec::new(),
            wander: None,
            commands: Vec::new(),
        }
    }

    /// Add sim feature `f` (the content must have been loaded with it).
    #[must_use]
    pub fn feature(mut self, f: Arc<dyn SimFeature>) -> Self {
        self.features.push(f);
        self
    }

    /// The match seed, overriding the scenario's (and the scale scenario's).
    #[must_use]
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Whether `debug.*` commands are accepted (the builder's own wander orders need them).
    #[must_use]
    pub fn cheats(mut self, on: bool) -> Self {
        self.cheats = on;
        self
    }

    /// Hash the state every `n` ticks; 0 turns hashing off.
    #[must_use]
    pub fn hash_interval(mut self, n: u32) -> Self {
        self.hash_interval = n;
        self
    }

    /// Shuffle the order of unordered `SimTick` systems with `seed`.
    #[must_use]
    pub fn shuffle_seed(mut self, seed: Option<u64>) -> Self {
        self.shuffle_seed = seed;
        self
    }

    /// Empty the flow-field cache at the start of every tick
    /// ([`RunConfig::flow_cache_cold`](fl_sim::RunConfig::flow_cache_cold)); a loaded save keeps
    /// the setting ([`Scenario::reload`]).
    #[must_use]
    pub fn flow_cache_cold(mut self, cold: bool) -> Self {
        self.flow_cache_cold = cold;
        self
    }

    /// Set up scenario `id` (its map, players and units); its seed is the match seed unless
    /// [`seed`](Self::seed) says otherwise.
    #[must_use]
    pub fn scenario(mut self, id: &str) -> Self {
        self.scenario = Some(id.to_owned());
        self
    }

    /// A flat `width_m` × `height_m` map, replacing the scenario's.
    #[must_use]
    pub fn map_flat(mut self, width_m: i32, height_m: i32) -> Self {
        self.map = Some((width_m, height_m));
        self
    }

    /// The map `"t.ascii"` drawn by `rows` with `cell_m`-metre cells
    /// ([`ascii_terrain_content`](crate::ascii_terrain_content)). It replaces the builder's
    /// content: the sim runs on the ASCII mod (factions `"t.red"` and `"t.blue"`, units
    /// `"t.tank"`, `"t.truck"`, `"t.rifleman"`, `"t.wide_tank"`), so it does not combine with
    /// a scenario, the scale scenario or a flat map.
    #[must_use]
    pub fn ascii_map(mut self, rows: &[&str], cell_m: i32) -> Self {
        self.ascii = Some((rows.iter().map(|&row| row.to_owned()).collect(), cell_m));
        self
    }

    /// The ASCII mod's `rules.ron` (default: the base rules); see
    /// [`ascii_map`](Self::ascii_map).
    #[must_use]
    pub fn ascii_rules(mut self, rules_ron: &str) -> Self {
        self.ascii_rules = Some(rules_ron.to_owned());
        self
    }

    /// A human player of faction `faction` on team `team`. Players get the `PlayerId`s after
    /// the scenario's, in call order.
    #[must_use]
    pub fn player(mut self, faction: &str, team: u8) -> Self {
        self.players.push((faction.to_owned(), team));
        self
    }

    /// A unit of type `unit` for `player` at `(x, y)` metres, heading 0. Builder units get the
    /// `SimId`s after the scenario's units, in call order.
    #[must_use]
    pub fn unit(mut self, unit: &str, player: u8, x: i32, y: i32) -> Self {
        self.units.push((unit.to_owned(), player, x, y));
        self
    }

    /// Set up the scale scenario `s`; its seed is the match seed unless
    /// [`seed`](Self::seed) says otherwise (which then places the units too).
    #[must_use]
    pub fn scale(mut self, s: ScaleScenario) -> Self {
        self.scale = Some(s);
        self
    }

    /// At tick 0, a `debug.wander` with radius `radius_m` for each player that owns units,
    /// naming all of them.
    #[must_use]
    pub fn wander_all(mut self, radius_m: i32) -> Self {
        self.wander = Some(radius_m);
        self
    }

    /// `player` orders `units` to carry out `payload`, issued through the [`TestDriver`]
    /// before tick `tick` runs (input delay 0), so it applies in tick `tick`.
    #[must_use]
    pub fn command<P: CommandPayload>(
        self,
        tick: u32,
        player: u8,
        units: &[u32],
        payload: P,
    ) -> Self {
        self.schedule(tick, player, units, payload, false)
    }

    /// As [`command`](Self::command), Shift-queued: the order is appended to the units'
    /// orders instead of replacing them.
    #[must_use]
    pub fn queued_command<P: CommandPayload>(
        self,
        tick: u32,
        player: u8,
        units: &[u32],
        payload: P,
    ) -> Self {
        self.schedule(tick, player, units, payload, true)
    }

    fn schedule<P: CommandPayload>(
        mut self,
        tick: u32,
        player: u8,
        units: &[u32],
        payload: P,
        queue: bool,
    ) -> Self {
        self.commands.push(Scheduled {
            tick,
            player,
            units: units.to_vec(),
            encode: Box::new(move |registry| registry.encode(&payload)),
            queue,
        });
        self
    }

    /// Build the sim and set it up.
    ///
    /// The [`Scenario`] records a [`Replay`] that starts from the scenario
    /// ([`ReplayStart::Scenario`]) or the scale scenario ([`ReplayStart::Scale`]) when the
    /// builder set up nothing else, and otherwise from a save of the world as set up
    /// ([`ReplayStart::Save`]).
    ///
    /// # Panics
    /// With the error's text if the sim does not build or the setup fails, and if both a
    /// scenario and a scale scenario are set.
    pub fn build(self) -> Scenario {
        assert!(
            self.scenario.is_none() || self.scale.is_none(),
            "a scenario and a scale scenario both set up the map and the players: pick one"
        );
        assert!(
            self.map.is_none() || self.ascii.is_none(),
            "a flat map and an ASCII map: pick one"
        );
        let content = match &self.ascii {
            Some((rows, cell_m)) => {
                let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
                crate::ascii_terrain_content_with(&rows, *cell_m, self.ascii_rules.as_deref())
            }
            None => self.content,
        };
        let scenario_seed = match (&self.scenario, &self.scale) {
            (Some(id), _) => {
                MatchSetup::for_scenario(&content, id)
                    .unwrap_or_else(|err| panic!("{err}"))
                    .seed
            }
            (None, Some(scale)) => scale.seed,
            (None, None) => 0,
        };
        let seed = self.seed.unwrap_or(scenario_seed);
        let setup = MatchSetup {
            seed,
            cheats_allowed: self.cheats,
        };
        // Built from the default so that fields later tasks add keep their defaults.
        let mut run = RunConfig {
            hash_interval: Some(self.hash_interval),
            ..RunConfig::default()
        };
        run.shuffle_seed = self.shuffle_seed;
        run.flow_cache_cold = self.flow_cache_cold;
        let mut app = build_sim_app(&self.features, Arc::clone(&content), setup, run.clone())
            .unwrap_or_else(|err| panic!("the sim does not build: {err}"));
        let world = app.world_mut();

        if let Some(id) = &self.scenario {
            setup_scenario(world, id).unwrap_or_else(|err| panic!("{err}"));
        }
        let scale = self.scale.map(|scale| ScaleScenario { seed, ..scale });
        if let Some(scale) = &scale {
            scale.populate(world).unwrap_or_else(|err| panic!("{err}"));
        }
        if let Some((width, height)) = self.map {
            let size = FxVec2::new(Fx::from_int(width), Fx::from_int(height));
            let map = MapInfo::flat(size, Fx::from_int(FLAT_CELL_M));
            world.insert_resource(TerrainGrid::open(map.cells_x, map.cells_y));
            world.insert_resource(map);
            rebuild_spatial(world);
            fl_sim::nav::rebuild_nav(world);
        }
        if self.ascii.is_some() {
            setup_map(world, ASCII_MAP).unwrap_or_else(|err| panic!("{err}"));
        }
        for (faction, team) in &self.players {
            add_player(world, &content, faction, *team);
        }
        for (index, (unit, player, x, y)) in self.units.iter().enumerate() {
            let request = SpawnRequest {
                unit: Some(
                    content
                        .unit_id(unit)
                        .unwrap_or_else(|| panic!("builder unit {index}: no unit '{unit}'")),
                ),
                owner: PlayerId(*player),
                pos: FxVec2::new(Fx::from_int(*x), Fx::from_int(*y)),
                heading: Angle::ZERO,
                init: None,
            };
            setup_spawn(world, &request).unwrap_or_else(|err| {
                panic!("builder unit {index} ('{unit}') cannot spawn: {err}")
            });
        }

        let own_setup = self.map.is_some()
            || self.ascii.is_some()
            || !self.players.is_empty()
            || !self.units.is_empty();
        let start = match (&self.scenario, scale) {
            (Some(id), None) if !own_setup => ReplayStart::Scenario(id.clone()),
            (None, Some(scale)) if !own_setup => ReplayStart::scale(&scale),
            _ => ReplayStart::Save(save_world(world, &[]).expect("a fresh setup saves")),
        };
        let header = world.resource::<MatchHeaderRes>().0.clone();
        let recorder = ReplayRecorder::new(header, start, self.hash_interval);

        let mut scheduled: BTreeMap<Tick, Vec<CommandDraft>> = BTreeMap::new();
        let registry = world.resource::<CommandRegistry>();
        if let Some(radius) = self.wander {
            let payload = DebugWanderCmd {
                radius: Fx::from_int(radius),
            };
            for (player, units) in units_by_owner(world) {
                scheduled.entry(Tick(0)).or_default().push(CommandDraft {
                    player,
                    units,
                    kind: registry.encode(&payload),
                    queue: false,
                });
            }
        }
        for command in &self.commands {
            scheduled
                .entry(Tick(command.tick))
                .or_default()
                .push(CommandDraft {
                    player: PlayerId(command.player),
                    units: command.units.iter().map(|&id| SimId(id)).collect(),
                    kind: (command.encode)(registry),
                    queue: command.queue,
                });
        }

        Scenario {
            runner: SimRunner::new(app),
            driver: TestDriver::new(),
            scheduled,
            hashes: Vec::new(),
            last_output: None,
            journal: Vec::new(),
            recorder,
            features: self.features,
            content,
            run,
        }
    }
}

/// Append a human player of `faction` on `team` to [`Players`].
fn add_player(world: &mut World, content: &ContentDb, faction: &str, team: u8) {
    let faction_id = content
        .kind::<Faction>()
        .index_of(faction)
        .unwrap_or_else(|| panic!("faction '{faction}' does not exist"));
    let mut players = world.resource_mut::<Players>();
    let id = u8::try_from(players.list.len()).expect("at most 256 players");
    players.list.push(PlayerInfo {
        id: PlayerId(id),
        faction: FactionId(u16::try_from(faction_id).expect("fewer than 65536 factions")),
        team: TeamId(team),
        controller: Controller::Human,
    });
}

/// Every unit's id, grouped by owner, in `SimId` order.
fn units_by_owner(world: &World) -> BTreeMap<PlayerId, Vec<SimId>> {
    let mut owned: BTreeMap<PlayerId, Vec<SimId>> = BTreeMap::new();
    for (id, entity) in world.resource::<SimIndex>().iter() {
        if let Some(Owner(player)) = world.get::<Owner>(entity) {
            owned.entry(*player).or_default().push(id);
        }
    }
    owned
}

/// A built sim: its runner, the [`TestDriver`] that feeds it, the builder's commands not yet
/// issued, and every state hash, journal entry and tick input and output so far (as a
/// [`Replay`]), and the last output.
pub struct Scenario {
    pub runner: SimRunner,
    pub driver: TestDriver,
    /// Commands to issue before their tick runs.
    pub(crate) scheduled: BTreeMap<Tick, Vec<CommandDraft>>,
    hashes: Vec<(Tick, u64)>,
    last_output: Option<TickOutput>,
    journal: Vec<JournalEntry>,
    recorder: ReplayRecorder,
    features: Vec<Arc<dyn SimFeature>>,
    content: Arc<ContentDb>,
    run: RunConfig,
}

impl Scenario {
    /// Run `n` ticks. Before each, the builder's commands due by then are issued.
    pub fn run_ticks(&mut self, n: u32) {
        for _ in 0..n {
            let tick = self.driver.next_tick();
            while let Some(entry) = self.scheduled.first_entry() {
                if *entry.key() > tick {
                    break;
                }
                for draft in entry.remove() {
                    self.driver.issue(draft);
                }
            }
            let input = self.driver.poll().expect("the test driver always ticks");
            let output = self.runner.step(input.clone());
            self.recorder.record(&input, &output);
            if let Some(hash) = output.hash {
                self.hashes.push((output.tick, hash));
            }
            self.journal.extend(output.journal.iter().cloned());
            self.last_output = Some(output);
        }
    }

    /// The tick the next [`run_ticks`](Self::run_ticks) starts with: the number of ticks run
    /// since tick 0.
    pub fn tick(&self) -> Tick {
        self.runner.next_tick()
    }

    /// Every state hash taken so far, as `(tick, hash)`.
    pub fn hashes(&self) -> Vec<(Tick, u64)> {
        self.hashes.clone()
    }

    /// Every journal entry of the ticks run so far, in tick order (empty unless the journal
    /// is on: in debug builds).
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    /// The replay of the ticks run so far.
    pub fn replay(&self) -> Replay {
        self.recorder.clone().finish()
    }

    /// The features the sim was built with.
    pub fn features(&self) -> &[Arc<dyn SimFeature>] {
        &self.features
    }

    /// The output of the last tick that ran.
    pub fn last_output(&self) -> Option<&TickOutput> {
        self.last_output.as_ref()
    }

    /// The sim world.
    pub fn world(&self) -> &World {
        self.runner.world()
    }

    /// The sim world, mutably: for setup between ticks (such as
    /// [`setup_despawn`](fl_sim::setup_despawn)) in tests.
    pub fn world_mut(&mut self) -> &mut World {
        self.runner.world_mut()
    }

    /// The [`Pose`] of unit `id`, if it is alive and has one.
    pub fn pose(&self, id: u32) -> Option<Pose> {
        let world = self.world();
        let entity = world.resource::<SimIndex>().get(SimId(id))?;
        world.get::<Pose>(entity).copied()
    }

    /// `player` orders `units` to carry out `payload` from the next tick on.
    pub fn issue<P: CommandPayload>(&mut self, player: u8, units: &[u32], payload: P, queue: bool) {
        let kind = self.world().resource::<CommandRegistry>().encode(&payload);
        self.driver.issue(CommandDraft {
            player: PlayerId(player),
            units: units.iter().map(|&id| SimId(id)).collect(),
            kind,
            queue,
        });
    }

    /// The save of the sim now, through `SimQuery::Save` with the driver's pending commands.
    ///
    /// # Panics
    /// If the runner does not answer with a save.
    pub fn save(&mut self) -> Vec<u8> {
        let pending = self.driver.pending();
        match self.runner.query(SimQuery::Save { pending }) {
            SimReply::Save(bytes) => bytes,
            other => panic!("the save query failed: {other:?}"),
        }
    }

    /// The sim saved in `bytes`, loaded with `features` and `content`, a state hash every
    /// tick and no shuffle; its driver resumes at the saved tick with the saved pending
    /// commands and seqs. Its replay starts from the save ([`ReplayStart::Save`]).
    ///
    /// # Panics
    /// With the load error's text.
    pub fn load(
        features: Vec<Arc<dyn SimFeature>>,
        content: Arc<ContentDb>,
        bytes: &[u8],
    ) -> Scenario {
        let run = RunConfig {
            hash_interval: Some(1),
            ..RunConfig::default()
        };
        Scenario::load_with(features, content, bytes, run)
    }

    /// [`Scenario::load`] with this scenario's features, content and run config.
    ///
    /// # Panics
    /// With the load error's text.
    pub fn reload(&self, bytes: &[u8]) -> Scenario {
        Scenario::load_with(
            self.features.clone(),
            Arc::clone(&self.content),
            bytes,
            self.run.clone(),
        )
    }

    fn load_with(
        features: Vec<Arc<dyn SimFeature>>,
        content: Arc<ContentDb>,
        bytes: &[u8],
        run: RunConfig,
    ) -> Scenario {
        let loaded = load_world(&features, Arc::clone(&content), bytes, run.clone())
            .unwrap_or_else(|err| panic!("the save does not load: {err}"));
        let header = loaded.app.world().resource::<MatchHeaderRes>().0.clone();
        let interval = loaded.app.world().resource::<HashInterval>().0;
        let recorder = ReplayRecorder::new(header, ReplayStart::Save(bytes.to_vec()), interval);
        let runner = SimRunner::new(loaded.app);
        let mut driver = TestDriver::new().starting_at(runner.next_tick());
        driver.restore(loaded.pending, &loaded.last_seq);
        Scenario {
            runner,
            driver,
            scheduled: BTreeMap::new(),
            hashes: Vec::new(),
            last_output: None,
            journal: Vec::new(),
            recorder,
            features,
            content,
            run,
        }
    }
}
