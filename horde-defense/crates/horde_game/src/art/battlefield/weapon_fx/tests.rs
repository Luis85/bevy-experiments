//! W5-03: weapon effect profiles, volleys and the gear-snapshot class.
use super::super::fx;
use super::*;
use crate::{
    animation::{ParticleKind, SceneAnimation},
    art::ArenaProjection,
    battle_view::{BattleViewModel, SceneBlend},
};
use bevy_egui::egui::{Rect, Shape, pos2};
use horde_core::{
    content::StructureKind,
    sim::{Event, EventKind, Game},
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

fn flashes(shapes: &[Shape]) -> usize {
    shapes
        .iter()
        .filter(|shape| matches!(shape, Shape::Path(_)))
        .count()
}

/// Every painted line: its ends, width and colour.
fn lines(shapes: &[Shape]) -> Vec<([Pos2; 2], f32, C)> {
    shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke } => Some((*points, stroke.width, stroke.color)),
            _ => None,
        })
        .collect()
}

fn rgb(color: C) -> [u8; 3] {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    [r, g, b]
}

/// The peak fixture with an empty event log, its shooter carrying `class` in
/// the gear snapshot.
fn game_with(class: WeaponClass) -> Game {
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt_mut().unwrap();
    a.events.clear();
    a.shots = 0;
    set_class(a, class);
    game
}

fn set_class(a: &mut Attempt, class: WeaponClass) {
    let ranged = a
        .gear_snapshot
        .iter_mut()
        .find(|record| record.slot == Slot::Ranged)
        .expect("the fixture deploys with a ranged weapon");
    ranged.weapon_class = Some(class);
}

/// Logs one Shot event `ticks_ago` ticks back, `n` setting its end point.
fn shot(a: &mut Attempt, ticks_ago: u64, n: usize, actor_id: Option<u64>) {
    let seq = a.events.len() as u64 + 1;
    a.events.push_back(Event {
        kind: EventKind::Shot,
        from: a.defender_position(),
        to: [30.0, 1.5 + n as f32 * 0.5],
        value: 0,
        tick: a.tick - ticks_ago,
        actor_id,
        target_id: None,
        lane: None,
        duration_ticks: 0,
        seq,
    });
}

/// One trigger pull of `pellets` pellets, `ticks_ago` ticks back.
fn volley(a: &mut Attempt, ticks_ago: u64, pellets: usize) {
    a.shots += 1;
    for n in 0..pellets {
        shot(a, ticks_ago, n, None);
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

/// The world-pass event shapes of `game`, painted through `fx::events`.
fn paint_events(game: &Game, reduced: bool) -> Vec<Shape> {
    let a = game.attempt().unwrap();
    let view = view_of(game, reduced);
    let projection = ArenaProjection::for_attempt(arena(), a);
    let options = fx::FxOptions {
        numbers: false,
        reduced,
        textures: Default::default(),
        occluder: None,
    };
    painted(|p| fx::events(p, projection, &view, None, &options))
}

/// G7 (FX-1): every pair of the seven profiles differs on at least two of
/// the axes a player reads: flash shape, flash colour, flash life, tracer
/// stroke, tracer colour and tracer life; each names its own silhouette skin.
#[test]
fn every_class_and_turret_profile_distinct_on_two_axes() {
    let axes = |fx: &WeaponFx| {
        [
            format!("{} {} {}", fx.flash_len, fx.flash_width, fx.flash_points),
            format!("{:?}", fx.flash_rgb),
            fx.flash_life_ticks.to_string(),
            format!(
                "{} {:?} {} {} {:?}",
                fx.tracer_width,
                fx.tracer_core_rgb,
                fx.tracer_every,
                fx.dim_tracer_width,
                fx.pellet_streak
            ),
            format!("{:?}", fx.tracer_rgb),
            fx.tracer_life_ticks.to_string(),
        ]
    };
    for (n, left) in FxSource::ALL.into_iter().enumerate() {
        for right in FxSource::ALL.into_iter().skip(n + 1) {
            let (a, b) = (weapon_fx(left), weapon_fx(right));
            let differing = axes(a)
                .iter()
                .zip(axes(b).iter())
                .filter(|(x, y)| x != y)
                .count();
            assert!(
                differing >= 2,
                "{left:?} and {right:?} differ on {differing} axes"
            );
            assert_ne!(a.weapon_skin, b.weapon_skin, "{left:?} and {right:?}");
        }
    }
    // The design table (section 5), spot-checked.
    let shotgun = weapon_fx(FxSource::Weapon(WeaponClass::Shotgun));
    assert_eq!((shotgun.flash_len, shotgun.flash_width), (0.34, 0.60));
    assert_eq!(shotgun.pellet_streak, Some(0.35));
    let mg = weapon_fx(FxSource::Weapon(WeaponClass::MachineGun));
    assert_eq!((mg.tracer_every, mg.tracer_rgb), (3, [255, 120, 60]));
    let marksman = weapon_fx(FxSource::Weapon(WeaponClass::Marksman));
    assert_eq!(
        (marksman.tracer_life_ticks, marksman.tracer_core_rgb),
        (12, Some([255, 252, 240]))
    );
    assert_eq!(weapon_fx(FxSource::Turret), &TURRET_FX);
    assert_eq!(TURRET_FX.flash_rgb, [200, 230, 240]);
    // No profile outlives the Shot row of the shared event filter.
    assert!(longest_shot_seconds() <= fx::fx_lifetime(EventKind::Shot));
    assert_eq!((width_scale(40.0), width_scale(62.9)), (1.0, 1.0));
    assert_eq!(width_scale(200.0), 1.6);
}

/// A nine-pellet shell is one volley: one flash, nine streaks 35% of each
/// pellet's path in the shotgun colour, and one casing pair (18 before).
#[test]
fn shotgun_volley_paints_one_flash_and_nine_streaks() {
    let mut game = game_with(WeaponClass::Shotgun);
    volley(game.attempt_mut().unwrap(), 1, 9);
    let view = view_of(&game, false);
    let shots: Vec<_> = view
        .fx
        .iter()
        .filter(|e| e.kind == EventKind::Shot)
        .collect();
    assert_eq!(shots.len(), 9);
    assert!(
        shots
            .iter()
            .all(|e| e.source == Some(FxSource::Weapon(WeaponClass::Shotgun)))
    );
    assert_eq!(
        shots
            .iter()
            .filter(|e| e.volley.is_some_and(|v| v.first))
            .count(),
        1
    );
    assert!(shots[0].volley.unwrap().first, "the lowest seq is first");
    assert!(shots.iter().all(|e| e.volley.unwrap().ordinal == 1));

    let shapes = paint_events(&game, false);
    assert_eq!(flashes(&shapes), 1, "one flash per volley");
    let streaks = lines(&shapes);
    assert_eq!(streaks.len(), 9, "one streak per pellet");
    let a = game.attempt().unwrap();
    let projection = ArenaProjection::for_attempt(arena(), a);
    for (n, ([from, to], _, color)) in streaks.iter().enumerate() {
        let end = projection.world(shots[n].to[0], shots[n].to[1]);
        let fraction = (*to - *from).length() / (end - *from).length();
        assert!((fraction - 0.35).abs() < 1e-3, "streak {n} is {fraction}");
        assert_eq!(rgb(*color), SHOTGUN_FX.tracer_rgb);
    }
    let casings = SceneAnimation::sample(a, false)
        .particles
        .into_iter()
        .filter(|p| p.kind == ParticleKind::Casing)
        .count();
    assert_eq!(casings, 2, "one casing pair per trigger pull");
}

/// The machine gun's bright tracer is drawn on every third volley by shot
/// count (`Attempt::shots`), the others dim; the ordinals hold as the event
/// log rolls.
#[test]
fn machine_gun_bright_tracer_every_third_volley() {
    let mut game = game_with(WeaponClass::MachineGun);
    let a = game.attempt_mut().unwrap();
    // Shots 1 to 6 on consecutive ticks; shot 6 is the newest.
    for ticks_ago in (1..=6).rev() {
        volley(a, ticks_ago, 1);
    }
    let ordinals = |game: &Game| -> Vec<u32> {
        view_of(game, false)
            .fx
            .iter()
            .filter(|e| e.kind == EventKind::Shot)
            .map(|e| e.volley.unwrap().ordinal)
            .collect()
    };
    assert_eq!(ordinals(&game), [1, 2, 3, 4, 5, 6]);
    let shapes = paint_events(&game, false);
    let tracers = lines(&shapes);
    assert_eq!(tracers.len(), 6);
    let bright: Vec<usize> = tracers
        .iter()
        .enumerate()
        .filter(|(_, (_, width, _))| *width >= MACHINE_GUN_FX.tracer_width)
        .map(|(n, _)| n + 1)
        .collect();
    assert_eq!(bright, [3, 6], "shots 3 and 6 are bright");
    for (n, (_, width, color)) in tracers.iter().enumerate() {
        if (n + 1) % 3 != 0 {
            assert!(
                *width < MACHINE_GUN_FX.tracer_width,
                "shot {} is dim",
                n + 1
            );
            assert!(color.a() <= 128, "dim tracers are at most half alpha");
        }
    }
    // Colours of the two newest (the older ones have faded too far to read
    // back exactly): shot 6 bright, shot 5 dim.
    let near = |color: C, want: [u8; 3]| {
        rgb(color)
            .iter()
            .zip(want)
            .all(|(got, want)| got.abs_diff(want) <= 3)
    };
    assert!(near(tracers[5].2, MACHINE_GUN_FX.tracer_rgb));
    assert!(near(tracers[4].2, MACHINE_GUN_FX.dim_tracer_rgb));
    // The log drops its two oldest events: the rest keep their ordinals.
    let a = game.attempt_mut().unwrap();
    a.events.pop_front();
    a.events.pop_front();
    assert_eq!(ordinals(&game), [3, 4, 5, 6]);
    // A turret volley in between is not counted.
    let a = game.attempt_mut().unwrap();
    let turret = a
        .structures
        .iter()
        .find(|s| s.kind == StructureKind::Turret)
        .expect("the fixture has a turret")
        .id;
    shot(a, 0, 0, Some(turret));
    volley(a, 0, 1);
    let view = view_of(&game, false);
    let newest: Vec<_> = view.fx.iter().rev().take(2).collect();
    assert_eq!(newest[0].volley.unwrap().ordinal, 7);
    assert_eq!(newest[1].source, Some(FxSource::Turret));
    assert_eq!(newest[1].volley.unwrap().ordinal, 0);
}

/// Reduced effects: no flash for any source, and one static 1-pt line per
/// volley in the tracer colour (the same on every tick of the tracer's
/// life, then nothing). A trap's spring has no source and paints no shot.
#[test]
fn reduced_shot_paints_no_flash_and_one_static_line() {
    for class in WeaponClass::ALL {
        let fx = weapon_fx(FxSource::Weapon(class));
        let pellets = if fx.pellet_streak.is_some() { 9 } else { 1 };
        let mut seen = None;
        for ticks_ago in 0..u64::from(fx.tracer_life_ticks) {
            let mut game = game_with(class);
            volley(game.attempt_mut().unwrap(), ticks_ago, pellets);
            let shapes = paint_events(&game, true);
            assert_eq!(flashes(&shapes), 0, "{class:?} flashes in reduced effects");
            let drawn = lines(&shapes);
            assert_eq!(drawn.len(), 1, "{class:?} at {ticks_ago} ticks");
            let (_, width, color) = drawn[0];
            assert_eq!(width, 1.0);
            assert_eq!(
                color,
                C::from_rgb(fx.tracer_rgb[0], fx.tracer_rgb[1], fx.tracer_rgb[2])
            );
            assert_eq!(
                *seen.get_or_insert(drawn[0]),
                drawn[0],
                "the line is static"
            );
        }
        let mut game = game_with(class);
        volley(
            game.attempt_mut().unwrap(),
            u64::from(fx.tracer_life_ticks),
            pellets,
        );
        assert!(
            paint_events(&game, true).is_empty(),
            "{class:?} outlives its tracer"
        );
        // Normal effects do flash, so the reduced assertion is not vacuous.
        let mut game = game_with(class);
        volley(game.attempt_mut().unwrap(), 0, pellets);
        assert_eq!(flashes(&paint_events(&game, false)), 1, "{class:?}");
    }
    // The turret and the trap.
    let mut game = game_with(WeaponClass::Rifle);
    let a = game.attempt_mut().unwrap();
    let id = |a: &Attempt, kind| {
        a.structures
            .iter()
            .find(|s| s.kind == kind)
            .expect("the fixture has it")
            .id
    };
    let (turret, trap) = (id(a, StructureKind::Turret), id(a, StructureKind::Trap));
    shot(a, 0, 0, Some(turret));
    shot(a, 0, 1, Some(trap));
    let view = view_of(&game, true);
    assert_eq!(view.fx[0].source, Some(FxSource::Turret));
    assert_eq!(view.fx[1].source, None);
    let shapes = paint_events(&game, true);
    assert_eq!(flashes(&shapes), 0);
    let drawn = lines(&shapes);
    assert_eq!(drawn.len(), 1, "the turret's line; the trap paints no shot");
    assert_eq!(rgb(drawn[0].2), TURRET_FX.tracer_rgb);
    let shapes = paint_events(&game, false);
    assert_eq!((flashes(&shapes), lines(&shapes).len()), (1, 1));
}

/// The class is the deployed one: the gear snapshot's Ranged record wins
/// over the live profile, which is the fallback when the record has none.
#[test]
fn shooter_class_reads_the_gear_snapshot() {
    let mut game = game_with(WeaponClass::Marksman);
    let a = game.attempt_mut().unwrap();
    a.profile.weapon.class = WeaponClass::Pistol;
    assert_eq!(shooter_class(a), WeaponClass::Marksman);
    for record in &mut a.gear_snapshot {
        record.weapon_class = None;
    }
    assert_eq!(shooter_class(a), WeaponClass::Pistol);
    a.gear_snapshot.clear();
    assert_eq!(shooter_class(a), WeaponClass::Pistol);
}

/// Design-vfx-lighting §13 `tracers 96`: one frame draws the Shot events of
/// its newest volleys up to the cap, whole volleys only, so a crowded frame
/// keeps its most recent tracers and never splits a shell from its flash.
#[test]
fn shots_past_the_tracer_cap_keep_the_newest_whole_volleys() {
    const CAP: usize = TRACERS_MAX;
    assert_eq!(CAP, 96);
    // 150 one-shot turret volleys on one tick (one per structure id).
    let mut game = game_with(WeaponClass::Pistol);
    let a = game.attempt_mut().unwrap();
    for n in 0..150 {
        shot(a, 1, n % 40, Some(9_000_000 + n as u64));
    }
    let view = view_of(&game, false);
    let ends: Vec<_> = view
        .fx
        .iter()
        .filter(|e| e.kind == EventKind::Shot)
        .map(|e| e.to)
        .collect();
    assert_eq!(ends.len(), 150);
    let shapes = paint_events(&game, false);
    let drawn = lines(&shapes);
    assert_eq!(drawn.len(), CAP, "one tracer per drawn shot");
    assert_eq!(flashes(&shapes), CAP, "each drawn volley flashes");
    let projection = ArenaProjection::for_attempt(arena(), game.attempt().unwrap());
    for (([_, to], _, _), end) in drawn.iter().zip(&ends[150 - CAP..]) {
        assert_eq!(*to, projection.world(end[0], end[1]), "the newest shots");
    }
    assert_eq!(lines(&paint_events(&game, true)).len(), CAP, "reduced");

    // A nine-pellet shell that does not fit whole is left out whole: 90
    // newer turret shots leave room for 6 pellets, not 9.
    let mut game = game_with(WeaponClass::Shotgun);
    let a = game.attempt_mut().unwrap();
    volley(a, 2, 9);
    for n in 0..90 {
        shot(a, 1, n % 40, Some(9_000_000 + n as u64));
    }
    let shapes = paint_events(&game, false);
    let drawn = lines(&shapes);
    assert_eq!(drawn.len(), 90);
    assert!(
        drawn
            .iter()
            .all(|(_, _, color)| rgb(*color) != SHOTGUN_FX.tracer_rgb),
        "no pellet of the shell is drawn"
    );
    // With room for it, the shell draws whole, its flash included.
    let mut game = game_with(WeaponClass::Shotgun);
    let a = game.attempt_mut().unwrap();
    volley(a, 2, 9);
    for n in 0..87 {
        shot(a, 1, n % 40, Some(9_000_000 + n as u64));
    }
    let shapes = paint_events(&game, false);
    assert_eq!(lines(&shapes).len(), CAP);
    assert_eq!(flashes(&shapes), 88, "87 turret flashes and the shell's");
}
