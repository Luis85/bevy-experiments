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

#[cfg(any(test, feature = "review"))]
pub fn bundled_catalogue() -> Result<PlayerModelCatalogue, String> {
    PlayerModelCatalogue::from_json(
        &[
            include_str!("../../../../assets/player_models/rook.json"),
            include_str!("../../../../assets/player_models/june.json"),
        ],
        include_str!("../../../../assets/player_models/skins.json"),
    )
}
#[cfg(any(test, feature = "review"))]
pub fn builtin() -> Result<&'static CompiledCatalogue, String> {
    static CACHE: std::sync::OnceLock<Result<CompiledCatalogue, String>> =
        std::sync::OnceLock::new();
    match CACHE.get_or_init(|| bundled_catalogue().and_then(backend_egui::decode_bundled)) {
        Ok(value) => Ok(value),
        Err(error) => Err(error.clone()),
    }
}
pub fn bundled_equipment_catalogue() -> Result<PlayerModelCatalogue, String> {
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
pub fn equipped_builtin() -> Result<&'static CompiledCatalogue, String> {
    static CACHE: std::sync::OnceLock<Result<CompiledCatalogue, String>> =
        std::sync::OnceLock::new();
    match CACHE.get_or_init(|| bundled_equipment_catalogue().and_then(startup::decode_embedded)) {
        Ok(value) => Ok(value),
        Err(e) => Err(e.clone()),
    }
}
