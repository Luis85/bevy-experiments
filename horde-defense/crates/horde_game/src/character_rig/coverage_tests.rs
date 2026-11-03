//! Read-only raster coverage of the meshes actually submitted by the rig.
//! This catches hidden limbs even when all anatomical endpoint equations pass.
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
fn fixture() -> (ArtTextures, Image, Image) {
    let components = image(include_bytes!(
        "../../../../assets/art/pixel/survivor-components-v3.png"
    ));
    let weapons = image(include_bytes!(
        "../../../../assets/art/pixel/defender-rig-v1.png"
    ));
    static GUNS: OnceLock<RigAtlas> = OnceLock::new();
    let textures = ArtTextures {
        components: Some(ComponentSheet {
            texture: TextureId::User(90),
            atlas: component_atlas(&components),
        }),
        rig: Some(RigSheet {
            texture: TextureId::User(91),
            atlas: GUNS.get_or_init(|| RigAtlas::from_image(&weapons)),
        }),
        ..Default::default()
    };
    (textures, components, weapons)
}
fn motion(class: WeaponClass) -> DefenderRigMotion {
    DefenderRigMotion {
        target: pos2(1000., 120.),
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
fn meshes(textures: ArtTextures, female: bool, class: WeaponClass) -> Vec<egui::epaint::Mesh> {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        assert!(draw(
            ui.painter(),
            textures,
            DefenderRigStyle {
                female,
                equipment: &EquipmentVisual::default()
            },
            pos2(160., 140.),
            vec2(200., 205.),
            Color32::WHITE,
            motion(class)
        ));
    });
    output.textures_delta.clear();
    output
        .shapes
        .into_iter()
        .filter_map(|s| {
            if let egui::Shape::Mesh(m) = s.shape {
                Some((*m).clone())
            } else {
                None
            }
        })
        .collect()
}
fn coverage(meshes: &[egui::epaint::Mesh], components: &Image, weapons: &Image) -> Vec<usize> {
    let mut bounds = Rect::NOTHING;
    for mesh in meshes {
        for v in &mesh.vertices {
            bounds.extend_with(v.pos);
        }
    }
    let origin = pos2(bounds.left().floor(), bounds.top().floor());
    let width = bounds.width().ceil() as usize + 2;
    let height = bounds.height().ceil() as usize + 2;
    assert!(width * height < 500_000);
    let mut owners = vec![usize::MAX; width * height];
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    for (owner, mesh) in meshes.iter().enumerate() {
        let source = if mesh.texture_id == TextureId::User(90) {
            components
        } else {
            weapons
        };
        let rgba = source.data.as_deref().unwrap();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let a = mesh.vertices[triangle[0] as usize];
            let b = mesh.vertices[triangle[1] as usize];
            let c = mesh.vertices[triangle[2] as usize];
            let divisor = cross(b.pos - a.pos, c.pos - a.pos);
            if divisor.abs() < 0.00001 {
                continue;
            }
            let mut rect = Rect::NOTHING;
            for point in [a.pos, b.pos, c.pos] {
                rect.extend_with(point);
            }
            for y in (rect.top() - origin.y).floor().max(0.) as usize
                ..((rect.bottom() - origin.y).ceil() as usize).min(height)
            {
                for x in (rect.left() - origin.x).floor().max(0.) as usize
                    ..((rect.right() - origin.x).ceil() as usize).min(width)
                {
                    let point = origin + vec2(x as f32 + 0.5, y as f32 + 0.5);
                    let wb = cross(point - a.pos, c.pos - a.pos) / divisor;
                    let wc = cross(b.pos - a.pos, point - a.pos) / divisor;
                    let wa = 1. - wb - wc;
                    if wa < 0. || wb < 0. || wc < 0. {
                        continue;
                    }
                    let uv = a.uv.to_vec2() * wa + b.uv.to_vec2() * wb + c.uv.to_vec2() * wc;
                    let sx =
                        (uv.x * source.width() as f32).clamp(0., source.width() as f32 - 1.) as u32;
                    let sy = (uv.y * source.height() as f32).clamp(0., source.height() as f32 - 1.)
                        as u32;
                    if rgba[((sy * source.width() + sx) * 4 + 3) as usize] >= 128 {
                        owners[y * width + x] = owner;
                    }
                }
            }
        }
    }
    let mut counts = vec![0; meshes.len()];
    for owner in owners {
        if owner != usize::MAX {
            counts[owner] += 1;
        }
    }
    counts
}
#[test]
fn both_arm_chains_have_visible_opaque_pixels_for_both_survivors_and_all_firearms() {
    let (textures, components, weapons) = fixture();
    for female in [false, true] {
        for class in WeaponClass::ALL {
            let meshes = meshes(textures, female, class);
            let counts = coverage(&meshes, &components, &weapons);
            let prefix = if female { "june" } else { "rook" };
            let atlas = textures.components.unwrap().atlas;
            let matching = |suffix: &str| {
                let region = region_for(
                    atlas.bounds[definition().parts[&format!("{prefix}_{suffix}")]],
                    atlas.size,
                );
                meshes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, m)| {
                        (m.texture_id == TextureId::User(90) && region.contains(m.vertices[0].uv))
                            .then_some(i)
                    })
                    .collect::<Vec<_>>()
            };
            let upper = matching("upper_arm");
            let fore = matching("forearm");
            assert_eq!(upper.len(), 2);
            assert_eq!(fore.len(), 2);
            for side in 0..2 {
                assert!(
                    counts[upper[side]] + counts[fore[side]] >= 85,
                    "{prefix} {class:?} arm{side} is hidden despite correct IK: upper{} fore{}",
                    counts[upper[side]],
                    counts[fore[side]]
                );
                assert!(
                    counts[fore[side]] >= 35,
                    "{prefix} {class:?} forearm{side} lacks a visible connection ({} opaque pixels)",
                    counts[fore[side]]
                );
            }
            let pose = Pose::new(
                textures.rig.unwrap().atlas,
                pos2(160., 140.),
                vec2(200., 205.),
                motion(class),
            );
            assert!(
                pose.near.shoulder.x + 6. * pose.unit < pose.far.shoulder.x,
                "Visible trigger shoulder must start behind the chest in this camera"
            );
        }
    }
}
#[test]
fn articulated_boot_meshes_have_separate_bounds_and_source_slices_cover_attached_joints() {
    let (textures, _, _) = fixture();
    let atlas = textures.components.unwrap().atlas;
    for female in [false, true] {
        let pose = Pose::new(
            textures.rig.unwrap().atlas,
            pos2(160., 140.),
            vec2(200., 205.),
            motion(WeaponClass::Rifle),
        );
        let mut boots = Vec::new();
        for (index, role) in ["far_leg", "near_leg"].iter().enumerate() {
            let name = format!("{}_{role}", if female { "june" } else { "rook" });
            let slices = leg_slices(
                atlas.bounds[definition().parts[&name]],
                definition().leg_sources[&name],
                pose.stance.legs[index],
                definition().part_layouts[&name].size[0] * pose.unit,
            );
            assert_eq!(
                slices[0].clip[1],
                atlas.bounds[definition().parts[&name]][1]
            );
            assert_eq!(slices[0].clip[3], slices[1].clip[1]);
            assert_eq!(slices[1].clip[3], slices[2].clip[1]);
            assert_eq!(
                slices[2].clip[3],
                atlas.bounds[definition().parts[&name]][3]
            );
            for slice in slices {
                assert!(slice.point(slice.source_start).distance(slice.start) < 0.001);
                assert!(slice.point(slice.source_end).distance(slice.end) < 0.001);
            }
            let mut bounds = Rect::NOTHING;
            for point in slices[2].corners() {
                bounds.extend_with(point);
            }
            boots.push(bounds);
            let leg = pose.stance.legs[index];
            assert!(
                (leg.hip.distance(leg.knee) - definition().stance.thigh * pose.unit).abs() < 0.001
            );
            assert!(
                (leg.knee.distance(leg.ankle) - definition().stance.shin * pose.unit).abs() < 0.001
            );
        }
        assert!(
            boots[0].right() + pose.unit < boots[1].left(),
            "Actual submitted boot hulls must leave a visible gap"
        );
        assert!(
            boots[0].bottom() < boots[1].bottom(),
            "Far foot has an authored depth stagger"
        );
    }
}
