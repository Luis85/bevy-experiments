//! Island activity poses: which motion a resident's current sim state reads
//! as. Presentation only: the mapping never feeds back into the simulation,
//! and it is active only where the island atmosphere installs
//! [`IslandPoses`], so legacy scenarios keep their original animation.
use std::collections::BTreeMap;

use bevy::{ecs::query::QueryState, ecs::system::SystemParam, prelude::*};
use npc_framework::ai::NodeKind;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::Interacting;
use npc_framework::prelude::{
    ActiveLeaf, AiRegistry, Brain, Inventory, Item, SimClock, SimConfig, SimIndex,
};
use testbed_content::village::{
    Gathering, ResourcePatch, Role, VillagePlace, Villager, catalog,
    frontier::activities::EcologyWork, homestead::CottageBuild, indoors::Repose, view,
};

use crate::anim::Pose;
use crate::assets::ModelKind;
use crate::island_atmosphere::goods::{self, CarriedGood};

/// Present while the island living-world scene owns the view.
#[derive(Resource, Default)]
pub(crate) struct IslandPoses;

/// Who is asleep, as the simulation has it (`view::repose`): read once a
/// tick, because the animation pass cannot ask the world itself.
#[derive(Resource, Default)]
pub(crate) struct ReposeCache {
    tick: Option<u64>,
    asleep: BTreeMap<Entity, bool>,
    /// What each resident holds for their work (`view::held`): the tool
    /// good and whether one is carried. Without one the work goes bare-handed
    /// and no tool is drawn.
    held: BTreeMap<Entity, (String, bool)>,
}

impl ReposeCache {
    /// Whether `actor` is asleep; `None` for somebody the cache has not seen.
    pub(crate) fn asleep(&self, actor: Entity) -> Option<bool> {
        self.asleep.get(&actor).copied()
    }

    /// The tool `actor` works with, if the simulation names one, and
    /// whether they carry it.
    pub(crate) fn held(&self, actor: Entity) -> Option<(&str, bool)> {
        self.held
            .get(&actor)
            .map(|(tool, carried)| (tool.as_str(), *carried))
    }
}

/// The drawn tool for a work pose and what the simulation says is in the
/// hand: the pose's own tool, a rod, axe or hoe where the simulation names
/// one, and nothing when the work goes bare-handed.
pub(crate) fn drawn_tool(pose: Pose, held: Option<(&str, bool)>) -> Option<Tool> {
    let by_pose = Tool::of(pose)?;
    let Some((tool, carried)) = held else {
        return Some(by_pose);
    };
    if !carried {
        return None;
    }
    Some(if tool.contains("rod") {
        Tool::Rod
    } else if tool.contains("pickaxe") {
        by_pose
    } else if tool.contains("axe") {
        Tool::Axe
    } else if tool.contains("hoe") {
        Tool::Hoe
    } else {
        by_pose
    })
}

/// Fill the [`ReposeCache`] when the clock has moved.
pub(crate) fn cache_repose(world: &mut World, people: &mut QueryState<Entity, With<Villager>>) {
    let tick = world.get_resource::<SimClock>().map(|clock| clock.tick);
    if world.resource::<ReposeCache>().tick == tick {
        return;
    }
    let actors: Vec<Entity> = people.iter(world).collect();
    let asleep: BTreeMap<Entity, bool> = actors
        .iter()
        .map(|actor| (*actor, view::repose(world, *actor) == Repose::Asleep))
        .collect();
    let held: BTreeMap<Entity, (String, bool)> = actors
        .iter()
        .filter_map(|actor| {
            view::held(world, *actor).map(|held| (*actor, (held.tool, held.carried)))
        })
        .collect();
    let mut cache = world.resource_mut::<ReposeCache>();
    cache.tick = tick;
    cache.asleep = asleep;
    cache.held = held;
}

/// What the sim currently says about one resident, flattened for the mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Activity<'a> {
    pub resting: bool,
    /// Asleep in a bed, as the simulation has it; a rest that is not sleep
    /// is a sit.
    pub asleep: bool,
    pub moving: bool,
    pub carrying: Option<CarriedGood>,
    /// Kind of the resource patch being gathered.
    pub gathering: Option<&'a str>,
    pub verb: Option<&'a str>,
    pub crafting: bool,
    /// Role of the workstation a craft runs at.
    pub station: Option<Role>,
    pub building: bool,
    pub restoring: bool,
    /// Free time spent with a neighbour (the `Leisure` goal's meeting leaf).
    pub meeting: bool,
}

/// `None` leaves the generic testbed mapping (`pose_of`) in charge.
pub(crate) fn island_pose(activity: Activity) -> Option<Pose> {
    if activity.resting {
        return Some(if activity.asleep {
            Pose::Sleep
        } else {
            Pose::Sit
        });
    }
    if activity.moving {
        return activity.carrying.map(|_| Pose::Carry);
    }
    if activity.building {
        return Some(Pose::Hammer);
    }
    if activity.restoring {
        return Some(Pose::Farm);
    }
    if let Some(kind) = activity.gathering {
        return Some(match kind {
            "log" => Pose::Chop,
            catalog::FISH => Pose::Fish,
            catalog::GRAIN | catalog::FIBER | catalog::BERRIES => Pose::Farm,
            catalog::WATER => Pose::PickUp,
            // Ore and anything unrecognised keep the generic tool swing.
            _ => Pose::Work,
        });
    }
    if let Some(verb) = activity.verb {
        return matches!(verb, "plant" | "water_crop" | "harvest" | "plant_tree")
            .then_some(Pose::Farm);
    }
    if activity.crafting {
        return matches!(
            activity.station,
            Some(Role::Smith | Role::Carpenter | Role::Woodcutter | Role::Miner | Role::Trade(_))
        )
        .then_some(Pose::Hammer);
    }
    // Leisure: a stroll walks and idling stands, as the generic mapping
    // already shows them; standing with a neighbour is a greeting.
    if activity.meeting {
        return Some(Pose::Interact);
    }
    None
}

/// The tool a work pose holds in the right hand: an axe at a tree, a hoe on
/// a plot, a rod at the water, a hammer at a smithy or a building site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Axe,
    Hoe,
    Rod,
    Hammer,
}

impl Tool {
    /// The tool a pose shows, if any. Keyed on the pose, which is keyed on
    /// the simulation's activity: nothing here reads anything else.
    pub(crate) fn of(pose: Pose) -> Option<Tool> {
        match pose {
            Pose::Chop => Some(Tool::Axe),
            Pose::Farm => Some(Tool::Hoe),
            Pose::Fish => Some(Tool::Rod),
            Pose::Hammer => Some(Tool::Hammer),
            _ => None,
        }
    }

    /// The code-built tools' boxes, in hand-slot units (a hand slot carries
    /// the rig's scale, [`crate::visuals::V`] metres a unit): the handle
    /// runs up the slot's +Y. `None` for the tools drawn from a model.
    pub(crate) fn boxes(self) -> Option<Vec<(Vec3, Vec3, bool)>> {
        let m = |metres: f32| metres / crate::visuals::V;
        match self {
            // A long handle and a flat iron blade turned forward at its top.
            Tool::Hoe => Some(vec![
                (
                    Vec3::new(0.0, m(0.45), 0.0),
                    Vec3::new(m(0.04), m(1.35), m(0.04)),
                    false,
                ),
                (
                    Vec3::new(0.0, m(1.1), m(0.1)),
                    Vec3::new(m(0.16), m(0.03), m(0.2)),
                    true,
                ),
            ]),
            // A long thin rod, darker at its tip.
            Tool::Rod => Some(vec![
                (
                    Vec3::new(0.0, m(0.75), 0.0),
                    Vec3::new(m(0.035), m(1.7), m(0.035)),
                    false,
                ),
                (
                    Vec3::new(0.0, m(1.8), 0.0),
                    Vec3::new(m(0.02), m(0.45), m(0.02)),
                    true,
                ),
            ]),
            Tool::Axe | Tool::Hammer => None,
        }
    }
}

/// Work tools are drawn this much larger than the kit's own scale, so a
/// tool still reads at settlement zoom (20 m); a hand-sized axe or hammer
/// did not.
pub(crate) const WORK_TOOL_SCALE: f32 = 1.3;

/// The work tool in a resident's hand, by visual root.
#[derive(Resource, Default)]
pub(crate) struct WorkProps(BTreeMap<Entity, (Tool, Entity)>);

/// Shared meshes and materials of the code-built tools.
#[derive(Resource)]
pub(crate) struct ToolAssets {
    cube: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
}

pub(crate) fn setup_tools(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ToolAssets {
        cube: meshes.add(Cuboid::default()),
        wood: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(122, 82, 52),
            perceptual_roughness: 0.9,
            ..default()
        }),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(120, 124, 130),
            perceptual_roughness: 0.5,
            metallic: 0.4,
            ..default()
        }),
    });
}

/// Put the pose's tool in the right hand while a work clip plays, and take
/// it away when it stops. A hand that holds an item of the simulation's (an
/// equipped tool) is left alone. View entities only.
#[allow(clippy::too_many_arguments)]
pub(crate) fn work_props(
    mut commands: Commands,
    enabled: Option<Res<IslandPoses>>,
    assets: Option<Res<crate::assets::ViewAssets>>,
    tools: Option<Res<ToolAssets>>,
    targets: Query<&crate::anim::ActorAnimTarget>,
    actors: Query<&crate::visuals::ActorVisual>,
    held: Query<&Children>,
    items: Query<(), With<npc_framework::view_helpers::VisualOf>>,
    (sims, repose): (
        Query<&npc_framework::view_helpers::VisualOf>,
        Option<Res<ReposeCache>>,
    ),
    mut props: ResMut<WorkProps>,
    (time, mut since): (Res<Time>, Local<f32>),
) {
    if enabled.is_none() {
        return;
    }
    // A clip changes at most a few times a second: look five times a second.
    *since += time.delta_secs();
    if *since < 0.2 {
        return;
    }
    *since = 0.0;
    let (Some(assets), Some(tools)) = (assets, tools) else {
        return;
    };
    // Gone with their actor.
    props.0.retain(|root, (_, prop)| {
        let alive = actors.contains(*root);
        if !alive {
            commands.entity(*prop).try_despawn();
        }
        alive
    });
    for target in &targets {
        let root = target.root();
        let Ok(actor) = actors.get(root) else {
            continue;
        };
        let hand = actor.hand_r;
        let busy_hand = hand.is_some_and(|hand| {
            held.get(hand)
                .is_ok_and(|children| children.iter().any(|child| items.contains(child)))
        });
        let held = sims
            .get(root)
            .ok()
            .zip(repose.as_deref())
            .and_then(|(of, cache)| cache.held(of.0));
        let want = target
            .playing()
            .and_then(|pose| drawn_tool(pose, held))
            .filter(|_| !busy_hand)
            .zip(hand);
        let have = props.0.get(&root).map(|(tool, _)| *tool);
        if want.map(|(tool, _)| tool) == have {
            continue;
        }
        if let Some((_, prop)) = props.0.remove(&root) {
            commands.entity(prop).try_despawn();
        }
        let Some((tool, hand)) = want else { continue };
        let models = &assets.models;
        let prop = match tool {
            // As an equipped axe or hammer of the simulation is held.
            Tool::Axe => commands
                .spawn((
                    Name::new("work tool"),
                    ChildOf(hand),
                    bevy::world_serialization::WorldAssetRoot(models.axe.clone()),
                    Transform::from_xyz(0.0, 0.03, 0.0)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                        .with_scale(Vec3::splat(WORK_TOOL_SCALE)),
                ))
                .id(),
            Tool::Hammer => commands
                .spawn((
                    Name::new("work tool"),
                    ChildOf(hand),
                    bevy::world_serialization::WorldAssetRoot(models.hammer.clone()),
                    Transform::from_xyz(0.0, -0.18, 0.0)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                        .with_scale(Vec3::splat(crate::visuals::KENNEY_TOOL * WORK_TOOL_SCALE)),
                ))
                .id(),
            Tool::Hoe | Tool::Rod => {
                let prop = commands
                    .spawn((
                        Name::new("work tool"),
                        ChildOf(hand),
                        // The rod is held out over the water.
                        Transform::from_rotation(Quat::from_rotation_x(if tool == Tool::Rod {
                            0.5
                        } else {
                            0.0
                        }))
                        .with_scale(Vec3::splat(WORK_TOOL_SCALE)),
                        Visibility::default(),
                    ))
                    .id();
                for (centre, size, iron) in tool.boxes().unwrap_or_default() {
                    commands.spawn((
                        ChildOf(prop),
                        Mesh3d(tools.cube.clone()),
                        MeshMaterial3d(if iron {
                            tools.iron.clone()
                        } else {
                            tools.wood.clone()
                        }),
                        Transform::from_translation(centre).with_scale(size),
                        bevy::light::NotShadowCaster,
                    ));
                }
                prop
            }
        };
        props.0.insert(root, (tool, prop));
    }
}

/// How far round a fisher the water is looked for, cells.
const WATER_REACH: i32 = 3;

/// The turn that faces somebody standing at `at` (world x, z) towards the
/// nearest water cell within [`WATER_REACH`]; `None` with no water near.
/// A model's +Z faces the turn, as everywhere in the view.
pub(crate) fn water_yaw(at: Vec2, water: impl Fn(i32, i32) -> bool) -> Option<f32> {
    let cell = (at.x.floor() as i32, at.y.floor() as i32);
    let mut best: Option<(f32, Vec2)> = None;
    for dz in -WATER_REACH..=WATER_REACH {
        for dx in -WATER_REACH..=WATER_REACH {
            let (x, z) = (cell.0 + dx, cell.1 + dz);
            if !water(x, z) {
                continue;
            }
            let centre = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let away = centre.distance_squared(at);
            if best.is_none_or(|(least, _)| away < least) {
                best = Some((away, centre));
            }
        }
    }
    best.map(|(_, centre)| {
        let to = centre - at;
        to.x.atan2(to.y)
    })
}

/// A fisher at work faces the water: the simulation's fishing ground is the
/// dry cell at the water's edge and says nothing of which way the line goes.
/// Runs after the transforms are written, for whoever plays the fishing
/// clip.
pub(crate) fn face_water(
    terrain: Option<Res<testbed_content::village::frontier::terrain::TerrainState>>,
    targets: Query<&crate::anim::ActorAnimTarget>,
    mut roots: Query<&mut Transform, With<crate::visuals::ActorVisual>>,
) {
    let Some(terrain) = terrain else { return };
    for target in &targets {
        if target.playing() != Some(Pose::Fish) {
            continue;
        }
        let Ok(mut transform) = roots.get_mut(target.root()) else {
            continue;
        };
        let at = Vec2::new(transform.translation.x, transform.translation.z);
        if let Some(yaw) = water_yaw(at, |x, z| {
            terrain.cell(x, z).is_some_and(|cell| {
                cell.surface == testbed_content::village::frontier::terrain::TerrainSurface::Water
            })
        }) {
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    }
}

/// The turn that faces somebody standing at `at` towards `site` (world x,
/// z); `None` when they stand on it.
pub(crate) fn site_yaw(at: Vec2, site: Vec2) -> Option<f32> {
    let to = site - at;
    (to.length_squared() > 1e-4).then(|| to.x.atan2(to.y))
}

/// How near the owner's door a hired builder that stands still counts as
/// waiting between stages there, metres: the simulation starts work
/// anywhere within 4 m of the door.
pub(crate) const WAITING_REACH_M: f32 = 4.5;

/// Whether somebody standing at `at` waits at `door` (world x, z).
pub(crate) fn waits_at(at: Vec2, door: Vec2) -> bool {
    at.distance(door) <= WAITING_REACH_M
}

/// A builder or a helping friend at work on a homestead upgrade faces the
/// house: the simulation puts them at its door and says nothing of which
/// way they look, and a hammer swung at the open yard reads as idling. A
/// hired builder standing still at the door between stages ("waiting")
/// faces it too, standing calmly (no work pose: it is not at work). Runs
/// after the transforms are written; looks only at the workers and the
/// upgrades under way, never at everybody.
#[allow(clippy::type_complexity)]
pub(crate) fn face_site(
    index: Res<npc_framework::view_helpers::VisualIndex>,
    sims: Res<npc_framework::prelude::SimIndex>,
    clock: Option<Res<npc_framework::prelude::SimClock>>,
    live: Option<Res<testbed_content::village::live::LiveState>>,
    workers: Query<(
        Entity,
        &testbed_content::village::live::UpgradeWork,
        &npc_framework::prelude::SimPos,
    )>,
    still: Query<
        &npc_framework::prelude::SimPos,
        (
            Without<npc_framework::locomotion::MoveIntent>,
            Without<testbed_content::village::live::UpgradeWork>,
        ),
    >,
    homes: Query<(
        &testbed_content::village::colony::Household,
        &npc_framework::prelude::SimPos,
    )>,
    mut roots: Query<&mut Transform, With<crate::visuals::ActorVisual>>,
) {
    let mm = |p: &npc_framework::prelude::SimPos| Vec2::new(p.x as f32, p.z as f32) / 1000.0;
    let home_of = |owner: npc_framework::prelude::SimId| {
        homes
            .iter()
            .find(|(home, _)| home.residents.contains(&owner))
            .map(|(home, pos)| (mm(&home.door), mm(pos)))
    };
    // (actor, where it stands, the house it faces)
    let mut facing: Vec<(Entity, Vec2, Vec2)> = workers
        .iter()
        .filter_map(|(worker, work, at)| {
            home_of(work.owner).map(|(_, house)| (worker, mm(at), house))
        })
        .collect();
    if let (Some(live), Some(clock)) = (live, clock) {
        for (owner, upgrade) in &live.upgrades {
            let Some(builder) = upgrade.builder else {
                continue;
            };
            if upgrade.builder_until <= clock.tick {
                continue;
            }
            let (Some(actor), Some((door, house))) = (sims.get(builder), home_of(*owner)) else {
                continue;
            };
            if let Ok(at) = still.get(actor)
                && waits_at(mm(at), door)
            {
                facing.push((actor, mm(at), house));
            }
        }
    }
    for (actor, at, house) in facing {
        let Some(mut transform) = index
            .map
            .get(&actor)
            .and_then(|root| roots.get_mut(*root).ok())
        else {
            continue;
        };
        if let Some(yaw) = site_yaw(at, house) {
            let turn = Quat::from_rotation_y(yaw);
            if transform.rotation.angle_between(turn) > 1e-3 {
                transform.rotation = turn;
            }
        }
    }
}

/// The fishing line in a fisher's own frame (+Z ahead, the root at the
/// feet): from the rod's tip held out in front down to the water ahead,
/// `water` the water's height under the root. Returns the line's middle,
/// its turn (+Y along the line) and its length.
pub(crate) fn fishing_line(water: f32) -> (Vec3, Quat, f32) {
    let tip = Vec3::new(0.22, 2.15, 1.35);
    let float = Vec3::new(0.22, water, 2.9);
    let along = float - tip;
    (
        (tip + float) * 0.5,
        Quat::from_rotation_arc(Vec3::Y, along.normalize()),
        along.length(),
    )
}

/// The lines of the fishers now fishing, by visual root.
#[derive(Resource, Default)]
pub(crate) struct FishingLines(BTreeMap<Entity, Entity>);

/// A line from the rod into the water for whoever plays the fishing clip.
pub(crate) fn fishing_lines(
    mut commands: Commands,
    tools: Option<Res<ToolAssets>>,
    targets: Query<&crate::anim::ActorAnimTarget>,
    roots: Query<&GlobalTransform, With<crate::visuals::ActorVisual>>,
    mut lines: ResMut<FishingLines>,
    mut placed: Query<&mut Transform>,
) {
    let Some(tools) = tools else { return };
    let mut fishing = std::collections::BTreeSet::new();
    for target in &targets {
        if target.playing() != Some(Pose::Fish) {
            continue;
        }
        let root = target.root();
        let Ok(at) = roots.get(root) else { continue };
        fishing.insert(root);
        // The water's surface under the root, in the root's own height.
        let scale = at.compute_transform().scale.y.max(1e-3);
        let (middle, turn, length) = fishing_line((-0.11 - at.translation().y) / scale);
        let transform = Transform::from_translation(middle)
            .with_rotation(turn)
            .with_scale(Vec3::new(0.012, length, 0.012));
        match lines.0.get(&root) {
            Some(line) => {
                if let Ok(mut current) = placed.get_mut(*line) {
                    *current = transform;
                }
            }
            None => {
                let line = commands
                    .spawn((
                        Name::new("fishing line"),
                        ChildOf(root),
                        Mesh3d(tools.cube.clone()),
                        MeshMaterial3d(tools.iron.clone()),
                        transform,
                        bevy::light::NotShadowCaster,
                    ))
                    .id();
                lines.0.insert(root, line);
            }
        }
    }
    lines.0.retain(|root, line| {
        let keep = fishing.contains(root);
        if !keep {
            commands.entity(*line).try_despawn();
        }
        keep
    });
}

/// Whether the visual root itself is tipped over. Death always is. A rest is
/// tipped in legacy scenes; on the island only sleep on a rig whose clip
/// does not already lie down, so a rest that is not sleep sits upright.
pub(crate) fn lays_root(
    island: bool,
    kind: ModelKind,
    dead: bool,
    resting: bool,
    asleep: bool,
) -> bool {
    if dead {
        return true;
    }
    if !resting {
        return false;
    }
    if !island {
        return true;
    }
    asleep && !Pose::Sleep.clip_lies_down(kind)
}

#[derive(SystemParam)]
pub(crate) struct IslandPoseContext<'w, 's> {
    enabled: Option<Res<'w, IslandPoses>>,
    repose: Option<Res<'w, ReposeCache>>,
    clock: Option<Res<'w, SimClock>>,
    config: Option<Res<'w, SimConfig>>,
    index: Option<Res<'w, SimIndex>>,
    people: Query<
        'w,
        's,
        (
            &'static Villager,
            Option<&'static Inventory>,
            Has<CottageBuild>,
            Has<EcologyWork>,
            // A stage of work at a homestead upgrade, at its door.
            Has<testbed_content::village::live::UpgradeWork>,
        ),
    >,
    leaves: Query<'w, 's, (&'static ActiveLeaf, &'static Brain)>,
    registry: Option<Res<'w, AiRegistry>>,
    patches: Query<'w, 's, &'static ResourcePatch>,
    places: Query<'w, 's, &'static VillagePlace>,
    items: Query<'w, 's, &'static Item>,
}

impl IslandPoseContext<'_, '_> {
    pub(crate) fn pose(
        &self,
        actor: Entity,
        interacting: Option<&Interacting>,
        crafting: Option<&Crafting>,
        gathering: Option<&Gathering>,
        moving: bool,
    ) -> Option<Pose> {
        self.enabled.as_ref()?;
        let (clock, config, index) = (
            self.clock.as_ref()?,
            self.config.as_ref()?,
            self.index.as_ref()?,
        );
        let (person, inventory, building, restoring, upgrading) = self.people.get(actor).ok()?;
        let building = building || upgrading;
        let carrying = inventory.and_then(|inventory| {
            goods::bulk_load(inventory.items.iter().filter_map(|id| {
                let item = self.items.get(index.get(*id)?).ok()?;
                Some((item.kind.as_str(), item.stack))
            }))
        });
        let resting = person.resting_until > clock.tick;
        island_pose(Activity {
            resting,
            // Without the cache (a host that does not fill it) a rest at
            // night reads as sleep, as before.
            asleep: resting
                && self
                    .repose
                    .as_ref()
                    .and_then(|cache| cache.asleep(actor))
                    .unwrap_or_else(|| config.is_night(clock.tick)),
            moving,
            carrying,
            gathering: gathering
                .and_then(|g| index.get(g.patch))
                .and_then(|patch| self.patches.get(patch).ok())
                .map(|patch| patch.kind.as_str()),
            verb: interacting.map(|i| i.verb.as_str()),
            crafting: crafting.is_some(),
            station: crafting
                .and_then(|craft| craft.workstation)
                .and_then(|station| index.get(station))
                .and_then(|station| self.places.get(station).ok())
                .map(|place| place.role),
            building,
            restoring,
            meeting: self
                .leaves
                .get(actor)
                .ok()
                .zip(self.registry.as_ref())
                .is_some_and(|((leaf, brain), registry)| {
                    matches!(
                        registry
                            .tree(brain.tree)
                            .nodes
                            .get(leaf.node as usize)
                            .map(|node| &node.kind),
                        Some(NodeKind::Leaf { action, .. }) if action.name == "life.socialise"
                    )
                }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_only_trade_works_with_the_generic_hammer() {
        let crafting = Activity {
            crafting: true,
            station: Some(Role::from_key("weaver")),
            ..Activity::default()
        };
        assert_eq!(island_pose(crafting), Some(Pose::Hammer));
    }

    fn at_rest(asleep: bool) -> Activity<'static> {
        Activity {
            resting: true,
            asleep,
            ..default()
        }
    }

    #[test]
    fn the_cache_says_who_is_asleep_and_nothing_about_strangers() {
        let mut cache = ReposeCache::default();
        let (a, b) = (
            Entity::from_bits(1 << 32 | 1),
            Entity::from_bits(1 << 32 | 2),
        );
        assert_eq!(cache.asleep(a), None);
        cache.asleep.insert(a, true);
        cache.asleep.insert(b, false);
        assert_eq!(
            (cache.asleep(a), cache.asleep(b)),
            (Some(true), Some(false))
        );
    }

    #[test]
    fn the_hand_holds_what_the_simulation_says() {
        // No word from the simulation: the pose's own tool.
        assert_eq!(drawn_tool(Pose::Chop, None), Some(Tool::Axe));
        // Bare-handed work draws no tool.
        assert_eq!(drawn_tool(Pose::Chop, Some(("axe", false))), None);
        assert_eq!(
            drawn_tool(Pose::Fish, Some(("village_fishing_rod", true))),
            Some(Tool::Rod)
        );
        assert_eq!(
            drawn_tool(Pose::Farm, Some(("village_hoe", true))),
            Some(Tool::Hoe)
        );
        assert_eq!(
            drawn_tool(Pose::Hammer, Some(("village_tool", true))),
            Some(Tool::Hammer)
        );
        // A pose without a tool never gets one.
        assert_eq!(drawn_tool(Pose::Walk, Some(("axe", true))), None);
    }

    #[test]
    fn a_builder_at_the_door_faces_the_house() {
        // The house's middle 3 m to +Z of the door: the builder faces +Z.
        let yaw = site_yaw(Vec2::new(10.0, 4.0), Vec2::new(10.0, 7.0)).unwrap();
        let facing = Quat::from_rotation_y(yaw) * Vec3::Z;
        assert!((facing - Vec3::Z).length() < 1e-4, "{facing}");
        // And to -X when the house is that way.
        let yaw = site_yaw(Vec2::new(10.0, 4.0), Vec2::new(6.0, 4.0)).unwrap();
        let facing = Quat::from_rotation_y(yaw) * Vec3::Z;
        assert!((facing - Vec3::NEG_X).length() < 1e-4, "{facing}");
        assert_eq!(site_yaw(Vec2::ONE, Vec2::ONE), None);
    }

    #[test]
    fn a_hired_builder_standing_by_the_door_waits_there() {
        let door = Vec2::new(40.0, -50.0);
        assert!(
            waits_at(Vec2::new(40.5, -46.4), door),
            "within 4 m of the door"
        );
        assert!(
            !waits_at(Vec2::new(46.6, -49.6), door),
            "6.6 m off: not at the door"
        );
        assert!(!waits_at(Vec2::new(8.9, -25.9), door), "at its own home");
    }

    #[test]
    fn a_fisher_faces_the_nearest_water() {
        // Water for x < 0: standing at x 0.5 the fisher faces -X.
        let yaw = water_yaw(Vec2::new(0.5, 3.5), |x, _| x < 0).expect("water");
        let facing = Vec2::new(yaw.sin(), yaw.cos());
        assert!((facing - Vec2::NEG_X).length() < 1e-4, "{facing}");
        // Water for z > 5: faces +Z.
        let yaw = water_yaw(Vec2::new(2.5, 4.5), |_, z| z > 4).expect("water");
        assert!((Vec2::new(yaw.sin(), yaw.cos()) - Vec2::Y).length() < 1e-4);
        assert_eq!(water_yaw(Vec2::new(0.5, 0.5), |_, _| false), None);
    }

    #[test]
    fn sleep_lies_down_and_any_other_rest_sits_before_anything_else() {
        assert_eq!(island_pose(at_rest(true)), Some(Pose::Sleep));
        assert_eq!(island_pose(at_rest(false)), Some(Pose::Sit));
        let busy = Activity {
            gathering: Some("log"),
            crafting: true,
            ..at_rest(true)
        };
        assert_eq!(island_pose(busy), Some(Pose::Sleep));
    }

    #[test]
    fn leisure_greets_a_neighbour_and_otherwise_keeps_the_plain_stance() {
        let meeting = Activity {
            meeting: true,
            ..default()
        };
        assert_eq!(island_pose(meeting), Some(Pose::Interact));
        // Walking over to them is a walk; a rest or real work comes first.
        assert_eq!(
            island_pose(Activity {
                moving: true,
                ..meeting
            }),
            None
        );
        assert_eq!(
            island_pose(Activity {
                resting: true,
                ..meeting
            }),
            Some(Pose::Sit)
        );
        // A stroll and standing idle have no pose of their own.
        assert_eq!(island_pose(Activity::default()), None);
    }

    #[test]
    fn each_trade_has_its_own_motion() {
        let gather = |kind| {
            island_pose(Activity {
                gathering: Some(kind),
                ..default()
            })
        };
        assert_eq!(gather("log"), Some(Pose::Chop));
        assert_eq!(gather(catalog::FISH), Some(Pose::Fish));
        assert_eq!(gather(catalog::GRAIN), Some(Pose::Farm));
        assert_eq!(gather(catalog::BERRIES), Some(Pose::Farm));
        assert_eq!(gather(catalog::WATER), Some(Pose::PickUp));
        assert_eq!(gather("ore"), Some(Pose::Work));
        let craft = |station| {
            island_pose(Activity {
                crafting: true,
                station,
                ..default()
            })
        };
        assert_eq!(craft(Some(Role::Smith)), Some(Pose::Hammer));
        assert_eq!(craft(Some(Role::Carpenter)), Some(Pose::Hammer));
        // Baking, milling and unknown stations keep the generic craft clip.
        assert_eq!(craft(Some(Role::Baker)), None);
        assert_eq!(craft(None), None);
        let verb = |verb| {
            island_pose(Activity {
                verb: Some(verb),
                ..default()
            })
        };
        assert_eq!(verb("harvest"), Some(Pose::Farm));
        assert_eq!(verb("plant"), Some(Pose::Farm));
        assert_eq!(verb("open"), None);
        assert_eq!(
            island_pose(Activity {
                building: true,
                ..default()
            }),
            Some(Pose::Hammer)
        );
        assert_eq!(
            island_pose(Activity {
                restoring: true,
                ..default()
            }),
            Some(Pose::Farm)
        );
    }

    #[test]
    fn every_work_pose_holds_its_own_tool_and_nothing_else_does() {
        assert_eq!(Tool::of(Pose::Chop), Some(Tool::Axe));
        assert_eq!(Tool::of(Pose::Farm), Some(Tool::Hoe));
        assert_eq!(Tool::of(Pose::Fish), Some(Tool::Rod));
        assert_eq!(Tool::of(Pose::Hammer), Some(Tool::Hammer));
        for pose in [
            Pose::Idle,
            Pose::Walk,
            Pose::Sleep,
            Pose::Sit,
            Pose::Carry,
            Pose::Interact,
        ] {
            assert_eq!(Tool::of(pose), None, "{pose:?}");
        }
        // The code-built ones are a resident's size: a hoe about 1.4 m, a rod
        // about 2 m, in hand-slot units.
        let reach = |tool: Tool| {
            tool.boxes()
                .expect("built")
                .iter()
                .map(|(centre, size, _)| (centre.y + size.y * 0.5) * crate::visuals::V)
                .fold(0.0, f32::max)
        };
        assert!(
            (1.0..1.5).contains(&reach(Tool::Hoe)),
            "{}",
            reach(Tool::Hoe)
        );
        assert!(
            (1.8..2.3).contains(&reach(Tool::Rod)),
            "{}",
            reach(Tool::Rod)
        );
        assert_eq!(Tool::Axe.boxes(), None);
    }

    #[test]
    fn walking_carries_only_with_a_bulk_load() {
        let walking = Activity {
            moving: true,
            gathering: Some("log"),
            ..default()
        };
        assert_eq!(island_pose(walking), None, "plain walk stays generic");
        assert_eq!(
            island_pose(Activity {
                carrying: Some(CarriedGood::Lumber),
                ..walking
            }),
            Some(Pose::Carry)
        );
        // Standing still with goods is not a carry walk.
        assert_eq!(
            island_pose(Activity {
                carrying: Some(CarriedGood::Sack),
                ..default()
            }),
            None
        );
    }

    #[test]
    fn only_island_sleep_on_a_lying_clip_stays_upright_at_the_root() {
        use ModelKind::{Barbarian, VillageWoman};
        // Legacy scenes: unchanged, every rest and death tips the root.
        assert!(lays_root(false, Barbarian, false, true, false));
        assert!(lays_root(false, VillageWoman, false, true, true));
        assert!(lays_root(false, Barbarian, true, false, false));
        assert!(!lays_root(false, Barbarian, false, false, true));
        // Island: KayKit sleeps through its own lying clip, civilians are laid
        // down, a rest that is not sleep sits, the dead still lie.
        assert!(!lays_root(true, Barbarian, false, true, true));
        assert!(lays_root(true, VillageWoman, false, true, true));
        assert!(!lays_root(true, Barbarian, false, true, false));
        assert!(!lays_root(true, VillageWoman, false, true, false));
        assert!(lays_root(true, Barbarian, true, true, true));
    }
}
