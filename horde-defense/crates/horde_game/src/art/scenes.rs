use super::{AMBER, TEXT, box_at, line, structure};
use crate::visual_assets::{self, ArtTextures};
use bevy_egui::egui::{self, Align2, Color32 as C, FontId, Painter, Rect, Stroke, pos2, vec2};
use horde_core::content::StructureKind;

pub fn camp_scene(p: &Painter, r: Rect, time: f32) {
    p.rect_filled(r, 0.0, C::from_rgb(47, 62, 59));
    let horizon = r.top() + r.height() * 0.37;
    // Layered, original skyline and checkpoint silhouette.
    for i in 0..22 {
        let x = r.left() + i as f32 * r.width() / 21.0;
        let h = 30.0 + ((i * 37) % 73) as f32;
        let b = Rect::from_min_max(
            pos2(x, horizon - h),
            pos2(x + r.width() / 18.0, horizon + 20.0),
        );
        p.rect_filled(b, 0.0, C::from_rgb(38, 51, 49));
        for w in 0..3 {
            box_at(
                p,
                pos2(x + 8.0 + w as f32 * 9.0, horizon - h + 12.0),
                vec2(3.0, 5.0),
                C::from_rgb(64, 75, 65),
            );
        }
    }
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), horizon), r.max),
        0.0,
        C::from_rgb(40, 48, 40),
    );
    let road = vec![
        pos2(r.left() + r.width() * 0.38, horizon),
        pos2(r.left() + r.width() * 0.65, horizon),
        pos2(r.right(), r.bottom()),
        pos2(r.left(), r.bottom()),
    ];
    p.add(egui::Shape::convex_polygon(
        road,
        C::from_rgb(53, 57, 48),
        Stroke::NONE,
    ));
    for i in 0..12 {
        let x = r.left() + i as f32 * r.width() / 11.0;
        line(
            p,
            pos2(x, horizon + 20.0),
            pos2(x, r.bottom()),
            1.0,
            C::from_rgb(86, 94, 76),
        );
    }
    for j in 0..8 {
        let y = horizon + 20.0 + j as f32 * 30.0;
        line(
            p,
            pos2(r.left(), y),
            pos2(r.right(), y),
            1.0,
            C::from_rgb(75, 85, 70),
        );
    }
    let base = pos2(r.left() + r.width() * 0.64, r.top() + r.height() * 0.69);
    for i in 0..6 {
        structure(
            p,
            base + vec2((i as f32 - 2.5) * 33.0, 0.0),
            vec2(30.0, 27.0),
            StructureKind::Wall,
        );
    }
    structure(
        p,
        base + vec2(80.0, -24.0),
        vec2(36.0, 32.0),
        StructureKind::Turret,
    );
    let lamp = base + vec2(-116.0, -100.0);
    line(
        p,
        lamp,
        lamp + vec2(0.0, 150.0),
        5.0,
        C::from_rgb(24, 33, 29),
    );
    line(
        p,
        lamp,
        lamp + vec2(28.0, 0.0),
        4.0,
        C::from_rgb(24, 33, 29),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            lamp + vec2(27.0, 4.0),
            lamp + vec2(-25.0, 160.0),
            lamp + vec2(115.0, 160.0),
        ],
        C::from_rgba_unmultiplied(230, 186, 92, 14),
        Stroke::NONE,
    ));
    box_at(p, lamp + vec2(27.0, 3.0), vec2(16.0, 5.0), AMBER);
    for i in 0..8 {
        let x = (i as f32 * 0.127 + time * 0.006).fract();
        let y = (i as f32 * 0.313 + time * 0.008).fract();
        p.circle_filled(
            r.min + vec2(r.width() * x, r.height() * y),
            1.0,
            C::from_rgba_unmultiplied(206, 186, 125, 60),
        );
    }
}

pub fn country_scene_textured(
    p: &Painter,
    r: Rect,
    biome: &str,
    culture: &str,
    stage: usize,
    textures: ArtTextures,
) {
    if visual_assets::panorama(p, r, textures, biome) {
        p.rect_filled(
            Rect::from_min_max(pos2(r.left(), r.bottom() - 22.), r.max),
            0.,
            C::from_black_alpha(180),
        );
        p.text(
            r.left_bottom() + vec2(10., -5.),
            Align2::LEFT_BOTTOM,
            format!("{} · {}", biome.to_uppercase(), culture),
            FontId::proportional(11.),
            TEXT,
        );
    } else {
        country_scene(p, r, biome, culture, stage);
    }
}
pub fn country_scene(p: &Painter, r: Rect, biome: &str, culture: &str, stage: usize) {
    let (sky, ground) = match biome {
        "Arid" => (C::from_rgb(155, 132, 89), C::from_rgb(105, 88, 60)),
        "Boreal" => (C::from_rgb(129, 154, 159), C::from_rgb(94, 111, 104)),
        "Tropical" => (C::from_rgb(87, 123, 106), C::from_rgb(54, 86, 58)),
        "Alpine" => (C::from_rgb(110, 135, 142), C::from_rgb(74, 87, 80)),
        _ => (C::from_rgb(109, 130, 123), C::from_rgb(66, 82, 63)),
    };
    p.rect_filled(r, 0.0, sky);
    if biome == "Alpine" || biome == "Boreal" {
        for n in 0..7 {
            let x = r.left() + r.width() * n as f32 / 6.0;
            let h = r.height() * (0.5 + (n % 3) as f32 * 0.12);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x - 100.0, r.bottom()),
                    pos2(x, r.bottom() - h),
                    pos2(x + 120.0, r.bottom()),
                ],
                ground.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
    }
    let horizon = r.top() + r.height() * 0.68;
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), horizon), r.max),
        0.0,
        ground,
    );
    let east = culture.contains("Asia");
    let tropical = biome == "Tropical";
    let arid = biome == "Arid";
    for n in 0..13 {
        let x = r.left() + n as f32 * r.width() / 12.0;
        let w = r.width() / 17.0;
        let h = 18.0 + ((n * 17 + stage * 11) % 23) as f32;
        let material = if arid {
            C::from_rgb(170, 146, 101)
        } else if tropical {
            C::from_rgb(106, 109, 70)
        } else {
            C::from_rgb(79, 91, 82)
        };
        let building = Rect::from_min_max(pos2(x, horizon - h), pos2(x + w, horizon));
        p.rect_filled(building, 0.0, material);
        if !arid {
            let peak = if east { 12.0 } else { 8.0 };
            p.add(egui::Shape::convex_polygon(
                vec![
                    building.left_top() - vec2(6.0, 0.0),
                    pos2(building.center().x, building.top() - peak),
                    building.right_top() + vec2(6.0, 0.0),
                ],
                C::from_rgb(61, 72, 62),
                Stroke::NONE,
            ));
        }
        for window in 0..3 {
            box_at(
                p,
                pos2(x + 7.0 + window as f32 * w / 4.0, horizon - h + 8.0),
                vec2(4.0, 6.0),
                C::from_rgb(37, 51, 46),
            );
        }
        if tropical {
            let at = pos2(x + w + 9.0, horizon);
            line(p, at, at + vec2(3.0, -38.0), 3.0, C::from_rgb(55, 75, 48));
            for d in [-1.0, 1.0] {
                line(
                    p,
                    at + vec2(3.0, -37.0),
                    at + vec2(d * 19.0, -25.0),
                    4.0,
                    C::from_rgb(50, 82, 45),
                );
            }
        }
    }
    for i in 0..3 {
        let at = pos2(r.right() - 70.0 - i as f32 * 55.0, horizon + 4.0);
        box_at(p, at, vec2(43.0, 12.0), C::from_rgb(61, 75, 67));
        box_at(
            p,
            at + vec2(-2.0, -9.0),
            vec2(27.0, 12.0),
            C::from_rgb(76, 88, 73),
        );
        for dx in [-13.0, 13.0] {
            p.circle_filled(at + vec2(dx, 7.0), 4.0, C::from_rgb(25, 37, 33));
        }
    }
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), r.bottom() - 24.0), r.max),
        0.0,
        C::from_black_alpha(140),
    );
    p.text(
        r.left_bottom() + vec2(10.0, -6.0),
        Align2::LEFT_BOTTOM,
        format!("{} · {}", biome.to_uppercase(), culture),
        FontId::proportional(11.0),
        TEXT,
    );
}
