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

mod appearance;

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
        scene: SceneLook::neutral(0),
        sky: SkyClock::default(),
        // Beside the arena, so the canvas-clipped grade and vignette stay
        // out of the arena's own primitives, as in the battle screen.
        canvas: arena().expand(24.0),
        textures,
        occluder: None,
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
/// paints its battlefield in `rect` on every tick `paint` selects.
/// `stress-5000-corpses` is the stress fixture with both corpse draw caps
/// full ([`stage_bodies`], which moves the attempt to tick 1,000,000 first).
fn live_battlefield_load(
    scene: &str,
    ticks: usize,
    paint: &dyn Fn(usize) -> bool,
    rect: Rect,
) -> LiveLoad {
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
        if paint(tick) {
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
/// Each run paints every 2nd tick, the measurement's own stride, of the
/// first 420 (R22; the W1-51 runs painted every 10th tick, which missed the
/// stress primitive maximum at tick 2) and also the exact ticks the
/// full-length `measure_battlefield_egui_load` first painted its maxima at
/// (`PERF_BUDGET.json` `evidence.egui_measured.max_tick_by_scene`), running
/// to the latest of them. On the W2-47 stress fixture the stress vertex
/// maxima fall at ticks 1980 and 1700 (full corpse caps), past tick 420.
///
/// W2 review (tests): the run paints a subset of the measurement's even
/// ticks that holds the recorded maximum ticks, so while the evidence is
/// current its maxima and their first ticks equal the recorded
/// `battlefield_primitives_by_scene`, `battlefield_vertices_by_scene` and
/// `max_tick_by_scene` exactly. A fixture change that moves or changes a
/// recorded maximum (as 787929f did, unnoticed) fails here until
/// `measure_battlefield_egui_load` is re-run and the evidence updated.
#[test]
fn live_battlefield_runs_stay_within_the_egui_gates() {
    for scene in LIVE_GATED_SCENES {
        let plan = live_gate_plan(scene);
        let load = live_battlefield_load(scene, plan.ticks, &|tick| plan.paints(tick), arena());
        assert_live_load_within_gates(scene, "normal", plan.ticks, &load);
        assert_eq!(
            (
                load.primitives_max,
                load.primitives_max_tick,
                load.vertices_max,
                load.vertices_max_tick
            ),
            recorded_maxima(scene),
            "{scene} normal: (primitives, tick, vertices, tick) no longer match PERF_BUDGET.json \
             evidence.egui_measured; re-run measure_battlefield_egui_load"
        );
    }
}

/// The perf scenes the live gate runs.
const LIVE_GATED_SCENES: [&str; 3] = ["progressed-peak", "stress", "stress-5000-corpses"];
/// The live gate paints every `LIVE_GATE_STRIDE`-th tick (the measurement's
/// own stride) of the first `LIVE_GATE_TICKS`.
const LIVE_GATE_TICKS: usize = 420;
const LIVE_GATE_STRIDE: usize = 2;

/// The ticks the live gate runs and paints for one scene: every 2nd tick of
/// the first 420, plus each recorded maximum tick, running to the latest.
struct LiveGatePlan {
    ticks: usize,
    maxima: Vec<usize>,
}

impl LiveGatePlan {
    fn paints(&self, tick: usize) -> bool {
        tick <= self.ticks
            && ((tick <= LIVE_GATE_TICKS && tick.is_multiple_of(LIVE_GATE_STRIDE))
                || self.maxima.contains(&tick))
    }
}

fn live_gate_plan(scene: &str) -> LiveGatePlan {
    let maxima = recorded_max_ticks(scene);
    let ticks = maxima.iter().copied().fold(LIVE_GATE_TICKS, usize::max);
    LiveGatePlan { ticks, maxima }
}

/// The count at `PERF_BUDGET.json` `evidence.egui_measured` + `/path`.
fn recorded_count(path: &str) -> usize {
    const BUDGET: &str = include_str!("../../../../../docs/PERF_BUDGET.json");
    let json: serde_json::Value = serde_json::from_str(BUDGET).expect("PERF_BUDGET.json parses");
    let path = format!("/evidence/egui_measured/{path}");
    json.pointer(&path)
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_else(|| panic!("PERF_BUDGET.json {path} is not a count"))
}

/// `PERF_BUDGET.json` `evidence.egui_measured.max_tick_by_scene` for `scene`:
/// the ticks the full-length measurement first painted its primitive and
/// vertex maxima at.
fn recorded_max_ticks(scene: &str) -> Vec<usize> {
    ["primitives", "vertices"]
        .iter()
        .map(|kind| recorded_count(&format!("max_tick_by_scene/{scene}/{kind}")))
        .collect()
}

/// The full-length measurement's maxima for `scene` at the normal arena, as
/// (primitives, first tick, vertices, first tick).
fn recorded_maxima(scene: &str) -> (usize, usize, usize, usize) {
    (
        recorded_count(&format!("battlefield_primitives_by_scene/{scene}")),
        recorded_count(&format!("max_tick_by_scene/{scene}/primitives")),
        recorded_count(&format!("battlefield_vertices_by_scene/{scene}")),
        recorded_count(&format!("max_tick_by_scene/{scene}/vertices")),
    )
}

/// W2 review (perf-diagnostics): the live gate's plan paints every tick
/// `max_tick_by_scene` records and runs to the latest of them. This pins
/// only the plan builder; whether the recorded ticks are still where the
/// current fixtures put their maxima is checked by
/// `live_battlefield_runs_stay_within_the_egui_gates`, which requires its
/// maxima and their ticks to equal the recorded evidence.
#[test]
fn live_battlefield_gate_paints_every_recorded_maximum_tick() {
    for scene in LIVE_GATED_SCENES {
        let plan = live_gate_plan(scene);
        for tick in recorded_max_ticks(scene) {
            assert!(
                plan.paints(tick),
                "{scene}: the live gate (first {} ticks) never paints recorded maximum tick {tick}",
                plan.ticks
            );
        }
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
            let load = live_battlefield_load(scene, ticks, &|tick| tick % 2 == 0, rect);
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

/// W2-47: with the rig sheet loaded (as in a live run), fire-zone flames are
/// rig-sheet quads. Painting each zone's ground and then its flames
/// alternated untextured and textured runs per zone, so the stress scene's
/// three zones cost six primitives and the live battlefield drew 25, over
/// `BATTLEFIELD_PRIMITIVES_MAX` (the frame report's live battlefield count;
/// the headless gates have no rig sheet). Every zone's ground, then every
/// zone's flames, costs the same for one zone or three.
#[test]
fn fire_zones_cost_the_same_primitives_for_one_zone_or_three() {
    use crate::visual_assets::{RigAtlas, RigSheet};
    let mut textures = textures();
    textures.rig = Some(RigSheet {
        texture: TextureId::User(7),
        atlas: RigAtlas::grid([1024.0, 1024.0]),
    });
    let three = horde_core::sim::stress_render_fixture().expect("stress fixture");
    assert_eq!(three.attempt().unwrap().ordnance.fire_zones.len(), 3);
    let mut one = horde_core::sim::stress_render_fixture().expect("stress fixture");
    one.attempt_mut().unwrap().ordnance.fire_zones.truncate(1);
    let at_three = battlefield_primitives(&three, textures).len();
    let at_one = battlefield_primitives(&one, textures).len();
    assert_eq!(
        at_three, at_one,
        "three fire zones paint {at_three} primitives, one paints {at_one}"
    );
    assert!(
        at_three <= crate::perf_budget::BATTLEFIELD_PRIMITIVES_MAX,
        "stress with the rig sheet paints {at_three} primitives"
    );
}

/// The peak view with alternating walker and standing sprites (so the actor
/// layer crosses sheets enemy by enemy) and every third hostile flashing.
/// Two visible hostiles share one spot, as a crowd does at a wall or the line
/// since W1-69, and the later id has the farther drawn feet, so the painter
/// order's drawn-feet tie-break and an id tie-break disagree on them.
fn mixed_sheet_view() -> (Game, BattleViewModel) {
    use crate::battle_view::crowd_stagger;
    let mut game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt_mut().unwrap();
    let visible: Vec<usize> = (0..a.enemies.len())
        .filter(|&n| {
            let e = &a.enemies[n];
            e.condition(a.mode) != horde_core::sim::EnemyCondition::Defeated && e.in_view()
        })
        .collect();
    let (under, onto) = visible
        .iter()
        .flat_map(|&p| visible.iter().map(move |&q| (p, q)))
        .find(|&(p, q)| {
            let (p, q) = (&a.enemies[p], &a.enemies[q]);
            p.id < q.id && crowd_stagger(q.id)[1] < crowd_stagger(p.id)[1]
        })
        .expect("two hostiles whose id and drawn-feet orders disagree");
    let (lane, x, pass_offset) = {
        let e = &a.enemies[under];
        (e.lane, e.x, e.pass_offset)
    };
    let e = &mut a.enemies[onto];
    (e.lane, e.x, e.pass_offset) = (lane, x, pass_offset);
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
            let [x, y] = e.drawn();
            let painted_at = arena.world(
                x + pose.offset[0],
                y + if has_walk { 0. } else { pose.offset[1] },
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
                // The flash is one quad on the packed silhouette (W1-66).
                let masks = visual_assets::flash_masks(p.ctx()).unwrap();
                let mut flash = egui::Mesh::default();
                let quads = visual_assets::hit_flash_quad(
                    &masks,
                    textures,
                    actor,
                    painted_at,
                    size,
                    visual_assets::HitFlash {
                        strength,
                        reduced: false,
                    },
                    pose,
                    &mut flash,
                    &textures.battle_atlas.unwrap(),
                );
                assert_eq!(quads, 1);
                p.add(egui::Shape::mesh(flash));
            }
        }
    });
    meshes(&shapes).into_iter().cloned().collect()
}

#[test]
fn batched_actor_layer_keeps_painter_order_across_sheets() {
    use crate::battle_view::crowd_stagger;
    let (game, view) = mixed_sheet_view();
    let a = game.attempt().unwrap();
    let textures = textures();
    let atlas = textures.battle_atlas.unwrap();
    let a_enemy = |id: u64| a.enemies.iter().find(|e| e.id == id).unwrap();
    // The production painter key (`BattleViewModel::build_with`): lane feet,
    // far to near, then the drawn feet of a stack on one spot, then id.
    let key = |id: u64| {
        let e = a_enemy(id);
        (e.y(), -e.x, e.y() + crowd_stagger(e.id)[1], e.id)
    };
    assert!(
        view.enemies
            .windows(2)
            .all(|pair| key(pair[0].id) <= key(pair[1].id)),
        "the view is in lane-feet, x-descending, drawn-feet, id order"
    );
    assert!(
        view.enemies.windows(2).any(|pair| {
            let (p, q) = (a_enemy(pair[0].id), a_enemy(pair[1].id));
            (p.y(), p.x) == (q.y(), q.x) && p.id > q.id
        }),
        "a stack on one spot paints by drawn feet, not by id"
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
    // The legacy painters, enemy by enemy: sprite, then its flash quad
    // (already in atlas coordinates).
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
                None => vertex.uv,
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

/// The pre-W1-48 `actor_animated` pose, kept verbatim as the pin: the quad's
/// mesh is sheared and scaled about the pivot, then turned with egui's own
/// `Mesh::rotate`. `pose_vertices` must reproduce it bit for bit.
fn legacy_posed_mesh(quad: &visual_assets::ActorQuad) -> egui::Mesh {
    let (pose, pivot) = (quad.pose, quad.pivot);
    let mut mesh = egui::Mesh::with_texture(quad.texture);
    mesh.add_rect_with_uv(quad.rect, quad.uv, quad.tint);
    for vertex in &mut mesh.vertices {
        let relative = vertex.pos - pivot;
        vertex.pos = pivot
            + vec2(
                relative.x * pose.scale[0] - relative.y * pose.lean * 0.45,
                relative.y * pose.scale[1],
            );
    }
    mesh.rotate(egui::emath::Rot2::from_angle(pose.lean * 0.5), pivot);
    mesh
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
            let quad = visual_assets::actor_quad(
                textures,
                actor,
                center,
                size,
                egui::Color32::WHITE,
                &pose,
            )
            .unwrap();
            let legacy = legacy_posed_mesh(&quad);
            let animated = painted(|p| {
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
            assert_eq!(
                *meshes(&animated)[0],
                legacy,
                "{:?} frame {frame}: the per-actor painter keeps the pre-W1-48 pose",
                sprite.sprite
            );
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

/// Sub-pixel particles keep one physical pixel at reduced opacity, as egui's
/// feathered tessellator did before the batch: a 0.8-pt rain streak at 1.0
/// pixels per point is a 1-px quad at 80 % alpha (continuous, not dashed), and
/// a 0.5-pt ash or dust mote is a fan whose inscribed circle reaches the pixel
/// lattice's covering radius (so it always covers a pixel centre and never
/// blinks), its alpha scaled by the area ratio so its coverage is kept.
#[test]
fn sub_pixel_particles_keep_one_pixel_at_reduced_opacity() {
    use crate::animation::{Particle, ParticleKind};
    let projection = ArenaProjection::with_geometry(arena(), 11, 30.0);
    let particle = |kind, size| Particle {
        kind,
        at: [10.0, 5.0],
        size,
        alpha: 0.1,
        angle: 1.0,
    };
    let source_alpha = (0.1_f32 * 255.0) as u8;
    let rain = painted(|p| fx::ambient(p, projection, &[particle(ParticleKind::Rain, 0.08)]));
    let quad = &meshes(&rain)[0].vertices;
    let width = (quad[0].pos - quad[1].pos).length();
    assert!(
        (width - 1.0).abs() < 1e-4,
        "a 0.8-pt rain streak is {width} pt wide at 1.0 ppp"
    );
    assert_eq!(
        quad[0].color.a(),
        (f32::from(source_alpha) * 0.8).round() as u8,
        "the widened streak keeps its coverage"
    );
    let min_radius = std::f32::consts::FRAC_1_SQRT_2 / (std::f32::consts::PI / 8.0).cos();
    for kind in [ParticleKind::Ash, ParticleKind::Dust] {
        let mote = painted(|p| fx::ambient(p, projection, &[particle(kind, 0.001)]));
        let fan = &meshes(&mote)[0].vertices;
        let radius = (fan[1].pos - fan[0].pos).length();
        assert!(
            radius >= min_radius - 1e-4,
            "{kind:?}: a 0.5-pt mote is a {radius}-pt fan, under the {min_radius}-pt floor"
        );
        let kept = f32::from(fan[0].color.a()) * radius * radius;
        let source = f32::from(source_alpha) * 0.5 * 0.5;
        assert!(
            (kept - source).abs() <= radius * radius,
            "{kind:?}: coverage {kept} against {source}"
        );
    }
    // Wider than a pixel, nothing changes: the 3-pt gauge line and a 2-pt dot.
    let mut mesh = batch::MeshBuilder::untextured(2).pixel_floor(1.0);
    mesh.line(pos2(0.0, 0.0), pos2(10.0, 0.0), 3.0, egui::Color32::WHITE);
    mesh.fan(pos2(20.0, 0.0), vec2(2.0, 2.0), 8, egui::Color32::WHITE);
    let built = &mesh.mesh_mut().vertices;
    assert_eq!((built[0].pos - built[1].pos).length(), 3.0);
    assert_eq!((built[5].pos - built[4].pos).length(), 2.0);
    assert!(built.iter().all(|v| v.color == egui::Color32::WHITE));
    // At 2.0 pixels per point a 0.8-pt streak is already 1.6 px wide.
    let mut mesh = batch::MeshBuilder::untextured(1).pixel_floor(2.0);
    mesh.line(pos2(0.0, 0.0), pos2(10.0, 0.0), 0.8, egui::Color32::WHITE);
    let built = &mesh.mesh_mut().vertices;
    assert!(((built[0].pos - built[1].pos).length() - 0.8).abs() < 1e-5);
    assert_eq!(built[0].color, egui::Color32::WHITE);
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
        tick: 0,
        seq: 0,
        volley: Some(crate::battle_view::Volley::SINGLE),
        source: Some(crate::art::FxSource::Turret),
    };
    let projection = ArenaProjection::for_attempt(arena(), a);
    let options = fx::FxOptions {
        numbers: true,
        reduced: false,
        blood: true,
        textures: textures(),
        occluder: None,
    };
    // Both halves of the event painters: the world pass and the cue pass.
    let both = |view: &BattleViewModel| {
        painted(|p| {
            fx::events(p, projection, view, None, &options);
            let mut batch = cue::CueBatch::new(0);
            fx::event_cues(&mut batch, projection, view, &options);
            batch.paint(p);
        })
    };
    for kind in kinds {
        view.fx = vec![event(kind, 0.0)];
        let fresh = both(&view);
        assert!(!fresh.is_empty(), "{kind:?} draws when fresh");
        view.fx = vec![event(kind, fx::fx_lifetime(kind) + 1.0 / 60.0)];
        let stale = both(&view);
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

/// W1-66: with the packed atlas every hit flash is one posed quad on its
/// white silhouette, right after its sprite: the sprite layer holds one quad
/// per hostile plus one per flashing hostile, in normal and reduced effects.
#[test]
fn hit_flash_is_one_quad_per_sprite() {
    let (game, view) = mixed_sheet_view();
    let a = game.attempt().unwrap();
    let textures = textures();
    let atlas = textures.battle_atlas.unwrap();
    let flashing = view
        .enemies
        .iter()
        .filter(|e| e.hit_flash.is_some())
        .count();
    assert!(flashing > 10, "{flashing} flashing hostiles");
    for reduced in [false, true] {
        let mut drawn = enemies::HostilesDrawn::default();
        let shapes = painted(|p| {
            drawn = enemies::paint(
                p,
                ArenaProjection::for_attempt(arena(), a),
                &view,
                textures,
                false,
                reduced,
                rig_shadow(),
            );
        });
        assert_eq!((drawn.flashing, drawn.flash_quads), (flashing, flashing));
        let sprites = meshes(&shapes)
            .into_iter()
            .find(|mesh| mesh.texture_id == atlas.texture)
            .expect("the sprite layer");
        assert_eq!(
            sprites.vertices.len(),
            4 * (view.enemies.len() + flashing),
            "reduced {reduced}"
        );
        // Each flash quad samples the silhouette column, never the white
        // block or a sheet.
        let column_top = atlas.silhouette_origin()[1] as f32 / atlas.size[1] as f32;
        let column_right = atlas.actors_size[0] as f32 / atlas.size[0] as f32;
        let flash_vertices = sprites
            .vertices
            .iter()
            .filter(|v| v.uv.y >= column_top && v.uv.x <= column_right)
            .count();
        assert_eq!(flash_vertices, 4 * flashing, "reduced {reduced}");
    }
}

/// Struck structures cost no primitives (W1-69): since enemies stop queueing,
/// every wall in a 17-lane field can be struck in one frame. Each hit outline
/// used to split the props pass (props texture, outline, props texture...),
/// so the field passed the 48-primitive gate at the progressed peak.
#[test]
fn struck_structures_add_no_primitives() {
    let game = horde_core::sim::maximum_progression_render_fixture().unwrap();
    let a = game.attempt().unwrap();
    let primitives = |hit: f32| {
        let mut view =
            BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
        for s in &mut view.structures {
            s.pose.hit = hit;
        }
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter().with_clip_rect(arena());
            battlefield(
                &painter,
                arena(),
                &view,
                a,
                [10.0, 5.0],
                options(textures()),
            );
        });
        output.textures_delta.clear();
        ctx.tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .filter(|primitive| primitive.clip_rect == arena())
            .count()
    };
    assert!(a.structures.len() >= 10, "a field of structures");
    assert_eq!(primitives(1.0), primitives(0.0));
}

/// A stack on the line (W1-69, enemies overlap) draws one contact band and
/// one windup ring for its lane, and every body keeps its own windup bar.
#[test]
fn a_stack_on_the_line_draws_one_band_and_one_windup_ring() {
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut().attempts = 1;
    game.deploy(0, horde_core::content::Mode::Personal).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    a.paused = false;
    for (lane, x) in [
        (1, 1.5),
        (1, 1.5),
        (1, 1.5),
        (1, 1.5),
        (1, 1.5),
        (1, 1.5),
        (0, 1.5),
        (1, 3.0),
    ] {
        game.spawn_enemy(
            &mut a,
            horde_core::content::EnemyKind::Shambler,
            lane,
            false,
        );
        a.enemies.last_mut().unwrap().x = x;
    }
    *game.attempt_slot_mut() = Some(a);
    game.set_paused(false);
    game.tick(horde_core::sim::Input::default());
    let a = game.attempt().unwrap();
    let view = BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    let winding = view.enemies.iter().filter(|e| e.windup.is_some()).count();
    assert_eq!(
        winding,
        7,
        "the six stacked bodies and the lane-0 body wind up: {:?}",
        a.enemies
            .iter()
            .map(|e| (e.id, e.lane, e.x, e.attack_phase, e.in_view()))
            .collect::<Vec<_>>()
    );
    let lanes: Vec<u8> = enemies::lane_contacts(&view)
        .into_iter()
        .map(|(lane, _)| lane)
        .collect();
    assert_eq!(lanes, [0, 1], "one band per contact lane");
    let rings = enemies::windup_rings(&view);
    let ringed: Vec<(u8, f32)> = view
        .enemies
        .iter()
        .zip(&rings)
        .filter(|(_, ring)| **ring)
        .map(|(e, _)| (e.lane, e.position[0]))
        .collect();
    assert_eq!(ringed, [(0, 1.5), (1, 1.5)], "one ring per stack");
}

/// CF-1-11: a boss at the far right of a 17x30 field at 1050x700@1.35 (the
/// arena is the field, 24/124/24/84 inset) kept its name label centred on
/// its body, so the arena clip cut "SIEGE BRUTE" to "SIEGE BR". Every
/// callout a hostile paints (boss names, EXPOSED, CRIT) stays whole inside
/// the battlefield clip: at the right edge, the left edge and in the top
/// lane, on the batched path and the rejected-art figures. A callout the clip
/// would cut moves the least that keeps it `CALLOUT_MARGIN` inside, along
/// only the axes that cut it; one already inside (the mid-field boss) paints
/// exactly where `Painter::text` did.
#[test]
fn boss_label_at_the_field_edge_stays_inside_the_clip() {
    use horde_core::{balance::BalanceSettings, content::EnemyKind};
    let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1050.0, 700.0) / 1.35);
    let clip = Rect::from_min_max(
        canvas.min + vec2(24.0, 124.0),
        canvas.max - vec2(24.0, 84.0),
    );
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(BalanceSettings {
            start_lanes: 17,
            max_lanes: 17,
            start_width: 30,
            max_width: 30,
            ..Default::default()
        })
        .unwrap();
    game.deploy(0, horde_core::content::Mode::Personal).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    a.enemies.clear();
    let w = a.width_yards;
    // Core spawns one boss; the painter takes copies at the other spots.
    game.spawn_enemy(&mut a, EnemyKind::Brute, 8, true);
    let boss = a.enemies.pop().expect("a boss");
    for (lane, x) in [
        (8, w * 0.5),
        (8, w + 0.5),
        (8, w - 0.3),
        (0, w + 0.5),
        (16, 0.2),
        (0, 0.2),
    ] {
        let mut copy = boss.clone();
        (copy.id, copy.lane, copy.x) = (a.next_id, lane, x);
        a.next_id += 1;
        a.enemies.push(copy);
    }
    *game.attempt_slot_mut() = Some(a);
    let a = game.attempt().unwrap();
    let mut view =
        BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    assert_eq!(view.enemies.len(), 6, "every boss is in view");
    assert!(view.enemies.iter().all(|e| e.label.is_some()));
    // One body is critically wounded, so its CRIT callout is checked too.
    view.enemies[1].health = crate::battle_view::HealthView::Critical;
    let arena = ArenaProjection::with_geometry(clip, view.lanes, view.width);
    // Where `Painter::text` anchored each callout, in paint order: a CRIT
    // above the body, then the name higher up; flagged for the name of the
    // boss past the right edge in the middle lane.
    let sy = arena.lane_height();
    let anchors: Vec<(Pos2, Align2, bool)> =
        view.enemies
            .iter()
            .flat_map(|e| {
                let [x, y] = e.drawn();
                let at = arena.world(x, y);
                let crit = matches!(e.health, crate::battle_view::HealthView::Critical)
                    .then_some((at + vec2(0.0, -sy * 0.44), Align2::CENTER_BOTTOM, false));
                let right = e.lane == 8 && x > w;
                let name = e
                    .label
                    .map(|_| (at + vec2(0.0, -sy * 0.7), Align2::CENTER_CENTER, right));
                crit.into_iter().chain(name)
            })
            .collect();
    let inner = clip.shrink(enemies::CALLOUT_MARGIN);
    for rejected in [false, true] {
        let textures = ArtTextures {
            enemy_art_rejected: rejected,
            ..textures()
        };
        let ctx = egui::Context::default();
        crate::settings::install_font(&ctx);
        crate::settings::configure(&ctx, 1.35);
        crate::visual_assets::publish_source_flash_masks(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(canvas),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            enemies::paint(
                &ui.painter().with_clip_rect(clip),
                arena,
                &view,
                textures,
                false,
                false,
                rig_shadow(),
            );
        });
        output.textures_delta.clear();
        let labels: Vec<(String, Rect)> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 7, "rejected {rejected}: {labels:?}");
        assert_eq!(labels.len(), anchors.len(), "one anchor per callout");
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let mut unmoved = 0;
        for ((text, rect), &(at, align, _)) in labels.iter().zip(&anchors) {
            let was = align.anchor_size(at, rect.size());
            assert!(
                clip.contains_rect(*rect),
                "rejected {rejected}: {text:?} at {rect:?} is cut by the clip {clip:?}"
            );
            if clip.contains_rect(was) {
                unmoved += 1;
                assert!(
                    close(rect.min.x, was.min.x) && close(rect.min.y, was.min.y),
                    "rejected {rejected}: {text:?} inside the clip moved from {was:?} to {rect:?}"
                );
                continue;
            }
            assert!(
                inner.contains_rect(rect.shrink(0.01)),
                "rejected {rejected}: {text:?} at {rect:?} is not the margin inside {inner:?}"
            );
            // The least move: an axis the clip did not cut keeps its place,
            // an axis it cut lands on the margin's nearest edge.
            let least = |lo: f32, hi: f32, min: f32, max: f32, now: f32| {
                if lo >= min && hi <= max {
                    close(now, lo)
                } else if lo < min {
                    close(now, min)
                } else {
                    close(now + (hi - lo), max)
                }
            };
            assert!(
                least(
                    was.left(),
                    was.right(),
                    inner.left(),
                    inner.right(),
                    rect.left()
                ),
                "rejected {rejected}: {text:?} moved further than the margin across: {was:?} -> {rect:?}"
            );
            assert!(
                least(
                    was.top(),
                    was.bottom(),
                    inner.top(),
                    inner.bottom(),
                    rect.top()
                ),
                "rejected {rejected}: {text:?} moved further than the margin down: {was:?} -> {rect:?}"
            );
        }
        assert_eq!(
            unmoved, 1,
            "rejected {rejected}: only the mid-field boss name is unmoved"
        );
        // The right-edge mid-lane boss: pulled in across only, to the margin.
        let edge = anchors.iter().position(|a| a.2).expect("the right boss");
        let ((_, right), (at, ..)) = (&labels[edge], anchors[edge]);
        assert!(
            close(right.right(), inner.right()),
            "{right:?} vs {inner:?}"
        );
        assert!(
            close(right.center().y, at.y),
            "{right:?} moved down from {at:?}"
        );
    }
}

/// Paints `view` of `game`'s attempt and tessellates the arena's primitives.
fn view_primitives(
    game: &Game,
    view: &BattleViewModel,
    textures: ArtTextures,
    reduced: bool,
) -> Vec<ClippedPrimitive> {
    let a = game.attempt().expect("fixture battle");
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let painter = ui.painter().with_clip_rect(arena());
        let options = RenderOptions {
            reduced,
            ..options(textures)
        };
        battlefield(&painter, arena(), view, a, [10.0, 5.0], options);
    });
    output.textures_delta.clear();
    ctx.tessellate(output.shapes, output.pixels_per_point)
        .into_iter()
        .filter(|primitive| primitive.clip_rect == arena())
        .collect()
}

/// W2-28 (ruling W-13): the visual twin of a ranged windup. A spitter
/// stationed behind a wall aims a dashed line at the wall, one with a clear
/// lane at the defender; a melee windup draws none. The line is static, so
/// reduced effects draw the same line.
#[test]
fn spitter_windup_draws_aim_line_to_wall_or_defender() {
    use horde_core::{
        content::{EnemyKind, Mode, StructureKind},
        enemy_catalog::resolve_kind,
        sim::{DEFENSE_LINE, EnemyPhase, Input},
    };
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut().attempts = 1;
    game.campaign_mut().legacy_features = true;
    game.campaign_mut().blueprints = horde_core::kind_map::KindMap::filled(true);
    game.deploy(0, Mode::Personal).unwrap();
    a_mut(&mut game).width_yards = 12.0;
    a_mut(&mut game).max_hp = 1_000_000;
    a_mut(&mut game).hp = 1_000_000;
    game.set_paused(true);
    game.place(StructureKind::Wall, 5, 0).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    let placed = &mut a.structures[0];
    (placed.hp, placed.max_hp) = (1_000_000, 1_000_000);
    let wall = (placed.id, [placed.x(), placed.y()]);
    let reach = 4.0;
    let mut spitters = Vec::new();
    for lane in [5, 2] {
        game.spawn_enemy(&mut a, EnemyKind::Runner, lane, false);
        let e = a.enemies.last_mut().unwrap();
        let mut profile = resolve_kind(EnemyKind::Runner);
        profile.archetype_id = "test-spitter".into();
        profile.reach = reach;
        profile.validate().unwrap();
        e.resolved = Some(profile);
        e.x = DEFENSE_LINE + reach + 0.5;
        spitters.push(e.id);
    }
    game.spawn_enemy(&mut a, EnemyKind::Runner, 8, false);
    a.enemies.last_mut().unwrap().x = DEFENSE_LINE + 0.3;
    let melee = a.enemies.last().unwrap().id;
    *game.attempt_slot_mut() = Some(a);
    game.set_paused(false);
    let ids = [spitters[0], spitters[1], melee];
    let winding = |game: &Game, id: u64| {
        game.attempt()
            .unwrap()
            .enemies
            .iter()
            .any(|e| e.id == id && e.attack_phase == EnemyPhase::Windup)
    };
    for _ in 0..600 {
        if ids.iter().all(|&id| winding(&game, id)) {
            break;
        }
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    for id in ids {
        assert!(winding(&game, id), "{id} winds up: {:?}", a.enemies);
    }
    let first = a.enemies.iter().find(|e| e.id == spitters[0]).unwrap();
    assert_eq!(first.target_structure, Some(wall.0), "the wall intercepts");
    let projection = super::super::ArenaProjection::for_attempt(arena(), a);
    let sy = projection.lane_height();
    for reduced in [false, true] {
        let view = BattleViewModel::build(
            a,
            SceneAnimation::sample(a, reduced),
            SceneBlend::NONE,
            reduced,
        );
        let aim = |id: u64| {
            let e = view.enemies.iter().find(|e| e.id == id).unwrap();
            assert!(
                e.windup.is_some(),
                "reduced {reduced}: {id} shows its windup"
            );
            e.aim_target
        };
        assert_eq!(
            aim(spitters[0]),
            Some(wall.1),
            "reduced {reduced}: at the wall"
        );
        assert_eq!(
            aim(spitters[1]),
            Some(a.defender_position()),
            "reduced {reduced}: at the defender"
        );
        assert_eq!(aim(melee), None, "reduced {reduced}: a melee windup aims");
        // The cue pass draws each aim line as its dashes, one 4-vertex quad
        // apiece on a 4-vertex INK halo (W5-01), and nothing else.
        let dashes: usize = view
            .enemies
            .iter()
            .filter_map(|e| {
                let [x, y] = e.drawn();
                let to = e.aim_target?;
                Some(
                    enemies::aim_dashes(projection.world(x, y), projection.world(to[0], to[1]), sy)
                        .len(),
                )
            })
            .sum();
        assert!(
            dashes >= 16,
            "reduced {reduced}: two dashed lines, {dashes} dashes"
        );
        let mut unaimed = view.clone();
        for e in &mut unaimed.enemies {
            e.aim_target = None;
        }
        assert_eq!(
            vertices(&view_primitives(&game, &view, textures(), reduced)),
            vertices(&view_primitives(&game, &unaimed, textures(), reduced)) + 8 * dashes,
            "reduced {reduced}: the aim lines paint as dashes"
        );
        // The rejected-art path draws them too.
        let rejected = ArtTextures {
            enemy_art_rejected: true,
            ..textures()
        };
        assert!(
            vertices(&view_primitives(&game, &view, rejected, reduced))
                > vertices(&view_primitives(&game, &unaimed, rejected, reduced)),
            "reduced {reduced}: the rejected-art path draws the aim lines"
        );
    }
}

/// W2-37 (ruling W-13, audio AUD-15): the visual twin of `BossPhase`. A
/// boss that crosses half its hit points enters its phase and holds for 90
/// ticks; on every frame of the hold the view carries the phase and the
/// painters draw "PHASE 1 - BREACH" over the boss and a ring at its feet,
/// in normal and reduced effects and on the rejected-art path. The ring
/// breathes in normal effects and is static in reduced; the frame the hold
/// ends draws neither.
#[test]
fn boss_phase_hold_shows_phase_label_and_ring_for_90_ticks() {
    use crate::battle_view::BossPhaseView;
    use horde_core::{
        balance::BalanceSettings,
        content::{EnemyKind, Mode},
        sim::{EventKind, Input},
    };
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(BalanceSettings::legacy())
        .unwrap();
    game.deploy(0, Mode::Personal).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    a.enemies.clear();
    (a.max_hp, a.hp) = (1_000_000, 1_000_000);
    // One transition frozen at spawn, the level's Breacher profile.
    a.progression.boss_phases = 1;
    game.spawn_enemy(&mut a, EnemyKind::Brute, 4, true);
    let boss = a.enemies.last_mut().expect("a boss");
    let id = boss.id;
    boss.x = a.width_yards * 0.6;
    boss.hp = boss.max_hp / 2;
    let hold = boss
        .boss_state
        .as_ref()
        .expect("a phased boss")
        .profile
        .hold_ticks;
    assert_eq!(hold, 90, "the shipped hold");
    *game.attempt_slot_mut() = Some(a);
    game.tick(Input::default());
    let a = game.attempt().unwrap();
    let entry = a
        .events
        .iter()
        .find(|e| e.kind == EventKind::BossPhase && e.actor_id == Some(id))
        .expect("the boss enters its phase on the first tick");
    assert_eq!((entry.value, entry.duration_ticks), (1, hold));
    let (entered, hold_until) = (entry.tick, entry.tick + u64::from(hold));
    let projection = super::super::ArenaProjection::for_attempt(arena(), a);
    let sy = projection.lane_height();
    let label = "PHASE 1 - BREACH";
    let texts = |view: &BattleViewModel, rejected: bool, reduced: bool| -> Vec<String> {
        let textures = ArtTextures {
            enemy_art_rejected: rejected,
            ..textures()
        };
        painted(|p| {
            enemies::paint(p, projection, view, textures, false, reduced, rig_shadow());
        })
        .into_iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
    };
    let mut rings = [Vec::new(), Vec::new()];
    let mut painted_rings = [Vec::new(), Vec::new()];
    let mut frames = [0, 0];
    for frame in 0..hold {
        if frame > 0 {
            game.tick(Input::default());
        }
        let a = game.attempt().unwrap();
        assert_eq!(a.tick, entered + u64::from(frame), "one frame per tick");
        let held = a
            .enemies
            .iter()
            .find(|e| e.id == id)
            .expect("the boss lives");
        assert!(held.boss_held(a.tick), "tick {}: the boss holds", a.tick);
        for reduced in [false, true] {
            let view = BattleViewModel::build(
                a,
                SceneAnimation::sample(a, reduced),
                SceneBlend::NONE,
                reduced,
            );
            let phase = view.boss_phase.clone();
            assert_eq!(
                phase,
                Some(BossPhaseView {
                    boss_id: id,
                    transition: 1,
                    name: "Breach".into(),
                    hold_left_ticks: hold_until - a.tick,
                }),
                "reduced {reduced}: tick {} of the hold",
                a.tick
            );
            let phase = phase.unwrap();
            assert_eq!(phase.label(), label);
            let unphased = BattleViewModel {
                boss_phase: None,
                ..view.clone()
            };
            // The painters draw a pure function of the view: painting the
            // first, middle and last frames of the hold keeps the test fast.
            let painting = [0, hold / 2, hold - 1].contains(&frame);
            for rejected in [false, true].into_iter().filter(|_| painting) {
                assert!(
                    texts(&view, rejected, reduced).iter().any(|t| t == label),
                    "reduced {reduced}, rejected {rejected}: tick {} draws {label:?}",
                    a.tick
                );
                assert!(
                    !texts(&unphased, rejected, reduced)
                        .iter()
                        .any(|t| t == label)
                );
                let art = ArtTextures {
                    enemy_art_rejected: rejected,
                    ..textures()
                };
                assert!(
                    vertices(&view_primitives(&game, &view, art, reduced))
                        > vertices(&view_primitives(&game, &unphased, art, reduced)),
                    "reduced {reduced}, rejected {rejected}: tick {} draws the ring",
                    a.tick
                );
            }
            let ring = enemies::phase_ring(sy, phase.hold_left_ticks, reduced);
            assert!(ring.x > sy * 0.4 && ring.y > 0.0, "a ring around the boss");
            if painting {
                // W2 review: the ring the battlefield paints is the one the
                // pin checks. Since W5-01 it is a cue: the first ring of the
                // cue pass's fill mesh (20 segments, an outer and an inner
                // vertex each), AMBER on its INK halo, for rejected art too.
                let art = ArtTextures {
                    enemy_art_rejected: true,
                    ..textures()
                };
                let shapes = painted(|p| {
                    let options = RenderOptions {
                        reduced,
                        ..options(art)
                    };
                    battlefield(p, arena(), &view, a, [10.0, 5.0], options);
                });
                let drawn: Vec<_> = meshes(&shapes)
                    .into_iter()
                    .filter(|mesh| {
                        mesh.texture_id == TextureId::default()
                            && mesh.vertices.len() >= 40
                            && mesh.vertices[..40]
                                .iter()
                                .all(|v| v.color == super::super::AMBER)
                    })
                    .map(|mesh| {
                        let bounds = mesh.vertices[..40].iter().fold(Rect::NOTHING, |r, v| {
                            r.union(Rect::from_min_max(v.pos, v.pos))
                        });
                        (bounds.size() - vec2(1.0, 1.0) * enemies::PHASE_RING_WIDTH) * 0.5
                    })
                    .collect();
                assert_eq!(drawn.len(), 1, "one phase ring mesh");
                assert!(
                    (drawn[0] - ring).length() < 0.01,
                    "reduced {reduced}: tick {}: {:?} vs {ring:?}",
                    a.tick,
                    drawn[0]
                );
                painted_rings[usize::from(reduced)].push(drawn[0]);
            }
            rings[usize::from(reduced)].push(ring);
            frames[usize::from(reduced)] += 1;
        }
    }
    assert_eq!(frames, [90, 90], "the whole hold, normal and reduced");
    let [normal, reduced] = &rings;
    assert!(
        reduced.iter().all(|r| *r == reduced[0]),
        "the reduced ring is static: {reduced:?}"
    );
    assert!(
        normal.iter().any(|r| *r != normal[0]),
        "the normal ring breathes"
    );
    // On screen too, over the first, middle and last frames of the hold.
    let [normal, reduced] = &painted_rings;
    assert_eq!((normal.len(), reduced.len()), (3, 3));
    assert!(
        reduced.iter().all(|r| *r == reduced[0]),
        "the painted reduced ring is static: {reduced:?}"
    );
    assert!(
        normal.iter().any(|r| *r != normal[0]),
        "the painted normal ring breathes: {normal:?}"
    );
    // The hold is over: no phase, no label, no ring.
    game.tick(Input::default());
    let a = game.attempt().unwrap();
    assert_eq!(a.tick, hold_until);
    for reduced in [false, true] {
        let view = BattleViewModel::build(
            a,
            SceneAnimation::sample(a, reduced),
            SceneBlend::NONE,
            reduced,
        );
        assert_eq!(view.boss_phase, None, "reduced {reduced}: the hold ended");
        assert!(!texts(&view, false, reduced).iter().any(|t| t == label));
    }
}

/// W2 review: a spit at close range still draws its aim line, the visual
/// twin of the ranged windup. `spit_range` lets a spit fly from just past
/// the wall's face, closer than the line's lead, where the dashes used to
/// start beyond the target and none was drawn. A short line draws at least
/// one dash, or its whole second half when that is shorter, and its last
/// dash ends on the target; a long line keeps its lead.
#[test]
fn aim_line_at_close_range_still_draws() {
    let sy = 40.0;
    let from = pos2(300.0, 200.0);
    for fraction in [0.02, 0.1, 0.2, 0.25, 0.3, 0.45] {
        let length = sy * fraction;
        let to = from + vec2(-length * 0.8, length * 0.6);
        let dashes = enemies::aim_dashes(from, to, sy);
        assert!(!dashes.is_empty(), "{fraction} lanes: no aim line");
        let drawn: f32 = dashes.iter().map(|[a, b]| a.distance(*b)).sum();
        assert!(
            drawn >= (length * 0.5).min(sy * enemies::AIM_DASH) - 1e-3,
            "{fraction} lanes: {drawn} of {length} drawn"
        );
        let [first, ..] = dashes[0];
        assert!(first.distance(from) > 0.0, "{fraction}: clear of the body");
        let last = dashes.last().unwrap()[1];
        assert!(
            last.distance(to) < 1e-3,
            "{fraction}: ends {} short of the target",
            last.distance(to)
        );
    }
    // Past twice the lead the line is unchanged: its first dash starts the
    // lead clear of the body.
    for fraction in [0.5, 1.0, 4.0] {
        let to = from + vec2(-sy * fraction, 0.0);
        let first = enemies::aim_dashes(from, to, sy)[0][0];
        assert!(
            (first.distance(from) - sy * enemies::AIM_LEAD).abs() < 1e-3,
            "{fraction} lanes: lead {}",
            first.distance(from)
        );
    }
}

/// W2 review: an aim line ends on its target, as `aim_dashes` documents.
/// The loop used to stop once the next dash would start past the target, so
/// a line whose length fell in the trailing gap ended up to one gap (0.1
/// lane heights) short. Over a sweep of lengths the last dash now ends on
/// the target, the dashes keep their order and gaps, and no dash grows past
/// a dash plus a gap.
#[test]
fn aim_line_ends_on_its_target() {
    let sy = 40.0;
    let from = pos2(300.0, 200.0);
    for step in 1..=600 {
        let length = sy * step as f32 * 0.01;
        let to = from + vec2(-length * 0.6, -length * 0.8);
        let dashes = enemies::aim_dashes(from, to, sy);
        let last = dashes.last().expect("a dash")[1];
        assert!(
            last.distance(to) < 1e-3,
            "{length} points: ends {} short of the target",
            last.distance(to)
        );
        for pair in dashes.windows(2) {
            let gap = pair[0][1].distance(pair[1][0]);
            assert!(
                (gap - sy * enemies::AIM_GAP).abs() < 1e-3,
                "{length} points: gap {gap}"
            );
        }
        for [a, b] in &dashes {
            assert!(
                a.distance(*b) <= sy * (enemies::AIM_DASH + enemies::AIM_GAP) + 1e-3,
                "{length} points: dash {}",
                a.distance(*b)
            );
        }
    }
}

/// W2 review (W2-37's BossPhase twin): a holding boss in the top lane has
/// its name callout and its "PHASE n - NAME" label both pulled inside the
/// clip, and they used to land on the same row and overprint. The pair now
/// moves as one block: the label stays over the name, neither overlaps the
/// other and both stay whole inside the clip, on the batched path and the
/// rejected-art figures. A mid-field boss keeps both where they were.
#[test]
fn boss_phase_label_and_name_never_overprint_in_the_top_lane() {
    use crate::battle_view::BossPhaseView;
    use horde_core::{balance::BalanceSettings, content::EnemyKind};
    let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1050.0, 700.0) / 1.35);
    let clip = Rect::from_min_max(
        canvas.min + vec2(24.0, 124.0),
        canvas.max - vec2(24.0, 84.0),
    );
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(BalanceSettings {
            start_lanes: 17,
            max_lanes: 17,
            start_width: 30,
            max_width: 30,
            ..Default::default()
        })
        .unwrap();
    game.deploy(0, horde_core::content::Mode::Personal).unwrap();
    for lane in [0, 8] {
        let mut a = game.attempt_slot_mut().take().unwrap();
        a.enemies.clear();
        game.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
        let boss = a.enemies.last_mut().expect("a boss");
        boss.x = a.width_yards * 0.5;
        let id = boss.id;
        *game.attempt_slot_mut() = Some(a);
        let a = game.attempt().unwrap();
        let mut view =
            BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
        view.boss_phase = Some(BossPhaseView {
            boss_id: id,
            transition: 1,
            name: "Breach".into(),
            hold_left_ticks: 45,
        });
        let phase_text = view.boss_phase.as_ref().unwrap().label();
        let name_text = view.enemies[0].label.expect("a boss name");
        let arena = ArenaProjection::with_geometry(clip, view.lanes, view.width);
        let sy = arena.lane_height();
        let [x, y] = view.enemies[0].drawn();
        let at = arena.world(x, y);
        for rejected in [false, true] {
            let textures = ArtTextures {
                enemy_art_rejected: rejected,
                ..textures()
            };
            let ctx = egui::Context::default();
            crate::settings::install_font(&ctx);
            crate::settings::configure(&ctx, 1.35);
            crate::visual_assets::publish_source_flash_masks(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(canvas),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                enemies::paint(
                    &ui.painter().with_clip_rect(clip),
                    arena,
                    &view,
                    textures,
                    false,
                    false,
                    rig_shadow(),
                );
            });
            output.textures_delta.clear();
            let rect_of = |wanted: &str| -> Rect {
                let rects: Vec<Rect> = output
                    .shapes
                    .iter()
                    .filter_map(|clipped| match &clipped.shape {
                        egui::Shape::Text(text) if text.galley.text() == wanted => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(rects.len(), 1, "rejected {rejected}: one {wanted:?}");
                rects[0]
            };
            let (name, phase) = (rect_of(name_text), rect_of(&phase_text));
            let label = format!("lane {lane}, rejected {rejected}: {name:?} {phase:?}");
            assert!(
                clip.contains_rect(name) && clip.contains_rect(phase),
                "{label}"
            );
            assert!(!name.intersects(phase), "{label}: they overprint");
            assert!(
                phase.bottom() <= name.top() + 0.01,
                "{label}: over the name"
            );
            if lane == 8 {
                // Mid-field: the name centred `0.7` lanes over the feet and
                // the label's bottom `PHASE_LABEL_GAP` over that, as before.
                assert!(
                    (name.center().y - (at.y - sy * 0.7)).abs() < 0.01,
                    "{label}"
                );
                assert!(
                    (phase.bottom() - (at.y - sy * 0.7 - enemies::PHASE_LABEL_GAP)).abs() < 0.01,
                    "{label}"
                );
            }
        }
    }
}

/// CF-2-05: a holding boss's "PHASE n - NAME" label, its name or EXPOSED
/// callout and a CRIT callout drew as bare amber text, so over the hostiles
/// behind a boss "PHASE" crossed a walker's head. Every hostile callout now
/// paints an INK backing, padded `CALLOUT_PAD` round its text, before the
/// text it backs, on the batched path and the rejected-art figures, normal
/// and reduced, in the top lane and mid-field.
#[test]
fn hostile_callouts_paint_an_ink_backing_under_their_text() {
    use crate::battle_view::{BossPhaseView, HealthView};
    use horde_core::{balance::BalanceSettings, content::EnemyKind};
    let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1050.0, 700.0) / 1.35);
    let clip = Rect::from_min_max(
        canvas.min + vec2(24.0, 124.0),
        canvas.max - vec2(24.0, 84.0),
    );
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(BalanceSettings {
            start_lanes: 17,
            max_lanes: 17,
            start_width: 30,
            max_width: 30,
            ..Default::default()
        })
        .unwrap();
    game.deploy(0, horde_core::content::Mode::Personal).unwrap();
    for lane in [0, 8] {
        let mut a = game.attempt_slot_mut().take().unwrap();
        a.enemies.clear();
        game.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
        let boss = a.enemies.last_mut().expect("a boss");
        boss.x = a.width_yards * 0.5;
        let id = boss.id;
        // A second boss further on, critically wounded, so its CRIT
        // callout and its own name are checked too.
        let mut other = a.enemies[0].clone();
        (other.id, other.x) = (a.next_id, a.width_yards * 0.8);
        a.next_id += 1;
        a.enemies.push(other);
        *game.attempt_slot_mut() = Some(a);
        let a = game.attempt().unwrap();
        for reduced in [false, true] {
            let mut view = BattleViewModel::build(
                a,
                SceneAnimation::sample(a, reduced),
                SceneBlend::NONE,
                reduced,
            );
            view.boss_phase = Some(BossPhaseView {
                boss_id: id,
                transition: 1,
                name: "Breach".into(),
                hold_left_ticks: 45,
            });
            view.enemies[1].health = HealthView::Critical;
            let arena = ArenaProjection::with_geometry(clip, view.lanes, view.width);
            for rejected in [false, true] {
                let textures = ArtTextures {
                    enemy_art_rejected: rejected,
                    ..textures()
                };
                let ctx = egui::Context::default();
                crate::settings::install_font(&ctx);
                crate::settings::configure(&ctx, 1.35);
                crate::visual_assets::publish_source_flash_masks(&ctx);
                let input = egui::RawInput {
                    screen_rect: Some(canvas),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    enemies::paint(
                        &ui.painter().with_clip_rect(clip),
                        arena,
                        &view,
                        textures,
                        false,
                        reduced,
                        rig_shadow(),
                    );
                });
                output.textures_delta.clear();
                let backings: Vec<(usize, Rect)> = output
                    .shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, clipped)| match &clipped.shape {
                        egui::Shape::Rect(rect) if rect.fill == enemies::CALLOUT_BACKING => {
                            Some((i, rect.rect))
                        }
                        _ => None,
                    })
                    .collect();
                let mut texts = 0;
                for (i, clipped) in output.shapes.iter().enumerate() {
                    let egui::Shape::Text(text) = &clipped.shape else {
                        continue;
                    };
                    texts += 1;
                    let drawn = text.galley.rect.translate(text.pos.to_vec2());
                    let padded = drawn.expand(enemies::CALLOUT_PAD);
                    let label = format!(
                        "lane {lane}, reduced {reduced}, rejected {rejected}: {:?} at {drawn:?}",
                        text.galley.text()
                    );
                    assert!(
                        backings.iter().any(|&(at, rect)| {
                            at < i
                                && (rect.min - padded.min).length() < 0.01
                                && (rect.max - padded.max).length() < 0.01
                        }),
                        "{label}: no INK backing {padded:?} under it, backings {backings:?}"
                    );
                }
                // The phase label, two names (or EXPOSED) and the CRIT.
                assert_eq!(
                    texts, 4,
                    "lane {lane}, reduced {reduced}, rejected {rejected}"
                );
            }
        }
    }
}

/// A peak-fixture view staged with every cue the field paints: a contact
/// lane, a windup with its aim line, a stun, a health bar, CRIT and name
/// callouts, a holding boss's phase, the survivor's hit ring and each event
/// cue (boss warning, reinforcements, spawn, boss, contact damage, coins).
fn every_cue_view() -> (Game, BattleViewModel) {
    use crate::battle_view::{BossPhaseView, ContactView, FxView, HealthView};
    use horde_core::sim::EventKind;
    let game = horde_core::sim::peak_render_fixture().expect("peak fixture");
    let a = game.attempt().unwrap();
    let mut view =
        BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false);
    assert!(view.enemies.len() >= 4, "the peak fixture has hostiles");
    let target = view.defender.position;
    let e = &mut view.enemies[0];
    e.contact = Some(ContactView {
        ready: 0.5,
        recovering: false,
    });
    e.windup = Some(0.5);
    e.aim_target = Some(target);
    e.stunned = true;
    e.health = HealthView::Bar {
        fraction: 0.4,
        boss: false,
    };
    view.enemies[1].contact = Some(ContactView {
        ready: 0.2,
        recovering: true,
    });
    view.enemies[1].health = HealthView::Critical;
    view.enemies[2].label = Some("SIEGE BRUTE");
    view.enemies[3].label = Some("WARDEN");
    view.boss_phase = Some(BossPhaseView {
        boss_id: view.enemies[3].id,
        transition: 1,
        name: "BREACH".into(),
        hold_left_ticks: 30,
    });
    view.defender.pose.hit = 0.6;
    let event = |kind| FxView {
        kind,
        age: 0.05,
        from: [2.0, 3.5],
        to: [12.0, 3.5],
        value: 150,
        lane: Some(3),
        actor_id: None,
        target_id: None,
        tick: 0,
        seq: 0,
        volley: Some(crate::battle_view::Volley::SINGLE),
        source: Some(crate::art::FxSource::Turret),
    };
    view.fx = [
        EventKind::BossWarning,
        EventKind::Reinforcements,
        EventKind::Spawn,
        EventKind::Boss,
        EventKind::Damage,
        EventKind::Kill,
    ]
    .map(event)
    .to_vec();
    (game, view)
}

/// `color` without its alpha: the colour the cue shows at full strength.
fn opaque(color: egui::Color32) -> [u8; 3] {
    let [r, g, b, _] = color.to_srgba_unmultiplied();
    [r, g, b]
}

/// G7.2 (W5-01): every cue primitive the field collects, outlines on, is
/// painted over an INK backing it contrasts with by at least 3:1 (WCAG
/// 1.4.11), in normal and reduced effects. The backing is INK at the halo
/// alpha (230; label plates 190), fading only with its cue, and the halo
/// geometry covers the fill geometry.
#[test]
fn every_cue_primitive_has_an_ink_backing_with_contrast_at_least_3() {
    use super::super::{INK, lighting};
    use cue::CueKind;
    let (game, view) = every_cue_view();
    let a = game.attempt().unwrap();
    let projection = ArenaProjection::for_attempt(arena(), a);
    let ink = lighting::relative_luminance([INK.r(), INK.g(), INK.b()]);
    for reduced in [false, true] {
        let options = RenderOptions {
            outlines: true,
            reduced,
            ..options(textures())
        };
        let fx_options = fx::FxOptions {
            numbers: true,
            reduced,
            blood: true,
            textures: textures(),
            occluder: None,
        };
        let mut batch = cue::CueBatch::new(view.enemies.len());
        collect_cues(&mut batch, projection, &view, &options, &fx_options);
        let kinds: std::collections::HashSet<CueKind> =
            batch.records.iter().map(|record| record.kind).collect();
        assert_eq!(kinds.len(), 7, "every cue kind is staged: {kinds:?}");
        for record in &batch.records {
            let strength = f32::from(record.color.a()) / 255.0;
            assert!(strength > 0.0, "{record:?} is visible");
            let backing = opaque(record.backing);
            for (got, want) in backing.into_iter().zip([INK.r(), INK.g(), INK.b()]) {
                assert!(got.abs_diff(want) <= 2, "{record:?}: the backing is INK");
            }
            let floor = if record.kind == CueKind::Label {
                190.0
            } else {
                230.0
            };
            assert!(
                f32::from(record.backing.a()) >= floor * strength - 2.0,
                "{record:?}: backing alpha under {floor} x {strength}"
            );
            let ratio = lighting::contrast(lighting::relative_luminance(opaque(record.color)), ink);
            assert!(ratio >= 3.0, "{record:?}: contrast {ratio:.2} against INK");
        }
        // Three meshes however many cues: halo, fill, marker; then text.
        let shapes = painted(|p| {
            let mut batch = cue::CueBatch::new(view.enemies.len());
            collect_cues(&mut batch, projection, &view, &options, &fx_options);
            batch.paint(p);
        });
        let geometry: Vec<&egui::Mesh> = shapes
            .iter()
            .take_while(|shape| matches!(shape, egui::Shape::Mesh(_)))
            .filter_map(|shape| match shape {
                egui::Shape::Mesh(mesh) => Some(&**mesh),
                _ => None,
            })
            .collect();
        assert_eq!(geometry.len(), 3, "halo, fill and marker meshes");
        assert_eq!(meshes(&shapes).len(), 3, "no cue mesh after the text");
        let bounds = |mesh: &egui::Mesh| {
            mesh.vertices.iter().fold(Rect::NOTHING, |r, v| {
                r.union(Rect::from_min_max(v.pos, v.pos))
            })
        };
        let halo = bounds(geometry[0]);
        for fill in &geometry[1..] {
            assert!(
                halo.contains_rect(bounds(fill)),
                "the halo mesh covers the cue geometry"
            );
        }
    }
}

/// A cue painted without its helper would dodge the guard above: no hostile
/// or event painter of the battlefield draws text itself, so every field
/// label goes through `cue` and its INK plate.
#[test]
fn only_the_cue_pass_paints_field_labels() {
    for (name, source) in [
        ("enemies.rs", include_str!("enemies.rs")),
        ("fx.rs", include_str!("fx.rs")),
    ] {
        for call in ["p.text(", "p.galley(", "layout_no_wrap("] {
            assert!(!source.contains(call), "{name} paints text with {call}");
        }
    }
}

/// W5-01 pass order: world, grade, vignette, then cues. On a night stage
/// the grade over-rect and the vignette mesh are painted across the canvas
/// before any cue, so neither dims a warning; a neutral stage paints no
/// grade rect, and reduced effects keep the same order.
#[test]
fn cue_pass_paints_after_the_grade_and_the_vignette() {
    use super::super::lighting::{self, Lighting, SceneLook};
    let (game, view) = every_cue_view();
    let a = game.attempt().unwrap();
    let canvas = arena().expand(24.0);
    let night = lighting::grade(Lighting::Night);
    let night_fill = egui::Color32::from_rgba_unmultiplied(
        night.tint[0],
        night.tint[1],
        night.tint[2],
        (night.alpha * 255.0).round() as u8,
    );
    let pass = |scene: SceneLook, reduced: bool| {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter().with_clip_rect(arena());
            let options = RenderOptions {
                scene,
                reduced,
                canvas,
                ..options(textures())
            };
            battlefield(&painter, arena(), &view, a, [10.0, 5.0], options);
        });
        output.textures_delta.clear();
        output.shapes
    };
    let night_scene = SceneLook {
        lighting: Lighting::Night,
        ..SceneLook::neutral(0)
    };
    for reduced in [false, true] {
        let shapes = pass(night_scene, reduced);
        let grade = shapes
            .iter()
            .position(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == night_fill))
            .expect("the night grade rect");
        assert_eq!(
            shapes[grade].clip_rect, canvas,
            "the grade covers the canvas"
        );
        match &shapes[grade].shape {
            egui::Shape::Rect(rect) => assert_eq!(rect.rect, canvas),
            _ => unreachable!(),
        }
        let vignette = &shapes[grade + 1];
        assert_eq!(vignette.clip_rect, canvas, "the vignette follows the grade");
        match &vignette.shape {
            egui::Shape::Mesh(mesh) => {
                assert_eq!(mesh.vertices.len(), 24, "the edge-shade rings");
                assert!(mesh.vertices.iter().all(|v| opaque(v.color) == [0, 0, 0]));
            }
            other => panic!("the vignette is one mesh: {other:?}"),
        }
        let texts: Vec<(usize, String)> = shapes
            .iter()
            .enumerate()
            .filter_map(|(n, s)| match &s.shape {
                egui::Shape::Text(text) => Some((n, text.galley.text().to_owned())),
                _ => None,
            })
            .collect();
        for cue in [
            "BOSS APPROACHING",
            "REINFORCEMENTS",
            "INCOMING",
            "SIEGE BRUTE",
            "+150c",
        ] {
            assert!(
                texts.iter().any(|(n, text)| text == cue && *n > grade + 1),
                "reduced {reduced}: {cue} paints after the vignette: {texts:?}"
            );
        }
        // The only text under the grade is the structures' own readouts (a
        // trap's charges), which are world dressing, not warnings.
        assert!(
            texts
                .iter()
                .filter(|(n, _)| *n < grade)
                .all(|(_, text)| text.parse::<u32>().is_ok()),
            "reduced {reduced}: no cue label is painted under the grade: {texts:?}"
        );
        assert!(
            shapes[grade + 2..].iter().all(|s| s.clip_rect == arena()),
            "the cue pass is clipped to the arena"
        );
    }
    let neutral = pass(SceneLook::neutral(0), false);
    assert!(
        !neutral
            .iter()
            .any(|s| s.clip_rect == canvas && matches!(s.shape, egui::Shape::Rect(_))),
        "a neutral stage paints no grade rect"
    );
    assert_eq!(
        neutral.iter().filter(|s| s.clip_rect == canvas).count(),
        1,
        "a neutral stage paints the vignette only on the canvas"
    );
}

/// G7.3: the sky flash is one over-rect on the canvas, straight after the
/// grade and under the vignette and the whole cue pass, so it cannot mask a
/// cue; and it is not painted at all near an attack cue, while paused or in
/// reduced effects.
#[test]
fn sky_flash_paints_under_cue_pass() {
    use super::super::lighting::{self, Lighting, SceneLook, SkyClock, SkyQuiet};
    let (game, view) = every_cue_view();
    let canvas = arena().expand(24.0);
    let storm_scene = SceneLook {
        lighting: Lighting::Storm,
        storm: true,
        seed: 7,
        ..SceneLook::neutral(0)
    };
    let peak = (0..200_000u64)
        .find(|&ms| lighting::sky_flash(7, ms, SkyQuiet::default(), false, false, true) >= 1.0)
        .expect("a pulse peak");
    let sky = SkyClock {
        wall_ms: peak as f64,
        speed: 1,
    };
    let rgba = |rgb: [u8; 3], alpha: f32| {
        egui::Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], (alpha * 255.0).round() as u8)
    };
    let storm = lighting::grade(Lighting::Storm);
    let grade_fill = rgba(storm.tint, storm.alpha);
    let flash_fill = rgba(lighting::SKY_RGB, lighting::SKY_PEAK_ALPHA);
    let pass = |a: &horde_core::sim::Attempt, reduced: bool| {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter().with_clip_rect(arena());
            let options = RenderOptions {
                scene: storm_scene,
                sky,
                reduced,
                canvas,
                ..options(textures())
            };
            battlefield(&painter, arena(), &view, a, [10.0, 5.0], options);
        });
        output.textures_delta.clear();
        output.shapes
    };
    let fills = |shapes: &[egui::epaint::ClippedShape], fill: egui::Color32| -> Vec<usize> {
        shapes
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == fill && r.rect == canvas))
            .map(|(n, _)| n)
            .collect()
    };
    // A calm moment of the same field: no cue event in the quiet windows.
    let mut calm = game.attempt().unwrap().clone();
    calm.events.clear();
    calm.paused = false;
    let shapes = pass(&calm, false);
    let grade = fills(&shapes, grade_fill);
    let flash = fills(&shapes, flash_fill);
    assert_eq!(grade.len(), 1, "one storm grade rect");
    assert_eq!(flash, [grade[0] + 1], "the flash follows the grade");
    assert_eq!(shapes[flash[0]].clip_rect, canvas);
    let vignette = &shapes[flash[0] + 1];
    assert!(
        vignette.clip_rect == canvas && matches!(vignette.shape, egui::Shape::Mesh(_)),
        "the vignette follows the flash"
    );
    let texts: Vec<(usize, String)> = shapes
        .iter()
        .enumerate()
        .filter_map(|(n, s)| match &s.shape {
            egui::Shape::Text(text) => Some((n, text.galley.text().to_owned())),
            _ => None,
        })
        .collect();
    for cue in ["BOSS APPROACHING", "REINFORCEMENTS", "INCOMING", "+150c"] {
        assert!(
            texts
                .iter()
                .any(|(n, text)| text == cue && *n > flash[0] + 1),
            "{cue} paints over the flash: {texts:?}"
        );
    }
    assert!(
        shapes[flash[0] + 2..]
            .iter()
            .all(|s| s.clip_rect == arena()),
        "everything after the vignette is the arena-clipped grid and cue pass"
    );
    // Reduced effects and a paused battle: the static storm grade only.
    assert!(fills(&pass(&calm, true), flash_fill).is_empty(), "reduced");
    assert_eq!(fills(&pass(&calm, true), grade_fill).len(), 1, "reduced");
    let mut paused = calm.clone();
    paused.paused = true;
    assert!(
        fills(&pass(&paused, false), flash_fill).is_empty(),
        "paused"
    );
    // A windup this tick: the flash stays out of the cue's way.
    let mut windup = calm.clone();
    let mut event: horde_core::sim::Event =
        serde_json::from_str(r#"{"kind":"Windup","from":[0,0],"to":[0,0],"value":0,"tick":0}"#)
            .unwrap();
    event.tick = windup.tick;
    windup.events.push_back(event);
    assert!(
        fills(&pass(&windup, false), flash_fill).is_empty(),
        "windup"
    );
    assert_eq!(fills(&pass(&windup, false), grade_fill).len(), 1);
}

/// Cue bars and labels scale with the zoom (design-vfx-lighting, cue
/// helpers): bar height clamp(0.05 sy, 3, 6) pt, font clamp(0.16 sy, 10, 14).
#[test]
fn cue_bars_and_labels_scale_with_zoom_inside_their_clamps() {
    assert_eq!(cue::bar_height(18.3), 3.0);
    assert_eq!(cue::bar_height(100.0), 5.0);
    assert_eq!(cue::bar_height(230.0), 6.0);
    assert_eq!(cue::font(18.3), 10.0);
    assert_eq!(cue::font(75.0), 12.0);
    assert_eq!(cue::font(230.0), 14.0);
    for sy in (10..400).map(|n| n as f32) {
        assert!((3.0..=6.0).contains(&cue::bar_height(sy)));
        assert!((10.0..=14.0).contains(&cue::font(sy)));
        assert_eq!((cue::font(sy) * 2.0).fract(), 0.0, "half-point steps");
    }
}
