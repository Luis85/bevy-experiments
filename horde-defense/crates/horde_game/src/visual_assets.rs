//! Presentation assets. Asset handles and atlas coordinates never enter simulation state.
use crate::animation::ActorPose;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{Color32, Painter, Pos2, Rect, TextureId, Vec2, pos2, vec2},
};
use horde_core::{ordnance::OrdnanceKind, weapons::WeaponClass};
use std::sync::OnceLock;
// Characters and equipment previews share the source-owned model resolver.
mod player_preview;
pub use player_preview::{defender_preview, hold_sample, paint_model};

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
mod ground_material;

static FLASH_MASKS: OnceLock<FlashMasks> = OnceLock::new();
static RIG_ATLAS: OnceLock<RigAtlas> = OnceLock::new();

/// Cached geometry derived from the unchanged source alpha, never a new raster.
#[derive(Debug)]
pub struct FlashMasks {
    walkers: Vec<FlashMask>,
    actors: Vec<FlashMask>,
}
#[derive(Debug)]
struct FlashMask {
    runs: Vec<[f32; 4]>,
}
impl FlashMask {
    fn from_rgba(width: u32, pixels: &[u8], bounds: [f32; 4]) -> Self {
        let mut runs = Vec::new();
        // A conservative 2x2 block is lit only when every source pixel is solid.
        // Transparent RGB noise cannot become a rectangular additive glow.
        for y in (bounds[1] as u32..bounds[3] as u32).step_by(2) {
            let bottom = (y + 2).min(bounds[3] as u32);
            let mut start = None;
            for x in (bounds[0] as u32..bounds[2] as u32).step_by(2) {
                let right = (x + 2).min(bounds[2] as u32);
                let solid = (y..bottom).all(|row| {
                    (x..right)
                        .all(|column| pixels[((row * width + column) * 4 + 3) as usize] >= 128)
                });
                if solid {
                    start.get_or_insert(x);
                }
                if (!solid || right == bounds[2] as u32)
                    && let Some(left) = start.take()
                {
                    runs.push([
                        left as f32,
                        y as f32,
                        if solid { right } else { x } as f32,
                        bottom as f32,
                    ]);
                }
            }
        }
        // The measured art is well below this bound; an accidental high-frequency
        // replacement cannot allocate an unbounded flash mesh during combat.
        runs.truncate(2048);
        Self { runs }
    }
}
impl FlashMasks {
    fn from_images(walkers: &Image, actors: &Image) -> Self {
        let walk_pixels = walkers
            .data
            .as_deref()
            .expect("RGBA walker pixels remain available");
        let actor_pixels = actors
            .data
            .as_deref()
            .expect("RGBA actor pixels remain available");
        Self {
            walkers: WALK_SPECS
                .iter()
                .flat_map(|spec| spec.frames.iter())
                .map(|frame| FlashMask::from_rgba(walkers.width(), walk_pixels, frame.bounds))
                .collect(),
            actors: ACTOR_RECTS[..5]
                .iter()
                .map(|bounds| FlashMask::from_rgba(actors.width(), actor_pixels, *bounds))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HitFlash {
    pub strength: f32,
    pub reduced: bool,
}

#[derive(Resource)]
pub struct VisualAssets {
    actors: Handle<Image>,
    props: Handle<Image>,
    keyart: Handle<Image>,
    ground: Handle<Image>,
    walkers: Handle<Image>,
    /// Legacy `defender-rig-v1.png`: now only the firearm/ordnance icon source.
    rig: Handle<Image>,
    menu_idle: Handle<Image>,
    egui_textures: [OnceLock<Result<bevy_egui::egui::TextureHandle, String>>; 7],
}
impl VisualAssets {
    pub fn all_loaded(&self, images: &Assets<Image>) -> bool {
        [
            &self.actors,
            &self.props,
            &self.keyart,
            &self.ground,
            &self.walkers,
            &self.rig,
            &self.menu_idle,
        ]
        .into_iter()
        .all(|handle| images.get(handle).is_some())
    }
}
pub fn load_assets(mut commands: Commands, server: Res<AssetServer>) {
    let pixel = |path: &'static str| {
        server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::nearest();
                // Egui owns the only GPU copy; retain untouched source pixels
                // for atlas calibration and alpha masks in the main world.
                settings.asset_usage = bevy::asset::RenderAssetUsages::MAIN_WORLD;
            })
            .load(path)
    };
    commands.insert_resource(VisualAssets {
        actors: pixel("art/pixel/actors-v1.png"),
        props: pixel("art/pixel/props-v1.png"),
        keyart: pixel("art/pixel/keyart-v2.png"),
        ground: pixel("art/pixel/terrain-v1.png"),
        walkers: pixel("art/pixel/walkers-v1.png"),
        rig: pixel("art/pixel/defender-rig-v1.png"),
        menu_idle: pixel("art/pixel/menu-survivors-v1.png"),
        egui_textures: std::array::from_fn(|_| OnceLock::new()),
    });
}
#[derive(Clone, Copy, Default, Debug)]
pub struct ArtTextures {
    pub actors: Option<TextureId>,
    pub props: Option<TextureId>,
    pub keyart: Option<TextureId>,
    pub ground: Option<TextureId>,
    pub walkers: [Option<WalkSheet>; 5],
    pub flash_masks: Option<&'static FlashMasks>,
    /// Weapon and ordnance icon atlas (legacy defender rig sheet; no bodies are drawn from it).
    pub rig: Option<RigSheet>,
    pub menu_idle: Option<TextureId>,
}
#[derive(Clone, Copy, Debug)]
pub struct RigSheet {
    pub texture: TextureId,
    pub atlas: &'static RigAtlas,
}
/// Icon cells cut from the legacy defender rig sheet: 4..=9 firearms, 10..=13 ordnance.
/// The body cells (0..=3) are retained only because the grid is cut uniformly.
#[derive(Debug)]
pub struct RigAtlas {
    size: [f32; 2],
    cells: [[f32; 4]; 16],
}
impl RigAtlas {
    fn from_image(image: &Image) -> Self {
        let width = image.width();
        let height = image.height();
        let pixels = image
            .data
            .as_deref()
            .expect("RGBA rig pixels remain available");
        // The authored rows are deliberately unequal: standing boots extend
        // farther than the isolated guns. Equal quarters would amputate feet.
        let rows = [0., 0.34, 0.51, 0.75, 1.];
        let cells = std::array::from_fn(|index| {
            let left = index as u32 % 4 * width / 4;
            let right = (index as u32 % 4 + 1) * width / 4;
            let top = (rows[index / 4] * height as f32) as u32;
            let bottom = (rows[index / 4 + 1] * height as f32) as u32;
            let mut bounds = [right, bottom, left, top];
            for y in top..bottom {
                for x in left..right {
                    if pixels[((y * width + x) * 4 + 3) as usize] > 8 {
                        bounds[0] = bounds[0].min(x);
                        bounds[1] = bounds[1].min(y);
                        bounds[2] = bounds[2].max(x + 1);
                        bounds[3] = bounds[3].max(y + 1);
                    }
                }
            }
            if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
                [left as f32, top as f32, right as f32, bottom as f32]
            } else {
                [
                    bounds[0].saturating_sub(2).max(left) as f32,
                    bounds[1].saturating_sub(2).max(top) as f32,
                    (bounds[2] + 2).min(right) as f32,
                    (bounds[3] + 2).min(bottom) as f32,
                ]
            }
        });
        Self {
            size: [width as f32, height as f32],
            cells,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WalkSheet {
    pub texture: TextureId,
    pub size: [u32; 2],
}
#[derive(Clone, Copy, Debug, Default)]
pub struct PropMotion {
    pub angle: f32,
    pub target: Option<Pos2>,
    pub recoil: f32,
    pub trigger: f32,
    pub hit: f32,
}
/// bevy_egui 0.43.1 uses premultiplied blending and its fragment shader does
/// not premultiply external textures. Convert straight PNG RGBA through egui's
/// supported managed-image path once, preserving nearest sampling and originals.
fn image_for_egui(image: &Image) -> bevy_egui::egui::ColorImage {
    bevy_egui::egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image
            .data
            .as_deref()
            .expect("Original RGBA pixels remain in main world"),
    )
}

fn managed_texture(
    context: &bevy_egui::egui::Context,
    name: String,
    image: &Image,
    max_side: usize,
) -> Result<bevy_egui::egui::TextureHandle, String> {
    // Egui's backend-independent default is 2048. The active render device,
    // rather than that default or an assumed adapter maximum, owns this budget.
    context.input_mut(|input| {
        input.max_texture_side = max_side;
        input.raw.max_texture_side = Some(max_side);
    });
    if image.width() as usize > max_side || image.height() as usize > max_side {
        return Err(format!(
            "Art texture {name} is {}x{}, exceeding the active render device limit {max_side}; using procedural fallback",
            image.width(),
            image.height()
        ));
    }
    Ok(context.load_texture(
        name,
        image_for_egui(image),
        bevy_egui::egui::TextureOptions::NEAREST,
    ))
}

pub fn register(
    contexts: &mut EguiContexts,
    visuals: &VisualAssets,
    images: &Assets<Image>,
    max_texture_side: usize,
) -> ArtTextures {
    let Ok(context) = contexts.ctx_mut() else {
        return ArtTextures::default();
    };
    let context = context.clone();
    let texture = |slot: usize, handle: &Handle<Image>| {
        let image = images.get(handle)?;
        let cached = visuals.egui_textures[slot].get_or_init(|| {
            let uploaded = managed_texture(
                &context,
                format!("lastlight-art-{slot}"),
                image,
                max_texture_side,
            );
            if let Err(error) = &uploaded {
                warn!("{error}");
            }
            uploaded
        });
        cached.as_ref().ok().map(bevy_egui::egui::TextureHandle::id)
    };
    let sheet = images.get(&visuals.walkers).and_then(|image| {
        texture(4, &visuals.walkers).map(|texture| WalkSheet {
            texture,
            size: [image.width(), image.height()],
        })
    });
    let flash_masks = images
        .get(&visuals.walkers)
        .zip(images.get(&visuals.actors))
        .map(|(walkers, actors)| {
            FLASH_MASKS.get_or_init(|| FlashMasks::from_images(walkers, actors))
        });
    let rig = images.get(&visuals.rig).and_then(|image| {
        texture(5, &visuals.rig).map(|texture| RigSheet {
            texture,
            atlas: RIG_ATLAS.get_or_init(|| RigAtlas::from_image(image)),
        })
    });
    ArtTextures {
        actors: texture(0, &visuals.actors),
        props: texture(1, &visuals.props),
        keyart: texture(2, &visuals.keyart),
        ground: texture(3, &visuals.ground),
        walkers: [sheet; 5],
        flash_masks,
        rig,
        menu_idle: texture(6, &visuals.menu_idle),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Shambler,
    Runner,
    Brute,
    Armored,
    Demolisher,
    Breacher,
    Player,
    Mara,
    Caller,
    SiegeBrute,
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
// Metadata uses untouched source pixels; final values are measured from each
// generated sheet's alpha, with transparent padding around every silhouette.
// Alpha <=8 is only the generator's invisible fringe/noise, not figure content;
// all more-opaque hands, boots and equipment are enclosed with 3-4px padding.
// Hips are the median solid pixels in the 51-64% torso band. Each gait keeps
// its row's deepest boot baseline; passing feet are never pulled to the floor.
const ACTOR_SIZE: [f32; 2] = [1774., 887.];
const PROP_SIZE: [f32; 2] = [1261., 1247.];
const WALK_SIZE: [f32; 2] = [1122., 1402.];
const GROUND_SIZE: [f32; 2] = [2172., 724.];
const KEYART_RATIO: f32 = 1672. / 941.;
const ACTOR_RECTS: [[f32; 4]; 8] = [
    [72., 47., 396., 430.],
    [506., 58., 826., 432.],
    [917., 10., 1293., 435.],
    [1371., 10., 1700., 432.],
    [10., 447., 428., 877.],
    [496., 513., 816., 837.],
    [925., 517., 1257., 835.],
    [1342., 619., 1768., 758.],
];
// Solid torso median, deepest boot and occupied height for each standing actor.
const ACTOR_ANCHORS: [[f32; 3]; 5] = [
    [195., 427., 377.],
    [622., 429., 368.],
    [1109., 432., 419.],
    [1577., 429., 416.],
    [204., 874., 424.],
];
const PROP_RECTS: [[f32; 4]; 16] = [
    [25., 54., 295., 287.],
    [325., 89., 613., 277.],
    [658., 57., 933., 291.],
    [951., 110., 1243., 241.],
    [38., 343., 292., 605.],
    [341., 340., 595., 600.],
    [644., 341., 919., 600.],
    [998., 395., 1202., 564.],
    [15., 628., 305., 915.],
    [362., 624., 580., 926.],
    [649., 734., 907., 901.],
    [952., 677., 1247., 902.],
    [12., 969., 322., 1222.],
    [336., 994., 612., 1190.],
    [644., 984., 935., 1197.],
    [1006., 925., 1205., 1233.],
];
#[derive(Clone, Copy)]
struct WalkFrame {
    bounds: [f32; 4],
    hip_x: f32,
}
#[derive(Clone, Copy)]
struct WalkSpec {
    frames: [WalkFrame; 4],
    feet_y: f32,
    height: f32,
    source_size: [f32; 2],
}
const WALK_SPECS: [WalkSpec; 5] = [
    WalkSpec {
        frames: [
            WalkFrame {
                bounds: [74., 18., 250., 275.],
                hip_x: 173.0,
            },
            WalkFrame {
                bounds: [357., 18., 499., 275.],
                hip_x: 440.0,
            },
            WalkFrame {
                bounds: [609., 18., 782., 275.],
                hip_x: 714.0,
            },
            WalkFrame {
                bounds: [897., 18., 1041., 275.],
                hip_x: 977.0,
            },
        ],
        feet_y: 271.,
        height: 249.,
        source_size: WALK_SIZE,
    },
    WalkSpec {
        frames: [
            WalkFrame {
                bounds: [69., 302., 257., 544.],
                hip_x: 172.0,
            },
            WalkFrame {
                bounds: [358., 302., 511., 544.],
                hip_x: 439.0,
            },
            WalkFrame {
                bounds: [608., 302., 795., 544.],
                hip_x: 707.0,
            },
            WalkFrame {
                bounds: [892., 301., 1041., 544.],
                hip_x: 975.0,
            },
        ],
        feet_y: 540.,
        height: 235.,
        source_size: WALK_SIZE,
    },
    WalkSpec {
        frames: [
            WalkFrame {
                bounds: [38., 567., 271., 837.],
                hip_x: 162.0,
            },
            WalkFrame {
                bounds: [331., 562., 531., 837.],
                hip_x: 432.0,
            },
            WalkFrame {
                bounds: [575., 569., 814., 837.],
                hip_x: 710.0,
            },
            WalkFrame {
                bounds: [871., 569., 1074., 837.],
                hip_x: 980.0,
            },
        ],
        feet_y: 833.,
        height: 267.,
        source_size: WALK_SIZE,
    },
    WalkSpec {
        frames: [
            WalkFrame {
                bounds: [68., 852., 251., 1116.],
                hip_x: 171.0,
            },
            WalkFrame {
                bounds: [345., 851., 504., 1116.],
                hip_x: 439.0,
            },
            WalkFrame {
                bounds: [605., 855., 791., 1116.],
                hip_x: 716.0,
            },
            WalkFrame {
                bounds: [887., 851., 1050., 1116.],
                hip_x: 981.0,
            },
        ],
        feet_y: 1112.,
        height: 257.,
        source_size: WALK_SIZE,
    },
    WalkSpec {
        frames: [
            WalkFrame {
                bounds: [64., 1130., 249., 1391.],
                hip_x: 163.0,
            },
            WalkFrame {
                bounds: [344., 1135., 520., 1391.],
                hip_x: 433.0,
            },
            WalkFrame {
                bounds: [601., 1135., 789., 1391.],
                hip_x: 706.0,
            },
            WalkFrame {
                bounds: [877., 1136., 1059., 1391.],
                hip_x: 976.0,
            },
        ],
        feet_y: 1387.,
        height: 253.,
        source_size: WALK_SIZE,
    },
];
fn walk_spec(actor: Actor) -> Option<WalkSpec> {
    WALK_SPECS.get(actor as usize).copied()
}
fn region_for(px: [f32; 4], size: [f32; 2]) -> Rect {
    Rect::from_min_max(
        pos2(px[0] / size[0], px[1] / size[1]),
        pos2(px[2] / size[0], px[3] / size[1]),
    )
}
fn region(px: [f32; 4]) -> Rect {
    region_for(px, PROP_SIZE)
}
fn fit(center: Pos2, size: Vec2, source: [f32; 4]) -> Rect {
    let ratio = (source[2] - source[0]) / (source[3] - source[1]);
    let height = size.y.min(size.x / ratio);
    Rect::from_center_size(center, vec2(height * ratio, height))
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
    let sheet = if (kind as usize) < textures.walkers.len() {
        textures.walkers[kind as usize]
    } else {
        None
    };
    let (texture, rect, uv, pivot) = if let Some(sheet) = sheet
        && let Some((rect, uv, pivot)) =
            walk_frame_layout(kind, sheet.size, pose.frame, center, size)
    {
        (Some(sheet.texture), rect, uv, pivot)
    } else {
        let index = match kind {
            Actor::Player => 0,
            Actor::Mara => 1,
            Actor::Breacher => 2,
            Actor::Caller => 3,
            Actor::SiegeBrute => 4,
            _ => return false,
        };
        let source = ACTOR_RECTS[index];
        let (rect, pivot) = standing_actor_layout(index, center, size);
        (textures.actors, rect, region_for(source, ACTOR_SIZE), pivot)
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

/// Brief impact light follows the exact current pose and stays inside solid
/// source pixels. It cannot expose transparent atlas RGB or light a rectangle.
pub fn actor_hit_flash(
    p: &Painter,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    flash: HitFlash,
    pose: &ActorPose,
) {
    if flash.strength <= 0. {
        return;
    }
    let Some(masks) = textures.flash_masks else {
        return;
    };
    let (mask, source, rect, pivot) = if let Some(spec) = walk_spec(kind) {
        let frame = (pose.frame % 4) as usize;
        let Some(sheet) = textures.walkers[kind as usize] else {
            return;
        };
        let Some((rect, _, pivot)) = walk_frame_layout(kind, sheet.size, pose.frame, center, size)
        else {
            return;
        };
        (
            &masks.walkers[kind as usize * 4 + frame],
            spec.frames[frame].bounds,
            rect,
            pivot,
        )
    } else {
        let index = match kind {
            Actor::Player => 0,
            Actor::Mara => 1,
            Actor::Breacher => 2,
            Actor::Caller => 3,
            Actor::SiegeBrute => 4,
            _ => return,
        };
        if textures.actors.is_none() {
            return;
        }
        let (rect, pivot) = standing_actor_layout(index, center, size);
        (&masks.actors[index], ACTOR_RECTS[index], rect, pivot)
    };
    let color = hit_flash_color(flash);
    let mut mesh = bevy_egui::egui::epaint::Mesh::default();
    for run in &mask.runs {
        let point = |x, y| {
            rect.min
                + vec2(
                    (x - source[0]) / (source[2] - source[0]) * rect.width(),
                    (y - source[1]) / (source[3] - source[1]) * rect.height(),
                )
        };
        mesh.add_colored_rect(
            Rect::from_min_max(point(run[0], run[1]), point(run[2], run[3])),
            color,
        );
    }
    apply_actor_pose(&mut mesh, pose, pivot);
    p.add(bevy_egui::egui::Shape::mesh(mesh));
}
fn hit_flash_color(flash: HitFlash) -> Color32 {
    if flash.reduced {
        // One steady warm tint during impact, with no bright additive burst.
        Color32::from_rgba_unmultiplied(238, 128, 92, 72)
    } else {
        let light = (flash.strength.clamp(0., 1.) * 210.).round() as u8;
        Color32::from_rgb_additive(
            light,
            (u16::from(light) * 9 / 10) as u8,
            (u16::from(light) * 4 / 5) as u8,
        )
    }
}
fn standing_actor_layout(index: usize, center: Pos2, available: Vec2) -> (Rect, Pos2) {
    let source = ACTOR_RECTS[index];
    let [hip, feet, height] = ACTOR_ANCHORS[index];
    let scale = (available.y / height).min(available.x / (source[2] - source[0]));
    let foot = center + vec2(0., available.y * 0.5);
    let rect = Rect::from_min_max(
        foot + vec2((source[0] - hip) * scale, (source[1] - feet) * scale),
        foot + vec2((source[2] - hip) * scale, (source[3] - feet) * scale),
    );
    (rect, foot)
}
fn walk_frame_layout(
    actor: Actor,
    size: [u32; 2],
    frame: u8,
    center: Pos2,
    available: Vec2,
) -> Option<(Rect, Rect, Pos2)> {
    let spec = walk_spec(actor)?;
    let frame = spec.frames[(frame % 4) as usize];
    let max_width = spec
        .frames
        .iter()
        .map(|frame| frame.bounds[2] - frame.bounds[0])
        .fold(0., f32::max);
    let scale = (available.y / spec.height).min(available.x / max_width);
    let foot = center + vec2(0., available.y * 0.5);
    let rect = Rect::from_min_max(
        foot + vec2(
            (frame.bounds[0] - frame.hip_x) * scale,
            (frame.bounds[1] - spec.feet_y) * scale,
        ),
        foot + vec2(
            (frame.bounds[2] - frame.hip_x) * scale,
            (frame.bounds[3] - spec.feet_y) * scale,
        ),
    );
    // Pixel-space metadata remains valid for a uniformly rescaled copy of the
    // complete atlas; the measured common baseline never follows a lifted foot.
    let gutter = vec2(0.5 / size[0].max(1) as f32, 0.5 / size[1].max(1) as f32);
    let uv = Rect::from_min_max(
        pos2(
            frame.bounds[0] / spec.source_size[0],
            frame.bounds[1] / spec.source_size[1],
        ) + gutter,
        pos2(
            frame.bounds[2] / spec.source_size[0],
            frame.bounds[3] / spec.source_size[1],
        ) - gutter,
    );
    Some((rect, uv, foot))
}
/// Every settled body shares one draw mesh. Count and lifetime come from the
/// saved attempt; only the short collapse animation is rendered separately.
pub fn corpse_batch(
    p: &Painter,
    textures: ArtTextures,
    bodies: impl IntoIterator<Item = (Pos2, Vec2, Color32)>,
) -> bool {
    let Some(texture) = textures.actors else {
        return false;
    };
    let source = ACTOR_RECTS[7];
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
        if npc { 6 } else { 5 },
        center,
        size,
        Color32::WHITE,
    )
}
fn actor_region(
    p: &Painter,
    textures: ArtTextures,
    index: usize,
    center: Pos2,
    size: Vec2,
    tint: Color32,
) -> bool {
    let Some(texture) = textures.actors else {
        return false;
    };
    let source = ACTOR_RECTS[index];
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cohesive_menu_poses_keep_both_boots_and_lower_body_fixed_while_upper_body_breathes() {
        let foot = pos2(320.0, 410.0);
        let height = 190.0;
        for female in [false, true] {
            let (source, rect, waist) = menu_idle_layout(female, foot, height);
            let scale = height / (848.0 - source[1]);
            assert!((rect.left() + (source[2] - source[0]) * 0.53 * scale - foot.x).abs() < 0.001);
            assert!((rect.top() + (848.0 - source[1]) * scale - foot.y).abs() < 0.001);
            assert!(waist.y < foot.y && waist.y > rect.top());
            let ctx = bevy_egui::egui::Context::default();
            let mut lower = None;
            for breath in [0.0, 0.008, -0.008] {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    assert!(menu_survivor(
                        ui.painter(),
                        ArtTextures {
                            menu_idle: Some(TextureId::User(88)),
                            ..Default::default()
                        },
                        female,
                        foot,
                        height,
                        breath
                    ));
                });
                let meshes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                            Some(mesh)
                        } else {
                            None
                        }
                    })
                    .collect();
                let vertices: Vec<_> = meshes[0]
                    .vertices
                    .iter()
                    .map(|vertex| (vertex.pos, vertex.uv))
                    .collect();
                if let Some(original) = &lower {
                    assert_eq!(&vertices, original);
                } else {
                    lower = Some(vertices);
                }
                output.textures_delta.clear();
            }
        }
    }
    #[test]
    fn terrain_managed_upload_uses_advertised_device_budget_above_egui_default() {
        let image = Image::from_buffer(
            include_bytes!("../../../assets/art/pixel/terrain-v1.png"),
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap();
        assert_eq!([image.width(), image.height()], [2172, 724]);
        let original = image.data.clone();
        let context = bevy_egui::egui::Context::default();
        let mut output = context.run_ui(Default::default(), |_| {
            assert_eq!(context.input(|i| i.max_texture_side), 2048);
            let handle =
                managed_texture(&context, "wide terrain regression".into(), &image, 4096).unwrap();
            assert_eq!(handle.size(), [2172, 724]);
            assert_eq!(context.input(|i| i.max_texture_side), 4096);
            assert_eq!(context.input(|i| i.raw.max_texture_side), Some(4096));
        });
        assert!(
            output
                .textures_delta
                .set
                .values()
                .flatten()
                .any(|delta| delta.image.size() == [2172, 724])
        );
        output.textures_delta.clear();
        assert_eq!(image.data, original);
    }
    #[test]
    fn terrain_larger_than_actual_device_budget_returns_fallback_error_without_uploading() {
        let image = Image::from_buffer(
            include_bytes!("../../../assets/art/pixel/terrain-v1.png"),
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap();
        let context = bevy_egui::egui::Context::default();
        let mut output = context.run_ui(Default::default(), |_| {
            let error = managed_texture(&context, "unsupported terrain".into(), &image, 2048)
                .err()
                .expect("device budget must be enforced");
            assert!(error.contains("2172x724") && error.contains("limit 2048"));
        });
        assert!(
            !output
                .textures_delta
                .set
                .values()
                .flatten()
                .any(|delta| delta.image.size() == [2172, 724])
        );
        output.textures_delta.clear();
    }
    #[test]
    fn managed_cutout_upload_removes_transparent_rgb_without_changing_opaque_source_pixels() {
        for bytes in [
            include_bytes!("../../../assets/art/pixel/actors-v1.png").as_slice(),
            include_bytes!("../../../assets/art/pixel/props-v1.png").as_slice(),
            include_bytes!("../../../assets/art/pixel/walkers-v1.png").as_slice(),
            include_bytes!("../../../assets/art/pixel/defender-rig-v1.png").as_slice(),
        ] {
            let image = Image::from_buffer(
                bytes,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                true,
                ImageSampler::nearest(),
                bevy::asset::RenderAssetUsages::MAIN_WORLD,
            )
            .unwrap();
            let original = image.data.clone().unwrap();
            let converted = image_for_egui(&image);
            let mut transparent = 0;
            for (source, pixel) in original.as_chunks::<4>().0.iter().zip(&converted.pixels) {
                let rgba = pixel.to_array();
                assert_eq!(rgba[3], source[3]);
                if source[3] == 0 {
                    transparent += 1;
                    assert_eq!(rgba, [0, 0, 0, 0]);
                } else if source[3] == 255 {
                    assert_eq!(rgba, *source);
                } else {
                    assert!(rgba[..3].iter().all(|channel| *channel <= rgba[3]));
                }
            }
            assert!(transparent > 0);
            assert_eq!(
                image.data.as_ref().unwrap(),
                &original,
                "Upload must never mutate the original atlas"
            );
        }
    }
    #[test]
    fn every_settled_body_is_batched_without_a_count_cap() {
        let ctx = bevy_egui::egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            assert!(corpse_batch(
                ui.painter(),
                ArtTextures {
                    actors: Some(TextureId::User(77)),
                    ..Default::default()
                },
                (0..1024).map(|n| (pos2(n as f32, 30.), vec2(20., 10.), Color32::WHITE))
            ));
        });
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].vertices.len(), 1024 * 4);
        assert_eq!(meshes[0].indices.len(), 1024 * 6);
        output.textures_delta.clear();
    }
    #[test]
    fn weapon_icon_atlas_cells_cover_every_firearm_and_ordnance_icon_with_solid_pixels() {
        let image = Image::from_buffer(
            include_bytes!("../../../assets/art/pixel/defender-rig-v1.png"),
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap();
        let atlas = RigAtlas::from_image(&image);
        let pixels = image.data.as_ref().unwrap();
        for cell in [4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
            let [left, top, right, bottom] = atlas.cells[cell];
            let solid = (top as u32..bottom as u32)
                .flat_map(|y| (left as u32..right as u32).map(move |x| (x, y)))
                .any(|(x, y)| pixels[((y * image.width() + x) * 4 + 3) as usize] >= 128);
            assert!(solid, "Icon cell {cell} must contain displayed source art");
        }
    }
    #[test]
    fn peripheral_sprite_bases_share_the_requested_ground_plane() {
        let ctx = bevy_egui::egui::Context::default();
        let foot = pos2(200., 160.);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for kind in [
                Prop::TreeBroadleaf,
                Prop::TreePine,
                Prop::Bush,
                Prop::Fence,
                Prop::Wreck,
                Prop::Debris,
                Prop::Rock,
                Prop::Lamp,
            ] {
                assert!(prop_grounded(
                    ui.painter(),
                    ArtTextures {
                        props: Some(TextureId::User(2)),
                        ..Default::default()
                    },
                    kind,
                    foot,
                    vec2(90., 80.),
                    Color32::WHITE
                ));
            }
        });
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(meshes.len(), 8);
        for mesh in meshes {
            let bottom = mesh
                .vertices
                .iter()
                .map(|vertex| vertex.pos.y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (bottom - foot.y).abs() < 0.001,
                "Wide wrecks and tall canopies share a planted baseline"
            );
        }
        output.textures_delta.clear();
    }
    #[test]
    fn impact_mask_ignores_transparent_rgb_and_translucent_fringe() {
        let mut pixels = vec![255; 8 * 6 * 4];
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 0;
        }
        assert!(
            FlashMask::from_rgba(8, &pixels, [0., 0., 8., 6.])
                .runs
                .is_empty()
        );
        for y in 2..4 {
            for x in 2..6 {
                pixels[(y * 8 + x) * 4 + 3] = 255;
            }
        }
        pixels[(2 * 8 + 2) * 4 + 3] = 127;
        let mask = FlashMask::from_rgba(8, &pixels, [0., 0., 8., 6.]);
        assert_eq!(
            mask.runs,
            vec![[4., 2., 6., 4.]],
            "Only completely solid 2x2 pixel blocks may receive additive light"
        );
        let bright = hit_flash_color(HitFlash {
            strength: 1.,
            reduced: false,
        });
        assert!(bright.is_additive() && bright.r() >= 200 && bright.g() >= 180);
        let reduced = hit_flash_color(HitFlash {
            strength: 1.,
            reduced: true,
        });
        assert!(!reduced.is_additive() && reduced.a() <= 80);
    }
    #[test]
    fn every_enemy_gait_and_boss_has_a_bounded_solid_silhouette_flash() {
        let decode = |bytes| {
            Image::from_buffer(
                bytes,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                true,
                ImageSampler::nearest(),
                bevy::asset::RenderAssetUsages::default(),
            )
            .unwrap()
        };
        let walkers = decode(include_bytes!("../../../assets/art/pixel/walkers-v1.png"));
        let actors = decode(include_bytes!("../../../assets/art/pixel/actors-v1.png"));
        let masks = FlashMasks::from_images(&walkers, &actors);
        assert_eq!(masks.walkers.len(), 5 * 4);
        assert_eq!(masks.actors.len(), 5);
        for (masks, image) in [(&masks.walkers, &walkers), (&masks.actors, &actors)] {
            let pixels = image.data.as_ref().unwrap();
            for mask in masks {
                assert!(!mask.runs.is_empty() && mask.runs.len() < 2048);
                for [left, top, right, bottom] in mask.runs.iter().copied() {
                    for y in top as u32..bottom as u32 {
                        for x in left as u32..right as u32 {
                            assert!(pixels[((y * image.width() + x) * 4 + 3) as usize] >= 128);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn calibrated_pixel_sources_keep_their_original_dimensions_and_rgba_pixels() {
        for (name, bytes, size, expected) in [
            (
                "defender-rig",
                include_bytes!("../../../assets/art/pixel/defender-rig-v1.png").as_slice(),
                [1254., 1254.],
                0x980a_ee7b_fe71_c4bc,
            ),
            (
                "actors",
                include_bytes!("../../../assets/art/pixel/actors-v1.png").as_slice(),
                ACTOR_SIZE,
                0x7c28_5d8a_91be_d1f4,
            ),
            (
                "terrain",
                include_bytes!("../../../assets/art/pixel/terrain-v1.png").as_slice(),
                GROUND_SIZE,
                0xbe09_5ad1_ad84_d49d,
            ),
            (
                "keyart",
                include_bytes!("../../../assets/art/pixel/keyart-v1.png").as_slice(),
                [1672., 941.],
                0x6759_02fc_df6f_a532,
            ),
            (
                "menu-survivors",
                include_bytes!("../../../assets/art/pixel/menu-survivors-v1.png").as_slice(),
                MENU_IDLE_SIZE,
                0xf6b2_cb17_2a17_bb3d,
            ),
            (
                "keyart-v2",
                include_bytes!("../../../assets/art/pixel/keyart-v2.png").as_slice(),
                [1672., 941.],
                0xf98c_8862_bc3c_db95,
            ),
            (
                "walkers",
                include_bytes!("../../../assets/art/pixel/walkers-v1.png").as_slice(),
                WALK_SIZE,
                0x5540_6952_c6b3_c223,
            ),
            (
                "props",
                include_bytes!("../../../assets/art/pixel/props-v1.png").as_slice(),
                PROP_SIZE,
                0x14d9_9e7a_1bcf_2aa1,
            ),
        ] {
            let image = Image::from_buffer(
                bytes,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                true,
                ImageSampler::nearest(),
                bevy::asset::RenderAssetUsages::default(),
            )
            .expect("Original pixel atlas must decode");
            assert_eq!([image.width() as f32, image.height() as f32], size);
            let pixels = image.data.as_ref().expect("RGBA atlas retains CPU pixels");
            let hash = pixels.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            });
            assert_eq!(
                hash, expected,
                "{name} source pixels changed; inspect and recalibrate explicitly"
            );
        }
    }
    #[test]
    fn survivor_and_boss_torso_pivots_stay_on_the_fixed_foot_anchor() {
        for (index, [hip, feet, _]) in ACTOR_ANCHORS.into_iter().enumerate() {
            let center = pos2(200., 150.);
            let available = vec2(70., 60.);
            let (rect, foot) = standing_actor_layout(index, center, available);
            let source = ACTOR_RECTS[index];
            let scale = rect.width() / (source[2] - source[0]);
            let reconstructed =
                rect.min + vec2((hip - source[0]) * scale, (feet - source[1]) * scale);
            assert!(reconstructed.distance(foot) < 0.001);
            assert_eq!(foot, center + vec2(0., available.y * 0.5));
            assert!(source[0] < hip && hip < source[2]);
            assert!(source[1] < feet && feet < source[3]);
        }
    }
    #[test]
    fn calibrated_turret_bore_targets_every_quadrant_with_a_planted_mount() {
        for available in [vec2(45., 70.), vec2(120., 60.)] {
            let center = pos2(200., 180.);
            let rect = fit(center, available, PROP_RECTS[Prop::Turret as usize]);
            let mount = turret_source_point(rect, TURRET_MOUNT);
            for degrees in [
                -179_f32, -135., -91., -90., -89., -45., 0., 45., 89., 90., 91., 135., 179.,
            ] {
                let angle = degrees.to_radians();
                let target = mount + vec2(angle.cos(), angle.sin()) * 150.;
                for recoil in [0., 0.5, 1.] {
                    let motion = PropMotion {
                        target: Some(target),
                        recoil,
                        ..Default::default()
                    };
                    let transform = TurretTransform::new(rect, motion);
                    assert!(transform.point(mount).distance(mount) < 0.0001);
                    let muzzle = turret_muzzle(center, available, motion);
                    let breech = transform.point(turret_source_point(rect, TURRET_BREECH));
                    let bore = (muzzle - breech).normalized();
                    let requested = (target - muzzle).normalized();
                    assert!(
                        bore.dot(requested) > 0.99999,
                        "Bore points away from target at {degrees}°, recoil={recoil}: {bore:?} / {requested:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn aiming_and_recoil_leave_the_pixel_tripod_vertices_and_uv_unchanged() {
        let ctx = bevy_egui::egui::Context::default();
        let mut bases = Vec::new();
        for target in [pos2(400., 20.), pos2(10., 300.), pos2(400., 300.)] {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                prop_animated(
                    ui.painter(),
                    ArtTextures {
                        props: Some(TextureId::User(1)),
                        ..Default::default()
                    },
                    Prop::Turret,
                    pos2(200., 180.),
                    vec2(100., 100.),
                    Color32::WHITE,
                    PropMotion {
                        target: Some(target),
                        recoil: 1.,
                        ..Default::default()
                    },
                );
            });
            let meshes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                        Some(mesh)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                meshes.len(),
                2,
                "A separate fixed base and moving head must be rendered"
            );
            bases.push(
                meshes[0]
                    .vertices
                    .iter()
                    .map(|v| (v.pos, v.uv))
                    .collect::<Vec<_>>(),
            );
            output.textures_delta.clear();
        }
        assert_eq!(bases[0], bases[1]);
        assert_eq!(bases[1], bases[2]);
    }
    #[test]
    fn pixel_atlas_regions_fit_their_own_source_and_preserve_aspect() {
        for (rect, size) in ACTOR_RECTS
            .into_iter()
            .map(|r| (r, ACTOR_SIZE))
            .chain(PROP_RECTS.into_iter().map(|r| (r, PROP_SIZE)))
        {
            assert!(rect[0] >= 0. && rect[1] >= 0. && rect[2] <= size[0] && rect[3] <= size[1]);
            assert!(rect[2] > rect[0] && rect[3] > rect[1]);
            let uv = region_for(rect, size);
            assert!(uv.min.x >= 0. && uv.max.x <= 1. && uv.min.y >= 0. && uv.max.y <= 1.);
            let drawn = fit(Pos2::ZERO, vec2(70., 70.), rect);
            assert!(drawn.width() <= 70.001 && drawn.height() <= 70.001);
            assert!(
                (drawn.aspect_ratio() - (rect[2] - rect[0]) / (rect[3] - rect[1])).abs() < 0.001
            );
        }
    }
    const OCCUPIED_WALKS: [[[f32; 4]; 4]; 5] = [
        [
            [78., 22., 246., 271.],
            [361., 22., 495., 268.],
            [613., 22., 778., 268.],
            [901., 22., 1037., 270.],
        ],
        [
            [73., 306., 253., 539.],
            [362., 306., 507., 537.],
            [612., 306., 791., 539.],
            [896., 305., 1037., 540.],
        ],
        [
            [42., 571., 267., 833.],
            [335., 566., 527., 828.],
            [579., 573., 810., 831.],
            [875., 573., 1070., 830.],
        ],
        [
            [72., 856., 247., 1111.],
            [349., 855., 500., 1110.],
            [609., 859., 787., 1112.],
            [891., 855., 1046., 1111.],
        ],
        [
            [68., 1134., 245., 1387.],
            [348., 1139., 516., 1387.],
            [605., 1139., 785., 1387.],
            [881., 1140., 1055., 1387.],
        ],
    ];
    #[test]
    fn pixel_gaits_keep_common_scale_hips_and_foot_baselines() {
        for actor in [
            Actor::Shambler,
            Actor::Runner,
            Actor::Brute,
            Actor::Armored,
            Actor::Demolisher,
        ] {
            let spec = walk_spec(actor).unwrap();
            let mut previous = 0.;
            let mut pixel_scale: Option<f32> = None;
            for frame in 0..4 {
                let source = spec.frames[frame as usize];
                let (rect, uv, foot) = walk_frame_layout(
                    actor,
                    [spec.source_size[0] as u32, spec.source_size[1] as u32],
                    frame,
                    pos2(100., 100.),
                    vec2(72., 60.),
                )
                .unwrap();
                assert!(uv.min.x >= previous && uv.max.x <= 1. && uv.min.y > 0. && uv.max.y < 1.);
                previous = uv.max.x;
                let occupied = OCCUPIED_WALKS[actor as usize][frame as usize];
                assert!(
                    uv.min.x * spec.source_size[0] < occupied[0]
                        && uv.min.y * spec.source_size[1] < occupied[1]
                );
                assert!(
                    uv.max.x * spec.source_size[0] > occupied[2]
                        && uv.max.y * spec.source_size[1] > occupied[3],
                    "Every opaque boot, hand and tool retains a transparent gutter"
                );
                assert!(
                    spec.feet_y - occupied[3] <= 8.,
                    "A raised passing foot retains the common floor without rescaling"
                );
                let scale = rect.height() / (source.bounds[3] - source.bounds[1]);
                if let Some(previous) = pixel_scale {
                    assert!((scale - previous).abs() < 0.0001);
                }
                pixel_scale = Some(scale);
                let reconstructed = rect.min
                    + vec2(
                        (source.hip_x - source.bounds[0]) * scale,
                        (spec.feet_y - source.bounds[1]) * scale,
                    );
                assert!(reconstructed.distance(foot) < 0.001);
                assert!(foot.distance(pos2(100., 130.)) < 0.001);
                assert!(source.bounds[0] < source.hip_x && source.hip_x < source.bounds[2]);
                assert!(source.bounds[1] < spec.feet_y && spec.feet_y <= source.bounds[3]);
            }
        }
    }
}
