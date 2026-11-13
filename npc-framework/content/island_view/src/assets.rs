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
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};

/// Absolute path of the workspace `assets/` folder, fixed at compile time:
/// the developer fallback, so `cargo run -p island_game` finds the assets
/// regardless of the cwd. A copied build resolves its own (see
/// [`resolve_asset_dir`]).
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// Environment variable naming the asset folder itself; it wins over
/// everything else.
pub const ASSET_DIR_ENV: &str = "ISLAND_ASSETS";

/// Where the asset folder was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetSource {
    /// The `ISLAND_ASSETS` environment variable.
    Environment,
    /// An `assets` folder beside the executable: a packaged build.
    BesideExecutable,
    /// `<BEVY_ASSET_ROOT>/assets`, as Bevy itself resolves it.
    BevyAssetRoot,
    /// The workspace this build was compiled in.
    Workspace,
}

impl AssetSource {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Environment => "the ISLAND_ASSETS environment variable",
            Self::BesideExecutable => "the assets folder beside the executable",
            Self::BevyAssetRoot => "BEVY_ASSET_ROOT",
            Self::Workspace => "the workspace this build was compiled in",
        }
    }
}

/// The asset folder a run uses, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetDir {
    pub path: PathBuf,
    pub source: AssetSource,
    /// An `ISLAND_ASSETS` value that named no folder and was passed over.
    pub ignored: Option<PathBuf>,
}

/// Picks the asset folder at run time, first match wins:
///
/// 1. `ISLAND_ASSETS`, when it names an existing folder;
/// 2. an `assets` folder beside the executable;
/// 3. the developer fallback: `<BEVY_ASSET_ROOT>/assets` when that variable
///    is set, else the compile-time workspace folder.
///
/// The fallback is returned even when it does not exist, so a build copied
/// without its assets still reports where it looked.
pub fn resolve_asset_dir(
    island_assets: Option<&Path>,
    executable: Option<&Path>,
    bevy_asset_root: Option<&Path>,
    workspace: &Path,
) -> AssetDir {
    let ignored = island_assets
        .filter(|path| !path.is_dir())
        .map(Path::to_path_buf);
    let found = |path: PathBuf, source| AssetDir {
        path,
        source,
        ignored: ignored.clone(),
    };
    if let Some(path) = island_assets.filter(|path| path.is_dir()) {
        return found(path.to_path_buf(), AssetSource::Environment);
    }
    if let Some(beside) = executable
        .and_then(Path::parent)
        .map(|folder| folder.join("assets"))
        .filter(|path| path.is_dir())
    {
        return found(beside, AssetSource::BesideExecutable);
    }
    match bevy_asset_root {
        Some(root) => found(root.join("assets"), AssetSource::BevyAssetRoot),
        None => found(workspace.to_path_buf(), AssetSource::Workspace),
    }
}

/// This process's asset folder, resolved once from its environment.
pub fn asset_dir() -> &'static AssetDir {
    static CHOSEN: OnceLock<AssetDir> = OnceLock::new();
    CHOSEN.get_or_init(|| {
        let variable = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        resolve_asset_dir(
            variable(ASSET_DIR_ENV).as_deref(),
            std::env::current_exe().ok().as_deref(),
            variable("BEVY_ASSET_ROOT").as_deref(),
            Path::new(ASSET_DIR),
        )
    })
}

/// `AssetPlugin` reading from [`asset_dir`].
pub fn asset_plugin() -> AssetPlugin {
    let chosen = asset_dir();
    // Bevy joins a relative path onto `BEVY_ASSET_ROOT` itself; every other
    // choice is absolute, so the working directory never matters.
    let file_path = if chosen.source == AssetSource::BevyAssetRoot {
        "assets".to_string()
    } else {
        chosen.path.to_string_lossy().into_owned()
    };
    AssetPlugin {
        file_path,
        ..default()
    }
}

/// Says once which asset folder this run reads, and why.
fn log_asset_dir() {
    static LOGGED: Once = Once::new();
    LOGGED.call_once(|| {
        let chosen = asset_dir();
        if let Some(ignored) = &chosen.ignored {
            warn!(
                "{ASSET_DIR_ENV}={} is not a folder; ignored",
                ignored.display()
            );
        }
        if chosen.path.is_dir() {
            info!(
                "assets: {} (from {})",
                chosen.path.display(),
                chosen.source.describe()
            );
        } else {
            error!(
                "assets: {} (from {}) does not exist; set {ASSET_DIR_ENV} or put an assets \
                 folder beside the executable",
                chosen.path.display(),
                chosen.source.describe()
            );
        }
    });
}

/// Presentation identity. Occupations never select a frontier resident's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModelKind {
    Knight,
    Rogue,
    SkeletonMinion,
    SkeletonWarrior,
    Barbarian,
    Mage,
    UnhoodedRogue,
    VillageWoman,
    TravellerWoman,
    FarmerMan,
    TravellerMan,
    Rover,
    Sentinel,
}

impl ModelKind {
    pub const ANIMATED: [ModelKind; 11] = [
        ModelKind::Knight,
        ModelKind::Rogue,
        ModelKind::SkeletonMinion,
        ModelKind::SkeletonWarrior,
        ModelKind::Barbarian,
        ModelKind::Mage,
        ModelKind::UnhoodedRogue,
        ModelKind::VillageWoman,
        ModelKind::TravellerWoman,
        ModelKind::FarmerMan,
        ModelKind::TravellerMan,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn path(self) -> Option<&'static str> {
        Some(match self {
            ModelKind::Knight => "testbed/characters/Knight.glb",
            ModelKind::Rogue => "characters/Rogue_Hooded.glb",
            ModelKind::SkeletonMinion => "testbed/characters/Skeleton_Minion.glb",
            ModelKind::SkeletonWarrior => "testbed/characters/Skeleton_Warrior.glb",
            ModelKind::Barbarian => "living/characters/Barbarian.glb",
            ModelKind::Mage => "living/characters/Mage.glb",
            ModelKind::UnhoodedRogue => "living/characters/Rogue.glb",
            ModelKind::VillageWoman => "living/characters/VillageWoman.glb",
            ModelKind::TravellerWoman => "living/characters/TravellerWoman.glb",
            ModelKind::FarmerMan => "living/characters/FarmerMan.glb",
            ModelKind::TravellerMan => "living/characters/TravellerMan.glb",
            ModelKind::Rover | ModelKind::Sentinel => return None,
        })
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "knight" => Self::Knight,
            "rogue" => Self::Rogue,
            "skeleton_minion" => Self::SkeletonMinion,
            "skeleton_warrior" => Self::SkeletonWarrior,
            "barbarian" => Self::Barbarian,
            "mage" => Self::Mage,
            "unhooded_rogue" => Self::UnhoodedRogue,
            "village_woman" => Self::VillageWoman,
            "traveller_woman" => Self::TravellerWoman,
            "farmer_man" => Self::FarmerMan,
            "traveller_man" => Self::TravellerMan,
            "rover" => Self::Rover,
            "sentinel" => Self::Sentinel,
            _ => return None,
        })
    }

    pub fn is_robot(self) -> bool {
        matches!(self, Self::Rover | Self::Sentinel)
    }

    pub fn civilian_rig(self) -> bool {
        matches!(
            self,
            Self::VillageWoman | Self::TravellerWoman | Self::FarmerMan | Self::TravellerMan
        )
    }

    pub fn scale(self) -> f32 {
        // Measured imported bind-pose heights: Quaternius ~1.8m, KayKit ~2.3m.
        if self.civilian_rig() {
            1.0
        } else {
            crate::visuals::V
        }
    }

    pub fn idle_clip(self) -> &'static str {
        if self.civilian_rig() {
            "CharacterArmature|Idle"
        } else {
            "Idle"
        }
    }

    pub fn for_resident(appearance: u8) -> Self {
        const ROSTER: [ModelKind; 8] = [
            ModelKind::VillageWoman,
            ModelKind::FarmerMan,
            ModelKind::TravellerWoman,
            ModelKind::TravellerMan,
            ModelKind::UnhoodedRogue,
            ModelKind::Mage,
            ModelKind::Barbarian,
            ModelKind::Rogue,
        ];
        ROSTER[appearance as usize % ROSTER.len()]
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
    pub actor_gltf: Vec<Handle<Gltf>>,
    pub actor: Vec<Handle<WorldAsset>>,
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
    log_asset_dir();
    let Some(server) = server else {
        warn!("island_view: no AssetServer, nothing will be drawn");
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
        actor_gltf: ModelKind::ANIMATED
            .into_iter()
            .map(|k| server.load(k.path().expect("animated model path")))
            .collect(),
        actor: ModelKind::ANIMATED
            .into_iter()
            .map(|k| scene(&server, k.path().expect("animated model path")))
            .collect(),
        models: Models::load(&server),
        grass,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of this test's own, removed when dropped.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "island-assets-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn assets_resolve_from_the_environment_then_the_executable_then_the_workspace() {
        let scratch = Scratch::new("order");
        let named = scratch.folder("named");
        let packaged = scratch.folder("package");
        let executable = packaged.join("game_menu.exe");
        let beside = scratch.folder("package/assets");
        let bare = scratch.folder("bare").join("game_menu.exe");
        let workspace = scratch.folder("workspace/assets");
        let bevy_root = scratch.folder("bevy-root");
        let pick = |env: Option<&Path>, exe: Option<&Path>, root: Option<&Path>| {
            let chosen = resolve_asset_dir(env, exe, root, &workspace);
            (chosen.path, chosen.source)
        };

        // 1. The variable wins over a packaged folder and every fallback.
        assert_eq!(
            pick(Some(&named), Some(&executable), Some(&bevy_root)),
            (named.clone(), AssetSource::Environment)
        );
        // 2. Without it, the folder beside the executable.
        assert_eq!(
            pick(None, Some(&executable), Some(&bevy_root)),
            (beside.clone(), AssetSource::BesideExecutable)
        );
        // 3. A build with nothing beside it is a developer build.
        assert_eq!(
            pick(None, Some(&bare), None),
            (workspace.clone(), AssetSource::Workspace)
        );
        assert_eq!(
            pick(None, None, None),
            (workspace.clone(), AssetSource::Workspace)
        );
        assert_eq!(
            pick(None, Some(&bare), Some(&bevy_root)),
            (bevy_root.join("assets"), AssetSource::BevyAssetRoot),
            "capture scripts keep steering a developer build"
        );

        // A variable that names no folder is passed over, and reported.
        let missing = scratch.0.join("nowhere");
        let chosen = resolve_asset_dir(Some(&missing), Some(&executable), None, &workspace);
        assert_eq!(chosen.source, AssetSource::BesideExecutable);
        assert_eq!(chosen.ignored, Some(missing.clone()));
        let chosen = resolve_asset_dir(Some(&named), Some(&executable), None, &workspace);
        assert_eq!(chosen.ignored, None);
        // A file called `assets` beside the executable is not an asset folder.
        let odd = scratch.folder("odd");
        std::fs::write(odd.join("assets"), b"").unwrap();
        assert_eq!(
            pick(None, Some(&odd.join("game_menu.exe")), None),
            (workspace.clone(), AssetSource::Workspace)
        );
        // The fallback is named even when it is absent, so the log can say so.
        let gone = scratch.0.join("gone/assets");
        let chosen = resolve_asset_dir(None, Some(&bare), None, &gone);
        assert_eq!((chosen.path, chosen.source), (gone, AssetSource::Workspace));
    }

    #[test]
    fn resident_roster_uses_distinct_models_and_separate_rig_families() {
        let roster: std::collections::BTreeSet<_> = (0..42).map(ModelKind::for_resident).collect();
        assert!(roster.len() >= 6);
        assert!(roster.contains(&ModelKind::VillageWoman));
        assert!(roster.contains(&ModelKind::FarmerMan));
        for (index, kind) in ModelKind::ANIMATED.into_iter().enumerate() {
            assert_eq!(kind.index(), index);
            assert!(
                std::path::Path::new(ASSET_DIR)
                    .join(kind.path().unwrap())
                    .is_file()
            );
        }
        assert_ne!(
            ModelKind::VillageWoman.idle_clip(),
            ModelKind::Rogue.idle_clip()
        );
    }
}
