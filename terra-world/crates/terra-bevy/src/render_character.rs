//! Bounded per-instance character appearance. It only reads the existing
//! public cosmetic profile and the account whose occupancy is actually known.
use super::{
    animation::SceneAnimationBinding,
    scenes::{ActorSceneInstance, ActorSceneOverrides},
};
use bevy::prelude::*;
use std::borrow::Cow;
use terra_core::ActorKind;
use terra_sandbox::{CharacterProfile, SandboxState, UnitRole};

/// Returns skin, hair and outfit colors as sRGB triples, matching the procedural
/// character palette. Convert once to linear RGB when supplying vertex colors.
pub fn character_appearance_colors(profile: &CharacterProfile) -> ([f32; 3], [f32; 3], [f32; 3]) {
    const SKIN: [[f32; 3]; 8] = [
        [0.96, 0.82, 0.72],
        [0.88, 0.70, 0.55],
        [0.79, 0.59, 0.40],
        [0.68, 0.44, 0.27],
        [0.58, 0.35, 0.23],
        [0.45, 0.28, 0.18],
        [0.32, 0.20, 0.14],
        [0.22, 0.14, 0.10],
    ];
    const HAIR: [[f32; 3]; 8] = [
        [0.09, 0.07, 0.06],
        [0.21, 0.12, 0.07],
        [0.34, 0.23, 0.14],
        [0.48, 0.20, 0.10],
        [0.65, 0.44, 0.18],
        [0.79, 0.66, 0.37],
        [0.55, 0.57, 0.55],
        [0.88, 0.86, 0.78],
    ];
    const OUTFIT: [[f32; 3]; 8] = [
        [0.32, 0.46, 0.24],
        [0.69, 0.47, 0.18],
        [0.35, 0.43, 0.48],
        [0.62, 0.25, 0.16],
        [0.16, 0.41, 0.52],
        [0.43, 0.28, 0.45],
        [0.76, 0.73, 0.61],
        [0.20, 0.24, 0.23],
    ];
    (
        SKIN[usize::from(profile.skin.min(7))],
        HAIR[usize::from(profile.hair.min(7))],
        OUTFIT[usize::from(profile.outfit.min(7))],
    )
}

pub fn character_body_scale(profile: &CharacterProfile) -> [f32; 3] {
    match profile.body_style {
        1 => [1.14, 1.02, 1.12],
        2 => [0.88, 1.06, 0.94],
        _ => [1.0, 1.0, 1.0],
    }
}

/// Return a driver only when the caller's state actually contains that
/// principal's account. Other clients' public vehicle appearance is not proof
/// that their avatar is seated. No private vehicle state is required.
pub fn known_driver_vehicle(state: &SandboxState, avatar: u64) -> Option<u64> {
    let unit = state.living.units.get(&avatar)?;
    if unit.kind != ActorKind::Human || unit.role != UnitRole::Player {
        return None;
    }
    let account = state.living.accounts.get(&unit.owner?)?;
    if account.avatar_unit != avatar || !account.possessed || account.active_unit == avatar {
        return None;
    }
    let vehicle = state.transport.appearance(account.active_unit)?;
    let body = state.living.units.get(&account.active_unit)?;
    (vehicle.owner == account.player_id
        && body.owner == Some(account.player_id)
        && body.kind == ActorKind::Vehicle)
        .then_some(body.id)
}

/// The current pack can declare a hip origin for one exact catalog model.
/// The same point is usable by the model's procedural fallback during loading.
pub fn driver_seat_presentation(
    registry: &ActorSceneOverrides,
    state: &SandboxState,
    avatar: u64,
) -> Option<(u64, Vec3)> {
    registry
        .resolve(ActorKind::Human, None)?
        .animation
        .as_ref()?
        .policy
        .human
        .as_ref()?;
    let vehicle = known_driver_vehicle(state, avatar)?;
    let model = state.transport.appearance(vehicle)?.model;
    let appearance = registry.resolve_model(ActorKind::Vehicle, None, Some(model))?;
    Some((vehicle, appearance.driver_seat_translation()?))
}

pub(super) fn profile_for_body(
    state: Option<&SandboxState>,
    body: u64,
) -> Cow<'_, CharacterProfile> {
    if let Some(state) = state {
        if let Some(unit) = state.living.units.get(&body) {
            if unit.role == UnitRole::Player {
                if let Some(profile) = unit
                    .owner
                    .and_then(|owner| state.transport.profiles.get(&owner))
                {
                    return Cow::Borrowed(profile);
                }
            }
        }
    }
    // Background people have no saved customization. Keep a small deterministic
    // palette rather than assigning the local player's appearance to employees.
    Cow::Owned(CharacterProfile {
        name: String::new(),
        body_style: 0,
        skin: (body % 8) as u8,
        hair: ((body / 3) % 8) as u8,
        outfit: (body % 8) as u8,
    })
}

#[derive(Component)]
pub(super) struct HumanAppearanceInstance {
    pub materials: [Handle<StandardMaterial>; 3],
    pub key: [u8; 4],
    pub ground_offset_m: f32,
}
fn profile_key(profile: &CharacterProfile) -> [u8; 4] {
    [
        profile.body_style,
        profile.skin,
        profile.hair,
        profile.outfit,
    ]
}

pub(super) fn tint_materials(
    materials: &mut Assets<StandardMaterial>,
    handles: &[Handle<StandardMaterial>; 3],
    profile: &CharacterProfile,
) {
    let (skin, hair, outfit) = character_appearance_colors(profile);
    for (handle, color) in handles.iter().zip([skin, hair, outfit]) {
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = Color::srgb(color[0], color[1], color[2]);
        }
    }
}

pub(super) fn initial_appearance(
    handles: [Handle<StandardMaterial>; 3],
    profile: &CharacterProfile,
    ground_offset_m: f32,
) -> HumanAppearanceInstance {
    HumanAppearanceInstance {
        materials: handles,
        key: profile_key(profile),
        ground_offset_m,
    }
}

pub(super) fn update_human_appearances(
    sandbox: Option<Res<crate::SandboxRuntime>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut instances: Query<(
        &ActorSceneInstance,
        &SceneAnimationBinding,
        &mut HumanAppearanceInstance,
        &mut Transform,
    )>,
) {
    for (instance, binding, mut appearance, mut transform) in &mut instances {
        let profile = profile_for_body(sandbox.as_ref().map(|s| &s.state), instance.body_id);
        let key = profile_key(&profile);
        if appearance.key != key {
            tint_materials(&mut materials, &appearance.materials, &profile);
            appearance.key = key;
        }
        let scale = Vec3::from_array(character_body_scale(&profile)) * binding.0.scale;
        let ground_offset = if sandbox
            .as_ref()
            .is_some_and(|s| known_driver_vehicle(&s.state, instance.body_id).is_some())
        {
            0.
        } else {
            appearance.ground_offset_m
        };
        if transform.scale != scale {
            transform.scale = scale;
        }
        if transform.translation.y != ground_offset {
            transform.translation.y = ground_offset;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driver_occupancy_requires_the_actual_known_account_and_never_uses_public_appearance_alone() {
        let origin = terra_core::GeoPosition::new(48., 11., 0.).unwrap();
        let mut sandbox = crate::SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let vehicle = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == 1)
            .unwrap()
            .unit_id;
        let avatar = sandbox.state.living.accounts[&1].avatar_unit;
        let position = sandbox.state.living.units[&vehicle].position;
        sandbox.state.set_player_position(1, position).unwrap();
        sandbox
            .state
            .apply(
                1,
                terra_sandbox::Command::Transport {
                    action: terra_sandbox::TransportCommand::EnterVehicle {
                        vehicle_id: vehicle,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        sandbox
            .state
            .set_character_profile(1, CharacterProfile::default())
            .unwrap();
        let before = sandbox.state.clone();
        assert_eq!(known_driver_vehicle(&sandbox.state, avatar), Some(vehicle));
        assert_eq!(sandbox.state, before);
        // An own-account replica can render from public vehicle appearance
        // without private fuel, cargo, job or rental data.
        let public = sandbox.state.transport.appearance(vehicle).unwrap();
        sandbox.state.transport.vehicles.remove(&vehicle);
        sandbox
            .state
            .transport
            .visible_vehicles
            .insert(vehicle, public);
        assert_eq!(known_driver_vehicle(&sandbox.state, avatar), Some(vehicle));
        sandbox.state.living.accounts.remove(&1);
        assert_eq!(known_driver_vehicle(&sandbox.state, avatar), None);
        // Public owner/profile data alone cannot prove remote occupancy.
        assert!(sandbox.state.transport.profiles.contains_key(&1));
        assert!(sandbox.state.transport.appearance(vehicle).is_some());
    }
}
