//! Presentation-asset tests: layouts, masks, validation and fallbacks.

use super::loading::{image_for_egui, managed_texture};
use super::*;
use bevy::image::ImageSampler;
use horde_core::enemy_catalog::VisualSprite;
use std::sync::Arc;
#[test]
fn cohesive_menu_poses_keep_both_boots_and_lower_body_fixed_while_upper_body_breathes() {
    let foot = pos2(320.0, 410.0);
    let height = 190.0;
    for female in [false, true] {
        let (source, rect, waist) = menu_idle_layout(female, foot, height);
        let scale = height / (848.0 - source[1]);
        assert!((rect.left() + (source[2] - source[0]) * 0.53 * scale - foot.x).abs() < 0.001);
        assert!((rect.top() + (848.0 - source[1]) * scale - foot.y).abs() < 0.001);
        assert!(waist.y < foot.y && waist.y > rect.top());
        let ctx = bevy_egui::egui::Context::default();
        let mut lower = None;
        for breath in [0.0, 0.008, -0.008] {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                assert!(menu_survivor(
                    ui.painter(),
                    ArtTextures {
                        menu_idle: Some(TextureId::User(88)),
                        ..Default::default()
                    },
                    female,
                    foot,
                    height,
                    breath
                ));
            });
            let meshes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                        Some(mesh)
                    } else {
                        None
                    }
                })
                .collect();
            let vertices: Vec<_> = meshes[0]
                .vertices
                .iter()
                .map(|vertex| (vertex.pos, vertex.uv))
                .collect();
            if let Some(original) = &lower {
                assert_eq!(&vertices, original);
            } else {
                lower = Some(vertices);
            }
            output.textures_delta.clear();
        }
    }
}
#[test]
fn terrain_managed_upload_uses_advertised_device_budget_above_egui_default() {
    let image = Image::from_buffer(
        include_bytes!("../../../../assets/art/pixel/terrain-v1.png"),
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    )
    .unwrap();
    assert_eq!([image.width(), image.height()], [2172, 724]);
    let original = image.data.clone();
    let context = bevy_egui::egui::Context::default();
    let mut output = context.run_ui(Default::default(), |_| {
        assert_eq!(context.input(|i| i.max_texture_side), 2048);
        let handle =
            managed_texture(&context, "wide terrain regression".into(), &image, 4096).unwrap();
        assert_eq!(handle.size(), [2172, 724]);
        assert_eq!(context.input(|i| i.max_texture_side), 4096);
        assert_eq!(context.input(|i| i.raw.max_texture_side), Some(4096));
    });
    assert!(
        output
            .textures_delta
            .set
            .values()
            .flatten()
            .any(|delta| delta.image.size() == [2172, 724])
    );
    output.textures_delta.clear();
    assert_eq!(image.data, original);
}
#[test]
fn terrain_larger_than_actual_device_budget_returns_fallback_error_without_uploading() {
    let image = Image::from_buffer(
        include_bytes!("../../../../assets/art/pixel/terrain-v1.png"),
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    )
    .unwrap();
    let context = bevy_egui::egui::Context::default();
    let mut output = context.run_ui(Default::default(), |_| {
        let error = managed_texture(&context, "unsupported terrain".into(), &image, 2048)
            .err()
            .expect("device budget must be enforced");
        assert!(error.contains("2172x724") && error.contains("limit 2048"));
    });
    assert!(
        !output
            .textures_delta
            .set
            .values()
            .flatten()
            .any(|delta| delta.image.size() == [2172, 724])
    );
    output.textures_delta.clear();
}
#[test]
fn managed_cutout_upload_removes_transparent_rgb_without_changing_opaque_source_pixels() {
    for bytes in [
        include_bytes!("../../../../assets/art/pixel/actors-v1.png").as_slice(),
        include_bytes!("../../../../assets/art/pixel/props-v1.png").as_slice(),
        include_bytes!("../../../../assets/art/pixel/walkers-v1.png").as_slice(),
        include_bytes!("../../../../assets/art/pixel/defender-rig-v1.png").as_slice(),
    ] {
        let image = Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap();
        let original = image.data.clone().unwrap();
        let converted = image_for_egui(&image).unwrap();
        let mut transparent = 0;
        for (source, pixel) in original.as_chunks::<4>().0.iter().zip(&converted.pixels) {
            let rgba = pixel.to_array();
            assert_eq!(rgba[3], source[3]);
            if source[3] == 0 {
                transparent += 1;
                assert_eq!(rgba, [0, 0, 0, 0]);
            } else if source[3] == 255 {
                assert_eq!(rgba, *source);
            } else {
                assert!(rgba[..3].iter().all(|channel| *channel <= rgba[3]));
            }
        }
        assert!(transparent > 0);
        assert_eq!(
            image.data.as_ref().unwrap(),
            &original,
            "Upload must never mutate the original atlas"
        );
    }
}
#[test]
fn every_settled_body_is_batched_without_a_count_cap() {
    let ctx = bevy_egui::egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        assert!(corpse_batch(
            ui.painter(),
            ArtTextures {
                actors: Some(TextureId::User(77)),
                ..Default::default()
            },
            (0..1024).map(|n| (pos2(n as f32, 30.), vec2(20., 10.), Color32::WHITE))
        ));
    });
    let meshes: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                Some(mesh)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].vertices.len(), 1024 * 4);
    assert_eq!(meshes[0].indices.len(), 1024 * 6);
    output.textures_delta.clear();
}
#[test]
fn weapon_icon_atlas_cells_cover_every_firearm_and_ordnance_icon_with_solid_pixels() {
    let image = Image::from_buffer(
        include_bytes!("../../../../assets/art/pixel/defender-rig-v1.png"),
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        bevy::asset::RenderAssetUsages::default(),
    )
    .unwrap();
    let atlas = RigAtlas::from_image(&image);
    let pixels = image.data.as_ref().unwrap();
    for cell in [4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
        let [left, top, right, bottom] = atlas.cells[cell];
        let solid = (top as u32..bottom as u32)
            .flat_map(|y| (left as u32..right as u32).map(move |x| (x, y)))
            .any(|(x, y)| pixels[((y * image.width() + x) * 4 + 3) as usize] >= 128);
        assert!(solid, "Icon cell {cell} must contain displayed source art");
    }
}
#[test]
fn peripheral_sprite_bases_share_the_requested_ground_plane() {
    let ctx = bevy_egui::egui::Context::default();
    let foot = pos2(200., 160.);
    let mut output = ctx.run_ui(Default::default(), |ui| {
        for kind in [
            Prop::TreeBroadleaf,
            Prop::TreePine,
            Prop::Bush,
            Prop::Fence,
            Prop::Wreck,
            Prop::Debris,
            Prop::Rock,
            Prop::Lamp,
        ] {
            assert!(prop_grounded(
                ui.painter(),
                ArtTextures {
                    props: Some(TextureId::User(2)),
                    ..Default::default()
                },
                kind,
                foot,
                vec2(90., 80.),
                Color32::WHITE
            ));
        }
    });
    let meshes: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                Some(mesh)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(meshes.len(), 8);
    for mesh in meshes {
        let bottom = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.pos.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (bottom - foot.y).abs() < 0.001,
            "Wide wrecks and tall canopies share a planted baseline"
        );
    }
    output.textures_delta.clear();
}
#[test]
fn impact_mask_ignores_transparent_rgb_and_translucent_fringe() {
    let mut pixels = vec![255; 8 * 6 * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 0;
    }
    assert!(
        FlashMask::from_rgba(8, &pixels, [0., 0., 8., 6.])
            .runs
            .is_empty()
    );
    for y in 2..4 {
        for x in 2..6 {
            pixels[(y * 8 + x) * 4 + 3] = 255;
        }
    }
    pixels[(2 * 8 + 2) * 4 + 3] = 127;
    let mask = FlashMask::from_rgba(8, &pixels, [0., 0., 8., 6.]);
    assert_eq!(
        mask.runs,
        vec![[4., 2., 6., 4.]],
        "Only completely solid 2x2 pixel blocks may receive additive light"
    );
    let bright = hit_flash_color(HitFlash {
        strength: 1.,
        reduced: false,
    });
    assert!(bright.is_additive() && bright.r() >= 200 && bright.g() >= 180);
    let reduced = hit_flash_color(HitFlash {
        strength: 1.,
        reduced: true,
    });
    assert!(!reduced.is_additive() && reduced.a() <= 80);
}
#[test]
fn every_enemy_gait_and_boss_has_a_bounded_solid_silhouette_flash() {
    let decode = |bytes| {
        Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap()
    };
    let walkers = decode(include_bytes!(
        "../../../../assets/art/pixel/walkers-v1.png"
    ));
    let actors = decode(include_bytes!("../../../../assets/art/pixel/actors-v1.png"));
    let masks = FlashMasks::from_images(&walkers, &actors);
    // Five four-frame gaits and three standing bosses, plus both survivors.
    let catalog = enemy_visuals::active();
    assert_eq!(masks.enemies.len(), catalog.sprites().len());
    assert_eq!(masks.enemies.iter().map(Vec::len).sum::<usize>(), 5 * 4 + 3);
    assert_eq!(masks.survivors.len(), 2);
    let enemy_masks = catalog.sprites().iter().flat_map(|sprite| {
        let image = if sprite.is_walk() { &walkers } else { &actors };
        masks.enemies[sprite.index]
            .iter()
            .map(move |mask| (mask, image))
    });
    for (mask, image) in enemy_masks.chain(masks.survivors.iter().map(|mask| (mask, &actors))) {
        let pixels = image.data.as_ref().unwrap();
        {
            assert!(!mask.runs.is_empty() && mask.runs.len() < 2048);
            for [left, top, right, bottom] in mask.runs.iter().copied() {
                for y in top as u32..bottom as u32 {
                    for x in left as u32..right as u32 {
                        assert!(pixels[((y * image.width() + x) * 4 + 3) as usize] >= 128);
                    }
                }
            }
        }
    }
}
#[test]
fn calibrated_pixel_sources_keep_their_original_dimensions_and_rgba_pixels() {
    for (name, bytes, size, expected) in [
        (
            "defender-rig",
            include_bytes!("../../../../assets/art/pixel/defender-rig-v1.png").as_slice(),
            [1254., 1254.],
            0x980a_ee7b_fe71_c4bc,
        ),
        (
            "actors",
            include_bytes!("../../../../assets/art/pixel/actors-v1.png").as_slice(),
            ACTOR_SIZE,
            0x7c28_5d8a_91be_d1f4,
        ),
        (
            "terrain",
            include_bytes!("../../../../assets/art/pixel/terrain-v1.png").as_slice(),
            GROUND_SIZE,
            0xbe09_5ad1_ad84_d49d,
        ),
        (
            "keyart",
            include_bytes!("../../../../assets/art/pixel/keyart-v1.png").as_slice(),
            [1672., 941.],
            0x6759_02fc_df6f_a532,
        ),
        (
            "menu-survivors",
            include_bytes!("../../../../assets/art/pixel/menu-survivors-v1.png").as_slice(),
            MENU_IDLE_SIZE,
            0xf6b2_cb17_2a17_bb3d,
        ),
        (
            "keyart-v2",
            include_bytes!("../../../../assets/art/pixel/keyart-v2.png").as_slice(),
            [1672., 941.],
            0xf98c_8862_bc3c_db95,
        ),
        (
            "walkers",
            include_bytes!("../../../../assets/art/pixel/walkers-v1.png").as_slice(),
            enemy_visuals::active()
                .get(VisualSprite::Shambler)
                .unwrap()
                .source_size,
            0x5540_6952_c6b3_c223,
        ),
        (
            "props",
            include_bytes!("../../../../assets/art/pixel/props-v1.png").as_slice(),
            PROP_SIZE,
            0x14d9_9e7a_1bcf_2aa1,
        ),
    ] {
        let image = Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .expect("Original pixel atlas must decode");
        assert_eq!([image.width() as f32, image.height() as f32], size);
        let pixels = image.data.as_ref().expect("RGBA atlas retains CPU pixels");
        let hash = pixels.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        });
        assert_eq!(
            hash, expected,
            "{name} source pixels changed; inspect and recalibrate explicitly"
        );
    }
}
#[test]
fn survivor_and_boss_torso_pivots_stay_on_the_fixed_foot_anchor() {
    let bosses = enemy_visuals::active()
        .sprites()
        .iter()
        .filter_map(|sprite| match &sprite.art {
            SpriteArt::Standing(art) => Some((art.bounds, [art.hip_x, art.feet_y, art.height])),
            SpriteArt::Walk(_) => None,
        });
    let standing: Vec<_> = SURVIVOR_RECTS
        .into_iter()
        .zip(SURVIVOR_ANCHORS)
        .chain(bosses)
        .collect();
    assert_eq!(standing.len(), 5);
    for (source, anchor) in standing {
        let [hip, feet, _] = anchor;
        let center = pos2(200., 150.);
        let available = vec2(70., 60.);
        let (rect, foot) = standing_actor_layout(source, anchor, center, available);
        let scale = rect.width() / (source[2] - source[0]);
        let reconstructed = rect.min + vec2((hip - source[0]) * scale, (feet - source[1]) * scale);
        assert!(reconstructed.distance(foot) < 0.001);
        assert_eq!(foot, center + vec2(0., available.y * 0.5));
        assert!(source[0] < hip && hip < source[2]);
        assert!(source[1] < feet && feet < source[3]);
    }
}
#[test]
fn calibrated_turret_bore_targets_every_quadrant_with_a_planted_mount() {
    for available in [vec2(45., 70.), vec2(120., 60.)] {
        let center = pos2(200., 180.);
        let rect = fit(center, available, PROP_RECTS[Prop::Turret as usize]);
        let mount = turret_source_point(rect, TURRET_MOUNT);
        for degrees in [
            -179_f32, -135., -91., -90., -89., -45., 0., 45., 89., 90., 91., 135., 179.,
        ] {
            let angle = degrees.to_radians();
            let target = mount + vec2(angle.cos(), angle.sin()) * 150.;
            for recoil in [0., 0.5, 1.] {
                let motion = PropMotion {
                    target: Some(target),
                    recoil,
                    ..Default::default()
                };
                let transform = TurretTransform::new(rect, motion);
                assert!(transform.point(mount).distance(mount) < 0.0001);
                let muzzle = turret_muzzle(center, available, motion);
                let breech = transform.point(turret_source_point(rect, TURRET_BREECH));
                let bore = (muzzle - breech).normalized();
                let requested = (target - muzzle).normalized();
                assert!(
                    bore.dot(requested) > 0.99999,
                    "Bore points away from target at {degrees}°, recoil={recoil}: {bore:?} / {requested:?}"
                );
            }
        }
    }
}
#[test]
fn aiming_and_recoil_leave_the_pixel_tripod_vertices_and_uv_unchanged() {
    let ctx = bevy_egui::egui::Context::default();
    let mut bases = Vec::new();
    for target in [pos2(400., 20.), pos2(10., 300.), pos2(400., 300.)] {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            prop_animated(
                ui.painter(),
                ArtTextures {
                    props: Some(TextureId::User(1)),
                    ..Default::default()
                },
                Prop::Turret,
                pos2(200., 180.),
                vec2(100., 100.),
                Color32::WHITE,
                PropMotion {
                    target: Some(target),
                    recoil: 1.,
                    ..Default::default()
                },
            );
        });
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let bevy_egui::egui::Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            meshes.len(),
            2,
            "A separate fixed base and moving head must be rendered"
        );
        bases.push(
            meshes[0]
                .vertices
                .iter()
                .map(|v| (v.pos, v.uv))
                .collect::<Vec<_>>(),
        );
        output.textures_delta.clear();
    }
    assert_eq!(bases[0], bases[1]);
    assert_eq!(bases[1], bases[2]);
}
#[test]
fn pixel_atlas_regions_fit_their_own_source_and_preserve_aspect() {
    let boss_rects = enemy_visuals::active()
        .sprites()
        .iter()
        .filter_map(|sprite| match &sprite.art {
            SpriteArt::Standing(art) => Some((art.bounds, sprite.source_size)),
            SpriteArt::Walk(_) => None,
        });
    for (rect, size) in SURVIVOR_RECTS
        .into_iter()
        .chain(PORTRAIT_RECTS)
        .chain([CORPSE_RECT])
        .map(|r| (r, ACTOR_SIZE))
        .chain(boss_rects)
        .chain(PROP_RECTS.into_iter().map(|r| (r, PROP_SIZE)))
    {
        assert!(rect[0] >= 0. && rect[1] >= 0. && rect[2] <= size[0] && rect[3] <= size[1]);
        assert!(rect[2] > rect[0] && rect[3] > rect[1]);
        let uv = region_for(rect, size);
        assert!(uv.min.x >= 0. && uv.max.x <= 1. && uv.min.y >= 0. && uv.max.y <= 1.);
        let drawn = fit(Pos2::ZERO, vec2(70., 70.), rect);
        assert!(drawn.width() <= 70.001 && drawn.height() <= 70.001);
        assert!((drawn.aspect_ratio() - (rect[2] - rect[0]) / (rect[3] - rect[1])).abs() < 0.001);
    }
}
const OCCUPIED_WALKS: [[[f32; 4]; 4]; 5] = [
    [
        [78., 22., 246., 271.],
        [361., 22., 495., 268.],
        [613., 22., 778., 268.],
        [901., 22., 1037., 270.],
    ],
    [
        [73., 306., 253., 539.],
        [362., 306., 507., 537.],
        [612., 306., 791., 539.],
        [896., 305., 1037., 540.],
    ],
    [
        [42., 571., 267., 833.],
        [335., 566., 527., 828.],
        [579., 573., 810., 831.],
        [875., 573., 1070., 830.],
    ],
    [
        [72., 856., 247., 1111.],
        [349., 855., 500., 1110.],
        [609., 859., 787., 1112.],
        [891., 855., 1046., 1111.],
    ],
    [
        [68., 1134., 245., 1387.],
        [348., 1139., 516., 1387.],
        [605., 1139., 785., 1387.],
        [881., 1140., 1055., 1387.],
    ],
];
#[test]
fn pixel_gaits_keep_common_scale_hips_and_foot_baselines() {
    for (row, sprite) in [
        VisualSprite::Shambler,
        VisualSprite::Runner,
        VisualSprite::Brute,
        VisualSprite::Armored,
        VisualSprite::Demolisher,
    ]
    .into_iter()
    .enumerate()
    {
        let art = enemy_visuals::active().get(sprite).unwrap();
        let SpriteArt::Walk(spec) = &art.art else {
            panic!("{sprite:?} walks");
        };
        let source_size = art.source_size;
        assert_eq!(spec.frames.len(), 4);
        let mut previous = 0.;
        let mut pixel_scale: Option<f32> = None;
        for frame in 0..4 {
            let source = spec.frames[frame as usize];
            let (rect, uv, foot) = walk_frame_layout(
                spec,
                source_size,
                [source_size[0] as u32, source_size[1] as u32],
                frame,
                pos2(100., 100.),
                vec2(72., 60.),
            );
            assert!(uv.min.x >= previous && uv.max.x <= 1. && uv.min.y > 0. && uv.max.y < 1.);
            previous = uv.max.x;
            let occupied = OCCUPIED_WALKS[row][frame as usize];
            assert!(
                uv.min.x * source_size[0] < occupied[0] && uv.min.y * source_size[1] < occupied[1]
            );
            assert!(
                uv.max.x * source_size[0] > occupied[2] && uv.max.y * source_size[1] > occupied[3],
                "Every opaque boot, hand and tool retains a transparent gutter"
            );
            assert!(
                spec.feet_y - occupied[3] <= 8.,
                "A raised passing foot retains the common floor without rescaling"
            );
            let scale = rect.height() / (source.bounds[3] - source.bounds[1]);
            if let Some(previous) = pixel_scale {
                assert!((scale - previous).abs() < 0.0001);
            }
            pixel_scale = Some(scale);
            let reconstructed = rect.min
                + vec2(
                    (source.hip_x - source.bounds[0]) * scale,
                    (spec.feet_y - source.bounds[1]) * scale,
                );
            assert!(reconstructed.distance(foot) < 0.001);
            assert!(foot.distance(pos2(100., 130.)) < 0.001);
            assert!(source.bounds[0] < source.hip_x && source.hip_x < source.bounds[2]);
            assert!(source.bounds[1] < spec.feet_y && spec.feet_y <= source.bounds[3]);
        }
    }
}
fn blank_sheet(width: u32, height: u32) -> Image {
    Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    )
}
#[test]
fn mismatched_sheets_disable_enemy_art_without_reading_outside_the_image() {
    let decode = |bytes| {
        Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap()
    };
    let walkers = decode(include_bytes!(
        "../../../../assets/art/pixel/walkers-v1.png"
    ));
    let actors = decode(include_bytes!("../../../../assets/art/pixel/actors-v1.png"));
    assert_eq!(enemy_art_problem(Some(&walkers), Some(&actors)), None);
    assert_eq!(enemy_art_problem(None, None), None);
    // A replacement smaller than the declared sheet is rejected per sheet.
    let small = blank_sheet(64, 64);
    assert!(enemy_art_problem(Some(&small), Some(&actors)).is_some());
    assert!(enemy_art_problem(Some(&walkers), Some(&small)).is_some());
    assert!(enemy_art_problem(Some(&small), None).is_some());
    // Building masks from it neither panics nor lights anything outside it.
    let masks = FlashMasks::from_images(&small, &small);
    assert_eq!(masks.enemies.len(), enemy_visuals::active().sprites().len());
    assert!(masks.enemies.iter().all(Vec::is_empty));
    for mask in &masks.survivors {
        for run in &mask.runs {
            assert!(run[2] <= 64. && run[3] <= 64.);
        }
    }
    let clipped = FlashMask::from_rgba(4, &[255; 4 * 4 * 4], [-8., 2., 900., f32::NAN]);
    assert!(clipped.runs.is_empty());
    let inside = FlashMask::from_rgba(4, &[255; 4 * 4 * 4], [0., 0., 900., 900.]);
    assert_eq!(inside.runs, vec![[0., 0., 4., 2.], [0., 2., 4., 4.]]);
    // Rejected art draws nothing for enemies, so the caller uses its fallback
    // figure; survivors are unaffected.
    let ctx = bevy_egui::egui::Context::default();
    let shambler = Actor::enemy(VisualSprite::Shambler).unwrap();
    let boss = Actor::enemy(VisualSprite::Caller).unwrap();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let textures = ArtTextures {
            actors: Some(TextureId::User(1)),
            walkers: Some(WalkSheet {
                texture: TextureId::User(2),
                size: [1122, 1402],
            }),
            ..Default::default()
        };
        let rejected = ArtTextures {
            enemy_art_rejected: true,
            ..textures
        };
        let draw = |textures, actor| {
            actor_animated(
                ui.painter(),
                textures,
                actor,
                pos2(50., 50.),
                vec2(40., 40.),
                Color32::WHITE,
                &ActorPose::default(),
            )
        };
        assert!(draw(textures, shambler) && draw(textures, boss));
        assert!(!draw(rejected, shambler) && !draw(rejected, boss));
        assert!(draw(rejected, Actor::Player));
    });
    output.textures_delta.clear();
}
#[test]
fn mismatched_survivor_and_prop_sheets_are_rejected_and_draw_fallbacks() {
    let decode = |bytes| {
        Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap()
    };
    let actors = decode(include_bytes!("../../../../assets/art/pixel/actors-v1.png"));
    let props = decode(include_bytes!("../../../../assets/art/pixel/props-v1.png"));
    assert_eq!(survivor_art_problem(&actors), None);
    assert_eq!(prop_art_problem(&props), None);
    // Wrong size, or the right size with a cell outside it, is rejected.
    assert!(survivor_art_problem(&blank_sheet(64, 64)).is_some());
    assert!(prop_art_problem(&blank_sheet(64, 64)).is_some());
    assert!(prop_art_problem(&actors).is_some());
    assert!(sheet_problem(&blank_sheet(10, 10), "x", [10., 10.], &[[0., 0., 11., 5.]]).is_some());
    assert!(sheet_problem(&blank_sheet(10, 10), "x", [10., 10.], &[[0., 0., 10., 10.]]).is_none());
    // A rejected actor sheet draws no survivor, portrait or corpse art (the
    // callers paint procedural figures) while enemies are unaffected.
    let ctx = bevy_egui::egui::Context::default();
    let shambler = Actor::enemy(VisualSprite::Shambler).unwrap();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let textures = ArtTextures {
            actors: Some(TextureId::User(1)),
            walkers: Some(WalkSheet {
                texture: TextureId::User(2),
                size: [1122, 1402],
            }),
            ..Default::default()
        };
        let rejected = ArtTextures {
            survivor_art_rejected: true,
            ..textures
        };
        let p = ui.painter();
        let (at, size) = (pos2(50., 50.), vec2(40., 40.));
        assert!(actor(p, textures, Actor::Player, at, size, Color32::WHITE));
        assert!(portrait(p, textures, true, at, size));
        assert!(corpse_batch(p, textures, [(at, size, Color32::WHITE)]));
        assert!(!actor(p, rejected, Actor::Player, at, size, Color32::WHITE));
        assert!(!actor(p, rejected, Actor::Mara, at, size, Color32::WHITE));
        assert!(!portrait(p, rejected, true, at, size));
        assert!(!corpse_batch(p, rejected, [(at, size, Color32::WHITE)]));
        assert!(actor(p, rejected, shambler, at, size, Color32::WHITE));
    });
    output.textures_delta.clear();
}
#[test]
fn hit_flash_paints_additive_light_only_from_the_published_masks() {
    let decode = |bytes| {
        Image::from_buffer(
            bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            bevy::asset::RenderAssetUsages::default(),
        )
        .unwrap()
    };
    let walkers = decode(include_bytes!(
        "../../../../assets/art/pixel/walkers-v1.png"
    ));
    let actors = decode(include_bytes!("../../../../assets/art/pixel/actors-v1.png"));
    let context = bevy_egui::egui::Context::default();
    let textures = ArtTextures {
        actors: Some(TextureId::User(1)),
        walkers: Some(WalkSheet {
            texture: TextureId::User(2),
            size: [walkers.width(), walkers.height()],
        }),
        ..Default::default()
    };
    let sprite = &enemy_visuals::active().sprites()[0];
    let lit = HitFlash {
        strength: 1.,
        reduced: false,
    };
    // Paints one flash and returns every mesh it added.
    let paint = |textures: ArtTextures, flash: HitFlash, kind: Actor| {
        let mut output = context.run_ui(Default::default(), |ui| {
            actor_hit_flash(
                ui.painter(),
                textures,
                kind,
                pos2(200., 300.),
                vec2(120., 160.),
                flash,
                &ActorPose::default(),
            );
        });
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                bevy_egui::egui::Shape::Mesh(mesh) => Some(mesh.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // The masks live only in the egui context: before they are published
    // (or after they are removed again) nothing lights up.
    assert!(paint(textures, lit, Actor::Enemy(sprite)).is_empty());
    let masks = Arc::new(FlashMasks::from_images(&walkers, &actors));
    loading::publish_flash_masks(&context, Some(masks));
    for kind in [Actor::Enemy(sprite), Actor::Player, Actor::Mara] {
        let meshes = paint(textures, lit, kind);
        assert_eq!(meshes.len(), 1, "{kind:?} draws one flash mesh");
        let mesh = &meshes[0];
        assert!(!mesh.vertices.is_empty());
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.color == hit_flash_color(lit) && vertex.color.is_additive())
        );
    }
    let reduced = HitFlash {
        reduced: true,
        ..lit
    };
    let meshes = paint(textures, reduced, Actor::Player);
    assert!(
        meshes[0]
            .vertices
            .iter()
            .all(|vertex| !vertex.color.is_additive())
    );
    let off = HitFlash {
        strength: 0.,
        ..lit
    };
    assert!(paint(textures, off, Actor::Player).is_empty());
    // Rejected art never lights up, even with masks present.
    let rejected = ArtTextures {
        enemy_art_rejected: true,
        ..textures
    };
    assert!(paint(rejected, lit, Actor::Enemy(sprite)).is_empty());
    loading::publish_flash_masks(&context, None);
    assert!(paint(textures, lit, Actor::Player).is_empty());
}
#[test]
fn procedural_shadow_texture_matches_three_ring_profile() {
    let image = soft_shadow_image();
    assert_eq!(image.size, SOFT_SHADOW_SIZE);
    let [width, height] = SOFT_SHADOW_SIZE;
    // Premultiplied black everywhere.
    assert!(
        image
            .pixels
            .iter()
            .all(|p| p.r() == 0 && p.g() == 0 && p.b() == 0)
    );
    let alpha = |x: usize, y: usize| image.pixels[y * width + x].a();
    // Centre: all three rings, 10 + 15 + 23.
    assert_eq!(alpha(width / 2, height / 2), 48);
    // Along the horizontal axis, the texel at spread `s` of the shadow size
    // (the image spans 1.35 either side of the centre).
    let at = |spread: f32| (width as f32 * 0.5 * (1.0 + spread / 1.35)) as usize;
    assert_eq!(alpha(at(0.8), height / 2), 48);
    assert_eq!(alpha(at(1.0), height / 2), 25, "outer two rings");
    assert_eq!(alpha(at(1.25), height / 2), 10, "outer ring only");
    assert_eq!(
        alpha(width - 1, height / 2),
        10,
        "the image spans the outer ring"
    );
    assert_eq!(alpha(0, 0), 0);
    assert_eq!(alpha(width - 1, height - 1), 0);
    // The quad spans the outer ring at the requested strength.
    let (rect, tint) = soft_shadow_quad(pos2(10.0, 20.0), vec2(4.0, 2.0), 0.7);
    assert_eq!(
        rect,
        Rect::from_center_size(pos2(10.0, 20.0), vec2(10.8, 5.4))
    );
    assert_eq!(tint, Color32::from_black_alpha(179));
    // Symmetric about both axes.
    for y in 0..height {
        for x in 0..width {
            assert_eq!(alpha(x, y), alpha(width - 1 - x, y));
            assert_eq!(alpha(x, y), alpha(x, height - 1 - y));
        }
    }
}
