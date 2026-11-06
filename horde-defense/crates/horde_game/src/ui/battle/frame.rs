//! The battle views of one egui pass. `draw_screen` builds a `BattleFrame`
//! once and hands it to every reader (the controls, the battle screen and the
//! published combat input), so no widget rebuilds a view from the game. This
//! is exact because the game is immutable during a pass: dispatch only
//! enqueues commands for the runtime to apply after it.
use crate::{
    art,
    battle_view::{BattleScreen, BattleViewModel},
    ui::{UiCtx, hud},
};
use bevy_egui::egui;

/// The battle screen, its HUD and its field view, built from one game state.
pub(in crate::ui) struct BattleFrame {
    pub(super) screen: BattleScreen,
    pub(super) hud: hud::HudView,
    pub(super) field: BattleViewModel,
}
impl BattleFrame {
    /// The active battle's views, or `None` outside a battle.
    ///
    /// The field samples the cosmetic scene cache for the root background
    /// layer, the layer the main battle paints on (`battle` asserts it).
    pub(in crate::ui) fn build(ctx: &egui::Context, f: &UiCtx) -> Option<Self> {
        let game = f.game;
        let a = game.attempt()?;
        let screen = BattleScreen::build(game)?;
        let hud = hud::view_model(game)?;
        let reduced = f.settings.reduced_effects;
        let motion = art::sample_motion(ctx, egui::LayerId::background(), a, reduced);
        let field =
            BattleViewModel::build(a, &motion, f.battle.interpolation.scene(None, a), reduced);
        Some(Self { screen, hud, field })
    }
}
