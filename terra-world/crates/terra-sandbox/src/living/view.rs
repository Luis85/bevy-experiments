//! Public, intentionally partial network replication.
use super::*;
use crate::{GeoSpatialIndex, SpatialQuery};

/// A broadcast-scoped immutable index. Borrowing the world prevents a host
/// from moving/removing bodies while their indexed positions are in use.
#[derive(Debug)]
pub struct LivingInterestIndex<'a> {
    world: &'a LivingWorld,
    positions: GeoSpatialIndex,
    owners: BTreeMap<u64, Vec<u64>>,
}

impl<'a> LivingInterestIndex<'a> {
    pub fn new(world: &'a LivingWorld) -> Result<Self, SandboxError> {
        let positions = GeoSpatialIndex::from_positions(
            256.,
            world.units.values().map(|u| (u.id, u.position)),
        )?;
        let mut owners: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for unit in world.units.values() {
            if let Some(owner) = unit.owner {
                owners.entry(owner).or_default().push(unit.id);
            }
        }
        Ok(Self {
            world,
            positions,
            owners,
        })
    }

    /// Includes the principal's owned bodies even outside the geographic AOI.
    pub fn query_units(
        &self,
        player: u64,
        center: GeoPosition,
        radius: f64,
    ) -> Result<SpatialQuery, SandboxError> {
        if !radius.is_finite() || radius < 0. {
            return Err(SandboxError::Invalid("interest radius".into()));
        }
        let mut result = self.positions.query(center, radius.min(5000.))?;
        if let Some(owned) = self.owners.get(&player) {
            result.ids.extend(owned.iter().copied());
            result.ids.sort_unstable();
            result.ids.dedup();
        }
        Ok(result)
    }
}

impl LivingWorld {
    pub fn snapshot(&self, player: u64, center: GeoPosition, radius: f64) -> LivingView {
        self.snapshot_selected(player, center, radius, None)
    }

    pub fn snapshot_indexed(
        &self,
        player: u64,
        center: GeoPosition,
        radius: f64,
        index: &LivingInterestIndex<'_>,
    ) -> Result<LivingView, SandboxError> {
        if !std::ptr::eq(self, index.world) {
            return Err(SandboxError::Invalid(
                "interest index world mismatch".into(),
            ));
        }
        let ids = index.query_units(player, center, radius)?.ids;
        Ok(self.snapshot_selected(player, center, radius, Some(&ids)))
    }

    fn snapshot_selected(
        &self,
        player: u64,
        center: GeoPosition,
        radius: f64,
        indexed_ids: Option<&[u64]>,
    ) -> LivingView {
        if !self.enabled {
            return LivingView::default();
        }
        let near = |p: GeoPosition| center.surface_distance_m(p) <= radius.min(5000.);
        let selected: Vec<_> = if let Some(ids) = indexed_ids {
            ids.iter().filter_map(|id| self.units.get(id)).collect()
        } else {
            self.units
                .values()
                .filter(|u| u.owner == Some(player) || near(u.position))
                .collect()
        };
        // Calculate geographic distance once per candidate, rather than once
        // for every comparison in the priority sort.
        let mut units: Vec<_> = selected
            .into_iter()
            .map(|u| (u, center.surface_distance_m(u.position)))
            .collect();
        let protected = self
            .accounts
            .get(&player)
            .map(|a| [a.active_unit, a.avatar_unit]);
        units.sort_by(|(a, da), (b, db)| {
            protected
                .is_none_or(|ids| !ids.contains(&a.id))
                .cmp(&protected.is_none_or(|ids| !ids.contains(&b.id)))
                .then_with(|| (a.owner != Some(player)).cmp(&(b.owner != Some(player))))
                .then(da.total_cmp(db))
                .then(a.id.cmp(&b.id))
        });
        let shops: Vec<_> = self.shops.values().filter(|s| near(s.position)).collect();
        let producers: Vec<_> = self
            .producers
            .values()
            .filter(|s| near(s.position))
            .collect();
        let roads: Vec<_> = self
            .roads
            .iter()
            .filter(|r| r.points.iter().any(|p| near(*p)))
            .collect();
        let truncated = units.len() > 64
            || shops.len() > 8
            || producers.len() > 16
            || self.deliveries.len() > 16
            || roads.len() > 8
            || roads.iter().take(8).any(|r| r.points.len() > 32);
        // Paths and pockets are server-owned details, not needed for drawing.
        let units = units
            .into_iter()
            .take(64)
            .map(|(u, _)| {
                let mut v = u.clone();
                v.path.clear();
                v.path_index = 0;
                v.patrol.clear();
                v.pocket_money = 0;
                v.supplies.clear();
                v
            })
            .collect();
        LivingView {
            enabled: true,
            origin: self.origin,
            account: self.accounts.get(&player).cloned(),
            units,
            shops: shops.into_iter().take(8).cloned().collect(),
            producers: producers.into_iter().take(16).cloned().collect(),
            deliveries: self.deliveries.iter().take(16).cloned().collect(),
            roads: roads
                .into_iter()
                .take(8)
                .map(|r| {
                    let mut r = r.clone();
                    r.points.truncate(32);
                    r
                })
                .collect(),
            elapsed_seconds: self.elapsed_seconds,
            metrics: self.metrics.clone(),
            truncated,
        }
    }

    /// Replace a client's read-only replica. Views are incomplete and must never
    /// be saved as an authoritative world or advanced by the client.
    pub fn from_view(view: LivingView) -> Self {
        let mut world = Self::default();
        world.enabled = view.enabled;
        world.origin = view.origin;
        world.units = view.units.into_iter().map(|u| (u.id, u)).collect();
        if let Some(account) = view.account {
            world.accounts.insert(account.player_id, account);
        }
        world.shops = view.shops.into_iter().map(|s| (s.id, s)).collect();
        world.producers = view.producers.into_iter().map(|s| (s.id, s)).collect();
        world.deliveries = view.deliveries;
        world.roads = view.roads;
        world.elapsed_seconds = view.elapsed_seconds;
        world.metrics = view.metrics;
        world
    }
}
