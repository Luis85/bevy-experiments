//! Public, intentionally partial network replication.
use super::*;

impl LivingWorld {
    pub fn snapshot(&self, player: u64, center: GeoPosition, radius: f64) -> LivingView {
        if !self.enabled {
            return LivingView::default();
        }
        let near = |p: GeoPosition| center.surface_distance_m(p) <= radius.min(5000.);
        let mut units: Vec<_> = self
            .units
            .values()
            .filter(|u| u.owner == Some(player) || near(u.position))
            .collect();
        let protected = self
            .accounts
            .get(&player)
            .map(|a| [a.active_unit, a.avatar_unit]);
        units.sort_by(|a, b| {
            protected
                .is_none_or(|ids| !ids.contains(&a.id))
                .cmp(&protected.is_none_or(|ids| !ids.contains(&b.id)))
                .then_with(|| (a.owner != Some(player)).cmp(&(b.owner != Some(player))))
                .then(
                    center
                        .surface_distance_m(a.position)
                        .total_cmp(&center.surface_distance_m(b.position)),
                )
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
            .map(|u| {
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
        Self {
            enabled: view.enabled,
            origin: view.origin,
            units: view.units.into_iter().map(|u| (u.id, u)).collect(),
            accounts: view.account.into_iter().map(|a| (a.player_id, a)).collect(),
            shops: view.shops.into_iter().map(|s| (s.id, s)).collect(),
            producers: view.producers.into_iter().map(|s| (s.id, s)).collect(),
            deliveries: view.deliveries,
            roads: view.roads,
            elapsed_seconds: view.elapsed_seconds,
            metrics: view.metrics,
            ..Self::default()
        }
    }
}
