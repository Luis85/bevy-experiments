use super::{AMBER, box_at, line};
use bevy_egui::egui::{self, Color32 as C, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use horde_core::content::{EnemyKind, StructureKind};

/// How the procedural fallback figure draws one enemy kind when no sprite is
/// available: coat tint plus optional overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FallbackLook {
    pub(super) coat: C,
    /// Chest plate with an amber stripe.
    pub(super) armor_plate: bool,
    /// Raised charge pack on a pole.
    pub(super) charge_pack: bool,
}
impl FallbackLook {
    const fn plain(coat: C) -> Self {
        Self {
            coat,
            armor_plate: false,
            charge_pack: false,
        }
    }
}

/// One row per enemy kind; a new kind only adds a row here. Kinds without a
/// row draw as the first row (the unit tests require full coverage).
pub(super) const FALLBACK_LOOKS: &[(EnemyKind, FallbackLook)] = &[
    (
        EnemyKind::Shambler,
        FallbackLook::plain(C::from_rgb(126, 144, 104)),
    ),
    (
        EnemyKind::Runner,
        FallbackLook::plain(C::from_rgb(193, 151, 102)),
    ),
    (
        EnemyKind::Brute,
        FallbackLook::plain(C::from_rgb(123, 108, 89)),
    ),
    (
        EnemyKind::Armored,
        FallbackLook {
            coat: C::from_rgb(133, 154, 158),
            armor_plate: true,
            charge_pack: false,
        },
    ),
    (
        EnemyKind::Demolisher,
        FallbackLook {
            coat: C::from_rgb(197, 150, 61),
            armor_plate: false,
            charge_pack: true,
        },
    ),
];
/// Bosses keep their kind's overlays, wear this coat and always get a plate.
const BOSS_COAT: C = C::from_rgb(135, 73, 53);

pub(super) fn fallback_look(kind: EnemyKind, boss: bool) -> FallbackLook {
    let base = FALLBACK_LOOKS
        .iter()
        .find(|(row, _)| *row == kind)
        .map_or(FALLBACK_LOOKS[0].1, |&(_, look)| look);
    if boss {
        FallbackLook {
            coat: BOSS_COAT,
            armor_plate: true,
            ..base
        }
    } else {
        base
    }
}

pub fn figure(
    p: &Painter,
    at: Pos2,
    size: f32,
    kind: EnemyKind,
    boss: bool,
    phase: f32,
    outline: bool,
) {
    let s = size;
    let look = fallback_look(kind, boss);
    let coat = look.coat;
    let skin = C::from_rgb(164, 163, 121);
    p.add(egui::Shape::ellipse_filled(
        at + vec2(s * 0.1, s * 0.55),
        vec2(s * 0.66, s * 0.24),
        C::from_black_alpha(65),
    ));
    let sway = phase.sin() * s * 0.07;
    if outline {
        p.circle_stroke(at, s * 0.78, Stroke::new(1.5, AMBER));
    }
    let leg = phase.sin() * s * 0.19;
    line(
        p,
        at + vec2(s * 0.15, s * 0.18),
        at + vec2(s * 0.25 + leg, s * 0.55),
        s * 0.25,
        C::from_rgb(54, 60, 52),
    );
    line(
        p,
        at + vec2(-s * 0.18, s * 0.19),
        at + vec2(-s * 0.24 - leg, s * 0.52),
        s * 0.25,
        C::from_rgb(64, 70, 59),
    );
    box_at(p, at + vec2(sway, 0.0), vec2(s * 0.88, s * 0.65), coat);
    box_at(
        p,
        at + vec2(s * 0.27 + sway, s * 0.02),
        vec2(s * 0.18, s * 0.6),
        coat.gamma_multiply(0.72),
    );
    line(
        p,
        at + vec2(-s * 0.3, -s * 0.08),
        at + vec2(-s * 0.78, -s * 0.16 + sway),
        s * 0.18,
        coat,
    );
    box_at(
        p,
        at + vec2(-s * 0.83, -s * 0.16 + sway),
        vec2(s * 0.2, s * 0.16),
        skin,
    );
    p.circle_filled(at + vec2(-s * 0.18 + sway, -s * 0.44), s * 0.27, skin);
    box_at(
        p,
        at + vec2(-s * 0.32 + sway, -s * 0.43),
        vec2(s * 0.15, s * 0.08),
        C::from_rgb(64, 59, 47),
    );
    if look.armor_plate {
        box_at(
            p,
            at + vec2(sway, -s * 0.05),
            vec2(s * 0.62, s * 0.36),
            C::from_rgb(75, 89, 87),
        );
        line(
            p,
            at + vec2(-s * 0.24, -s * 0.1),
            at + vec2(s * 0.24, -s * 0.1),
            s * 0.07,
            AMBER.gamma_multiply(0.7),
        );
    }
    if look.charge_pack {
        line(
            p,
            at + vec2(-s * 0.6, 0.0),
            at + vec2(-s * 0.6, -s * 0.9),
            s * 0.12,
            C::from_rgb(93, 82, 60),
        );
        box_at(
            p,
            at + vec2(-s * 0.6, -s * 0.88),
            vec2(s * 0.6, s * 0.22),
            C::from_rgb(133, 140, 131),
        );
    }
    if boss {
        for i in 0..3 {
            box_at(
                p,
                at + vec2((i as f32 - 1.0) * s * 0.23, s * 0.14),
                vec2(s * 0.12, s * 0.22),
                AMBER,
            );
        }
    }
}

pub fn structure(p: &Painter, at: Pos2, size: Vec2, kind: StructureKind) {
    let r = Rect::from_center_size(at, size);
    p.rect_filled(r.translate(vec2(3.0, 4.0)), 1.0, C::from_black_alpha(75));
    match kind {
        StructureKind::Wall => {
            p.rect_filled(r, 1.0, C::from_rgb(120, 115, 89));
            for i in 0..4 {
                let x = r.left() + size.x * (i as f32 + 0.5) / 4.0;
                line(
                    p,
                    pos2(x, r.top()),
                    pos2(x, r.bottom()),
                    2.0,
                    C::from_rgb(76, 80, 67),
                );
            }
            line(
                p,
                r.left_top() + vec2(1.0, 4.0),
                r.right_bottom() - vec2(1.0, 4.0),
                3.0,
                C::from_rgb(154, 145, 101),
            );
            p.rect_stroke(
                r,
                1.0,
                Stroke::new(1.0, C::from_rgb(164, 158, 123)),
                egui::StrokeKind::Inside,
            );
        }
        StructureKind::Trap => {
            p.rect_filled(r, 1.0, C::from_rgb(75, 72, 53));
            for i in 0..5 {
                let x = r.left() + size.x * (i as f32 + 0.5) / 5.0;
                line(
                    p,
                    pos2(x, r.bottom() - 2.0),
                    pos2(x + size.x * 0.08, r.top() + 2.0),
                    2.0,
                    C::from_rgb(194, 185, 145),
                );
            }
            line(
                p,
                r.left_bottom(),
                r.right_bottom(),
                3.0,
                AMBER.gamma_multiply(0.6),
            );
        }
        StructureKind::Turret => {
            for d in [vec2(-1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)] {
                line(p, at, at + d * size * 0.42, 3.0, C::from_rgb(56, 65, 60));
            }
            p.circle_filled(at, size.y * 0.32, C::from_rgb(104, 131, 117));
            line(
                p,
                at,
                at + vec2(size.x * 0.65, 0.0),
                size.y * 0.18,
                C::from_rgb(189, 191, 158),
            );
            box_at(
                p,
                at + vec2(0.0, -size.y * 0.14),
                vec2(size.x * 0.3, size.y * 0.32),
                C::from_rgb(55, 67, 60),
            );
            p.circle_filled(at + vec2(size.x * 0.16, 0.0), 2.0, AMBER);
        }
    }
}

pub(super) fn soft_shadow(p: &Painter, at: Pos2, size: Vec2, alpha: f32) {
    for (spread, opacity) in [(1.35, 10.), (1.12, 15.), (0.9, 23.)] {
        p.add(egui::Shape::ellipse_filled(
            at,
            size * spread,
            C::from_black_alpha((opacity * alpha) as u8),
        ));
    }
}
pub(super) fn structure_fallback(
    p: &Painter,
    at: Pos2,
    size: Vec2,
    kind: StructureKind,
    target: Vec2,
    recoil: f32,
) {
    if kind != StructureKind::Turret {
        structure(p, at, size, kind);
        return;
    }
    let direction = target.normalized();
    for d in [vec2(-1., -1.), vec2(-1., 1.), vec2(1., 1.)] {
        line(p, at, at + d * size * 0.42, 3., C::from_rgb(56, 65, 60));
    }
    p.circle_filled(at, size.y * 0.30, C::from_rgb(104, 131, 117));
    let breech = at - direction * recoil * size.y * 0.10;
    line(
        p,
        breech,
        breech + direction * size.y * 0.70,
        size.y * 0.16,
        C::from_rgb(173, 178, 153),
    );
    p.circle_filled(breech, size.y * 0.18, C::from_rgb(64, 78, 68));
}

pub fn bar(p: &Painter, r: Rect, value: f32, color: C) {
    p.rect_filled(r, 1.0, C::from_black_alpha(100));
    let mut fill = r;
    fill.max.x = fill.min.x + fill.width() * value.clamp(0.0, 1.0);
    p.rect_filled(fill, 1.0, color);
}
