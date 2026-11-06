use super::projection;
use crate::{
    art::{GREEN, RED, TEXT},
    battle_view::{self, BattleScreen},
    ui::{UiCtx, muted},
};
use bevy_egui::egui::{self, Align2, FontId, Rect, Stroke, vec2};
use horde_core::{
    command::Command,
    content::{StructureKind, content},
};

pub(super) fn construction_controls(ui: &mut egui::Ui, f: &mut UiCtx, view: &BattleScreen) {
    ui.strong(format!("{} supply remaining", view.supply));
    muted(ui, "Cost in supply per defense");
    for kind in StructureKind::ALL {
        let def = content().structure(kind);
        ui.add_enabled_ui(view.blueprint(kind), |ui| {
            ui.selectable_value(
                &mut f.battle.build_kind,
                kind,
                format!("{} · {}", def.name, def.supply),
            );
        })
        .response
        .on_disabled_hover_text(
            "Learn this blueprint in the Fortification branch of Journey upgrades.",
        );
    }
    let def = content().structure(f.battle.build_kind);
    muted(
        ui,
        format!(
            "{} HP · {} damage · {:.1} yard range",
            def.health, def.damage, def.range
        ),
    );
    ui.add_space(8.0);
    if ui.button("Save country plan").clicked() {
        f.dispatch(
            Command::SaveStartingLayout,
            "Country plan saved. Next deployment pays its supply costs.",
        );
    }
    if ui.button("Clear country plan").clicked() {
        f.dispatch(
            Command::ClearLayout,
            "Country plan cleared; deployed defenses remain.",
        );
    }
    ui.add_space(10.0);
    muted(
        ui,
        "Before combat: full refund. During combat: salvage half the remaining-health value, rounded down.",
    );
    muted(
        ui,
        if view.global_paused {
            "Paused explicitly"
        } else {
            "Combat continues while building"
        },
    );
    if !view.any_blueprint {
        muted(
            ui,
            "Unlock traps after 1 boss clear, walls after 2, turrets after 3. Purchase blueprints in camp.",
        );
    }
}
pub(super) fn placement_cursor(ui: &mut egui::Ui, r: Rect, f: &mut UiCtx) {
    let arena = projection(f, r);
    let Some(preview) = battle_view::placement_preview(f.game, f.battle.build_kind, f.battle.aim)
    else {
        return;
    };
    let (lane, column, center) = (preview.lane, preview.column, preview.center);
    let color = if preview.valid.is_ok() { GREEN } else { RED };
    let cell = Rect::from_min_max(
        arena.world(center[0] - preview.half_width, f32::from(lane)),
        arena.world(center[0] + preview.half_width, f32::from(lane) + 1.0),
    );
    ui.painter()
        .rect_filled(cell, 0.0, color.gamma_multiply(0.22));
    ui.painter()
        .rect_stroke(cell, 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    let def = content().structure(f.battle.build_kind);
    if def.range > 0.0 {
        ui.painter().add(egui::Shape::ellipse_stroke(
            cell.center(),
            arena.radius(def.range),
            Stroke::new(1.0, color.gamma_multiply(0.45)),
        ));
    }
    if let Err(reason) = &preview.valid {
        ui.painter().text(
            cell.left_top() + vec2(3.0, -5.0),
            Align2::LEFT_BOTTOM,
            reason,
            FontId::proportional(11.0),
            TEXT,
        );
    }
    if ui.input(|i| i.pointer.primary_clicked()) {
        f.dispatch(
            Command::Place {
                kind: f.battle.build_kind,
                lane,
                column,
            },
            "Defense placed.",
        );
    }
    if ui.input(|i| i.pointer.secondary_clicked())
        && let Some(id) = preview.occupant
    {
        f.dispatch(Command::Salvage(id), "Structure salvaged.");
    }
}
