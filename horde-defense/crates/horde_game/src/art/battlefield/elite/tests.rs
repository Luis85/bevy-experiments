//! W5-07: elite and affix tells.
use super::super::{ArenaProjection, cue::CueKind, enemies};
use super::*;
use crate::{
    animation::SceneAnimation,
    art::{INK, lighting},
    battle_view::SceneBlend,
    visual_assets::{
        ArtTextures, BattleAtlas, WalkSheet,
        enemy_visuals::{self, BUILTIN_ELITE_TELL, EliteTell},
    },
};
use bevy_egui::egui::{self, Painter, Rect, Shape, TextureId};
use horde_core::{enemy_catalog::enemy_catalog, sim::Game};

fn arena() -> Rect {
    Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0))
}

fn textures() -> ArtTextures {
    ArtTextures {
        actors: Some(TextureId::User(1)),
        props: Some(TextureId::User(2)),
        ground: Some(TextureId::User(3)),
        walkers: Some(WalkSheet {
            texture: TextureId::User(4),
            size: [1122, 1402],
        }),
        battle_atlas: Some(BattleAtlas::layout(
            TextureId::User(5),
            [1774, 887],
            [1122, 1402],
        )),
        shadow: Some(TextureId::User(6)),
        ..Default::default()
    }
}

fn painted(paint: impl FnOnce(&Painter)) -> Vec<Shape> {
    let ctx = egui::Context::default();
    let mut paint = Some(paint);
    let mut output = ctx.run_ui(Default::default(), |ui| {
        (paint.take().expect("one pass"))(&ui.painter().with_clip_rect(arena()));
    });
    output.textures_delta.clear();
    output.shapes.into_iter().map(|shape| shape.shape).collect()
}

fn view_of(game: &Game, reduced: bool) -> BattleViewModel {
    let a = game.attempt().unwrap();
    BattleViewModel::build(
        a,
        SceneAnimation::sample(a, reduced),
        SceneBlend::NONE,
        reduced,
    )
}

/// The peak fixture's view with no elite, and its projection.
fn plain() -> (BattleViewModel, ArenaProjection) {
    let game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let mut view = view_of(&game, false);
    for e in &mut view.enemies {
        e.elite = None;
    }
    assert!(view.enemies.len() >= 4, "the peak fixture has hostiles");
    let projection = ArenaProjection::for_attempt(arena(), game.attempt().unwrap());
    (view, projection)
}

fn elite(tell: &'static EliteTell) -> EliteView {
    EliteView {
        tell,
        glow: tell.glow * 0.5,
    }
}

/// The shipped tells: the default, then each of core's affixes.
fn shipped() -> Vec<(Option<&'static str>, &'static EliteTell)> {
    let catalog = enemy_visuals::active();
    std::iter::once((None, catalog.elite_tell(None)))
        .chain(
            enemy_catalog()
                .elite_affix_ids()
                .map(|id| (Some(id), catalog.elite_tell(Some(id)))),
        )
        .collect()
}

fn cue_records(
    view: &BattleViewModel,
    projection: ArenaProjection,
    reduced: bool,
) -> Vec<super::super::cue::CueRecord> {
    let mut batch = CueBatch::new(view.enemies.len());
    enemies::cues(&mut batch, projection, view, false, reduced);
    batch.records
}

fn contrast_with_ink(color: C) -> f32 {
    lighting::contrast(
        lighting::relative_luminance([color.r(), color.g(), color.b()]),
        lighting::relative_luminance([INK.r(), INK.g(), INK.b()]),
    )
}

/// Design-vfx-lighting §7 (acceptance G7.6): an elite draws a ground ring in
/// its tell's colour and width and its tell's shape marker over its health
/// bar, both collected in the cue pass (so painted after the grade) and each
/// on an INK backing it contrasts with by at least 3:1. The view takes the
/// tell from the baked affix (`ResolvedEnemy.elite`).
#[test]
fn elite_enemy_paints_ring_and_marker_with_ink_backing_in_cue_pass() {
    // The view reads the baked affix of a real elite.
    let stress = horde_core::sim::stress_render_fixture().expect("stress fixture");
    let a = stress.attempt().unwrap();
    let view = view_of(&stress, false);
    let catalog = enemy_visuals::active();
    let mut seen = 0;
    for (e, drawn) in a.enemies.iter().filter_map(|e| {
        let drawn = view.enemies.iter().find(|v| v.id == e.id)?;
        Some((e, drawn))
    }) {
        let mark = e.resolved.as_ref().and_then(|r| r.elite.as_ref());
        match (mark, drawn.elite) {
            (Some(mark), Some(shown)) => {
                seen += 1;
                assert_eq!(shown.tell, catalog.elite_tell(Some(&mark.affix_id)));
                assert_eq!(shown.glow, shown.tell.glow_at(a.tick, false));
            }
            (None, None) => {}
            other => panic!("hostile {}: mark and view disagree: {other:?}", e.id),
        }
    }
    assert!(seen > 0, "the stress fixture stages elites");

    let (base, projection) = plain();
    let sy = projection.lane_height();
    let before = cue_records(&base, projection, false);
    for (affix, tell) in shipped() {
        for reduced in [false, true] {
            let mut view = base.clone();
            view.enemies[0].elite = Some(elite(tell));
            let records = cue_records(&view, projection, reduced);
            let color = C::from_rgb(
                tell.outline_rgb[0],
                tell.outline_rgb[1],
                tell.outline_rgb[2],
            );
            let triangles = marker_triangles(tell.marker, pos2(0.0, 0.0), marker_half(sy)).len();
            let added: Vec<_> = records.iter().filter(|r| r.color == color).collect();
            let rings = added.iter().filter(|r| r.kind == CueKind::Ellipse).count();
            let markers = added.iter().filter(|r| r.kind == CueKind::Marker).count();
            let own = |kind: CueKind| {
                before
                    .iter()
                    .filter(|r| r.color == color && r.kind == kind)
                    .count()
            };
            assert_eq!(
                rings - own(CueKind::Ellipse),
                1,
                "{affix:?} (reduced {reduced}): one ground ring"
            );
            assert_eq!(
                markers - own(CueKind::Marker),
                triangles,
                "{affix:?} (reduced {reduced}): the marker's triangles"
            );
            assert_eq!(
                records.len(),
                before.len() + 1 + triangles,
                "{affix:?}: the ring and the marker are all an elite adds"
            );
            for record in added
                .into_iter()
                .filter(|r| matches!(r.kind, CueKind::Ellipse | CueKind::Marker))
            {
                let [r, g, b, _] = record.backing.to_srgba_unmultiplied();
                for (got, want) in [r, g, b].into_iter().zip([INK.r(), INK.g(), INK.b()]) {
                    assert!(got.abs_diff(want) <= 2, "{record:?}: the backing is INK");
                }
                assert!(record.backing.a() >= 228, "{record:?}: backing alpha");
                let ratio = contrast_with_ink(record.color);
                assert!(ratio >= 3.0, "{affix:?}: contrast {ratio:.2} against INK");
            }
        }
    }

    // Geometry: three cue meshes as before; the ring is `ring_px` wide round
    // the feet and the marker sits clear over the health bar.
    let tell = enemy_visuals::active().elite_tell(Some("volatile"));
    let mut view = base.clone();
    view.enemies[0].elite = Some(elite(tell));
    let shapes = painted(|p| {
        let mut batch = CueBatch::new(view.enemies.len());
        enemies::cues(&mut batch, projection, &view, false, false);
        batch.paint(p);
    });
    let meshes: Vec<&egui::Mesh> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(&**mesh),
            _ => None,
        })
        .collect();
    assert_eq!(meshes.len(), 3, "halo, fill and marker meshes");
    let color = tell_color(&elite(tell));
    let bounds = |mesh: &egui::Mesh, of: C| {
        mesh.vertices
            .iter()
            .filter(|v| v.color == of)
            .fold(Rect::NOTHING, |r, v| {
                r.union(Rect::from_min_max(v.pos, v.pos))
            })
    };
    let e = &view.enemies[0];
    let [x, y] = e.drawn();
    let at = projection.world(x, y);
    let ring = bounds(meshes[1], color);
    let half = f32::from(tell.ring_px) * 0.5;
    assert!((ring.center().x - at.x).abs() < 0.01);
    assert!((ring.center().y - (at.y + sy * RING_DROP)).abs() < 0.01);
    assert!((ring.width() * 0.5 - (sy * RING_RADII[0] + half)).abs() < 0.01);
    assert!((ring.height() * 0.5 - (sy * RING_RADII[1] + half)).abs() < 0.01);
    let marker = bounds(meshes[2], color);
    assert!(marker.is_positive(), "the marker is in the marker mesh");
    let bar_top = at.y - sy * enemies::HEALTH_BAR_LIFT - cue::bar_height(sy) * 0.5 - cue::HALO_BAR;
    assert!(
        marker.bottom() + cue::HALO_MARKER * 2.0 <= bar_top,
        "the marker's backing clears the health bar's: {} over {bar_top}",
        marker.bottom()
    );
    assert!(
        (marker.center().x - at.x).abs() < 0.01,
        "centred on the body"
    );
    assert!(
        halo_covers(meshes[0], marker) && halo_covers(meshes[0], ring),
        "INK lies under the ring and the marker"
    );
}

/// Whether the halo mesh `halo` has INK round `rect` on every side.
fn halo_covers(halo: &egui::Mesh, rect: Rect) -> bool {
    let near = rect.expand(8.0);
    let ink = halo
        .vertices
        .iter()
        .filter(|v| near.contains(v.pos) && v.color.r() < 40 && v.color.a() > 200)
        .fold(Rect::NOTHING, |r, v| {
            r.union(Rect::from_min_max(v.pos, v.pos))
        });
    ink.contains_rect(rect)
}

/// Reduced effects (ISO-10): the body tint holds `glow x 0.5` at every tick,
/// where normal effects pulse it between 0 and `glow` on sim ticks; the ring
/// and the marker are the same static cues in both.
#[test]
fn reduced_elite_has_no_pulse() {
    for (affix, tell) in shipped() {
        assert!(tell.glow > 0.0 && tell.pulse_hz > 0.0, "{affix:?} pulses");
        let period = (60.0 / tell.pulse_hz).ceil() as u64;
        let normal: Vec<f32> = (0..=period).map(|tick| tell.glow_at(tick, false)).collect();
        let (low, high) = normal
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), &glow| {
                (low.min(glow), high.max(glow))
            });
        assert!(
            high - low > tell.glow * 0.8 && low >= 0.0 && high <= tell.glow + 1e-6,
            "{affix:?}: normal effects pulse over 0..=glow ({low}..{high})"
        );
        // On sim ticks only: the same tick is the same glow (a paused field
        // holds still).
        assert_eq!(tell.glow_at(77, false), tell.glow_at(77, false));
        for tick in 0..=period * 3 {
            assert_eq!(
                tell.glow_at(tick, true),
                tell.glow * 0.5,
                "{affix:?}: reduced effects hold glow x 0.5 at tick {tick}"
            );
        }
    }

    // The view and the painters: a real elite at two ticks.
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    let textures = textures();
    let snapshot = |game: &Game, reduced: bool| {
        let view = view_of(game, reduced);
        let projection = ArenaProjection::for_attempt(arena(), game.attempt().unwrap());
        let elites: Vec<(u64, EliteView, C)> = view
            .enemies
            .iter()
            .filter_map(|e| Some((e.id, e.elite?, enemies::body_tint(e))))
            .collect();
        let tell_cues: usize = {
            let colors: Vec<C> = elites
                .iter()
                .map(|(_, elite, _)| tell_color(elite))
                .collect();
            cue_records(&view, projection, reduced)
                .into_iter()
                .filter(|r| colors.contains(&r.color))
                .filter(|r| matches!(r.kind, CueKind::Ellipse | CueKind::Marker))
                .count()
        };
        (elites, tell_cues)
    };
    let mut frames = Vec::new();
    for step in [0u64, 7, 13] {
        game.attempt_mut().unwrap().tick += step;
        frames.push((snapshot(&game, false), snapshot(&game, true)));
    }
    let ((first_normal, normal_cues), (first_reduced, reduced_cues)) = frames[0].clone();
    assert!(
        !first_reduced.is_empty(),
        "the stress fixture stages elites"
    );
    assert!(
        normal_cues > 0 && normal_cues == reduced_cues,
        "static cues"
    );
    for (id, shown, tint) in &first_reduced {
        assert_eq!(shown.glow, shown.tell.glow * 0.5, "hostile {id}");
        let base = enemies::tint(view_tint(&game, *id));
        assert_eq!(*tint, glow_tint(base, shown));
        assert_ne!(*tint, base, "hostile {id}: the glow tints the body");
    }
    for ((later_normal, later_normal_cues), (later_reduced, later_reduced_cues)) in &frames[1..] {
        assert_eq!(*later_reduced, first_reduced, "reduced: no pulse");
        assert_eq!(*later_reduced_cues, reduced_cues);
        assert_eq!(*later_normal_cues, normal_cues, "the cues never pulse");
        assert!(
            later_normal
                .iter()
                .zip(&first_normal)
                .any(|(later, first)| later.1.glow != first.1.glow && later.2 != first.2),
            "normal effects pulse the body tint"
        );
    }

    // The world pass paints the same sprite quads, only tinted.
    let view = view_of(&game, true);
    let projection = ArenaProjection::for_attempt(arena(), game.attempt().unwrap());
    let rig = enemies::RigShadow {
        at: pos2(100.0, 400.0),
        size: vec2(20.0, 8.0),
        alpha: 0.8,
    };
    let mut bare = view.clone();
    for e in &mut bare.enemies {
        e.elite = None;
    }
    let vertices = |view: &BattleViewModel| -> Vec<(egui::Pos2, C)> {
        painted(|p| {
            enemies::bodies(p, projection, view, textures, true, rig);
        })
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.vertices.clone()),
            _ => None,
        })
        .flatten()
        .map(|v| (v.pos, v.color))
        .collect()
    };
    let (with, without) = (vertices(&view), vertices(&bare));
    assert_eq!(with.len(), without.len(), "no body geometry is added");
    assert!(
        with.iter()
            .zip(&without)
            .all(|(with, without)| with.0 == without.0),
        "no body moves"
    );
    assert!(
        with.iter()
            .zip(&without)
            .any(|(with, without)| with.1 != without.1),
        "elite bodies are tinted in the world pass"
    );
}

/// The archetype tint of hostile `id` in the view of `game`.
fn view_tint(game: &Game, id: u64) -> Option<[f32; 4]> {
    view_of(game, true)
        .enemies
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| e.tint)
}

/// The glow leans the tint toward the tell's colour: none at 0, the colour
/// itself at 1, and a translucent tint keeps its alpha.
#[test]
fn glow_tint_leans_toward_the_tell_colour() {
    let tell = enemy_visuals::active().elite_tell(Some("hardened"));
    let at = |glow| EliteView { tell, glow };
    let [r, g, b] = tell.outline_rgb;
    assert_eq!(glow_tint(C::WHITE, &at(0.0)), C::WHITE);
    assert_eq!(glow_tint(C::WHITE, &at(1.0)), C::from_rgb(r, g, b));
    let half = glow_tint(C::WHITE, &at(0.5));
    assert!(half.r().abs_diff(((255 + u16::from(r)) / 2) as u8) <= 1);
    assert_eq!(half.a(), 255);
    let ghost = C::from_rgba_unmultiplied(200, 200, 200, 128);
    let leaned = glow_tint(ghost, &at(0.3));
    assert_eq!(leaned.a(), ghost.a());
    assert!(leaned.r() <= leaned.a() && leaned.g() <= leaned.a() && leaned.b() <= leaned.a());
}

/// Design §13 `elite_tells` 48: more elites than the cap draw the ring and
/// marker of the 48 nearest the defence line; every elite keeps its tint.
#[test]
fn elite_tells_capped_at_48_nearest_the_line() {
    let game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    let mut view = view_of(&game, false);
    assert!(view.enemies.len() > ELITE_TELLS_MAX + 12);
    let tell = enemy_visuals::active().elite_tell(None);
    for e in &mut view.enemies {
        e.elite = None;
    }
    let count = ELITE_TELLS_MAX + 12;
    for e in view.enemies.iter_mut().take(count) {
        e.elite = Some(elite(tell));
    }
    let shown = drawn(&view);
    assert_eq!(shown.len(), view.enemies.len());
    assert_eq!(
        shown.iter().filter(|&&shown| shown).count(),
        ELITE_TELLS_MAX
    );
    let key = |e: &EnemyView| (e.position[0], e.id);
    let farthest_shown = view
        .enemies
        .iter()
        .zip(&shown)
        .filter(|(_, shown)| **shown)
        .map(|(e, _)| key(e))
        .fold((f32::MIN, 0), |a, b| if b > a { b } else { a });
    for (e, shown) in view.enemies.iter().zip(&shown) {
        if e.elite.is_some() && !shown {
            assert!(key(e) > farthest_shown, "a nearer elite was dropped");
        }
        assert!(e.elite.is_some() || !shown, "only elites draw a tell");
    }
    let projection = ArenaProjection::for_attempt(arena(), game.attempt().unwrap());
    let color = tell_color(&elite(tell));
    let rings = |view: &BattleViewModel| {
        cue_records(view, projection, false)
            .iter()
            .filter(|r| r.kind == CueKind::Ellipse && r.color == color)
            .count()
    };
    let mut bare = view.clone();
    for e in &mut bare.enemies {
        e.elite = None;
    }
    assert_eq!(rings(&view) - rings(&bare), ELITE_TELLS_MAX);
    // At the cap every elite draws.
    for e in view.enemies.iter_mut().skip(ELITE_TELLS_MAX) {
        e.elite = None;
    }
    assert_eq!(
        drawn(&view).iter().filter(|&&shown| shown).count(),
        ELITE_TELLS_MAX
    );
    assert_eq!(rings(&view) - rings(&bare), ELITE_TELLS_MAX);
}

/// The bundled enemy_visuals.json names a tell for each of core's affixes
/// (W5-07): the default and the affix tells are pairwise distinct by marker
/// shape, so the affix reads without colour, and every marker fits its box.
#[test]
fn shipped_affix_tells_are_distinct_by_shape() {
    let tells = shipped();
    assert_eq!(tells[0].1, &BUILTIN_ELITE_TELL, "the default tell is kept");
    assert!(tells.len() >= 6, "core has five affixes: {}", tells.len());
    for (index, (affix, tell)) in tells.iter().enumerate() {
        for (other_affix, other) in &tells[..index] {
            assert_ne!(
                tell.marker, other.marker,
                "{affix:?} and {other_affix:?} share a marker"
            );
            assert_ne!(
                tell.outline_rgb, other.outline_rgb,
                "{affix:?} and {other_affix:?} share a colour"
            );
        }
        let color = C::from_rgb(
            tell.outline_rgb[0],
            tell.outline_rgb[1],
            tell.outline_rgb[2],
        );
        assert!(contrast_with_ink(color) >= 3.0, "{affix:?} against INK");
    }
    // Every marker shape: inside its box, and unlike every other.
    use enemy_visuals::TellMarker::{Chevron, Crown, Pips1, Pips2, Pips3, Shield, Spark};
    let markers = [Chevron, Crown, Pips1, Pips2, Pips3, Shield, Spark];
    let centre = pos2(100.0, 100.0);
    let shapes: Vec<Vec<[egui::Pos2; 3]>> = markers
        .iter()
        .map(|&marker| marker_triangles(marker, centre, 6.0))
        .collect();
    for (index, shape) in shapes.iter().enumerate() {
        let bounds = marker_bounds(shape);
        assert!(
            Rect::from_center_size(centre, egui::Vec2::splat(12.02)).contains_rect(bounds),
            "{:?} leaves its box: {bounds:?}",
            markers[index]
        );
        assert!(bounds.width() >= 4.0 && bounds.height() >= 3.0);
        assert!(
            (bounds.center().x - centre.x).abs() < 0.01,
            "{:?} is centred",
            markers[index]
        );
        for triangle in shape {
            let [a, b, c] = *triangle;
            let area = ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() * 0.5;
            assert!(area > 1.0, "{:?}: a sliver triangle", markers[index]);
        }
        for other in &shapes[..index] {
            assert_ne!(shape, other);
            assert_ne!(marker_bounds(shape).size(), marker_bounds(other).size());
        }
    }
}
