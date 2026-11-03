use super::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
};
fn image(bytes: &[u8]) -> Image {
    Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::MAIN_WORLD,
    )
    .unwrap()
}
fn textures() -> ArtTextures {
    static GUNS: OnceLock<RigAtlas> = OnceLock::new();
    ArtTextures {
        menu_idle: Some(TextureId::User(6)),
        canonical: Some(CanonicalSheet {
            texture: TextureId::User(7),
            size: [1448, 1086],
            sleeves: Some(source_sleeves(&image(include_bytes!(
                "../../../../assets/art/pixel/menu-survivors-v1.png"
            )))),
        }),
        rig: Some(RigSheet {
            texture: TextureId::User(5),
            atlas: GUNS.get_or_init(|| {
                RigAtlas::from_image(&image(include_bytes!(
                    "../../../../assets/art/pixel/defender-rig-v1.png"
                )))
            }),
        }),
        ..Default::default()
    }
}
fn motion(class: WeaponClass, target: Pos2) -> DefenderRigMotion {
    DefenderRigMotion {
        target,
        weapon: class,
        recoil: 0.,
        lean: 0.,
        reload: None,
        throwing: 0.,
        throw_kind: None,
        melee: 0.,
        brace: 0.,
        hit: 0.,
    }
}
#[test]
fn original_faces_hair_trousers_and_boots_keep_the_canonical_source_and_uniform_scale() {
    let source = image(include_bytes!(
        "../../../../assets/art/pixel/menu-survivors-v1.png"
    ));
    let pixels = source.data.as_deref().unwrap();
    assert_eq!([source.width(), source.height()], [1774, 887]);
    for female in [false, true] {
        let body = Body::new(
            female,
            pos2(200., 180.),
            vec2(200., 205.),
            motion(WeaponClass::Pistol, pos2(1000., 180.)),
        );
        let frame = body.frame;
        assert_eq!(
            frame.source_rect[3] - frame.source_rect[1],
            if female { 788. } else { 806. }
        );
        let head = frame.parts.iter().find(|p| p.id == "head").unwrap();
        assert_eq!(head.source, RigSource::MenuStill);
        for part in &frame.parts {
            assert_eq!(part.source, RigSource::MenuStill);
        }
        for y in 500..848 {
            for x in frame.source_rect[0] as u32..frame.source_rect[2] as u32 {
                if pixels[((y * source.width() + x) * 4 + 3) as usize] < 128 {
                    continue;
                }
                let count = frame
                    .parts
                    .iter()
                    .filter(|p| {
                        p.id != "head"
                            && x as f32 >= p.source_rect[0]
                            && (x as f32) < p.source_rect[2]
                            && y as f32 >= p.source_rect[1]
                            && (y as f32) < p.source_rect[3]
                    })
                    .count();
                assert_eq!(
                    count, 1,
                    "Every original trouser/boot pixel must survive exactly once: {} ({x},{y})",
                    frame.id
                );
            }
        }
        let (_, reference, _) = crate::visual_assets::menu_idle_layout(female, body.foot, 205.);
        let top = body.lower(point([frame.source_rect[0], frame.source_rect[1]]));
        let bottom = body.lower(point([frame.source_rect[2], frame.source_rect[3]]));
        assert!(
            top.distance(reference.min) < 0.001 && bottom.distance(reference.max) < 0.001,
            "Assembled reference parts must share the still artwork's exact source-height scale"
        );
    }
}
#[test]
fn canonical_assembly_never_paints_a_generic_v3_body_and_keeps_lower_pixels_planted() {
    let textures = textures();
    let ctx = egui::Context::default();
    let mut baselines = Vec::new();
    for female in [false, true] {
        baselines.clear();
        for action in 0..4 {
            let mut m = motion(WeaponClass::Rifle, pos2(1000., 180.));
            match action {
                0 => {}
                1 => {
                    m.reload = Some(0.5);
                    m.recoil = 1.;
                }
                2 => m.throwing = 0.9,
                _ => m.melee = 0.6,
            }
            let mut out = ctx.run_ui(Default::default(), |ui| {
                assert!(super::super::draw(
                    ui.painter(),
                    textures,
                    DefenderRigStyle {
                        female,
                        equipment: &EquipmentVisual::default()
                    },
                    pos2(200., 180.),
                    vec2(200., 205.),
                    Color32::WHITE,
                    m
                ));
            });
            let meshes: Vec<_> = out
                .shapes
                .iter()
                .filter_map(|s| {
                    if let egui::Shape::Mesh(m) = &s.shape {
                        Some(m)
                    } else {
                        None
                    }
                })
                .collect();
            assert!(meshes.iter().all(|m| {
                [
                    TextureId::User(5),
                    TextureId::User(6),
                    TextureId::User(7),
                    TextureId::Managed(0),
                ]
                .contains(&m.texture_id)
            }));
            let lower: Vec<_> = meshes
                .iter()
                .take(5)
                .map(|m| m.vertices.iter().map(|v| (v.pos, v.uv)).collect::<Vec<_>>())
                .collect();
            assert!(
                meshes
                    .iter()
                    .take(5)
                    .all(|m| m.texture_id == TextureId::User(6))
            );
            baselines.push(lower);
            out.textures_delta.clear();
        }
        assert!(
            baselines.windows(2).all(|pair| pair[0] == pair[1]),
            "Original trousers/boots must not bend, slide or change source when arms animate"
        );
    }
}
#[test]
fn canonical_hand_contacts_and_muzzle_use_the_same_source_derived_armature() {
    let textures = textures();
    let center = pos2(200., 180.);
    let size = vec2(200., 205.);
    for female in [false, true] {
        for class in WeaponClass::ALL {
            for degrees in [-150_f32, -75., 0., 55., 90., 150.] {
                let angle = degrees.to_radians();
                let target = center + vec2(angle.cos(), angle.sin()) * 1500.;
                let motion = motion(class, target);
                let pose = Pose::new(textures.rig.unwrap().atlas, female, center, size, motion);
                assert!(pose.near_hand.pose.palm.distance(pose.gun.grip) < 0.001);
                assert!(pose.far_hand.pose.palm.distance(pose.gun.support) < 0.001);
                assert!(
                    pose.near.wrist.distance(pose.near_hand.pose.wrist) < 0.01,
                    "{} {class:?} {degrees}: trigger wrist",
                    pose.body.frame.id
                );
                assert!(
                    pose.far.wrist.distance(pose.far_hand.pose.wrist) < 0.01,
                    "{} {class:?} {degrees}: support wrist",
                    pose.body.frame.id
                );
                let old = &definition().weapons[weapon_index(class)];
                let atlas = textures.rig.unwrap().atlas;
                let breech = pose.gun.point(pos2(
                    old.breech[0] * atlas.size[0] / 1254.,
                    old.breech[1] * atlas.size[1] / 1254.,
                ));
                assert!(
                    (pose.gun.muzzle - breech)
                        .normalized()
                        .dot((target - pose.gun.muzzle).normalized())
                        > 0.99999
                );
                assert_eq!(
                    crate::visual_assets::defender_muzzle(textures, female, center, size, motion),
                    Some(pose.gun.muzzle)
                );
                if let Some(stock) = pose.gun.stock {
                    assert!(stock.distance(pose.near.shoulder) <= size.y / 48. * 0.56);
                }
            }
        }
    }
}
#[test]
fn canonical_kit_regions_and_anatomical_anchors_match_the_untouched_asset() {
    let source = image(include_bytes!(
        "../../../../assets/art/pixel/canonical-parts-v1.png"
    ));
    assert_eq!([source.width(), source.height()], [1448, 1086]);
    for part in &config().kit_parts {
        assert_eq!(part.source, RigSource::CanonicalKit);
        let b = part.source_rect;
        assert!(b[0] >= 0. && b[1] >= 0. && b[2] <= 1448. && b[3] <= 1086.);
        for anchor in part.anchors.values() {
            assert!(
                anchor[0] >= b[0] && anchor[0] <= b[2] && anchor[1] >= b[1] && anchor[1] <= b[3]
            );
        }
    }
    assert_eq!(config().weapon_widths[&WeaponClass::Pistol], 5.5);
}
