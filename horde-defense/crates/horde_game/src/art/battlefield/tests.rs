//! Batched battlefield tests: primitive counts, layer order and the pure
//! geometry the batched and legacy paths share.
use super::*;
use crate::diagnostics::SceneDraw;
use crate::{
    animation::SceneAnimation,
    battle_view::{BattleViewModel, SceneBlend},
    visual_assets::{ArtTextures, BattleAtlas, WalkSheet},
};
use bevy_egui::egui::{ClippedPrimitive, TextureId, pos2, vec2};
use horde_core::sim::Game;
use std::sync::Arc;

/// Fake egui ids for every sheet the battlefield samples.
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

fn arena() -> Rect {
    Rect::from_min_size(pos2(24.0, 124.0), vec2(1392.0, 692.0))
}

fn options(textures: ArtTextures) -> RenderOptions {
    RenderOptions {
        construction: false,
        outlines: false,
        numbers: true,
        reduced: false,
        blood: true,
        theme: 0,
        textures,
    }
}

/// Paints the battlefield of `game` and tessellates it: the primitives whose
/// clip is the arena (the battlefield painter's own).
fn battlefield_primitives(game: &Game, textures: ArtTextures) -> Vec<ClippedPrimitive> {
    battlefield_primitives_in(game, textures, arena())
}

/// [`battlefield_primitives`] in any arena `rect`.
fn battlefield_primitives_in(
    game: &Game,
    textures: ArtTextures,
    rect: Rect,
) -> Vec<ClippedPrimitive> {
    let a = game.attempt().expect("fixture battle");
    let motion = SceneAnimation::sample(a, false);
    let view = BattleViewModel::build(a, motion, SceneBlend::NONE, false);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let painter = ui.painter().with_clip_rect(rect);
        battlefield(&painter, rect, &view, a, [10.0, 5.0], options(textures));
    });
    output.textures_delta.clear();
    ctx.tessellate(output.shapes, output.pixels_per_point)
        .into_iter()
        .filter(|primitive| primitive.clip_rect == rect)
        .collect()
}

/// Mesh vertices over `primitives` (callbacks have none).
fn vertices(primitives: &[ClippedPrimitive]) -> usize {
    primitives
        .iter()
        .map(|clipped| match &clipped.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
            egui::epaint::Primitive::Callback(_) => 0,
        })
        .sum()
}

/// The `PERF_BUDGET.json` vertex gate of `scene`.
fn vertex_gate(scene: &str) -> usize {
    crate::perf_budget::BATTLEFIELD_VERTICES_MAX
        .iter()
        .find(|(name, _)| *name == scene)
        .map(|(_, max)| *max)
        .unwrap_or_else(|| panic!("{scene} has a vertex gate"))
}

/// The battlefield stays within its measured vertex gates (W1-51): the
/// progressed-peak fixture, and the stress fixture as staged and with both
/// corpse draw caps full (5000 records, 64 of them collapsing).
#[test]
fn battlefield_vertices_stay_within_the_measured_gates() {
    let progressed = horde_core::sim::maximum_progression_render_fixture().expect("fixture");
    let painted = vertices(&battlefield_primitives(&progressed, textures()));
    let gate = vertex_gate("progressed-peak");
    assert!(
        painted <= gate,
        "progressed-peak paints {painted} vertices, gate {gate}"
    );
    let mut stress = horde_core::sim::stress_render_fixture().expect("stress fixture");
    let staged = vertices(&battlefield_primitives(&stress, textures()));
    stage_bodies(
        &mut stress,
        5_000,
        crate::perf_budget::CORPSES_COLLAPSING_DRAWN,
    );
    let full = vertices(&battlefield_primitives(&stress, textures()));
    let gate = vertex_gate("stress");
    assert!(full > staged, "the full corpse caps add vertices");
    for (case, painted) in [("staged", staged), ("full corpse caps", full)] {
        assert!(
            painted <= gate,
            "stress {case} paints {painted} vertices, gate {gate}"
        );
    }
}

/// The largest battlefield load of one live sim run: its maxima, the ticks
/// they were painted at and how many ticks the run actually simulated.
#[derive(Debug)]
struct LiveLoad {
    ticks_run: usize,
    primitives_max: usize,
    primitives_max_tick: usize,
    vertices_max: usize,
    vertices_max_tick: usize,
}

/// Runs `scene`'s perf fixture unpaused for `ticks` ticks with no input and
/// paints its battlefield in `rect` on every `stride`-th tick.
/// `stress-5000-corpses` is the stress fixture with both corpse draw caps
/// full ([`stage_bodies`], which moves the attempt to tick 1,000,000 first).
fn live_battlefield_load(scene: &str, ticks: usize, stride: usize, rect: Rect) -> LiveLoad {
    let mut game = match scene {
        "peak" => horde_core::sim::peak_render_fixture(),
        "progressed-peak" => horde_core::sim::maximum_progression_render_fixture(),
        "stress" | "stress-5000-corpses" => horde_core::sim::stress_render_fixture(),
        _ => panic!("{scene}: no perf fixture"),
    }
    .expect("perf fixture");
    if scene == "stress-5000-corpses" {
        stage_bodies(
            &mut game,
            5_000,
            crate::perf_budget::CORPSES_COLLAPSING_DRAWN,
        );
    }
    game.set_paused(false);
    game.attempt_mut().expect("fixture battle").paused = false;
    let mut load = LiveLoad {
        ticks_run: 0,
        primitives_max: 0,
        primitives_max_tick: 0,
        vertices_max: 0,
        vertices_max_tick: 0,
    };
    for tick in 0..=ticks {
        if tick % stride == 0 {
            let primitives = battlefield_primitives_in(&game, textures(), rect);
            if primitives.len() > load.primitives_max {
                (load.primitives_max, load.primitives_max_tick) = (primitives.len(), tick);
            }
            let painted = vertices(&primitives);
            if painted > load.vertices_max {
                (load.vertices_max, load.vertices_max_tick) = (painted, tick);
            }
        }
        if tick == ticks {
            break;
        }
        game.tick(horde_core::sim::Input::default());
        load.ticks_run += 1;
        if game.attempt().is_none() {
            break;
        }
    }
    load
}

/// The vertex gate that covers `scene` (`stress-5000-corpses` is a stress
/// case); peak has none.
fn vertex_gate_covering(scene: &str) -> Option<usize> {
    crate::perf_budget::BATTLEFIELD_VERTICES_MAX
        .iter()
        .find(|(name, _)| scene == *name || scene.starts_with(&format!("{name}-")))
        .map(|(_, max)| *max)
}

/// `load` ran all `ticks` and stays within the egui gates of `scene`.
fn assert_live_load_within_gates(scene: &str, size: &str, ticks: usize, load: &LiveLoad) {
    assert_eq!(
        load.ticks_run, ticks,
        "{scene} {size}: the run ended early ({load:?})"
    );
    let primitives_gate = crate::perf_budget::BATTLEFIELD_PRIMITIVES_MAX;
    assert!(
        load.primitives_max <= primitives_gate,
        "{scene} {size}: {} primitives at tick {}, gate {primitives_gate}",
        load.primitives_max,
        load.primitives_max_tick
    );
    if let Some(gate) = vertex_gate_covering(scene) {
        assert!(
            load.vertices_max <= gate,
            "{scene} {size}: {} vertices at tick {}, gate {gate}",
            load.vertices_max,
            load.vertices_max_tick
        );
    }
}

/// The live gate (W1-51 review): the first `LIVE_GATE_TICKS` ticks of each
/// gated perf fixture's sim run, painted at the normal arena, stay within
/// `BATTLEFIELD_PRIMITIVES_MAX` and the scene's vertex gate, so a live frame
/// (event FX, not only the static fixture) that crosses a gate fails here.
/// Every maximum the full-length `measure_battlefield_egui_load` records is
/// painted by tick 420 and on a tick divisible by 20 (stress primitives 47 at
/// tick 420), so these shorter runs, painting every 10th tick to stay fast,
/// reach the measured maxima.
#[test]
fn live_battlefield_runs_stay_within_the_egui_gates() {
    const LIVE_GATE_TICKS: usize = 420;
    const LIVE_GATE_STRIDE: usize = 10;
    for scene in ["progressed-peak", "stress", "stress-5000-corpses"] {
        let load = live_battlefield_load(scene, LIVE_GATE_TICKS, LIVE_GATE_STRIDE, arena());
        assert_live_load_within_gates(scene, "normal", LIVE_GATE_TICKS, &load);
    }
}

/// The measurement behind `PERF_BUDGET.json` `evidence.egui_measured`: the
/// battlefield's largest primitive and vertex counts over every 2nd tick of a
/// sim run from each perf fixture (warmup_frames + run.frames ticks), at the
/// normal and small arenas. Several minutes in a debug build; prints the
/// measurement and asserts every run is complete and within the gates.
#[test]
#[ignore = "W1-51 measurement, run with --ignored --nocapture"]
#[allow(clippy::print_stdout, reason = "prints the measurement it exists for")]
fn measure_battlefield_egui_load() {
    let small = Rect::from_min_size(pos2(24.0, 124.0), vec2(729.8, 310.5));
    for (scene, ticks) in [
        ("peak", 780),
        ("progressed-peak", 780),
        ("stress", 1980),
        ("stress-5000-corpses", 1980),
    ] {
        for (size, rect) in [("normal", arena()), ("small", small)] {
            let load = live_battlefield_load(scene, ticks, 2, rect);
            println!(
                "W1-51 {scene} {size}: ticks {} primitives {} (tick {}) vertices {} (tick {})",
                load.ticks_run,
                load.primitives_max,
                load.primitives_max_tick,
                load.vertices_max,
                load.vertices_max_tick
            );
            assert_live_load_within_gates(scene, size, ticks, &load);
        }
    }
}

/// Pin of the batched layers: the battlefield costs at most
/// `BATTLEFIELD_PRIMITIVES_MAX` primitives at peak (52 hostiles) and in the
/// stress scene (400), and 400 hostiles cost at most
/// `PRIMITIVE_GROWTH_52_TO_400_MAX` more than the first 52 of the same scene.
/// Before batching, sprites, flashes and shadows alternated textures per
/// hostile: 135 primitives at peak and 855 in stress.
#[test]
fn battlefield_primitives_are_independent_of_enemy_count() {
    use crate::perf_budget::{BATTLEFIELD_PRIMITIVES_MAX, PRIMITIVE_GROWTH_52_TO_400_MAX};
    let peak = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let stress = horde_core::sim::stress_render_fixture().expect("stress fixture");
    assert_eq!(peak.attempt().unwrap().enemies.len(), 52);
    assert_eq!(stress.attempt().unwrap().enemies.len(), 400);
    let mut thinned = horde_core::sim::stress_render_fixture().expect("stress fixture");
    thinned.attempt_mut().unwrap().enemies.truncate(52);
    let at_peak = battlefield_primitives(&peak, textures()).len();
    let at_stress = battlefield_primitives(&stress, textures()).len();
    let at_thinned = battlefield_primitives(&thinned, textures()).len();
    assert!(
        at_peak <= BATTLEFIELD_PRIMITIVES_MAX,
        "peak paints {at_peak} primitives"
    );
    assert!(
        at_stress <= BATTLEFIELD_PRIMITIVES_MAX,
        "stress paints {at_stress} primitives"
    );
    assert!(
        at_stress <= at_thinned + PRIMITIVE_GROWTH_52_TO_400_MAX,
        "400 hostiles paint {at_stress} primitives, 52 of the same scene {at_thinned}"
    );
}

/// The peak view with alternating walker and standing sprites (so the actor
/// layer crosses sheets enemy by enemy) and every third hostile flashing.
fn mixed_sheet_view() -> (Game, BattleViewModel) {
    let game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt().unwrap();
    let mut view =
        BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    let sprites = crate::visual_assets::enemy_visuals::active().sprites();
    let walk = sprites
        .iter()
        .find(|s| s.is_walk())
        .expect("a walker sprite");
    let standing = sprites
        .iter()
        .find(|s| !s.is_walk())
        .expect("a standing sprite");
    for (n, e) in view.enemies.iter_mut().enumerate() {
        e.sprite = Some(if n % 2 == 0 { walk } else { standing });
        e.hit_flash = (n % 3 == 0).then_some(1.0);
    }
    (game, view)
}

/// Every shape `paint` adds on a context holding the shipped flash masks.
fn painted(paint: impl FnMut(&Painter)) -> Vec<egui::Shape> {
    let mut paint = paint;
    let ctx = egui::Context::default();
    crate::visual_assets::publish_source_flash_masks(&ctx);
    let mut output = ctx.run_ui(Default::default(), |ui| {
        paint(&ui.painter().with_clip_rect(arena()));
    });
    output.textures_delta.clear();
    output.shapes.into_iter().map(|shape| shape.shape).collect()
}

fn meshes(shapes: &[egui::Shape]) -> Vec<&egui::Mesh> {
    shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Mesh(mesh) => Some(&**mesh),
            _ => None,
        })
        .collect()
}

fn rig_shadow() -> enemies::RigShadow {
    enemies::RigShadow {
        at: pos2(100.0, 400.0),
        size: vec2(20.0, 8.0),
        alpha: 0.8,
    }
}

/// The legacy per-enemy painters' sprite and flash meshes, in painter order.
fn legacy_actor_meshes(view: &BattleViewModel, textures: ArtTextures) -> Vec<egui::Mesh> {
    let arena = ArenaProjection::with_geometry(arena(), view.lanes, view.width);
    let sy = arena.lane_height();
    let shapes = painted(|p| {
        for e in &view.enemies {
            let pose = &e.pose;
            let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
            let painted_at = arena.world(
                e.position[0] + pose.offset[0],
                e.position[1] + if has_walk { 0. } else { pose.offset[1] },
            );
            let height = e.height.pixels(sy);
            let actor = visual_assets::Actor::Enemy(e.sprite.unwrap());
            let size = vec2(height * 1.2, height);
            assert!(visual_assets::actor_animated(
                p,
                textures,
                actor,
                painted_at,
                size,
                egui::Color32::WHITE,
                pose
            ));
            if let Some(strength) = e.hit_flash {
                visual_assets::actor_hit_flash(
                    p,
                    textures,
                    actor,
                    painted_at,
                    size,
                    visual_assets::HitFlash {
                        strength,
                        reduced: false,
                    },
                    pose,
                );
            }
        }
    });
    meshes(&shapes).into_iter().cloned().collect()
}

#[test]
fn batched_actor_layer_keeps_feet_then_id_order_across_sheets() {
    let (game, view) = mixed_sheet_view();
    let a = game.attempt().unwrap();
    let textures = textures();
    let atlas = textures.battle_atlas.unwrap();
    assert!(
        view.enemies
            .windows(2)
            .all(|pair| (pair[0].position[1], pair[0].id) <= (pair[1].position[1], pair[1].id)),
        "the view is in feet-then-id order"
    );
    let arena_projection = ArenaProjection::for_attempt(arena(), a);
    let shapes = painted(|p| {
        assert!(
            enemies::paint(
                p,
                arena_projection,
                &view,
                textures,
                true,
                false,
                rig_shadow()
            )
            .rig_shadow
        );
    });
    let batched = meshes(&shapes);
    let sprites: Vec<_> = batched
        .iter()
        .filter(|mesh| mesh.texture_id == atlas.texture)
        .collect();
    assert_eq!(sprites.len(), 1, "one sprite mesh for every hostile");
    let sprites = sprites[0];
    // The legacy painters, enemy by enemy: sprite, then its flash runs.
    let mut expected = Vec::new();
    let mut sheets = [0; 2];
    for mesh in legacy_actor_meshes(&view, textures) {
        let sheet = match mesh.texture_id {
            id if Some(id) == textures.walkers.map(|w| w.texture) => {
                sheets[0] += 1;
                Some(visual_assets::ActorSheet::Walkers)
            }
            id if Some(id) == textures.actors => {
                sheets[1] += 1;
                Some(visual_assets::ActorSheet::Actors)
            }
            _ => None,
        };
        for vertex in &mesh.vertices {
            let uv = match sheet {
                Some(sheet) => {
                    atlas
                        .uv(sheet, Rect::from_min_max(vertex.uv, vertex.uv))
                        .min
                }
                None => atlas.white_uv,
            };
            expected.push((vertex.pos, uv));
        }
    }
    assert!(sheets[0] > 10 && sheets[1] > 10, "both sheets: {sheets:?}");
    let actual: Vec<_> = sprites.vertices.iter().map(|v| (v.pos, v.uv)).collect();
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "sprites and flashes keep the legacy per-enemy order"
    );
    // Shadows sit under the sprites and the overlay above them.
    let shadow = batched
        .iter()
        .position(|mesh| Some(mesh.texture_id) == textures.shadow)
        .unwrap();
    let sprite = batched
        .iter()
        .position(|mesh| mesh.texture_id == atlas.texture)
        .unwrap();
    assert!(shadow < sprite && sprite + 1 < batched.len());
    assert_eq!(
        batched[shadow].vertices.len(),
        4 * (view.enemies.len() + 1),
        "one shadow quad per hostile plus the rig"
    );
    assert!(
        sprites
            .vertices
            .iter()
            .any(|v| v.color.a() == 0 && v.color.r() > 0),
        "normal flashes are additive"
    );

    // Reduced effects keep the steady tint: no additive vertex anywhere.
    let reduced = painted(|p| {
        enemies::paint(
            p,
            arena_projection,
            &view,
            textures,
            true,
            true,
            rig_shadow(),
        );
    });
    for mesh in meshes(&reduced) {
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.color.a() > 0 || v.color == egui::Color32::TRANSPARENT),
            "reduced mode shows no flash"
        );
    }
}

#[test]
fn batched_sprite_quads_match_legacy_actor_layout() {
    let textures = textures();
    let atlas = textures.battle_atlas.unwrap();
    let mut checked = 0;
    for sprite in crate::visual_assets::enemy_visuals::active().sprites() {
        for (frame, lean, scale) in [
            (0, 0.0, [1.0, 1.0]),
            (3, 0.2, [1.1, 0.7]),
            (7, -0.3, [0.9, 1.2]),
        ] {
            let pose = crate::animation::ActorPose {
                frame,
                lean,
                scale,
                ..Default::default()
            };
            let actor = visual_assets::Actor::Enemy(sprite);
            let (center, size) = (pos2(300.5, 211.25), vec2(61.0, 52.0));
            let legacy = painted(|p| {
                assert!(visual_assets::actor_animated(
                    p,
                    textures,
                    actor,
                    center,
                    size,
                    egui::Color32::WHITE,
                    &pose
                ));
            });
            let legacy = meshes(&legacy)[0].clone();
            let quad = visual_assets::actor_quad(
                textures,
                actor,
                center,
                size,
                egui::Color32::WHITE,
                &pose,
            )
            .unwrap();
            assert_eq!(quad.texture, legacy.texture_id);
            let shapes = painted(|p| batch::ActorMeshes::new(textures, 0).paint(p));
            assert!(shapes.is_empty(), "an empty layer adds no shape");
            let shapes = painted(move |p| {
                let mut layer = batch::ActorMeshes::new(textures, 1);
                layer.sprite(&quad);
                layer.paint(p);
            });
            let batched_mesh = meshes(&shapes)[0];
            assert_eq!(batched_mesh.texture_id, atlas.texture);
            assert_eq!(batched_mesh.indices, legacy.indices);
            for (batched, legacy) in batched_mesh.vertices.iter().zip(&legacy.vertices) {
                assert_eq!(batched.pos, legacy.pos, "{:?} frame {frame}", sprite.sprite);
                assert_eq!(
                    batched.uv,
                    atlas
                        .uv(quad.sheet, Rect::from_min_max(legacy.uv, legacy.uv))
                        .min
                );
                assert_eq!(batched.color, legacy.color);
            }
            checked += 1;
        }
    }
    assert!(checked >= 15);
}

#[test]
fn packed_atlas_over_device_limit_falls_back_to_two_meshes() {
    let (game, view) = mixed_sheet_view();
    let a = game.attempt().unwrap();
    let textures = ArtTextures {
        battle_atlas: None,
        ..textures()
    };
    let shapes = painted(|p| {
        enemies::paint(
            p,
            ArenaProjection::for_attempt(arena(), a),
            &view,
            textures,
            false,
            false,
            rig_shadow(),
        );
    });
    let textured: Vec<_> = meshes(&shapes)
        .into_iter()
        .filter(|mesh| mesh.texture_id != egui::TextureId::default())
        .map(|mesh| (mesh.texture_id, mesh.vertices.len()))
        .collect();
    let walkers = textures.walkers.unwrap().texture;
    let actors = textures.actors.unwrap();
    let half = view.enemies.len().div_ceil(2);
    assert_eq!(
        textured,
        vec![
            (textures.shadow.unwrap(), 4 * (view.enemies.len() + 1)),
            (walkers, 4 * half),
            (actors, 4 * (view.enemies.len() - half)),
        ],
        "shadows, then one walker mesh, then one actor mesh"
    );
    // The flash runs follow both sprite meshes in their own untextured mesh.
    let all = meshes(&shapes);
    let after_actors = all
        .iter()
        .position(|mesh| mesh.texture_id == actors)
        .unwrap()
        + 1;
    assert!(
        all[after_actors]
            .vertices
            .iter()
            .all(|v| v.uv == egui::epaint::WHITE_UV && v.color.a() == 0)
    );
}

#[test]
fn particles_paint_as_one_untextured_mesh() {
    use crate::animation::{Particle, ParticleKind};
    let kinds = [
        ParticleKind::Casing,
        ParticleKind::Spark,
        ParticleKind::Dust,
        ParticleKind::Rain,
        ParticleKind::Snow,
        ParticleKind::Mist,
        ParticleKind::Ash,
        ParticleKind::Debris,
    ];
    let particles: Vec<_> = (0..crate::animation::MAX_PARTICLES)
        .map(|n| Particle {
            kind: kinds[n % kinds.len()],
            at: [3.0 + n as f32 * 0.2, 1.0 + (n % 9) as f32],
            size: 0.08,
            // Dust alternates between ambient motes and impact dust.
            alpha: if n % 16 < 8 { 0.1 } else { 0.8 },
            angle: n as f32,
        })
        .collect();
    let projection = ArenaProjection::with_geometry(arena(), 11, 30.0);
    for (layer, painter) in [
        (
            "ambient",
            fx::ambient as fn(&Painter, ArenaProjection, &[Particle]),
        ),
        ("impact", fx::particles),
    ] {
        let shapes = painted(|p| painter(p, projection, &particles));
        assert_eq!(shapes.len(), 1, "{layer}: one shape for every particle");
        let mesh = meshes(&shapes)[0];
        assert_eq!(mesh.texture_id, egui::TextureId::default());
        assert!(mesh.vertices.iter().all(|v| v.uv == egui::epaint::WHITE_UV));
    }
    // Lines are 4-vertex quads and dots 8-segment fans around a hub.
    let impact = painted(|p| fx::particles(p, projection, &particles));
    let lines = particles
        .iter()
        .filter(|p| {
            matches!(
                p.kind,
                ParticleKind::Casing | ParticleKind::Spark | ParticleKind::Debris
            )
        })
        .count();
    let dots = particles
        .iter()
        .filter(|p| p.kind == ParticleKind::Dust && p.alpha > 0.15)
        .count();
    assert_eq!(meshes(&impact)[0].vertices.len(), lines * 4 + dots * 9);
    assert_eq!(meshes(&impact)[0].indices.len(), lines * 6 + dots * 8 * 3);
    assert!(painted(|p| fx::particles(p, projection, &[])).is_empty());
}

#[test]
fn fx_painters_draw_nothing_past_fx_lifetime() {
    use horde_core::sim::EventKind;
    let game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt().unwrap();
    let mut view =
        BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    let kinds = [
        EventKind::Shot,
        EventKind::Hit,
        EventKind::WeakPoint,
        EventKind::Armor,
        EventKind::Kill,
        EventKind::Damage,
        EventKind::Melee,
        EventKind::Spawn,
        EventKind::Boss,
        EventKind::BossWarning,
        EventKind::Reinforcements,
        EventKind::Explosion,
    ];
    let event = |kind, age| crate::battle_view::FxView {
        kind,
        age,
        from: [2.0, 3.5],
        to: [12.0, 3.5],
        value: 150,
        lane: Some(3),
        actor_id: None,
        target_id: None,
    };
    let projection = ArenaProjection::for_attempt(arena(), a);
    let options = fx::FxOptions {
        numbers: true,
        reduced: false,
        blood: true,
        textures: textures(),
    };
    for kind in kinds {
        view.fx = vec![event(kind, 0.0)];
        let fresh = painted(|p| fx::events(p, projection, &view, None, &options));
        assert!(!fresh.is_empty(), "{kind:?} draws when fresh");
        view.fx = vec![event(kind, fx::fx_lifetime(kind) + 1.0 / 60.0)];
        let stale = painted(|p| fx::events(p, projection, &view, None, &options));
        assert!(stale.is_empty(), "{kind:?} draws past its lifetime");
    }
}

/// Gives `game`'s attempt `count` saved bodies, copies of its staged ones
/// with fresh ids at ascending ticks: the newest `fresh` died this tick, the
/// rest long ago.
fn stage_bodies(game: &mut Game, count: usize, fresh: usize) {
    let a = game.attempt_mut().unwrap();
    a.tick = 1_000_000;
    let staged = std::mem::take(&mut a.corpses);
    assert!(!staged.is_empty(), "the fixture stages bodies");
    a.corpses = (0..count)
        .map(|n| horde_core::sim::CorpseRecord {
            id: 5_000_000 + n as u64,
            tick: if n + fresh < count { n as u64 } else { a.tick },
            ..staged[n % staged.len()].clone()
        })
        .collect();
}

/// One pass of `game`'s battlefield on `ctx`: the painter's layer, its
/// shapes and what it drew.
fn paint_pass(
    ctx: &egui::Context,
    game: &Game,
    rect: Rect,
    options: RenderOptions,
) -> (egui::LayerId, Vec<egui::Shape>, SceneDraw) {
    let a = game.attempt().unwrap();
    let motion = SceneAnimation::sample(a, options.reduced);
    let view = BattleViewModel::build(a, motion, SceneBlend::NONE, options.reduced);
    let mut drawn = (egui::LayerId::background(), SceneDraw::default());
    let mut options = Some(options);
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let painter = ui.painter().with_clip_rect(rect);
        let options = options.take().expect("one pass");
        drawn = (
            painter.layer_id(),
            battlefield(&painter, rect, &view, a, [10.0, 5.0], options),
        );
    });
    output.textures_delta.clear();
    let shapes = output.shapes.into_iter().map(|shape| shape.shape).collect();
    (drawn.0, shapes, drawn.1)
}

/// The painted shape that is the layer's cached settled-corpse mesh.
fn settled_mesh_of(shapes: &[egui::Shape], cached: &corpses::SettledMesh) -> Arc<egui::Mesh> {
    shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Mesh(mesh) if Arc::ptr_eq(mesh, &cached.mesh) => Some(Arc::clone(mesh)),
            _ => None,
        })
        .expect("the cached settled mesh is painted")
}

#[test]
fn settled_mesh_is_reused_until_a_body_settles() {
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    stage_bodies(&mut game, 1_500, 1);
    let ctx = egui::Context::default();
    let (layer, first, drawn) = paint_pass(&ctx, &game, arena(), options(textures()));
    assert_eq!(drawn.settled_corpses, 1_499);
    assert_eq!(drawn.collapsing_corpses, 1);
    let cached = corpses::cached(&ctx, layer).expect("a cached settled mesh");
    assert_eq!((cached.count, cached.rebuilds), (1_499, 1));
    let first_mesh = settled_mesh_of(&first, &cached);
    assert_eq!(first_mesh.vertices.len(), 4 * 1_499);
    assert_eq!(
        first_mesh.texture_id,
        textures().battle_atlas.unwrap().texture,
        "settled bodies sample the packed atlas"
    );

    // Nothing settles: the same mesh is painted again, nothing is rebuilt.
    let (_, second, _) = paint_pass(&ctx, &game, arena(), options(textures()));
    let cached = corpses::cached(&ctx, layer).unwrap();
    assert!(Arc::ptr_eq(&settled_mesh_of(&second, &cached), &first_mesh));
    assert_eq!(cached.rebuilds, 1);

    // The last body settles: it is appended, the others are kept.
    game.attempt_mut().unwrap().tick += crate::animation::COLLAPSE_TICKS;
    let (_, third, drawn) = paint_pass(&ctx, &game, arena(), options(textures()));
    assert_eq!(
        (drawn.settled_corpses, drawn.collapsing_corpses),
        (1_500, 0)
    );
    let cached = corpses::cached(&ctx, layer).unwrap();
    assert_eq!(
        (cached.count, cached.rebuilds),
        (1_500, 1),
        "appended, not rebuilt"
    );
    let grown = settled_mesh_of(&third, &cached);
    assert_eq!(grown.vertices.len(), 4 * 1_500);
    assert_eq!(grown.vertices[..4 * 1_499], first_mesh.vertices[..]);
    assert_eq!(grown.indices[..6 * 1_499], first_mesh.indices[..]);

    // The appended quad is the pure corpse quad of the newest settled view.
    let a = game.attempt().unwrap();
    let view = BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    let projection = ArenaProjection::for_attempt(arena(), a);
    let body = view.settled.view(a, 1_499).unwrap();
    let height = body.height.pixels(projection.lane_height());
    let quad = visual_assets::settled_corpse_quad(
        textures(),
        projection.world(body.at[0], body.at[1]) + vec2(0., height * 0.28),
        vec2(height * 1.25, height * 0.45),
        enemies::tint(body.tint).gamma_multiply(0.8),
    )
    .unwrap();
    let mut expected = egui::Mesh::with_texture(quad.texture);
    quad.push(&mut expected);
    assert_eq!(grown.vertices[4 * 1_499..], expected.vertices[..]);
}

#[test]
fn corpse_cache_rebuilds_on_new_attempt_resize_or_atlas_reload() {
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    stage_bodies(&mut game, 300, 0);
    let ctx = egui::Context::default();
    let rebuilds = |game: &Game, rect: Rect, options: RenderOptions| {
        let (layer, shapes, drawn) = paint_pass(&ctx, game, rect, options);
        let cached = corpses::cached(&ctx, layer).unwrap();
        assert_eq!(cached.count, drawn.settled_corpses);
        assert_eq!(
            settled_mesh_of(&shapes, &cached).vertices.len(),
            4 * cached.count
        );
        cached.rebuilds
    };
    let bloodless = |textures| RenderOptions {
        blood: false,
        ..options(textures)
    };
    assert_eq!(rebuilds(&game, arena(), options(textures())), 1);
    assert_eq!(
        rebuilds(&game, arena(), options(textures())),
        1,
        "unchanged"
    );
    // A resized arena.
    let resized = Rect::from_min_size(pos2(24.0, 124.0), vec2(1166.6, 692.0));
    assert_eq!(rebuilds(&game, resized, options(textures())), 2);
    assert_eq!(rebuilds(&game, resized, options(textures())), 2);
    // A reloaded atlas is a new texture.
    let reloaded = ArtTextures {
        battle_atlas: Some(BattleAtlas::layout(
            TextureId::User(50),
            [1774, 887],
            [1122, 1402],
        )),
        ..textures()
    };
    assert_eq!(rebuilds(&game, resized, options(reloaded)), 3);
    // A new attempt, even with the same bodies.
    game.attempt_mut().unwrap().id += 1;
    assert_eq!(rebuilds(&game, resized, options(reloaded)), 4);
    // Fewer bodies (an earlier save of the same attempt).
    game.attempt_mut().unwrap().corpses.truncate(200);
    assert_eq!(rebuilds(&game, resized, options(reloaded)), 5);
    // A different newest body at the same count.
    let a = game.attempt_mut().unwrap();
    a.corpses.truncate(199);
    let mut other = a.corpses[0].clone();
    other.id = 9_000_000;
    a.corpses.push(other);
    assert_eq!(rebuilds(&game, resized, options(reloaded)), 6);
    // The blood setting is part of the key.
    assert_eq!(rebuilds(&game, resized, bloodless(reloaded)), 7);
    // Another body settling only appends.
    let a = game.attempt_mut().unwrap();
    let mut next = a.corpses[1].clone();
    next.id = 9_000_001;
    a.corpses.push(next);
    assert_eq!(rebuilds(&game, resized, bloodless(reloaded)), 7);
}

/// A loaded save of another run can reuse the attempt id and the corpse ids
/// (both restart at 1 every campaign): the cache must not keep its bodies.
#[test]
fn corpse_cache_does_not_keep_another_runs_bodies_with_the_same_ids() {
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    stage_bodies(&mut game, 300, 0);
    let ctx = egui::Context::default();
    // The settled mesh `ctx` paints, its rebuild count, and the mesh a fresh
    // context paints for the same game.
    let pass = |game: &Game| {
        let (layer, shapes, _) = paint_pass(&ctx, game, arena(), options(textures()));
        let cached = corpses::cached(&ctx, layer).unwrap();
        let painted = settled_mesh_of(&shapes, &cached);
        let fresh = egui::Context::default();
        let (layer, shapes, _) = paint_pass(&fresh, game, arena(), options(textures()));
        let expected = settled_mesh_of(&shapes, &corpses::cached(&fresh, layer).unwrap());
        (cached.rebuilds, painted.vertices == expected.vertices)
    };
    assert_eq!(pass(&game), (1, true));
    let mut rebuilds = 1;
    let mut expect_rebuild = |game: &Game, what: &str| {
        rebuilds += 1;
        assert_eq!(pass(game), (rebuilds, true), "{what}");
    };
    // Another run: the same attempt id and corpse ids, other bodies and lanes.
    let a = game.attempt_mut().unwrap();
    for body in &mut a.corpses {
        body.at = [body.at[0] * 0.5 + 1.0, body.at[1] * 0.5];
    }
    a.lane_count -= 4;
    expect_rebuild(&game, "other positions and lane count, same ids");
    // Only the newest body differs (a divergent save of the same attempt).
    let a = game.attempt_mut().unwrap();
    a.corpses.last_mut().unwrap().at[0] += 2.0;
    expect_rebuild(&game, "another newest body with the same id");
    // Only the first body differs.
    a_mut(&mut game).corpses[0].at[0] += 2.0;
    expect_rebuild(&game, "another first body with the same id");
    let a = a_mut(&mut game);
    a.corpses.last_mut().unwrap().tick += 1;
    expect_rebuild(&game, "another newest tick with the same id");
    a_mut(&mut game).width_yards += 5.0;
    expect_rebuild(&game, "another arena width");
    a_mut(&mut game).country_id = Some("another-country".into());
    expect_rebuild(&game, "another campaign country");
    let a = a_mut(&mut game);
    a.level = (a.level + 1) % horde_core::content::content().levels.len();
    expect_rebuild(&game, "another level");
    let a = a_mut(&mut game);
    a.mode = match a.mode {
        horde_core::content::Mode::Personal => horde_core::content::Mode::Npc,
        horde_core::content::Mode::Npc => horde_core::content::Mode::Personal,
    };
    expect_rebuild(&game, "another mode");
    // The same run again only repaints.
    assert_eq!(pass(&game), (rebuilds, true), "unchanged");
}

fn a_mut(game: &mut Game) -> &mut horde_core::sim::Attempt {
    game.attempt_mut().unwrap()
}

/// Bodies settling one per pass on one context are each built once: the
/// settled mesh is never rebuilt below the cap, so the paint does not grow
/// with the bodies already on the ground.
#[test]
fn settled_bodies_are_built_once_across_passes_below_the_cap() {
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    stage_bodies(&mut game, 1_000, 0);
    let ctx = egui::Context::default();
    let mut layer = egui::LayerId::background();
    for pass in 0..200 {
        let a = a_mut(&mut game);
        let mut next = a.corpses[pass % 7].clone();
        next.id = 6_000_000 + pass as u64;
        next.tick = 1_000 + pass as u64;
        a.corpses.push(next);
        let (painted, _, drawn) = paint_pass(&ctx, &game, arena(), options(textures()));
        layer = painted;
        assert_eq!(drawn.settled_corpses, 1_001 + pass);
    }
    let cached = corpses::cached(&ctx, layer).unwrap();
    assert_eq!(
        (cached.rebuilds, cached.quads_built, cached.count),
        (1, 1_200, 1_200),
        "every body built once, the mesh never rebuilt"
    );
}

#[test]
fn corpse_draw_respects_caps_with_5000_records() {
    use crate::perf_budget::{
        BATTLEFIELD_PRIMITIVES_MAX, CORPSES_COLLAPSING_DRAWN, CORPSES_SETTLED_DRAWN,
    };
    let mut game = horde_core::sim::stress_render_fixture().expect("stress fixture");
    for (fresh, collapsing) in [(5_000, CORPSES_COLLAPSING_DRAWN), (0, 0), (30, 30)] {
        stage_bodies(&mut game, 5_000, fresh);
        let before = serde_json::to_string(game.attempt().unwrap()).unwrap();
        let ctx = egui::Context::default();
        let (layer, shapes, drawn) = paint_pass(&ctx, &game, arena(), options(textures()));
        assert_eq!(
            (drawn.settled_corpses, drawn.collapsing_corpses),
            (CORPSES_SETTLED_DRAWN, collapsing),
            "{fresh} fresh bodies of 5000"
        );
        let cached = corpses::cached(&ctx, layer).unwrap();
        assert_eq!(
            settled_mesh_of(&shapes, &cached).vertices.len(),
            4 * CORPSES_SETTLED_DRAWN
        );
        // The newest settled bodies are the ones drawn.
        let a = game.attempt().unwrap();
        let settled = 5_000 - collapsing;
        assert_eq!(crate::animation::settled_corpse_count(a, false), settled);
        assert_eq!(
            cached.key.first.id,
            a.corpses[settled - CORPSES_SETTLED_DRAWN].id
        );
        // Saved records are untouched and uncapped.
        assert_eq!(a.corpses.len(), 5_000);
        assert_eq!(serde_json::to_string(a).unwrap(), before);
        let primitives = battlefield_primitives(&game, textures()).len();
        assert!(
            primitives <= BATTLEFIELD_PRIMITIVES_MAX,
            "{primitives} primitives"
        );
    }
}

/// W1-66: the battlefield reports its hit flashes for the frame report's
/// `scene_load`: the hostiles that flashed and the quads their flashes drew
/// into the sprite layer.
#[test]
fn battlefield_reports_hit_flash_quads() {
    let (game, view) = mixed_sheet_view();
    let a = game.attempt().unwrap();
    let textures = textures();
    let flashing = view
        .enemies
        .iter()
        .filter(|e| e.hit_flash.is_some())
        .count();
    assert!(flashing > 10, "{flashing} flashing hostiles");
    let mut drawn = SceneDraw::default();
    let shapes = painted(|p| {
        drawn = battlefield(p, arena(), &view, a, [10.0, 5.0], options(textures));
    });
    assert_eq!(drawn.flashing_enemies, flashing);
    // Every flash vertex is additive in the sprite layer, so the quads it
    // reports are the sprite layer's additive quads.
    let atlas = textures.battle_atlas.unwrap();
    let sprites = meshes(&shapes)
        .into_iter()
        .find(|mesh| mesh.texture_id == atlas.texture)
        .expect("the sprite layer");
    let additive = sprites
        .vertices
        .iter()
        .filter(|v| v.color.a() == 0 && v.color.r() > 0)
        .count();
    assert!(drawn.flash_quads >= flashing);
    assert_eq!(4 * drawn.flash_quads, additive);
}
