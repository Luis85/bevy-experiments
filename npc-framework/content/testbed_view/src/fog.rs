//! Fog of war in the window (spec G8, plan Task 10): the world as the played
//! actor (the one with `RevealsFog`, the explorer) knows it, like Age of
//! Empires.
//!
//! - **never explored** ground is black;
//! - **explored, not visible now**: the ground greyed, the statics (walls,
//!   doors, trees, rocks, workstations, chests, items lying on the ground)
//!   dimmed and shown **as last seen** (`SeenStatics`: a door seen open stays
//!   open, a tree seen standing stays standing), never a unit;
//! - **visible now**: drawn as it is.
//!
//! **Units** (actors and vehicles) are drawn only while the actor **senses**
//! them now (Ruling R14: a percept with `sensed` in its `PerceptionMemory`,
//! the "You perceive" list), never by the fog cells (those are seen from the
//! actor's cell centre and can differ by half a cell from real sensing).
//! The played actor and the vehicle it rides are always drawn. A carried
//! item goes with its holder.
//!
//! **Technique.** A mask plane over the ground: one texel per grid cell
//! ([`mask_texel`]: black at [`MASK_UNEXPLORED`], dark grey at
//! [`MASK_EXPLORED`], clear at [`MASK_VISIBLE`]; unlit, alpha-blended,
//! linearly filtered so cell edges are soft),
//! rewritten only when the explored or visible bits change (a hash of the
//! bits, checked once per sim tick). Things standing above the ground are
//! dimmed per visual instead: a static drawn as last seen (and view-only
//! dressing in explored ground) swaps its meshes' materials for darkened
//! copies (cached per material) when its state changes. Units are hidden.
//! The looks are decided once per sim tick (and when visuals appear or F8
//! toggles); a frame with no new tick decides and writes nothing.
//!
//! **Gizmos and selection.** The sim entities whose look is hidden go into
//! `npc_view_helpers`' `ViewHidden` (once per tick): its crew, path,
//! selection and order gizmos skip them, they cannot be clicked or
//! box-selected, and a selected one is deselected. The F1 item markers
//! follow the item's look (Ruling R15, [`marker_pos`]); the rest of F1 and
//! the F5/F6/F7 panels are not fogged.
//!
//! **Toggles.** F8 here; [`FogOfWar`] (the app inserts it: `--fog on|off`,
//! on in explorer mode). With fog off, or without a `RevealsFog` actor,
//! everything shows as before. The debug panels (F1, F5, F6, F7) are not
//! fogged.
//!
//! Read-only on the sim: this module only reads sim components and writes
//! view entities (`Visibility`, materials, its own components).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use npc_framework::perception::{
    ExploredCells, PerceptionMemory, RevealsFog, SeenState, SeenStatics, VisibleCells,
};
use npc_framework::prelude::{
    CarriedBy, CellCoord, GridBounds, Item, MountedIn, SimClock, SimId, SimPos, SpatialGrid,
};
use npc_framework::view_helpers::{ViewHidden, ViewSystems, VisualOf};
use testbed_content::{Prop, PropKind, TestbedUnit};

/// Mask alpha of a never-explored cell: black.
pub const MASK_UNEXPLORED: u8 = 255;
/// Mask alpha of an explored cell not visible now. Blending is linear: 200
/// (78 % of a dark grey over the ground) reads about 55 % darker on screen,
/// and the grey takes some of the colour out (AoE's greyed ground).
pub const MASK_EXPLORED: u8 = 200;
/// The colour (linear RGB) laid over explored ground; never-explored is black.
pub const MASK_GREY: [u8; 3] = [22, 23, 27];
/// Mask alpha of a visible cell: clear.
pub const MASK_VISIBLE: u8 = 0;

/// How much of its colour a dimmed (last-seen) thing keeps, and how far it
/// is pulled towards grey first.
const DIM_KEEP: f32 = 0.22;
const DIM_GREY: f32 = 0.45;
/// The mask plane's height over the ground (above the floor tiles).
const MASK_Y: f32 = 0.07;
/// The mask reaches at least this far from the origin (cells): the grass
/// plane is 90 m wide.
const MASK_MIN_HALF_CELLS: i32 = 46;

/// Whether the window shows fog of war. The app inserts it (`--fog on|off`,
/// default on in explorer mode); F8 toggles it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FogOfWar {
    pub enabled: bool,
}

/// What one grid cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CellFog {
    Unexplored,
    Explored,
    Visible,
}

/// The mask alpha of a cell state.
pub const fn mask_alpha(state: CellFog) -> u8 {
    match state {
        CellFog::Unexplored => MASK_UNEXPLORED,
        CellFog::Explored => MASK_EXPLORED,
        CellFog::Visible => MASK_VISIBLE,
    }
}

/// The mask texel (RGBA8, linear) of a cell state.
pub const fn mask_texel(state: CellFog) -> [u8; 4] {
    let a = mask_alpha(state);
    match state {
        CellFog::Explored => [MASK_GREY[0], MASK_GREY[1], MASK_GREY[2], a],
        _ => [0, 0, 0, a],
    }
}

/// How a visual is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// As it is now.
    #[default]
    Live,
    /// As it is now, dimmed (dressing and wall pieces in explored ground).
    Dim,
    /// Dimmed, in its last-seen state.
    LastSeen(SeenState),
    /// Not drawn.
    Hidden,
}

impl Look {
    /// Drawn dimmed.
    pub const fn dimmed(self) -> bool {
        matches!(self, Look::Dim | Look::LastSeen(_))
    }

    pub const fn shown(self) -> bool {
        !matches!(self, Look::Hidden)
    }
}

/// The played actor's fog record, as the view reads it.
#[derive(Clone, Copy)]
pub struct FogLens<'a> {
    pub grid: &'a SpatialGrid,
    pub explored: &'a ExploredCells,
    pub visible: &'a VisibleCells,
    pub seen: &'a SeenStatics,
    /// Its perception memory (units are drawn while sensed in it).
    pub memory: Option<&'a PerceptionMemory>,
    /// The played actor.
    pub actor: SimId,
    /// The vehicle it rides.
    pub riding: Option<SimId>,
}

impl FogLens<'_> {
    /// The state of `cell`.
    pub fn cell(&self, cell: CellCoord) -> CellFog {
        if self.visible.is_visible(self.grid, cell) {
            CellFog::Visible
        } else if self.explored.is_explored(self.grid, cell) {
            CellFog::Explored
        } else {
            CellFog::Unexplored
        }
    }

    /// The state of the cell at `pos`.
    pub fn at(&self, pos: SimPos) -> CellFog {
        self.cell(self.grid.cell_of(pos))
    }

    /// The clearest state of the cells from `lo` to `hi` (inclusive).
    pub fn area(&self, lo: SimPos, hi: SimPos) -> CellFog {
        let (a, b) = (self.grid.cell_of(lo), self.grid.cell_of(hi));
        let mut best = CellFog::Unexplored;
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for x in a.x.min(b.x)..=a.x.max(b.x) {
                best = best.max(self.cell(CellCoord::new(x, z)));
                if best == CellFog::Visible {
                    return best;
                }
            }
        }
        best
    }

    /// Whether the actor senses `id` now (R14), is it, or rides it.
    pub fn senses(&self, id: SimId) -> bool {
        id == self.actor
            || self.riding == Some(id)
            || self
                .memory
                .and_then(|m| m.get(id))
                .is_some_and(|p| p.sensed)
    }
}

/// A unit or vehicle: drawn while the actor senses it (R14), else hidden.
pub fn unit_look(lens: Option<&FogLens>, id: SimId) -> Look {
    match lens {
        Some(l) if !l.senses(id) => Look::Hidden,
        _ => Look::Live,
    }
}

/// A static at `pos`: live in a visible cell, else as last seen when its
/// record is in ground not visible now, else hidden (never seen).
pub fn static_look(lens: Option<&FogLens>, id: SimId, pos: SimPos) -> Look {
    let Some(l) = lens else { return Look::Live };
    if l.at(pos) == CellFog::Visible {
        return Look::Live;
    }
    match l.seen.0.get(&id) {
        Some(s) if l.at(s.pos) != CellFog::Visible => Look::LastSeen(*s),
        _ => Look::Hidden,
    }
}

/// Something that never changes over `lo..=hi` (a wall piece, view-only
/// dressing): live where any cell is visible, dimmed where explored,
/// hidden where never explored.
pub fn area_look(lens: Option<&FogLens>, lo: SimPos, hi: SimPos) -> Look {
    match lens.map(|l| l.area(lo, hi)) {
        None | Some(CellFog::Visible) => Look::Live,
        Some(CellFog::Explored) => Look::Dim,
        Some(CellFog::Unexplored) => Look::Hidden,
    }
}

/// Where the F1 overlay marks an item lying at `pos` (Ruling R15): nowhere
/// when its visual is hidden, at the last-seen position when drawn as last
/// seen, else where it is.
pub fn marker_pos(look: Look, pos: SimPos) -> Option<SimPos> {
    match look {
        Look::Hidden => None,
        Look::LastSeen(s) => Some(s.pos),
        Look::Live | Look::Dim => Some(pos),
    }
}

/// The mask's cells: `width × height` from `min`, row by row (+x along a
/// row, rows towards +z), so texel `(i, j)` is cell `(min.x + i, min.z + j)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskRegion {
    pub min: CellCoord,
    pub width: u32,
    pub height: u32,
}

impl MaskRegion {
    /// The grid's bounds widened to cover the ground (at least
    /// [`MASK_MIN_HALF_CELLS`] around the origin at this cell size) plus a
    /// cell all round.
    pub fn covering(grid: &SpatialGrid) -> Self {
        let b = grid.bounds();
        let half = MASK_MIN_HALF_CELLS * 1000 / grid.cell_mm().max(1);
        let to_i32 = |v: u32| i32::try_from(v).unwrap_or(i32::MAX / 4);
        let x0 = b.min.x.min(-half) - 1;
        let z0 = b.min.z.min(-half) - 1;
        let x1 = (b.min.x + to_i32(b.width)).max(half) + 1;
        let z1 = (b.min.z + to_i32(b.height)).max(half) + 1;
        Self {
            min: CellCoord::new(x0, z0),
            width: (x1 - x0).max(1) as u32,
            height: (z1 - z0).max(1) as u32,
        }
    }
}

/// The mask texels ([`mask_texel`] of each cell) over `region`; all clear
/// without a lens (fog off).
pub fn mask_texels(lens: Option<&FogLens>, region: MaskRegion) -> Vec<u8> {
    let n = region.width as usize * region.height as usize;
    let mut out = vec![0u8; n * 4];
    let Some(l) = lens else { return out };
    for j in 0..region.height {
        for i in 0..region.width {
            let cell = CellCoord::new(region.min.x + i as i32, region.min.z + j as i32);
            let k = (j as usize * region.width as usize + i as usize) * 4;
            out[k..k + 4].copy_from_slice(&mask_texel(l.cell(cell)));
        }
    }
    out
}

/// A cheap hash of what the mask shows (the bits and the bounds): a
/// multiply-rotate fold over the words, not FNV byte by byte.
pub fn mask_hash(lens: &FogLens) -> u64 {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    let b: GridBounds = lens.grid.bounds();
    let mut h: u64 = 0x243f_6a88_85a3_08d3;
    let mut eat = |w: u64| h = (h ^ w).wrapping_mul(K).rotate_left(29);
    eat(u64::from(b.min.x as u32) | (u64::from(b.min.z as u32) << 32));
    eat(u64::from(b.width) | (u64::from(b.height) << 32));
    eat(u64::from(lens.grid.cell_mm() as u32));
    eat(u64::from(lens.explored.fits(b)) | (u64::from(lens.visible.fits(b)) << 1));
    for w in &lens.explored.bits {
        eat(*w);
    }
    eat(K);
    for w in &lens.visible.bits {
        eat(*w);
    }
    h
}

// ---------------------------------------------------------------------------
// Bevy side
// ---------------------------------------------------------------------------

/// The look decided for a visual root (sim-linked).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fogged(pub Look);

/// A view entity that never changes, fogged by the cells from `lo` to `hi`
/// (wall pieces, dressing).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct FogArea {
    pub lo: SimPos,
    pub hi: SimPos,
}

impl FogArea {
    /// The cells under `centre ± half` (metres, world xz), shrunk a hair so
    /// an edge on a cell border does not reach into the next cell.
    pub fn around(centre: Vec3, half: Vec3) -> Self {
        let half = (half.abs() - Vec3::splat(0.01)).max(Vec3::ZERO);
        let mm = |v: f32| (v * 1000.0).round() as i32;
        let (a, b) = (centre - half, centre + half);
        Self {
            lo: SimPos::new(mm(a.x.min(b.x)), mm(a.z.min(b.z))),
            hi: SimPos::new(mm(a.x.max(b.x)), mm(a.z.max(b.z))),
        }
    }
}

/// Whether this entity's meshes (down to the next `FogDim`) are dimmed.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FogDim(pub bool);

/// On a mesh swapped to its dimmed material: the original.
#[derive(Component, Clone, Debug)]
struct FogTinted(Handle<StandardMaterial>);

/// Dimmed copies of materials, by original.
#[derive(Resource, Default)]
struct DimMaterials(HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>);

/// Marks the mask plane.
#[derive(Component, Clone, Copy, Debug)]
struct FogMaskPlane;

/// Meshes to dim whose material was not loaded yet: retried every frame
/// until it is (or the mesh is gone).
#[derive(Resource, Default)]
struct DimPending(Vec<Entity>);

/// The mask texture.
#[derive(Resource)]
struct FogMask {
    image: Handle<Image>,
    region: Option<MaskRegion>,
    /// [`mask_hash`] of the texels written.
    hash: Option<u64>,
}

/// The fog's decide step (in `ViewSystems::Animate`, before the visuals).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FogSet;

/// The fog: F8, the looks, the mask and the dimming.
pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<FogOfWar>()
        .init_resource::<DimMaterials>()
        .init_resource::<DimPending>()
        .add_systems(Startup, spawn_mask)
        .add_systems(Update, toggle_fog.in_set(ViewSystems::Input))
        .add_systems(
            Update,
            decide_looks.in_set(FogSet).in_set(ViewSystems::Animate),
        )
        .add_systems(
            Update,
            (update_mask, apply_dim).in_set(ViewSystems::Present),
        );
}

/// F8 toggles fog.
fn toggle_fog(keys: Option<Res<ButtonInput<KeyCode>>>, mut fog: ResMut<FogOfWar>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::F8)) {
        fog.enabled = !fog.enabled;
    }
}

/// The played actor's fog components: the `RevealsFog` actor with the
/// lowest `SimId`.
type FogActor<'a> = (
    &'a SimId,
    &'a ExploredCells,
    &'a VisibleCells,
    &'a SeenStatics,
    Option<&'a PerceptionMemory>,
    Option<&'a MountedIn>,
);

fn lens_of<'a>(
    fog: &FogOfWar,
    grid: Option<&'a SpatialGrid>,
    actors: impl Iterator<Item = FogActor<'a>>,
) -> Option<FogLens<'a>> {
    if !fog.enabled {
        return None;
    }
    let grid = grid?;
    let (id, explored, visible, seen, memory, mounted) = actors.min_by_key(|a| *a.0)?;
    Some(FogLens {
        grid,
        explored,
        visible,
        seen,
        memory,
        actor: *id,
        riding: mounted.map(|m| m.vehicle),
    })
}

/// What changes the looks: the tick, the toggle and whether there is a
/// played actor.
type DirtyKey = (Option<u64>, bool, Option<SimId>);

#[allow(clippy::too_many_arguments)]
fn decide_looks(
    fog: Res<FogOfWar>,
    clock: Option<Res<SimClock>>,
    grid: Option<Res<SpatialGrid>>,
    actors: Query<FogActor, With<RevealsFog>>,
    sims: Query<(
        &SimId,
        &SimPos,
        Has<TestbedUnit>,
        Option<&Prop>,
        Has<Item>,
        Has<CarriedBy>,
    )>,
    fresh: Query<(), Or<(Added<VisualOf>, Added<FogArea>)>>,
    mut roots: Query<(&VisualOf, &mut Fogged, &mut FogDim, &mut Visibility), Without<FogArea>>,
    mut areas: Query<(&FogArea, &mut FogDim, &mut Visibility), Without<VisualOf>>,
    view: Option<ResMut<ViewHidden>>,
    mut last: Local<Option<DirtyKey>>,
) {
    let lens = lens_of(&fog, grid.as_deref(), actors.iter());
    let key = (
        clock.as_ref().map(|c| c.tick),
        fog.enabled,
        lens.map(|l| l.actor),
    );
    if *last == Some(key) && fresh.is_empty() {
        return;
    }
    *last = Some(key);
    let lens = lens.as_ref();
    let mut hidden = ViewHidden::default();
    for (of, mut fogged, mut dim, mut vis) in &mut roots {
        let Ok((&id, &pos, unit, prop, item, carried)) = sims.get(of.0) else {
            continue;
        };
        let look = match prop.map(|p| p.kind) {
            _ if unit => unit_look(lens, id),
            Some(PropKind::Cart | PropKind::Wagon) => unit_look(lens, id),
            // Its pieces are fogged one by one.
            Some(PropKind::Wall) => Look::Live,
            _ if item && carried => Look::Live,
            _ => static_look(lens, id, pos),
        };
        fogged.set_if_neq(Fogged(look));
        dim.set_if_neq(FogDim(look.dimmed()));
        if !look.shown() {
            hidden.0.insert(of.0);
        }
        // `item_transforms` owns an item's visibility (it reads `Fogged`).
        if !item {
            vis.set_if_neq(if look.shown() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    if let Some(mut view) = view {
        view.set_if_neq(hidden);
    }
    for (area, mut dim, mut vis) in &mut areas {
        let look = area_look(lens, area.lo, area.hi);
        dim.set_if_neq(FogDim(look.dimmed()));
        vis.set_if_neq(if look.shown() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn spawn_mask(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let image = images.add(mask_image(1, 1, vec![0, 0, 0, 0]));
    commands.spawn((
        Name::new("fog of war mask"),
        FogMaskPlane,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(image.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, MASK_Y, 0.0),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.insert_resource(FogMask {
        image,
        region: None,
        hash: None,
    });
}

fn mask_image(width: u32, height: u32, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Rewrites the mask texture when the bits changed (checked once per tick).
#[allow(clippy::too_many_arguments)]
fn update_mask(
    fog: Res<FogOfWar>,
    clock: Option<Res<SimClock>>,
    grid: Option<Res<SpatialGrid>>,
    actors: Query<FogActor, With<RevealsFog>>,
    mask: Option<ResMut<FogMask>>,
    mut images: ResMut<Assets<Image>>,
    mut planes: Query<(&mut Transform, &mut Visibility), With<FogMaskPlane>>,
    mut last: Local<Option<DirtyKey>>,
) {
    let Some(mut mask) = mask else { return };
    let lens = lens_of(&fog, grid.as_deref(), actors.iter());
    let key = (
        clock.as_ref().map(|c| c.tick),
        fog.enabled,
        lens.map(|l| l.actor),
    );
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let Ok((mut transform, mut vis)) = planes.single_mut() else {
        return;
    };
    let Some(lens) = lens else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    vis.set_if_neq(Visibility::Inherited);
    let region = MaskRegion::covering(lens.grid);
    let hash = mask_hash(&lens);
    if mask.region == Some(region) && mask.hash == Some(hash) {
        return;
    }
    let texels = mask_texels(Some(&lens), region);
    let cell = lens.grid.cell_mm() as f32 / 1000.0;
    if mask.region != Some(region) {
        let image = mask_image(region.width, region.height, texels);
        if let Err(e) = images.insert(&mask.image, image) {
            warn!("testbed_view: fog mask texture: {e}");
            return;
        }
        let (w, h) = (region.width as f32 * cell, region.height as f32 * cell);
        let (x0, z0) = (region.min.x as f32 * cell, region.min.z as f32 * cell);
        transform.translation = Vec3::new(x0 + w / 2.0, MASK_Y, z0 + h / 2.0);
        transform.scale = Vec3::new(w, 1.0, h);
        mask.region = Some(region);
    } else if let Some(mut image) = images.get_mut(&mask.image) {
        image.data = Some(texels);
    }
    mask.hash = Some(hash);
}

/// Swaps the materials of meshes under an entity whose [`FogDim`] changed
/// (or that just appeared under a dimmed one) for darkened copies, or back.
/// A mesh whose material is not loaded yet waits in [`DimPending`].
#[allow(clippy::too_many_arguments)]
fn apply_dim(
    mut commands: Commands,
    changed: Query<Entity, Changed<FogDim>>,
    dims: Query<&FogDim>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut meshes: ParamSet<(
        Query<Entity, Added<MeshMaterial3d<StandardMaterial>>>,
        Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&FogTinted>)>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<DimMaterials>,
    mut pending: ResMut<DimPending>,
) {
    let dim_of = |e: Entity| -> bool {
        let mut at = e;
        loop {
            if let Ok(d) = dims.get(at) {
                return d.0;
            }
            match parents.get(at) {
                Ok(p) => at = p.parent(),
                Err(_) => return false,
            }
        }
    };
    let mut todo: Vec<Entity> = std::mem::take(&mut pending.0);
    todo.extend(meshes.p0().iter());
    for e in &changed {
        todo.push(e);
        todo.extend(children.iter_descendants(e));
    }
    let mut meshes = meshes.p1();
    for e in todo {
        let Ok((mut mat, tinted)) = meshes.get_mut(e) else {
            continue;
        };
        match (dim_of(e), tinted) {
            (true, None) => {
                let original = mat.0.clone();
                let dimmed = match cache.0.get(&original.id()) {
                    Some(d) => d.clone(),
                    None => {
                        // Not loaded yet: tried again next frame.
                        let Some(copy) = materials.get(&original).map(dimmed_material) else {
                            if !pending.0.contains(&e) {
                                pending.0.push(e);
                            }
                            continue;
                        };
                        let d = materials.add(copy);
                        cache.0.insert(original.id(), d.clone());
                        d
                    }
                };
                mat.0 = dimmed;
                commands.entity(e).insert(FogTinted(original));
            }
            (false, Some(t)) => {
                mat.0 = t.0.clone();
                commands.entity(e).remove::<FogTinted>();
            }
            _ => {}
        }
    }
}

/// A darkened, greyer copy of `m`.
fn dimmed_material(m: &StandardMaterial) -> StandardMaterial {
    let c = m.base_color.to_linear();
    let grey = 0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue;
    let mix = |v: f32| (v + (grey - v) * DIM_GREY) * DIM_KEEP;
    let mut out = m.clone();
    out.base_color = Color::LinearRgba(LinearRgba::new(
        mix(c.red),
        mix(c.green),
        mix(c.blue),
        c.alpha,
    ));
    out.emissive = LinearRgba::BLACK;
    out
}

#[cfg(test)]
mod tests {
    use npc_framework::perception::{Percept, SenseKind};
    use npc_framework::prelude::Footprint;

    use super::*;

    /// A 1 m grid of 8 × 8 cells (-4..4) and a played actor `#1`.
    struct World {
        grid: SpatialGrid,
        explored: ExploredCells,
        visible: VisibleCells,
        seen: SeenStatics,
        memory: PerceptionMemory,
    }

    impl World {
        fn new() -> Self {
            let bounds = GridBounds::centered(8, 8);
            Self {
                grid: SpatialGrid::new(1000, bounds),
                explored: ExploredCells::sized(bounds),
                visible: VisibleCells {
                    bits: vec![0; ExploredCells::sized(bounds).bits.len()],
                },
                seen: SeenStatics::default(),
                memory: PerceptionMemory::default(),
            }
        }

        fn set(bits: &mut [u64], bounds: GridBounds, cell: CellCoord) {
            let i = bounds.index(cell).unwrap();
            bits[(i / 64) as usize] |= 1 << (i % 64);
        }

        fn explore(&mut self, x: i32, z: i32) {
            let b = self.grid.bounds();
            Self::set(&mut self.explored.bits, b, CellCoord::new(x, z));
        }

        fn see(&mut self, x: i32, z: i32) {
            self.explore(x, z);
            let b = self.grid.bounds();
            Self::set(&mut self.visible.bits, b, CellCoord::new(x, z));
        }

        fn sense(&mut self, id: u32, sensed: bool) {
            self.memory.percepts.insert(
                SimId(id),
                Percept {
                    last_seen_tick: 0,
                    pos: SimPos::new(0, 0),
                    via: SenseKind::Vision,
                    confidence: 1000,
                    kinds: 1,
                    sensed,
                },
            );
        }

        fn lens(&self) -> FogLens<'_> {
            FogLens {
                grid: &self.grid,
                explored: &self.explored,
                visible: &self.visible,
                seen: &self.seen,
                memory: Some(&self.memory),
                actor: SimId(1),
                riding: None,
            }
        }
    }

    /// The centre of cell `(x, z)`.
    fn at(x: i32, z: i32) -> SimPos {
        SimPos::new(x * 1000 + 500, z * 1000 + 500)
    }

    fn door(pos: SimPos, open: bool) -> SeenState {
        SeenState {
            kind: 1 << 2,
            pos,
            open: Some(open),
            felled: None,
            footprint: None,
        }
    }

    #[test]
    fn mask_values_for_three_states() {
        assert_eq!(
            mask_alpha(CellFog::Unexplored),
            255,
            "never explored: black"
        );
        assert_eq!(mask_alpha(CellFog::Visible), 0, "visible: clear");
        let grey = mask_alpha(CellFog::Explored);
        assert!(
            grey > MASK_VISIBLE && grey < MASK_UNEXPLORED,
            "explored: greyed, between clear and black, {grey}"
        );

        let mut w = World::new();
        w.explore(0, 0);
        w.see(1, 0);
        let lens = w.lens();
        assert_eq!(lens.cell(CellCoord::new(0, 0)), CellFog::Explored);
        assert_eq!(lens.cell(CellCoord::new(1, 0)), CellFog::Visible);
        assert_eq!(lens.cell(CellCoord::new(2, 0)), CellFog::Unexplored);
        // Outside the grid: never explored.
        assert_eq!(lens.cell(CellCoord::new(40, 0)), CellFog::Unexplored);

        let region = MaskRegion {
            min: CellCoord::new(-1, 0),
            width: 4,
            height: 1,
        };
        let texels = mask_texels(Some(&lens), region);
        let alphas: Vec<u8> = texels.chunks(4).map(|t| t[3]).collect();
        assert_eq!(alphas, vec![255, grey, 0, 255]);
        // Black where never explored, dark grey where explored.
        assert_eq!(texels[..3], [0, 0, 0]);
        assert_eq!(texels[4..7], MASK_GREY);
        assert_eq!(texels[12..15], [0, 0, 0]);

        // The hash moves with the bits, and only with them.
        let before = mask_hash(&w.lens());
        assert_eq!(before, mask_hash(&w.lens()));
        w.explore(-2, -2);
        assert_ne!(before, mask_hash(&w.lens()));

        // The region covers the grid and the ground.
        let r = MaskRegion::covering(&w.grid);
        assert!(r.min.x <= -MASK_MIN_HALF_CELLS && r.min.z <= -MASK_MIN_HALF_CELLS);
        assert!(r.min.x + r.width as i32 >= MASK_MIN_HALF_CELLS);
    }

    #[test]
    fn unsensed_units_hidden_in_explored_and_dark_ground() {
        let mut w = World::new();
        w.see(0, 0);
        w.explore(3, 3);
        // Unit #7 stands in explored ground the actor does not sense now:
        // hidden. Unit #8 is out in never-explored ground: hidden.
        w.sense(7, false);
        let lens = w.lens();
        assert_eq!(unit_look(Some(&lens), SimId(7)), Look::Hidden);
        assert_eq!(unit_look(Some(&lens), SimId(8)), Look::Hidden);
        // The played actor always shows.
        assert_eq!(unit_look(Some(&lens), SimId(1)), Look::Live);
    }

    #[test]
    fn units_hidden_unless_sensed() {
        let mut w = World::new();
        // Every cell is visible: still only sensed units show (R14).
        for x in -4..4 {
            for z in -4..4 {
                w.see(x, z);
            }
        }
        w.sense(5, true);
        w.sense(6, false);
        let mut lens = w.lens();
        assert_eq!(unit_look(Some(&lens), SimId(5)), Look::Live);
        assert_eq!(unit_look(Some(&lens), SimId(6)), Look::Hidden, "remembered");
        assert_eq!(unit_look(Some(&lens), SimId(9)), Look::Hidden, "unknown");
        assert_eq!(unit_look(Some(&lens), SimId(1)), Look::Live, "the actor");

        // A sensed unit in a cell the fog calls not visible still shows.
        let w2 = {
            let mut w2 = World::new();
            w2.sense(5, true);
            w2
        };
        assert_eq!(unit_look(Some(&w2.lens()), SimId(5)), Look::Live);

        // The ridden vehicle shows; without a memory only the actor does.
        lens.riding = Some(SimId(12));
        lens.memory = None;
        assert_eq!(unit_look(Some(&lens), SimId(12)), Look::Live);
        assert_eq!(unit_look(Some(&lens), SimId(5)), Look::Hidden);
    }

    #[test]
    fn statics_use_last_seen_state() {
        let mut w = World::new();
        w.see(0, 0);
        w.explore(2, 0);
        w.explore(-2, 0);
        // Door #20 was seen open at (2, 0) and is closed now (the live state
        // the view would otherwise draw): it stays open, as last seen.
        w.seen.0.insert(SimId(20), door(at(2, 0), true));
        // Tree #21 was seen standing; felled since.
        let tree = SeenState {
            kind: 1 << 3,
            pos: at(-2, 0),
            open: None,
            felled: Some(false),
            footprint: None,
        };
        w.seen.0.insert(SimId(21), tree);
        let lens = w.lens();
        assert_eq!(
            static_look(Some(&lens), SimId(20), at(2, 0)),
            Look::LastSeen(door(at(2, 0), true))
        );
        let look = static_look(Some(&lens), SimId(21), at(-2, 0));
        assert_eq!(look, Look::LastSeen(tree));
        assert!(look.dimmed() && look.shown());
        // An item seen at (2, 0) and moved out of sight is drawn where it
        // was seen.
        w.seen.0.insert(SimId(22), door(at(2, 0), false));
        let lens = w.lens();
        assert_eq!(
            static_look(Some(&lens), SimId(22), at(3, 3)),
            Look::LastSeen(door(at(2, 0), false))
        );
        // Never seen, in explored ground: not drawn.
        assert_eq!(static_look(Some(&lens), SimId(23), at(2, 0)), Look::Hidden);
        // In a visible cell: as it is now.
        assert_eq!(static_look(Some(&lens), SimId(20), at(0, 0)), Look::Live);
        assert_eq!(static_look(Some(&lens), SimId(24), at(0, 0)), Look::Live);

        // Wall pieces and dressing by their cells.
        assert_eq!(area_look(Some(&lens), at(-1, 0), at(1, 0)), Look::Live);
        assert_eq!(area_look(Some(&lens), at(2, 0), at(2, 0)), Look::Dim);
        assert_eq!(area_look(Some(&lens), at(3, 3), at(3, 3)), Look::Hidden);
        let wall = SeenState {
            footprint: Some(Footprint::default()),
            ..door(at(2, 0), false)
        };
        assert!(Look::LastSeen(wall).dimmed() && !Look::Live.dimmed());
    }

    /// Ruling R15: the F1 item marker follows the item's look.
    #[test]
    fn item_markers_follow_the_look() {
        let here = at(3, 3);
        let seen = door(at(2, 0), false);
        assert_eq!(marker_pos(Look::Hidden, here), None, "hidden: no marker");
        assert_eq!(marker_pos(Look::LastSeen(seen), here), Some(at(2, 0)));
        assert_eq!(marker_pos(Look::Live, here), Some(here));
        assert_eq!(marker_pos(Look::Dim, here), Some(here));
        // Fog off: every look is live, so every marker is where it is.
        assert_eq!(
            marker_pos(static_look(None, SimId(3), here), here),
            Some(here)
        );
    }

    #[test]
    fn fog_off_shows_everything() {
        // Fog off: no lens.
        assert_eq!(unit_look(None, SimId(5)), Look::Live);
        assert_eq!(static_look(None, SimId(20), at(2, 0)), Look::Live);
        assert_eq!(area_look(None, at(3, 3), at(3, 3)), Look::Live);
        let region = MaskRegion {
            min: CellCoord::new(0, 0),
            width: 3,
            height: 2,
        };
        assert!(mask_texels(None, region).iter().all(|&b| b == 0));
        let w = World::new();
        let actors = [(
            &SimId(1),
            &w.explored,
            &w.visible,
            &w.seen,
            Some(&w.memory),
            None,
        )];
        let off = FogOfWar { enabled: false };
        assert!(lens_of(&off, Some(&w.grid), actors.into_iter()).is_none());
        let on = FogOfWar { enabled: true };
        assert!(lens_of(&on, Some(&w.grid), actors.into_iter()).is_some());
        // On, but nobody reveals fog (a testbed scenario): no fog either.
        assert!(lens_of(&on, Some(&w.grid), std::iter::empty()).is_none());
    }
}
