//! Debug tools: console, spawn panel, stats, time controls, inspector, perf panel, overlays and
//! the remote debug endpoint (spec §11, §15).
//!
//! ADR-0008: every sim change is a command issued through
//! [`LocalDriver`](crate::driver::LocalDriver); pause, step, speed and loading are driver
//! controls; "reveal all", the player switch and overlays are client-local render overrides;
//! the inspector and the remote methods read through [`SimQuery`](fl_view::SimQuery).
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use fl_content::ContentDb;
use fl_protocol::{PlayerCommand, PlayerId, SimId, Tick, UnitTypeId};
use fl_view::{EntityRecord, FromRunner, SimEvent, SimSnapshot};

pub mod codecs;
pub mod console;
pub mod inspector;
pub mod perf;
#[cfg(all(feature = "debug_remote", debug_assertions))]
pub mod remote;
#[cfg(all(feature = "debug_remote", debug_assertions))]
pub mod screenshot;
pub mod spawn_panel;
pub mod stats;

pub use codecs::{ClientCommandCodecs, CommandCodecAppExt, issue_json};
pub use console::{
    ConsoleAppExt, ConsoleCommands, ConsoleFn, ConsoleState, issue_command, issue_in_world,
    run_console_line,
};
pub use inspector::{InspectorState, inspect};
pub use perf::{PerfPanel, ProfilerSource, frame_stats};
#[cfg(all(feature = "debug_remote", debug_assertions))]
pub use remote::RemoteDebugPlugin;
pub use spawn_panel::{CursorGround, SpawnPanel};

use crate::ai::AiHost;
use crate::driver::LocalDriver;
use crate::link::{
    InputLog, JournalBuffer, PendingReplies, RunnerBacklog, RunnerLink, RunnerStatus, SnapshotAge,
    SnapshotBuffer,
};
use crate::map_view::MapView;
use crate::overlays::OverlayData;
use crate::selection::{ControlGroups, InputGate, Selection};
use crate::view::{ClientContent, SimViewSystems, UnitView, ViewIndex};

/// The headless half of the debug tools: the console commands, the JSON command codecs and the
/// debug resources, and the system that routes inspector replies. Needs no window, input or
/// egui.
pub struct DebugCommandsPlugin;

impl Plugin for DebugCommandsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleCommands>()
            .init_resource::<ConsoleState>()
            .init_resource::<InspectorState>()
            .init_resource::<SpawnPanel>()
            .init_resource::<CursorGround>()
            .init_resource::<OverlayRegistry>()
            .init_resource::<PendingReplies>()
            .add_systems(
                Update,
                inspector::receive_inspector_reply.after(SimViewSystems),
            );
        console::register_builtin_commands(app);
        codecs::register_builtin_codecs(app);
    }
}

/// The debug tools: [`DebugCommandsPlugin`] plus the egui windows (console on backquote or F1,
/// spawn panel, stats, time controls, inspector, perf panel on F2) and click-to-place spawning.
pub struct DebugToolsPlugin;

impl Plugin for DebugToolsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin::default());
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        app.add_plugins(DebugCommandsPlugin)
            .init_resource::<PerfPanel>()
            .add_systems(
                Update,
                (
                    console::toggle_console.run_if(resource_exists::<ButtonInput<KeyCode>>),
                    perf::toggle_perf_panel.run_if(resource_exists::<ButtonInput<KeyCode>>),
                    perf::sample_perf.run_if(|panel: Res<PerfPanel>| panel.open),
                    (
                        spawn_panel::update_cursor_ground,
                        spawn_panel::click_to_place
                            .run_if(resource_exists::<ButtonInput<MouseButton>>),
                    )
                        .chain(),
                ),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    stats::stats_window,
                    stats::time_controls_window,
                    spawn_panel::spawn_panel_window,
                    inspector::inspector_window,
                    perf::perf_window.run_if(|panel: Res<PerfPanel>| panel.open),
                    console::console_window.run_if(|state: Res<ConsoleState>| state.open),
                )
                    .chain(),
            );
    }
}

/// Toggles an overlay; returns whether it is now shown.
pub type OverlayToggle = fn(&mut World) -> bool;

/// One registered overlay.
#[derive(Clone, Copy, Debug)]
struct Overlay {
    key: Option<KeyCode>,
    toggle: OverlayToggle,
}

/// The client-local debug overlays by name, each with an optional toggle key (`"grid"` on F3
/// in M0). Toggled by key or by `fl.toggle_overlay`.
#[derive(Resource, Default)]
pub struct OverlayRegistry {
    overlays: BTreeMap<&'static str, Overlay>,
}

impl OverlayRegistry {
    /// Adds (or replaces) the overlay `name`.
    pub fn insert(&mut self, name: &'static str, key: Option<KeyCode>, toggle: OverlayToggle) {
        self.overlays.insert(name, Overlay { key, toggle });
    }

    /// Every overlay name, in name order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.overlays.keys().copied()
    }

    /// Toggles the overlay `name` of `world`; returns whether it is now shown.
    ///
    /// # Errors
    ///
    /// If no overlay has that name.
    pub fn toggle(world: &mut World, name: &str) -> Result<bool, String> {
        let registry = world.get_resource::<OverlayRegistry>();
        let Some(toggle) = registry
            .and_then(|r| r.overlays.get(name))
            .map(|o| o.toggle)
        else {
            let known: Vec<&str> = registry.map(|r| r.names().collect()).unwrap_or_default();
            return Err(format!(
                "unknown overlay '{name}'; known overlays: {}",
                known.join(", ")
            ));
        };
        Ok(toggle(world))
    }

    fn pressed(&self, keys: &ButtonInput<KeyCode>) -> Vec<OverlayToggle> {
        self.overlays
            .values()
            .filter(|o| o.key.is_some_and(|key| keys.just_pressed(key)))
            .map(|o| o.toggle)
            .collect()
    }
}

/// Registers overlays on an [`App`].
pub trait OverlayAppExt {
    /// Adds the overlay `name`, toggled by `key` (if any) and by `fl.toggle_overlay`.
    fn register_overlay(
        &mut self,
        name: &'static str,
        key: Option<KeyCode>,
        toggle: OverlayToggle,
    ) -> &mut Self;
}

impl OverlayAppExt for App {
    fn register_overlay(
        &mut self,
        name: &'static str,
        key: Option<KeyCode>,
        toggle: OverlayToggle,
    ) -> &mut Self {
        if !self.world().contains_resource::<OverlayKeys>() {
            self.init_resource::<OverlayKeys>().add_systems(
                Update,
                toggle_overlays_by_key
                    .run_if(resource_exists::<ButtonInput<KeyCode>>)
                    .run_if(keyboard_free)
                    .in_set(OverlayToggles),
            );
        }
        self.world_mut()
            .get_resource_or_init::<OverlayRegistry>()
            .insert(name, key, toggle);
        self
    }
}

/// The system that toggles overlays by key, in `Update`; overlay drawing runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverlayToggles;

/// Marks that [`toggle_overlays_by_key`] is scheduled.
#[derive(Resource, Default)]
struct OverlayKeys;

/// Whether no console or egui widget owns the keyboard.
fn keyboard_free(gate: InputGate) -> bool {
    !gate.keyboard_blocked()
}

/// Toggles every overlay whose key was just pressed.
fn toggle_overlays_by_key(world: &mut World) {
    let toggles = match (
        world.get_resource::<OverlayRegistry>(),
        world.get_resource::<ButtonInput<KeyCode>>(),
    ) {
        (Some(registry), Some(keys)) => registry.pressed(keys),
        _ => return,
    };
    for toggle in toggles {
        toggle(world);
    }
}

/// What [`ScenarioLoader`] starts.
#[derive(Clone)]
pub enum LoadRequest {
    /// A scenario of the loaded content, by id.
    Scenario(String),
    /// A save, as its bytes.
    Save(Vec<u8>),
    /// The running match rebuilt under new content (a hot reload whose ID sets changed,
    /// ADR-0012): `save` is the match saved just now, loaded under `content` with the units
    /// whose type vanished dropped. It continues the same match.
    Rebuild {
        save: Vec<u8>,
        content: Arc<ContentDb>,
    },
}

impl fmt::Debug for LoadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadRequest::Scenario(id) => f.debug_tuple("Scenario").field(id).finish(),
            LoadRequest::Save(bytes) => write!(f, "Save({} bytes)", bytes.len()),
            LoadRequest::Rebuild { save, .. } => {
                write!(f, "Rebuild {{ save: {} bytes, .. }}", save.len())
            }
        }
    }
}

/// Game-side state of a newly loaded match that the client cannot name (the game's replay
/// capture, its copy of the terrain), put into the world after the swap.
pub type LoadHook = Box<dyn FnOnce(&mut World)>;

/// A newly started sim runner and what the [`LocalDriver`] needs to drive it.
pub struct LoadedMatch {
    pub link: RunnerLink,
    /// The first tick the new runner runs.
    pub next: Tick,
    /// Commands stamped but not yet delivered when the save was made.
    pub pending: Vec<PlayerCommand>,
    /// The highest command seq the sim took from each player.
    pub last_seq: BTreeMap<PlayerId, u32>,
    /// The new match's map; `None` keeps the current [`MapView`].
    pub map: Option<MapView>,
    /// The AI controllers of the new match's `Controller::Ai` players (ruling R91); `None`
    /// keeps the current [`AiHost`] (a rebuild continues the same match).
    pub ai: Option<AiHost>,
    /// The saved units that were not loaded, with their unit type (a rebuild drops the units
    /// whose type vanished).
    pub dropped: Vec<(SimId, String)>,
    /// Applied to the world once the new match is in place.
    pub hook: Option<LoadHook>,
}

/// Starts a new sim runner for a scenario, a save or a rebuild (`fl.load_scenario`,
/// `fl.load`, hot reload); inserted by the game, which owns the sim.
#[derive(Resource)]
pub struct ScenarioLoader(
    pub Box<dyn Fn(LoadRequest) -> Result<LoadedMatch, String> + Send + Sync>,
);

/// The scenario the running match started from; `None` for a match without one (a stress run
/// or a loaded save).
#[derive(Resource, Clone, Default, Debug, PartialEq, Eq)]
pub struct CurrentScenario(pub Option<String>);

/// What [`replace_match`] started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replaced {
    /// The first tick of the new match.
    pub next: Tick,
    /// [`LoadedMatch::dropped`].
    pub dropped: Vec<(SimId, String)>,
}

/// [`replace_match`], returning the first tick of the new match.
///
/// # Errors
///
/// As [`replace_match`].
pub fn load_match(world: &mut World, request: LoadRequest) -> Result<Tick, String> {
    replace_match(world, request).map(|replaced| replaced.next)
}

/// Replaces the running match: starts `request` with the [`ScenarioLoader`], swaps in its
/// [`RunnerLink`] (the old runner stops when its link drops), restores the [`LocalDriver`],
/// and inserts the new [`MapView`] and [`AiHost`] when the loader gives them. Pause and speed
/// are kept. Blocks until the new sim is built.
///
/// Every query in flight to the old runner is forgotten (its reply never arrives), the
/// overlays' polls and the inspector's included, and query ids are never reused.
///
/// A scenario or save is a new match: what the client received from or awaits of the old
/// runner is cleared, with the [`InputLog`], [`CurrentScenario`], the [`Selection`] and the
/// [`ControlGroups`]. A rebuild continues the match, so the received snapshots, the journal,
/// the runner status and backlog (tick outputs the old runner sent before its save), the input
/// log, the current scenario, the selection and the groups stay; only replies the old runner
/// owed are dropped. Since the new content renumbers its dense unit ids, every unit id the
/// client holds (snapshots, backlog, views) is mapped to the new content by name, the dropped
/// units vanish from them, and the new [`ClientContent`] is put in place.
///
/// # Errors
///
/// If there is no loader or driver, or the loader fails; the running match is then kept.
pub fn replace_match(world: &mut World, request: LoadRequest) -> Result<Replaced, String> {
    if !world.contains_resource::<LocalDriver>() {
        return Err("no match is running".to_owned());
    }
    let scenario = match &request {
        LoadRequest::Scenario(id) => Some(Some(id.clone())),
        LoadRequest::Save(_) => Some(None),
        LoadRequest::Rebuild { .. } => None,
    };
    let rebuilt_content = match &request {
        LoadRequest::Rebuild { content, .. } => Some(content.clone()),
        _ => None,
    };
    let loaded = {
        let loader = world
            .get_resource::<ScenarioLoader>()
            .ok_or("this game cannot load matches (no ScenarioLoader)")?;
        (loader.0)(request)?
    };
    let next = loaded.next;
    world.insert_resource(loaded.link);
    if let Some(map) = loaded.map {
        world.insert_resource(map);
    }
    if let Some(ai) = loaded.ai {
        world.insert_resource(ai);
    }
    world
        .resource_mut::<LocalDriver>()
        .restore(next, loaded.pending, &loaded.last_seq);
    // Replies of the old runner never arrive; nothing waits for them. Query ids stay unique,
    // so no holder of an old id can claim a new reply.
    world
        .get_resource_or_init::<PendingReplies>()
        .forget_runner();
    if let Some(mut overlays) = world.get_resource_mut::<OverlayData>() {
        overlays.forget_runner();
    }
    if let Some(new) = rebuilt_content {
        remap_rebuilt(world, new, &loaded.dropped);
    }
    if let Some(scenario) = scenario {
        world.insert_resource(SnapshotBuffer::default());
        world.insert_resource(SnapshotAge::default());
        world.insert_resource(RunnerStatus::default());
        world.insert_resource(RunnerBacklog::default());
        world.insert_resource(JournalBuffer::default());
        world.insert_resource(CurrentScenario(scenario));
        // The new match's sim ids name unrelated units (final review M9).
        if world.contains_resource::<Selection>() {
            world.insert_resource(Selection::default());
        }
        if world.contains_resource::<ControlGroups>() {
            world.insert_resource(ControlGroups::default());
        }
        if world.contains_resource::<InputLog>() {
            world.insert_resource(InputLog::default());
        }
    }
    if let Some(mut inspector) = world.get_resource_mut::<InspectorState>() {
        inspector.pending = None;
    }
    if let Some(hook) = loaded.hook {
        hook(world);
    }
    Ok(Replaced {
        next,
        dropped: loaded.dropped,
    })
}

/// After a rebuild under `new`: maps every dense unit id the client holds from the current
/// [`ClientContent`] to `new` by name (ADR-0004: dense ids never outlive their content), and
/// removes the `dropped` units, in the [`SnapshotBuffer`], the [`RunnerBacklog`]'s tick
/// outputs (snapshots and spawn events) and the [`UnitView`]s; then makes `new` the client's
/// content. A unit whose type `new` lacks is removed too.
fn remap_rebuilt(world: &mut World, new: Arc<ContentDb>, dropped: &[(SimId, String)]) {
    let Some(old) = world.get_resource::<ClientContent>().map(|c| c.0.clone()) else {
        world.insert_resource(ClientContent(new));
        return;
    };
    let ids: Vec<Option<UnitTypeId>> = old
        .units()
        .iter()
        .map(|unit| new.unit_id(&unit.id))
        .collect();
    let map = |id: UnitTypeId| ids.get(usize::from(id.0)).copied().flatten();
    let gone: BTreeSet<SimId> = dropped.iter().map(|(sim_id, _)| *sim_id).collect();
    let snapshot = |s: &SimSnapshot| -> Arc<SimSnapshot> {
        Arc::new(SimSnapshot {
            tick: s.tick,
            entities: s
                .entities
                .iter()
                .filter(|r| !gone.contains(&r.sim_id))
                .filter_map(|r| {
                    Some(EntityRecord {
                        unit: map(r.unit)?,
                        ..r.clone()
                    })
                })
                .collect(),
            channels: s.channels.clone(),
        })
    };

    if let Some(mut buffer) = world.get_resource_mut::<SnapshotBuffer>() {
        buffer.prev = buffer.prev.as_deref().map(snapshot);
        buffer.curr = buffer.curr.as_deref().map(snapshot);
    }
    if let Some(mut backlog) = world.get_resource_mut::<RunnerBacklog>() {
        for message in &mut backlog.0 {
            let FromRunner::Tick(output) = message else {
                continue;
            };
            output.snapshot = snapshot(&output.snapshot);
            output.events.retain_mut(|event| match event {
                SimEvent::Spawned { sim_id, unit, .. } => {
                    *unit = unit.and_then(map);
                    !gone.contains(sim_id)
                }
                _ => true,
            });
        }
    }
    let mut views = world.query::<(Entity, &mut UnitView)>();
    let mut stale = Vec::new();
    for (entity, mut view) in views.iter_mut(world) {
        match map(view.unit).filter(|_| !gone.contains(&view.sim_id)) {
            Some(unit) if unit != view.unit => view.unit = unit,
            Some(_) => {}
            None => stale.push((entity, view.sim_id)),
        }
    }
    for (entity, sim_id) in stale {
        // Despawns the view's placeholder parts with it.
        let _ = world.try_despawn(entity);
        if let Some(mut index) = world.get_resource_mut::<ViewIndex>() {
            index.0.remove(&sim_id);
        }
    }
    world.insert_resource(ClientContent(new));
}
