//! The host's own world, next to the framework: a physics-like system in
//! the host's `FixedUpdate` at the host's own rate ([`HOST_FIXED_HZ`]),
//! integrating a few bouncing "supply drops" in `f32` (host state, never sim
//! state). The framework runs in the same `FixedUpdate` at 10 Hz from its
//! own accumulator and never touches `Time<Fixed>`
//! (`host_fixed_timestep_is_untouched_in_demo`).

use bevy::prelude::*;

/// The host's fixed timestep (its physics rate), Hz.
pub const HOST_FIXED_HZ: f64 = 60.0;
/// m/s².
const GRAVITY: f32 = -9.81;
/// Energy kept per bounce.
const RESTITUTION: f32 = 0.8;

/// Host physics steps taken (the HUD shows it; the host test counts it).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostSteps(pub u64);

/// A host-simulated body: falls, bounces, keeps bouncing.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HostBody {
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f32,
}

/// The host side: its fixed rate and its physics system in `FixedUpdate`.
pub fn add_host(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_hz(HOST_FIXED_HZ))
        .init_resource::<HostSteps>()
        .add_systems(FixedUpdate, host_physics);
}

/// One host physics step (the fixed delta of `Time` in `FixedUpdate`).
pub fn host_physics(
    time: Res<Time>,
    mut steps: ResMut<HostSteps>,
    mut bodies: Query<(&mut HostBody, Option<&mut Transform>)>,
) {
    steps.0 += 1;
    let dt = time.delta_secs();
    for (mut body, transform) in &mut bodies {
        body.vel.y += GRAVITY * dt;
        let v = body.vel;
        body.pos += v * dt;
        if body.pos.y < body.radius {
            body.pos.y = body.radius;
            body.vel.y = body.vel.y.abs() * RESTITUTION;
            if body.vel.y < 1.0 {
                // Re-launch, so the host's world keeps moving.
                body.vel.y = 9.0;
            }
        }
        if let Some(mut t) = transform {
            t.translation = body.pos;
        }
    }
}
