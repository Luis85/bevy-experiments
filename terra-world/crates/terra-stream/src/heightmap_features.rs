//! Deliberately bounded GeoJSON subset for local elevation bundles. Unsupported
//! geometry fails at import; it is never silently filled, flattened or dropped.
use crate::StreamError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{FeatureKind, GeoBounds, GeoPosition, TileKey, WorldFeature};

pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FEATURES: usize = 2048;
const MAX_PARTS: usize = 8192;
const MAX_VERTICES: usize = 131_072;
const MAX_POINTS: usize = 1024;
const MAX_PROPERTIES: usize = 65_536;
const MAX_PROPERTY_BYTES: usize = 8 * 1024 * 1024;

/// Multipart normalization and tile fragmentation must not multiply a small
/// source document's attribute maps into unbounded resident allocations.
#[derive(Default)]
struct PropertyBudget {
    properties: usize,
    bytes: usize,
}
impl PropertyBudget {
    fn reserve(
        &mut self,
        properties: &BTreeMap<String, String>,
        copies: usize,
    ) -> Result<(), StreamError> {
        let bytes = properties
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>();
        self.properties = self
            .properties
            .saturating_add(properties.len().saturating_mul(copies));
        self.bytes = self.bytes.saturating_add(bytes.saturating_mul(copies));
        if self.properties > MAX_PROPERTIES || self.bytes > MAX_PROPERTY_BYTES {
            return Err(error(
                "normalized/clipped property budget exceeded (65536 properties / 8 MiB text)",
            ));
        }
        Ok(())
    }
}

fn error(message: impl Into<String>) -> StreamError {
    StreamError::Payload(format!("heightmap GeoJSON: {}", message.into()))
}

pub(crate) struct FeatureIndex {
    entries: Vec<(GeoBounds, WorldFeature)>,
}
impl FeatureIndex {
    pub(crate) fn resident_property_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|(_, f)| {
                f.properties
                    .iter()
                    .map(|(k, v)| k.len() + v.len())
                    .sum::<usize>()
            })
            .sum()
    }
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    pub fn parse(bytes: &[u8], coverage: GeoBounds) -> Result<Self, StreamError> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error("source byte budget exceeded (8 MiB)"));
        }
        let document: Value = serde_json::from_slice(bytes).map_err(|e| error(e.to_string()))?;
        if document.get("type").and_then(Value::as_str) != Some("FeatureCollection")
            || document.get("crs").is_some()
        {
            return Err(error(
                "requires a WGS84 FeatureCollection without a legacy crs member",
            ));
        }
        let features = document
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| error("features must be an array"))?;
        if features.len() > MAX_FEATURES {
            return Err(error("feature budget exceeded (2048)"));
        }
        let mut entries = Vec::new();
        let mut ids = BTreeSet::new();
        let mut vertices = 0usize;
        let mut property_budget = PropertyBudget::default();
        for feature in features {
            if feature.get("type").and_then(Value::as_str) != Some("Feature") {
                return Err(error("collection entries must be Features"));
            }
            let id = match feature.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(Value::Number(id)) => id.to_string(),
                _ => return Err(error("every feature needs a stable string or numeric id")),
            };
            if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
                return Err(error("feature id must contain 1..128 printable bytes"));
            }
            if !ids.insert(id.clone()) {
                return Err(error(format!("duplicate id {id}")));
            }
            let object = feature
                .get("properties")
                .and_then(Value::as_object)
                .ok_or_else(|| error(format!("{id}: properties must be an object")))?;
            if object.len() > 64 {
                return Err(error(format!("{id}: property budget exceeded (64)")));
            }
            let mut properties = BTreeMap::new();
            for (key, value) in object {
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    Value::Null => continue,
                    _ => return Err(error(format!("{id}: nested properties are unsupported"))),
                };
                if key.is_empty()
                    || key.len() > 128
                    || value.len() > 2048
                    || key.chars().any(char::is_control)
                    || value.chars().any(char::is_control)
                {
                    return Err(error(format!("{id}: invalid or oversized property")));
                }
                properties.insert(key.clone(), value);
            }
            let tag = |key: &str| properties.get(key).map(String::as_str).unwrap_or("");
            let kind = if !tag("terra:kind").is_empty() {
                serde_json::from_value::<FeatureKind>(Value::String(tag("terra:kind").into()))
                    .map_err(|_| error(format!("{id}: unknown terra:kind")))?
            } else if !tag("building").is_empty() && tag("building") != "no" {
                FeatureKind::Building
            } else if !tag("highway").is_empty() {
                FeatureKind::Road
            } else if tag("natural") == "water" || tag("waterway") == "riverbank" {
                FeatureKind::Water
            } else if !tag("waterway").is_empty() {
                FeatureKind::Waterway
            } else if tag("natural") == "tree" {
                FeatureKind::Tree
            } else if tag("natural") == "wood" || tag("landuse") == "forest" {
                FeatureKind::Forest
            } else if matches!(
                tag("landuse"),
                "farmland" | "farmyard" | "orchard" | "vineyard"
            ) {
                FeatureKind::Farm
            } else if matches!(tag("landuse"), "grass" | "meadow") || tag("natural") == "grassland"
            {
                FeatureKind::Grass
            } else if ["shop", "amenity", "craft", "tourism"]
                .iter()
                .any(|key| !tag(key).is_empty())
            {
                FeatureKind::PointOfInterest
            } else {
                return Err(error(format!("{id}: add terra:kind or supported map tags")));
            };
            let geometry = feature
                .get("geometry")
                .ok_or_else(|| error(format!("{id}: missing geometry")))?;
            let geometry_type = geometry.get("type").and_then(Value::as_str).unwrap_or("");
            let coordinates = geometry
                .get("coordinates")
                .ok_or_else(|| error(format!("{id}: missing coordinates")))?;
            let parts = match geometry_type {
                "Point" | "LineString" | "Polygon" => vec![coordinates],
                "MultiPoint" | "MultiLineString" | "MultiPolygon" => coordinates
                    .as_array()
                    .ok_or_else(|| error(format!("{id}: invalid multi geometry")))?
                    .iter()
                    .collect(),
                _ => return Err(error(format!("{id}: unsupported geometry {geometry_type}"))),
            };
            if parts.is_empty() || entries.len().saturating_add(parts.len()) > MAX_PARTS {
                return Err(error(
                    "normalized feature budget exceeded or empty geometry",
                ));
            }
            let primitive = geometry_type.strip_prefix("Multi").unwrap_or(geometry_type);
            let expected = match kind {
                FeatureKind::Tree | FeatureKind::PointOfInterest => "Point",
                FeatureKind::Road | FeatureKind::Waterway => "LineString",
                _ => "Polygon",
            };
            if primitive != expected {
                return Err(error(format!("{id}: {kind:?} requires {expected}")));
            }
            let size_key = if matches!(kind, FeatureKind::Road | FeatureKind::Waterway) {
                "width"
            } else {
                "height"
            };
            let explicit = if tag(size_key).is_empty() {
                None
            } else {
                Some(
                    crate::providers::parse_osm_length(tag(size_key))
                        .filter(|value| {
                            value.is_finite()
                                && *value > 0.
                                && *value <= if size_key == "width" { 100. } else { 2000. }
                        })
                        .ok_or_else(|| error(format!("{id}: invalid {size_key}")))?,
                )
            };
            let default_height = match kind {
                FeatureKind::Building => {
                    tag("building:levels")
                        .parse::<f32>()
                        .ok()
                        .filter(|n| n.is_finite() && *n > 0. && *n <= 400.)
                        .unwrap_or(2.)
                        * 3.
                }
                FeatureKind::Forest | FeatureKind::Tree => 8.,
                FeatureKind::Road => match tag("highway") {
                    "footway" | "path" | "cycleway" => 2.,
                    "primary" | "secondary" => 7.,
                    _ => 6.,
                },
                FeatureKind::Waterway => 3.,
                _ => 0.,
            };
            let height_m = explicit.unwrap_or(default_height);
            properties.insert(
                "geometry".into(),
                match primitive {
                    "LineString" => "line",
                    "Polygon" => "polygon",
                    _ => "point",
                }
                .into(),
            );
            properties.insert(
                "elevation_source".into(),
                "bundle heightmap; coordinate altitude is not used".into(),
            );
            if explicit.is_none() && default_height > 0. {
                properties.insert(
                    format!("{size_key}_source"),
                    "inferred/default; not measured".into(),
                );
            }
            properties.insert("source_id".into(), id.clone());
            property_budget.reserve(&properties, parts.len())?;
            for (part, coordinates) in parts.into_iter().enumerate() {
                let points = points(coordinates, primitive, coverage)?;
                vertices = vertices.saturating_add(points.len());
                if vertices > MAX_VERTICES {
                    return Err(error("vertex budget exceeded (131072)"));
                }
                let feature = WorldFeature {
                    id: normalized_id(&id, part),
                    kind,
                    points,
                    height_m,
                    properties: properties.clone(),
                };
                feature.validate().map_err(|e| error(e.to_string()))?;
                let bounds = feature.points.iter().fold(
                    GeoBounds {
                        south: 90.,
                        north: -90.,
                        west: 180.,
                        east: -180.,
                    },
                    |mut b, p| {
                        b.south = b.south.min(p.latitude);
                        b.north = b.north.max(p.latitude);
                        b.west = b.west.min(p.longitude);
                        b.east = b.east.max(p.longitude);
                        b
                    },
                );
                entries.push((bounds, feature));
            }
        }
        Ok(Self { entries })
    }
    pub fn clipped(&self, key: TileKey) -> Result<Vec<WorldFeature>, StreamError> {
        let b = key.bounds();
        let mut result = Vec::new();
        let mut property_budget = PropertyBudget::default();
        let mut vertices = 0usize;
        for (_, feature) in self.entries.iter().filter(|(a, _)| {
            a.west <= b.east && a.east >= b.west && a.south <= b.north && a.north >= b.south
        }) {
            // Clip small geometry-only records first. A 1024-vertex line can
            // cross the tile hundreds of times; never clone its full property
            // map for every fragment before admitting the aggregate budget.
            let geometry = WorldFeature {
                id: feature.id.clone(),
                kind: feature.kind,
                points: feature.points.clone(),
                height_m: feature.height_m,
                properties: BTreeMap::new(),
            };
            let mut fragments = crate::clip::to_tile(vec![geometry], key);
            if matches!(
                feature.kind,
                FeatureKind::Building
                    | FeatureKind::Water
                    | FeatureKind::Forest
                    | FeatureKind::Farm
                    | FeatureKind::Grass
            ) {
                // A boundary-only intersection has no polygon area. A concave
                // exterior can also split into separate islands when clipped;
                // the normalized single-ring representation cannot express
                // those safely, so reject it instead of filling a false bridge.
                fragments.retain(|part| polygon_area_twice(&part.points).abs() >= 1e-14);
                for part in &fragments {
                    validate_polygon_exterior(&part.points).map_err(|_| error(format!("{}: clipped exterior has disconnected or touching parts; export explicit simple parts", feature.id)))?;
                }
            }
            if result.len().saturating_add(fragments.len()) > MAX_PARTS {
                return Err(error("clipped feature budget exceeded (8192)"));
            }
            vertices = vertices.saturating_add(
                fragments
                    .iter()
                    .map(|part| part.points.len())
                    .sum::<usize>(),
            );
            if vertices > MAX_VERTICES {
                return Err(error("clipped vertex budget exceeded (131072)"));
            }
            property_budget.reserve(&feature.properties, fragments.len())?;
            for part in &mut fragments {
                // Keep the external source id stable across tile fragments.
                part.properties = feature.properties.clone();
            }
            result.extend(fragments);
        }
        Ok(result)
    }
}
/// Reserve the clipping suffix while honoring the gameplay/persistence ID
/// contract (128 ASCII identifier bytes). Ordinary old IDs stay unchanged;
/// long IDs and printable external identifiers retain their exact provenance
/// in `source_id` and receive a deterministic collision-resistant identity.
fn normalized_id(source: &str, part: usize) -> String {
    let legacy = format!("geojson:{}:{source}:part:{part}", source.len());
    if legacy.len() <= 88
        && legacy
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:-./".contains(&b))
    {
        legacy
    } else {
        format!(
            "geojson-hash:{}:part:{part}",
            blake3::hash(source.as_bytes()).to_hex()
        )
    }
}

fn points(
    value: &Value,
    primitive: &str,
    coverage: GeoBounds,
) -> Result<Vec<GeoPosition>, StreamError> {
    let coordinate = |value: &Value| -> Result<GeoPosition, StreamError> {
        let values = value
            .as_array()
            .filter(|v| (2..=3).contains(&v.len()))
            .ok_or_else(|| {
                error("positions require [longitude, latitude] with optional altitude")
            })?;
        if values
            .iter()
            .any(|n| n.as_f64().is_none_or(|v| !v.is_finite()))
        {
            return Err(error("coordinate is not finite"));
        }
        let point = GeoPosition::new(values[1].as_f64().unwrap(), values[0].as_f64().unwrap(), 0.)
            .map_err(|e| error(e.to_string()))?;
        if !coverage.contains(point) {
            return Err(error(
                "feature coordinates exceed the heightmap coverage; clip the source export first",
            ));
        }
        Ok(point)
    };
    if primitive == "Point" {
        return Ok(vec![coordinate(value)?]);
    }
    let value = if primitive == "Polygon" {
        let rings=value.as_array().filter(|v|v.len()==1)
            .ok_or_else(|| error("polygon holes are unsupported; supply explicit exterior-only parts, never filled holes"))?;
        &rings[0]
    } else {
        value
    };
    let values = value
        .as_array()
        .filter(|v| (2..=MAX_POINTS).contains(&v.len()))
        .ok_or_else(|| error("each line/ring needs 2..1024 positions"))?;
    let points = values
        .iter()
        .map(coordinate)
        .collect::<Result<Vec<_>, _>>()?;
    if points.windows(2).any(|p| p[0] == p[1]) {
        return Err(error("consecutive duplicate positions"));
    }
    if primitive == "Polygon" {
        validate_polygon_exterior(&points)?;
    }
    Ok(points)
}
fn polygon_area_twice(points: &[GeoPosition]) -> f64 {
    let Some(origin) = points.first() else {
        return 0.;
    };
    points
        .windows(2)
        .map(|p| {
            (p[0].longitude - origin.longitude) * (p[1].latitude - origin.latitude)
                - (p[1].longitude - origin.longitude) * (p[0].latitude - origin.latitude)
        })
        .sum()
}
fn validate_polygon_exterior(points: &[GeoPosition]) -> Result<(), StreamError> {
    if points.len() < 4 || points.first() != points.last() {
        return Err(error(
            "polygon exterior must be closed with at least four positions",
        ));
    }
    if polygon_area_twice(points).abs() < 1e-14 {
        return Err(error("zero-area polygon exterior"));
    }
    // A bounded import/clip check protects the renderer from self-crossing or
    // touching rings. Adjacent edges share their one expected endpoint.
    for i in 0..points.len() - 1 {
        for j in i + 2..points.len() - 1 {
            if i == 0 && j == points.len() - 2 {
                continue;
            }
            if segments_touch(points[i], points[i + 1], points[j], points[j + 1]) {
                return Err(error("self-intersecting or touching polygon exterior"));
            }
        }
    }
    Ok(())
}
fn segments_touch(a: GeoPosition, b: GeoPosition, c: GeoPosition, d: GeoPosition) -> bool {
    let orient = |a: GeoPosition, b: GeoPosition, c: GeoPosition| {
        (b.longitude - a.longitude) * (c.latitude - a.latitude)
            - (b.latitude - a.latitude) * (c.longitude - a.longitude)
    };
    let within = |p: GeoPosition, a: GeoPosition, b: GeoPosition| {
        p.longitude >= a.longitude.min(b.longitude)
            && p.longitude <= a.longitude.max(b.longitude)
            && p.latitude >= a.latitude.min(b.latitude)
            && p.latitude <= a.latitude.max(b.latitude)
    };
    let (x, y, z, w) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    (x * y < 0. && z * w < 0.)
        || (x == 0. && within(c, a, b))
        || (y == 0. && within(d, a, b))
        || (z == 0. && within(a, c, d))
        || (w == 0. && within(b, c, d))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_ids_remain_provenance_and_fit_persistent_gameplay_after_clipping() {
        let (key, bounds) = extent();
        for source_id in [
            "x".repeat(128),
            "road with spaces/ä".to_owned(),
            "ordinary-road".to_owned(),
        ] {
            let data = serde_json::json!({"type":"FeatureCollection","features":[{
                "type":"Feature", "id":source_id, "properties":{"highway":"residential"},
                "geometry":{"type":"LineString","coordinates":[coordinate(bounds,0.1,0.5),coordinate(bounds,0.9,0.5)]}
            }]});
            let index = FeatureIndex::parse(&serde_json::to_vec(&data).unwrap(), bounds).unwrap();
            let parts = index.clipped(key).unwrap();
            assert!(!parts.is_empty());
            for part in parts {
                assert!(part.id.len() <= 128, "{}", part.id);
                assert!(
                    part.id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_:-./".contains(&b))
                );
                assert_eq!(part.properties["source_id"], source_id);
            }
        }
        assert_ne!(
            normalized_id(&"x".repeat(128), 0),
            normalized_id(&"y".repeat(128), 0)
        );
        assert_eq!(normalized_id("road", 0), "geojson:4:road:part:0");
    }
    fn extent() -> (TileKey, GeoBounds) {
        let west = TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let b = west.bounds();
        (
            west,
            GeoBounds {
                east: b.east + b.east - b.west,
                ..b
            },
        )
    }
    fn coordinate(bounds: GeoBounds, u: f64, v: f64) -> Value {
        let point = bounds.point(u, v, 0.).unwrap();
        serde_json::json!([point.longitude, point.latitude])
    }
    fn bytes(feature: Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"type":"FeatureCollection", "features":[feature]}))
            .unwrap()
    }
    #[test]
    fn multipart_properties_cannot_expand_small_sources_past_memory_budgets() {
        let (_, bounds) = extent();
        for large_values in [true, false] {
            let mut properties = serde_json::Map::new();
            properties.insert("natural".into(), Value::String("tree".into()));
            let (copies, count, size) = if large_values {
                (4096, 1, 2048)
            } else {
                (1100, 60, 1)
            };
            for n in 0..count {
                properties.insert(format!("note{n}"), Value::String("x".repeat(size)));
            }
            let feature = serde_json::json!({
                "type":"Feature", "id":"copies", "properties":properties,
                "geometry":{"type":"MultiPoint", "coordinates":vec![coordinate(bounds,0.25,0.5);copies]}
            });
            let input = bytes(feature);
            assert!(input.len() < MAX_BYTES as usize);
            let error = FeatureIndex::parse(&input, bounds)
                .err()
                .expect("normalized maps must be budgeted before cloning");
            assert!(error.to_string().contains("property budget exceeded"));
        }
    }
    #[test]
    fn line_fragmentation_budgets_attributes_before_duplicating_them() {
        let (west, bounds) = extent();
        let mut properties = serde_json::Map::new();
        properties.insert("highway".into(), Value::String("residential".into()));
        properties.insert("surface".into(), Value::String("gravel".into()));
        for n in 0..32 {
            properties.insert(format!("note{n}"), Value::String("x".repeat(2048)));
        }
        let coordinates: Vec<_> = (0..1000)
            .map(|n| {
                coordinate(
                    bounds,
                    if n % 2 == 0 { 0.1 } else { 0.9 },
                    0.1 + 0.8 * f64::from(n) / 999.,
                )
            })
            .collect();
        let feature = serde_json::json!({"type":"Feature","id":"zigzag","properties":properties,"geometry":{"type":"LineString","coordinates":coordinates}});
        let index = FeatureIndex::parse(&bytes(feature), bounds).unwrap();
        let error = index
            .clipped(west)
            .err()
            .expect("clipped map copies must be bounded");
        assert!(error.to_string().contains("property budget exceeded"));
    }
    #[test]
    fn disconnected_concave_clip_is_rejected_instead_of_filling_a_false_polygon() {
        let (west, bounds) = extent();
        let points = [
            [0.1, 0.2],
            [0.9, 0.2],
            [0.9, 0.8],
            [0.1, 0.8],
            [0.1, 0.65],
            [0.7, 0.65],
            [0.7, 0.35],
            [0.1, 0.35],
            [0.1, 0.2],
        ];
        let feature = serde_json::json!({"type":"Feature","id":"two-lobes","properties":{"landuse":"forest"},"geometry":{"type":"Polygon","coordinates":[points.map(|p|coordinate(bounds,p[0],p[1]))]}});
        let index = FeatureIndex::parse(&bytes(feature), bounds).unwrap();
        assert!(
            index
                .clipped(west)
                .err()
                .unwrap()
                .to_string()
                .contains("disconnected or touching")
        );
        let east = TileKey::new(west.level, west.x + 1, west.y).unwrap();
        let supported = index.clipped(east).unwrap();
        assert_eq!(supported.len(), 1);
        assert_eq!(supported[0].properties["source_id"], "two-lobes");
        assert_eq!(supported[0].properties["geometry"], "polygon");
    }
}
