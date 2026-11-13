//! Burning ground (its four-frame flames, W5-05) and thrown
//! grenades/Molotovs in flight.
use super::super::{AMBER, ArenaProjection, box_at, soft_shadow};
use crate::{
    battle_view::BattleViewModel,
    visual_assets::{self, ArtTextures},
};
use bevy_egui::egui::{self, Color32 as C, Painter, Stroke, vec2};
use horde_core::{ordnance::OrdnanceKind, sim::Attempt};

/// Flames drawn on one burning zone, and in reduced effects.
pub(crate) const FLAMES_PER_ZONE: usize = 21;
pub(crate) const FLAMES_PER_ZONE_REDUCED: usize = 9;
/// The most zones whose flames are drawn: the newest (highest id). Every
/// zone keeps its ground and its radius ring.
pub(crate) const FIRE_ZONES_DRAWN_MAX: usize = 8;
/// Sim ticks one flame frame lasts.
const FLAME_FRAME_TICKS: u64 = 6;
const FLAME_Y_SCALE: [f32; 4] = [0.85, 1.0, 1.1, 0.95];
const FLAME_TINTS: [[u8; 3]; 2] = [[255, 220, 160], [255, 180, 110]];

/// One frame of a flame's four-frame cycle (W5-05): a mirror flip, a height
/// scale and a tint over the one flame icon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FlameFrame {
    pub mirror: bool,
    pub y_scale: f32,
    pub tint: [u8; 3],
}

/// The frame of flame `index` at sim tick `tick`: `(tick / 6 + index) % 4`,
/// so neighbours are out of step and the fire holds still while the sim is
/// paused. Reduced effects hold frame 0.
pub(crate) fn flame_frame(tick: u64, index: usize, reduced: bool) -> FlameFrame {
    let frame = if reduced {
        0
    } else {
        ((tick / FLAME_FRAME_TICKS + index as u64) % 4) as usize
    };
    FlameFrame {
        mirror: matches!(frame, 1 | 3),
        y_scale: FLAME_Y_SCALE[frame],
        tint: FLAME_TINTS[frame / 2],
    }
}

pub(super) fn fire_zones(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    reduced: bool,
    textures: ArtTextures,
) {
    let unit = vec2(arena.yard_width(), arena.lane_height());
    // Two passes (every zone's ground, then every zone's flames), so the
    // rig-sheet flames form one textured run however many zones burn: one
    // pass per zone alternated untextured and textured runs (W2-47).
    for zone in &view.fire_zones {
        let at = arena.world(zone.center[0], zone.center[1]);
        let radii = unit * zone.radius;
        let fade = (zone.ticks_left as f32 / 45.).min(1.);
        p.add(egui::Shape::ellipse_filled(
            at,
            radii,
            C::from_rgba_unmultiplied(79, 43, 23, (fade * 90.) as u8),
        ));
        p.add(egui::Shape::ellipse_stroke(
            at,
            radii,
            Stroke::new(
                1.,
                C::from_rgba_unmultiplied(229, 136, 58, (fade * 125.) as u8),
            ),
        ));
    }
    let mut burning: Vec<_> = view.fire_zones.iter().collect();
    if burning.len() > FIRE_ZONES_DRAWN_MAX {
        burning.sort_by_key(|zone| std::cmp::Reverse(zone.id));
        burning.truncate(FIRE_ZONES_DRAWN_MAX);
    }
    for zone in burning {
        let at = arena.world(zone.center[0], zone.center[1]);
        let radii = unit * zone.radius;
        let fade = (zone.ticks_left as f32 / 45.).min(1.);
        // The authoritative radius remains readable; small pixel flames stay
        // sparse enough that hostiles and contact windups remain legible.
        let count = if reduced {
            FLAMES_PER_ZONE_REDUCED
        } else {
            FLAMES_PER_ZONE
        };
        for n in 0..count {
            let theta = n as f32 * 2.399_963 + (zone.id % 19) as f32 * 0.3;
            let spread = ((n + 1) as f32 / (count + 1) as f32).sqrt() * 0.83;
            let point = at + vec2(theta.cos() * radii.x, theta.sin() * radii.y) * spread;
            let frame = flame_frame(view.tick, n, reduced);
            let px = (unit.y * 0.055).max(1.);
            let height = px * 4. * frame.y_scale;
            let [r, g, b] = frame.tint;
            if visual_assets::ordnance_effect_flipped(
                p,
                textures,
                true,
                point - vec2(0., height * 0.4),
                vec2(px * 4., height * 2.),
                C::from_rgba_unmultiplied(r, g, b, (fade * 220.) as u8),
                frame.mirror,
            ) {
                continue;
            }
            // No rig sheet: a two-box flame, its bright core leaning with
            // the mirror flip.
            let lean = if frame.mirror { px * 0.2 } else { -px * 0.2 };
            box_at(
                p,
                point - vec2(0., height * 0.5),
                vec2(px * 2., height),
                C::from_rgba_unmultiplied(218, 104, 38, (fade * 190.) as u8),
            );
            box_at(
                p,
                point + vec2(lean, -height * 0.3),
                vec2(px, height * 0.5),
                C::from_rgba_unmultiplied(r, g, b, (fade * 200.) as u8),
            );
        }
    }
}

/// Projectiles fly from the survivor rig's throw socket. The release pose is
/// resolved from the attempt's release cues, so this painter also reads `a`.
pub(super) fn thrown(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    reduced: bool,
    textures: ArtTextures,
    equipment: &horde_core::survivors::EquipmentVisual,
) {
    let r = arena.rect;
    let sy = arena.lane_height();
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    let size = vec2(sy * 0.82, sy * 0.87);
    // The same stamped appearance the rig is drawn with (ruling D17).
    let model_id = visual_assets::player_model_id(a.defender_appearance()).ok();
    let cache_key = egui::Id::new(("lastlight.player_release.v1", p.layer_id().id));
    let mut releases = p
        .ctx()
        .data(|data| data.get_temp::<crate::defender_presentation::ReleaseOrigins>(cache_key))
        .unwrap_or_default();
    let live = a
        .ordnance
        .projectiles
        .iter()
        .map(|projectile| {
            (
                projectile.id,
                crate::defender_presentation::release_tick(a.tick, &a.events, projectile),
            )
        })
        .collect();
    releases.begin_frame(
        a.id,
        a.tick,
        model_id.as_deref().unwrap_or_default(),
        equipment,
        &live,
    );
    for flight in &view.projectiles {
        let Some(projectile) = a
            .ordnance
            .projectiles
            .iter()
            .find(|projectile| projectile.id == flight.id)
        else {
            continue;
        };
        let end = arena.world(flight.target[0], flight.target[1]);
        let born = crate::defender_presentation::release_tick(a.tick, &a.events, projectile);
        let source = releases.source_point((projectile.id, born), || {
            let model_id = model_id.as_deref()?;
            let sample = crate::defender_presentation::release_sample_for_projectile(
                a,
                projectile,
                [end.x, end.y],
                reduced,
            );
            visual_assets::resolve_player_model(model_id, equipment, sample, origin, size)
                .ok()?
                .source_sockets
                .get("throw_origin")
                .copied()
        });
        let start = model_id
            .as_deref()
            .zip(source)
            .and_then(|(model_id, point)| {
                visual_assets::project_player_socket(model_id, point, origin, size).ok()
            })
            .unwrap_or(origin);
        let t = flight.flight;
        let lift = (std::f32::consts::PI * t).sin()
            * sy
            * (0.9 + (end.x - start.x).abs() / r.width() * 1.2);
        let at = start.lerp(end, t) - vec2(0., lift);
        let shadow = start.lerp(end, t);
        soft_shadow(p, shadow, vec2(sy * 0.10, sy * 0.035), 0.45);
        let px = (sy * 0.045).max(1.);
        let painted = visual_assets::ordnance_sprite(
            p,
            textures,
            flight.kind,
            at,
            vec2(sy * 0.2, sy * 0.27),
            if reduced { 0. } else { t * 7. },
        );
        if !painted {
            match flight.kind {
                OrdnanceKind::Grenade => {
                    box_at(p, at, vec2(px * 2.5, px * 3.), C::from_rgb(101, 111, 66));
                    box_at(
                        p,
                        at - vec2(0., px * 2.),
                        vec2(px * 1.5, px),
                        C::from_rgb(193, 190, 143),
                    );
                }
                OrdnanceKind::Molotov => {
                    box_at(p, at, vec2(px * 2., px * 4.), C::from_rgb(112, 139, 84));
                    box_at(
                        p,
                        at - vec2(0., px * 2.4),
                        vec2(px, px * 1.5),
                        C::from_rgb(192, 177, 126),
                    );
                    box_at(
                        p,
                        at - vec2(px, px * 3.),
                        vec2(px * 1.5, px * 1.5),
                        C::from_rgb(239, 164, 59),
                    );
                }
            }
        }
        if !reduced {
            p.circle_stroke(end, sy * 0.11, Stroke::new(1., AMBER.gamma_multiply(0.5)));
        }
    }
    p.ctx()
        .data_mut(|data| data.insert_temp(cache_key, releases));
}
