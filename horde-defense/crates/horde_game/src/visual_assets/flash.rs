//! Hit-flash geometry: solid-pixel masks derived from the source alpha, their
//! white silhouettes in the packed battle atlas, and the painters that light
//! them.
use super::*;

/// Geometry derived from the unchanged source alpha, never a new asset file.
/// Rebuilt by `loading` whenever the walker or actor sheet (re)loads and kept
/// beside the egui textures it describes (see `loading::flash_masks`).
#[derive(Debug)]
pub struct FlashMasks {
    /// Per enemy sprite (catalogue order): one mask per gait frame or standing pose.
    pub(super) enemies: Vec<Vec<FlashMask>>,
    /// Player and companion standing poses.
    pub(super) survivors: Vec<FlashMask>,
    /// The packed battle atlas the enemy silhouettes are placed in (W1-66):
    /// the sheets' own layout, grown only when the silhouettes need more rows.
    /// Its texture is the default id; the batched battlefield samples the
    /// silhouettes only from an atlas with this layout.
    pub(super) atlas: BattleAtlas,
}
#[derive(Debug)]
pub(super) struct FlashMask {
    pub(super) runs: Vec<[f32; 4]>,
    /// The source texels the runs were cut from, clipped to the image.
    pub(super) bounds: [u32; 4],
    /// Top-left atlas texel of this mask's white silhouette: one texel per
    /// 2x2 mask block, opaque white where a run lights the block. `None` when
    /// the atlas does not pack it (survivor poses, an empty mask, or a
    /// silhouette wider than the free column).
    pub(super) silhouette: Option<[u32; 2]>,
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
                (value.max(0.0) as u32).min(limit)
            } else {
                0
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
        for y in (bounds[1]..bounds[3]).step_by(2) {
            let bottom = (y + 2).min(bounds[3]);
            let mut start = None;
            for x in (bounds[0]..bounds[2]).step_by(2) {
                let right = (x + 2).min(bounds[2]);
                let solid = (y..bottom).all(|row| {
                    (x..right)
                        .all(|column| pixels[((row * width + column) * 4 + 3) as usize] >= 128)
                });
                if solid {
                    start.get_or_insert(x);
                }
                if (!solid || right == bounds[2])
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
        Self {
            runs,
            bounds: [
                bounds[0],
                bounds[1],
                bounds[2].max(bounds[0]),
                bounds[3].max(bounds[1]),
            ],
            silhouette: None,
        }
    }
    /// Silhouette size in atlas texels: one per (possibly clipped) 2x2 block.
    pub(super) fn silhouette_size(&self) -> [u32; 2] {
        [
            (self.bounds[2] - self.bounds[0]).div_ceil(2),
            (self.bounds[3] - self.bounds[1]).div_ceil(2),
        ]
    }
    /// The silhouette texels the runs light, relative to its top-left texel:
    /// exactly the blocks the runs cover, so the silhouette and the runs light
    /// the same source texels.
    pub(super) fn silhouette_texels(&self) -> impl Iterator<Item = [u32; 2]> + '_ {
        let [left, top, ..] = self.bounds;
        self.runs.iter().flat_map(move |run| {
            let row = (run[1] as u32 - top) / 2;
            let first = (run[0] as u32 - left) / 2;
            let end = (run[2] as u32 - left).div_ceil(2);
            (first..end).map(move |column| [column, row])
        })
    }
}
/// Shelf-packs `sizes` left to right into rows `width` texels wide, each
/// silhouette and row `BattleAtlas::GUTTER` texels apart. Returns each
/// item's offset (`None` when empty or wider than `width`) and the rows'
/// total height.
pub(super) fn shelf_pack(width: u32, sizes: &[[u32; 2]]) -> (Vec<Option<[u32; 2]>>, u32) {
    let (mut x, mut y, mut shelf) = (0, 0, 0);
    let offsets = sizes
        .iter()
        .map(|&[w, h]| {
            if w == 0 || h == 0 || w > width {
                return None;
            }
            if x > 0 && x + w > width {
                y += shelf + BattleAtlas::GUTTER;
                (x, shelf) = (0, 0);
            }
            let offset = [x, y];
            x += w + BattleAtlas::GUTTER;
            shelf = shelf.max(h);
            Some(offset)
        })
        .collect();
    (offsets, y + shelf)
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
        let mut enemies: Vec<Vec<FlashMask>> = catalog
            .sprites()
            .iter()
            .map(|sprite| match &sprite.art {
                // Rejected art keeps per-sprite slots but has no masks.
                _ if !enemy_art_fits => Vec::new(),
                SpriteArt::Walk(walk) => walk
                    .frames
                    .iter()
                    .map(|frame| FlashMask::from_rgba(walkers.width(), walk_pixels, frame.bounds))
                    .collect(),
                SpriteArt::Standing(art) => {
                    vec![FlashMask::from_rgba(
                        actors.width(),
                        actor_pixels,
                        art.bounds,
                    )]
                }
            })
            .collect();
        // The silhouettes fill the free column under the actor sheet, below
        // the white block; the atlas grows only if they need more rows.
        let sheets = |image: &Image| [image.width(), image.height()];
        let base = BattleAtlas::layout(TextureId::default(), sheets(actors), sheets(walkers));
        let origin = base.silhouette_origin();
        let sizes: Vec<[u32; 2]> = enemies
            .iter()
            .flatten()
            .map(|mask| {
                if mask.runs.is_empty() {
                    [0, 0]
                } else {
                    mask.silhouette_size()
                }
            })
            .collect();
        let (offsets, rows) = shelf_pack(base.actors_size[0], &sizes);
        for (mask, offset) in enemies.iter_mut().flatten().zip(offsets) {
            mask.silhouette = offset.map(|[x, y]| [origin[0] + x, origin[1] + y]);
        }
        Self {
            enemies,
            survivors: SURVIVOR_RECTS
                .iter()
                .map(|bounds| FlashMask::from_rgba(actors.width(), actor_pixels, *bounds))
                .collect(),
            atlas: BattleAtlas::layout_with_silhouettes(
                TextureId::default(),
                sheets(actors),
                sheets(walkers),
                rows,
            ),
        }
    }
    /// Whether `atlas` holds these masks' silhouettes: its layout is theirs.
    pub(super) fn packed_in(&self, atlas: &BattleAtlas) -> bool {
        BattleAtlas {
            texture: atlas.texture,
            ..self.atlas
        } == *atlas
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
    let Some(masks) = loading::flash_masks(p.ctx()) else {
        return;
    };
    let mut mesh = bevy_egui::egui::epaint::Mesh::default();
    hit_flash_runs(
        &masks,
        textures,
        kind,
        center,
        size,
        flash,
        pose,
        &mut mesh,
        bevy_egui::egui::epaint::WHITE_UV,
    );
    if !mesh.is_empty() {
        p.add(bevy_egui::egui::Shape::mesh(mesh));
    }
}

/// The mask of the sprite `kind` in its current pose, and where it is drawn.
pub(super) struct FlashTarget<'a> {
    pub(super) mask: &'a FlashMask,
    /// The source rectangle `rect` shows.
    pub(super) source: [f32; 4],
    pub(super) rect: Rect,
    pub(super) pivot: Pos2,
}
impl FlashTarget<'_> {
    /// The screen point of source texel coordinate `(x, y)`, before posing.
    fn point(&self, x: f32, y: f32) -> Pos2 {
        let source = self.source;
        self.rect.min
            + vec2(
                (x - source[0]) / (source[2] - source[0]) * self.rect.width(),
                (y - source[1]) / (source[3] - source[1]) * self.rect.height(),
            )
    }
}
/// The mask and layout a hit flash on `kind` uses, or `None` when its art
/// is unavailable.
pub(super) fn flash_target<'a>(
    masks: &'a FlashMasks,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    pose: &ActorPose,
) -> Option<FlashTarget<'a>> {
    let (mask, source, rect, pivot) = match kind {
        Actor::Enemy(_) if textures.enemy_art_rejected => return None,
        Actor::Enemy(sprite) => {
            let frames = masks.enemies.get(sprite.index)?;
            match &sprite.art {
                SpriteArt::Walk(walk) => {
                    let frame = pose.frame as usize % walk.frames.len();
                    let sheet = textures.walkers?;
                    let (rect, _, pivot) = walk_frame_layout(
                        walk,
                        sprite.source_size,
                        sheet.size,
                        pose.frame,
                        center,
                        size,
                    );
                    (frames.get(frame)?, walk.frames[frame].bounds, rect, pivot)
                }
                SpriteArt::Standing(art) => {
                    textures.actors?;
                    let (rect, pivot) = standing_actor_layout(
                        art.bounds,
                        [art.hip_x, art.feet_y, art.height],
                        center,
                        size,
                    );
                    (frames.first()?, art.bounds, rect, pivot)
                }
            }
        }
        Actor::Player | Actor::Mara => {
            let index = kind.survivor_index().unwrap_or_default();
            if textures.actors.is_none() || textures.survivor_art_rejected {
                return None;
            }
            let source = SURVIVOR_RECTS[index];
            let (rect, pivot) =
                standing_actor_layout(source, SURVIVOR_ANCHORS[index], center, size);
            (&masks.survivors[index], source, rect, pivot)
        }
    };
    Some(FlashTarget {
        mask,
        source,
        rect,
        pivot,
    })
}

/// The hit-flash runs of [`actor_hit_flash`], appended to `mesh` as quads
/// sampling `white_uv` (the font atlas's white texel, or the packed battle
/// atlas's white block) and posed like the sprite. Returns the quads
/// appended: none when the flash is spent or the art is unavailable.
#[allow(clippy::too_many_arguments)]
pub fn hit_flash_runs(
    masks: &FlashMasks,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    flash: HitFlash,
    pose: &ActorPose,
    mesh: &mut bevy_egui::egui::epaint::Mesh,
    white_uv: Pos2,
) -> usize {
    if flash.strength <= 0. {
        return 0;
    }
    let Some(target) = flash_target(masks, textures, kind, center, size, pose) else {
        return 0;
    };
    append_runs(&target, hit_flash_color(flash), pose, mesh, white_uv)
}
fn append_runs(
    target: &FlashTarget,
    color: Color32,
    pose: &ActorPose,
    mesh: &mut bevy_egui::egui::epaint::Mesh,
    white_uv: Pos2,
) -> usize {
    let runs = &target.mask.runs;
    let white = Rect::from_min_max(white_uv, white_uv);
    let start = mesh.vertices.len();
    mesh.reserve_vertices(runs.len() * 4);
    mesh.reserve_triangles(runs.len() * 2);
    for run in runs {
        mesh.add_rect_with_uv(
            Rect::from_min_max(target.point(run[0], run[1]), target.point(run[2], run[3])),
            white,
            color,
        );
    }
    pose_vertices(&mut mesh.vertices[start..], pose, target.pivot);
    runs.len()
}

/// The hit flash of [`hit_flash_runs`] as one posed quad over the mask's
/// clipped bounds, sampling its white silhouette in the packed `atlas`
/// (NEAREST, so it lights the pixels the runs light). Falls back to the runs
/// on the atlas's white block when `atlas` does not hold the silhouette.
/// Returns the quads appended.
#[allow(clippy::too_many_arguments)]
pub fn hit_flash_quad(
    masks: &FlashMasks,
    textures: ArtTextures,
    kind: Actor,
    center: Pos2,
    size: Vec2,
    flash: HitFlash,
    pose: &ActorPose,
    mesh: &mut bevy_egui::egui::epaint::Mesh,
    atlas: &BattleAtlas,
) -> usize {
    if flash.strength <= 0. {
        return 0;
    }
    let Some(target) = flash_target(masks, textures, kind, center, size, pose) else {
        return 0;
    };
    let color = hit_flash_color(flash);
    let Some(origin) = target.mask.silhouette.filter(|_| masks.packed_in(atlas)) else {
        return append_runs(&target, color, pose, mesh, atlas.white_uv);
    };
    let [left, top, right, bottom] = target.mask.bounds.map(|v| v as f32);
    // Source texel x maps to silhouette texel (x - left) / 2: the quad spans
    // half the bounds in texels, so a clipped 1-texel block maps to half a
    // silhouette texel, as the clipped run did.
    let texel = |x: f32, y: f32| pos2(x / atlas.size[0] as f32, y / atlas.size[1] as f32);
    let [x, y] = origin.map(|v| v as f32);
    let uv = Rect::from_min_max(
        texel(x, y),
        texel(x + (right - left) * 0.5, y + (bottom - top) * 0.5),
    );
    let start = mesh.vertices.len();
    mesh.add_rect_with_uv(
        Rect::from_min_max(target.point(left, top), target.point(right, bottom)),
        uv,
        color,
    );
    pose_vertices(&mut mesh.vertices[start..], pose, target.pivot);
    1
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
