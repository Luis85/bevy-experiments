//! Native homestead presentation. Only visual entities, caches and visibility change here.
use super::{StaticId, label};
use crate::fog::{FogDim, FogOfWar, FogSet, Fogged, Look};
use bevy::{ecs::system::SystemParam, prelude::*, world_serialization::WorldAsset};
use homestead::{Cottage, HomesteadPlot};
use npc_framework::agent_io::api::AgentIoState;
use npc_framework::perception::PerceptionMemory;
use npc_framework::prelude::{SimClock, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{Selectable, ViewSystems, VisualIndex, VisualOf, sim_to_world};
use testbed_content::village::{VillageState, Villager, geography, homestead};

#[derive(Resource)]
struct FarmAssets {
    cube: Handle<Mesh>,
    round: Handle<Mesh>,
    soil: Handle<StandardMaterial>,
    green: Handle<StandardMaterial>,
    leaf: Handle<StandardMaterial>,
    ripe: Handle<StandardMaterial>,
    berry: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    owners: [Handle<StandardMaterial>; 3],
    cottage: Handle<WorldAsset>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BedStage {
    Empty,
    NeedsWater,
    Growing,
    Ready,
}
impl BedStage {
    fn from_crop(crop: Option<&homestead::Crop>, now: u64) -> Self {
        match homestead::crop_stage(crop, now) {
            "needs_water" => Self::NeedsWater,
            "growing" => Self::Growing,
            "ready" => Self::Ready,
            _ => Self::Empty,
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::NeedsWater => 1,
            Self::Growing => 2,
            Self::Ready => 3,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CropLook {
    Grain,
    Berries,
}
impl CropLook {
    fn from_crop(crop: Option<&homestead::Crop>) -> Self {
        if crop.is_some_and(|crop| crop.kind == testbed_content::village::catalog::BERRIES) {
            Self::Berries
        } else {
            Self::Grain
        }
    }
}
struct BedVisual {
    grain: [Entity; 4],
    berries: [Entity; 4],
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlotSnapshot {
    owner: Option<SimId>,
    owner_name: String,
    stages: Vec<BedStage>,
    kinds: Vec<CropLook>,
    cottage_built: bool,
}
#[derive(Component)]
struct FarmVisual {
    source: Entity,
    epoch: u64,
    checked_tick: u64,
    allowed: bool,
    details: Entity,
    owner_markers: [Entity; 3],
    beds: Vec<BedVisual>,
    observed: Option<PlotSnapshot>,
    caption: String,
}
#[derive(Component)]
struct CottageVisual {
    source: Entity,
    epoch: u64,
    checked_tick: u64,
    allowed: bool,
    observed_owner: Option<String>,
    caption: String,
}
#[derive(Component)]
struct FarmCaption {
    visual: Entity,
    cottage: bool,
}

#[derive(SystemParam)]
struct FarmFacts<'w, 's> {
    clock: Res<'w, SimClock>,
    village: Res<'w, VillageState>,
    geography: Option<Res<'w, geography::Geography>>,
    regional: Option<Res<'w, testbed_content::village::frontier::world_spec::LivingWorldSpec>>,
    index: Res<'w, SimIndex>,
    fog: Option<Res<'w, FogOfWar>>,
    io: Option<Res<'w, AgentIoState>>,
    plots: Query<'w, 's, (&'static SimId, &'static SimPos, Ref<'static, HomesteadPlot>)>,
    cottages: Query<'w, 's, (&'static SimId, &'static SimPos, &'static Cottage)>,
    people: Query<'w, 's, &'static Villager>,
    memory: Query<'w, 's, &'static PerceptionMemory>,
}
impl FarmFacts<'_, '_> {
    fn epoch(&self) -> u64 {
        self.io.as_ref().map_or(0, |state| state.epoch)
    }
    fn owner_name(&self, owner: Option<SimId>) -> String {
        owner.map_or_else(
            || "Unclaimed".into(),
            |id| {
                self.index
                    .get(id)
                    .and_then(|entity| self.people.get(entity).ok())
                    .map_or_else(|| format!("Actor #{}", id.0), |person| person.name.clone())
            },
        )
    }
    fn allowed(&self, id: SimId, owner: Option<SimId>, look: Look) -> bool {
        !self.fog.as_ref().is_some_and(|fog| fog.enabled)
            || owner == Some(self.village.played)
            || matches!(look, Look::Live)
            || self
                .index
                .get(self.village.played)
                .and_then(|actor| self.memory.get(actor).ok())
                .and_then(|memory| memory.get(id))
                .is_some_and(|percept| percept.sensed)
    }
    fn definition(&self, id: &str) -> Option<geography::ExpansionPlot<'_>> {
        if let Some(spec) = &self.regional {
            return spec
                .plots
                .iter()
                .find(|plot| plot.id == id)
                .map(|plot| plot.as_expansion());
        }
        self.geography
            .as_deref()?
            .expansion_plots
            .iter()
            .find(|definition| definition.id == id)
            .copied()
    }
}

/// The parent installs this only for an expanded Living world. No sim systems are added.
pub(super) fn plugin(app: &mut App) {
    if !geography::is_expanded(app.world()) {
        return;
    }
    app.add_systems(Startup, prepare_assets)
        .add_systems(
            Update,
            spawn_roots
                .in_set(ViewSystems::Sync)
                .after(super::relink_static)
                .before(crate::visuals::despawn_visuals),
        )
        .add_systems(
            Update,
            present
                .in_set(ViewSystems::Present)
                .after(FogSet)
                .before(super::actor_captions)
                .before(super::project),
        );
}
fn prepare_assets(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut material = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 1.0,
            ..default()
        })
    };
    commands.insert_resource(FarmAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        round: meshes.add(
            Sphere::new(1.0)
                .mesh()
                .ico(1)
                .expect("one subdivision crop mesh"),
        ),
        soil: material(Color::srgb(0.40, 0.29, 0.18)),
        green: material(Color::srgb(0.25, 0.43, 0.20)),
        leaf: material(Color::srgb(0.50, 0.65, 0.28)),
        ripe: material(Color::srgb(0.84, 0.65, 0.24)),
        berry: material(Color::srgb(0.72, 0.14, 0.28)),
        water: material(Color::srgb(0.29, 0.60, 0.72)),
        wood: material(Color::srgb(0.42, 0.32, 0.22)),
        owners: [
            material(Color::srgb(0.65, 0.66, 0.48)),
            material(Color::srgb(0.91, 0.72, 0.33)),
            material(Color::srgb(0.34, 0.56, 0.42)),
        ],
        cottage: server.load(GltfAssetLabel::Scene(0).from_asset("hex/building_home_A_blue.gltf")),
    });
}
fn cube(
    commands: &mut Commands,
    parent: Entity,
    assets: &FarmAssets,
    material: Handle<StandardMaterial>,
    position: Vec3,
    scale: Vec3,
) {
    commands.spawn((
        ChildOf(parent),
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(position).with_scale(scale),
    ));
}
fn group(commands: &mut Commands, parent: Entity, position: Vec3) -> Entity {
    commands
        .spawn((
            ChildOf(parent),
            Transform::from_translation(position),
            Visibility::Hidden,
        ))
        .id()
}
fn round_crop(
    commands: &mut Commands,
    parent: Entity,
    assets: &FarmAssets,
    material: Handle<StandardMaterial>,
    position: Vec3,
    scale: Vec3,
) {
    commands.spawn((
        ChildOf(parent),
        Mesh3d(assets.round.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(position).with_scale(scale),
    ));
}
fn bed_visual(
    commands: &mut Commands,
    parent: Entity,
    assets: &FarmAssets,
    position: Vec3,
) -> BedVisual {
    let bed = commands
        .spawn((
            ChildOf(parent),
            Transform::from_translation(position),
            Visibility::Inherited,
        ))
        .id();
    cube(
        commands,
        bed,
        assets,
        assets.soil.clone(),
        Vec3::new(0.0, 0.04, 0.0),
        Vec3::new(1.0, 0.08, 0.72),
    );
    let stages = std::array::from_fn(|_| group(commands, bed, Vec3::ZERO));
    let water = stages[BedStage::NeedsWater.index()];
    cube(
        commands,
        water,
        assets,
        assets.leaf.clone(),
        Vec3::new(0.0, 0.12, 0.0),
        Vec3::new(0.12, 0.13, 0.12),
    );
    cube(
        commands,
        water,
        assets,
        assets.water.clone(),
        Vec3::new(0.36, 0.25, 0.15),
        Vec3::new(0.12, 0.20, 0.12),
    );
    for x in [-0.23, 0.23] {
        cube(
            commands,
            stages[2],
            assets,
            assets.green.clone(),
            Vec3::new(x, 0.17, 0.0),
            Vec3::new(0.045, 0.25, 0.045),
        );
        cube(
            commands,
            stages[2],
            assets,
            assets.leaf.clone(),
            Vec3::new(x - 0.04, 0.26, 0.0),
            Vec3::new(0.23, 0.06, 0.13),
        );
        cube(
            commands,
            stages[3],
            assets,
            assets.green.clone(),
            Vec3::new(x, 0.30, 0.0),
            Vec3::new(0.045, 0.50, 0.045),
        );
        cube(
            commands,
            stages[3],
            assets,
            assets.ripe.clone(),
            Vec3::new(x, 0.57, 0.0),
            Vec3::new(0.15, 0.25, 0.13),
        );
    }
    let berries = std::array::from_fn(|_| group(commands, bed, Vec3::ZERO));
    round_crop(
        commands,
        berries[1],
        assets,
        assets.leaf.clone(),
        Vec3::new(0., 0.16, 0.),
        Vec3::new(0.14, 0.08, 0.13),
    );
    cube(
        commands,
        berries[1],
        assets,
        assets.water.clone(),
        Vec3::new(0.36, 0.25, 0.15),
        Vec3::new(0.12, 0.20, 0.12),
    );
    for stage in [2, 3] {
        let size = if stage == 2 { 0.24 } else { 0.30 };
        for position in [
            Vec3::new(-0.16, 0.23, 0.04),
            Vec3::new(0.16, 0.23, 0.04),
            Vec3::new(0., 0.33, -0.08),
        ] {
            round_crop(
                commands,
                berries[stage],
                assets,
                assets.green.clone(),
                position,
                Vec3::new(size, size * 0.7, size * 0.8),
            );
        }
    }
    for position in [
        Vec3::new(-0.20, 0.37, 0.13),
        Vec3::new(-0.04, 0.43, 0.18),
        Vec3::new(0.16, 0.35, 0.14),
        Vec3::new(0.19, 0.41, -0.05),
    ] {
        round_crop(
            commands,
            berries[3],
            assets,
            assets.berry.clone(),
            position,
            Vec3::splat(0.055),
        );
    }
    BedVisual {
        grain: stages,
        berries,
    }
}
fn spawn_roots(
    mut commands: Commands,
    facts: FarmFacts,
    assets: Option<Res<FarmAssets>>,
    mut index: ResMut<VisualIndex>,
) {
    let Some(assets) = assets else {
        return;
    };
    let mut plots: Vec<_> = facts
        .plots
        .iter()
        .filter(|(id, _, _)| {
            facts
                .index
                .get(**id)
                .is_some_and(|sim| !index.map.contains_key(&sim))
        })
        .collect();
    plots.sort_by_key(|(id, _, _)| **id);
    for (id, pos, plot) in plots {
        let Some(sim) = facts.index.get(*id) else {
            continue;
        };
        if index.map.contains_key(&sim) {
            continue;
        }
        let Some(definition) = facts.definition(&plot.plot_id) else {
            continue;
        };
        let root = commands
            .spawn((
                Name::new("homestead plot visual"),
                VisualOf(sim),
                StaticId(*id),
                Selectable,
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::Hidden,
                Fogged(Look::Hidden),
                FogDim::default(),
            ))
            .id();
        let details = group(&mut commands, root, Vec3::ZERO);
        let marker_at = Vec3::new(
            (definition.x - pos.x - definition.width_mm / 2 + 300) as f32 / 1000.0,
            0.0,
            (definition.z - pos.z - definition.depth_mm / 2 + 300) as f32 / 1000.0,
        );
        cube(
            &mut commands,
            details,
            &assets,
            assets.wood.clone(),
            marker_at + Vec3::Y * 0.35,
            Vec3::new(0.055, 0.7, 0.055),
        );
        let markers = std::array::from_fn(|kind| {
            let flag = group(&mut commands, details, marker_at);
            cube(
                &mut commands,
                flag,
                &assets,
                assets.owners[kind].clone(),
                Vec3::new(0.17, 0.59, 0.0),
                Vec3::new(0.30, 0.19, 0.055),
            );
            flag
        });
        let beds = plot
            .beds
            .iter()
            .enumerate()
            .filter_map(|(index, _)| {
                let bed = homestead::bed_position_in(&definition, index as u32)?;
                Some(bed_visual(
                    &mut commands,
                    details,
                    &assets,
                    sim_to_world(bed) - sim_to_world(*pos),
                ))
            })
            .collect();
        commands.entity(root).insert(FarmVisual {
            source: sim,
            epoch: facts.epoch(),
            checked_tick: facts.clock.tick,
            allowed: false,
            details,
            owner_markers: markers,
            beds,
            observed: None,
            caption: definition.label.into(),
        });
        let caption = label(&mut commands, *id, root, definition.label, 0.8, false);
        commands.entity(caption).insert(FarmCaption {
            visual: root,
            cottage: false,
        });
        index.map.insert(sim, root);
    }
    let mut cottages: Vec<_> = facts
        .cottages
        .iter()
        .filter(|(id, _, _)| {
            facts
                .index
                .get(**id)
                .is_some_and(|sim| !index.map.contains_key(&sim))
        })
        .collect();
    cottages.sort_by_key(|(id, _, _)| **id);
    for (id, pos, _) in cottages {
        let Some(sim) = facts.index.get(*id) else {
            continue;
        };
        if index.map.contains_key(&sim) {
            continue;
        }
        let root = commands
            .spawn((
                Name::new("completed homestead cottage"),
                VisualOf(sim),
                StaticId(*id),
                Selectable,
                Transform::from_translation(sim_to_world(*pos)),
                Visibility::Hidden,
                Fogged(Look::Hidden),
                FogDim::default(),
                CottageVisual {
                    source: sim,
                    epoch: facts.epoch(),
                    checked_tick: facts.clock.tick,
                    allowed: false,
                    observed_owner: None,
                    caption: "Cottage".into(),
                },
            ))
            .id();
        commands.entity(root).with_child((
            WorldAssetRoot(assets.cottage.clone()),
            Transform::from_scale(Vec3::splat(2.4)),
        ));
        let caption = label(&mut commands, *id, root, "Cottage", 3.0, false);
        commands.entity(caption).insert(FarmCaption {
            visual: root,
            cottage: true,
        });
        index.map.insert(sim, root);
    }
}

fn set_visible(query: &mut Query<&mut Visibility>, entity: Entity, shown: bool) {
    if let Ok(mut visibility) = query.get_mut(entity) {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
fn short(value: &str, limit: usize) -> String {
    let mut output: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        output.push_str("...");
    }
    output
}
fn caption(definition: &geography::ExpansionPlot, snapshot: Option<&PlotSnapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return definition.label.into();
    };
    let ready = snapshot
        .stages
        .iter()
        .filter(|stage| **stage == BedStage::Ready)
        .count();
    let dry = snapshot
        .stages
        .iter()
        .filter(|stage| **stage == BedStage::NeedsWater)
        .count();
    let growing = snapshot
        .stages
        .iter()
        .filter(|stage| **stage == BedStage::Growing)
        .count();
    let work = if ready > 0 {
        format!("{ready} ready")
    } else if dry > 0 {
        format!("{dry} need water")
    } else if growing > 0 {
        format!("{growing} growing")
    } else if snapshot.cottage_built {
        "Home built".into()
    } else {
        "Empty beds".into()
    };
    format!(
        "{}\n{} | {work}",
        definition.label,
        short(&snapshot.owner_name, 16)
    )
}

#[derive(SystemParam)]
struct FarmPresentation<'w, 's> {
    plots: Query<
        'w,
        's,
        (
            &'static VisualOf,
            &'static Fogged,
            &'static mut FarmVisual,
            &'static mut Transform,
        ),
        Without<CottageVisual>,
    >,
    cottages: Query<
        'w,
        's,
        (
            &'static VisualOf,
            &'static Fogged,
            &'static mut CottageVisual,
            &'static mut Transform,
        ),
        Without<FarmVisual>,
    >,
    visibility: Query<'w, 's, &'static mut Visibility>,
    captions: Query<
        'w,
        's,
        (
            &'static FarmCaption,
            &'static Children,
            &'static mut super::Label,
            &'static mut Node,
        ),
    >,
    texts: Query<'w, 's, &'static mut Text>,
}
fn present(facts: FarmFacts, mut view: FarmPresentation) {
    for (link, fog, mut visual, mut transform) in &mut view.plots {
        let Ok((id, pos, plot)) = facts.plots.get(link.0) else {
            continue;
        };
        let reset = visual.source != link.0
            || visual.epoch != facts.epoch()
            || facts.clock.tick < visual.checked_tick;
        if reset {
            visual.observed = None;
            visual.allowed = false;
            visual.source = link.0;
            visual.epoch = facts.epoch();
            visual.caption = facts
                .definition(&plot.plot_id)
                .map_or("Plot", |definition| definition.label)
                .into();
        }
        let allowed = facts.allowed(*id, plot.owner, fog.0);
        if allowed
            && (visual.observed.is_none()
                || !visual.allowed
                || visual.checked_tick != facts.clock.tick
                || plot.is_changed())
        {
            let snapshot = PlotSnapshot {
                owner: plot.owner,
                owner_name: facts.owner_name(plot.owner),
                stages: plot
                    .beds
                    .iter()
                    .map(|crop| BedStage::from_crop(crop.as_ref(), facts.clock.tick))
                    .collect(),
                kinds: plot
                    .beds
                    .iter()
                    .map(|crop| CropLook::from_crop(crop.as_ref()))
                    .collect(),
                cottage_built: plot.cottage_built,
            };
            if visual.observed.as_ref() != Some(&snapshot) {
                if let Some(definition) = facts.definition(&plot.plot_id) {
                    visual.caption = caption(&definition, Some(&snapshot));
                }
                visual.observed = Some(snapshot);
            }
        }
        visual.allowed = allowed;
        visual.checked_tick = facts.clock.tick;
        transform.translation = sim_to_world(*pos);
        set_visible(
            &mut view.visibility,
            visual.details,
            visual.observed.is_some(),
        );
        if let Some(snapshot) = &visual.observed {
            let owner = if snapshot.owner.is_none() {
                0
            } else if snapshot.owner == Some(facts.village.played) {
                1
            } else {
                2
            };
            for (index, marker) in visual.owner_markers.iter().enumerate() {
                set_visible(&mut view.visibility, *marker, index == owner);
            }
            for (index, bed) in visual.beds.iter().enumerate() {
                let selected = snapshot
                    .stages
                    .get(index)
                    .copied()
                    .unwrap_or(BedStage::Empty)
                    .index();
                let kind = snapshot
                    .kinds
                    .get(index)
                    .copied()
                    .unwrap_or(CropLook::Grain);
                for (stage, entity) in bed.grain.iter().enumerate() {
                    set_visible(
                        &mut view.visibility,
                        *entity,
                        kind == CropLook::Grain && stage == selected,
                    );
                }
                for (stage, entity) in bed.berries.iter().enumerate() {
                    set_visible(
                        &mut view.visibility,
                        *entity,
                        kind == CropLook::Berries && stage == selected,
                    );
                }
            }
        }
    }
    for (link, fog, mut visual, mut transform) in &mut view.cottages {
        let Ok((id, pos, cottage)) = facts.cottages.get(link.0) else {
            continue;
        };
        if visual.source != link.0
            || visual.epoch != facts.epoch()
            || facts.clock.tick < visual.checked_tick
        {
            visual.source = link.0;
            visual.epoch = facts.epoch();
            visual.observed_owner = None;
            visual.caption = "Cottage".into();
            visual.allowed = false;
        }
        let owner = facts
            .index
            .get(cottage.plot)
            .and_then(|plot| facts.plots.get(plot).ok())
            .and_then(|(_, _, plot)| plot.owner);
        let allowed = facts.allowed(*id, owner, fog.0);
        if allowed
            && (visual.observed_owner.is_none()
                || !visual.allowed
                || visual.checked_tick != facts.clock.tick)
        {
            let name = facts.owner_name(owner);
            if visual.observed_owner.as_ref() != Some(&name) {
                visual.caption = format!("Cottage\n{}", short(&name, 24));
                visual.observed_owner = Some(name);
            }
        }
        visual.checked_tick = facts.clock.tick;
        visual.allowed = allowed;
        transform.translation = sim_to_world(*pos);
    }
    for (tag, children, mut layout, mut node) in &mut view.captions {
        let text = if tag.cottage {
            let Ok((_, _, visual, _)) = view.cottages.get(tag.visual) else {
                continue;
            };
            visual.caption.as_str()
        } else {
            let Ok((_, _, visual, _)) = view.plots.get(tag.visual) else {
                continue;
            };
            visual.caption.as_str()
        };
        for child in children.iter() {
            if let Ok(mut current) = view.texts.get_mut(child)
                && current.0 != text
            {
                current.0 = text.to_owned();
                let width = (text
                    .lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(1) as f32
                    * 6.7
                    + 10.0)
                    .clamp(60.0, 210.0);
                node.width = px(width);
                layout.fallback_size =
                    Vec2::new(width, if text.contains('\n') { 34.0 } else { 18.0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regional_farm_facts_use_frozen_plots_without_legacy_geography_resource() {
        let mut spec = testbed_content::village::frontier::world_spec::LivingWorldSpec::default();
        spec.plots[0].x += 1000;
        let expected = spec.plots[0].clone();
        let mut app = App::new();
        app.insert_resource(spec)
            .init_resource::<SimIndex>()
            .init_resource::<SimClock>()
            .insert_resource(VillageState {
                played: SimId(1),
                agent_alias: "QA".into(),
                life: 1,
                fund: 0,
                last_dividend_day: 0,
                produced: default(),
                transactions: 0,
            })
            .add_systems(Update, move |facts: FarmFacts| {
                let plot = facts
                    .definition(&expected.id)
                    .expect("frozen regional plot");
                assert_eq!(plot.x, expected.x);
                assert_eq!(plot.label, expected.label);
            });
        app.update();
        assert!(!app.world().contains_resource::<geography::Geography>());
    }

    #[test]
    fn native_farm_remembers_observed_foreign_crops_and_refreshes_owned_crops() {
        let mut app = App::new();
        app.insert_resource(testbed_content::world_runtime::ActiveWorld(
            testbed_content::worlds::builtin_preset("living_island").unwrap(),
        ));
        let geography = *geography::for_world(app.world());
        let definition = geography.expansion_plots[0];
        app.insert_resource(geography)
            .init_resource::<SimIndex>()
            .init_resource::<SimClock>()
            .insert_resource(FogOfWar { enabled: true })
            .insert_resource(VillageState {
                played: SimId(1),
                agent_alias: "Herp".into(),
                life: 1,
                fund: 0,
                last_dividend_day: 0,
                produced: default(),
                transactions: 0,
            })
            .add_systems(Update, present);
        let state = HomesteadPlot {
            plot_id: definition.id.into(),
            owner: Some(SimId(2)),
            cottage_built: false,
            cottage_entity: None,
            beds: vec![
                Some(homestead::Crop {
                    kind: "village_grain".into(),
                    planted_at: 0,
                    watered_at: None,
                    ready_at: None,
                    ready_announced: false,
                }),
                None,
                None,
            ],
        };
        let sim = app
            .world_mut()
            .spawn((SimId(44), SimPos::new(definition.x, definition.z), state))
            .id();
        let details = app.world_mut().spawn(Visibility::Hidden).id();
        let markers = std::array::from_fn(|_| app.world_mut().spawn(Visibility::Hidden).id());
        let stages = std::array::from_fn(|_| app.world_mut().spawn(Visibility::Hidden).id());
        let berry_stages = std::array::from_fn(|_| app.world_mut().spawn(Visibility::Hidden).id());
        let root = app
            .world_mut()
            .spawn((
                VisualOf(sim),
                Fogged(Look::Hidden),
                Transform::default(),
                Visibility::Hidden,
                FarmVisual {
                    source: sim,
                    epoch: 0,
                    checked_tick: 0,
                    allowed: false,
                    details,
                    owner_markers: markers,
                    beds: vec![BedVisual {
                        grain: stages,
                        berries: berry_stages,
                    }],
                    observed: None,
                    caption: definition.label.into(),
                },
            ))
            .id();
        app.update();
        assert!(
            app.world()
                .get::<FarmVisual>(root)
                .unwrap()
                .observed
                .is_none(),
            "unseen roots must not disclose live crops or owners"
        );
        assert_eq!(
            *app.world().get::<Visibility>(details).unwrap(),
            Visibility::Hidden
        );
        app.world_mut().get_mut::<Fogged>(root).unwrap().0 = Look::Live;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(stages[1]).unwrap(),
            Visibility::Inherited
        );
        app.world_mut().get_mut::<Fogged>(root).unwrap().0 = Look::Dim;
        app.world_mut().resource_mut::<SimClock>().tick = 100;
        {
            let mut plot = app.world_mut().get_mut::<HomesteadPlot>(sim).unwrap();
            plot.owner = Some(SimId(3));
            let crop = plot.beds[0].as_mut().unwrap();
            crop.watered_at = Some(1);
            crop.ready_at = Some(20);
        }
        let untouched = app.world().get::<HomesteadPlot>(sim).unwrap().clone();
        let clock = *app.world().resource::<SimClock>();
        app.update();
        let snapshot = app
            .world()
            .get::<FarmVisual>(root)
            .unwrap()
            .observed
            .as_ref()
            .unwrap();
        assert_eq!(
            snapshot.owner,
            Some(SimId(2)),
            "unobserved ownership must remain remembered"
        );
        assert_eq!(
            snapshot.stages[0],
            BedStage::NeedsWater,
            "a remembered crop must not advance using live time"
        );
        assert_eq!(
            app.world().get::<HomesteadPlot>(sim).unwrap(),
            &untouched,
            "presentation must not mutate authoritative crop state"
        );
        assert_eq!(*app.world().resource::<SimClock>(), clock);
        app.world_mut().get_mut::<HomesteadPlot>(sim).unwrap().owner = Some(SimId(1));
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(stages[3]).unwrap(),
            Visibility::Inherited,
            "owned remembered crops may refresh"
        );
        assert_eq!(
            *app.world().get::<Visibility>(markers[1]).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden,
            "presentation must leave root visibility to the fog system"
        );
        app.world_mut().get_mut::<HomesteadPlot>(sim).unwrap().beds[0]
            .as_mut()
            .unwrap()
            .kind = testbed_content::village::catalog::BERRIES.into();
        let berry_state = app.world().get::<HomesteadPlot>(sim).unwrap().clone();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(stages[3]).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(berry_stages[3]).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world()
                .get::<FarmVisual>(root)
                .unwrap()
                .observed
                .as_ref()
                .unwrap()
                .kinds[0],
            CropLook::Berries
        );
        assert_eq!(
            app.world().get::<HomesteadPlot>(sim).unwrap(),
            &berry_state,
            "changing crop appearance never writes the crop"
        );
        app.world_mut().get_mut::<HomesteadPlot>(sim).unwrap().owner = Some(SimId(2));
        app.world_mut().get_mut::<HomesteadPlot>(sim).unwrap().beds[0]
            .as_mut()
            .unwrap()
            .kind = testbed_content::village::catalog::GRAIN.into();
        app.world_mut().resource_mut::<SimClock>().tick = 101;
        app.update();
        assert_eq!(
            app.world()
                .get::<FarmVisual>(root)
                .unwrap()
                .observed
                .as_ref()
                .unwrap()
                .kinds[0],
            CropLook::Berries,
            "unseen foreign crop changes cannot replace remembered kinds"
        );
        assert_eq!(
            *app.world().get::<Visibility>(berry_stages[3]).unwrap(),
            Visibility::Inherited
        );
        app.world_mut().get_mut::<Fogged>(root).unwrap().0 = Look::Hidden;
        app.world_mut().resource_mut::<SimClock>().tick = 0;
        app.update();
        assert!(
            app.world()
                .get::<FarmVisual>(root)
                .unwrap()
                .observed
                .is_none(),
            "tick rollback must discard prior-world observations"
        );
        assert_eq!(
            *app.world().get::<Visibility>(details).unwrap(),
            Visibility::Hidden
        );
    }
}
