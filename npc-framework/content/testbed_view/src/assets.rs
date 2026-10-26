//! Asset paths and handles (`assets/testbed/`, measured in
//! `docs/_notes/testbed-assets-measured.md`). Everything is loaded once at
//! `PreStartup`; systems tolerate assets that are missing or still loading.

use bevy::{
    asset::AssetPlugin,
    gltf::Gltf,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    world_serialization::WorldAsset,
};

/// Absolute path of the workspace `assets/` folder, fixed at compile time so
/// `cargo run -p actor_testbed` and the built executable find the assets
/// regardless of the cwd.
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// `AssetPlugin` pointing at the workspace assets (`BEVY_ASSET_ROOT`, when
/// set, wins: `<BEVY_ASSET_ROOT>/assets`).
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

/// The four actor models (one animation graph each; they share the KayKit
/// rig and clip names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModelKind {
    Knight,
    Rogue,
    SkeletonMinion,
    SkeletonWarrior,
}

impl ModelKind {
    pub const ALL: [ModelKind; 4] = [
        ModelKind::Knight,
        ModelKind::Rogue,
        ModelKind::SkeletonMinion,
        ModelKind::SkeletonWarrior,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn path(self) -> &'static str {
        match self {
            ModelKind::Knight => "testbed/characters/Knight.glb",
            ModelKind::Rogue => "characters/Rogue_Hooded.glb",
            ModelKind::SkeletonMinion => "testbed/characters/Skeleton_Minion.glb",
            ModelKind::SkeletonWarrior => "testbed/characters/Skeleton_Warrior.glb",
        }
    }

    /// Faction 1 = adventurers (Knight for the smith and the woodcutter,
    /// Rogue for everyone else), faction 2 = skeletons (Warrior for
    /// `*warrior*` templates).
    pub fn for_actor(template: &str, owner: Option<u32>) -> Self {
        if owner == Some(2) || template.starts_with("skeleton") {
            if template.contains("warrior") {
                ModelKind::SkeletonWarrior
            } else {
                ModelKind::SkeletonMinion
            }
        } else {
            match template {
                "smith" | "woodcutter" => ModelKind::Knight,
                _ => ModelKind::Rogue,
            }
        }
    }
}

macro_rules! models {
    ($($field:ident = $path:literal),* $(,)?) => {
        /// Handles to every static model the view uses.
        #[derive(Clone)]
        pub(crate) struct Models {
            $(pub $field: Handle<WorldAsset>,)*
        }

        impl Models {
            fn load(server: &AssetServer) -> Self {
                Self { $($field: scene(server, $path),)* }
            }
        }
    };
}

models! {
    wall = "testbed/dungeon/wall.gltf.glb",
    doorway = "testbed/dungeon/wall_doorway.glb",
    floor = "testbed/dungeon/floor_tile_large.gltf.glb",
    chest = "testbed/dungeon/chest.glb",
    key = "testbed/dungeon/key.gltf.glb",
    food = "testbed/dungeon/plate_food_A.gltf.glb",
    ingot = "testbed/dungeon/coin_stack_small.gltf.glb",
    barrel = "testbed/dungeon/barrel_small.gltf.glb",
    crate_large = "testbed/dungeon/box_large.gltf.glb",
    tree_a = "hex/tree_single_A.gltf",
    tree_b = "hex/tree_single_B.gltf",
    stump_a = "hex/tree_single_A_cut.gltf",
    stump_b = "hex/tree_single_B_cut.gltf",
    stone = "hex/resource_stone.gltf",
    lumber = "hex/resource_lumber.gltf",
    rock = "hex/rock_single_A.gltf",
    book = "furniture/book_single.gltf",
    axe = "props/axe_1handed.gltf",
    workbench = "testbed/survival/workbench.glb",
    anvil = "testbed/survival/workbench-anvil.glb",
    forge = "testbed/survival/campfire-pit.glb",
    pickaxe = "testbed/survival/tool-pickaxe.glb",
    hammer = "testbed/survival/tool-hammer.glb",
    log = "testbed/survival/tree-log.glb",
    planks = "testbed/survival/resource-planks.glb",
    scroll = "testbed/survival/bedroll-packed.glb",
    ram = "testbed/castle/siege-ram.glb",
    cart = "testbed/town/cart.glb",
}

pub(crate) const GRASS: &str = "textures/grass_color.jpg";

/// Every handle the view uses.
#[derive(Resource, Clone)]
pub(crate) struct ViewAssets {
    /// Per [`ModelKind`]: the glTF (for its clips) and its scene.
    pub actor_gltf: [Handle<Gltf>; 4],
    pub actor: [Handle<WorldAsset>; 4],
    pub models: Models,
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
        warn!("testbed_view: no AssetServer, nothing will be drawn");
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
        actor_gltf: ModelKind::ALL.map(|k| server.load(k.path())),
        actor: ModelKind::ALL.map(|k| scene(&server, k.path())),
        models: Models::load(&server),
        grass,
    });
}
