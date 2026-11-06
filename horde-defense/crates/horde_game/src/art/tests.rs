use super::backdrop::{
    dressing_unit, inset_tree_foot, perimeter_foot, peripheral_details, peripheral_kind,
    peripheral_strips,
};
use super::figures::{FALLBACK_LOOKS, FallbackLook, fallback_look};
use super::*;
use crate::visual_assets::{self, ArtTextures, Prop};
use bevy_egui::egui::{self, Color32 as C, Pos2, Rect, pos2, vec2};
use horde_core::content::EnemyKind;

#[cfg(test)]
mod fallback_look_tests {
    use super::*;

    /// The coat colours of the former per-kind `match` in `figure`, frozen.
    const LEGACY_COATS: [(EnemyKind, [u8; 3]); 5] = [
        (EnemyKind::Shambler, [126, 144, 104]),
        (EnemyKind::Runner, [193, 151, 102]),
        (EnemyKind::Brute, [123, 108, 89]),
        (EnemyKind::Armored, [133, 154, 158]),
        (EnemyKind::Demolisher, [197, 150, 61]),
    ];

    /// The former `figure` logic: boss coat override, plate on Armored or any
    /// boss, charge pack on Demolisher.
    fn legacy_look(kind: EnemyKind, [r, g, b]: [u8; 3], boss: bool) -> FallbackLook {
        FallbackLook {
            coat: if boss {
                C::from_rgb(135, 73, 53)
            } else {
                C::from_rgb(r, g, b)
            },
            armor_plate: kind == EnemyKind::Armored || boss,
            charge_pack: kind == EnemyKind::Demolisher,
        }
    }

    #[test]
    fn table_has_exactly_one_row_per_enemy_kind() {
        assert_eq!(FALLBACK_LOOKS.len(), EnemyKind::ALL.len());
        for kind in EnemyKind::ALL {
            let rows = FALLBACK_LOOKS.iter().filter(|(k, _)| *k == kind).count();
            assert_eq!(rows, 1, "{kind:?} needs exactly one fallback row");
        }
    }

    #[test]
    fn table_reproduces_the_legacy_colours_and_overlays() {
        for (kind, coat) in LEGACY_COATS {
            for boss in [false, true] {
                assert_eq!(
                    fallback_look(kind, boss),
                    legacy_look(kind, coat, boss),
                    "{kind:?} boss={boss}"
                );
            }
        }
    }
}

#[cfg(test)]
mod terrain_tests {
    use super::*;

    #[test]
    fn projection_covers_every_dynamic_lane_and_preserves_world_cursor_and_radius_geometry() {
        for lanes in [11u8, 13, 15, 17] {
            for size in [vec2(1392.0, 692.0), vec2(729.0, 310.0)] {
                let rect = Rect::from_min_size(pos2(24.0, 124.0), size);
                let projection = ArenaProjection::new(rect, lanes);
                assert_eq!(
                    projection.world(0.5, f32::from(lanes) * 0.5).y,
                    rect.center().y
                );
                assert!(projection.lane_height() >= 18.0);
                for lane in 0..lanes {
                    for x in [0.5, 4.2, 20.0, 29.5] {
                        let original = [x, f32::from(lane) + 0.5];
                        let point = projection.world(original[0], original[1]);
                        assert!(rect.contains(point));
                        let restored = projection.inverse(point);
                        assert!(
                            (restored[0] - original[0]).abs() < 0.001
                                && (restored[1] - original[1]).abs() < 0.001
                        );
                    }
                }
                let radius = projection.radius(2.0);
                assert!((radius.y - projection.lane_height() * 2.0).abs() < 0.001);
            }
        }
    }
    #[test]
    fn every_drawn_tree_fits_its_full_source_bounds_inside_the_peripheral_strip() {
        for strip in [
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1050.0, 124.0)),
            Rect::from_min_max(pos2(0.0, 616.0), pos2(1050.0, 700.0)),
            Rect::from_min_max(pos2(0.0, 124.0), pos2(24.0, 616.0)),
        ] {
            for prop in [Prop::TreeBroadleaf, Prop::TreePine] {
                for foot in [
                    strip.left_bottom(),
                    strip.right_bottom(),
                    strip.center_bottom() + vec2(0.0, 30.0),
                ] {
                    let size = vec2(90.0, 75.0);
                    match inset_tree_foot(prop, foot, size, strip) {
                        Some(foot) => assert!(
                            strip
                                .shrink(1.9)
                                .contains_rect(visual_assets::prop_ground_rect(prop, foot, size))
                        ),
                        None => {
                            let rect = visual_assets::prop_ground_rect(prop, foot, size);
                            assert!(
                                rect.width() > strip.shrink(2.0).width()
                                    || rect.height() > strip.shrink(2.0).height()
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn dressing_tracks_all_actual_clearing_edges_and_keeps_complete_sprites_outside_legal_space() {
        for size in [
            vec2(1440.0, 856.0),
            vec2(1050.0 / 1.35, 700.0 / 1.35 - 44.0),
        ] {
            let canvas = Rect::from_min_size(Pos2::ZERO, size);
            let maximum = Rect::from_min_max(pos2(24.0, 124.0), canvas.max - vec2(24.0, 84.0));
            for (lanes, width) in [(3, 12.0), (7, 16.0), (17, 30.0)] {
                // The zoomed field and the capped dressing unit the main
                // battle draws (ui::battle::BattleLayout).
                let arena = fit_field(maximum, maximum, lanes, width).rect;
                let unit = dressing_unit(
                    arena.height() / f32::from(lanes),
                    maximum.height() / camera::REFERENCE_LANES,
                );
                let gap = unit * 0.08;
                let mut visible = [0; 4];
                for (edge, strip) in peripheral_strips(canvas, arena).into_iter().enumerate() {
                    for prop in [Prop::TreeBroadleaf, Prop::TreePine, Prop::Bush, Prop::Rock] {
                        for along in [0.0, 0.2, 0.5, 0.8, 1.0] {
                            let size = vec2(unit * 1.8, unit * 1.6);
                            if let Some(foot) =
                                perimeter_foot(prop, size, arena, strip, edge, along, gap)
                            {
                                let drawn = visual_assets::prop_ground_rect(prop, foot, size);
                                assert!(canvas.contains_rect(drawn));
                                assert!(strip.shrink(1.9).contains_rect(drawn));
                                assert!(
                                    !arena.intersects(drawn),
                                    "Dressing cannot conceal legal lanes or cells"
                                );
                                let distance = match edge {
                                    0 => arena.top() - drawn.bottom(),
                                    1 => drawn.top() - arena.bottom(),
                                    2 => arena.left() - drawn.right(),
                                    _ => drawn.left() - arena.right(),
                                };
                                assert!(
                                    distance >= 1.9 && distance <= gap.max(2.0) + 0.01,
                                    "{lanes}x{width} edge{edge}: sprite gap{distance} must follow the clearing, not the canvas"
                                );
                                visible[edge] += 1;
                            }
                        }
                    }
                }
                assert!(visible[0] > 0 && visible[1] > 0);
                // A width-limited field spans the arena, leaving only the
                // side insets; a narrower (height-limited) one shows its
                // forest on all four edges.
                if arena.width() < maximum.width() - 1.0 {
                    assert!(
                        visible.iter().all(|count| *count > 0),
                        "{lanes}x{width}: a narrower field needs all four perimeter cues"
                    );
                }
            }
        }
    }

    #[test]
    fn continuous_ground_preserves_the_shared_line_without_a_combat_grid() {
        let ctx = egui::Context::default();
        let r = Rect::from_min_size(pos2(20., 50.), vec2(600., 220.));
        let mut output = ctx.run_ui(Default::default(), |ui| {
            battlefield::terrain::paint(
                ui.painter(),
                ArenaProjection::new(r, 11),
                0,
                ArtTextures {
                    ground: Some(egui::TextureId::User(1)),
                    ..Default::default()
                },
            );
        });
        let lines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::LineSegment { points, .. } = &shape.shape {
                    Some(*points)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(lines, vec![[world(r, 1.5, 0.), world(r, 1.5, 11.)]]);
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == egui::TextureId::User(1))
            }),
            "Original pixel ground must be drawn"
        );
        for shape in &output.shapes {
            if let egui::Shape::Mesh(mesh) = &shape.shape
                && mesh.texture_id == egui::TextureId::User(1)
            {
                assert!(
                    mesh.vertices.iter().all(|vertex| vertex.color.a() <= 192)
                        && mesh.vertices.iter().any(|vertex| vertex.color.a() >= 160)
                        && mesh.vertices.iter().any(|vertex| vertex.color.a() == 0),
                    "Ground material stays visible while patch edges blend continuously"
                );
                assert!(
                    mesh.vertices[0].uv.x < mesh.vertices[3].uv.x
                        && mesh.vertices[0].uv.y < mesh.vertices[12].uv.y,
                    "Ground motifs must never be reflected into a carpet"
                );
            }
        }
        output.textures_delta.clear();
    }

    #[test]
    fn forest_and_outpost_dressing_are_clipped_outside_all_combat_lanes() {
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(pos2(0., 0.), vec2(1000., 700.));
        let arena = canvas.shrink2(vec2(12., 60.));
        let mut output = ctx.run_ui(Default::default(), |ui| {
            world_backdrop(
                ui.painter(),
                canvas,
                ArenaProjection::new(arena, 11),
                0,
                ArtTextures {
                    props: Some(egui::TextureId::User(2)),
                    ..Default::default()
                },
                arena.height() / camera::REFERENCE_LANES,
            );
        });
        let props: Vec<_> = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == egui::TextureId::User(2))
            })
            .collect();
        assert!(!props.is_empty());
        assert!(
            props.len() <= 4 * (44 + 5),
            "Peripheral dressing stays bounded"
        );
        let first_strip = props[0].clip_rect;
        let mut previous_foot = f32::NEG_INFINITY;
        for prop in &props {
            if prop.clip_rect == first_strip
                && let egui::Shape::Mesh(mesh) = &prop.shape
            {
                let foot = mesh
                    .vertices
                    .iter()
                    .map(|vertex| vertex.pos.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    foot >= previous_foot - 0.001,
                    "Peripheral objects render from far feet to near feet"
                );
                previous_foot = foot;
            }
        }
        for prop in props {
            assert!(
                !prop.clip_rect.intersect(arena).is_positive(),
                "A tree, fence or wreck must never cover tactical ground"
            );
        }
        output.textures_delta.clear();
    }

    /// Pin: the zoom dressing cap never touches a field with 11 or more
    /// lanes: capped at the arena's reference lane, every such field paints
    /// exactly today's uncapped dressing.
    #[test]
    fn dressing_unchanged_for_filling_fields() {
        let textures = ArtTextures {
            props: Some(egui::TextureId::User(2)),
            ..Default::default()
        };
        let paint = |canvas: Rect, projection: ArenaProjection, theme: usize, max_unit: f32| {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                world_backdrop(ui.painter(), canvas, projection, theme, textures, max_unit);
            });
            output.textures_delta.clear();
            output.shapes
        };
        let mut pinned = 0;
        for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
            let canvas = Rect::from_min_size(Pos2::ZERO, size);
            let maximum = Rect::from_min_max(pos2(24.0, 124.0), canvas.max - vec2(24.0, 84.0));
            let cap = maximum.height() / camera::REFERENCE_LANES;
            for lanes in 11..=17u8 {
                for width in [8.0, 12.0, 20.0, 30.0] {
                    let fit = fit_field(maximum, maximum, lanes, width);
                    assert_eq!(fit.zoom, 1.0, "{lanes}x{width} already fills");
                    let projection = ArenaProjection::with_geometry(fit.rect, lanes, width);
                    for theme in [0, 2, 4] {
                        let capped = paint(canvas, projection, theme, cap);
                        assert!(capped.len() > 4, "{lanes}x{width}: no dressing painted");
                        assert!(
                            capped == paint(canvas, projection, theme, f32::INFINITY),
                            "{lanes}x{width} theme {theme} in {size:?}: the cap moved the dressing"
                        );
                        pinned += 1;
                    }
                }
            }
        }
        assert_eq!(pinned, 2 * 7 * 4 * 3);
    }

    #[test]
    fn biome_dressing_uses_arid_rocks_tropical_canopies_and_boreal_pines() {
        for index in 0..128 {
            assert!(matches!(
                peripheral_kind(2, index),
                Prop::Rock | Prop::Debris | Prop::Fence | Prop::Bush
            ));
            assert!(matches!(
                peripheral_kind(4, index),
                Prop::TreeBroadleaf | Prop::Bush
            ));
            assert!(matches!(
                peripheral_kind(3, index),
                Prop::TreePine | Prop::Bush
            ));
        }
        assert!((0..8).any(|index| peripheral_kind(0, index) == Prop::TreePine));
        assert!((0..8).any(|index| peripheral_kind(0, index) == Prop::TreeBroadleaf));
        assert!(peripheral_details(2, 0).len() <= 2);
        assert!(peripheral_details(4, 0).len() <= 2);
    }

    #[test]
    fn world_mapping_keeps_every_lane_and_the_fixed_defender_anchor() {
        for size in [vec2(1000., 580.), vec2(600., 350.), vec2(1920., 880.)] {
            let r = Rect::from_min_size(pos2(17., 51.), size);
            for lane in 0..11 {
                for x in [0.5, 1.5, 3., 15., 30.] {
                    let y = lane as f32 + 0.5;
                    let at = world(r, x, y);
                    assert!(r.contains(at));
                    let restored = inverse(r, at);
                    assert!((restored[0] - x).abs() < 0.00001);
                    assert!((restored[1] - y).abs() < 0.00001);
                }
            }
            assert_eq!(world(r, 0.5, 5.5).y, r.center().y);
        }
    }
}
