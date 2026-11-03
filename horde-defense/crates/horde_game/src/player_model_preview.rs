//! Read-only application previews. Production and review use the same resolved
//! source-owned packets as battlefield actors, with no legacy body fallback.
use super::*;
use crate::player_model::{PlayerAction, PlayerPoseSample};
use bevy_egui::egui::{Align2, FontId, Stroke};
use horde_core::{campaign::DefenderProfile, survivors::EquipmentVisual};

fn paint(
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
fn sample(aim: Pos2) -> PlayerPoseSample {
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
    paint(
        p,
        &model,
        &profile.equipment_visual,
        center,
        size,
        sample(center + direction * 10_000.),
    )
}
fn review_equipment(class: WeaponClass) -> Result<EquipmentVisual, String> {
    crate::player_model::verification::review_equipment(
        Some(class),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )
}
fn label(p: &Painter, area: Rect, text: &str, ground: f32) {
    p.text(
        area.center_top() + vec2(0., 8.),
        Align2::CENTER_TOP,
        text,
        FontId::proportional(13.),
        Color32::from_rgb(220, 222, 199),
    );
    p.line_segment(
        [
            pos2(area.left() + 8., ground),
            pos2(area.right() - 8., ground),
        ],
        Stroke::new(1., Color32::from_rgb(81, 103, 87)),
    );
}
/// Compatibility name for the isolated native review route. Every cell now
/// paints the production source model; historical kit fixtures are test-only.
pub fn rig_review(p: &Painter, _textures: ArtTextures, rect: Rect) -> bool {
    p.rect_filled(rect, 0., Color32::from_rgb(35, 48, 42));
    let cell = vec2(rect.width() / 6., rect.height() / 3.);
    let mut complete = true;
    for row in 0..3 {
        for col in 0..6 {
            let area = Rect::from_min_size(
                rect.min + vec2(col as f32 * cell.x, row as f32 * cell.y),
                cell,
            );
            let height = (cell.y - 40.).min(205.);
            let ground = area.bottom() - 10.;
            let center = pos2(area.center().x - height * 0.10, ground - height * 0.5);
            let model = if row == 0 { "rook" } else { "june" };
            let class = if row < 2 {
                WeaponClass::ALL[col]
            } else {
                WeaponClass::Rifle
            };
            let Ok(equipment) = review_equipment(class) else {
                return false;
            };
            let mut pose = sample(center + vec2(1000., -height * 0.1));
            let text = if row < 2 {
                format!("{model} · {class:?}")
            } else {
                match col {
                    0 => {
                        pose.aim = [center.x - 1000., center.y - 350.];
                        "Aim left"
                    }
                    1 => {
                        pose.aim = [center.x + 400., center.y - 1000.];
                        "Aim up"
                    }
                    2 => {
                        pose.aim = [center.x + 400., center.y + 1000.];
                        "Aim down"
                    }
                    3 => {
                        pose.action = PlayerAction::Reload { phase: 0.5 };
                        "Reload"
                    }
                    4 => {
                        pose.action = PlayerAction::Throw { phase: 0.0 };
                        "Throw / release"
                    }
                    _ => {
                        pose.action = PlayerAction::Melee { phase: 0.5 };
                        "Melee"
                    }
                }
                .into()
            };
            label(p, area, &text, ground);
            complete &= paint(
                p,
                model,
                &equipment,
                center,
                vec2(height * 0.95, height),
                pose,
            );
        }
    }
    complete
}
/// Literal still/assembled comparison remains an isolated review tool. Battle
/// never paints the whole still underneath its modular source-owned parts.
pub fn rig_comparison(p: &Painter, textures: ArtTextures, rect: Rect) -> bool {
    if textures.menu_idle.is_none() {
        return false;
    }
    p.rect_filled(rect, 0., Color32::from_rgb(35, 48, 42));
    let cell = vec2(rect.width() / 6., rect.height() / 2.);
    let mut complete = true;
    for row in 0..2 {
        for col in 0..6 {
            let area = Rect::from_min_size(
                rect.min + vec2(col as f32 * cell.x, row as f32 * cell.y),
                cell,
            );
            let height = (cell.y - 70.).min(205.);
            let foot = area.center_bottom() - vec2(0., 18.);
            let center = foot - vec2(0., height * 0.5);
            let female = if row == 0 { col >= 3 } else { col % 2 == 1 };
            let model = if female { "june" } else { "rook" };
            let class = if row == 0 && col % 3 == 1 {
                WeaponClass::Pistol
            } else {
                WeaponClass::Rifle
            };
            let Ok(equipment) = review_equipment(class) else {
                return false;
            };
            let mut pose = sample(center + vec2(1000., -height * 0.1));
            let text = if row == 0 {
                format!(
                    "{model} · {}",
                    ["reference still", "battle pistol", "battle rifle"][col % 3]
                )
            } else {
                let action = match col {
                    0 => {
                        pose.aim = [center.x - 1000., center.y - 200.];
                        "aim left"
                    }
                    1 => {
                        pose.aim = [center.x + 500., center.y - 1000.];
                        "aim up"
                    }
                    2 => {
                        pose.brace = 1.;
                        "brace"
                    }
                    3 => {
                        pose.action = PlayerAction::Reload { phase: 0.5 };
                        "reload"
                    }
                    4 => {
                        pose.action = PlayerAction::Throw { phase: 0.0 };
                        "throw"
                    }
                    _ => {
                        pose.action = PlayerAction::Melee { phase: 0.5 };
                        "melee"
                    }
                };
                format!("{model} · {action}")
            };
            label(p, area, &text, foot.y);
            if row == 0 && col % 3 == 0 {
                complete &= menu_survivor(p, textures, female, foot, height, 0.);
            } else {
                complete &= paint(
                    p,
                    model,
                    &equipment,
                    center,
                    vec2(height * 0.95, height),
                    pose,
                );
            }
        }
    }
    complete
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
                resolve_player_model(&model, &before, sample(aim), center, size).unwrap();
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
