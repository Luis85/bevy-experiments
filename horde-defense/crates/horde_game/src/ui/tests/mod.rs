//! Event-driven tests of the shipped egui widgets. No OS window or user saves.
use super::*;
use crate::profile::ProfilePaths;
use crate::runner::{AttackAims, Runtime};
use crate::{
    art::{self, MUTED},
    runner::{AppCommand, ControlFrame, SaveIntent},
    settings::{self, Action, Settings},
};
use bevy_egui::egui::{self, Color32, Rect, pos2, vec2};
use bevy_egui::egui::{Event, FullOutput, Key, Modifiers, PointerButton, Pos2, RawInput, Vec2};
use horde_core::{
    command::Command,
    content::{Mode, Slot, StructureKind, content},
    sim::{Game, Input, Outcome},
    survivors::{Appearance, SurvivorId},
    world::world,
};
use std::path::PathBuf;

struct IsolatedFrontend {
    frontend: FrontendParts,
    directory: PathBuf,
}
impl IsolatedFrontend {
    fn new() -> Self {
        let mut frontend = test_support::test_frontend();
        let directory = frontend.paths.campaign().parent().unwrap().to_path_buf();
        frontend.game = Game::default();
        // Existing control/layout cases exercise compatibility campaigns.
        frontend.game.campaign_mut().legacy_features = true;
        // Existing layout/control fixtures explicitly exercise legacy11×30 arenas.
        // Fresh3×12 geometry is tested separately below.
        frontend
            .game
            .campaign_mut()
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        frontend
            .game
            .campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        frontend.campaign.preview_survivor = SurvivorId::Rook;
        frontend.battle.aim = [15.0, f32::from(frontend.game.campaign().lane_count()) * 0.5];
        frontend.settings = Settings::default();
        frontend.screen = Screen::Camp;
        frontend.nav.return_screen = Screen::Camp;
        frontend.nav.tab = Tab::World;
        frontend.campaign.mode = Mode::Personal;
        frontend.campaign.stage = 0;
        frontend.campaign.selected_country = "DEU".into();
        frontend.campaign.search.clear();
        frontend.status.message.clear();
        frontend.battle.construction = false;
        frontend.battle.fire_latched = false;
        frontend.prefs.rebind = None;
        frontend.prefs.binding_armed = false;
        frontend.battle.pending = Input::default();
        frontend.battle.pending_attack_aims = AttackAims::default();
        frontend.battle.arena_rect = None;
        frontend.runtime = Runtime::default();
        frontend.status.save_pending = false;
        frontend.status.message_error = false;
        frontend.status.feedback = feedback::FeedbackState::default();
        frontend.status.feedback_revision = 0;
        frontend.status.practice_profile = false;
        frontend.status.profile_exists = false;
        Self {
            frontend,
            directory,
        }
    }
    fn unlock_trap(&mut self) {
        self.frontend
            .game
            .campaign_mut()
            .first_clear(0, Mode::Personal);
        self.frontend.game.campaign_mut().finish_attempt();
        self.frontend
            .game
            .campaign_mut()
            .reward(25, 0, Mode::Personal);
        self.frontend
            .game
            .campaign_mut()
            .purchase_blueprint(StructureKind::Trap)
            .unwrap();
    }
    fn deploy(&mut self) {
        self.frontend
            .game
            .campaign_mut()
            .choose_start_country("DEU")
            .unwrap();
        self.frontend
            .game
            .deploy_country_stage(0, Mode::Personal)
            .unwrap();
        self.frontend.game.set_paused(false);
        self.frontend.screen = Screen::Battle;
    }
}
impl Drop for IsolatedFrontend {
    fn drop(&mut self) {
        self.frontend.runtime.finish_storage_for_test();
        let archives = self.directory.join("journey-archives");
        if let (Ok(base), Ok(target)) = (self.directory.canonicalize(), archives.canonicalize())
            && target.starts_with(&base)
            && target != base
        {
            let _ = std::fs::remove_dir_all(target);
        }

        for file in [
            "campaign.json",
            "campaign.bak",
            "campaign.tmp",
            "settings.json",
        ] {
            let _ = std::fs::remove_file(self.directory.join(file));
        }
        let _ = std::fs::remove_dir(&self.directory);
    }
}
#[derive(Clone, Copy)]
enum View {
    Menu,
    SurvivorSelection,
    Header,
    Dossier,
    Battle,
    Settings,
    Camp,
    WorldWithChrome,
    Armory,
    Results,
    /// The upgrade home with the global header and field strip, as drawn.
    ResultsWithChrome,
    History,
    Feedback,
}
struct Harness {
    ctx: egui::Context,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        settings::install_font(&ctx);
        settings::configure(&ctx, 1.0);
        let mut warmup = ctx.run_ui(RawInput::default(), |_| {});
        // This headless event backend deliberately has no GPU texture uploader.
        warmup.textures_delta.clear();
        Self { ctx }
    }
    fn frame(&self, f: &mut FrontendParts, view: View, events: Vec<Event>) -> FullOutput {
        self.frame_size(f, view, events, vec2(1440.0, 900.0))
    }
    fn frame_size(
        &self,
        f: &mut FrontendParts,
        view: View,
        events: Vec<Event>,
        size: Vec2,
    ) -> FullOutput {
        f.runtime.begin_frame(self.ctx.cumulative_frame_nr() as u32);
        let mut output = {
            let mut c = f.ctx();
            let f = &mut c;
            self.ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..RawInput::default()
                },
                |ui| match view {
                    View::Menu => menu(ui, f, 0.0),
                    View::SurvivorSelection => menu::survivor_selection(ui, f, 0.0),
                    View::Header => navigation::header(ui, f, 1.0),
                    View::Dossier => {
                        egui::CentralPanel::default().show(ui, |ui| country_dossier(ui, f));
                    }
                    View::Battle => {
                        controls(ui.ctx(), f);
                        battle(ui, f);
                    }
                    View::Settings => settings_ui(ui, f),
                    View::WorldWithChrome => {
                        navigation::header(ui, f, 1.0);
                        navigation::field_status(ui, f);
                        camp(ui, f);
                    }
                    View::Camp => {
                        navigation::field_status(ui, f);
                        camp(ui, f);
                    }
                    View::Armory => {
                        egui::CentralPanel::default().show(ui, |ui| armory(ui, f));
                    }
                    View::Results => results(ui, f),
                    View::ResultsWithChrome => {
                        navigation::header(ui, f, 1.0);
                        navigation::field_status(ui, f);
                        results(ui, f);
                    }
                    View::History => history::show(ui, f),
                    View::Feedback => {
                        egui::CentralPanel::default().show(ui, |ui| status(ui, f));
                    }
                },
            )
        };
        output.textures_delta.clear();
        f.flush_for_test();
        output
    }
    fn click(&self, f: &mut FrontendParts, view: View, label: &str) {
        self.click_size(f, view, label, vec2(1440.0, 900.0));
    }
    fn click_size(&self, f: &mut FrontendParts, view: View, label: &str, size: Vec2) {
        let output = self.frame_size(f, view, vec![], size);
        let position = output
            .shapes
            .iter()
            .find_map(|clipped| text_position(&clipped.shape, label))
            .unwrap_or_else(|| panic!("Rendered action not found: {label}"));
        self.frame_size(
            f,
            view,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            size,
        );
        self.frame_size(
            f,
            view,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            size,
        );
    }
    fn key(&self, f: &mut FrontendParts, view: View, key: Key) {
        self.frame(f, view, vec![key_event(key, true)]);
        self.frame(f, view, vec![key_event(key, false)]);
    }
    fn feedback_at(&self, f: &mut FrontendParts, time: f64) -> FullOutput {
        let mut output = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                time: Some(time),
                ..RawInput::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| status(ui, &mut f.ctx()));
            },
        );
        output.textures_delta.clear();
        output
    }
    fn publish_combat(&self, f: &mut FrontendParts, events: Vec<Event>) {
        f.runtime.begin_frame(self.ctx.cumulative_frame_nr() as u32);
        let mut output = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                events,
                ..RawInput::default()
            },
            |ui| publish_controls(ui.ctx(), &mut f.ctx(), true, false),
        );
        output.textures_delta.clear();
    }
    fn combat_input(&self, f: &mut FrontendParts, events: Vec<Event>) -> Input {
        let mut input = Input::default();
        let mut output = self.ctx.run_ui(
            RawInput {
                events,
                ..RawInput::default()
            },
            |ui| {
                input = gather_combat_input(ui.ctx(), &mut f.ctx());
            },
        );
        output.textures_delta.clear();
        input
    }
}
fn key_event(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}
fn text_position(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
    match shape {
        egui::epaint::Shape::Text(text) if text.galley.text() == label => {
            Some(text.pos + text.galley.rect.center().to_vec2())
        }
        egui::epaint::Shape::Vec(shapes) => {
            shapes.iter().find_map(|shape| text_position(shape, label))
        }
        _ => None,
    }
}

fn shape_has_text(shape: &egui::epaint::Shape, label: &str) -> bool {
    match shape {
        egui::epaint::Shape::Text(text) => text.galley.text().contains(label),
        egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|shape| shape_has_text(shape, label)),
        _ => false,
    }
}
/// How many painted text runs read exactly `label`.
fn text_count(output: &FullOutput, label: &str) -> usize {
    fn count(shape: &egui::epaint::Shape, label: &str) -> usize {
        match shape {
            egui::epaint::Shape::Text(text) => usize::from(text.galley.text() == label),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().map(|s| count(s, label)).sum(),
            _ => 0,
        }
    }
    output.shapes.iter().map(|c| count(&c.shape, label)).sum()
}
fn pointer_event(pressed: bool) -> Event {
    Event::PointerButton {
        pos: pos2(200.0, 200.0),
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn input_enemy(f: &mut FrontendParts, lane: u8, x: f32) {
    let mut attempt = f.game.attempt_slot_mut().take().unwrap();
    f.game.spawn_enemy(
        &mut attempt,
        horde_core::content::EnemyKind::Shambler,
        lane,
        false,
    );
    attempt.enemies.last_mut().unwrap().x = x;
    *f.game.attempt_slot_mut() = Some(attempt);
}
fn attack_pointer(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn text_color(shape: &egui::epaint::Shape, label: &str) -> Option<Color32> {
    match shape {
        egui::epaint::Shape::Text(text) if text.galley.text() == label => text
            .galley
            .job
            .sections
            .first()
            .map(|section| section.format.color),
        egui::epaint::Shape::Vec(shapes) => {
            shapes.iter().find_map(|shape| text_color(shape, label))
        }
        _ => None,
    }
}
fn hud_text<'a>(
    shape: &'a egui::epaint::Shape,
    label: &str,
    arena: Rect,
) -> Option<&'a egui::epaint::TextShape> {
    match shape {
        egui::epaint::Shape::Text(text)
            if text.galley.text() == label
                && text.pos.y < arena.top()
                && text.pos.x < arena.right() =>
        {
            Some(text)
        }
        egui::epaint::Shape::Vec(shapes) => shapes
            .iter()
            .find_map(|shape| hud_text(shape, label, arena)),
        _ => None,
    }
}
fn labeled_bounds(shape: &egui::epaint::Shape, label: &str) -> Option<Rect> {
    match shape {
        egui::epaint::Shape::Text(t) if t.galley.text() == label => {
            Some(t.galley.rect.translate(t.pos.to_vec2()))
        }
        egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| labeled_bounds(s, label)),
        _ => None,
    }
}

/// The painted bounds of the text run reading exactly `label`, with the clip
/// rect it was painted under (a scroll viewport clips its content).
fn clipped_bounds(output: &FullOutput, label: &str) -> Option<(Rect, Rect)> {
    output.shapes.iter().find_map(|clipped| {
        labeled_bounds(&clipped.shape, label).map(|rect| (rect, clipped.clip_rect))
    })
}
/// Whether the text run reading exactly `label` is painted and fully visible.
fn fully_visible(output: &FullOutput, label: &str) -> bool {
    clipped_bounds(output, label).is_some_and(|(rect, clip)| clip.contains_rect(rect))
}

mod armory_tests;
mod battle_tests;
mod briefing_tests;
mod camp_tests;
mod character_tests;
mod feedback_tests;
mod history_tests;
mod menu_tests;
mod navigation_tests;
mod operations_tests;
mod progression_tests;
mod results_tests;
mod settings_tests;
