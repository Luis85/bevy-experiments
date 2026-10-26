use crate::catalog::{inventory, validate_inventory};
use crate::{
    Catalog, Inventory, MAX_ENTITIES, MAX_FACTIONS, MAX_PLAYER_FACTIONS, MAX_PLAYERS, SAVE_VERSION,
    SandboxError, finite_range, valid_id,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{FeatureKind, GeoPosition, WeatherSample, WorldChunk};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: u64,
    pub position: GeoPosition,
    pub inventory: Inventory,
    pub home: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionRole {
    Leader,
    Builder,
    Member,
}

/// Player-created gameplay organization, separate from real political states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Faction {
    pub id: u64,
    pub name: String,
    pub leader: u64,
    // Internally tagged serde envelopes buffer their nested values. A JSON
    // object's numeric keys then reach this field as strings, without the
    // ordinary JSON map-key adapter that deserializes u64 automatically.
    #[serde(deserialize_with = "deserialize_faction_members")]
    pub members: BTreeMap<u64, FactionRole>,
    pub invites: BTreeSet<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandPlot {
    pub id: u64,
    pub owner: u64,
    /// None: personal owner. Some: current permissions follow the faction;
    /// `owner` remains the original founder for audit, without a bypass.
    #[serde(default)]
    pub faction_owner: Option<u64>,
    pub center: GeoPosition,
    pub radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub id: u64,
    pub owner: u64,
    #[serde(default)]
    pub faction_owner: Option<u64>,
    pub blueprint: String,
    pub position: GeoPosition,
    pub hitpoints: u32,
    pub max_hitpoints: u32,
    pub radius_m: f64,
    pub housing: bool,
    pub workbench: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceNode {
    pub id: String,
    pub position: GeoPosition,
    pub item: String,
    pub remaining: u32,
    pub capacity: u32,
    /// Seconds per regenerated unit; zero means finite/non-regrowing resource.
    pub regrow_seconds: f64,
    pub regrow_accumulator: f64,
    pub source_feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FarmPlot {
    pub id: u64,
    pub owner: u64,
    #[serde(default)]
    pub faction_owner: Option<u64>,
    pub plot_id: u64,
    pub position: GeoPosition,
    pub crop: String,
    pub growth_seconds: f64,
    pub required_growth_seconds: f64,
    pub moisture: f32,
    pub harvest_item: String,
    pub harvest_amount: u32,
}

impl FarmPlot {
    pub fn progress(&self) -> f32 {
        (self.growth_seconds / self.required_growth_seconds).clamp(0.0, 1.0) as f32
    }
    pub fn ready(&self) -> bool {
        self.growth_seconds >= self.required_growth_seconds
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureReference {
    pub position: GeoPosition,
    pub radius_m: f64,
    pub kind: FeatureKind,
}

/// Mutable overlay only. Original sourced chunks remain immutable and can be
/// refreshed independently. Keys in `feature_edits` are persistent tombstones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxState {
    #[serde(default)]
    pub living: crate::LivingWorld,
    pub version: u32,
    pub tick_seconds: f64,
    pub next_id: u64,
    pub players: BTreeMap<u64, PlayerState>,
    #[serde(default)]
    pub factions: BTreeMap<u64, Faction>,
    pub plots: BTreeMap<u64, LandPlot>,
    pub structures: BTreeMap<u64, Structure>,
    pub resource_nodes: BTreeMap<String, ResourceNode>,
    pub farms: BTreeMap<u64, FarmPlot>,
    pub world_features: BTreeMap<String, FeatureReference>,
    pub feature_edits: BTreeSet<String>,
}

impl Default for SandboxState {
    fn default() -> Self {
        Self {
            version: SAVE_VERSION,
            living: crate::LivingWorld::default(),
            tick_seconds: 0.0,
            next_id: 1,
            players: BTreeMap::new(),
            factions: BTreeMap::new(),
            plots: BTreeMap::new(),
            structures: BTreeMap::new(),
            resource_nodes: BTreeMap::new(),
            farms: BTreeMap::new(),
            world_features: BTreeMap::new(),
            feature_edits: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxView {
    #[serde(default)]
    pub living: crate::LivingView,
    pub version: u32,
    pub tick_seconds: f64,
    pub player: PlayerState,
    /// Full bounded rosters: at most eight factions per network snapshot.
    #[serde(default)]
    pub factions: Vec<Faction>,
    pub plots: Vec<LandPlot>,
    pub structures: Vec<Structure>,
    pub resource_nodes: Vec<ResourceNode>,
    pub farms: Vec<FarmPlot>,
    pub feature_edits: Vec<String>,
    pub truncated: bool,
}

impl SandboxState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_player(&mut self, id: u64, position: GeoPosition) -> Result<(), SandboxError> {
        self.register_player(
            id,
            position,
            inventory(&[("wood", 30), ("stone", 20), ("wheat_seed", 8)]),
        )
    }

    /// Register a player using a game's own catalog and starting equipment.
    /// The convenience `add_player` uses the built-in demo's starter items.
    /// Registration is transactional: unknown items or invalid quantities leave
    /// the state unchanged. Inventory is supplied by trusted game/server policy.
    pub fn add_player_with_inventory(
        &mut self,
        id: u64,
        position: GeoPosition,
        inventory: Inventory,
        catalog: &Catalog,
    ) -> Result<(), SandboxError> {
        catalog.validate()?;
        validate_inventory(&inventory)?;
        if inventory.keys().any(|item| !catalog.items.contains(item)) {
            return Err(SandboxError::Invalid(
                "starting inventory catalog reference".into(),
            ));
        }
        self.register_player(id, position, inventory)
    }

    fn register_player(
        &mut self,
        id: u64,
        position: GeoPosition,
        inventory: Inventory,
    ) -> Result<(), SandboxError> {
        position.validate()?;
        if self.players.contains_key(&id) {
            return Err(SandboxError::Invalid("player already registered".into()));
        }
        if id == 0 || self.players.len() >= MAX_PLAYERS {
            return Err(SandboxError::Capacity);
        }
        self.players.insert(
            id,
            PlayerState {
                id,
                position,
                inventory,
                home: None,
            },
        );
        Ok(())
    }

    /// Only a trusted simulation/server calls this after movement validation.
    /// Player commands cannot change their own authoritative position.
    pub fn set_player_position(
        &mut self,
        id: u64,
        position: GeoPosition,
    ) -> Result<(), SandboxError> {
        position.validate()?;
        self.players
            .get_mut(&id)
            .ok_or(SandboxError::PlayerMissing)?
            .position = position;
        if let Some(account) = self.living.accounts.get(&id)
            && let Some(unit) = self.living.units.get_mut(&account.active_unit)
        {
            unit.position = position;
        }
        Ok(())
    }

    /// Register bounded metadata without changing source chunks. Repeated chunk
    /// loads do not refill gathered resources or resurrect removed features.
    pub fn register_chunk(&mut self, chunk: &WorldChunk) {
        for feature in &chunk.features {
            if !valid_id(&feature.id) || feature.validate().is_err() {
                continue;
            }
            let Some(&position) = feature.points.first() else {
                continue;
            };
            if !self.world_features.contains_key(&feature.id)
                && self.world_features.len() < MAX_ENTITIES
            {
                let radius_m = feature
                    .points
                    .iter()
                    .map(|p| position.surface_distance_m(*p))
                    .fold(0.0_f64, f64::max);
                self.world_features.insert(
                    feature.id.clone(),
                    FeatureReference {
                        position,
                        radius_m,
                        kind: feature.kind,
                    },
                );
            }
            if feature.kind == FeatureKind::Tree
                && self.world_features.contains_key(&feature.id)
                && !self.feature_edits.contains(&feature.id)
                && !self.resource_nodes.contains_key(&feature.id)
                && self.resource_nodes.len() < MAX_ENTITIES
            {
                self.resource_nodes.insert(
                    feature.id.clone(),
                    ResourceNode {
                        id: feature.id.clone(),
                        position,
                        item: "wood".into(),
                        remaining: 20,
                        capacity: 20,
                        regrow_seconds: 30.0,
                        regrow_accumulator: 0.0,
                        source_feature: Some(feature.id.clone()),
                    },
                );
            }
        }
    }

    /// Seed authored gameplay resources (e.g. quarry deposits) separately from
    /// geographic truth. Callers label these as procedural gameplay content.
    pub fn seed_resource(
        &mut self,
        id: String,
        position: GeoPosition,
        item: &str,
        amount: u32,
        regrow_seconds: f64,
    ) -> Result<(), SandboxError> {
        position.validate()?;
        if !valid_id(&id)
            || !valid_id(item)
            || amount == 0
            || amount > 10_000
            || !finite_range(regrow_seconds, 0.0, 31_536_000.0)
        {
            return Err(SandboxError::Invalid("resource node".into()));
        }
        if self.resource_nodes.contains_key(&id) {
            return Err(SandboxError::Invalid("resource already exists".into()));
        }
        if self.resource_nodes.len() >= MAX_ENTITIES {
            return Err(SandboxError::Capacity);
        }
        self.resource_nodes.insert(
            id.clone(),
            ResourceNode {
                id,
                position,
                item: item.into(),
                remaining: amount,
                capacity: amount,
                regrow_seconds,
                regrow_accumulator: 0.0,
                source_feature: None,
            },
        );
        Ok(())
    }

    /// Bounded O(nodes + farms) advancement with no per-elapsed-tick loops.
    /// Weather is sampled by the hosting simulation. Growth is illustrative;
    /// wetness and temperature modify speed, without claiming agronomic accuracy.
    pub fn advance(&mut self, dt: f64, weather: &WeatherSample) -> Result<(), SandboxError> {
        if !finite_range(dt, 0.0, 86_400.0) || !finite_range(self.tick_seconds + dt, 0.0, 1e12) {
            return Err(SandboxError::Invalid("simulation delta".into()));
        }
        weather.validate()?;
        self.tick_seconds += dt;
        for node in self.resource_nodes.values_mut() {
            if node.regrow_seconds <= 0.0 || node.remaining == node.capacity {
                node.regrow_accumulator = 0.0;
                continue;
            }
            let accumulated = node.regrow_accumulator + dt;
            let added = (accumulated / node.regrow_seconds)
                .floor()
                .min((node.capacity - node.remaining) as f64) as u32;
            node.remaining += added;
            node.regrow_accumulator = if node.remaining == node.capacity {
                0.0
            } else {
                accumulated.rem_euclid(node.regrow_seconds)
            };
        }
        let temperature =
            (1.0 - ((weather.temperature_c as f64 - 20.0) / 35.0).abs()).clamp(0.0, 1.0);
        for farm in self.farms.values_mut() {
            if farm.ready() {
                continue;
            }
            let start_moisture = farm.moisture as f64;
            let moisture_rate = weather.precipitation_mm_h as f64 / 3_600.0 * 0.15 - 0.000_03;
            farm.moisture = (start_moisture + dt * moisture_rate).clamp(0.0, 1.0) as f32;
            // Average endpoints makes a fixed step deterministic and avoids a
            // one-large-tick penalty for ordinary linear drying intervals.
            let moisture = (start_moisture + farm.moisture as f64) * 0.5;
            let rate = temperature * (0.2 + 0.8 * moisture);
            farm.growth_seconds =
                (farm.growth_seconds + dt * rate).min(farm.required_growth_seconds);
        }
        Ok(())
    }

    pub fn snapshot_for(&self, player_id: u64, radius_m: f64) -> Result<SandboxView, SandboxError> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(SandboxError::PlayerMissing)?
            .clone();
        if !radius_m.is_finite() || radius_m < 0.0 {
            return Err(SandboxError::Invalid("interest radius".into()));
        }
        let radius = radius_m.min(5_000.0);
        let near = |point| player.position.surface_distance_m(point) <= radius;
        let mut truncated = false;
        let plots = take_bounded(
            self.plots.values().filter(|p| near(p.center)),
            &mut truncated,
        );
        let structures = take_bounded(
            self.structures.values().filter(|s| near(s.position)),
            &mut truncated,
        );
        let resource_nodes = take_bounded(
            self.resource_nodes.values().filter(|n| near(n.position)),
            &mut truncated,
        );
        let farms = take_bounded(
            self.farms.values().filter(|f| near(f.position)),
            &mut truncated,
        );
        let feature_edits = take_bounded(
            self.feature_edits.iter().filter(|id| {
                self.world_features
                    .get(*id)
                    .is_some_and(|f| near(f.position))
            }),
            &mut truncated,
        );
        // Prioritize the player's own memberships/invitations over nearby
        // ownership. Eight full rosters keep the snapshot under network bounds.
        let mut relevant_factions: Vec<&Faction> = self
            .factions
            .values()
            .filter(|f| f.members.contains_key(&player_id) || f.invites.contains(&player_id))
            .collect();
        let own_ids: BTreeSet<_> = relevant_factions.iter().map(|f| f.id).collect();
        let nearby_ids: BTreeSet<_> = plots.iter().filter_map(|p| p.faction_owner).collect();
        relevant_factions.extend(
            self.factions
                .values()
                .filter(|f| nearby_ids.contains(&f.id) && !own_ids.contains(&f.id)),
        );
        if relevant_factions.len() > 8 {
            truncated = true;
        }
        let factions = relevant_factions.into_iter().take(8).cloned().collect();
        let mut view = SandboxView {
            version: self.version,
            living: self.living.snapshot(player_id, player.position, radius),
            tick_seconds: self.tick_seconds,
            player,
            factions,
            plots,
            structures,
            resource_nodes,
            farms,
            feature_edits,
            truncated,
        };
        view.enforce_wire_budget()?;
        Ok(view)
    }

    pub fn validate_with_catalog(&self, catalog: &Catalog) -> Result<(), SandboxError> {
        self.validate()?;
        catalog.validate()?;
        self.living.validate(&self.players, Some(catalog))?;
        if self
            .players
            .values()
            .any(|p| p.inventory.keys().any(|id| !catalog.items.contains(id)))
            || self
                .structures
                .values()
                .any(|s| !catalog.blueprints.contains_key(&s.blueprint))
            || self
                .farms
                .values()
                .any(|f| !catalog.crops.contains_key(&f.crop))
            || self
                .resource_nodes
                .values()
                .any(|n| !catalog.items.contains(&n.item))
        {
            return Err(SandboxError::Invalid("catalog reference".into()));
        }
        // Saved assets cache their creation-time parameters so clients can
        // render snapshots without the catalog. These fields must still agree
        // with the authoritative catalog; silently accepting stale balance or
        // edited yields would change the rules after a save/load round trip.
        for structure in self.structures.values() {
            let definition = &catalog.blueprints[&structure.blueprint];
            if structure.max_hitpoints != definition.hitpoints
                || structure.radius_m != definition.radius_m
                || structure.housing != definition.housing
                || structure.workbench != definition.workbench
            {
                return Err(SandboxError::Invalid(
                    "saved blueprint definition differs from catalog; migrate this save explicitly"
                        .into(),
                ));
            }
        }
        for farm in self.farms.values() {
            let definition = &catalog.crops[&farm.crop];
            if farm.required_growth_seconds != definition.growth_seconds
                || farm.harvest_item != definition.harvest_item
                || farm.harvest_amount != definition.harvest_amount
            {
                return Err(SandboxError::Invalid(
                    "saved crop definition differs from catalog; migrate this save explicitly"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), SandboxError> {
        self.living.validate(&self.players, None)?;
        if self.version != SAVE_VERSION {
            return Err(SandboxError::SaveVersion(self.version));
        }
        if !finite_range(self.tick_seconds, 0.0, 1e12)
            || self.next_id == 0
            || self.next_id == u64::MAX
        {
            return Err(SandboxError::Invalid("state clock or id".into()));
        }
        if self.players.len() > MAX_PLAYERS
            || self.factions.len() > MAX_FACTIONS
            || [
                self.plots.len(),
                self.structures.len(),
                self.resource_nodes.len(),
                self.farms.len(),
                self.world_features.len(),
                self.feature_edits.len(),
            ]
            .iter()
            .any(|count| *count > MAX_ENTITIES)
        {
            return Err(SandboxError::Capacity);
        }
        let mut ids = BTreeSet::new();
        for (id, faction) in &self.factions {
            if *id == 0
                || *id != faction.id
                || *id >= self.next_id
                || !ids.insert(*id)
                || !valid_faction_name(&faction.name)
                || faction.members.len() > MAX_PLAYERS
                || faction.invites.len() > MAX_PLAYERS
                || faction.members.get(&faction.leader) != Some(&FactionRole::Leader)
                || faction
                    .members
                    .values()
                    .filter(|role| **role == FactionRole::Leader)
                    .count()
                    != 1
                || faction
                    .members
                    .keys()
                    .any(|id| !self.players.contains_key(id))
                || faction
                    .invites
                    .iter()
                    .any(|id| !self.players.contains_key(id) || faction.members.contains_key(id))
            {
                return Err(SandboxError::Invalid("faction roster or leadership".into()));
            }
        }
        for (id, player) in &self.players {
            if *id == 0 || *id != player.id {
                return Err(SandboxError::Invalid("player id".into()));
            }
            player.position.validate()?;
            validate_inventory(&player.inventory)?;
            if self.affiliation_count(*id) > MAX_PLAYER_FACTIONS {
                return Err(SandboxError::Capacity);
            }
            if player.home.is_some_and(|id| {
                !self
                    .structures
                    .get(&id)
                    .is_some_and(|s| self.may_use(player.id, s.faction_owner, s.owner) && s.housing)
            }) {
                return Err(SandboxError::Invalid("home reference".into()));
            }
        }
        for (id, plot) in &self.plots {
            if *id == 0
                || *id != plot.id
                || *id >= self.next_id
                || !ids.insert(*id)
                || !self.players.contains_key(&plot.owner)
                || plot
                    .faction_owner
                    .is_some_and(|id| !self.factions.contains_key(&id))
                || !finite_range(plot.radius_m, 5.0, 100.0)
            {
                return Err(SandboxError::Invalid("land plot".into()));
            }
            plot.center.validate()?;
        }
        for (id, structure) in &self.structures {
            if *id == 0
                || *id != structure.id
                || *id >= self.next_id
                || !ids.insert(*id)
                || !self.players.contains_key(&structure.owner)
                || !valid_id(&structure.blueprint)
                || structure.hitpoints == 0
                || structure.hitpoints > structure.max_hitpoints
                || structure.max_hitpoints > 1_000_000
                || !finite_range(structure.radius_m, 0.1, 50.0)
            {
                return Err(SandboxError::Invalid("structure".into()));
            }
            structure.position.validate()?;
            if !self.plots.values().any(|plot| {
                plot.faction_owner == structure.faction_owner
                    && (plot.faction_owner.is_some() || plot.owner == structure.owner)
                    && plot.center.surface_distance_m(structure.position) + structure.radius_m
                        <= plot.radius_m
            }) {
                return Err(SandboxError::NotOwner);
            }
        }
        for (id, node) in &self.resource_nodes {
            if id != &node.id
                || !valid_id(id)
                || !valid_id(&node.item)
                || node.capacity == 0
                || node.capacity > 10_000
                || node.remaining > node.capacity
                || !finite_range(node.regrow_seconds, 0.0, 31_536_000.0)
                || !node.regrow_accumulator.is_finite()
                || node.regrow_accumulator < 0.0
                || (node.regrow_seconds == 0.0 && node.regrow_accumulator != 0.0)
                || (node.regrow_seconds > 0.0 && node.regrow_accumulator >= node.regrow_seconds)
                || node.source_feature.as_ref().is_some_and(|id| {
                    !self.world_features.contains_key(id) || self.feature_edits.contains(id)
                })
            {
                return Err(SandboxError::Invalid("resource node".into()));
            }
            node.position.validate()?;
        }
        for (id, farm) in &self.farms {
            if *id == 0
                || *id != farm.id
                || *id >= self.next_id
                || !ids.insert(*id)
                || !self.players.contains_key(&farm.owner)
                || !valid_id(&farm.crop)
                || !valid_id(&farm.harvest_item)
                || !finite_range(farm.growth_seconds, 0.0, farm.required_growth_seconds)
                || !finite_range(farm.required_growth_seconds, 1.0, 31_536_000.0)
                || !finite_range(farm.moisture as f64, 0.0, 1.0)
                || farm.harvest_amount == 0
                || farm.harvest_amount > 10_000
                || !self.plots.get(&farm.plot_id).is_some_and(|p| {
                    p.faction_owner == farm.faction_owner
                        && (p.faction_owner.is_some() || p.owner == farm.owner)
                        && p.center.surface_distance_m(farm.position) + 0.5 <= p.radius_m
                })
            {
                return Err(SandboxError::Invalid("farm".into()));
            }
            farm.position.validate()?;
        }
        for (id, feature) in &self.world_features {
            if !valid_id(id) || !finite_range(feature.radius_m, 0.0, 40_100_000.0) {
                return Err(SandboxError::Invalid("source feature".into()));
            }
            feature.position.validate()?;
        }
        if self.feature_edits.iter().any(|id| {
            !self
                .world_features
                .get(id)
                .is_some_and(|feature| feature.kind == FeatureKind::Tree)
        }) {
            return Err(SandboxError::Invalid("feature tombstone".into()));
        }
        Ok(())
    }

    pub(crate) fn owns_area(&self, player: u64, position: GeoPosition, radius: f64) -> bool {
        self.plot_for_area(player, position, radius).is_some()
    }

    pub(crate) fn plot_for_area(
        &self,
        player: u64,
        position: GeoPosition,
        radius: f64,
    ) -> Option<&LandPlot> {
        self.plots.values().find(|plot| {
            self.may_build(player, plot.faction_owner, plot.owner)
                && plot.center.surface_distance_m(position) + radius <= plot.radius_m
        })
    }

    pub fn is_faction_member(&self, player: u64, faction_id: u64) -> bool {
        self.factions
            .get(&faction_id)
            .is_some_and(|f| f.members.contains_key(&player))
    }

    pub fn may_build(&self, player: u64, faction_owner: Option<u64>, owner: u64) -> bool {
        match faction_owner {
            None => player == owner,
            Some(id) => self
                .factions
                .get(&id)
                .and_then(|f| f.members.get(&player))
                .is_some_and(|role| matches!(role, FactionRole::Leader | FactionRole::Builder)),
        }
    }

    pub fn may_interact(&self, player: u64, faction_owner: Option<u64>, owner: u64) -> bool {
        self.may_build(player, faction_owner, owner)
    }

    /// Members may use shared housing/workbenches without altering property.
    pub fn may_use(&self, player: u64, faction_owner: Option<u64>, owner: u64) -> bool {
        faction_owner.map_or(player == owner, |id| self.is_faction_member(player, id))
    }

    pub(crate) fn membership_count(&self, player: u64) -> usize {
        self.factions
            .values()
            .filter(|f| f.members.contains_key(&player))
            .count()
    }

    pub(crate) fn affiliation_count(&self, player: u64) -> usize {
        self.factions
            .values()
            .filter(|f| f.members.contains_key(&player) || f.invites.contains(&player))
            .count()
    }

    pub(crate) fn fresh_id(&self) -> Result<u64, SandboxError> {
        if self.next_id == 0 || self.next_id >= u64::MAX - 1 {
            Err(SandboxError::Capacity)
        } else {
            Ok(self.next_id)
        }
    }
}

pub(crate) fn valid_faction_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.len() <= 128
        && name.chars().count() <= 64
        && name.chars().all(|c| !c.is_control())
}

fn take_bounded<'a, T: Clone + 'a>(
    values: impl Iterator<Item = &'a T>,
    truncated: &mut bool,
) -> Vec<T> {
    let values: Vec<T> = values.take(65).cloned().collect();
    if values.len() > 64 {
        *truncated = true;
    }
    values.into_iter().take(64).collect()
}

fn deserialize_faction_members<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<u64, FactionRole>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, MapAccess, Visitor};
    struct MemberKey(u64);
    impl<'de> Deserialize<'de> for MemberKey {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct KeyVisitor;
            impl Visitor<'_> for KeyVisitor {
                type Value = MemberKey;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a positive player ID as an integer or decimal map key")
                }
                fn visit_u64<E: Error>(self, value: u64) -> Result<Self::Value, E> {
                    if value == 0 {
                        Err(E::custom("faction player IDs must be positive"))
                    } else {
                        Ok(MemberKey(value))
                    }
                }
                fn visit_i64<E: Error>(self, value: i64) -> Result<Self::Value, E> {
                    self.visit_u64(u64::try_from(value).map_err(E::custom)?)
                }
                fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                    if value.is_empty()
                        || value.len() > 20
                        || !value.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(E::custom("invalid faction player ID"));
                    }
                    self.visit_u64(value.parse::<u64>().map_err(E::custom)?)
                }
            }
            deserializer.deserialize_any(KeyVisitor)
        }
    }
    struct MembersVisitor;
    impl<'de> Visitor<'de> for MembersVisitor {
        type Value = BTreeMap<u64, FactionRole>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded faction player-to-role map")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            if map.size_hint().is_some_and(|count| count > MAX_PLAYERS) {
                return Err(M::Error::custom("faction roster exceeds player limit"));
            }
            let mut members = BTreeMap::new();
            while let Some((MemberKey(id), role)) = map.next_entry::<MemberKey, FactionRole>()? {
                if members.len() >= MAX_PLAYERS || members.insert(id, role).is_some() {
                    return Err(M::Error::custom(
                        "faction roster has duplicate IDs or exceeds player limit",
                    ));
                }
            }
            Ok(members)
        }
    }
    deserializer.deserialize_map(MembersVisitor)
}

/// Reserve space for the transport envelope beneath the 256 KiB frame limit.
pub const MAX_SANDBOX_VIEW_BYTES: usize = 240 * 1024;
impl SandboxView {
    /// A bounded number of geometric reductions makes pathological long-name
    /// inventories fit a frame without dropping the active player or body.
    /// `truncated` explicitly marks omitted regional entities.
    pub fn enforce_wire_budget(&mut self) -> Result<(), SandboxError> {
        self.truncated |= self.living.truncated;
        for _ in 0..8 {
            if serde_json::to_vec(self)?.len() <= MAX_SANDBOX_VIEW_BYTES {
                return Ok(());
            }
            self.truncated = true;
            self.living.truncated = true;
            self.plots.truncate(self.plots.len() / 2);
            self.structures.truncate(self.structures.len() / 2);
            self.resource_nodes.truncate(self.resource_nodes.len() / 2);
            self.farms.truncate(self.farms.len() / 2);
            self.feature_edits.truncate(self.feature_edits.len() / 2);
            self.factions.truncate(self.factions.len().div_ceil(2));
            self.living.shops.truncate(self.living.shops.len() / 2);
            self.living
                .producers
                .truncate(self.living.producers.len() / 2);
            self.living
                .deliveries
                .truncate(self.living.deliveries.len() / 2);
            self.living.roads.truncate(self.living.roads.len() / 2);
            let keep = self.living.units.len().div_ceil(2);
            let protected = self
                .living
                .account
                .as_ref()
                .map(|a| [a.active_unit, a.avatar_unit]);
            let mut ordinal = 0;
            self.living.units.retain(|u| {
                ordinal += 1;
                ordinal <= keep || protected.is_some_and(|p| p.contains(&u.id))
            });
        }
        if serde_json::to_vec(self)?.len() <= MAX_SANDBOX_VIEW_BYTES {
            Ok(())
        } else {
            Err(SandboxError::Capacity)
        }
    }
}
