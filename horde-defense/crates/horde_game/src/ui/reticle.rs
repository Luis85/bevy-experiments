//! A cursor readout of frozen authoritative ammunition and action timers.
use super::*;
use horde_core::sim::Attempt;
#[derive(Clone, Debug, PartialEq)]
struct Resources {
    remaining: u32,
    capacity: u32,
    progress: Option<f32>,
    label: String,
}
fn resources(a: &Attempt, selection: battle::AttackSelection) -> Resources {
    let reload = if a.reload_left > 0 {
        let total = if a.mode == Mode::Npc {
            a.profile.npc.reload_turns.max(1) * 60
        } else {
            (a.profile.personal.reload.max(0.5) * 60.0).ceil() as u32
        };
        Some((1.0 - a.reload_left as f32 / total.max(1) as f32).clamp(0.0, 1.0))
    } else {
        None
    };
    if a.mode == Mode::Personal
        && let Some(kind) = selection.ordnance()
    {
        let index = kind.index();
        let left = a.ordnance.cooldowns[index];
        let total = (horde_core::ordnance::catalog()
            .definition(kind)
            .cooldown_ticks as f32
            * a.profile.modifiers.ordnance_cooldown_multiplier)
            .round()
            .clamp(1.0, 3600.0);
        Resources {
            remaining: a.ordnance.charges[index],
            capacity: a.ordnance.issued[index].max(1),
            progress: reload.or((left > 0).then(|| (1.0 - left as f32 / total).clamp(0.0, 1.0))),
            label: if reload.is_some() {
                "RELOAD".into()
            } else {
                format!("{} left", a.ordnance.charges[index])
            },
        }
    } else {
        Resources {
            remaining: a.ammo,
            capacity: a.magazine.max(1),
            progress: reload,
            label: if reload.is_some() {
                format!("R {:.1}s", a.reload_left as f32 / 60.0)
            } else {
                format!("{}/{}", a.ammo, a.magazine)
            },
        }
    }
}
fn arc(p: &egui::Painter, at: egui::Pos2, radius: f32, progress: f32, color: Color32) {
    let count = (48.0 * progress).ceil().max(1.0) as usize;
    let points = (0..=count)
        .map(|n| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * progress * n as f32 / count as f32;
            at + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    p.add(egui::Shape::line(points, Stroke::new(2.0, color)));
}
fn badge_center(rect: Rect, at: egui::Pos2) -> egui::Pos2 {
    pos2(
        at.x.clamp(rect.left() + 26.0, rect.right() - 26.0),
        at.y.clamp(rect.top() + 26.0, rect.bottom() - 43.0),
    )
}
fn readout_origin(rect: Rect, center: egui::Pos2, text: egui::Vec2) -> egui::Pos2 {
    pos2(
        (center.x - text.x * 0.5).clamp(rect.left() + 4.0, rect.right() - text.x - 4.0),
        center.y + 27.0,
    )
}
pub(super) fn paint(
    ui: &egui::Ui,
    a: &Attempt,
    arena: art::ArenaProjection,
    aim: [f32; 2],
    selection: battle::AttackSelection,
    building: bool,
    reduced: bool,
) {
    let at = arena.world(aim[0], aim[1]);
    let p = ui.painter_at(arena.effects_clip);
    let state = resources(a, selection);
    let color = if state.remaining == 0 {
        RED
    } else if a.mode == Mode::Npc {
        TEXT
    } else {
        AMBER
    };
    for d in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        p.line_segment(
            [at + d * 4.0, at + d * 10.0],
            Stroke::new(3.0, Color32::BLACK),
        );
        p.line_segment([at + d * 4.0, at + d * 10.0], Stroke::new(1.3, color));
    }
    if building {
        return;
    }
    // Keep resource readouts clear of arena edges without moving the actual
    // aim crosshair. The small edge badge remains anchored to that crosshair.
    let center = badge_center(arena.effects_clip, at);
    let segments = state.capacity.min(if reduced { 12 } else { 24 }).max(1);
    for index in 0..segments {
        let angle =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / segments as f32;
        let d = vec2(angle.cos(), angle.sin());
        let filled = u64::from(index) * u64::from(state.capacity)
            < u64::from(state.remaining) * u64::from(segments);
        p.line_segment(
            [center + d * 16.0, center + d * 19.0],
            Stroke::new(3.5, Color32::BLACK),
        );
        p.line_segment(
            [center + d * 16.0, center + d * 19.0],
            Stroke::new(
                1.8,
                if filled {
                    color
                } else {
                    MUTED.gamma_multiply(0.4)
                },
            ),
        );
    }
    if let Some(progress) = state.progress {
        arc(&p, center, 23.0, progress, GREEN);
    }
    let galley = p.layout_no_wrap(state.label, FontId::proportional(11.0), color);
    let pos = readout_origin(arena.effects_clip, center, galley.size());
    p.rect_filled(
        Rect::from_min_size(pos - vec2(3.0, 1.0), galley.size() + vec2(6.0, 2.0)),
        0.0,
        Color32::from_black_alpha(200),
    );
    p.galley(pos, galley, color);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reticle_uses_exact_ammo_and_frozen_reload_ticks_without_a_presentation_clock() {
        let mut game = Game::default();
        game.campaign.choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let a = game.attempt.as_mut().unwrap();
        a.ammo = 3;
        a.reload_left = 63;
        a.profile.personal.reload = 2.1;
        let state = resources(a, battle::AttackSelection::Gun);
        assert_eq!((state.remaining, state.capacity), (3, a.magazine));
        assert_eq!(state.progress, Some(0.5));
        assert_eq!(state.label, "R 1.0s");
        assert_eq!(resources(a, battle::AttackSelection::Gun), state);
        a.mode = Mode::Npc;
        a.profile.npc.reload_turns = 3;
        a.reload_left = 90;
        assert_eq!(
            resources(a, battle::AttackSelection::Gun).progress,
            Some(0.5)
        );
    }
    #[test]
    fn resources_stay_readable_at_every_clearing_edge_without_moving_true_aim() {
        for size in [vec2(160.0, 86.0), vec2(292.0, 86.0), vec2(729.0, 310.0)] {
            let rect = Rect::from_min_size(pos2(40.0, 160.0), size);
            for aim in [
                rect.left_top(),
                rect.right_top(),
                rect.left_bottom(),
                rect.right_bottom(),
                rect.center(),
            ] {
                let original = aim;
                let badge = badge_center(rect, aim);
                assert!(rect.contains_rect(Rect::from_center_size(badge, vec2(48.0, 48.0))));
                let text = vec2(72.0, 13.0);
                assert!(rect.contains_rect(
                    Rect::from_min_size(readout_origin(rect, badge, text), text).expand(2.0)
                ));
                assert_eq!(aim, original);
            }
        }
    }
}
