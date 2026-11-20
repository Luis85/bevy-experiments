use super::*;

const MANIFEST: &[u8] = include_bytes!("../assets/wildlife-pack/pack.json");
const DEER: &[u8] = include_bytes!("../assets/wildlife-pack/deer.glb");

fn sample() -> (AssetPackManifest, Vec<u8>) {
    let mut manifest: AssetPackManifest = serde_json::from_slice(MANIFEST).unwrap();
    manifest.models.truncate(1);
    manifest.bindings.truncate(1);
    (manifest, DEER.to_vec())
}
fn load(manifest: &AssetPackManifest, bytes: &[u8]) -> Result<ValidatedAssetPack, String> {
    ValidatedAssetPack::from_bytes(&serde_json::to_vec(manifest).unwrap(), |_| {
        Ok(bytes.to_vec())
    })
}
fn mutate_glb(bytes: &[u8], mutation: impl FnOnce(&mut Value)) -> Vec<u8> {
    mutate_glb_binary(bytes, |json, _| mutation(json))
}
fn mutate_glb_binary(bytes: &[u8], mutation: impl FnOnce(&mut Value, &mut Vec<u8>)) -> Vec<u8> {
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let mut json: Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let mut bin = bytes[28 + json_len..].to_vec();
    mutation(&mut json, &mut bin);
    json["buffers"][0]["byteLength"] = Value::from(bin.len());
    let mut encoded = serde_json::to_vec(&json).unwrap();
    while !encoded.len().is_multiple_of(4) {
        encoded.push(b' ');
    }
    let mut out = b"glTF".to_vec();
    out.extend(2_u32.to_le_bytes());
    out.extend(((28 + encoded.len() + bin.len()) as u32).to_le_bytes());
    out.extend((encoded.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(encoded);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}
fn mutated_error(mutation: impl FnOnce(&mut Value)) -> String {
    let (mut manifest, bytes) = sample();
    let bytes = mutate_glb(&bytes, mutation);
    manifest.models[0].sha256 = asset_sha256(&bytes);
    load(&manifest, &bytes).unwrap_err()
}

fn with_embedded_png(bytes: &[u8], png: &[u8]) -> Vec<u8> {
    mutate_glb_binary(bytes, |json, bin| {
        let offset = bin.len();
        bin.extend_from_slice(png);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let view = json["bufferViews"].as_array().unwrap().len();
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":offset,"byteLength":png.len()}));
        // Unused images are still part of the imported model and its budgets.
        json["images"] = serde_json::json!([{"bufferView":view,"mimeType":"image/png"}]);
    })
}

#[test]
fn embedded_png_payloads_are_decoded_and_corruption_is_rejected_in_preflight() {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        3,
        image::Rgba([10, 20, 30, 255]),
    ))
    .write_to(&mut output, ImageFormat::Png)
    .unwrap();
    let png = output.into_inner();
    let mut corrupt = png.clone();
    let idat = corrupt
        .windows(4)
        .position(|bytes| bytes == b"IDAT")
        .unwrap();
    corrupt[idat + 4] ^= 0xff;

    for (image, expected) in [
        (png.as_slice(), None),
        (&png[..24], Some("failed bounded decoding")),
        (corrupt.as_slice(), Some("failed bounded decoding")),
    ] {
        let (mut manifest, bytes) = sample();
        let bytes = with_embedded_png(&bytes, image);
        manifest.models[0].sha256 = asset_sha256(&bytes);
        let result = load(&manifest, &bytes);
        if let Some(message) = expected {
            let error = result.unwrap_err();
            assert!(error.contains(message), "{error}");
        } else {
            assert_eq!(result.unwrap().models["deer"].budget.texture_pixels, 6);
        }
    }

    // Dimensions are rejected before allocating the declared pixel buffer.
    let mut oversized = png;
    oversized[16..20].copy_from_slice(&4097_u32.to_be_bytes());
    let (mut manifest, bytes) = sample();
    let bytes = with_embedded_png(&bytes, &oversized);
    manifest.models[0].sha256 = asset_sha256(&bytes);
    assert!(
        load(&manifest, &bytes)
            .unwrap_err()
            .contains("Texture dimensions")
    );
}

#[test]
fn renderer_budgets_count_texture_slot_aliases_and_generated_flat_vertices() {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        3,
        image::Rgba([1, 2, 3, 255]),
    ))
    .write_to(&mut output, ImageFormat::Png)
    .unwrap();
    let (mut manifest, bytes) = sample();
    let bytes = with_embedded_png(&bytes, &output.into_inner());
    let bytes = mutate_glb(&bytes, |json| {
        json["textures"] = serde_json::json!([{"source":0},{"source":0},{"source":0}]);
        for mesh in json["meshes"].as_array_mut().unwrap() {
            for primitive in mesh["primitives"].as_array_mut().unwrap() {
                primitive["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("NORMAL");
            }
        }
    });
    manifest.models[0].sha256 = asset_sha256(&bytes);
    let pack = load(&manifest, &bytes).unwrap();
    let budget = &pack.models["deer"].budget;
    assert_eq!(budget.texture_pixels, 6);
    assert_eq!(budget.texture_slot_pixels, 18);
    assert_eq!(budget.renderer_vertices, budget.triangles * 3);
    assert_eq!(budget.scene_renderer_vertices, budget.scene_triangles * 3);
}

#[test]
fn undeclared_extension_payloads_are_rejected_but_extras_remain_opaque() {
    for target in ["root", "node", "material"] {
        let error = mutated_error(|json| {
            let payload = serde_json::json!({"KHR_materials_unlit":{}});
            match target {
                "root" => json["extensions"] = payload,
                "node" => json["nodes"][0]["extensions"] = payload,
                _ => json["materials"][0]["extensions"] = payload,
            }
        });
        assert!(error.contains("including undeclared extensions"), "{error}");
    }
    let (mut manifest, bytes) = sample();
    let bytes = mutate_glb(&bytes, |json| {
        json["nodes"][0]["extras"] = serde_json::json!({
            "extensions":{"studio-authoring":{"note":"opaque metadata"}}
        });
        json["materials"][0]["extensions"] = serde_json::json!({});
    });
    manifest.models[0].sha256 = asset_sha256(&bytes);
    load(&manifest, &bytes).unwrap();
}

#[test]
fn repeated_mesh_nodes_count_toward_instantiated_geometry_budgets() {
    for (copies, primitives, index_count, accepted) in [
        (2, 2, None, true),
        (16, 20, None, false),
        (50, 1, Some(7200), false),
    ] {
        let (mut manifest, bytes) = sample();
        let bytes = mutate_glb_binary(&bytes, |json, bin| {
            let mut primitive = json["meshes"][0]["primitives"][0].clone();
            if let Some(count) = index_count {
                let offset = bin.len();
                for i in 0..count {
                    bin.extend(((i % 3) as u16).to_le_bytes());
                }
                let view = json["bufferViews"].as_array().unwrap().len();
                json["bufferViews"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"buffer":0,"byteOffset":offset,"byteLength":count*2}));
                let accessor = json["accessors"].as_array().unwrap().len();
                json["accessors"].as_array_mut().unwrap().push(
                    serde_json::json!({"bufferView":view,"componentType":5123,"count":count,"type":"SCALAR"}),
                );
                primitive["indices"] = Value::from(accessor);
            }
            json["meshes"][0]["primitives"] = Value::from(vec![primitive; primitives]);
            for i in 0..copies {
                let node = json["nodes"].as_array().unwrap().len();
                json["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"name":format!("reused-mesh-{i}"),"mesh":0}));
                json["nodes"][0]["children"]
                    .as_array_mut()
                    .unwrap()
                    .push(Value::from(node));
            }
        });
        manifest.models[0].sha256 = asset_sha256(&bytes);
        let result = load(&manifest, &bytes);
        if accepted {
            let pack = result.unwrap();
            let budget = &pack.models["deer"].budget;
            assert!(budget.scene_vertices > budget.vertices);
            assert!(budget.scene_triangles > budget.triangles);
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("Instantiated scene"), "{error}");
        }
    }
}

#[test]
fn bundled_wildlife_is_valid_and_content_identified() {
    let manifest: AssetPackManifest = serde_json::from_slice(MANIFEST).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wildlife-pack");
    let pack = ValidatedAssetPack::from_bytes(MANIFEST, |file| {
        fs::read(root.join(file)).map_err(|e| e.to_string())
    })
    .unwrap();
    assert_eq!(pack.models.len(), 5);
    assert_eq!(pack.identity.len(), 71);
    for model in pack.models.values() {
        assert_eq!(model.budget.clips, 5);
        assert!(model.budget.vertices > 1000 && model.budget.vertices < 20_000);
        for name in ["idle", "walk", "run", "feed", "drink"] {
            assert!(model.clips.contains_key(name));
        }
    }
    assert_eq!(pack.manifest.bindings.len(), manifest.bindings.len());
}
#[test]
fn model_bytes_are_copied_and_hash_mismatch_rejected() {
    let (manifest, mut bytes) = sample();
    let pack = load(&manifest, &bytes).unwrap();
    bytes[40] ^= 1;
    assert_eq!(pack.models["deer"].bytes, DEER);
    assert!(load(&manifest, &bytes).unwrap_err().contains("SHA-256"));
}
#[test]
fn same_model_can_have_distinct_binding_animation_policies() {
    let (mut manifest, bytes) = sample();
    let mut other = manifest.bindings[0].clone();
    other.target = PackTarget::Species(AnimalSpecies::Ibex);
    other.animation.as_mut().unwrap().walk_speed_m_s = 2.1;
    manifest.bindings.push(other);
    let pack = load(&manifest, &bytes).unwrap();
    assert_ne!(
        pack.manifest.bindings[0].animation,
        pack.manifest.bindings[1].animation
    );
    let identity = pack.identity.clone();
    manifest.bindings[1]
        .animation
        .as_mut()
        .unwrap()
        .blend_seconds = 0.6;
    assert_ne!(load(&manifest, &bytes).unwrap().identity, identity);
}
#[test]
fn importer_rejects_cycles_multiple_parents_and_orphan_nodes() {
    assert!(
        mutated_error(|json| json["nodes"][1]["children"]
            .as_array_mut()
            .unwrap()
            .push(Value::from(0)))
        .contains("cyclic")
    );
    assert!(
        mutated_error(|json| json["nodes"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(Value::from(2)))
        .contains("multiple parents")
    );
    assert!(
        mutated_error(|json| {
            json["nodes"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"name":"unused"}));
        })
        .contains("reachable")
    );
}
#[test]
fn all_unused_animation_samplers_count_toward_allocation_budget() {
    let error = mutated_error(|json| {
        let mut unused = json["animations"][0].clone();
        let sampler = unused["samplers"][0].clone();
        unused["samplers"]
            .as_array_mut()
            .unwrap()
            .resize(512, sampler);
        for n in 0..8 {
            unused["name"] = Value::from(format!("unused-{n}"));
            json["animations"]
                .as_array_mut()
                .unwrap()
                .push(unused.clone());
        }
    });
    assert!(error.contains("including unused"), "{error}");
}
#[test]
fn unused_aliased_accessors_are_bounded_before_binary_scanning() {
    let error = mutated_error(|json| {
        let length = json["buffers"][0]["byteLength"].as_u64().unwrap();
        let view = json["bufferViews"].as_array().unwrap().len();
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":0,"byteLength":length}));
        let accessor = serde_json::json!({"bufferView":view,"componentType":5126,"count":length/4,"type":"SCALAR"});
        json["accessors"]
            .as_array_mut()
            .unwrap()
            .resize(2048, accessor);
    });
    assert!(
        error.contains("Aggregate accessor scalar budget"),
        "{error}"
    );
}

#[test]
fn skinned_vertices_require_valid_joint_indices_and_normalized_nonzero_weights() {
    for (joint, weight, expected) in [
        (1_u16, 1_f32, Some("JOINTS_0")),
        (0, 0., Some("zero-weight")),
        (0, 1., None),
    ] {
        let (mut manifest, bytes) = sample();
        let bytes = mutate_glb_binary(&bytes, |json, bin| {
            let count = json["accessors"][0]["count"].as_u64().unwrap() as usize;
            let joint_start = bin.len();
            for _ in 0..count {
                for _ in 0..4 {
                    bin.extend(joint.to_le_bytes());
                }
            }
            let weight_start = bin.len();
            for _ in 0..count {
                for value in [weight, 0., 0., 0.] {
                    bin.extend(value.to_le_bytes());
                }
            }
            let joint_view = json["bufferViews"].as_array().unwrap().len();
            json["bufferViews"].as_array_mut().unwrap().push(
                serde_json::json!({"buffer":0,"byteOffset":joint_start,"byteLength":count*8}),
            );
            let weight_view = json["bufferViews"].as_array().unwrap().len();
            json["bufferViews"].as_array_mut().unwrap().push(
                serde_json::json!({"buffer":0,"byteOffset":weight_start,"byteLength":count*16}),
            );
            let joints = json["accessors"].as_array().unwrap().len();
            json["accessors"].as_array_mut().unwrap().push(serde_json::json!({"bufferView":joint_view,"componentType":5123,"count":count,"type":"VEC4"}));
            let weights = json["accessors"].as_array().unwrap().len();
            json["accessors"].as_array_mut().unwrap().push(serde_json::json!({"bufferView":weight_view,"componentType":5126,"count":count,"type":"VEC4"}));
            json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = Value::from(joints);
            json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = Value::from(weights);
            json["skins"] = serde_json::json!([{"joints":[0]}]);
            let mesh_node = json["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|n| n.get("mesh") == Some(&Value::from(0)))
                .unwrap();
            mesh_node["skin"] = Value::from(0);
        });
        manifest.models[0].sha256 = asset_sha256(&bytes);
        let result = load(&manifest, &bytes);
        if let Some(message) = expected {
            let error = result.unwrap_err();
            assert!(error.contains(message), "{error}");
        } else {
            result.unwrap();
        }
    }
}
#[test]
fn external_resources_traversal_missing_clips_and_invalid_accessors_fail_closed() {
    let (mut manifest, bytes) = sample();
    manifest.models[0].file = "../escape.glb".into();
    assert!(
        ValidatedAssetPack::from_bytes(&serde_json::to_vec(&manifest).unwrap(), |_| panic!(
            "unsafe path reached callback"
        ))
        .is_err()
    );
    let (mut manifest, _) = sample();
    manifest.bindings[0].animation.as_mut().unwrap().walk = "absent".into();
    assert!(
        load(&manifest, &bytes)
            .unwrap_err()
            .contains("missing animation")
    );
    assert!(
        mutated_error(
            |json| json["buffers"][0]["uri"] = Value::from("https://example.invalid/model.bin")
        )
        .contains("External")
    );
    assert!(
        mutated_error(|json| json["accessors"][0]["count"] = Value::from(100_000_000_u64))
            .contains("allocation")
    );
}
#[test]
fn duplicate_targets_and_oversized_input_are_rejected_before_install() {
    let (mut manifest, bytes) = sample();
    manifest.bindings.push(manifest.bindings[0].clone());
    assert!(
        load(&manifest, &bytes)
            .unwrap_err()
            .contains("bound only once")
    );
    assert!(
        ValidatedAssetPack::from_bytes(&vec![b' '; MAX_MANIFEST_BYTES + 1], |_| unreachable!())
            .is_err()
    );
}

#[test]
fn materials_meshless_scenes_and_composed_transforms_are_rejected_before_loading() {
    assert!(
        mutated_error(
            |json| json["materials"][0]["pbrMetallicRoughness"]["roughnessFactor"] =
                Value::from(-10.)
        )
        .contains("roughness")
    );
    assert!(
        mutated_error(
            |json| json["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
                serde_json::json!([1e30, 0., 0., 1.])
        )
        .contains("Material color")
    );
    assert!(
        mutated_error(|json| for node in json["nodes"].as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("mesh");
        })
        .contains("no drawable mesh")
    );
    assert!(
        mutated_error(|json| {
            let empty = json["meshes"].as_array().unwrap().len();
            json["meshes"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"primitives":[]}));
            for node in json["nodes"].as_array_mut().unwrap() {
                if node.get("mesh").is_some() {
                    node["mesh"] = Value::from(empty);
                }
            }
        })
        .contains("no drawable mesh")
    );
    assert!(
        mutated_error(|json| {
            json["nodes"][0]["scale"] = serde_json::json!([100., 100., 100.]);
            json["nodes"][1]["scale"] = serde_json::json!([100., 100., 100.]);
        })
        .contains("Composed hierarchy")
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wildlife-pack");
    assert!(
        ValidatedAssetPack::read(directory)
            .unwrap_err()
            .contains("regular file")
    );
}

#[test]
fn scale_animation_cannot_cross_zero_during_keys_or_clip_blends() {
    let (mut manifest, bytes) = sample();
    let bytes = mutate_glb_binary(&bytes, |json, bin| {
        let channel = json["animations"][0]["channels"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c["target"]["path"] == "translation")
            .unwrap();
        let sampler = json["animations"][0]["channels"][channel]["sampler"]
            .as_u64()
            .unwrap() as usize;
        let output = json["animations"][0]["samplers"][sampler]["output"]
            .as_u64()
            .unwrap() as usize;
        let view = json["accessors"][output]["bufferView"].as_u64().unwrap() as usize;
        let offset = json["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize;
        let count = json["accessors"][output]["count"].as_u64().unwrap() as usize;
        json["animations"][0]["channels"][channel]["target"]["path"] = Value::from("scale");
        for i in 0..count {
            for axis in 0..3 {
                let value: f32 = if axis == 0 && i < count / 2 { -1. } else { 1. };
                let start = offset + i * 12 + axis * 4;
                bin[start..start + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    });
    manifest.models[0].sha256 = asset_sha256(&bytes);
    assert!(
        load(&manifest, &bytes)
            .unwrap_err()
            .contains("rest-pose sign")
    );
}

fn human_sample() -> (AssetPackManifest, Vec<u8>) {
    (
        serde_json::from_slice(include_bytes!("../assets/character-pack/pack.json")).unwrap(),
        include_bytes!("../assets/character-pack/fieldhand.glb").to_vec(),
    )
}

#[test]
fn character_example_preserves_bounded_customization_and_named_postures() {
    let (manifest, bytes) = human_sample();
    let pack = load(&manifest, &bytes).unwrap();
    let model = &pack.models["fieldhand"];
    assert_eq!(
        model.clips.keys().map(String::as_str).collect::<Vec<_>>(),
        ["idle", "run", "seated", "walk"]
    );
    assert!(model.budget.nodes < 64 && model.budget.triangles < 3000);
    let human = manifest.bindings[0]
        .animation
        .as_ref()
        .unwrap()
        .human
        .as_ref()
        .unwrap();
    for name in human.material_names() {
        let role = model
            .materials
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap();
        assert!(role.customizable && role.scene_primitives > 0);
    }
    assert!(
        model
            .clips
            .values()
            .all(|clip| clip.channels.iter().all(|channel| channel.node != 0)),
        "no actor-root motion"
    );
}

#[test]
fn character_policy_rejects_wrong_targets_missing_clips_and_ambiguous_material_roles() {
    let (manifest, bytes) = human_sample();
    let mut wrong = manifest.clone();
    wrong.bindings[0].target = PackTarget::Kind(ActorKind::Animal);
    assert!(load(&wrong, &bytes).unwrap_err().contains("human kind"));
    let mut missing = manifest.clone();
    missing.bindings[0]
        .animation
        .as_mut()
        .unwrap()
        .human
        .as_mut()
        .unwrap()
        .seated = "absent".into();
    assert!(load(&missing, &bytes).unwrap_err().contains("seated"));
    let mut aliased = manifest.clone();
    let policy = aliased.bindings[0]
        .animation
        .as_mut()
        .unwrap()
        .human
        .as_mut()
        .unwrap();
    policy.hair_material = policy.skin_material.clone();
    assert!(load(&aliased, &bytes).unwrap_err().contains("distinct"));
    let mut absent = manifest.clone();
    absent.bindings[0]
        .animation
        .as_mut()
        .unwrap()
        .human
        .as_mut()
        .unwrap()
        .hair_material = "absent".into();
    assert!(
        load(&absent, &bytes)
            .unwrap_err()
            .contains("uniquely named")
    );
}

#[test]
fn character_customization_rejects_unused_duplicate_translucent_and_precolored_materials() {
    let (manifest, bytes) = human_sample();
    for which in 0..5 {
        let changed = mutate_glb(&bytes, |json| match which {
            0 => {
                json["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
                    serde_json::json!([0.5, 1., 1., 1.]);
            }
            1 => {
                json["materials"][0]["alphaMode"] = "BLEND".into();
            }
            2 => {
                let mut duplicate = json["materials"][0].clone();
                duplicate["name"] = "character_skin".into();
                json["materials"].as_array_mut().unwrap().push(duplicate);
            }
            3 => {
                json["materials"][0]["emissiveFactor"] = serde_json::json!([1., 0., 0.]);
            }
            _ => {
                for mesh in json["meshes"].as_array_mut().unwrap() {
                    for p in mesh["primitives"].as_array_mut().unwrap() {
                        if p["material"] == 0 {
                            p["material"] = 3.into();
                        }
                    }
                }
            }
        });
        let mut candidate = manifest.clone();
        candidate.models[0].sha256 = asset_sha256(&changed);
        assert!(
            load(&candidate, &changed)
                .unwrap_err()
                .contains("opaque white"),
            "case {which}"
        );
    }
}

#[test]
fn character_rejects_reflected_ancestry_and_negative_scales_in_unselected_clips() {
    let (manifest, bytes) = human_sample();
    let changed = mutate_glb(&bytes, |json| {
        json["nodes"][0]["scale"] = serde_json::json!([-1., 1., 1.]);
    });
    let mut candidate = manifest.clone();
    candidate.models[0].sha256 = asset_sha256(&changed);
    assert!(
        load(&candidate, &changed)
            .unwrap_err()
            .contains("positive scales")
    );
    let changed = mutate_glb_binary(&bytes, |json, bin| {
        let values = [-1_f32, 1., 1., -1., 1., 1.];
        let offset = bin.len();
        for value in values {
            bin.extend(value.to_le_bytes());
        }
        let view = json["bufferViews"].as_array().unwrap().len();
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":offset,"byteLength":24}));
        let accessor = json["accessors"].as_array().unwrap().len();
        json["accessors"].as_array_mut().unwrap().push(
            serde_json::json!({"bufferView":view,"componentType":5126,"count":2,"type":"VEC3"}),
        );
        let times_offset = bin.len();
        bin.extend(0_f32.to_le_bytes());
        bin.extend(1_f32.to_le_bytes());
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"buffer":0,"byteOffset":times_offset,"byteLength":8}));
        json["accessors"].as_array_mut().unwrap().push(serde_json::json!({"bufferView":view+1,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]}));
        json["animations"].as_array_mut().unwrap().push(serde_json::json!({"name":"unused_reflection","channels":[{"sampler":0,"target":{"node":1,"path":"scale"}}],"samplers":[{"input":accessor+1,"output":accessor,"interpolation":"LINEAR"}]}));
    });
    candidate.models[0].sha256 = asset_sha256(&changed);
    let error = load(&candidate, &changed).unwrap_err();
    assert!(error.contains("rest-pose sign"), "{error}");
}

#[test]
fn driver_seat_is_explicit_bounded_model_policy_and_changes_pack_identity() {
    let mut manifest: AssetPackManifest =
        serde_json::from_slice(include_bytes!("../assets/transport-pack/pack.json")).unwrap();
    let bytes = include_bytes!("../assets/transport-pack/comet.glb");
    let original = load(&manifest, bytes).unwrap();
    manifest.bindings[0].driver_seat = Some(DriverSeatPolicy {
        translation_m: [-0.42, 0.72, -0.24],
    });
    let seated = load(&manifest, bytes).unwrap();
    assert_ne!(original.identity(), seated.identity());
    manifest.bindings[0]
        .driver_seat
        .as_mut()
        .unwrap()
        .translation_m[0] = 11.;
    assert!(load(&manifest, bytes).unwrap_err().contains("driver seat"));
    manifest.bindings[0]
        .driver_seat
        .as_mut()
        .unwrap()
        .translation_m[0] = 0.;
    manifest.bindings[0].target = PackTarget::Kind(ActorKind::Vehicle);
    assert!(
        load(&manifest, bytes)
            .unwrap_err()
            .contains("vehicle-model target")
    );
}
