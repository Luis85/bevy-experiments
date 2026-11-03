//! Source-derived survivor assembly. Identity pixels and lower-body proportions
//! come directly from the canonical menu artwork; the kit fills occluded cloth.
use super::*;
#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug)]
pub struct CanonicalSheet {
    pub texture: TextureId,
    pub size: [u32; 2],
    pub sleeves: Option<&'static SourceSleeves>,
}
#[derive(Debug)]
pub struct SourceSleeves {
    runs: BTreeMap<String, Vec<[f32; 4]>>,
}
static SOURCE_SLEEVES: OnceLock<SourceSleeves> = OnceLock::new();
pub fn source_sleeves(menu: &bevy::prelude::Image) -> &'static SourceSleeves {
    SOURCE_SLEEVES.get_or_init(|| {
        let pixels = menu
            .data
            .as_deref()
            .expect("Canonical menu source pixels retained");
        let mut runs = BTreeMap::new();
        for (name, part) in &config().near_sleeves {
            let b = part.source_rect;
            let mut spans = Vec::new();
            for y in b[1] as u32..b[3] as u32 {
                let mut start = None;
                for x in b[0] as u32..=b[2] as u32 {
                    let solid = if x >= b[2] as u32 {
                        false
                    } else {
                        let rgba = &pixels[((y * menu.width() + x) * 4) as usize..][..4];
                        let in_polygon = inside_polygon(
                            pos2(x as f32 + 0.5, y as f32 + 0.5),
                            &part.source_polygon,
                        );
                        let cloth = !part.cloth_only
                            || rgba[0] < 50
                            || f32::from(rgba[0]) <= f32::from(rgba[1]) * 1.18
                            || rgba[2] >= rgba[1];
                        rgba[3] >= 128 && in_polygon && cloth
                    };
                    if solid {
                        start.get_or_insert(x);
                    } else if let Some(left) = start.take() {
                        spans.push([left as f32, y as f32, x as f32, y as f32 + 1.]);
                    }
                }
            }
            assert!(
                !spans.is_empty() && spans.len() < 2048,
                "Canonical sleeve UV mask is present and bounded"
            );
            runs.insert(name.clone(), spans);
        }
        SourceSleeves { runs }
    })
}
fn inside_polygon(p: Pos2, vertices: &[[f32; 2]]) -> bool {
    if vertices.is_empty() {
        return true;
    }
    let mut inside = false;
    let mut previous = vertices.len() - 1;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[previous];
        if (a[1] > p.y) != (b[1] > p.y) && p.x < (b[0] - a[0]) * (p.y - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        previous = i;
    }
    inside
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum RigSource {
    MenuStill,
    CanonicalKit,
}
#[derive(Debug, Deserialize)]
pub struct RigPartDefinition {
    pub id: String,
    pub source: RigSource,
    pub source_rect: [f32; 4],
    pub anchors: BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    pub mask_rects: Vec<[f32; 4]>,
    #[serde(default)]
    pub source_polygon: Vec<[f32; 2]>,
    #[serde(default)]
    pub cloth_only: bool,
}
#[derive(Deserialize)]
struct CanonicalFrame {
    id: String,
    source_rect: [f32; 4],
    ground: [f32; 2],
    hips: [f32; 2],
    neck: [f32; 2],
    near_shoulder: [f32; 2],
    far_shoulder: [f32; 2],
    upper_arm: f32,
    forearm: f32,
    hand_width: f32,
    torso_reference_width: f32,
    torso_source_top: f32,
    field_webbing: String,
    carriage_anchor: [f32; 2],
    carriage_height: f32,
    parts: Vec<RigPartDefinition>,
}
#[derive(Deserialize)]
struct CanonicalDefinition {
    version: u8,
    source_size: [f32; 2],
    kit_size: [f32; 2],
    frames: Vec<CanonicalFrame>,
    weapon_widths: BTreeMap<WeaponClass, f32>,
    kit_parts: Vec<RigPartDefinition>,
    near_sleeves: BTreeMap<String, RigPartDefinition>,
}
static CANON: OnceLock<CanonicalDefinition> = OnceLock::new();
fn config() -> &'static CanonicalDefinition {
    CANON.get_or_init(|| {
        let data: CanonicalDefinition = serde_json::from_str(include_str!(
            "../../../../assets/art/pixel/canonical-rig.json"
        ))
        .expect("Canonical source metadata");
        assert_eq!(data.version, 1);
        data
    })
}
fn point(p: [f32; 2]) -> Pos2 {
    pos2(p[0], p[1])
}
fn part(id: &str) -> &'static RigPartDefinition {
    config()
        .kit_parts
        .iter()
        .find(|p| p.id == id)
        .expect("Authored canonical kit part")
}
#[derive(Clone, Copy)]
struct Body {
    frame: &'static CanonicalFrame,
    foot: Pos2,
    scale: f32,
    hips: Pos2,
    rotation: egui::emath::Rot2,
    mirror: bool,
}
impl Body {
    fn new(female: bool, center: Pos2, size: Vec2, motion: DefenderRigMotion) -> Self {
        let frame = &config().frames[usize::from(female)];
        let scale = size.y / (frame.source_rect[3] - frame.source_rect[1]);
        let foot = center + vec2(0., size.y * 0.5);
        let hips = foot + (point(frame.hips) - point(frame.ground)) * scale;
        let mirror = motion.target.x < hips.x;
        // Reference proportions are the bind pose. Actions add only restrained
        // weight shift; original trousers/boots are never bent or rescaled.
        let lean = (motion.lean * 0.45 - motion.recoil * 0.012 + motion.melee * 0.025
            - motion.hit * 0.02)
            .clamp(-0.055, 0.055);
        Self {
            frame,
            foot,
            scale,
            hips,
            rotation: egui::emath::Rot2::from_angle(if mirror { -lean } else { lean }),
            mirror,
        }
    }
    fn lower(&self, p: Pos2) -> Pos2 {
        self.foot + (p - point(self.frame.ground)) * self.scale
    }
    fn upper(&self, p: Pos2) -> Pos2 {
        let mut relative = (p - point(self.frame.hips)) * self.scale;
        if self.mirror {
            relative.x = -relative.x;
        }
        self.hips + self.rotation * relative
    }
}
#[derive(Clone, Copy)]
struct Hand {
    pose: HandPose,
    pixel_scale: f32,
    source_palm: Pos2,
    part: &'static RigPartDefinition,
}
impl Hand {
    fn new(id: &str, palm: Pos2, body: Body, angle: f32, mirror: bool) -> Self {
        let part = part(id);
        let b = part.source_rect;
        let physical_width = body.frame.hand_width * if id == "support_hand" { 0.72 } else { 1. };
        let pixel_scale = physical_width * body.scale / (b[2] - b[0]);
        let normalize =
            |p: [f32; 2]| [(p[0] - b[0]) / (b[2] - b[0]), (p[1] - b[1]) / (b[3] - b[1])];
        let sockets = HandSockets {
            wrist: normalize(part.anchors["wrist"]),
            palm: normalize(part.anchors["palm"]),
        };
        let pose = HandPose::at_palm(
            palm,
            vec2(b[2] - b[0], b[3] - b[1]) * pixel_scale,
            sockets,
            angle,
            mirror,
        );
        Self {
            pose,
            pixel_scale,
            source_palm: point(part.anchors["palm"]),
            part,
        }
    }
    fn point(&self, p: Pos2) -> Pos2 {
        let mut relative = (p - self.source_palm) * self.pixel_scale;
        if self.pose.mirror {
            relative.x = -relative.x;
        }
        self.pose.palm + self.pose.rotation * relative
    }
}
struct Pose {
    body: Body,
    gun: GunPose,
    near: ArmPose,
    far: ArmPose,
    near_hand: Hand,
    far_hand: Hand,
}
impl Pose {
    fn new(
        atlas: &RigAtlas,
        female: bool,
        center: Pos2,
        size: Vec2,
        motion: DefenderRigMotion,
    ) -> Self {
        let body = Body::new(female, center, size, motion);
        let f = body.frame;
        let shoulder = body.upper(point(f.near_shoulder));
        let far_shoulder = body.upper(point(f.far_shoulder));
        let old = &definition().weapons[weapon_index(motion.weapon)];
        let weapon = WeaponDefinition {
            class: old.class,
            cell: old.cell,
            width: config().weapon_widths[&motion.weapon],
            grip: old.grip,
            support: old.support,
            stock: old.stock,
            muzzle: old.muzzle,
            breech: old.breech,
            magazine: old.magazine,
            support_extension: old.support_extension,
            pistol_extension: 13.2,
            dominant_rotation: old.dominant_rotation,
            support_rotation: old.support_rotation,
        };
        let unit = size.y / 48.;
        let upper = f.upper_arm * body.scale;
        let lower = f.forearm * body.scale;
        let hands = |gun: &GunPose| {
            let sign = if gun.mirror { -1. } else { 1. };
            (
                Hand::new(
                    "dominant_hand",
                    gun.grip,
                    body,
                    gun.rotation.angle() + weapon.dominant_rotation * sign,
                    gun.mirror,
                ),
                Hand::new(
                    "support_hand",
                    gun.support,
                    body,
                    gun.rotation.angle() + weapon.support_rotation * sign,
                    gun.mirror,
                ),
            )
        };
        let mut gun = GunPose::new(atlas, unit, shoulder, motion.target, motion, &weapon, 1.);
        for n in 1..=16 {
            let (near, far) = hands(&gun);
            if near.pose.wrist.distance(shoulder) < upper + lower - 0.1
                && far.pose.wrist.distance(far_shoulder) < (upper + lower) * 0.93
            {
                break;
            }
            gun = GunPose::new(
                atlas,
                unit,
                shoulder,
                motion.target,
                motion,
                &weapon,
                1. - n as f32 * 0.025,
            );
        }
        let (mut near_hand, mut far_hand) = hands(&gun);
        if let Some(t) = motion.reload {
            let phase = (t * std::f32::consts::PI).sin();
            far_hand = Hand::new(
                "support_hand",
                gun.support.lerp(
                    body.upper(point(f.hips)) + vec2(0., -30.) * body.scale,
                    phase,
                ),
                body,
                gun.rotation.angle() + phase * 0.4,
                gun.mirror,
            );
        }
        if motion.throwing > 0. || motion.melee > 0. {
            let angle = if motion.throwing > 0. {
                -0.3 - motion.throwing * 1.7
            } else {
                (motion.target - shoulder).angle() - 0.6 + motion.melee
            };
            let direction = vec2(
                angle.cos() * if body.mirror { -1. } else { 1. },
                angle.sin(),
            );
            let palm = shoulder + direction * (upper + lower) * 0.75;
            near_hand = Hand::new("dominant_hand", palm, body, direction.angle(), false);
            far_hand = Hand::new(
                "support_hand",
                body.upper(point(f.hips)) + vec2(0., -90.) * body.scale,
                body,
                0.,
                false,
            );
        }
        let near = solve_arm(shoulder, near_hand.pose.wrist, upper, lower, 1.);
        let far = solve_arm(far_shoulder, far_hand.pose.wrist, upper, lower, 1.);
        Self {
            body,
            gun,
            near,
            far,
            near_hand,
            far_hand,
        }
    }
}
fn paint(
    p: &Painter,
    textures: ArtTextures,
    definition: &RigPartDefinition,
    tint: Color32,
    transform: impl Fn(Pos2) -> Pos2,
) {
    let (texture, size) = match definition.source {
        RigSource::MenuStill => (textures.menu_idle, config().source_size),
        RigSource::CanonicalKit => {
            let size = textures.canonical.map_or(config().kit_size, |sheet| {
                [sheet.size[0] as f32, sheet.size[1] as f32]
            });
            (textures.canonical.map(|sheet| sheet.texture), size)
        }
    };
    let Some(texture) = texture else {
        return;
    };
    let mut mesh = egui::epaint::Mesh::with_texture(texture);
    let regions = if definition.mask_rects.is_empty() {
        std::slice::from_ref(&definition.source_rect)
    } else {
        &definition.mask_rects
    };
    for b in regions {
        mesh.add_rect_with_uv(
            Rect::from_min_max(pos2(b[0], b[1]), pos2(b[2], b[3])),
            region_for(*b, size),
            tint,
        );
    }
    for vertex in &mut mesh.vertices {
        vertex.pos = transform(vertex.pos);
    }
    p.add(egui::Shape::mesh(mesh));
}
fn axis_part(p: &Painter, textures: ArtTextures, id: &str, start: Pos2, end: Pos2, tint: Color32) {
    let definition = part(id);
    let a = point(definition.anchors["start"]);
    let b = point(definition.anchors["end"]);
    let axis = b - a;
    let target = end - start;
    let scale = target.length() / axis.length();
    let rotation = egui::emath::Rot2::from_angle(target.angle() - axis.angle());
    paint(p, textures, definition, tint, |point| {
        start + rotation * ((point - a) * scale)
    });
}
fn torso(p: &Painter, textures: ArtTextures, body: Body, tint: Color32) {
    let id = if body.frame.id == "June" {
        "june_torso"
    } else {
        "rook_torso"
    };
    let part = part(id);
    paint(p, textures, part, tint, |p| {
        body.upper(torso_source_point(body.frame, part, p))
    });
}
fn torso_source_point(frame: &CanonicalFrame, part: &RigPartDefinition, p: Pos2) -> Pos2 {
    let b = part.source_rect;
    let waist = point(part.anchors["waist"]);
    pos2(
        frame.hips[0] + (p.x - waist.x) * frame.torso_reference_width / (b[2] - b[0]),
        frame.torso_source_top
            + (p.y - b[1]) * (frame.hips[1] - frame.torso_source_top) / (b[3] - b[1]),
    )
}
fn near_axis_part(
    p: &Painter,
    textures: ArtTextures,
    id: &str,
    start: Pos2,
    end: Pos2,
    tint: Color32,
    mirror: bool,
) -> bool {
    let Some(mask) = textures.canonical.and_then(|sheet| sheet.sleeves) else {
        return false;
    };
    let Some(texture) = textures.menu_idle else {
        return false;
    };
    let source = &config().near_sleeves[id];
    let a = point(source.anchors["start"]);
    let b = point(source.anchors["end"]);
    let reflect = |v: Vec2| if mirror { vec2(-v.x, v.y) } else { v };
    let scale = (end - start).length() / (b - a).length();
    let rotation = egui::emath::Rot2::from_angle((end - start).angle() - reflect(b - a).angle());
    let mut mesh = egui::epaint::Mesh::with_texture(texture);
    for run in &mask.runs[id] {
        mesh.add_rect_with_uv(
            Rect::from_min_max(pos2(run[0], run[1]), pos2(run[2], run[3])),
            region_for(*run, config().source_size),
            tint,
        );
    }
    for vertex in &mut mesh.vertices {
        vertex.pos = start + rotation * (reflect(vertex.pos - a) * scale);
    }
    p.add(egui::Shape::mesh(mesh));
    true
}
fn wearable(p: &Painter, textures: ArtTextures, id: &str, body: Body, tint: Color32) {
    let part = part(id);
    let b = part.source_rect;
    let height = (body.frame.hips[1] - body.frame.neck[1]) * body.scale;
    let scale = height / (b[3] - b[1]);
    let anchor = point(part.anchors["pivot"]);
    paint(p, textures, part, tint, |p| {
        let mut r = (p - anchor) * scale;
        if body.mirror {
            r.x = -r.x;
        }
        body.hips + body.rotation * r
    });
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
    let (Some(sheet), Some(_), Some(_)) = (textures.rig, textures.menu_idle, textures.canonical)
    else {
        return false;
    };
    let pose = Pose::new(sheet.atlas, style.female, center, size, motion);
    let body = pose.body;
    for definition in body.frame.parts.iter().filter(|part| part.id != "head") {
        paint(p, textures, definition, tint, |p| body.lower(p));
    }
    let field_webbing = style
        .equipment
        .accessory
        .as_ref()
        .is_some_and(|item| item.style == "accessory.pouch");
    if field_webbing && body.frame.field_webbing == "backpack_side" {
        let definition = part("backpack_side");
        let b = definition.source_rect;
        let scale = body.frame.carriage_height * body.scale / (b[3] - b[1]);
        let anchor = point(definition.anchors["attach"]);
        let target = body.upper(point(body.frame.carriage_anchor));
        paint(p, textures, definition, tint, |p| {
            let mut r = (p - anchor) * scale;
            if body.mirror {
                r.x = -r.x;
            }
            target + body.rotation * r
        });
    } else if style.equipment.accessory.is_some() && !field_webbing {
        let definition = part("backpack");
        let b = definition.source_rect;
        let scale = 115. * body.scale / (b[2] - b[0]);
        let anchor = point([b[2] - 10., b[1] + 20.]);
        let target = body.upper(point(body.frame.near_shoulder)) - vec2(20., 35.) * body.scale;
        paint(p, textures, definition, tint, |p| {
            target + (p - anchor) * scale
        });
    }
    torso(p, textures, body, tint);
    if let Some(item) = &style.equipment.outfit {
        match item.style.as_str() {
            "armor.leather" => wearable(p, textures, "leather", body, tint),
            "armor.vest" => wearable(p, textures, "webbing", body, tint),
            "armor.plate" => wearable(p, textures, "plate", body, tint),
            _ => {}
        }
    }
    if field_webbing && body.frame.field_webbing == "webbing" {
        wearable(p, textures, "webbing", body, tint);
    }
    let prefix = if style.female { "june" } else { "rook" };
    axis_part(
        p,
        textures,
        &format!("{prefix}_upper"),
        pose.far.shoulder,
        pose.far.elbow,
        tint.gamma_multiply(0.88),
    );
    axis_part(
        p,
        textures,
        &format!("{prefix}_fore"),
        pose.far.elbow,
        pose.far.wrist,
        tint.gamma_multiply(0.92),
    );
    let head = body.frame.parts.iter().find(|p| p.id == "head").unwrap();
    paint(p, textures, head, tint, |p| body.upper(p));
    if motion.throwing <= 0. && motion.melee <= 0. {
        super::weapon(p, sheet, &pose.gun, tint);
    }
    if !near_axis_part(
        p,
        textures,
        &format!("{prefix}_near_upper"),
        pose.near.shoulder,
        pose.near.elbow,
        tint,
        body.mirror,
    ) {
        axis_part(
            p,
            textures,
            &format!("{prefix}_upper"),
            pose.near.shoulder,
            pose.near.elbow,
            tint,
        );
    }
    if !near_axis_part(
        p,
        textures,
        &format!("{prefix}_near_fore"),
        pose.near.elbow,
        pose.near.wrist,
        tint,
        body.mirror,
    ) {
        axis_part(
            p,
            textures,
            &format!("{prefix}_fore"),
            pose.near.elbow,
            pose.near.wrist,
            tint,
        );
    }
    for hand in [pose.far_hand, pose.near_hand] {
        paint(p, textures, hand.part, tint, |p| hand.point(p));
    }
    if motion.throwing > 0.7
        && let Some(kind) = motion.throw_kind
    {
        super::super::ordnance_sprite(
            p,
            textures,
            kind,
            pose.near_hand.pose.palm,
            Vec2::splat(size.y * 0.10),
            0.,
        );
    }
    if motion.melee > 0.
        && let Some(item) = &style.equipment.melee
    {
        super::pattern(
            p,
            &item.style,
            pose.near_hand.pose.palm + vec2(size.y * 0.12, 0.),
            size.y / 60.,
            egui::emath::Rot2::IDENTITY,
            false,
            [
                Color32::from_rgb(40, 43, 34),
                Color32::from_rgb(112, 121, 103),
                Color32::from_rgb(177, 175, 143),
            ],
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
    textures.canonical?;
    textures.menu_idle?;
    Some(
        Pose::new(textures.rig?.atlas, female, center, size, motion)
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
    textures.canonical?;
    textures.menu_idle?;
    Some(
        Pose::new(textures.rig?.atlas, female, center, size, motion)
            .near_hand
            .pose
            .palm,
    )
}
