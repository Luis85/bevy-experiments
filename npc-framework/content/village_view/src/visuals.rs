//! Visual entities for sim entities. Each sim entity with `Place` or
//! `Villager` gets a separate visual root entity carrying `VisualOf(sim)`
//! (villager roots are also `Selectable`). Nothing is ever inserted on the
//! sim entity itself.

use std::f32::consts::PI;

use bevy::prelude::*;
use npc_framework::prelude::{
    PrevSimPos, SimClock, SimControl, SimIndex, SimPos, SimRate, SimTime,
};
use npc_framework::view_helpers::interp::interp_alpha;
use npc_framework::view_helpers::{Selectable, ViewSystems, VisualIndex, VisualOf, sim_to_world};
use village_content::actions::Sleep;
use village_content::{Activity, Bed, Place, PlaceKind, Stockpile, Villager};

use crate::anim::VillagerModel;
use crate::assets::ViewAssets;

// Model scales (local KayKit units → metres), see plan Task 0/C.
const HOUSE_SCALE: f32 = 5.0;
const FIELD_SCALE: f32 = 3.0;
const BED_SCALE: f32 = 0.8;
const STOCK_SCALE: f32 = 5.0;
const WELL_SCALE: f32 = 3.0;
const VILLAGER_SCALE: f32 = 0.8;
/// Visible sacks are capped at this many.
const MAX_SACKS: u32 = 10;

/// Height of the bed's mattress top above the ground, in metres.
const BED_TOP: f32 = 0.48;
/// Extra yaw applied on top of the bed's yaw so the `Lie_Idle` pose lies
/// along the bed's long (local Z) axis.
const LIE_YAW: f32 = 0.0;
/// Offset (bed-local, metres) of the villager root while lying in bed.
const LIE_OFFSET: Vec3 = Vec3::new(0.0, BED_TOP, 0.55);

/// Yaw smoothing rate (1/s) when turning to face the walking direction.
const TURN_RATE: f32 = 12.0;

/// Marker on a place's visual root.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PlaceVisual;

/// State on a villager's visual root.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct VillagerVisual {
    /// Current (smoothed) yaw in radians; model +Z faces this way.
    yaw: f32,
    /// Interpolation endpoints in metres and the sim tick they belong to.
    from: Vec3,
    to: Vec3,
    tick: Option<u64>,
}

/// Row of sacks next to the stockpile showing `min(food, MAX_SACKS)`.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct SackPile {
    sim: Entity,
    shown: Option<u32>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (despawn_visuals, spawn_visuals)
            .chain()
            .in_set(ViewSystems::Sync),
    )
    .add_systems(
        Update,
        (place_transforms, villager_transforms, update_sacks).in_set(ViewSystems::Animate),
    );
}

fn model(
    scene: &Handle<bevy::world_serialization::WorldAsset>,
    transform: Transform,
) -> impl Bundle {
    (WorldAssetRoot(scene.clone()), transform)
}

fn spawn_visuals(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    mut index: ResMut<VisualIndex>,
    places: Query<(Entity, &Place, &SimPos)>,
    villagers: Query<(Entity, &SimPos), (With<Villager>, Without<Place>)>,
) {
    let Some(assets) = assets else { return };

    for (sim, place, pos) in &places {
        if index.map.contains_key(&sim) {
            continue;
        }
        let root = commands
            .spawn((
                Name::new(format!("visual {:?}", place.kind)),
                VisualOf(sim),
                PlaceVisual,
                place_transform(place, *pos),
                Visibility::default(),
            ))
            .id();
        match place.kind {
            PlaceKind::House => {
                commands.spawn((
                    ChildOf(root),
                    model(
                        &assets.house,
                        Transform::from_scale(Vec3::splat(HOUSE_SCALE)),
                    ),
                ));
            }
            PlaceKind::Field => {
                commands.spawn((
                    ChildOf(root),
                    model(
                        &assets.field,
                        Transform::from_scale(Vec3::splat(FIELD_SCALE)),
                    ),
                ));
            }
            PlaceKind::Bed => {
                commands.spawn((
                    ChildOf(root),
                    model(&assets.bed, Transform::from_scale(Vec3::splat(BED_SCALE))),
                ));
            }
            PlaceKind::Stockpile => {
                let s = Vec3::splat(STOCK_SCALE);
                commands.spawn((
                    ChildOf(root),
                    model(
                        &assets.crate_big,
                        Transform::from_xyz(-0.75, 0.0, -0.2).with_scale(s),
                    ),
                ));
                commands.spawn((
                    ChildOf(root),
                    model(
                        &assets.barrel,
                        Transform::from_xyz(0.45, 0.0, -0.35)
                            .with_rotation(Quat::from_rotation_y(0.4))
                            .with_scale(s),
                    ),
                ));
                commands.spawn((
                    ChildOf(root),
                    SackPile { sim, shown: None },
                    Transform::default(),
                    Visibility::default(),
                ));
            }
            PlaceKind::Well => {
                let s = Vec3::splat(WELL_SCALE);
                commands.spawn((
                    ChildOf(root),
                    model(&assets.stone, Transform::from_scale(s)),
                ));
                commands.spawn((
                    ChildOf(root),
                    model(
                        &assets.barrel,
                        Transform::from_xyz(0.0, 0.0, 0.9).with_scale(Vec3::splat(STOCK_SCALE)),
                    ),
                ));
            }
        }
        index.map.insert(sim, root);
    }

    for (sim, pos) in &villagers {
        if index.map.contains_key(&sim) {
            continue;
        }
        let at = sim_to_world(*pos);
        let root = commands
            .spawn((
                Name::new("visual Villager"),
                VisualOf(sim),
                Selectable,
                VillagerVisual {
                    from: at,
                    to: at,
                    ..default()
                },
                Transform::from_translation(at),
                Visibility::default(),
            ))
            .id();
        commands
            .spawn((
                ChildOf(root),
                VillagerModel { root },
                model(
                    &assets.villager,
                    Transform::from_scale(Vec3::splat(VILLAGER_SCALE)),
                ),
            ))
            .observe(crate::anim::on_villager_ready);
        index.map.insert(sim, root);
    }
}

/// Despawns visuals whose sim entity is gone (or no longer a place/villager).
fn despawn_visuals(
    mut commands: Commands,
    mut index: ResMut<VisualIndex>,
    sims: Query<(), Or<(With<Place>, With<Villager>)>>,
    visuals: Query<(), With<VisualOf>>,
) {
    index.map.retain(|&sim, &mut visual| {
        if sims.contains(sim) {
            return true;
        }
        if visuals.contains(visual) {
            commands.entity(visual).despawn();
        }
        false
    });
}

fn place_transform(place: &Place, pos: SimPos) -> Transform {
    Transform::from_translation(sim_to_world(pos))
        .with_rotation(Quat::from_rotation_y((place.yaw_deg as f32).to_radians()))
}

fn place_transforms(
    sims: Query<(&Place, &SimPos)>,
    mut visuals: Query<(&VisualOf, &mut Transform), With<PlaceVisual>>,
) {
    for (of, mut transform) in &mut visuals {
        if let Ok((place, pos)) = sims.get(of.0) {
            let want = place_transform(place, *pos);
            if *transform != want {
                *transform = want;
            }
        }
    }
}

fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let mut d = (to - from) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    } else if d < -PI {
        d += 2.0 * PI;
    }
    from + d * t
}

#[allow(clippy::too_many_arguments)]
fn villager_transforms(
    time: Res<Time>,
    (sim_time, rate, control): (
        Option<Res<SimTime>>,
        Option<Res<SimRate>>,
        Option<Res<SimControl>>,
    ),
    clock: Option<Res<SimClock>>,
    sims: Query<
        (
            &SimPos,
            Option<&PrevSimPos>,
            Option<&Activity>,
            Option<&Sleep>,
        ),
        With<Villager>,
    >,
    index: Option<Res<SimIndex>>,
    beds: Query<(&Place, &SimPos), With<Bed>>,
    mut visuals: Query<(&VisualOf, &mut Transform, &mut VillagerVisual)>,
) {
    let alpha = interp_alpha(sim_time.as_deref(), rate.as_deref(), control.as_deref());
    let tick = clock.map(|c| c.tick);
    let turn = 1.0 - (-TURN_RATE * time.delta_secs()).exp();

    for (of, mut transform, mut vis) in &mut visuals {
        let Ok((pos, prev, activity, sleep)) = sims.get(of.0) else {
            continue;
        };
        let now = sim_to_world(*pos);
        let prev = prev.map_or(now, |p| sim_to_world(p.0));

        // Interpolation endpoints. At 1× they are exactly PrevSimPos → SimPos.
        // When several ticks ran since the last frame we saw (speed > 1×),
        // interpolate from the last position we drew towards so motion stays
        // continuous instead of jumping by `multiplier - 1` ticks.
        match (tick, vis.tick) {
            (Some(t), Some(seen)) if t == seen => {}
            (Some(t), Some(seen)) if t == seen + 1 => {
                vis.from = prev;
                vis.to = now;
            }
            (Some(_), Some(_)) => {
                vis.from = vis.to;
                vis.to = now;
            }
            _ => {
                vis.from = prev;
                vis.to = now;
            }
        }
        vis.tick = tick;
        if vis.to != now {
            // SimPos changed without a tick change (e.g. no clock): follow it.
            vis.from = prev;
            vis.to = now;
        }

        if activity == Some(&Activity::Sleep) {
            // Lie in the bed the Sleep action actually uses.
            let bed = sleep
                .zip(index.as_deref())
                .and_then(|(sleep, index)| index.get(sleep.bed))
                .and_then(|e| beds.get(e).ok());
            if let Some((place, bed_pos)) = bed {
                let bed_rot = Quat::from_rotation_y((place.yaw_deg as f32).to_radians());
                transform.translation = sim_to_world(*bed_pos) + bed_rot * LIE_OFFSET;
                transform.rotation = bed_rot * Quat::from_rotation_y(LIE_YAW);
                continue;
            }
        }

        let at = vis.from.lerp(vis.to, alpha);
        let delta = vis.to - vis.from;
        if delta.length_squared() > 1e-8 {
            let target = delta.x.atan2(delta.z);
            vis.yaw = lerp_angle(vis.yaw, target, turn);
        }
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(vis.yaw);
    }
}

fn update_sacks(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    stockpiles: Query<&Stockpile>,
    mut piles: Query<(Entity, &mut SackPile)>,
) {
    let Some(assets) = assets else { return };
    for (entity, mut pile) in &mut piles {
        let Ok(stock) = stockpiles.get(pile.sim) else {
            continue;
        };
        let want = stock.food.min(MAX_SACKS);
        if pile.shown == Some(want) {
            continue;
        }
        pile.shown = Some(want);
        commands.entity(entity).despawn_children();
        for i in 0..want {
            // Two rows of five beside the crate and barrel, away from the
            // approach point (which is in front, +Z).
            let (row, col) = (i / 5, i % 5);
            let x = 1.3 + col as f32 * 0.55;
            let z = -0.55 + row as f32 * 0.85;
            let yaw = ((i * 37) % 11) as f32 * 0.12 - 0.6;
            commands.spawn((
                ChildOf(entity),
                model(
                    &assets.sack,
                    Transform::from_xyz(x, 0.0, z)
                        .with_rotation(Quat::from_rotation_y(yaw))
                        .with_scale(Vec3::splat(STOCK_SCALE)),
                ),
            ));
        }
    }
}
