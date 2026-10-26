//! Hot reload, game side (ADR-0012 and its M1 refinement): the content file watcher, both
//! reload paths, and the visual refresh that follows a reload or a `debug.set_terrain`.
//!
//! [`reload_content`] loads the content again and compares it with the running match's:
//! - ID sets and rules hash unchanged: only the visuals changed, so only the client's content
//!   is swapped ([`ReloadOutcome::VisualsOnly`]);
//! - ID sets unchanged: the runner applies the content in place between ticks
//!   (`RunnerControl::ReloadContent`, [`ReloadOutcome::Values`]);
//! - ID sets changed: the match is saved and loaded again under the new content, dropping the
//!   units whose type vanished (`LoadRequest::Rebuild`, [`ReloadOutcome::Rebuilt`]).
//!
//! A reload never runs in a lockstep match ([`NetDriven`]) or one without `cheats_allowed`. A
//! reload that fails (content that does not load, a save that cannot be taken, a rebuild
//! that does not load) leaves the running match as it was. Values and rebuild reloads append
//! `ReplayEvent::ContentReload` to the [`ReplayCapture`].
//!
//! Waits for the runner are bounded ([`ContentSource::reply_timeout`]): a runner that does not
//! answer in time fails the reload, and its late reply is dropped. A timed-out value reload is
//! still recorded as a replay event, since the runner may apply it after all.

use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use fl_client::LocalDriver;
use fl_client::debug::{ConsoleAppExt, ConsoleState, LoadRequest, replace_match};
use fl_client::link::{
    PendingReplies, REPLY_WAIT, RunnerLink, SimJournalMsg, send_control, send_query, wait_for_reply,
};
use fl_client::map_view::MapView;
use fl_client::terrain::TerrainView;
use fl_client::view::{ClientContent, SimViewSystems};
use fl_content::{ContentDb, KindTable, MapDef, TerrainDef};
use fl_protocol::{SimId, Tick};
use fl_sim::{CellRect, MapInfo, ReplayEvent, RunConfig, SimFeature, TerrainGrid, load_content};
use fl_view::{OpaqueContent, RunnerControl, SimQuery, SimReply, TickDriver};
use serde_json::Value;

use crate::builder::scenario_loader;
use crate::replay_capture::ReplayCapture;

/// The ground colour of a match without a map file.
const DEFAULT_GROUND: (f32, f32, f32) = (0.22, 0.27, 0.16);

/// What a hot reload loads the content from, and how the game starts matches with it.
#[derive(Resource, Clone)]
pub struct ContentSource {
    pub features: Vec<Arc<dyn SimFeature>>,
    /// The mod directories, in load order.
    pub mods: Vec<PathBuf>,
    pub run: RunConfig,
    /// Whether matches the game starts allow cheats (the testbed's do).
    pub cheats: bool,
    /// The longest a reload waits for the runner's answer.
    pub reply_timeout: Duration,
}

impl ContentSource {
    /// The source of `features` and `mods`, waiting [`REPLY_WAIT`] for the runner.
    #[must_use]
    pub fn new(
        features: Vec<Arc<dyn SimFeature>>,
        mods: Vec<PathBuf>,
        run: RunConfig,
        cheats: bool,
    ) -> Self {
        ContentSource {
            features,
            mods,
            run,
            cheats,
            reply_timeout: REPLY_WAIT,
        }
    }
}

/// Marks a match driven by a `LockstepDriver` over a net session. Such a match never
/// hot-reloads (ADR-0012), whatever its `cheats_allowed`: the reload would reach one peer's
/// runner only and desync it. A multiplayer shell inserts it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NetDriven;

/// What [`reload_content`] did.
#[derive(Debug, Clone, PartialEq)]
pub enum ReloadOutcome {
    /// Applied in place: the match runs on from `tick` under the content with `rules_hash`.
    Values { tick: Tick, rules_hash: u64 },
    /// The ID sets changed: the match was rebuilt from a save at `tick`, without the units
    /// whose type vanished (`dropped`, in `SimId` order).
    Rebuilt {
        tick: Tick,
        dropped: Vec<(SimId, String)>,
    },
    /// Only content outside the rules (visuals) changed: the client's content was swapped.
    VisualsOnly,
    /// The reload failed; the match keeps running as it was.
    Failed(String),
    /// The match does not allow hot reload.
    Refused(String),
}

impl fmt::Display for ReloadOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReloadOutcome::Values { tick, rules_hash } => write!(
                f,
                "content reloaded in place at tick {} (rules hash {rules_hash:016x})",
                tick.0
            ),
            ReloadOutcome::Rebuilt { tick, dropped } => {
                write!(
                    f,
                    "content ids changed: match rebuilt at tick {}, {} unit(s) dropped",
                    tick.0,
                    dropped.len()
                )?;
                for (sim_id, unit) in dropped {
                    write!(f, "\n  dropped {sim_id} ({unit})")?;
                }
                Ok(())
            }
            ReloadOutcome::VisualsOnly => f.write_str("visuals reloaded; the rules are unchanged"),
            ReloadOutcome::Failed(error) => {
                write!(f, "reload failed; the match keeps running:\n{error}")
            }
            ReloadOutcome::Refused(reason) => write!(f, "reload refused: {reason}"),
        }
    }
}

/// Reloads the content of the running match from its [`ContentSource`] (see the module docs),
/// blocking until the runner has answered. The outcome is logged.
pub fn reload_content(world: &mut World) -> ReloadOutcome {
    let outcome = reload(world);
    match &outcome {
        ReloadOutcome::Values { .. } | ReloadOutcome::VisualsOnly => {
            tracing::info!(%outcome, "hot reload");
        }
        ReloadOutcome::Rebuilt { tick, dropped } => {
            for (sim_id, unit) in dropped {
                tracing::warn!(
                    sim_id = sim_id.0,
                    unit,
                    "hot reload dropped a unit whose type vanished"
                );
            }
            tracing::info!(
                tick = tick.0,
                dropped = dropped.len(),
                "hot reload rebuilt the match"
            );
        }
        ReloadOutcome::Failed(_) | ReloadOutcome::Refused(_) => {
            tracing::warn!(%outcome, "hot reload");
        }
    }
    outcome
}

fn reload(world: &mut World) -> ReloadOutcome {
    if world.contains_resource::<NetDriven>() {
        return ReloadOutcome::Refused("a lockstep match never hot-reloads (ADR-0012)".to_owned());
    }
    let Some(link) = world.get_resource::<RunnerLink>() else {
        return ReloadOutcome::Refused("no match is running".to_owned());
    };
    if !link.header.cheats_allowed {
        return ReloadOutcome::Refused("hot reload needs a match with cheats allowed".to_owned());
    }
    if !world.contains_resource::<LocalDriver>() {
        return ReloadOutcome::Refused("no local driver runs the match".to_owned());
    }
    let Some(source) = world.get_resource::<ContentSource>().cloned() else {
        return ReloadOutcome::Failed(
            "this game cannot reload content (no ContentSource)".to_owned(),
        );
    };
    let Some(current) = world.get_resource::<ClientContent>().map(|c| c.0.clone()) else {
        return ReloadOutcome::Failed("the client has no content".to_owned());
    };
    let new = match load_content(&source.features, &source.mods) {
        Ok(content) => content,
        Err(err) => return ReloadOutcome::Failed(err.to_string()),
    };
    if !new.same_id_sets(&current) {
        return rebuild(world, &source, new);
    }
    if new.rules_hash() == current.rules_hash() {
        adopt(world, &source, new);
        return ReloadOutcome::VisualsOnly;
    }
    reload_values(world, &source, new)
}

/// The values path: the runner applies `new` in place.
fn reload_values(world: &mut World, source: &ContentSource, new: Arc<ContentDb>) -> ReloadOutcome {
    world.init_resource::<PendingReplies>();
    // The client sends no tick while it waits, so this is the tick the runner applies it on.
    let next = world.resource::<LocalDriver>().next_tick();
    let control = RunnerControl::ReloadContent(OpaqueContent(new.clone()));
    let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
        send_control(world.resource::<RunnerLink>(), &mut replies, control)
    });
    match wait_for_reply(world, id, source.reply_timeout) {
        Ok(SimReply::Reloaded { tick, rules_hash }) => {
            world.resource_mut::<RunnerLink>().header.rules_hash = rules_hash;
            adopt(world, source, new);
            record_reload(world, tick, rules_hash);
            ReloadOutcome::Values { tick, rules_hash }
        }
        Ok(SimReply::Error(error)) => ReloadOutcome::Failed(error),
        Ok(other) => ReloadOutcome::Failed(format!("unexpected reply to the reload: {other:?}")),
        // The runner may still apply the content. A replay that falsely says it cannot be
        // reproduced is safe; one that falsely says it can is not: record the reload. The next
        // reload sends the content again.
        Err(error) => {
            record_reload(world, next, new.rules_hash());
            ReloadOutcome::Failed(format!(
                "{error}; the client keeps its content, reload again once the runner answers"
            ))
        }
    }
}

/// The rebuild path: save the match, load it under `new` dropping vanished unit types.
fn rebuild(world: &mut World, source: &ContentSource, new: Arc<ContentDb>) -> ReloadOutcome {
    world.init_resource::<PendingReplies>();
    let pending = world.resource::<LocalDriver>().pending();
    let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
        send_query(
            world.resource::<RunnerLink>(),
            &mut replies,
            SimQuery::Save { pending },
        )
    });
    let save = match wait_for_reply(world, id, source.reply_timeout) {
        Ok(SimReply::Save(bytes)) => bytes,
        Ok(SimReply::Error(error)) => {
            return ReloadOutcome::Failed(format!(
                "the match cannot be saved for the rebuild: {error}"
            ));
        }
        Ok(other) => {
            return ReloadOutcome::Failed(format!("unexpected reply to the save: {other:?}"));
        }
        Err(error) => return ReloadOutcome::Failed(error),
    };
    let request = LoadRequest::Rebuild {
        save,
        content: new.clone(),
    };
    // The save names no map, but the running match's map is known.
    let map = world
        .get_resource::<MatchTerrain>()
        .and_then(|terrain| terrain.map.clone());
    match replace_match(world, request) {
        Ok(replaced) => {
            if let (Some(map), Some(mut terrain)) = (map, world.get_resource_mut::<MatchTerrain>())
            {
                terrain.map = Some(map);
            }
            let rules_hash = world.resource::<RunnerLink>().header.rules_hash;
            adopt(world, source, new);
            record_reload(world, replaced.next, rules_hash);
            ReloadOutcome::Rebuilt {
                tick: replaced.next,
                dropped: replaced.dropped,
            }
        }
        Err(error) => ReloadOutcome::Failed(format!("the rebuilt match does not load: {error}")),
    }
}

/// Makes `content` the client's: [`ClientContent`], the scenario loader, and the map view.
fn adopt(world: &mut World, source: &ContentSource, content: Arc<ContentDb>) {
    world.insert_resource(scenario_loader(source, content.clone()));
    world.insert_resource(ClientContent(content));
    refresh_map_view(world);
}

/// Appends the reload to the replay capture. Built from the runner's answer, never from the
/// journal, which may be off.
fn record_reload(world: &mut World, tick: Tick, rules_hash: u64) {
    if let Some(mut capture) = world.get_resource_mut::<ReplayCapture>() {
        capture.push(ReplayEvent::ContentReload { tick, rules_hash });
    }
}

// --- the watcher ---

/// Watches the files under some directories (ADR-0012 refinement: content is read with
/// `std::fs`, not the asset server, so the testbed polls). A file's modification time and
/// length decide when it is read again; its bytes decide whether it changed, so a touched or
/// rewritten-as-is file is no change.
#[derive(Resource, Debug)]
pub struct ContentWatcher {
    dirs: Vec<PathBuf>,
    every: Duration,
    /// Time since the last scan.
    since: Duration,
    /// Every file by path, sorted.
    files: Vec<FileStamp>,
    /// A change was seen, and no scan since has found the files unchanged.
    unsettled: bool,
}

/// What [`ContentWatcher`] knows of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    path: PathBuf,
    /// Since `UNIX_EPOCH`: the client clippy profile bans naming `SystemTime`.
    mtime: Duration,
    len: u64,
    /// The hash of its bytes.
    hash: u64,
}

impl ContentWatcher {
    /// A watcher of every file under `dirs` that scans every `every`; it takes its first
    /// look now, so [`ContentWatcher::scan`] reports changes from here on.
    #[must_use]
    pub fn new(dirs: Vec<PathBuf>, every: Duration) -> Self {
        let files = look(&dirs, &[]);
        ContentWatcher {
            dirs,
            every,
            since: Duration::ZERO,
            files,
            unsettled: false,
        }
    }

    /// Looks at the files again; true when a file was added or removed, or its bytes
    /// changed, since the last look. Only files whose modification time or length moved are
    /// read.
    pub fn scan(&mut self) -> bool {
        let now = look(&self.dirs, &self.files);
        let changed = now.len() != self.files.len()
            || now
                .iter()
                .zip(&self.files)
                .any(|(a, b)| a.path != b.path || a.hash != b.hash);
        self.files = now;
        changed
    }

    /// [`ContentWatcher::scan`], debounced: true once the files changed and then a further
    /// scan found them unchanged, so an edit that saves several files (or writes one in
    /// parts) reloads once, after it is complete.
    pub fn settled_change(&mut self) -> bool {
        if self.scan() {
            self.unsettled = true;
            return false;
        }
        std::mem::take(&mut self.unsettled)
    }

    /// Counts `dt`; true once per `every`.
    fn due(&mut self, dt: Duration) -> bool {
        self.since += dt;
        if self.since < self.every {
            return false;
        }
        self.since = Duration::ZERO;
        true
    }
}

/// Every file under `dirs`, sorted by path; a file whose modification time and length equal
/// its entry in `known` keeps that entry's hash, the others are read and hashed. Unreadable
/// entries are skipped.
fn look(dirs: &[PathBuf], known: &[FileStamp]) -> Vec<FileStamp> {
    let mut out = Vec::new();
    let mut todo: Vec<PathBuf> = dirs.to_vec();
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let path = entry.path();
            if meta.is_dir() {
                todo.push(path);
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .unwrap_or_default();
            let len = meta.len();
            let same = known
                .binary_search_by(|f| f.path.cmp(&path))
                .ok()
                .map(|i| &known[i])
                .filter(|f| f.mtime == mtime && f.len == len);
            let hash = match same {
                Some(f) => f.hash,
                None => match std::fs::read(&path) {
                    Ok(bytes) => {
                        let mut hasher = DefaultHasher::new();
                        bytes.hash(&mut hasher);
                        hasher.finish()
                    }
                    Err(_) => continue,
                },
            };
            out.push(FileStamp {
                path,
                mtime,
                len,
                hash,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Scans the [`ContentWatcher`] every period and reloads on a settled change
/// ([`ContentWatcher::settled_change`]), printing the outcome to the console.
fn watch_content(world: &mut World) {
    let dt = world
        .get_resource::<Time<Real>>()
        .map_or(Duration::ZERO, Time::delta);
    let mut watcher = world.resource_mut::<ContentWatcher>();
    if !watcher.due(dt) || !watcher.settled_change() {
        return;
    }
    let outcome = reload_content(world);
    if let Some(mut console) = world.get_resource_mut::<ConsoleState>() {
        console
            .history
            .extend(format!("reload: {outcome}").lines().map(str::to_owned));
    }
}

/// The console command `reload`.
fn reload_command(world: &mut World, args: &[&str]) -> Result<String, String> {
    if !args.is_empty() {
        return Err("usage: reload".to_owned());
    }
    Ok(reload_content(world).to_string())
}

// --- the visual refresh ---

/// The client's copy of the running match's terrain: the sim's [`TerrainGrid`] as the match
/// was set up or loaded, with every `core.terrain_changed` journal record applied since. The
/// [`MapView`] is derived from it and the client's content, so it follows `debug.set_terrain`
/// and content reloads (new terrain colours), and a loaded save shows its own map.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct MatchTerrain {
    /// The match's map, when known: the scenario's, else the content map with the same size
    /// and cell size (a save names no map).
    pub map: Option<String>,
    pub size_m: Vec2,
    pub cell_m: f32,
    pub grid: Option<TerrainGrid>,
}

impl MatchTerrain {
    /// The terrain of the sim in `world`, whose map is `map` when known.
    #[must_use]
    pub fn of_sim(world: &World, content: &ContentDb, map: Option<&str>) -> Self {
        let info = world.get_resource::<MapInfo>();
        let map = map.map(str::to_owned).or_else(|| {
            let info = info?;
            content
                .kind::<MapDef>()
                .iter()
                .find(|def| def.size_m == info.size && def.cell_m == info.cell)
                .map(|def| def.id.clone())
        });
        MatchTerrain {
            map,
            size_m: info.map_or(Vec2::ZERO, |i| {
                Vec2::new(i.size.x.to_f32(), i.size.y.to_f32())
            }),
            cell_m: info.map_or(1.0, |i| i.cell.to_f32()),
            grid: world.get_resource::<TerrainGrid>().cloned(),
        }
    }

    /// The map view under `content`: the map's ground colour, and the heightfield coloured by
    /// the content's terrain kinds (a kind it lacks shows the ground colour).
    #[must_use]
    pub fn map_view(&self, content: &ContentDb) -> MapView {
        let (r, g, b) = self
            .map
            .as_deref()
            .and_then(|id| content.kind::<MapDef>().get(id))
            .map_or(DEFAULT_GROUND, |def| def.ground_color);
        let terrain = self.grid.as_ref().and_then(|grid| {
            terrain_view(grid, self.cell_m, content.kind::<TerrainDef>(), [r, g, b])
        });
        MapView {
            size_m: self.size_m,
            ground_color: Color::linear_rgb(r, g, b),
            terrain,
        }
    }

    /// Applies one `core.terrain_changed` record's data; false if it does not fit the grid.
    fn apply(&mut self, data: &Value) -> bool {
        let cell = |key: &str| -> Option<(u32, u32)> {
            let pair = data.get(key)?.as_array()?;
            let at = |i: usize| u32::try_from(pair.get(i)?.as_u64()?).ok();
            Some((at(0)?, at(1)?))
        };
        let (Some(min), Some(max), Some(terrain), Some(grid)) = (
            cell("min"),
            cell("max"),
            data.get("terrain").and_then(Value::as_str),
            self.grid.as_mut(),
        ) else {
            return false;
        };
        let fits = min.0 <= max.0
            && min.1 <= max.1
            && max.0 < grid.cells_x
            && max.1 < grid.cells_y
            && (grid.palette.len() < usize::from(TerrainGrid::OPEN)
                || grid.palette.iter().any(|id| id == terrain));
        if fits {
            grid.set_rect(CellRect { min, max }, terrain);
        }
        fits
    }
}

/// The heightfield of `grid`, coloured by `terrains`; `None` for open, flat ground.
fn terrain_view(
    grid: &TerrainGrid,
    cell_m: f32,
    terrains: &KindTable<TerrainDef>,
    ground: [f32; 3],
) -> Option<TerrainView> {
    if grid.types.is_empty() && grid.heights.is_empty() {
        return None;
    }
    let cells = grid.cells_x as usize * grid.cells_y as usize;
    let (lo, hi) = (grid.height_m.0.to_f32(), grid.height_m.1.to_f32());
    let palette: Vec<[f32; 3]> = grid
        .palette
        .iter()
        .map(|id| {
            terrains
                .get(id)
                .map_or(ground, |t| [t.color.0, t.color.1, t.color.2])
        })
        .collect();
    let heights_m = (0..cells)
        .map(|i| {
            grid.heights.get(i).map_or(lo, |&raw| {
                lo + (hi - lo) * (f32::from(raw) / f32::from(u16::MAX))
            })
        })
        .collect();
    let colors = (0..cells)
        .map(|i| {
            grid.types
                .get(i)
                .and_then(|&t| palette.get(usize::from(t)))
                .copied()
                .unwrap_or(ground)
        })
        .collect();
    Some(TerrainView {
        cells: UVec2::new(grid.cells_x, grid.cells_y),
        cell_m,
        heights_m,
        colors,
    })
}

/// Whether two map views draw the same ground.
fn same_view(a: &MapView, b: &MapView) -> bool {
    a.size_m == b.size_m && a.ground_color == b.ground_color && a.terrain == b.terrain
}

/// Puts the [`MapView`] of the [`MatchTerrain`] under the [`ClientContent`] in place, if it
/// differs (a new view rebuilds the ground mesh).
fn refresh_map_view(world: &mut World) {
    let (Some(terrain), Some(content)) = (
        world.get_resource::<MatchTerrain>(),
        world.get_resource::<ClientContent>(),
    ) else {
        return;
    };
    let view = terrain.map_view(&content.0);
    match world.get_resource_mut::<MapView>() {
        Some(mut map) if !same_view(&map, &view) => *map = view,
        Some(_) => {}
        None => {
            world.insert_resource(view);
        }
    }
}

/// The position of [`follow_terrain_changes`] in the [`SimJournalMsg`]s. Every load puts it
/// past the messages already written ([`install_terrain`]), so a record of the old runner
/// still unread at the swap never reaches the new match's terrain.
#[derive(Resource, Default)]
struct TerrainCursor(MessageCursor<SimJournalMsg>);

/// Makes `terrain` the running match's, at a load: inserts it and skips the terrain records
/// of the runner it replaces.
pub(crate) fn install_terrain(world: &mut World, terrain: MatchTerrain) {
    world.insert_resource(terrain);
    world.init_resource::<TerrainCursor>();
    world.resource_scope(|world, mut cursor: Mut<TerrainCursor>| {
        if let Some(messages) = world.get_resource::<Messages<SimJournalMsg>>() {
            cursor.0.clear(messages);
        }
    });
}

/// Applies the `core.terrain_changed` journal records received since the last run (or the
/// last load) to the [`MatchTerrain`], and refreshes the [`MapView`] once if any did.
///
/// It follows the journal, which a match keeps only in debug builds (`RunConfig::journal`):
/// in a release build the drawn terrain follows `debug.set_terrain` only at the next load
/// (ruling R134; an always-on sim event for terrain changes is due before M7's bridges).
fn follow_terrain_changes(
    mut cursor: ResMut<TerrainCursor>,
    messages: Res<Messages<SimJournalMsg>>,
    terrain: Option<ResMut<MatchTerrain>>,
    content: Option<Res<ClientContent>>,
    map: Option<ResMut<MapView>>,
) {
    let changes: Vec<&Value> = cursor
        .0
        .read(&messages)
        .filter(|msg| msg.0.kind == "core.terrain_changed")
        .map(|msg| &msg.0.data)
        .collect();
    let (Some(mut terrain), Some(content), Some(mut map)) = (terrain, content, map) else {
        return;
    };
    let mut changed = false;
    for data in changes {
        changed |= terrain.apply(data);
    }
    if changed {
        let view = terrain.map_view(&content.0);
        if !same_view(&map, &view) {
            *map = view;
        }
    }
}

/// The hot reload and visual refresh of the game: the `reload` console command, the terrain
/// follower and, with `watcher`, the content watcher.
pub(crate) fn build(app: &mut App, watcher: Option<ContentWatcher>) {
    app.register_console_command(
        "reload",
        "reload: hot-reload the content now (testbed matches only)",
        reload_command,
    )
    .init_resource::<TerrainCursor>()
    .add_systems(
        Update,
        follow_terrain_changes
            .after(SimViewSystems)
            .run_if(resource_exists::<Messages<SimJournalMsg>>),
    );
    if let Some(watcher) = watcher {
        app.insert_resource(watcher)
            .add_systems(Update, watch_content.after(follow_terrain_changes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_is_due_once_per_period() {
        let mut watcher = ContentWatcher::new(Vec::new(), Duration::from_millis(500));
        assert!(!watcher.due(Duration::from_millis(300)));
        assert!(watcher.due(Duration::from_millis(300)));
        assert!(!watcher.due(Duration::from_millis(300)));
        assert!(!watcher.scan(), "no dirs, no change");
    }
}
