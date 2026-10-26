use std::collections::BTreeMap;
use terra_core::{
    FeatureKind, GeoPosition, HeightField, SourceRecord, TileKey, WorldChunk, WorldFeature,
};

/// Continuous deterministic synthetic relief. Integer longitude frequencies and
/// latitude taper preserve date-line and pole continuity; no real survey claim.
pub fn procedural_height(position: GeoPosition, seed: u64) -> f32 {
    let raw = raw_height(position, seed);
    // Shape the authored pond basin in the same continuous elevation function.
    // Its blend ends before the level-15 tile edges, preserving shared samples.
    let Ok(tile) = TileKey::at(position, 15) else {
        return raw;
    };
    let [u, v] = tile.bounds().uv(position);
    let distance = ((u - 0.91) / 0.075).abs().max(((v - 0.735) / 0.12).abs());
    if distance >= 1.1 {
        return raw;
    }
    let center = tile.bounds().point(0.91, 0.735, 0.).unwrap_or(position);
    let basin = raw_height(center, seed) - 1.2;
    let amount = ((1.1 - distance) / 0.1).clamp(0., 1.) as f32;
    let smooth = amount * amount * (3. - 2. * amount);
    raw * (1. - smooth) + basin * smooth
}

fn raw_height(position: GeoPosition, seed: u64) -> f32 {
    let latitude = position.latitude.to_radians();
    let longitude = position.longitude.to_radians();
    let phase = (seed % 10_000) as f64 * 0.017;
    let taper = latitude.cos().powi(2);
    let continental = taper * ((longitude * 7. + phase).sin() * 65. + (latitude * 11.).cos() * 55.);
    let hills = taper * ((longitude * 1800. + phase).sin() * (latitude * 1400.).cos() * 95.);
    let ridges = taper * ((longitude * 4100. + latitude * 3700.).sin().abs() - 0.5) * 42.;
    let detail = taper * ((longitude * 12_000. + phase).sin() * (latitude * 11_000.).cos() * 8.);
    (170. + continental + hills + ridges + detail) as f32
}

/// Fully authored offline example: connected roads, small buildings, tagged
/// services and agricultural land. This is not a reconstruction of its GPS site.
pub fn procedural_chunk(key: TileKey, resolution: usize, seed: u64) -> WorldChunk {
    let resolution = resolution.clamp(2, 257);
    let bounds = key.bounds();
    let position = |u: f64, v: f64| {
        let u = u.clamp(0., 1.);
        let v = v.clamp(0., 1.);
        let mut p = GeoPosition {
            latitude: bounds.north - (bounds.north - bounds.south) * v,
            longitude: bounds.west + (bounds.east - bounds.west) * u,
            altitude_m: 0.,
        };
        p.altitude_m = procedural_height(p, seed) as f64;
        p
    };
    let samples_m = (0..resolution)
        .flat_map(|row| {
            let position = &position;
            (0..resolution).map(move |col| {
                position(
                    col as f64 / (resolution - 1) as f64,
                    row as f64 / (resolution - 1) as f64,
                )
                .altitude_m as f32
            })
        })
        .collect();
    let span_x = position(0., 0.5)
        .surface_distance_m(position(1., 0.5))
        .max(1.);
    let span_z = position(0.5, 0.)
        .surface_distance_m(position(0.5, 1.))
        .max(1.);
    let mut features = Vec::new();
    let mut add = |suffix: &str,
                   kind: FeatureKind,
                   coordinates: &[(f64, f64)],
                   height: f32,
                   tags: &[(&str, &str)]| {
        let mut properties: BTreeMap<String, String> = tags
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        properties.insert("synthetic".into(), "true".into());
        if kind == FeatureKind::Road {
            properties.insert("width".into(), height.to_string());
        }
        features.push(WorldFeature {
            id: format!("synthetic-v2:{}:{}:{}:{suffix}", key.level, key.x, key.y),
            kind,
            points: coordinates.iter().map(|&(u, v)| position(u, v)).collect(),
            height_m: height,
            properties,
        });
    };
    for (i, v) in [0.22, 0.56, 0.92].into_iter().enumerate() {
        add(
            &format!("east-west-{i}"),
            FeatureKind::Road,
            &[(0., v), (0.28, v), (0.58, v), (0.82, v), (1., v)],
            if i == 2 { 8.0 } else { 6.0 },
            &[
                ("highway", "residential"),
                ("lanes", "2"),
                ("surface", "asphalt"),
                (
                    "name",
                    if i == 2 {
                        "Authored Market Road"
                    } else {
                        "Authored Village Street"
                    },
                ),
            ],
        );
    }
    for (i, u) in [0.28, 0.58, 0.82].into_iter().enumerate() {
        add(
            &format!("north-south-{i}"),
            FeatureKind::Road,
            &[(u, 0.), (u, 0.22), (u, 0.56), (u, 0.92), (u, 1.)],
            6.0,
            &[
                ("highway", "residential"),
                ("lanes", "2"),
                ("surface", "asphalt"),
                ("name", "Authored Cross Lane"),
            ],
        );
    }
    let rectangle = |u: f64, v: f64, du: f64, dv: f64| {
        vec![
            (u - du, v - dv),
            (u + du, v - dv),
            (u + du, v + dv),
            (u - du, v + dv),
            (u - du, v - dv),
        ]
    };
    add(
        "grain-field",
        FeatureKind::Farm,
        &rectangle(0.14, 0.37, 0.10, 0.115),
        0.,
        &[
            ("landuse", "farmland"),
            ("crop", "wheat"),
            ("name", "Authored Grain Field"),
        ],
    );
    add(
        "orchard",
        FeatureKind::Farm,
        &rectangle(0.42, 0.36, 0.09, 0.10),
        0.,
        &[
            ("landuse", "orchard"),
            ("crop", "apple"),
            ("name", "Authored Orchard"),
        ],
    );
    add(
        "forest",
        FeatureKind::Forest,
        &[
            (0.63, 0.04),
            (0.79, 0.03),
            (0.78, 0.15),
            (0.74, 0.18),
            (0.63, 0.15),
            (0.63, 0.04),
        ],
        12.,
        &[("landuse", "forest"), ("leaf_type", "mixed")],
    );
    add(
        "forest-west",
        FeatureKind::Forest,
        &rectangle(0.14, 0.10, 0.10, 0.07),
        10.,
        &[("landuse", "forest"), ("leaf_type", "broadleaved")],
    );
    add(
        "meadow",
        FeatureKind::Grass,
        &rectangle(0.70, 0.72, 0.085, 0.12),
        0.,
        &[("landuse", "meadow")],
    );
    add(
        "pond",
        FeatureKind::Water,
        &[
            (0.85, 0.65),
            (0.95, 0.63),
            (0.98, 0.72),
            (0.93, 0.83),
            (0.87, 0.82),
            (0.84, 0.73),
            (0.85, 0.65),
        ],
        0.,
        &[
            ("natural", "water"),
            ("water", "pond"),
            ("name", "Authored Mill Pond"),
        ],
    );
    let mut index = 0usize;
    for v in [0.56, 0.92] {
        for u in [0.10, 0.18, 0.36, 0.44, 0.67, 0.75, 0.91] {
            for side in [-1., 1.] {
                let width = 9. + (index % 4) as f64 * 2.;
                let depth = 9. + (index % 3) as f64 * 2.;
                let vc = v + side * (14. + depth * 0.5) / span_z;
                let levels = 1 + index % 3;
                let shape = if index.is_multiple_of(6) {
                    "hipped"
                } else if index.is_multiple_of(5) {
                    "flat"
                } else {
                    "gabled"
                };
                let level_text = levels.to_string();
                add(
                    &format!("home-{index}"),
                    FeatureKind::Building,
                    &rectangle(u, vc, width * 0.5 / span_x, depth * 0.5 / span_z),
                    levels as f32 * 3. + 1.9,
                    &[
                        ("building", "house"),
                        ("building:levels", &level_text),
                        ("roof:shape", shape),
                        ("roof:material", "roof_tiles"),
                        (
                            "building:material",
                            if index.is_multiple_of(4) {
                                "brick"
                            } else {
                                "plaster"
                            },
                        ),
                    ],
                );
                index += 1;
            }
        }
    }
    add(
        "general-store",
        FeatureKind::Building,
        &rectangle(0.616, 0.870, 6. / span_x, 5. / span_z),
        5.0,
        &[
            ("building", "retail"),
            ("shop", "general"),
            ("name", "Authored Village Store"),
            ("building:levels", "1"),
            ("roof:shape", "gabled"),
            ("building:colour", "beige"),
        ],
    );
    add(
        "bakery",
        FeatureKind::Building,
        &rectangle(0.54, 0.868, 5.5 / span_x, 5. / span_z),
        7.0,
        &[
            ("building", "retail"),
            ("shop", "bakery"),
            ("name", "Authored Bakery"),
            ("building:levels", "2"),
            ("roof:shape", "gabled"),
            ("building:material", "brick"),
        ],
    );
    add(
        "sawmill",
        FeatureKind::Building,
        &rectangle(0.73, 0.16, 10. / span_x, 7. / span_z),
        7.5,
        &[
            ("building", "industrial"),
            ("industrial", "sawmill"),
            ("name", "Authored Sawmill"),
            ("building:levels", "1"),
            ("roof:shape", "gabled"),
            ("building:material", "wood"),
        ],
    );
    add(
        "market",
        FeatureKind::PointOfInterest,
        &[(0.606, 0.900)],
        0.,
        &[
            ("amenity", "marketplace"),
            ("shop", "general"),
            ("name", "Authored Market Stall"),
        ],
    );
    add(
        "water",
        FeatureKind::PointOfInterest,
        &[(0.565, 0.903)],
        0.,
        &[
            ("amenity", "drinking_water"),
            ("name", "Authored Village Well"),
        ],
    );
    for i in 0..16 {
        let u = 0.32 + (i % 8) as f64 * 0.031;
        let v = 0.69 + (i / 8) as f64 * 0.08;
        add(
            &format!("tree-{i}"),
            FeatureKind::Tree,
            &[(u, v)],
            5. + (i % 4) as f32,
            &[("natural", "tree"), ("leaf_type", "broadleaved")],
        );
    }
    WorldChunk {key,heights:HeightField {width:resolution,height:resolution,samples_m},features,
        sources:vec![SourceRecord {provider:"Terra procedural settlement v2".into(),license:"MIT OR Apache-2.0".into(),attribution:"Authored synthetic terrain, connected streets and settlement; not a reconstruction of this real GPS location".into(),url:"urn:terra:procedural-settlement:v2".into(),retrieved_unix_seconds:0,synthetic:true}],
        warnings:vec!["OFFLINE SYNTHETIC WORLD: geographic coordinates are real; relief, buildings, facilities and land use are authored examples.".into()]}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_tiles_share_height_samples() {
        let west = procedural_chunk(
            TileKey {
                level: 14,
                x: 8800,
                y: 3900,
            },
            17,
            42,
        );
        let east = procedural_chunk(
            TileKey {
                level: 14,
                x: 8801,
                y: 3900,
            },
            17,
            42,
        );
        for row in 0..17 {
            assert_eq!(
                west.heights.samples_m[row * 17 + 16],
                east.heights.samples_m[row * 17]
            );
        }
    }
    #[test]
    fn antimeridian_and_poles_are_continuous() {
        let height = |lat, lon| {
            procedural_height(
                GeoPosition {
                    latitude: lat,
                    longitude: lon,
                    altitude_m: 0.0,
                },
                42,
            )
        };
        assert!((height(42.0, 180.0) - height(42.0, -180.0)).abs() < 0.001);
        assert_eq!(height(90.0, -177.0), height(90.0, 123.0));
    }
}
