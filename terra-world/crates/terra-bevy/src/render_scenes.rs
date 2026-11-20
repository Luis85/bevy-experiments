//! Optional game-owned production assets over the same authoritative bodies.
//! Bevy 0.20 loads glTF scene labels as `WorldAsset` handles. The optional asset
//! pack path validates immutable models and installs per-binding animation.
use super::{
    animation::{SceneAnimationBinding, SceneAnimator},
    budget::{ActorSceneAdmission, ActorSceneBudget, ActorSceneLimits, Candidate},
    pack::{ActorSceneAnimation, ActorSceneModel},
    quality::{RenderQuality, RenderQualitySettings},
};
use bevy::{prelude::*, world_serialization::WorldInstanceReady};
use std::collections::BTreeMap;
use terra_core::ActorKind;
use terra_sandbox::{AnimalSpecies, VehicleModel};

#[derive(Clone, Debug, PartialEq)]
pub struct ActorSceneAppearance {
    pub scene: Handle<WorldAsset>,
    pub scale: f32,
    /// Adjust a model whose local origin is not at its feet/wheels.
    pub ground_offset_m: f32,
    /// Reference actors face local -Z. Correct the imported forward direction.
    pub yaw_offset_degrees: f32,
    /// Present only for validated pack models; each binding owns its policy.
    pub animation: Option<ActorSceneAnimation>,
    /// Immutable validated-pack metadata. Direct game-owned scene overrides do
    /// not have this and cannot claim the pack's geometry/memory accounting.
    pub model: Option<ActorSceneModel>,
    /// Model-space hip anchor; presentation only, never an occupancy claim.
    pub driver_seat: Option<crate::asset_pack::DriverSeatPolicy>,
}
impl ActorSceneAppearance {
    pub fn new(scene: Handle<WorldAsset>) -> Self {
        Self {
            scene,
            scale: 1.,
            ground_offset_m: 0.,
            yaw_offset_degrees: 0.,
            animation: None,
            model: None,
            driver_seat: None,
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.driver_seat.as_ref().is_some_and(|seat| {
            seat.translation_m
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 10. || (v * self.scale).abs() > 30.)
        }) {
            return Err("driver seat must be a finite bounded model-space position");
        }
        if !self.scale.is_finite()
            || !(0.001..=100.).contains(&self.scale)
            || !self.ground_offset_m.is_finite()
            || self.ground_offset_m.abs() > 100.
            || !self.yaw_offset_degrees.is_finite()
            || self.yaw_offset_degrees.abs() > 3600.
        {
            return Err(
                "scene scale/orientation/ground offset must be finite and within presentation limits",
            );
        }
        Ok(())
    }
    pub fn local_transform(&self) -> Transform {
        Transform::from_translation(Vec3::Y * self.ground_offset_m)
            .with_rotation(Quat::from_rotation_y(self.yaw_offset_degrees.to_radians()))
            .with_scale(Vec3::splat(self.scale))
    }
    pub fn driver_seat_translation(&self) -> Option<Vec3> {
        let seat = self.driver_seat.as_ref()?;
        Some(
            self.local_transform()
                .transform_point(Vec3::from_array(seat.translation_m)),
        )
    }
}

/// Bounded to eight actor kinds, five wildlife species and nine catalog vehicle
/// models. Specific entries take precedence over a generic kind entry. No
/// source or gameplay state is changed by installing/removing an appearance.
#[derive(Resource, Default)]
pub struct ActorSceneOverrides {
    kinds: BTreeMap<u8, ActorSceneAppearance>,
    species: BTreeMap<AnimalSpecies, ActorSceneAppearance>,
    vehicle_models: BTreeMap<u8, ActorSceneAppearance>,
}
impl ActorSceneOverrides {
    pub fn set_kind(
        &mut self,
        kind: ActorKind,
        appearance: ActorSceneAppearance,
    ) -> Result<(), &'static str> {
        appearance.validate()?;
        self.kinds.insert(kind as u8, appearance);
        Ok(())
    }
    pub fn set_species(
        &mut self,
        species: AnimalSpecies,
        appearance: ActorSceneAppearance,
    ) -> Result<(), &'static str> {
        appearance.validate()?;
        self.species.insert(species, appearance);
        Ok(())
    }
    pub fn remove_kind(&mut self, kind: ActorKind) {
        self.kinds.remove(&(kind as u8));
    }
    pub fn remove_species(&mut self, species: AnimalSpecies) {
        self.species.remove(&species);
    }
    pub fn set_vehicle_model(
        &mut self,
        model: VehicleModel,
        appearance: ActorSceneAppearance,
    ) -> Result<(), &'static str> {
        appearance.validate()?;
        self.vehicle_models.insert(model as u8, appearance);
        Ok(())
    }
    pub fn remove_vehicle_model(&mut self, model: VehicleModel) {
        self.vehicle_models.remove(&(model as u8));
    }
    pub fn resolve(
        &self,
        kind: ActorKind,
        species: Option<AnimalSpecies>,
    ) -> Option<&ActorSceneAppearance> {
        self.resolve_model(kind, species, None)
    }
    /// Resolve a public catalog model without requiring private vehicle state.
    /// A model hint cannot apply a road-vehicle appearance to another kind.
    pub fn resolve_model(
        &self,
        kind: ActorKind,
        species: Option<AnimalSpecies>,
        vehicle_model: Option<VehicleModel>,
    ) -> Option<&ActorSceneAppearance> {
        if kind == ActorKind::Vehicle
            && let Some(appearance) =
                vehicle_model.and_then(|m| self.vehicle_models.get(&(m as u8)))
        {
            return Some(appearance);
        }
        species
            .and_then(|s| self.species.get(&s))
            .or_else(|| self.kinds.get(&(kind as u8)))
    }
}

pub(super) fn model_for_anchor(
    anchor: &ActorSceneAnchor,
    sandbox: Option<&crate::SandboxRuntime>,
) -> Option<VehicleModel> {
    (anchor.kind == ActorKind::Vehicle)
        .then(|| {
            sandbox?
                .state
                .transport
                .appearance(anchor.body_id)
                .map(|v| v.model)
        })
        .flatten()
}

/// Attach this and `ProceduralActorMesh` to a body's visual root. The root keeps
/// the authoritative geographic transform and control-driven Visibility.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ActorSceneAnchor {
    pub body_id: u64,
    pub kind: ActorKind,
    pub species: Option<AnimalSpecies>,
}
#[derive(Component, Clone)]
pub struct ProceduralActorMesh {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}
/// Present only when a ready supplied model replaces the procedural root mesh.
#[derive(Component)]
pub struct ActorSceneActive;
/// The model child receiving Bevy's `WorldInstanceReady` event. Games can use
/// this body ID to attach their animation graph or model-specific effects.
#[derive(Component)]
pub struct ActorSceneInstance {
    pub body_id: u64,
}
#[derive(Component)]
pub(super) struct AppearanceReady;
#[derive(Component)]
pub(super) struct AppearanceRejected(pub String);
#[derive(Component)]
pub(super) struct AppearanceInstance {
    child: Entity,
    appearance: ActorSceneAppearance,
}

pub(super) fn scene_ready(
    event: On<WorldInstanceReady>,
    instances: Query<(
        &ActorSceneInstance,
        Option<&SceneAnimationBinding>,
        &Transform,
    )>,
    children: Query<&Children>,
    names: Query<&Name, (Without<Mesh3d>, Without<bevy::gltf::GltfSceneName>)>,
    mut visibility: Query<&mut Visibility>,
    primitive_materials: Query<(
        &bevy::gltf::GltfMaterialName,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    sandbox: Option<Res<crate::SandboxRuntime>>,
    mut commands: Commands,
) {
    if let Ok((instance, animation, instance_transform)) = instances.get(event.entity) {
        // Imported nodes marked Visible would bypass a hidden possessed root.
        // Retain intentionally hidden parts, but make all visible parts inherit
        // body ownership, actor-layer and first-person visibility.
        for descendant in children.iter_descendants(event.entity) {
            if let Ok(mut visible) = visibility.get_mut(descendant) {
                if *visible == Visibility::Visible {
                    *visible = Visibility::Inherited;
                }
            }
        }
        if let Some(animation) = animation {
            let mut found = BTreeMap::new();
            let mut duplicates = false;
            for descendant in children.iter_descendants(event.entity) {
                if let Ok(name) = names.get(descendant) {
                    duplicates |= found.insert(name.as_str(), descendant).is_some();
                }
            }
            let entities: Option<Vec<_>> = animation
                .0
                .model
                .nodes
                .iter()
                .map(|node| found.get(node.name.as_str()).copied())
                .collect();
            if duplicates || entities.is_none() {
                commands.entity(event.entity).insert(AppearanceRejected("Imported animation hierarchy does not match the validated named nodes; procedural model retained".into()));
                return;
            }
            let mut animator = SceneAnimator::new(animation.0.clone(), entities.unwrap());
            if let Some(human) = &animation.0.policy.human {
                let Some(materials) = materials.as_deref_mut() else {
                    commands.entity(event.entity).insert(AppearanceRejected(
                        "Character materials are unavailable; procedural model retained".into(),
                    ));
                    return;
                };
                let names = human.material_names();
                let mut roles: [Vec<(Entity, Handle<StandardMaterial>)>; 3] =
                    std::array::from_fn(|_| Vec::new());
                for descendant in children.iter_descendants(event.entity) {
                    if let Ok((name, material)) = primitive_materials.get(descendant) {
                        if let Some(role) = names.iter().position(|role| *role == name.0) {
                            roles[role].push((descendant, material.0.clone()));
                        }
                    }
                }
                let valid = roles.iter().zip(names).all(|(role, name)| {
                    let expected = animation
                        .0
                        .model
                        .materials
                        .iter()
                        .find(|material| material.name.as_deref() == Some(name))
                        .map_or(0, |material| material.scene_primitives);
                    role.len() == expected
                        && !role.is_empty()
                        && role.iter().all(|(_, handle)| {
                            handle == &role[0].1 && materials.get(handle).is_some()
                        })
                });
                if !valid {
                    commands.entity(event.entity).insert(AppearanceRejected("Imported character material roles do not match the validated scene; procedural model retained".into()));
                    return;
                }
                // Clone precisely three materials. Shared GLB materials remain
                // immutable and other actors retain their own cosmetic choices.
                let handles: [Handle<StandardMaterial>; 3] = std::array::from_fn(|role| {
                    materials.add(materials.get(&roles[role][0].1).unwrap().clone())
                });
                let profile = super::character::profile_for_body(
                    sandbox.as_ref().map(|s| &s.state),
                    instance.body_id,
                );
                super::character::tint_materials(materials, &handles, &profile);
                for (role, targets) in roles.iter().enumerate() {
                    for (entity, _) in targets {
                        commands
                            .entity(*entity)
                            .insert(MeshMaterial3d(handles[role].clone()));
                    }
                }
                let seated = sandbox.as_ref().is_some_and(|s| {
                    super::character::known_driver_vehicle(&s.state, instance.body_id).is_some()
                });
                let mut transform = *instance_transform;
                transform.scale =
                    Vec3::from_array(super::character::character_body_scale(&profile))
                        * animation.0.scale;
                if seated {
                    transform.translation.y = 0.;
                }
                commands.entity(event.entity).insert((
                    super::character::initial_appearance(
                        handles,
                        &profile,
                        instance_transform.translation.y,
                    ),
                    transform,
                ));
                animator.set_human_seated(seated);
            }
            animator.install_initial_pose(&mut commands);
            commands.entity(event.entity).insert(animator);
        }
        commands.entity(event.entity).insert(AppearanceReady);
    }
}

fn asset_ready(
    appearance: &ActorSceneAppearance,
    scenes: &Assets<WorldAsset>,
    server: &AssetServer,
) -> bool {
    scenes.get(&appearance.scene).is_some_and(|scene| {
        scene.world.component_id::<Mesh3d>().is_some_and(|mesh| {
            scene
                .world
                .archetypes()
                .iter()
                .any(|a| !a.is_empty() && a.contains(mesh))
        })
    }) && (appearance.scene.path().is_none()
        || server.is_loaded_with_dependencies(&appearance.scene))
}

pub(super) fn sync_actor_scenes(
    mut commands: Commands,
    overrides: Res<ActorSceneOverrides>,
    scenes: Res<Assets<WorldAsset>>,
    server: Res<AssetServer>,
    budget: Option<Res<ActorSceneBudget>>,
    quality: Option<Res<RenderQualitySettings>>,
    runtime: Option<Res<crate::WorldRuntime>>,
    sandbox: Option<Res<crate::SandboxRuntime>>,
    mut admission: Option<ResMut<ActorSceneAdmission>>,
    roots: Query<(
        Entity,
        &ActorSceneAnchor,
        &ProceduralActorMesh,
        Option<&AppearanceInstance>,
        Option<&ActorSceneActive>,
        Option<&Transform>,
        Option<&Visibility>,
        Option<&crate::GeoAnchor>,
        Option<&InheritedVisibility>,
    )>,
    instances: Query<Option<&AppearanceReady>, With<ActorSceneInstance>>,
) {
    let mut candidates = Vec::new();
    let mut current = BTreeMap::new();
    let active_body = sandbox
        .as_ref()
        .and_then(|sandbox| sandbox.state.living.accounts.get(&sandbox.player_id))
        .map(|a| a.active_unit);
    for (entity, anchor, _, instance, _, transform, visibility, geo, inherited) in &roots {
        if let Some(instance) = instance.filter(|i| i.appearance.model.is_some()) {
            current.insert(entity, instance.appearance.clone());
        }
        let Some(appearance) = overrides
            .resolve_model(
                anchor.kind,
                anchor.species,
                model_for_anchor(anchor, sandbox.as_deref()),
            )
            .filter(|a| a.model.is_some())
        else {
            continue;
        };
        let unit = sandbox
            .as_ref()
            .and_then(|s| s.state.living.units.get(&anchor.body_id));
        let seated_position = sandbox.as_ref().and_then(|s| {
            super::character::driver_seat_presentation(&overrides, &s.state, anchor.body_id)
                .and_then(|(vehicle, _)| s.state.living.units.get(&vehicle).map(|u| u.position))
        });
        let owned = unit.is_some_and(|u| {
            sandbox
                .as_ref()
                .is_some_and(|s| u.owner == Some(s.player_id))
        });
        let distance_m = runtime.as_ref().map_or_else(
            || transform.map_or(0., |t| t.translation.length()),
            |runtime| {
                if let Some(position) = seated_position
                    .or_else(|| geo.map(|g| g.0))
                    .or_else(|| unit.map(|u| u.position))
                {
                    position.surface_distance_m(runtime.focus) as f32
                } else {
                    let focus = runtime.frame.to_local(runtime.focus).map(|v| v as f32);
                    transform.map_or(0., |t| t.translation.distance(Vec3::from_array(focus)))
                }
            },
        );
        candidates.push(Candidate {
            entity,
            body_id: anchor.body_id,
            priority: if active_body == Some(anchor.body_id) {
                0
            } else if owned {
                1
            } else {
                2
            },
            distance_m,
            visible: match visibility {
                Some(Visibility::Hidden) => false,
                Some(Visibility::Inherited) => inherited.is_none_or(|v| v.get()),
                _ => true,
            },
            appearance: appearance.clone(),
        });
    }
    let limits = budget
        .as_ref()
        .and_then(|b| b.limits.clone())
        .unwrap_or_else(|| {
            ActorSceneLimits::for_quality(
                quality
                    .as_ref()
                    .map_or(RenderQuality::Balanced, |q| q.quality),
            )
        });
    let admitted = admission
        .as_mut()
        .map(|state| state.select(candidates, &current, limits, &server))
        .unwrap_or_default();
    // A scene can be shared by thousands of bodies; inspect its readiness only
    // once per update, and create instances only on an actual registry change.
    let mut ready_assets = BTreeMap::new();
    for (entity, anchor, fallback, instance, active, _, _, _, _) in &roots {
        let desired = overrides
            .resolve_model(
                anchor.kind,
                anchor.species,
                model_for_anchor(anchor, sandbox.as_deref()),
            )
            .and_then(|appearance| {
                if appearance.model.is_some() {
                    admitted.get(&entity)
                } else {
                    Some(appearance)
                }
            })
            .filter(|appearance| {
                *ready_assets
                    .entry(appearance.scene.id())
                    .or_insert_with(|| asset_ready(appearance, &scenes, &server))
            });
        let unchanged = match (instance, desired) {
            (Some(instance), Some(desired)) => {
                instance.appearance == *desired && instances.contains(instance.child)
            }
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            if let Some(instance) = instance {
                commands.entity(instance.child).try_despawn();
                commands.entity(entity).remove::<AppearanceInstance>();
            }
            commands
                .entity(entity)
                .insert((
                    Mesh3d(fallback.mesh.clone()),
                    MeshMaterial3d(fallback.material.clone()),
                ))
                .remove::<ActorSceneActive>();
            if let Some(appearance) = desired {
                let child = commands
                    .spawn((
                        ActorSceneInstance {
                            body_id: anchor.body_id,
                        },
                        WorldAssetRoot(appearance.scene.clone()),
                        appearance.local_transform(),
                        Visibility::Hidden,
                    ))
                    .id();
                if let Some(animation) = &appearance.animation {
                    let mut animation = animation.clone();
                    // SDK callers can adjust a registered presentation's
                    // uniform scale. Match its actual child transform rather
                    // than retaining the pack's original wheel radius scale.
                    animation.scale = appearance.scale;
                    commands
                        .entity(child)
                        .insert(SceneAnimationBinding(animation));
                }
                commands
                    .entity(entity)
                    .add_child(child)
                    .insert(AppearanceInstance {
                        child,
                        appearance: appearance.clone(),
                    });
            }
        } else if let Some(instance) = instance {
            if active.is_none()
                && instances
                    .get(instance.child)
                    .is_ok_and(|ready| ready.is_some())
            {
                // Keep a fallback until the scene instance, including its
                // dependencies, is actually ready. Hiding the owned root in
                // first person also hides this inherited child hierarchy.
                commands
                    .entity(instance.child)
                    .insert(Visibility::Inherited);
                commands
                    .entity(entity)
                    .remove::<Mesh3d>()
                    .insert(ActorSceneActive);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_model_binding_is_specific_and_falls_back_to_the_generic_kind() {
        let mut registry = ActorSceneOverrides::default();
        let mut generic = ActorSceneAppearance::new(Handle::default());
        generic.scale = 2.;
        let mut compact = ActorSceneAppearance::new(Handle::default());
        compact.scale = 1.1;
        registry
            .set_kind(ActorKind::Vehicle, generic.clone())
            .unwrap();
        registry
            .set_vehicle_model(VehicleModel::Compact, compact.clone())
            .unwrap();
        assert_eq!(
            registry.resolve_model(ActorKind::Vehicle, None, Some(VehicleModel::Compact)),
            Some(&compact)
        );
        assert_eq!(
            registry.resolve_model(ActorKind::Vehicle, None, Some(VehicleModel::Pickup)),
            Some(&generic)
        );
        assert_eq!(registry.resolve(ActorKind::Vehicle, None), Some(&generic));
        assert!(
            registry
                .resolve_model(ActorKind::Human, None, Some(VehicleModel::Compact))
                .is_none()
        );
        registry.remove_vehicle_model(VehicleModel::Compact);
        assert_eq!(
            registry.resolve_model(ActorKind::Vehicle, None, Some(VehicleModel::Compact)),
            Some(&generic)
        );
    }

    #[test]
    fn public_vehicle_appearance_selects_model_without_private_state_or_mutation() {
        let mut sandbox =
            crate::SandboxRuntime::new(terra_core::GeoPosition::new(48., 11., 0.).unwrap());
        sandbox.state.transport.visible_vehicles.insert(
            77,
            terra_sandbox::VehicleAppearance {
                unit_id: 77,
                owner: 2,
                model: VehicleModel::Compact,
                paint: 3,
                lights: false,
                indicator: terra_sandbox::TurnIndicator::Off,
                steering_rad: 0.,
                trailer: None,
            },
        );
        let before = serde_json::to_vec(&sandbox.state).unwrap();
        let mut anchor = ActorSceneAnchor {
            body_id: 77,
            kind: ActorKind::Vehicle,
            species: None,
        };
        assert!(!sandbox.state.transport.vehicles.contains_key(&77));
        assert_eq!(
            model_for_anchor(&anchor, Some(&sandbox)),
            Some(VehicleModel::Compact)
        );
        anchor.kind = ActorKind::Human;
        assert_eq!(model_for_anchor(&anchor, Some(&sandbox)), None);
        assert_eq!(serde_json::to_vec(&sandbox.state).unwrap(), before);
    }

    #[test]
    fn animated_hierarchy_is_checked_before_fallback_is_hidden() {
        use bevy::{
            asset::AssetPlugin, transform::components::TransformTreeChanged,
            world_serialization::WorldSerializationPlugin,
        };
        let pack = super::super::pack::bundled_wildlife_pack().unwrap();
        let binding = &pack.manifest.bindings[0];
        let animation = ActorSceneAnimation {
            model: pack.models[&binding.model].clone(),
            policy: binding.animation.clone().unwrap(),
            scale: binding.scale,
        };
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .register_type::<Mesh3d>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<TransformTreeChanged>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .register_type::<Name>()
        .init_resource::<ActorSceneOverrides>()
        .add_observer(scene_ready)
        .add_systems(Update, sync_actor_scenes);
        let mut unmatched = World::new();
        unmatched.spawn((
            Name::new("unexpected"),
            Mesh3d::default(),
            Transform::default(),
            Visibility::Visible,
        ));
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(unmatched));
        let mut appearance = ActorSceneAppearance::new(handle);
        appearance.animation = Some(animation.clone());
        appearance.scale = 2.;
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_species(AnimalSpecies::Deer, appearance)
            .unwrap();
        let root = app
            .world_mut()
            .spawn((
                ActorSceneAnchor {
                    body_id: 51,
                    kind: ActorKind::Animal,
                    species: Some(AnimalSpecies::Deer),
                },
                ProceduralActorMesh {
                    mesh: Handle::default(),
                    material: Handle::default(),
                },
                Mesh3d::default(),
                MeshMaterial3d::<StandardMaterial>::default(),
                Transform::from_xyz(10., 20., 30.),
                Visibility::Hidden,
            ))
            .id();
        for _ in 0..5 {
            app.update();
        }
        let rejected = app.world().get::<AppearanceInstance>(root).unwrap().child;
        assert_eq!(
            app.world()
                .get::<SceneAnimationBinding>(rejected)
                .unwrap()
                .0
                .scale,
            2.
        );
        assert!(app.world().get::<AppearanceRejected>(rejected).is_some());
        assert!(app.world().get::<Mesh3d>(root).is_some());
        assert!(app.world().get::<ActorSceneActive>(root).is_none());

        let mut matched = World::new();
        for node in &animation.model.nodes {
            matched.spawn((
                Name::new(node.name.clone()),
                Transform::default(),
                Visibility::Visible,
            ));
        }
        matched.spawn((Mesh3d::default(), Transform::default(), Visibility::Visible));
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(matched));
        let mut appearance = ActorSceneAppearance::new(handle);
        appearance.animation = Some(animation.clone());
        appearance.scale = 3.;
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_species(AnimalSpecies::Deer, appearance)
            .unwrap();
        for _ in 0..5 {
            app.update();
        }
        let ready = app.world().get::<AppearanceInstance>(root).unwrap().child;
        assert!(app.world().get_entity(rejected).is_err());
        assert!(app.world().get::<ActorSceneActive>(root).is_some());
        assert!(app.world().get::<Mesh3d>(root).is_none());
        assert_eq!(
            app.world()
                .get::<SceneAnimationBinding>(ready)
                .unwrap()
                .0
                .scale,
            3.
        );
        assert_eq!(
            app.world()
                .get::<SceneAnimator>(ready)
                .unwrap()
                .entities
                .len(),
            animation.model.nodes.len()
        );
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world().get::<Transform>(root).unwrap().translation,
            Vec3::new(10., 20., 30.)
        );
        for child in app.world().get::<Children>(ready).unwrap() {
            assert_eq!(
                *app.world().get::<Visibility>(*child).unwrap(),
                Visibility::Inherited
            );
        }
    }

    #[test]
    fn actual_scene_ready_replaces_fallback_and_removal_restores_the_same_body() {
        use bevy::{
            asset::AssetPlugin, transform::components::TransformTreeChanged,
            world_serialization::WorldSerializationPlugin,
        };
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .register_type::<Mesh3d>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<TransformTreeChanged>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .init_resource::<ActorSceneOverrides>()
        .add_observer(scene_ready)
        .add_systems(Update, sync_actor_scenes);
        let pending = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .reserve_handle();
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_kind(ActorKind::Human, ActorSceneAppearance::new(pending.clone()))
            .unwrap();
        let ground = Transform::from_xyz(12., 340., -8.);
        let root = app
            .world_mut()
            .spawn((
                ActorSceneAnchor {
                    body_id: 42,
                    kind: ActorKind::Human,
                    species: None,
                },
                ProceduralActorMesh {
                    mesh: Handle::default(),
                    material: Handle::default(),
                },
                Mesh3d::default(),
                MeshMaterial3d::<StandardMaterial>::default(),
                ground,
                Visibility::Hidden,
            ))
            .id();
        app.update();
        assert!(app.world().get::<Mesh3d>(root).is_some());
        assert!(app.world().get::<AppearanceInstance>(root).is_none());

        let mut model = World::new();
        model.spawn((Mesh3d::default(), Visibility::Visible));
        app.world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .insert(&pending, WorldAsset::new(model))
            .unwrap();
        // Exercise Bevy's actual scene spawner and ready event, not a fabricated
        // ready flag. The fallback stays until that event has been handled.
        app.update();
        assert!(app.world().get::<Mesh3d>(root).is_some());
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().get::<ActorSceneActive>(root).is_some());
        assert!(app.world().get::<Mesh3d>(root).is_none());
        assert_eq!(*app.world().get::<Transform>(root).unwrap(), ground);
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        let child = app.world().get::<AppearanceInstance>(root).unwrap().child;
        assert_eq!(
            app.world()
                .get::<ActorSceneInstance>(child)
                .unwrap()
                .body_id,
            42
        );
        assert_eq!(
            *app.world().get::<Visibility>(child).unwrap(),
            Visibility::Inherited
        );
        let imported = app.world().get::<Children>(child).unwrap()[0];
        assert_eq!(
            *app.world().get::<Visibility>(imported).unwrap(),
            Visibility::Inherited
        );
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().get::<AppearanceInstance>(root).unwrap().child,
            child
        );

        // Losing or failing a model does not lose the actor, its transform or
        // visibility. Re-loading restores the optional scene safely.
        app.world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .remove(pending.id());
        app.update();
        assert!(app.world().get::<Mesh3d>(root).is_some());
        assert!(app.world().get::<ActorSceneActive>(root).is_none());
        assert!(app.world().get::<AppearanceInstance>(root).is_none());
        assert!(app.world().get_entity(child).is_err());
        assert!(app.world().get_entity(imported).is_err());
        assert_eq!(*app.world().get::<Transform>(root).unwrap(), ground);
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn scene_registry_validates_scale_and_preserves_specific_species_precedence() {
        let mut assets = Assets::<WorldAsset>::default();
        let generic = assets.add(WorldAsset::new(World::new()));
        let specific = assets.add(WorldAsset::new(World::new()));
        let mut registry = ActorSceneOverrides::default();
        registry
            .set_kind(
                ActorKind::Animal,
                ActorSceneAppearance::new(generic.clone()),
            )
            .unwrap();
        let mut deer = ActorSceneAppearance::new(specific.clone());
        deer.scale = 0.5;
        deer.ground_offset_m = 0.35;
        deer.yaw_offset_degrees = 90.;
        registry
            .set_species(AnimalSpecies::Deer, deer.clone())
            .unwrap();
        assert_eq!(
            registry
                .resolve(ActorKind::Animal, Some(AnimalSpecies::Deer))
                .unwrap()
                .scene,
            specific
        );
        assert_eq!(
            registry
                .resolve(ActorKind::Animal, Some(AnimalSpecies::Hare))
                .unwrap()
                .scene,
            generic
        );
        let transform = deer.local_transform();
        assert_eq!(transform.translation, Vec3::Y * 0.35);
        assert_eq!(transform.scale, Vec3::splat(0.5));
        assert!((transform.rotation * Vec3::NEG_Z - Vec3::NEG_X).length() < 0.0001);
        let before = registry
            .resolve(ActorKind::Animal, Some(AnimalSpecies::Deer))
            .unwrap()
            .clone();
        deer.scale = f32::NAN;
        assert!(registry.set_species(AnimalSpecies::Deer, deer).is_err());
        assert_eq!(
            *registry
                .resolve(ActorKind::Animal, Some(AnimalSpecies::Deer))
                .unwrap(),
            before
        );
        registry.remove_species(AnimalSpecies::Deer);
        assert_eq!(
            registry
                .resolve(ActorKind::Animal, Some(AnimalSpecies::Deer))
                .unwrap()
                .scene,
            generic
        );
        registry.remove_kind(ActorKind::Animal);
        assert!(registry.resolve(ActorKind::Animal, None).is_none());
    }

    #[test]
    fn customized_humans_keep_profiles_isolated_reuse_materials_and_release_them_on_teardown() {
        use super::super::character::{
            HumanAppearanceInstance, character_body_scale, update_human_appearances,
        };
        use bevy::{
            asset::{AssetApp, AssetPlugin},
            transform::components::TransformTreeChanged,
            world_serialization::WorldSerializationPlugin,
        };
        let pack = crate::asset_pack::ValidatedAssetPack::from_bytes(
            include_bytes!("../assets/character-pack/pack.json"),
            |_| Ok(include_bytes!("../assets/character-pack/fieldhand.glb").to_vec()),
        )
        .unwrap();
        let binding = &pack.manifest.bindings[0];
        let animation = ActorSceneAnimation {
            model: pack.models[&binding.model].clone(),
            policy: binding.animation.clone().unwrap(),
            scale: 1.,
        };
        let origin = terra_core::GeoPosition::new(48., 11., 0.).unwrap();
        let mut sandbox = crate::SandboxRuntime::new(origin);
        sandbox.state.add_player(2, origin).unwrap();
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        for id in [1, 2] {
            sandbox
                .state
                .set_character_profile(
                    id,
                    terra_sandbox::CharacterProfile {
                        name: format!("Player {id}"),
                        body_style: id as u8,
                        skin: (id * 3) as u8,
                        hair: id as u8,
                        outfit: (id * 2) as u8,
                    },
                )
                .unwrap();
        }
        let bodies = [
            sandbox.state.living.accounts[&1].avatar_unit,
            sandbox.state.living.accounts[&2].avatar_unit,
        ];
        let before = sandbox.state.clone();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .init_asset::<StandardMaterial>()
        .register_type::<Mesh3d>()
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .register_type::<bevy::gltf::GltfMaterialName>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<TransformTreeChanged>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .register_type::<Name>()
        .init_resource::<ActorSceneOverrides>()
        .insert_resource(sandbox)
        .add_observer(scene_ready)
        .add_systems(
            Update,
            (sync_actor_scenes, update_human_appearances).chain(),
        );
        let source: [Handle<StandardMaterial>; 3] = std::array::from_fn(|_| {
            app.world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial::default())
        });
        let mut model_world = World::new();
        for node in &animation.model.nodes {
            model_world.spawn((
                Name::new(node.name.clone()),
                Transform::default(),
                Visibility::Visible,
            ));
        }
        for (role, name) in animation
            .policy
            .human
            .as_ref()
            .unwrap()
            .material_names()
            .iter()
            .enumerate()
        {
            let count = animation
                .model
                .materials
                .iter()
                .find(|m| m.name.as_deref() == Some(name))
                .unwrap()
                .scene_primitives;
            for _ in 0..count {
                model_world.spawn((
                    Mesh3d::default(),
                    MeshMaterial3d(source[role].clone()),
                    bevy::gltf::GltfMaterialName((*name).into()),
                    Transform::default(),
                    Visibility::Visible,
                ));
            }
        }
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(model_world));
        let mut appearance = ActorSceneAppearance::new(handle);
        appearance.animation = Some(animation.clone());
        appearance.ground_offset_m = 0.4;
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_kind(ActorKind::Human, appearance)
            .unwrap();
        let roots = bodies.map(|body| {
            app.world_mut()
                .spawn((
                    ActorSceneAnchor {
                        body_id: body,
                        kind: ActorKind::Human,
                        species: None,
                    },
                    ProceduralActorMesh {
                        mesh: Handle::default(),
                        material: Handle::default(),
                    },
                    Mesh3d::default(),
                    Transform::default(),
                    Visibility::Visible,
                ))
                .id()
        });
        for _ in 0..6 {
            app.update();
        }
        let instances = roots.map(|root| {
            assert!(app.world().get::<ActorSceneActive>(root).is_some());
            app.world().get::<AppearanceInstance>(root).unwrap().child
        });
        let ids = instances.map(|entity| {
            app.world()
                .get::<HumanAppearanceInstance>(entity)
                .unwrap()
                .materials
                .each_ref()
                .map(Handle::id)
        });
        assert_ne!(ids[0], ids[1]);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 9);
        for (index, entity) in instances.iter().enumerate() {
            let profile = &before.transport.profiles[&(index as u64 + 1)];
            assert_eq!(
                app.world().get::<Transform>(*entity).unwrap().scale,
                Vec3::from_array(character_body_scale(profile))
            );
            let (skin, hair, outfit) =
                super::super::character::character_appearance_colors(profile);
            for (id, color) in ids[index].iter().zip([skin, hair, outfit]) {
                assert_eq!(
                    app.world()
                        .resource::<Assets<StandardMaterial>>()
                        .get(*id)
                        .unwrap()
                        .base_color,
                    Color::srgb(color[0], color[1], color[2])
                );
            }
        }
        for handle in &source {
            assert_eq!(
                app.world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(handle)
                    .unwrap()
                    .base_color,
                Color::WHITE
            );
        }
        assert_eq!(
            app.world().resource::<crate::SandboxRuntime>().state,
            before
        );
        let original_second = ids[1].map(|id| {
            app.world()
                .resource::<Assets<StandardMaterial>>()
                .get(id)
                .unwrap()
                .base_color
        });
        for palette in 0..8 {
            let mut sandbox = app.world_mut().resource_mut::<crate::SandboxRuntime>();
            let mut profile = sandbox.state.transport.profiles[&1].clone();
            profile.skin = palette;
            profile.hair = palette;
            profile.outfit = palette;
            sandbox.state.set_character_profile(1, profile).unwrap();
            drop(sandbox);
            app.update();
            assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 9);
            assert_eq!(
                app.world()
                    .get::<HumanAppearanceInstance>(instances[0])
                    .unwrap()
                    .materials
                    .each_ref()
                    .map(Handle::id),
                ids[0]
            );
            assert_eq!(
                ids[1].map(|id| app
                    .world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(id)
                    .unwrap()
                    .base_color),
                original_second
            );
        }
        // Valid standing ground correction is suppressed exactly while seated,
        // then restored on Release; it must not lift the hip away from the seat.
        for entity in instances {
            assert_eq!(
                app.world().get::<Transform>(entity).unwrap().translation.y,
                0.4
            );
        }
        {
            let mut s = app.world_mut().resource_mut::<crate::SandboxRuntime>();
            let vehicle = s
                .state
                .transport
                .vehicles
                .values()
                .find(|v| v.owner == 1)
                .unwrap()
                .unit_id;
            let position = s.state.living.units[&vehicle].position;
            s.state.set_player_position(1, position).unwrap();
            let catalog = s.catalog.clone();
            s.state
                .apply(
                    1,
                    terra_sandbox::Command::Transport {
                        action: terra_sandbox::TransportCommand::EnterVehicle {
                            vehicle_id: vehicle,
                        },
                    },
                    &catalog,
                )
                .unwrap();
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<Transform>(instances[0])
                .unwrap()
                .translation
                .y,
            0.
        );
        assert_eq!(
            app.world()
                .get::<Transform>(instances[1])
                .unwrap()
                .translation
                .y,
            0.4
        );
        {
            let mut s = app.world_mut().resource_mut::<crate::SandboxRuntime>();
            let catalog = s.catalog.clone();
            s.state
                .apply(
                    1,
                    terra_sandbox::Command::Living {
                        action: terra_sandbox::LivingCommand::Release,
                    },
                    &catalog,
                )
                .unwrap();
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<Transform>(instances[0])
                .unwrap()
                .translation
                .y,
            0.4
        );
        // Ordinary actor despawn also releases its three private materials.
        app.world_mut().entity_mut(roots[0]).despawn();
        for _ in 0..6 {
            app.update();
        }
        for id in ids[0] {
            assert!(
                app.world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(id)
                    .is_none()
            );
        }
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 6);
        // A ready but mismatched named material scene must not hide fallback or
        // allocate a partial tint set. No validated source material is mutated.
        let mut bad_world = World::new();
        for node in &animation.model.nodes {
            bad_world.spawn((
                Name::new(node.name.clone()),
                Transform::default(),
                Visibility::Visible,
            ));
        }
        bad_world.spawn((
            Mesh3d::default(),
            MeshMaterial3d(source[0].clone()),
            bevy::gltf::GltfMaterialName("missing_role".into()),
            Transform::default(),
            Visibility::Visible,
        ));
        let bad = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(bad_world));
        let mut bad_appearance = ActorSceneAppearance::new(bad);
        bad_appearance.animation = Some(animation.clone());
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_kind(ActorKind::Human, bad_appearance)
            .unwrap();
        for _ in 0..6 {
            app.update();
        }
        assert!(app.world().get::<Mesh3d>(roots[1]).is_some());
        assert!(app.world().get::<ActorSceneActive>(roots[1]).is_none());
        let bad_child = app
            .world()
            .get::<AppearanceInstance>(roots[1])
            .unwrap()
            .child;
        assert!(
            app.world()
                .get::<AppearanceRejected>(bad_child)
                .unwrap()
                .0
                .contains("material roles")
        );
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 3);
        // Pack removal retains the same procedural body after a failed import.
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .remove_kind(ActorKind::Human);
        for _ in 0..8 {
            app.update();
        }
        assert!(app.world().get::<Mesh3d>(roots[1]).is_some());
        assert!(app.world().get::<ActorSceneActive>(roots[1]).is_none());
        for id in ids.into_iter().flatten() {
            assert!(
                app.world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(id)
                    .is_none()
            );
        }
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 3);
    }
}
