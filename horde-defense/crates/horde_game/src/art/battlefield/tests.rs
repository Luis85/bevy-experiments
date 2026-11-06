//! Batched battlefield tests: primitive counts, layer order and the pure
//! geometry the batched and legacy paths share.
use super::*;
use crate::{
    animation::SceneAnimation,
    battle_view::{BattleViewModel, SceneBlend},
    visual_assets::{ArtTextures, BattleAtlas, WalkSheet},
};
use bevy_egui::egui::{ClippedPrimitive, TextureId, pos2, vec2};
use horde_core::sim::Game;

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
    let a = game.attempt().expect("fixture battle");
    let motion = SceneAnimation::sample(a, false);
    let view = BattleViewModel::build(a, motion, SceneBlend::NONE, false);
    let ctx = egui::Context::default();
    let rect = arena();
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
        assert!(enemies::paint(
            p,
            arena_projection,
            &view,
            textures,
            true,
            false,
            rig_shadow()
        ));
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
