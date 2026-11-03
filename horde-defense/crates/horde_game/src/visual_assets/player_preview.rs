//! Read-only application previews. Production and review use the same resolved
//! source-owned packets as battlefield actors, with no legacy body fallback.
//! Review-only workshop grids live in `ui/review/player_model.rs`.
use super::*;
use crate::player_model::{PlayerAction, PlayerPoseSample};
use bevy_egui::egui::{Align2, FontId};
use horde_core::{campaign::DefenderProfile, survivors::EquipmentVisual};

/// Paints one resolved model, or a visible error label; shared with the review workshop.
pub fn paint_model(
    p: &Painter,
    model: &str,
    equipment: &EquipmentVisual,
    center: Pos2,
    size: Vec2,
    sample: PlayerPoseSample,
) -> bool {
    match resolve_player_model(model, equipment, sample, center, size)
        .and_then(|pose| draw_player_model(p, &pose))
    {
        Ok(()) => true,
        Err(error) => {
            p.text(
                center,
                Align2::CENTER_CENTER,
                format!("Model unavailable: {error}"),
                FontId::proportional(10.),
                Color32::from_rgb(242, 164, 140),
            );
            false
        }
    }
}
/// Neutral held pose aimed at a source-space point; review cells vary it.
pub fn hold_sample(aim: Pos2) -> PlayerPoseSample {
    PlayerPoseSample {
        tick: 0,
        aim: [aim.x, aim.y],
        action: PlayerAction::Hold,
        recoil: 0.,
        brace: 0.,
        hit: 0.,
        reduced_effects: false,
    }
}
/// Character/equipment screen: real frozen equipment and the shared production
/// pose. The UI target expresses a direction, not a close physical aim target.
pub fn defender_preview(
    p: &Painter,
    _textures: ArtTextures,
    profile: &DefenderProfile,
    center: Pos2,
    size: Vec2,
    target: Pos2,
) -> bool {
    let Ok(model) = player_model_id(profile.appearance) else {
        return false;
    };
    let delta = target - center;
    let direction = if delta.length_sq() > 0.000_001 {
        delta.normalized()
    } else {
        vec2(1., 0.)
    };
    paint_model(
        p,
        &model,
        &profile.equipment_visual,
        center,
        size,
        hold_sample(center + direction * 10_000.),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equipment_preview_uses_actual_battle_packet_geometry_without_mutating_gear() {
        use horde_core::{campaign::Campaign, content::Mode, survivors::SurvivorId};
        for id in SurvivorId::ALL {
            let mut campaign = Campaign::default();
            campaign.choose_survivor(id).unwrap();
            let profile = campaign.profile(Mode::Personal);
            let before = profile.equipment_visual.clone();
            let center = pos2(200., 200.);
            let size = vec2(150., 240.);
            let target = center + vec2(120., -30.);
            let aim = center + (target - center).normalized() * 10_000.;
            let model = player_model_id(profile.appearance).unwrap();
            let expected =
                resolve_player_model(&model, &before, hold_sample(aim), center, size).unwrap();
            let context = bevy_egui::egui::Context::default();
            let output = context.run_ui(Default::default(), |ui| {
                assert!(defender_preview(
                    ui.painter(),
                    ArtTextures::default(),
                    &profile,
                    center,
                    size,
                    target
                ))
            });
            let actual = output
                .shapes
                .iter()
                .filter_map(|s| {
                    if let bevy_egui::egui::Shape::Mesh(mesh) = &s.shape {
                        Some(mesh)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let expected = expected
                .packets
                .iter()
                .filter(|p| !p.local_mesh.vertices.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(
                actual.len(),
                expected.len(),
                "Preview must not use a different body route"
            );
            for (actual, expected) in actual.iter().zip(expected) {
                assert_eq!(actual.indices, expected.local_mesh.indices);
                assert_eq!(actual.vertices.len(), expected.local_mesh.vertices.len());
                for (a, e) in actual.vertices.iter().zip(&expected.local_mesh.vertices) {
                    let point =
                        crate::player_model::verification::packet_point(expected, e.position);
                    assert_eq!([a.pos.x, a.pos.y], point);
                    assert_eq!([a.uv.x, a.uv.y], e.uv);
                }
            }
            assert_eq!(profile.equipment_visual, before);
            output.drop_without_applying_deltas();
        }
    }
    #[test]
    fn centered_preview_pointer_still_renders_a_valid_neutral_direction() {
        let profile =
            horde_core::campaign::Campaign::default().profile(horde_core::content::Mode::Personal);
        let context = bevy_egui::egui::Context::default();
        context
            .run_ui(Default::default(), |ui| {
                assert!(defender_preview(
                    ui.painter(),
                    ArtTextures::default(),
                    &profile,
                    pos2(100., 100.),
                    vec2(80., 100.),
                    pos2(100., 100.)
                ))
            })
            .drop_without_applying_deltas();
    }
}
