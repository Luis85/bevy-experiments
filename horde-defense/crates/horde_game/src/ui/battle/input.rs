use super::projection;
use crate::{
    art,
    battle_view::{self, BattleScreen},
    settings::Action,
    ui::{UiCtx, feedback},
};
use bevy_egui::egui;
use horde_core::{
    command::Command,
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::Input,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum AttackSelection {
    #[default]
    Gun,
    Grenade,
    Molotov,
}
impl AttackSelection {
    pub(super) fn ordnance(self) -> Option<OrdnanceKind> {
        match self {
            Self::Gun => None,
            Self::Grenade => Some(OrdnanceKind::Grenade),
            Self::Molotov => Some(OrdnanceKind::Molotov),
        }
    }
}
pub(super) fn target_cell(aim: [f32; 2], lanes: u8, width: f32) -> Option<[u8; 2]> {
    (aim[0].is_finite()
        && aim[1].is_finite()
        && (0.0..width).contains(&aim[0])
        && (0.0..f32::from(lanes)).contains(&aim[1]))
    .then(|| [aim[0].floor() as u8, aim[1].floor() as u8])
    .filter(|cell| horde_core::ordnance::valid_cell_for_arena(*cell, lanes, width))
}

pub(in crate::ui) fn controls(ctx: &egui::Context, f: &mut UiCtx) {
    let frame = ctx.cumulative_frame_nr();
    if f.battle.last_control_frame == frame {
        return;
    }
    f.battle.last_control_frame = frame;
    if feedback::overlay_owns_keyboard(ctx, f) {
        return;
    }
    if f.settings.pressed(ctx, Action::Pause, false) {
        let pause = !BattleScreen::build(f.game).is_some_and(|view| view.paused);
        f.dispatch(Command::SetPaused(pause), "");
        f.battle.construction = false;
        f.battle.fire_latched = false;
    }
    if f.settings.pressed(ctx, Action::Build, false) {
        if f.game.campaign().blueprints.iter().any(|owned| *owned) {
            f.battle.construction = !f.battle.construction;
        } else {
            f.battle.construction = false;
            f.status.message = format!(
                "{} Combat continues; use Pause when you need time.",
                f.game.campaign().construction_blocker().unwrap_or_default()
            );
            f.status.message_error = false;
            f.status.feedback_revision = f.status.feedback_revision.wrapping_add(1);
        }
        f.battle.fire_latched = false;
    }
    if f.battle.attack_selection == AttackSelection::Gun
        && f.settings.pressed(ctx, Action::Toggle, false)
    {
        f.toggle_fire(ctx);
    }
}
pub(in crate::ui) fn gather_combat_input(ctx: &egui::Context, f: &mut UiCtx) -> Input {
    if f.battle.construction
        || ctx.text_edit_focused()
        || ctx.any_popup_open()
        || f.status.feedback.details_open
    {
        return Input {
            aim: f.battle.aim,
            auto_reload: f.settings.auto_reload,
            ..Input::default()
        };
    }
    let view = BattleScreen::build(f.game);
    let view = view.as_ref();
    let mut input = Input {
        aim: assisted_aim(f, view),
        auto_reload: f.settings.auto_reload,
        ..Input::default()
    };
    let fire_allowed =
        f.battle.arena_hover || !f.settings.bindings[Action::Fire as usize].starts_with("Mouse");
    if let Some(kind) = f.battle.attack_selection.ordnance() {
        // Throwing consumes a press edge, never a held button or firing latch.
        f.battle.fire_latched = false;
        let (lanes, width) = battle_view::arena_geometry(f.game);
        if let Some(cell) =
            raw_press_aim(ctx, f, Action::Fire).and_then(|aim| target_cell(aim, lanes, width))
        {
            f.battle
                .pending
                .throw
                .get_or_insert(ThrowRequest { kind, cell });
        }
    } else {
        let fire_press = press_aim(ctx, f, view, Action::Fire);
        if f.settings.toggle_fire && fire_press.is_some() {
            f.toggle_fire(ctx);
        }
        input.fire = f.battle.fire_latched
            || (!f.settings.toggle_fire
                && fire_allowed
                && f.settings.pressed(ctx, Action::Fire, true));
        if !f.settings.toggle_fire
            && let Some(aim) = fire_press
        {
            f.battle.pending.fire = true;
            f.battle.pending_attack_aims.fire.get_or_insert(aim);
        }
    }
    if let Some(aim) = press_aim(ctx, f, view, Action::Melee) {
        f.battle.pending.melee = true;
        f.battle.pending_attack_aims.melee.get_or_insert(aim);
    }
    input.melee |= f.settings.pressed(ctx, Action::Melee, true)
        && (f.battle.arena_hover
            || !f.settings.bindings[Action::Melee as usize].starts_with("Mouse"));
    input.reload |= f.settings.pressed(ctx, Action::Reload, false);
    input.heal |= f.settings.pressed(ctx, Action::Heal, false);
    input.brace |= f.settings.pressed(ctx, Action::Brace, false);
    input
}
pub(super) fn assisted_aim(f: &UiCtx, view: Option<&BattleScreen>) -> [f32; 2] {
    assist_at(f, view, f.battle.aim)
}
fn press_aim(
    ctx: &egui::Context,
    f: &UiCtx,
    view: Option<&BattleScreen>,
    action: Action,
) -> Option<[f32; 2]> {
    raw_press_aim(ctx, f, action).map(|aim| assist_at(f, view, aim))
}
fn raw_press_aim(ctx: &egui::Context, f: &UiCtx, action: Action) -> Option<[f32; 2]> {
    let binding = &f.settings.bindings[action as usize];
    let button = match binding.as_str() {
        "Mouse1" => Some(egui::PointerButton::Primary),
        "Mouse2" => Some(egui::PointerButton::Secondary),
        "Mouse3" => Some(egui::PointerButton::Middle),
        _ => None,
    };
    if let Some(button) = button {
        let position = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: pressed,
                    pressed: true,
                    ..
                } if *pressed == button => Some(*pos),
                _ => None,
            })
        });
        if let Some(position) = position
            && let Some(arena) = f.battle.arena_rect
        {
            return arena
                .contains(position)
                .then(|| projection(f, arena).inverse(position));
        }
        return (f.battle.arena_hover && f.settings.pressed(ctx, action, false))
            .then_some(f.battle.aim);
    }
    f.settings
        .pressed(ctx, action, false)
        .then_some(f.battle.aim)
}
fn assist_at(f: &UiCtx, view: Option<&BattleScreen>, aim: [f32; 2]) -> [f32; 2] {
    view.map_or(aim, |view| view.assist(aim, f.settings.aim_assist))
}
pub(super) fn reticle_ownership(
    preview: bool,
    hovered: bool,
    focused: bool,
    covered: bool,
) -> Option<bool> {
    if preview {
        Some(false)
    } else if hovered && focused && !covered {
        Some(true)
    } else {
        None
    }
}
pub(super) fn preview_aim(arena: art::ArenaProjection) -> [f32; 2] {
    [(arena.width * 0.6).min(18.0), f32::from(arena.lanes) * 0.5]
}
