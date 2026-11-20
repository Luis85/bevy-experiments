//! Supported shortcut planning for ordinary owned ground walks. Planning only
//! changes a route; each actual step still passes the ordinary motion resolver.
use super::*;

const MAX_SHORTCUT_M: f64 = 160.;
const SAMPLE_STEP_M: f64 = 0.25;
const MAX_COMMAND_SAMPLES: usize = 16_384;
const MAX_CANDIDATES_PER_LEG: usize = 8;
const MAX_ROUTE_WAYPOINTS: usize = 129;

enum CorridorFailure {
    Unavailable,
    Body(u64),
}

impl SandboxState {
    /// Issue the ordinary owned group order, then shorten pedestrian routes
    /// only where complete supported, source-clear and body-clear corridors
    /// exist. A stopped body may contribute two local avoidance corners; every
    /// leg must be admitted before replacing any of the existing route.
    /// Missing terrain leaves the existing road/facility route intact.
    /// The callback must return actual support without sliding the horizontal
    /// coordinate. Bodies and authored structures are checked here as well.
    ///
    /// This is an authority adapter for the existing MoveUnits intent, not a
    /// coordinate update. It preserves ownership, licensing, group and distance
    /// admission, selected actor, economy and the ordinary command destination.
    /// Shortcut waypoints preserve their exact geographic coordinates and use
    /// the resolver's actual supported height. No arrival rule is changed.
    /// Hosts hydrate regional command dependencies before calling it.
    pub fn move_units_with_environment<F>(
        &mut self,
        player: u64,
        unit_ids: Vec<u64>,
        destination: GeoPosition,
        catalog: &Catalog,
        resolve: F,
    ) -> Result<Outcome, SandboxError>
    where
        F: Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
    {
        if self.is_partial() {
            return Err(SandboxError::Invalid(
                "hydrate the complete regional world before planning a ground route".into(),
            ));
        }
        let contacts = LivingContacts::new(self)?;
        let outcome = self.apply(
            player,
            crate::Command::Living {
                action: LivingCommand::MoveUnits {
                    unit_ids: unit_ids.clone(),
                    destination,
                },
            },
            catalog,
        )?;
        let mut footprints = self.living.facility_footprints();
        footprints.extend(self.structures.values().map(|s| (s.position, s.radius_m)));
        // One total command budget also bounds a maximum-size group. A unit
        // whose budget is exhausted keeps its ordinary order/route proposal.
        let mut samples_left = MAX_COMMAND_SAMPLES;
        for id in unit_ids {
            let unit = &self.living.units[&id];
            if !matches!(
                unit.kind,
                ActorKind::Human | ActorKind::Creature | ActorKind::Animal
            ) || unit.path.len() < 2
            {
                continue;
            }
            let path = WalkingPlanner {
                unit,
                contacts: &contacts,
                footprints: &footprints,
                resolve: &resolve,
                samples_left: &mut samples_left,
            }
            .shorten();
            self.living.units.get_mut(&id).unwrap().path = path;
        }
        Ok(outcome)
    }
}

struct WalkingPlanner<'a, F> {
    unit: &'a LivingUnit,
    contacts: &'a LivingContacts,
    footprints: &'a [(GeoPosition, f64)],
    resolve: &'a F,
    samples_left: &'a mut usize,
}

impl<F> WalkingPlanner<'_, F>
where
    F: Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
{
    fn shorten(&mut self) -> Vec<GeoPosition> {
        let path = self.unit.path.clone();
        let mut result = Vec::with_capacity(path.len());
        let mut from = self.unit.position;
        let mut index = 0;
        while index < path.len() {
            if *self.samples_left < 9 {
                result.extend_from_slice(&path[index..]);
                break;
            }
            let mut travelled = 0.;
            let mut previous = from;
            let costs: Vec<_> = path[index..]
                .iter()
                .map(|point| {
                    travelled += previous.surface_distance_m(*point);
                    previous = *point;
                    travelled
                })
                .collect();
            let mut selected = (index, vec![path[index]]);
            // Prefer the farthest useful visible point. This removes whole
            // perimeter excursions without changing a hidden or blocked leg.
            for candidate in ((index + 1)..path.len())
                .rev()
                .filter(|n| {
                    let distance = from.surface_distance_m(path[*n]);
                    distance <= MAX_SHORTCUT_M && costs[*n - index] > distance + 2.
                })
                .take(MAX_CANDIDATES_PER_LEG)
            {
                match self.corridor(from, path[candidate]) {
                    Ok(point) => {
                        selected = (candidate, vec![point]);
                        break;
                    }
                    Err(CorridorFailure::Body(obstacle))
                        if result.len() + 3 + path.len() - candidate - 1 <= MAX_ROUTE_WAYPOINTS =>
                    {
                        if let Some(detour) =
                            self.detour(from, path[candidate], obstacle, costs[candidate - index])
                        {
                            selected = (candidate, detour.to_vec());
                            break;
                        }
                    }
                    _ => {}
                }
            }
            from = *selected.1.last().unwrap();
            result.extend(selected.1);
            index = selected.0 + 1;
        }
        result
    }

    fn detour(
        &mut self,
        from: GeoPosition,
        to: GeoPosition,
        obstacle: u64,
        old_cost: f64,
    ) -> Option<[GeoPosition; 3]> {
        // Only the first body that blocked a supported straight corridor can
        // suggest corners. There is no recursive obstacle search, and moving
        // bodies do not provide proposals. The helper reserves body clearance
        // for the ordinary controller's waypoint arrival tolerance as well.
        let mut candidates: Vec<_> = [1., -1.]
            .into_iter()
            .filter_map(|side| {
                let corners =
                    self.contacts
                        .avoidance_corners(self.unit.id, obstacle, from, to, side)?;
                let points = [corners[0], corners[1], to];
                let mut previous = from;
                let mut cost = 0.;
                let mut samples = 0;
                for point in points {
                    let distance = previous.surface_distance_m(point);
                    cost += distance;
                    samples += Self::corridor_samples(distance);
                    previous = point;
                }
                // The whole replacement, not just its straight-line span,
                // stays inside the existing 160 m local planning envelope.
                (cost <= MAX_SHORTCUT_M && cost + 2. < old_cost).then_some((cost, samples, points))
            })
            .collect();
        // Stable sorting leaves the positive side first for an exact tie.
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, samples, mut points) in candidates {
            if samples > *self.samples_left {
                continue;
            }
            let mut previous = from;
            let mut admitted = true;
            for point in &mut points {
                let Ok(supported) = self.corridor(previous, *point) else {
                    admitted = false;
                    break;
                };
                *point = supported;
                previous = supported;
            }
            if admitted {
                return Some(points);
            }
        }
        None
    }

    fn corridor_samples(distance: f64) -> usize {
        let steps = (distance / SAMPLE_STEP_M).ceil().max(1.) as usize;
        steps.saturating_add(1).saturating_mul(9)
    }

    fn corridor(
        &mut self,
        from: GeoPosition,
        to: GeoPosition,
    ) -> Result<GeoPosition, CorridorFailure> {
        if !facility_path_clear(self.unit.kind, from, to, self.footprints) {
            return Err(CorridorFailure::Unavailable);
        }
        let distance = from.surface_distance_m(to);
        if distance > MAX_SHORTCUT_M {
            return Err(CorridorFailure::Unavailable);
        }
        let steps = (distance / SAMPLE_STEP_M).ceil().max(1.) as usize;
        if Self::corridor_samples(distance) > *self.samples_left {
            return Err(CorridorFailure::Unavailable);
        }
        let frame = LocalFrame::new(from);
        let delta = frame.to_local(to);
        let mut previous = self
            .support(from, from)
            .ok_or(CorridorFailure::Unavailable)?;
        if (previous.altitude_m - from.altitude_m).abs() > 1. {
            return Err(CorridorFailure::Unavailable);
        }
        for step in 1..=steps {
            let mut candidate = if step == steps {
                to
            } else {
                frame
                    .to_geo(delta.map(|v| v * step as f64 / steps as f64))
                    .map_err(|_| CorridorFailure::Unavailable)?
            };
            candidate.altitude_m = previous.altitude_m;
            let supported = self
                .support(previous, candidate)
                .ok_or(CorridorFailure::Unavailable)?;
            if (supported.altitude_m - previous.altitude_m).abs() > 0.4
                || !facility_path_clear(self.unit.kind, previous, supported, self.footprints)
            {
                return Err(CorridorFailure::Unavailable);
            }
            let motion = self
                .contacts
                .resolve(
                    self.unit.id,
                    previous,
                    self.unit.heading_deg,
                    supported,
                    self.unit.heading_deg,
                    [0.; 3],
                )
                .map_err(|_| CorridorFailure::Unavailable)?;
            if let Some(contact) = motion.contact {
                return Err(CorridorFailure::Body(contact.other_id));
            }
            previous = supported;
        }
        Ok(previous)
    }

    fn checked(&mut self, from: GeoPosition, to: GeoPosition) -> Option<GeoPosition> {
        if *self.samples_left == 0 {
            return None;
        }
        *self.samples_left -= 1;
        (self.resolve)(self.unit.kind, from, to)
            .filter(|p| validate_living_position(*p).is_ok() && p.surface_distance_m(to) < 1e-6)
            .map(|supported| GeoPosition {
                altitude_m: supported.altitude_m,
                ..to
            })
    }

    fn support(&mut self, from: GeoPosition, point: GeoPosition) -> Option<GeoPosition> {
        let center = self.checked(from, point)?;
        let [half_x, half_z] = match self.unit.kind {
            ActorKind::Human => [0.275, 0.275],
            ActorKind::Animal => [0.325, 0.625],
            _ => [0.425, 0.425],
        };
        let frame = LocalFrame::new(center);
        // These ground actors use axis-aligned contact footprints. Sampling
        // that same support footprint avoids heading-dependent admission.
        for (x, z) in [
            (-half_x, -half_z),
            (half_x, -half_z),
            (-half_x, half_z),
            (half_x, half_z),
            (-half_x, 0.),
            (half_x, 0.),
            (0., -half_z),
            (0., half_z),
        ] {
            let mut sample = frame.to_geo([x, 0., z]).ok()?;
            sample.altitude_m = center.altitude_m;
            let support = self.checked(sample, sample)?;
            if (support.altitude_m - center.altitude_m).abs() > 0.5 {
                return None;
            }
        }
        Some(center)
    }
}
