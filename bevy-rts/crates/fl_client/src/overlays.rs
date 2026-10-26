//! Debug overlays fed by the sim's debug queries (ADR-0015): unit paths (F5), blocked cells
//! (F6) and the cell grid with HPA* cluster borders and entrances (F7).
//!
//! The overlays read sim truth only through [`SimQuery::Debug`] over the runner channel. Each
//! active overlay polls its query every [`POLL_FRAMES`] frames, never with one in flight, and
//! draws the last answer with gizmos on the terrain of the [`MapView`]. Paths start at the
//! unit visual's `Transform`, so they follow the interpolated pose (R42).
use std::collections::BTreeMap;

use bevy::gizmos::config::GizmoConfigStore;
use bevy::prelude::*;
use fl_math::Fx;
use fl_protocol::SimId;
use fl_view::{SimQuery, SimReply};
use serde_json::{Value, json};

use crate::camera::RtsCamera;
use crate::debug::{CursorGround, OverlayAppExt, OverlayToggles};
use crate::link::{PendingReplies, RunnerLink, send_query};
use crate::map_view::MapView;
use crate::selection::Selection;
use crate::view::{SimViewSystems, ViewIndex};

/// Frames between two polls of an active overlay's query.
pub const POLL_FRAMES: u32 = 15;
/// Blocked cells and cluster details are drawn this far from the camera focus, metres.
pub const VIEW_RADIUS_M: f32 = 120.0;
/// Cell lines are drawn this far from the cursor, metres.
pub const CURSOR_RADIUS_M: f32 = 60.0;
/// Spacing of the samples a line takes the terrain height at, metres.
const SAMPLE_M: f32 = 4.0;
/// Height of the overlays above the ground, metres.
const LIFT_M: f32 = 0.3;

/// Which overlays are shown.
#[derive(Resource, Default, Debug)]
pub struct OverlayState {
    pub paths: bool,
    pub passability: bool,
    pub cells: bool,
}

/// The cells a movement profile cannot enter (`core.passability`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockedCells {
    /// Map size in cells.
    pub cells: UVec2,
    /// The navigation grid's version when the answer was made.
    pub version: u64,
    /// Runs `(start, len)` of blocked cells in row-major order, index `y × cells.x + x`.
    pub runs: Vec<(u32, u32)>,
}

/// The HPA* graph of a movement profile (`core.clusters`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterView {
    /// Cluster side in cells.
    pub cluster_cells: u32,
    /// Map size in clusters.
    pub clusters: UVec2,
    /// Entrance cells.
    pub entrances: Vec<UVec2>,
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value, String> {
    v.get(name).ok_or_else(|| format!("missing \"{name}\""))
}

fn as_u32(v: &Value, what: &str) -> Result<u32, String> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("{what} must be a u32, not {v}"))
}

fn uvec2(v: &Value, what: &str) -> Result<UVec2, String> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok(UVec2::new(as_u32(x, what)?, as_u32(y, what)?)),
        _ => Err(format!("{what} must be [x, y], not {v}")),
    }
}

/// Metres of a raw `Fx` pair.
fn metres(v: &Value) -> Result<Vec2, String> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => {
            let raw = |n: &Value| {
                n.as_i64()
                    .map(|r| Fx::from_raw(r).to_f32())
                    .ok_or_else(|| format!("a raw Fx must be an integer, not {n}"))
            };
            Ok(Vec2::new(raw(x)?, raw(y)?))
        }
        _ => Err(format!("a position must be [x, y], not {v}")),
    }
}

/// A unit's corners and the index of the corner it heads for.
pub type PathEntry = (Vec<Vec2>, usize);

fn decode_path_entries(v: &Value) -> Result<BTreeMap<SimId, PathEntry>, String> {
    let units = v
        .as_object()
        .ok_or_else(|| format!("core.paths must answer an object, not {v}"))?;
    let mut out = BTreeMap::new();
    for (key, unit) in units {
        let id: u32 = key
            .parse()
            .map_err(|_| format!("\"{key}\" is not a sim id"))?;
        let corners = field(unit, "corners")?
            .as_array()
            .ok_or("\"corners\" must be an array")?
            .iter()
            .map(metres)
            .collect::<Result<Vec<_>, _>>()?;
        let next = unit
            .get("next")
            .and_then(Value::as_u64)
            .map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
        out.insert(SimId(id), (corners, next));
    }
    Ok(out)
}

/// The corners of each unit's path in metres, from the `core.paths` answer (positions are raw
/// `Fx`).
///
/// # Errors
///
/// If `v` isn't shaped like the answer.
pub fn decode_paths(v: &Value) -> Result<BTreeMap<SimId, Vec<Vec2>>, String> {
    Ok(decode_path_entries(v)?
        .into_iter()
        .map(|(id, (corners, _))| (id, corners))
        .collect())
}

/// The blocked cells of the `core.passability` answer.
///
/// # Errors
///
/// If `v` isn't shaped like the answer.
pub fn decode_passability(v: &Value) -> Result<BlockedCells, String> {
    let runs = field(v, "blocked")?
        .as_array()
        .ok_or("\"blocked\" must be an array")?
        .iter()
        .map(|run| match run.as_array().map(Vec::as_slice) {
            Some([start, len]) => Ok((as_u32(start, "a run start")?, as_u32(len, "a run length")?)),
            _ => Err(format!("a run must be [start, len], not {run}")),
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BlockedCells {
        cells: uvec2(field(v, "cells")?, "\"cells\"")?,
        version: field(v, "version")?
            .as_u64()
            .ok_or("\"version\" must be an integer")?,
        runs,
    })
}

/// The HPA* graph of the `core.clusters` answer.
///
/// # Errors
///
/// If `v` isn't shaped like the answer.
pub fn decode_clusters(v: &Value) -> Result<ClusterView, String> {
    let entrances = field(v, "entrances")?
        .as_array()
        .ok_or("\"entrances\" must be an array")?
        .iter()
        .map(|e| uvec2(e, "an entrance"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ClusterView {
        cluster_cells: as_u32(field(v, "cluster_cells")?, "\"cluster_cells\"")?,
        clusters: uvec2(field(v, "clusters")?, "\"clusters\"")?,
        entrances,
    })
}

/// The lines on multiples of `step_m` within the square `centre ± radius_m`: first the
/// vertical ones (constant x, west to east), then the horizontal ones (constant y, south to
/// north), each spanning the square.
#[must_use]
pub fn grid_lines(centre: Vec2, radius_m: f32, step_m: f32) -> Vec<(Vec2, Vec2)> {
    if !(step_m > 0.0 && step_m.is_finite() && radius_m >= 0.0 && radius_m.is_finite()) {
        return Vec::new();
    }
    let multiples = |lo: f32, hi: f32| {
        let (first, last) = ((lo / step_m).ceil() as i64, (hi / step_m).floor() as i64);
        (first..=last).map(move |k| k as f32 * step_m)
    };
    let (lo, hi) = (
        centre - Vec2::splat(radius_m),
        centre + Vec2::splat(radius_m),
    );
    let vertical = multiples(lo.x, hi.x).map(|x| (Vec2::new(x, lo.y), Vec2::new(x, hi.y)));
    let horizontal = multiples(lo.y, hi.y).map(|y| (Vec2::new(lo.x, y), Vec2::new(hi.x, y)));
    vertical.chain(horizontal).collect()
}

/// The blocked cells of `blocked` whose centre lies within `radius_m` of `focus`, with
/// `cell_m` the cell side.
#[must_use]
pub fn blocked_near(blocked: &BlockedCells, cell_m: f32, focus: Vec2, radius_m: f32) -> Vec<UVec2> {
    let width = blocked.cells.x.max(1);
    let mut out = Vec::new();
    for &(start, len) in &blocked.runs {
        for index in start..start.saturating_add(len) {
            let cell = UVec2::new(index % width, index / width);
            let centre = (cell.as_vec2() + Vec2::splat(0.5)) * cell_m;
            if centre.distance(focus) <= radius_m {
                out.push(cell);
            }
        }
    }
    out
}

/// The cell side in metres: the terrain's, else the map size over `cells`.
fn cell_side(map: &MapView, cells: UVec2) -> f32 {
    map.terrain
        .as_ref()
        .map_or_else(|| map.size_m.x / cells.x.max(1) as f32, |t| t.cell_m)
}

/// The arguments of a profile query: the first selected unit's profile, else tracked size 1.
fn profile_args(selection: &Selection) -> Value {
    selection.0.first().map_or_else(
        || json!({ "class": "Tracked", "size": 1 }),
        |unit| json!({ "unit": unit.0 }),
    )
}

/// The three overlays' queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Paths,
    Passability,
    Cells,
}

impl Kind {
    const ALL: [Kind; 3] = [Kind::Paths, Kind::Passability, Kind::Cells];

    fn query_name(self) -> &'static str {
        match self {
            Kind::Paths => "core.paths",
            Kind::Passability => "core.passability",
            Kind::Cells => "core.clusters",
        }
    }

    fn active(self, state: &OverlayState) -> bool {
        match self {
            Kind::Paths => state.paths,
            Kind::Passability => state.passability,
            Kind::Cells => state.cells,
        }
    }
}

/// The polling of one overlay.
#[derive(Debug)]
struct Poll {
    /// Frames since the last query was sent; starts due, so an overlay polls when it appears.
    since: u32,
    /// The query in flight.
    pending: Option<u64>,
    /// The last error, so a repeating one is logged once.
    error: Option<String>,
}

impl Default for Poll {
    fn default() -> Self {
        Poll {
            since: POLL_FRAMES,
            pending: None,
            error: None,
        }
    }
}

/// What the overlays last heard from the sim.
#[derive(Resource, Default, Debug)]
pub struct OverlayData {
    polls: BTreeMap<Kind, Poll>,
    paths: BTreeMap<SimId, PathEntry>,
    blocked: Option<BlockedCells>,
    clusters: Option<ClusterView>,
    /// The selection the polls were made for.
    selection: Selection,
    /// Counts the passability answers, to tell a new one from the cached one.
    blocked_serial: u64,
    /// The blocked cells near the camera, and what they were computed for.
    in_view: Vec<UVec2>,
    in_view_key: Option<(u64, IVec2)>,
    rebuilds: u32,
}

/// Camera moves smaller than this, metres, don't rebuild the blocked cells in view.
const VIEW_QUANTUM_M: f32 = 8.0;

impl OverlayData {
    /// The last paths: corners and the index of the corner each unit heads for.
    #[must_use]
    pub fn paths(&self) -> &BTreeMap<SimId, PathEntry> {
        &self.paths
    }

    /// The blocked cells within [`VIEW_RADIUS_M`] of the camera focus (to within one quantum),
    /// cached until a new answer arrives or the camera moves.
    #[must_use]
    pub fn blocked_in_view(&self) -> &[UVec2] {
        &self.in_view
    }

    /// How many times [`OverlayData::blocked_in_view`] was recomputed.
    #[must_use]
    pub fn view_rebuilds(&self) -> u32 {
        self.rebuilds
    }

    /// Forgets the queries in flight and what they drew, for a new runner
    /// ([`replace_match`](crate::debug::replace_match)): the old one never answers, so every
    /// active overlay polls the new one at once.
    pub fn forget_runner(&mut self) {
        self.polls.clear();
        for kind in Kind::ALL {
            self.clear(kind);
        }
    }

    fn clear(&mut self, kind: Kind) {
        match kind {
            Kind::Paths => self.paths.clear(),
            Kind::Passability => {
                self.blocked = None;
                self.in_view.clear();
                self.in_view_key = None;
            }
            Kind::Cells => self.clusters = None,
        }
    }
}

fn toggle_paths(world: &mut World) -> bool {
    let mut state = world.get_resource_or_init::<OverlayState>();
    state.paths = !state.paths;
    state.paths
}

fn toggle_passability(world: &mut World) -> bool {
    let mut state = world.get_resource_or_init::<OverlayState>();
    state.passability = !state.passability;
    state.passability
}

fn toggle_cells(world: &mut World) -> bool {
    let mut state = world.get_resource_or_init::<OverlayState>();
    state.cells = !state.cells;
    state.cells
}

/// Sends the query of each active overlay that is due, and forgets the answers of the others.
fn poll_overlays(
    state: Res<OverlayState>,
    selection: Option<Res<Selection>>,
    link: Option<Res<RunnerLink>>,
    mut replies: ResMut<PendingReplies>,
    mut data: ResMut<OverlayData>,
) {
    let selection = selection.as_deref().cloned().unwrap_or_default();
    for kind in Kind::ALL {
        if !kind.active(&state) {
            if let Some(poll) = data.polls.remove(&kind)
                && let Some(id) = poll.pending
            {
                replies.abandon(id);
            }
            data.clear(kind);
            continue;
        }
        let Some(link) = link.as_deref() else {
            continue;
        };
        let poll = data.polls.entry(kind).or_default();
        poll.since = poll.since.saturating_add(1);
        if poll.pending.is_some() || poll.since < POLL_FRAMES {
            continue;
        }
        let args = match kind {
            Kind::Paths => {
                if selection.0.is_empty() {
                    data.paths.clear();
                    continue;
                }
                json!({ "units": selection.0.iter().map(|id| id.0).collect::<Vec<_>>() })
            }
            Kind::Passability | Kind::Cells => profile_args(&selection),
        };
        let query = SimQuery::Debug {
            name: kind.query_name().to_owned(),
            args,
        };
        let id = send_query(link, &mut replies, query);
        let poll = data.polls.entry(kind).or_default();
        poll.pending = Some(id);
        poll.since = 0;
    }
}

/// Decodes the answers that arrived. When the selection changed, the queries in flight were
/// made for the old one: their answers are dropped and every overlay polls again at once.
fn receive_overlay_replies(
    selection: Option<Res<Selection>>,
    mut replies: ResMut<PendingReplies>,
    mut data: ResMut<OverlayData>,
) {
    let selection = selection.as_deref().cloned().unwrap_or_default();
    if data.selection != selection {
        data.selection = selection;
        for poll in data.polls.values_mut() {
            if let Some(id) = poll.pending.take() {
                replies.abandon(id);
            }
            poll.since = POLL_FRAMES;
        }
    }
    for kind in Kind::ALL {
        let Some(id) = data.polls.get(&kind).and_then(|p| p.pending) else {
            continue;
        };
        let Some(reply) = replies.take(id) else {
            continue;
        };
        let decoded = match reply {
            SimReply::Debug(value) => match kind {
                Kind::Paths => decode_path_entries(&value).map(|p| data.paths = p),
                Kind::Passability => decode_passability(&value).map(|b| {
                    data.blocked = Some(b);
                    data.blocked_serial += 1;
                }),
                Kind::Cells => decode_clusters(&value).map(|c| data.clusters = Some(c)),
            },
            SimReply::Error(message) => Err(message),
            other => Err(format!("unexpected reply: {other:?}")),
        };
        if decoded.is_err() {
            // Nothing stale stays drawn.
            data.clear(kind);
        }
        let poll = data.polls.entry(kind).or_default();
        poll.pending = None;
        match decoded {
            Ok(()) => poll.error = None,
            Err(message) => {
                if poll.error.as_ref() != Some(&message) {
                    tracing::warn!(query = kind.query_name(), %message, "overlay query failed");
                }
                poll.error = Some(message);
            }
        }
    }
}

/// Recomputes the blocked cells near the camera when a new answer arrived or the camera moved.
fn refresh_blocked_view(
    mut data: ResMut<OverlayData>,
    map: Option<Res<MapView>>,
    cameras: Query<&RtsCamera>,
) {
    let (Some(map), Some(blocked)) = (map, data.blocked.as_ref()) else {
        return;
    };
    let focus = camera_focus(&cameras);
    let key = (
        data.blocked_serial,
        (focus / VIEW_QUANTUM_M).floor().as_ivec2(),
    );
    if data.in_view_key == Some(key) {
        return;
    }
    let cell_m = cell_side(&map, blocked.cells);
    let near = blocked_near(blocked, cell_m, focus, VIEW_RADIUS_M);
    data.in_view = near;
    data.in_view_key = Some(key);
    data.rebuilds += 1;
}

/// The ground point the camera looks at, in sim metres; the origin without a camera.
fn camera_focus(cameras: &Query<&RtsCamera>) -> Vec2 {
    cameras
        .iter()
        .next()
        .map_or(Vec2::ZERO, |c| Vec2::new(c.focus.x, c.focus.z))
}

/// The world point over the sim position `p`, lifted onto the terrain.
fn ground(map: &MapView, p: Vec2) -> Vec3 {
    let height = map.terrain.as_ref().map_or(0.0, |t| t.height_at(p.x, p.y));
    Vec3::new(p.x, height + LIFT_M, p.y)
}

/// The points of the segment `a`-`b` on the terrain, sampled every [`SAMPLE_M`].
fn ground_line(map: &MapView, a: Vec2, b: Vec2) -> impl Iterator<Item = Vec3> {
    let steps = if map.terrain.is_some() {
        ((b - a).length() / SAMPLE_M).ceil().max(1.0) as u32
    } else {
        1
    };
    (0..=steps).map(move |i| ground(map, a.lerp(b, i as f32 / steps as f32)))
}

fn draw_paths(
    data: Res<OverlayData>,
    map: Option<Res<MapView>>,
    index: Res<ViewIndex>,
    transforms: Query<&Transform>,
    mut gizmos: Gizmos,
) {
    let color = Color::srgb(0.2, 0.9, 0.3);
    for (id, (corners, next)) in &data.paths {
        let start = index
            .0
            .get(id)
            .and_then(|&e| transforms.get(e).ok())
            .map(|t| t.translation + Vec3::Y * LIFT_M);
        let ahead = corners.iter().skip((*next).min(corners.len()));
        let points = start.into_iter().chain(ahead.map(|&c| {
            map.as_deref()
                .map_or(Vec3::new(c.x, LIFT_M, c.y), |m| ground(m, c))
        }));
        gizmos.linestrip(points, color);
    }
}

fn draw_passability(data: Res<OverlayData>, map: Option<Res<MapView>>, mut gizmos: Gizmos) {
    let (Some(blocked), Some(map)) = (&data.blocked, map) else {
        return;
    };
    let cell_m = cell_side(&map, blocked.cells);
    let color = Color::srgba(1.0, 0.2, 0.2, 0.8);
    for &cell in data.blocked_in_view() {
        let lo = cell.as_vec2() * cell_m;
        let hi = lo + Vec2::splat(cell_m);
        gizmos.line(ground(&map, lo), ground(&map, hi), color);
        gizmos.line(
            ground(&map, Vec2::new(lo.x, hi.y)),
            ground(&map, Vec2::new(hi.x, lo.y)),
            color,
        );
    }
}

/// Draws `lines`, clamped to the map, on the terrain.
fn draw_lines(gizmos: &mut Gizmos, map: &MapView, lines: Vec<(Vec2, Vec2)>, color: Color) {
    let (lo, hi) = (Vec2::ZERO, map.size_m);
    for (a, b) in lines {
        let (a, b) = (a.clamp(lo, hi), b.clamp(lo, hi));
        if a != b {
            gizmos.linestrip(ground_line(map, a, b), color);
        }
    }
}

fn draw_cells(
    data: Res<OverlayData>,
    map: Option<Res<MapView>>,
    cameras: Query<&RtsCamera>,
    cursor: Res<CursorGround>,
    mut gizmos: Gizmos,
) {
    let Some(map) = map else {
        return;
    };
    let cells = map.terrain.as_ref().map_or(UVec2::ONE, |t| t.cells);
    let cell_m = cell_side(&map, cells);
    if let Some(at) = cursor.0 {
        let lines = grid_lines(Vec2::new(at.x, at.z), CURSOR_RADIUS_M, cell_m);
        draw_lines(&mut gizmos, &map, lines, Color::srgba(1.0, 1.0, 1.0, 0.3));
    }
    let Some(clusters) = &data.clusters else {
        return;
    };
    let focus = camera_focus(&cameras);
    let border = clusters.cluster_cells as f32 * cell_m;
    let lines = grid_lines(focus, VIEW_RADIUS_M, border);
    draw_lines(&mut gizmos, &map, lines, Color::srgba(0.3, 0.6, 1.0, 0.9));
    let marker = Color::srgb(1.0, 0.8, 0.1);
    for entrance in &clusters.entrances {
        let centre = (entrance.as_vec2() + Vec2::splat(0.5)) * cell_m;
        if centre.distance(focus) > VIEW_RADIUS_M {
            continue;
        }
        let at = ground(&map, centre);
        gizmos.line(at, at + Vec3::Y * 3.0, marker);
    }
}

/// Registers the overlays `"paths"` (F5), `"passability"` (F6) and `"cells"` (F7) and draws
/// them from debug queries.
pub struct OverlaysPlugin;

impl Plugin for OverlaysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OverlayState>()
            .init_resource::<OverlayData>()
            .init_resource::<PendingReplies>()
            .init_resource::<CursorGround>()
            .init_resource::<ViewIndex>()
            .register_overlay("paths", Some(KeyCode::F5), toggle_paths)
            .register_overlay("passability", Some(KeyCode::F6), toggle_passability)
            .register_overlay("cells", Some(KeyCode::F7), toggle_cells)
            .add_systems(
                Update,
                (
                    (receive_overlay_replies, poll_overlays)
                        .chain()
                        .after(SimViewSystems),
                    draw_paths
                        .run_if(resource_exists::<GizmoConfigStore>)
                        .run_if(|s: Res<OverlayState>| s.paths),
                    refresh_blocked_view
                        .before(draw_passability)
                        .run_if(|s: Res<OverlayState>| s.passability),
                    draw_passability
                        .run_if(resource_exists::<GizmoConfigStore>)
                        .run_if(|s: Res<OverlayState>| s.passability),
                    draw_cells
                        .run_if(resource_exists::<GizmoConfigStore>)
                        .run_if(|s: Res<OverlayState>| s.cells),
                )
                    .after(OverlayToggles),
            );
    }
}
