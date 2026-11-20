//! The island's deer, drawn from the Quaternius `Deer` and `Stag` models
//! under each animal's [`AnimalBody`](crate::village_world::AnimalBody), moved
//! and played from what the simulation says (`view::wildlife`): grazing and
//! standing about (`Eating`, `Idle`, `Idle_2`, `Idle_Headlow`, each deer at
//! its own phase), walking (`Walk`) and fleeing (`Gallop`) at the playback
//! speed of their ground speed, toward their target, and dead (`Death`, once,
//! held on its last frame). Every fourth animal by SimId is a stag, as the
//! simulation has it. The clips stop while the game is paused. View only:
//! nothing here writes the sim.
use std::collections::BTreeMap;
use std::time::Duration;

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use island_content::village::hunting::Hunting;
use island_content::village::wildlife::WildlifeView;
use npc_framework::prelude::{SimClock, SimControl, SimId};

/// Model scale: the native rigs stand 4.26 units to the ear tips; at this
/// scale a doe's shoulder is about a metre.
pub(crate) const DEER_SCALE: f32 = 0.35;
pub(crate) const STAG_SCALE: f32 = 0.33;
/// The models face +Z; the old box deer and the body face -Z.
const MODEL_TURN: f32 = std::f32::consts::PI;
/// Ticks before a hunt's end from which the hunted deer falls.
const DEATH_TICKS: u64 = 40;
/// Seconds of one grazing round: eating, then looking about.
const GRAZE_ROUND_SECS: f32 = 26.0;
const FADE: Duration = Duration::from_millis(350);

/// The clips the view uses, in graph order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DeerClip {
    Idle,
    Idle2,
    IdleHeadlow,
    Eating,
    Walk,
    Gallop,
    Death,
}

impl DeerClip {
    pub(crate) const ALL: [DeerClip; 7] = [
        DeerClip::Idle,
        DeerClip::Idle2,
        DeerClip::IdleHeadlow,
        DeerClip::Eating,
        DeerClip::Walk,
        DeerClip::Gallop,
        DeerClip::Death,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            DeerClip::Idle => "Idle",
            DeerClip::Idle2 => "Idle_2",
            DeerClip::IdleHeadlow => "Idle_Headlow",
            DeerClip::Eating => "Eating",
            DeerClip::Walk => "Walk",
            DeerClip::Gallop => "Gallop",
            DeerClip::Death => "Death",
        }
    }

    fn index(self) -> usize {
        DeerClip::ALL
            .iter()
            .position(|clip| *clip == self)
            .unwrap_or(0)
    }
}

/// Whether the animal with this SimId is a stag: every fourth by id, the
/// simulation's own rule (`WildlifeView::kind`).
pub(crate) fn is_stag(id: SimId) -> bool {
    id.0.is_multiple_of(4)
}

/// Which way an animal faces before it first moves (stable in its SimId),
/// and an offset off its spot, 1.5 to 3.5 m: no longer drawn, since the
/// simulation spaces its animals itself.
pub(crate) fn herd_place(id: SimId) -> (Vec3, f32) {
    let h = id.0.wrapping_mul(374_761_393).rotate_left(13) ^ id.0.wrapping_mul(2_654_435_761);
    let angle = (h % 6283) as f32 / 1000.0;
    let reach = 1.5 + ((h >> 13) % 2000) as f32 / 1000.0;
    let heading = ((h >> 3) % 6283) as f32 / 1000.0;
    (
        Vec3::new(angle.cos() * reach, 0.0, angle.sin() * reach),
        heading,
    )
}

/// Half the least distance a doe keeps from another animal, metres: a deer
/// is some 1.5 m long, so two does 1.8 m apart never touch. A stag keeps
/// more ([`STAG_REACH_M`]): it is larger and its antlers reach past its
/// body.
pub(crate) const DOE_REACH_M: f32 = 0.9;
/// Half the least distance a stag keeps from another animal, metres.
pub(crate) const STAG_REACH_M: f32 = 1.25;

/// How far an animal keeps the others off: [`STAG_REACH_M`] for a stag.
pub(crate) fn reach_of(id: SimId) -> f32 {
    if is_stag(id) {
        STAG_REACH_M
    } else {
        DOE_REACH_M
    }
}
/// A drawn animal's own radius against scenery, metres.
pub(crate) const BODY_RADIUS_M: f32 = 0.8;

/// Where the herd's bodies stand: each starts at its spot (with its reach)
/// and is pushed until no two are nearer than their reaches together and
/// none stands in an obstacle (centre, radius) of the scenery.
/// Deterministic: the spots are taken in the order given (sorted by SimId
/// by the caller). The animals are spaced last, so where the scenery and
/// the spacing cannot both be met, the animals never overlap.
pub(crate) fn spread(spots: &[(Vec2, f32)], obstacles: &[(Vec2, f32)]) -> Vec<Vec2> {
    let mut at: Vec<Vec2> = spots.iter().map(|(spot, _)| *spot).collect();
    let reach: Vec<f32> = spots.iter().map(|(_, reach)| *reach).collect();
    for _ in 0..96 {
        let mut moved = false;
        for point in at.iter_mut() {
            for (centre, radius) in obstacles {
                let off = *point - *centre;
                let clear = radius + BODY_RADIUS_M;
                let distance = off.length();
                if distance < clear {
                    let dir = off.try_normalize().unwrap_or(Vec2::X);
                    *point = *centre + dir * (clear + 1e-3);
                    moved = true;
                }
            }
        }
        for i in 0..at.len() {
            for j in i + 1..at.len() {
                let gap = reach[i] + reach[j];
                let apart = at[j] - at[i];
                let distance = apart.length();
                if distance < gap {
                    // Coinciding spots part along a direction of their own.
                    let dir = apart.try_normalize().unwrap_or_else(|| {
                        let angle = (i * 7 + j * 13) as f32;
                        Vec2::new(angle.cos(), angle.sin())
                    });
                    let push = (gap - distance) * 0.5 + 1e-3;
                    at[i] -= dir * push;
                    at[j] += dir * push;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    at
}

/// Keep the drawn herd apart and out of the scenery: every second, each
/// deer's spacing nudge is worked out by [`spread`] from where it is drawn,
/// the trees, bushes and rocks within reach being the obstacles, and
/// bounded to [`MAX_NUDGE_M`]. The simulation keeps its animals 1.5 m apart;
/// this only parts the larger stags a little more and steps them out of a
/// bush, and [`move_deer`] fades it out while an animal moves, so it never
/// fights a moving herd.
#[allow(clippy::type_complexity)]
pub(crate) fn space_herd(
    time: Res<Time<Real>>,
    mut since: Local<f32>,
    models: Query<(&DeerModel, &ChildOf)>,
    mut bodies: Query<(&ChildOf, &mut DeerMotion), Without<DeerModel>>,
    roots: Query<&GlobalTransform>,
    scenery: Query<
        (
            &GlobalTransform,
            Option<&crate::island_homes::Canopy>,
            Has<crate::island_atmosphere::dressing::EdgeBush>,
        ),
        Or<(
            With<crate::colony_scene::living_scene::SceneryCell>,
            With<crate::island_homes::Canopy>,
            With<crate::island_atmosphere::dressing::EdgeBush>,
            With<crate::island_atmosphere::dressing::OutcropRock>,
        )>,
    >,
) {
    *since += time.delta_secs();
    if *since < 1.0 {
        return;
    }
    *since = 0.0;
    // Each deer: its SimId, its body and the spot it starts from.
    let mut herd: Vec<(SimId, Entity, Vec2, Vec2)> = models
        .iter()
        .filter_map(|(model, body)| {
            let (root, _) = bodies.get(body.parent()).ok()?;
            let root_at = roots.get(root.parent()).ok()?.translation();
            let root_at = Vec2::new(root_at.x, root_at.z);
            Some((model.sim, body.parent(), root_at, root_at))
        })
        .collect();
    if herd.is_empty() {
        return;
    }
    herd.sort_by_key(|(sim, ..)| *sim);
    let near = |p: Vec2| herd.iter().any(|(.., spot)| spot.distance(p) < 8.0);
    let obstacles: Vec<(Vec2, f32)> = scenery
        .iter()
        .filter_map(|(at, canopy, bush)| {
            let p = Vec2::new(at.translation().x, at.translation().z);
            near(p).then(|| {
                let radius = match (canopy, bush) {
                    (Some(canopy), _) => canopy.shape.reach().min(2.5),
                    (None, true) => 0.9,
                    _ => 0.8,
                };
                (p, radius)
            })
        })
        .collect();
    let spots: Vec<(Vec2, f32)> = herd
        .iter()
        .map(|(sim, .., spot)| (*spot, reach_of(*sim)))
        .collect();
    let placed = spread(&spots, &obstacles);
    for ((_, body, root_at, _), at) in herd.iter().zip(placed) {
        if let Ok((_, mut motion)) = bodies.get_mut(*body) {
            let nudge = (at - *root_at).clamp_length_max(MAX_NUDGE_M);
            if motion.nudge.distance(nudge) > 1e-3 {
                motion.nudge = nudge;
            }
        }
    }
}

/// The two models and their graphs, once loaded.
#[derive(Resource)]
pub(crate) struct DeerAssets {
    gltf: [Handle<bevy::gltf::Gltf>; 2],
    scene: [Handle<WorldAsset>; 2],
    graphs: [Option<(Handle<AnimationGraph>, Vec<AnimationNodeIndex>)>; 2],
}

impl FromWorld for DeerAssets {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let path = |stag: bool| {
            if stag {
                "living/animals/Stag.gltf"
            } else {
                "living/animals/Deer.gltf"
            }
        };
        Self {
            gltf: [server.load(path(false)), server.load(path(true))],
            scene: [
                server.load(GltfAssetLabel::Scene(0).from_asset(path(false))),
                server.load(GltfAssetLabel::Scene(0).from_asset(path(true))),
            ],
            graphs: [None, None],
        }
    }
}

impl DeerAssets {
    pub(crate) fn scene(&self, stag: bool) -> Handle<WorldAsset> {
        self.scene[stag as usize].clone()
    }
}

/// On a deer model under an animal's body.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct DeerModel {
    pub(crate) sim: SimId,
    pub(crate) stag: bool,
    pub(crate) seed: f32,
}

/// On the model's animation player: what it plays.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct DeerPlayer {
    model: Entity,
    current: Option<DeerClip>,
}

impl DeerPlayer {
    /// Whether this deer gallops now (its hooves are heard).
    pub(crate) fn galloping(&self) -> bool {
        self.current == Some(DeerClip::Gallop)
    }
}

/// The model of a deer: a child of its body, turned to face the body's way.
pub(crate) fn model_bundle(assets: &DeerAssets, sim: SimId, seed: f32) -> impl Bundle {
    let stag = is_stag(sim);
    let scale = if stag { STAG_SCALE } else { DEER_SCALE };
    (
        Name::new(if stag { "stag model" } else { "deer model" }),
        DeerModel { sim, stag, seed },
        WorldAssetRoot(assets.scene(stag)),
        Transform::from_rotation(Quat::from_rotation_y(MODEL_TURN)).with_scale(Vec3::splat(scale)),
    )
}

pub(crate) fn build_graphs(
    assets: Option<ResMut<DeerAssets>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Some(mut assets) = assets else { return };
    for kind in 0..2 {
        if assets.graphs[kind].is_some() {
            continue;
        }
        let Some(gltf) = gltfs.get(&assets.gltf[kind]) else {
            continue;
        };
        let clips: Option<Vec<_>> = DeerClip::ALL
            .iter()
            .map(|clip| gltf.named_animations.get(clip.name()).cloned())
            .collect();
        let Some(clips) = clips else {
            warn!("island_view: a deer model lacks one of its clips; it stands still");
            continue;
        };
        let (graph, nodes) = AnimationGraph::from_clips(clips);
        assets.graphs[kind] = Some((graphs.add(graph), nodes));
    }
}

/// Observer on a deer model: mark its animation player.
pub(crate) fn on_ready(
    ready: On<WorldInstanceReady>,
    models: Query<(), With<DeerModel>>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    mut commands: Commands,
) {
    if !models.contains(ready.entity) {
        return;
    }
    for e in children.iter_descendants(ready.entity) {
        if players.contains(e) {
            commands.entity(e).insert(DeerPlayer {
                model: ready.entity,
                current: None,
            });
        }
    }
}

/// What the simulation says each animal is doing (`view::wildlife`, by
/// SimId), and the tick it was read at.
#[derive(Resource, Default)]
pub(crate) struct WildlifeStates(
    pub(crate) BTreeMap<SimId, WildlifeView>,
    pub(crate) Option<u64>,
);

/// Read the animals' states when the simulation's clock has moved (the
/// states change only on a tick). View only.
pub(crate) fn read_wildlife(world: &mut World) {
    let tick = world.get_resource::<SimClock>().map(|clock| clock.tick);
    let Some(states) = world.get_resource::<WildlifeStates>() else {
        return;
    };
    if tick.is_some() && states.1 == tick {
        return;
    }
    let views = island_content::village::view::wildlife(world);
    let mut states = world.resource_mut::<WildlifeStates>();
    states.0.clear();
    states
        .0
        .extend(views.into_iter().map(|view| (view.id, view)));
    states.1 = tick;
}

/// An animal's state as the view plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeerState {
    Grazing,
    Walking,
    Fleeing,
    Dead,
}

impl DeerState {
    pub(crate) fn of(state: &str) -> Self {
        match state {
            "walking" => DeerState::Walking,
            "fleeing" => DeerState::Fleeing,
            "dead" => DeerState::Dead,
            _ => DeerState::Grazing,
        }
    }

    fn moving(self) -> bool {
        matches!(self, DeerState::Walking | DeerState::Fleeing)
    }
}

/// Ground speed in metres per real second: the simulation's step per tick
/// at its tick rate and speed-up.
pub(crate) fn ground_speed(speed_mm_per_tick: i32, hz: u32, multiplier: u32) -> f32 {
    speed_mm_per_tick.max(0) as f32 / 1000.0 * hz as f32 * multiplier as f32
}

/// The clip for a deer in `state`: `hunt_left` the ticks left of a hunt
/// aimed at it, `seconds` the sim time and `seed` its own phase, so the
/// grazing of a herd is not in step.
pub(crate) fn deer_clip(
    state: DeerState,
    hunt_left: Option<u64>,
    seconds: f32,
    seed: f32,
) -> DeerClip {
    match state {
        DeerState::Dead => return DeerClip::Death,
        DeerState::Fleeing => return DeerClip::Gallop,
        DeerState::Walking => return DeerClip::Walk,
        DeerState::Grazing => {}
    }
    if let Some(left) = hunt_left {
        // A hunter close: it looks up, alert, until the shot.
        return if left <= DEATH_TICKS {
            DeerClip::Death
        } else {
            DeerClip::Idle2
        };
    }
    let round = ((seconds / GRAZE_ROUND_SECS) + seed).fract();
    match round {
        r if r < 0.5 => DeerClip::Eating,
        r if r < 0.65 => DeerClip::Idle,
        r if r < 0.82 => DeerClip::Idle2,
        _ => DeerClip::IdleHeadlow,
    }
}

/// Ground speed (m/s at the drawn scale) at which the `Walk` clip's feet
/// keep their hold on the ground when it plays at speed 1.
const WALK_STRIDE_MPS: f32 = 1.1;
/// The same for `Gallop`.
const GALLOP_STRIDE_MPS: f32 = 5.0;

/// How fast a clip plays for a ground speed (m/s): the legs keep up with
/// the ground, so feet do not slide.
pub(crate) fn clip_speed(clip: DeerClip, ground: f32) -> f32 {
    match clip {
        DeerClip::Walk => (ground / WALK_STRIDE_MPS).clamp(0.2, 4.0),
        DeerClip::Gallop => (ground / GALLOP_STRIDE_MPS).clamp(0.3, 4.0),
        _ => 1.0,
    }
}

/// Whether a clip loops: everything but `Death`, played once and then held
/// on its last frame.
pub(crate) fn repeats(clip: DeerClip) -> bool {
    clip != DeerClip::Death
}

/// The body's turn that faces `heading` (world x, z): the body looks down
/// its -Z.
pub(crate) fn heading_yaw(heading: Vec2) -> f32 {
    (-heading.x).atan2(-heading.y)
}

/// `yaw` turned toward `goal` by at most `step` radians, the short way.
pub(crate) fn turn_toward(yaw: f32, goal: f32, step: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let off = (goal - yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    yaw + off.clamp(-step, step)
}

/// The drawn position following the simulation's, which moves in steps
/// every few ticks: at the animal's own ground speed (a little faster, and
/// faster the further behind), so it walks on smoothly between the steps
/// instead of starting and stopping; snapped when far behind (a load).
pub(crate) fn follow(drawn: Vec2, sim: Vec2, ground: f32, dt: f32) -> Vec2 {
    let off = sim - drawn;
    let behind = off.length();
    if behind > SNAP_M {
        return sim;
    }
    if behind < 1e-4 {
        return sim;
    }
    let pace = ground * 0.9 + behind * 1.2 + 0.02;
    drawn + off * (pace * dt / behind).min(1.0)
}

/// Further behind than this the drawn animal is put where it is, metres.
const SNAP_M: f32 = 4.0;
/// The farthest the view's spacing moves a body off its simulation spot,
/// metres: a small nudge that fades while the animal moves.
pub(crate) const MAX_NUDGE_M: f32 = 0.7;

/// On a wild deer's visual root: its body, which turns and takes the
/// spacing nudge.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WildDeer {
    pub(crate) body: Entity,
}

/// On a wild deer's body: which way it faces, the spacing nudge and how
/// still it is (1 standing, 0 moving), which scales the nudge.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct DeerMotion {
    pub(crate) yaw: f32,
    pub(crate) nudge: Vec2,
    pub(crate) still: f32,
}

impl DeerMotion {
    pub(crate) fn facing(yaw: f32) -> Self {
        Self {
            yaw,
            nudge: Vec2::ZERO,
            still: 1.0,
        }
    }
}

/// Move each wild deer's root after the simulation's position, turn its
/// body toward where it is going, and ease the spacing nudge in while it
/// stands and out while it moves. The clock's pause stops it like
/// everything the simulation drives; Reduce motion does not (it is the
/// game's state).
#[allow(clippy::type_complexity)]
pub(crate) fn move_deer(
    time: Res<Time<Real>>,
    states: Option<Res<WildlifeStates>>,
    (rate, control): (
        Option<Res<npc_framework::prelude::SimRate>>,
        Option<Res<SimControl>>,
    ),
    terrain: Option<Res<island_content::village::frontier::terrain::TerrainState>>,
    mut roots: Query<(&WildDeer, &crate::village_world::StaticId, &mut Transform)>,
    mut bodies: Query<(&mut Transform, &mut DeerMotion), Without<WildDeer>>,
) {
    let Some(states) = states else { return };
    let dt = time.delta_secs().min(0.1);
    let running = control
        .as_deref()
        .is_some_and(|control| !control.is_stopped());
    let (hz, multiplier) = (
        rate.as_deref().map_or(20, |rate| rate.hz),
        control
            .as_deref()
            .map_or(1, |control| control.multiplier.max(1)),
    );
    for (deer, id, mut root) in &mut roots {
        let Some(view) = states.0.get(&id.0) else {
            continue;
        };
        let state = DeerState::of(view.state);
        let sim = Vec2::new(view.position.x as f32, view.position.z as f32) / 1000.0;
        let drawn = Vec2::new(root.translation.x, root.translation.z);
        let ground = ground_speed(view.speed_mm_per_tick, hz, multiplier);
        let next = if running {
            follow(drawn, sim, ground, dt)
        } else if drawn.distance(sim) > SNAP_M {
            sim
        } else {
            drawn
        };
        if next != drawn {
            let y = terrain
                .as_deref()
                .and_then(|terrain| terrain.cell(next.x.floor() as i32, next.y.floor() as i32))
                .filter(|cell| {
                    cell.surface
                        != island_content::village::frontier::terrain::TerrainSurface::Water
                })
                .map_or(root.translation.y, |cell| cell.height_mm as f32 / 1000.0);
            root.translation = Vec3::new(next.x, y, next.y);
        }
        let Ok((mut body, mut motion)) = bodies.get_mut(deer.body) else {
            continue;
        };
        if running {
            let heading = Vec2::new(
                (view.target.x - view.position.x) as f32,
                (view.target.z - view.position.z) as f32,
            ) / 1000.0;
            if state.moving() && heading.length() > 0.05 {
                let rate = if state == DeerState::Fleeing {
                    6.0
                } else {
                    2.5
                };
                motion.yaw = turn_toward(motion.yaw, heading_yaw(heading), rate * dt);
            }
            let want = if state.moving() { 0.0 } else { 1.0 };
            let ease = if state.moving() { 4.0 } else { 0.8 };
            motion.still += (want - motion.still) * (ease * dt).min(1.0);
        }
        let place = motion.nudge * motion.still;
        let turn = Quat::from_rotation_y(motion.yaw);
        let at = Vec3::new(place.x, body.translation.y, place.y);
        if body.translation.distance(at) > 1e-4 || body.rotation.angle_between(turn) > 1e-4 {
            body.translation = at;
            body.rotation = turn;
        }
    }
}

/// Choose and play each deer's clip from the simulation's state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn play(
    mut commands: Commands,
    assets: Option<Res<DeerAssets>>,
    states: Option<Res<WildlifeStates>>,
    (clock, control, motion, rate): (
        Option<Res<SimClock>>,
        Option<Res<SimControl>>,
        Option<Res<super::ambient::AmbientMotion>>,
        Option<Res<npc_framework::prelude::SimRate>>,
    ),
    hunts: Query<&Hunting>,
    models: Query<&DeerModel>,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut DeerPlayer,
        Option<&mut AnimationTransitions>,
    )>,
) {
    let (Some(assets), Some(clock)) = (assets, clock) else {
        return;
    };
    let now = clock.tick;
    let hunted: BTreeMap<SimId, u64> = hunts
        .iter()
        .map(|hunt| (hunt.target, hunt.ends.saturating_sub(now)))
        .collect();
    let stopped = control.as_deref().is_none_or(SimControl::is_stopped);
    let calm = motion.is_some_and(|motion| motion.scale <= 0.0);
    let (hz, multiplier) = (
        rate.as_deref().map_or(20, |rate| rate.hz),
        control
            .as_deref()
            .map_or(1, |control| control.multiplier.max(1)),
    );
    let seconds = now as f32 / 20.0;
    for (entity, mut player, mut deer, transitions) in &mut players {
        let Ok(model) = models.get(deer.model) else {
            continue;
        };
        let Some((graph, nodes)) = assets.graphs[model.stag as usize].as_ref() else {
            continue;
        };
        let view = states
            .as_deref()
            .and_then(|states| states.0.get(&model.sim));
        let state = view.map_or(DeerState::Grazing, |view| DeerState::of(view.state));
        let ground = view.map_or(0.0, |view| {
            ground_speed(view.speed_mm_per_tick, hz, multiplier)
        });
        let clip = deer_clip(state, hunted.get(&model.sim).copied(), seconds, model.seed);
        let node = nodes[clip.index()];
        match transitions {
            None => {
                let mut transitions = AnimationTransitions::new();
                let active = transitions.play(&mut player, node, Duration::ZERO);
                if repeats(clip) {
                    active.repeat();
                }
                commands
                    .entity(entity)
                    .insert((AnimationGraphHandle(graph.clone()), transitions));
                deer.current = Some(clip);
            }
            Some(mut transitions) if deer.current != Some(clip) => {
                let active = transitions.play(&mut player, node, FADE);
                if repeats(clip) {
                    active.repeat();
                }
                deer.current = Some(clip);
            }
            Some(_) => {}
        }
        // Paused: everything holds. Reduce motion: the standing round holds
        // still; walking and fleeing play, since the animals move.
        let speed = if stopped || (calm && !state.moving()) {
            0.0
        } else {
            clip_speed(clip, ground)
        };
        for (_, active) in player.playing_animations_mut() {
            if active.speed() != speed {
                active.set_speed(speed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clip_follows_the_simulations_state() {
        // Grazing: eating most of the time, looking about the rest, each
        // deer at its own phase, so a herd is not in step.
        let rounds: Vec<_> = (0..52)
            .map(|s| deer_clip(DeerState::Grazing, None, s as f32, 0.0))
            .collect();
        assert!(rounds.contains(&DeerClip::Eating));
        assert!(rounds.contains(&DeerClip::Idle));
        assert!(rounds.contains(&DeerClip::IdleHeadlow));
        assert_ne!(
            deer_clip(DeerState::Grazing, None, 3.0, 0.0),
            deer_clip(DeerState::Grazing, None, 3.0, 0.55)
        );
        assert_eq!(DeerState::of("grazing"), DeerState::Grazing);
        assert_eq!(
            deer_clip(DeerState::of("walking"), None, 3.0, 0.0),
            DeerClip::Walk
        );
        assert_eq!(
            deer_clip(DeerState::of("fleeing"), None, 3.0, 0.0),
            DeerClip::Gallop
        );
        assert_eq!(
            deer_clip(DeerState::of("dead"), None, 3.0, 0.0),
            DeerClip::Death
        );
        // Hunted while grazing: alert, then it falls over the last ticks.
        assert_eq!(
            deer_clip(DeerState::Grazing, Some(200), 3.0, 0.0),
            DeerClip::Idle2
        );
        assert_eq!(
            deer_clip(DeerState::Grazing, Some(20), 3.0, 0.0),
            DeerClip::Death
        );
    }

    #[test]
    fn playback_follows_the_simulations_speed_and_death_is_held() {
        // 25 mm a tick at 20 Hz is a walk of half a metre a second; the
        // gallop's 160 mm is 3.2 m/s; three times the speed-up, three times
        // as fast.
        let walk = ground_speed(25, 20, 1);
        assert!((walk - 0.5).abs() < 1e-5, "{walk}");
        let gallop = ground_speed(160, 20, 1);
        assert!((gallop - 3.2).abs() < 1e-5, "{gallop}");
        assert!((ground_speed(25, 20, 3) - 1.5).abs() < 1e-5);
        // The legs keep the pace of the ground: the playback is the ground
        // speed over the clip's stride.
        assert!((clip_speed(DeerClip::Walk, walk) - walk / WALK_STRIDE_MPS).abs() < 1e-5);
        assert!((clip_speed(DeerClip::Gallop, gallop) - gallop / GALLOP_STRIDE_MPS).abs() < 1e-5);
        assert!(clip_speed(DeerClip::Walk, 1.5) > clip_speed(DeerClip::Walk, 0.5));
        assert_eq!(clip_speed(DeerClip::Eating, 0.0), 1.0);
        // Death plays once and holds its last frame; the rest loop.
        assert!(!repeats(DeerClip::Death));
        assert!(repeats(DeerClip::Walk) && repeats(DeerClip::Eating));
    }

    #[test]
    fn a_moving_deer_turns_smoothly_toward_its_target_and_follows_without_jumps() {
        // Heading +X: the body (looking down -Z) turns to face +X.
        let goal = heading_yaw(Vec2::X);
        let ahead = Quat::from_rotation_y(goal) * Vec3::NEG_Z;
        assert!((ahead - Vec3::X).length() < 1e-4, "{ahead}");
        // It turns by a bounded step each frame, the short way round.
        let mut yaw = 0.0;
        let mut steps = 0;
        while (yaw - goal).abs() > 1e-3 && steps < 100 {
            let next = turn_toward(yaw, goal, 0.1);
            assert!((next - yaw).abs() <= 0.1 + 1e-5);
            yaw = next;
            steps += 1;
        }
        assert!(steps > 5 && steps < 100, "{steps}");
        assert!(
            (turn_toward(3.0, -3.0, 1.0) - 3.0).abs() < 0.3,
            "the short way"
        );
        // Following steps of 125 mm every five ticks at 0.5 m/s: the drawn
        // animal moves every frame, never jumps, and stays close.
        let (mut drawn, mut sim) = (Vec2::ZERO, Vec2::ZERO);
        let dt = 1.0 / 60.0;
        let mut frames_moved = 0;
        for frame in 0..600 {
            // A sim tick every 1/20 s, a step every fifth tick.
            let tick = (frame as f32 * dt * 20.0) as i32;
            sim.x = (tick / 5) as f32 * 0.125;
            let next = follow(drawn, sim, 0.5, dt);
            assert!(next.distance(drawn) < 0.05, "no jump at frame {frame}");
            if next.distance(drawn) > 1e-5 {
                frames_moved += 1;
            }
            drawn = next;
            assert!(drawn.distance(sim) < 0.4, "{drawn} {sim}");
        }
        assert!(frames_moved > 500, "{frames_moved}");
        // A load far away: put there at once.
        assert_eq!(
            follow(Vec2::ZERO, Vec2::new(30.0, 0.0), 0.5, dt),
            Vec2::new(30.0, 0.0)
        );
    }

    #[test]
    fn a_herd_on_one_spot_stands_apart_as_a_loose_group() {
        // Eight animals the simulation puts on the same spot.
        let places: Vec<_> = (168..176).map(|id| herd_place(SimId(id))).collect();
        for (i, (a, _)) in places.iter().enumerate() {
            assert!(a.length() <= 3.5 + 1e-4 && a.length() >= 1.5 - 1e-4, "{a}");
            for (b, _) in places.iter().skip(i + 1) {
                assert!(a.distance(*b) > 0.5, "{a} {b}");
            }
        }
        let headings: std::collections::BTreeSet<i32> =
            places.iter().map(|(_, h)| (h * 10.0) as i32).collect();
        assert!(headings.len() >= 6, "{headings:?}");
        assert_eq!(herd_place(SimId(170)), herd_place(SimId(170)));
    }

    #[test]
    fn a_herd_keeps_its_distance_and_stays_out_of_the_bushes() {
        // Eight deer born on two spots 1.5 m apart, a bush and a tree among
        // them.
        let spots: Vec<Vec2> = (0..8)
            .map(|i| Vec2::new(-28.0 - 1.5 * (i % 2) as f32, -2.0 + 1.5 * (i / 4) as f32))
            .collect();
        let obstacles = [(Vec2::new(-28.5, -1.0), 0.9), (Vec2::new(-30.0, -3.0), 1.4)];
        // Two of them stags, born on the same spot as a doe.
        let reach = |i: usize| {
            if i.is_multiple_of(4) {
                STAG_REACH_M
            } else {
                DOE_REACH_M
            }
        };
        let spots: Vec<(Vec2, f32)> = spots
            .iter()
            .enumerate()
            .map(|(i, spot)| (*spot, reach(i)))
            .collect();
        let placed = spread(&spots, &obstacles);
        let spots: Vec<Vec2> = spots.iter().map(|(spot, _)| *spot).collect();
        for (i, a) in placed.iter().enumerate() {
            for (j, b) in placed.iter().enumerate().skip(i + 1) {
                assert!(a.distance(*b) >= reach(i) + reach(j) - 1e-2, "{a} {b}");
                assert!(a.distance(*b) >= 1.6);
            }
            for (centre, radius) in obstacles {
                assert!(
                    a.distance(centre) >= radius + BODY_RADIUS_M - 1e-2,
                    "{a} in {centre}"
                );
            }
            // A loose group, not scattered: each within a few metres of
            // where the simulation put it.
            assert!(a.distance(spots[i]) < 6.0, "{a} {}", spots[i]);
        }
        assert!(placed[0].distance(placed[4]) >= 2.0 * STAG_REACH_M - 1e-2);
        let again: Vec<(Vec2, f32)> = spots
            .iter()
            .enumerate()
            .map(|(i, s)| (*s, reach(i)))
            .collect();
        assert_eq!(placed, spread(&again, &obstacles));
    }

    #[test]
    fn every_fourth_animal_is_a_stag_as_the_simulation_says() {
        let stags = (1..401).filter(|id| is_stag(SimId(*id))).count();
        assert_eq!(stags, 100);
        assert!(is_stag(SimId(168)) && !is_stag(SimId(169)));
    }
}
