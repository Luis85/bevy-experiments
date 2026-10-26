use bevy::prelude::*;
use std::{collections::BTreeMap, path::Path};
use terra_core::{GeoPosition, WorldClock};
use terra_history::{PoliticalFeature, PoliticalKind, PoliticalPolygon, PoliticsDataset};

#[derive(Resource)]
pub struct PoliticalWorld {
    pub dataset: Option<PoliticsDataset>,
    pub atlas: bool,
    pub borders: bool,
    pub drawing: bool,
    pub draft: Vec<GeoPosition>,
    pub revision: u64,
    pub atlas_zoom: f32,
    pub atlas_center: Vec2,
    pub hover: String,
    pub selected: Option<GeoPosition>,
}
impl PoliticalWorld {
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let dataset = path
            .map(PoliticsDataset::load)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            dataset,
            atlas: false,
            borders: true,
            drawing: false,
            draft: Vec::new(),
            revision: 0,
            atlas_zoom: 1.,
            atlas_center: Vec2::ZERO,
            hover: String::new(),
            selected: None,
        })
    }
    pub fn describe(&self, position: GeoPosition, clock: &WorldClock) -> String {
        let Some(data) = &self.dataset else {
            return "No political dataset loaded. Use --politics FILE.".into();
        };
        match data.at(position, clock.unix_seconds as i64) {
            Ok(features) => {
                if features.is_empty() {
                    "No country or territorial claim at this coordinate.".into()
                } else {
                    features
                        .iter()
                        .map(|f| format!("{} [{}]", f.name, f.status))
                        .collect::<Vec<_>>()
                        .join(" / ")
                }
            }
            Err(e) => format!("Political coverage: {e}"),
        }
    }
    /// Frame the geometry visible at the requested date, including small regional packs.
    /// Date-line-spanning datasets retain a global extent in this flat atlas.
    pub fn fit_at_epoch(&mut self, epoch: i64) -> bool {
        let Some(data) = &self.dataset else {
            return false;
        };
        if !data.contains_time(epoch) {
            return false;
        }
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);
        for feature in &data.features {
            if !terra_history::valid_at(epoch, feature.valid_from_unix, feature.valid_until_unix) {
                continue;
            }
            for point in feature.polygons.iter().flat_map(|p| &p.rings).flatten() {
                let mapped = Vec2::new(
                    (point.longitude / 180. * 500.) as f32,
                    (point.latitude / 90. * 250.) as f32,
                );
                minimum = minimum.min(mapped);
                maximum = maximum.max(mapped);
            }
        }
        if !minimum.is_finite() || !maximum.is_finite() {
            return false;
        }
        let extent = (maximum - minimum).max(Vec2::splat(0.00001));
        self.atlas_center = (minimum + maximum) * 0.5;
        self.atlas_zoom = (900. / extent.x).min(450. / extent.y).clamp(1., 20_000.);
        true
    }
    pub fn finish_draft(&mut self, epoch: i64, faction_id: Option<u64>) -> Result<String, String> {
        if self.draft.len() > 128 {
            return Err("Territories are limited to 128 vertices.".into());
        }
        if self.draft.len() < 3 {
            return Err("A territory needs at least three vertices.".into());
        }
        validate_draft(&self.draft)?;
        let data = self
            .dataset
            .as_ref()
            .ok_or("Load a political dataset before drawing.")?;
        if !data.contains_time(epoch) {
            return Err("Dataset has no coverage at the simulation date.".into());
        }
        let mut edited = data.clone();
        let mut serial = self.revision + 1;
        while data
            .features
            .iter()
            .any(|f| f.id == format!("scenario-territory-{serial}"))
        {
            serial += 1;
        }
        let id = format!("scenario-territory-{serial}");
        let mut ring = self.draft.clone();
        ring.push(ring[0]);
        edited.features.push(PoliticalFeature {
            id: id.clone(),
            name: format!("Scenario territory {serial}"),
            kind: PoliticalKind::ScenarioTerritory,
            country: String::new(),
            claimants: Vec::new(),
            status: "player-authored scenario territory".into(),
            valid_from_unix: epoch,
            valid_until_unix: edited.valid_until_unix,
            polygons: vec![PoliticalPolygon { rings: vec![ring] }],
            faction_id,
            properties: BTreeMap::new(),
        });
        edited.validate().map_err(|e| e.to_string())?;
        // Drawing adds authored claims; retain the original source attribution.
        edited.synthetic = true;
        if !edited.title.ends_with(" + authored scenario territories") {
            edited.title.push_str(" + authored scenario territories");
        }
        self.dataset = Some(edited);
        self.draft.clear();
        self.drawing = false;
        self.revision += 1;
        Ok(format!("Territory drawn: {id}. Export GeoJSON to keep it."))
    }
    /// Export edited geometry under a new content-derived revision ID. The in-memory
    /// base ID remains stable so local saves can identify their original base world.
    pub fn export_geojson(&self) -> Result<String, String> {
        let mut data = self
            .dataset
            .clone()
            .ok_or("No political dataset to export")?;
        if data
            .features
            .iter()
            .any(|f| f.kind == PoliticalKind::ScenarioTerritory)
        {
            let serialized = data.to_geojson().map_err(|e| e.to_string())?;
            // Non-cryptographic revision fingerprint; never used as authentication.
            let hash = serialized.bytes().fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x100000001b3)
            });
            data.id = format!("scenario-export-{hash:016x}");
        }
        data.to_geojson().map_err(|e| e.to_string())
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        crate::sandbox::atomic_write(path, self.export_geojson()?.as_bytes())
    }
}

fn validate_draft(points: &[GeoPosition]) -> Result<(), String> {
    if points.len() < 3 {
        return Err("A territory needs at least three vertices.".into());
    }
    let anchor = points[0].longitude;
    let xy: Vec<_> = points
        .iter()
        .map(|p| {
            (
                terra_core::normalize_longitude(p.longitude - anchor),
                p.latitude,
            )
        })
        .collect();
    let cross = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    };
    let between = |a: (f64, f64), b: (f64, f64), p: (f64, f64)| {
        p.0 >= a.0.min(b.0) - 1e-12
            && p.0 <= a.0.max(b.0) + 1e-12
            && p.1 >= a.1.min(b.1) - 1e-12
            && p.1 <= a.1.max(b.1) + 1e-12
    };
    for i in 0..xy.len() {
        let a = xy[i];
        let b = xy[(i + 1) % xy.len()];
        if (a.0 - b.0).hypot(a.1 - b.1) < 1e-10 {
            return Err("Consecutive territory vertices must be distinct.".into());
        }
        for j in i + 1..xy.len() {
            if j == i + 1 || (i == 0 && j == xy.len() - 1) {
                continue;
            }
            let c = xy[j];
            let d = xy[(j + 1) % xy.len()];
            let u = cross(a, b, c);
            let v = cross(a, b, d);
            let w = cross(c, d, a);
            let x = cross(c, d, b);
            if (u * v < 0. && w * x < 0.)
                || (u.abs() < 1e-12 && between(a, b, c))
                || (v.abs() < 1e-12 && between(a, b, d))
                || (w.abs() < 1e-12 && between(c, d, a))
                || (x.abs() < 1e-12 && between(c, d, b))
            {
                return Err(
                    "Territory edges intersect. Undo a vertex or redraw a simple boundary.".into(),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(lat: f64, lon: f64) -> GeoPosition {
        GeoPosition::new(lat, lon, 0.).unwrap()
    }
    fn world() -> PoliticalWorld {
        let mut w = PoliticalWorld::load(None).unwrap();
        w.dataset = Some(PoliticsDataset {
            id: "base-v1".into(),
            title: "Authored test".into(),
            valid_from_unix: 0,
            valid_until_unix: None,
            source_url: "urn:terra:test".into(),
            license: "MIT".into(),
            attribution: "Authored test".into(),
            synthetic: true,
            features: Vec::new(),
        });
        w
    }
    #[test]
    fn atlas_frames_a_small_historical_region_and_keeps_missing_dates_explicit() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-history/data/synthetic-politics-eras.geojson");
        let mut w = PoliticalWorld::load(Some(&path)).unwrap();
        assert!(w.fit_at_epoch(-631_152_000));
        assert!(w.atlas_zoom > 1000. && w.atlas_zoom <= 20_000.);
        assert!((w.atlas_center.x as f64 * 180. / 500. - 11.574).abs() < 0.01);
        assert!((w.atlas_center.y as f64 * 90. / 250. - 48.139).abs() < 0.01);
        let view = (w.atlas_center, w.atlas_zoom);
        assert!(!w.fit_at_epoch(1_800_000_000));
        assert_eq!((w.atlas_center, w.atlas_zoom), view);
    }
    #[test]
    fn drawing_closes_a_faction_boundary_and_exports_a_new_revision() {
        let mut w = world();
        w.draft = vec![p(0., 0.), p(0., 1.), p(1., 1.), p(1., 0.)];
        w.finish_draft(0, Some(7)).unwrap();
        let data = w.dataset.as_ref().unwrap();
        assert_eq!(data.id, "base-v1");
        assert_eq!(data.features[0].faction_id, Some(7));
        assert_eq!(data.features[0].polygons[0].rings[0].len(), 5);
        let exported = PoliticsDataset::from_geojson(&w.export_geojson().unwrap()).unwrap();
        assert_ne!(exported.id, data.id);
        assert_eq!(exported.attribution, data.attribution);
    }
    #[test]
    fn self_intersecting_drafts_leave_the_dataset_untouched() {
        let mut w = world();
        let before = w.dataset.clone();
        w.draft = vec![p(0., 0.), p(1., 1.), p(0., 1.), p(1., 0.)];
        assert!(w.finish_draft(0, None).is_err());
        assert_eq!(w.dataset, before);
    }
}

#[cfg(feature = "render")]
mod drawing {
    use super::*;
    use crate::{ResidentChunks, WorldEnvironment, WorldRuntime, geometry::MeshBuilder};
    #[derive(Component)]
    pub(crate) struct AtlasMesh;
    #[derive(Resource, Default)]
    pub struct AtlasRenderState {
        entities: Vec<(Entity, Handle<Mesh>)>,
        revision: u64,
        day: i64,
        zoom: f32,
        initialized: bool,
        material: Option<Handle<StandardMaterial>>,
    }
    fn map(p: GeoPosition) -> Vec3 {
        Vec3::new(
            (p.longitude / 180. * 500.) as f32,
            (p.latitude / 90. * 250.) as f32,
            0.,
        )
    }
    fn line(builder: &mut MeshBuilder, a: Vec3, b: Vec3, width: f32, color: [f32; 4]) {
        let side = (b - a).cross(Vec3::Z).normalize_or_zero() * width;
        builder.quad(a - side, b - side, b + side, a + side, color);
    }
    fn clip_local_border(
        a: GeoPosition,
        b: GeoPosition,
        focus: GeoPosition,
    ) -> Option<(GeoPosition, GeoPosition)> {
        let ax = terra_core::normalize_longitude(a.longitude - focus.longitude);
        let ay = a.latitude - focus.latitude;
        let dx = terra_core::normalize_longitude(b.longitude - a.longitude);
        let dy = b.latitude - a.latitude;
        let mut low: f64 = 0.;
        let mut high: f64 = 1.;
        for (p, q) in [
            (-dx, ax + 0.2),
            (dx, 0.2 - ax),
            (-dy, ay + 0.1),
            (dy, 0.1 - ay),
        ] {
            if p.abs() < 1e-12 {
                if q < 0. {
                    return None;
                }
            } else {
                let ratio = q / p;
                if p < 0. {
                    low = low.max(ratio);
                } else {
                    high = high.min(ratio);
                }
                if low > high {
                    return None;
                }
            }
        }
        let at = |t| GeoPosition {
            latitude: a.latitude + dy * t,
            longitude: terra_core::normalize_longitude(focus.longitude + ax + dx * t),
            altitude_m: 0.,
        };
        Some((at(low), at(high)))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bevy injects each ECS system parameter"
    )]
    pub fn render_atlas(
        mut commands: Commands,
        politics: Res<PoliticalWorld>,
        runtime: Res<WorldRuntime>,
        resident: Res<ResidentChunks>,
        mut state: ResMut<AtlasRenderState>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut visibility: Query<&mut Visibility, With<AtlasMesh>>,
        mut gizmos: Gizmos,
    ) {
        for mut visible in &mut visibility {
            *visible = if politics.atlas {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        let epoch = runtime.clock.unix_seconds as i64;
        let day = epoch.div_euclid(86400);
        let zoom = politics.atlas_zoom.max(0.6);
        if politics.atlas {
            if let Some(selected) = politics.selected {
                gizmos.circle(
                    Isometry3d::from_translation(map(selected) + Vec3::Z / zoom),
                    3. / zoom,
                    Color::srgb(1., 0.85, 0.45),
                );
            }
            for pair in politics.draft.windows(2) {
                let a = map(pair[0]) + Vec3::Z / zoom;
                let b = map(pair[1]) + Vec3::Z / zoom;
                gizmos.line(a, b, Color::srgb(1., 0.73, 0.35));
            }
            for p in &politics.draft {
                gizmos.circle(
                    Isometry3d::from_translation(map(*p) + Vec3::Z / zoom),
                    1.5 / zoom,
                    Color::srgb(1., 0.73, 0.35),
                );
            }
        } else if politics.borders
            && let Some(data) = &politics.dataset
            && let Ok(features) = data.visible(
                terra_core::GeoBounds {
                    south: (runtime.focus.latitude - 0.1).max(-90.),
                    north: (runtime.focus.latitude + 0.1).min(90.),
                    west: terra_core::normalize_longitude(runtime.focus.longitude - 0.2),
                    east: terra_core::normalize_longitude(runtime.focus.longitude + 0.2),
                },
                epoch,
            )
        {
            for feature in features {
                for polygon in &feature.polygons {
                    for ring in &polygon.rings {
                        for pair in ring.windows(2) {
                            let Some((from, to)) =
                                clip_local_border(pair[0], pair[1], runtime.focus)
                            else {
                                continue;
                            };
                            let steps =
                                (from.surface_distance_m(to) / 100.).ceil().clamp(1., 64.) as usize;
                            let lon_delta =
                                terra_core::normalize_longitude(to.longitude - from.longitude);
                            let mut previous: Option<Vec3> = None;
                            for index in 0..=steps {
                                let t = index as f64 / steps as f64;
                                let p = GeoPosition {
                                    latitude: from.latitude + (to.latitude - from.latitude) * t,
                                    longitude: terra_core::normalize_longitude(
                                        from.longitude + lon_delta * t,
                                    ),
                                    altitude_m: 0.,
                                };
                                if let Some(height) = resident.terrain_height(p) {
                                    let q = Vec3::from_array(
                                        runtime
                                            .frame
                                            .to_local(GeoPosition {
                                                altitude_m: height + 2.,
                                                ..p
                                            })
                                            .map(|v| v as f32),
                                    );
                                    if let Some(old) = previous {
                                        gizmos.line(old, q, Color::srgb(0.95, 0.76, 0.32));
                                    }
                                    previous = Some(q);
                                } else {
                                    previous = None;
                                }
                            }
                        }
                    }
                }
            }
        }
        if state.initialized
            && state.revision == politics.revision
            && state.day == day
            && state.zoom == zoom
        {
            return;
        }
        state.initialized = true;
        state.revision = politics.revision;
        state.day = day;
        state.zoom = zoom;
        for (e, m) in state.entities.drain(..) {
            commands.entity(e).despawn();
            meshes.remove(m.id());
        }
        let mut builder = MeshBuilder::default();
        builder.quad(
            Vec3::new(-515., -265., -2. / zoom),
            Vec3::new(515., -265., -2. / zoom),
            Vec3::new(515., 265., -2. / zoom),
            Vec3::new(-515., 265., -2. / zoom),
            [0.045, 0.11, 0.14, 1.],
        );
        for lon in (-180..=180).step_by(30) {
            let x = lon as f32 / 180. * 500.;
            line(
                &mut builder,
                Vec3::new(x, -250., -1. / zoom),
                Vec3::new(x, 250., -1. / zoom),
                0.12 / zoom,
                [0.13, 0.23, 0.25, 1.],
            );
        }
        for lat in (-90..=90).step_by(30) {
            let y = lat as f32 / 90. * 250.;
            line(
                &mut builder,
                Vec3::new(-500., y, -1. / zoom),
                Vec3::new(500., y, -1. / zoom),
                0.12 / zoom,
                [0.13, 0.23, 0.25, 1.],
            );
        }
        if let Some(data) = &politics.dataset
            && data.contains_time(epoch)
        {
            for feature in &data.features {
                if !terra_history::valid_at(
                    epoch,
                    feature.valid_from_unix,
                    feature.valid_until_unix,
                ) {
                    continue;
                }
                let color = if feature.faction_id.is_some() {
                    [0.58, 0.88, 0.43, 1.]
                } else if feature.kind == PoliticalKind::DisputedArea
                    || feature.status.to_lowercase().contains("disput")
                {
                    [0.95, 0.64, 0.31, 1.]
                } else {
                    [0.51, 0.77, 0.74, 1.]
                };
                for polygon in &feature.polygons {
                    for ring in &polygon.rings {
                        for pair in ring.windows(2) {
                            if (pair[0].longitude - pair[1].longitude).abs() < 180. {
                                line(&mut builder, map(pair[0]), map(pair[1]), 0.65 / zoom, color);
                            }
                        }
                    }
                }
            }
        }
        if let Some(mesh) = builder.finish() {
            let handle = meshes.add(mesh);
            let mat = state
                .material
                .get_or_insert_with(|| {
                    materials.add(StandardMaterial {
                        unlit: true,
                        cull_mode: None,
                        fog_enabled: false,
                        ..default()
                    })
                })
                .clone();
            let e = commands
                .spawn((
                    AtlasMesh,
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(mat),
                    Transform::default(),
                    if politics.atlas {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ))
                .id();
            state.entities.push((e, handle));
        }
    }
}
#[cfg(feature = "render")]
pub(crate) use drawing::{AtlasRenderState, render_atlas};
