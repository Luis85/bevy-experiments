//! Hit-flash geometry: solid-pixel masks derived from the source alpha and the
//! painter that lights them.
use super::*;

/// Geometry derived from the unchanged source alpha, never a new raster.
/// Rebuilt by `loading` whenever the walker or actor sheet (re)loads and kept
/// beside the egui textures it describes (see `loading::flash_masks`).
#[derive(Debug)]
pub struct FlashMasks {
    /// Per enemy sprite (catalogue order): one mask per gait frame or standing pose.
    pub(super) enemies: Vec<Vec<FlashMask>>,
    /// Player and companion standing poses.
    pub(super) survivors: Vec<FlashMask>,
}
#[derive(Debug)]
pub(super) struct FlashMask {
    pub(super) runs: Vec<[f32; 4]>,
}
impl FlashMask {
    /// Bounds are clipped to the real image, so a mismatched replacement sheet
    /// yields a smaller (or empty) mask rather than an out-of-range read.
    pub(super) fn from_rgba(width: u32, pixels: &[u8], bounds: [f32; 4]) -> Self {
        let height = if width == 0 {
            0
        } else {
            (pixels.len() / 4 / width as usize) as u32
        };
        let clip = |value: f32, limit: u32| {
            if value.is_finite() {
                (value.max(0.0) as u32).min(limit) as f32
            } else {
                0.0
            }
        };
        let bounds = [
            clip(bounds[0], width),
            clip(bounds[1], height),
            clip(bounds[2], width),
            clip(bounds[3], height),
        ];
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
    pub(super) fn from_images(walkers: &Image, actors: &Image) -> Self {
        let walk_pixels = walkers
            .data
            .as_deref()
            .expect("RGBA walker pixels remain available");
        let actor_pixels = actors
            .data
            .as_deref()
            .expect("RGBA actor pixels remain available");
        let catalog = enemy_visuals::active();
        let enemy_art_fits = enemy_art_problem(Some(walkers), Some(actors)).is_none();
        Self {
            enemies: catalog
                .sprites()
                .iter()
                .map(|sprite| match &sprite.art {
                    // Rejected art keeps per-sprite slots but has no masks.
                    _ if !enemy_art_fits => Vec::new(),
                    SpriteArt::Walk(walk) => walk
                        .frames
                        .iter()
                        .map(|frame| {
                            FlashMask::from_rgba(walkers.width(), walk_pixels, frame.bounds)
                        })
                        .collect(),
                    SpriteArt::Standing(art) => {
                        vec![FlashMask::from_rgba(
                            actors.width(),
                            actor_pixels,
                            art.bounds,
                        )]
                    }
                })
                .collect(),
            survivors: SURVIVOR_RECTS
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
    let Some(masks) = loading::flash_masks(p.ctx()) else {
        return;
    };
    let (mask, source, rect, pivot) = match kind {
        Actor::Enemy(_) if textures.enemy_art_rejected => return,
        Actor::Enemy(sprite) => {
            let Some(frames) = masks.enemies.get(sprite.index) else {
                return;
            };
            match &sprite.art {
                SpriteArt::Walk(walk) => {
                    let frame = pose.frame as usize % walk.frames.len();
                    let Some(sheet) = textures.walkers else {
                        return;
                    };
                    let (rect, _, pivot) = walk_frame_layout(
                        walk,
                        sprite.source_size,
                        sheet.size,
                        pose.frame,
                        center,
                        size,
                    );
                    let Some(mask) = frames.get(frame) else {
                        return;
                    };
                    (mask, walk.frames[frame].bounds, rect, pivot)
                }
                SpriteArt::Standing(art) => {
                    if textures.actors.is_none() {
                        return;
                    }
                    let (rect, pivot) = standing_actor_layout(
                        art.bounds,
                        [art.hip_x, art.feet_y, art.height],
                        center,
                        size,
                    );
                    let Some(mask) = frames.first() else {
                        return;
                    };
                    (mask, art.bounds, rect, pivot)
                }
            }
        }
        Actor::Player | Actor::Mara => {
            let index = kind.survivor_index().unwrap_or_default();
            if textures.actors.is_none() || textures.survivor_art_rejected {
                return;
            }
            let source = SURVIVOR_RECTS[index];
            let (rect, pivot) =
                standing_actor_layout(source, SURVIVOR_ANCHORS[index], center, size);
            (&masks.survivors[index], source, rect, pivot)
        }
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
pub(super) fn hit_flash_color(flash: HitFlash) -> Color32 {
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
