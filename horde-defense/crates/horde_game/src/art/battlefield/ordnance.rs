//! Burning ground and thrown grenades/Molotovs in flight.
use super::super::{AMBER, ArenaProjection, box_at, soft_shadow};
use crate::{
    battle_view::BattleViewModel,
    visual_assets::{self, ArtTextures},
};
use bevy_egui::egui::{self, Color32 as C, Painter, Stroke, vec2};
use horde_core::{content::Mode, ordnance::OrdnanceKind, sim::Attempt};

pub(super) fn fire_zones(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    reduced: bool,
    textures: ArtTextures,
) {
    let unit = vec2(arena.yard_width(), arena.lane_height());
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
        // The authoritative radius remains readable; small pixel flames stay
        // sparse enough that hostiles and contact windups remain legible.
        let count = if reduced { 9 } else { 21 };
        for n in 0..count {
            let theta = n as f32 * 2.399_963 + (zone.id % 19) as f32 * 0.3;
            let spread = ((n + 1) as f32 / (count + 1) as f32).sqrt() * 0.83;
            let point = at + vec2(theta.cos() * radii.x, theta.sin() * radii.y) * spread;
            let flicker = if reduced {
                0.5
            } else {
                ((view.tick / 5 + n * 7) % 5) as f32 / 4.
            };
            let px = (unit.y * 0.055).max(1.);
            let height = px * (2. + flicker * 2.);
            if visual_assets::ordnance_effect(
                p,
                textures,
                true,
                point - vec2(0., height * 0.4),
                vec2(px * 4., height * 2.),
                C::from_white_alpha((fade * 220.) as u8),
            ) {
                continue;
            }
            box_at(
                p,
                point - vec2(0., height * 0.5),
                vec2(px * 2., height),
                C::from_rgba_unmultiplied(218, 104, 38, (fade * 190.) as u8),
            );
            box_at(
                p,
                point - vec2(px * 0.2, height * 0.3),
                vec2(px, height * 0.5),
                C::from_rgba_unmultiplied(246, 187, 79, (fade * 200.) as u8),
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
    let appearance = if a.mode == Mode::Npc {
        horde_core::survivors::Appearance::Female
    } else {
        a.profile.appearance
    };
    let model_id = visual_assets::player_model_id(appearance).ok();
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
    for (projectile, flight) in a.ordnance.projectiles.iter().zip(&view.projectiles) {
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
