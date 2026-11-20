//! Install only bytes already accepted by the bounded presentation importer.
use super::{
    budget::{ActorSceneAdmission, ActorSceneBudget, ActorSceneBudgetDiagnostics},
    scenes::{ActorSceneAppearance, ActorSceneOverrides},
};
use crate::asset_pack::{AnimationPolicy, PackTarget, ValidatedAssetPack, ValidatedModel};
use bevy::{
    asset::{
        AssetApp,
        io::{
            AssetSourceBuilder,
            memory::{Dir, MemoryAssetReader},
        },
    },
    gltf::{GltfAssetLabel, GltfLoaderSettings, convert_coordinates::GltfConvertCoordinates},
    prelude::*,
};
use std::{path::Path, sync::Arc};

/// Animation policy belongs to this binding. Several species may share the
/// same immutable model while using different speeds, clips and blend times.
#[derive(Clone, Debug)]
pub struct ActorSceneAnimation {
    pub(crate) model: Arc<ValidatedModel>,
    pub(crate) policy: AnimationPolicy,
    pub(crate) scale: f32,
}
impl PartialEq for ActorSceneAnimation {
    fn eq(&self, other: &Self) -> bool {
        self.model.hash == other.model.hash
            && self.model.scene == other.model.scene
            && self.policy == other.policy
            && self.scale == other.scale
    }
}
impl ActorSceneAnimation {
    pub fn policy(&self) -> &AnimationPolicy {
        &self.policy
    }
    pub fn model_identity(&self) -> &str {
        &self.model.hash
    }
}

/// Opaque immutable model metadata. Constructed only by `AssetPackPlugin`, so
/// geometry accounting cannot be paired with different unvalidated GLB bytes.
#[derive(Clone, Debug)]
pub struct ActorSceneModel {
    pub(crate) model: Arc<ValidatedModel>,
    pub(crate) path: String,
}
impl PartialEq for ActorSceneModel {
    fn eq(&self, other: &Self) -> bool {
        self.model.hash == other.model.hash
            && self.model.scene == other.model.scene
            && self.path == other.path
    }
}
impl ActorSceneModel {
    pub fn identity(&self) -> &str {
        &self.model.hash
    }
    pub(super) fn key(&self) -> &str {
        &self.path
    }
    pub(super) fn load(&self, server: &AssetServer) -> Handle<WorldAsset> {
        server
            .load_builder()
            .with_settings(|settings: &mut GltfLoaderSettings| {
                settings.load_animations = false;
                settings.load_cameras = false;
                settings.load_lights = false;
                settings.convert_coordinates = Some(GltfConvertCoordinates::default());
            })
            .load(GltfAssetLabel::Scene(self.model.scene).from_asset(self.path.clone()))
    }
}

/// Add before `DefaultPlugins`: Bevy freezes its asset-source registry when
/// AssetPlugin is built. The private in-memory reader has no file watcher or
/// writable path, so changing a source file cannot change an installed pack.
pub struct AssetPackPlugin {
    pack: ValidatedAssetPack,
}
impl AssetPackPlugin {
    pub fn new(pack: ValidatedAssetPack) -> Self {
        Self { pack }
    }
}
#[derive(Resource)]
struct PackInstallation {
    pack: ValidatedAssetPack,
    source: String,
}
#[derive(Resource, Clone, Debug, Default)]
pub struct AssetPackDiagnostics {
    pub name: String,
    pub identity: String,
    pub model_count: usize,
    pub ready_instances: usize,
    pub loading_instances: usize,
    pub rejected_instances: usize,
    /// Includes failures before any supplied scene instance could be created.
    pub fallback_instances: usize,
    pub failed_models: usize,
    pub last_error: Option<String>,
    pub scene_budget: ActorSceneBudgetDiagnostics,
}

impl Plugin for AssetPackPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            !app.is_plugin_added::<bevy::asset::AssetPlugin>(),
            "Add AssetPackPlugin before DefaultPlugins/AssetPlugin"
        );
        let directory = Dir::default();
        for model in self.pack.models.values() {
            directory.insert_asset(
                Path::new(&format!("{}.glb", model.hash)),
                model.bytes.clone(),
            );
        }
        let source = format!(
            "terra-pack-{}",
            self.pack.identity.trim_start_matches("sha256:")
        );
        app.register_asset_source(
            source.clone(),
            AssetSourceBuilder::new(move || {
                Box::new(MemoryAssetReader {
                    root: directory.clone(),
                })
            }),
        )
        .insert_resource(PackInstallation {
            pack: self.pack.clone(),
            source,
        })
        .insert_resource(AssetPackDiagnostics {
            name: self.pack.name().into(),
            identity: self.pack.identity().into(),
            model_count: self.pack.models.len(),
            ..Default::default()
        })
        .init_resource::<ActorSceneOverrides>()
        .init_resource::<ActorSceneBudget>()
        .init_resource::<ActorSceneAdmission>()
        .add_systems(Startup, install_bindings)
        .add_systems(Update, update_diagnostics.after(crate::WorldSet::Render));
    }
}
fn install_bindings(
    pack: Res<PackInstallation>,
    mut registry: ResMut<ActorSceneOverrides>,
    mut admission: ResMut<ActorSceneAdmission>,
) {
    let mut seen = std::collections::BTreeSet::new();
    admission.immutable_estimated_bytes = pack
        .pack
        .models
        .values()
        .map(|model| {
            // Validated bytes plus immutable asset-reader snapshot; channel vectors
            // and node metadata remain installed even when a GPU model is deferred.
            model
                .bytes
                .len()
                .saturating_mul(1 + usize::from(seen.insert(model.hash.clone())))
                .saturating_add(
                    model
                        .clips
                        .values()
                        .flat_map(|c| &c.channels)
                        .map(|c| {
                            c.times
                                .len()
                                .saturating_mul(4)
                                .saturating_add(c.values.len().saturating_mul(16))
                        })
                        .sum::<usize>(),
                )
                .saturating_add(model.nodes.len().saturating_mul(256))
        })
        .sum();
    for binding in &pack.pack.manifest.bindings {
        let model = &pack.pack.models[&binding.model];
        let path = format!("{}://{}.glb", pack.source, model.hash);
        // A binding is metadata, not a request to decode or upload every model.
        let mut appearance = ActorSceneAppearance::new(Handle::default());
        appearance.model = Some(ActorSceneModel {
            model: model.clone(),
            path,
        });
        appearance.scale = binding.scale;
        appearance.ground_offset_m = binding.ground_offset_m;
        appearance.yaw_offset_degrees = binding.yaw_offset_degrees;
        appearance.driver_seat = binding.driver_seat.clone();
        appearance.animation = binding.animation.clone().map(|policy| ActorSceneAnimation {
            model: model.clone(),
            policy,
            scale: binding.scale,
        });
        // Both transform and target were checked before plugin construction.
        let result = match binding.target {
            PackTarget::Kind(kind) => registry.set_kind(kind, appearance),
            PackTarget::Species(species) => registry.set_species(species, appearance),
            PackTarget::VehicleModel(model) => registry.set_vehicle_model(model, appearance),
        };
        if let Err(error) = result {
            error!("Asset binding rejected: {error}");
        }
    }
}

fn update_diagnostics(
    admission: Res<ActorSceneAdmission>,
    registry: Res<ActorSceneOverrides>,
    sandbox: Option<Res<crate::SandboxRuntime>>,
    actors: Query<(
        &super::scenes::ActorSceneAnchor,
        Option<&super::scenes::ActorSceneActive>,
    )>,
    instances: Query<
        (
            Option<&super::scenes::AppearanceReady>,
            Option<&super::scenes::AppearanceRejected>,
        ),
        With<super::scenes::ActorSceneInstance>,
    >,
    mut status: ResMut<AssetPackDiagnostics>,
) {
    status.ready_instances = 0;
    status.loading_instances = 0;
    status.rejected_instances = 0;
    status.fallback_instances = actors
        .iter()
        .filter(|(anchor, active)| {
            active.is_none()
                && registry
                    .resolve_model(
                        anchor.kind,
                        anchor.species,
                        super::scenes::model_for_anchor(anchor, sandbox.as_deref()),
                    )
                    .is_some()
        })
        .count();
    status.failed_models = admission.failed_models.len();
    status.scene_budget = admission.diagnostics.clone();
    status.scene_budget.unmetered_instances = actors
        .iter()
        .filter(|(anchor, _)| {
            registry
                .resolve_model(
                    anchor.kind,
                    anchor.species,
                    super::scenes::model_for_anchor(anchor, sandbox.as_deref()),
                )
                .is_some_and(|a| a.model.is_none())
        })
        .count();
    status.last_error = None;
    if status.failed_models > 0 {
        status.last_error=Some("A supplied model could not load; procedural actors remain available. Check the asset pack and its textures.".into());
    } else if admission.diagnostics.invalid_limits {
        status.last_error = Some("Presentation distance limits must be finite and nonnegative; procedural actors remain available.".into());
    }
    for (ready, rejected) in &instances {
        if let Some(rejected) = rejected {
            status.rejected_instances += 1;
            status.last_error = Some(rejected.0.clone());
        } else if ready.is_some() {
            status.ready_instances += 1;
        } else {
            status.loading_instances += 1;
        }
    }
}

/// The demo uses this same public importer; embedded content has no privileged
/// bypass. Only wildlife is bound, preserving the character creator and fleet.
pub fn bundled_wildlife_pack() -> Result<ValidatedAssetPack, String> {
    ValidatedAssetPack::from_bytes(
        include_bytes!("../assets/wildlife-pack/pack.json"),
        |file| {
            let bytes: &[u8] = match file {
                "deer.glb" => include_bytes!("../assets/wildlife-pack/deer.glb"),
                "boar.glb" => include_bytes!("../assets/wildlife-pack/boar.glb"),
                "hare.glb" => include_bytes!("../assets/wildlife-pack/hare.glb"),
                "fox.glb" => include_bytes!("../assets/wildlife-pack/fox.glb"),
                "ibex.glb" => include_bytes!("../assets/wildlife-pack/ibex.glb"),
                _ => return Err(format!("Unknown bundled model {file}")),
            };
            Ok(bytes.to_vec())
        },
    )
}
