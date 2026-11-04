//! Presentation assets. Asset handles and atlas coordinates never enter simulation state.
use crate::animation::ActorPose;
use bevy::prelude::*;
use bevy_egui::egui::{Color32, Painter, Pos2, Rect, TextureId, Vec2, pos2, vec2};
use horde_core::{ordnance::OrdnanceKind, weapons::WeaponClass};
// Characters and equipment previews share the source-owned model resolver.
mod player_preview;
pub use player_preview::defender_preview;
#[cfg(feature = "review")]
pub use player_preview::{hold_sample, paint_model};

/// Source catalogue selection is an application compatibility seam, not a
/// gender-specific painter. The production manifest can remap these keys.
pub fn player_model_id(appearance: horde_core::survivors::Appearance) -> Result<String, String> {
    crate::player_model::startup::model_for_appearance_key(
        &format!("{appearance:?}").to_ascii_lowercase(),
    )
}
pub fn player_model_placement(
    model_id: &str,
    center: Pos2,
    size: Vec2,
) -> Result<crate::player_model::ModelPlacement, String> {
    if !center.x.is_finite() || !center.y.is_finite() || !size.y.is_finite() || size.y <= 0. {
        return Err("Invalid player display placement".into());
    }
    let catalogue = crate::player_model::startup::active_or_bundled()?;
    let model = catalogue
        .model(model_id)
        .ok_or("Unknown displayed player model")?;
    let frame = model.definition.source.layout_frame;
    Ok(crate::player_model::ModelPlacement {
        source_origin: [center.x, center.y + size.y * 0.5],
        scale: size.y / (frame[3] - frame[1]) as f32,
    })
}
pub fn resolve_player_model(
    model_id: &str,
    equipment: &horde_core::survivors::EquipmentVisual,
    sample: crate::player_model::PlayerPoseSample,
    center: Pos2,
    size: Vec2,
) -> Result<crate::player_model::ResolvedPlayerPose, String> {
    let catalogue = crate::player_model::startup::active_or_bundled()?;
    crate::player_model::resolve_equipped_model(
        catalogue,
        model_id,
        equipment,
        sample,
        player_model_placement(model_id, center, size)?,
    )
}
pub fn draw_player_model(
    painter: &Painter,
    pose: &crate::player_model::ResolvedPlayerPose,
) -> Result<(), String> {
    let textures = crate::player_model::backend_egui::textures(painter.ctx())?;
    crate::player_model::draw_egui(painter, pose, &textures)
}
pub fn project_player_socket(
    model_id: &str,
    source_point: [f32; 2],
    center: Pos2,
    size: Vec2,
) -> Result<Pos2, String> {
    let catalogue = crate::player_model::startup::active_or_bundled()?;
    let model = catalogue
        .model(model_id)
        .ok_or("Unknown released player model")?;
    let placement = player_model_placement(model_id, center, size)?;
    let point = crate::player_model::SourceToDisplay {
        source_origin: placement.source_origin,
        source_ground: model.definition.ground,
        scale: placement.scale,
    }
    .point(source_point);
    Ok(pos2(point[0], point[1]))
}
/// Invoke after the primary context has its actual RenderDevice texture limit.
pub fn prepare_player_models(context: &bevy_egui::egui::Context) -> Result<(), String> {
    crate::player_model::backend_egui::textures(context).map(|_| ())
}
mod atlas;
pub mod enemy_visuals;
mod flash;
mod ground_material;
mod loading;
#[cfg(test)]
mod tests;
mod validation;
use atlas::*;
pub use atlas::{RigAtlas, RigSheet, WalkSheet};
use enemy_visuals::{EnemySprite, SpriteArt, WalkArt};
#[cfg(test)]
use flash::{FlashMask, hit_flash_color};
pub use flash::{FlashMasks, HitFlash, actor_hit_flash};
#[cfg(feature = "review")]
pub use loading::VisualAssets;
pub use loading::{load_assets, refresh_art};
use validation::*;

/// Egui texture ids and derived data for the loaded art, republished each
/// frame by `loading::refresh_art`.
#[derive(Resource, Clone, Copy, Default, Debug)]
pub struct ArtTextures {
    pub actors: Option<TextureId>,
    pub props: Option<TextureId>,
    pub keyart: Option<TextureId>,
    pub ground: Option<TextureId>,
    pub walkers: Option<WalkSheet>,
    /// The loaded sheets do not match `enemy_visuals.json`: draw fallback figures.
    pub enemy_art_rejected: bool,
    /// The actor sheet does not match the survivor/portrait/corpse cells.
    pub survivor_art_rejected: bool,
    /// Weapon and ordnance icon atlas (legacy defender rig sheet; no bodies are drawn from it).
    pub rig: Option<RigSheet>,
    pub menu_idle: Option<TextureId>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct PropMotion {
    pub angle: f32,
    pub target: Option<Pos2>,
    pub recoil: f32,
    pub trigger: f32,
    pub hit: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Actor {
    Player,
    Mara,
    /// Data-driven enemy art from `assets/art/enemy_visuals.json`.
    Enemy(&'static EnemySprite),
}
impl Actor {
    pub fn enemy(sprite: horde_core::enemy_catalog::VisualSprite) -> Option<Self> {
        enemy_visuals::active().get(sprite).map(Actor::Enemy)
    }
    fn survivor_index(self) -> Option<usize> {
        match self {
            Actor::Player => Some(0),
            Actor::Mara => Some(1),
            Actor::Enemy(_) => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Trap,
    Wall,
    Turret,
    Rifle,
    Melee,
    Vest,
    Pack,
    Medical,
    TreeBroadleaf,
    TreePine,
    Bush,
    Fence,
    Wreck,
    Debris,
    Rock,
    Lamp,
}
fn weapon_index(class: WeaponClass) -> usize {
    match class {
        WeaponClass::Pistol => 0,
        WeaponClass::Smg => 1,
        WeaponClass::Shotgun => 2,
        WeaponClass::Rifle => 3,
        WeaponClass::Marksman => 4,
        WeaponClass::MachineGun => 5,
    }
}
fn rig_region(
    p: &Painter,
    sheet: RigSheet,
    source: [f32; 4],
    rect: Rect,
    tint: Color32,
    transform: impl Fn(Pos2) -> Pos2,
) {
    let mut mesh = bevy_egui::egui::epaint::Mesh::with_texture(sheet.texture);
    mesh.add_rect_with_uv(rect, region_for(source, sheet.atlas.size), tint);
    for vertex in &mut mesh.vertices {
        vertex.pos = transform(vertex.pos);
    }
    p.add(bevy_egui::egui::Shape::mesh(mesh));
}
const MENU_IDLE_SIZE: [f32; 2] = [1774.0, 887.0];
const MENU_IDLE_CROPS: [[f32; 4]; 2] = [[282., 42., 652., 848.], [1128., 60., 1465., 848.]];
fn menu_idle_layout(female: bool, foot: Pos2, height: f32) -> ([f32; 4], Rect, Pos2) {
    let source = MENU_IDLE_CROPS[usize::from(female)];
    let scale = height / (848.0 - source[1]);
    let source_foot = pos2(source[0] + (source[2] - source[0]) * 0.53, 848.0);
    let rect = Rect::from_min_size(
        foot - (source_foot - pos2(source[0], source[1])) * scale,
        vec2(source[2] - source[0], source[3] - source[1]) * scale,
    );
    let waist = rect.min + vec2(0.0, (500.0 - source[1]) * scale);
    (source, rect, waist)
}
/// Cohesive authored menu pose. Hands and pistol remain one source image; only
/// the whole upper body breathes around its waist while both boots stay planted.
pub fn menu_survivor(
    p: &Painter,
    textures: ArtTextures,
    female: bool,
    foot: Pos2,
    height: f32,
    breath: f32,
) -> bool {
    let Some(texture) = textures.menu_idle else {
        return false;
    };
    let (source, rect, waist) = menu_idle_layout(female, foot, height);
    let lower = [source[0], 500.0, source[2], source[3]];
    p.image(
        texture,
        crop_destination(rect, source, lower),
        region_for(lower, MENU_IDLE_SIZE),
        Color32::WHITE,
    );
    let upper = [source[0], source[1], source[2], 500.0];
    let mut mesh = bevy_egui::egui::Mesh::with_texture(texture);
    mesh.add_rect_with_uv(
        crop_destination(rect, source, upper),
        region_for(upper, MENU_IDLE_SIZE),
        Color32::WHITE,
    );
    for vertex in &mut mesh.vertices {
        let delta = vertex.pos.y - waist.y;
        vertex.pos.y = waist.y + delta * (1.0 + breath * 0.4);
        vertex.pos.x -= delta * breath * 0.08;
    }
    p.add(bevy_egui::egui::Shape::mesh(mesh));
    true
}

pub fn firearm_icon(
    p: &Painter,
    textures: ArtTextures,
    class: WeaponClass,
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let Some(sheet) = textures.rig else {
        return false;
    };
    let source = sheet.atlas.cells[[4, 5, 7, 6, 9, 8][weapon_index(class)]];
    rig_region(p, sheet, source, fit(center, size, source), tint, |point| {
        point
    });
    true
}
pub fn ordnance_sprite(
    p: &Painter,
    textures: ArtTextures,
    kind: OrdnanceKind,
    center: Pos2,
    size: Vec2,
    angle: f32,
) -> bool {
    let Some(sheet) = textures.rig else {
        return false;
    };
    let source = sheet.atlas.cells[if kind == OrdnanceKind::Grenade {
        10
    } else {
        11
    }];
    let rotation = bevy_egui::egui::emath::Rot2::from_angle(angle);
    rig_region(
        p,
        sheet,
        source,
        fit(center, size, source),
        Color32::WHITE,
        |point| center + rotation * (point - center),
    );
    true
}
pub fn ordnance_effect(
    p: &Painter,
    textures: ArtTextures,
    flame: bool,
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let Some(sheet) = textures.rig else {
        return false;
    };
    let source = sheet.atlas.cells[if flame { 13 } else { 12 }];
    rig_region(p, sheet, source, fit(center, size, source), tint, |point| {
        point
    });
    true
}
pub fn actor(
    p: &Painter,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    actor_animated(p, textures, kind, center, size, tint, &ActorPose::default())
}
pub fn actor_animated(
    p: &Painter,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    tint: Color32,
    pose: &ActorPose,
) -> bool {
    let (texture, rect, uv, pivot) = match kind {
        Actor::Enemy(_) if textures.enemy_art_rejected => return false,
        Actor::Enemy(sprite) => match &sprite.art {
            SpriteArt::Walk(walk) => {
                let Some(sheet) = textures.walkers else {
                    return false;
                };
                let (rect, uv, pivot) = walk_frame_layout(
                    walk,
                    sprite.source_size,
                    sheet.size,
                    pose.frame,
                    center,
                    size,
                );
                (Some(sheet.texture), rect, uv, pivot)
            }
            SpriteArt::Standing(art) => {
                let (rect, pivot) = standing_actor_layout(
                    art.bounds,
                    [art.hip_x, art.feet_y, art.height],
                    center,
                    size,
                );
                (
                    textures.actors,
                    rect,
                    region_for(art.bounds, sprite.source_size),
                    pivot,
                )
            }
        },
        Actor::Player | Actor::Mara if textures.survivor_art_rejected => return false,
        Actor::Player | Actor::Mara => {
            let index = kind.survivor_index().unwrap_or_default();
            let source = SURVIVOR_RECTS[index];
            let (rect, pivot) =
                standing_actor_layout(source, SURVIVOR_ANCHORS[index], center, size);
            (textures.actors, rect, region_for(source, ACTOR_SIZE), pivot)
        }
    };
    let Some(texture) = texture else {
        return false;
    };
    let mut mesh = bevy_egui::egui::epaint::Mesh::with_texture(texture);
    mesh.add_rect_with_uv(rect, uv, tint);
    apply_actor_pose(&mut mesh, pose, pivot);
    p.add(bevy_egui::egui::Shape::mesh(mesh));
    true
}
fn apply_actor_pose(mesh: &mut bevy_egui::egui::epaint::Mesh, pose: &ActorPose, pivot: Pos2) {
    for vertex in &mut mesh.vertices {
        let relative = vertex.pos - pivot;
        vertex.pos = pivot
            + vec2(
                relative.x * pose.scale[0] - relative.y * pose.lean * 0.45,
                relative.y * pose.scale[1],
            );
    }
    mesh.rotate(
        bevy_egui::egui::emath::Rot2::from_angle(pose.lean * 0.5),
        pivot,
    );
}

fn standing_actor_layout(
    source: [f32; 4],
    [hip, feet, height]: [f32; 3],
    center: Pos2,
    available: Vec2,
) -> (Rect, Pos2) {
    let scale = (available.y / height).min(available.x / (source[2] - source[0]));
    let foot = center + vec2(0., available.y * 0.5);
    let rect = Rect::from_min_max(
        foot + vec2((source[0] - hip) * scale, (source[1] - feet) * scale),
        foot + vec2((source[2] - hip) * scale, (source[3] - feet) * scale),
    );
    (rect, foot)
}
fn walk_frame_layout(
    walk: &WalkArt,
    source_size: [f32; 2],
    size: [u32; 2],
    frame: u8,
    center: Pos2,
    available: Vec2,
) -> (Rect, Rect, Pos2) {
    let frame = walk.frames[frame as usize % walk.frames.len()];
    let max_width = walk
        .frames
        .iter()
        .map(|frame| frame.bounds[2] - frame.bounds[0])
        .fold(0., f32::max);
    let scale = (available.y / walk.height).min(available.x / max_width);
    let foot = center + vec2(0., available.y * 0.5);
    let rect = Rect::from_min_max(
        foot + vec2(
            (frame.bounds[0] - frame.hip_x) * scale,
            (frame.bounds[1] - walk.feet_y) * scale,
        ),
        foot + vec2(
            (frame.bounds[2] - frame.hip_x) * scale,
            (frame.bounds[3] - walk.feet_y) * scale,
        ),
    );
    // Pixel-space metadata remains valid for a uniformly rescaled copy of the
    // complete atlas; the measured common baseline never follows a lifted foot.
    let gutter = vec2(0.5 / size[0].max(1) as f32, 0.5 / size[1].max(1) as f32);
    let uv = Rect::from_min_max(
        pos2(
            frame.bounds[0] / source_size[0],
            frame.bounds[1] / source_size[1],
        ) + gutter,
        pos2(
            frame.bounds[2] / source_size[0],
            frame.bounds[3] / source_size[1],
        ) - gutter,
    );
    (rect, uv, foot)
}
/// Every settled body shares one draw mesh. Count and lifetime come from the
/// saved attempt; only the short collapse animation is rendered separately.
pub fn corpse_batch(
    p: &Painter,
    textures: ArtTextures,
    bodies: impl IntoIterator<Item = (Pos2, Vec2, Color32)>,
) -> bool {
    let Some(texture) = textures.actors.filter(|_| !textures.survivor_art_rejected) else {
        return false;
    };
    let source = CORPSE_RECT;
    let uv = region_for(source, ACTOR_SIZE);
    let mut mesh = bevy_egui::egui::epaint::Mesh::with_texture(texture);
    for (center, size, tint) in bodies {
        mesh.add_rect_with_uv(fit(center, size, source), uv, tint);
    }
    if !mesh.vertices.is_empty() {
        p.add(bevy_egui::egui::Shape::mesh(mesh));
    }
    true
}
/// Portrait artwork shares the current survivor sheet and nearest sampler.
pub fn portrait(p: &Painter, textures: ArtTextures, npc: bool, center: Pos2, size: Vec2) -> bool {
    actor_region(
        p,
        textures,
        PORTRAIT_RECTS[usize::from(npc)],
        center,
        size,
        Color32::WHITE,
    )
}
fn actor_region(
    p: &Painter,
    textures: ArtTextures,
    source: [f32; 4],
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let Some(texture) = textures.actors.filter(|_| !textures.survivor_art_rejected) else {
        return false;
    };
    p.image(
        texture,
        fit(center, size, source),
        region_for(source, ACTOR_SIZE),
        tint,
    );
    true
}
pub fn prop(
    p: &Painter,
    textures: ArtTextures,
    kind: Prop,
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let Some(texture) = textures.props else {
        return false;
    };
    let source = PROP_RECTS[kind as usize];
    p.image(texture, fit(center, size, source), region(source), tint);
    true
}
/// Full fitted source bounds, including transparent padding. Placement checks
/// use this same rectangle as rendering rather than only a trunk/center point.
pub fn prop_ground_rect(kind: Prop, foot: Pos2, size: Vec2) -> Rect {
    let fitted = fit(Pos2::ZERO, size, PROP_RECTS[kind as usize]);
    Rect::from_center_size(foot - vec2(0.0, fitted.height() * 0.5), fitted.size())
}
pub fn prop_grounded(
    p: &Painter,
    textures: ArtTextures,
    kind: Prop,
    foot: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let rect = prop_ground_rect(kind, foot, size);
    prop(p, textures, kind, rect.center(), size, tint)
}
pub fn prop_animated(
    p: &Painter,
    textures: ArtTextures,
    kind: Prop,
    center: Pos2,
    size: Vec2,
    tint: Color32,
    motion: PropMotion,
) -> bool {
    let Some(texture) = textures.props else {
        return false;
    };
    let source = PROP_RECTS[kind as usize];
    let rect = fit(center, size, source);
    match kind {
        Prop::Turret => {
            // The tripod stays fixed; only the authored gun/receiver region turns.
            let base = [658., 174., 933., 291.];
            let head = [658., 57., 933., 174.];
            p.image(
                texture,
                crop_destination(rect, source, base),
                region(base),
                tint,
            );
            let transform = TurretTransform::new(rect, motion);
            let mut mesh = bevy_egui::egui::epaint::Mesh::with_texture(texture);
            mesh.add_rect_with_uv(crop_destination(rect, source, head), region(head), tint);
            for vertex in &mut mesh.vertices {
                vertex.pos = transform.point(vertex.pos);
            }
            p.add(bevy_egui::egui::Shape::mesh(mesh));
        }
        Prop::Trap => {
            let mut mesh = bevy_egui::egui::epaint::Mesh::with_texture(texture);
            mesh.add_rect_with_uv(rect, region(source), tint);
            let pivot = rect.center_bottom();
            for vertex in &mut mesh.vertices {
                vertex.pos.y = pivot.y + (vertex.pos.y - pivot.y) * (1. - motion.trigger * 0.2);
            }
            p.add(bevy_egui::egui::Shape::mesh(mesh));
        }
        _ => {
            p.image(texture, rect, region(source), tint);
        }
    }
    if motion.hit > 0. {
        p.rect_stroke(
            rect,
            1.,
            bevy_egui::egui::Stroke::new(
                1.,
                Color32::from_rgba_unmultiplied(227, 164, 108, (motion.hit * 180.) as u8),
            ),
            bevy_egui::egui::StrokeKind::Outside,
        );
    }
    true
}
// Source-space spindle, breech and barrel end from the pixel turret. These
// presentation-only calibration points share the head's rigid transform.
const TURRET_MOUNT: Pos2 = pos2(774., 174.);
const TURRET_BREECH: Pos2 = pos2(819., 99.);
const TURRET_MUZZLE: Pos2 = pos2(923., 99.);
struct TurretTransform {
    pivot: Pos2,
    mirror: bool,
    rotation: bevy_egui::egui::emath::Rot2,
    direction: Vec2,
    compression: f32,
}
impl TurretTransform {
    fn new(rect: Rect, motion: PropMotion) -> Self {
        let pivot = turret_source_point(rect, TURRET_MOUNT);
        let requested = motion.target.map_or_else(
            || vec2(motion.angle.cos(), motion.angle.sin()) * 10_000.,
            |target| target - pivot,
        );
        let mirror = requested.x < 0.;
        let reflect = |v: Vec2| if mirror { vec2(-v.x, v.y) } else { v };
        let bore = reflect(TURRET_MUZZLE - TURRET_BREECH).normalized();
        let muzzle = reflect(turret_source_point(rect, TURRET_MUZZLE) - pivot);
        // The bore sits above its mount. Correct that offset as well as its
        // authored elevation, so the muzzle ray passes through the target.
        let offset = bore.x * muzzle.y - bore.y * muzzle.x;
        let correction = (offset / requested.length().max(0.001))
            .clamp(-1., 1.)
            .asin();
        let heading = requested.y.atan2(requested.x) - correction;
        let direction = vec2(heading.cos(), heading.sin());
        Self {
            pivot,
            mirror,
            rotation: bevy_egui::egui::emath::Rot2::from_angle(heading - bore.y.atan2(bore.x)),
            direction,
            compression: motion.recoil.clamp(0., 1.) * 0.12,
        }
    }
    fn point(&self, point: Pos2) -> Pos2 {
        let mut relative = point - self.pivot;
        if self.mirror {
            relative.x = -relative.x;
        }
        relative = self.rotation * relative;
        // Axial compression recoils the barrel without moving its attachment
        // point or tripod, and leaves the muzzle ray on the same firing line.
        self.pivot + relative - self.direction * relative.dot(self.direction) * self.compression
    }
}
fn turret_source_point(rect: Rect, point: Pos2) -> Pos2 {
    let source = PROP_RECTS[Prop::Turret as usize];
    rect.min
        + vec2(
            (point.x - source[0]) / (source[2] - source[0]) * rect.width(),
            (point.y - source[1]) / (source[3] - source[1]) * rect.height(),
        )
}
/// Cosmetic muzzle position only; authoritative shot origin stays in the model.
pub fn turret_muzzle(center: Pos2, size: Vec2, motion: PropMotion) -> Pos2 {
    let rect = fit(center, size, PROP_RECTS[Prop::Turret as usize]);
    TurretTransform::new(rect, motion).point(turret_source_point(rect, TURRET_MUZZLE))
}
fn crop_destination(full: Rect, source: [f32; 4], crop: [f32; 4]) -> Rect {
    let width = source[2] - source[0];
    let height = source[3] - source[1];
    Rect::from_min_max(
        full.min
            + vec2(
                (crop[0] - source[0]) / width * full.width(),
                (crop[1] - source[1]) / height * full.height(),
            ),
        full.min
            + vec2(
                (crop[2] - source[0]) / width * full.width(),
                (crop[3] - source[1]) / height * full.height(),
            ),
    )
}
fn keyart_uv(r: Rect) -> Rect {
    let target_ratio = r.width() / r.height();
    if target_ratio > KEYART_RATIO {
        Rect::from_center_size(pos2(0.5, 0.5), vec2(1.0, KEYART_RATIO / target_ratio))
    } else {
        Rect::from_center_size(pos2(0.5, 0.5), vec2(target_ratio / KEYART_RATIO, 1.0))
    }
}
/// A fraction of source-image height, including any vertical cover crop.
pub fn keyart_source_height(r: Rect, fraction: f32) -> f32 {
    r.height() * fraction / keyart_uv(r).height()
}
/// Convert authored checkpoint anchors through the same cover crop as the image.
pub fn keyart_point(r: Rect, normalized: [f32; 2]) -> Pos2 {
    let uv = keyart_uv(r);
    r.min
        + vec2(
            (normalized[0] - uv.left()) / uv.width() * r.width(),
            (normalized[1] - uv.top()) / uv.height() * r.height(),
        )
}
/// Cover a menu canvas without stretching the cinematic composition.
pub fn keyart(p: &Painter, r: Rect, textures: ArtTextures) -> bool {
    let Some(texture) = textures.keyart else {
        return false;
    };
    let uv = keyart_uv(r);
    p.image(texture, r, uv, Color32::WHITE);
    true
}
pub fn ground(p: &Painter, r: Rect, textures: ArtTextures, theme: usize) -> bool {
    ground_material::ground(p, r, textures, theme)
}

pub fn panorama(p: &Painter, r: Rect, textures: ArtTextures, _biome: &str) -> bool {
    let Some(texture) = textures.keyart else {
        return false;
    };
    // The regional briefing shows the outpost horizon from the current pixel
    // world; historical painted panoramas remain archived and are never loaded.
    let height = (KEYART_RATIO * 0.7 / (r.width() / r.height())).min(0.5);
    let uv = Rect::from_center_size(pos2(0.65, 0.32), vec2(0.7, height));
    p.image(texture, r, uv, Color32::WHITE);
    true
}
