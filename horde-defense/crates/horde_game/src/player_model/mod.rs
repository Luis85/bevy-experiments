//! Original-pixel player models: cached source ownership, frozen equipment,
//! shared posed draw packets/effect sockets and a backend-only egui adapter.
pub mod appearance;
#[cfg(any(test, feature = "review"))]
pub mod assembly;
pub mod attachment_compile;
pub mod attachments;
pub mod backend_egui;
pub mod bind;
pub mod catalogue;
pub mod equipped_pose;
pub mod pose;
pub mod source_masks;
pub mod startup;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "review"))]
pub mod verification;

pub use appearance::{ResolvedAppearance, resolve_appearance};
#[cfg(any(test, feature = "review"))]
pub use assembly::resolve_model;
pub use backend_egui::draw_egui;
pub use bind::{ModelPlacement, SourceToDisplay};
pub use catalogue::{
    ModelDefinition, PlayerModelCatalogue, SkinDefinition, SourceChannel, SourceDefinition,
};
pub use equipped_pose::resolve_equipped_model;
pub use pose::{DrawPacket, PlayerAction, PlayerPoseSample, ResolvedPlayerPose};
pub use source_masks::{
    CompiledCatalogue, CompiledModel, CompiledSource, LocalMesh, MaskRect, MaskRun,
    SourceImageView, compile_catalogue,
};

/// The bundled player-model documents, matching `player_models/manifest.json`.
/// This is the one embedded catalogue: it backs the manifest-absent startup
/// fallback and tests/review tools. The JSON is embedded because startup
/// validates it before the app (and its AssetServer) exists; the source PNGs it
/// names are never embedded and are always read from the asset root.
pub fn bundled_catalogue() -> Result<PlayerModelCatalogue, String> {
    PlayerModelCatalogue::from_documents(
        &[
            include_str!("../../../../assets/player_models/rook.json"),
            include_str!("../../../../assets/player_models/june.json"),
        ],
        &[
            include_str!("../../../../assets/player_models/skins.json"),
            include_str!("../../../../assets/player_models/attachments.json"),
        ],
    )
}
/// The bundled catalogue compiled once from the startup asset root (or, before
/// startup ran, the development `assets/` folder).
pub fn builtin() -> Result<&'static CompiledCatalogue, String> {
    static CACHE: std::sync::OnceLock<Result<CompiledCatalogue, String>> =
        std::sync::OnceLock::new();
    match CACHE.get_or_init(|| {
        startup::bundled_asset_root().and_then(|root| startup::compile_bundled(&root))
    }) {
        Ok(value) => Ok(value),
        Err(e) => Err(e.clone()),
    }
}
