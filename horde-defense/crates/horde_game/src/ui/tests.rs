//! Event-driven tests of the shipped egui widgets. No OS window or user saves.
use super::*;
use crate::runner;
use bevy_egui::egui::{Event, FullOutput, Key, Modifiers, PointerButton, Pos2, RawInput, Vec2};

struct IsolatedFrontend {
    frontend: Frontend,
    directory: PathBuf,
}
impl IsolatedFrontend {
    fn new() -> Self {
        let mut frontend = test_support::test_frontend();
        let directory = frontend.save_path.parent().unwrap().to_path_buf();
        frontend.game = Game::default();
        // Existing control/layout cases exercise compatibility campaigns.
        frontend.game.campaign.legacy_features = true;
        // Existing layout/control fixtures explicitly exercise legacy11×30 arenas.
        // Fresh3×12 geometry is tested separately below.
        frontend
            .game
            .campaign
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        frontend
            .game
            .campaign
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        frontend.preview_survivor = SurvivorId::Rook;
        frontend.aim = [15.0, f32::from(frontend.game.campaign.lane_count()) * 0.5];
        frontend.settings = Settings::default();
        frontend.screen = Screen::Camp;
        frontend.return_screen = Screen::Camp;
        frontend.tab = Tab::World;
        frontend.mode = Mode::Personal;
        frontend.stage = 0;
        frontend.selected_country = "DEU".into();
        frontend.search.clear();
        frontend.message.clear();
        frontend.construction = false;
        frontend.fire_latched = false;
        frontend.rebind = None;
        frontend.binding_armed = false;
        frontend.pending = Input::default();
        frontend.pending_attack_aims = AttackAims::default();
        frontend.arena_rect = None;
        frontend.initialized = true;
        frontend.runtime = Runtime::default();
        frontend.save_pending = false;
        frontend.message_error = false;
        frontend.feedback = feedback::FeedbackState::default();
        frontend.feedback_revision = 0;
        frontend.practice_profile = false;
        frontend.profile_exists = false;
        Self {
            frontend,
            directory,
        }
    }
    fn unlock_trap(&mut self) {
        self.frontend.game.campaign.first_clear(0, Mode::Personal);
        self.frontend.game.campaign.finish_attempt();
        self.frontend.game.campaign.reward(25, 0, Mode::Personal);
        self.frontend
            .game
            .campaign
            .purchase_blueprint(StructureKind::Trap)
            .unwrap();
    }
    fn deploy(&mut self) {
        self.frontend
            .game
            .campaign
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
    fn frame(&self, f: &mut Frontend, view: View, events: Vec<Event>) -> FullOutput {
        self.frame_size(f, view, events, vec2(1440.0, 900.0))
    }
    fn frame_size(
        &self,
        f: &mut Frontend,
        view: View,
        events: Vec<Event>,
        size: Vec2,
    ) -> FullOutput {
        f.runtime.begin_frame(self.ctx.cumulative_frame_nr() as u32);
        let mut output = self.ctx.run_ui(
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
                View::History => history::show(ui, f),
                View::Feedback => {
                    egui::CentralPanel::default().show(ui, |ui| status(ui, f));
                }
            },
        );
        output.textures_delta.clear();
        runner::flush_for_test(f);
        output
    }
    fn click(&self, f: &mut Frontend, view: View, label: &str) {
        self.click_size(f, view, label, vec2(1440.0, 900.0));
    }
    fn click_size(&self, f: &mut Frontend, view: View, label: &str, size: Vec2) {
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
    fn key(&self, f: &mut Frontend, view: View, key: Key) {
        self.frame(f, view, vec![key_event(key, true)]);
        self.frame(f, view, vec![key_event(key, false)]);
    }
    fn feedback_at(&self, f: &mut Frontend, time: f64) -> FullOutput {
        let mut output = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                time: Some(time),
                ..RawInput::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| status(ui, f));
            },
        );
        output.textures_delta.clear();
        output
    }
    fn publish_combat(&self, f: &mut Frontend, events: Vec<Event>) {
        f.runtime.begin_frame(self.ctx.cumulative_frame_nr() as u32);
        let mut output = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                events,
                ..RawInput::default()
            },
            |ui| publish_controls(ui.ctx(), f, true, false),
        );
        output.textures_delta.clear();
    }
    fn combat_input(&self, f: &mut Frontend, events: Vec<Event>) -> Input {
        let mut input = Input::default();
        let mut output = self.ctx.run_ui(
            RawInput {
                events,
                ..RawInput::default()
            },
            |ui| {
                input = gather_combat_input(ui.ctx(), f);
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

#[test]
fn first_country_selection_immediately_starts_and_saves_a_live_personal_defense() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert_eq!(f.game.campaign.world.start.as_deref(), Some("DEU"));
    assert!(f.screen == Screen::Battle);
    assert!(!f.construction);
    let a = f.game.attempt.as_ref().unwrap();
    assert!(!a.paused);
    assert_eq!(a.tick, 0);
    assert_eq!(a.country_id.as_deref(), Some("DEU"));
    assert_eq!(a.mode, Mode::Personal);
    let restored = Game::load(&f.save_path).unwrap();
    assert_eq!(restored.attempt.as_ref().unwrap().tick, 0);
    assert!(!restored.attempt.as_ref().unwrap().paused);
}
#[test]
fn pointer_pause_and_resume_freeze_and_restart_authoritative_ticks() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Battle, "Pause");
    assert!(f.game.global_paused);
    assert!(f.game.attempt.as_ref().unwrap().paused);
    let frozen = serde_json::to_string(&f.game).unwrap();
    for _ in 0..30 {
        f.game.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&f.game).unwrap(), frozen);
    h.click(f, View::Battle, "Begin defense");
    assert!(!f.game.global_paused);
    assert!(!f.construction);
    for _ in 0..30 {
        f.game.tick(Input::default());
    }
    assert_eq!(f.game.attempt.as_ref().unwrap().tick, 30);
}
#[test]
fn npc_controls_do_not_take_a_permanent_column_from_the_fullscreen_arena() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        settings::configure(&h.ctx, scale);
        let f = &mut fixture.frontend;
        f.game.campaign.choose_start_country("DEU").unwrap();
        f.game.campaign.attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.screen = Screen::Battle;
        h.frame_size(f, View::Battle, vec![], size);
        let arena = f.arena_rect.unwrap();
        let canvas = h.ctx.content_rect();
        assert!(arena.left() >= canvas.left() && arena.right() <= canvas.right());
        assert!(
            arena.width() >= canvas.width() * 0.90,
            "Battlefield must fill the screen instead of reserving a sidebar"
        );
        assert!(
            arena.height() > 11.0 * 12.0,
            "All eleven lanes need a visible tactical rectangle"
        );
    }
}
#[test]
fn owned_defense_blueprints_allow_live_building_without_an_automatic_pause() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert!(!f.construction);
    assert!(!f.game.attempt.as_ref().unwrap().paused);
    h.click(f, View::Battle, "Build defenses");
    assert!(f.construction);
    assert!(!f.game.global_paused);
    assert!(!f.game.attempt.as_ref().unwrap().paused);
    assert!(Game::load(&f.save_path).is_ok());
}
#[test]
fn keyboard_pause_and_construction_stop_toggle_firing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.fire_latched = true;
    h.key(f, View::Battle, Key::Escape);
    assert!(f.game.global_paused);
    assert!(!f.fire_latched);
    h.key(f, View::Battle, Key::Escape);
    assert!(!f.game.global_paused);
    f.fire_latched = true;
    h.key(f, View::Battle, Key::B);
    assert!(f.construction);
    assert!(!f.game.global_paused);
    assert!(!f.fire_latched);
    h.key(f, View::Battle, Key::B);
    assert!(!f.construction);
    assert!(!f.game.global_paused);
}
#[test]
fn settings_from_main_menu_save_back_for_fresh_and_existing_profiles() {
    for existing in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        if existing {
            f.game.campaign.choose_start_country("DEU").unwrap();
            f.game.save(&f.save_path).unwrap();
            f.game = Game::load(&f.save_path).unwrap();
        }
        f.screen = Screen::Menu;
        h.click(f, View::Menu, "Settings");
        assert!(f.screen == Screen::Settings);
        assert!(f.return_screen == Screen::Menu);
        f.settings.master = 0.23;
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.message);
        assert!(!f.message_error);
        assert!(!f.save_pending);
        let saved = Game::load(&f.save_path).unwrap();
        assert_eq!(saved.campaign.world.start.is_some(), existing);
        assert!(saved.global_paused);
        assert_eq!(Settings::load(&f.settings_path).master, 0.23);
        h.click(f, View::Menu, "Settings");
        h.key(f, View::Settings, Key::Escape);
        assert!(f.screen == Screen::Menu, "{}", f.message);
    }
}
#[test]
fn settings_name_the_origin_and_offer_a_durable_route_to_main_menu() {
    for origin in [Screen::Battle, Screen::Camp] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        // A real reload is in progress; navigating must not reset its clock.
        f.game.attempt.as_mut().unwrap().ammo = 3;
        f.game.tick(Input {
            reload: true,
            ..Input::default()
        });
        assert!(f.game.attempt.as_ref().unwrap().reload_left > 0);
        f.screen = origin;
        f.fire_latched = true;
        h.click(f, View::Header, "Settings");
        assert!(f.screen == Screen::Settings);
        assert!(f.return_screen == origin);
        assert!(f.game.global_paused);
        assert!(!f.fire_latched);
        let paused = serde_json::to_value(&f.game).unwrap();
        let output = h.frame(f, View::Settings, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, f.settings_return_label()) })
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, "Save and return to main menu") })
        );
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.message);
        assert!(f.game.global_paused);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), paused);
        let saved = Game::load(&f.save_path).unwrap();
        assert_eq!(serde_json::to_value(&saved).unwrap(), paused);
        h.click(f, View::Menu, "Resume journey");
        assert!(f.screen == Screen::Battle);
        assert!(f.game.global_paused);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), paused);
    }
}
#[test]
fn settings_return_to_camp_restores_its_previous_world_pause() {
    for paused in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game.set_paused(paused);
        h.click(f, View::Header, "Settings");
        assert!(f.game.global_paused);
        h.click(f, View::Settings, "Save and return to camp");
        assert!(f.screen == Screen::Camp, "{}", f.message);
        // The successful save queues restoration through the runtime.
        runner::flush_for_test(f);
        assert_eq!(f.game.global_paused, paused);
    }
}
#[test]
fn settings_save_failure_is_visible_and_never_claims_a_return_or_success() {
    for fail_settings in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.screen = Screen::Menu;
        h.click(f, View::Menu, "Settings");
        // A file cannot be used as a parent directory, on any supported OS.
        std::fs::create_dir_all(f.save_path.parent().unwrap()).unwrap();
        let blocker = fixture.directory.join("blocked");
        std::fs::write(&blocker, b"blocked").unwrap();
        if fail_settings {
            f.settings_path = blocker.join("settings.json");
        } else {
            f.save_path = blocker.join("campaign.json");
        }
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Settings);
        assert!(f.message_error);
        assert!(!f.save_pending);
        assert!(f.message.starts_with("Save failed:"));
        let output = h.frame(f, View::Settings, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, &f.message) })
        );
        std::fs::remove_file(blocker).unwrap();
        f.save_path = fixture.directory.join("campaign.json");
        f.settings_path = fixture.directory.join("settings.json");
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.message);
        assert!(!f.message_error);
    }
}
#[test]
fn binding_capture_ignores_its_activation_click_and_swaps_duplicates() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Settings;
    h.click(f, View::Settings, "R");
    assert!(f.rebind == Some(Action::Reload));
    assert_eq!(f.settings.bindings[Action::Reload as usize], "R");
    h.frame(f, View::Settings, vec![]);
    h.key(f, View::Settings, Key::B);
    assert!(f.rebind.is_none());
    assert_eq!(f.settings.bindings[Action::Reload as usize], "B");
    assert_eq!(f.settings.bindings[Action::Build as usize], "R");
    h.click(f, View::Settings, "Save and return to camp");
    assert!(f.screen == Screen::Camp);
    let saved = Settings::load(&f.settings_path);
    assert_eq!(saved.bindings[Action::Reload as usize], "B");
    fixture.unlock_trap();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "B reload"))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "R build"))
    );
    h.key(f, View::Battle, Key::R);
    assert!(f.construction);
    assert!(!f.game.global_paused);
}
#[test]
fn mouse_reload_button_queues_a_legal_reload_without_firing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt.as_mut().unwrap().ammo = 3;
    h.click(f, View::Battle, "Reload");
    assert!(f.pending.reload);
    f.game.tick(f.pending.clone());
    let a = f.game.attempt.as_ref().unwrap();
    assert!(a.reload_left > 0);
    assert_eq!(a.ammo, 3);
    assert_eq!(a.shots, 0);
}
#[test]
fn legacy_body_points_are_spent_only_through_the_unified_upgrade_graph() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.reward(0, 20, Mode::Personal);
    f.game.campaign.choose_start_country("DEU").unwrap();
    let before = f.game.campaign.profile(Mode::Personal).personal.health;
    f.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        "body_vitality_1".into(),
    ));
    f.open_preparation(Tab::Skills);
    h.click(f, View::Results, "Learn skill · 1 legacy CP");
    assert_eq!(f.game.campaign.player.points, 0);
    assert_eq!(f.game.campaign.player.legacy_graph_points, 1);
    assert!(f.game.campaign.has_node("body_vitality_1"));
    assert_eq!(
        f.game.campaign.profile(Mode::Personal).personal.health,
        before + 5
    );
    let output = h.frame(f, View::Results, vec![]);
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Improve · 1 CP"))
    );
}

fn shape_has_text(shape: &egui::epaint::Shape, label: &str) -> bool {
    match shape {
        egui::epaint::Shape::Text(text) => text.galley.text().contains(label),
        egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|shape| shape_has_text(shape, label)),
        _ => false,
    }
}

#[test]
fn remapped_keyboard_fire_toggles_once_per_press_and_ignores_repeat_and_unbound_mouse() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.bindings[Action::Fire as usize] = "Space".into();
    f.settings.toggle_fire = true;
    f.arena_hover = false;
    assert!(h.combat_input(f, vec![key_event(Key::Space, true)]).fire);
    assert!(
        f.fire_latched,
        "The remapped keyboard fire works outside the arena"
    );
    assert!(
        h.combat_input(f, vec![]).fire,
        "Holding the key must retain the latch"
    );
    let repeat = Event::Key {
        key: Key::Space,
        physical_key: Some(Key::Space),
        pressed: true,
        repeat: true,
        modifiers: Modifiers::NONE,
    };
    assert!(
        h.combat_input(f, vec![repeat]).fire,
        "OS key-repeat cannot toggle firing off"
    );
    assert!(h.combat_input(f, vec![key_event(Key::Space, false)]).fire);
    assert!(!h.combat_input(f, vec![key_event(Key::Space, true)]).fire);
    assert!(!f.fire_latched);
    h.combat_input(f, vec![key_event(Key::Space, false)]);
    f.arena_hover = true;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "Unbound Mouse1 cannot toggle keyboard fire"
    );
    assert!(!h.combat_input(f, vec![pointer_event(false)]).fire);
}

#[test]
fn mouse_fire_toggle_is_restricted_to_the_arena_and_ui_clicks_preserve_its_state() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.toggle_fire = true;
    f.arena_hover = false;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "Clicking a UI panel cannot begin firing"
    );
    h.combat_input(f, vec![pointer_event(false)]);
    assert!(!f.fire_latched);
    f.arena_hover = true;
    assert!(h.combat_input(f, vec![pointer_event(true)]).fire);
    assert!(h.combat_input(f, vec![pointer_event(false)]).fire);
    f.arena_hover = false;
    assert!(
        h.combat_input(f, vec![pointer_event(true)]).fire,
        "A UI click cannot silently stop an active latch"
    );
    h.combat_input(f, vec![pointer_event(false)]);
    assert!(f.fire_latched);
    f.arena_hover = true;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "A second arena click stops firing"
    );
    assert!(!f.fire_latched);
}
fn pointer_event(pressed: bool) -> Event {
    Event::PointerButton {
        pos: pos2(200.0, 200.0),
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn item_sale_requires_review_and_confirmation_before_changing_the_wallet() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let id = f.game.campaign.add_item(4).unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    assert_eq!(f.pending_sale, Some(id));
    assert!(f.game.campaign.item(id).is_some());
    assert_eq!(f.game.campaign.coins, 0);
    h.click(f, View::Armory, "Keep item");
    assert!(f.pending_sale.is_none());
    assert!(f.game.campaign.item(id).is_some());
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    h.click(f, View::Armory, "Confirm item sale");
    assert!(f.game.campaign.item(id).is_none());
    assert_eq!(f.game.campaign.coins, price);
    assert!(f.pending_sale.is_none());
}
#[test]
fn preset_sale_warning_names_the_actual_reference_before_confirmation() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.coins = 65;
    f.game.campaign.purchase_node("quick_reload").unwrap();
    f.game.campaign.purchase_node("rifle_training").unwrap();
    let id = f.game.campaign.add_item(4).unwrap();
    f.game.campaign.equip(Mode::Personal, id).unwrap();
    f.game
        .campaign
        .save_preset("Patrol rifle".into(), Mode::Personal)
        .unwrap();
    let price = content().items[4].sell_value;
    h.click(f, View::Armory, &format!("Review sale · {price}"));
    let output = h.frame(f, View::Armory, vec![]);
    assert!(output.shapes.iter().any(|shape| shape_has_text(
        &shape.shape,
        "These presets will be incomplete: Patrol rifle"
    )));
    assert!(f.game.campaign.item(id).is_some());
    h.click(f, View::Armory, "Confirm item sale");
    assert_eq!(f.game.campaign.player.loadout[0], 5);
    assert!(f.game.campaign.apply_preset(1, Mode::Personal).is_err());
}

#[test]
fn large_armory_renders_only_visible_rows_and_filters_without_discarding_items() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..1000 {
        f.game.campaign.add_item(4).unwrap();
    }
    let output = h.frame(f, View::Armory, vec![]);
    let rows = output
        .shapes
        .iter()
        .filter(|shape| shape_has_text(&shape.shape, " #"))
        .count();
    assert!(
        rows > 0 && rows < 20,
        "Expected a bounded visible row set, got {rows}"
    );
    assert!(
        output.shapes.iter().any(|shape| shape_has_text(
            &shape.shape,
            &format!("{} #1005", content().items[4].name)
        ))
    );
    f.armory_search = "no-matching-equipment-name".into();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Showing 0 of 1005 items"))
    );
    assert_eq!(f.game.campaign.items.len(), 1005);
}

fn input_enemy(f: &mut Frontend, lane: u8, x: f32) {
    let mut attempt = f.game.attempt.take().unwrap();
    f.game.spawn_enemy(
        &mut attempt,
        horde_core::content::EnemyKind::Shambler,
        lane,
        false,
    );
    attempt.enemies.last_mut().unwrap().x = x;
    f.game.attempt = Some(attempt);
}
fn attack_pointer(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
#[test]
fn quick_fire_tap_survives_release_and_reaim_before_a_fixed_opportunity() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.arena_rect = Some(arena);
    f.arena_hover = true;
    f.aim = [8.0, 5.5];
    let pressed = art::world(arena, 8.0, 5.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
        ],
    );
    f.aim = [27.0, 0.5];
    let released = art::world(arena, 27.0, 0.5);
    h.publish_combat(
        f,
        vec![
            attack_pointer(pressed, PointerButton::Primary, false),
            Event::PointerMoved(released),
        ],
    );
    assert_eq!(f.game.attempt.as_ref().unwrap().shots, 0);
    runner::step(f);
    let attempt = f.game.attempt.as_ref().unwrap();
    assert_eq!(attempt.shots, 1);
    assert_eq!(
        attempt.hits, 1,
        "The released tap must retain its original target direction"
    );
    for _ in 0..60 {
        runner::step(f);
    }
    assert_eq!(f.game.attempt.as_ref().unwrap().shots, 1);
}
#[test]
fn one_render_frame_click_and_motion_uses_the_mouse_press_event_position() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.arena_rect = Some(arena);
    f.arena_hover = true;
    f.aim = [27.0, 0.5];
    let pressed = art::world(arena, 8.0, 5.5);
    let moved = art::world(arena, 27.0, 0.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
            attack_pointer(pressed, PointerButton::Primary, false),
            Event::PointerMoved(moved),
        ],
    );
    runner::step(f);
    let attempt = f.game.attempt.as_ref().unwrap();
    assert_eq!(attempt.shots, 1);
    assert_eq!(attempt.hits, 1);
}
#[test]
fn quick_outer_lane_melee_tap_keeps_its_selected_lane_after_release_and_motion() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 0, horde_core::sim::DEFENSE_LINE);
    input_enemy(f, 0, 2.2);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.arena_rect = Some(arena);
    f.arena_hover = true;
    f.aim = [1.5, 0.5];
    let pressed = art::world(arena, 1.5, 0.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Secondary, true),
        ],
    );
    f.aim = [1.5, 10.5];
    h.publish_combat(
        f,
        vec![
            attack_pointer(pressed, PointerButton::Secondary, false),
            Event::PointerMoved(art::world(arena, 1.5, 10.5)),
        ],
    );
    let mut strikes = 0;
    for tick in 0..150 {
        runner::step(f);
        let attempt = f.game.attempt.as_ref().unwrap();
        for event in attempt.events.iter().filter(|event| {
            event.tick == attempt.tick && event.kind == horde_core::sim::EventKind::Melee
        }) {
            strikes += 1;
            assert_eq!(event.to[1], 0.5);
        }
        if tick == 0 {
            assert_eq!(
                strikes, 1,
                "The tap must strike the original outer contact lane"
            );
        }
    }
    assert_eq!(
        strikes, 1,
        "A released tap cannot repeat after melee cooldown"
    );
}
#[test]
fn sidebar_strike_button_uses_the_actual_selected_outer_contact_lane() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 0, horde_core::sim::DEFENSE_LINE);
    f.aim = [1.5, 0.5];
    h.click(f, View::Battle, "Strike contact lane");
    assert!(f.pending.melee);
    h.publish_combat(f, vec![]);
    runner::step(f);
    let attempt = f.game.attempt.as_ref().unwrap();
    let event = attempt
        .events
        .iter()
        .find(|event| event.kind == horde_core::sim::EventKind::Melee)
        .expect("A legal targeted strike should resolve");
    assert_eq!(event.to[1], 0.5);
}
#[test]
fn rapid_remapped_fire_taps_coalesce_without_bypassing_the_weapon_cadence() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    f.settings.bindings[Action::Fire as usize] = "Space".into();
    f.aim = [8.0, 5.5];
    for _ in 0..8 {
        h.publish_combat(f, vec![key_event(Key::Space, true)]);
        h.publish_combat(f, vec![key_event(Key::Space, false)]);
    }
    runner::step(f);
    assert_eq!(f.game.attempt.as_ref().unwrap().shots, 1);
    for _ in 0..4 {
        h.publish_combat(f, vec![key_event(Key::Space, true)]);
        h.publish_combat(f, vec![key_event(Key::Space, false)]);
        runner::step(f);
    }
    assert_eq!(f.game.attempt.as_ref().unwrap().shots, 1);
    for _ in 0..60 {
        runner::step(f);
    }
    assert_eq!(
        f.game.attempt.as_ref().unwrap().shots,
        1,
        "Unready taps cannot become a deferred firing burst"
    );
}

#[test]
fn camp_escape_and_footer_control_pause_and_resume_all_parallel_work() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.attempts = 1;
    f.game.campaign.coins = 500;
    f.game.campaign.first_clears = [true; 5];
    f.game.campaign.unlocked = 5;
    for id in ["DEU", "FRA"] {
        f.game.campaign.world.cleared.push(id.into());
        f.game
            .campaign
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let recruit = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "DEU")
        .unwrap();
    f.game.deploy_survivor(recruit, "POL", 0).unwrap();
    h.key(f, View::Camp, Key::Escape);
    assert!(f.game.global_paused);
    let frozen = serde_json::to_value(&f.game).unwrap();
    for _ in 0..60 {
        f.game.tick(Input::default());
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), frozen);
    h.click(f, View::Camp, "Resume world clock");
    assert!(!f.game.global_paused);
    f.game.tick(Input::default());
    assert!(f.game.battles[0].game.attempt.as_ref().unwrap().tick > 0);
}
#[test]
fn camp_pause_binding_does_not_steal_text_from_country_search() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.bindings[Action::Pause as usize] = "P".into();
    h.frame(f, View::Camp, vec![]);
    h.ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("country_search")));
    h.frame(
        f,
        View::Camp,
        vec![key_event(Key::P, true), Event::Text("p".into())],
    );
    assert_eq!(f.search, "p");
    assert!(!f.game.global_paused);
}
#[test]
fn locked_construction_stays_out_of_the_first_battle_and_explains_the_gate() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .all(|shape| text_position(&shape.shape, "Build defenses").is_none())
    );
    let supply = f.game.attempt.as_ref().unwrap().supply;
    h.key(f, View::Battle, Key::B);
    assert!(!f.game.global_paused);
    assert!(!f.construction);
    assert_eq!(f.game.attempt.as_ref().unwrap().supply, supply);
    assert!(f.message.contains("first boss clear"));
}
#[test]
fn starting_country_action_is_above_the_fold_at_the_minimum_scaled_window() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    settings::configure(&h.ctx, 1.35);
    let f = &mut fixture.frontend;
    f.settings.scale = 1.35;
    let output = h.frame_size(f, View::Camp, vec![], vec2(1050.0, 700.0));
    let start = output
        .shapes
        .iter()
        .find_map(|shape| text_position(&shape.shape, "Start journey"))
        .expect("The first-session primary action must be painted");
    let footer = output
        .shapes
        .iter()
        .find_map(|shape| text_position(&shape.shape, "Back to survivor"))
        .unwrap();
    assert!(h.ctx.content_rect().contains(start));
    assert!(start.y < footer.y - 20.0);
}
#[test]
fn feedback_expires_success_but_keeps_errors_and_never_changes_a_battle() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.reduced_effects = true;
    let before = serde_json::to_value(&f.game).unwrap();
    let base = h.ctx.input(|input| input.time) + 1.0;
    f.message = "Equipment changed.".into();
    f.feedback_revision += 1;
    let first = h.feedback_at(f, base);
    assert!(
        first
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Equipment changed."))
    );
    let expired = h.feedback_at(f, base + 6.0);
    assert!(
        expired
            .shapes
            .iter()
            .all(|shape| !shape_has_text(&shape.shape, "Equipment changed."))
    );
    f.message = "Save failed. Retry saving before closing.".into();
    f.message_error = true;
    f.feedback_revision += 1;
    h.feedback_at(f, base + 7.0);
    let error = h.feedback_at(f, base + 40.0);
    assert!(
        error
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Save failed. Retry saving"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
#[test]
fn field_guidance_defaults_on_for_old_settings_and_is_saved_when_changed() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert!(settings.show_guidance);
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.settings.show_guidance = false;
    f.request_save(SaveIntent::Manual);
    runner::flush_for_test(f);
    assert!(!Settings::load(&f.settings_path).show_guidance);
}
#[test]
fn hud_reports_actual_reload_and_outer_lane_damage_without_advancing_them() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt.as_mut().unwrap().ammo = 3;
    f.game.tick(Input {
        reload: true,
        ..Input::default()
    });
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Reload ·"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    input_enemy(f, 10, horde_core::sim::DEFENSE_LINE);
    for _ in 0..180 {
        f.game.tick(Input::default());
        if f.game.attempt.as_ref().unwrap().hp < f.game.attempt.as_ref().unwrap().max_hp {
            break;
        }
    }
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "HIT · LANE 11"))
    );
}
#[test]
fn results_keep_rewards_banked_and_open_the_actual_recovered_equipment() {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign.coins = 75;
    fixture
        .frontend
        .game
        .campaign
        .purchase_node("steady_hands")
        .unwrap();
    fixture
        .frontend
        .game
        .campaign
        .purchase_node("stopping_power")
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let mut attempt = f.game.attempt.take().unwrap();
    attempt.boss_spawned = true;
    f.game
        .spawn_enemy(&mut attempt, horde_core::content::EnemyKind::Brute, 5, true);
    let boss = attempt.enemies.last_mut().unwrap();
    boss.x = 8.0;
    boss.hp = 1;
    f.game.attempt = Some(attempt);
    f.game.tick(Input {
        aim: [8.0, 5.5],
        fire: true,
        ..Input::default()
    });
    assert!(f.game.attempt.is_none());
    assert!(f.game.last_result.as_ref().unwrap().loot > 0);
    f.screen = Screen::Results;
    let coins = f.game.campaign.coins;
    for _ in 0..4 {
        let output = h.frame(f, View::Results, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| shape_has_text(&shape.shape, "Already banked:"))
        );
    }
    assert_eq!(f.game.campaign.coins, coins);
    h.click(f, View::Results, "Equipment");
    assert!(f.screen == Screen::Camp);
    assert!(f.tab == Tab::Armory);
    assert!(f.selected_item.is_some());
    assert_eq!(f.game.campaign.coins, coins);
}
#[test]
fn ambience_selection_is_a_read_only_projection_of_actual_battle_and_pause() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let before = serde_json::to_value(&f.game).unwrap();
    let active = f.ambience_target();
    assert_eq!(active.scene.mode, Some(Mode::Personal));
    assert_eq!(
        active.context,
        crate::atmosphere_audio::AudioContext::Battle
    );
    f.screen = Screen::Settings;
    assert_eq!(
        f.ambience_target().context,
        crate::atmosphere_audio::AudioContext::Paused
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
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
#[test]
fn dismissing_an_error_restores_neutral_feedback_without_claiming_success() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.message = "An operation failed. Retry when ready.".into();
    f.message_error = true;
    f.feedback_revision += 1;
    h.click(f, View::Feedback, "Dismiss");
    let output = h.frame(f, View::Feedback, vec![]);
    let color = output
        .shapes
        .iter()
        .find_map(|shape| text_color(&shape.shape, "Coins and equipment bank as they are earned."))
        .expect("Neutral status should replace the dismissed acknowledgement");
    assert_eq!(color, MUTED);
    assert!(
        f.message_error,
        "Dismissal must not pretend the failed operation succeeded"
    );
}
#[test]
fn camp_observation_speed_never_offers_a_hidden_speedup_to_personal_combat() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Camp;
    f.game.set_paused(true);
    f.game.strategy.peak_freed = 4;
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Camp, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Personal defense · 1×"))
    );
    assert!(
        output
            .shapes
            .iter()
            .all(|shape| text_position(&shape.shape, "4×").is_none())
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn reduced_review_override_is_isolated_and_cannot_change_authority_or_saved_preferences() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let authority = serde_json::to_value(&f.game).unwrap();
    for (scenario, practice, force, expected) in [
        (false, false, true, false),
        (true, true, true, false),
        (true, false, false, false),
        (true, false, true, true),
    ] {
        f.settings.reduced_effects = false;
        let mut settings = f.settings.clone();
        settings.reduced_effects = expected;
        review::apply_reduced_override(f, scenario, practice, force);
        assert_eq!(
            serde_json::to_value(&f.settings).unwrap(),
            serde_json::to_value(settings).unwrap()
        );
        assert_eq!(serde_json::to_value(&f.game).unwrap(), authority);
    }
    f.settings.reduced_effects = true;
    review::apply_reduced_override(f, false, false, false);
    assert!(
        f.settings.reduced_effects,
        "An ordinary profile keeps its existing accessibility preference"
    );
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
#[test]
fn long_npc_action_is_one_elided_hud_line_without_changing_height_or_authority() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        settings::configure(&h.ctx, scale);
        let f = &mut fixture.frontend;
        f.settings.scale = scale;
        f.game.campaign.choose_start_country("DEU").unwrap();
        f.game.campaign.attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.screen = Screen::Battle;
        f.game.attempt.as_mut().unwrap().decision = "Aim at Runner in lane 5".into();
        // Egui's first complete panel/legend pass settles its cached sizing
        // (the initial arena differs by one 15-point legend row even without
        // an action change). Compare two established layouts, as native
        // captures do after warmup; none of the geometry assertions are relaxed.
        h.frame_size(f, View::Battle, vec![], size);
        h.frame_size(f, View::Battle, vec![], size);
        let short_arena = f.arena_rect.unwrap();
        let long = "Attack: shoot Armored survivor in lane 11 — close engagement; preparing a fully aimed follow-up when the shared defense line is safe";
        f.game.attempt.as_mut().unwrap().decision = long.into();
        let before = serde_json::to_value(&f.game).unwrap();
        let output = h.frame_size(f, View::Battle, vec![], size);
        let arena = f.arena_rect.unwrap();
        let text = output
            .shapes
            .iter()
            .find_map(|shape| hud_text(&shape.shape, long, arena))
            .expect("The long current action must remain a visible HUD readout");
        assert_eq!(
            text.galley.rows.len(),
            1,
            "An action cannot wrap under its progress stroke"
        );
        assert!(
            text.galley.elided,
            "The native label must indicate omitted text with ellipsis"
        );
        assert!(text.galley.rect.height() <= 20.0);
        assert!((arena.top() - short_arena.top()).abs() < 0.01);
        assert!(
            (arena.height() - short_arena.height()).abs() < 0.01,
            "Short action arena {:?}; long action arena {:?}",
            short_arena,
            arena
        );
        assert_eq!(
            f.game.attempt.as_ref().unwrap().decision,
            long,
            "The complete sidebar/tooltip text must remain available"
        );
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}

#[test]
fn preparation_keeps_the_planned_replay_and_defender_without_creating_an_attempt() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.attempts = 1;
    f.game.campaign.world.country_levels.insert("DEU".into(), 2);
    f.mode = Mode::Npc;
    f.stage = 0;
    f.open_preparation(Tab::Armory);
    let before = serde_json::to_value(&f.game).unwrap();
    f.open_preparation(Tab::Briefing);
    h.frame(f, View::Camp, vec![]);
    assert!(f.tab == Tab::Briefing);
    assert_eq!(f.stage, 0);
    assert!(f.mode == Mode::Npc);
    assert!(f.game.attempt.is_none());
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
#[test]
fn victory_stops_at_upgrades_until_continue_deploys_the_next_defense_without_repaying() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result.as_mut().unwrap().outcome = Outcome::Victory;
    f.game.campaign.world.country_levels.insert("DEU".into(), 1);
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let before = serde_json::to_value(&f.game).unwrap();
    let coins = f.game.campaign.coins;
    let records = f.game.campaign.run_history.len();
    for _ in 0..3 {
        h.frame(f, View::Results, vec![]);
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    assert!(f.game.attempt.is_none());
    h.click(f, View::Results, "Next defense");
    assert!(f.screen == Screen::Battle);
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!(a.stage, 1);
    assert_eq!(a.tick, 0);
    assert!(!a.paused);
    assert_eq!(f.game.campaign.coins, coins);
    assert_eq!(f.game.campaign.run_history.len(), records);
}

#[test]
fn recommended_market_item_is_visible_and_purchased_only_after_explicit_input() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.first_clear(0, Mode::Personal);
    f.game.campaign.coins = 200;
    let index = content()
        .items
        .iter()
        .position(|i| i.tier == 1 && i.slot == Slot::Ranged)
        .unwrap();
    f.improvement_focus = Some(horde_core::campaign::RecommendationAction::BuyEquipment(
        index,
    ));
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(output.shapes.iter().any(|s| shape_has_text(
        &s.shape,
        &format!("Recommended: {}", content().items[index].name)
    )));
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    let count = f.game.campaign.items.len();
    let coins = f.game.campaign.coins;
    h.click(
        f,
        View::Armory,
        &format!("Buy · {} coins", content().items[index].buy_value),
    );
    assert_eq!(f.game.campaign.items.len(), count + 1);
    assert_eq!(
        f.game.campaign.coins,
        coins - content().items[index].buy_value
    );
    assert!(f.improvement_focus.is_none());
}
#[test]
fn continuing_from_the_menu_preserves_a_saved_world_pause() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.set_paused(true);
    f.screen = Screen::Menu;
    let before = serde_json::to_value(&f.game).unwrap();
    f.resume();
    runner::flush_for_test(f);
    assert!(f.screen == Screen::Camp);
    assert!(f.game.global_paused);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
#[test]
fn a_personal_field_remains_returnable_from_operations_without_reset_or_resume() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.tick(Input::default());
    f.game.set_paused(true);
    f.screen = Screen::Camp;
    f.tab = Tab::Operations;
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Camp, "Return to active defense");
    assert!(f.screen == Screen::Battle);
    assert!(f.game.attempt.as_ref().unwrap().paused);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn leaving_a_debrief_keeps_strategic_time_paused_until_explicitly_resumed() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.screen = Screen::Results;
    f.game.global_paused = false;
    let mut expected = f.game.clone();
    expected.set_paused(true);
    f.open_campaign();
    runner::flush_for_test(f);
    assert!(f.screen == Screen::Camp && f.tab == Tab::World);
    assert!(f.game.global_paused);
    assert_eq!(
        serde_json::to_value(&f.game).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn battle_management_popup_cannot_leak_reload_or_fire_input() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt.as_mut().unwrap().ammo = 4;
    h.click(f, View::Battle, "Management");
    h.key(f, View::Battle, Key::R);
    assert!(!f.pending.reload && !f.pending.fire);
    assert_eq!(f.game.attempt.as_ref().unwrap().ammo, 4);
    assert_eq!(f.game.attempt.as_ref().unwrap().shots, 0);
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
#[test]
fn npc_resolution_popup_and_observation_controls_fit_at_both_sizes() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        f.game.campaign.choose_start_country("DEU").unwrap();
        f.game.campaign.attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.game.set_paused(false);
        input_enemy(f, 5, 8.0);
        for _ in 0..600 {
            f.game.tick(Input::default());
            if f.game
                .attempt
                .as_ref()
                .unwrap()
                .log
                .iter()
                .any(|s| s.contains("3d6"))
            {
                break;
            }
        }
        assert!(
            f.game
                .attempt
                .as_ref()
                .unwrap()
                .log
                .iter()
                .any(|s| s.contains("3d6"))
        );
        f.game.set_paused(true);
        f.screen = Screen::Battle;
        let before = serde_json::to_value(&f.game).unwrap();
        h.frame_size(f, View::Battle, vec![], size);
        h.frame_size(f, View::Battle, vec![], size);
        let base = h.frame_size(f, View::Battle, vec![], size);
        for label in ["Observe", "4×", "Survivor"] {
            let r = base
                .shapes
                .iter()
                .find_map(|s| labeled_bounds(&s.shape, label))
                .unwrap_or_else(|| panic!("Missing visible {label}"));
            assert!(
                h.ctx.content_rect().contains_rect(r),
                "{label} escaped viewport {r:?}"
            );
        }
        h.click_size(f, View::Battle, "Survivor", size);
        for expanded in [false, true] {
            if expanded {
                h.click_size(f, View::Battle, "Latest resolution", size);
            }
            let output = h.frame_size(f, View::Battle, vec![], size);
            let area = h.ctx.content_rect();
            for label in ["Current survivor decision", "Latest resolution"] {
                let r = output
                    .shapes
                    .iter()
                    .find_map(|s| labeled_bounds(&s.shape, label))
                    .unwrap_or_else(|| panic!("Missing {label}, expanded {expanded}"));
                assert!(
                    area.contains_rect(r),
                    "{label} escaped popup viewport {r:?}"
                );
            }
        }
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}
#[test]
fn failed_outbreak_front_change_keeps_operations_and_the_requested_country_context() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.screen = Screen::Camp;
    f.tab = Tab::Operations;
    f.game.campaign.world.cleared.push("FRA".into());
    f.game
        .campaign
        .world
        .country_levels
        .insert("FRA".into(), world().country("FRA").unwrap().level_count());
    f.game.strategy.peak_freed = 1;
    f.game.strategy.next_outbreak_id = 2;
    f.game
        .strategy
        .outbreaks
        .push(horde_core::strategy::Outbreak {
            id: 1,
            country_id: "FRA".into(),
            started_tick: 0,
            deadline_tick: 9999,
            strength: 2,
            enemy: horde_core::rules::InjuryState::new(9, 10),
            outcome: horde_core::strategy::OutbreakOutcome::Active,
            ready_turns: 0,
        });
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Camp, "Plan outbreak response");
    assert!(f.screen == Screen::Camp && f.tab == Tab::Operations);
    assert_eq!(f.selected_country, "FRA");
    assert_eq!(
        f.game.attempt.as_ref().unwrap().country_id.as_deref(),
        Some("DEU")
    );
    assert!(f.message_error);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn immersive_skill_choices_and_purchase_remain_visible_in_the_compact_first_view() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.coins = 200;
    f.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        "piercing".into(),
    ));
    f.open_preparation(Tab::Skills);
    runner::flush_for_test(f);
    settings::configure(&h.ctx, 1.35);
    f.settings.scale = 1.35;
    let size = vec2(1050., 700.);
    let before = serde_json::to_value(&f.game).unwrap();
    h.frame_size(f, View::Results, vec![], size);
    h.frame_size(f, View::Results, vec![], size);
    let output = h.frame_size(f, View::Results, vec![], size);
    let area = h.ctx.content_rect();
    for text in ["Controlled penetration", "Learn skill · 55 coins"] {
        let p = output
            .shapes
            .iter()
            .find_map(|s| text_position(&s.shape, text))
            .unwrap_or_else(|| panic!("Required upgrade detail not visible: {text}"));
        assert!(area.contains(p));
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn selected_grenade_uses_one_press_cell_and_never_repeats_from_hold_or_fire_toggle() {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign.coins = 100;
    fixture
        .frontend
        .game
        .campaign
        .purchase_node("grenade_training")
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Battle, "Grenade");
    assert_eq!(f.attack_selection, battle::AttackSelection::Grenade);
    f.fire_latched = true;
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.arena_rect = Some(arena);
    f.arena_hover = true;
    let pressed = art::world(arena, 19.25, 3.75);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
        ],
    );
    f.aim = [28.0, 9.0];
    h.publish_combat(f, vec![Event::PointerMoved(art::world(arena, 28.0, 9.0))]);
    runner::step(f);
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!(a.ordnance.spent[0], 1);
    assert_eq!(a.ordnance.projectiles[0].cell, [19, 3]);
    assert_eq!(a.shots, 0);
    for _ in 0..180 {
        runner::step(f);
    }
    assert_eq!(f.game.attempt.as_ref().unwrap().ordnance.spent[0], 1);
    assert!(!f.fire_latched);
}
#[test]
fn retreat_opens_retained_reward_skill_tree_and_retry_starts_live() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.reward(25, 0, Mode::Personal);
    h.click(f, View::Battle, "Retreat & train");
    assert!(f.screen == Screen::Results);
    assert!(f.game.attempt.is_none());
    assert_eq!(f.game.campaign.coins, 25);
    h.click(f, View::Results, "Try again");
    assert!(f.screen == Screen::Battle);
    assert!(!f.game.attempt.as_ref().unwrap().paused);
}

#[test]
fn a_fresh_campaign_previews_then_confirms_survivor_before_selecting_a_country() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game = Game::default();
    f.screen = Screen::Menu;
    let original = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Menu, "New game");
    assert!(f.screen == Screen::SurvivorSelection);
    h.click(f, View::SurvivorSelection, "June");
    assert_eq!(f.preview_survivor, SurvivorId::June);
    assert_eq!(
        serde_json::to_value(&f.game).unwrap(),
        original,
        "Preview cannot spend or change authoritative identity"
    );
    h.click(f, View::SurvivorSelection, "Continue as June");
    assert_eq!(f.game.campaign.selected_survivor, Some(SurvivorId::June));
    assert!(f.screen == Screen::Camp && f.tab == Tab::World);
    assert_eq!(f.game.campaign.coins, 0);
    assert_eq!(f.game.campaign.npc.name, "Mara Vale");
    let saved = Game::load(&f.save_path).unwrap();
    assert_eq!(saved.campaign.selected_survivor, Some(SurvivorId::June));
    h.click(f, View::Dossier, "Start journey");
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!(a.profile.appearance, Appearance::Female);
    assert_eq!(
        a.profile.weapon.class,
        horde_core::weapons::WeaponClass::Pistol
    );
    assert!(!a.paused);
}
#[test]
fn both_survivor_choices_and_confirmation_fit_the_minimum_scaled_view() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game = Game::default();
    f.screen = Screen::SurvivorSelection;
    let size = vec2(1050.0, 700.0) / 1.35;
    for choice in SurvivorId::ALL {
        f.preview_survivor = choice;
        let output = h.frame_size(f, View::SurvivorSelection, vec![], size);
        let viewport = h.ctx.content_rect();
        for label in [
            "Rook".to_owned(),
            "June".to_owned(),
            format!("Continue as {}", choice.name()),
            "Back to main menu".to_owned(),
        ] {
            let point = output
                .shapes
                .iter()
                .find_map(|shape| text_position(&shape.shape, &label))
                .unwrap_or_else(|| panic!("Missing first-view survivor action: {label}"));
            assert!(
                viewport.contains(point),
                "{label} must fit the selection viewport"
            );
        }
    }
}
#[test]
fn leveled_equipment_ui_uses_the_same_price_and_comparison_as_the_transaction() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.coins = 1000;
    f.game.campaign.purchase_node("quick_reload").unwrap();
    f.game.campaign.purchase_node("rifle_training").unwrap();
    let rifle = f.game.campaign.buy_item(4).unwrap();
    f.game.campaign.equip(Mode::Personal, rifle).unwrap();
    // Real replay victories raise threat; merely marking country clears no
    // longer stands in for completed runs under the current progression rule.
    for _ in 0..6 {
        f.game.deploy_country_stage(0, Mode::Personal).unwrap();
        let mut attempt = f.game.attempt.take().unwrap();
        attempt.boss_spawned = true;
        let lane = attempt.lane_count / 2;
        f.game.spawn_enemy(
            &mut attempt,
            horde_core::content::EnemyKind::Brute,
            lane,
            true,
        );
        let target = [8.0, attempt.defender_position()[1]];
        let boss = attempt.enemies.last_mut().unwrap();
        boss.x = target[0];
        boss.hp = 1;
        f.game.attempt = Some(attempt);
        f.game.tick(Input {
            aim: target,
            fire: true,
            ..Input::default()
        });
        assert!(f.game.attempt.is_none());
        assert_eq!(
            f.game.last_result.as_ref().unwrap().outcome,
            Outcome::Victory
        );
    }
    assert_eq!(f.game.campaign.victories, 6);
    f.game.campaign.coins = 1000;
    let definition = content()
        .items
        .iter()
        .position(|i| i.slot == Slot::Ranged && i.tier == 1)
        .unwrap();
    let price = f.game.campaign.buy_price(definition).unwrap();
    let id = f.game.campaign.buy_item(definition).unwrap();
    let item_level = f.game.campaign.item(id).unwrap().level;
    assert!(item_level > 1);
    assert_eq!(f.game.campaign.coins, 1000 - price);
    let effective = f.game.campaign.effective_item(id).unwrap();
    assert!(effective.personal.damage > content().items[definition].personal.damage);
    f.selected_item = Some(id);
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, &format!("Lv {}", item_level)))
    );
    assert_eq!(
        f.game
            .campaign
            .compare_item(Mode::Personal, id)
            .unwrap()
            .after
            .damage,
        effective.personal.damage
    );
}

#[test]
fn quality_names_and_filter_sort_use_owned_quality_without_awarding_anything() {
    use horde_core::quality::Quality;
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let definition = content()
        .items
        .iter()
        .position(|i| i.slot == Slot::Ranged && i.tier == 1)
        .unwrap();
    let poor = f
        .game
        .campaign
        .add_item_with_quality(definition, 2, Some(Quality::Poor))
        .unwrap();
    let legendary = f
        .game
        .campaign
        .add_item_with_quality(definition, 2, Some(Quality::Legendary))
        .unwrap();
    assert!(
        f.game
            .campaign
            .effective_item(legendary)
            .unwrap()
            .personal
            .damage
            > f.game
                .campaign
                .effective_item(poor)
                .unwrap()
                .personal
                .damage
    );
    f.armory_sort = armory::ArmorySort::Quality;
    f.armory_quality = Some(Quality::Legendary);
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Armory, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, &format!("#{}", legendary)))
    );
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, &format!("#{}", poor)))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Legendary"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
#[test]
fn npc_critical_state_stays_explicit_at_one_and_four_times_observation_speed() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    input_enemy(f, 5, 8.0);
    let a = f.game.attempt.as_mut().unwrap();
    let enemy = a.enemies.last_mut().unwrap();
    enemy.hp = -2;
    enemy.injury.hp = -2;
    assert_eq!(
        enemy.condition(Mode::Npc),
        horde_core::sim::EnemyCondition::Critical
    );
    for speed in [1, 4] {
        f.speed = speed;
        let before = serde_json::to_value(&f.game).unwrap();
        let output = h.frame(f, View::Battle, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "CRIT · FIGHTING"))
        );
        let mut dossier = h.ctx.run_ui(RawInput::default(), |ui| {
            crate::inspector::dossier(ui, f.game.attempt.as_ref().unwrap(), [8.0, 5.5])
        });
        assert!(
            dossier
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "HP -2"))
        );
        assert!(
            dossier
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "CRITICAL · still fighting"))
        );
        dossier.textures_delta.clear();
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}

#[test]
fn visible_quality_drop_receipts_never_grant_the_item_or_coins_twice() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let quality = horde_core::quality::Quality::Rare;
    let id = f
        .game
        .campaign
        .add_item_with_quality(4, 2, Some(quality))
        .unwrap();
    let a = f.game.attempt.as_mut().unwrap();
    a.loot_events.push(horde_core::quality::LootEvent {
        item_id: id,
        definition: 4,
        level: 2,
        quality,
        name: content().items[4].name.clone(),
        tick: a.tick,
        at: [20.0, 5.5],
    });
    let before = serde_json::to_value(&f.game).unwrap();
    for _ in 0..3 {
        let output = h.frame(f, View::Battle, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Last loot · already banked"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Rare"))
        );
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    assert_eq!(
        f.game
            .campaign
            .items
            .iter()
            .filter(|item| item.id == id)
            .count(),
        1
    );
}

#[test]
fn grown_lane_count_drives_outer_row_construction_and_grenade_cells() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.player.level = f.game.campaign.player_level_cap();
    f.game.campaign.player.points = f.game.campaign.player.level - 1;
    f.game.campaign.player.xp = 0;
    f.game.campaign.coins = 100;
    f.game.campaign.purchase_node("grenade_training").unwrap();
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.screen = Screen::Battle;
    assert_eq!(f.game.attempt.as_ref().unwrap().lane_count, 17);
    h.key(f, View::Battle, Key::B);
    assert!(f.construction);
    h.frame(f, View::Battle, vec![]);
    let rect = f.arena_rect.unwrap();
    let projection = art::ArenaProjection::new(rect, 17);
    let point = projection.world(4.2, 16.5);
    h.frame(
        f,
        View::Battle,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, true),
        ],
    );
    h.frame(
        f,
        View::Battle,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, false),
        ],
    );
    assert!(
        f.game
            .attempt
            .as_ref()
            .unwrap()
            .structures
            .iter()
            .any(|structure| structure.lane == 16)
    );
    f.construction = false;
    f.attack_selection = battle::AttackSelection::Grenade;
    h.frame(f, View::Battle, vec![]);
    let projection = art::ArenaProjection::new(f.arena_rect.unwrap(), 17);
    let point = projection.world(20.5, 16.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, true),
        ],
    );
    runner::step(f);
    assert_eq!(
        f.game.attempt.as_ref().unwrap().ordnance.projectiles[0].cell,
        [20, 16]
    );
    assert_eq!(
        f.game.attempt.as_ref().unwrap().defender_position(),
        [0.5, 8.5]
    );
}
#[test]
fn active_legacy_geometry_stays_frozen_when_character_development_opens_more_lanes() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert_eq!(f.game.attempt.as_ref().unwrap().lane_count, 11);
    f.game.campaign.player.level = f.game.campaign.player_level_cap();
    f.game.campaign.player.points = f.game.campaign.player.level - 1;
    f.game.campaign.player.xp = 0;
    assert_eq!(f.game.campaign.lane_count(), 17);
    h.frame(f, View::Battle, vec![]);
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!(a.lane_count, 11);
    assert_eq!(a.defender_position(), [0.5, 5.5]);
    let projection = art::ArenaProjection::for_attempt(f.arena_rect.unwrap(), a);
    assert_eq!(
        projection.world(0.5, 5.5).y,
        f.arena_rect.unwrap().center().y
    );
}
#[test]
fn character_max_has_an_explicit_full_bar_and_next_deployment_lane_preview() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.player.level = f.game.campaign.player_level_cap();
    f.game.campaign.player.points = f.game.campaign.player.level - 1;
    f.game.campaign.player.xp = 0;
    f.open_preparation(Tab::Character);
    let output = h.frame(f, View::Camp, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "MAX · level"))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Next deployment: 17 lanes"))
    );
}

#[test]
fn difficulty_apply_is_a_validated_saved_command_and_keeps_active_geometry_frozen() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let original = f.game.attempt.as_ref().unwrap().balance.clone();
    assert_eq!(
        (
            f.game.attempt.as_ref().unwrap().lane_count,
            f.game.attempt.as_ref().unwrap().width_yards
        ),
        (11, 30.0)
    );
    f.open_settings();
    runner::flush_for_test(f);
    let active_snapshot = serde_json::to_value(f.game.attempt.as_ref().unwrap()).unwrap();
    h.click(f, View::Settings, "Normal");
    assert_eq!(f.game.campaign.balance_settings, original);
    h.click(f, View::Settings, "Apply to next run");
    assert_eq!(
        f.game.campaign.balance_settings,
        BalanceSettings::for_preset(BalancePreset::Normal)
    );
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (11, 30.0));
    assert_eq!(a.balance, original);
    assert!(a.paused);
    assert_eq!(serde_json::to_value(a).unwrap(), active_snapshot);
    h.click(f, View::Settings, "Save and return to defense");
    let restored = Game::load(&f.save_path).unwrap();
    assert_eq!(
        restored.campaign.balance_settings,
        f.game.campaign.balance_settings
    );
    assert_eq!(restored.attempt.as_ref().unwrap().width_yards, 30.0);
    let before = serde_json::to_value(&f.game).unwrap();
    let invalid = BalanceSettings {
        enemy_speed: f32::NAN,
        ..BalanceSettings::default()
    };
    f.dispatch(Command::SetBalance(invalid), "");
    runner::flush_for_test(f);
    assert!(f.message_error);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    f.game.withdraw();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
}
#[test]
fn history_is_reachable_from_menu_camp_and_results_without_repaying_settled_runs() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..120 {
        f.game.tick(Input::default());
    }
    f.game.withdraw();
    assert_eq!(f.game.campaign.run_history.len(), 1);
    f.game.set_paused(true);
    let before = serde_json::to_value(&f.game).unwrap();
    f.screen = Screen::Menu;
    h.click(f, View::Menu, "History");
    assert!(f.screen == Screen::History);
    for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
        let output = h.frame_size(f, View::History, vec![], size);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Run history"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Withdrawal"))
        );
    }
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Menu);
    f.screen = Screen::Camp;
    h.click(f, View::Header, "History");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Camp);
    f.screen = Screen::Results;
    h.click(f, View::Results, "Run analysis");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Results);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
#[test]
fn fresh_three_lane_battle_uses_actual_small_clearing_and_invalid_clicks_do_not_throw() {
    use horde_core::balance::BalanceSettings;
    let mut fixture = IsolatedFrontend::new();
    fixture
        .frontend
        .game
        .campaign
        .set_balance_settings(BalanceSettings::default())
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.frame_size(f, View::Battle, vec![], vec2(1050.0 / 1.35, 700.0 / 1.35));
    let rect = f.arena_rect.unwrap();
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
    assert!(rect.width() < 400.0 && rect.height() < 150.0);
    assert!(rect.height() >= 86.0);
    let projection = art::ArenaProjection::for_attempt(rect, a);
    let center = projection.inverse(rect.center());
    assert!((center[0] - 6.0).abs() < 0.0001 && (center[1] - 1.5).abs() < 0.0001);
    assert_eq!(projection.world(0.5, 1.5).y, rect.center().y);
    f.attack_selection = battle::AttackSelection::Grenade;
    let outside = rect.right_center() + vec2(8.0, 0.0);
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    battle::gather_combat_input(&h.ctx, f);
    assert!(f.pending.throw.is_none());
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    let inside = projection.world(11.5, 2.5);
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: inside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    battle::gather_combat_input(&h.ctx, f);
    assert_eq!(f.pending.throw.unwrap().cell, [11, 2]);
}

#[test]
fn new_game_requires_confirmation_archives_previous_progress_and_restarts_survivor_choice() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.game.campaign.coins = 91;
    f.screen = Screen::Menu;
    f.settings.scale = 1.1;
    f.game.save(&f.save_path).unwrap();
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Menu, "New game");
    assert!(f.new_journey_confirm);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    h.click(f, View::Menu, "Keep current journey");
    assert!(!f.new_journey_confirm);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    h.click(f, View::Menu, "New game");
    h.click(f, View::Menu, "Start new journey");
    assert!(f.screen == Screen::SurvivorSelection);
    assert!(f.game.campaign.selected_survivor.is_none());
    assert!(f.game.campaign.world.start.is_none());
    assert_eq!(f.game.campaign.coins, 0);
    assert!(!f.game.campaign.journey_active);
    assert_eq!(f.settings.scale, 1.1);
    let archive = std::fs::read_dir(f.save_path.parent().unwrap().join("journey-archives"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("campaign.json");
    assert_eq!(
        serde_json::to_value(Game::load(&archive).unwrap()).unwrap(),
        before
    );
}
#[test]
fn defeat_and_withdraw_each_reach_upgrade_home_and_leave_resume_returns_to_the_map() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt.as_mut().unwrap().hp = 0;
    f.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        ..Default::default()
    });
    runner::step(f);
    assert!(f.screen == Screen::Results);
    assert!(f.game.attempt.is_none());
    assert!(f.game.global_paused);
    assert!(f.game.campaign.journey_active);
    assert_eq!(
        f.game.last_result.as_ref().unwrap().outcome,
        Outcome::Defeat
    );
    assert_eq!(f.runtime.simulated_ticks, 1);
    h.click(f, View::Results, "Try again");
    assert!(f.screen == Screen::Battle);
    assert_eq!(f.game.attempt.as_ref().unwrap().stage, 0);
    h.click(f, View::Battle, "Retreat & train");
    assert!(f.screen == Screen::Results);
    assert_eq!(
        f.game.last_result.as_ref().unwrap().outcome,
        Outcome::Withdrawal
    );
    let coins = f.game.campaign.coins;
    let records = f.game.campaign.run_history.len();
    h.click(f, View::Results, "Leave journey");
    assert!(f.screen == Screen::Menu);
    assert!(!f.game.campaign.journey_active);
    f.game = Game::load(&f.save_path).unwrap();
    h.click(f, View::Menu, "Resume journey");
    assert!(f.screen == Screen::Camp && f.tab == Tab::World);
    assert_eq!(f.game.campaign.coins, coins);
    assert_eq!(f.game.campaign.run_history.len(), records);
}
#[test]
fn liberated_country_continue_opens_map_and_does_not_autoselect_or_pay_again() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result.as_mut().unwrap().outcome = Outcome::Victory;
    let count = world().country("DEU").unwrap().level_count();
    f.game
        .campaign
        .world
        .country_levels
        .insert("DEU".into(), count);
    f.screen = Screen::Results;
    let coins = f.game.campaign.coins;
    let records = f.game.campaign.run_history.len();
    h.click(f, View::Results, "Choose next country");
    assert!(f.screen == Screen::Camp && f.tab == Tab::World);
    assert!(f.game.attempt.is_none());
    assert!(!f.game.campaign.journey_active);
    assert_eq!(f.game.campaign.world.active.as_deref(), Some("DEU"));
    assert_eq!(f.game.campaign.coins, coins);
    assert_eq!(f.game.campaign.run_history.len(), records);
}

#[test]
fn blank_and_unreadable_saved_generations_still_require_new_game_confirmation_without_writes() {
    for unreadable in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game = Game::default();
        f.profile_exists = false;
        f.screen = Screen::Menu;
        if unreadable {
            std::fs::create_dir_all(f.save_path.parent().unwrap()).unwrap();
            std::fs::write(&f.save_path, b"unreadable saved generation").unwrap();
        } else {
            f.game.save(&f.save_path).unwrap();
        }
        let before = std::fs::read(&f.save_path).unwrap();
        let model = serde_json::to_value(&f.game).unwrap();
        h.click(f, View::Menu, "New game");
        assert!(f.new_journey_confirm);
        assert!(f.screen == Screen::Menu);
        assert_eq!(std::fs::read(&f.save_path).unwrap(), before);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), model);
    }
}

#[test]
fn older_autosave_ack_keeps_navigation_busy_and_freezes_raw_settings_binding_capture() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let gate = f.runtime.gated_storage_for_test();
    f.dispatch(AppCommand::Save(SaveIntent::Autosave), "");
    assert!(!runner::flush(f));
    gate.started();
    f.open_settings();
    f.request_save(SaveIntent::SettingsBack);
    assert!(!runner::flush(f));
    gate.release();
    assert!(!runner::poll_one_save_for_test(f));
    gate.started();
    assert!(f.screen == Screen::Settings);
    assert!(f.save_pending);
    assert!(f.runtime.durable_transition_pending());
    f.rebind = Some(Action::Fire);
    f.binding_armed = true;
    let settings = f.settings.clone();
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: vec![key_event(Key::X, true)],
            ..Default::default()
        },
        |ui| settings_ui(ui, f),
    );
    output.textures_delta.clear();
    assert_eq!(f.settings.bindings, settings.bindings);
    assert_eq!(f.settings.scale, settings.scale);
    assert!(f.save_pending);
    gate.release();
    assert!(!runner::poll_one_save_for_test(f));
    assert!(f.screen == Screen::Camp);
    assert!(!f.save_pending);
}

#[test]
fn compact_country_selection_keeps_a_useful_map_visible_before_the_fixed_journey_footer() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    settings::configure(&h.ctx, 1.35);
    let f = &mut fixture.frontend;
    f.settings.scale = 1.35;
    f.practice_profile = true;
    f.message = "Practice scenario - progress is isolated from your campaign".into();
    for search in ["", "deu", "a"] {
        f.search = search.into();
        let output = h.frame_size(f, View::WorldWithChrome, vec![], vec2(1050.0, 700.0));
        let map = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == Color32::from_rgb(22, 39, 42) => {
                    Some(shape.rect.intersect(clipped.clip_rect))
                }
                _ => None,
            })
            .expect("Map background must be painted");
        assert!(
            map.height() >= 180.0,
            "Search {search:?}: map needs180logical pixels, got {map:?}"
        );
        assert!(map.width() > 500.0);
        let start = output
            .shapes
            .iter()
            .find_map(|shape| text_position(&shape.shape, "Start journey"))
            .unwrap();
        assert!(start.y >= map.bottom());
        assert!(h.ctx.content_rect().contains(start));
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| shape_has_text(&shape.shape, "Find country"))
        );
    }
}
