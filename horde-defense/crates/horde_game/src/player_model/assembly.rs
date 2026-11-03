//! Bind-only assembly. No aiming/animation/skin replacement before parity acceptance.
use super::{
    CompiledCatalogue, DrawPacket, ModelPlacement, ResolvedPlayerPose,
    appearance::reference_appearance,
};

pub fn resolve_model(
    catalogue: &CompiledCatalogue,
    model_id: &str,
    placement: ModelPlacement,
) -> Result<ResolvedPlayerPose, String> {
    let appearance = reference_appearance(catalogue, model_id)?;
    let model = catalogue.model(model_id).ok_or("Missing compiled model")?;
    let transform = placement.at_ground(model.definition.ground)?;
    let packets = appearance
        .part_indices
        .iter()
        .map(|index| {
            let part = &model.parts[*index];
            let d = &part.definition;
            DrawPacket {
                part_id: d.id.clone(),
                source_id: model.definition.source.id.clone(),
                channel: d.channel,
                slot: d.slot.clone(),
                material: d.material.clone(),
                draw_order: d.draw_order,
                rectangles: part.geometry.clone(),
                transform,
                local_mesh: part.local_mesh.clone(),
                source_to_bind: super::bind::Affine2::IDENTITY,
                bone_transform: super::bind::Affine2::IDENTITY,
            }
        })
        .collect();
    let sockets = model
        .definition
        .sockets
        .iter()
        .map(|s| (s.id.clone(), transform.point(s.bind_position)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let source_sockets = model
        .definition
        .sockets
        .iter()
        .map(|s| (s.id.clone(), s.bind_position))
        .collect();
    let muzzle = *sockets.get("muzzle").ok_or("Missing compiled muzzle")?;
    let throw_origin = *sockets
        .get("throw_origin")
        .ok_or("Missing compiled throw palm")?;
    Ok(ResolvedPlayerPose {
        model_id: model_id.into(),
        skin_id: appearance.skin_id,
        packets,
        sockets,
        source_sockets,
        muzzle,
        throw_origin,
    })
}
