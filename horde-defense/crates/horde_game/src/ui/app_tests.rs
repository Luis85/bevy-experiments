//! Headless App tests of the real state wiring: `FrontendParts::insert_into`,
//! `SimRuntimePlugin`, the notice system and `OnEnter(Screen::Battle)`, with
//! Bevy's own `StateTransition` (no egui, no window).
use super::{
    BattleUi, FrontendParts, Screen, Tab, UiParams, battle::AttackSelection,
    plugin::NavigationPlugin, test_support::test_frontend,
};
use crate::{
    launch::LaunchConfig,
    runner::{GameSession, Runtime, SimRuntimePlugin},
};
use bevy::{ecs::system::RunSystemOnce, prelude::*, state::app::StatesPlugin};
use horde_core::{command::Command, content::Mode, survivors::SurvivorId};

struct HeadlessApp {
    app: App,
    directory: std::path::PathBuf,
}
impl HeadlessApp {
    fn new(parts: FrontendParts) -> Self {
        let directory = parts.paths.campaign().parent().unwrap().to_path_buf();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(LaunchConfig::default());
        parts.insert_into(&mut app);
        app.add_plugins((SimRuntimePlugin, NavigationPlugin));
        Self { app, directory }
    }
    fn ui(&mut self, action: impl FnOnce(&mut super::UiCtx) + Send + Sync + 'static) {
        let mut action = Some(action);
        self.app
            .world_mut()
            .run_system_once(move |mut params: UiParams| {
                let mut cues = Vec::new();
                if let Some(action) = action.take() {
                    action(&mut params.ctx(&mut cues));
                }
            })
            .unwrap();
    }
    fn screen(&self) -> Screen {
        *self.app.world().resource::<State<Screen>>().get()
    }
}
impl Drop for HeadlessApp {
    fn drop(&mut self) {
        self.app
            .world_mut()
            .resource_mut::<Runtime>()
            .finish_storage_for_test();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Sends `Command::Deploy` for stage 0 in person.
fn deploy(f: &mut super::UiCtx) {
    f.dispatch(
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
        "",
    );
}

/// The Briefing primary: `Command::BeginDefense` (ruling D16).
fn briefing_primary(f: &mut super::UiCtx) {
    f.begin();
}

/// `Deploy` and the Briefing primary's `BeginDefense` share one notice arm
/// (`ui/notices.rs`), so each enters the battle through
/// `OnEnter(Screen::Battle)`, which resets the battle controls.
#[test]
fn deploy_notice_enters_battle_resets_controls_and_camp_tab_is_a_real_sub_state() {
    for (name, send) in [
        ("Deploy", deploy as fn(&mut super::UiCtx)),
        ("BeginDefense", briefing_primary),
    ] {
        deploy_notice_enters_battle_and_resets_controls(name, send);
    }
}

fn deploy_notice_enters_battle_and_resets_controls(name: &str, send: fn(&mut super::UiCtx)) {
    let mut parts = test_frontend();
    parts.game.campaign_mut().legacy_features = true;
    parts
        .game
        .campaign_mut()
        .choose_survivor(SurvivorId::Rook)
        .unwrap();
    parts
        .game
        .campaign_mut()
        .choose_start_country("DEU")
        .unwrap();
    parts.screen = Screen::Camp;
    parts.nav.tab = Tab::Briefing;
    parts.battle.construction = true;
    parts.battle.speed = 4;
    parts.battle.fire_latched = true;
    parts.battle.attack_selection = AttackSelection::Grenade;
    parts.battle.aim = [0.0, 0.0];
    let mut h = HeadlessApp::new(parts);
    h.app.update();
    assert_eq!(h.screen(), Screen::Camp);
    assert_eq!(
        *h.app.world().resource::<State<Tab>>().get(),
        Tab::Briefing,
        "the initial camp tab comes from Navigation"
    );

    h.ui(send);
    for _ in 0..3 {
        h.app.update();
    }
    assert_eq!(h.screen(), Screen::Battle, "{name}");
    assert!(
        h.app.world().get_resource::<State<Tab>>().is_none(),
        "{name}"
    );
    let attempt = h
        .app
        .world()
        .resource::<GameSession>()
        .game
        .attempt()
        .cloned()
        .unwrap_or_else(|| panic!("{name}: deployed"));
    let battle = h.app.world().resource::<BattleUi>();
    assert!(!battle.construction && !battle.fire_latched, "{name}");
    assert_eq!(battle.speed, 1, "{name}");
    assert_eq!(battle.attack_selection, AttackSelection::Gun, "{name}");
    assert_eq!(
        battle.aim,
        [attempt.width_yards * 0.5, attempt.defender_position()[1]],
        "{name}"
    );

    h.ui(|f| f.open_preparation(Tab::Armory));
    h.app.update();
    assert_eq!(h.screen(), Screen::Camp, "{name}");
    assert_eq!(
        *h.app.world().resource::<State<Tab>>().get(),
        Tab::Armory,
        "{name}"
    );
}
