//! Primitive placeholder visuals for [`UnitView`]s (spec §6.3, §10).
//!
//! Each unit type gets its parts from its [`VisualDef`] placeholder, built once and shared, and
//! each player one material, so Bevy batches every unit of a type and owner.
use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use fl_content::{ContentDb, Faction, Placeholder, VisualDef};
use fl_protocol::{PlayerId, UnitTypeId};

use crate::view::{ClientContent, SimViewSystems, UnitView};

/// Drawn for unit types without a [`VisualDef`]: a 2.5 m wide, 2 m high, 5 m long box.
const FALLBACK: Placeholder = Placeholder::Box {
    size: (2.5, 2.0, 5.0),
};

/// Player colours for players without a [`PlayerColors`] entry, by player index.
const PALETTE: [(f32, f32, f32); 4] = [
    (0.20, 0.30, 0.65),
    (0.65, 0.15, 0.12),
    (0.20, 0.50, 0.20),
    (0.70, 0.60, 0.15),
];

/// The material colour of parts that are not recoloured.
const NEUTRAL: (f32, f32, f32) = (0.35, 0.35, 0.32);

/// Each player's colour, usually their faction's; the match setup inserts it.
#[derive(Resource, Default, Clone, Debug)]
pub struct PlayerColors(pub BTreeMap<PlayerId, Color>);

impl PlayerColors {
    /// Player `i` gets the colour of `factions[i]`; unknown factions are skipped.
    #[must_use]
    pub fn from_factions<'a>(
        content: &ContentDb,
        factions: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let table = content.kind::<Faction>();
        let colors = factions
            .into_iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let player = PlayerId(u8::try_from(i).ok()?);
                let (r, g, b) = table.get(id)?.color;
                Some((player, Color::linear_rgb(r, g, b)))
            })
            .collect();
        PlayerColors(colors)
    }

    /// The colour of `player`, or a palette colour when it has none.
    #[must_use]
    pub fn color(&self, player: PlayerId) -> Color {
        self.0.get(&player).copied().unwrap_or_else(|| {
            let (r, g, b) = PALETTE[usize::from(player.0) % PALETTE.len()];
            Color::linear_rgb(r, g, b)
        })
    }
}

/// The shared parts of one unit type.
#[derive(Clone, Debug)]
struct UnitParts {
    parts: Vec<(Handle<Mesh>, Transform)>,
    recolor: bool,
}

/// Mesh and material handles shared by every placeholder, built on first use.
#[derive(Resource, Default)]
pub struct PlaceholderAssets {
    units: BTreeMap<UnitTypeId, UnitParts>,
    players: BTreeMap<PlayerId, Handle<StandardMaterial>>,
    neutral: Option<Handle<StandardMaterial>>,
}

impl PlaceholderAssets {
    /// The mesh handles of `unit`'s parts, once built.
    #[must_use]
    pub fn meshes(&self, unit: UnitTypeId) -> Option<Vec<Handle<Mesh>>> {
        let parts = &self.units.get(&unit)?.parts;
        Some(parts.iter().map(|(mesh, _)| mesh.clone()).collect())
    }

    /// The material of `player`, once built.
    #[must_use]
    pub fn material(&self, player: PlayerId) -> Option<Handle<StandardMaterial>> {
        self.players.get(&player).cloned()
    }
}

/// The meshes of a placeholder and their transforms relative to the unit, which faces local +X.
/// Sizes are `(width, height, length)` in metres, multiplied by `scale`; the unit stands on y = 0.
#[must_use]
pub fn placeholder_parts(placeholder: &Placeholder, scale: f32) -> Vec<(Mesh, Transform)> {
    // A box `(width, height, length)` centred at `(x, y)` with its length along +X.
    let block = |(w, h, l): (f32, f32, f32), x: f32, y: f32| {
        (
            Mesh::from(Cuboid::new(l, h, w)),
            Transform::from_xyz(x, y, 0.0),
        )
    };
    let parts = match *placeholder {
        Placeholder::Box { size } => vec![block(size, 0.0, size.1 * 0.5)],
        Placeholder::Tank {
            hull,
            turret,
            barrel_len,
        } => {
            let turret_y = hull.1 + turret.1 * 0.5;
            let radius = (turret.1 * 0.15).max(0.05);
            // A cylinder's axis is +Y; a -90° turn about Z lays it along +X.
            let barrel = (
                Mesh::from(Cylinder::new(radius, barrel_len)),
                Transform::from_xyz(turret.2 * 0.5 + barrel_len * 0.5, turret_y, 0.0)
                    .with_rotation(Quat::from_rotation_z(-FRAC_PI_2)),
            );
            vec![
                block(hull, 0.0, hull.1 * 0.5),
                block(turret, 0.0, turret_y),
                barrel,
            ]
        }
        Placeholder::Infantry { radius, height } => {
            let length = (height - 2.0 * radius).max(0.0);
            vec![(
                Mesh::from(Capsule3d::new(radius, length)),
                Transform::from_xyz(0.0, height * 0.5, 0.0),
            )]
        }
        Placeholder::Truck { cab, bed } => {
            // The bed at the back, the cab in front, together centred on the origin.
            let total = cab.2 + bed.2;
            vec![
                block(bed, -total * 0.5 + bed.2 * 0.5, bed.1 * 0.5),
                block(cab, total * 0.5 - cab.2 * 0.5, cab.1 * 0.5),
            ]
        }
    };
    parts
        .into_iter()
        .map(|(mesh, transform)| {
            let scaled = Transform {
                translation: transform.translation * scale,
                rotation: transform.rotation,
                scale: Vec3::splat(scale),
            };
            (mesh, scaled)
        })
        .collect()
}

/// Gives every new or changed [`UnitView`] child meshes for its unit type and owner.
pub struct PlaceholderPlugin;

impl Plugin for PlaceholderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaceholderAssets>().add_systems(
            Update,
            attach_placeholders
                .after(SimViewSystems)
                .run_if(resource_exists::<Assets<Mesh>>)
                .run_if(resource_exists::<Assets<StandardMaterial>>),
        );
    }
}

/// The parts of `unit`, from its [`VisualDef`] or the fallback box.
fn build_unit_parts(
    content: Option<&ContentDb>,
    unit: UnitTypeId,
    meshes: &mut Assets<Mesh>,
) -> UnitParts {
    let visual = content.and_then(|db| {
        let id = &db.unit(unit)?.id;
        db.kind::<VisualDef>().get(id).cloned()
    });
    let (placeholder, scale, recolor) = match &visual {
        Some(visual) => (&visual.placeholder, visual.scale, visual.recolor),
        None => {
            tracing::debug!(unit = unit.0, "no VisualDef; drawing the fallback box");
            (&FALLBACK, 1.0, true)
        }
    };
    let parts = placeholder_parts(placeholder, scale)
        .into_iter()
        .map(|(mesh, transform)| (meshes.add(mesh), transform))
        .collect();
    UnitParts { parts, recolor }
}

/// Attaches parts to new or changed views. When [`ClientContent`] or [`PlayerColors`] is
/// inserted, changed or removed, the cached parts and materials are rebuilt and every view gets
/// new parts.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system; each parameter is one resource or query"
)]
fn attach_placeholders(
    mut commands: Commands,
    changed: Query<(Entity, &UnitView), Changed<UnitView>>,
    all: Query<(Entity, &UnitView)>,
    content: Option<Res<ClientContent>>,
    colors: Option<Res<PlayerColors>>,
    mut assets: ResMut<PlaceholderAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut present: Local<(bool, bool)>,
) {
    let now_present = (content.is_some(), colors.is_some());
    let stale = now_present != *present
        || content.as_ref().is_some_and(Res::is_changed)
        || colors.as_ref().is_some_and(Res::is_changed);
    *present = now_present;
    let assets = &mut *assets;
    let views: Vec<(Entity, UnitView)> = if stale {
        *assets = PlaceholderAssets::default();
        all.iter().map(|(entity, view)| (entity, *view)).collect()
    } else {
        changed
            .iter()
            .map(|(entity, view)| (entity, *view))
            .collect()
    };
    for (entity, view) in views {
        let unit = assets.units.entry(view.unit).or_insert_with(|| {
            build_unit_parts(content.as_deref().map(|c| &*c.0), view.unit, &mut meshes)
        });
        let material = if unit.recolor {
            assets
                .players
                .entry(view.owner)
                .or_insert_with(|| {
                    let color = colors.as_deref().map_or_else(
                        || PlayerColors::default().color(view.owner),
                        |c| c.color(view.owner),
                    );
                    materials.add(color)
                })
                .clone()
        } else {
            assets
                .neutral
                .get_or_insert_with(|| {
                    let (r, g, b) = NEUTRAL;
                    materials.add(Color::linear_rgb(r, g, b))
                })
                .clone()
        };
        let mut view_entity = commands.entity(entity);
        view_entity.despawn_children();
        view_entity.with_children(|parent| {
            for (mesh, transform) in &unit.parts {
                parent.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    *transform,
                ));
            }
        });
    }
}
