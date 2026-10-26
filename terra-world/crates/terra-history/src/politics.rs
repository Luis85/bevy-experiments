use crate::{
    HistoryError,
    pack::{MAX_BYTES, bounded_read},
    time::{json_date, valid_at, valid_range},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use terra_core::{GeoBounds, GeoPosition, normalize_longitude};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoliticalKind {
    Country,
    Region,
    DisputedArea,
    ScenarioTerritory,
}

/// GeoJSON polygon: ring zero is the exterior, subsequent rings are holes.
/// Coordinates use geodetic longitude/latitude; queries unwrap antimeridian edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoliticalPolygon {
    pub rings: Vec<Vec<GeoPosition>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoliticalFeature {
    pub id: String,
    pub name: String,
    pub kind: PoliticalKind,
    pub country: String,
    pub claimants: Vec<String>,
    pub status: String,
    pub valid_from_unix: i64,
    pub valid_until_unix: Option<i64>,
    /// Optional author-defined gameplay faction link. It never rewrites the
    /// source country's real sovereignty, control, or claimant properties.
    pub faction_id: Option<u64>,
    pub polygons: Vec<PoliticalPolygon>,
    /// All original properties survive import/export. Canonical public fields
    /// overwrite their corresponding properties when exported after an edit.
    pub properties: BTreeMap<String, Value>,
}

impl PoliticalFeature {
    pub fn contains_time(&self, epoch: i64) -> bool {
        valid_at(epoch, self.valid_from_unix, self.valid_until_unix)
    }
    pub fn contains_position(&self, position: GeoPosition) -> bool {
        self.polygons
            .iter()
            .any(|polygon| polygon.contains(position))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoliticsDataset {
    pub id: String,
    pub title: String,
    pub valid_from_unix: i64,
    pub valid_until_unix: Option<i64>,
    pub source_url: String,
    pub license: String,
    pub attribution: String,
    pub synthetic: bool,
    pub features: Vec<PoliticalFeature>,
}

fn text(value: &Value, key: &str) -> Result<String, HistoryError> {
    let string = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| HistoryError::Invalid(format!("missing string field {key}")))?;
    if string.trim().is_empty() || string.len() > 4096 {
        return Err(HistoryError::Invalid(format!("invalid or oversized {key}")));
    }
    Ok(string.to_owned())
}

fn coordinate(value: &Value) -> Result<GeoPosition, HistoryError> {
    let point = value
        .as_array()
        .filter(|p| (2..=3).contains(&p.len()))
        .ok_or_else(|| {
            HistoryError::Invalid(
                "GeoJSON position must have longitude, latitude and optional altitude".into(),
            )
        })?;
    let lon = point[0]
        .as_f64()
        .ok_or_else(|| HistoryError::Invalid("invalid longitude".into()))?;
    let lat = point[1]
        .as_f64()
        .ok_or_else(|| HistoryError::Invalid("invalid latitude".into()))?;
    let altitude = if point.len() == 3 {
        point[2]
            .as_f64()
            .ok_or_else(|| HistoryError::Invalid("invalid altitude".into()))?
    } else {
        0.0
    };
    if !(-180.0..=180.0).contains(&lon) {
        return Err(HistoryError::Invalid(
            "GeoJSON longitude must be within -180..=180".into(),
        ));
    }
    // Preserve +180 versus -180 at the dateline; both are valid GeoJSON values.
    let p = GeoPosition {
        latitude: lat,
        longitude: lon,
        altitude_m: altitude,
    };
    p.validate()
        .map_err(|e| HistoryError::Invalid(e.to_string()))?;
    Ok(p)
}

fn parse_polygon(value: &Value, total: &mut usize) -> Result<PoliticalPolygon, HistoryError> {
    let raw_rings = value
        .as_array()
        .ok_or_else(|| HistoryError::Invalid("polygon rings array missing".into()))?;
    if raw_rings.is_empty() || raw_rings.len() > 4096 {
        return Err(HistoryError::Invalid("polygon needs 1..=4096 rings".into()));
    }
    let mut rings = Vec::new();
    for raw in raw_rings {
        let vertices = raw
            .as_array()
            .ok_or_else(|| HistoryError::Invalid("ring coordinates missing".into()))?;
        *total = total.saturating_add(vertices.len());
        if *total > 500_000 {
            return Err(HistoryError::Invalid(
                "political geometry vertex budget exceeded".into(),
            ));
        }
        let ring = vertices
            .iter()
            .map(coordinate)
            .collect::<Result<Vec<_>, _>>()?;
        validate_ring(&ring)?;
        rings.push(ring);
    }
    Ok(PoliticalPolygon { rings })
}

fn validate_ring(ring: &[GeoPosition]) -> Result<(), HistoryError> {
    if ring.len() < 4 || ring.first() != ring.last() {
        return Err(HistoryError::Invalid(
            "polygon rings must have ≥4 positions and be explicitly closed".into(),
        ));
    }
    for position in ring {
        position
            .validate()
            .map_err(|e| HistoryError::Invalid(e.to_string()))?;
        if !(-180.0..=180.0).contains(&position.longitude) {
            return Err(HistoryError::Invalid(
                "GeoJSON longitude must be within -180..=180".into(),
            ));
        }
    }
    let points = unwrapped(ring);
    let area: f64 = points
        .windows(2)
        .map(|pair| pair[0].0 * pair[1].1 - pair[1].0 * pair[0].1)
        .sum();
    if !area.is_finite() || area.abs() < 1e-12 {
        return Err(HistoryError::Invalid(
            "polygon ring has zero/invalid area".into(),
        ));
    }
    Ok(())
}

impl PoliticsDataset {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HistoryError> {
        let bytes = bounded_read(path.as_ref(), MAX_BYTES)?;
        let source =
            std::str::from_utf8(&bytes).map_err(|e| HistoryError::Invalid(e.to_string()))?;
        Self::from_geojson(source)
    }

    /// Requires an explicit `terra` metadata object. Undated current geometry
    /// cannot silently become a boundary dataset for an arbitrary historical era.
    pub fn from_geojson(source: &str) -> Result<Self, HistoryError> {
        if source.len() as u64 > MAX_BYTES {
            return Err(HistoryError::Invalid("GeoJSON exceeds 32 MiB".into()));
        }
        let root: Value =
            serde_json::from_str(source).map_err(|e| HistoryError::Invalid(e.to_string()))?;
        if root.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
            return Err(HistoryError::Invalid(
                "expected GeoJSON FeatureCollection".into(),
            ));
        }
        let metadata = root.get("terra").ok_or_else(|| {
            HistoryError::Invalid("explicit terra provenance and date metadata required".into())
        })?;
        if metadata
            .get("schema_version")
            .is_some_and(|value| value.as_u64() != Some(1))
        {
            return Err(HistoryError::Invalid(
                "unsupported politics schema version".into(),
            ));
        }
        let from = json_date(
            metadata
                .get("valid_from_unix")
                .or_else(|| metadata.get("valid_from")),
            None,
        )?
        .ok_or_else(|| HistoryError::Invalid("dataset valid_from is required".into()))?;
        let until = json_date(
            metadata
                .get("valid_until_unix")
                .or_else(|| metadata.get("valid_until")),
            None,
        )?;
        valid_range(from, until)?;
        let raw_features = root
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| HistoryError::Invalid("features array missing".into()))?;
        if raw_features.len() > 8192 {
            return Err(HistoryError::Invalid(
                "political feature count exceeds 8192".into(),
            ));
        }
        let mut features = Vec::new();
        let mut total_vertices = 0usize;
        for raw in raw_features {
            if raw.get("type").and_then(Value::as_str) != Some("Feature") {
                return Err(HistoryError::Invalid("invalid GeoJSON feature".into()));
            }
            let props = raw
                .get("properties")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    HistoryError::Invalid("feature properties object required".into())
                })?;
            let id = props
                .get("id")
                .or_else(|| raw.get("id"))
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
                .ok_or_else(|| {
                    HistoryError::Invalid("stable political feature id required".into())
                })?;
            let p = Value::Object(props.clone());
            let name = text(&p, "name")?;
            let kind = match props
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("country")
            {
                "country" => PoliticalKind::Country,
                "region" => PoliticalKind::Region,
                "disputed_area" => PoliticalKind::DisputedArea,
                "scenario_territory" => PoliticalKind::ScenarioTerritory,
                _ => {
                    return Err(HistoryError::Invalid(
                        "unknown political feature kind".into(),
                    ));
                }
            };
            let country = props
                .get("country")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let status = props
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("source-defined")
                .to_owned();
            let claimants = match props.get("claimants") {
                None | Some(Value::Null) => vec![],
                Some(Value::Array(values)) => values
                    .iter()
                    .map(|v| {
                        v.as_str().map(str::to_owned).ok_or_else(|| {
                            HistoryError::Invalid("claimants must contain strings".into())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(HistoryError::Invalid("claimants must be an array".into())),
            };
            let feature_from = json_date(
                props
                    .get("valid_from_unix")
                    .or_else(|| props.get("valid_from")),
                Some(from),
            )?
            .unwrap_or(from);
            let feature_until = json_date(
                props
                    .get("valid_until_unix")
                    .or_else(|| props.get("valid_until")),
                until,
            )?;
            let faction_id = match props.get("faction_id") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_u64().ok_or_else(|| {
                    HistoryError::Invalid("faction_id must be a nonnegative integer".into())
                })?),
            };
            let geometry = raw
                .get("geometry")
                .ok_or_else(|| HistoryError::Invalid("feature geometry required".into()))?;
            let coordinates = geometry
                .get("coordinates")
                .ok_or_else(|| HistoryError::Invalid("geometry coordinates required".into()))?;
            let polygons = match geometry.get("type").and_then(Value::as_str) {
                Some("Polygon") => vec![parse_polygon(coordinates, &mut total_vertices)?],
                Some("MultiPolygon") => coordinates
                    .as_array()
                    .ok_or_else(|| {
                        HistoryError::Invalid("multipolygon coordinates must be an array".into())
                    })?
                    .iter()
                    .map(|v| parse_polygon(v, &mut total_vertices))
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(HistoryError::Invalid(
                        "only Polygon and MultiPolygon political geometry is supported".into(),
                    ));
                }
            };
            features.push(PoliticalFeature {
                id,
                name,
                kind,
                country,
                claimants,
                status,
                valid_from_unix: feature_from,
                valid_until_unix: feature_until,
                faction_id,
                polygons,
                properties: props.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            });
        }
        let dataset = Self {
            id: text(metadata, "id")?,
            title: text(metadata, "title")?,
            valid_from_unix: from,
            valid_until_unix: until,
            source_url: text(metadata, "source_url")?,
            license: text(metadata, "license")?,
            attribution: text(metadata, "attribution")?,
            synthetic: metadata
                .get("synthetic")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    HistoryError::Invalid("dataset synthetic flag is required".into())
                })?,
            features,
        };
        dataset.validate()?;
        Ok(dataset)
    }

    pub fn validate(&self) -> Result<(), HistoryError> {
        valid_range(self.valid_from_unix, self.valid_until_unix)?;
        for text in [
            &self.id,
            &self.title,
            &self.source_url,
            &self.license,
            &self.attribution,
        ] {
            if text.trim().is_empty() || text.len() > 4096 {
                return Err(HistoryError::Invalid(
                    "dataset metadata is empty or oversized".into(),
                ));
            }
        }
        if self.features.len() > 8192 {
            return Err(HistoryError::Invalid(
                "feature count budget exceeded".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut vertices = 0usize;
        for feature in &self.features {
            if feature.id.trim().is_empty() || feature.id.len() > 4096 || !ids.insert(&feature.id) {
                return Err(HistoryError::Invalid(
                    "political feature IDs must be nonempty and unique".into(),
                ));
            }
            if feature.name.trim().is_empty()
                || feature.name.len() > 4096
                || feature.country.len() > 4096
                || feature.status.len() > 4096
                || feature.claimants.len() > 256
                || feature
                    .claimants
                    .iter()
                    .any(|c| c.is_empty() || c.len() > 4096)
            {
                return Err(HistoryError::Invalid(
                    "invalid or oversized political feature metadata".into(),
                ));
            }
            valid_range(feature.valid_from_unix, feature.valid_until_unix)?;
            if feature.valid_from_unix < self.valid_from_unix
                || self
                    .valid_until_unix
                    .is_some_and(|end| feature.valid_until_unix.is_none_or(|own| own > end))
            {
                return Err(HistoryError::Invalid(
                    "feature dates exceed dataset coverage".into(),
                ));
            }
            if feature.polygons.is_empty() || feature.polygons.len() > 4096 {
                return Err(HistoryError::Invalid(
                    "feature requires 1..=4096 polygons".into(),
                ));
            }
            for polygon in &feature.polygons {
                if polygon.rings.is_empty() || polygon.rings.len() > 4096 {
                    return Err(HistoryError::Invalid(
                        "polygon requires an exterior ring".into(),
                    ));
                }
                for ring in &polygon.rings {
                    vertices = vertices.saturating_add(ring.len());
                    if vertices > 500_000 {
                        return Err(HistoryError::Invalid(
                            "political geometry vertex budget exceeded".into(),
                        ));
                    }
                    validate_ring(ring)?;
                }
            }
        }
        Ok(())
    }

    pub fn contains_time(&self, epoch: i64) -> bool {
        valid_at(epoch, self.valid_from_unix, self.valid_until_unix)
    }
    fn check_time(&self, epoch: i64) -> Result<(), HistoryError> {
        if self.contains_time(epoch) {
            Ok(())
        } else {
            Err(HistoryError::OutsideTime {
                requested: epoch,
                from: self.valid_from_unix,
                until: self.valid_until_unix,
            })
        }
    }
    /// Return every overlapping claim/region. Source ordering never grants a
    /// country or faction authority over another feature.
    pub fn at(
        &self,
        position: GeoPosition,
        epoch: i64,
    ) -> Result<Vec<&PoliticalFeature>, HistoryError> {
        self.check_time(epoch)?;
        position
            .validate()
            .map_err(|e| HistoryError::Invalid(e.to_string()))?;
        Ok(self
            .features
            .iter()
            .filter(|f| f.contains_time(epoch) && f.contains_position(position))
            .collect())
    }
    /// Conservative geographic bounding-box culling. Holes are preserved for the
    /// caller's drawing/containment test, not used to suppress coarse candidates.
    pub fn visible(
        &self,
        bounds: GeoBounds,
        epoch: i64,
    ) -> Result<Vec<&PoliticalFeature>, HistoryError> {
        self.check_time(epoch)?;
        if [bounds.south, bounds.west, bounds.north, bounds.east]
            .iter()
            .any(|n| !n.is_finite())
            || bounds.south > bounds.north
            || bounds.south < -90.0
            || bounds.north > 90.0
            || !(-180.0..=180.0).contains(&bounds.west)
            || !(-180.0..=180.0).contains(&bounds.east)
        {
            return Err(HistoryError::Invalid("invalid view bounds".into()));
        }
        Ok(self
            .features
            .iter()
            .filter(|f| {
                f.contains_time(epoch) && f.polygons.iter().any(|p| p.intersects_bounds(bounds))
            })
            .collect())
    }

    pub fn to_geojson(&self) -> Result<String, HistoryError> {
        self.validate()?;
        let metadata = json!({"schema_version":1,"id":self.id,"title":self.title,"valid_from_unix":self.valid_from_unix,"valid_until_unix":self.valid_until_unix,"source_url":self.source_url,"license":self.license,"attribution":self.attribution,"synthetic":self.synthetic});
        let features = self
            .features
            .iter()
            .map(|feature| {
                let mut props: serde_json::Map<String, Value> =
                    feature.properties.clone().into_iter().collect();
                for key in ["valid_from", "valid_until"] {
                    props.remove(key);
                }
                let kind = match feature.kind {
                    PoliticalKind::Country => "country",
                    PoliticalKind::Region => "region",
                    PoliticalKind::DisputedArea => "disputed_area",
                    PoliticalKind::ScenarioTerritory => "scenario_territory",
                };
                for (key, value) in [
                    ("id", json!(feature.id)),
                    ("name", json!(feature.name)),
                    ("kind", json!(kind)),
                    ("country", json!(feature.country)),
                    ("status", json!(feature.status)),
                    ("claimants", json!(feature.claimants)),
                    ("faction_id", json!(feature.faction_id)),
                    ("valid_from_unix", json!(feature.valid_from_unix)),
                    ("valid_until_unix", json!(feature.valid_until_unix)),
                ] {
                    props.insert(key.into(), value);
                }
                let coordinates = feature
                    .polygons
                    .iter()
                    .map(|polygon| {
                        polygon
                            .rings
                            .iter()
                            .map(|ring| {
                                ring.iter()
                                    .map(|p| vec![p.longitude, p.latitude, p.altitude_m])
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let geometry = if coordinates.len() == 1 {
                    json!({"type":"Polygon","coordinates":coordinates[0]})
                } else {
                    json!({"type":"MultiPolygon","coordinates":coordinates})
                };
                json!({"type":"Feature","id":feature.id,"properties":props,"geometry":geometry})
            })
            .collect::<Vec<_>>();
        let output = serde_json::to_string_pretty(
            &json!({"type":"FeatureCollection","terra":metadata,"features":features}),
        )
        .map_err(|e| HistoryError::Invalid(e.to_string()))?;
        if output.len() as u64 > MAX_BYTES {
            return Err(HistoryError::Invalid("export exceeds 32 MiB".into()));
        }
        Ok(output)
    }
}

fn unwrapped(ring: &[GeoPosition]) -> Vec<(f64, f64)> {
    let mut output = Vec::with_capacity(ring.len());
    let mut previous = 0.0;
    let mut unwrapped_lon = 0.0;
    for (index, point) in ring.iter().enumerate() {
        if index == 0 {
            unwrapped_lon = point.longitude;
        } else {
            let delta = point.longitude - previous;
            // A deliberate pole-spanning edge can represent the world seam.
            unwrapped_lon += if delta.abs() > 359.999 && point.latitude.abs() > 89.999 {
                delta
            } else {
                normalize_longitude(delta)
            };
        }
        output.push((unwrapped_lon, point.latitude));
        previous = point.longitude;
    }
    output
}

fn in_ring(position: GeoPosition, ring: &[GeoPosition]) -> bool {
    if ring.is_empty() {
        return false;
    }
    let coordinates = unwrapped(ring);
    let min = coordinates
        .iter()
        .map(|p| p.0)
        .fold(f64::INFINITY, f64::min);
    let max = coordinates
        .iter()
        .map(|p| p.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let longitude = normalize_longitude(position.longitude);
    for shift in -2..=2 {
        let x = longitude + shift as f64 * 360.0;
        let y = position.latitude;
        if x < min - 1e-10 || x > max + 1e-10 {
            continue;
        }
        let mut inside = false;
        for pair in coordinates.windows(2) {
            let (ax, ay) = pair[0];
            let (bx, by) = pair[1];
            let cross = (x - ax) * (by - ay) - (y - ay) * (bx - ax);
            if cross.abs() < 1e-10
                && x >= ax.min(bx) - 1e-10
                && x <= ax.max(bx) + 1e-10
                && y >= ay.min(by) - 1e-10
                && y <= ay.max(by) + 1e-10
            {
                return true;
            }
            if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
                inside = !inside;
            }
        }
        if inside {
            return true;
        }
    }
    false
}

impl PoliticalPolygon {
    pub fn contains(&self, position: GeoPosition) -> bool {
        self.rings
            .first()
            .is_some_and(|outer| in_ring(position, outer))
            && !self
                .rings
                .iter()
                .skip(1)
                .any(|hole| in_ring(position, hole))
    }
    pub fn intersects_bounds(&self, bounds: GeoBounds) -> bool {
        let Some(exterior) = self.rings.first() else {
            return false;
        };
        let coords = unwrapped(exterior);
        let min_y = coords.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = coords.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        if max_y < bounds.south || min_y > bounds.north {
            return false;
        }
        let min_x = coords.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = coords.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let intervals = if bounds.west <= bounds.east {
            vec![(bounds.west, bounds.east)]
        } else {
            vec![(bounds.west, 180.0), (-180.0, bounds.east)]
        };
        intervals.iter().any(|&(west, east)| {
            (-2..=2).any(|shift| {
                max_x >= west + shift as f64 * 360.0 && min_x <= east + shift as f64 * 360.0
            })
        })
    }
}
