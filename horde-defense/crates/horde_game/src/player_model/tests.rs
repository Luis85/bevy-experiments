use super::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    prelude::Image,
};
use std::collections::BTreeSet;

fn held_sample(aim: [f32; 2]) -> PlayerPoseSample {
    // The pinned rendering API takes DISPLAY coordinates, not source joints.
    PlayerPoseSample {
        tick: 180,
        aim,
        action: PlayerAction::Hold,
        recoil: 0.,
        brace: 0.,
        hit: 0.,
        reduced_effects: false,
    }
}

fn slot_geometry(pose: &ResolvedPlayerPose, slot: &str) -> Vec<(String, Vec<[u32; 2]>)> {
    pose.packets
        .iter()
        .filter(|packet| packet.slot == slot && !packet.local_mesh.vertices.is_empty())
        .map(|packet| {
            (
                packet.source_id.clone(),
                packet
                    .local_mesh
                    .vertices
                    .iter()
                    .map(|v| [v.uv[0].to_bits(), v.uv[1].to_bits()])
                    .collect(),
            )
        })
        .collect()
}

fn source_image() -> Image {
    Image::from_buffer(
        include_bytes!("../../../../assets/art/pixel/menu-survivors-v1.png"),
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::MAIN_WORLD,
    )
    .expect("Untouched original survivor source decodes")
}

fn identity_bind(catalogue: &CompiledCatalogue, id: &str) -> ResolvedPlayerPose {
    let model = catalogue.model(id).unwrap();
    resolve_model(
        catalogue,
        id,
        ModelPlacement {
            source_origin: model.definition.ground,
            scale: 1.0,
        },
    )
    .expect("Source-space default bind resolves")
}

#[test]
fn actual_bind_packets_reconstruct_every_original_rgba_byte_and_alpha_class() {
    let catalogue = builtin().unwrap();
    let decoded = source_image();
    for (id, nonzero, partial) in [("rook", 195_379, 194_427), ("june", 182_305, 181_738)] {
        let model = catalogue.model(id).unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        assert_eq!(source.rgba.as_ref(), decoded.data.as_deref().unwrap());
        let pose = identity_bind(catalogue, id);
        let audit = verification::audit_bind(source, model.definition.source.tile, &pose).unwrap();
        assert!(audit.is_exact(), "{id}: {audit:#?}");
        assert_eq!(audit.alpha_histogram.iter().sum::<usize>(), 887 * 887);
        assert_eq!(audit.source_pixels - audit.alpha_histogram[0], nonzero);
        assert_eq!(audit.alpha_histogram[1..255].iter().sum::<usize>(), partial);
        assert_eq!(audit.alpha_histogram[255], nonzero - partial);
        assert!(audit.alpha_histogram[1..128].iter().sum::<usize>() > 0);
        assert_eq!(audit.vertices, audit.rectangles * 4);
        assert_eq!(audit.indices, audit.rectangles * 6);
        assert_eq!(
            audit.rectangles,
            model
                .parts
                .iter()
                .map(|part| part.rectangles.len())
                .sum::<usize>()
        );
    }
}

#[test]
fn packet_masks_preserve_the_compiled_semantic_owners_without_a_whole_body_layer() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let pose = identity_bind(catalogue, id);
        assert_eq!(pose.packets.len(), model.parts.len());
        let mut seen = BTreeSet::new();
        for packet in &pose.packets {
            assert!(
                seen.insert(&packet.part_id),
                "One packet per real source part"
            );
            let part = model
                .parts
                .iter()
                .find(|part| part.definition.id == packet.part_id)
                .unwrap();
            assert_eq!(packet.slot, part.definition.slot);
            assert_eq!(packet.material, part.definition.material);
            assert_eq!(packet.channel, part.definition.channel);
            assert_eq!(packet.rectangles.as_ref(), part.rectangles.as_slice());
            for rect in packet.rectangles.iter() {
                for y in rect.y0..rect.y1 {
                    for x in rect.x0..rect.x1 {
                        let owner = model
                            .owner_at(x, y)
                            .expect("Every painted source pixel has an owner");
                        assert_eq!(owner.definition.id, packet.part_id, "{id} ({x},{y})");
                    }
                }
            }
        }
    }
}

#[test]
fn removing_any_source_slot_removes_exactly_its_pixels_and_no_baked_fallback_remains() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        let pose = identity_bind(catalogue, id);
        let complete =
            verification::audit_bind(source, model.definition.source.tile, &pose).unwrap();
        assert!(complete.is_exact());
        let slots: BTreeSet<_> = pose
            .packets
            .iter()
            .map(|packet| packet.slot.clone())
            .collect();
        for slot in slots {
            let mut isolated = pose.clone();
            isolated.packets.retain(|packet| packet.slot != slot);
            let expected_removed: usize = complete
                .parts
                .iter()
                .filter(|part| part.slot == slot)
                .map(|part| part.visible_pixels)
                .sum();
            assert!(
                expected_removed > 0,
                "{id}/{slot} must own actual visible source pixels"
            );
            let audit =
                verification::audit_bind(source, model.definition.source.tile, &isolated).unwrap();
            assert_eq!(
                audit.missing_visible_pixels, expected_removed,
                "{id}/{slot}"
            );
            assert_eq!(audit.differing_pixels, expected_removed, "{id}/{slot}");
            assert_eq!(audit.duplicate_visible_pixels, 0);
            assert!(!audit.is_exact());
        }
    }
}

#[test]
fn parity_auditor_rejects_duplicate_visible_packets_and_hidden_fill_at_bind() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        let pose = identity_bind(catalogue, id);
        let selected = pose
            .packets
            .iter()
            .find(|packet| packet.channel == SourceChannel::VisibleOriginal)
            .unwrap();
        let mut duplicate = pose.clone();
        duplicate.packets.push(selected.clone());
        let audit =
            verification::audit_bind(source, model.definition.source.tile, &duplicate).unwrap();
        assert!(audit.duplicate_visible_pixels > 0);
        assert!(!audit.is_exact());
        let mut hidden = pose.clone();
        hidden.packets[0].channel = SourceChannel::HiddenFill;
        let audit =
            verification::audit_bind(source, model.definition.source.tile, &hidden).unwrap();
        assert_eq!(audit.hidden_fill_packets, 1);
        assert!(!audit.is_exact());
        let mut mapped = pose.clone();
        mapped.packets[0].source_to_bind = bind::Affine2::translation([1., 0.]);
        mapped.packets[0].bone_transform = bind::Affine2::translation([-1., 0.]);
        let audit =
            verification::audit_bind(source, model.definition.source.tile, &mapped).unwrap();
        assert!(
            audit.nonidentity_packet_mappings > 0,
            "Canceling extra transforms still violate the reference bind contract"
        );
        assert!(!audit.is_exact());
    }
}

#[test]
fn parity_auditor_checks_actual_cached_uv_geometry_and_rejects_missing_meshes() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        let pose = identity_bind(catalogue, id);
        let mut wrong_uv = pose.clone();
        std::sync::Arc::make_mut(&mut wrong_uv.packets[0].local_mesh).vertices[0].uv = [-1., -1.];
        let audit =
            verification::audit_bind(source, model.definition.source.tile, &wrong_uv).unwrap();
        assert!(audit.geometry_mismatches > 0);
        assert!(!audit.is_exact());
        let mut empty = pose.clone();
        empty.packets[0].local_mesh = std::sync::Arc::new(LocalMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        });
        let audit = verification::audit_bind(source, model.definition.source.tile, &empty).unwrap();
        assert!(audit.geometry_mismatches > 0);
        assert!(!audit.is_exact());
    }
}

#[test]
fn both_arms_hands_legs_feet_and_original_pistol_have_independent_real_source_owners() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        let pose = identity_bind(catalogue, id);
        let audit = verification::audit_bind(source, model.definition.source.tile, &pose).unwrap();
        for part_id in [
            "head",
            "leg.near",
            "leg.far",
            "boot.near",
            "boot.far",
            "jacket.near.upper",
            "jacket.near.fore",
            "jacket.far.upper",
            "jacket.far.fore",
            "hand.near.palm",
            "hand.far.palm",
            "weapon.original.rear",
        ] {
            let part = audit
                .parts
                .iter()
                .find(|part| part.part_id == part_id)
                .unwrap_or_else(|| panic!("{id} lacks source part {part_id}"));
            assert!(
                part.visible_pixels > 0,
                "{id}/{part_id} is only an empty label"
            );
        }
        assert!(
            model.residue_stats.max_alpha <= 2,
            "Residue cannot conceal unsegmented anatomy"
        );
        for part in model
            .parts
            .iter()
            .filter(|part| part.definition.channel == SourceChannel::Residue)
        {
            for run in &part.runs {
                for x in run.x0..run.x1 {
                    assert!(source.pixel(x, run.y).unwrap()[3] <= 2);
                }
            }
        }
    }
}

#[test]
fn all_anatomical_sockets_and_visual_effect_origins_share_the_same_bind_transform() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let placement = ModelPlacement {
            source_origin: [913.0, 811.0],
            scale: 0.25,
        };
        let pose = resolve_model(catalogue, id, placement).unwrap();
        for socket in &model.definition.sockets {
            let expected = [
                (socket.bind_position[0] - model.definition.ground[0]) * placement.scale
                    + placement.source_origin[0],
                (socket.bind_position[1] - model.definition.ground[1]) * placement.scale
                    + placement.source_origin[1],
            ];
            assert_eq!(pose.sockets[&socket.id], expected, "{id}/{}", socket.id);
        }
        assert_eq!(pose.muzzle, pose.sockets["muzzle"]);
        assert_eq!(pose.throw_origin, pose.sockets["throw_origin"]);
    }
}

#[test]
fn egui_adapter_submits_only_selected_cached_parts_with_white_tint_and_shared_transform() {
    use bevy_egui::egui;
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let complete = resolve_model(
            catalogue,
            id,
            ModelPlacement {
                source_origin: [250., 300.],
                scale: 0.25,
            },
        )
        .unwrap();
        let texture = egui::TextureId::User(816);
        let bindings =
            std::collections::BTreeMap::from([(model.definition.source.id.clone(), texture)]);
        for removed_slot in [None, Some("outfit"), Some("ranged"), Some("body")] {
            let mut pose = complete.clone();
            if let Some(slot) = removed_slot {
                pose.packets.retain(|packet| packet.slot != slot);
            }
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                draw_egui(ui.painter(), &pose, &bindings).unwrap();
            });
            let meshes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .collect();
            let drawable: Vec<_> = pose
                .packets
                .iter()
                .filter(|packet| !packet.local_mesh.vertices.is_empty())
                .collect();
            assert_eq!(
                meshes.len(),
                drawable.len(),
                "No whole-art fallback or generated visible replacement: {id}/{removed_slot:?}"
            );
            for (mesh, packet) in meshes.into_iter().zip(drawable) {
                assert_eq!(mesh.texture_id, texture);
                assert_eq!(mesh.indices, packet.local_mesh.indices);
                assert_eq!(mesh.vertices.len(), packet.local_mesh.vertices.len());
                for (actual, source) in mesh.vertices.iter().zip(&packet.local_mesh.vertices) {
                    let expected = verification::packet_point(packet, source.position);
                    assert_eq!(actual.pos, egui::pos2(expected[0], expected[1]));
                    assert_eq!(actual.uv, egui::pos2(source.uv[0], source.uv[1]));
                    assert_eq!(
                        actual.color,
                        egui::Color32::WHITE,
                        "Preserve source RGBA; no per-part alpha/color alteration"
                    );
                }
            }
            output.drop_without_applying_deltas();
        }
    }
}

#[test]
fn selected_real_starter_equipment_keeps_the_original_bind_exact_with_no_generated_visible_fill() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let equipment = verification::review_equipment(
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )
    .unwrap();
    let before = equipment.clone();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let mut sample = held_sample([1200., 600.]);
        sample.action = PlayerAction::Bind;
        let pose = resolve_equipped_model(
            catalogue,
            id,
            &equipment,
            sample,
            ModelPlacement {
                source_origin: model.definition.ground,
                scale: 1.,
            },
        )
        .unwrap();
        let source = catalogue.source(&model.definition.source.id).unwrap();
        let audit = verification::audit_bind(source, model.definition.source.tile, &pose).unwrap();
        assert!(
            audit.is_exact(),
            "Selected starter must preserve approved source bind: {id}/{audit:#?}"
        );
        assert!(
            pose.packets
                .iter()
                .all(|packet| packet.source_id == source.id
                    && matches!(
                        packet.channel,
                        SourceChannel::VisibleOriginal | SourceChannel::Residue
                    ))
        );
    }
    assert_eq!(equipment, before, "Resolver cannot change frozen equipment");
}

#[test]
fn actual_campaign_starter_snapshot_including_stowed_melee_keeps_source_bind_exact() {
    let catalogue = builtin().unwrap();
    let campaign = horde_core::campaign::Campaign::default();
    let frozen = campaign
        .profile(horde_core::content::Mode::Personal)
        .equipment_visual;
    assert!(
        frozen.melee.is_some(),
        "Exercise real starter metadata, including stowed equipment"
    );
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let pose = resolve_equipped_model(
            catalogue,
            id,
            &frozen,
            PlayerPoseSample {
                action: PlayerAction::Bind,
                ..held_sample([1200., 600.])
            },
            ModelPlacement {
                source_origin: model.definition.ground,
                scale: 1.,
            },
        )
        .unwrap();
        let audit = verification::audit_bind(
            catalogue.source(&model.definition.source.id).unwrap(),
            model.definition.source.tile,
            &pose,
        )
        .unwrap();
        assert!(
            audit.is_exact(),
            "Real starter snapshot changed accepted art: {id}/{audit:#?}"
        );
    }
}

#[test]
fn every_authored_item_style_can_resolve_on_both_survivors_without_losing_the_body() {
    use horde_core::{
        content::{Slot, content},
        survivors::EquipmentVisualItem,
        weapons::{WeaponClass, WeaponSpec},
    };
    let catalogue = builtin().unwrap();
    for (definition, recipe) in content().items.iter().enumerate() {
        let mut frozen = verification::review_equipment(
            Some(WeaponClass::Pistol),
            Some("armor.cloth"),
            Some("accessory.pouch"),
        )
        .unwrap();
        let item = Some(EquipmentVisualItem {
            definition: horde_core::content::ItemDefId::at(definition),
            level: 1,
            quality: recipe.base_quality,
            style: recipe.visual_style.clone(),
        });
        match recipe.slot {
            Slot::Ranged => {
                frozen.ranged = item;
                frozen.weapon_class = Some(WeaponSpec::from_item(recipe).class);
            }
            Slot::Melee => frozen.melee = item,
            Slot::Outfit => frozen.outfit = item,
            Slot::Accessory => frozen.accessory = item,
        }
        for id in ["rook", "june"] {
            let pose = resolve_equipped_model(
                catalogue,
                id,
                &frozen,
                held_sample([1200., 600.]),
                ModelPlacement {
                    source_origin: [900., 700.],
                    scale: 0.25,
                },
            )
            .unwrap_or_else(|error| {
                panic!(
                    "Valid authored item {} / {} must not hide survivor {id}: {error}",
                    recipe.name, recipe.visual_style
                )
            });
            assert!(
                pose.packets.iter().any(
                    |packet| packet.part_id == "head" && !packet.local_mesh.vertices.is_empty()
                )
            );
            assert!(
                pose.packets
                    .iter()
                    .any(|packet| packet.part_id == "boot.near")
            );
            assert!(
                pose.packets
                    .iter()
                    .any(|packet| packet.part_id == "boot.far")
            );
        }
    }
}

#[test]
fn all_six_firearms_and_three_outfits_select_distinct_actual_cached_geometry() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    for id in ["rook", "june"] {
        let mut weapons = BTreeSet::new();
        for class in WeaponClass::ALL {
            let equipment = verification::review_equipment(
                Some(class),
                Some("armor.cloth"),
                Some("accessory.pouch"),
            )
            .unwrap();
            let pose = resolve_equipped_model(
                catalogue,
                id,
                &equipment,
                held_sample([1200., 600.]),
                placement,
            )
            .unwrap();
            let geometry = slot_geometry(&pose, "ranged");
            assert!(
                !geometry.is_empty(),
                "{id}/{class:?} has no actual weapon mesh"
            );
            assert!(
                weapons.insert(geometry),
                "{id}/{class:?} reuses another gun's actual source UVs"
            );
        }
        assert_eq!(weapons.len(), 6);
        let mut outfits = BTreeSet::new();
        for style in ["armor.leather", "armor.vest", "armor.plate"] {
            let equipment =
                verification::review_equipment(Some(WeaponClass::Pistol), Some(style), None)
                    .unwrap();
            let pose = resolve_equipped_model(
                catalogue,
                id,
                &equipment,
                held_sample([1200., 600.]),
                placement,
            )
            .unwrap();
            let geometry = slot_geometry(&pose, "outfit");
            assert!(!geometry.is_empty(), "{id}/{style} has no actual garment");
            assert!(
                pose.packets
                    .iter()
                    .filter(|packet| packet.slot == "outfit")
                    .all(|packet| !packet.part_id.starts_with("jacket.")),
                "Original jacket cannot remain baked beneath a replacement bundle"
            );
            assert!(
                outfits.insert(geometry),
                "{id}/{style} differs only by its label"
            );
        }
    }
}

#[test]
fn absent_frozen_slots_remove_original_gear_without_borrowing_a_default_loadout() {
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        let empty = verification::review_equipment(None, None, None).unwrap();
        let pose = resolve_equipped_model(
            catalogue,
            id,
            &empty,
            held_sample([1200., 600.]),
            ModelPlacement {
                source_origin: [900., 700.],
                scale: 0.25,
            },
        )
        .unwrap();
        assert!(
            pose.packets
                .iter()
                .all(|packet| packet.slot != "ranged" && packet.slot != "accessory"),
            "No original gun, pack or webbing may survive an empty snapshot: {id}"
        );
        assert!(
            pose.packets
                .iter()
                .all(|packet| !packet.part_id.starts_with("jacket.")),
            "Original owned outfit cannot survive an absent slot: {id}"
        );
        assert!(
            pose.packets
                .iter()
                .any(|packet| packet.slot == "outfit" && !packet.local_mesh.vertices.is_empty()),
            "Absent owned outfit still needs an explicit base garment: {id}"
        );
    }
}

#[test]
fn repeated_paused_and_reduced_samples_resolve_identical_actual_pixels_and_sockets() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let equipment = verification::review_equipment(
        Some(WeaponClass::Rifle),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )
    .unwrap();
    let before = equipment.clone();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    for id in ["rook", "june"] {
        for reduced in [false, true] {
            for action in [
                PlayerAction::Hold,
                PlayerAction::Reload { phase: 0.5 },
                PlayerAction::Throw { phase: 0.6 },
                PlayerAction::Melee { phase: 0.4 },
            ] {
                let sample = PlayerPoseSample {
                    action,
                    reduced_effects: reduced,
                    recoil: 0.3,
                    brace: 0.2,
                    hit: 0.1,
                    ..held_sample([1100., 520.])
                };
                let first =
                    resolve_equipped_model(catalogue, id, &equipment, sample, placement).unwrap();
                let repeated =
                    resolve_equipped_model(catalogue, id, &equipment, sample, placement).unwrap();
                assert!(
                    verification::equivalent_pose(&first, &repeated),
                    "Paused sample changed actual packets: {id}/{action:?}/reduced={reduced}"
                );
                for (a, b) in first.packets.iter().zip(&repeated.packets) {
                    assert!(
                        std::sync::Arc::ptr_eq(&a.local_mesh, &b.local_mesh),
                        "Source geometry must stay cached"
                    );
                }
                assert!(
                    first
                        .sockets
                        .values()
                        .flatten()
                        .all(|value| value.is_finite())
                );
                assert!(
                    first
                        .packets
                        .iter()
                        .all(|packet| packet.local_mesh.vertices.iter().all(|vertex| {
                            verification::packet_point(packet, vertex.position)
                                .iter()
                                .all(|value| value.is_finite())
                        }))
                );
            }
        }
    }
    assert_eq!(equipment, before);
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn actual_published_hand_socket<'c, 'p>(
    catalogue: &'c CompiledCatalogue,
    pose: &'p ResolvedPlayerPose,
    alias: &str,
) -> (&'p DrawPacket, &'c attachments::AttachmentSocket) {
    let mut owners = Vec::new();
    for packet in pose.packets.iter().filter(|packet| packet.slot == "hands") {
        let Some(definition) = catalogue
            .definition
            .attachments
            .iter()
            .find(|a| a.id == packet.part_id)
        else {
            continue;
        };
        for socket in definition
            .sockets
            .iter()
            .filter(|socket| socket.publish_as.iter().any(|name| name == alias))
        {
            owners.push((packet, socket));
        }
    }
    assert_eq!(
        owners.len(),
        1,
        "Exactly one actually painted hand cel must publish {alias}"
    );
    owners.pop().unwrap()
}

#[test]
fn held_contacts_bone_lengths_and_muzzle_follow_actual_drawn_geometry_for_every_gun_and_facing() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let mut unsupported = Vec::new();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        for class in WeaponClass::ALL {
            let equipment = verification::review_equipment(
                Some(class),
                Some("armor.cloth"),
                Some("accessory.pouch"),
            )
            .unwrap();
            let hold = catalogue
                .definition
                .holds
                .iter()
                .find(|hold| hold.model == id && hold.weapon_class == format!("{class:?}"))
                .unwrap();
            for aim in [[1500., 550.], [900., 0.], [900., 1400.], [100., 550.]] {
                let pose = match resolve_equipped_model(
                    catalogue,
                    id,
                    &equipment,
                    held_sample(aim),
                    placement,
                ) {
                    Ok(pose) => pose,
                    Err(error) => {
                        unsupported.push(format!("{id}/{class:?}/display_aim={aim:?}: {error}"));
                        continue;
                    }
                };
                for (contact, target, definition, palm_id) in [
                    (
                        "contact.near",
                        hold.weapon_sockets.grip.as_str(),
                        &hold.hand_contacts.near,
                        "hand.near.palm",
                    ),
                    (
                        "contact.far",
                        hold.weapon_sockets.support.as_deref().unwrap(),
                        &hold.hand_contacts.far,
                        "hand.far.palm",
                    ),
                ] {
                    assert!(
                        distance(pose.sockets[contact], pose.sockets[target]) <= 0.05,
                        "{id}/{class:?}/{aim:?}: {contact} misses {target}"
                    );
                    let palm = pose
                        .packets
                        .iter()
                        .find(|packet| packet.part_id == palm_id)
                        .unwrap();
                    assert!(
                        distance(
                            verification::packet_point(palm, definition.bind_position),
                            pose.sockets[contact]
                        ) <= 0.05,
                        "Reported contact must be on the actual drawn hand: {id}/{class:?}/{contact}"
                    );
                }
                for (bone_id, start, end) in [
                    ("arm.near.upper", "shoulder.near", "elbow.near"),
                    ("arm.near.fore", "elbow.near", "wrist.near"),
                    ("arm.far.upper", "shoulder.far", "elbow.far"),
                    ("arm.far.fore", "elbow.far", "wrist.far"),
                ] {
                    let bone = model
                        .definition
                        .bones
                        .iter()
                        .find(|bone| bone.id == bone_id)
                        .unwrap();
                    let expected =
                        distance(bone.bind_origin, bone.bind_tip.unwrap()) * placement.scale;
                    let actual = distance(pose.sockets[start], pose.sockets[end]);
                    assert!(
                        (actual - expected).abs() <= 0.01,
                        "No source-derived limb stretch: {id}/{class:?}/{bone_id}/{aim:?}, expected={expected}, actual={actual}"
                    );
                }
                let (weapon_id, muzzle_point) = if let Some(attachment_id) = &hold.weapon_attachment
                {
                    let attachment = catalogue
                        .definition
                        .attachments
                        .iter()
                        .find(|attachment| attachment.id == *attachment_id)
                        .unwrap();
                    let socket = attachment
                        .sockets
                        .iter()
                        .find(|socket| socket.id == hold.weapon_sockets.muzzle)
                        .unwrap();
                    (attachment_id.as_str(), socket.source_position)
                } else {
                    let socket = model
                        .definition
                        .sockets
                        .iter()
                        .find(|socket| socket.id == hold.weapon_sockets.muzzle)
                        .unwrap();
                    ("weapon.original.rear", socket.bind_position)
                };
                let gun = pose
                    .packets
                    .iter()
                    .find(|packet| packet.part_id == weapon_id)
                    .unwrap();
                assert!(
                    distance(verification::packet_point(gun, muzzle_point), pose.muzzle) <= 0.001,
                    "Muzzle must be the actual drawn barrel endpoint: {id}/{class:?}/{aim:?}"
                );
                assert_eq!(pose.muzzle, pose.sockets[&hold.weapon_sockets.muzzle]);
                assert_eq!(pose.throw_origin, pose.sockets["throw_origin"]);
                let barrel = pose.sockets[&hold.weapon_sockets.barrel_back];
                let forward = [pose.muzzle[0] - barrel[0], pose.muzzle[1] - barrel[1]];
                let target = [aim[0] - pose.muzzle[0], aim[1] - pose.muzzle[1]];
                let cosine = (forward[0] * target[0] + forward[1] * target[1])
                    / (forward[0].hypot(forward[1]) * target[0].hypot(target[1]));
                assert!(
                    cosine >= 0.98,
                    "Drawn gun must follow crosshair, including up/down/left: {id}/{class:?}/{aim:?}, cosine={cosine}"
                );
            }
        }
    }
    assert!(
        unsupported.is_empty(),
        "Required held poses remain unsupported:\n{}",
        unsupported.join("\n")
    );
}

#[test]
fn throw_and_melee_effect_origins_are_on_the_actual_drawn_hand_and_weapon() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let equipment = verification::review_equipment(
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )
    .unwrap();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    let mut unsupported = Vec::new();
    for id in ["rook", "june"] {
        let model = catalogue.model(id).unwrap();
        let throw_socket = model
            .definition
            .sockets
            .iter()
            .find(|socket| socket.id == "throw_origin")
            .unwrap();
        for aim in [[1500., 550.], [100., 550.]] {
            for action in [
                PlayerAction::Throw { phase: 0. },
                PlayerAction::Throw { phase: 0.6 },
                PlayerAction::Melee { phase: 0.5 },
            ] {
                let pose = match resolve_equipped_model(
                    catalogue,
                    id,
                    &equipment,
                    PlayerPoseSample {
                        action,
                        ..held_sample(aim)
                    },
                    placement,
                ) {
                    Ok(pose) => pose,
                    Err(error) => {
                        unsupported.push(format!("{id}/{action:?}/display_aim={aim:?}: {error}"));
                        continue;
                    }
                };
                if matches!(action, PlayerAction::Throw { .. }) {
                    let palm = pose
                        .packets
                        .iter()
                        .find(|packet| packet.part_id == "hand.near.palm")
                        .unwrap();
                    assert!(
                        distance(
                            verification::packet_point(palm, throw_socket.bind_position),
                            pose.throw_origin
                        ) <= 0.001,
                        "Throw release must be the actual transformed palm: {id}/{aim:?}"
                    );
                } else {
                    let (packet, socket) = pose
                        .packets
                        .iter()
                        .filter(|packet| packet.slot == "melee")
                        .find_map(|packet| {
                            let attachment = catalogue
                                .definition
                                .attachments
                                .iter()
                                .find(|attachment| attachment.id == packet.part_id)?;
                            let socket = attachment
                                .sockets
                                .iter()
                                .find(|socket| socket.id == "melee.grip")?;
                            Some((packet, socket))
                        })
                        .expect("Active melee must display a real owned melee attachment");
                    let actual_grip = verification::packet_point(packet, socket.source_position);
                    let (palm, contact) =
                        actual_published_hand_socket(catalogue, &pose, "contact.near");
                    let actual_hand = verification::packet_point(palm, contact.source_position);
                    assert!(
                        distance(actual_hand, actual_grip) <= 0.05,
                        "Reported contact equality cannot conceal a detached actual hand: {id}/{aim:?}"
                    );
                    assert!(distance(actual_grip, pose.sockets["melee.grip"]) <= 0.001);
                    assert!(distance(actual_hand, pose.sockets["contact.near"]) <= 0.001);
                }
            }
        }
    }
    assert!(
        unsupported.is_empty(),
        "Required source actions remain unsupported:\n{}",
        unsupported.join("\n")
    );
}

/// Compare the production packet's full transformed vertex stream, not just
/// reported IK/socket endpoints. Required source parts may not disappear.
fn action_boundary_geometry_errors(
    held: &ResolvedPlayerPose,
    boundary: &ResolvedPlayerPose,
    part_ids: &[&str],
    include_gun: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    let selected: Vec<_> = held
        .packets
        .iter()
        .filter(|packet| {
            part_ids.contains(&packet.part_id.as_str()) || (include_gun && packet.slot == "ranged")
        })
        .collect();
    for id in part_ids {
        if !selected
            .iter()
            .any(|packet| packet.part_id == *id && !packet.local_mesh.vertices.is_empty())
        {
            errors.push(format!("Missing nonempty held source part {id}"));
        }
    }
    if include_gun
        && !selected
            .iter()
            .any(|packet| packet.slot == "ranged" && !packet.local_mesh.vertices.is_empty())
    {
        errors.push("No actual held weapon geometry to compare".into());
    }
    for source in selected {
        let Some(actual) = boundary
            .packets
            .iter()
            .find(|packet| packet.part_id == source.part_id)
        else {
            errors.push(format!(
                "Boundary removed retained source part {}",
                source.part_id
            ));
            continue;
        };
        if actual.source_id != source.source_id
            || actual.local_mesh.indices != source.local_mesh.indices
            || actual.local_mesh.vertices.len() != source.local_mesh.vertices.len()
        {
            errors.push(format!(
                "Boundary changed actual source topology for {}",
                source.part_id
            ));
            continue;
        }
        let mut maximum_distance = 0.0_f32;
        let mut invalid = false;
        for (a, b) in source
            .local_mesh
            .vertices
            .iter()
            .zip(&actual.local_mesh.vertices)
        {
            let expected = verification::packet_point(source, a.position);
            let point = verification::packet_point(actual, b.position);
            invalid |= a.uv != b.uv
                || expected
                    .iter()
                    .chain(point.iter())
                    .any(|value| !value.is_finite());
            maximum_distance = maximum_distance.max(distance(expected, point));
        }
        // One thousandth of a display pixel permits harmless float order
        // differences while rejecting even a subpixel visible pose snap.
        if invalid || maximum_distance > 0.001 {
            errors.push(format!("{} actual vertex discontinuity: max={maximum_distance:.6} display pixels, invalid_uv_or_position={invalid}", source.part_id));
        }
    }
    errors
}

#[test]
fn action_boundaries_preserve_actual_aimed_gun_and_recover_held_arm_geometry() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    let near = [
        "jacket.near.upper",
        "jacket.near.fore",
        "jacket.near.cuff",
        "hand.near.palm",
    ];
    let both = [
        "jacket.near.upper",
        "jacket.near.fore",
        "jacket.near.cuff",
        "hand.near.palm",
        "jacket.far.upper",
        "jacket.far.fore",
        "hand.far.palm",
    ];
    let mut failures = Vec::new();
    for id in ["rook", "june"] {
        for class in WeaponClass::ALL {
            let equipment = verification::review_equipment(
                Some(class),
                Some("armor.cloth"),
                Some("accessory.pouch"),
            )
            .unwrap();
            for aim in [[1500., 550.], [1400., 150.], [1400., 1000.], [100., 500.]] {
                let sample = held_sample(aim);
                let held =
                    match resolve_equipped_model(catalogue, id, &equipment, sample, placement) {
                        Ok(pose) => pose,
                        Err(error) => {
                            failures
                                .push(format!("{id}/{class:?}/display_aim={aim:?}/held: {error}"));
                            continue;
                        }
                    };
                for action in [
                    PlayerAction::Reload { phase: 0. },
                    PlayerAction::Reload { phase: 1. },
                    PlayerAction::Melee { phase: 1. },
                    PlayerAction::Throw { phase: 1. },
                ] {
                    // Identical frozen equipment, clock, aim and placement. The
                    // action layer must return to this aimed baseline, never
                    // the original low-ready still before the next Hold frame.
                    let boundary = match resolve_equipped_model(
                        catalogue,
                        id,
                        &equipment,
                        PlayerPoseSample { action, ..sample },
                        placement,
                    ) {
                        Ok(pose) => pose,
                        Err(error) => {
                            failures.push(format!(
                                "{id}/{class:?}/display_aim={aim:?}/{action:?}: {error}"
                            ));
                            continue;
                        }
                    };
                    let reload = matches!(action, PlayerAction::Reload { .. });
                    for error in action_boundary_geometry_errors(
                        &held,
                        &boundary,
                        if reload { &near } else { &both },
                        reload,
                    ) {
                        failures.push(format!(
                            "{id}/{class:?}/display_aim={aim:?}/{action:?}: {error}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Required actual-geometry action boundaries are discontinuous or unsupported:\n{}",
        failures.join("\n")
    );
}

#[test]
fn active_hand_cel_suppresses_only_the_original_palm_and_retains_source_wrist_and_cuff() {
    use horde_core::weapons::WeaponClass;
    let catalogue = builtin().unwrap();
    let equipment = verification::review_equipment(
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )
    .unwrap();
    let placement = ModelPlacement {
        source_origin: [900., 700.],
        scale: 0.25,
    };
    for (id, wrist_pixels) in [("rook", 1492), ("june", 1308)] {
        let model = catalogue.model(id).unwrap();
        let cels: Vec<_> = catalogue
            .definition
            .attachments
            .iter()
            .filter(|attachment| {
                attachment.model == id
                    && attachment.slot == "hands"
                    && attachment
                        .replace_parts
                        .iter()
                        .any(|part| part == "hand.near.palm")
            })
            .collect();
        assert_eq!(
            cels.len(),
            1,
            "One explicitly authored source-native melee grasp"
        );
        let cel = cels[0];
        assert_eq!(cel.channel, SourceChannel::SkinReplacement);
        assert_eq!(cel.source_id, model.definition.source.id);
        assert_eq!(cel.bone, "hand.near");
        assert_eq!(cel.replace_parts, ["hand.near.palm"]);
        let compiled_cel = &catalogue.attachments[&cel.id];
        let far_palm = model
            .parts
            .iter()
            .find(|part| part.definition.id == "hand.far.palm")
            .unwrap();
        assert_eq!(
            compiled_cel.runs, far_palm.runs,
            "The grasp must use the actual original far-fist pixels"
        );
        let original_wrist = model
            .parts
            .iter()
            .find(|part| part.definition.id == "hand.near.wrist")
            .unwrap();
        assert_eq!(
            original_wrist
                .runs
                .iter()
                .map(|run| (run.x1 - run.x0) as usize)
                .sum::<usize>(),
            wrist_pixels
        );
        let wrist_anchor = model
            .definition
            .sockets
            .iter()
            .find(|socket| socket.id == "wrist.near")
            .unwrap()
            .bind_position;
        for aim in [[1500., 550.], [100., 550.]] {
            for action in [
                PlayerAction::Bind,
                PlayerAction::Hold,
                PlayerAction::Reload { phase: 0.5 },
                PlayerAction::Throw { phase: 0.5 },
                PlayerAction::Melee { phase: 0.5 },
                PlayerAction::Melee { phase: 1. },
            ] {
                let pose = resolve_equipped_model(
                    catalogue,
                    id,
                    &equipment,
                    PlayerPoseSample {
                        action,
                        ..held_sample(aim)
                    },
                    placement,
                )
                .unwrap_or_else(|error| panic!("{id}/{aim:?}/{action:?}: {error}"));
                let active = matches!(action, PlayerAction::Melee {phase} if phase < 1.);
                assert_eq!(
                    pose.packets.iter().any(|packet| packet.part_id == cel.id),
                    active,
                    "{id}/{action:?}"
                );
                assert_eq!(
                    pose.packets
                        .iter()
                        .any(|packet| packet.part_id == "hand.near.palm"),
                    !active,
                    "Only the actually active cel suppresses the original pointing palm"
                );
                for part_id in ["hand.near.wrist", "jacket.near.cuff"] {
                    let original = model
                        .parts
                        .iter()
                        .find(|part| part.definition.id == part_id)
                        .unwrap();
                    let packet = pose
                        .packets
                        .iter()
                        .find(|packet| packet.part_id == part_id)
                        .expect("Wrist bridge and cuff may not disappear with the palm");
                    assert_eq!(packet.source_id, model.definition.source.id);
                    assert_eq!(packet.material, original.definition.material);
                    assert!(std::sync::Arc::ptr_eq(
                        &packet.local_mesh,
                        &original.local_mesh
                    ));
                    assert!(!packet.local_mesh.vertices.is_empty());
                }
                let wrist = pose
                    .packets
                    .iter()
                    .find(|packet| packet.part_id == "hand.near.wrist")
                    .unwrap();
                let cuff = pose
                    .packets
                    .iter()
                    .find(|packet| packet.part_id == "jacket.near.cuff")
                    .unwrap();
                assert!(
                    distance(
                        verification::packet_point(wrist, wrist_anchor),
                        verification::packet_point(cuff, wrist_anchor)
                    ) <= 0.01,
                    "Actual wrist/cuff anchor disconnected: {id}/{aim:?}/{action:?}"
                );
                if matches!(action, PlayerAction::Bind) {
                    let source = catalogue.source(&model.definition.source.id).unwrap();
                    let reference = resolve_equipped_model(
                        catalogue,
                        id,
                        &equipment,
                        PlayerPoseSample {
                            action,
                            ..held_sample(aim)
                        },
                        ModelPlacement {
                            source_origin: model.definition.ground,
                            scale: 1.,
                        },
                    )
                    .unwrap();
                    assert!(
                        verification::audit_bind(source, model.definition.source.tile, &reference)
                            .unwrap()
                            .is_exact(),
                        "Action cel cannot change a single original default byte"
                    );
                }
            }
        }
    }
}

#[test]
fn active_fist_aliases_and_all_melee_grips_match_actual_painted_source_surfaces() {
    use horde_core::{content::content, survivors::EquipmentVisualItem, weapons::WeaponClass};
    let catalogue = builtin().unwrap();
    for id in ["rook", "june"] {
        for style in [
            "melee.crowbar",
            "melee.axe",
            "melee.machete",
            "melee.hammer",
            "melee.saber",
        ] {
            let mut equipment = verification::review_equipment(
                Some(WeaponClass::Pistol),
                Some("armor.cloth"),
                Some("accessory.pouch"),
            )
            .unwrap();
            let (definition, recipe) = content()
                .items
                .iter()
                .enumerate()
                .find(|(_, recipe)| recipe.visual_style == style)
                .unwrap();
            equipment.melee = Some(EquipmentVisualItem {
                definition: horde_core::content::ItemDefId::at(definition),
                level: 1,
                quality: recipe.base_quality,
                style: style.into(),
            });
            for aim in [[1500., 550.], [100., 550.]] {
                let pose = resolve_equipped_model(
                    catalogue,
                    id,
                    &equipment,
                    PlayerPoseSample {
                        action: PlayerAction::Melee { phase: 0.5 },
                        ..held_sample(aim)
                    },
                    ModelPlacement {
                        source_origin: [900., 700.],
                        scale: 0.25,
                    },
                )
                .unwrap_or_else(|error| panic!("{id}/{style}/{aim:?}: {error}"));
                for alias in ["palm.near", "throw_origin", "contact.near"] {
                    let (packet, socket) = actual_published_hand_socket(catalogue, &pose, alias);
                    let compiled = &catalogue.attachments[&packet.part_id];
                    let x = socket.source_position[0].floor() as u32;
                    let y = socket.source_position[1].floor() as u32;
                    assert!(
                        compiled
                            .runs
                            .iter()
                            .any(|run| run.y == y && run.x0 <= x && x < run.x1),
                        "{alias} must actually lie on the visible source-native fist"
                    );
                    let actual = verification::packet_point(packet, socket.source_position);
                    let source_point = packet
                        .bone_transform
                        .point(packet.source_to_bind.point(socket.source_position));
                    assert!(
                        distance(actual, pose.sockets[alias]) <= 0.001,
                        "Alias must come from actual painted cel: {id}/{style}/{alias}/{aim:?}"
                    );
                    let qualified = format!("{}::{}", packet.part_id, socket.id);
                    assert!(distance(actual, pose.sockets[&qualified]) <= 0.001);
                    assert!(
                        distance(source_point, pose.source_sockets[alias]) <= 0.01,
                        "Cached source-space alias must include the actually drawn facing transform"
                    );
                    assert!(distance(source_point, pose.source_sockets[&qualified]) <= 0.01);
                    if alias == "throw_origin" {
                        assert!(distance(actual, pose.throw_origin) <= 0.001);
                    }
                }
                let (hand, contact) =
                    actual_published_hand_socket(catalogue, &pose, "contact.near");
                let actual_hand = verification::packet_point(hand, contact.source_position);
                let (tool, grip) = pose
                    .packets
                    .iter()
                    .filter(|packet| packet.slot == "melee")
                    .find_map(|packet| {
                        let definition = catalogue
                            .definition
                            .attachments
                            .iter()
                            .find(|a| a.id == packet.part_id)?;
                        definition
                            .sockets
                            .iter()
                            .find(|socket| socket.id == "melee.grip")
                            .map(|socket| (packet, socket))
                    })
                    .unwrap();
                assert!(
                    distance(
                        actual_hand,
                        verification::packet_point(tool, grip.source_position)
                    ) <= 0.05,
                    "Actual fist and actual tool grip diverge: {id}/{style}/{aim:?}"
                );
            }
        }
    }
}

fn recompile_review_definition(
    definition: PlayerModelCatalogue,
) -> Result<CompiledCatalogue, String> {
    let source = builtin()?;
    let views: Vec<_> = source
        .sources
        .values()
        .map(|source| SourceImageView {
            id: &source.id,
            width: source.width,
            height: source.height,
            rgba: &source.rgba,
        })
        .collect();
    compile_catalogue(definition, &views)
}

#[test]
fn action_cel_schema_rejects_protected_owners_missing_activation_and_illegal_aliases() {
    let catalogue = builtin().unwrap();
    let index = catalogue
        .definition
        .attachments
        .iter()
        .position(|a| a.model == "rook" && a.slot == "hands" && !a.replace_parts.is_empty())
        .unwrap();
    for invalid in 0..6 {
        let mut definition = catalogue.definition.clone();
        let cel = &mut definition.attachments[index];
        match invalid {
            0 => cel.replace_parts = vec!["head".into()],
            1 => cel.replace_parts = vec![cel.id.clone()],
            2 => cel.activation.actions.clear(),
            3 => cel.channel = SourceChannel::HiddenFill,
            4 => cel.sockets[0].publish_as.push("muzzle".into()),
            _ => {
                let duplicate = cel.sockets[0].publish_as[0].clone();
                cel.sockets[0].publish_as.push(duplicate);
            }
        }
        assert!(
            definition.validate().is_err(),
            "Malformed action cel configuration {invalid} must fail authoring validation"
        );
    }
}

#[test]
fn conflicting_active_hand_and_tool_aliases_fail_instead_of_publishing_a_fake_contact() {
    use horde_core::weapons::WeaponClass;
    let source = builtin().unwrap();
    for id in ["rook", "june"] {
        let mut definition = source.definition.clone();
        let cel = definition
            .attachments
            .iter_mut()
            .find(|a| a.model == id && a.slot == "hands" && !a.replace_parts.is_empty())
            .unwrap();
        let contact = cel
            .sockets
            .iter_mut()
            .find(|socket| {
                socket
                    .publish_as
                    .iter()
                    .any(|alias| alias == "contact.near")
            })
            .unwrap();
        contact.source_position[0] += 4.;
        let catalogue = recompile_review_definition(definition).unwrap();
        let equipment = verification::review_equipment(
            Some(WeaponClass::Pistol),
            Some("armor.cloth"),
            Some("accessory.pouch"),
        )
        .unwrap();
        let result = resolve_equipped_model(
            &catalogue,
            id,
            &equipment,
            PlayerPoseSample {
                action: PlayerAction::Melee { phase: 0.5 },
                ..held_sample([1500., 550.])
            },
            ModelPlacement {
                source_origin: [900., 700.],
                scale: 0.25,
            },
        );
        let error = result
            .expect_err("Two actual active surfaces cannot publish conflicting contact aliases");
        assert!(
            error.contains("disagree") && error.contains("contact.near"),
            "Wrong rejection: {error}"
        );
    }
}

#[test]
fn missing_gear_cannot_activate_an_action_local_hand_replacement_in_bind() {
    use horde_core::weapons::WeaponClass;
    let source = builtin().unwrap();
    for id in ["rook", "june"] {
        let mut definition = source.definition.clone();
        let missing = definition
            .model(id)
            .unwrap()
            .parts
            .iter()
            .find(|part| part.slot == "accessory")
            .unwrap()
            .id
            .clone();
        let cel = definition
            .attachments
            .iter_mut()
            .find(|a| {
                a.model == id
                    && a.slot == "hands"
                    && a.replace_parts.iter().any(|part| part == "hand.near.palm")
            })
            .unwrap();
        let cel_id = cel.id.clone();
        cel.activation.kind = attachments::ActivationKind::Revealed;
        cel.activation.missing_parts = vec![missing];
        // Either reject this contradictory authoring combination, or enforce
        // the action-only contract during resolution. Bind may never activate it.
        let Ok(catalogue) = recompile_review_definition(definition) else {
            continue;
        };
        let equipment =
            verification::review_equipment(Some(WeaponClass::Pistol), Some("armor.cloth"), None)
                .unwrap();
        let pose = resolve_equipped_model(
            &catalogue,
            id,
            &equipment,
            PlayerPoseSample {
                action: PlayerAction::Bind,
                ..held_sample([1500., 550.])
            },
            ModelPlacement {
                source_origin: [900., 700.],
                scale: 0.25,
            },
        )
        .unwrap();
        assert!(
            pose.packets
                .iter()
                .any(|packet| packet.part_id == "hand.near.palm"),
            "Missing gear cannot suppress the source palm during Bind: {id}"
        );
        assert!(
            !pose.packets.iter().any(|packet| packet.part_id == cel_id),
            "Action-local cel escaped activation into Bind: {id}"
        );
    }
}
