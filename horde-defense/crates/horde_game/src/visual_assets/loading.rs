//! Art loading and the derived state rebuilt from it: egui uploads, hit-flash
//! masks, the weapon-icon atlas and sheet validation. Everything is rebuilt
//! from `AssetEvent<Image>` (`LoadedWithDependencies`/`Modified`), so a
//! replaced sheet (with the `dev` feature's file watcher) hot-reloads, and a
//! rejected sheet recovers once a valid one replaces it.
use super::{
    ArtTextures, FlashMasks, RigAtlas, RigSheet, WalkSheet, enemy_art_problem, prop_art_problem,
    survivor_art_problem,
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
const PATHS: [&str; 7] = [
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
    flash_masks: Option<Arc<FlashMasks>>,
    rig_atlas: Option<RigAtlas>,
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
            flash_masks: None,
            rig_atlas: None,
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
            self.flash_masks = walkers
                .zip(actors)
                .map(|(walkers, actors)| Arc::new(FlashMasks::from_images(walkers, actors)));
            let masks = self.flash_masks.clone();
            context.data_mut(|data| data.insert_temp(flash_masks_id(), masks));
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
        }
    }
}

fn flash_masks_id() -> egui::Id {
    egui::Id::new("lastlight.art.flash_masks.v1")
}
/// The current hit-flash masks. Like the art textures they describe, they are
/// published into the egui context (keeping `ArtTextures` a plain `Copy`
/// value), and replaced or removed whenever their sheets reload.
pub(super) fn flash_masks(context: &egui::Context) -> Option<Arc<FlashMasks>> {
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
