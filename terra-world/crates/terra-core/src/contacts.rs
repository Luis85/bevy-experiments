//! Conservative swept ground-body contacts. This is a kinematic admission
//! primitive, not a rigid-body impulse, suspension or pedestrian injury model.
//! Shapes are oriented rectangular prisms with their position at ground level.

use crate::{GeoPosition, LocalFrame, VehicleError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactBody {
    pub id: u64,
    pub position: GeoPosition,
    pub heading_deg: f64,
    /// Width, height, length, in metres.
    pub dimensions_m: [f64; 3],
}

impl ContactBody {
    pub fn validate(&self) -> Result<(), VehicleError> {
        if self.position.validate().is_err()
            || !self.heading_deg.is_finite()
            || self
                .dimensions_m
                .iter()
                .any(|v| !v.is_finite() || !(0.1..=32.).contains(v))
        {
            return Err(VehicleError::Invalid);
        }
        Ok(())
    }

    pub fn horizontal_radius_m(&self) -> f64 {
        self.dimensions_m[0].hypot(self.dimensions_m[2]) * 0.5
    }
}

/// Test an oriented body against a stationary, upright circular footprint.
/// Both positions are ground/base elevations. This is an admission query for
/// newly placed objects; unlike a movement sweep, existing overlap cannot be
/// treated as an escape. The circle/rectangle test retains usable space beside
/// a long vehicle instead of reserving its entire bounding circle.
pub fn body_overlaps_column(
    body: ContactBody,
    base: GeoPosition,
    radius_m: f64,
    height_m: f64,
) -> Result<bool, VehicleError> {
    body.validate()?;
    if base.validate().is_err()
        || !radius_m.is_finite()
        || !(0.0..=100.).contains(&radius_m)
        || !height_m.is_finite()
        || !(0.1..=100.).contains(&height_m)
    {
        return Err(VehicleError::Invalid);
    }
    if body.position.altitude_m + body.dimensions_m[1] < base.altitude_m
        || base.altitude_m + height_m < body.position.altitude_m
        || body.position.surface_distance_m(base) > body.horizontal_radius_m() + radius_m + 0.01
    {
        return Ok(false);
    }
    let local = LocalFrame::new(body.position).to_local(base);
    let offset = [local[0], -local[2], 0.];
    let basis = axes(body.heading_deg.rem_euclid(360.).to_radians());
    let right = (dot(offset, basis[0]).abs() - body.dimensions_m[0] * 0.5).max(0.);
    let forward = (dot(offset, basis[1]).abs() - body.dimensions_m[2] * 0.5).max(0.);
    Ok(right.hypot(forward) <= radius_m + 1e-7)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyContact {
    pub other_id: u64,
    /// Earliest conservative time of contact along the proposed motion, 0..1.
    pub fraction: f64,
    /// Unit normal pointing away from the obstacle; physical east/north/up axes.
    pub normal_enu: [f64; 3],
}

/// Sweep a body against a stationary body. Translation is continuous (there is
/// no distance-based sampling gap). Rotation is divided into at most 90 two-degree
/// intervals and each midpoint box is expanded by its maximum angular excursion.
/// Vertical admission is continuous too, so bridge traffic does not block roads
/// beneath it. A saved/spawned overlap can escape if it moves monotonically away
/// without increasing endpoint penetration; it cannot move deeper or turn in place.
pub fn sweep_body_contact(
    moving: ContactBody,
    destination: GeoPosition,
    destination_heading_deg: f64,
    other: ContactBody,
) -> Result<Option<BodyContact>, VehicleError> {
    moving.validate()?;
    other.validate()?;
    if destination.validate().is_err() || !destination_heading_deg.is_finite() {
        return Err(VehicleError::Invalid);
    }
    if moving.id == other.id {
        return Ok(None);
    }
    let surface_distance = moving.position.surface_distance_m(destination);
    if surface_distance > 250. {
        return Err(VehicleError::Invalid);
    }
    if moving.position.surface_distance_m(other.position)
        > surface_distance + moving.horizontal_radius_m() + other.horizontal_radius_m() + 1.
    {
        return Ok(None);
    }
    let frame = LocalFrame::new(moving.position);
    let to = frame.to_local(destination);
    let obstacle = frame.to_local(other.position);
    // Use explicit geodetic heights. ENU up contains Earth curvature, which
    // must not make a flat, long sweep climb into an overhead bridge collider.
    let displacement = [
        to[0],
        -to[2],
        destination.altitude_m - moving.position.altitude_m,
    ];
    let start = [
        -obstacle[0],
        obstacle[2],
        moving.position.altitude_m - other.position.altitude_m,
    ];
    if displacement.iter().any(|v| !v.is_finite())
        || displacement[0].hypot(displacement[1]) > 250.
        || displacement[2].abs() > 250.
    {
        return Err(VehicleError::Invalid);
    }
    let start_heading = moving.heading_deg.rem_euclid(360.);
    let end_heading = destination_heading_deg.rem_euclid(360.);
    let turn = ((end_heading - start_heading + 180.).rem_euclid(360.) - 180.).to_radians();
    let heading = start_heading.to_radians();
    let other_heading = other.heading_deg.rem_euclid(360.).to_radians();
    let start_depth = penetration(
        start,
        heading,
        moving.dimensions_m,
        other_heading,
        other.dimensions_m,
    );
    if let Some(depth) = start_depth {
        let end = std::array::from_fn(|i| start[i] + displacement[i]);
        let end_depth = penetration(
            end,
            heading + turn,
            moving.dimensions_m,
            other_heading,
            other.dimensions_m,
        )
        .unwrap_or(0.);
        let outward =
            start[0] * displacement[0] + start[1] * displacement[1] + start[2] * displacement[2];
        if displacement.iter().map(|v| v * v).sum::<f64>() > 1e-12
            && outward >= -1e-8
            && end_depth <= depth + 1e-7
        {
            return Ok(None);
        }
    }
    let segments = ((turn.abs().to_degrees() / 2.).ceil() as usize).clamp(1, 90);
    for segment in 0..segments {
        let a = segment as f64 / segments as f64;
        let b = (segment + 1) as f64 / segments as f64;
        let orientation = heading + turn * (a + b) * 0.5;
        // The farthest corner travels a chord from the interval midpoint to
        // either end: 2*r*sin(angular_excursion/2), not r*sin(excursion).
        let expansion = 2. * moving.horizontal_radius_m() * (turn.abs() * (b - a) * 0.25).sin();
        let segment_start = std::array::from_fn(|i| start[i] + displacement[i] * a);
        let segment_delta = displacement.map(|v| v * (b - a));
        if let Some((fraction, normal)) = swept_prisms(
            segment_start,
            segment_delta,
            orientation,
            moving.dimensions_m,
            other_heading,
            other.dimensions_m,
            expansion,
        ) {
            return Ok(Some(BodyContact {
                other_id: other.id,
                fraction: (a + fraction * (b - a)).clamp(0., 1.),
                normal_enu: normal,
            }));
        }
    }
    Ok(None)
}

fn axes(heading: f64) -> [[f64; 3]; 2] {
    let (s, c) = heading.sin_cos();
    [[c, -s, 0.], [s, c, 0.]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn projection(axis: [f64; 3], heading: f64, dimensions: [f64; 3]) -> f64 {
    let basis = axes(heading);
    dot(axis, basis[0]).abs() * dimensions[0] * 0.5
        + dot(axis, basis[1]).abs() * dimensions[2] * 0.5
        + axis[2].abs() * dimensions[1] * 0.5
}
fn all_axes(a: f64, b: f64) -> [[f64; 3]; 5] {
    let aa = axes(a);
    let bb = axes(b);
    [aa[0], aa[1], bb[0], bb[1], [0., 0., 1.]]
}
fn penetration(
    mut separation: [f64; 3],
    a: f64,
    da: [f64; 3],
    b: f64,
    db: [f64; 3],
) -> Option<f64> {
    separation[2] += (da[1] - db[1]) * 0.5;
    let mut depth = f64::INFINITY;
    for axis in all_axes(a, b) {
        let overlap =
            projection(axis, a, da) + projection(axis, b, db) - dot(axis, separation).abs();
        if overlap < -1e-7 {
            return None;
        }
        depth = depth.min(overlap.max(0.));
    }
    Some(depth)
}
fn swept_prisms(
    mut separation: [f64; 3],
    delta: [f64; 3],
    a: f64,
    da: [f64; 3],
    b: f64,
    db: [f64; 3],
    expansion: f64,
) -> Option<(f64, [f64; 3])> {
    separation[2] += (da[1] - db[1]) * 0.5;
    let mut enter: f64 = 0.;
    let mut leave: f64 = 1.;
    let mut normal = [0., 0., 0.];
    let mut nearest = f64::INFINITY;
    for axis in all_axes(a, b) {
        let extent = projection(axis, a, da)
            + projection(axis, b, db)
            + if axis[2] == 0. { expansion } else { 0. };
        let p = dot(axis, separation);
        let v = dot(axis, delta);
        let depth = extent - p.abs();
        if depth < nearest {
            nearest = depth;
            // At coincident centres, oppose the attempted motion.
            let sign = if p.abs() > 1e-9 {
                p.signum()
            } else if v.abs() > 1e-9 {
                -v.signum()
            } else {
                1.
            };
            if enter == 0. {
                normal = axis.map(|x| x * sign);
            }
        }
        if v.abs() < 1e-12 {
            if p.abs() > extent {
                return None;
            }
            continue;
        }
        let t1 = (-extent - p) / v;
        let t2 = (extent - p) / v;
        let near = t1.min(t2);
        let far = t1.max(t2);
        if near > enter {
            enter = near;
            normal = axis.map(|x| x * (p + v * near).signum());
        }
        leave = leave.min(far);
        if enter > leave {
            return None;
        }
    }
    if leave <= 1e-9 || enter > 1. {
        return None;
    }
    Some((enter.max(0.), normal))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_column_uses_oriented_dimensions_and_vertical_clearance() {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let frame = LocalFrame::new(origin);
        let point = |east: f64, north: f64, height: f64| {
            let mut point = frame.to_geo([east, 0., -north]).unwrap();
            point.altitude_m = 100. + height;
            point
        };
        let bus = ContactBody {
            id: 1,
            position: origin,
            heading_deg: 90.,
            dimensions_m: [2.65, 3.25, 10.8],
        };
        assert!(body_overlaps_column(bus, point(4., 0., 0.), 0.7, 2.5).unwrap());
        assert!(!body_overlaps_column(bus, point(0., 3., 0.), 1., 2.5).unwrap());
        let turned = ContactBody {
            heading_deg: 0.,
            ..bus
        };
        assert!(!body_overlaps_column(turned, point(4., 0., 0.), 0.7, 2.5).unwrap());
        assert!(body_overlaps_column(turned, point(0., 4., 0.), 0.7, 2.5).unwrap());
        // A circular footprint must not fill the square beyond a body corner.
        assert!(!body_overlaps_column(bus, point(5.9, 1.825, 0.), 0.7, 2.5).unwrap());
        assert!(body_overlaps_column(bus, point(5.9, 1.825, 0.), 0.72, 2.5).unwrap());
        assert!(!body_overlaps_column(bus, point(4., 0., 3.3), 0.7, 2.5).unwrap());
        assert!(!body_overlaps_column(bus, point(4., 0., -3.), 0.7, 2.5).unwrap());
        assert!(body_overlaps_column(bus, origin, f64::NAN, 2.5).is_err());
        assert!(body_overlaps_column(bus, origin, 1., 0.).is_err());
    }

    fn body(id: u64, east: f64, north: f64) -> ContactBody {
        ContactBody {
            id,
            position: LocalFrame::new(GeoPosition::new(48., 11., 10.).unwrap())
                .to_geo([east, 0., -north])
                .unwrap(),
            heading_deg: 0.,
            dimensions_m: [2., 2., 4.],
        }
    }
    #[test]
    fn continuous_sweep_stops_fast_body_at_first_contact() {
        let moving = body(1, 0., 0.);
        let other = body(2, 0., 20.);
        let hit = sweep_body_contact(moving, body(3, 0., 40.).position, 0., other)
            .unwrap()
            .unwrap();
        assert!((hit.fraction - 0.4).abs() < 0.0001);
        assert!(hit.normal_enu[1] < -0.99);
    }
    #[test]
    fn adjacent_lanes_and_overpass_are_separate() {
        let moving = body(1, 0., 0.);
        let end = body(3, 0., 40.).position;
        assert!(
            sweep_body_contact(moving, end, 0., body(2, 2.2, 20.))
                .unwrap()
                .is_none()
        );
        let mut overhead = body(2, 0., 20.);
        overhead.position.altitude_m += 5.;
        assert!(
            sweep_body_contact(moving, end, 0., overhead)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn initial_overlap_can_escape_but_not_push_deeper() {
        let moving = body(1, 0., 0.);
        let other = body(2, 0., 3.);
        assert!(
            sweep_body_contact(moving, body(3, 0., -1.).position, 0., other)
                .unwrap()
                .is_none()
        );
        assert!(
            sweep_body_contact(moving, body(3, 0., 1.).position, 0., other)
                .unwrap()
                .is_some()
        );
        let same = body(2, 0., 0.);
        let stationary = sweep_body_contact(moving, moving.position, 0., same)
            .unwrap()
            .unwrap();
        assert!((stationary.normal_enu.iter().map(|v| v * v).sum::<f64>() - 1.).abs() < 1e-9);
        assert!(
            sweep_body_contact(moving, body(3, 2., 0.).position, 90., same)
                .unwrap()
                .is_none()
        );
        assert!(
            sweep_body_contact(moving, body(3, 0., 0.1).position, 0., same)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn turning_corner_sweeps_and_touching_body_can_depart() {
        let moving = body(1, 0., 0.);
        let other = ContactBody {
            dimensions_m: [0.2, 1., 0.2],
            ..body(2, 1.6, 1.6)
        };
        assert!(
            sweep_body_contact(moving, moving.position, 90., other)
                .unwrap()
                .is_some()
        );
        assert!(
            sweep_body_contact(moving, body(3, 0., -1.).position, 0., body(2, 0., 4.))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn slope_vertical_crossing_is_continuous() {
        let moving = body(1, 0., 0.);
        let mut end = body(3, 0., 20.).position;
        end.altitude_m += 8.;
        let mut other = body(2, 0., 10.);
        other.position.altitude_m += 4.;
        assert!(
            sweep_body_contact(moving, end, 0., other)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn invalid_input_never_returns_a_clear_path() {
        let moving = body(1, 0., 0.);
        let other = body(2, 0., 10.);
        assert!(sweep_body_contact(moving, moving.position, f64::NAN, other).is_err());
        let bad = ContactBody {
            dimensions_m: [f64::INFINITY, 1., 1.],
            ..moving
        };
        assert!(sweep_body_contact(bad, moving.position, 0., other).is_err());
        assert!(sweep_body_contact(moving, body(3, 0., 300.).position, 0., other).is_err());
        let opposite = GeoPosition::new(-48., -169., 10.).unwrap();
        assert!(sweep_body_contact(moving, opposite, 0., other).is_err());
        let extreme = ContactBody {
            heading_deg: -f64::MAX,
            ..moving
        };
        if let Some(hit) =
            sweep_body_contact(extreme, body(3, 0., 20.).position, f64::MAX, other).unwrap()
        {
            assert!(hit.fraction.is_finite());
            assert!(hit.normal_enu.iter().all(|v| v.is_finite()));
        }
    }
    #[test]
    fn seam_and_polar_frames_remain_finite() {
        for origin in [
            GeoPosition::new(0., 179.99999, 10.).unwrap(),
            GeoPosition::new(89.9999, 30., 10.).unwrap(),
        ] {
            let frame = LocalFrame::new(origin);
            let moving = ContactBody {
                position: origin,
                heading_deg: 90.,
                ..body(1, 0., 0.)
            };
            let other = ContactBody {
                position: frame.to_geo([10., 0., 0.]).unwrap(),
                heading_deg: 90.,
                ..body(2, 0., 0.)
            };
            let hit = sweep_body_contact(moving, frame.to_geo([20., 0., 0.]).unwrap(), 90., other)
                .unwrap()
                .unwrap();
            assert!(hit.fraction.is_finite() && (0.2..0.4).contains(&hit.fraction));
        }
    }
}
