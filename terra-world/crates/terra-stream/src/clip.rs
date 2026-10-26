use terra_core::{FeatureKind, GeoBounds, GeoPosition, TileKey, WorldFeature};

/// Clip downloaded ways to the resident chunk. A long OSM road must not create
/// kilometre-scale duplicate meshes in every intersecting chunk. Preserve source
/// identity in properties when a polyline produces multiple fragments.
pub(crate) fn to_tile(features: Vec<WorldFeature>, key: TileKey) -> Vec<WorldFeature> {
    let bounds = key.bounds();
    let center = (bounds.east + bounds.west) * 0.5;
    let mut result = Vec::new();
    for mut feature in features {
        for point in &mut feature.points {
            point.longitude = center + (point.longitude - center + 180.0).rem_euclid(360.0) - 180.0;
        }
        if feature.kind == FeatureKind::Tree {
            if feature.points.iter().any(|p| within(*p, bounds)) {
                result.push(feature);
            }
        } else if feature.kind == FeatureKind::Road {
            let mut fragments: Vec<Vec<GeoPosition>> = Vec::new();
            for pair in feature.points.windows(2) {
                if let Some((a, b)) = segment(pair[0], pair[1], bounds) {
                    if let Some(last) = fragments.last_mut().filter(|line| line.last() == Some(&a))
                    {
                        last.push(b);
                    } else {
                        fragments.push(vec![a, b]);
                    }
                }
            }
            for (index, points) in fragments.into_iter().enumerate() {
                let mut fragment = feature.clone();
                fragment
                    .properties
                    .insert("source_id".into(), feature.id.clone());
                fragment.id = format!(
                    "{}:clip:{}:{}:{}:{index}",
                    feature.id, key.level, key.x, key.y
                );
                fragment.points = points;
                result.push(fragment);
            }
        } else {
            let points = polygon(&feature.points, bounds);
            if points.len() >= 4 {
                feature.points = points;
                result.push(feature);
            }
        }
    }
    result
}

fn within(p: GeoPosition, b: GeoBounds) -> bool {
    p.longitude >= b.west && p.longitude <= b.east && p.latitude >= b.south && p.latitude <= b.north
}

fn interpolate(a: GeoPosition, b: GeoPosition, t: f64) -> GeoPosition {
    GeoPosition {
        latitude: a.latitude + (b.latitude - a.latitude) * t,
        longitude: a.longitude + (b.longitude - a.longitude) * t,
        altitude_m: a.altitude_m + (b.altitude_m - a.altitude_m) * t,
    }
}

fn segment(
    a: GeoPosition,
    b: GeoPosition,
    bounds: GeoBounds,
) -> Option<(GeoPosition, GeoPosition)> {
    let dx = b.longitude - a.longitude;
    let dy = b.latitude - a.latitude;
    let mut enter = 0.0f64;
    let mut leave = 1.0f64;
    for (p, q) in [
        (-dx, a.longitude - bounds.west),
        (dx, bounds.east - a.longitude),
        (-dy, a.latitude - bounds.south),
        (dy, bounds.north - a.latitude),
    ] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            enter = enter.max(t);
        } else {
            leave = leave.min(t);
        }
        if enter > leave {
            return None;
        }
    }
    let first = interpolate(a, b, enter);
    let last = interpolate(a, b, leave);
    (first != last).then_some((first, last))
}

fn polygon(input: &[GeoPosition], bounds: GeoBounds) -> Vec<GeoPosition> {
    let mut points = input.to_vec();
    if points.first() == points.last() {
        points.pop();
    }
    for edge in 0..4 {
        let inside = |p: GeoPosition| match edge {
            0 => p.longitude >= bounds.west,
            1 => p.longitude <= bounds.east,
            2 => p.latitude >= bounds.south,
            _ => p.latitude <= bounds.north,
        };
        let intersect = |a: GeoPosition, b: GeoPosition| {
            let t = match edge {
                0 => (bounds.west - a.longitude) / (b.longitude - a.longitude),
                1 => (bounds.east - a.longitude) / (b.longitude - a.longitude),
                2 => (bounds.south - a.latitude) / (b.latitude - a.latitude),
                _ => (bounds.north - a.latitude) / (b.latitude - a.latitude),
            };
            interpolate(a, b, t.clamp(0.0, 1.0))
        };
        let mut output = Vec::new();
        if let Some(&last) = points.last() {
            let mut a = last;
            for &b in &points {
                let (a_in, b_in) = (inside(a), inside(b));
                if a_in != b_in {
                    output.push(intersect(a, b));
                }
                if b_in {
                    output.push(b);
                }
                a = b;
            }
        }
        points = output;
    }
    points.dedup();
    if points.len() >= 3 {
        points.push(points[0]);
    } else {
        points.clear();
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(lat: f64, lon: f64) -> GeoPosition {
        GeoPosition::new(lat, lon, 0.0).unwrap()
    }
    #[test]
    fn crossing_segment_is_limited_and_outside_segment_removed() {
        let b = GeoBounds {
            south: 0.0,
            west: 0.0,
            north: 1.0,
            east: 1.0,
        };
        let (a, z) = segment(point(0.5, -1.0), point(0.5, 2.0), b).unwrap();
        assert_eq!(a, point(0.5, 0.0));
        assert_eq!(z, point(0.5, 1.0));
        assert!(segment(point(2.0, -1.0), point(2.0, 2.0), b).is_none());
    }
    #[test]
    fn polygon_clipping_keeps_closed_boundary() {
        let b = GeoBounds {
            south: 0.0,
            west: 0.0,
            north: 1.0,
            east: 1.0,
        };
        let clipped = polygon(
            &[
                point(-1.0, -1.0),
                point(-1.0, 2.0),
                point(2.0, 2.0),
                point(2.0, -1.0),
                point(-1.0, -1.0),
            ],
            b,
        );
        assert_eq!(clipped.len(), 5);
        assert_eq!(clipped.first(), clipped.last());
        assert!(clipped.iter().all(|&p| within(p, b)));
    }
}
