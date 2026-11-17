//! W5-05: multi-frame bursts, grenade against Molotov, scorch marks and the
//! four-frame fire-zone flames.
use super::super::ordnance::{self, FLAMES_PER_ZONE, FLAMES_PER_ZONE_REDUCED, flame_frame};
use super::*;
use crate::{
    animation::SceneAnimation,
    battle_view::{SCORCH_MAX, SceneBlend},
    visual_assets::{RigAtlas, RigSheet},
};
use bevy_egui::egui::{self, Rect, TextureId, pos2};
use horde_core::{
    ordnance::FireZone,
    sim::{Attempt, Event, Game},
};
use std::collections::BTreeSet;

fn arena() -> Rect {
    Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0))
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

/// The kind of every painted shape, sorted: what a player can tell apart
/// without colour.
fn signature(shapes: &[Shape]) -> Vec<&'static str> {
    let mut kinds: Vec<_> = shapes
        .iter()
        .map(|shape| match shape {
            Shape::Ellipse(ellipse) if ellipse.fill == C::TRANSPARENT => "ring",
            Shape::Ellipse(_) => "disc",
            Shape::LineSegment { .. } => "line",
            Shape::Rect(_) => "box",
            Shape::Mesh(_) => "icon",
            _ => "other",
        })
        .collect();
    kinds.sort_unstable();
    kinds
}

fn count(shapes: &[Shape], kind: &str) -> usize {
    signature(shapes).iter().filter(|k| **k == kind).count()
}

/// The peak fixture with an empty event log and no burning ground.
fn game() -> Game {
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt_mut().unwrap();
    a.events.clear();
    a.ordnance.fire_zones.clear();
    game
}

fn event(a: &mut Attempt, kind: EventKind, ticks_ago: u64, projectile: u64) {
    let seq = a.events.len() as u64 + 1;
    a.events.push_back(Event {
        kind,
        from: a.defender_position(),
        to: [12.0, 3.5],
        value: 250,
        tick: a.tick - ticks_ago,
        actor_id: Some(projectile),
        target_id: None,
        lane: Some(3),
        duration_ticks: 30,
        seq,
    });
}

/// A burst `ticks_ago` ticks back; a Molotov also logs its Burn event.
fn burst(a: &mut Attempt, kind: OrdnanceKind, ticks_ago: u64, projectile: u64) {
    event(a, EventKind::Explosion, ticks_ago, projectile);
    if kind == OrdnanceKind::Molotov {
        event(a, EventKind::Burn, ticks_ago, projectile);
    }
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

fn projection(game: &Game) -> ArenaProjection {
    ArenaProjection::for_attempt(arena(), game.attempt().unwrap())
}

/// One burst of `kind`, `ticks_ago` ticks old, painted through [`paint`].
fn paint_burst(
    kind: OrdnanceKind,
    ticks_ago: u64,
    reduced: bool,
    textures: ArtTextures,
) -> Vec<Shape> {
    let mut game = game();
    burst(game.attempt_mut().unwrap(), kind, ticks_ago, 901);
    let view = view_of(&game, reduced);
    let arena = projection(&game);
    painted(|p| paint(p, arena, &view, reduced, textures))
}

fn blast(ordnance: OrdnanceKind, age: f32) -> Blast {
    Blast {
        at: pos2(400.0, 400.0),
        range: vec2(120.0, 80.0),
        lane: 40.0,
        age,
        ordnance,
        phase: 0.0,
    }
}

/// The middle of frame `index`, in seconds, and in ticks.
fn frame_age(index: usize) -> f32 {
    frame_tick(index) as f32 / 60.0
}
fn frame_tick(index: usize) -> u64 {
    index as u64 * 5 + 2
}

fn parts(prims: &[Prim]) -> BTreeSet<Part> {
    prims.iter().map(|prim| prim.part).collect()
}

const KINDS: [OrdnanceKind; 2] = [OrdnanceKind::Grenade, OrdnanceKind::Molotov];

/// G7.8: at every frame a grenade and a Molotov are built from different
/// parts and paint different shape kinds, and the Molotov never has the
/// grenade's ring. In reduced effects the grenade is one solid ring and the
/// Molotov one dashed ring: distinct without colour.
#[test]
fn grenade_and_molotov_explosions_paint_distinct_shape_sets() {
    for index in 0..EXPLOSION_FRAMES {
        let age = frame_age(index);
        assert_eq!(frame(age).0, index);
        let grenade = parts(&prims(&blast(OrdnanceKind::Grenade, age), false));
        let molotov = parts(&prims(&blast(OrdnanceKind::Molotov, age), false));
        assert_ne!(grenade, molotov, "frame {index}");
        for part in [Part::Core, Part::ShockRing, Part::Debris, Part::Puff] {
            assert!(!molotov.contains(&part), "frame {index}: Molotov {part:?}");
        }
        for part in [Part::Glint, Part::Bloom, Part::Tongue] {
            assert!(!grenade.contains(&part), "frame {index}: grenade {part:?}");
        }
        let tick = frame_tick(index);
        let [g, m] = KINDS.map(|kind| paint_burst(kind, tick, false, Default::default()));
        assert_ne!(signature(&g), signature(&m), "frame {index}");
        assert_eq!(count(&m, "ring"), 0, "frame {index}: a Molotov has no ring");
    }
    // The view tells the two apart from the event log alone.
    let mut game = game();
    let a = game.attempt_mut().unwrap();
    burst(a, OrdnanceKind::Grenade, 3, 901);
    burst(a, OrdnanceKind::Molotov, 2, 902);
    let view = view_of(&game, false);
    let kinds: Vec<_> = blasts(projection(&game), &view)
        .iter()
        .map(|blast| blast.ordnance)
        .collect();
    assert_eq!(kinds, KINDS);
    // Reduced: one solid ring against one dashed ring.
    for tick in [0, 14, 29] {
        let [g, m] = KINDS.map(|kind| paint_burst(kind, tick, true, Default::default()));
        assert_eq!(signature(&g), ["ring"], "tick {tick}");
        assert_eq!(signature(&m), ["line"; REDUCED_DASHES], "tick {tick}");
    }
}

/// G7.8: each ordnance plays at least four frames that differ in their
/// parts and in the shape kinds painted (all six do), over the event's half
/// second.
#[test]
fn explosion_has_at_least_4_distinct_frames_per_ordnance() {
    let indices: BTreeSet<_> = (0..30).map(|tick| frame(tick as f32 / 60.0).0).collect();
    assert_eq!(indices.len(), EXPLOSION_FRAMES);
    for kind in KINDS {
        let by_parts: BTreeSet<_> = (0..EXPLOSION_FRAMES)
            .map(|index| parts(&prims(&blast(kind, frame_age(index)), false)))
            .collect();
        assert!(
            by_parts.len() >= 4,
            "{kind:?}: {} part sets",
            by_parts.len()
        );
        let by_shapes: BTreeSet<_> = (0..EXPLOSION_FRAMES)
            .map(|index| {
                signature(&paint_burst(
                    kind,
                    frame_tick(index),
                    false,
                    Default::default(),
                ))
            })
            .collect();
        assert!(
            by_shapes.len() >= 4,
            "{kind:?}: {} painted frames",
            by_shapes.len()
        );
        // Nothing is left once the event's life is over.
        assert!(paint_burst(kind, 30, false, Default::default()).is_empty());
    }
}

/// Reduced effects: the ring is the same at every age (no keyframes, no
/// fade) and no icon is drawn.
#[test]
fn reduced_explosion_is_one_static_ring_without_icons() {
    for kind in KINDS {
        let early = prims(&blast(kind, 0.02), true);
        let late = prims(&blast(kind, 0.48), true);
        assert_eq!(early, late, "{kind:?}");
        assert!(
            early
                .iter()
                .all(|prim| !matches!(prim.geom, Geom::Sprite { .. })),
            "{kind:?}"
        );
    }
    assert_eq!(
        parts(&prims(&blast(OrdnanceKind::Grenade, 0.2), true)),
        BTreeSet::from([Part::SolidRing])
    );
    assert_eq!(
        parts(&prims(&blast(OrdnanceKind::Molotov, 0.2), true)),
        BTreeSet::from([Part::Dash])
    );
}

/// The rig icons are magnified at most 1.5 screen pixels per texel, however
/// wide the blast; the vector parts carry its size.
#[test]
fn explosion_icons_are_magnified_at_most_1_5x() {
    assert_eq!(
        clamp_sprite(vec2(300.0, 300.0), vec2(64.0, 64.0), 1.0),
        vec2(96.0, 96.0)
    );
    assert_eq!(
        clamp_sprite(vec2(300.0, 300.0), vec2(64.0, 64.0), 2.0),
        vec2(48.0, 48.0)
    );
    // Never grown.
    assert_eq!(
        clamp_sprite(vec2(10.0, 20.0), vec2(64.0, 64.0), 1.0),
        vec2(10.0, 20.0)
    );
    // A 64 px sheet has 16-texel cells: no icon is wider than 24 points.
    let textures = ArtTextures {
        rig: Some(RigSheet {
            texture: TextureId::User(7),
            atlas: RigAtlas::grid([64.0, 64.0]),
        }),
        ..Default::default()
    };
    let mut icons = 0;
    for kind in KINDS {
        for index in 0..EXPLOSION_FRAMES {
            for shape in paint_burst(kind, frame_tick(index), false, textures) {
                if let Shape::Mesh(mesh) = shape {
                    icons += 1;
                    let bounds = mesh.calc_bounds();
                    assert!(
                        bounds.width() <= 24.01 && bounds.height() <= 24.01,
                        "{kind:?} frame {index}: icon {:?}",
                        bounds.size()
                    );
                }
            }
        }
    }
    assert!(icons >= 4, "both kinds draw rig icons: {icons}");
}

/// Budget (design section 13): at most 8 bursts and 8 scorch marks a frame,
/// the newest ones.
#[test]
fn bursts_and_scorch_marks_are_capped_at_the_newest_8() {
    let mut game = game();
    let a = game.attempt_mut().unwrap();
    for ticks_ago in (0..12).rev() {
        burst(a, OrdnanceKind::Grenade, ticks_ago, 900 + ticks_ago);
    }
    let view = view_of(&game, false);
    let bursts = blasts(projection(&game), &view);
    assert_eq!(bursts.len(), EXPLOSIONS_MAX);
    assert!(bursts.iter().all(|blast| blast.age < 8.0 / 60.0));
    // Oldest first, so the newest paints on top.
    assert!(bursts.windows(2).all(|pair| pair[0].age > pair[1].age));
    assert_eq!(view.scorch.len(), SCORCH_MAX);
    assert!(view.scorch.iter().all(|mark| mark.age < 8.0 / 60.0));
}

/// The scorch tail: a burst leaves a ground mark for 3 s after its frames
/// end, fading as it ages; reduced effects hold one alpha (no motion).
#[test]
fn scorch_mark_outlives_the_burst_and_fades_over_3_seconds() {
    let scorch_at = |ticks_ago: u64, reduced: bool| {
        let mut game = game();
        burst(
            game.attempt_mut().unwrap(),
            OrdnanceKind::Grenade,
            ticks_ago,
            901,
        );
        let view = view_of(&game, reduced);
        let arena = projection(&game);
        assert!(blasts(arena, &view).is_empty(), "the frames are over");
        let shapes = painted(|p| scorch(p, arena, &view, reduced));
        assert_eq!(shapes.len(), view.scorch.len());
        view.scorch
            .first()
            .map(|mark| scorch_prim(arena, mark, reduced))
    };
    let alpha = |prim: Option<Prim>| prim.expect("a scorch mark").color.a();
    let (young, old) = (alpha(scorch_at(40, false)), alpha(scorch_at(150, false)));
    assert!(young > old && old > 0, "alpha {young} then {old}");
    assert_eq!(
        scorch_at(180, false).map(|prim| prim.part),
        Some(Part::Scorch)
    );
    assert_eq!(scorch_at(181, false), None);
    assert_eq!(alpha(scorch_at(40, true)), alpha(scorch_at(150, true)));
}

fn fire_zone(a: &mut Attempt, id: u64) {
    a.ordnance.fire_zones.push(FireZone {
        id,
        cell: [1, 4],
        radius: 2.5,
        damage: 1,
        ticks_left: 600,
        tick_left: 10,
        tick_interval: 10,
    });
}

/// Fire zones: each flame cycles four frames (mirror, height and tint) every
/// 6 sim ticks, so it holds still while paused; 21 flames a zone, and 9
/// static frame-0 flames in reduced effects.
#[test]
fn fire_zone_flames_cycle_4_frames_and_hold_frame_0_when_reduced() {
    let cycle: Vec<_> = (0..4).map(|step| flame_frame(step * 6, 0, false)).collect();
    for (n, left) in cycle.iter().enumerate() {
        for right in &cycle[n + 1..] {
            assert_ne!(left, right);
        }
    }
    assert_eq!(flame_frame(24, 0, false), cycle[0]);
    assert_eq!(flame_frame(5, 0, false), cycle[0], "a frame lasts 6 ticks");
    assert_eq!(flame_frame(0, 1, false), cycle[1], "neighbours are offset");
    for tick in [0, 6, 13, 400] {
        assert_eq!(flame_frame(tick, 3, true), cycle[0], "reduced, tick {tick}");
    }
    let mut game = game();
    fire_zone(game.attempt_mut().unwrap(), 7);
    let arena = projection(&game);
    let paint_at = |tick: u64, reduced: bool| {
        let mut view = view_of(&game, reduced);
        view.tick = tick;
        painted(|p| ordnance::fire_zones(p, arena, &view, reduced, Default::default()))
    };
    // Without the rig sheet a flame is two boxes.
    assert_eq!(count(&paint_at(600, false), "box"), FLAMES_PER_ZONE * 2);
    assert_eq!(
        count(&paint_at(600, true), "box"),
        FLAMES_PER_ZONE_REDUCED * 2
    );
    assert_eq!((FLAMES_PER_ZONE, FLAMES_PER_ZONE_REDUCED), (21, 9));
    assert_ne!(paint_at(600, false), paint_at(606, false));
    assert_eq!(
        paint_at(600, false),
        paint_at(605, false),
        "frozen in a frame"
    );
    assert_eq!(
        paint_at(600, true),
        paint_at(606, true),
        "reduced is static"
    );
    assert_eq!(
        paint_at(600, true),
        paint_at(613, true),
        "reduced is static"
    );
}

/// Molotov detection: a Burn event of the same projectile and tick, or
/// burning ground with that id; a Shot carries no ordnance.
#[test]
fn explosion_ordnance_is_molotov_with_a_same_tick_burn_or_its_fire_zone() {
    let ordnance_of = |game: &Game| {
        view_of(game, false)
            .fx
            .iter()
            .filter(|e| e.kind == EventKind::Explosion)
            .map(|e| e.ordnance)
            .collect::<Vec<_>>()
    };
    let mut grenade = game();
    burst(
        grenade.attempt_mut().unwrap(),
        OrdnanceKind::Grenade,
        2,
        901,
    );
    // Another projectile's Burn, and this one's at another tick, do not count.
    event(grenade.attempt_mut().unwrap(), EventKind::Burn, 2, 555);
    event(grenade.attempt_mut().unwrap(), EventKind::Burn, 1, 901);
    assert_eq!(ordnance_of(&grenade), [Some(OrdnanceKind::Grenade)]);
    let mut molotov = game();
    burst(
        molotov.attempt_mut().unwrap(),
        OrdnanceKind::Molotov,
        2,
        901,
    );
    assert_eq!(ordnance_of(&molotov), [Some(OrdnanceKind::Molotov)]);
    // The Burn event rolled out of the log: the fire zone still names it.
    let mut zone = game();
    burst(zone.attempt_mut().unwrap(), OrdnanceKind::Grenade, 2, 901);
    fire_zone(zone.attempt_mut().unwrap(), 901);
    assert_eq!(ordnance_of(&zone), [Some(OrdnanceKind::Molotov)]);
    let mut shot = game();
    event(shot.attempt_mut().unwrap(), EventKind::Shot, 1, 901);
    assert!(
        view_of(&shot, false)
            .fx
            .iter()
            .all(|e| e.ordnance.is_none())
    );
}
