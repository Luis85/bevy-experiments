//! Asset paths and handles. Everything is loaded once at `Startup`; systems
//! that use the handles tolerate assets that are missing or still loading
//! (a `WorldAssetRoot` with an unloaded handle simply renders nothing).

use bevy::{
    asset::AssetPlugin,
    gltf::Gltf,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    world_serialization::WorldAsset,
};

/// Absolute path of the workspace `assets/` folder, fixed at compile time so
/// `cargo run -p village`, `cargo run --example view_smoke` and running the
/// built executable directly all find the assets regardless of the cwd.
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// `AssetPlugin` pointing at the workspace assets. When `BEVY_ASSET_ROOT` is
/// set at runtime it is respected instead (`<BEVY_ASSET_ROOT>/assets`).
pub fn asset_plugin() -> AssetPlugin {
    let file_path = if std::env::var_os("BEVY_ASSET_ROOT").is_some() {
        "assets".to_string()
    } else {
        ASSET_DIR.to_string()
    };
    AssetPlugin {
        file_path,
        ..default()
    }
}

pub(crate) const VILLAGER_GLB: &str = "characters/Rogue_Hooded.glb";
pub(crate) const HOUSE: &str = "hex/building_home_A_blue.gltf";
pub(crate) const FIELD: &str = "hex/building_grain.gltf";
pub(crate) const BED: &str = "furniture/bed_single_A.gltf";
pub(crate) const CRATE: &str = "hex/crate_A_big.gltf";
pub(crate) const BARREL: &str = "hex/barrel.gltf";
pub(crate) const SACK: &str = "hex/sack.gltf";
pub(crate) const TREE_A: &str = "hex/tree_single_A.gltf";
pub(crate) const TREE_B: &str = "hex/tree_single_B.gltf";
pub(crate) const TREES_MEDIUM: &str = "hex/trees_A_medium.gltf";
pub(crate) const ROCK_A: &str = "hex/rock_single_A.gltf";
pub(crate) const ROCK_C: &str = "hex/rock_single_C.gltf";
pub(crate) const FENCE: &str = "hex/fence_wood_straight.gltf";
pub(crate) const LUMBER: &str = "hex/resource_lumber.gltf";
pub(crate) const STONE: &str = "hex/resource_stone.gltf";
pub(crate) const GRASS: &str = "textures/grass_color.jpg";

/// Handles to every model the view uses.
#[derive(Resource, Clone)]
pub(crate) struct ViewAssets {
    pub villager_gltf: Handle<Gltf>,
    pub villager: Handle<WorldAsset>,
    pub house: Handle<WorldAsset>,
    pub field: Handle<WorldAsset>,
    pub bed: Handle<WorldAsset>,
    pub crate_big: Handle<WorldAsset>,
    pub barrel: Handle<WorldAsset>,
    pub sack: Handle<WorldAsset>,
    pub tree_a: Handle<WorldAsset>,
    pub tree_b: Handle<WorldAsset>,
    pub trees_medium: Handle<WorldAsset>,
    pub rock_a: Handle<WorldAsset>,
    pub rock_c: Handle<WorldAsset>,
    pub fence: Handle<WorldAsset>,
    pub lumber: Handle<WorldAsset>,
    pub stone: Handle<WorldAsset>,
    pub grass: Handle<Image>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, load_assets);
}

fn scene(server: &AssetServer, path: &'static str) -> Handle<WorldAsset> {
    server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

fn load_assets(mut commands: Commands, server: Option<Res<AssetServer>>) {
    let Some(server) = server else {
        warn!("village_view: no AssetServer, nothing will be drawn");
        return;
    };
    let grass = server
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(GRASS);
    commands.insert_resource(ViewAssets {
        villager_gltf: server.load(VILLAGER_GLB),
        villager: scene(&server, VILLAGER_GLB),
        house: scene(&server, HOUSE),
        field: scene(&server, FIELD),
        bed: scene(&server, BED),
        crate_big: scene(&server, CRATE),
        barrel: scene(&server, BARREL),
        sack: scene(&server, SACK),
        tree_a: scene(&server, TREE_A),
        tree_b: scene(&server, TREE_B),
        trees_medium: scene(&server, TREES_MEDIUM),
        rock_a: scene(&server, ROCK_A),
        rock_c: scene(&server, ROCK_C),
        fence: scene(&server, FENCE),
        lumber: scene(&server, LUMBER),
        stone: scene(&server, STONE),
        grass,
    });
}
