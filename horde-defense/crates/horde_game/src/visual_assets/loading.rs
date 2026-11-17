//! Art loading and the derived state rebuilt from it: egui uploads, hit-flash
//! masks, the weapon-icon atlas and sheet validation. Everything is rebuilt
//! from `AssetEvent<Image>` (`LoadedWithDependencies`/`Modified`), so a
//! replaced sheet (with the `dev` feature's file watcher) hot-reloads, and a
//! rejected sheet recovers once a valid one replaces it.
use super::{
    ArtTextures, BattleAtlas, FlashMasks, RigAtlas, RigSheet, WalkSheet, enemy_art_problem,
    prop_art_problem, soft_shadow_image, survivor_art_problem,
};
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::sync::Arc;

const ACTORS: usize = 0;
const PROPS: usize = 1;
const KEYART: usize = 2;
const GROUND: usize = 3;
const WALKERS: usize = 4;
/// Legacy `defender-rig-v1.png`: now only the firearm/ordnance icon source.
const RIG: usize = 5;
const MENU_IDLE: usize = 6;
/// The egui art slots, relative to `assets/`; the asset audit's runtime set.
pub(crate) const PATHS: [&str; 7] = [
    "art/pixel/actors-v1.png",
    "art/pixel/props-v1.png",
    "art/pixel/keyart-v2.png",
    "art/pixel/terrain-v1.png",
    "art/pixel/walkers-v1.png",
    "art/pixel/defender-rig-v1.png",
    "art/pixel/menu-survivors-v1.png",
];

/// One sheet's egui upload. `texture` is `None` until the loaded image has
/// been processed; `stale` marks a reload that has not been processed yet.
#[derive(Default)]
struct ArtSlot {
    texture: Option<Result<egui::TextureHandle, String>>,
    size: [u32; 2],
    stale: bool,
}
impl ArtSlot {
    fn id(&self) -> Option<egui::TextureId> {
        self.texture
            .as_ref()?
            .as_ref()
            .ok()
            .map(egui::TextureHandle::id)
    }
}

/// Logs a rejected sheet once per distinct problem; a passing check clears it,
/// so a later bad reload logs again.
#[derive(Default)]
struct ArtCheck {
    reported: Option<String>,
}
impl ArtCheck {
    fn rejected(&mut self, what: &str, problem: Option<String>) -> bool {
        let Some(problem) = problem else {
            self.reported = None;
            return false;
        };
        if self.reported.as_ref() != Some(&problem) {
            error!("{what} disabled: {problem}");
            self.reported = Some(problem);
        }
        true
    }
}

/// Source art handles and everything derived from their loaded pixels.
#[derive(Resource)]
pub struct VisualAssets {
    handles: [Handle<Image>; 7],
    slots: [ArtSlot; 7],
    rig_atlas: Option<RigAtlas>,
    /// The actor and walker sheets packed for the batched battlefield,
    /// rebuilt beside the flash masks whenever either sheet reloads.
    battle_atlas: Option<(egui::TextureHandle, BattleAtlas)>,
    atlas_check: ArtCheck,
    /// The procedural soft shadow, uploaded once.
    shadow: Option<egui::TextureHandle>,
    enemy_check: ArtCheck,
    survivor_check: ArtCheck,
    prop_check: ArtCheck,
    enemy_art_rejected: bool,
    survivor_art_rejected: bool,
    prop_art_rejected: bool,
    ready: bool,
}
impl VisualAssets {
    fn new(handles: [Handle<Image>; 7]) -> Self {
        Self {
            handles,
            slots: Default::default(),
            rig_atlas: None,
            battle_atlas: None,
            atlas_check: ArtCheck::default(),
            shadow: None,
            enemy_check: ArtCheck::default(),
            survivor_check: ArtCheck::default(),
            prop_check: ArtCheck::default(),
            enemy_art_rejected: false,
            survivor_art_rejected: false,
            prop_art_rejected: false,
            ready: false,
        }
    }
    /// Every sheet has loaded and its current pixels have been processed
    /// (uploaded, or rejected with a logged reason).
    #[cfg(feature = "review")]
    pub fn ready(&self) -> bool {
        self.ready
    }
    fn mark_stale(&mut self, id: AssetId<Image>) {
        for (handle, slot) in self.handles.iter().zip(&mut self.slots) {
            if handle.id() == id {
                slot.stale = true;
            }
        }
    }
    /// Uploads every newly loaded or reloaded sheet, then rebuilds whatever
    /// was derived from the sheets that changed.
    fn refresh(&mut self, context: &egui::Context, images: &Assets<Image>, max_side: usize) {
        let mut changed = [false; 7];
        for (index, (handle, slot)) in self.handles.iter().zip(&mut self.slots).enumerate() {
            let Some(image) = images.get(handle) else {
                continue;
            };
            if slot.texture.is_some() && !slot.stale {
                continue;
            }
            let uploaded =
                managed_texture(context, format!("lastlight-art-{index}"), image, max_side);
            if let Err(error) = &uploaded {
                warn!("{error}");
            }
            *slot = ArtSlot {
                texture: Some(uploaded),
                size: [image.width(), image.height()],
                stale: false,
            };
            changed[index] = true;
        }
        let rgba = |index: usize| {
            images
                .get(&self.handles[index])
                .filter(|image| rgba8(image).is_ok())
        };
        if changed[ACTORS] || changed[WALKERS] {
            let (walkers, actors) = (rgba(WALKERS), rgba(ACTORS));
            self.enemy_art_rejected = self
                .enemy_check
                .rejected("Enemy sprite art", enemy_art_problem(walkers, actors));
            let masks = walkers
                .zip(actors)
                .map(|(walkers, actors)| Arc::new(FlashMasks::from_images(walkers, actors)));
            publish_flash_masks(context, masks.clone());
            // The atlas packs the masks' silhouettes, so it is rebuilt with them.
            self.battle_atlas = match actors.zip(walkers).zip(masks) {
                Some(((actors, walkers), masks)) => {
                    match pack_battle_atlas(context, actors, walkers, &masks, max_side) {
                        Ok(packed) => {
                            self.atlas_check.rejected("Packed battle atlas", None);
                            Some(packed)
                        }
                        Err(problem) => {
                            self.atlas_check
                                .rejected("Packed battle atlas", Some(problem));
                            None
                        }
                    }
                }
                None => None,
            };
        }
        if self.shadow.is_none() {
            self.shadow = Some(context.load_texture(
                "lastlight-soft-shadow",
                soft_shadow_image(),
                egui::TextureOptions::LINEAR,
            ));
        }
        if changed[ACTORS] {
            let problem = rgba(ACTORS).and_then(survivor_art_problem);
            self.survivor_art_rejected = self.survivor_check.rejected("Survivor art", problem);
        }
        if changed[PROPS] {
            let problem = rgba(PROPS).and_then(prop_art_problem);
            self.prop_art_rejected = self.prop_check.rejected("Prop art", problem);
        }
        if changed[RIG] {
            self.rig_atlas = rgba(RIG).map(RigAtlas::from_image);
        }
        self.ready = self
            .slots
            .iter()
            .all(|slot| slot.texture.is_some() && !slot.stale);
    }
    fn textures(&self) -> ArtTextures {
        let id = |index: usize| self.slots[index].id();
        ArtTextures {
            actors: id(ACTORS),
            props: id(PROPS).filter(|_| !self.prop_art_rejected),
            keyart: id(KEYART),
            ground: id(GROUND),
            walkers: id(WALKERS).map(|texture| WalkSheet {
                texture,
                size: self.slots[WALKERS].size,
            }),
            enemy_art_rejected: self.enemy_art_rejected,
            survivor_art_rejected: self.survivor_art_rejected,
            rig: id(RIG)
                .zip(self.rig_atlas)
                .map(|(texture, atlas)| RigSheet { texture, atlas }),
            menu_idle: id(MENU_IDLE),
            battle_atlas: self.battle_atlas.as_ref().map(|(_, atlas)| *atlas),
            shadow: self.shadow.as_ref().map(egui::TextureHandle::id),
        }
    }
}

fn flash_masks_id() -> egui::Id {
    egui::Id::new("lastlight.art.flash_masks.v1")
}
/// Replaces (or, with `None`, removes) the hit-flash masks. They live only in
/// the egui context, like the art textures they describe, which keeps
/// `ArtTextures` a plain `Copy` value and leaves one source of truth.
pub(super) fn publish_flash_masks(context: &egui::Context, masks: Option<Arc<FlashMasks>>) {
    context.data_mut(|data| data.insert_temp(flash_masks_id(), masks));
}
/// Publishes the masks of the shipped walker and actor sheets, for painter
/// tests that run without the asset pipeline.
#[cfg(test)]
pub(crate) fn publish_source_flash_masks(context: &egui::Context) {
    let decode = |path: &str| {
        let bytes = std::fs::read(format!(
            "{}/../../assets/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Image::from_buffer(
            &bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap()
    };
    let masks = FlashMasks::from_images(&decode(PATHS[WALKERS]), &decode(PATHS[ACTORS]));
    publish_flash_masks(context, Some(Arc::new(masks)));
}
/// The current hit-flash masks, as last published by `refresh`.
pub fn flash_masks(context: &egui::Context) -> Option<Arc<FlashMasks>> {
    context
        .data(|data| data.get_temp::<Option<Arc<FlashMasks>>>(flash_masks_id()))
        .flatten()
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
    commands.insert_resource(VisualAssets::new(PATHS.map(pixel)));
}

/// Registers this frame's egui textures into their own resource, so drawing
/// never needs mutable access to the game. Uploads and derived data change
/// only when an image event names one of the sheets (or it first appears).
pub fn refresh_art(
    mut contexts: EguiContexts,
    mut visuals: ResMut<VisualAssets>,
    images: Res<Assets<Image>>,
    mut events: MessageReader<AssetEvent<Image>>,
    render_device: Res<bevy::render::renderer::RenderDevice>,
    mut textures: ResMut<ArtTextures>,
) {
    for event in events.read() {
        if let AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } = event {
            visuals.mark_stale(*id);
        }
    }
    let Ok(context) = contexts.ctx_mut() else {
        *textures = ArtTextures::default();
        return;
    };
    let context = context.clone();
    let max_side = render_device.limits().max_texture_dimension_2d as usize;
    visuals.refresh(&context, &images, max_side);
    *textures = visuals.textures();
}

/// The untouched straight RGBA8 pixels every derived structure reads. Other
/// formats (for example a 16-bit replacement PNG) are rejected, not indexed.
fn rgba8(image: &Image) -> Result<&[u8], String> {
    let pixels = image
        .data
        .as_deref()
        .ok_or("Original RGBA pixels are not retained in the main world")?;
    if pixels.len() as u64 != u64::from(image.width()) * u64::from(image.height()) * 4 {
        return Err(format!(
            "{}x{} art is not 8-bit RGBA",
            image.width(),
            image.height()
        ));
    }
    Ok(pixels)
}

/// bevy_egui 0.43.1 uses premultiplied blending and its fragment shader does
/// not premultiply external textures. Convert straight PNG RGBA through egui's
/// supported managed-image path once, preserving nearest sampling and originals.
pub(super) fn image_for_egui(image: &Image) -> Result<egui::ColorImage, String> {
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        rgba8(image)?,
    ))
}

/// Packs the actor and walker sheets, an opaque white block and the enemy
/// hit-flash silhouettes of `masks` (derived from those sheets) into one
/// NEAREST texture with the masks' layout (see [`BattleAtlas::layout`]).
/// Errs, uploading nothing, when the packed size exceeds the device limit:
/// the battlefield then draws the two sheets as two meshes.
fn pack_battle_atlas(
    context: &egui::Context,
    actors: &Image,
    walkers: &Image,
    masks: &FlashMasks,
    max_side: usize,
) -> Result<(egui::TextureHandle, BattleAtlas), String> {
    let layout = masks.atlas;
    if layout.size.iter().any(|side| *side as usize > max_side) {
        return Err(format!(
            "{}x{} exceeds the active render device limit {max_side}; drawing walkers and actors as two meshes",
            layout.size[0], layout.size[1]
        ));
    }
    let image = battle_atlas_image(
        &image_for_egui(actors)?,
        &image_for_egui(walkers)?,
        &layout,
        masks,
    );
    context.input_mut(|input| {
        input.max_texture_side = max_side;
        input.raw.max_texture_side = Some(max_side);
    });
    let handle = context.load_texture(
        "lastlight-battle-atlas",
        image,
        egui::TextureOptions::NEAREST,
    );
    let atlas = BattleAtlas {
        texture: handle.id(),
        ..layout
    };
    Ok((handle, atlas))
}

/// The packed pixels of `layout`: both sheets copied unchanged, transparent
/// gutters, the opaque white block and, when `layout` is the masks' own, each
/// enemy mask's white silhouette.
pub(super) fn battle_atlas_image(
    actors: &egui::ColorImage,
    walkers: &egui::ColorImage,
    layout: &BattleAtlas,
    masks: &FlashMasks,
) -> egui::ColorImage {
    let width = layout.size[0] as usize;
    let mut image =
        egui::ColorImage::filled([width, layout.size[1] as usize], egui::Color32::TRANSPARENT);
    for (sheet, origin) in [
        (actors, layout.actors_origin),
        (walkers, layout.walkers_origin),
    ] {
        for (row, pixels) in sheet.pixels.chunks_exact(sheet.size[0]).enumerate() {
            let start = (origin[1] as usize + row) * width + origin[0] as usize;
            image.pixels[start..start + pixels.len()].copy_from_slice(pixels);
        }
    }
    let [left, top] = layout.white_origin().map(|v| v as usize);
    let side = BattleAtlas::WHITE_SIDE as usize;
    for row in top..top + side {
        image.pixels[row * width + left..row * width + left + side].fill(egui::Color32::WHITE);
    }
    if masks.packed_in(layout) {
        for mask in masks.enemies.iter().flatten() {
            let Some([x, y]) = mask.silhouette else {
                continue;
            };
            for [column, row] in mask.silhouette_texels() {
                let index = (y + row) as usize * width + (x + column) as usize;
                image.pixels[index] = egui::Color32::WHITE;
            }
        }
    }
    image
}

pub(super) fn managed_texture(
    context: &egui::Context,
    name: String,
    image: &Image,
    max_side: usize,
) -> Result<egui::TextureHandle, String> {
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
    let image = image_for_egui(image).map_err(|error| format!("Art texture {name}: {error}"))?;
    Ok(context.load_texture(name, image, egui::TextureOptions::NEAREST))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    fn sheet(width: u32, height: u32) -> Image {
        Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[255, 255, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
    }
    fn decode(path: &str) -> Image {
        let bytes = std::fs::read(format!(
            "{}/../../assets/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Image::from_buffer(
            &bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap()
    }

    /// Loads every sheet into `Assets<Image>` (`small` replaces the actor and
    /// walker sheets) and returns the resource with handles into it.
    fn loaded(images: &mut Assets<Image>, small: bool) -> VisualAssets {
        VisualAssets::new(PATHS.map(|path| {
            let replaced = small && (path.contains("actors") || path.contains("walkers"));
            images.add(if replaced {
                sheet(64, 64)
            } else {
                decode(path)
            })
        }))
    }

    #[test]
    fn rejected_sheets_recover_on_a_valid_reload_and_rebuild_derived_state() {
        let context = egui::Context::default();
        let mut images = Assets::<Image>::default();
        let mut visuals = loaded(&mut images, true);
        assert!(!visuals.ready);
        visuals.refresh(&context, &images, 8192);
        assert!(visuals.ready);
        let first = visuals.textures();
        assert!(first.enemy_art_rejected && first.survivor_art_rejected);
        assert!(
            flash_masks(&context)
                .unwrap()
                .enemies
                .iter()
                .all(Vec::is_empty)
        );
        let rejected_actor_texture = first.actors.unwrap();

        // Nothing is re-uploaded or re-derived without an image event.
        visuals.refresh(&context, &images, 8192);
        assert_eq!(visuals.textures().actors, Some(rejected_actor_texture));

        // A valid replacement (as the file watcher reports it) clears the rejection,
        // rebuilds masks and uploads a new texture.
        for (index, path) in [(ACTORS, PATHS[ACTORS]), (WALKERS, PATHS[WALKERS])] {
            images
                .insert(visuals.handles[index].id(), decode(path))
                .unwrap();
            visuals.mark_stale(visuals.handles[index].id());
        }
        assert!(visuals.slots[ACTORS].stale && visuals.slots[WALKERS].stale);
        visuals.refresh(&context, &images, 8192);
        let reloaded = visuals.textures();
        assert!(!reloaded.enemy_art_rejected && !reloaded.survivor_art_rejected);
        assert!(
            flash_masks(&context)
                .unwrap()
                .enemies
                .iter()
                .all(|masks| !masks.is_empty())
        );
        assert_ne!(reloaded.actors, Some(rejected_actor_texture));
        assert!(visuals.enemy_check.reported.is_none());

        // A later bad reload is detected and logged again.
        images
            .insert(visuals.handles[ACTORS].id(), sheet(64, 64))
            .unwrap();
        visuals.mark_stale(visuals.handles[ACTORS].id());
        visuals.refresh(&context, &images, 8192);
        assert!(visuals.textures().survivor_art_rejected);
        assert!(visuals.survivor_check.reported.is_some());
    }

    #[test]
    fn packed_atlas_rebuilds_with_the_sheets_and_respects_the_device_limit() {
        let context = egui::Context::default();
        let mut images = Assets::<Image>::default();
        let mut visuals = loaded(&mut images, false);
        visuals.refresh(&context, &images, 4096);
        let textures = visuals.textures();
        let atlas = textures.battle_atlas.expect("the packed atlas fits 4096");
        assert_eq!(atlas.size, [2898, 1402]);
        assert_eq!(atlas.actors_origin, [0, 0]);
        assert_eq!(atlas.walkers_origin, [1776, 0]);
        assert_eq!(atlas.white_origin(), [0, 889]);
        assert!(textures.shadow.is_some());
        assert!(visuals.atlas_check.reported.is_none());
        // The packed pixels are the sheets' own, plus an opaque white block.
        let masks = FlashMasks::from_images(&decode(PATHS[WALKERS]), &decode(PATHS[ACTORS]));
        assert!(masks.packed_in(&atlas), "the atlas has the masks' layout");
        let actors = image_for_egui(&decode(PATHS[ACTORS])).unwrap();
        let walkers = image_for_egui(&decode(PATHS[WALKERS])).unwrap();
        let packed = battle_atlas_image(&actors, &walkers, &atlas, &masks);
        assert_eq!(packed.size, [2898, 1402]);
        let at = |x: u32, y: u32| packed.pixels[(y * 2898 + x) as usize];
        for (x, y) in [(0, 0), (900, 400), (1773, 886)] {
            assert_eq!(at(x, y), actors.pixels[(y * 1774 + x) as usize]);
        }
        for (x, y) in [(0, 0), (600, 700), (1121, 1401)] {
            assert_eq!(at(1776 + x, y), walkers.pixels[(y * 1122 + x) as usize]);
        }
        assert_eq!(at(1, 890), egui::Color32::WHITE);
        assert_eq!(at(1775, 10), egui::Color32::TRANSPARENT, "gutter");
        assert_eq!(
            at(10, 893),
            egui::Color32::TRANSPARENT,
            "below the white block"
        );

        // Nothing is repacked without an image event.
        visuals.refresh(&context, &images, 4096);
        assert_eq!(visuals.textures().battle_atlas, Some(atlas));
        // A reload repacks into a new texture.
        visuals.mark_stale(visuals.handles[WALKERS].id());
        visuals.refresh(&context, &images, 4096);
        let repacked = visuals.textures().battle_atlas.unwrap();
        assert_ne!(repacked.texture, atlas.texture);

        // Both sheets fit 2048 on their own, the packed atlas does not: the
        // sheets stay drawable, the atlas is dropped and the problem logged.
        let mut limited = loaded(&mut images, false);
        limited.refresh(&context, &images, 2048);
        let textures = limited.textures();
        assert!(textures.actors.is_some() && textures.walkers.is_some());
        assert!(textures.battle_atlas.is_none());
        assert!(
            limited
                .atlas_check
                .reported
                .as_ref()
                .is_some_and(
                    |problem| problem.contains("2898x1402") && problem.contains("two meshes")
                )
        );
    }

    /// W1-66: each enemy mask's silhouette in the packed atlas lights exactly
    /// the source texels its runs light, one atlas texel per 2x2 mask block,
    /// in the free column under the actor sheet with transparent gutters; and
    /// the one quad that samples it covers the screen pixels the runs cover.
    #[test]
    fn flash_silhouette_matches_mask_runs() {
        use super::super::{
            Actor, ActorPose, HitFlash, SpriteArt, WalkSheet, enemy_visuals, flash::flash_target,
            hit_flash_quad, hit_flash_runs, pose_vertices,
        };
        let (actors, walkers) = (decode(PATHS[ACTORS]), decode(PATHS[WALKERS]));
        let masks = FlashMasks::from_images(&walkers, &actors);
        let layout = masks.atlas;
        assert_eq!(
            layout,
            BattleAtlas::layout(egui::TextureId::default(), [1774, 887], [1122, 1402]),
            "the shipped silhouettes fit the existing atlas"
        );
        let packed = battle_atlas_image(
            &image_for_egui(&actors).unwrap(),
            &image_for_egui(&walkers).unwrap(),
            &layout,
            &masks,
        );
        let width = layout.size[0];
        let at = |x: u32, y: u32| packed.pixels[(y * width + x) as usize];
        let mut regions: Vec<[u32; 4]> = Vec::new();
        for mask in masks.enemies.iter().flatten() {
            let origin = mask.silhouette.expect("every enemy mask is packed");
            let [w, h] = mask.silhouette_size();
            let [left, top, right, bottom] = mask.bounds;
            let mut lit = vec![false; ((right - left) * (bottom - top)) as usize];
            for run in &mask.runs {
                for y in run[1] as u32..run[3] as u32 {
                    for x in run[0] as u32..run[2] as u32 {
                        lit[((y - top) * (right - left) + x - left) as usize] = true;
                    }
                }
            }
            assert!(lit.iter().any(|lit| *lit), "{origin:?} lights something");
            for y in top..bottom {
                for x in left..right {
                    let texel = at(origin[0] + (x - left) / 2, origin[1] + (y - top) / 2);
                    let expected = if lit[((y - top) * (right - left) + x - left) as usize] {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    assert_eq!(texel, expected, "source texel ({x}, {y})");
                }
            }
            let region = [origin[0], origin[1], origin[0] + w, origin[1] + h];
            assert!(
                region[1] >= layout.silhouette_origin()[1]
                    && region[2] <= layout.actors_size[0]
                    && region[3] <= layout.size[1],
                "{region:?} lies in the free column under the actors"
            );
            // A transparent one-texel ring keeps posed NEAREST sampling inside.
            for x in region[0].saturating_sub(1)..=region[2] {
                for y in [region[1] - 1, region[3]] {
                    assert_eq!(at(x, y), egui::Color32::TRANSPARENT, "gutter ({x}, {y})");
                }
            }
            for y in region[1]..region[3] {
                for x in [region[0].checked_sub(1), Some(region[2])]
                    .into_iter()
                    .flatten()
                {
                    assert_eq!(at(x, y), egui::Color32::TRANSPARENT, "gutter ({x}, {y})");
                }
            }
            regions.push(region);
        }
        for (n, a) in regions.iter().enumerate() {
            for b in &regions[n + 1..] {
                assert!(
                    a[2] < b[0] || b[2] < a[0] || a[3] < b[1] || b[3] < a[1],
                    "{a:?} and {b:?} overlap"
                );
            }
        }
        assert!(masks.survivors.iter().all(|mask| mask.silhouette.is_none()));

        // One quad per flash, covering the pixels the runs cover: sample the
        // quad (NEAREST, as the atlas is) at every pixel centre and compare
        // with the run rectangles, skipping centres on a run edge.
        let textures = ArtTextures {
            actors: Some(egui::TextureId::User(1)),
            walkers: Some(WalkSheet {
                texture: egui::TextureId::User(2),
                size: [walkers.width(), walkers.height()],
            }),
            battle_atlas: Some(layout),
            ..Default::default()
        };
        let flash = HitFlash {
            strength: 1.0,
            reduced: false,
        };
        let mut sampled = 0;
        for sprite in enemy_visuals::active().sprites() {
            let frames = match &sprite.art {
                SpriteArt::Walk(walk) => walk.frames.len() as u8,
                SpriteArt::Standing(_) => 1,
            };
            let actor = Actor::Enemy(sprite);
            for frame in 0..frames {
                let pose = ActorPose {
                    frame,
                    ..Default::default()
                };
                let (center, size) = (egui::pos2(400.25, 300.5), egui::vec2(140.0, 117.0));
                let mut quad = egui::Mesh::default();
                let quads = hit_flash_quad(
                    &masks, textures, actor, center, size, flash, &pose, &mut quad, &layout,
                );
                assert_eq!((quads, quad.vertices.len()), (1, 4), "{:?}", sprite.sprite);
                let mut runs = egui::Mesh::default();
                let run_quads = hit_flash_runs(
                    &masks,
                    textures,
                    actor,
                    center,
                    size,
                    flash,
                    &pose,
                    &mut runs,
                    layout.white_uv,
                );
                assert!(run_quads > 1);
                assert!(
                    quad.vertices
                        .iter()
                        .all(|v| v.color == runs.vertices[0].color)
                );
                let (min, max) = (quad.vertices[0], quad.vertices[3]);
                let bounds = egui::Rect::from_min_max(min.pos, max.pos);
                let rects: Vec<egui::Rect> = runs
                    .vertices
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|v| egui::Rect::from_min_max(v[0].pos, v[3].pos))
                    .collect();
                for py in min.pos.y.floor() as i32..max.pos.y.ceil() as i32 {
                    for px in min.pos.x.floor() as i32..max.pos.x.ceil() as i32 {
                        let point = egui::pos2(px as f32 + 0.5, py as f32 + 0.5);
                        let near_edge = rects.iter().any(|r| {
                            r.expand(1e-3).contains(point) && !r.shrink(1e-3).contains(point)
                        });
                        if !bounds.shrink(1e-3).contains(point) || near_edge {
                            continue;
                        }
                        let t = (point - min.pos) / (max.pos - min.pos);
                        let uv = min.uv + (max.uv - min.uv) * t;
                        let texel = at(
                            (uv.x * width as f32) as u32,
                            (uv.y * layout.size[1] as f32) as u32,
                        );
                        let inside = rects.iter().any(|r| r.contains(point));
                        assert_eq!(
                            texel == egui::Color32::WHITE,
                            inside,
                            "{:?} frame {frame} at {point:?}",
                            sprite.sprite
                        );
                        sampled += 1;
                    }
                }
                // A posed flash is the same quad under the sprite's pose.
                let posed = ActorPose {
                    frame,
                    lean: 0.3,
                    scale: [1.1, 0.8],
                    ..Default::default()
                };
                let mut posed_quad = egui::Mesh::default();
                hit_flash_quad(
                    &masks,
                    textures,
                    actor,
                    center,
                    size,
                    flash,
                    &posed,
                    &mut posed_quad,
                    &layout,
                );
                let pivot = flash_target(&masks, textures, actor, center, size, &posed)
                    .unwrap()
                    .pivot;
                let mut expected = quad.vertices.clone();
                pose_vertices(&mut expected, &posed, pivot);
                assert_eq!(posed_quad.vertices, expected);
            }
        }
        assert!(sampled > 10_000, "{sampled} pixel centres sampled");
    }

    #[test]
    fn readiness_waits_for_every_sheet_and_over_budget_art_falls_back() {
        let context = egui::Context::default();
        let mut images = Assets::<Image>::default();
        let mut visuals = loaded(&mut images, false);
        let keyart = images.remove(visuals.handles[KEYART].id()).unwrap();
        visuals.refresh(&context, &images, 8192);
        assert!(!visuals.ready);
        assert!(visuals.textures().keyart.is_none() && visuals.textures().actors.is_some());
        images.insert(visuals.handles[KEYART].id(), keyart).unwrap();
        visuals.refresh(&context, &images, 8192);
        assert!(visuals.ready && visuals.textures().keyart.is_some());
        let atlas = visuals.textures().rig.unwrap().atlas;
        assert_eq!(atlas, RigAtlas::from_image(&decode(PATHS[RIG])));

        // A sheet over the device limit is processed (ready) but never drawn.
        let mut limited = loaded(&mut images, false);
        limited.refresh(&context, &images, 1024);
        assert!(limited.ready);
        assert!(limited.textures().actors.is_none() && limited.textures().rig.is_none());
    }

    #[test]
    fn non_rgba8_replacements_are_rejected_without_indexing_pixels() {
        let wide = Image::new_fill(
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0; 8],
            TextureFormat::Rgba16Unorm,
            RenderAssetUsages::MAIN_WORLD,
        );
        assert!(rgba8(&wide).is_err());
        let context = egui::Context::default();
        assert!(managed_texture(&context, "wide".into(), &wide, 8192).is_err());
        assert!(rgba8(&sheet(4, 4)).is_ok());
    }
}
