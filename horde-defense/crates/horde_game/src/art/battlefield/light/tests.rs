//! W5-04: additive light pools.
use super::super::super::lighting::{self, Lighting};
use super::super::super::{SceneLook, SkyClock};
use super::super::{RenderOptions, battlefield, weapon_fx::weapon_fx};
use super::*;
use crate::{
    animation::SceneAnimation,
    art::{ArenaProjection, FxSource},
    battle_view::SceneBlend,
};
use bevy_egui::egui::{self, Rect, pos2};
use horde_core::{
    content::{Slot, StructureKind},
    ordnance::FireZone,
    sim::{Attempt, Event, Game},
    weapons::WeaponClass,
};

fn arena() -> Rect {
    Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0))
}

/// The peak fixture with no events and no burning ground, its shooter
/// carrying `class` in the gear snapshot.
fn game_with(class: WeaponClass) -> Game {
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt_mut().unwrap();
    a.events.clear();
    a.ordnance.fire_zones.clear();
    a.shots = 0;
    a.gear_snapshot
        .iter_mut()
        .find(|record| record.slot == Slot::Ranged)
        .expect("the fixture deploys with a ranged weapon")
        .weapon_class = Some(class);
    game
}

fn event(a: &mut Attempt, kind: EventKind, ticks_ago: u64, n: usize, actor_id: Option<u64>) {
    let seq = a.events.len() as u64 + 1;
    a.events.push_back(Event {
        kind,
        from: a.defender_position(),
        to: [10.0 + n as f32, 1.5 + (n % 3) as f32],
        value: 250,
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
        event(a, EventKind::Shot, ticks_ago, n, None);
    }
}

fn turret_shot(a: &mut Attempt, ticks_ago: u64) {
    let turret = a
        .structures
        .iter()
        .find(|s| s.kind == StructureKind::Turret)
        .expect("the peak fixture has a turret")
        .id;
    event(a, EventKind::Shot, ticks_ago, 0, Some(turret));
}

/// Burning ground `id`, its radius `1 + id / 100` yards so a pool names its
/// zone.
fn fire_zone(a: &mut Attempt, id: u64) {
    a.ordnance.fire_zones.push(FireZone {
        id,
        cell: [(id % 3) as u8, (id % 8) as u8],
        radius: zone_radius(id),
        damage: 1,
        ticks_left: 600,
        tick_left: 10,
        tick_interval: 10,
    });
}

fn zone_radius(id: u64) -> f32 {
    1.0 + id as f32 / 100.0
}

const MUZZLE: Pos2 = pos2(300.0, 400.0);

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

fn pools_of(game: &Game, reduced: bool) -> Vec<LightPool> {
    collect(
        projection(game),
        &view_of(game, reduced),
        Some(MUZZLE),
        Default::default(),
        reduced,
    )
}

/// More lights than the cap: 10 explosions, 20 burning zones, a survivor
/// volley and a turret shot.
fn crowded() -> Game {
    let mut game = game_with(WeaponClass::Rifle);
    let a = game.attempt_mut().unwrap();
    for n in 0..10 {
        event(a, EventKind::Explosion, 9 - n as u64, n, None);
    }
    for id in 1..=20 {
        fire_zone(a, id);
    }
    volley(a, 0, 1);
    turret_shot(a, 0);
    game
}

fn is_additive(color: C) -> bool {
    color.a() == 0 && color != C::TRANSPARENT
}

/// G7 (FX-4): a frame draws at most 24 light pools. A crowded frame keeps
/// explosions first, then burning ground, then the survivor's muzzle, then
/// turrets, the newest first inside each class; the pools are one mesh of
/// 16-segment fans.
#[test]
fn light_pools_capped_at_24_by_priority_newest_first() {
    let game = crowded();
    let lane = projection(&game).lane_height();
    let pools = pools_of(&game, false);
    assert_eq!(pools.len(), LIGHTS_MAX, "32 lights, 24 drawn");
    assert_eq!(LIGHTS_MAX, 24);
    let (explosions, rest) = pools.split_at(10);
    assert!(
        explosions.iter().all(|p| p.rgb == EXPLOSION_LIGHT_RGB),
        "every explosion is kept, first: {pools:?}"
    );
    assert!(
        explosions
            .windows(2)
            .all(|pair| pair[0].intensity > pair[1].intensity),
        "explosions newest (brightest) first: {explosions:?}"
    );
    assert!(rest.iter().all(|p| p.rgb == FIRE_LIGHT_RGB), "{rest:?}");
    let zones: Vec<f32> = rest.iter().map(|p| p.radius.y / lane).collect();
    let newest: Vec<f32> = (7..=20).rev().map(zone_radius).collect();
    for (drawn, expected) in zones.iter().zip(&newest) {
        assert!(
            (drawn - expected).abs() < 1e-4,
            "the 14 newest zones, newest first: {zones:?}"
        );
    }
    assert_eq!(zones.len(), newest.len());

    let mesh = pools_mesh(&pools, 1.0);
    assert_eq!(mesh.vertices.len(), 24 * 17, "one 16-segment fan per pool");
    assert_eq!(mesh.indices.len(), 24 * 16 * 3);
    assert_eq!(mesh.texture_id, egui::TextureId::default(), "untextured");

    // Under the cap every class shows, in priority order.
    let mut game = game_with(WeaponClass::Rifle);
    let a = game.attempt_mut().unwrap();
    turret_shot(a, 1);
    volley(a, 0, 1);
    fire_zone(a, 1);
    fire_zone(a, 2);
    for n in 0..3 {
        event(a, EventKind::Explosion, n as u64, n, None);
    }
    let rgbs: Vec<[u8; 3]> = pools_of(&game, false).iter().map(|p| p.rgb).collect();
    let rifle = weapon_fx(FxSource::Weapon(WeaponClass::Rifle)).flash_rgb;
    let turret = weapon_fx(FxSource::Turret).flash_rgb;
    assert_eq!(
        rgbs,
        [
            EXPLOSION_LIGHT_RGB,
            EXPLOSION_LIGHT_RGB,
            EXPLOSION_LIGHT_RGB,
            FIRE_LIGHT_RGB,
            FIRE_LIGHT_RGB,
            rifle,
            turret
        ]
    );
}

/// The canvas-wide shapes of one battlefield pass and every additive mesh.
fn frame(game: &Game, lighting: Lighting, reduced: bool) -> Vec<egui::epaint::ClippedShape> {
    let a = game.attempt().unwrap();
    let view = view_of(game, reduced);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let painter = ui.painter().with_clip_rect(arena());
        let options = RenderOptions {
            construction: false,
            outlines: false,
            numbers: true,
            reduced,
            blood: false,
            scene: SceneLook {
                lighting,
                ..SceneLook::neutral(0)
            },
            sky: SkyClock::default(),
            canvas: arena().expand(24.0),
            textures: Default::default(),
            occluder: None,
        };
        battlefield(&painter, arena(), &view, a, [10.0, 5.0], options);
    });
    output.textures_delta.clear();
    output.shapes
}

fn light_meshes(shapes: &[egui::epaint::ClippedShape]) -> Vec<usize> {
    shapes
        .iter()
        .enumerate()
        .filter(|(_, s)| match &s.shape {
            Shape::Mesh(mesh) => mesh.vertices.iter().any(|v| is_additive(v.color)),
            _ => false,
        })
        .map(|(n, _)| n)
        .collect()
}

/// Reduced effects draw no light pool: none is collected and no additive
/// vertex is painted, on the same crowded frame that lights 24 pools with
/// normal effects.
#[test]
fn reduced_has_no_light_pools() {
    let game = crowded();
    assert_eq!(pools_of(&game, false).len(), LIGHTS_MAX);
    assert_eq!(pools_of(&game, true), [], "reduced collects no pool");
    assert_eq!(
        light_meshes(&frame(&game, Lighting::Night, false)).len(),
        1,
        "normal effects paint the pools as one mesh"
    );
    let reduced = frame(&game, Lighting::Night, true);
    assert!(
        light_meshes(&reduced).is_empty(),
        "reduced paints no light mesh"
    );
    assert!(
        reduced.iter().all(|s| match &s.shape {
            Shape::Mesh(mesh) => !mesh.vertices.iter().any(|v| is_additive(v.color)),
            _ => true,
        }),
        "reduced paints no additive vertex"
    );
}

/// The light pass sits between the grade and the vignette: over the grade
/// rect (so the grade does not dim it), under the vignette and every cue,
/// clipped to the field.
#[test]
fn light_pools_paint_after_the_grade_and_under_the_vignette() {
    let game = crowded();
    let canvas = arena().expand(24.0);
    let shapes = frame(&game, Lighting::Night, false);
    let night = lighting::grade(Lighting::Night);
    let fill = lighting::over(night.tint, night.alpha);
    let grade = shapes
        .iter()
        .position(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == fill))
        .expect("the night grade rect");
    assert_eq!(
        light_meshes(&shapes),
        [grade + 1],
        "lights follow the grade"
    );
    assert_eq!(shapes[grade + 1].clip_rect, arena(), "clipped to the field");
    let vignette = &shapes[grade + 2];
    assert_eq!(
        vignette.clip_rect, canvas,
        "the vignette follows the lights"
    );
    assert!(matches!(&vignette.shape, Shape::Mesh(mesh) if mesh.vertices.len() == 24));
    let warning = {
        let mut game = crowded();
        let a = game.attempt_mut().unwrap();
        event(a, EventKind::Reinforcements, 0, 0, None);
        frame(&game, Lighting::Night, false)
    };
    let lights = light_meshes(&warning)[0];
    let banner = warning
        .iter()
        .position(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "REINFORCEMENTS"))
        .expect("the reinforcements cue");
    assert!(banner > lights, "cues paint over the lights");
}

/// A volley lights one pool at the drawn muzzle from its class profile
/// (radius in lanes, the flash colour, the profile's intensity), however
/// many pellets it fires; the pool fades for 4 ticks after the flash and is
/// gone after that. A turret's shot lights the turret profile's pool.
#[test]
fn muzzle_light_reads_the_class_profile_once_per_volley() {
    for source in FxSource::ALL {
        let FxSource::Weapon(class) = source else {
            continue;
        };
        let profile = weapon_fx(source);
        let at_age = |ticks_ago: u64| {
            let mut game = game_with(class);
            volley(game.attempt_mut().unwrap(), ticks_ago, 9);
            let lane = projection(&game).lane_height();
            (pools_of(&game, false), lane)
        };
        let (pools, lane) = at_age(0);
        assert_eq!(pools.len(), 1, "{class:?}: nine pellets, one pool");
        assert_eq!(pools[0].at, MUZZLE, "{class:?}: at the drawn muzzle");
        assert_eq!(pools[0].rgb, profile.flash_rgb);
        assert_eq!(pools[0].intensity, profile.light_intensity);
        assert!((pools[0].radius.x - lane * profile.light_radius).abs() < 1e-3);
        assert_eq!(pools[0].radius.x, pools[0].radius.y);
        let last = u64::from(profile.flash_life_ticks) + u64::from(MUZZLE_LIGHT_TAIL_TICKS);
        let (fading, _) = at_age(last);
        assert_eq!(fading.len(), 1, "{class:?}: still lit {last} ticks on");
        assert!(
            fading[0].intensity > 0.0 && fading[0].intensity < profile.light_intensity,
            "{class:?}: fading in its tail"
        );
        assert_eq!(at_age(last + 1).0, [], "{class:?}: out after the tail");
    }
    let mut game = game_with(WeaponClass::Rifle);
    turret_shot(game.attempt_mut().unwrap(), 0);
    let lane = projection(&game).lane_height();
    let turret = weapon_fx(FxSource::Turret);
    let pools = pools_of(&game, false);
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].rgb, turret.flash_rgb);
    assert_eq!(pools[0].intensity, turret.light_intensity);
    assert!((pools[0].radius.y - lane * turret.light_radius).abs() < 1e-3);
    assert_ne!(pools[0].at, MUZZLE, "a turret's pool is at the turret");
}

/// Explosion and fire pools follow the blast and the zone: 1.2 blast radii
/// fading from 0.8 over 0.35 s, and a zone-wide pool flickering between
/// 0.30 and 0.40 every 6 sim ticks (so it holds still while paused).
#[test]
fn explosion_and_fire_pools_follow_the_blast_and_the_zone() {
    let mut game = game_with(WeaponClass::Rifle);
    event(
        game.attempt_mut().unwrap(),
        EventKind::Explosion,
        0,
        0,
        None,
    );
    let arena = projection(&game);
    let pools = pools_of(&game, false);
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].intensity, 0.8);
    // value 250 is a 2.5-yard blast.
    assert!((pools[0].radius.x - arena.yard_width() * 3.0).abs() < 1e-3);
    assert!((pools[0].radius.y - arena.lane_height() * 3.0).abs() < 1e-3);
    let aged = |ticks_ago: u64| {
        let mut game = game_with(WeaponClass::Rifle);
        event(
            game.attempt_mut().unwrap(),
            EventKind::Explosion,
            ticks_ago,
            0,
            None,
        );
        pools_of(&game, false)
    };
    assert_eq!(aged(20).len(), 1, "lit for 0.35 s");
    assert!(aged(20)[0].intensity < 0.1);
    assert_eq!(aged(21), [], "dark after 0.35 s (21 ticks)");

    let mut game = game_with(WeaponClass::Rifle);
    fire_zone(game.attempt_mut().unwrap(), 5);
    let mut seen = Vec::new();
    for _ in 0..2 {
        let pools = pools_of(&game, false);
        assert_eq!(pools, pools_of(&game, false), "still on an unmoved tick");
        assert_eq!(pools.len(), 1);
        assert!((pools[0].radius.x - arena.yard_width() * zone_radius(5)).abs() < 1e-3);
        seen.push((pools[0].intensity * 100.0).round() as i32);
        game.attempt_mut().unwrap().tick += 6;
    }
    seen.sort_unstable();
    assert_eq!(seen, [30, 40], "0.35 +- 0.05, alternating every 6 ticks");
    // A dying zone's light fades with its flames.
    game.attempt_mut().unwrap().ordnance.fire_zones[0].ticks_left = 9;
    assert!(pools_of(&game, false)[0].intensity < 0.1);
}

/// The fan's centre is additive `rgb * intensity * light_gain` and its rim
/// transparent, so night (gain 1.0) shows a pool brighter than a neutral
/// stage (0.55) and the darkest grade the strongest.
#[test]
fn light_centre_scales_with_the_grade_light_gain() {
    let pool = LightPool {
        at: pos2(100.0, 100.0),
        radius: vec2(40.0, 20.0),
        rgb: [200, 120, 40],
        intensity: 0.5,
    };
    let centre = |lighting: Lighting| {
        let mesh = pools_mesh(&[pool], lighting::grade(lighting).light_gain);
        assert_eq!(mesh.vertices[0].pos, pool.at);
        assert!(
            mesh.vertices[1..].iter().all(|v| v.color == C::TRANSPARENT),
            "the rim is transparent"
        );
        let on_rim = |v: &egui::epaint::Vertex| {
            let d = v.pos - pool.at;
            ((d.x / 40.0).powi(2) + (d.y / 20.0).powi(2) - 1.0).abs() < 1e-3
        };
        assert!(mesh.vertices[1..].iter().all(on_rim));
        mesh.vertices[0].color
    };
    let night = centre(Lighting::Night);
    assert_eq!(night, C::from_rgb_additive(100, 60, 20));
    assert!(is_additive(night));
    let neutral = centre(Lighting::Neutral);
    assert_eq!(neutral, C::from_rgb_additive(55, 33, 11));
    let mut by_gain = Lighting::ALL.map(|l| (lighting::grade(l).light_gain, centre(l).r()));
    by_gain.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(by_gain.windows(2).all(|pair| pair[0].1 <= pair[1].1));
}
