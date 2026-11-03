//! Read-only survivor component graph. Socket/IK geometry owns both pixels and muzzle.
//! Parts are independent original sprites; equipment is a frozen core snapshot.
use super::{
    ArtTextures, DefenderRigMotion, DefenderRigStyle, RigAtlas, RigSheet, region_for, weapon_index,
};
#[cfg(test)]
use bevy::prelude::Image;
use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, TextureId, Vec2, pos2, vec2};
use horde_core::survivors::EquipmentVisual;
use horde_core::weapons::WeaponClass;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug)]
pub struct ComponentAtlas {
    size: [f32; 2],
    bounds: [[f32; 4]; 16],
}
#[derive(Clone, Copy, Debug)]
pub struct ComponentSheet {
    pub texture: TextureId,
    pub atlas: &'static ComponentAtlas,
}
#[path = "character_rig/armature.rs"]
mod armature;
pub use armature::{ArmPose, HandPose, HandSockets, solve_arm};
#[path = "character_rig/stance.rs"]
mod stance;
use stance::{BattleStance, BodyResponse, StancePose};
#[path = "character_rig/limbs.rs"]
mod limbs;
use limbs::{LegSource, leg_slices};
#[path = "character_rig/canonical.rs"]
pub mod canonical;
#[cfg(test)]
#[path = "character_rig/coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
static COMPONENTS: OnceLock<ComponentAtlas> = OnceLock::new();
static CONFIG: OnceLock<RigDefinition> = OnceLock::new();
#[derive(Deserialize)]
struct RigDefinition {
    version: u8,
    logical_height: f32,
    stance: BattleStance,
    leg_sources: BTreeMap<String, LegSource>,
    upper_arm: f32,
    forearm: f32,
    parts: BTreeMap<String, usize>,
    hands: BTreeMap<String, HandSockets>,
    part_layouts: BTreeMap<String, PartLayout>,
    weapons: Vec<WeaponDefinition>,
    gear_patterns: BTreeMap<String, Vec<String>>,
}
#[derive(Deserialize)]
struct PartLayout {
    size: [f32; 2],
    pivot: [f32; 2],
    #[serde(default)]
    offset: [f32; 2],
    #[serde(default = "full_clip")]
    source_clip: [f32; 4],
    #[serde(default)]
    left_contour: Vec<[f32; 2]>,
}
fn full_clip() -> [f32; 4] {
    [0., 0., 1., 1.]
}
#[derive(Deserialize)]
struct WeaponDefinition {
    class: WeaponClass,
    cell: usize,
    width: f32,
    grip: [f32; 2],
    support: [f32; 2],
    stock: Option<[f32; 2]>,
    muzzle: [f32; 2],
    breech: [f32; 2],
    magazine: [f32; 2],
    support_extension: f32,
    pistol_extension: f32,
    dominant_rotation: f32,
    support_rotation: f32,
}
fn definition() -> &'static RigDefinition {
    CONFIG.get_or_init(|| {
        let result: RigDefinition =
            serde_json::from_str(include_str!("../../../assets/art/pixel/character-rig.json"))
                .expect("Bundled survivor component authoring is valid JSON");
        assert_eq!(result.version, 1);
        result
    })
}
#[cfg(test)]
pub fn component_atlas(image: &Image) -> &'static ComponentAtlas {
    COMPONENTS.get_or_init(|| ComponentAtlas::from_image(image))
}
impl ComponentAtlas {
    #[cfg(test)]
    fn from_image(image: &Image) -> Self {
        let width = image.width();
        let height = image.height();
        let pixels = image
            .data
            .as_deref()
            .expect("Component source retained in main world");
        let bounds = std::array::from_fn(|index| {
            let left = index as u32 % 4 * width / 4;
            let right = (index as u32 % 4 + 1) * width / 4;
            let top = index as u32 / 4 * height / 4;
            let bottom = (index as u32 / 4 + 1) * height / 4;
            let mut b = [right, bottom, left, top];
            for y in top..bottom {
                for x in left..right {
                    if pixels[((y * width + x) * 4 + 3) as usize] >= 128 {
                        b[0] = b[0].min(x);
                        b[1] = b[1].min(y);
                        b[2] = b[2].max(x + 1);
                        b[3] = b[3].max(y + 1);
                    }
                }
            }
            assert!(
                b[0] < b[2] && b[1] < b[3],
                "Every authored body component must exist"
            );
            [
                b[0].saturating_sub(2).max(left) as f32,
                b[1].saturating_sub(2).max(top) as f32,
                (b[2] + 2).min(right) as f32,
                (b[3] + 2).min(bottom) as f32,
            ]
        });
        Self {
            size: [width as f32, height as f32],
            bounds,
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct GunPose {
    cell: usize,
    source_origin: Pos2,
    pivot: Pos2,
    scale: f32,
    rotation: egui::emath::Rot2,
    mirror: bool,
    muzzle: Pos2,
    grip: Pos2,
    support: Pos2,
    magazine: Pos2,
    stock: Option<Pos2>,
}
impl GunPose {
    fn point(&self, point: Pos2) -> Pos2 {
        let mut relative = (point - self.source_origin) * self.scale;
        if self.mirror {
            relative.x = -relative.x;
        }
        self.pivot + self.rotation * relative
    }
    fn new(
        atlas: &RigAtlas,
        unit: f32,
        shoulder: Pos2,
        target: Pos2,
        motion: DefenderRigMotion,
        definition: &WeaponDefinition,
        fit: f32,
    ) -> Self {
        let point = |p: [f32; 2]| pos2(p[0] * atlas.size[0] / 1254., p[1] * atlas.size[1] / 1254.);
        let source = atlas.cells[definition.cell];
        let width_fit = if definition.stock.is_some() { fit } else { 1. };
        let scale = definition.width * unit * width_fit / (source[2] - source[0]);
        let source_origin = point(definition.stock.unwrap_or(definition.grip));
        let initial = (target - shoulder).normalized();
        let pivot = if definition.stock.is_some() {
            shoulder - initial * unit * motion.recoil * 0.55
        } else {
            shoulder
                + initial * unit * (definition.pistol_extension * fit - motion.recoil * 0.8)
                + vec2(0., unit * 1.2)
        };
        let mut requested = target - pivot;
        if let Some(t) = motion.reload {
            requested =
                egui::emath::Rot2::from_angle((t * std::f32::consts::PI).sin() * 0.40) * requested;
        }
        let mirror = requested.x < 0.;
        let reflect = |v: Vec2| if mirror { vec2(-v.x, v.y) } else { v };
        let muzzle = point(definition.muzzle);
        let breech = point(definition.breech);
        let axis = reflect(muzzle - breech).normalized();
        let relative = reflect((muzzle - source_origin) * scale);
        let cross = axis.x * relative.y - axis.y * relative.x;
        let heading = requested.y.atan2(requested.x)
            - (cross / requested.length().max(0.001))
                .clamp(-1., 1.)
                .asin();
        let rotation = egui::emath::Rot2::from_angle(heading - axis.y.atan2(axis.x));
        let mut result = Self {
            cell: definition.cell,
            source_origin,
            pivot,
            scale,
            rotation,
            mirror,
            muzzle: Pos2::ZERO,
            grip: Pos2::ZERO,
            support: Pos2::ZERO,
            magazine: Pos2::ZERO,
            stock: None,
        };
        result.muzzle = result.point(muzzle);
        result.grip = result.point(point(definition.grip));
        result.support = result.point(point(definition.support));
        result.magazine = result.point(point(definition.magazine));
        result.stock = definition.stock.map(|p| result.point(point(p)));
        result
    }
}
#[derive(Clone, Copy, Debug)]
struct Pose {
    stance: StancePose,
    hips: Pos2,
    neck: Pos2,
    near: ArmPose,
    far: ArmPose,
    gun: GunPose,
    near_hand: HandPose,
    far_hand: HandPose,
    unit: f32,
    rotation: egui::emath::Rot2,
    mirror: bool,
}
impl Pose {
    fn new(atlas: &RigAtlas, center: Pos2, size: Vec2, motion: DefenderRigMotion) -> Self {
        let cfg = definition();
        let stance = StancePose::sample(
            cfg.stance,
            center,
            size.y,
            cfg.logical_height,
            motion.target,
            BodyResponse {
                lean: motion.lean,
                recoil: motion.recoil,
                melee: motion.melee,
                hit: motion.hit,
                brace: motion.brace,
            },
        );
        let unit = stance.unit;
        let hips = stance.hips;
        let mirror = stance.mirror;
        let rotation = stance.rotation;
        let near_shoulder = stance.near_shoulder;
        let far_shoulder = stance.far_shoulder;
        let weapon = &cfg.weapons[weapon_index(motion.weapon)];
        debug_assert_eq!(weapon.class, motion.weapon);
        let mut gun = GunPose::new(
            atlas,
            unit,
            near_shoulder,
            motion.target,
            motion,
            weapon,
            1.,
        );
        let reach = (cfg.upper_arm + cfg.forearm - 0.25) * unit;
        let support_reach = (cfg.upper_arm + cfg.forearm) * unit * weapon.support_extension;
        let gun_hands = |gun: &GunPose| {
            (
                hand_pose(
                    "dominant_hand",
                    gun.grip,
                    unit,
                    gun.rotation.angle()
                        + weapon.dominant_rotation * if gun.mirror { -1. } else { 1. },
                    gun.mirror,
                ),
                hand_pose(
                    "support_hand",
                    gun.support,
                    unit,
                    gun.rotation.angle()
                        + weapon.support_rotation * if gun.mirror { -1. } else { 1. },
                    gun.mirror,
                ),
            )
        };
        // Fit the support socket before solving arms. The stock remains seated
        // at the shoulder, even for vertical/leftward poses and short bodies.
        for n in 1..=16 {
            let (near, far) = gun_hands(&gun);
            if far.wrist.distance(far_shoulder) <= support_reach
                && near.wrist.distance(near_shoulder) <= reach
            {
                break;
            }
            gun = GunPose::new(
                atlas,
                unit,
                near_shoulder,
                motion.target,
                motion,
                weapon,
                1. - n as f32 * 0.025,
            );
        }
        if let Some(stock) = gun.stock {
            debug_assert!(stock.distance(near_shoulder) <= unit * 0.56);
        }
        let reload = motion
            .reload
            .map_or(0., |t| (t * std::f32::consts::PI).sin());
        let mut near_palm = gun.grip;
        let mut far_palm = gun.support.lerp(
            hips + vec2(if mirror { 3. } else { -3. }, -2.) * unit,
            reload,
        );
        if motion.throwing > 0. {
            let phase = motion.throwing.clamp(0., 1.);
            let angle = -0.35 - phase * 1.8;
            let x = if mirror { -angle.cos() } else { angle.cos() };
            near_palm = near_shoulder + vec2(x, angle.sin()) * unit * 12.;
            far_palm = hips + vec2(if mirror { -4. } else { 4. }, -8.) * unit;
        } else if motion.melee > 0. {
            let angle = (motion.target - near_shoulder).angle() - 0.9 + motion.melee * 1.8;
            near_palm = near_shoulder + vec2(angle.cos(), angle.sin()) * unit * 11.;
            far_palm = hips + vec2(if mirror { -3. } else { 3. }, -7.) * unit;
        }
        let active = motion.throwing > 0. || motion.melee > 0.;
        let near_angle = if active {
            (near_palm - near_shoulder).angle()
        } else {
            gun.rotation.angle() + weapon.dominant_rotation * if gun.mirror { -1. } else { 1. }
        };
        let far_angle = if active {
            (far_palm - far_shoulder).angle()
        } else {
            gun.rotation.angle()
                + weapon.support_rotation * if gun.mirror { -1. } else { 1. }
                + reload * 0.4
        };
        let near_hand = hand_pose(
            "dominant_hand",
            near_palm,
            unit,
            near_angle,
            !active && gun.mirror,
        );
        let far_hand = hand_pose(
            "support_hand",
            far_palm,
            unit,
            far_angle,
            !active && gun.mirror,
        );
        Self {
            near_hand,
            far_hand,
            stance,
            hips,
            neck: stance.neck,
            unit,
            rotation,
            mirror,
            gun,
            near: solve_arm(
                near_shoulder,
                near_hand.wrist,
                cfg.upper_arm * unit,
                cfg.forearm * unit,
                1.,
            ),
            far: solve_arm(
                far_shoulder,
                far_hand.wrist,
                cfg.upper_arm * unit,
                cfg.forearm * unit,
                1.,
            ),
        }
    }
    fn upper(&self, local: Vec2) -> Pos2 {
        self.stance.upper(local)
    }
}
fn hand_pose(name: &str, palm: Pos2, unit: f32, angle: f32, mirror: bool) -> HandPose {
    let cfg = definition();
    let size = cfg.part_layouts[name].size;
    let pose = HandPose::at_palm(
        palm,
        vec2(size[0], size[1]) * unit,
        cfg.hands[name],
        angle,
        mirror,
    );
    debug_assert!(
        pose.source_point(
            cfg.hands[name].wrist,
            vec2(size[0], size[1]) * unit,
            cfg.hands[name]
        )
        .distance(pose.wrist)
            < 0.001
    );
    pose
}
#[derive(Clone, Copy)]
struct PartStyle {
    angle: f32,
    mirror: bool,
    tint: Color32,
}
struct PartTransform<'a> {
    anchor: Pos2,
    size: Vec2,
    pivot: Vec2,
    style: PartStyle,
    clip: [f32; 4],
    contour: &'a [[f32; 2]],
}
fn component(p: &Painter, sheet: ComponentSheet, index: usize, pose: PartTransform<'_>) {
    let rect = Rect::from_min_size(pose.anchor - pose.size * pose.pivot, pose.size);
    let mut mesh = egui::epaint::Mesh::with_texture(sheet.texture);
    let source = sheet.atlas.bounds[index];
    let source = [
        source[0] + (source[2] - source[0]) * pose.clip[0],
        source[1] + (source[3] - source[1]) * pose.clip[1],
        source[0] + (source[2] - source[0]) * pose.clip[2],
        source[1] + (source[3] - source[1]) * pose.clip[3],
    ];
    if pose.contour.is_empty() {
        mesh.add_rect_with_uv(rect, region_for(source, sheet.atlas.size), pose.style.tint);
    } else {
        // Piecewise UV contour removes the old baked shoulder lobe while
        // retaining original fabric pixels and their exact sampling scale.
        for band in pose.contour.windows(2) {
            let indices = mesh.vertices.len() as u32;
            for (x, y) in [
                (band[0][1], band[0][0]),
                (1., band[0][0]),
                (1., band[1][0]),
                (band[1][1], band[1][0]),
            ] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: rect.min + vec2(x * rect.width(), y * rect.height()),
                    uv: pos2(
                        (source[0] + x * (source[2] - source[0])) / sheet.atlas.size[0],
                        (source[1] + y * (source[3] - source[1])) / sheet.atlas.size[1],
                    ),
                    color: pose.style.tint,
                });
            }
            mesh.indices.extend([
                indices,
                indices + 1,
                indices + 2,
                indices,
                indices + 2,
                indices + 3,
            ]);
        }
    }
    let rotation = egui::emath::Rot2::from_angle(pose.style.angle);
    for vertex in &mut mesh.vertices {
        let mut relative = vertex.pos - pose.anchor;
        if pose.style.mirror {
            relative.x = -relative.x;
        }
        vertex.pos = pose.anchor + rotation * relative;
    }
    p.add(egui::Shape::mesh(mesh));
}
fn part(p: &Painter, sheet: ComponentSheet, name: &str, anchor: Pos2, unit: f32, style: PartStyle) {
    let cfg = definition();
    let layout = &cfg.part_layouts[name];
    component(
        p,
        sheet,
        cfg.parts[name],
        PartTransform {
            anchor: anchor + vec2(layout.offset[0], layout.offset[1]) * unit,
            size: vec2(layout.size[0], layout.size[1]) * unit,
            pivot: vec2(layout.pivot[0], layout.pivot[1]),
            style,
            clip: layout.source_clip,
            contour: &layout.left_contour,
        },
    );
}
fn segment(
    p: &Painter,
    sheet: ComponentSheet,
    name: &str,
    from: Pos2,
    to: Pos2,
    unit: f32,
    tint: Color32,
) {
    let cfg = definition();
    let layout = &cfg.part_layouts[name];
    let delta = to - from;
    let width = layout.size[1] * unit;
    component(
        p,
        sheet,
        cfg.parts[name],
        PartTransform {
            anchor: from,
            size: vec2(delta.length() + width * 0.2, width),
            pivot: vec2(layout.pivot[0], layout.pivot[1]),
            style: PartStyle {
                angle: delta.angle(),
                mirror: false,
                tint,
            },
            clip: layout.source_clip,
            contour: &[],
        },
    );
}
fn draw_arm(
    p: &Painter,
    sheet: ComponentSheet,
    pose: ArmPose,
    female: bool,
    unit: f32,
    tint: Color32,
) {
    let prefix = if female { "june" } else { "rook" };
    segment(
        p,
        sheet,
        &format!("{prefix}_upper_arm"),
        pose.shoulder,
        pose.elbow,
        unit,
        tint,
    );
    segment(
        p,
        sheet,
        &format!("{prefix}_forearm"),
        pose.elbow,
        pose.wrist,
        unit,
        tint,
    );
}
fn pattern(
    p: &Painter,
    style: &str,
    origin: Pos2,
    unit: f32,
    rotation: egui::emath::Rot2,
    mirror: bool,
    colors: [Color32; 3],
) {
    let Some(rows) = definition().gear_patterns.get(style) else {
        return;
    };
    let width = rows.iter().map(String::len).max().unwrap_or(0) as f32;
    let mut mesh = egui::epaint::Mesh::default();
    for (y, row) in rows.iter().enumerate() {
        for (x, token) in row.bytes().enumerate() {
            let color = match token {
                b'2' => colors[0],
                b'3' => colors[1],
                b'4' => colors[2],
                _ => continue,
            };
            let rect = Rect::from_min_size(
                pos2(
                    (x as f32 - width * 0.5) * unit,
                    (y as f32 - rows.len() as f32 * 0.5) * unit,
                ),
                Vec2::splat(unit),
            );
            let start = mesh.vertices.len();
            mesh.add_colored_rect(rect, color);
            for vertex in &mut mesh.vertices[start..] {
                let mut local = vertex.pos.to_vec2();
                if mirror {
                    local.x = -local.x;
                }
                vertex.pos = origin + rotation * local;
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
fn weapon(p: &Painter, sheet: RigSheet, gun: &GunPose, tint: Color32) {
    let source = sheet.atlas.cells[gun.cell];
    let mut mesh = egui::epaint::Mesh::with_texture(sheet.texture);
    mesh.add_rect_with_uv(
        Rect::from_min_max(pos2(source[0], source[1]), pos2(source[2], source[3])),
        region_for(source, sheet.atlas.size),
        tint,
    );
    for vertex in &mut mesh.vertices {
        vertex.pos = gun.point(vertex.pos);
    }
    p.add(egui::Shape::mesh(mesh));
}
pub(super) fn draw(
    p: &Painter,
    textures: ArtTextures,
    style: DefenderRigStyle<'_>,
    center: Pos2,
    size: Vec2,
    tint: Color32,
    motion: DefenderRigMotion,
) -> bool {
    if canonical::draw(p, textures, style, center, size, tint, motion) {
        return true;
    }
    let (Some(sheet), Some(guns)) = (textures.components, textures.rig) else {
        return false;
    };
    let female = style.female;
    let equipment = style.equipment;
    let pose = Pose::new(guns.atlas, center, size, motion);
    let unit = pose.unit;
    let prefix = if female { "june" } else { "rook" };
    for (index, role) in ["far_leg", "near_leg"].iter().enumerate() {
        let name = format!("{prefix}_{role}");
        let source = sheet.atlas.bounds[definition().parts[&name]];
        let slices = leg_slices(
            source,
            definition().leg_sources[&name],
            pose.stance.legs[index],
            definition().part_layouts[&name].size[0] * unit,
        );
        for slice in slices {
            let mut mesh = egui::epaint::Mesh::with_texture(sheet.texture);
            mesh.add_rect_with_uv(
                Rect::from_min_max(
                    pos2(slice.clip[0], slice.clip[1]),
                    pos2(slice.clip[2], slice.clip[3]),
                ),
                region_for(slice.clip, sheet.atlas.size),
                tint,
            );
            for vertex in &mut mesh.vertices {
                vertex.pos = slice.point(vertex.pos);
            }
            p.add(egui::Shape::mesh(mesh));
        }
    }
    part(
        p,
        sheet,
        &format!("{prefix}_pelvis"),
        pose.hips,
        unit,
        PartStyle {
            angle: 0.,
            mirror: false,
            tint,
        },
    );
    if let Some(item) = &equipment.accessory {
        pattern(
            p,
            &item.style,
            pose.upper(vec2(-5.3, -7.2)),
            unit * 0.60,
            pose.rotation,
            pose.mirror,
            [
                Color32::from_rgb(37, 40, 30),
                Color32::from_rgb(107, 88, 56),
                Color32::from_rgb(158, 137, 87),
            ],
        );
    }
    part(
        p,
        sheet,
        &format!("{prefix}_torso"),
        pose.hips,
        unit,
        PartStyle {
            angle: pose.rotation.angle(),
            mirror: pose.mirror,
            tint,
        },
    );
    if let Some(item) = &equipment.outfit {
        let colors = match item.style.as_str() {
            "armor.leather" => [
                Color32::from_rgb(42, 34, 27),
                Color32::from_rgb(99, 69, 43),
                Color32::from_rgb(145, 103, 60),
            ],
            "armor.plate" => [
                Color32::from_rgb(29, 43, 44),
                Color32::from_rgb(79, 103, 106),
                Color32::from_rgb(116, 137, 132),
            ],
            "armor.cloth" => [
                Color32::from_rgb(39, 48, 40),
                Color32::from_rgb(77, 89, 64),
                Color32::from_rgb(103, 115, 80),
            ],
            _ => [
                Color32::from_rgb(34, 41, 29),
                Color32::from_rgb(92, 105, 60),
                Color32::from_rgb(142, 148, 96),
            ],
        };
        pattern(
            p,
            &item.style,
            pose.upper(vec2(1.1, -9.8)),
            unit * 0.60,
            pose.rotation,
            pose.mirror,
            colors,
        );
    }
    // The support shoulder is the front/right shoulder in this camera basis.
    // Its upper arm sits over the jacket, underneath head/weapon/trigger arm.
    let far_upper = format!("{prefix}_upper_arm");
    segment(
        p,
        sheet,
        &far_upper,
        pose.far.shoulder,
        pose.far.elbow,
        unit,
        tint.gamma_multiply(0.85),
    );
    part(
        p,
        sheet,
        &format!("{prefix}_head"),
        pose.neck,
        unit,
        PartStyle {
            angle: pose.rotation.angle() * 0.45,
            mirror: pose.mirror,
            tint,
        },
    );
    let active_hand = motion.throwing > 0. || motion.melee > 0.;
    let far_forearm = format!("{prefix}_forearm");
    // The support forearm must remain visible over the jacket and underneath
    // the gun. Otherwise its palm survives as an apparently floating fist.
    segment(
        p,
        sheet,
        &far_forearm,
        pose.far.elbow,
        pose.far.wrist,
        unit,
        tint.gamma_multiply(0.92),
    );
    if !active_hand {
        weapon(p, guns, &pose.gun, tint);
    }
    draw_arm(p, sheet, pose.near, female, unit, tint);
    part(
        p,
        sheet,
        "support_hand",
        pose.far_hand.palm,
        unit,
        PartStyle {
            angle: pose.far_hand.rotation.angle(),
            mirror: pose.far_hand.mirror,
            tint,
        },
    );
    part(
        p,
        sheet,
        "dominant_hand",
        pose.near_hand.palm,
        unit,
        PartStyle {
            angle: pose.near_hand.rotation.angle(),
            mirror: pose.near_hand.mirror,
            tint,
        },
    );
    if motion.melee > 0. {
        if let Some(item) = &equipment.melee {
            let angle = (pose.near.wrist - pose.near.elbow).angle();
            pattern(
                p,
                &item.style,
                pose.near_hand.palm + vec2(angle.cos(), angle.sin()) * unit * 7.,
                unit * 0.8,
                egui::emath::Rot2::from_angle(angle),
                false,
                [
                    Color32::from_rgb(35, 40, 35),
                    Color32::from_rgb(122, 131, 121),
                    Color32::from_rgb(192, 190, 156),
                ],
            );
        }
    } else if motion.throwing > 0.7 {
        if let Some(kind) = motion.throw_kind {
            super::ordnance_sprite(
                p,
                textures,
                kind,
                pose.near_hand.palm,
                vec2(4., 6.) * unit,
                -0.2,
            );
        }
    } else if let Some(item) = &equipment.melee {
        pattern(
            p,
            &item.style,
            pose.upper(vec2(-3.8, -3.)),
            unit * 0.5,
            egui::emath::Rot2::from_angle(-1.3),
            pose.mirror,
            [
                Color32::from_rgb(31, 36, 31),
                Color32::from_rgb(90, 100, 90),
                Color32::from_rgb(146, 151, 126),
            ],
        );
    }
    if let Some(t) = motion.reload {
        let travel = (t * std::f32::consts::PI).sin();
        let at = pose
            .gun
            .magazine
            .lerp(pose.hips + vec2(-2., -1.) * unit, travel);
        let rect = Rect::from_center_size(at, vec2(2., 4.) * unit);
        p.rect_filled(rect, 0., Color32::from_rgb(31, 42, 39));
        p.line_segment(
            [rect.left_top(), rect.right_top()],
            egui::Stroke::new(unit * 0.7, Color32::from_rgb(143, 152, 131)),
        );
    }
    true
}
pub(super) fn muzzle(
    textures: ArtTextures,
    female: bool,
    center: Pos2,
    size: Vec2,
    motion: DefenderRigMotion,
) -> Option<Pos2> {
    if let Some(point) = canonical::muzzle(textures, female, center, size, motion) {
        return Some(point);
    }
    textures.components?;
    Some(
        Pose::new(textures.rig?.atlas, center, size, motion)
            .gun
            .muzzle,
    )
}
pub(super) fn throw_origin(
    textures: ArtTextures,
    female: bool,
    center: Pos2,
    size: Vec2,
    motion: DefenderRigMotion,
) -> Option<Pos2> {
    if let Some(point) = canonical::throw_origin(textures, female, center, size, motion) {
        return Some(point);
    }
    textures.components?;
    Some(
        Pose::new(textures.rig?.atlas, center, size, motion)
            .near_hand
            .palm,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        image::{CompressedImageFormats, ImageSampler, ImageType},
    };
    fn image(bytes: &[u8]) -> Image {
        Image::from_buffer(
            bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap()
    }
    fn atlas() -> &'static RigAtlas {
        static TEST_ATLAS: OnceLock<RigAtlas> = OnceLock::new();
        TEST_ATLAS.get_or_init(|| {
            RigAtlas::from_image(&image(include_bytes!(
                "../../../assets/art/pixel/defender-rig-v1.png"
            )))
        })
    }
    fn motion(weapon: WeaponClass, target: Pos2) -> DefenderRigMotion {
        DefenderRigMotion {
            target,
            weapon,
            recoil: 0.,
            lean: 0.,
            reload: None,
            throwing: 0.,
            throw_kind: None,
            melee: 0.,
            brace: 0.,
            hit: 0.,
        }
    }
    #[test]
    fn authored_parts_are_isolated_and_all_equipment_socket_definitions_are_distinct() {
        let source = image(include_bytes!(
            "../../../assets/art/pixel/survivor-components-v3.png"
        ));
        let components = ComponentAtlas::from_image(&source);
        let cfg = definition();
        let gun_image = image(include_bytes!(
            "../../../assets/art/pixel/defender-rig-v1.png"
        ));
        let gun_pixels = gun_image.data.as_deref().unwrap();
        assert_eq!(cfg.parts.len(), 16);
        assert_eq!(cfg.hands.len(), 2);
        for name in ["dominant_hand", "support_hand"] {
            let sockets = cfg.hands[name];
            assert_eq!(
                cfg.part_layouts[name].pivot, sockets.palm,
                "Pixels and grasp solver must share the same palm pivot"
            );
            assert!(
                sockets
                    .wrist
                    .iter()
                    .chain(sockets.palm.iter())
                    .all(|v| (0. ..=1.).contains(v))
            );
            assert_ne!(sockets.wrist, sockets.palm);
        }
        assert_eq!(cfg.part_layouts.len(), 16);
        for name in cfg.parts.keys() {
            let layout = &cfg.part_layouts[name];
            assert!(layout.size.iter().all(|v| v.is_finite() && *v > 0.));
            assert!(layout.pivot.iter().all(|v| (0. ..=1.).contains(v)));
            assert!(
                layout.source_clip[0] >= 0.
                    && layout.source_clip[1] >= 0.
                    && layout.source_clip[2] <= 1.
                    && layout.source_clip[3] <= 1.
            );
            assert!(
                layout.source_clip[0] < layout.source_clip[2]
                    && layout.source_clip[1] < layout.source_clip[3]
            );
        }
        assert_eq!(
            cfg.parts
                .values()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            16
        );
        for (index, b) in components.bounds.iter().enumerate() {
            let x = index % 4;
            let y = index / 4;
            assert!(
                b[0] >= x as f32 * components.size[0] / 4.
                    && b[2] <= (x + 1) as f32 * components.size[0] / 4.
            );
            assert!(
                b[1] >= y as f32 * components.size[1] / 4.
                    && b[3] <= (y + 1) as f32 * components.size[1] / 4.
            );
            assert!(b[2] - b[0] > 30. && b[3] - b[1] > 30.);
        }
        assert_eq!(
            cfg.weapons.iter().map(|d| d.class).collect::<Vec<_>>(),
            WeaponClass::ALL
        );
        assert_eq!(
            cfg.weapons
                .iter()
                .map(|d| d.cell)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            6
        );
        for weapon in &cfg.weapons {
            assert!((0.75..0.95).contains(&weapon.support_extension));
            let point = weapon.support;
            let offset = (point[1] as u32 * gun_image.width() + point[0] as u32) as usize * 4;
            assert!(
                gun_pixels[offset + 3] >= 128,
                "Support grasp must land on actual authored weapon material"
            );
            assert!(weapon.muzzle[0] > weapon.grip[0]);
            assert!(weapon.support[0] > weapon.grip[0]);
            if let Some(stock) = weapon.stock {
                assert!(stock[0] < weapon.grip[0]);
            }
        }
        for style in [
            "armor.cloth",
            "armor.leather",
            "armor.vest",
            "armor.plate",
            "accessory.pouch",
            "accessory.satchel",
            "accessory.radio",
            "accessory.medic",
            "accessory.rig",
            "melee.crowbar",
            "melee.axe",
            "melee.machete",
            "melee.hammer",
            "melee.saber",
        ] {
            let rows = &cfg.gear_patterns[style];
            assert!(rows.iter().any(|row| row.contains('4')));
            assert!(rows.iter().any(|row| row.contains('.')));
        }
    }
    #[test]
    fn source_necks_and_thighs_meet_the_adult_body_without_splayed_hips_or_squeezed_faces() {
        let source = image(include_bytes!(
            "../../../assets/art/pixel/survivor-components-v3.png"
        ));
        let components = ComponentAtlas::from_image(&source);
        let pixels = source.data.as_deref().unwrap();
        let cfg = definition();
        let mass_x = |index: usize, bottom: bool| {
            let b = components.bounds[index];
            let band = (b[3] - b[1]) * 0.1;
            let (top, end) = if bottom {
                (b[3] - band, b[3])
            } else {
                (b[1], b[1] + band)
            };
            let mut total = 0.;
            let mut count = 0.;
            for y in top as u32..end as u32 {
                for x in b[0] as u32..b[2] as u32 {
                    if pixels[((y * source.width() + x) * 4 + 3) as usize] >= 128 {
                        total += x as f32;
                        count += 1.;
                    }
                }
            }
            assert!(count > 0.);
            (total / count - b[0]) / (b[2] - b[0])
        };
        for who in ["rook", "june"] {
            let head = &cfg.part_layouts[&format!("{who}_head")];
            let b = components.bounds[cfg.parts[&format!("{who}_head")]];
            let source_aspect = (b[2] - b[0]) / (b[3] - b[1]);
            assert!(
                (head.size[0] / head.size[1] / source_aspect - 1.).abs() < 0.08,
                "A ponytail must not squeeze the face to fit Rook's head width"
            );
            assert!(
                head.size[1] / cfg.logical_height <= 0.2,
                "Head must fit the reviewed adult proportion"
            );
            let neck_offset =
                (mass_x(cfg.parts[&format!("{who}_head")], true) - head.pivot[0]) * head.size[0];
            assert!(
                neck_offset.abs() < 0.35,
                "Source neck must meet the torso collar: {who} {neck_offset}"
            );
            let pelvis = &cfg.part_layouts[&format!("{who}_pelvis")];
            let pose = Pose::new(
                atlas(),
                pos2(200., 180.),
                vec2(200., 205.),
                motion(WeaponClass::Rifle, pos2(1000., 180.)),
            );
            for (index, role) in ["far_leg", "near_leg"].iter().enumerate() {
                let name = format!("{who}_{role}");
                let b = components.bounds[cfg.parts[&name]];
                let slices = leg_slices(
                    b,
                    cfg.leg_sources[&name],
                    pose.stance.legs[index],
                    cfg.part_layouts[&name].size[0] * pose.unit,
                );
                let source_hip = pos2(b[0] + mass_x(cfg.parts[&name], false) * (b[2] - b[0]), b[1]);
                let projected = slices[0].point(source_hip);
                assert!(
                    (projected.x - pose.hips.x).abs() < pelvis.size[0] * 0.27 * pose.unit,
                    "Articulated thigh source must meet beneath pelvis: {name} {projected:?}"
                );
                assert!((projected.y - pose.hips.y).abs() < pelvis.size[1] * 0.5 * pose.unit);
            }
        }
        for name in ["dominant_hand", "support_hand"] {
            let layout = &cfg.part_layouts[name];
            let b = components.bounds[cfg.parts[name]];
            assert!(
                (layout.size[0] / layout.size[1] / ((b[2] - b[0]) / (b[3] - b[1])) - 1.).abs()
                    < 0.12,
                "Hands must preserve palm proportions instead of becoming square fists"
            );
        }
    }
    #[test]
    fn standing_leg_sources_keep_the_knee_over_the_ankle_instead_of_a_walking_zigzag() {
        let source = image(include_bytes!(
            "../../../assets/art/pixel/survivor-components-v3.png"
        ));
        let atlas = ComponentAtlas::from_image(&source);
        let pixels = source.data.as_deref().unwrap();
        for index in [6, 7, 8, 9] {
            let b = atlas.bounds[index];
            let height = b[3] - b[1];
            let mut centers = Vec::new();
            for f in [0.15, 0.30, 0.45, 0.60, 0.75] {
                let mut xsum = 0.;
                let mut count = 0.;
                for y in (b[1] + height * f) as u32..(b[1] + height * (f + 0.08)) as u32 {
                    for x in b[0] as u32..b[2] as u32 {
                        if pixels[((y * source.width() + x) * 4 + 3) as usize] >= 128 {
                            xsum += x as f32;
                            count += 1.;
                        }
                    }
                }
                assert!(count > 0.);
                centers.push(xsum / count);
            }
            let min = centers.iter().copied().fold(f32::INFINITY, f32::min);
            let max = centers.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (max - min) / height < 0.05,
                "Standing component {index} contains a curved walking knee"
            );
        }
    }
    #[test]
    fn the_support_forearm_is_visible_over_the_torso_and_under_the_weapon() {
        let source = image(include_bytes!(
            "../../../assets/art/pixel/survivor-components-v3.png"
        ));
        let components = component_atlas(&source);
        let ctx = egui::Context::default();
        let textures = ArtTextures {
            components: Some(ComponentSheet {
                texture: TextureId::User(90),
                atlas: components,
            }),
            rig: Some(RigSheet {
                texture: TextureId::User(91),
                atlas: atlas(),
            }),
            ..Default::default()
        };
        let mut output = ctx.run_ui(Default::default(), |ui| {
            draw(
                ui.painter(),
                textures,
                DefenderRigStyle {
                    female: true,
                    equipment: &EquipmentVisual::default(),
                },
                pos2(200., 180.),
                vec2(200., 205.),
                Color32::WHITE,
                motion(WeaponClass::Rifle, pos2(1000., 180.)),
            );
        });
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Mesh(m) = &s.shape {
                    Some(m)
                } else {
                    None
                }
            })
            .collect();
        let region =
            |name: &str| region_for(components.bounds[definition().parts[name]], components.size);
        let torso = meshes
            .iter()
            .position(|m| {
                m.texture_id == TextureId::User(90)
                    && region("june_torso").contains(m.vertices[0].uv)
            })
            .unwrap();
        let gun = meshes
            .iter()
            .position(|m| m.texture_id == TextureId::User(91))
            .unwrap();
        let forearms: Vec<_> = meshes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                (m.texture_id == TextureId::User(90)
                    && region("june_forearm").contains(m.vertices[0].uv))
                .then_some(i)
            })
            .collect();
        assert_eq!(forearms.len(), 2);
        assert!(
            torso < forearms[0] && forearms[0] < gun,
            "The support palm needs a visible connecting forearm"
        );
        assert!(
            forearms[1] > gun,
            "The dominant forearm and grip remain in front of the receiver"
        );
        output.textures_delta.clear();
    }
    #[test]
    fn both_hands_meet_the_actual_weapon_sockets_and_bores_track_all_quadrants() {
        let center = pos2(200., 180.);
        let size = vec2(60., 65.);
        for class in WeaponClass::ALL {
            for degrees in [-179_f32, -135., -90., -45., 0., 45., 90., 135., 179.] {
                for recoil in [0., 0.5, 1.] {
                    let angle = degrees.to_radians();
                    let target = center + vec2(angle.cos(), angle.sin()) * 300.;
                    let mut motion = motion(class, target);
                    motion.recoil = recoil;
                    let pose = Pose::new(atlas(), center, size, motion);
                    assert!(
                        pose.near_hand.palm.distance(pose.gun.grip) < 0.01,
                        "{class:?} {degrees}: dominant hand lost grip"
                    );
                    assert!(
                        pose.far_hand.palm.distance(pose.gun.support) < 0.01,
                        "{class:?} {degrees}: support hand lost fore-end"
                    );
                    assert!(
                        pose.near.wrist.distance(pose.near_hand.wrist) < 0.01,
                        "Dominant cuff must meet its wrist, not the center of its palm"
                    );
                    assert!(
                        pose.far.wrist.distance(pose.far_hand.wrist) < 0.01,
                        "Support cuff must meet its wrist, not the weapon contact"
                    );
                    assert!(
                        pose.far.shoulder.distance(pose.far.wrist)
                            <= (definition().upper_arm + definition().forearm)
                                * pose.unit
                                * definition().weapons[weapon_index(class)].support_extension
                                + 0.01,
                        "{class:?} {degrees}: support arm must retain a comfortable elbow bend"
                    );
                    let spec = &definition().weapons[weapon_index(class)];
                    let breech = pos2(
                        spec.breech[0] * atlas().size[0] / 1254.,
                        spec.breech[1] * atlas().size[1] / 1254.,
                    );
                    let bore = (pose.gun.muzzle - pose.gun.point(breech)).normalized();
                    assert!(bore.dot((target - pose.gun.muzzle).normalized()) > 0.99999);
                    if let Some(stock) = pose.gun.stock {
                        assert!(
                            stock.distance(pose.near.shoulder) <= pose.unit * 0.56,
                            "Long gun must remain shouldered"
                        );
                    } else {
                        assert!(
                            pose.gun.grip.distance(pose.gun.support) < pose.unit,
                            "Pistol must use one shared two-handed grip"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn fixed_boots_and_bounded_bones_survive_every_action_and_unreachable_ik_request() {
        let center = pos2(200., 180.);
        let size = vec2(60., 65.);
        let baseline = Pose::new(
            atlas(),
            center,
            size,
            motion(WeaponClass::Rifle, pos2(500., 180.)),
        )
        .stance
        .legs
        .map(|leg| leg.sole);
        for mut m in [
            motion(WeaponClass::Pistol, pos2(-200., 10.)),
            motion(WeaponClass::Marksman, pos2(400., 350.)),
        ] {
            for action in 0..5 {
                m.recoil = 0.;
                m.reload = None;
                m.throwing = 0.;
                m.melee = 0.;
                m.brace = 0.;
                match action {
                    0 => m.recoil = 1.,
                    1 => m.reload = Some(0.5),
                    2 => m.throwing = 1.,
                    3 => m.melee = 1.,
                    _ => m.brace = 1.,
                }
                let pose = Pose::new(atlas(), center, size, m);
                assert_eq!(pose.stance.legs.map(|leg| leg.sole), baseline);
                for arm in [pose.near, pose.far] {
                    assert!(
                        (arm.shoulder.distance(arm.elbow) - definition().upper_arm * pose.unit)
                            .abs()
                            < 0.001
                    );
                    assert!(
                        (arm.elbow.distance(arm.wrist) - definition().forearm * pose.unit).abs()
                            < 0.001
                    );
                }
            }
        }
        for requested in [Pos2::ZERO, pos2(10000., 10000.)] {
            let arm = solve_arm(Pos2::ZERO, requested, 8., 7., 1.);
            assert!((arm.shoulder.distance(arm.elbow) - 8.).abs() < 0.001);
            assert!((arm.elbow.distance(arm.wrist) - 7.).abs() < 0.001);
            assert!(arm.wrist.distance(arm.shoulder) <= 15.);
        }
    }
    #[test]
    fn real_equipment_layers_swap_without_mutating_legacy_or_equipped_snapshots() {
        let ctx = egui::Context::default();
        let source = image(include_bytes!(
            "../../../assets/art/pixel/survivor-components-v3.png"
        ));
        let sheet = ComponentSheet {
            texture: TextureId::User(90),
            atlas: component_atlas(&source),
        };
        let textures = ArtTextures {
            components: Some(sheet),
            rig: Some(RigSheet {
                texture: TextureId::User(91),
                atlas: atlas(),
            }),
            ..Default::default()
        };
        let legacy = EquipmentVisual::default();
        let item = |style: &str| horde_core::survivors::EquipmentVisualItem {
            definition: 0,
            level: 1,
            quality: Default::default(),
            style: style.into(),
        };
        let equipped = EquipmentVisual {
            outfit: Some(item("armor.plate")),
            accessory: Some(item("accessory.radio")),
            melee: Some(item("melee.axe")),
            ..Default::default()
        };
        let before = serde_json::to_string(&equipped).unwrap();
        let mut counts = Vec::new();
        for gear in [&legacy, &equipped, &legacy] {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                assert!(draw(
                    ui.painter(),
                    textures,
                    DefenderRigStyle {
                        female: true,
                        equipment: gear
                    },
                    pos2(100., 100.),
                    vec2(80., 100.),
                    Color32::WHITE,
                    motion(WeaponClass::Pistol, pos2(400., 100.))
                ));
            });
            counts.push(
                output
                    .shapes
                    .iter()
                    .filter(|s| matches!(&s.shape, egui::Shape::Mesh(_)))
                    .count(),
            );
            output.textures_delta.clear();
        }
        assert!(counts[1] > counts[0]);
        assert_eq!(counts[0], counts[2]);
        assert_eq!(serde_json::to_string(&equipped).unwrap(), before);
        assert_eq!(legacy, EquipmentVisual::default());
        let m = motion(WeaponClass::Pistol, pos2(400., 100.));
        assert_eq!(
            super::super::defender_muzzle(textures, true, pos2(100., 100.), vec2(80., 100.), m),
            Some(
                Pose::new(atlas(), pos2(100., 100.), vec2(80., 100.), m)
                    .gun
                    .muzzle
            )
        );
        let mut throwing = m;
        throwing.throwing = 0.9;
        let pose = Pose::new(atlas(), pos2(100., 100.), vec2(80., 100.), throwing);
        assert_eq!(
            super::super::defender_throw_origin(
                textures,
                true,
                pos2(100., 100.),
                vec2(80., 100.),
                throwing
            ),
            Some(pose.near_hand.palm)
        );
        assert!(
            pose.near_hand.palm.distance(pose.near.wrist) > 1.,
            "Projectile leaves the grasp, not the anatomical wrist"
        );
    }
}
