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

#[test]
fn deploy_notice_enters_battle_resets_controls_and_camp_tab_is_a_real_sub_state() {
    let mut parts = test_frontend();
    parts.game.campaign.legacy_features = true;
    parts
        .game
        .campaign
        .choose_survivor(SurvivorId::Rook)
        .unwrap();
    parts.game.campaign.choose_start_country("DEU").unwrap();
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

    h.ui(|f| {
        f.dispatch(
            Command::Deploy {
                stage: 0,
                mode: Mode::Personal,
            },
            "",
        )
    });
    for _ in 0..3 {
        h.app.update();
    }
    assert_eq!(h.screen(), Screen::Battle);
    assert!(h.app.world().get_resource::<State<Tab>>().is_none());
    let attempt = h
        .app
        .world()
        .resource::<GameSession>()
        .game
        .attempt
        .clone()
        .expect("deployed");
    let battle = h.app.world().resource::<BattleUi>();
    assert!(!battle.construction && !battle.fire_latched);
    assert_eq!(battle.speed, 1);
    assert_eq!(battle.attack_selection, AttackSelection::Gun);
    assert_eq!(
        battle.aim,
        [attempt.width_yards * 0.5, attempt.defender_position()[1]]
    );

    h.ui(|f| f.open_preparation(Tab::Armory));
    h.app.update();
    assert_eq!(h.screen(), Screen::Camp);
    assert_eq!(*h.app.world().resource::<State<Tab>>().get(), Tab::Armory);
}
