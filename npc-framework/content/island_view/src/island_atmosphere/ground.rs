//! How the regional ground is coloured. The simulation's terrain cells are
//! only read: a colour is computed for every corner of the cell grid, so
//! neighbouring cells blend into each other instead of meeting at hard
//! edges. Paths are drawn narrower than the walkable strip, authored roads
//! as straight lines without stair steps, biome borders are feathered and
//! wobbled, turf carries low-frequency variation, ridge stone gathers into
//! outcrops and the land turns to sand at the water. A small tiling grain
//! texture, generated here, gives the ground detail up close; roads wander
//! a little, vary in width and wear at the edges; and the coast gets a
//! sloped beach instead of a stepped bank. Nothing here changes which cells
//! are walkable or what they cost.
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use island_content::village::{
    Season,
    frontier::{
        terrain::{TerrainState, TerrainSurface},
        world_spec::{Biome, LivingWorldSpec},
    },
};
use npc_framework::prelude::CellCoord;

/// How far (cells) beyond its own chunk a colour can read a cell's surface
/// or height. A chunk is rebuilt when a cell this close to it is edited.
pub(crate) const NEAR_MARGIN: i32 = 4;
/// How far (cells) beyond its own chunk a colour can read a cell's biome.
pub(crate) const MARGIN: i32 = 9;
/// Cells beyond the chunk whose look is needed (the rest is biome only).
const LOOK_MARGIN: i32 = 2;

/// The generator scatters ridge stone as runs of four cells every nineteen
/// and turns everything above this height to rock.
const SCATTER_PERIOD: u32 = 19;
const SCATTER_RUN: u32 = 4;
const HIGH_ROCK_MM: i32 = 6500;
/// Share of the outcrop field that shows as stone, about a fifth of a ridge.
const OUTCROP_THRESHOLD: f32 = 0.645;
const BINOMIAL: [f32; 4] = [1.0, 3.0, 3.0, 1.0];

/// What a cell looks like, which is not always its simulation surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ground {
    Water,
    /// Walkable soil that is not part of an authored road.
    Path,
    /// Walkable soil inside the strip of an authored road.
    Road,
    Rock,
    Turf,
    /// Untouched ridge ground: turf or stone as the outcrop field decides.
    Ridge,
}

fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn linear(srgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b, _] = Color::srgb(srgb[0], srgb[1], srgb[2])
        .to_linear()
        .to_f32_array();
    [r, g, b]
}

/// A repeatable number in `0..1` for a lattice point and a purpose.
pub(crate) fn lattice(x: i32, z: i32, salt: u32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (z as u32).wrapping_mul(0x85EB_CA77)
        ^ salt.wrapping_mul(0xC2B2_AE3D);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    n = n.wrapping_mul(0x297A_2D39);
    n ^= n >> 15;
    (n >> 8) as f32 / 16_777_216.0
}

/// Smooth noise in `0..1` that repeats nothing and varies over `period` metres.
pub(crate) fn value_noise(x: f32, z: f32, period: f32, salt: u32) -> f32 {
    let (u, v) = (x / period, z / period);
    let (x0, z0) = (u.floor(), v.floor());
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, tz) = (fade(u - x0), fade(v - z0));
    let (x0, z0) = (x0 as i32, z0 as i32);
    let near = lattice(x0, z0, salt) + (lattice(x0 + 1, z0, salt) - lattice(x0, z0, salt)) * tx;
    let far = lattice(x0, z0 + 1, salt)
        + (lattice(x0 + 1, z0 + 1, salt) - lattice(x0, z0 + 1, salt)) * tx;
    near + (far - near) * tz
}

/// Stone weight (`0..1`) of the ridge outcrops at a point: a thresholded
/// low-frequency field, so stone gathers into patches several metres across.
pub(crate) fn outcrop(x: f32, z: f32) -> f32 {
    // Lattice noise is axis-aligned; a displaced and rotated lookup keeps
    // the patches from reading as soft rectangles.
    let (x, z) = (
        x + (value_noise(x, z, 11.0, 13) - 0.5) * 10.0,
        z + (value_noise(x, z, 11.0, 14) - 0.5) * 10.0,
    );
    let (rx, rz) = (x * 0.819 - z * 0.574, x * 0.574 + z * 0.819);
    let field = 0.68 * value_noise(rx, rz, 17.0, 11) + 0.32 * value_noise(x, z, 6.0, 12);
    smoothstep(OUTCROP_THRESHOLD - 0.018, OUTCROP_THRESHOLD + 0.018, field)
}

fn scatter_slot(terrain: &TerrainState, x: i32, z: i32) -> bool {
    terrain
        .bounds()
        .index(CellCoord::new(x, z))
        .is_some_and(|index| index % SCATTER_PERIOD < SCATTER_RUN)
}

/// Generated ridge ground is redrawn as outcrops. Anything a player has
/// painted keeps its own surface: stone outside a generated slot (and stone
/// touching it) stays rock, and turf painted over a generated stone slot
/// keeps the outcrop away from the cells around it.
pub(crate) fn classify(terrain: &TerrainState, x: i32, z: i32) -> Ground {
    let Some(cell) = terrain.cell(x, z) else {
        return Ground::Water;
    };
    let plain = match cell.surface {
        TerrainSurface::Water => return Ground::Water,
        TerrainSurface::Soil => return Ground::Path,
        TerrainSurface::Stone => Ground::Rock,
        TerrainSurface::Grass => Ground::Turf,
    };
    if cell.biome != Biome::Ridge || cell.height_mm > HIGH_ROCK_MM {
        return plain;
    }
    if cell.surface == TerrainSurface::Stone {
        let painted = !scatter_slot(terrain, x, z)
            || [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .into_iter()
                .any(|(dx, dz)| {
                    terrain.cell(x + dx, z + dz).is_some_and(|near| {
                        near.surface == TerrainSurface::Stone
                            && !scatter_slot(terrain, x + dx, z + dz)
                    })
                });
        if painted {
            return Ground::Rock;
        }
    }
    // Every ridge cell has a generated stone slot within two cells.
    for dz in -2..=2 {
        for dx in -2..=2 {
            if scatter_slot(terrain, x + dx, z + dz)
                && terrain.cell(x + dx, z + dz).is_some_and(|near| {
                    near.biome == Biome::Ridge
                        && near.height_mm <= HIGH_ROCK_MM
                        && near.surface == TerrainSurface::Grass
                })
            {
                return Ground::Turf;
            }
        }
    }
    Ground::Ridge
}

/// The generator measures in integer millimetres; allow for its rounding
/// when deciding whether a soil cell belongs to a strip.
const STRIP_SLACK: f32 = 0.03;

/// One straight stretch of an authored road, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stretch {
    from: Vec2,
    to: Vec2,
    /// Half the width of the walkable strip the generator laid along it.
    radius: f32,
}

impl Stretch {
    fn distance(&self, point: Vec2) -> f32 {
        let along = self.to - self.from;
        let length = along.length_squared();
        let t = if length > 0.0 {
            ((point - self.from).dot(along) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        point.distance(self.from + along * t)
    }

    /// Unit vector along the stretch; zero for a stretch without length.
    fn direction(&self) -> Vec2 {
        (self.to - self.from).normalize_or_zero()
    }

    /// Drawn share of road at `distance` from the centre line: the walkable
    /// strip is 2 to 3.4 m wide, the drawn road 1.6 to 3 m at half strength
    /// with a ramp of more than a metre on each side.
    /// This is the nominal profile; `road_look` draws it with wander, width
    /// variation and fray, and the tests compare the two.
    #[cfg(test)]
    fn drawn(&self, distance: f32) -> f32 {
        1.0 - smoothstep(self.radius - 0.9, self.radius + 0.5, distance)
    }
}

/// The radius (m) of the trodden earth round a door or a site's entrance,
/// and of a desire line's walked strip between neighbouring workplaces.
const DOOR_APRON_M: f32 = 1.3;
const DESIRE_LINE_M: f32 = 0.55;
/// How far worn ground goes towards the path's colour at its most worn.
const WEAR_STRENGTH: f32 = 0.6;

/// The centre lines of the authored roads, from the same world description
/// the terrain generator used: a main road from the arrival point to every
/// settlement centre, a spine through each settlement and a lane to every
/// site and door. Drawing from the lines gives straight, smooth edges where
/// the cells themselves are stair-stepped.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub(crate) struct Roads(Vec<Stretch>, Vec<Stretch>);

impl Roads {
    pub(crate) fn from_spec(spec: &LivingWorldSpec) -> Self {
        let metres = |x: i32, z: i32| Vec2::new(x as f32 / 1000.0, z as f32 / 1000.0);
        let mut stretches = Vec::new();
        // Trodden earth that is not a road the generator laid: door aprons
        // and desire lines, drawn as worn ground only.
        let mut worn = Vec::new();
        let mut add = |from: Vec2, to: Vec2, radius: f32| {
            let stretch = Stretch { from, to, radius };
            if !stretches.contains(&stretch) {
                stretches.push(stretch);
            }
        };
        let mut wear = |from: Vec2, to: Vec2, radius: f32| {
            let stretch = Stretch { from, to, radius };
            if !worn.contains(&stretch) {
                worn.push(stretch);
            }
        };
        let start = metres(spec.actor_start.x, spec.actor_start.z);
        for settlement in &spec.settlements {
            let centre = metres(settlement.center.x, settlement.center.z);
            add(start, centre, 1.7);
            for entrance in settlement
                .sites
                .iter()
                .map(|site| site.position)
                .chain(settlement.homes.iter().map(|home| home.door))
            {
                let bend = metres(settlement.center.x, entrance.z);
                add(centre, bend, 1.2);
                add(bend, metres(entrance.x, entrance.z), 1.0);
                // Trodden earth round every door and site entrance.
                let door = metres(entrance.x, entrance.z);
                wear(door, door, DOOR_APRON_M);
            }
            // Desire lines: people cut straight across from each workplace
            // to its nearest neighbour instead of going round by the lanes.
            let sites: Vec<Vec2> = settlement
                .sites
                .iter()
                .map(|site| metres(site.position.x, site.position.z))
                .collect();
            for (i, site) in sites.iter().enumerate() {
                if let Some(nearest) = sites
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| *j != i && site.distance(**other) > 2.0)
                    .min_by(|a, b| site.distance(*a.1).total_cmp(&site.distance(*b.1)))
                    .map(|(_, other)| *other)
                {
                    wear(*site, nearest, DESIRE_LINE_M);
                }
            }
        }
        Self(stretches, worn)
    }

    /// Whether `point` (metres) is at least `margin` metres clear of every
    /// road's walkable strip: scenery keeps off the paths.
    pub(crate) fn clear_of(&self, point: Vec2, margin: f32) -> bool {
        self.0
            .iter()
            .chain(&self.1)
            .all(|stretch| stretch.distance(point) > stretch.radius + margin)
    }

    /// The worn stretches that can reach the square `min..max` (metres).
    fn worn_near(&self, min: Vec2, max: Vec2) -> Vec<Stretch> {
        Self(self.1.clone(), Vec::new()).near(min, max)
    }

    /// The stretches that can reach the square `min..max` (metres).
    fn near(&self, min: Vec2, max: Vec2) -> Vec<Stretch> {
        self.0
            .iter()
            .filter(|stretch| {
                // Wider than anything a stretch draws (width variation,
                // wander, fray and verge), so two chunks sharing a corner
                // always see the same stretches there.
                let reach = Vec2::splat(stretch.radius * 1.2 + 2.4);
                let low = stretch.from.min(stretch.to) - reach;
                let high = stretch.from.max(stretch.to) + reach;
                low.x <= max.x && high.x >= min.x && low.y <= max.y && high.y >= min.y
            })
            .copied()
            .collect()
    }
}

/// How far (m) a drawn road may wander from its centre line. The walkable
/// strip is at least a metre to each side, so the drawn path stays on it.
const ROAD_WANDER: f32 = 0.26;
/// Width varies by this share to either side along a road.
const ROAD_WIDTH_VARIATION: f32 = 0.15;
/// Roundness (m) of the fillet where two stretches meet.
const JUNCTION_FILLET: f32 = 0.55;
/// The worn verge: trampled ground this far (m) beyond the drawn edge.
const VERGE_REACH: f32 = 1.1;
const VERGE_STRENGTH: f32 = 0.34;

/// Drawn road and worn verge at a point, both `0..1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RoadLook {
    pub(crate) path: f32,
    pub(crate) verge: f32,
}

/// How the stretches near a point are drawn there. The path is a trodden
/// one: it wanders a little inside its walkable strip, breathes in width,
/// frays at the edge, rounds off where stretches meet and leaves a worn
/// verge beside it. Pure in the point, so chunks agree on shared corners.
fn road_look(roads: &[Stretch], point: Vec2) -> RoadLook {
    if roads.is_empty() {
        return RoadLook::default();
    }
    let wander = Vec2::new(
        value_noise(point.x, point.y, 13.0, 31) - 0.5,
        value_noise(point.x, point.y, 13.0, 32) - 0.5,
    ) * (2.0 * ROAD_WANDER);
    let at = point + wander;
    let width = 1.0 + (value_noise(point.x, point.y, 8.5, 33) - 0.5) * (2.0 * ROAD_WIDTH_VARIATION);
    let fray = (value_noise(point.x, point.y, 1.9, 34) - 0.5) * 0.30;
    let signed_to = |road: &Stretch| road.distance(at) - road.radius * width;
    let Some(nearest) = roads
        .iter()
        .min_by(|a, b| signed_to(a).total_cmp(&signed_to(b)))
    else {
        return RoadLook::default();
    };
    let near = signed_to(nearest);
    // A smooth union with the stretch that crosses the nearest one: a
    // fillet, not a crease, where a lane leaves a road. Stretches along the
    // same line (the spine is laid once per entrance) add nothing, so a
    // straight road keeps its width.
    let fillet = roads
        .iter()
        .map(|other| {
            let crossing = nearest.direction().perp_dot(other.direction()).abs();
            let gap = (signed_to(other) - near).max(0.0);
            crossing * JUNCTION_FILLET * (1.0 + (-gap / JUNCTION_FILLET).exp()).ln()
        })
        .fold(0.0, f32::max);
    let signed = near - fillet;
    let path = 1.0 - smoothstep(-0.9, 0.5, signed + fray);
    let verge = (1.0 - smoothstep(-0.2, VERGE_REACH, signed + fray * 2.2)) * (1.0 - path);
    RoadLook { path, verge }
}

/// The cells one chunk's colours can read, fetched and classified once.
pub(crate) struct Patch {
    x0: i32,
    z0: i32,
    side: i32,
    ground: Vec<Ground>,
    biome: Vec<Biome>,
    roads: Vec<Stretch>,
    /// Door aprons and desire lines near the patch.
    worn: Vec<Stretch>,
}

impl Patch {
    /// Covers the `size` x `size` cells at `(x, z)` plus every margin.
    pub(crate) fn around(terrain: &TerrainState, roads: &Roads, x: i32, z: i32, size: i32) -> Self {
        let (x0, z0) = (x - MARGIN, z - MARGIN);
        let side = size + 2 * MARGIN;
        let skip = MARGIN - LOOK_MARGIN;
        let reach = (
            Vec2::new((x - LOOK_MARGIN) as f32, (z - LOOK_MARGIN) as f32),
            Vec2::new(
                (x + size + LOOK_MARGIN) as f32,
                (z + size + LOOK_MARGIN) as f32,
            ),
        );
        let worn = roads.worn_near(reach.0, reach.1);
        let roads = roads.near(reach.0, reach.1);
        let mut ground = Vec::with_capacity((side * side) as usize);
        let mut biome = Vec::with_capacity((side * side) as usize);
        for dz in 0..side {
            for dx in 0..side {
                let cell = terrain.cell(x0 + dx, z0 + dz);
                biome.push(cell.map_or(Biome::Coast, |cell| cell.biome));
                // `classify` reads two cells further than the one it is given.
                let wanted = dx >= skip && dz >= skip && dx < side - skip && dz < side - skip;
                let look = if wanted {
                    classify(terrain, x0 + dx, z0 + dz)
                } else {
                    Ground::Water
                };
                // The generator tested each cell centre against the strip.
                let centre = Vec2::new((x0 + dx) as f32 + 0.5, (z0 + dz) as f32 + 0.5);
                ground.push(
                    if look == Ground::Path
                        && roads
                            .iter()
                            .any(|road| road.distance(centre) <= road.radius + STRIP_SLACK)
                    {
                        Ground::Road
                    } else {
                        look
                    },
                );
            }
        }
        Self {
            x0,
            z0,
            side,
            ground,
            biome,
            roads,
            worn,
        }
    }

    /// Drawn share of authored road, and of its worn verge, at a corner.
    fn road_line(&self, x: i32, z: i32) -> RoadLook {
        road_look(&self.roads, Vec2::new(x as f32, z as f32))
    }

    /// How worn the ground is at a corner (0..1): trodden earth round the
    /// doors and along the desire lines, soft at its edge.
    fn wear(&self, x: i32, z: i32) -> f32 {
        let point = Vec2::new(x as f32, z as f32);
        self.worn
            .iter()
            .map(|stretch| {
                1.0 - smoothstep(
                    stretch.radius - 0.5,
                    stretch.radius + 0.7,
                    stretch.distance(point),
                )
            })
            .fold(0.0, f32::max)
    }

    fn slot(&self, x: i32, z: i32) -> Option<usize> {
        let (dx, dz) = (x - self.x0, z - self.z0);
        (dx >= 0 && dz >= 0 && dx < self.side && dz < self.side)
            .then(|| (dz * self.side + dx) as usize)
    }

    pub(crate) fn ground(&self, x: i32, z: i32) -> Ground {
        self.slot(x, z).map_or(Ground::Water, |i| self.ground[i])
    }

    fn biome(&self, x: i32, z: i32) -> Biome {
        self.slot(x, z).map_or(Biome::Coast, |i| self.biome[i])
    }
}

fn turf_srgb(biome: Biome) -> [f32; 3] {
    match biome {
        Biome::Coast => [0.59, 0.68, 0.40],
        Biome::Meadow => [0.45, 0.64, 0.28],
        Biome::Woodland => [0.27, 0.46, 0.24],
        Biome::Ridge => [0.49, 0.58, 0.33],
        Biome::Wetland => [0.30, 0.50, 0.36],
    }
}

/// Linear turf colour of one biome in one season.
pub(crate) fn turf(biome: Biome, season: Season) -> [f32; 3] {
    let [r, g, b] = turf_srgb(biome);
    linear(match season {
        Season::Spring => [r, g, b],
        Season::Summer => [r * 1.03, g * 0.97, b * 0.92],
        // Muted, not yellow: each green dulls a little and takes a tenth
        // of ochre, so the coast stays olive and the woods stay green.
        Season::Autumn => {
            let dulled = [r * 0.95, g * 0.90, b * 0.93];
            let mixed = mix(dulled, AUTUMN_OCHRE, 0.10);
            [mixed[0], mixed[1], mixed[2]]
        }
        Season::Winter => [0.84, 0.88, 0.90],
    })
}

const AUTUMN_OCHRE: [f32; 3] = [0.60, 0.52, 0.30];

struct Palette {
    rock: [f32; 3],
    /// The ground of a ridge outcrop, under its rocks: scree, not bare slab.
    scree: [f32; 3],
    path: [f32; 3],
    /// Trampled ground beside a path.
    verge: [f32; 3],
    sand: [f32; 3],
    dry: [f32; 3],
    /// Most a dry patch replaces the turf.
    dry_strength: f32,
}

fn palette(season: Season) -> Palette {
    let snow = linear([0.84, 0.88, 0.90]);
    let winter = if season == Season::Winter { 1.0 } else { 0.0 };
    let autumn = season == Season::Autumn;
    Palette {
        rock: mix(linear([0.60, 0.60, 0.59]), snow, 0.45 * winter),
        scree: mix(linear([0.47, 0.46, 0.42]), snow, 0.35 * winter),
        path: mix(linear([0.47, 0.36, 0.25]), snow, 0.25 * winter),
        verge: mix(linear([0.52, 0.47, 0.30]), snow, 0.6 * winter),
        sand: mix(linear([0.80, 0.74, 0.53]), snow, 0.5 * winter),
        // Autumn's dry grass is ochre and there is less of it; the summer
        // straw made the coast read as yellow.
        dry: mix(
            linear(if autumn {
                [0.57, 0.52, 0.33]
            } else {
                [0.64, 0.60, 0.33]
            }),
            snow,
            winter,
        ),
        dry_strength: if autumn { 0.20 } else { 0.30 },
    }
}

/// Turf colour at a corner: sixteen biome taps around a wobbled position,
/// so a border is a soft, irregular band about nine metres wide.
fn feathered_turf(patch: &Patch, x: i32, z: i32, season: Season) -> [f32; 3] {
    let (fx, fz) = (x as f32, z as f32);
    let wobble_x = (value_noise(fx, fz, 19.0, 21) - 0.5) * 7.0;
    let wobble_z = (value_noise(fx, fz, 19.0, 22) - 0.5) * 7.0;
    let mut sum = [0.0_f32; 3];
    let mut total = 0.0;
    const TAPS: [f32; 4] = [-4.5, -1.5, 1.5, 4.5];
    for (row, dz) in TAPS.into_iter().enumerate() {
        for (column, dx) in TAPS.into_iter().enumerate() {
            let weight = BINOMIAL[row] * BINOMIAL[column];
            let tap_x = (fx + wobble_x + dx).floor() as i32;
            let tap_z = (fz + wobble_z + dz).floor() as i32;
            let colour = turf(patch.biome(tap_x, tap_z), season);
            for channel in 0..3 {
                sum[channel] += colour[channel] * weight;
            }
            total += weight;
        }
    }
    sum.map(|channel| channel / total)
}

/// How much of each look surrounds the corner at `(x, z)`, the lattice point
/// shared by cells `(x-1, z-1)..=(x, z)`: a 4 x 4 binomial window, which is
/// what rounds off stair-stepped diagonals.
struct Mix {
    path: f32,
    road: f32,
    rock: f32,
    ridge: f32,
    water: f32,
}

fn surroundings(patch: &Patch, x: i32, z: i32) -> Mix {
    let mut mix = Mix {
        path: 0.0,
        road: 0.0,
        rock: 0.0,
        ridge: 0.0,
        water: 0.0,
    };
    for (row, dz) in (-2..2).enumerate() {
        for (column, dx) in (-2..2).enumerate() {
            let weight = BINOMIAL[row] * BINOMIAL[column] / 64.0;
            match patch.ground(x + dx, z + dz) {
                Ground::Water => mix.water += weight,
                Ground::Path => mix.path += weight,
                Ground::Road => mix.road += weight,
                Ground::Rock => mix.rock += weight,
                Ground::Ridge => mix.ridge += weight,
                Ground::Turf => {}
            }
        }
    }
    mix
}

/// Drawn share of path at a corner from the share of walkable soil around
/// it. A lane two cells wide draws about 1.5 m, three cells 2.5 m and the
/// four-cell main road 3.5 m, each with a soft edge.
pub(crate) fn path_weight(soil_share: f32) -> f32 {
    smoothstep(0.38, 0.75, soil_share)
}

/// Linear RGBA of the terrain corner at `(x, z)`.
pub(crate) fn corner_color(patch: &Patch, x: i32, z: i32, season: Season) -> [f32; 4] {
    let (fx, fz) = (x as f32, z as f32);
    let around = surroundings(patch, x, z);
    let palette = palette(season);
    // Broad light and dark patches plus a finer grain, a few percent each.
    let shade = 1.0
        + (value_noise(fx, fz, 23.0, 1) - 0.5) * 0.20
        + (value_noise(fx, fz, 7.0, 2) - 0.5) * 0.09;
    let dry = smoothstep(0.52, 0.80, value_noise(fx, fz, 37.0, 3)) * palette.dry_strength;
    let mut colour = mix(feathered_turf(patch, x, z, season), palette.dry, dry);
    // Winter's snow lies unevenly: drifts a little brighter, and bare
    // patches where the wind took it, the dull autumn turf showing through.
    if season == Season::Winter {
        let (bare, drift) = snow_cover(fx, fz);
        let thaw = feathered_turf(patch, x, z, Season::Autumn).map(|c| c * 0.82);
        colour = mix(colour, thaw, bare);
        colour = colour.map(|c| (c * (1.0 + 0.06 * drift)).min(1.0));
    }
    let land = (1.0 - around.water).max(0.001);
    let rock_shade = 0.90 + value_noise(fx, fz, 2.6, 4) * 0.20;
    // Painted stone is bare slab. A ridge outcrop is scree: darker ground
    // that the rock meshes standing on it (`dressing`) turn into an outcrop.
    let slab = smoothstep(0.42, 0.78, around.rock / land);
    let scree = outcrop(fx, fz) * smoothstep(0.3, 0.9, around.ridge / land);
    colour = mix(colour, palette.scree.map(|c| c * rock_shade), scree * 0.85);
    colour = mix(colour, palette.rock.map(|c| c * rock_shade), slab);
    // An authored road follows its centre line for as long as its cells are
    // still soil; any other soil is drawn from the cells themselves, taking
    // road cells along where the two touch.
    let road = patch.road_line(x, z);
    let soil = smoothstep(0.15, 0.45, around.path + around.road);
    let loose = path_weight(around.path + around.road * smoothstep(0.0, 0.25, around.path));
    colour = mix(colour, palette.verge, road.verge * soil * VERGE_STRENGTH);
    // Trodden earth at the doors and along desire lines, patchy.
    let trodden = patch.wear(x, z) * (0.55 + 0.25 * value_noise(fx, fz, 3.1, 44));
    colour = mix(colour, palette.path, trodden * WEAR_STRENGTH);
    colour = mix(colour, palette.path, (road.path * soil).max(loose));
    colour = colour.map(|channel| channel * shade);
    // The last metre or two before water is beach.
    colour = mix(
        colour,
        palette.sand,
        smoothstep(0.02, 0.5, around.water) * 0.9,
    );
    [colour[0], colour[1], colour[2], 1.0]
}

/// How the snow lies at ground point `(x, z)`: the share of bare ground
/// showing (0 deep snow, up to 0.7 a wind-scoured patch) and a drift's
/// extra depth (0..1). Large soft patches, so the island is not one white
/// sheet.
pub(crate) fn snow_cover(x: f32, z: f32) -> (f32, f32) {
    let scoured = smoothstep(0.58, 0.8, value_noise(x, z, 29.0, 41));
    let thin = smoothstep(0.62, 0.86, value_noise(x, z, 9.0, 42)) * 0.35;
    let drift = smoothstep(0.55, 0.85, value_noise(x, z, 17.0, 43));
    (((scoured * 0.7) + thin * (1.0 - scoured)).min(0.7), drift)
}

/// Opacity (`0..1`) of the shallow-water tint at a corner: full where land
/// surrounds half of the 6 m window, gone three metres from the shore.
pub(crate) fn shallows(terrain: &TerrainState, x: i32, z: i32) -> f32 {
    let mut land = 0;
    for dz in -3..3 {
        for dx in -3..3 {
            if terrain
                .cell(x + dx, z + dz)
                .is_some_and(|cell| cell.surface != TerrainSurface::Water)
            {
                land += 1;
            }
        }
    }
    smoothstep(0.0, 0.5, land as f32 / 36.0)
}

/// Height (m) the shore slope starts from at most: a taller cliff keeps its
/// face above this and gets a scree slope at its foot.
pub(crate) const BEACH_TOP: f32 = 3.0;
/// Up to this height (m) the slope is sand; by `BEACH_TOP` it is the bank's
/// own ground, a shade darker.
const SAND_TOP: f32 = 0.7;

/// How much of a shore-slope corner at `height` metres is sand (`0..1`).
pub(crate) fn sandiness(height: f32) -> f32 {
    1.0 - smoothstep(SAND_TOP, 2.2, height)
}
/// Depth (m) the beach reaches a metre out, under the water surface.
pub(crate) const BEACH_FOOT: f32 = -0.5;

/// Height (m) of the beach at the grid corner `(x, z)`: the mean height of
/// the dry cells touching it, or `None` where the corner touches no land.
/// A water cell's four corners make its beach; two cells sharing a corner
/// get the same height, so the beach has no cracks.
pub(crate) fn beach_corner(terrain: &TerrainState, x: i32, z: i32) -> Option<f32> {
    let (mut sum, mut count) = (0.0_f32, 0);
    for (dx, dz) in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
        if let Some(cell) = terrain.cell(x + dx, z + dz)
            && cell.surface != TerrainSurface::Water
        {
            sum += (cell.height_mm as f32 / 1000.0).min(BEACH_TOP);
            count += 1;
        }
    }
    (count > 0).then(|| sum / count as f32)
}

/// Linear RGBA of the beach below the waterline.
pub(crate) fn wet_sand(season: Season) -> [f32; 4] {
    let [r, g, b] = palette(season).sand.map(|channel| channel * 0.62);
    [r, g, b, 1.0]
}

/// Linear RGBA of the beach where it meets the land.
pub(crate) fn dry_sand(season: Season) -> [f32; 4] {
    let [r, g, b] = palette(season).sand;
    [r, g, b, 1.0]
}

/// Side (texels) and world size (m) of one tile of the ground grain.
pub(crate) const DETAIL_SIZE: usize = 256;
pub(crate) const DETAIL_TILE: f32 = 4.0;
/// Darkest texel and mean of the grain, as multipliers of the ground colour.
const DETAIL_FLOOR: f32 = 0.66;
pub(crate) const DETAIL_MEAN: f32 = 0.83;

/// Texture coordinate of a ground point. The tile is turned against the
/// cell grid, so its repeat never lines up with a road or a field edge.
pub(crate) fn detail_uv(x: f32, z: f32) -> [f32; 2] {
    let (sin, cos) = (0.515_038_1_f32, 0.857_167_3_f32);
    [
        (x * cos - z * sin) / DETAIL_TILE,
        (x * sin + z * cos) / DETAIL_TILE,
    ]
}

fn wrapped_lattice(x: i32, z: i32, cells: i32, salt: u32) -> f32 {
    lattice(x.rem_euclid(cells), z.rem_euclid(cells), salt)
}

/// Value noise with `cells` lattice cells across a tile that wraps.
fn tile_noise(u: f32, v: f32, cells: i32, salt: u32) -> f32 {
    let (x, z) = (u * cells as f32, v * cells as f32);
    let (x0, z0) = (x.floor(), z.floor());
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, tz) = (fade(x - x0), fade(z - z0));
    let (x0, z0) = (x0 as i32, z0 as i32);
    let at = |dx: i32, dz: i32| wrapped_lattice(x0 + dx, z0 + dz, cells, salt);
    let near = at(0, 0) + (at(1, 0) - at(0, 0)) * tx;
    let far = at(0, 1) + (at(1, 1) - at(0, 1)) * tx;
    near + (far - near) * tz
}

/// The grain as multipliers in `DETAIL_FLOOR..=1`, row-major, one per texel
/// of a `size` x `size` tile. Only fine features (6 to 40 cm): by the zoom
/// at which a 4 m repeat could be seen, the mip chain has averaged them
/// away. The mean is `DETAIL_MEAN`, so the material can cancel it.
pub(crate) fn detail_grain(size: usize) -> Vec<f32> {
    let mut grain = Vec::with_capacity(size * size);
    for row in 0..size {
        for column in 0..size {
            let (u, v) = (column as f32 / size as f32, row as f32 / size as f32);
            // Clumps, blades and specks.
            let field = 0.34 * tile_noise(u, v, 10, 41)
                + 0.38 * tile_noise(u, v, 24, 42)
                + 0.28 * tile_noise(u, v, 64, 43);
            grain.push(field);
        }
    }
    // Stretch to the full range, then shift so the mean is exact.
    let low = grain.iter().copied().fold(f32::INFINITY, f32::min);
    let high = grain.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    for value in &mut grain {
        *value = DETAIL_FLOOR + (*value - low) / (high - low).max(1e-6) * (1.0 - DETAIL_FLOOR);
    }
    let mean = grain.iter().sum::<f32>() / grain.len() as f32;
    for value in &mut grain {
        *value = (*value + DETAIL_MEAN - mean).clamp(DETAIL_FLOOR - 0.06, 1.0);
    }
    grain
}

/// The next mip level: each texel the mean of four.
fn halved(grain: &[f32], size: usize) -> Vec<f32> {
    let half = size / 2;
    (0..half * half)
        .map(|i| {
            let (x, y) = (i % half * 2, i / half * 2);
            (grain[y * size + x]
                + grain[y * size + x + 1]
                + grain[(y + 1) * size + x]
                + grain[(y + 1) * size + x + 1])
                / 4.0
        })
        .collect()
}

/// The grain as a repeating texture with a full mip chain and trilinear,
/// anisotropic sampling: detail up close, plain colour from afar, and no
/// shimmer in between. Generated in code; no asset file.
pub(crate) fn detail_image() -> Image {
    let mut level = detail_grain(DETAIL_SIZE);
    let mut size = DETAIL_SIZE;
    let mut data = Vec::new();
    let mut levels = 0;
    loop {
        data.extend(level.iter().flat_map(|value| {
            let byte = (value * 255.0).round().clamp(0.0, 255.0) as u8;
            [byte, byte, byte, 255]
        }));
        levels += 1;
        if size == 1 {
            break;
        }
        level = halved(&level, size);
        size /= 2;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: DETAIL_SIZE as u32,
            height: DETAIL_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // Linear: the texels are multipliers, not colours.
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(data);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_is_worn_at_the_doors_and_along_desire_lines() {
        let app = island_content::new_sim_app(7, "living_world");
        let spec = app.world().resource::<LivingWorldSpec>();
        let terrain = app.world().resource::<TerrainState>();
        let roads = Roads::from_spec(spec);
        assert!(!roads.1.is_empty(), "door aprons and desire lines");
        let door = spec.settlements[0].homes[0].door;
        let (x, z) = (door.x.div_euclid(1000), door.z.div_euclid(1000));
        let patch = Patch::around(terrain, &roads, x - 8, z - 8, 16);
        assert!(patch.wear(x, z) > 0.8, "worn at the door");
        assert!(patch.wear(x + 12, z + 12) < 0.2 || patch.worn.len() > 1);
        // Scenery keeps off the worn ground too.
        assert!(!roads.clear_of(
            Vec2::new(door.x as f32 / 1000.0, door.z as f32 / 1000.0),
            0.0
        ));
    }

    #[test]
    fn winter_snow_lies_unevenly_with_bare_patches_and_drifts() {
        let mut bare = 0;
        let mut deep = 0;
        let mut drifts = 0;
        for z in 0..120 {
            for x in 0..120 {
                let (open, drift) = snow_cover(x as f32 * 1.5, z as f32 * 1.5);
                assert!((0.0..=0.7).contains(&open));
                if open > 0.3 {
                    bare += 1;
                }
                if open < 0.05 {
                    deep += 1;
                }
                if drift > 0.5 {
                    drifts += 1;
                }
            }
        }
        let total = 120 * 120;
        // Mostly snow, with patches of ground and of drifts: not one sheet.
        assert!(deep > total / 3, "{deep}");
        assert!(bare > total / 25 && bare < total / 2, "{bare}");
        assert!(drifts > total / 25, "{drifts}");
    }
    use island_content::village::frontier::terrain::TerrainCell;

    fn field(width: u32, height: u32, biome: Biome) -> TerrainState {
        TerrainState {
            width,
            height,
            cells: vec![
                TerrainCell {
                    height_mm: 1000,
                    surface: TerrainSurface::Grass,
                    biome,
                };
                (width * height) as usize
            ],
            revision: 0,
        }
    }

    fn set(terrain: &mut TerrainState, x: i32, z: i32, edit: impl Fn(&mut TerrainCell)) {
        let index = terrain.bounds().index(CellCoord::new(x, z)).unwrap() as usize;
        edit(&mut terrain.cells[index]);
    }

    /// A ridge exactly as the generator leaves it.
    fn generated_ridge() -> TerrainState {
        let mut terrain = field(64, 64, Biome::Ridge);
        for index in 0..terrain.cells.len() {
            if index as u32 % SCATTER_PERIOD < SCATTER_RUN {
                terrain.cells[index].surface = TerrainSurface::Stone;
            }
        }
        terrain
    }

    fn luminance(colour: [f32; 4]) -> f32 {
        0.2126 * colour[0] + 0.7152 * colour[1] + 0.0722 * colour[2]
    }

    #[test]
    fn noise_is_smooth_bounded_and_repeatable() {
        let mut previous = value_noise(0.0, 3.3, 17.0, 5);
        for step in 1..2000 {
            let value = value_noise(step as f32 * 0.05, 3.3, 17.0, 5);
            assert!((0.0..=1.0).contains(&value));
            assert!((value - previous).abs() < 0.02, "noise jumps at {step}");
            previous = value;
        }
        assert_eq!(
            value_noise(12.5, -7.25, 9.0, 3),
            value_noise(12.5, -7.25, 9.0, 3)
        );
        assert_ne!(
            value_noise(12.5, -7.25, 9.0, 3),
            value_noise(12.5, -7.25, 9.0, 4)
        );
    }

    #[test]
    fn ridge_stone_gathers_into_outcrops_instead_of_scattered_cells() {
        let stone = |x: i32, z: i32| outcrop(x as f32 + 0.5, z as f32 + 0.5) > 0.5;
        let (mut cells, mut rock, mut clustered) = (0, 0, 0);
        for z in -100..100 {
            for x in -100..100 {
                cells += 1;
                if !stone(x, z) {
                    continue;
                }
                rock += 1;
                let neighbours = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                    .into_iter()
                    .filter(|(dx, dz)| stone(x + dx, z + dz))
                    .count();
                if neighbours >= 3 {
                    clustered += 1;
                }
            }
        }
        let share = rock as f32 / cells as f32;
        assert!((0.12..0.30).contains(&share), "stone covers {share}");
        // The generator's 4 x 1 dashes never have three stone neighbours.
        let clustered = clustered as f32 / rock as f32;
        assert!(clustered > 0.85, "only {clustered} of stone is in a patch");
        // The edge of an outcrop is a ramp, not a step.
        assert!((0..4000).any(|step| {
            let weight = outcrop(step as f32 * 0.05, 20.0);
            weight > 0.2 && weight < 0.8
        }));
    }

    #[test]
    fn generated_ridge_scatter_is_redrawn_but_painted_ground_is_kept() {
        let mut terrain = generated_ridge();
        for z in -20..20 {
            for x in -20..20 {
                assert_eq!(classify(&terrain, x, z), Ground::Ridge, "{x},{z}");
            }
        }
        // Stone painted with a brush is more than the generated dashes.
        for z in -3..=3 {
            for x in -3..=3 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Stone
                });
            }
        }
        for z in -3..=3 {
            for x in -3..=3 {
                assert_eq!(classify(&terrain, x, z), Ground::Rock, "{x},{z}");
            }
        }
        assert_eq!(classify(&terrain, 12, 12), Ground::Ridge);
        // Turf painted over the same area is kept as turf.
        for z in -3..=3 {
            for x in -3..=3 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Grass
                });
            }
        }
        for z in -3..=3 {
            for x in -3..=3 {
                assert_eq!(classify(&terrain, x, z), Ground::Turf, "{x},{z}");
            }
        }
        set(&mut terrain, 15, 15, |cell| {
            cell.surface = TerrainSurface::Soil
        });
        set(&mut terrain, 16, 15, |cell| {
            cell.surface = TerrainSurface::Water
        });
        assert_eq!(classify(&terrain, 15, 15), Ground::Path);
        assert_eq!(classify(&terrain, 16, 15), Ground::Water);
        assert_eq!(classify(&terrain, 500, 0), Ground::Water);
        // Other biomes and the high ridge show exactly their surface.
        let mut meadow = field(16, 16, Biome::Meadow);
        set(&mut meadow, 0, 0, |cell| {
            cell.surface = TerrainSurface::Stone
        });
        assert_eq!(classify(&meadow, 0, 0), Ground::Rock);
        assert_eq!(classify(&meadow, 1, 0), Ground::Turf);
    }

    #[test]
    fn neighbouring_cells_blend_and_shared_corners_agree() {
        let mut terrain = field(64, 64, Biome::Meadow);
        for z in -32..32 {
            for x in 0..32 {
                set(&mut terrain, x, z, |cell| cell.biome = Biome::Woodland);
            }
        }
        let west = Patch::around(&terrain, &Roads::default(), -16, 0, 16);
        let east = Patch::around(&terrain, &Roads::default(), 0, 0, 16);
        // The corner column x = 0 belongs to both chunks and must match.
        for z in 0..=16 {
            assert_eq!(
                corner_color(&west, 0, z, Season::Spring),
                corner_color(&east, 0, z, Season::Spring)
            );
        }
        // Across the border the colour moves in small steps, and far from it
        // each side is its own biome.
        let row: Vec<f32> = (-16..=16)
            .map(|x| {
                let patch = if x <= 0 { &west } else { &east };
                luminance(corner_color(patch, x, 8, Season::Spring))
            })
            .collect();
        let largest = row
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs())
            .fold(0.0, f32::max);
        let span = (row[2] - row[30]).abs();
        assert!(span > 0.05, "the two biomes differ by {span}");
        assert!(
            largest < span * 0.6,
            "one step of {largest} in a span of {span}"
        );
    }

    #[test]
    fn turf_varies_gently_inside_one_biome() {
        let terrain = field(96, 96, Biome::Meadow);
        let patch = Patch::around(&terrain, &Roads::default(), -32, -32, 64);
        let values: Vec<f32> = (-32..32)
            .flat_map(|z| (-32..32).map(move |x| (x, z)))
            .map(|(x, z)| luminance(corner_color(&patch, x, z, Season::Summer)))
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let low = values.iter().copied().fold(f32::INFINITY, f32::min);
        let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(high - low > mean * 0.08, "flat: {low}..{high}");
        assert!(high < mean * 1.5 && low > mean * 0.6, "{low}..{high}");
        let step = luminance(corner_color(&patch, 0, 0, Season::Summer))
            - luminance(corner_color(&patch, 1, 0, Season::Summer));
        assert!(step.abs() < mean * 0.06, "cell-to-cell step {step}");
    }

    #[test]
    fn paths_draw_narrower_than_the_walkable_strip_with_soft_edges() {
        // A straight strip four cells wide, like the main road.
        let mut terrain = field(64, 64, Biome::Meadow);
        for x in -32..32 {
            for z in 0..4 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Soil
                });
            }
        }
        let patch = Patch::around(&terrain, &Roads::default(), 0, -8, 16);
        let share = |z: i32| path_weight(surroundings(&patch, 4, z).path);
        // Corners z = 0 and z = 4 are the strip's edges.
        assert!(share(-1) < 0.01 && share(5) < 0.01);
        assert!(share(0) < 0.3 && share(4) < 0.3);
        assert!(share(1) > 0.95 && share(2) > 0.95 && share(3) > 0.95);
        // Drawn width: 2 m of full path plus part of each ramp.
        let width: f32 = (-2..7).map(share).sum();
        assert!((2.5..3.6).contains(&width), "drawn width {width} m");
        // The simulation still has all four rows.
        assert!((0..4).all(|z| terrain.cell(4, z).unwrap().surface == TerrainSurface::Soil));
        // A two-cell lane, the commonest path, stays visible and thin.
        let mut lane = field(64, 64, Biome::Meadow);
        for x in -32..32 {
            for z in 0..2 {
                set(&mut lane, x, z, |cell| cell.surface = TerrainSurface::Soil);
            }
        }
        let patch = Patch::around(&lane, &Roads::default(), 0, -8, 16);
        let share = |z: i32| path_weight(surroundings(&patch, 4, z).path);
        assert!(share(1) > 0.95);
        let width: f32 = (-2..5).map(share).sum();
        assert!((1.2..2.0).contains(&width), "drawn lane {width} m");

        // A stair-stepped strip (one cell up every two along) has no hard
        // step left: every column crosses both edges through partial weights.
        let mut stairs = field(64, 64, Biome::Meadow);
        for x in -32..32_i32 {
            for z in x.div_euclid(2)..x.div_euclid(2) + 4 {
                set(&mut stairs, x, z, |cell| {
                    cell.surface = TerrainSurface::Soil
                });
            }
        }
        let patch = Patch::around(&stairs, &Roads::default(), -8, -8, 16);
        let weight = |x: i32, z: i32| path_weight(surroundings(&patch, x, z).path);
        for x in -6..6 {
            let column: Vec<f32> = (-8..8).map(|z| weight(x, z)).collect();
            assert!(column.iter().any(|w| *w > 0.95), "no path at {x}");
            assert!(
                column.iter().filter(|w| **w > 0.05 && **w < 0.95).count() >= 2,
                "hard edge at {x}: {column:?}"
            );
        }
    }

    /// A two-cell lane along z = 0 and a stair-stepped main road, laid by
    /// the same centre-of-cell test the generator uses.
    fn authored() -> (TerrainState, Roads) {
        let roads = Roads(
            vec![
                Stretch {
                    from: Vec2::new(-30.0, 0.0),
                    to: Vec2::new(30.0, 0.0),
                    radius: 1.0,
                },
                Stretch {
                    from: Vec2::new(-30.0, -24.0),
                    to: Vec2::new(30.0, 6.0),
                    radius: 1.7,
                },
            ],
            Vec::new(),
        );
        let mut terrain = field(64, 64, Biome::Meadow);
        for z in -32..32 {
            for x in -32..32 {
                let centre = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
                if roads
                    .0
                    .iter()
                    .any(|road| road.distance(centre) <= road.radius)
                {
                    set(&mut terrain, x, z, |cell| {
                        cell.surface = TerrainSurface::Soil
                    });
                }
            }
        }
        (terrain, roads)
    }

    #[test]
    fn authored_roads_follow_their_centre_line_without_stair_steps() {
        let (terrain, roads) = authored();
        // The lane's cells are road, not loose soil.
        let patch = Patch::around(&terrain, &roads, -16, -8, 16);
        assert_eq!(patch.ground(-10, 0), Ground::Road);
        assert_eq!(patch.ground(-10, -1), Ground::Road);
        assert_eq!(patch.ground(-10, 3), Ground::Turf);
        let patch = Patch::around(&terrain, &roads, -16, -24, 16);
        // Along the diagonal road the drawn share depends only on the
        // distance from the centre line: no step where the cells step.
        let diagonal = roads.0[1];
        let path = linear([0.47, 0.36, 0.25]);
        let mut by_distance: Vec<(f32, f32)> = Vec::new();
        for z in -22..-10 {
            for x in -14..-2 {
                let corner = Vec2::new(x as f32, z as f32);
                // Stay clear of the lane.
                if roads.0[0].distance(corner) < 4.0 {
                    continue;
                }
                let distance = diagonal.distance(corner);
                let colour = corner_color(&patch, x, z, Season::Spring);
                if distance > 4.0 {
                    assert!(colour[1] > colour[0] * 1.3, "turf away from the road");
                }
                by_distance.push((distance, patch.road_line(x, z).path));
            }
        }
        by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
        // The drawn share follows the distance from the centre line, not the
        // cells' stair steps: full on the line, none beyond the strip, and
        // only the path's own wander and fray in between.
        for (distance, share) in &by_distance {
            if *distance < 0.25 {
                assert!(*share > 0.97, "{share} on the line");
            }
            if *distance > diagonal.radius + 1.3 {
                assert!(*share < 0.01, "{share} at {distance} m");
            }
            let nominal = diagonal.drawn(*distance);
            assert!((share - nominal).abs() < 0.62, "{share} vs {nominal}");
        }
        assert!(by_distance.iter().any(|(_, share)| *share > 0.99));
        assert!(
            by_distance
                .iter()
                .filter(|(_, share)| *share > 0.05 && *share < 0.95)
                .count()
                > 12,
            "a ramp, not an edge"
        );
        // On the centre line the ground is the path colour (but for shade).
        let centre = corner_color(&patch, -10, -14, Season::Spring);
        assert!(diagonal.distance(Vec2::new(-10.0, -14.0)) < 0.01);
        assert!((centre[0] / centre[1] - path[0] / path[1]).abs() < 0.07);
        // Drawn widths at half strength: 3 m for the main road, 1.6 m lanes.
        let half = |road: &Stretch| {
            (0..400)
                .map(|i| i as f32 * 0.01)
                .find(|d| road.drawn(*d) < 0.5)
                .unwrap()
                * 2.0
        };
        assert!((half(&diagonal) - 3.0).abs() < 0.1);
        assert!((half(&roads.0[0]) - 1.6).abs() < 0.1);
        assert!(half(&diagonal) < diagonal.radius * 2.0);
    }

    #[test]
    fn a_road_painted_over_disappears_and_other_soil_still_shows() {
        let (mut terrain, roads) = authored();
        // Turf painted across the lane.
        for z in -3..3 {
            for x in -12..-6 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Grass
                });
            }
        }
        // Soil tilled beside it, away from every road.
        for z in 10..16 {
            for x in -12..-6 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Soil
                });
            }
        }
        let patch = Patch::around(&terrain, &roads, -16, -8, 32);
        let brown = |x: i32, z: i32| {
            let colour = corner_color(&patch, x, z, Season::Spring);
            colour[0] > colour[1]
        };
        assert!(!brown(-9, 0), "the painted stretch is turf");
        assert!(brown(-2, 0), "the lane continues beyond it");
        assert!(brown(-9, 13), "tilled soil is drawn from its cells");
        assert_eq!(patch.ground(-9, 13), Ground::Path);
        assert!(!brown(-9, 20));
    }

    #[test]
    fn roads_match_the_soil_the_generator_laid_in_the_authored_world() {
        let app = island_content::new_sim_app(7, "living_world");
        let spec = app.world().resource::<LivingWorldSpec>();
        let terrain = app.world().resource::<TerrainState>();
        let roads = Roads::from_spec(spec);
        let entrances: usize = spec
            .settlements
            .iter()
            .map(|settlement| settlement.sites.len() + settlement.homes.len())
            .sum();
        // One main road per settlement, a lane per entrance, and spine
        // pieces that several entrances share only once.
        assert_eq!(
            roads.0.iter().filter(|road| road.radius == 1.7).count(),
            spec.settlements.len()
        );
        assert!(roads.0.len() > spec.settlements.len() + entrances);
        assert!(roads.0.len() < spec.settlements.len() + 2 * entrances);
        // Every soil cell of the fresh world is inside a road strip, and
        // every dry cell inside a strip is soil: the lines are the roads.
        // Cells within the generator's rounding of a strip's edge may be either.
        let (mut soil, mut strip) = (0, 0);
        for (coord, cell) in terrain.iter() {
            let centre = Vec2::new(coord.x as f32 + 0.5, coord.z as f32 + 0.5);
            let within = |slack: f32| {
                roads
                    .0
                    .iter()
                    .any(|road| road.distance(centre) <= road.radius + slack)
            };
            if cell.surface == TerrainSurface::Soil {
                soil += 1;
                assert!(within(STRIP_SLACK), "soil outside every road at {coord:?}");
            }
            if within(-STRIP_SLACK) && cell.surface != TerrainSurface::Water {
                strip += 1;
                assert_eq!(cell.surface, TerrainSurface::Soil, "{coord:?}");
            }
        }
        assert!(soil > 1000 && strip <= soil && strip * 100 >= soil * 97);
        // Only the stretches near a chunk are kept for it.
        let near = roads.near(Vec2::new(100.0, 100.0), Vec2::new(116.0, 116.0));
        assert!(near.len() < roads.0.len() / 4);
    }

    #[test]
    fn land_turns_to_sand_at_the_water_and_shallows_fade_out() {
        let mut terrain = field(64, 64, Biome::Meadow);
        for z in -32..32 {
            for x in 0..32 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Water;
                    cell.height_mm = -700;
                });
            }
        }
        let patch = Patch::around(&terrain, &Roads::default(), -16, -8, 16);
        let inland = corner_color(&patch, -8, 0, Season::Summer);
        let shore = corner_color(&patch, 0, 0, Season::Summer);
        assert!(shore[0] > inland[0] * 1.5, "sand is paler and warmer");
        assert!(shore[0] > shore[2]);
        assert_eq!(corner_color(&patch, -3, 0, Season::Summer)[3], 1.0);
        // Shallows: strongest at the shoreline, gone three metres out.
        let band: Vec<f32> = (0..5).map(|x| shallows(&terrain, x, 0)).collect();
        assert!(band[0] > 0.95, "{band:?}");
        assert!(band.windows(2).all(|pair| pair[1] <= pair[0]));
        assert!(band[1] > 0.4 && band[2] > 0.05);
        assert_eq!(band[3], 0.0);
        assert_eq!(shallows(&terrain, 20, 0), 0.0);
    }

    #[test]
    fn roads_wander_vary_fray_and_round_their_junctions_inside_the_strip() {
        let lane = Stretch {
            from: Vec2::new(-200.0, 0.0),
            to: Vec2::new(200.0, 0.0),
            radius: 1.0,
        };
        // Half-strength edges of the drawn path along 300 m of lane.
        let mut centres = Vec::new();
        let mut widths = Vec::new();
        for step in 0..600 {
            let x = -150.0 + step as f32 * 0.5;
            let drawn: Vec<f32> = (-60..=60)
                .map(|i| i as f32 * 0.05)
                .filter(|z| road_look(&[lane], Vec2::new(x, *z)).path >= 0.5)
                .collect();
            assert!(!drawn.is_empty(), "the path never breaks at x = {x}");
            let (low, high) = (drawn[0], drawn[drawn.len() - 1]);
            // Centred on the walkable strip, which is a metre to each side;
            // only the soft edge strays past it, as it always did.
            assert!(low > -1.3 && high < 1.3, "{low}..{high} at x = {x}");
            assert!(
                low < 0.0 && high > 0.0,
                "the centre line is path at x = {x}"
            );
            centres.push((low + high) / 2.0);
            widths.push(high - low + 0.05);
        }
        let span = |values: &[f32]| {
            values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - values.iter().copied().fold(f32::INFINITY, f32::min)
        };
        let mean = widths.iter().sum::<f32>() / widths.len() as f32;
        assert!((1.2..1.9).contains(&mean), "mean drawn width {mean} m");
        assert!(span(&widths) > 0.3, "the width varies: {}", span(&widths));
        assert!(
            span(&centres) > 0.25,
            "the path wanders: {}",
            span(&centres)
        );
        // Never a jump: the centre drifts, it does not step.
        assert!(
            centres
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.3)
        );
        // A road laid as several stretches along one line is no wider.
        let twice = [lane, lane, lane];
        for step in 0..40 {
            let point = Vec2::new(step as f32 * 3.0, 0.9 + step as f32 * 0.01);
            assert_eq!(road_look(&twice, point), road_look(&[lane], point));
        }
        // The verge is beside the path, not on it and not far from it.
        let on = road_look(&[lane], Vec2::new(3.0, 0.0));
        assert!(on.path > 0.85 && on.verge < 0.15, "{on:?}");
        let beside: f32 = (0..200)
            .map(|step| road_look(&[lane], Vec2::new(step as f32, 1.35)).verge)
            .sum::<f32>()
            / 200.0;
        assert!(beside > 0.2, "worn verge beside the path: {beside}");
        assert_eq!(road_look(&[lane], Vec2::new(3.0, 4.0)), RoadLook::default());
        assert_eq!(road_look(&[], Vec2::ZERO), RoadLook::default());

        // A lane leaving a road at a right angle: the inside of the corner is
        // filled in more than either stretch alone would draw it.
        let branch = Stretch {
            from: Vec2::new(0.0, 0.0),
            to: Vec2::new(0.0, 200.0),
            radius: 1.0,
        };
        let (mut joined, mut apart) = (0.0, 0.0);
        for step in 0..40 {
            // Along the diagonal of the inside corner, 1.0 to 1.8 m out.
            let reach = 1.0 + step as f32 * 0.02;
            let point = Vec2::new(reach, reach);
            joined += road_look(&[lane, branch], point).path;
            apart += road_look(&[lane], point)
                .path
                .max(road_look(&[branch], point).path);
        }
        assert!(joined > apart + 1.0, "fillet: {joined} vs {apart}");
    }

    #[test]
    fn the_beach_slopes_from_the_land_edge_and_shares_its_corners() {
        let mut terrain = field(32, 32, Biome::Coast);
        for z in -16..16 {
            for x in 0..16 {
                set(&mut terrain, x, z, |cell| {
                    cell.surface = TerrainSurface::Water;
                    cell.height_mm = -700;
                });
            }
        }
        // Corners on the shoreline take the land's height; corners out in
        // the water have no beach.
        assert_eq!(beach_corner(&terrain, 0, 4), Some(1.0));
        assert_eq!(beach_corner(&terrain, 1, 4), None);
        assert_eq!(beach_corner(&terrain, -1, 4), Some(1.0));
        // A cliff does not become a metres-long ramp.
        for z in -16..16 {
            set(&mut terrain, -1, z, |cell| cell.height_mm = 6000);
        }
        assert_eq!(beach_corner(&terrain, 0, 4), Some(BEACH_TOP));
        // Mixed heights meet at their mean, the same from either cell.
        set(&mut terrain, -1, 3, |cell| cell.height_mm = 500);
        assert_eq!(beach_corner(&terrain, 0, 4), Some((0.5 + BEACH_TOP) / 2.0));
        // The foot is under the water surface.
        const { assert!(BEACH_FOOT < -0.11) };
        // A low shore is sand; a high bank keeps its own ground.
        assert_eq!(sandiness(0.3), 1.0);
        assert_eq!(sandiness(BEACH_TOP), 0.0);
        assert!(sandiness(1.4) > 0.2 && sandiness(1.4) < 0.8);
        let (wet, dry) = (wet_sand(Season::Summer), dry_sand(Season::Summer));
        assert!(wet[0] < dry[0] && wet[1] < dry[1]);
    }

    #[test]
    fn the_ground_grain_tiles_keeps_its_mean_and_fades_out_with_distance() {
        let size = 64;
        let grain = detail_grain(size);
        assert_eq!(grain.len(), size * size);
        let mean = grain.iter().sum::<f32>() / grain.len() as f32;
        assert!((mean - DETAIL_MEAN).abs() < 0.004, "mean {mean}");
        let low = grain.iter().copied().fold(f32::INFINITY, f32::min);
        let high = grain.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(low >= DETAIL_FLOOR - 0.07 && high <= 1.0);
        assert!(high - low > 0.18, "visible contrast: {low}..{high}");
        // Seamless: the step across the wrap is no larger than inside.
        let step = |a: usize, b: usize| (grain[a] - grain[b]).abs();
        let mut inside = 0.0_f32;
        let mut seam = 0.0_f32;
        for row in 0..size {
            for column in 0..size - 1 {
                inside = inside.max(step(row * size + column, row * size + column + 1));
            }
            seam = seam.max(step(row * size + size - 1, row * size));
        }
        assert!(seam <= inside * 1.05, "seam {seam} vs {inside}");
        // Each mip keeps the mean and loses contrast: plain from afar.
        let mut level = grain;
        let mut side = size;
        let mut contrast = high - low;
        while side > 4 {
            level = halved(&level, side);
            side /= 2;
            let mean_here = level.iter().sum::<f32>() / level.len() as f32;
            assert!((mean_here - mean).abs() < 1e-4);
            let spread = level.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - level.iter().copied().fold(f32::INFINITY, f32::min);
            assert!(spread <= contrast + 1e-6);
            contrast = spread;
        }
        assert!(contrast < 0.06, "a 4 m tile is nearly flat at 1 m texels");
        // The image carries the whole chain down to one texel.
        let image = detail_image();
        assert_eq!(image.texture_descriptor.mip_level_count, 9);
        let texels: usize = (0..9).map(|mip| (DETAIL_SIZE >> mip).pow(2)).sum();
        assert_eq!(image.data.as_ref().unwrap().len(), texels * 4);
        // The tile is turned against the grid and repeats every 4 m.
        let [u, v] = detail_uv(4.0, 0.0);
        assert!((u * u + v * v - 1.0).abs() < 1e-4 && v.abs() > 0.2);
    }

    #[test]
    fn autumn_is_muted_olive_and_ochre_rather_than_yellow() {
        let srgb = |biome| {
            let [r, g, b] = turf(biome, Season::Autumn);
            Color::linear_rgb(r, g, b).to_srgba()
        };
        for biome in [
            Biome::Coast,
            Biome::Meadow,
            Biome::Woodland,
            Biome::Ridge,
            Biome::Wetland,
        ] {
            let autumn = srgb(biome);
            let [r, g, b] = turf_srgb(biome);
            // Still green-led, clearly so, and less saturated than spring.
            assert!(autumn.green > autumn.red + 0.03, "{biome:?}: {autumn:?}");
            let saturation = |r: f32, g: f32, b: f32| {
                let (high, low) = (r.max(g).max(b), r.min(g).min(b));
                (high - low) / high
            };
            assert!(
                saturation(autumn.red, autumn.green, autumn.blue) < saturation(r, g, b),
                "{biome:?}"
            );
            assert!(autumn.green < g, "{biome:?} darkens a little");
        }
        // The coast, which read as yellow: red no longer reaches the green.
        let coast = srgb(Biome::Coast);
        assert!(coast.red < 0.58 && coast.green > 0.59, "{coast:?}");
        // Dry patches are fewer and ochre.
        let (autumn, summer) = (palette(Season::Autumn), palette(Season::Summer));
        assert!(autumn.dry_strength < summer.dry_strength);
        assert!(autumn.dry[0] < summer.dry[0]);
    }

    #[test]
    fn seasons_keep_every_surface_distinct() {
        for season in [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ] {
            let palette = palette(season);
            let grass = turf(Biome::Meadow, season);
            let lum = |c: [f32; 3]| luminance([c[0], c[1], c[2], 1.0]);
            assert!(
                (lum(palette.path) - lum(grass)).abs() > 0.03
                    || (palette.path[0] - grass[0]) > 0.05
            );
            assert!(palette.rock.iter().all(|c| (0.0..=1.0).contains(c)));
        }
        assert_ne!(
            turf(Biome::Ridge, Season::Autumn),
            turf(Biome::Ridge, Season::Spring)
        );
        assert!(turf(Biome::Meadow, Season::Winter)[2] > 0.7);
    }
}
