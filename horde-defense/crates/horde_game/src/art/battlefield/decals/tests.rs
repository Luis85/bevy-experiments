//! W5-06: blood decals are opt-in, capped at the newest 64 bodies and
//! static in reduced effects.
use super::*;
use bevy_egui::egui::{self, Rect, Shape, pos2};
use horde_core::sim::Game;
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

/// The peak fixture with `count` saved bodies of its first hostile's kind,
/// with ids from 5,000,000 at ascending ticks; the newest `fresh` died this
/// tick, the rest long ago.
fn game(count: usize, fresh: usize) -> Game {
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt_mut().unwrap();
    a.tick = 1_000_000;
    let kind = a.enemies.first().expect("the fixture stages hostiles").kind;
    a.corpses = (0..count)
        .map(|n| CorpseRecord {
            id: 5_000_000 + n as u64,
            kind,
            boss_kind: None,
            at: [3.0 + (n % 20) as f32, 0.5 + (n % 9) as f32],
            visual: None,
            tick: if n + fresh < count { n as u64 } else { a.tick },
        })
        .collect();
    game
}

fn paint_of(game: &Game, blood: bool, reduced: bool) -> Vec<Shape> {
    let a = game.attempt().unwrap();
    let arena = ArenaProjection::for_attempt(arena(), a);
    painted(|p| paint(p, arena, a, blood, reduced))
}

/// G7.8: no stain with the blood setting off; with it on, one stain for
/// each of the newest 64 bodies, 3 to 5 hash-shaped blobs each, all in one
/// untextured mesh of the stain colour.
#[test]
fn blood_decals_zero_when_off_newest_64_when_on() {
    let game = game(100, 0);
    let a = game.attempt().unwrap();
    for reduced in [false, true] {
        assert!(decals(a, false, reduced).is_empty());
        assert!(paint_of(&game, false, reduced).is_empty());
    }
    let on = decals(a, true, false);
    assert_eq!(on.len(), BLOOD_DECALS_MAX);
    assert_eq!(BLOOD_DECALS_MAX, 64);
    assert_eq!(
        on.iter().map(|decal| decal.id).collect::<Vec<_>>(),
        (5_000_036..5_000_100).collect::<Vec<u64>>(),
        "the newest 64 bodies, oldest first"
    );
    for decal in &on {
        assert!((3..=5).contains(&decal.blobs().len()), "{decal:?}");
        for blob in decal.blobs() {
            assert!(blob.radii[0] > 0.0 && blob.radii[1] > 0.0);
        }
    }
    // Hash-shaped: the stains differ in blob count and in shape, and the
    // same body always gets the same stain.
    let counts: BTreeSet<_> = on.iter().map(|decal| decal.blobs().len()).collect();
    assert_eq!(counts, BTreeSet::from([3, 4, 5]));
    let shapes: BTreeSet<_> = on
        .iter()
        .map(|decal| format!("{:?}", decal.blobs()))
        .collect();
    assert_eq!(shapes.len(), on.len());
    assert_eq!(on, decals(a, true, false));

    let shapes = paint_of(&game, true, false);
    assert_eq!(shapes.len(), 1, "one mesh for every stain");
    let Shape::Mesh(mesh) = &shapes[0] else {
        panic!("not a mesh: {:?}", shapes[0]);
    };
    assert_eq!(mesh.texture_id, egui::TextureId::default());
    assert_eq!(
        blood_color(),
        egui::Color32::from_rgba_unmultiplied(78, 30, 26, 150)
    );
    assert!(mesh.vertices.iter().all(|v| v.color == blood_color()));
    let blobs: usize = on.iter().map(|decal| decal.blobs().len()).sum();
    assert_eq!(mesh.vertices.len(), blobs * 9);
    assert!(blobs <= BLOOD_DECALS_MAX * BLOBS_MAX);

    // Below the cap every body has one.
    let few = self::game(10, 0);
    assert_eq!(decals(few.attempt().unwrap(), true, false).len(), 10);
}

/// A fresh stain spreads over its first ticks and then holds; reduced
/// effects paint the spread stain from the first frame, the same on every
/// later frame.
#[test]
fn reduced_blood_decals_are_static() {
    let mut game = game(8, 8);
    let fresh = |game: &Game, reduced| {
        let a = game.attempt().unwrap();
        (decals(a, true, reduced), paint_of(game, true, reduced))
    };
    let (normal_at_death, normal_paint_at_death) = fresh(&game, false);
    let (reduced_at_death, reduced_paint_at_death) = fresh(&game, true);
    assert!(normal_at_death.iter().all(|decal| decal.spread == 0.4));
    assert!(reduced_at_death.iter().all(|decal| decal.spread == 1.0));
    let mut last = 0.4;
    for _ in 0..SPREAD_TICKS {
        game.attempt_mut().unwrap().tick += 1;
        let spread = fresh(&game, false).0[0].spread;
        assert!(spread > last, "the stain spreads");
        last = spread;
        // Reduced: the same stain on every frame.
        assert_eq!(fresh(&game, true).0, reduced_at_death);
        assert!(fresh(&game, true).1 == reduced_paint_at_death);
    }
    assert_eq!(last, 1.0);
    game.attempt_mut().unwrap().tick += 600;
    let (settled, settled_paint) = fresh(&game, false);
    assert!(settled.iter().all(|decal| decal.spread == 1.0));
    // The settled stain is the reduced one.
    assert_eq!(settled, reduced_at_death);
    assert!(settled_paint == reduced_paint_at_death);
    assert!(normal_paint_at_death != settled_paint);
}

/// A boss's body leaves a larger stain.
#[test]
fn boss_body_leaves_a_larger_stain() {
    let mut game = game(1, 0);
    let a = game.attempt_mut().unwrap();
    a.corpses[0].boss_kind = None;
    let plain = decals(a, true, true)[0];
    a.corpses[0].boss_kind = Some(horde_core::content::content().levels[a.level].boss);
    let boss = decals(a, true, true)[0];
    assert_eq!(plain.blobs().len(), boss.blobs().len());
    for (plain, boss) in plain.blobs().iter().zip(boss.blobs()) {
        assert!((boss.radii[0] / plain.radii[0] - 1.6).abs() < 1e-5);
    }
}
