//! Shared native presentation grammar. No simulation or persistence operations.
use super::UiCtx;
use crate::{
    art::{MUTED, TEXT},
    settings::{Palette, Settings},
};
use bevy_egui::egui::{self, Color32, FontId, Pos2, RichText, Sense, Stroke, vec2};
use horde_core::{content::Mode, quality::Quality, survivors::SurvivorId};

pub(super) fn page_title(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(
        RichText::new(title).font(FontId::new(28.0, egui::FontFamily::Name("display".into()))),
    );
    if !description.is_empty() {
        ui.label(RichText::new(description).color(MUTED));
    }
    ui.add_space(12.0);
}
pub(super) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(18.0).strong());
    ui.add_space(4.0);
}
pub(super) fn defender_name<'a>(f: &UiCtx<'a>) -> &'a str {
    if f.campaign.mode == Mode::Personal {
        f.game
            .campaign()
            .selected_survivor
            .map_or("You", SurvivorId::name)
    } else {
        &f.game.campaign().npc.name
    }
}
pub(super) fn mode_picker(ui: &mut egui::Ui, f: &mut UiCtx) {
    if !f
        .game
        .campaign()
        .has_feature(horde_core::skill_catalog::CampaignFeature::Companion)
    {
        f.campaign.mode = Mode::Personal;
        return;
    }
    let previous = f.campaign.mode;
    // Its own centred row, so the label meets its buttons wherever it is placed.
    row(ui, |ui| {
        ui.label("Defender");
        ui.selectable_value(&mut f.campaign.mode, Mode::Personal, "You");
        let available = f.game.campaign().npc_available();
        ui.add_enabled_ui(available, |ui| {
            ui.selectable_value(
                &mut f.campaign.mode,
                Mode::Npc,
                f.game.campaign().npc.name.clone(),
            );
        })
        .response
        .on_disabled_hover_text(
            "Unlock Mara on the journey upgrade tree after the required country milestone.",
        );
    });
    if previous != f.campaign.mode {
        f.campaign.improvement_focus = None;
    }
}
/// Height of a padded button or combo box. Wrapped filter rows set their
/// interact height to this before adding widgets: egui centres each widget in
/// the row height known when it is placed, so a shorter start leaves the first
/// widget above its taller siblings.
pub(super) fn filter_row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y
}
/// Starts a wrapped filter row whose widgets share one centre line.
pub(super) fn begin_filter_row(ui: &mut egui::Ui) -> f32 {
    let row = filter_row_height(ui);
    ui.spacing_mut().interact_size.y = row;
    ui.set_min_height(row);
    row
}
/// A wrapped row whose labels and buttons share one centre line. Use it for
/// any row that mixes plain text with buttons or pickers.
pub(super) fn row<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        begin_filter_row(ui);
        add(ui)
    })
}
/// The colour a quality is drawn in. The standard palette is the quality
/// catalogue's; the colour-blind one is Okabe-Ito, whose hues stay apart
/// under the common colour-vision deficiencies.
pub(super) fn quality_colour(quality: Quality, palette: Palette) -> Color32 {
    match palette {
        Palette::Standard => {
            let [r, g, b, a] = quality.color();
            Color32::from_rgba_unmultiplied(
                (r * 255.0).round() as u8,
                (g * 255.0).round() as u8,
                (b * 255.0).round() as u8,
                (a * 255.0).round() as u8,
            )
        }
        Palette::ColorBlind => match quality {
            Quality::Poor => Color32::from_rgb(120, 120, 120),
            Quality::Common => TEXT,
            Quality::Uncommon => Color32::from_rgb(0, 158, 115),
            Quality::Rare => Color32::from_rgb(86, 180, 233),
            Quality::Epic => Color32::from_rgb(204, 121, 167),
            Quality::Legendary => Color32::from_rgb(230, 159, 0),
        },
    }
}
/// The shape that marks a quality without colour: one per quality, painted,
/// never a font glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QualityShape {
    /// Poor: the only outline, pointing down.
    HollowTriangleDown,
    /// Common.
    Disc,
    /// Uncommon.
    Diamond,
    /// Rare.
    Square,
    /// Epic.
    TriangleUp,
    /// Legendary.
    Star,
}
pub(super) fn quality_shape(quality: Quality) -> QualityShape {
    match quality {
        Quality::Poor => QualityShape::HollowTriangleDown,
        Quality::Common => QualityShape::Disc,
        Quality::Uncommon => QualityShape::Diamond,
        Quality::Rare => QualityShape::Square,
        Quality::Epic => QualityShape::TriangleUp,
        Quality::Legendary => QualityShape::Star,
    }
}
/// Side of the square a quality shape is drawn in beside text.
pub(super) const QUALITY_SHAPE_SIDE: f32 = 12.0;
/// Paints `quality`'s shape as one egui shape inside the `side`-wide square
/// at `center`. Static: nothing here animates, so reduced effects needs no
/// second form.
pub(super) fn paint_quality_shape(
    painter: &egui::Painter,
    quality: Quality,
    center: Pos2,
    side: f32,
    colour: Color32,
) {
    let r = side * 0.5;
    // A point on the circle of radius `radius`, `turn` of a turn clockwise
    // from straight up.
    let at = |turn: f32, radius: f32| {
        let angle = turn * std::f32::consts::TAU;
        center + vec2(angle.sin(), -angle.cos()) * radius
    };
    match quality_shape(quality) {
        QualityShape::HollowTriangleDown => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at(0.5, r - 1.0),
                    at(1.0 / 6.0, r - 1.0),
                    at(5.0 / 6.0, r - 1.0),
                ],
                Stroke::new(1.5, colour),
            ));
        }
        QualityShape::Disc => {
            painter.circle_filled(center, r * 0.8, colour);
        }
        QualityShape::Diamond => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(0.0, r), at(0.25, r * 0.8), at(0.5, r), at(0.75, r * 0.8)],
                colour,
                Stroke::NONE,
            ));
        }
        QualityShape::Square => {
            painter.rect_filled(
                egui::Rect::from_center_size(center, egui::Vec2::splat(side * 0.78)),
                0.0,
                colour,
            );
        }
        QualityShape::TriangleUp => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(0.0, r), at(1.0 / 3.0, r), at(2.0 / 3.0, r)],
                colour,
                Stroke::NONE,
            ));
        }
        QualityShape::Star => {
            // A star is not convex: a triangle fan from its centre is.
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(center, colour);
            for point in 0..10u8 {
                let radius = if point % 2 == 0 { r } else { r * 0.45 };
                mesh.colored_vertex(at(f32::from(point) / 10.0, radius), colour);
            }
            for point in 0..10u32 {
                mesh.add_triangle(0, 1 + point, 1 + (point + 1) % 10);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}
/// The quality's shape beside the text that follows, when the player chose
/// shape markers; nothing otherwise.
pub(super) fn quality_shape_icon(ui: &mut egui::Ui, quality: Quality, settings: &Settings) {
    if !settings.quality_markers.shape() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(QUALITY_SHAPE_SIDE), Sense::hover());
    paint_quality_shape(
        ui.painter(),
        quality,
        rect.center(),
        QUALITY_SHAPE_SIDE,
        quality_colour(quality, settings.palette),
    );
}
/// A quality as the player's settings show it: its name in the palette's
/// colour, behind its shape when shape markers are on.
pub(super) fn quality_marker(
    ui: &mut egui::Ui,
    quality: Quality,
    settings: &Settings,
) -> egui::Response {
    with_quality_shape(ui, quality, settings, |ui| {
        ui.label(RichText::new(quality.name()).color(quality_colour(quality, settings.palette)))
    })
}
/// Lays `add` out behind the quality's shape when shape markers are on.
/// Without them `add` runs in `ui` itself, so the default layout is the one
/// it had before markers existed.
pub(super) fn with_quality_shape<R>(
    ui: &mut egui::Ui,
    quality: Quality,
    settings: &Settings,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if !settings.quality_markers.shape() {
        return add(ui);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        quality_shape_icon(ui, quality, settings);
        add(ui)
    })
    .inner
}
/// An item label in its quality's colour. Where colour alone carries the
/// quality, text markers add its name.
pub(super) fn quality_item_text(
    text: impl Into<String>,
    quality: Quality,
    settings: &Settings,
) -> RichText {
    let mut text = text.into();
    if settings.quality_markers.text() {
        text = format!("{text} · {}", quality.name());
    }
    RichText::new(text).color(quality_colour(quality, settings.palette))
}
/// `count` followed by the singular or plural noun: `1 alert`, `2 alerts`.
pub(super) fn count_noun(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}
/// Sentence-case label for an authored snake_case or kebab-case content id,
/// such as a stage theme's weather (`ground_fog` → `Ground fog`) or lighting.
/// Theme ids have no separate authored display name, so presentation derives one.
pub(super) fn display_label(id: &str) -> String {
    let words: Vec<&str> = id
        .split(['_', '-', ' '])
        .filter(|word| !word.is_empty())
        .collect();
    let mut label = words.join(" ").to_lowercase();
    if let Some(first) = label.chars().next() {
        label.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{count_noun, display_label};

    #[test]
    fn counts_agree_with_their_noun() {
        assert_eq!(count_noun(1, "alert", "alerts"), "1 alert");
        assert_eq!(count_noun(0, "alert", "alerts"), "0 alerts");
        assert_eq!(
            count_noun(3, "live expedition", "live expeditions"),
            "3 live expeditions"
        );
    }

    #[test]
    fn display_label_humanises_authored_theme_ids() {
        assert_eq!(display_label("ground_fog"), "Ground fog");
        assert_eq!(display_label("light_rain"), "Light rain");
        assert_eq!(display_label("dawn"), "Dawn");
        assert_eq!(display_label("sea-mist"), "Sea mist");
        assert_eq!(display_label("_double__under_"), "Double under");
        assert_eq!(display_label("Overcast"), "Overcast");
        assert_eq!(display_label(""), "");
    }

    #[test]
    fn every_authored_weather_and_lighting_id_has_a_readable_label() {
        let themes = &horde_core::world::world().stage_themes;
        assert!(!themes.is_empty());
        for theme in themes {
            for id in [&theme.weather, &theme.lighting] {
                let label = display_label(id);
                assert!(!label.is_empty() && !label.contains('_'), "{id} -> {label}");
            }
        }
    }
}
