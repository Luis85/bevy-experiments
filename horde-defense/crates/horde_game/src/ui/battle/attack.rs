use super::{
    input::{AttackSelection, assisted_aim, target_cell},
    projection,
};
use crate::{
    art::{AMBER, MUTED, RED, TEXT},
    battle_view::{self, BattleScreen},
    settings::Action,
    ui::UiCtx,
};
use bevy_egui::egui::{self, Align2, Color32, FontId, Rect, RichText, Stroke, pos2, vec2};

pub(super) fn attack_selector_rect(canvas: Rect, arena: Rect) -> Rect {
    Rect::from_min_max(
        pos2(canvas.left() + 350.0, canvas.top() + 12.0),
        pos2(canvas.right() - 222.0, arena.top() - 12.0),
    )
}
pub(super) fn attack_selector(ui: &mut egui::Ui, f: &mut UiCtx, view: &BattleScreen) {
    let class = view.weapon_class;
    let spacing = 4.0;
    let button_width = ((ui.available_width() - 2.0 * spacing) / 3.0).max(1.0);
    ui.spacing_mut().item_spacing.x = spacing;
    ui.spacing_mut().button_padding = vec2(3.0, 4.0);
    ui.horizontal(|ui| {
        for (selection, label) in [
            (AttackSelection::Gun, "Gun"),
            (AttackSelection::Grenade, "Grenade"),
            (AttackSelection::Molotov, "Molotov"),
        ] {
            let slot = selection.ordnance().map(|kind| view.slot(kind));
            let unlocked = slot.is_none_or(|slot| slot.unlocked);
            let fire = &f.settings.bindings[Action::Fire as usize];
            let (hint, color) =
                throwable_state(label, slot, || format!("{class}: hold {fire} to fire."));
            let selected = f.battle.attack_selection == selection;
            let button = egui::Button::new(RichText::new(label).size(12.0).color(color))
                .selected(selected)
                .frame(true);
            let response = ui
                .add_enabled_ui(unlocked, |ui| {
                    ui.add_sized(vec2(button_width, 26.0), button)
                })
                .inner;
            // Cooldown progress underlines a recharging throwable.
            if let Some(slot) =
                slot.filter(|slot| slot.unlocked && slot.cooldown > 0 && slot.charges > 0)
            {
                let rect = response.rect.shrink2(vec2(4.0, 2.0));
                let ready = 1.0 - (slot.cooldown as f32 / slot.cooldown_total).clamp(0.0, 1.0);
                let y = rect.bottom();
                ui.painter().line_segment(
                    [
                        pos2(rect.left(), y),
                        pos2(rect.left() + rect.width() * ready, y),
                    ],
                    Stroke::new(2.0, AMBER),
                );
            }
            if response
                .on_hover_text(&hint)
                .on_disabled_hover_text(&hint)
                .clicked()
            {
                f.battle.attack_selection = selection;
                f.battle.fire_latched = false;
                f.battle.pending.fire = false;
                f.battle.pending.throw = None;
                f.battle.pending_attack_aims.fire = None;
                f.runtime.clear_input();
            }
        }
    });
    let helper = if let Some(kind) = f.battle.attack_selection.ordnance() {
        let slot = view.slot(kind);
        if slot.charges == 0 {
            "Empty · renewed next attempt".to_owned()
        } else if slot.cooldown > 0 {
            format!(
                "{} left · ready {:.1}s",
                slot.charges,
                slot.cooldown as f32 / 60.0
            )
        } else {
            format!("{} left · click ground cell", slot.charges)
        }
    } else {
        "Hold fire to shoot".to_owned()
    };
    ui.add(egui::Label::new(RichText::new(&helper).size(11.0).color(MUTED)).truncate())
        .on_hover_text(helper);
}
/// Hover reason and label colour of one attack selector. A locked throwable
/// names where it is learned; an unlocked one states its charges or cooldown.
fn throwable_state(
    label: &str,
    slot: Option<&battle_view::OrdnanceSlotView>,
    gun: impl FnOnce() -> String,
) -> (String, Color32) {
    match slot {
        None => (gun(), TEXT),
        Some(slot) if !slot.unlocked => (
            format!(
                "{label} locked. Learn it in the Ordnance branch of Journey upgrades after this attempt."
            ),
            MUTED,
        ),
        Some(slot) if slot.charges == 0 => (
            format!("{label}: no charges left. Renewed next attempt."),
            RED,
        ),
        Some(slot) if slot.cooldown > 0 => (
            format!(
                "{label}: {} charges · ready in {:.1}s. Select, then click a ground cell to throw.",
                slot.charges,
                slot.cooldown as f32 / 60.0
            ),
            AMBER,
        ),
        Some(slot) => (
            format!(
                "{label}: {} charges. Select, then click a ground cell to throw.",
                slot.charges
            ),
            TEXT,
        ),
    }
}
pub(super) fn ordnance_cursor(ui: &mut egui::Ui, r: Rect, f: &UiCtx, view: &BattleScreen) {
    let arena = projection(f, r);
    let Some(kind) = f.battle.attack_selection.ordnance() else {
        return;
    };
    let Some(cell) = target_cell(f.battle.aim, arena.lanes, arena.width) else {
        return;
    };
    let slot = view.slot(kind);
    let color = if slot.ready { AMBER } else { RED };
    let painter = ui.painter_at(r);
    for column in 0..=arena.width.ceil() as u8 {
        painter.line_segment(
            [
                arena.world(column as f32, 0.0),
                arena.world(column as f32, f32::from(arena.lanes)),
            ],
            Stroke::new(0.5, MUTED.gamma_multiply(0.18)),
        );
    }
    for lane in 0..=arena.lanes {
        painter.line_segment(
            [
                arena.world(0.0, lane as f32),
                arena.world(arena.width, lane as f32),
            ],
            Stroke::new(0.5, MUTED.gamma_multiply(0.18)),
        );
    }
    let [x, y] = cell.map(f32::from);
    let rect = Rect::from_min_max(arena.world(x, y), arena.world(x + 1.0, y + 1.0));
    painter.rect_filled(rect, 0.0, color.gamma_multiply(0.25));
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5, color), egui::StrokeKind::Inside);
    painter.add(egui::Shape::ellipse_stroke(
        rect.center(),
        arena.radius(slot.radius),
        Stroke::new(1.2, color),
    ));
    painter.text(
        rect.left_top() - vec2(0.0, 5.0),
        Align2::LEFT_BOTTOM,
        format!("{} · cell {},{}", kind.name(), cell[0] + 1, cell[1] + 1),
        FontId::proportional(12.0),
        TEXT,
    );
}

pub(super) fn personal_controls(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    ctx: &egui::Context,
    view: &BattleScreen,
) {
    f.battle.pending.reload |= ui.button("Reload").clicked();
    if ui
        .button("Strike contact lane")
        .on_hover_text("Strike the selected contact lane. Melee cancels an unfinished reload.")
        .clicked()
    {
        let aim = assisted_aim(f, Some(view));
        f.battle.pending.melee = true;
        f.battle.pending.aim = aim;
        f.battle.pending_attack_aims.melee.get_or_insert(aim);
    }
    if f.battle.attack_selection == AttackSelection::Gun
        && ui
            .button(if f.battle.fire_latched {
                "Stop firing"
            } else {
                "Toggle firing"
            })
            .clicked()
    {
        f.toggle_fire(ctx);
    }
    if view.heal {
        f.battle.pending.heal |= ui
            .button("Emergency aid")
            .on_hover_text(&f.settings.bindings[Action::Heal as usize])
            .clicked();
    }
    if view.brace {
        f.battle.pending.brace |= ui
            .button("Brace the line")
            .on_hover_text(&f.settings.bindings[Action::Brace as usize])
            .clicked();
    }
}
