use crate::{MapKind, Point, ScenarioConfig};

pub const RIVER_HALF_WIDTH: f32 = 5.2;
pub const BRIDGE_HALF_WIDTH: f32 = 5.5;

/// Derived dimensions keep even a million-unit asymmetric wedge inside bounds.
pub fn arena_half_extent(config: &ScenarioConfig) -> f32 {
    let spacing = config
        .blue
        .category
        .spawn_spacing(config.blue.kind)
        .max(config.red.category.spawn_spacing(config.red.kind));
    (26.0 + (config.blue.count.max(config.red.count) as f32).sqrt() * 1.3 * spacing / 1.15)
        .max(58.0)
}

/// Ground or bridge-deck height used by both simulation consumers and renderer.
pub fn terrain_height(map: MapKind, x: f32, z: f32) -> f32 {
    match map {
        MapKind::Meadow => (x * 0.065).sin() * (z * 0.055).cos() * 0.28,
        MapKind::Valley => {
            let ridge = (z * 0.035).sin();
            ridge * ridge * 4.4 + (x * 0.04).sin() * 0.25
        }
        MapKind::Crossing => {
            if x.abs() <= RIVER_HALF_WIDTH + 1.0 && z.abs() <= BRIDGE_HALF_WIDTH {
                0.35
            } else if x.abs() < RIVER_HALF_WIDTH {
                -1.0
            } else {
                let bank = ((x.abs() - RIVER_HALF_WIDTH) / 3.0).clamp(0.0, 1.0);
                bank * 0.18 + (x * 0.05).sin() * (z * 0.07).cos() * 0.12
            }
        }
    }
}

pub fn is_walkable(map: MapKind, x: f32, z: f32) -> bool {
    map != MapKind::Crossing || x.abs() >= RIVER_HALF_WIDTH || z.abs() <= BRIDGE_HALF_WIDTH
}

pub(crate) fn speed_factor(map: MapKind, position: Point) -> f32 {
    if map == MapKind::Valley {
        1.0 - (position.y * 0.035).sin().powi(2) * 0.23
    } else {
        1.0
    }
}

/// A deliberately simple portal route for the starter map. Keeping a safety
/// margin at bank and bridge edges prevents a diagonal shortcut through water.
pub(crate) fn route_direction(map: MapKind, position: Point, goal: Point) -> Point {
    if map != MapKind::Crossing {
        return (goal - position).normalized_or_zero();
    }
    let opposite_bank = position.x * goal.x < 0.0;
    let entering_river = goal.x.abs() < RIVER_HALF_WIDTH + 1.0;
    if opposite_bank || entering_river {
        let lane = goal.y.clamp(-3.8, 3.8);
        if position.x.abs() > RIVER_HALF_WIDTH + 1.0 && position.y.abs() > 4.2 {
            let approach = Point::new(position.x.signum() * 7.3, lane);
            return (approach - position).normalized_or_zero();
        }
        if position.x.abs() < 7.4 {
            let exit = Point::new(goal.x.signum() * 7.4, lane);
            return (exit - position).normalized_or_zero();
        }
    }
    (goal - position).normalized_or_zero()
}

/// Correct a ground movement without teleporting across an impassable area.
pub(crate) fn constrain_move(map: MapKind, from: Point, mut to: Point, extent: f32) -> Point {
    to.x = to.x.clamp(-extent + 1.0, extent - 1.0);
    to.y = to.y.clamp(-extent + 1.0, extent - 1.0);
    if map != MapKind::Crossing {
        return to;
    }
    // Tick travel is <1m. Test midpoint too, keeping this correct if a speed
    // preset increases later. Fast physical projectiles require swept tests.
    let midpoint = from.lerp(to, 0.5);
    if is_walkable(map, to.x, to.y) && is_walkable(map, midpoint.x, midpoint.y) {
        return to;
    }
    let along_z = Point::new(from.x, to.y);
    if is_walkable(map, along_z.x, along_z.y) {
        return along_z;
    }
    let along_x = Point::new(to.x, from.y);
    if is_walkable(map, along_x.x, along_x.y) {
        return along_x;
    }
    from
}
