//! Pause-safe node-transform animation. Never changes an actor's authoritative
//! root transform, movement, interactions, inventory or saved representation.
use super::{
    pack::ActorSceneAnimation,
    scenes::{ActorSceneInstance, AppearanceReady},
};
use crate::asset_pack::{ChannelProperty, ModelChannel};
use crate::{RegionalPersistence, SandboxRuntime, WorldRuntime};
use bevy::prelude::*;
use terra_sandbox::WildlifeBehavior;

#[derive(Component, Clone)]
pub(super) struct SceneAnimationBinding(pub ActorSceneAnimation);

#[derive(Component)]
pub(super) struct SceneAnimator {
    pub entities: Vec<Entity>,
    binding: ActorSceneAnimation,
    rest: Vec<Transform>,
    pose: Vec<Transform>,
    blend_from: Vec<Transform>,
    clip: String,
    elapsed: f32,
    blend_elapsed: f32,
    vehicle: Option<VehicleRigPose>,
    human_seated: bool,
}

struct VehicleRigPose {
    wheels: Vec<usize>,
    steering_nodes: Vec<usize>,
    radius_m: f64,
    angle: f64,
    steering: f32,
    previous_heading: Option<f32>,
}
impl SceneAnimator {
    pub fn new(binding: ActorSceneAnimation, entities: Vec<Entity>) -> Self {
        let rest: Vec<_> = binding
            .model
            .nodes
            .iter()
            .map(|node| Transform {
                translation: Vec3::from_array(node.translation),
                rotation: Quat::from_array(node.rotation).normalize(),
                scale: Vec3::from_array(node.scale),
            })
            .collect();
        let vehicle = binding.policy.vehicle_motion.as_ref().map(|rig| {
            let indices = |names: &[String]| {
                names
                    .iter()
                    .map(|name| {
                        binding
                            .model
                            .nodes
                            .iter()
                            .position(|node| node.name == *name)
                            .expect("validated wheel rig node")
                    })
                    .collect()
            };
            VehicleRigPose {
                wheels: indices(&rig.wheels),
                steering_nodes: indices(&rig.steering),
                radius_m: f64::from(rig.wheel_radius_m) * f64::from(binding.scale),
                angle: 0.,
                steering: 0.,
                previous_heading: None,
            }
        });
        let mut animator = Self {
            entities,
            clip: binding.policy.idle.clone(),
            binding,
            pose: rest.clone(),
            blend_from: rest.clone(),
            rest,
            elapsed: 0.,
            blend_elapsed: 0.,
            vehicle,
            human_seated: false,
        };
        // A fresh scene may become ready while virtual time is paused. Show
        // the authored idle at phase zero, not a rest/T-pose awaiting a frame.
        animator.sample_pose(1.);
        animator.blend_from.copy_from_slice(&animator.pose);
        animator
    }
    /// Occupancy changes are a discrete authoritative handoff, not a boarding
    /// animation. Initialize the correct pose even when opening a paused save.
    pub(super) fn set_human_seated(&mut self, seated: bool) -> bool {
        let Some(human) = &self.binding.policy.human else {
            return false;
        };
        if self.human_seated == seated {
            return false;
        }
        self.human_seated = seated;
        self.clip = if seated {
            human.seated.clone()
        } else {
            self.binding.policy.idle.clone()
        };
        self.elapsed = 0.;
        self.blend_elapsed = self.binding.policy.blend_seconds;
        self.sample_pose(1.);
        true
    }
    pub(super) fn install_initial_pose(&self, commands: &mut Commands) {
        for (&entity, &pose) in self.entities.iter().zip(&self.pose) {
            commands.entity(entity).insert(pose);
        }
    }
    fn step(&mut self, dt: f32, speed: f32, behavior: Option<WildlifeBehavior>) {
        // Do not select a different clip or advance a blend while paused.
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let speed = if speed.is_finite() { speed.max(0.) } else { 0. };
        let policy = &self.binding.policy;
        let (desired, rate) = if self.human_seated {
            (
                &policy.human.as_ref().expect("seated human policy").seated,
                1.,
            )
        } else if speed > (policy.walk_speed_m_s + policy.run_speed_m_s) * 0.5 {
            (&policy.run, (speed / policy.run_speed_m_s).clamp(0.1, 3.))
        } else if speed > 0.08 {
            (
                &policy.walk,
                (speed / policy.walk_speed_m_s).clamp(0.05, 3.),
            )
        } else {
            (
                match behavior {
                    Some(WildlifeBehavior::Grazing) => &policy.feed,
                    Some(WildlifeBehavior::Drinking) => &policy.drink,
                    _ => &policy.idle,
                },
                1.,
            )
        };
        if *desired != self.clip {
            self.blend_from.copy_from_slice(&self.pose);
            self.clip = desired.clone();
            self.elapsed = 0.;
            self.blend_elapsed = 0.;
        }
        let clip = &self.binding.model.clips[&self.clip];
        // Sampling is constant work regardless of frame duration. Capping the
        // delta made clips and blends lag physical motion below 10 FPS. Use the
        // actual virtual-time delta; f64 arithmetic avoids overflow for any
        // finite accepted delta before reducing it to one bounded clip period.
        self.elapsed = ((f64::from(self.elapsed) + f64::from(dt) * f64::from(rate))
            .rem_euclid(f64::from(clip.duration)) as f32)
            .rem_euclid(clip.duration);
        self.blend_elapsed = (self.blend_elapsed + dt).min(policy.blend_seconds);
        let weight = if policy.blend_seconds <= f32::EPSILON {
            1.
        } else {
            self.blend_elapsed / policy.blend_seconds
        };
        self.sample_pose(weight);
    }
    fn sample_pose(&mut self, weight: f32) {
        let clip = &self.binding.model.clips[&self.clip];
        self.pose.copy_from_slice(&self.rest);
        for channel in &clip.channels {
            let value = sample_channel(channel, self.elapsed);
            match channel.property {
                ChannelProperty::Translation => {
                    self.pose[channel.node].translation = Vec3::new(value[0], value[1], value[2])
                }
                ChannelProperty::Rotation => {
                    // LINEAR interpolation normalizes keys, but STEP and held
                    // final keys also need this for the admitted unit tolerance.
                    self.pose[channel.node].rotation = Quat::from_array(value).normalize()
                }
                ChannelProperty::Scale => {
                    self.pose[channel.node].scale = Vec3::new(value[0], value[1], value[2])
                }
            }
        }
        if weight < 1. {
            for (pose, from) in self.pose.iter_mut().zip(&self.blend_from) {
                pose.translation = from.translation.lerp(pose.translation, weight);
                pose.rotation = from.rotation.slerp(pose.rotation, weight);
                pose.scale = from.scale.lerp(pose.scale, weight);
            }
        }
    }

    fn step_vehicle(
        &mut self,
        dt: f32,
        signed_speed: f32,
        heading: f32,
        wheelbase: Option<f32>,
        manual_steering: Option<f32>,
    ) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let signed_speed = if signed_speed.is_finite() {
            signed_speed
        } else {
            0.
        };
        self.step(dt, signed_speed.abs(), None);
        let Some(rig) = &mut self.vehicle else {
            return;
        };
        // Clip rotations are an export preview. Actual rolling uses signed,
        // admitted velocity and the binding-scaled radius, with no gait floor
        // or playback cap. Phase is retained through stop/reverse/clip changes.
        rig.angle = (rig.angle - f64::from(signed_speed) * f64::from(dt) / rig.radius_m)
            .rem_euclid(std::f64::consts::TAU);
        let measured = match (rig.previous_heading, wheelbase) {
            (Some(previous), Some(wheelbase)) if signed_speed.abs() > 0.2 => {
                let yaw = ((heading - previous + 540.).rem_euclid(360.) - 180.).to_radians();
                (yaw / dt * wheelbase / signed_speed)
                    .atan()
                    .clamp(-0.60, 0.60)
            }
            _ => 0.,
        };
        let target = manual_steering
            .filter(|v| v.is_finite())
            .unwrap_or(measured)
            .clamp(-0.75, 0.75);
        rig.steering += (target - rig.steering) * (1. - (-12. * dt).exp());
        rig.previous_heading = Some(heading);
        for &node in &rig.wheels {
            self.pose[node].rotation =
                self.rest[node].rotation * Quat::from_rotation_x(rig.angle as f32);
        }
        for &node in &rig.steering_nodes {
            self.pose[node].rotation =
                self.rest[node].rotation * Quat::from_rotation_y(-rig.steering);
        }
    }
}

fn sample_channel(channel: &ModelChannel, time: f32) -> [f32; 4] {
    // One channel can end before the overall clip. STEP must retain its final
    // key after that point instead of retaining the penultimate value.
    if time >= *channel.times.last().unwrap() {
        return *channel.values.last().unwrap();
    }
    let upper = channel
        .times
        .partition_point(|key| *key <= time)
        .clamp(1, channel.times.len() - 1);
    let lower = upper - 1;
    let a = channel.values[lower];
    if channel.step {
        return a;
    }
    let b = channel.values[upper];
    let weight = ((time - channel.times[lower]) / (channel.times[upper] - channel.times[lower]))
        .clamp(0., 1.);
    if channel.property == ChannelProperty::Rotation {
        Quat::from_array(a)
            .normalize()
            .slerp(Quat::from_array(b).normalize(), weight)
            .to_array()
    } else {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * weight)
    }
}

pub(super) fn animate_actor_scenes(
    time: Res<Time>,
    runtime: Option<Res<WorldRuntime>>,
    persistence: Option<Res<RegionalPersistence>>,
    sandbox: Option<Res<SandboxRuntime>>,
    mut animators: Query<(&ActorSceneInstance, &mut SceneAnimator), With<AppearanceReady>>,
    mut transforms: Query<&mut Transform>,
) {
    let paused = runtime.as_ref().is_some_and(|world| world.clock.paused)
        || persistence
            .as_ref()
            .is_some_and(|store| store.authority_paused());
    let dt = if paused { 0. } else { time.delta_secs() };
    for (instance, mut animator) in &mut animators {
        let seated = sandbox.as_ref().is_some_and(|s| {
            super::character::known_driver_vehicle(&s.state, instance.body_id).is_some()
        });
        let posture_changed = animator.set_human_seated(seated);
        if dt <= 0. && !posture_changed {
            continue;
        }
        let unit = sandbox
            .as_ref()
            .and_then(|world| world.state.living.units.get(&instance.body_id));
        let speed = unit
            .map(|u| u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) as f32)
            .unwrap_or(0.);
        let behavior = sandbox
            .as_ref()
            .and_then(|world| world.state.ecology.animals.get(&instance.body_id))
            .map(|animal| animal.behavior);
        if animator.vehicle.is_some() {
            let heading = unit.map_or(0., |u| u.heading_deg);
            let radians = f64::from(heading).to_radians();
            let signed_speed = unit.map_or(0., |u| {
                (u.velocity_enu_m_s[0] * radians.sin() + u.velocity_enu_m_s[1] * radians.cos())
                    as f32
            });
            let appearance = sandbox
                .as_ref()
                .and_then(|world| world.state.transport.appearance(instance.body_id));
            let manual_steering = sandbox.as_ref().and_then(|world| {
                world
                    .state
                    .transport
                    .vehicles
                    .get(&instance.body_id)
                    .filter(|vehicle| {
                        world
                            .state
                            .living
                            .accounts
                            .get(&vehicle.owner)
                            .is_some_and(|a| a.possessed && a.active_unit == instance.body_id)
                    })
                    .map(|vehicle| vehicle.dynamics.steering_rad as f32)
            });
            animator.step_vehicle(
                dt,
                signed_speed,
                heading,
                appearance.map(|vehicle| vehicle.model.spec().wheelbase_m as f32),
                manual_steering,
            );
        } else {
            animator.step(dt, speed, behavior);
        }
        for (&entity, &pose) in animator.entities.iter().zip(&animator.pose) {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                *transform = pose;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::pack::bundled_wildlife_pack;
    use super::*;
    fn animator() -> SceneAnimator {
        let pack = bundled_wildlife_pack().unwrap();
        let binding = &pack.manifest.bindings[0];
        SceneAnimator::new(
            ActorSceneAnimation {
                model: pack.models[&binding.model].clone(),
                policy: binding.animation.clone().unwrap(),
                scale: binding.scale,
            },
            Vec::new(),
        )
    }
    fn vehicle_animator(scale: f32) -> SceneAnimator {
        let pack = crate::asset_pack::ValidatedAssetPack::from_bytes(
            include_bytes!("../assets/transport-pack/pack.json"),
            |_| Ok(include_bytes!("../assets/transport-pack/comet.glb").to_vec()),
        )
        .unwrap();
        let binding = &pack.manifest.bindings[0];
        SceneAnimator::new(
            ActorSceneAnimation {
                model: pack.models[&binding.model].clone(),
                policy: binding.animation.clone().unwrap(),
                scale,
            },
            Vec::new(),
        )
    }
    #[test]
    fn wheel_phase_follows_signed_movement_and_survives_stopping_reverse_and_pause() {
        let mut vehicle = vehicle_animator(1.);
        vehicle.step_vehicle(0.1, 1., 0., Some(2.55), None);
        let wheel = vehicle.vehicle.as_ref().unwrap().wheels[0];
        let expected = Quat::from_rotation_x(-0.1 / 0.31);
        assert!(
            vehicle.pose[wheel].rotation.abs_diff_eq(expected, 0.00001)
                || vehicle.pose[wheel].rotation.abs_diff_eq(-expected, 0.00001)
        );
        let rolling = vehicle.pose[wheel].rotation;
        let angle = vehicle.vehicle.as_ref().unwrap().angle;
        vehicle.step_vehicle(0.2, 0., 0., Some(2.55), Some(0.4));
        assert_eq!(vehicle.vehicle.as_ref().unwrap().angle, angle);
        assert_eq!(vehicle.pose[wheel].rotation, rolling);
        assert!(vehicle.vehicle.as_ref().unwrap().steering > 0.);
        let pose = vehicle.pose.clone();
        let elapsed = vehicle.elapsed;
        let blend = vehicle.blend_elapsed;
        let steer = vehicle.vehicle.as_ref().unwrap().steering;
        vehicle.step_vehicle(0., -10., 90., Some(2.55), Some(-0.4));
        assert_eq!(vehicle.pose, pose);
        assert_eq!(vehicle.elapsed, elapsed);
        assert_eq!(vehicle.blend_elapsed, blend);
        assert_eq!(vehicle.vehicle.as_ref().unwrap().steering, steer);
        assert_eq!(vehicle.vehicle.as_ref().unwrap().previous_heading, Some(0.));
        vehicle.step_vehicle(0.1, -1., 0., Some(2.55), None);
        assert!(
            vehicle.pose[wheel]
                .rotation
                .abs_diff_eq(Quat::IDENTITY, 0.00001)
                || vehicle.pose[wheel]
                    .rotation
                    .abs_diff_eq(-Quat::IDENTITY, 0.00001)
        );
        assert_eq!(vehicle.pose[0], vehicle.rest[0]);
    }
    #[test]
    fn wheel_rigs_keep_instance_phase_and_binding_scale_independent() {
        let mut small = vehicle_animator(1.);
        let mut large = vehicle_animator(2.);
        large.binding.model = small.binding.model.clone();
        small.step_vehicle(0.5, 0.02, 0., Some(2.55), None);
        large.step_vehicle(0.5, 0.02, 0., Some(2.55), None);
        let small_angle = std::f64::consts::TAU - small.vehicle.as_ref().unwrap().angle;
        let large_angle = std::f64::consts::TAU - large.vehicle.as_ref().unwrap().angle;
        assert!((small_angle - large_angle * 2.).abs() < 0.000001);
        assert!((small_angle - 0.01 / 0.31).abs() < 0.000001);
        let old = large.pose.clone();
        small.step_vehicle(0.1, 20., 2., Some(2.55), None);
        assert_eq!(large.pose, old);
        assert_ne!(small.binding, large.binding);
    }
    #[test]
    fn vehicle_wheel_phase_matches_slow_frames_and_bounds_large_deltas() {
        let mut slow = vehicle_animator(1.);
        let mut fast = vehicle_animator(1.);
        slow.step_vehicle(0.5, -8., 0., Some(2.55), None);
        for _ in 0..10 {
            fast.step_vehicle(0.05, -8., 0., Some(2.55), None);
        }
        for &wheel in &slow.vehicle.as_ref().unwrap().wheels {
            assert!(
                slow.pose[wheel]
                    .rotation
                    .abs_diff_eq(fast.pose[wheel].rotation, 0.00001)
            );
        }
        slow.step_vehicle(f32::MAX, f32::MAX, 0., Some(2.55), None);
        assert!(slow.vehicle.as_ref().unwrap().angle.is_finite());
        assert!(slow.pose.iter().all(|p| p.rotation.is_finite()));
    }
    #[test]
    fn actual_movement_selects_locomotion_and_pause_freezes_clip_and_blend() {
        let mut state = animator();
        state.step(0.05, 1.4, Some(WildlifeBehavior::Grazing));
        assert_eq!(state.clip, "walk");
        assert!((state.elapsed - 0.05).abs() < 0.0001);
        let pose = state.pose.clone();
        let elapsed = state.elapsed;
        let blend = state.blend_elapsed;
        state.step(0., 5., Some(WildlifeBehavior::Drinking));
        assert_eq!(state.clip, "walk");
        assert_eq!(state.pose, pose);
        assert_eq!(state.elapsed, elapsed);
        assert_eq!(state.blend_elapsed, blend);
        state.step(0.05, 5., Some(WildlifeBehavior::Drinking));
        assert_eq!(state.clip, "run");
        state.step(0.05, 0., Some(WildlifeBehavior::Drinking));
        assert_eq!(state.clip, "drink");
    }
    #[test]
    fn animation_time_and_blends_match_across_slow_and_fast_render_frames() {
        let mut slow = animator();
        let mut fast = animator();
        slow.step(0.25, 1.4, None);
        for _ in 0..5 {
            fast.step(0.05, 1.4, None);
        }
        assert!((slow.elapsed - 0.25).abs() < 0.00001);
        assert!((slow.elapsed - fast.elapsed).abs() < 0.00001);
        assert_eq!(slow.blend_elapsed, fast.blend_elapsed);
        for (a, b) in slow.pose.iter().zip(&fast.pose) {
            assert!(a.translation.abs_diff_eq(b.translation, 0.00001));
            assert!(a.rotation.abs_diff_eq(b.rotation, 0.00001));
            assert!(a.scale.abs_diff_eq(b.scale, 0.00001));
        }
        slow.step(f32::MAX, 1000., None);
        assert!(slow.elapsed.is_finite());
        assert!(slow.elapsed >= 0. && slow.elapsed < slow.binding.model.clips[&slow.clip].duration);
        assert!(slow.pose.iter().all(|pose| {
            pose.translation.is_finite() && pose.rotation.is_finite() && pose.scale.is_finite()
        }));
    }
    #[test]
    fn step_and_held_final_rotation_keys_apply_unit_quaternions() {
        let mut state = animator();
        let mut model = (*state.binding.model).clone();
        model.clips.insert(
            state.binding.policy.idle.clone(),
            crate::asset_pack::ModelClip {
                duration: 1.,
                channels: vec![ModelChannel {
                    node: 0,
                    property: ChannelProperty::Rotation,
                    step: true,
                    times: vec![0., 0.05],
                    // Both satisfy the importer's 1% squared-length tolerance.
                    values: vec![[0., 0., 0., 1.004], [0., 0., 0., 0.996]],
                }],
            },
        );
        state.binding.model = std::sync::Arc::new(model);
        state.binding.policy.blend_seconds = 0.;
        state.step(0.025, 0., None);
        assert!((state.pose[0].rotation.length_squared() - 1.).abs() < 0.00001);
        state.step(0.1, 0., None);
        assert!((state.pose[0].rotation.length_squared() - 1.).abs() < 0.00001);
    }
    #[test]
    fn shared_model_keeps_binding_speed_and_blend_settings_independent() {
        let mut a = animator();
        let mut b = animator();
        b.binding.model = a.binding.model.clone();
        b.binding.policy.walk_speed_m_s = 2.8;
        b.binding.policy.blend_seconds = 0.8;
        a.step(0.05, 1.4, None);
        b.step(0.05, 1.4, None);
        assert!((a.elapsed - b.elapsed * 2.).abs() < 0.0001);
        assert_ne!(a.pose, b.pose);
        assert_ne!(a.binding, b.binding);
    }
    #[test]
    fn stopped_fleeing_actor_does_not_run_in_place() {
        let mut state = animator();
        state.step(0.05, 0., Some(WildlifeBehavior::Fleeing));
        assert_eq!(state.clip, "idle");
        state.step(0.05, 0., Some(WildlifeBehavior::Grazing));
        assert_eq!(state.clip, "feed");
    }
    #[test]
    fn step_channel_holds_its_final_key_when_another_channel_is_longer() {
        let channel = ModelChannel {
            node: 0,
            property: ChannelProperty::Translation,
            step: true,
            times: vec![0., 0.5, 1.],
            values: vec![[0.; 4], [1.; 4], [2.; 4]],
        };
        assert_eq!(sample_channel(&channel, 0.75), [1.; 4]);
        assert_eq!(sample_channel(&channel, 1.), [2.; 4]);
        assert_eq!(sample_channel(&channel, 2.), [2.; 4]);
    }
    #[test]
    fn animation_system_obeys_world_and_virtual_pause_without_coupling_calendar_speed() {
        use terra_core::{LocalFrame, WorldClock, synthetic_weather};
        let config = crate::WorldConfig::default();
        let clock = WorldClock {
            time_scale: 1.,
            ..Default::default()
        };
        let world = WorldRuntime {
            focus: config.focus,
            frame: LocalFrame::new(config.focus),
            weather: synthetic_weather(&config.focus, &clock, config.seed),
            clock,
            config,
            origin_revision: 0,
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let mut app = App::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs_f32(0.05));
        app.insert_resource(time)
            .insert_resource(world)
            .add_systems(Update, animate_actor_scenes);
        let instance = app
            .world_mut()
            .spawn((
                ActorSceneInstance { body_id: 999 },
                AppearanceReady,
                animator(),
            ))
            .id();
        app.update();
        let before = app.world().get::<SceneAnimator>(instance).unwrap();
        let elapsed = before.elapsed;
        let blend = before.blend_elapsed;
        let pose = before.pose.clone();
        assert!(elapsed > 0.);
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
        app.update();
        assert_eq!(
            app.world().get::<SceneAnimator>(instance).unwrap().elapsed,
            elapsed
        );
        assert_eq!(
            app.world()
                .get::<SceneAnimator>(instance)
                .unwrap()
                .blend_elapsed,
            blend
        );
        assert_eq!(
            app.world().get::<SceneAnimator>(instance).unwrap().pose,
            pose
        );
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = false;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::ZERO);
        app.update();
        assert_eq!(
            app.world().get::<SceneAnimator>(instance).unwrap().elapsed,
            elapsed
        );
        assert_eq!(
            app.world()
                .get::<SceneAnimator>(instance)
                .unwrap()
                .blend_elapsed,
            blend
        );
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.05));
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .clock
            .time_scale = 0.;
        app.update();
        assert!(app.world().get::<SceneAnimator>(instance).unwrap().elapsed > elapsed);
    }

    #[test]
    fn paused_fresh_animation_samples_authored_idle_without_waiting_for_virtual_time() {
        let mut source = animator();
        // A valid author can choose any named clip as idle. Its first pose may
        // differ from the exported rest hierarchy, including an on-foot human.
        source.binding.policy.idle = source.binding.policy.feed.clone();
        let idle = SceneAnimator::new(source.binding.clone(), Vec::new());
        let mut expected = SceneAnimator::new(source.binding, Vec::new());
        expected.sample_pose(1.);
        assert_eq!(idle.pose, expected.pose);
        assert_eq!(idle.elapsed, 0.);
        let mut human = crate::asset_pack::ValidatedAssetPack::from_bytes(
            include_bytes!("../assets/character-pack/pack.json"),
            |_| Ok(include_bytes!("../assets/character-pack/fieldhand.glb").to_vec()),
        )
        .unwrap();
        let binding = &mut human.manifest.bindings[0];
        binding.animation.as_mut().unwrap().idle = "seated".into();
        let animation = ActorSceneAnimation {
            model: human.models[&binding.model].clone(),
            policy: binding.animation.clone().unwrap(),
            scale: 1.,
        };
        let mut idle = SceneAnimator::new(animation, Vec::new());
        let hips = idle
            .binding
            .model
            .nodes
            .iter()
            .position(|n| n.name == "hips")
            .unwrap();
        assert_eq!(idle.pose[hips].translation, Vec3::ZERO);
        assert!(!idle.set_human_seated(false));
        let pose = idle.pose.clone();
        idle.step(0., 0., None);
        assert_eq!(idle.pose, pose);
        assert_eq!(idle.elapsed, 0.);
    }

    #[test]
    fn human_locomotion_seated_handoff_and_pause_do_not_move_the_actor_root() {
        let pack = crate::asset_pack::ValidatedAssetPack::from_bytes(
            include_bytes!("../assets/character-pack/pack.json"),
            |_| Ok(include_bytes!("../assets/character-pack/fieldhand.glb").to_vec()),
        )
        .unwrap();
        let binding = &pack.manifest.bindings[0];
        let make = || {
            SceneAnimator::new(
                ActorSceneAnimation {
                    model: pack.models[&binding.model].clone(),
                    policy: binding.animation.clone().unwrap(),
                    scale: binding.scale,
                },
                Vec::new(),
            )
        };
        let mut human = make();
        let mut other = make();
        human.step(0.25, 2., None);
        assert_eq!(human.clip, "walk");
        other.step(0.25, 0., None);
        assert_eq!(other.clip, "idle");
        assert_ne!(human.pose, other.pose);
        human.step(0.25, 5., None);
        assert_eq!(human.clip, "run");
        let hip = human
            .binding
            .model
            .nodes
            .iter()
            .position(|n| n.name == "hips")
            .unwrap();
        assert!(human.set_human_seated(true));
        assert_eq!(human.clip, "seated");
        assert_eq!(human.pose[hip].translation, Vec3::ZERO);
        assert_eq!(human.pose[0], human.rest[0]);
        // A seated driver cannot play a running clip from the vehicle's speed.
        human.step(0.5, 50., None);
        assert_eq!(human.clip, "seated");
        let frozen = human.pose.clone();
        let phase = human.elapsed;
        let blend = human.blend_elapsed;
        assert!(!human.set_human_seated(true));
        human.step(0., 0., None);
        assert_eq!(human.pose, frozen);
        assert_eq!(human.elapsed, phase);
        assert_eq!(human.blend_elapsed, blend);
        // Exiting is a discrete posture handoff, so a paused restored world can
        // show a valid standing avatar without advancing the animation clock.
        assert!(human.set_human_seated(false));
        assert_eq!(human.clip, "idle");
        assert_eq!(human.pose[hip].translation.y, 0.85);
        human.step(0.2, 0., None);
        assert_eq!(human.clip, "idle");
        assert_eq!(human.pose[0], human.rest[0]);
        assert_eq!(other.clip, "idle");
    }
}
