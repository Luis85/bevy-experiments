//! Visual entities for the island's residents and loose items. Each sim
//! entity with an `Item` or an `IslandUnit` gets its own visual root carrying
//! `VisualOf(sim)`; nothing is ever inserted on the sim entity itself. (The
//! island spawns no `Prop`s, walls, doors, trees, ore rocks or vehicles of the
//! older testbed layout; their visuals are gone.)
//!
//! Scale: every KayKit character is brought to 1:1 and scaled by the view
//! factor [`V`] = 0.75 (a Knight is 1.74 m).

use std::f32::consts::PI;

use bevy::prelude::*;
use island_content::content::{AXE, BREAD, HAMMER, INGOT, LOG, ORE, PICKAXE, PLANK};
use island_content::{IslandUnit, Prop};
use npc_framework::interaction::items::{MAIN_HAND, OFF_HAND};
use npc_framework::perception::Sensors;
use npc_framework::prelude::{
    CarriedBy, Item, Owner, PrevSimPos, SimClock, SimControl, SimId, SimIndex, SimPos, SimRate,
    SimTime,
};
use npc_framework::view_helpers::interp::interp_alpha;
use npc_framework::view_helpers::{Selectable, ViewSystems, VisualIndex, VisualOf, sim_to_world};

use crate::assets::{ModelKind, Models, ViewAssets};
use crate::fog::{FogDim, FogSet, Fogged, Look};
use crate::island_world::cast::{self, Cast};

/// The view factor: KayKit 1:1 model units → metres.
pub const V: f32 = 0.75;
pub(crate) const KENNEY_TOOL: f32 = 4.7;
/// Yaw smoothing rate (1/s).
const TURN_RATE: f32 = 10.0;

/// An item: where its model is and what holds it.
#[derive(Component, Clone, Debug)]
pub(crate) struct ItemVisual {
    model: Entity,
    look: ItemLook,
    /// The hand bone it is attached to.
    attached: Option<Entity>,
}

/// Interpolation and facing state of a moving visual.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Motion {
    /// Current (smoothed) yaw, radians; model +Z faces this way.
    pub yaw: f32,
    from: Vec3,
    to: Vec3,
    tick: Option<u64>,
    /// Whether the sim entity moved during the latest tick.
    pub moving: bool,
}

impl Motion {
    fn at(p: Vec3) -> Self {
        Self {
            from: p,
            to: p,
            ..default()
        }
    }

    /// The interpolated position for this frame (see the village view: at
    /// 1× exactly `PrevSimPos → SimPos`, continuous at higher speeds).
    fn step(&mut self, tick: Option<u64>, prev: Vec3, now: Vec3, alpha: f32) -> Vec3 {
        match (tick, self.tick) {
            (Some(t), Some(seen)) if t == seen => {}
            (Some(t), Some(seen)) if t == seen + 1 => {
                self.from = prev;
                self.to = now;
            }
            (Some(_), Some(_)) => {
                self.from = self.to;
                self.to = now;
            }
            _ => {
                self.from = prev;
                self.to = now;
            }
        }
        self.tick = tick;
        if self.to != now {
            self.from = prev;
            self.to = now;
        }
        self.moving = (self.to - self.from).length_squared() > 1e-8;
        self.from.lerp(self.to, alpha)
    }

    fn turn_towards(&mut self, target: f32, t: f32) {
        self.yaw = lerp_angle(self.yaw, target, t);
    }

    fn heading(&self) -> Option<f32> {
        let d = self.to - self.from;
        (d.length_squared() > 1e-8).then(|| d.x.atan2(d.z))
    }
}

/// An actor's visual root.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ActorVisual {
    pub kind: ModelKind,
    pub motion: Motion,
    pub hand_r: Option<Entity>,
    pub hand_l: Option<Entity>,
}

/// On an actor's model (the `WorldAssetRoot` child of its root).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ActorModel {
    pub root: Entity,
}

/// Faction ring mesh and materials.
#[derive(Resource, Clone)]
pub(crate) struct Rings {
    mesh: Handle<Mesh>,
    owner: [Handle<StandardMaterial>; 3],
    /// The played character's ring: wider and warm, the only thing that
    /// marks their body apart from a resident's.
    player_mesh: Handle<Mesh>,
    player: Handle<StandardMaterial>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, make_rings)
        .add_systems(
            Update,
            (despawn_visuals, spawn_visuals)
                .chain()
                .in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (actor_transforms, item_transforms)
                .chain()
                .after(FogSet)
                .in_set(ViewSystems::Animate),
        );
}

fn make_rings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat = |c: Color| {
        materials.add(StandardMaterial {
            base_color: c,
            unlit: true,
            ..default()
        })
    };
    commands.insert_resource(Rings {
        mesh: meshes.add(Annulus::new(0.42, 0.52)),
        owner: [
            mat(Color::srgb(0.75, 0.75, 0.75)),
            mat(Color::srgb(0.2, 0.5, 1.0)),
            mat(Color::srgb(0.95, 0.18, 0.15)),
        ],
        player_mesh: meshes.add(Annulus::new(0.5, 0.68)),
        player: mat(Color::srgb(1.0, 0.78, 0.22)),
    });
}

/// The body the played character wears: the adventurer Knight, which no
/// resident wears, unless its preset names a body.
pub(crate) fn player_kind(kind: ModelKind, played: bool, named: bool) -> ModelKind {
    if played && !named {
        ModelKind::Knight
    } else {
        kind
    }
}

/// Ring colour index of an owner (0 = none/other, 1, 2).
pub(crate) fn owner_slot(owner: Option<&Owner>) -> usize {
    match owner.map(|o| o.0.0) {
        Some(1) => 1,
        Some(2) => 2,
        _ => 0,
    }
}

fn model(scene: &Handle<bevy::world_serialization::WorldAsset>, t: Transform) -> impl Bundle {
    (WorldAssetRoot(scene.clone()), t)
}

#[allow(clippy::too_many_arguments)]
fn spawn_visuals(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    rings: Option<Res<Rings>>,
    (village, regional): (
        Option<Res<island_content::village::VillageState>>,
        Option<Res<island_content::village::frontier::terrain::TerrainState>>,
    ),
    mut index: ResMut<VisualIndex>,
    alive: Query<(), With<VisualOf>>,
    items: Query<(Entity, &Item, Option<&SimPos>), (Without<Prop>, Without<IslandUnit>)>,
    actors: Query<
        (
            Entity,
            &IslandUnit,
            &SimPos,
            Option<&Owner>,
            Option<&island_content::actors::ActorModel>,
            Option<&Sensors>,
            Option<&island_content::village::frontier::labor::ResidentProfile>,
            Option<&SimId>,
        ),
        Without<Prop>,
    >,
) {
    let (Some(assets), Some(rings)) = (assets, rings) else {
        return;
    };
    let m = &assets.models;
    let fresh = |index: &mut VisualIndex, sim: Entity| match index.map.get(&sim) {
        Some(v) if alive.contains(*v) => false,
        _ => {
            index.map.remove(&sim);
            true
        }
    };

    for (sim, item, pos) in &items {
        if !fresh(&mut index, sim) {
            continue;
        }
        let look = ItemLook::of(&item.kind);
        let root = commands
            .spawn((
                Name::new(format!("visual item {}", item.kind)),
                VisualOf(sim),
                Selectable,
                Transform::from_translation(pos.copied().map(sim_to_world).unwrap_or_default()),
                // Carried/remembered items have no ground position. The
                // animation pass decides their display before rendering.
                if pos.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        let model_id = commands
            .spawn((ChildOf(root), model(look.scene(m), look.lying())))
            .id();
        commands.entity(root).insert(ItemVisual {
            model: model_id,
            look,
            attached: None,
        });
        index.map.insert(sim, root);
    }

    // Only on the regional island: elsewhere a played robot stays a robot.
    let played = village
        .as_deref()
        .filter(|_| regional.is_some())
        .map(|state| state.played);
    for (sim, unit, pos, owner, preset_model, _sensors, profile, id) in &actors {
        if !fresh(&mut index, sim) {
            continue;
        }
        let is_played = id.is_some() && id.copied() == played;
        // On the regional island one family of bodies (`island_world::cast`)
        // dresses residents, children and the played character.
        let cast = regional.is_some().then(Cast::current);
        let (resident_kind, outfit) = match (profile, cast) {
            (Some(profile), Some(cast)) => {
                let (kind, outfit) =
                    cast::resident(cast, id.map_or(0, |id| id.0), profile.appearance);
                (Some(kind), outfit)
            }
            (Some(profile), None) => (Some(ModelKind::for_resident(profile.appearance)), None),
            (None, _) => (None, None),
        };
        let kind = resident_kind
            .or_else(|| preset_model.and_then(|m| ModelKind::from_name(&m.0)))
            .unwrap_or_else(|| ModelKind::for_actor(&unit.template, owner.map(|o| o.0.0)));
        // The island's played character has no resident profile: unless its
        // preset names a body, it wears the Knight, which no resident wears,
        // so it reads apart from the hooded rogues from above; poses, the
        // door step, the cutaway and unblocking treat it as a resident.
        let kind = player_kind(
            kind,
            is_played && profile.is_none(),
            preset_model.is_some_and(|m| ModelKind::from_name(&m.0).is_some()),
        );
        let kind = match cast {
            Some(cast) if is_played && profile.is_none() => cast::player(cast, kind),
            _ => kind,
        };
        let at = sim_to_world(*pos);
        let root = commands
            .spawn((
                Name::new(format!("visual actor {}", unit.template)),
                VisualOf(sim),
                Selectable,
                ActorVisual {
                    kind,
                    motion: Motion::at(at),
                    hand_r: None,
                    hand_l: None,
                },
                Transform::from_translation(at),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        {
            let scale = cast.map_or(kind.scale(), |cast| cast::adult_scale(cast, kind));
            let mut body = commands.spawn((
                ChildOf(root),
                ActorModel { root },
                model(
                    &assets.actor[kind.index()],
                    Transform::from_scale(Vec3::splat(scale)),
                ),
            ));
            body.observe(crate::anim::on_actor_ready);
            if cast.is_some() {
                body.insert(cast::IslandBody { adult: scale });
            }
            if let Some(outfit) = outfit {
                body.insert(outfit).observe(cast::on_outfit_ready);
            }
        }
        let (ring_name, ring_mesh, ring_material) = if is_played {
            ("player ring", &rings.player_mesh, &rings.player)
        } else {
            ("faction ring", &rings.mesh, &rings.owner[owner_slot(owner)])
        };
        commands.spawn((
            ChildOf(root),
            Name::new(ring_name),
            Mesh3d(ring_mesh.clone()),
            MeshMaterial3d(ring_material.clone()),
            Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-PI / 2.0)),
        ));
        index.map.insert(sim, root);
    }
}

/// Despawns visuals whose sim entity is gone.
type VisualSimKinds = Or<(
    With<Item>,
    With<IslandUnit>,
    With<island_content::village::VillagePlace>,
    With<island_content::village::ResourcePatch>,
    With<island_content::village::colony::Household>,
    With<island_content::village::hunting::Wildlife>,
    With<island_content::village::homestead::HomesteadPlot>,
    With<island_content::village::homestead::Cottage>,
    With<island_content::village::civic::ConstructionSite>,
    With<island_content::village::civic::Building>,
)>;
pub(crate) fn despawn_visuals(
    mut commands: Commands,
    mut index: ResMut<VisualIndex>,
    sims: Query<(), VisualSimKinds>,
    visuals: Query<(), With<VisualOf>>,
) {
    index.map.retain(|&sim, &mut visual| {
        if sims.contains(sim) {
            return true;
        }
        if visuals.contains(visual) {
            commands.entity(visual).despawn();
        }
        false
    });
}

pub(crate) fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let mut d = (to - from) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    } else if d < -PI {
        d += 2.0 * PI;
    }
    from + d * t
}

type MotionInputs<'w, 's> = (
    Res<'w, Time>,
    Option<Res<'w, SimTime>>,
    Option<Res<'w, SimRate>>,
    Option<Res<'w, SimControl>>,
    Option<Res<'w, SimClock>>,
);

fn frame_params(inputs: &MotionInputs) -> (f32, Option<u64>, f32) {
    let (time, sim_time, rate, control, clock) = inputs;
    let alpha = interp_alpha(sim_time.as_deref(), rate.as_deref(), control.as_deref());
    let turn = 1.0 - (-TURN_RATE * time.delta_secs()).exp();
    (alpha, clock.as_ref().map(|c| c.tick), turn)
}

#[allow(clippy::type_complexity)]
fn actor_transforms(
    inputs: MotionInputs,
    sims: Query<(
        &SimPos,
        Option<&PrevSimPos>,
        Option<&npc_framework::prelude::Facing>,
    )>,
    mut visuals: Query<(&VisualOf, &mut Transform, &mut ActorVisual)>,
) {
    let (alpha, tick, turn) = frame_params(&inputs);
    for (of, mut transform, mut vis) in &mut visuals {
        let Ok((pos, prev, facing)) = sims.get(of.0) else {
            continue;
        };
        let now = sim_to_world(*pos);
        let prev = prev.map_or(now, |p| sim_to_world(p.0));
        let at = vis.motion.step(tick, prev, now, alpha);

        let target = vis.motion.heading().or_else(|| {
            facing
                .filter(|f| f.x != 0 || f.z != 0)
                .map(|f| (f.x as f32).atan2(f.z as f32))
        });
        if let Some(t) = target {
            vis.motion.turn_towards(t, turn);
        }
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(vis.motion.yaw);
    }
}

/// How an item looks: model, size, how it lies on the ground and how a hand
/// holds it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ItemLook {
    model: fn(&Models) -> &Handle<bevy::world_serialization::WorldAsset>,
    /// World scale (metres per model unit).
    scale: f32,
    /// Rotation when lying, and the lift that puts it on the ground (model
    /// units).
    lie: Quat,
    lift: f32,
    /// Rotation and offset (hand-bone units) when held, if it can be held.
    hold: Option<(Quat, Vec3)>,
}

impl ItemLook {
    fn of(kind: &str) -> Self {
        let flat_x = Quat::from_rotation_x(PI / 2.0);
        let upright = Quat::IDENTITY;
        let (model, scale, lie, lift, hold): (
            fn(&Models) -> &Handle<bevy::world_serialization::WorldAsset>,
            f32,
            Quat,
            f32,
            Option<(Quat, Vec3)>,
        ) = match kind {
            AXE => (
                |m| &m.axe,
                V,
                flat_x,
                0.1,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, 0.03, 0.0))),
            ),
            PICKAXE => (
                |m| &m.pickaxe,
                KENNEY_TOOL * V,
                flat_x,
                0.03,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, -0.25, 0.0))),
            ),
            HAMMER => (
                |m| &m.hammer,
                KENNEY_TOOL * V,
                flat_x,
                0.03,
                Some((Quat::from_rotation_y(PI), Vec3::new(0.0, -0.18, 0.0))),
            ),
            island_content::village::catalog::DIARY => {
                (|m| &m.book, V, Quat::from_rotation_z(PI / 2.0), 0.13, None)
            }
            BREAD | island_content::village::catalog::BOWL => {
                (|m| &m.food, 0.5 * V, upright, 0.0, None)
            }
            island_content::village::catalog::WATER => (|m| &m.barrel, 0.4 * V, upright, 0.0, None),
            island_content::village::catalog::COINS => (|m| &m.ingot, 0.5 * V, upright, 0.0, None),
            LOG => (|m| &m.log, 2.0 * V, upright, 0.11, None),
            PLANK => (|m| &m.planks, 2.0 * V, upright, 0.0, None),
            ORE => (|m| &m.stone, 1.4 * V, upright, 0.0, None),
            INGOT => (|m| &m.ingot, 0.6 * V, upright, 0.0, None),
            _ => (|m| &m.crate_large, 0.25 * V, upright, 0.0, None),
        };
        Self {
            model,
            scale,
            lie,
            lift,
            hold,
        }
    }

    fn scene<'a>(&self, m: &'a Models) -> &'a Handle<bevy::world_serialization::WorldAsset> {
        (self.model)(m)
    }

    /// The model's transform under a root standing on the ground.
    fn lying(&self) -> Transform {
        Transform::from_xyz(0.0, self.lift * self.scale, 0.0)
            .with_rotation(self.lie)
            .with_scale(Vec3::splat(self.scale))
    }

    /// The model's transform under a hand bone (which already carries the
    /// actor's scale `V`).
    fn held(&self) -> Option<Transform> {
        self.hold.map(|(r, t)| {
            Transform::from_translation(t)
                .with_rotation(r)
                .with_scale(Vec3::splat(self.scale / V))
        })
    }
}

/// Items in the world lie at their `SimPos`; an equipped tool rides in its
/// holder's hand; anything else carried is hidden. Under fog an item lying
/// in ground not visible now lies where it was last seen, or is hidden.
#[allow(clippy::type_complexity)]
fn item_transforms(
    mut commands: Commands,
    sims: Query<(Option<&SimPos>, Option<&CarriedBy>), With<Item>>,
    index: Option<Res<SimIndex>>,
    vis_index: Res<VisualIndex>,
    actors: Query<&ActorVisual>,
    mut items: Query<
        (
            Entity,
            &VisualOf,
            &mut ItemVisual,
            &mut Transform,
            &mut Visibility,
            Has<Selectable>,
            Option<&Fogged>,
        ),
        Without<ActorVisual>,
    >,
) {
    for (root, of, mut item, mut transform, mut visibility, selectable, fogged) in &mut items {
        let Ok((pos, carried)) = sims.get(of.0) else {
            continue;
        };
        let look = fogged.map_or(Look::Live, |f| f.0);
        // The fog's last-seen object is independent of its real ownership:
        // an unseen pickup must not reveal that the ground is now empty.
        let ground = match look {
            Look::LastSeen(state) => Some(state.pos),
            Look::Live | Look::Dim if carried.is_none() => pos.copied(),
            _ => None,
        };
        let hand = carried
            .filter(|_| matches!(look, Look::Live | Look::Dim))
            .and_then(|c| {
                let slot = c.slot.as_deref()?;
                let holder = index.as_deref()?.get(c.holder)?;
                let actor = actors.get(*vis_index.map.get(&holder)?).ok()?;
                match slot {
                    MAIN_HAND => actor.hand_r,
                    OFF_HAND => actor.hand_l,
                    _ => None,
                }
            });
        let held = hand.zip(item.look.held());
        if let Some((hand, held)) = held {
            if item.attached != Some(hand) {
                item.attached = Some(hand);
                commands.entity(root).insert(ChildOf(hand));
                *transform = Transform::IDENTITY;
                commands.entity(item.model).insert(held);
            }
            if selectable {
                commands.entity(root).remove::<Selectable>();
            }
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            if item.attached.take().is_some() {
                commands.entity(root).remove::<ChildOf>();
                commands.entity(item.model).insert(item.look.lying());
            }
            if let Some(at) = ground {
                if !selectable {
                    commands.entity(root).insert(Selectable);
                }
                transform.set_if_neq(Transform::from_translation(sim_to_world(at)));
                visibility.set_if_neq(Visibility::Inherited);
            } else {
                if selectable {
                    commands.entity(root).remove::<Selectable>();
                }
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn the_played_character_wears_the_knight_unless_its_preset_names_a_body() {
        use ModelKind::{Knight, Rogue, TravellerMan};
        // Played, no named body: the Knight, whatever the faction default.
        assert_eq!(player_kind(Rogue, true, false), Knight);
        // A preset that names a body keeps it.
        assert_eq!(player_kind(TravellerMan, true, true), TravellerMan);
        // Nobody else changes.
        assert_eq!(player_kind(Rogue, false, false), Rogue);
        assert!(
            (0..8)
                .map(ModelKind::for_resident)
                .all(|kind| kind != Knight)
        );
    }

    #[test]
    fn settlement_house_visual_survives_cleanup_until_its_household_is_removed() {
        let mut app = App::new();
        app.init_resource::<VisualIndex>()
            .add_systems(Update, despawn_visuals);
        let home = app
            .world_mut()
            .spawn(island_content::village::colony::Household {
                label: "Household".into(),
                capacity: 2,
                residents: vec![],
                door: SimPos::new(0, 0),
                heated_until: 0,
                logs_burned: 0,
            })
            .id();
        let visual = app.world_mut().spawn(VisualOf(home)).id();
        app.world_mut()
            .resource_mut::<VisualIndex>()
            .map
            .insert(home, visual);
        app.update();
        app.update();
        assert!(app.world().get_entity(visual).is_ok());
        assert_eq!(
            app.world().resource::<VisualIndex>().map.get(&home),
            Some(&visual)
        );
        app.world_mut().despawn(home);
        app.update();
        assert!(app.world().get_entity(visual).is_err());
    }

    use npc_framework::interaction::items::{place_in_world, put_in_inventory, put_in_slot};
    use npc_framework::prelude::{Equipment, Inventory, spawn_sim};

    // Render-system fixtures have handles, not loaded models or a GPU. The
    // holder's hand/grip is a view entity; item ownership uses the real API.
    fn install_item_view(app: &mut App) {
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .init_resource::<VisualIndex>()
            .init_resource::<SimClock>();
        macro_rules! models {($($name:ident),* $(,)?)=>{Models{$($name:default(),)*}};}
        app.insert_resource(ViewAssets {
            actor_gltf: vec![default(); ModelKind::ANIMATED.len()],
            actor: vec![default(); ModelKind::ANIMATED.len()],
            grass: default(),
            models: models!(
                chest,
                food,
                ingot,
                barrel,
                crate_large,
                stone,
                lumber,
                book,
                axe,
                workbench,
                anvil,
                pickaxe,
                hammer,
                log,
                planks,
            ),
        });
        app.insert_resource(Rings {
            mesh: default(),
            owner: default(),
            player_mesh: default(),
            player: default(),
        });
        app.add_systems(
            Update,
            (spawn_visuals.before(FogSet), item_transforms.after(FogSet)).chain(),
        );
    }
    fn item_view_app() -> App {
        let mut app = App::new();
        install_item_view(&mut app);
        app
    }
    fn visual_holder(app: &mut App, holder: Entity, kind: ModelKind) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                VisualOf(holder),
                Transform::default(),
                Visibility::Inherited,
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        let grip = app
            .world_mut()
            .spawn((
                Name::new("human hand bone"),
                ChildOf(root),
                Transform::default(),
            ))
            .id();
        app.world_mut().entity_mut(root).insert(ActorVisual {
            kind,
            motion: Motion::default(),
            hand_r: Some(grip),
            hand_l: None,
        });
        app.world_mut()
            .resource_mut::<VisualIndex>()
            .map
            .insert(holder, root);
        grip
    }
    fn carried_fixture(kind: ModelKind) -> (App, Entity, Entity, Entity) {
        let mut app = item_view_app();
        let (_, holder) = spawn_sim(
            app.world_mut(),
            (
                SimPos::new(0, 0),
                Inventory::new(20, 100000),
                Equipment::hands(),
            ),
        );
        let grip = visual_holder(&mut app, holder, kind);
        let (_, item) = spawn_sim(app.world_mut(), (SimPos::new(1000, 0), Item::new(AXE)));
        (app, holder, item, grip)
    }
    fn item_root(app: &App, item: Entity) -> Entity {
        app.world()
            .resource::<VisualIndex>()
            .map
            .get(&item)
            .copied()
            .expect("every existing ground or carried item needs a visual root")
    }
    fn assert_carried_has_no_ground_position(app: &App, item: Entity) {
        assert!(app.world().get::<SimPos>(item).is_none());
        assert!(app.world().get::<CarriedBy>(item).is_some());
    }

    #[test]
    fn real_pickup_hides_inventory_item_and_drop_restores_ground_visual() {
        let (mut app, holder, item, _) = carried_fixture(ModelKind::Rogue);
        app.update();
        let root = item_root(&app, item);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
        put_in_inventory(app.world_mut(), holder, item);
        assert_carried_has_no_ground_position(&app, item);
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden),
            "a real pickup removes SimPos; it must still hide the old ground visual"
        );
        assert!(app.world().get::<Selectable>(root).is_none());
        place_in_world(app.world_mut(), item, SimPos::new(4000, 2000));
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
        assert_eq!(
            app.world().get::<Transform>(root).unwrap().translation,
            Vec3::new(4.0, 0.0, 2.0)
        );
        assert!(app.world().get::<Selectable>(root).is_some());
    }
    #[test]
    fn real_equipment_attaches_to_human_grips() {
        for kind in [ModelKind::Rogue, ModelKind::FarmerMan] {
            let (mut app, holder, item, grip) = carried_fixture(kind);
            app.update();
            let root = item_root(&app, item);
            put_in_inventory(app.world_mut(), holder, item);
            put_in_slot(app.world_mut(), holder, item, MAIN_HAND);
            assert_carried_has_no_ground_position(&app, item);
            app.update();
            assert_eq!(
                app.world().get::<ChildOf>(root).map(ChildOf::parent),
                Some(grip),
                "{kind:?}: equipped tool must be parented to its actual view hand/grip"
            );
            assert_eq!(
                app.world().get::<Visibility>(root),
                Some(&Visibility::Inherited)
            );
            assert_eq!(
                app.world().get::<Transform>(root),
                Some(&Transform::IDENTITY)
            );
            assert!(app.world().get::<Selectable>(root).is_none());
            put_in_inventory(app.world_mut(), holder, item);
            app.update();
            assert_eq!(
                app.world().get::<Visibility>(root),
                Some(&Visibility::Hidden)
            );
            assert!(app.world().get::<ChildOf>(root).is_none());
        }
    }
    #[test]
    fn models_follow_the_template() {
        assert_eq!(ModelKind::for_actor("smith", Some(1)), ModelKind::Knight);
        assert_eq!(ModelKind::for_actor("scout", Some(1)), ModelKind::Rogue);
        // The testbed's skeleton templates have no model of their own here.
        assert_eq!(
            ModelKind::for_actor("skeleton_minion", Some(2)),
            ModelKind::Rogue
        );
    }

    #[test]
    fn every_item_kind_has_a_look() {
        for kind in [AXE, PICKAXE, HAMMER, LOG, PLANK, ORE, INGOT, BREAD] {
            let look = ItemLook::of(kind);
            assert!(look.scale > 0.0, "{kind}");
            assert!(look.lying().translation.y >= 0.0, "{kind}");
        }
        // Tools can be held; the rest is hidden while carried.
        assert!(ItemLook::of(AXE).held().is_some());
        assert!(ItemLook::of(PICKAXE).held().is_some());
        assert!(
            ItemLook::of(island_content::village::catalog::DIARY)
                .held()
                .is_none()
        );
    }

    #[test]
    fn angles_take_the_short_way_round() {
        let a = lerp_angle(3.0, -3.0, 0.5);
        assert!(a > 3.0, "{a}");
        assert_eq!(lerp_angle(0.0, 1.0, 1.0), 1.0);
    }
}
