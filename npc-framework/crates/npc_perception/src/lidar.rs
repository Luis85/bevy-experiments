//! The LIDAR's scan ([`LaserScan`], plan Task 12, spec G10).
//!
//! Each beam is traced over the grid (the cells and order of
//! `SpatialGrid::trace_segment`) from the agent's position up to `range_mm`
//! and stops at the first cell (other than the agent's own) that is
//! blocking or occluding, or holds an obstacle thing (a perceivable with a
//! kind in [`LidarObstacles`]); the range is where the beam enters that
//! cell, at least [`LIDAR_RANGE_MIN_MM`]. A miss is `range_max_mm + 1`.
//! Hits get deterministic noise of `±noise_ppm` of the range from
//! `sim_rand(seed, agent, tick, LIDAR_NOISE_PURPOSE)` (one draw per scan,
//! mixed per beam with `splitmix64`).
//!
//! Cost: the solid and obstacle cells of the square the beams can reach are
//! marked in a reused bitmap ([`LidarScratch`]) and walked without a
//! callback per cell; a 360-beam, 20 m scan on 1 m cells visits about 7 000
//! cells and allocates nothing once warm. An idle robot (same position,
//! facing, LIDAR, grid revision and obstacle cells) reuses its noise-free
//! ranges and only draws the noise again.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_kernel::rng::{sim_rand, splitmix64};
use npc_kernel::spatial::{CellCoord, SpatialGrid};
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

use crate::fog::KindBits;
use crate::readings::{PPM, dir_ppm, facing_angle_mdeg};
use crate::sensors::{ALL_KINDS, Facing, Lidar};

/// `sim_rand` purpose of the LIDAR noise ("LIDR").
pub const LIDAR_NOISE_PURPOSE: u32 = 0x4c49_4452;

/// The shortest range a LIDAR reports (a hit closer than this reads as
/// this), mm.
pub const LIDAR_RANGE_MIN_MM: i32 = 50;

/// Above this many cells the scan square is not marked in a bitmap; beams
/// then read the grid cell by cell (huge ranges on fine grids).
const WINDOW_MAX_CELLS: i64 = 1 << 20;

/// A planar laser scan shaped like ROS `sensor_msgs/LaserScan`, in mm and
/// millidegrees: beam `i` points `angle_min_mdeg + i · angle_increment_mdeg`
/// from the facing (+ = left), and `ranges_mm[i]` is the distance to what
/// it hit, or `range_max_mm + 1` for a miss.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaserScan {
    pub angle_min_mdeg: i32,
    pub angle_increment_mdeg: i32,
    pub range_min_mm: i32,
    pub range_max_mm: i32,
    pub ranges_mm: Vec<i32>,
}

impl LaserScan {
    /// The angle of beam `i` relative to the facing, mdeg.
    pub fn angle_of(&self, i: usize) -> i64 {
        i64::from(self.angle_min_mdeg) + i as i64 * i64::from(self.angle_increment_mdeg)
    }

    /// Whether beam range `r` is a miss.
    pub fn is_miss(&self, r: i32) -> bool {
        r > self.range_max_mm
    }
}

/// `(angle_min, angle_increment)` of `lidar` in mdeg: a full circle starts
/// behind (−180°) and divides 360° by the beams; a partial field spans
/// `−fov/2 ..= +fov/2` with both ends included.
pub fn beam_angles(lidar: &Lidar) -> (i32, i32) {
    let beams = i32::from(lidar.beams.max(1));
    let fov = i32::from(lidar.fov_deg.clamp(1, 360)) * 1000;
    if fov >= 360_000 {
        (-180_000, 360_000 / beams)
    } else if beams > 1 {
        (-fov / 2, fov / (beams - 1))
    } else {
        (0, 0)
    }
}

/// Which perceivable things stop a LIDAR beam in their cell: those with any
/// of `kinds` (default: every kind). Content narrows it (set with
/// `PerceptionAppExt::set_lidar_obstacles`) so that small things (items)
/// do not fill their whole cell, and things whose cell is solid only while
/// they block (a closed door makes its cell blocking; an open one lets the
/// beam through) are left to the grid. Not hashed: it shapes only the
/// snapshot-only readings.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LidarObstacles {
    pub kinds: KindBits,
}

impl Default for LidarObstacles {
    fn default() -> Self {
        Self { kinds: ALL_KINDS }
    }
}

/// What one agent's noise-free ranges were last traced from: an idle
/// robot (same position, facing, LIDAR, grid cells and obstacle cells)
/// reuses them and only draws its noise again. Kept per entity (a loaded
/// world has new entities, so no stale entry is ever reused).
#[derive(Default)]
struct LidarCache {
    pos: SimPos,
    facing: Facing,
    lidar: Option<Lidar>,
    cell_mm: i32,
    revision: u64,
    /// The obstacle cells in the scan square, sorted.
    things: Vec<CellCoord>,
    /// Per beam: where it entered a solid cell (no noise), or -1 for a miss.
    clean: Vec<i32>,
}

/// Reused buffers of the LIDAR pass (not state).
#[derive(Default)]
pub struct LidarScratch {
    /// The scan square's cells: 1 = a beam stops there.
    solid: Vec<u8>,
    /// This scan's obstacle cells, sorted.
    thing_cells: Vec<CellCoord>,
    /// The beams' unit directions relative to the facing (ppm), for
    /// `(angle_min, increment, beams)`.
    beams: Vec<(i64, i64)>,
    beams_key: Option<(i32, i32, usize)>,
    cache: BTreeMap<Entity, LidarCache>,
    /// Scans that reused the traced ranges (tests; not state).
    hits: u64,
}

impl LidarScratch {
    /// How many scans reused their traced ranges.
    pub fn cache_hits(&self) -> u64 {
        self.hits
    }
}

/// The square of cells `(x0, z0, width)` the beams can reach.
struct Window {
    x0: i32,
    z0: i32,
    w: i32,
}

impl Window {
    fn index(&self, c: CellCoord) -> Option<usize> {
        let (x, z) = (c.x - self.x0, c.z - self.z0);
        (x >= 0 && z >= 0 && x < self.w && z < self.w).then(|| (z * self.w + x) as usize)
    }

    /// Marks the window's solid cells (blocking or occluding) and the
    /// obstacle cells `things` in `solid`.
    fn fill(&self, grid: &SpatialGrid, things: &[CellCoord], solid: &mut Vec<u8>) {
        solid.clear();
        solid.resize((self.w as usize) * (self.w as usize), 0);
        grid.for_each_solid(
            CellCoord::new(self.x0, self.z0),
            CellCoord::new(self.x0 + self.w - 1, self.z0 + self.w - 1),
            |c| {
                if let Some(i) = self.index(c) {
                    solid[i] = 1;
                }
            },
        );
        for &c in things {
            if let Some(i) = self.index(c) {
                solid[i] = 1;
            }
        }
    }
}

/// One scan's inputs.
pub(crate) struct LidarInput<'a, I: Iterator<Item = SimPos>> {
    pub grid: &'a SpatialGrid,
    /// The obstacle things' positions (the agent excluded).
    pub things: I,
    pub entity: Entity,
    pub id: SimId,
    pub pos: SimPos,
    pub facing: Facing,
    pub lidar: Lidar,
    pub seed: u64,
    pub tick: u64,
}

/// Fills `out` with the scan of `input.lidar` for the agent (see the module
/// docs).
pub(crate) fn scan_lidar<I: Iterator<Item = SimPos>>(
    input: LidarInput<'_, I>,
    out: &mut LaserScan,
    scratch: &mut LidarScratch,
) {
    let LidarInput {
        grid,
        things,
        entity,
        id,
        pos,
        facing,
        lidar,
        seed,
        tick,
    } = input;
    let range = lidar.range_mm.max(0);
    let (angle_min, inc) = beam_angles(&lidar);
    out.angle_min_mdeg = angle_min;
    out.angle_increment_mdeg = inc;
    out.range_min_mm = LIDAR_RANGE_MIN_MM.min(range);
    out.range_max_mm = range;
    out.ranges_mm.clear();
    let cs = grid.cell_mm();
    let start = grid.cell_of(pos);
    let reach = range / cs + 2;
    let w = 2 * i64::from(reach) + 1;
    // The bitmap walk ([`march`]) also needs i64-sized cross products.
    let narrow = range < 1 << 29 && cs < 1 << 29;
    let window = (w * w <= WINDOW_MAX_CELLS && narrow).then(|| Window {
        x0: start.x - reach,
        z0: start.z - reach,
        w: w as i32,
    });
    scratch.thing_cells.clear();
    scratch.thing_cells.extend(
        things
            .map(|p| grid.cell_of(p))
            .filter(|&c| window.as_ref().is_none_or(|win| win.index(c).is_some())),
    );
    scratch.thing_cells.sort_unstable();
    scratch.thing_cells.dedup();
    let beams = usize::from(lidar.beams.max(1));
    let key = (angle_min, inc, beams);
    if scratch.beams_key != Some(key) {
        scratch.beams.clear();
        scratch
            .beams
            .extend((0..beams).map(|i| dir_ppm(out.angle_of(i))));
        scratch.beams_key = Some(key);
    }
    let revision = grid.revision();
    let cached = scratch.cache.get(&entity).is_some_and(|c| {
        c.pos == pos
            && c.facing == facing
            && c.lidar == Some(lidar)
            && c.cell_mm == cs
            && c.revision == revision
            && c.things == scratch.thing_cells
            && c.clean.len() == beams
    });
    if cached {
        scratch.hits += 1;
    } else {
        let mut cache = scratch.cache.remove(&entity).unwrap_or_default();
        trace(
            grid,
            window.as_ref(),
            &scratch.thing_cells,
            &mut scratch.solid,
            &scratch.beams,
            (pos, facing, range),
            &mut cache.clean,
        );
        cache.pos = pos;
        cache.facing = facing;
        cache.lidar = Some(lidar);
        cache.cell_mm = cs;
        cache.revision = revision;
        cache.things.clear();
        cache.things.extend_from_slice(&scratch.thing_cells);
        scratch.cache.insert(entity, cache);
    }
    let clean = &scratch.cache[&entity].clean;
    let noise = i64::from(lidar.noise_ppm.min(1_000_000));
    let draw = sim_rand(seed, id, tick, LIDAR_NOISE_PURPOSE);
    let r = i64::from(range);
    for (i, &d) in clean.iter().enumerate() {
        if d < 0 {
            out.ranges_mm.push(range.saturating_add(1));
            continue;
        }
        let mut d = i64::from(d);
        if noise > 0 {
            // Uniform in `-noise..=noise` by multiply-shift (no division).
            let span = 2 * noise as u64 + 1;
            let n = (((splitmix64(draw ^ i as u64) >> 32) * span) >> 32) as i64 - noise;
            d += d * n / PPM;
        }
        out.ranges_mm
            .push(d.clamp(i64::from(out.range_min_mm), r) as i32);
    }
}

/// Traces every beam of an agent at `pos` facing `facing` up to `range`
/// into `clean` (the entry distance, -1 for a miss).
fn trace(
    grid: &SpatialGrid,
    window: Option<&Window>,
    things: &[CellCoord],
    solid: &mut Vec<u8>,
    beams: &[(i64, i64)],
    (pos, facing, range): (SimPos, Facing, i32),
    clean: &mut Vec<i32>,
) {
    clean.clear();
    if let Some(win) = window {
        win.fill(grid, things, solid);
    }
    let cs = i64::from(grid.cell_mm());
    let start = grid.cell_of(pos);
    let solid_at = |c: CellCoord| -> bool {
        let info = grid.cell(c).flags;
        info.blocking || info.occluding || things.binary_search(&c).is_ok()
    };
    // Turned by the facing once per beam.
    let (ux, uz) = dir_ppm(i64::from(facing_angle_mdeg(facing)));
    let r = i64::from(range);
    for &(c, s) in beams {
        let (dx, dz) = ((ux * c - uz * s) / PPM, (ux * s + uz * c) / PPM);
        let end = SimPos::new(
            (i64::from(pos.x) + dx * r / PPM).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                as i32,
            (i64::from(pos.z) + dz * r / PPM).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                as i32,
        );
        let hit = match window {
            Some(win) => march(win, solid, cs, pos, end, start)
                .map(|(num, den)| if den > 0 { num * r / den } else { 0 }),
            None => {
                let mut hit = None;
                grid.trace_segment(pos, end, |c| {
                    if c != start && solid_at(c) {
                        hit = Some(c);
                        return false;
                    }
                    true
                });
                hit.map(|cell| entry_distance(pos, end, r, cell, cs))
            }
        };
        clean.push(hit.map_or(-1, |d| d.clamp(0, i64::from(i32::MAX)) as i32));
    }
}

/// Where the segment `a → b` enters the first solid cell (other than
/// `start`) it touches, on the window's bitmap, as the fraction
/// `(num, den)` of the segment (one division per hit, by the caller):
/// exactly the cells and order of
/// `SpatialGrid::trace_segment` (the same conservative supercover, both
/// side cells first at an exact corner), stepped on the bitmap index
/// without a callback per cell. Every cell of the segment lies in the
/// window (it spans the beam's whole reach).
fn march(
    win: &Window,
    solid: &[u8],
    cs: i64,
    a: SimPos,
    b: SimPos,
    start: CellCoord,
) -> Option<(i64, i64)> {
    let w = i64::from(win.w);
    let at = |x: i32, z: i32| -> i64 { i64::from(z - win.z0) * w + i64::from(x - win.x0) };
    let is_solid = |i: i64| {
        usize::try_from(i)
            .ok()
            .and_then(|i| solid.get(i))
            .is_some_and(|&s| s != 0)
    };
    let dx = i64::from(b.x) - i64::from(a.x);
    let dz = i64::from(b.z) - i64::from(a.z);
    let (sx, sz) = (dx.signum() as i32, dz.signum() as i32);
    let (adx, adz) = (dx.abs(), dz.abs());
    let first = |c: i32, p: i32, s: i32| -> i64 {
        match s {
            1 => (i64::from(c) + 1) * cs - i64::from(p),
            -1 => i64::from(p) - i64::from(c) * cs,
            _ => 0,
        }
    };
    let mut nx = first(start.x, a.x, sx);
    let mut nz = first(start.z, a.z, sz);
    let mut idx = at(start.x, start.z);
    let (ix, iz) = (i64::from(sx), i64::from(sz) * w);
    // A crossing `n` along an axis is on the segment while `n < lim`: a
    // segment ending exactly on a boundary ends in the cell beyond it going
    // up (`cell_of` floors) and in the cell before it going down. So the
    // walk ends in `cell_of(b)` without dividing by the cell size.
    let (lim_x, lim_z) = (adx + i64::from(sx > 0), adz + i64::from(sz > 0));
    // The crossings as comparable times (`nx / adx` and `nz / adz` scaled
    // by `adx · adz`), advanced by additions; `i64::MAX` once an axis has
    // no crossing left (so the other always goes first, as in
    // `trace_segment`). A beam spans at most a window: no overflow.
    let (kx, kz) = (adz.max(1), adx.max(1));
    let (dtx, dtz) = (cs * kx, cs * kz);
    let mut tx = if nx < lim_x { nx * kx } else { i64::MAX };
    let mut tz = if nz < lim_z { nz * kz } else { i64::MAX };
    while tx != i64::MAX || tz != i64::MAX {
        if tx == tz {
            // Exactly through a corner (at nx / adx): both side cells,
            // then the diagonal.
            if is_solid(idx + ix) || is_solid(idx + iz) || is_solid(idx + ix + iz) {
                return Some((nx, adx));
            }
            idx += ix + iz;
            nx += cs;
            nz += cs;
            tx = if nx < lim_x { tx + dtx } else { i64::MAX };
            tz = if nz < lim_z { tz + dtz } else { i64::MAX };
            continue;
        }
        // The crossing into the next cell, as a fraction of the segment.
        let entry = if tx < tz {
            idx += ix;
            let e = (nx, adx);
            nx += cs;
            tx = if nx < lim_x { tx + dtx } else { i64::MAX };
            e
        } else {
            idx += iz;
            let e = (nz, adz);
            nz += cs;
            tz = if nz < lim_z { tz + dtz } else { i64::MAX };
            e
        };
        if is_solid(idx) {
            return Some(entry);
        }
    }
    None
}

/// How far along the segment `a → b` (of nominal length `len`) it enters
/// `cell`: the larger of the two axes' entry distances.
fn entry_distance(a: SimPos, b: SimPos, len: i64, cell: CellCoord, cs: i64) -> i64 {
    let axis = |p: i32, q: i32, c: i32| -> i64 {
        let dd = i64::from(q) - i64::from(p);
        let boundary = match dd.signum() {
            1 => i64::from(c) * cs - i64::from(p),
            -1 => i64::from(p) - (i64::from(c) + 1) * cs,
            _ => return 0,
        };
        let boundary = boundary.max(0);
        // Both factors are below 2^31 for any i32 range (the boundary never
        // exceeds the extent plus a cell): no 128-bit division per beam.
        match boundary.checked_mul(len) {
            Some(p) => p / dd.abs(),
            None => (i128::from(boundary) * i128::from(len) / i128::from(dd.abs())) as i64,
        }
    };
    axis(a.x, b.x, cell.x).max(axis(a.z, b.z, cell.z))
}

#[cfg(test)]
mod tests {
    use bevy_app::App;
    use npc_kernel::rng::splitmix64;
    use npc_kernel::spatial::{
        CellFlags, CellInfo, Footprint, GridBlocker, GridBounds, GridOccluder, SpatialPlugin,
    };
    use npc_kernel::{NpcKernelPlugin, run_ticks, spawn_sim};

    use super::*;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = splitmix64(self.0);
            self.0
        }

        /// `0..n`.
        fn below(&mut self, n: u64) -> i64 {
            (self.next() % n) as i64
        }
    }

    /// A grid of `cs` cells with random static blocking-only and
    /// occluding-only cells, plus random `GridBlocker` and `GridOccluder`
    /// entities (the dynamic layer, applied by the spatial system).
    fn random_grid(cs: i32, rng: &mut Rng) -> SpatialGrid {
        let mut app = App::new();
        app.insert_resource(SpatialGrid::new(cs, GridBounds::centered(80, 80)));
        app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin));
        app.finish();
        app.cleanup();
        {
            let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
            for k in 0..240 {
                let c = CellCoord::new(rng.below(60) as i32 - 30, rng.below(60) as i32 - 30);
                // Blocking only, occluding only, or both.
                let (blocking, occluding) = [(true, false), (false, true), (true, true)][k % 3];
                grid.set_static(
                    c,
                    CellInfo {
                        flags: CellFlags {
                            blocking,
                            occluding,
                            tags: 0,
                        },
                        extra_cost: 0,
                    },
                );
            }
        }
        let cell = i64::from(cs);
        for k in 0..60 {
            let pos = SimPos::new(
                ((rng.below(60) - 30) * cell + rng.below(cell as u64)) as i32,
                ((rng.below(60) - 30) * cell + rng.below(cell as u64)) as i32,
            );
            let fp = Footprint {
                half_x_mm: (rng.below(2) * cell) as i32,
                half_z_mm: (rng.below(2) * cell) as i32,
            };
            if k % 2 == 0 {
                spawn_sim(app.world_mut(), (pos, GridBlocker(fp)));
            } else {
                spawn_sim(app.world_mut(), (pos, GridOccluder(fp)));
            }
        }
        run_ticks(app.world_mut(), 1);
        app.world().resource::<SpatialGrid>().clone()
    }

    /// The bitmap walk finds the same first solid cell entry, at the same
    /// distance, as `trace_segment` with [`entry_distance`] (the fallback):
    /// static blocking-only and occluding-only cells, dynamic `GridBlocker`
    /// and `GridOccluder` cells, obstacle (thing) cells, starts inside a
    /// solid cell, and exact corners on 45° and non-45° rays.
    #[test]
    fn march_matches_trace_segment() {
        let mut rng = Rng(7);
        let range = 12_000;
        let mut solid_starts = 0;
        let mut corner_rays = 0;
        for cs in [1000, 500, 333] {
            let grid = random_grid(cs, &mut rng);
            let cell = i64::from(cs);
            let mut things: Vec<CellCoord> = (0..80)
                .map(|_| CellCoord::new(rng.below(60) as i32 - 30, rng.below(60) as i32 - 30))
                .collect();
            things.sort_unstable();
            things.dedup();
            let solid_at = |c: CellCoord| {
                let f = grid.cell(c).flags;
                f.blocking || f.occluding || things.binary_search(&c).is_ok()
            };
            for k in 0..3000 {
                // Starts on a cell corner, a cell centre or anywhere.
                let (x, z) = (rng.below(8000) - 4000, rng.below(8000) - 4000);
                let a = match k % 3 {
                    0 => SimPos::new((x / cell * cell) as i32, (z / cell * cell) as i32),
                    1 => grid.center(grid.cell_of(SimPos::new(x as i32, z as i32))),
                    _ => SimPos::new(x as i32, z as i32),
                };
                let r = i64::from(range);
                let b = if k % 4 == 0 {
                    // Through lattice corners: (p, q) cells per step, at
                    // 45° and at other angles (2:1, 3:1, 3:2, ..).
                    let (p, q) =
                        [(1, 1), (2, 1), (1, 2), (3, 1), (3, 2), (1, 3)][rng.below(6) as usize];
                    let (sx, sz) = (
                        if rng.below(2) == 0 { 1 } else { -1 },
                        if rng.below(2) == 0 { 1 } else { -1 },
                    );
                    let n = (r / (cell * (p + q))).max(1);
                    corner_rays += 1;
                    SimPos::new(
                        (i64::from(a.x) + sx * p * n * cell) as i32,
                        (i64::from(a.z) + sz * q * n * cell) as i32,
                    )
                } else {
                    let (dx, dz) = dir_ppm(rng.below(360_000));
                    SimPos::new(
                        (i64::from(a.x) + dx * r / PPM) as i32,
                        (i64::from(a.z) + dz * r / PPM) as i32,
                    )
                };
                let start = grid.cell_of(a);
                if solid_at(start) {
                    solid_starts += 1;
                }
                let reach = range / cs + 2;
                let win = Window {
                    x0: start.x - reach,
                    z0: start.z - reach,
                    w: 2 * reach + 1,
                };
                let mut solid = Vec::new();
                win.fill(&grid, &things, &mut solid);
                let len = i64::try_from(u64::try_from(a.dist2(b)).unwrap_or(0).isqrt())
                    .unwrap_or(i64::MAX);
                let fast = march(&win, &solid, cell, a, b, start)
                    .map(|(n, den)| if den > 0 { n * len / den } else { 0 });
                let mut hit = None;
                grid.trace_segment(a, b, |c| {
                    if c != start && solid_at(c) {
                        hit = Some(c);
                        return false;
                    }
                    true
                });
                let slow = hit.map(|c| entry_distance(a, b, len, c, cell));
                assert_eq!(fast, slow, "cs {cs}, {a:?} -> {b:?}");
            }
        }
        assert!(
            solid_starts > 50,
            "{solid_starts} starts inside a solid cell"
        );
        assert!(corner_rays > 500, "{corner_rays} rays through corners");
    }
}
