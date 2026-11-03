//! Menu presentation. Game changes are queued typed intents.
use super::*;

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

pub(super) fn menu(root: &mut egui::Ui, f: &mut Frontend, time: f32) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let all = ui.max_rect();
            let identity = f
                .game
                .campaign
                .selected_survivor
                .unwrap_or(f.preview_survivor);
            let lanes = f
                .game
                .attempt
                .as_ref()
                .map_or_else(|| f.game.campaign.lane_count(), |a| a.lane_count);
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
                        let has_journey = f.profile_exists
                            || f.game.campaign.selected_survivor.is_some()
                            || f.game.campaign.world.start.is_some();
                        if primary(
                            ui,
                            if has_journey {
                                "Resume journey"
                            } else {
                                "New game"
                            },
                        ) {
                            if has_journey {
                                f.resume();
                            } else {
                                f.request_new_game();
                            }
                        }
                        if has_journey && ui.button("New game").clicked() {
                            f.request_new_game();
                        }
                        if let Some(identity) = f.game.campaign.selected_survivor {
                            muted(
                                ui,
                                format!("{} · checkpoint · {} lanes", identity.name(), lanes),
                            );
                            if f.game.campaign.world.start.is_none()
                                && ui.small_button("Change survivor").clicked()
                            {
                                f.open_survivor_selection();
                            }
                        }
                        if f.practice_profile {
                            ui.label(RichText::new("Practice profile").color(AMBER).strong());
                            ui.add(
                                egui::Label::new(
                                    RichText::new("Separate practice saves").color(MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text("Progress and saves stay separate from your campaign.");
                        } else if let Some(country) = f.game.campaign.world.active_country() {
                            muted(
                                ui,
                                format!(
                                    "{} · {} / {} defenses secured",
                                    country.name,
                                    f.game.campaign.world.completed(&country.id),
                                    country.level_count()
                                ),
                            );
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized([92.0, 36.0], egui::Button::new("Settings"))
                                .clicked()
                            {
                                f.open_settings();
                            }
                            if ui
                                .add_sized([92.0, 36.0], egui::Button::new("History"))
                                .clicked()
                            {
                                f.open_history();
                            }
                        });
                        if ui
                            .add_sized([190.0, 36.0], egui::Button::new("Save and exit"))
                            .clicked()
                        {
                            f.request_save(SaveIntent::Quit);
                        }
                        ui.add_space(10.0);
                        egui::CollapsingHeader::new("Controls").show(ui, |ui| {
                            muted(
                                ui,
                                format!(
                                    "{} fires. {} strikes a contact lane.\n{} reloads. {} pauses.",
                                    f.settings.bindings[Action::Fire as usize],
                                    f.settings.bindings[Action::Melee as usize],
                                    f.settings.bindings[Action::Reload as usize],
                                    f.settings.bindings[Action::Pause as usize]
                                ),
                            );
                        });
                        if !f.message.is_empty() && (!f.practice_profile || f.message_error) {
                            ui.add_space(8.0);
                            ui.colored_label(if f.message_error { RED } else { MUTED }, &f.message);
                        }
                        if f.save_pending {
                            muted(ui, "Saving your campaign...");
                        }
                    });
            });
            ui.painter().text(
                all.left_bottom() + vec2(24.0, -20.0),
                Align2::LEFT_BOTTOM,
                "A BEVY EXPERIMENT  /  LOCAL CAMPAIGN  /  NO OFFLINE EARNINGS",
                FontId::proportional(10.0),
                MUTED,
            );
        });
    if f.new_journey_confirm {
        egui::Modal::new(egui::Id::new("new-journey-confirm")).show(root.ctx(),|ui| {
            ui.set_max_width(400.0);
            ui.heading("Start a new journey?");
            ui.label("Your current journey, equipment and active fields will be archived before a fresh campaign starts. Display, audio, controls and difficulty settings are kept.");
            ui.add_space(10.0);
            ui.add_enabled_ui(!f.save_pending&&!f.runtime.reset_in_progress(),|ui| {ui.horizontal(|ui| {
                if ui.button("Start new journey").clicked() {f.dispatch(GameCommand::NewJourney,"New journey ready.");}
                if ui.button("Keep current journey").clicked() {f.new_journey_confirm=false;}
            });});
            if f.save_pending||f.runtime.reset_in_progress(){muted(ui,"Archiving and saving the new journey...");}
            if f.message_error {ui.colored_label(RED,&f.message);}
        });
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
        crate::visual_assets::actor_animated(
            &p,
            textures,
            crate::visual_assets::Actor::Shambler,
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

pub(super) fn survivor_selection(root: &mut egui::Ui, f: &mut Frontend, time: f32) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK))
        .show(root, |ui| {
            let canvas = ui.max_rect();
            checkpoint_scene(
                ui.painter(),
                canvas,
                f.textures,
                f.preview_survivor,
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
                                                f.preview_survivor == identity,
                                                identity.name(),
                                            ),
                                        )
                                        .on_hover_text(identity.description())
                                        .clicked()
                                    {
                                        f.preview_survivor = identity;
                                    }
                                });
                            }
                        });
                        ui.add(
                            egui::Label::new(
                                RichText::new(f.preview_survivor.description()).color(MUTED),
                            )
                            .truncate(),
                        );
                        ui.add_space(6.0);
                        if primary(ui, &format!("Continue as {}", f.preview_survivor.name())) {
                            f.dispatch(
                                GameCommand::ChooseSurvivor(f.preview_survivor),
                                "Survivor chosen. Select your starting country.",
                            );
                        }
                        if ui.button("Back to main menu").clicked() {
                            f.screen = Screen::Menu;
                        }
                        if f.message_error {
                            status(ui, f);
                        }
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let mut f = Frontend::load();
            f.game = Game::default();
            f.runtime = Runtime::default();
            f.screen = Screen::Menu;
            f.practice_profile = practice;
            f.message.clear();
            f.save_pending = false;
            if chosen {
                f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
            }
            if campaign {
                f.game.campaign.choose_start_country("DEU").unwrap();
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
                    |ui| menu(ui, &mut f, 0.0),
                );
                output.textures_delta.clear();
                let launch = if f.profile_exists
                    || f.game.campaign.selected_survivor.is_some()
                    || f.game.campaign.world.start.is_some()
                {
                    "Resume journey"
                } else {
                    "New game"
                };
                for label in ["LASTLIGHT", launch, "Settings", "Save and exit"] {
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
