//! W5-06: damage-scaled hit sparks, the boss kill burst and the no-shake
//! pin.
use super::super::{RenderOptions, battlefield, terrain};
use super::*;
use crate::{
    animation::SceneAnimation,
    art::{SceneLook, SkyClock},
    battle_view::SceneBlend,
};
use bevy_egui::egui::{self, Pos2, Rect, Shape, pos2};
use horde_core::{
    content::BossKind,
    sim::{Attempt, CorpseRecord, Event, Game},
};

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

/// The peak fixture with an empty event log.
fn game() -> Game {
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    game.attempt_mut().unwrap().events.clear();
    game
}

const AT: [f32; 2] = [12.0, 3.5];

/// Logs an event at [`AT`] `ticks_ago` ticks back that names no hostile.
fn event(a: &mut Attempt, kind: EventKind, value: i32, ticks_ago: u64, actor_id: Option<u64>) {
    let seq = a.events.len() as u64 + 1;
    a.events.push_back(Event {
        kind,
        from: a.defender_position(),
        to: AT,
        value,
        tick: a.tick - ticks_ago,
        actor_id,
        target_id: None,
        lane: Some(3),
        duration_ticks: 30,
        seq,
    });
}

/// The body of hostile 7,000,001, felled at [`AT`] `ticks_ago` ticks back.
fn body(a: &Attempt, boss_kind: Option<BossKind>, ticks_ago: u64) -> CorpseRecord {
    CorpseRecord {
        id: 7_000_001,
        kind: a.enemies.first().expect("the fixture stages hostiles").kind,
        boss_kind,
        at: AT,
        visual: None,
        tick: a.tick - ticks_ago,
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

/// The spark mesh [`paint`] adds for `game`: its vertex positions.
fn sparks(game: &Game, reduced: bool) -> Vec<Pos2> {
    let view = view_of(game, reduced);
    let arena = projection(game);
    let shapes = painted(|p| paint(p, arena, &view, reduced));
    assert!(shapes.len() <= 1, "every spark is in one mesh");
    shapes
        .iter()
        .flat_map(|shape| match shape {
            Shape::Mesh(mesh) => {
                assert_eq!(mesh.texture_id, egui::TextureId::default());
                mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>()
            }
            other => panic!("not a mesh: {other:?}"),
        })
        .collect()
}

/// How far from the event point the painted sparks reach.
fn reach(game: &Game, vertices: &[Pos2]) -> f32 {
    let at = projection(game).world(AT[0], AT[1]);
    vertices
        .iter()
        .map(|v| (*v - at).length())
        .fold(0.0, f32::max)
}

/// G7.8: a spark grows with the damage and never leaves its clamps; the
/// painted burst of a harder hit has more sparks and reaches farther; at
/// most the newest 64 impacts draw; reduced effects draw none.
#[test]
fn hit_spark_length_monotonic_in_damage_and_clamped_to_the_lane() {
    let lane = 63.0;
    for damage in 0..3000 {
        let (shorter, longer) = (spark_length(lane, damage), spark_length(lane, damage + 1));
        assert!(longer >= shorter, "damage {damage}");
        assert!((0.06 * lane..=0.25 * lane).contains(&shorter));
    }
    assert_eq!(spark_length(lane, 0), 0.06 * lane);
    assert_eq!(spark_length(lane, -5), 0.06 * lane);
    assert_eq!(spark_length(lane, 3000), 0.25 * lane);
    // 0.05 + 0.04 * log2(1 + 70 / 10) = 0.17.
    assert!((spark_length(lane, 70) - 0.17 * lane).abs() < 1e-4);
    assert!(spark_length(lane, 5) < spark_length(lane, 40));
    assert!(spark_length(lane, 40) < spark_length(lane, 300));
    assert_eq!(
        [20.0, 63.0, 200.0].map(spark_width),
        [1.0, 63.0 * 0.024, 3.0]
    );
    assert_eq!(
        [0, 19, 20, 59, 60, 5000].map(spark_count),
        [4, 4, 6, 6, 8, 8]
    );

    let hit = |value| {
        let mut game = game();
        event(game.attempt_mut().unwrap(), EventKind::Hit, value, 0, None);
        let vertices = sparks(&game, false);
        (vertices.len(), reach(&game, &vertices))
    };
    let (light, medium, heavy) = (hit(5), hit(40), hit(400));
    // A spark is one 4-vertex quad.
    assert_eq!([light.0, medium.0, heavy.0], [4 * 4, 6 * 4, 8 * 4]);
    assert!(
        light.1 < medium.1 && medium.1 < heavy.1,
        "{light:?} {medium:?} {heavy:?}"
    );

    // The newest 64 impacts only.
    let mut game = game();
    for n in 0..100 {
        event(game.attempt_mut().unwrap(), EventKind::Hit, n, 0, None);
    }
    let view = view_of(&game, false);
    let drawn: Vec<_> = drawn(&view).map(|e| e.value).collect();
    assert_eq!(drawn, (36..100).collect::<Vec<_>>());
    let expected: usize = (36..100).map(|damage| spark_count(damage) * 4).sum();
    assert_eq!(sparks(&game, false).len(), expected);
    assert!(expected <= HIT_EVENTS_MAX * SPARKS_PER_HIT_MAX * 4);
    // Reduced effects: no sparks.
    assert!(sparks(&game, true).is_empty());
}

/// A kill bursts 6 sparks of 0.16 lanes; a boss's kill 1.8 times as long.
/// The boss is read from the body the kill left.
#[test]
fn boss_kill_burst_reaches_1_8_times_a_kill_burst() {
    let kill = |boss: bool| {
        let mut game = game();
        let a = game.attempt_mut().unwrap();
        let level_boss = horde_core::content::content().levels[a.level].boss;
        let body = body(a, boss.then_some(level_boss), 0);
        a.corpses.push(body);
        event(a, EventKind::Kill, 12, 0, Some(7_000_001));
        let view = view_of(&game, false);
        let lane = projection(&game).lane_height();
        let event = *view.fx.last().unwrap();
        assert_eq!(event.boss, boss);
        let burst = burst(&event, lane).unwrap();
        assert_eq!(burst.count, KILL_SPARKS);
        let vertices = sparks(&game, false);
        assert_eq!(vertices.len(), KILL_SPARKS * 4);
        (burst.length / lane, reach(&game, &vertices))
    };
    let (plain, boss) = (kill(false), kill(true));
    assert!((plain.0 - 0.16).abs() < 1e-6);
    assert!((boss.0 / plain.0 - 1.8).abs() < 1e-5);
    assert!(boss.1 > plain.1);
    assert_eq!(KILL_SPARKS, 6);
}

fn options(reduced: bool) -> RenderOptions {
    RenderOptions {
        construction: false,
        outlines: false,
        numbers: true,
        reduced,
        blood: true,
        scene: SceneLook::neutral(0),
        sky: SkyClock::default(),
        canvas: arena().expand(24.0),
        textures: Default::default(),
        occluder: None,
    }
}

/// ISO-9, no screen shake: survivor damage, an explosion, a heavy hit and a
/// boss kill, at every age of their effects, leave the field where it was.
/// The ground (the first thing the battlefield paints) is drawn exactly as
/// on a quiet field in the same rectangle, and the projection, every
/// hostile and the survivor keep their place, in normal and reduced
/// effects.
#[test]
fn field_rect_never_offsets_on_damage_or_explosion() {
    let quiet = game();
    let rect = arena();
    let ground = painted(|p| terrain::paint(p, projection(&quiet), 0, Default::default()));
    assert!(!ground.is_empty());
    let places = |view: &BattleViewModel| {
        (
            view.enemies.iter().map(|e| e.drawn()).collect::<Vec<_>>(),
            view.defender.position,
        )
    };
    for ticks_ago in 0..=27 {
        let mut game = game();
        let a = game.attempt_mut().unwrap();
        let level_boss = horde_core::content::content().levels[a.level].boss;
        let body = body(a, Some(level_boss), ticks_ago);
        a.corpses.push(body);
        event(a, EventKind::Damage, 40, ticks_ago, None);
        event(a, EventKind::Explosion, 250, ticks_ago, Some(901));
        event(a, EventKind::Hit, 5000, ticks_ago, None);
        event(a, EventKind::Kill, 12, ticks_ago, Some(7_000_001));
        let arena = projection(&game);
        assert_eq!(
            (arena.rect, arena.lanes, arena.width),
            (rect, projection(&quiet).lanes, projection(&quiet).width)
        );
        for reduced in [false, true] {
            let view = view_of(&game, reduced);
            assert!(
                view.fx.iter().any(|e| e.kind == EventKind::Damage),
                "the damage event is live at {ticks_ago} ticks"
            );
            assert_eq!(places(&view), places(&view_of(&quiet, reduced)));
            let a = game.attempt().unwrap();
            let shapes = painted(|p| {
                battlefield(p, rect, &view, a, [10.0, 5.0], options(reduced));
            });
            assert!(
                shapes[..ground.len()] == ground[..],
                "the ground moved at {ticks_ago} ticks (reduced {reduced})"
            );
        }
    }
}
