//! Revisioned, bounded replication of public interest views.
//!
//! Deltas describe the complete current interest view, including removals when
//! an object leaves interest. They are not persistent world deletion commands.
//! TCP ordering lets each delta refer to the last queued revision. A missing
//! baseline requires a keyframe; no partially applied state is installed.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use terra_core::{ActorState, GeoPosition};
use terra_sandbox::*;

use crate::{NetError, Result, ServerMessage, encode_frame, protocol::validate_position};

/// Counts replication payloads, not simulation throughput or total socket traffic.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReplicationStats {
    pub wire_bytes: u64,
    /// Bytes the equivalent complete Snapshot/SandboxSnapshot messages would use.
    pub full_snapshot_bytes: u64,
    pub keyframes: u64,
    pub deltas: u64,
    pub entity_upserts: u64,
    pub entity_removals: u64,
    pub resync_requests: u64,
    /// Optional views not built because the peer's output queue had insufficient
    /// headroom. Their revisions do not advance; later views use current state.
    #[serde(default)]
    pub backpressure_deferrals: u64,
}
impl ReplicationStats {
    pub fn accumulate(&mut self, other: &Self) {
        self.wire_bytes = self.wire_bytes.saturating_add(other.wire_bytes);
        self.full_snapshot_bytes = self
            .full_snapshot_bytes
            .saturating_add(other.full_snapshot_bytes);
        self.keyframes = self.keyframes.saturating_add(other.keyframes);
        self.deltas = self.deltas.saturating_add(other.deltas);
        self.entity_upserts = self.entity_upserts.saturating_add(other.entity_upserts);
        self.entity_removals = self.entity_removals.saturating_add(other.entity_removals);
        self.resync_requests = self.resync_requests.saturating_add(other.resync_requests);
        self.backpressure_deferrals = self
            .backpressure_deferrals
            .saturating_add(other.backpressure_deferrals);
    }
    pub fn saved_fraction(&self) -> f64 {
        if self.full_snapshot_bytes == 0 {
            return 0.;
        }
        1. - self.wire_bytes as f64 / self.full_snapshot_bytes as f64
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(deserialize = "K: Deserialize<'de>, T: Deserialize<'de>")
)]
pub struct CollectionDelta<K, T> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upserts: Vec<T>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<K>,
    /// Present only if membership or order changed; must name every result once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<K>>,
}
impl<K, T> Default for CollectionDelta<K, T> {
    fn default() -> Self {
        Self {
            upserts: vec![],
            removed: vec![],
            order: None,
        }
    }
}
impl<K, T> CollectionDelta<K, T> {
    pub fn is_empty(&self) -> bool {
        self.upserts.is_empty() && self.removed.is_empty() && self.order.is_none()
    }
    fn counts(&self) -> (u64, u64) {
        (self.upserts.len() as u64, self.removed.len() as u64)
    }
}
fn diff<K: Ord + Clone, T: Clone + PartialEq>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> K,
) -> CollectionDelta<K, T> {
    let previous: BTreeMap<_, _> = old.iter().map(|v| (key(v), v)).collect();
    let current: BTreeSet<_> = new.iter().map(&key).collect();
    let old_order: Vec<_> = old.iter().map(&key).collect();
    let new_order: Vec<_> = new.iter().map(&key).collect();
    CollectionDelta {
        upserts: new
            .iter()
            .filter(|v| previous.get(&key(v)).is_none_or(|old| *old != *v))
            .cloned()
            .collect(),
        removed: previous
            .keys()
            .filter(|id| !current.contains(*id))
            .cloned()
            .collect(),
        order: (old_order != new_order).then_some(new_order),
    }
}
fn apply<K: Ord + Clone, T: Clone>(
    old: &[T],
    change: &CollectionDelta<K, T>,
    limit: usize,
    key: impl Fn(&T) -> K,
) -> Result<Vec<T>> {
    if old.len() > limit
        || change.upserts.len() > limit
        || change.removed.len() > limit
        || change.order.as_ref().is_some_and(|v| v.len() > limit)
    {
        return invalid("collection count exceeds replication budget");
    }
    let mut values: BTreeMap<K, T> = old.iter().map(|v| (key(v), v.clone())).collect();
    if values.len() != old.len() {
        return invalid("duplicate baseline identity");
    }
    let removed: BTreeSet<_> = change.removed.iter().cloned().collect();
    let inserted: BTreeSet<_> = change.upserts.iter().map(&key).collect();
    if removed.len() != change.removed.len()
        || inserted.len() != change.upserts.len()
        || !removed.is_disjoint(&inserted)
    {
        return invalid("duplicate or conflicting delta identity");
    }
    let changed_membership =
        !removed.is_empty() || inserted.iter().any(|id| !values.contains_key(id));
    if changed_membership && change.order.is_none() {
        return invalid("membership changes require a complete order");
    }
    for id in &removed {
        if values.remove(id).is_none() {
            return invalid("delta removes absent identity");
        }
    }
    for value in &change.upserts {
        values.insert(key(value), value.clone());
    }
    if values.len() > limit {
        return invalid("reconstructed collection exceeds replication budget");
    }
    let order = change
        .order
        .clone()
        .unwrap_or_else(|| old.iter().map(&key).collect());
    if order.len() != values.len() {
        return invalid("delta order does not cover result");
    }
    let mut result = Vec::with_capacity(order.len());
    for id in order {
        result.push(
            values
                .remove(&id)
                .ok_or_else(|| NetError::Protocol("unknown or duplicate order identity".into()))?,
        );
    }
    Ok(result)
}
fn invalid<T>(message: &str) -> Result<T> {
    Err(NetError::Protocol(message.into()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorUpdate {
    pub revision: u64,
    /// None is a complete keyframe. Some(n) requires revision n locally.
    pub base_revision: Option<u64>,
    pub tick: u64,
    pub unix_seconds: f64,
    pub actors: CollectionDelta<u64, ActorState>,
    pub truncated: bool,
    pub total_in_interest: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxUpdate {
    pub revision: u64,
    pub base_revision: Option<u64>,
    pub tick: u64,
    /// A keyframe has full only; a delta has delta only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full: Option<SandboxView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<SandboxDelta>,
}

/// Small scalar state is always sent. Entity collections carry changes only.
/// An explicit header also preserves nullable values (e.g. clearing a home or
/// account) without conflating JSON null with an omitted patch field.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxHeader {
    pub version: u32,
    pub tick_seconds: f64,
    pub truncated: bool,
    pub living_enabled: bool,
    pub living_origin: Option<GeoPosition>,
    pub living_account: Option<LivingAccount>,
    pub living_elapsed_seconds: f64,
    pub living_metrics: LivingMetrics,
    pub living_truncated: bool,
    pub ecology_metrics: EcologyMetrics,
    pub ecology_truncated: bool,
    pub settlement_progress: Option<PlayerProgress>,
    pub settlement_wages_paid: u64,
    pub settlement_elapsed_seconds: f64,
    pub settlement_truncated: bool,
    pub transport_enabled: bool,
    pub transport_profile: Option<CharacterProfile>,
    pub transport_driver: Option<DriverProgress>,
    #[serde(default)]
    pub transport_company: Option<TransportCompany>,
    pub transport_race: Option<RaceRun>,
    pub transport_bests: Vec<RaceBest>,
    /// Tuple entries retain numeric IDs inside serde's tagged message buffer.
    /// JSON object keys would otherwise be strings on the delta path.
    pub transport_profiles: Vec<(u64, CharacterProfile)>,
    pub transport_elapsed_seconds: f64,
    pub transport_truncated: bool,
}
impl SandboxHeader {
    fn from_view(v: &SandboxView) -> Self {
        Self {
            version: v.version,
            tick_seconds: v.tick_seconds,
            truncated: v.truncated,
            living_enabled: v.living.enabled,
            living_origin: v.living.origin,
            living_account: v.living.account.clone(),
            living_elapsed_seconds: v.living.elapsed_seconds,
            living_metrics: v.living.metrics.clone(),
            living_truncated: v.living.truncated,
            ecology_metrics: v.ecology.metrics.clone(),
            ecology_truncated: v.ecology.truncated,
            settlement_progress: v.settlements.progress.clone(),
            settlement_wages_paid: v.settlements.wages_paid,
            settlement_elapsed_seconds: v.settlements.elapsed_seconds,
            settlement_truncated: v.settlements.truncated,
            transport_enabled: v.transport.enabled,
            transport_profile: v.transport.profile.clone(),
            transport_driver: v.transport.driver.clone(),
            transport_company: v.transport.company.clone(),
            transport_race: v.transport.race.clone(),
            transport_bests: v.transport.personal_bests.clone(),
            transport_profiles: v
                .transport
                .visible_profiles
                .iter()
                .map(|(&id, p)| (id, p.clone()))
                .collect(),
            transport_elapsed_seconds: v.transport.elapsed_seconds,
            transport_truncated: v.transport.truncated,
        }
    }
    fn install(&self, v: &mut SandboxView) {
        v.version = self.version;
        v.tick_seconds = self.tick_seconds;
        v.truncated = self.truncated;
        v.living.enabled = self.living_enabled;
        v.living.origin = self.living_origin;
        v.living.account = self.living_account.clone();
        v.living.elapsed_seconds = self.living_elapsed_seconds;
        v.living.metrics = self.living_metrics.clone();
        v.living.truncated = self.living_truncated;
        v.ecology.metrics = self.ecology_metrics.clone();
        v.ecology.truncated = self.ecology_truncated;
        v.settlements.progress = self.settlement_progress.clone();
        v.settlements.wages_paid = self.settlement_wages_paid;
        v.settlements.elapsed_seconds = self.settlement_elapsed_seconds;
        v.settlements.truncated = self.settlement_truncated;
        v.transport.enabled = self.transport_enabled;
        v.transport.profile = self.transport_profile.clone();
        v.transport.driver = self.transport_driver.clone();
        v.transport.company = self.transport_company.clone();
        v.transport.race = self.transport_race.clone();
        v.transport.personal_bests = self.transport_bests.clone();
        v.transport.visible_profiles = self.transport_profiles.iter().cloned().collect();
        v.transport.elapsed_seconds = self.transport_elapsed_seconds;
        v.transport.truncated = self.transport_truncated;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxDelta {
    pub header: SandboxHeader,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<PlayerState>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub factions: CollectionDelta<u64, Faction>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub plots: CollectionDelta<u64, LandPlot>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub structures: CollectionDelta<u64, Structure>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub resource_nodes: CollectionDelta<String, ResourceNode>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub farms: CollectionDelta<u64, FarmPlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_edits: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub units: CollectionDelta<u64, LivingUnit>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub shops: CollectionDelta<u64, Shop>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub producers: CollectionDelta<u64, ProductionSite>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub deliveries: CollectionDelta<u64, Delivery>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub roads: CollectionDelta<String, RoadRoute>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub animals: CollectionDelta<u64, WildlifeAnimal>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub habitats: CollectionDelta<String, WildlifeHabitat>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub natural_resources: CollectionDelta<String, NaturalResource>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub territories: CollectionDelta<u64, Settlement>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub contracts: CollectionDelta<u64, SupplyContract>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub vehicles: CollectionDelta<u64, VehicleState>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub visible_vehicles: CollectionDelta<u64, VehicleAppearance>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub transport_facilities: CollectionDelta<u64, TransportFacility>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub transport_jobs: CollectionDelta<u64, TransportJob>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub trailers: CollectionDelta<u64, TransportTrailer>,
    #[serde(default, skip_serializing_if = "CollectionDelta::is_empty")]
    pub courses: CollectionDelta<u64, RaceCourse>,
}
impl SandboxDelta {
    pub fn between(old: &SandboxView, new: &SandboxView) -> Self {
        Self {
            header: SandboxHeader::from_view(new),
            player: (old.player != new.player).then(|| new.player.clone()),
            factions: diff(&old.factions, &new.factions, |v| v.id),
            plots: diff(&old.plots, &new.plots, |v| v.id),
            structures: diff(&old.structures, &new.structures, |v| v.id),
            resource_nodes: diff(&old.resource_nodes, &new.resource_nodes, |v| v.id.clone()),
            farms: diff(&old.farms, &new.farms, |v| v.id),
            feature_edits: (old.feature_edits != new.feature_edits)
                .then(|| new.feature_edits.clone()),
            units: diff(&old.living.units, &new.living.units, |v| v.id),
            shops: diff(&old.living.shops, &new.living.shops, |v| v.id),
            producers: diff(&old.living.producers, &new.living.producers, |v| v.id),
            deliveries: diff(&old.living.deliveries, &new.living.deliveries, |v| v.id),
            roads: diff(&old.living.roads, &new.living.roads, |v| v.id.clone()),
            animals: diff(&old.ecology.animals, &new.ecology.animals, |v| v.unit_id),
            habitats: diff(&old.ecology.habitats, &new.ecology.habitats, |v| {
                v.id.clone()
            }),
            natural_resources: diff(&old.ecology.resources, &new.ecology.resources, |v| {
                v.node_id.clone()
            }),
            territories: diff(
                &old.settlements.territories,
                &new.settlements.territories,
                |v| v.id,
            ),
            contracts: diff(
                &old.settlements.contracts,
                &new.settlements.contracts,
                |v| v.id,
            ),
            vehicles: diff(&old.transport.vehicles, &new.transport.vehicles, |v| {
                v.unit_id
            }),
            visible_vehicles: diff(
                &old.transport.visible_vehicles,
                &new.transport.visible_vehicles,
                |v| v.unit_id,
            ),
            transport_facilities: diff(&old.transport.facilities, &new.transport.facilities, |v| {
                v.id
            }),
            transport_jobs: diff(&old.transport.jobs, &new.transport.jobs, |v| v.id),
            trailers: diff(&old.transport.trailers, &new.transport.trailers, |v| v.id),
            courses: diff(&old.transport.courses, &new.transport.courses, |v| v.id),
        }
    }
    pub fn apply_to(&self, old: &SandboxView) -> Result<SandboxView> {
        if self.header.transport_profiles.len() > 64
            || self
                .header
                .transport_profiles
                .iter()
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.header.transport_profiles.len()
        {
            return invalid("duplicate or excess public profile identities");
        }
        let mut v = old.clone();
        self.header.install(&mut v);
        if let Some(player) = &self.player {
            v.player = player.clone();
        }
        v.factions = apply(&old.factions, &self.factions, 8, |v| v.id)?;
        v.plots = apply(&old.plots, &self.plots, 64, |v| v.id)?;
        v.structures = apply(&old.structures, &self.structures, 64, |v| v.id)?;
        v.resource_nodes = apply(&old.resource_nodes, &self.resource_nodes, 64, |v| {
            v.id.clone()
        })?;
        v.farms = apply(&old.farms, &self.farms, 64, |v| v.id)?;
        if let Some(edits) = &self.feature_edits {
            v.feature_edits = edits.clone();
        }
        v.living.units = apply(&old.living.units, &self.units, 64, |v| v.id)?;
        v.living.shops = apply(&old.living.shops, &self.shops, 8, |v| v.id)?;
        v.living.producers = apply(&old.living.producers, &self.producers, 16, |v| v.id)?;
        v.living.deliveries = apply(&old.living.deliveries, &self.deliveries, 16, |v| v.id)?;
        v.living.roads = apply(&old.living.roads, &self.roads, 8, |v| v.id.clone())?;
        v.ecology.animals = apply(&old.ecology.animals, &self.animals, 64, |v| v.unit_id)?;
        v.ecology.habitats = apply(&old.ecology.habitats, &self.habitats, 32, |v| v.id.clone())?;
        v.ecology.resources = apply(&old.ecology.resources, &self.natural_resources, 64, |v| {
            v.node_id.clone()
        })?;
        v.settlements.territories =
            apply(&old.settlements.territories, &self.territories, 32, |v| {
                v.id
            })?;
        v.settlements.contracts = apply(&old.settlements.contracts, &self.contracts, 16, |v| v.id)?;
        v.transport.vehicles = apply(&old.transport.vehicles, &self.vehicles, 128, |v| v.unit_id)?;
        v.transport.visible_vehicles = apply(
            &old.transport.visible_vehicles,
            &self.visible_vehicles,
            128,
            |v| v.unit_id,
        )?;
        v.transport.facilities = apply(
            &old.transport.facilities,
            &self.transport_facilities,
            24,
            |v| v.id,
        )?;
        v.transport.jobs = apply(&old.transport.jobs, &self.transport_jobs, 32, |v| v.id)?;
        v.transport.trailers = apply(&old.transport.trailers, &self.trailers, 32, |v| v.id)?;
        v.transport.courses = apply(&old.transport.courses, &self.courses, 8, |v| v.id)?;
        validate_view(&v, old.player.id)?;
        Ok(v)
    }
    fn counts(&self) -> (u64, u64) {
        [
            self.factions.counts(),
            self.plots.counts(),
            self.structures.counts(),
            self.resource_nodes.counts(),
            self.farms.counts(),
            self.units.counts(),
            self.shops.counts(),
            self.producers.counts(),
            self.deliveries.counts(),
            self.roads.counts(),
            self.animals.counts(),
            self.habitats.counts(),
            self.natural_resources.counts(),
            self.territories.counts(),
            self.contracts.counts(),
            self.vehicles.counts(),
            self.visible_vehicles.counts(),
            self.transport_facilities.counts(),
            self.transport_jobs.counts(),
            self.trailers.counts(),
            self.courses.counts(),
        ]
        .into_iter()
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    }
}

fn unique<K: Ord, T>(v: &[T], limit: usize, key: impl Fn(&T) -> K) -> bool {
    v.len() <= limit && v.iter().map(key).collect::<BTreeSet<_>>().len() == v.len()
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:-./".contains(&b))
}
fn validate_view(v: &SandboxView, player: u64) -> Result<()> {
    if v.version != SAVE_VERSION
        || v.player.id != player
        || player == 0
        || !validate_position(v.player.position)
        || !v.tick_seconds.is_finite()
        || !(0.0..=1e12).contains(&v.tick_seconds)
        || v.player.inventory.len() > MAX_INVENTORY_ITEMS
        || v.player
            .inventory
            .iter()
            .any(|(id, n)| !valid_id(id) || *n > MAX_ITEM_COUNT)
        || !unique(&v.factions, 8, |x| x.id)
        || !unique(&v.plots, 64, |x| x.id)
        || !unique(&v.structures, 64, |x| x.id)
        || !unique(&v.resource_nodes, 64, |x| x.id.clone())
        || !unique(&v.farms, 64, |x| x.id)
        || !unique(&v.feature_edits, 64, |x| x.clone())
        || v.feature_edits.iter().any(|id| !valid_id(id))
        || v.plots
            .iter()
            .any(|p| p.id == 0 || !validate_position(p.center))
        || v.structures
            .iter()
            .any(|p| p.id == 0 || !validate_position(p.position))
        || v.farms
            .iter()
            .any(|p| p.id == 0 || !validate_position(p.position))
        || v.resource_nodes
            .iter()
            .any(|p| !valid_id(&p.id) || !validate_position(p.position))
        || v.factions.iter().any(|f| {
            f.id == 0
                || f.name.trim().is_empty()
                || f.name.len() > 128
                || f.name.chars().count() > 64
                || f.members.len() > 128
                || f.invites.len() > 128
                || f.members.get(&f.leader) != Some(&FactionRole::Leader)
                // The authority also publishes the roster of factions that
                // own visible plots. Departing members and nearby outsiders
                // must keep receiving that public property context without
                // gaining membership or another principal's private state.
                || (!f.members.contains_key(&player)
                    && !f.invites.contains(&player)
                    && !v.plots.iter().any(|plot| plot.faction_owner == Some(f.id)))
        })
    {
        return invalid("invalid sandbox replication shape or recipient");
    }
    v.living.validate_for(player)?;
    v.ecology.validate()?;
    v.settlements.validate_for(player)?;
    v.transport.validate_replica(player, &v.living)?;
    if v.ecology.animals.iter().any(|a| {
        !v.living.units.iter().any(|u| {
            u.id == a.unit_id
                && u.kind == terra_core::ActorKind::Animal
                && u.role == UnitRole::Animal
        })
    }) || v.ecology.resources.iter().any(|r| {
        !v.resource_nodes
            .iter()
            .any(|n| n.id == r.node_id && n.item == r.kind.item())
    }) {
        return invalid("incoherent ecology references in replication");
    }
    if serde_json::to_vec(v)?.len() > MAX_SANDBOX_VIEW_BYTES {
        return invalid("reconstructed sandbox exceeds byte budget");
    }
    Ok(())
}
fn validate_actors(
    v: &[ActorState],
    player: u64,
    total: usize,
    truncated: bool,
    time: f64,
) -> Result<()> {
    if !unique(v, 512, |a| a.id)
        || !v.iter().any(|a| a.id == player)
        || total < v.len()
        || truncated != (total > v.len())
        || !time.is_finite()
        || time.abs() > 1e12
        || v.iter().any(|a| {
            a.id == 0
                || !validate_position(a.position)
                || !a.heading_deg.is_finite()
                || a.velocity_enu_m_s.iter().any(|v| !v.is_finite())
        })
    {
        return invalid("invalid actor replication shape");
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct PeerReplication {
    actor_revision: u64,
    actor_tick: u64,
    actors: Option<Vec<ActorState>>,
    sandbox_revision: u64,
    sandbox_tick: u64,
    sandbox: Option<SandboxView>,
    pub pending: ReplicationStats,
    pub last_resync_at: Option<f64>,
}
impl PeerReplication {
    pub fn reset_baselines(&mut self) {
        self.actors = None;
        self.sandbox = None;
    }
    pub fn actors(
        &mut self,
        tick: u64,
        time: f64,
        actors: Vec<ActorState>,
        truncated: bool,
        total: usize,
        keyframe_ticks: u64,
    ) -> Result<ServerMessage> {
        let revision = self
            .actor_revision
            .checked_add(1)
            .ok_or_else(|| NetError::Protocol("replication revision exhausted".into()))?;
        let mut full = ActorUpdate {
            revision,
            base_revision: None,
            tick,
            unix_seconds: time,
            actors: diff(&[], &actors, |a| a.id),
            truncated,
            total_in_interest: total,
        };
        let baseline = encode_frame(&ServerMessage::Snapshot {
            tick,
            unix_seconds: time,
            actors: actors.clone(),
            truncated,
            total_in_interest: total,
        })?
        .len();
        let full_size = encode_frame(&ServerMessage::ActorUpdate {
            update: full.clone(),
        })?
        .len();
        if let Some(previous) = &self.actors {
            if tick.saturating_sub(self.actor_tick) < keyframe_ticks {
                let delta = ActorUpdate {
                    base_revision: Some(self.actor_revision),
                    actors: diff(previous, &actors, |a| a.id),
                    ..full.clone()
                };
                if encode_frame(&ServerMessage::ActorUpdate {
                    update: delta.clone(),
                })?
                .len()
                    < full_size
                {
                    full = delta;
                }
            }
        }
        let message = ServerMessage::ActorUpdate {
            update: full.clone(),
        };
        self.pending.wire_bytes += encode_frame(&message)?.len() as u64;
        self.pending.full_snapshot_bytes += baseline as u64;
        if full.base_revision.is_none() {
            self.pending.keyframes += 1;
            self.actor_tick = tick;
        } else {
            self.pending.deltas += 1;
        }
        let (upserts, removed) = full.actors.counts();
        self.pending.entity_upserts += upserts;
        self.pending.entity_removals += removed;
        self.actor_revision = revision;
        self.actors = Some(actors);
        Ok(message)
    }
    pub fn sandbox(
        &mut self,
        tick: u64,
        view: SandboxView,
        keyframe_ticks: u64,
    ) -> Result<ServerMessage> {
        let revision = self
            .sandbox_revision
            .checked_add(1)
            .ok_or_else(|| NetError::Protocol("replication revision exhausted".into()))?;
        let mut update = SandboxUpdate {
            revision,
            base_revision: None,
            tick,
            full: Some(view.clone()),
            delta: None,
        };
        let baseline = encode_frame(&ServerMessage::SandboxSnapshot {
            tick,
            view: view.clone(),
        })?
        .len();
        let full_size = encode_frame(&ServerMessage::SandboxUpdate {
            update: update.clone(),
        })?
        .len();
        if let Some(previous) = &self.sandbox {
            if tick.saturating_sub(self.sandbox_tick) < keyframe_ticks {
                let delta = SandboxUpdate {
                    revision,
                    base_revision: Some(self.sandbox_revision),
                    tick,
                    full: None,
                    delta: Some(SandboxDelta::between(previous, &view)),
                };
                if encode_frame(&ServerMessage::SandboxUpdate {
                    update: delta.clone(),
                })
                .is_ok_and(|v| v.len() < full_size)
                {
                    update = delta;
                }
            }
        }
        let message = ServerMessage::SandboxUpdate {
            update: update.clone(),
        };
        self.pending.wire_bytes += encode_frame(&message)?.len() as u64;
        self.pending.full_snapshot_bytes += baseline as u64;
        if let Some(delta) = &update.delta {
            self.pending.deltas += 1;
            let (u, r) = delta.counts();
            self.pending.entity_upserts += u;
            self.pending.entity_removals += r;
        } else {
            self.pending.keyframes += 1;
            self.sandbox_tick = tick;
        }
        self.sandbox_revision = revision;
        self.sandbox = Some(view);
        Ok(message)
    }
}

/// Client-side view reconstruction, intentionally independent from rendering.
#[derive(Default)]
pub struct ReplicaState {
    player: Option<u64>,
    actor_revision: u64,
    actors: Vec<ActorState>,
    sandbox_revision: u64,
    sandbox: Option<SandboxView>,
    actor_tick: u64,
    sandbox_tick: u64,
    pub stats: ReplicationStats,
}
#[derive(Debug)]
pub enum ReplicaResult {
    Applied(ServerMessage),
    NeedsKeyframe,
}
impl ReplicaState {
    pub fn bind_player(&mut self, player: u64) -> Result<()> {
        if player == 0 || self.player.is_some() {
            return invalid("invalid or repeated replication identity");
        }
        self.player = Some(player);
        Ok(())
    }
    pub fn actor_revision(&self) -> u64 {
        self.actor_revision
    }
    pub fn sandbox_revision(&self) -> u64 {
        self.sandbox_revision
    }
    pub fn apply_actor(&mut self, update: ActorUpdate) -> Result<ReplicaResult> {
        let player = self
            .player
            .ok_or_else(|| NetError::Protocol("replication before welcome".into()))?;
        if update.revision == 0
            || update.revision <= self.actor_revision
            || update.tick < self.actor_tick
            || update
                .base_revision
                .is_some_and(|v| v.checked_add(1) != Some(update.revision))
        {
            return invalid("stale or invalid actor revision");
        }
        if update
            .base_revision
            .is_some_and(|v| v != self.actor_revision || self.actor_revision == 0)
        {
            return Ok(ReplicaResult::NeedsKeyframe);
        }
        let old = if update.base_revision.is_none() {
            &[][..]
        } else {
            &self.actors[..]
        };
        if update.base_revision.is_none() && !update.actors.removed.is_empty() {
            return invalid("keyframe contains removals");
        }
        let actors = apply(old, &update.actors, 512, |v| v.id)?;
        validate_actors(
            &actors,
            player,
            update.total_in_interest,
            update.truncated,
            update.unix_seconds,
        )?;
        let normalized = ServerMessage::Snapshot {
            tick: update.tick,
            unix_seconds: update.unix_seconds,
            actors: actors.clone(),
            truncated: update.truncated,
            total_in_interest: update.total_in_interest,
        };
        let bytes = encode_frame(&ServerMessage::ActorUpdate {
            update: update.clone(),
        })?
        .len();
        self.stats.wire_bytes += bytes as u64;
        self.stats.full_snapshot_bytes += encode_frame(&normalized)?.len() as u64;
        if update.base_revision.is_none() {
            self.stats.keyframes += 1;
        } else {
            self.stats.deltas += 1;
        }
        let (u, r) = update.actors.counts();
        self.stats.entity_upserts += u;
        self.stats.entity_removals += r;
        self.actor_revision = update.revision;
        self.actor_tick = update.tick;
        self.actors = actors;
        Ok(ReplicaResult::Applied(normalized))
    }
    pub fn apply_sandbox(&mut self, update: SandboxUpdate) -> Result<ReplicaResult> {
        let player = self
            .player
            .ok_or_else(|| NetError::Protocol("replication before welcome".into()))?;
        if update.revision == 0
            || update.revision <= self.sandbox_revision
            || update.tick < self.sandbox_tick
            || update
                .base_revision
                .is_some_and(|v| v.checked_add(1) != Some(update.revision))
        {
            return invalid("stale or invalid sandbox revision");
        }
        let view = match (&update.base_revision, &update.full, &update.delta) {
            (None, Some(full), None) => {
                validate_view(full, player)?;
                full.clone()
            }
            (Some(base), None, Some(delta)) => {
                if *base != self.sandbox_revision || self.sandbox.is_none() {
                    return Ok(ReplicaResult::NeedsKeyframe);
                }
                delta.apply_to(self.sandbox.as_ref().unwrap())?
            }
            _ => return invalid("invalid sandbox keyframe/delta shape"),
        };
        let normalized = ServerMessage::SandboxSnapshot {
            tick: update.tick,
            view: view.clone(),
        };
        self.stats.wire_bytes += encode_frame(&ServerMessage::SandboxUpdate {
            update: update.clone(),
        })?
        .len() as u64;
        self.stats.full_snapshot_bytes += encode_frame(&normalized)?.len() as u64;
        if let Some(delta) = &update.delta {
            self.stats.deltas += 1;
            let (u, r) = delta.counts();
            self.stats.entity_upserts += u;
            self.stats.entity_removals += r;
        } else {
            self.stats.keyframes += 1;
        }
        self.sandbox_revision = update.revision;
        self.sandbox_tick = update.tick;
        self.sandbox = Some(view);
        Ok(ReplicaResult::Applied(normalized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::{ActorKind, LocalFrame};

    fn origin() -> GeoPosition {
        GeoPosition::new(48.137, 11.575, 0.).unwrap()
    }
    fn view() -> SandboxView {
        let mut state = SandboxState::new();
        state.add_player(1, origin()).unwrap();
        state
            .seed_living_demo(origin(), &[], &Catalog::default())
            .unwrap();
        state
            .seed_resource("deposit-a".into(), origin(), "stone", 100, 0.)
            .unwrap();
        state
            .seed_resource("deposit-b".into(), origin(), "wood", 30, 10.)
            .unwrap();
        state.snapshot_for(1, 5000.).unwrap()
    }
    fn actor(id: u64, offset: f64) -> ActorState {
        ActorState {
            id,
            kind: ActorKind::Human,
            position: LocalFrame::new(origin()).to_geo([offset, 0., 0.]).unwrap(),
            velocity_enu_m_s: [0.; 3],
            heading_deg: 0.,
        }
    }
    fn normalized(result: ReplicaResult) -> ServerMessage {
        match result {
            ReplicaResult::Applied(m) => m,
            _ => panic!("keyframe needed"),
        }
    }
    fn decode(message: ServerMessage) -> ServerMessage {
        let bytes = encode_frame(&message).unwrap();
        let mut decoder = crate::FrameDecoder::default();
        decoder.push(&bytes).unwrap();
        decoder.decode().unwrap().unwrap()
    }
    fn install_view(replica: &mut ReplicaState, view: SandboxView, revision: u64, tick: u64) {
        normalized(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision,
                    base_revision: None,
                    tick,
                    full: Some(view),
                    delta: None,
                })
                .unwrap(),
        );
    }

    #[test]
    fn sandbox_delta_roundtrip_preserves_stock_money_order_removals_and_nullable_home() {
        let old = view();
        let mut new = old.clone();
        new.tick_seconds = 3.;
        new.player.home = Some(99);
        new.player.inventory.insert("berries".into(), 4);
        new.living.account.as_mut().unwrap().money -= 120;
        new.living.shops[0].stock.insert("wood".into(), 3);
        new.living.units[0].position = LocalFrame::new(origin()).to_geo([8., 0., 12.]).unwrap();
        new.living.units.rotate_left(3);
        new.resource_nodes.remove(0);
        new.resource_nodes[0].remaining = 7;
        let patch = SandboxDelta::between(&old, &new);
        let decoded: SandboxDelta =
            serde_json::from_slice(&serde_json::to_vec(&patch).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(decoded.apply_to(&old).unwrap()).unwrap(),
            serde_json::to_value(&new).unwrap()
        );
        let mut clear = new.clone();
        clear.player.home = None;
        assert_eq!(
            SandboxDelta::between(&new, &clear)
                .apply_to(&new)
                .unwrap()
                .player
                .home,
            None
        );
    }

    #[test]
    fn faction_departure_keeps_visible_property_context_but_rejects_unrelated_rosters() {
        let mut before = view();
        before.factions.push(Faction {
            id: 91,
            name: "Visible commons".into(),
            leader: 2,
            members: [(1, FactionRole::Member), (2, FactionRole::Leader)]
                .into_iter()
                .collect(),
            invites: Default::default(),
        });
        before.plots.push(LandPlot {
            id: 92,
            owner: 1,
            faction_owner: Some(91),
            center: origin(),
            radius_m: 10.,
        });
        let mut outsider = before.clone();
        outsider.factions[0].members.remove(&1);
        let absent = view();
        let gained = SandboxDelta::between(&absent, &outsider)
            .apply_to(&absent)
            .unwrap();
        assert_eq!(gained.factions, outsider.factions);
        assert_eq!(gained.plots, outsider.plots);
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        install_view(&mut replica, before.clone(), 1, 1);
        let mut departed = before.clone();
        departed.factions[0].members.remove(&1);
        let ServerMessage::SandboxSnapshot { view: actual, .. } = normalized(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: Some(1),
                    tick: 2,
                    full: None,
                    delta: Some(SandboxDelta::between(&before, &departed)),
                })
                .unwrap(),
        ) else {
            panic!("expected reconstructed private view")
        };
        assert_eq!(actual.player, before.player);
        assert_eq!(actual.factions, departed.factions);
        assert_eq!(actual.plots, before.plots);
        let mut unrelated = departed.clone();
        unrelated.plots.clear();
        assert!(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 3,
                    base_revision: Some(2),
                    tick: 3,
                    full: None,
                    delta: Some(SandboxDelta::between(&departed, &unrelated)),
                })
                .is_err()
        );
        assert_eq!(replica.sandbox_revision(), 2);
        // Leaving interest removes the public plot and its now-irrelevant
        // roster together. A previous rejected patch cannot poison this base.
        unrelated.factions.clear();
        let ServerMessage::SandboxSnapshot { view: actual, .. } = normalized(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 3,
                    base_revision: Some(2),
                    tick: 3,
                    full: None,
                    delta: Some(SandboxDelta::between(&departed, &unrelated)),
                })
                .unwrap(),
        ) else {
            panic!("expected reconstructed private view")
        };
        assert!(actual.factions.is_empty() && actual.plots.is_empty());
        assert_eq!(actual.player, before.player);
    }

    #[test]
    fn actor_deltas_remove_departed_interest_and_keep_zero_change_heartbeats() {
        let mut peer = PeerReplication::default();
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        let actors: Vec<_> = (1..30).map(|id| actor(id, id as f64)).collect();
        let ServerMessage::ActorUpdate { update } = decode(
            peer.actors(1, 100., actors.clone(), false, actors.len(), 200)
                .unwrap(),
        ) else {
            panic!()
        };
        assert!(update.base_revision.is_none());
        normalized(replica.apply_actor(update).unwrap());
        let next: Vec<_> = actors.iter().filter(|a| a.id != 10).cloned().collect();
        let ServerMessage::ActorUpdate { update } = decode(
            peer.actors(2, 101., next.clone(), false, next.len(), 200)
                .unwrap(),
        ) else {
            panic!()
        };
        assert_eq!(update.base_revision, Some(1));
        assert_eq!(update.actors.removed, vec![10]);
        let ServerMessage::Snapshot { actors: actual, .. } =
            normalized(replica.apply_actor(update).unwrap())
        else {
            panic!()
        };
        assert_eq!(actual, next);
        let ServerMessage::ActorUpdate { update } = peer
            .actors(3, 102., next.clone(), false, next.len(), 200)
            .unwrap()
        else {
            panic!()
        };
        assert!(update.actors.is_empty());
        let ServerMessage::Snapshot { tick, actors, .. } =
            normalized(replica.apply_actor(update).unwrap())
        else {
            panic!()
        };
        assert_eq!(tick, 3);
        assert_eq!(actors, next);
    }

    #[test]
    fn missing_base_requests_keyframe_without_advancing_replica() {
        let mut peer = PeerReplication::default();
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        let actors = vec![actor(1, 0.), actor(2, 2.), actor(3, 3.)];
        let ServerMessage::ActorUpdate { update } =
            peer.actors(1, 100., actors.clone(), false, 3, 200).unwrap()
        else {
            panic!()
        };
        normalized(replica.apply_actor(update).unwrap());
        peer.actors(2, 101., actors.clone(), false, 3, 200).unwrap(); // deliberately lost baseline revision
        let ServerMessage::ActorUpdate { update } =
            peer.actors(3, 102., actors.clone(), false, 3, 200).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            replica.apply_actor(update).unwrap(),
            ReplicaResult::NeedsKeyframe
        ));
        assert_eq!(replica.actor_revision(), 1);
        peer.reset_baselines();
        let ServerMessage::ActorUpdate { update } =
            peer.actors(4, 103., actors, false, 3, 200).unwrap()
        else {
            panic!()
        };
        normalized(replica.apply_actor(update).unwrap());
        assert_eq!(replica.actor_revision(), 4);
    }

    #[test]
    fn malformed_delta_rejects_atomically_and_cannot_reassign_private_recipient() {
        let old = view();
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        install_view(&mut replica, old.clone(), 1, 1);
        let mut next = old.clone();
        next.player.inventory.insert("wood".into(), 12);
        let good = SandboxDelta::between(&old, &next);
        let mut wrong_owner = good.clone();
        wrong_owner.player.as_mut().unwrap().id = 2;
        assert!(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: Some(1),
                    tick: 2,
                    full: None,
                    delta: Some(wrong_owner)
                })
                .is_err()
        );
        assert_eq!(replica.sandbox_revision(), 1);
        let mut duplicate = good.clone();
        duplicate.units.order = Some(vec![old.living.units[0].id; old.living.units.len()]);
        assert!(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: Some(1),
                    tick: 2,
                    full: None,
                    delta: Some(duplicate)
                })
                .is_err()
        );
        assert_eq!(replica.sandbox_revision(), 1);
        let ServerMessage::SandboxSnapshot { view, .. } = normalized(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: Some(1),
                    tick: 2,
                    full: None,
                    delta: Some(good),
                })
                .unwrap(),
        ) else {
            panic!()
        };
        assert_eq!(view.player.inventory["wood"], 12);
    }

    #[test]
    fn delta_cannot_grow_collections_past_limits_or_mix_full_and_patch() {
        let old = view();
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        install_view(&mut replica, old.clone(), 1, 1);
        let mut patch = SandboxDelta::between(&old, &old);
        let template = old.resource_nodes[0].clone();
        patch.resource_nodes.upserts = (0..65)
            .map(|id| ResourceNode {
                id: format!("overflow-{id}"),
                ..template.clone()
            })
            .collect();
        patch.resource_nodes.order = Some(
            patch
                .resource_nodes
                .upserts
                .iter()
                .map(|n| n.id.clone())
                .collect(),
        );
        assert!(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: Some(1),
                    tick: 2,
                    full: None,
                    delta: Some(patch)
                })
                .is_err()
        );
        assert!(
            replica
                .apply_sandbox(SandboxUpdate {
                    revision: 2,
                    base_revision: None,
                    tick: 2,
                    full: Some(old.clone()),
                    delta: Some(SandboxDelta::between(&old, &old))
                })
                .is_err()
        );
        assert_eq!(replica.sandbox_revision(), 1);
    }

    #[test]
    fn selection_uses_keyframes_when_delta_is_larger_or_refresh_deadline_is_reached() {
        let mut peer = PeerReplication::default();
        let old: Vec<_> = (1..8).map(|id| actor(id, id as f64)).collect();
        peer.actors(1, 100., old.clone(), false, old.len(), 200)
            .unwrap();
        let different = vec![actor(1, 0.), actor(80, 0.)];
        let ServerMessage::ActorUpdate { update } = peer
            .actors(2, 101., different.clone(), false, 2, 200)
            .unwrap()
        else {
            panic!()
        };
        assert!(update.base_revision.is_none());
        let ServerMessage::ActorUpdate { update } =
            peer.actors(202, 102., different, false, 2, 200).unwrap()
        else {
            panic!()
        };
        assert!(update.base_revision.is_none());
    }

    #[test]
    fn unchanged_view_delta_reduces_bytes_and_preserves_private_progress() {
        let mut peer = PeerReplication::default();
        let original = view();
        peer.sandbox(1, original.clone(), 200).unwrap();
        let mut changed = original.clone();
        changed.tick_seconds = 0.1;
        let ServerMessage::SandboxUpdate { update } =
            peer.sandbox(2, changed.clone(), 200).unwrap()
        else {
            panic!()
        };
        assert!(update.delta.is_some());
        let full = encode_frame(&ServerMessage::SandboxSnapshot {
            tick: 2,
            view: changed.clone(),
        })
        .unwrap();
        let delta = encode_frame(&ServerMessage::SandboxUpdate {
            update: update.clone(),
        })
        .unwrap();
        assert!(
            delta.len() * 4 < full.len(),
            "{} delta vs {} full",
            delta.len(),
            full.len()
        );
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        install_view(&mut replica, original, 1, 1);
        let ServerMessage::SandboxSnapshot { view: actual, .. } =
            normalized(replica.apply_sandbox(update).unwrap())
        else {
            panic!()
        };
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(changed).unwrap()
        );
    }
    #[test]
    fn individually_small_patches_cannot_accumulate_an_oversized_baseline() {
        let mut current = view();
        let mut replica = ReplicaState::default();
        replica.bind_player(1).unwrap();
        install_view(&mut replica, current.clone(), 1, 1);
        let large_inventory: Inventory = (0..64)
            .map(|i| (format!("{i:03}{}", "a".repeat(125)), 1))
            .collect();
        let mut rejected = false;
        for index in 0..16 {
            let mut next = current.clone();
            if index >= next.living.producers.len() {
                let mut site = next.living.producers[0].clone();
                site.id = 9_900_000 + index as u64;
                next.living.producers.push(site);
            }
            let site = &mut next.living.producers[index];
            site.stock = large_inventory.clone();
            site.inputs = large_inventory.clone();
            site.outputs = large_inventory.clone();
            let revision = replica.sandbox_revision() + 1;
            let update = SandboxUpdate {
                revision,
                base_revision: Some(revision - 1),
                tick: revision,
                full: None,
                delta: Some(SandboxDelta::between(&current, &next)),
            };
            assert!(
                encode_frame(&ServerMessage::SandboxUpdate {
                    update: update.clone()
                })
                .unwrap()
                .len()
                    < crate::MAX_FRAME_BYTES
            );
            match replica.apply_sandbox(update) {
                Ok(ReplicaResult::Applied(ServerMessage::SandboxSnapshot { view, .. })) => {
                    current = view
                }
                Err(error) => {
                    assert!(error.to_string().contains("byte budget"), "{error}");
                    assert_eq!(replica.sandbox_revision(), revision - 1);
                    rejected = true;
                    break;
                }
                other => panic!("unexpected result {other:?}"),
            }
        }
        assert!(
            rejected,
            "cumulative result must hit the complete-view byte ceiling before collection limits"
        );
    }
}
