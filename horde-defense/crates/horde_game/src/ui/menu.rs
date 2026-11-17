//! Menu presentation. Game changes are queued typed intents.
use super::{Screen, UiCtx, muted, primary, status};
use crate::{
    art::{self, AMBER, INK, MUTED, RED, TEXT},
    runner::{AppCommand, SaveIntent},
    settings::{self, BindingId, Settings},
};
use bevy_egui::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, pos2, vec2};
use horde_core::{
    command::Command,
    survivors::{Appearance, SurvivorId},
};

const FOOTER_LINE: &str = "A BEVY EXPERIMENT  /  LOCAL CAMPAIGN  /  NO OFFLINE EARNINGS";
/// A secondary menu button: the primary's 190 pt column, 36 pt tall. Every
/// button under the primary fills the column (Settings and History split
/// it), so the stack reads as one edge.
const MENU_BUTTON: egui::Vec2 = vec2(super::primary_action::PRIMARY_MIN_SIZE.x, 36.0);

/// The menu's Controls list: every binding, core rows first, with the
/// Settings rebind table's label and its key (`settings::binding_text`, so
/// an unbound action reads "Unbound").
fn controls_rows(settings: &Settings) -> Vec<(&'static str, &str)> {
    BindingId::all()
        .map(|id| {
            (
                super::preferences::binding_label(id),
                settings::binding_text(settings.binding(id)),
            )
        })
        .collect()
}

fn title_galley(ui: &egui::Ui, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut size = (width * 0.205).clamp(52.0, 88.0);
    let mut galley = ui.painter().layout_no_wrap(
        "LASTLIGHT".into(),
        FontId::new(size, egui::FontFamily::Name("display".into())),
        TEXT,
    );
    // Display fonts have different advances. Fit the installed face, then use
    // this exact single-line galley so a final letter cannot start a new row.
    for _ in 0..3 {
        if galley.size().x <= width {
            break;
        }
        size = (size * (width - 2.0).max(1.0) / galley.size().x).floor();
        galley = ui.painter().layout_no_wrap(
            "LASTLIGHT".into(),
            FontId::new(size, egui::FontFamily::Name("display".into())),
            TEXT,
        );
    }
    galley
}

pub(super) fn menu(root: &mut egui::Ui, f: &mut UiCtx, time: f32) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let all = ui.max_rect();
            let identity = f
                .game
                .campaign()
                .selected_survivor
                .unwrap_or(f.campaign.preview_survivor);
            let lanes = f
                .game
                .attempt()
                .map_or_else(|| f.game.campaign().lane_count(), |a| a.lane_count);
            checkpoint_scene(
                ui.painter(),
                all,
                f.textures,
                identity,
                time,
                f.settings.reduced_effects,
            );
            // A light falloff keeps the title legible without boxing in the illustration.
            let fade =
                Rect::from_min_max(all.min, pos2(all.left() + all.width() * 0.58, all.bottom()));
            let mut mesh = egui::Mesh::default();
            for (pos, color) in [
                (fade.left_top(), Color32::from_black_alpha(150)),
                (fade.right_top(), Color32::TRANSPARENT),
                (fade.right_bottom(), Color32::TRANSPARENT),
                (fade.left_bottom(), Color32::from_black_alpha(150)),
            ] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv: egui::epaint::WHITE_UV,
                    color,
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            ui.painter().add(egui::Shape::mesh(mesh));
            let compact = all.height() < 680.0;
            let margin = if compact { 28.0 } else { 48.0 };
            let top = if compact { 24.0 } else { all.height() * 0.16 };
            let actor = checkpoint_survivor_bounds(all);
            let width = (actor.left() - all.left() - margin - 18.0).clamp(190.0, 460.0);
            let left = Rect::from_min_max(
                all.min + vec2(margin, top),
                pos2(all.left() + margin + width, all.bottom() - 52.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                ui.set_clip_rect(left.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt("menu_content")
                    .max_height(left.height())
                    .show(ui, |ui| {
                        ui.set_max_width(width);
                        let title = title_galley(ui, ui.available_width());
                        // Widget placement snaps to physical pixels. Reserve
                        // any ink overhang plus one pixel above the title so
                        // rounding cannot cross the scroll viewport's top.
                        let ink = title.rect.union(title.mesh_bounds);
                        let top_guard = (-ink.top()).max(0.0) + ui.ctx().pixels_per_point().recip();
                        ui.add_space(top_guard);
                        ui.label(title);
                        ui.add_space(if compact { 12.0 } else { 28.0 });
                        ui.label(
                            RichText::new(format!(
                                "One survivor. {lanes} lanes.\nA world to take back."
                            ))
                            .size(if compact { 18.0 } else { 22.0 })
                            .color(TEXT),
                        );
                        ui.add_space(if compact { 16.0 } else { 30.0 });
                        let has_journey = f.status.profile_exists
                            || f.game.campaign().selected_survivor.is_some()
                            || f.game.campaign().world.start.is_some();
                        if primary(
                            ui,
                            if has_journey {
                                "Resume journey"
                            } else {
                                "New game"
                            },
                        )
                        .clicked()
                        {
                            if has_journey {
                                f.resume();
                            } else {
                                f.request_new_game();
                            }
                        }
                        if has_journey
                            && ui
                                .add_sized(MENU_BUTTON, egui::Button::new("New game"))
                                .clicked()
                        {
                            f.request_new_game();
                        }
                        if let Some(identity) = f.game.campaign().selected_survivor {
                            muted(
                                ui,
                                format!("{} · checkpoint · {} lanes", identity.name(), lanes),
                            );
                            if f.game.campaign().world.start.is_none()
                                && ui
                                    .add_sized(MENU_BUTTON, egui::Button::new("Change survivor"))
                                    .clicked()
                            {
                                f.open_survivor_selection();
                            }
                        }
                        if f.status.practice_profile {
                            ui.label(RichText::new("Practice profile").color(AMBER).strong());
                            ui.add(
                                egui::Label::new(
                                    RichText::new("Separate practice saves").color(MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text("Progress and saves stay separate from your campaign.");
                        } else if let Some(country) = f.game.campaign().world.active_country() {
                            muted(
                                ui,
                                format!(
                                    "{} · {} / {} defenses secured",
                                    country.name,
                                    f.game.campaign().world.completed(&country.id),
                                    country.level_count()
                                ),
                            );
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            // The pair splits the column, gap included.
                            let half = vec2(
                                (MENU_BUTTON.x - ui.spacing().item_spacing.x) * 0.5,
                                MENU_BUTTON.y,
                            );
                            if ui.add_sized(half, egui::Button::new("Settings")).clicked() {
                                f.open_settings();
                            }
                            if ui.add_sized(half, egui::Button::new("History")).clicked() {
                                f.open_history();
                            }
                        });
                        if ui
                            .add_sized(MENU_BUTTON, egui::Button::new("Save and quit"))
                            .clicked()
                        {
                            f.request_save(SaveIntent::Quit);
                        }
                        ui.add_space(10.0);
                        egui::CollapsingHeader::new("Controls").show(ui, |ui| {
                            egui::Grid::new("menu_controls")
                                .num_columns(2)
                                .spacing([16.0, 2.0])
                                .show(ui, |ui| {
                                    for (label, key) in controls_rows(f.settings) {
                                        muted(ui, label);
                                        muted(ui, key);
                                        ui.end_row();
                                    }
                                });
                        });
                        let message = !f.status.message.is_empty()
                            && (!f.status.practice_profile || f.status.message_error);
                        if message {
                            ui.add_space(8.0);
                            ui.colored_label(
                                if f.status.message_error { RED } else { MUTED },
                                &f.status.message,
                            );
                        }
                        // The pending text, once: the status message is
                        // "Saving…" itself while a save it requested runs.
                        if f.status.save_pending
                            && !(message && f.status.message == super::context::SAVING)
                        {
                            muted(ui, super::context::SAVING);
                        }
                    });
            });
            ui.painter().text(
                all.left_bottom() + vec2(24.0, -20.0),
                Align2::LEFT_BOTTOM,
                FOOTER_LINE,
                FontId::proportional(10.0),
                MUTED,
            );
            // The build stamp a playtester quotes with feedback.
            ui.painter().text(
                all.right_bottom() + vec2(-24.0, -20.0),
                Align2::RIGHT_BOTTOM,
                crate::build_info::stamp(),
                FontId::proportional(10.0),
                MUTED,
            );
        });
    if f.menu.new_journey_confirm {
        let modal = egui::Modal::new(egui::Id::new("new-journey-confirm")).show(root.ctx(),|ui| {
            ui.set_max_width(400.0);
            ui.heading("Start a new journey?");
            ui.label("Your current journey, equipment and active fields will be archived before a fresh campaign starts. Display, audio, controls and difficulty settings are kept.");
            ui.add_space(10.0);
            ui.add_enabled_ui(!f.status.save_pending&&!f.runtime.reset_in_progress(),|ui| {ui.horizontal(|ui| {
                if ui.button("Start new journey").clicked() {f.dispatch(AppCommand::NewJourney,"New journey ready.");}
                if ui.button("Keep current journey").clicked() {f.menu.new_journey_confirm=false;}
            });});
            if f.status.save_pending||f.runtime.reset_in_progress(){muted(ui,"Archiving and saving the new journey...");}
            if f.status.message_error {ui.colored_label(RED,&f.status.message);}
        });
        // Escape or a backdrop click keeps the current journey, unless the
        // reset is already under way.
        if modal.should_close() && !f.status.save_pending && !f.runtime.reset_in_progress() {
            f.menu.new_journey_confirm = false;
        }
    }
}

/// Every phase is a pure function of presentation time. Reduced effects uses
/// one static checkpoint pose without consulting the simulation clock or RNG.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CheckpointMotion {
    parallax: egui::Vec2,
    fog: f32,
    breath: f32,
    lamp: u8,
    infected: f32,
}
fn checkpoint_motion(time: f32, reduced: bool) -> CheckpointMotion {
    let t = if reduced { 0.0 } else { time };
    CheckpointMotion {
        parallax: vec2((t * 0.09).sin() * 4.0, (t * 0.07).sin() * 2.0),
        fog: (t * 0.009).fract(),
        breath: (t * 0.9).sin() * 0.008,
        lamp: (10.0 + (t * 1.7).sin() * 2.0) as u8,
        infected: t * 0.35,
    }
}
fn checkpoint_survivor_bounds(r: Rect) -> Rect {
    let source = r.expand(8.0);
    let height = crate::visual_assets::keyart_source_height(source, 0.21);
    let foot = crate::visual_assets::keyart_point(source, [0.400, 0.807]);
    Rect::from_min_max(
        foot - vec2(height * 0.41, height),
        foot + vec2(height * 0.60, 2.0),
    )
}
fn checkpoint_scene(
    p: &egui::Painter,
    r: Rect,
    textures: crate::visual_assets::ArtTextures,
    identity: SurvivorId,
    time: f32,
    reduced: bool,
) {
    let motion = checkpoint_motion(time, reduced);
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    let backdrop = r.expand(8.0).translate(motion.parallax);
    if !crate::visual_assets::keyart(&p, backdrop, textures) {
        art::camp_scene(&p, backdrop, if reduced { 0.0 } else { time });
    }
    for anchor in [[0.728, 0.275], [0.572, 0.50]] {
        let lamp = crate::visual_assets::keyart_point(backdrop, anchor);
        for (radius, alpha) in [(0.075, motion.lamp / 2), (0.042, motion.lamp)] {
            p.add(egui::Shape::ellipse_filled(
                lamp,
                vec2(r.height() * radius, r.height() * radius * 0.8),
                Color32::from_rgba_unmultiplied(239, 181, 82, alpha),
            ));
        }
    }
    for i in 0..3 {
        let phase = motion.infected * 0.013 + i as f32 * 0.19;
        let point = crate::visual_assets::keyart_point(
            backdrop,
            [0.62 + phase.sin() * 0.06 + i as f32 * 0.075, 0.60],
        );
        let pose = crate::animation::ActorPose {
            frame: if reduced {
                0
            } else {
                (motion.infected + i as f32).rem_euclid(4.0) as u8
            },
            ..Default::default()
        };
        let Some(shambler) =
            crate::visual_assets::Actor::enemy(horde_core::enemy_catalog::VisualSprite::Shambler)
        else {
            continue;
        };
        crate::visual_assets::actor_animated(
            &p,
            textures,
            shambler,
            point,
            vec2(r.height() * 0.026, r.height() * 0.043),
            Color32::from_rgba_unmultiplied(99, 113, 101, 110),
            &pose,
        );
    }
    for i in 0..3 {
        let x = (i as f32 * 0.37 + motion.fog).fract();
        let center = r.min + vec2(r.width() * x, r.height() * (0.66 + i as f32 * 0.10));
        p.add(egui::Shape::ellipse_filled(
            center,
            vec2(r.width() * 0.28, r.height() * 0.035),
            Color32::from_rgba_unmultiplied(151, 171, 156, 10),
        ));
    }
    let height = crate::visual_assets::keyart_source_height(backdrop, 0.21);
    let foot = crate::visual_assets::keyart_point(backdrop, [0.400, 0.807]);
    p.add(egui::Shape::ellipse_filled(
        foot,
        vec2(height * 0.20, height * 0.045),
        Color32::from_black_alpha(95),
    ));
    let female = identity.appearance() == Appearance::Female;
    if !crate::visual_assets::menu_survivor(&p, textures, female, foot, height, motion.breath) {
        crate::visual_assets::actor(
            &p,
            textures,
            if female {
                crate::visual_assets::Actor::Mara
            } else {
                crate::visual_assets::Actor::Player
            },
            foot - vec2(0.0, height * 0.5),
            vec2(height * 0.82, height),
            Color32::WHITE,
        );
    }
}

pub(super) fn survivor_selection(root: &mut egui::Ui, f: &mut UiCtx, time: f32) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let canvas = ui.max_rect();
            checkpoint_scene(
                ui.painter(),
                canvas,
                f.textures,
                f.campaign.preview_survivor,
                time,
                f.settings.reduced_effects,
            );
            let compact = canvas.height() < 680.0;
            let margin = if compact { 24.0 } else { 44.0 };
            let width = (canvas.width() - margin * 2.0).min(560.0);
            let bottom = checkpoint_survivor_bounds(canvas).top() - 16.0;
            let panel = Rect::from_min_max(
                canvas.min + vec2(margin, if compact { 18.0 } else { 44.0 }),
                pos2(canvas.left() + margin + width, bottom),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                ui.set_clip_rect(panel.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt("survivor_selection_content")
                    .max_height(panel.height())
                    .show(ui, |ui| {
                        ui.set_max_width(width);
                        ui.label(
                            RichText::new("Choose your survivor")
                                .size(if compact { 28.0 } else { 36.0 })
                                .strong(),
                        );
                        ui.label("Same worn pistol and attributes. Choose your survivor’s look.");
                        ui.add_space(8.0);
                        ui.columns(2, |columns| {
                            for (column, identity) in columns.iter_mut().zip(SurvivorId::ALL) {
                                column.horizontal(|ui| {
                                    let (portrait, _) =
                                        ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                                    crate::visual_assets::portrait(
                                        ui.painter(),
                                        f.textures,
                                        identity.appearance() == Appearance::Female,
                                        portrait.center(),
                                        portrait.size(),
                                    );
                                    let width = ui.available_width().min(180.0);
                                    if ui
                                        .add_sized(
                                            vec2(width, 36.0),
                                            egui::Button::selectable(
                                                f.campaign.preview_survivor == identity,
                                                identity.name(),
                                            ),
                                        )
                                        .on_hover_text(identity.description())
                                        .clicked()
                                    {
                                        f.campaign.preview_survivor = identity;
                                    }
                                });
                            }
                        });
                        ui.add(
                            egui::Label::new(
                                RichText::new(f.campaign.preview_survivor.description())
                                    .color(MUTED),
                            )
                            .truncate(),
                        );
                        ui.add_space(6.0);
                        if primary(
                            ui,
                            &format!("Continue as {}", f.campaign.preview_survivor.name()),
                        )
                        .clicked()
                        {
                            f.dispatch(
                                Command::ChooseSurvivor(f.campaign.preview_survivor),
                                "Survivor chosen. Select your starting country.",
                            );
                        }
                        if ui.button("Back to main menu").clicked() {
                            f.go(Screen::Menu);
                        }
                        if f.status.message_error {
                            status(ui, f);
                        }
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;
    use horde_core::content::Mode;
    use horde_core::sim::Game;

    fn text_shape<'a>(
        shape: &'a egui::epaint::Shape,
        label: &str,
    ) -> Option<&'a egui::epaint::TextShape> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => Some(text),
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| text_shape(shape, label))
            }
            _ => None,
        }
    }

    #[test]
    fn menu_survivor_uses_original_image_anchor_and_controls_stay_left_of_the_body() {
        for size in [vec2(1440.0, 900.0), vec2(1050.0, 700.0) / 1.35] {
            let r = Rect::from_min_size(egui::Pos2::ZERO, size);
            let body = checkpoint_survivor_bounds(r);
            let source = r.expand(8.0);
            let foot = crate::visual_assets::keyart_point(source, [0.400, 0.807]);
            let height = crate::visual_assets::keyart_source_height(source, 0.21);
            assert!((body.bottom() - 2.0 - foot.y).abs() < 0.001);
            assert!((body.top() - (foot.y - height)).abs() < 0.001);
            let margin = if size.y < 680.0 { 28.0 } else { 48.0 };
            let width = (body.left() - r.left() - margin - 18.0).clamp(190.0, 460.0);
            assert!(
                margin + width < body.left(),
                "Menu content must not cover the survivor"
            );
        }
    }
    /// Closing: the menu's Controls list names all eight core bindings and
    /// the two W3-10 extra ones (world clock, cycle attack) with the labels
    /// and order of the Settings rebind table, and an unbound action reads
    /// "Unbound".
    #[test]
    fn menu_controls_list_every_binding() {
        let mut settings = settings::Settings::default();
        assert_eq!(
            controls_rows(&settings),
            [
                ("Fire", "Mouse1"),
                ("Contact melee", "Mouse2"),
                ("Reload", "R"),
                ("Emergency aid", "1"),
                ("Brace", "2"),
                ("Construction", "B"),
                ("Pause (battle)", "Escape"),
                ("Toggle firing", "F"),
                ("World clock (camp home)", "P"),
                ("Cycle attack", "Q"),
            ]
        );
        let cycle = settings::BindingId::Extra(settings::ExtraAction::CycleAttack);
        settings.set_binding(cycle, String::new());
        assert!(controls_rows(&settings).contains(&("Cycle attack", "Unbound")));
    }
    #[test]
    fn checkpoint_motion_changes_only_in_normal_mode_and_reduced_effects_are_static() {
        assert_ne!(
            checkpoint_motion(0.0, false),
            checkpoint_motion(12.0, false)
        );
        assert_eq!(checkpoint_motion(0.0, true), checkpoint_motion(12.0, true));
        assert_eq!(
            checkpoint_motion(0.0, true),
            checkpoint_motion(9999.0, true)
        );
    }
    #[test]
    fn menu_title_and_all_main_actions_fit_the_minimum_window_at_maximum_scale() {
        for (practice, campaign, active, chosen) in [
            (false, false, false, false),
            (false, false, false, true),
            (true, false, false, false),
            (false, true, false, true),
            (false, true, true, true),
        ] {
            let ctx = egui::Context::default();
            settings::install_font(&ctx);
            settings::configure(&ctx, 1.35);
            let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
            warmup.textures_delta.clear();
            // Native integrations report coordinates after applying UI zoom.
            let viewport = Rect::from_min_size(egui::Pos2::ZERO, vec2(1050.0, 700.0) / 1.35);
            let mut f = crate::ui::test_support::test_frontend();
            f.game = Game::default();
            f.runtime = crate::runner::Runtime::default();
            f.screen = Screen::Menu;
            f.status.practice_profile = practice;
            f.status.message.clear();
            f.status.save_pending = false;
            if chosen {
                f.game
                    .campaign_mut()
                    .choose_survivor(SurvivorId::Rook)
                    .unwrap();
            }
            if campaign {
                f.game.campaign_mut().choose_start_country("DEU").unwrap();
            }
            if active {
                f.game.deploy_country_stage(0, Mode::Personal).unwrap();
            }
            let before = serde_json::to_value(&f.game).unwrap();
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        ..Default::default()
                    },
                    |ui| menu(ui, &mut f.ctx(), 0.0),
                );
                output.textures_delta.clear();
                let launch = if f.status.profile_exists
                    || f.game.campaign().selected_survivor.is_some()
                    || f.game.campaign().world.start.is_some()
                {
                    "Resume journey"
                } else {
                    "New game"
                };
                // The footer: the build stamp sits right-aligned inside the
                // window and clear of the left-aligned footer line.
                let footer_rect = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|clipped| {
                            text_shape(&clipped.shape, label).map(|text| {
                                (
                                    clipped.clip_rect,
                                    text.galley.rect.translate(text.pos.to_vec2()),
                                )
                            })
                        })
                        .unwrap_or_else(|| panic!("Missing menu footer text: {label}"))
                };
                let stamp = crate::build_info::stamp();
                let (stamp_clip, stamp_rect) = footer_rect(&stamp);
                let (_, line_rect) = footer_rect(FOOTER_LINE);
                assert!(
                    stamp_clip.intersect(viewport).contains_rect(stamp_rect),
                    "Clipped stamp: {stamp_rect:?} / {stamp_clip:?}"
                );
                assert!((stamp_rect.right() - (viewport.right() - 24.0)).abs() < 1.0);
                assert!((stamp_rect.bottom() - (viewport.bottom() - 20.0)).abs() < 1.0);
                assert!(
                    !stamp_rect.expand(8.0).intersects(line_rect),
                    "Stamp {stamp_rect:?} overlaps the footer line {line_rect:?}"
                );
                // The longest stamp a package can carry still clears the line.
                let longest = crate::build_info::stamp_from(
                    env!("CARGO_PKG_VERSION"),
                    Some("0123456789abcdef0123+dirty"),
                );
                assert!(longest.ends_with("+dirty)"), "{longest}");
                let longest_width = ctx.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap(longest, FontId::proportional(10.0), MUTED)
                        .size()
                        .x
                });
                assert!(
                    viewport.right() - 24.0 - longest_width > line_rect.right() + 8.0,
                    "Longest stamp ({longest_width}) overlaps the footer line {line_rect:?}"
                );
                for label in ["LASTLIGHT", launch, "Settings", "Save and quit"] {
                    let (clip, text) = output
                        .shapes
                        .iter()
                        .find_map(|clipped| {
                            text_shape(&clipped.shape, label).map(|text| (clipped.clip_rect, text))
                        })
                        .unwrap_or_else(|| panic!("Missing menu text: {label}"));
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        clip.intersect(viewport).contains_rect(rect),
                        "Clipped {label} for practice={practice}, campaign={campaign}, active={active}: {rect:?} / {clip:?}"
                    );
                    if label == "LASTLIGHT" {
                        assert_eq!(text.galley.rows.len(), 1, "Title must stay on one line");
                        assert!(
                            clip.intersect(viewport)
                                .contains_rect(text.visual_bounding_rect())
                        );
                    } else {
                        // Include the button's vertical padding, not just its text.
                        let padding = ((36.0 - rect.height()) / 2.0).max(0.0);
                        assert!(clip.contains_rect(rect.expand2(vec2(0.0, padding))));
                    }
                }
            }
            assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
        }
    }
}
