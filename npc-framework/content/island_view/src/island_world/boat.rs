//! The merchant boat, from `view::boat`.
//!
//! The simulation says whether the boat is in, when it leaves and when it
//! next lands, and where it ties up (`BoatView::berth`, beside the trading
//! post at the waterfront). The view moors it there, broadside to the shore,
//! or straight out from there the least way that floats the whole hull clear
//! of the beach and the land, sails it in over the last ticks before a
//! landing and out again after it leaves, and hides it while it is away.
//!
//! The mooring and the sailing know the hull only by [`HULL`]'s length and
//! beam: the boat is one root (bow along +Z, waterline at its origin) with a
//! [`BoatHull`] child and a [`BoatSail`] child, so a ship model drops in as
//! those children. The swell, the rocking and the sail's flutter move the
//! root and the sail child, whatever meshes or clips are under them.
use bevy::prelude::*;
use island_content::village::frontier::terrain::{TerrainState, TerrainSurface};
use island_content::village::view;
use npc_framework::prelude::{SimClock, SimPos};

/// Ticks of sailing in before a landing, and out after leaving.
pub(crate) const SAIL_TICKS: u64 = 500;
/// How far out the boat comes into view, metres.
const SAIL_DISTANCE_M: f32 = 46.0;
/// Seconds between two reads of the boat's state.
const READ_SECS: f32 = 0.25;
/// How far round the berth the shore is weighed to find open water, cells.
const SEAWARD_REACH: i32 = 5;
/// How far out from the berth the boat may be moved to float clear, metres.
const CLEAR_REACH_M: f32 = 34.0;
/// Open water the pier crosses between the strand and the hull, metres:
/// the ship lies far enough out that its jetty is a real pier.
pub(crate) const PIER_WATER_M: f32 = 10.0;

/// The hull's size, metres: what the mooring keeps clear of land and the
/// jetty stands off. A ship model is fitted to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HullSize {
    pub(crate) length: f32,
    pub(crate) beam: f32,
}

pub(crate) const HULL: HullSize = HullSize {
    length: 11.9,
    beam: 6.6,
};

/// The merchant ship: Kenney's one-masted `ship-small`, 8.8 units long with
/// its bow along +Z and its keel at 0, drawn 11.9 m long.
const SHIP_MODEL: &str = "living/ships/ship-small.glb";
const SHIP_SCALE: f32 = 1.35;
/// How deep the keel sits under the waterline, metres.
const SHIP_DRAFT_M: f32 = 1.15;
/// The sail node's height, model units: it hangs from the yard at its top.
const SAIL_HEIGHT_UNITS: f32 = 4.725;
/// How much of the sail shows furled under the yard.
const FURLED: f32 = 0.14;

/// The boat's hull (and what rides on its deck): a child of the root.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BoatHull;

/// The ship model's sail node, with its own place in the model.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BoatSail {
    base: Transform,
}

/// A pennant on the ship's mast, with its own place in the model: it
/// streams downwind from its pole.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BoatFlag {
    base: Transform,
}

/// The sail's transform at `set` (1 set, [`FURLED`] furled up to the yard)
/// from its place in the model: the top stays at the yard.
pub(crate) fn sail_set(base: Transform, set: f32) -> Transform {
    let set = set.clamp(FURLED, 1.0);
    let mut transform = base;
    transform.scale.y = base.scale.y * set;
    transform.translation.y = base.translation.y + SAIL_HEIGHT_UNITS * base.scale.y * (1.0 - set);
    transform
}

/// A pennant's turn about its pole (local Y): streaming downwind
/// (`downwind` in the ship's own frame, x and z) and flapping.
pub(crate) fn pennant(downwind: Vec2, seconds: f32, strength: f32, seed: f32) -> Quat {
    // The pennant runs along its pole's -Z.
    let stream = (-downwind.x).atan2(-downwind.y);
    let flap = ((seconds * 6.3 + seed).sin() * 0.6 + (seconds * 9.7 + seed * 2.0).sin() * 0.4)
        * 0.12
        * strength.clamp(0.3, 3.0);
    Quat::from_rotation_y(stream + flap)
}

/// Observer on the ship model: find its sail and pennants.
pub(crate) fn on_ship_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    children: Query<&Children>,
    names: Query<(&Name, &Transform)>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ready.entity) {
        let Ok((name, transform)) = names.get(e) else {
            continue;
        };
        match name.as_str() {
            "sail-a" => {
                commands.entity(e).insert(BoatSail { base: *transform });
            }
            "flag-a" | "flag-b" => {
                commands.entity(e).insert(BoatFlag { base: *transform });
            }
            _ => {}
        }
    }
}

/// Whether the cell at (x, z) is open water deep enough for the keel:
/// water, and no land within two cells (a beach reaches a cell out into
/// the sea and shoals the next).
pub(crate) fn open_water(water: &impl Fn(i32, i32) -> bool, x: i32, z: i32) -> bool {
    (-2..=2).all(|dz| (-2..=2).all(|dx| water(x + dx, z + dz)))
}

/// Whether a hull of `size` lying broadside at `at` (its beam along `out`)
/// floats wholly on open water, a metre to spare at bow and stern and half
/// a metre abeam.
pub(crate) fn hull_floats(
    at: Vec2,
    out: Vec2,
    size: HullSize,
    water: &impl Fn(i32, i32) -> bool,
) -> bool {
    let along = Vec2::new(-out.y, out.x);
    let (half_l, half_b) = (size.length * 0.5 + 1.0, size.beam * 0.5 + 0.5);
    let steps = |half: f32| (half / 0.5).ceil() as i32;
    (-steps(half_l)..=steps(half_l)).all(|i| {
        (-steps(half_b)..=steps(half_b)).all(|j| {
            let p = at
                + along * (i as f32 / steps(half_l) as f32 * half_l)
                + out * (j as f32 / steps(half_b) as f32 * half_b);
            open_water(water, p.x.floor() as i32, p.y.floor() as i32)
        })
    })
}

/// Whether a hull at `at` lies a pier's length out: no land within
/// [`PIER_WATER_M`] of the pier's head beside its landward side, whichever
/// way the shore runs (the pier's foot is the nearest dry cell).
pub(crate) fn pier_room(
    at: Vec2,
    out: Vec2,
    size: HullSize,
    water: &impl Fn(i32, i32) -> bool,
) -> bool {
    let head = at - out * (size.beam * 0.5 + 0.3);
    let reach = PIER_WATER_M.ceil() as i32 + 1;
    let (cx, cz) = (head.x.floor() as i32, head.y.floor() as i32);
    (-reach..=reach).all(|dz| {
        (-reach..=reach).all(|dx| {
            let (x, z) = (cx + dx, cz + dz);
            let centre = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            water(x, z) || centre.distance(head) - 0.5 >= PIER_WATER_M
        })
    })
}

/// Where the hull floats: the berth itself when it floats there with a
/// pier's length of water inshore of it, else the least way straight out
/// from it that does (within [`CLEAR_REACH_M`]); the berth when nothing
/// does.
pub(crate) fn afloat(
    berth: Vec2,
    out: Vec2,
    size: HullSize,
    water: &impl Fn(i32, i32) -> bool,
) -> Vec2 {
    let steps = (CLEAR_REACH_M / 0.25) as i32;
    (0..=steps)
        .map(|i| berth + out * (i as f32 * 0.25))
        .find(|at| hull_floats(*at, out, size, water) && pier_room(*at, out, size, water))
        .unwrap_or(berth)
}

/// Where the boat is on its round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    /// Out of sight.
    Away,
    /// Sailing in: 0 far out, 1 at the mooring.
    Approaching(f32),
    Moored,
    /// Sailing out: 0 at the mooring, 1 out of sight.
    Leaving(f32),
}

/// The boat's phase at `now`. `left_at` is the tick the view saw it cast
/// off. A boat that is due but has not landed (a storm keeps it out) stands
/// off, most of the way in.
pub(crate) fn phase(docked: bool, now: u64, next_visit_tick: u64, left_at: Option<u64>) -> Phase {
    if docked {
        return Phase::Moored;
    }
    if let Some(left) = left_at
        && now >= left
        && now - left < SAIL_TICKS
    {
        return Phase::Leaving((now - left) as f32 / SAIL_TICKS as f32);
    }
    if next_visit_tick > now && next_visit_tick - now <= SAIL_TICKS {
        return Phase::Approaching(1.0 - (next_visit_tick - now) as f32 / SAIL_TICKS as f32);
    }
    if next_visit_tick <= now && now - next_visit_tick < SAIL_TICKS * 4 {
        return Phase::Approaching(0.8);
    }
    Phase::Away
}

/// Where the boat lies and which way open water is: the simulation's
/// `berth` (metres) and the unit direction from the shore round it out to
/// the sea (water cells pull, land cells push, within
/// [`SEAWARD_REACH`]); from the dock to the berth when the shore is even
/// all round. `water` says whether a cell is water.
pub(crate) fn mooring(
    berth: SimPos,
    dock: Option<SimPos>,
    water: impl Fn(i32, i32) -> bool,
) -> (Vec2, Vec2) {
    let (at, out) = seaward(berth, dock, &water);
    (afloat(at, out, HULL, &water), out)
}

/// The berth in metres and which way open water lies from it.
pub(crate) fn seaward(
    berth: SimPos,
    dock: Option<SimPos>,
    water: &impl Fn(i32, i32) -> bool,
) -> (Vec2, Vec2) {
    let at = Vec2::new(berth.x as f32 / 1000.0, berth.z as f32 / 1000.0);
    let cell = (berth.x.div_euclid(1000), berth.z.div_euclid(1000));
    let mut pull = Vec2::ZERO;
    for dz in -SEAWARD_REACH..=SEAWARD_REACH {
        for dx in -SEAWARD_REACH..=SEAWARD_REACH {
            if dx == 0 && dz == 0 {
                continue;
            }
            let towards = Vec2::new(dx as f32, dz as f32).normalize();
            pull += if water(cell.0 + dx, cell.1 + dz) {
                towards
            } else {
                -towards
            };
        }
    }
    let from_dock = dock
        .map(|dock| at - Vec2::new(dock.x as f32 / 1000.0, dock.z as f32 / 1000.0))
        .unwrap_or(Vec2::NEG_Y);
    let out = if pull.length() > 0.5 {
        pull.normalize()
    } else {
        from_dock.normalize_or(Vec2::NEG_Y)
    };
    (at, out)
}

/// The hull's place and heading for a phase: position on the water plane
/// (x, z) and the turn that points its bow. Moored, it lies broadside to the
/// shore; under way, bow first.
pub(crate) fn pose(phase: Phase, mooring: Vec2, out: Vec2) -> Option<(Vec2, f32)> {
    let ease = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let heading = |dir: Vec2| dir.x.atan2(dir.y);
    let broadside = heading(Vec2::new(-out.y, out.x));
    match phase {
        Phase::Away => None,
        Phase::Moored => Some((mooring, broadside)),
        Phase::Approaching(t) => {
            let near = ease(t);
            // Turning broadside over the last stretch.
            let turn = ((near - 0.8) / 0.2).clamp(0.0, 1.0);
            let bow = heading(-out);
            Some((
                mooring + out * SAIL_DISTANCE_M * (1.0 - near),
                bow + shortest(bow, broadside) * turn,
            ))
        }
        Phase::Leaving(t) => {
            let far = ease(t);
            let turn = (far / 0.2).clamp(0.0, 1.0);
            let bow = heading(out);
            Some((
                mooring + out * SAIL_DISTANCE_M * far,
                broadside + shortest(broadside, bow) * turn,
            ))
        }
    }
}

/// The signed shortest turn from one heading to another.
fn shortest(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

#[derive(Resource, Default)]
pub(crate) struct BoatScene {
    since: f32,
    root: Option<Entity>,
    docked: bool,
    left_at: Option<u64>,
    next_visit_tick: u64,
    /// Where it moors and which way is out; `None` without a dock on water.
    berth: Option<(Vec2, Vec2)>,
    dock: Option<SimPos>,
}

impl BoatScene {
    /// Where the boat moors and which way is out, once a dock on water is
    /// known.
    pub(crate) fn berth(&self) -> Option<(Vec2, Vec2)> {
        self.berth
    }

    /// The ship's root entity, once spawned.
    pub(crate) fn root(&self) -> Option<Entity> {
        self.root
    }
}

fn spawn(world: &mut World) -> Option<Entity> {
    let model = world
        .get_resource::<AssetServer>()
        .map(|server| server.load(GltfAssetLabel::Scene(0).from_asset(SHIP_MODEL)));
    let root = world
        .spawn((
            Name::new("merchant boat"),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    // The bow is the root's +Z, the waterline its origin; the ship model
    // hangs under it with its keel below the water.
    let mut hull = world.spawn((
        Name::new("boat hull"),
        BoatHull,
        ChildOf(root),
        Transform::from_xyz(0.0, -SHIP_DRAFT_M, 0.0).with_scale(Vec3::splat(SHIP_SCALE)),
        Visibility::Inherited,
    ));
    if let Some(model) = model {
        hull.insert(bevy::world_serialization::WorldAssetRoot(model))
            .observe(on_ship_ready)
            .observe(super::tint::on_wood_ready);
    }
    Some(root)
}

/// Read the boat's state a few times a second, find its mooring when the
/// dock changes, and remember when it cast off.
pub(crate) fn sync(world: &mut World) {
    let delta = world
        .get_resource::<Time>()
        .map_or(0.0, |time| time.delta_secs());
    {
        let mut scene = world.resource_mut::<BoatScene>();
        scene.since += delta;
        if scene.since < READ_SECS {
            return;
        }
        scene.since = 0.0;
    }
    let boat = view::boat(world);
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let dock = boat.dock.filter(|_| boat.enabled);
    // The simulation's berth, the boat moored on it; only which way the sea
    // lies is the view's.
    let berth = boat.berth.filter(|_| boat.enabled).map(|berth| {
        let terrain = world.get_resource::<TerrainState>();
        mooring(berth, dock, |x, z| {
            // Beyond the map's edge is the sea.
            terrain.is_none_or(|terrain| {
                terrain
                    .cell(x, z)
                    .is_none_or(|cell| cell.surface == TerrainSurface::Water)
            })
        })
    });
    if let (Some((at, _)), Some(dock)) = (berth, dock)
        && world.resource::<BoatScene>().berth != berth
    {
        // For captures and reviews: where the boat lies against the post.
        info!(
            "merchant boat berth: {:.1}, {:.1}; {:.0} m from the trading post's entrance",
            at.x,
            at.y,
            at.distance(Vec2::new(dock.x as f32 / 1000.0, dock.z as f32 / 1000.0))
        );
    }
    let needs_hull = berth.is_some() && world.resource::<BoatScene>().root.is_none();
    let root = if needs_hull { spawn(world) } else { None };
    let mut scene = world.resource_mut::<BoatScene>();
    if scene.docked && !boat.docked {
        scene.left_at = Some(now);
    }
    // A clock that went back (a loaded save) forgets the departure.
    if scene.left_at.is_some_and(|left| left > now) {
        scene.left_at = None;
    }
    scene.docked = boat.docked;
    scene.next_visit_tick = boat.next_visit_tick;
    scene.dock = dock;
    scene.berth = berth;
    if root.is_some() {
        scene.root = root;
    }
}

/// The played actor's fog record, when the fog is on: the boat is a thing
/// that moves, drawn only where the actor sees now.
type FogRecord<'a> = (
    &'a npc_framework::prelude::SimId,
    &'a npc_framework::perception::ExploredCells,
    &'a npc_framework::perception::VisibleCells,
    &'a npc_framework::perception::SeenStatics,
);

/// Whether the water from `lo` to `hi` (world metres) is in sight: always
/// without fog, else only cells the played actor sees now.
fn in_sight(
    fog: Option<&crate::fog::FogOfWar>,
    grid: Option<&npc_framework::prelude::SpatialGrid>,
    records: &Query<FogRecord, With<npc_framework::perception::RevealsFog>>,
    lo: Vec2,
    hi: Vec2,
) -> bool {
    if !fog.is_some_and(|fog| fog.enabled) {
        return true;
    }
    let Some(grid) = grid else { return true };
    let Some((id, explored, visible, seen)) = records.iter().min_by_key(|record| *record.0) else {
        return true;
    };
    let lens = crate::fog::FogLens {
        grid,
        explored,
        visible,
        seen,
        memory: None,
        actor: *id,
        riding: None,
    };
    let mm = |v: f32| (v * 1000.0).round() as i32;
    lens.area(
        SimPos::new(mm(lo.x), mm(lo.y)),
        SimPos::new(mm(hi.x), mm(hi.y)),
    ) == crate::fog::CellFog::Visible
}

/// Put the hull where its phase has it, riding the water a little, and show
/// the goods on the quay while it is in. Both only where the played actor
/// sees, under the fog.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sail(
    scene: Res<BoatScene>,
    harbour: Option<Res<super::harbour::Harbour>>,
    clock: Option<Res<SimClock>>,
    (time, wind): (Res<Time>, Option<Res<super::ambient::Wind>>),
    fog: Option<Res<crate::fog::FogOfWar>>,
    grid: Option<Res<npc_framework::prelude::SpatialGrid>>,
    records: Query<FogRecord, With<npc_framework::perception::RevealsFog>>,
    mut hulls: Query<(&mut Transform, &mut Visibility), (Without<BoatSail>, Without<BoatFlag>)>,
    (mut sails, mut flags, mut set): (
        Query<(&BoatSail, &mut Transform), Without<BoatFlag>>,
        Query<(&BoatFlag, &mut Transform), Without<BoatSail>>,
        Local<Option<f32>>,
    ),
) {
    let (Some(root), Some((berth, out))) = (scene.root, scene.berth) else {
        return;
    };
    let now = clock.map_or(0, |clock| clock.tick);
    let at_phase = phase(scene.docked, now, scene.next_visit_tick, scene.left_at);
    let placed = pose(at_phase, berth, out);
    let seen = |centre: Vec2, half: f32| {
        in_sight(
            fog.as_deref(),
            grid.as_deref(),
            &records,
            centre - Vec2::splat(half),
            centre + Vec2::splat(half),
        )
    };
    let hull_seen = placed.is_some_and(|(at, _)| seen(at, HULL.length * 0.5));
    if let Some(cargo) = harbour.as_deref().and_then(|harbour| {
        harbour
            .cargo
            .zip(harbour.landing.map(|landing| landing.foot))
    }) && let Ok((_, mut visibility)) = hulls.get_mut(cargo.0)
    {
        let want = if at_phase == Phase::Moored && seen(cargo.1, 1.5) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
    }
    let Ok((mut transform, mut visibility)) = hulls.get_mut(root) else {
        return;
    };
    let want = if hull_seen {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != want {
        *visibility = want;
    }
    let Some((at, heading)) = placed else { return };
    // The swell on the view clock: it rides on while the game is paused.
    let (seconds, strength, wind_yaw, downwind) = wind
        .as_deref()
        .map_or((time.elapsed_secs(), 1.0, 0.0, Vec2::X), |wind| {
            (wind.time, wind.strength, wind.yaw(), wind.direction)
        });
    let under_way = matches!(at_phase, Phase::Approaching(_) | Phase::Leaving(_));
    let ride = ride(seconds, strength, under_way);
    transform.translation = Vec3::new(at.x, -0.12 + ride.heave, at.y);
    transform.rotation = Quat::from_rotation_y(heading)
        * Quat::from_rotation_z(ride.roll)
        * Quat::from_rotation_x(ride.pitch);
    // The sail comes down to the yard at the berth and is set to sail in
    // and out; it eases between over a few seconds of sim time.
    let target = if at_phase == Phase::Moored {
        FURLED
    } else {
        1.0
    };
    let step = (time.delta_secs() * 0.4).min(1.0);
    let current = set.map_or(target, |set| set + (target - set) * step);
    *set = Some(current);
    for (sail, mut transform) in &mut sails {
        let mut next = sail_set(sail.base, current);
        next.rotation =
            sail.base.rotation * flutter(seconds, strength * current, wind_yaw - heading);
        if *transform != next {
            *transform = next;
        }
    }
    // The pennants stream downwind in the ship's own frame.
    let local = Quat::from_rotation_y(-heading) * Vec3::new(downwind.x, 0.0, downwind.y);
    for (i, (flag, mut transform)) in flags.iter_mut().enumerate() {
        transform.rotation = flag.base.rotation
            * pennant(
                Vec2::new(local.x, local.z),
                seconds,
                strength,
                i as f32 * 1.7,
            );
    }
}

/// How the hull rides the swell: heave (m), roll and pitch (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ride {
    pub(crate) heave: f32,
    pub(crate) roll: f32,
    pub(crate) pitch: f32,
}

/// The ride at view time `seconds` in a wind of `strength`: a slow bob and
/// rock at the berth, more in a blow, and a bow that lifts and dips under
/// way.
pub(crate) fn ride(seconds: f32, strength: f32, under_way: bool) -> Ride {
    let sea = 0.7 + 0.3 * strength.clamp(0.0, 3.0);
    let pitch_more = if under_way { 2.2 } else { 1.0 };
    Ride {
        heave: ((seconds * 0.9).sin() * 0.05 + (seconds * 1.7 + 0.4).sin() * 0.015) * sea,
        roll: ((seconds * 0.7).sin() * 0.03 + (seconds * 1.31).sin() * 0.008) * sea,
        pitch: (seconds * 0.53).cos() * 0.016 * sea * pitch_more,
    }
}

/// The sail's turn on its mast: swung a little to the wind's side
/// (`across` is the wind's yaw against the bow's) and shivering in it.
pub(crate) fn flutter(seconds: f32, strength: f32, across: f32) -> Quat {
    let set = across.sin() * 0.18;
    let shiver = ((seconds * 3.1).sin() * 0.6 + (seconds * 5.3 + 1.1).sin() * 0.4)
        * 0.035
        * strength.clamp(0.2, 3.0);
    Quat::from_rotation_y(set + shiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boat_sails_in_moors_and_sails_out_and_is_away_between() {
        let next = 10_000;
        assert_eq!(phase(false, 2_000, next, None), Phase::Away);
        // In over the last ticks before the landing.
        assert_eq!(
            phase(false, next - SAIL_TICKS, next, None),
            Phase::Approaching(0.0)
        );
        let Phase::Approaching(half) = phase(false, next - SAIL_TICKS / 2, next, None) else {
            panic!("on its way in")
        };
        assert!((half - 0.5).abs() < 1e-6);
        assert_eq!(phase(true, next, next + 24_000, None), Phase::Moored);
        // Out again from the tick it cast off, then away until the next visit.
        let left = next + 3_000;
        assert_eq!(
            phase(false, left, next + 24_000, Some(left)),
            Phase::Leaving(0.0)
        );
        let Phase::Leaving(out) = phase(false, left + SAIL_TICKS / 4, next + 24_000, Some(left))
        else {
            panic!("on its way out")
        };
        assert!((out - 0.25).abs() < 1e-6);
        assert_eq!(
            phase(false, left + SAIL_TICKS, next + 24_000, Some(left)),
            Phase::Away
        );
        // Due but kept out by a storm: it stands off.
        assert_eq!(phase(false, next + 50, next, None), Phase::Approaching(0.8));
        assert_eq!(phase(false, next + SAIL_TICKS * 4, next, None), Phase::Away);
    }

    #[test]
    fn the_boat_moors_on_the_simulations_berth_with_the_sea_beyond() {
        // Land for z >= 0, water beyond: the shore runs along x.
        let water = |_x: i32, z: i32| z < 0;
        let berth = SimPos::new(5_500, -1_500);
        let (at, out) = mooring(berth, Some(SimPos::new(5_500, 1_500)), water);
        assert!((out - Vec2::NEG_Y).length() < 1e-5, "{out}");
        // 1.5 m off the shore the hull would lie on the beach: it is moved
        // straight out, the least way that floats it clear.
        assert_eq!(at.x, 5.5);
        assert!(at.y < -1.5 && at.y > -20.0, "{at}");
        assert!(hull_floats(at, out, HULL, &water));
        // A pier's length of water inshore of it, and no less.
        assert!(pier_room(at, out, HULL, &water));
        assert!(
            !(hull_floats(at - out * 0.25, out, HULL, &water)
                && pier_room(at - out * 0.25, out, HULL, &water))
        );
        // A berth out in open water is kept exactly.
        let (at, _) = mooring(SimPos::new(5_500, -16_500), None, water);
        assert_eq!(at, Vec2::new(5.5, -16.5));
        // A post beside the berth along the shore does not turn the hull:
        // the sea decides.
        let (_, out) = mooring(berth, Some(SimPos::new(8_500, -1_500)), water);
        assert!((out - Vec2::NEG_Y).length() < 1e-5, "{out}");
        // Water all round: from the dock out.
        let (_, out) = mooring(berth, Some(SimPos::new(5_500, 1_500)), |_, _| true);
        assert!((out - Vec2::NEG_Y).length() < 1e-5, "{out}");
    }

    #[test]
    fn the_boat_rides_the_swell_on_the_view_clock_and_its_sail_shivers() {
        // Two view times, two rides: it moves while the sim is paused,
        // since only the view clock drives it.
        let (a, b) = (ride(1.0, 1.0, false), ride(2.0, 1.0, false));
        assert_ne!(a, b);
        // Small at the berth, more in a storm and under way.
        let calm = (0..200)
            .map(|i| ride(i as f32 * 0.1, 0.7, false).roll.abs())
            .fold(0.0, f32::max);
        let storm = (0..200)
            .map(|i| ride(i as f32 * 0.1, 2.6, false).roll.abs())
            .fold(0.0, f32::max);
        assert!(calm < 0.04 && storm > calm, "{calm} {storm}");
        let pitch = |way| {
            (0..200)
                .map(|i| ride(i as f32 * 0.1, 1.0, way).pitch.abs())
                .fold(0.0, f32::max)
        };
        assert!(pitch(true) > pitch(false));
        // The sail turns on its mast only: no lift, no shift.
        let turn = flutter(1.3, 1.0, 0.5);
        assert!((turn * Vec3::Y - Vec3::Y).length() < 1e-5);
        assert_ne!(flutter(1.3, 1.0, 0.5), flutter(1.6, 1.0, 0.5));
        // Furled at the berth the sail shrinks up to its yard; set, it is
        // the model's own.
        let base = Transform::from_xyz(0.0, 3.1, -0.63);
        assert_eq!(sail_set(base, 1.0), base);
        let furled = sail_set(base, 0.0);
        assert!(furled.scale.y < 0.2);
        let top = |t: Transform| t.translation.y + SAIL_HEIGHT_UNITS * t.scale.y;
        assert!((top(furled) - top(base)).abs() < 1e-4, "the yard stays");
        // A pennant streams downwind (its -Z along the wind).
        let along = pennant(Vec2::X, 0.0, 0.0, 0.0) * Vec3::NEG_Z;
        assert!(along.x > 0.9, "{along}");
    }

    #[test]
    fn the_hull_lies_broadside_at_its_mooring_and_bow_first_under_way() {
        let (berth, out) = (Vec2::new(5.5, -2.5), Vec2::NEG_Y);
        assert_eq!(pose(Phase::Away, berth, out), None);
        let (at, heading) = pose(Phase::Moored, berth, out).expect("moored");
        assert_eq!(at, berth);
        // Along the shore: the bow points along x.
        let bow = Vec2::new(heading.sin(), heading.cos());
        assert!(bow.dot(out).abs() < 1e-5, "{bow}");
        // Far out and coming in: bow towards the land.
        let (far, heading) = pose(Phase::Approaching(0.0), berth, out).expect("in sight");
        assert!((far - (berth + out * SAIL_DISTANCE_M)).length() < 1e-4);
        assert!((Vec2::new(heading.sin(), heading.cos()) + out).length() < 1e-4);
        // It only ever comes nearer, and ends at the mooring broadside.
        let mut last = f32::MAX;
        for step in 0..=20 {
            let (at, _) = pose(Phase::Approaching(step as f32 / 20.0), berth, out).expect("in");
            let away = at.distance(berth);
            assert!(away <= last + 1e-4);
            last = away;
        }
        assert!(last < 1e-4);
        let (_, heading) = pose(Phase::Approaching(1.0), berth, out).expect("in");
        assert!(Vec2::new(heading.sin(), heading.cos()).dot(out).abs() < 1e-4);
        // Leaving starts where mooring ends and heads out.
        let (at, _) = pose(Phase::Leaving(0.0), berth, out).expect("out");
        assert_eq!(at, berth);
        let (gone, heading) = pose(Phase::Leaving(1.0), berth, out).expect("out");
        assert!((gone - (berth + out * SAIL_DISTANCE_M)).length() < 1e-4);
        assert!((Vec2::new(heading.sin(), heading.cos()) - out).length() < 1e-4);
    }
}
