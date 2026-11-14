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

/// The summed catalogue price of the named skill nodes.
fn nodes_price(ids: &[&str]) -> u64 {
    ids.iter()
        .map(|id| horde_core::skill_catalog::skills().node(id).unwrap().price)
        .sum()
}

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
            "settings.tmp",
            crate::profile::UNREADABLE_SETTINGS,
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
    /// The whole App frame as `draw_screen` draws it: the current screen with
    /// its header and controls, and the primary registry cleared first. The
    /// harness sets `settings.scale` to its target's scale before drawing,
    /// since `draw_screen` applies that scale.
    Full,
}

/// A target window, the one harness size convention: `pixels` is the window
/// in physical pixels and `scale` the UI scale, so a frame is `pixels /
/// scale` points (`RawInput::screen_rect` is `pixels`, the context's zoom is
/// `scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Target {
    pixels: Vec2,
    scale: f32,
}
impl Target {
    /// 1440x900 at scale 1.0.
    const LARGE: Self = Self {
        pixels: vec2(1440.0, 900.0),
        scale: 1.0,
    };
    /// 1050x700 at scale 1.35 (777.8x518.5 points).
    const SMALL: Self = Self {
        pixels: vec2(1050.0, 700.0),
        scale: 1.35,
    };
    /// Both target sizes, large first.
    const BOTH: [Self; 2] = [Self::LARGE, Self::SMALL];
}

struct Harness {
    ctx: egui::Context,
    target: Target,
}
impl Harness {
    /// The large target (1440x900 at 1.0), as every older test assumed.
    fn new() -> Self {
        Self::at(Target::LARGE)
    }
    /// 1440x900 at scale 1.0.
    fn large() -> Self {
        Self::at(Target::LARGE)
    }
    /// 1050x700 at scale 1.35.
    fn small() -> Self {
        Self::at(Target::SMALL)
    }
    /// A harness whose frames run at `target`.
    fn at(target: Target) -> Self {
        let ctx = egui::Context::default();
        settings::install_font(&ctx);
        settings::configure(&ctx, target.scale);
        let mut warmup = ctx.run_ui(RawInput::default(), |_| {});
        // This headless event backend deliberately has no GPU texture uploader.
        warmup.textures_delta.clear();
        Self { ctx, target }
    }
    /// The primaries of the last frame (see `View::Full`; a partial view's
    /// frame clears the registry itself).
    fn primaries(&self) -> Vec<PrimaryRecord> {
        frame_primaries(&self.ctx)
    }
    /// The labels of the last frame's screen primaries.
    fn screen_primaries(&self) -> Vec<String> {
        self.primaries()
            .into_iter()
            .filter(|record| record.surface == Surface::Screen)
            .map(|record| record.label)
            .collect()
    }
    fn frame(&self, f: &mut FrontendParts, view: View, events: Vec<Event>) -> FullOutput {
        self.frame_size(f, view, events, self.target.pixels)
    }
    fn frame_size(
        &self,
        f: &mut FrontendParts,
        view: View,
        events: Vec<Event>,
        size: Vec2,
    ) -> FullOutput {
        self.frame_timed(f, view, events, size, None)
    }
    /// One frame at the harness target with the input clock at `time`
    /// (seconds): the wall clock a hold or a toast reads.
    fn frame_at(
        &self,
        f: &mut FrontendParts,
        view: View,
        events: Vec<Event>,
        time: f64,
    ) -> FullOutput {
        self.frame_timed(f, view, events, self.target.pixels, Some(time))
    }
    fn frame_timed(
        &self,
        f: &mut FrontendParts,
        view: View,
        events: Vec<Event>,
        size: Vec2,
        time: Option<f64>,
    ) -> FullOutput {
        f.runtime.begin_frame(self.ctx.cumulative_frame_nr() as u32);
        if matches!(view, View::Full) {
            f.settings.scale = self.target.scale;
        }
        let mut output = {
            let mut c = f.ctx();
            let f = &mut c;
            self.ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    time,
                    ..RawInput::default()
                },
                |ui| match view {
                    // draw_screen clears the primary registry itself.
                    View::Full => draw_screen(ui.ctx(), f, 0.0),
                    view => {
                        // A partial view clears it as draw_screen would, once
                        // per pass, so its registry holds this frame only.
                        primary_action::clear_frame_primaries(ui.ctx());
                        partial_view(ui, f, view);
                    }
                },
            )
        };
        output.textures_delta.clear();
        f.flush_for_test();
        output
    }
    fn click(&self, f: &mut FrontendParts, view: View, label: &str) {
        self.click_size(f, view, label, self.target.pixels);
    }
    fn click_size(&self, f: &mut FrontendParts, view: View, label: &str, size: Vec2) {
        let output = self.frame_size(f, view, vec![], size);
        let position = output
            .shapes
            .iter()
            .find_map(|clipped| text_position(&clipped.shape, label))
            .unwrap_or_else(|| panic!("Rendered action not found: {label}"));
        self.press_at(f, view, position, size);
    }
    /// Clicks the topmost text run reading `label`: the last one painted, so
    /// a modal's button wins over the same label on the screen under it.
    fn click_topmost(&self, f: &mut FrontendParts, view: View, label: &str) {
        let size = self.target.pixels;
        let output = self.frame_size(f, view, vec![], size);
        let position = output
            .shapes
            .iter()
            .rev()
            .find_map(|clipped| text_position(&clipped.shape, label))
            .unwrap_or_else(|| panic!("Rendered action not found: {label}"));
        self.press_at(f, view, position, size);
    }
    /// A primary press and release at `position`, one frame each.
    fn press_at(&self, f: &mut FrontendParts, view: View, position: Pos2, size: Vec2) {
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
    /// Presses the primary button on the text run reading `label` and keeps
    /// it down for `secs` of the input clock, a frame every 0.05 s, without
    /// releasing: how the battle bar's hold-to-retreat is driven (W3-13).
    fn hold(&self, f: &mut FrontendParts, view: View, label: &str, secs: f64) {
        let output = self.frame(f, view, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|clipped| text_position(&clipped.shape, label))
            .unwrap_or_else(|| panic!("Rendered action not found: {label}"));
        let start = self.ctx.input(|input| input.time) + 1.0;
        self.frame_at(f, view, pointer_at(position, true), start);
        let frames = (secs / 0.05).ceil() as u32;
        for frame in 1..=frames {
            self.frame_at(
                f,
                view,
                vec![Event::PointerMoved(position)],
                start + 0.05 * f64::from(frame),
            );
        }
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
            |ui| {
                let mut c = f.ctx();
                let views = BattleFrame::build(ui.ctx(), &c);
                publish_controls(ui.ctx(), &mut c, views.as_ref(), true, false);
            },
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
                let mut c = f.ctx();
                let views = BattleFrame::build(ui.ctx(), &c);
                input = gather_combat_input(ui.ctx(), &mut c, views.as_ref());
            },
        );
        output.textures_delta.clear();
        input
    }
    /// Combat input gathered outside a pass, from the last pass's input state.
    fn combat_input_after_pass(&self, f: &mut FrontendParts) -> Input {
        let mut c = f.ctx();
        let views = BattleFrame::build(&self.ctx, &c);
        gather_combat_input(&self.ctx, &mut c, views.as_ref())
    }
}
/// One frame of a partial `view` (every view but `View::Full`).
fn partial_view(ui: &mut egui::Ui, f: &mut UiCtx, view: View) {
    match view {
        View::Full => unreachable!("draw_screen draws the full frame"),
        View::Menu => menu(ui, f, 0.0),
        View::SurvivorSelection => menu::survivor_selection(ui, f, 0.0),
        View::Header => navigation::header(ui, f, 1.0),
        View::Dossier => {
            egui::CentralPanel::default().show(ui, |ui| country_dossier(ui, f));
        }
        View::Battle => {
            let views = BattleFrame::build(ui.ctx(), f);
            controls(ui.ctx(), f, views.as_ref());
            battle(ui, f, views.as_ref());
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
    }
}
/// The pointer moving to `position` with the primary button pressed or
/// released there.
fn pointer_at(position: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(position),
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        },
    ]
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
pub(super) fn text_count(output: &FullOutput, label: &str) -> usize {
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
pub(super) fn text_color(shape: &egui::epaint::Shape, label: &str) -> Option<Color32> {
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
mod boss_banner_tests;
mod briefing_tests;
mod camp_tests;
mod character_tests;
mod chrome_tests;
mod defender_tests;
mod escape_tests;
mod feedback_tests;
mod frame_tests;
mod history_tests;
mod home_tests;
mod loot_block_tests;
mod low_hp_tests;
mod menu_tests;
mod navigation_tests;
mod operations_tests;
mod primary_tests;
mod progression_tests;
mod recruit_tests;
mod results_tests;
mod retreat_tests;
mod settings_tests;
mod storage_tests;
mod toast_tests;
