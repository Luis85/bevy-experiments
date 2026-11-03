//! Isolated native review grids for the production player model. Every cell
//! paints the shared source-owned packets; nothing here is used in battle.
use crate::{
    player_model::PlayerAction,
    visual_assets::{ArtTextures, hold_sample, menu_survivor, paint_model},
};
use bevy_egui::egui::{Align2, Color32, FontId, Painter, Rect, Stroke, pos2, vec2};
use horde_core::{survivors::EquipmentVisual, weapons::WeaponClass};

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
/// paints the production source model.
pub(super) fn rig_review(p: &Painter, _textures: ArtTextures, rect: Rect) -> bool {
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
            let mut pose = hold_sample(center + vec2(1000., -height * 0.1));
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
            complete &= paint_model(
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
pub(super) fn rig_comparison(p: &Painter, textures: ArtTextures, rect: Rect) -> bool {
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
            let mut pose = hold_sample(center + vec2(1000., -height * 0.1));
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
                complete &= paint_model(
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
