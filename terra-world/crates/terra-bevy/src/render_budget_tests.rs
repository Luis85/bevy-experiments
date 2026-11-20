use super::*;

#[test]
fn wheel_rig_work_is_reserved_in_addition_to_preview_clip_channels() {
    let pack = crate::asset_pack::ValidatedAssetPack::from_bytes(
        include_bytes!("../assets/transport-pack/pack.json"),
        |_| Ok(include_bytes!("../assets/transport-pack/comet.glb").to_vec()),
    )
    .unwrap();
    let binding = &pack.manifest.bindings[0];
    let model = pack.models[&binding.model].clone();
    let mut appearance = ActorSceneAppearance::new(Handle::default());
    appearance.model = Some(super::super::pack::ActorSceneModel {
        model: model.clone(),
        path: "test://comet.glb".into(),
    });
    appearance.animation = Some(super::super::pack::ActorSceneAnimation {
        model: model.clone(),
        policy: binding.animation.clone().unwrap(),
        scale: 1.,
    });
    let cost = instance_cost(&appearance);
    assert_eq!(cost.animation_nodes, model.nodes.len());
    assert_eq!(cost.animation_channels, 5 + 4 + 2);
}
use bevy::asset::{
    AssetApp, AssetLoader, AssetPlugin, LoadContext,
    io::{
        AssetSourceBuilder, Reader,
        memory::{Dir, MemoryAssetReader},
    },
};
use bevy::gltf::GltfLoaderSettings;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

/// Exercise the real asynchronous asset lifetime without a GPU/glTF backend.
/// A separately retained material label reproduces a dependency outliving its
/// scene; new admission must wait for that actual asset-server lifetime.
#[derive(TypePath)]
struct ProbeLoader {
    retained: Arc<Mutex<Vec<Handle<WorldAsset>>>>,
}
impl AssetLoader for ProbeLoader {
    type Asset = WorldAsset;
    type Settings = GltfLoaderSettings;
    type Error = std::io::Error;
    async fn load(
        &self,
        _: &mut dyn Reader,
        _: &Self::Settings,
        context: &mut LoadContext<'_>,
    ) -> Result<WorldAsset, Self::Error> {
        let material = context.add_labeled_asset("Material0/std", WorldAsset::new(World::new()));
        self.retained.lock().unwrap().push(material);
        let mut scene = World::new();
        scene.spawn((Mesh3d::default(), Transform::default(), Visibility::Visible));
        context.add_labeled_asset("Scene0", WorldAsset::new(scene));
        Ok(WorldAsset::new(World::new()))
    }
    fn extensions(&self) -> &[&str] {
        &["glb"]
    }
}

fn fixture() -> (
    App,
    Vec<ActorSceneAppearance>,
    Arc<Mutex<Vec<Handle<WorldAsset>>>>,
) {
    let pack = super::super::pack::bundled_wildlife_pack().unwrap();
    let directory = Dir::default();
    let mut appearances = Vec::new();
    for binding in &pack.manifest.bindings {
        let model = pack.models[&binding.model].clone();
        directory.insert_asset(
            Path::new(&format!("{}.glb", model.hash)),
            b"test source".to_vec(),
        );
        let mut appearance = ActorSceneAppearance::new(Handle::default());
        appearance.model = Some(ActorSceneModel {
            path: format!("budget://{}.glb", model.hash),
            model: model.clone(),
        });
        appearance.animation =
            binding
                .animation
                .clone()
                .map(|policy| super::super::pack::ActorSceneAnimation {
                    model,
                    policy,
                    scale: 1.,
                });
        appearances.push(appearance);
    }
    let retained = Arc::new(Mutex::new(Vec::new()));
    let mut app = App::new();
    app.register_asset_source(
        "budget",
        AssetSourceBuilder::new(move || {
            Box::new(MemoryAssetReader {
                root: directory.clone(),
            })
        }),
    )
    .add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::world_serialization::WorldSerializationPlugin,
    ))
    .register_asset_loader(ProbeLoader {
        retained: retained.clone(),
    });
    (app, appearances, retained)
}
fn candidate(id: u32, priority: u8, distance: f32, appearance: &ActorSceneAppearance) -> Candidate {
    Candidate {
        entity: Entity::from_raw_u32(id).unwrap(),
        body_id: u64::from(id),
        priority,
        distance_m: distance,
        visible: true,
        appearance: appearance.clone(),
    }
}
fn limits(instances: usize) -> ActorSceneLimits {
    let mut limits = ActorSceneLimits::for_quality(RenderQuality::High);
    limits.instances = instances;
    limits
}
fn tick(app: &mut App) {
    app.update();
    std::thread::sleep(Duration::from_millis(1));
}

#[test]
fn stable_selection_prioritizes_active_owned_then_nearby_and_budget_increases() {
    let (app, models, _) = fixture();
    let server = app.world().resource::<AssetServer>();
    let mut admission = ActorSceneAdmission::default();
    let make = || {
        vec![
            candidate(3, 2, 1., &models[0]),
            candidate(2, 1, 400., &models[0]),
            candidate(1, 0, 600., &models[0]),
        ]
    };
    let chosen = admission.select(make(), &BTreeMap::new(), limits(2), server);
    assert_eq!(
        chosen
            .keys()
            .map(|e| e.index_u32())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2])
    );
    assert_eq!(admission.diagnostics.reserved.resident_models, 1);
    assert_eq!(
        admission.diagnostics.selected.vertices,
        models[0]
            .model
            .as_ref()
            .unwrap()
            .model
            .budget
            .scene_vertices
            * 2
    );
    let chosen = admission.select(make(), &BTreeMap::new(), limits(3), server);
    assert_eq!(chosen.len(), 3);
    assert_eq!(
        admission.diagnostics.reserved.resident_models, 1,
        "shared models must be charged once"
    );

    let mut admission = ActorSceneAdmission::default();
    let chosen = admission.select(
        vec![
            candidate(4, 2, 99., &models[0]),
            candidate(5, 2, 101., &models[0]),
        ],
        &BTreeMap::new(),
        limits(1),
        server,
    );
    assert!(chosen.contains_key(&Entity::from_raw_u32(4).unwrap()));
    let chosen = admission.select(
        vec![
            candidate(5, 2, 99., &models[0]),
            candidate(4, 2, 101., &models[0]),
        ],
        &BTreeMap::new(),
        limits(1),
        server,
    );
    assert!(
        chosen.contains_key(&Entity::from_raw_u32(4).unwrap()),
        "small distance changes must not churn instances"
    );
}

#[test]
fn hidden_distant_and_animation_or_geometry_excess_keep_procedural_eligibility() {
    let (app, models, _) = fixture();
    let server = app.world().resource::<AssetServer>();
    let mut admission = ActorSceneAdmission::default();
    let mut hidden = candidate(1, 0, 1., &models[0]);
    hidden.visible = false;
    let chosen = admission.select(
        vec![
            hidden,
            candidate(2, 1, 3000., &models[0]),
            candidate(3, 2, 1., &models[0]),
        ],
        &BTreeMap::new(),
        limits(3),
        server,
    );
    assert_eq!(chosen.len(), 1);
    assert_eq!(admission.diagnostics.hidden_instances, 1);
    assert_eq!(admission.diagnostics.distant_instances, 1);
    for kind in ["vertices", "channels", "nodes", "primitives", "bytes"] {
        let mut limit = limits(10);
        match kind {
            "vertices" => limit.vertices = 1,
            "channels" => limit.animation_channels = 0,
            "nodes" => limit.animation_nodes = 0,
            "primitives" => limit.primitives = 0,
            _ => limit.estimated_bytes = 1,
        }
        let chosen = admission.select(
            vec![candidate(3, 2, 1., &models[0])],
            &BTreeMap::new(),
            limit,
            server,
        );
        assert!(chosen.is_empty(), "{kind} limit must degrade the scene");
    }
}

#[test]
fn current_scene_teardown_is_charged_before_a_priority_replacement() {
    let (app, models, _) = fixture();
    let server = app.world().resource::<AssetServer>();
    let mut admission = ActorSceneAdmission::default();
    let current = admission.select(
        vec![candidate(1, 2, 1., &models[0])],
        &BTreeMap::new(),
        limits(1),
        server,
    );
    assert_eq!(current.len(), 1);
    let selected = admission.select(
        vec![
            candidate(2, 0, 1., &models[0]),
            candidate(1, 2, 1., &models[0]),
        ],
        &current,
        limits(1),
        server,
    );
    assert!(
        selected.is_empty(),
        "old instance must retire before a new one consumes its slot"
    );
    assert_eq!(admission.diagnostics.reserved.instances, 1);
    let selected = admission.select(
        vec![candidate(2, 0, 1., &models[0])],
        &BTreeMap::new(),
        limits(1),
        server,
    );
    assert_eq!(selected.len(), 1);
}

#[test]
fn missing_lazy_model_is_retired_once_and_does_not_retry_every_frame() {
    let (mut app, mut models, _) = fixture();
    models[0].model.as_mut().unwrap().path = "budget://missing.glb".into();
    let mut admission = ActorSceneAdmission::default();
    for _ in 0..200 {
        drop(admission.select(
            vec![candidate(1, 0, 1., &models[0])],
            &BTreeMap::new(),
            limits(1),
            app.world().resource::<AssetServer>(),
        ));
        tick(&mut app);
        if !admission.failed_models.is_empty() && admission.models.is_empty() {
            break;
        }
    }
    assert_eq!(admission.failed_models.len(), 1);
    assert!(admission.models.is_empty());
    for _ in 0..4 {
        assert!(
            admission
                .select(
                    vec![candidate(1, 0, 1., &models[0])],
                    &BTreeMap::new(),
                    limits(2),
                    app.world().resource::<AssetServer>()
                )
                .is_empty()
        );
    }
    assert_eq!(admission.diagnostics.deferred_instances, 1);
    assert_eq!(admission.diagnostics.reserved.resident_models, 0);
}

#[test]
fn retained_subassets_and_loader_transitions_cannot_evade_residency_limits() {
    let (mut app, models, retained) = fixture();
    let mut admission = ActorSceneAdmission::default();
    let mut limit = limits(1);
    limit.estimated_bytes = models[..2]
        .iter()
        .map(|m| {
            model_cost(m.model.as_ref().unwrap())
                .plus(instance_cost(m))
                .estimated_bytes
        })
        .max()
        .unwrap();
    for _ in 0..200 {
        drop(admission.select(
            vec![candidate(1, 2, 1., &models[0])],
            &BTreeMap::new(),
            limit.clone(),
            app.world().resource::<AssetServer>(),
        ));
        tick(&mut app);
        if !retained.lock().unwrap().is_empty()
            && admission
                .models
                .values()
                .all(|r| !r.loading(app.world().resource::<AssetServer>()))
        {
            break;
        }
    }
    assert_eq!(retained.lock().unwrap().len(), 1);
    for _ in 0..12 {
        let chosen = admission.select(
            vec![candidate(2, 0, 1., &models[1])],
            &BTreeMap::new(),
            limit.clone(),
            app.world().resource::<AssetServer>(),
        );
        assert!(chosen.is_empty());
        assert_eq!(admission.diagnostics.reserved.resident_models, 1);
        assert!(admission.diagnostics.reserved.estimated_bytes <= limit.estimated_bytes);
        tick(&mut app);
    }
    assert_eq!(
        retained.lock().unwrap().len(),
        1,
        "the second load cannot start while the old material is retained"
    );
    retained.lock().unwrap().clear();
    for _ in 0..200 {
        drop(admission.select(
            vec![candidate(2, 0, 1., &models[1])],
            &BTreeMap::new(),
            limit.clone(),
            app.world().resource::<AssetServer>(),
        ));
        tick(&mut app);
        if admission
            .models
            .contains_key(models[1].model.as_ref().unwrap().key())
        {
            break;
        }
    }
    assert!(
        admission
            .models
            .contains_key(models[1].model.as_ref().unwrap().key())
    );
    assert_eq!(admission.models.len(), 1);
    assert!(admission.diagnostics.peak_reserved_estimated_bytes <= limit.estimated_bytes);
}

#[test]
fn actual_scene_budget_degradation_restores_mesh_without_changing_body_or_pose() {
    use super::super::scenes::*;
    let (mut app, mut models, retained) = fixture();
    models[0].animation = None;
    app.register_type::<Mesh3d>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<bevy::transform::components::TransformTreeChanged>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .init_resource::<ActorSceneOverrides>()
        .init_resource::<ActorSceneAdmission>()
        .insert_resource(ActorSceneBudget {
            limits: Some(limits(1)),
        })
        .add_observer(scene_ready)
        .add_systems(Update, sync_actor_scenes);
    app.world_mut()
        .resource_mut::<ActorSceneOverrides>()
        .set_kind(terra_core::ActorKind::Human, models[0].clone())
        .unwrap();
    let mut roots = Vec::new();
    for id in 1..=3 {
        roots.push(
            app.world_mut()
                .spawn((
                    ActorSceneAnchor {
                        body_id: id,
                        kind: terra_core::ActorKind::Human,
                        species: None,
                    },
                    ProceduralActorMesh {
                        mesh: Handle::default(),
                        material: Handle::default(),
                    },
                    Mesh3d::default(),
                    MeshMaterial3d::<StandardMaterial>::default(),
                    Transform::from_xyz(id as f32, 40., 3.),
                    Visibility::Visible,
                ))
                .id(),
        );
    }
    for _ in 0..200 {
        tick(&mut app);
        if app.world().get::<ActorSceneActive>(roots[0]).is_some() {
            break;
        }
    }
    assert!(app.world().get::<ActorSceneActive>(roots[0]).is_some());
    assert!(
        roots[1..]
            .iter()
            .all(|e| app.world().get::<Mesh3d>(*e).is_some())
    );
    app.world_mut()
        .resource_mut::<ActorSceneBudget>()
        .limits
        .as_mut()
        .unwrap()
        .instances = 0;
    tick(&mut app);
    assert!(
        roots
            .iter()
            .all(|e| app.world().get::<ActorSceneActive>(*e).is_none()
                && app.world().get::<Mesh3d>(*e).is_some())
    );
    assert_eq!(
        app.world().get::<Transform>(roots[0]).unwrap().translation,
        Vec3::new(1., 40., 3.)
    );
    assert_eq!(
        app.world()
            .get::<ActorSceneAnchor>(roots[0])
            .unwrap()
            .body_id,
        1
    );
    retained.lock().unwrap().clear();
}

#[test]
fn human_pose_and_independent_materials_are_charged_to_each_instance() {
    let pack = crate::asset_pack::ValidatedAssetPack::from_bytes(
        include_bytes!("../assets/character-pack/pack.json"),
        |_| Ok(include_bytes!("../assets/character-pack/fieldhand.glb").to_vec()),
    )
    .unwrap();
    let binding = &pack.manifest.bindings[0];
    let model = pack.models[&binding.model].clone();
    let mut appearance = ActorSceneAppearance::new(Handle::default());
    appearance.model = Some(ActorSceneModel {
        model: model.clone(),
        path: "test://human.glb".into(),
    });
    let mut policy = binding.animation.clone().unwrap();
    // Selected walking clips can be cheap; seated work must still be admitted.
    policy.walk = policy.idle.clone();
    policy.run = policy.idle.clone();
    appearance.animation = Some(super::super::pack::ActorSceneAnimation {
        model,
        policy,
        scale: 1.,
    });
    let full = instance_cost(&appearance);
    assert_eq!(full.animation_channels, 12);
    appearance.animation.as_mut().unwrap().policy.human = None;
    let generic = instance_cost(&appearance);
    assert_eq!(generic.animation_channels, 2);
    assert_eq!(full.estimated_bytes - generic.estimated_bytes, 3 * 4096);
}
