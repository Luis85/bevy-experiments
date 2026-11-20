//! Geographic residency for mutable overlays, separate from immutable source tiles.
//!
//! The bounded global ledger retains principals, rights, money, funded work,
//! facilities, roads, couriers and active owners' bodies/homes. Environmental
//! records, ambient bodies and disconnected owners' bodies/homes live in
//! dependency-closed regional pages. Private descriptors preserve cold references. A
//! dormant page does not advance: loading never invents elapsed production,
//! resource regeneration, animal recovery or actor needs changes.
//!
//! This module defines a domain partition, not a database or distributed lease.
//! Hosts must load a source region before registering source features, and must
//! preserve the exact `active_regions` returned by [`RegionWorkingSet`] when
//! merging a changed working set. Existing whole-world capacity limits apply.

use crate::{
    Catalog, FarmPlot, FeatureReference, GeoSpatialIndex, LivingUnit, NaturalResource,
    ResourceNode, SandboxError, SandboxState, Structure, UnitRole, WildlifeAnimal, WildlifeHabitat,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// The same WGS84 equirectangular key used for source tiles. Construct with
/// `RegionKey::at(position, level)`; longitude wraps and both poles are supported.
pub type RegionKey = terra_core::TileKey;

/// Bounds partition metadata even when all records occupy different regions.
pub const MAX_REGION_PAGES: usize = 4096;
/// Metric safety coverage supplements angular tile neighbors at high latitude,
/// the poles and fine region levels. It exceeds the maximum supported mutable
/// structure radius (50 m) plus interaction reach (20 m).
pub const REGION_SAFETY_RADIUS_M: f64 = 160.;

/// Capacity held by records outside the current working set. These counts are
/// generated from actual cold pages and cannot be changed through the public
/// state API. They serialize with a partial state so it cannot accidentally be
/// reinterpreted as a complete legacy save.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DormantReservations {
    pub structures: usize,
    pub farms: usize,
    pub resource_nodes: usize,
    pub world_features: usize,
    pub feature_edits: usize,
    pub living_units: usize,
    pub ecology_habitats: usize,
    pub ecology_animals: usize,
    pub ecology_resources: usize,
    /// Minimal private referential indexes; these are never client views and
    /// never substitute for full records at complete-world trust boundaries.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) owned_units: BTreeMap<u64, crate::dormancy::DormantUnitReference>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) homes: BTreeMap<u64, crate::dormancy::DormantHomeReference>,
}

impl DormantReservations {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn from_pages<'a>(
        pages: impl Iterator<Item = &'a RegionPage>,
        players: &BTreeMap<u64, crate::PlayerState>,
    ) -> Self {
        let referenced_homes: BTreeSet<_> = players.values().filter_map(|p| p.home).collect();
        let mut counts = Self::default();
        for page in pages {
            counts.structures += page.structures.len();
            counts.farms += page.farms.len();
            counts.resource_nodes += page.resource_nodes.len();
            counts.world_features += page.world_features.len();
            counts.feature_edits += page.feature_edits.len();
            counts.living_units += page.units.len();
            counts.ecology_habitats += page.habitats.len();
            counts.ecology_animals += page.animals.len();
            counts.ecology_resources += page.natural_resources.len();
            counts
                .owned_units
                .extend(page.units.values().filter_map(|unit| {
                    unit.owner.map(|owner| {
                        (
                            unit.id,
                            crate::dormancy::DormantUnitReference {
                                owner,
                                kind: unit.kind,
                                position: unit.position,
                                courier_supplies: (unit.role == UnitRole::Hauler)
                                    .then(|| unit.supplies.clone()),
                            },
                        )
                    })
                }));
            counts.homes.extend(
                page.structures
                    .values()
                    .filter(|s| s.housing && referenced_homes.contains(&s.id))
                    .map(|s| {
                        (
                            s.id,
                            crate::dormancy::DormantHomeReference {
                                owner: s.owner,
                                faction_owner: s.faction_owner,
                            },
                        )
                    }),
            );
        }
        counts
    }
}

impl SandboxState {
    pub fn dormant_reservations(&self) -> &DormantReservations {
        &self.dormant
    }

    pub fn is_partial(&self) -> bool {
        !self.dormant.is_empty()
    }

    /// Full-save envelopes must call this before serializing a SandboxState.
    /// Use the regional store's complete export instead of a working-set save.
    pub fn require_complete(&self) -> Result<(), SandboxError> {
        if self.is_partial() {
            Err(invalid(
                "regional working set is partial; export the complete regional world",
            ))
        } else {
            Ok(())
        }
    }

    pub fn living_unit_count(&self) -> usize {
        self.living
            .units
            .len()
            .saturating_add(self.dormant.living_units)
    }
    pub fn structure_count(&self) -> usize {
        self.structures
            .len()
            .saturating_add(self.dormant.structures)
    }
    pub fn farm_count(&self) -> usize {
        self.farms.len().saturating_add(self.dormant.farms)
    }
    pub fn resource_node_count(&self) -> usize {
        self.resource_nodes
            .len()
            .saturating_add(self.dormant.resource_nodes)
    }
    pub fn world_feature_count(&self) -> usize {
        self.world_features
            .len()
            .saturating_add(self.dormant.world_features)
    }
    pub fn feature_edit_count(&self) -> usize {
        self.feature_edits
            .len()
            .saturating_add(self.dormant.feature_edits)
    }
    pub fn ecology_habitat_count(&self) -> usize {
        self.ecology
            .habitats
            .len()
            .saturating_add(self.dormant.ecology_habitats)
    }
    pub fn ecology_animal_count(&self) -> usize {
        self.ecology
            .animals
            .len()
            .saturating_add(self.dormant.ecology_animals)
    }
    pub fn ecology_resource_count(&self) -> usize {
        self.ecology
            .resources
            .len()
            .saturating_add(self.dormant.ecology_resources)
    }

    pub(crate) fn validate_reservations(&self) -> Result<(), SandboxError> {
        if [
            self.structure_count(),
            self.farm_count(),
            self.resource_node_count(),
            self.world_feature_count(),
            self.feature_edit_count(),
        ]
        .into_iter()
        .any(|n| n > crate::MAX_ENTITIES)
            || self.living_unit_count() > crate::MAX_LIVING_UNITS
            || self.ecology_habitat_count() > self.ecology.config.max_habitats
            || self.ecology_animal_count() > self.ecology.config.max_animals
            || self.ecology_resource_count() > self.ecology.config.max_resources
        {
            Err(SandboxError::Capacity)
        } else {
            Ok(())
        }
    }
}

/// A page is deliberately not a standalone authoritative `SandboxState`.
/// `dependencies` is a symmetric, canonical graph of cross-page references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionPage {
    pub key: RegionKey,
    pub dependencies: BTreeSet<RegionKey>,
    pub structures: BTreeMap<u64, Structure>,
    pub farms: BTreeMap<u64, FarmPlot>,
    pub resource_nodes: BTreeMap<String, ResourceNode>,
    pub world_features: BTreeMap<String, FeatureReference>,
    pub feature_edits: BTreeSet<String>,
    pub units: BTreeMap<u64, LivingUnit>,
    pub habitats: BTreeMap<String, WildlifeHabitat>,
    pub animals: BTreeMap<u64, WildlifeAnimal>,
    pub natural_resources: BTreeMap<String, NaturalResource>,
}

impl RegionPage {
    fn new(key: RegionKey) -> Self {
        Self {
            key,
            dependencies: BTreeSet::new(),
            structures: BTreeMap::new(),
            farms: BTreeMap::new(),
            resource_nodes: BTreeMap::new(),
            world_features: BTreeMap::new(),
            feature_edits: BTreeSet::new(),
            units: BTreeMap::new(),
            habitats: BTreeMap::new(),
            animals: BTreeMap::new(),
            natural_resources: BTreeMap::new(),
        }
    }

    /// Count stored records, including metadata and tombstones, not only bodies.
    pub fn record_count(&self) -> usize {
        self.structures.len()
            + self.farms.len()
            + self.resource_nodes.len()
            + self.world_features.len()
            + self.feature_edits.len()
            + self.units.len()
            + self.habitats.len()
            + self.animals.len()
            + self.natural_resources.len()
    }

    fn environment_record_count(&self) -> usize {
        self.resource_nodes.len() + self.world_features.len() + self.feature_edits.len()
    }

    fn append_dependents_to(&self, target: &mut SandboxState) -> Result<(), SandboxError> {
        append_map(&mut target.structures, &self.structures)?;
        append_map(&mut target.farms, &self.farms)?;
        append_map(&mut target.living.units, &self.units)?;
        append_map(&mut target.ecology.habitats, &self.habitats)?;
        append_map(&mut target.ecology.animals, &self.animals)?;
        append_map(&mut target.ecology.resources, &self.natural_resources)?;
        Ok(())
    }

    fn positions(&self) -> impl Iterator<Item = terra_core::GeoPosition> + '_ {
        self.structures
            .values()
            .map(|s| s.position)
            .chain(self.farms.values().map(|f| f.position))
            .chain(self.resource_nodes.values().map(|n| n.position))
            .chain(self.world_features.values().map(|f| f.position))
            .chain(self.units.values().map(|u| u.position))
            .chain(self.habitats.values().map(|h| h.center))
    }

    fn append_to(&self, target: &mut SandboxState) -> Result<(), SandboxError> {
        append_map(&mut target.structures, &self.structures)?;
        append_map(&mut target.farms, &self.farms)?;
        append_map(&mut target.resource_nodes, &self.resource_nodes)?;
        append_map(&mut target.world_features, &self.world_features)?;
        for id in &self.feature_edits {
            if !target.feature_edits.insert(id.clone()) {
                return Err(invalid("duplicate regional feature tombstone"));
            }
        }
        append_map(&mut target.living.units, &self.units)?;
        append_map(&mut target.ecology.habitats, &self.habitats)?;
        append_map(&mut target.ecology.animals, &self.animals)?;
        append_map(&mut target.ecology.resources, &self.natural_resources)?;
        Ok(())
    }
}

/// Complete bounded world split into a global ledger and geographical pages.
/// Store the global JSON and individual page JSON atomically; this map's keys
/// are structs and the partition itself is intentionally not JSON-serializable.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionPartition {
    pub level: u8,
    pub global: SandboxState,
    pub pages: BTreeMap<RegionKey, RegionPage>,
}

#[derive(Debug, Clone)]
pub struct RegionWorkingSet {
    pub state: SandboxState,
    /// Requested regions, required dependencies and global-reference pins.
    /// Requested empty regions are retained so later source seeding is owned.
    pub active_regions: BTreeSet<RegionKey>,
    /// Additional regions required by cross-page references, excluding direct
    /// global-reference pins and explicitly requested regions.
    pub dependency_regions: BTreeSet<RegionKey>,
    /// Existing pages in a one-region ring or within 160 m of retained bodies,
    /// players and homes, plus habitat references from retained owned wildlife.
    /// Source terrain residency remains a separate host responsibility.
    pub pinned_regions: BTreeSet<RegionKey>,
}

impl RegionWorkingSet {
    /// Active records, including the always-resident global ledger.
    pub fn record_count(&self) -> usize {
        state_record_count(&self.state)
    }
}

impl RegionPartition {
    /// Validate a complete authoritative state and partition it deterministically.
    pub fn from_state(
        state: &SandboxState,
        level: u8,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        RegionKey::new(level, 0, 0)?;
        state.require_complete()?;
        state.validate_with_catalog(catalog)?;
        Self::partition_validated(state, level)
    }

    fn partition_validated(state: &SandboxState, level: u8) -> Result<Self, SandboxError> {
        let mut result = Self {
            level,
            global: state.clone(),
            pages: BTreeMap::new(),
        };
        let homes: BTreeSet<_> = state
            .players
            .values()
            .filter(|p| !state.is_player_dormant(p.id))
            .filter_map(|p| p.home)
            .collect();
        for (&id, structure) in &state.structures {
            if !homes.contains(&id) {
                let key = RegionKey::at(structure.position, level)?;
                result.page(key).structures.insert(id, structure.clone());
                result.global.structures.remove(&id);
            }
        }
        for (&id, farm) in &state.farms {
            let key = RegionKey::at(farm.position, level)?;
            result.page(key).farms.insert(id, farm.clone());
        }
        result.global.farms.clear();

        let mut nodes = BTreeMap::new();
        for (id, node) in &state.resource_nodes {
            let key = RegionKey::at(node.position, level)?;
            result
                .page(key)
                .resource_nodes
                .insert(id.clone(), node.clone());
            nodes.insert(id.clone(), key);
        }
        result.global.resource_nodes.clear();
        let mut features = BTreeMap::new();
        for (id, feature) in &state.world_features {
            let key = RegionKey::at(feature.position, level)?;
            result
                .page(key)
                .world_features
                .insert(id.clone(), feature.clone());
            if state.feature_edits.contains(id) {
                result.page(key).feature_edits.insert(id.clone());
            }
            features.insert(id.clone(), key);
        }
        result.global.world_features.clear();
        result.global.feature_edits.clear();

        let mut units = BTreeMap::new();
        for (&id, unit) in &state.living.units {
            // Unowned couriers stay resident even between jobs, so dispatch and
            // cargo movement keep the same lifetime as the global economy.
            if unit
                .owner
                .is_some_and(|owner| state.is_player_dormant(owner))
                || (unit.owner.is_none() && unit.role != UnitRole::Hauler)
            {
                let key = RegionKey::at(unit.position, level)?;
                result.page(key).units.insert(id, unit.clone());
                result.global.living.units.remove(&id);
                units.insert(id, key);
            }
        }
        let mut habitats = BTreeMap::new();
        for (id, habitat) in &state.ecology.habitats {
            let key = RegionKey::at(habitat.center, level)?;
            result
                .page(key)
                .habitats
                .insert(id.clone(), habitat.clone());
            habitats.insert(id.clone(), key);
        }
        result.global.ecology.habitats.clear();
        for (&id, animal) in &state.ecology.animals {
            if let Some(&key) = units.get(&id) {
                result.page(key).animals.insert(id, animal.clone());
                result.global.ecology.animals.remove(&id);
                result.link(key, habitats[&animal.habitat_id]);
            }
        }
        for (id, resource) in &state.ecology.resources {
            let key = nodes[id];
            result
                .page(key)
                .natural_resources
                .insert(id.clone(), resource.clone());
            if let Some(habitat) = &resource.habitat_id {
                result.link(key, habitats[habitat]);
            }
        }
        result.global.ecology.resources.clear();
        for node in state.resource_nodes.values() {
            if let Some(feature) = &node.source_feature {
                result.link(nodes[&node.id], features[feature]);
            }
        }
        if result.pages.len() > MAX_REGION_PAGES {
            return Err(SandboxError::Capacity);
        }
        // The global ledger is also a partial state and must not be exported
        // through the legacy standalone save API without its regional pages.
        result.global.dormant =
            DormantReservations::from_pages(result.pages.values(), &state.players);
        Ok(result)
    }

    /// Restore and validate the whole world, including canonical placement and
    /// dependency links. This detects forged page metadata and duplicate IDs;
    /// no record is silently overwritten. It is a boundary operation, not a
    /// per-simulation-tick operation.
    pub fn assemble_all(&self, catalog: &Catalog) -> Result<SandboxState, SandboxError> {
        self.validate_keys()?;
        let mut state = self.global.clone();
        for page in self.pages.values() {
            page.append_to(&mut state)?;
        }
        state.dormant = DormantReservations::default();
        state.validate_with_catalog(catalog)?;
        let canonical = Self::partition_validated(&state, self.level)?;
        if canonical != *self {
            return Err(invalid("noncanonical regional placement or dependencies"));
        }
        Ok(state)
    }

    /// Materialize a dependency-closed, fully valid active state. Unrequested
    /// environmental records stay outside the simulation and remain unchanged.
    pub fn working_set(
        &self,
        requested: &BTreeSet<RegionKey>,
        catalog: &Catalog,
    ) -> Result<RegionWorkingSet, SandboxError> {
        // The public mutable partition must be fully validated at every call.
        self.assemble_all(catalog)?;
        self.working_set_validated(requested, catalog)
    }

    fn working_set_validated(
        &self,
        requested: &BTreeSet<RegionKey>,
        catalog: &Catalog,
    ) -> Result<RegionWorkingSet, SandboxError> {
        self.validate_requested(requested)?;
        let pinned_regions = self.global_pins()?;
        let mut active_regions = requested.clone();
        active_regions.extend(&pinned_regions);
        let mut pending: Vec<_> = active_regions.iter().copied().collect();
        while let Some(key) = pending.pop() {
            if let Some(page) = self.pages.get(&key) {
                for &dependency in &page.dependencies {
                    if active_regions.insert(dependency) {
                        pending.push(dependency);
                    }
                }
            }
        }
        if active_regions.len() > MAX_REGION_PAGES {
            return Err(SandboxError::Capacity);
        }
        let dependency_regions = active_regions
            .difference(requested)
            .filter(|key| !pinned_regions.contains(key))
            .copied()
            .collect();
        let mut state = self.global.clone();
        for key in &active_regions {
            if let Some(page) = self.pages.get(key) {
                page.append_to(&mut state)?;
            }
        }
        state.dormant = DormantReservations::from_pages(
            self.pages
                .iter()
                .filter(|(key, _)| !active_regions.contains(key))
                .map(|(_, page)| page),
            &self.global.players,
        );
        state.validate_with_catalog(catalog)?;
        Ok(RegionWorkingSet {
            state,
            active_regions,
            dependency_regions,
            pinned_regions,
        })
    }

    /// Replace exactly the previous working set while preserving every dormant
    /// record. Current records can move into a cold region: their new page is
    /// merged with that region's existing records, never used to replace it.
    /// Supply the exact `active_regions` returned by the previous working set.
    /// Failure leaves this partition and the caller's active state untouched.
    pub fn merge_active(
        &self,
        active: &SandboxState,
        previously_active: &BTreeSet<RegionKey>,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        self.assemble_all(catalog)?;
        self.merge_active_validated(active, previously_active, catalog)
    }

    fn merge_active_validated(
        &self,
        active: &SandboxState,
        previously_active: &BTreeSet<RegionKey>,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        self.validate_requested(previously_active)?;
        active.validate_with_catalog(catalog)?;
        let reserved = DormantReservations::from_pages(
            self.pages
                .iter()
                .filter(|(key, _)| !previously_active.contains(key))
                .map(|(_, page)| page),
            &self.global.players,
        );
        if active.dormant != reserved {
            return Err(invalid(
                "regional working-set reservations do not match the previous active set",
            ));
        }
        let mut complete = active.clone();
        for (key, page) in &self.pages {
            if !previously_active.contains(key) {
                page.append_to(&mut complete)?;
            }
        }
        complete.dormant = DormantReservations::default();
        Self::from_state(&complete, self.level, catalog)
    }

    /// Records kept outside the per-region payloads, including accounts and
    /// all funded logistics. Counts include metadata rather than only entities.
    pub fn global_record_count(&self) -> usize {
        state_record_count(&self.global)
    }

    /// All regional payload records, excluding the separate global ledger.
    pub fn record_count(&self) -> usize {
        self.pages.values().map(RegionPage::record_count).sum()
    }

    fn page(&mut self, key: RegionKey) -> &mut RegionPage {
        self.pages
            .entry(key)
            .or_insert_with(|| RegionPage::new(key))
    }

    fn link(&mut self, a: RegionKey, b: RegionKey) {
        if a != b {
            self.page(a).dependencies.insert(b);
            self.page(b).dependencies.insert(a);
        }
    }

    fn validate_keys(&self) -> Result<(), SandboxError> {
        RegionKey::new(self.level, 0, 0)?;
        if self.pages.len() > MAX_REGION_PAGES {
            return Err(SandboxError::Capacity);
        }
        for (&key, page) in &self.pages {
            key.validate()?;
            if key.level != self.level || page.key != key || page.record_count() == 0 {
                return Err(invalid("regional key, level or empty page"));
            }
            for &dependency in &page.dependencies {
                dependency.validate()?;
                if dependency == key
                    || dependency.level != self.level
                    || !self.pages.contains_key(&dependency)
                {
                    return Err(invalid("regional dependency key"));
                }
            }
        }
        Ok(())
    }

    fn validate_requested(&self, requested: &BTreeSet<RegionKey>) -> Result<(), SandboxError> {
        if requested.len() > MAX_REGION_PAGES {
            return Err(SandboxError::Capacity);
        }
        for &key in requested {
            key.validate()?;
            if key.level != self.level {
                return Err(invalid("requested region level differs from partition"));
            }
        }
        Ok(())
    }

    fn global_pins(&self) -> Result<BTreeSet<RegionKey>, SandboxError> {
        let habitats: BTreeSet<_> = self
            .global
            .ecology
            .animals
            .values()
            .map(|a| &a.habitat_id)
            .collect();
        let mut pins: BTreeSet<_> = self
            .pages
            .iter()
            .filter(|(_, page)| page.habitats.keys().any(|id| habitats.contains(id)))
            .map(|(&key, _)| key)
            .collect();
        // Work is bounded by existing records, never by the enormous number of
        // narrow longitude cells intersecting a metric circle near a pole.
        // Metadata anchors are already represented by their node/body/habitat.
        let anchors = GeoSpatialIndex::from_positions(
            REGION_SAFETY_RADIUS_M,
            self.pages.iter().flat_map(|(&key, page)| {
                page.positions()
                    .enumerate()
                    .map(move |(index, position)| ((key, index), position))
            }),
        )?;
        for position in self
            .global
            .players
            .values()
            .filter(|p| !self.global.is_player_dormant(p.id))
            .map(|p| p.position)
            .chain(
                self.global
                    .living
                    .units
                    .values()
                    .filter(|u| {
                        u.owner
                            .is_none_or(|owner| !self.global.is_player_dormant(owner))
                    })
                    .map(|u| u.position),
            )
            .chain(self.global.structures.values().map(|s| s.position))
        {
            // Position and partition level were validated by assemble_all.
            if let Ok(center) = RegionKey::at(position, self.level) {
                pins.extend(
                    center
                        .neighbors(1)
                        .into_iter()
                        .filter(|key| self.pages.contains_key(key)),
                );
            }
            pins.extend(
                anchors
                    .query(position, REGION_SAFETY_RADIUS_M)?
                    .ids
                    .into_iter()
                    .map(|(key, _)| key),
            );
        }
        Ok(pins)
    }
}

/// Work performed while certifying the most recent partition revision. These
/// are record counts, not timing estimates or a claim that all work is sublinear.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionValidationWork {
    /// The supplied active state receives its ordinary complete partial-state
    /// validation before any cold proof can be reused.
    pub active_records: usize,
    /// Global/active records plus cold bodies, housing, farms and ecology that
    /// receive complete merged-scope validation against current global ledgers.
    pub merged_scope_records: usize,
    /// Unchanged resources, source features and tombstones covered by the old
    /// immutable certificate instead of repeated record/catalog validation.
    pub certified_environment_records: usize,
    pub certified_environment_groups: usize,
}

/// Immutable, catalog-pinned proof that a partition has passed complete record,
/// canonical-placement and dependency validation. Cloning shares the immutable
/// partition and catalog. No mutable accessor, deserializer or unchecked public
/// constructor exists. Storage must still guard its own generation and bytes.
#[derive(Debug, Clone)]
pub struct ValidatedRegionPartition {
    partition: Arc<RegionPartition>,
    catalog: Arc<Catalog>,
    validation_work: RegionValidationWork,
}

impl std::ops::Deref for ValidatedRegionPartition {
    type Target = RegionPartition;
    fn deref(&self) -> &Self::Target {
        &self.partition
    }
}

impl ValidatedRegionPartition {
    pub fn from_partition(
        partition: RegionPartition,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        partition.assemble_all(catalog)?;
        let records = partition.record_count() + partition.global_record_count();
        Ok(Self {
            partition: Arc::new(partition),
            catalog: Arc::new(catalog.clone()),
            validation_work: RegionValidationWork {
                active_records: records,
                merged_scope_records: records,
                ..Default::default()
            },
        })
    }

    pub fn from_state(
        state: &SandboxState,
        level: u8,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        Ok(Self {
            partition: Arc::new(RegionPartition::from_state(state, level, catalog)?),
            catalog: Arc::new(catalog.clone()),
            validation_work: RegionValidationWork {
                active_records: state_record_count(state),
                merged_scope_records: state_record_count(state),
                ..Default::default()
            },
        })
    }

    pub fn validation_work(&self) -> RegionValidationWork {
        self.validation_work
    }

    /// Copy records already covered by this immutable proof. There is no
    /// repeated whole-world validation, repartition or catalog walk here.
    pub fn assemble(&self) -> Result<SandboxState, SandboxError> {
        let mut state = self.partition.global.clone();
        for page in self.partition.pages.values() {
            page.append_to(&mut state)?;
        }
        state.dormant = DormantReservations::default();
        Ok(state)
    }

    pub fn select(
        &self,
        requested: &BTreeSet<RegionKey>,
    ) -> Result<RegionWorkingSet, SandboxError> {
        self.partition
            .working_set_validated(requested, &self.catalog)
    }

    /// Explicit full-scope reference path for audits and comparative probes.
    /// It validates every merged cold record and does not use environment-group
    /// certification. Both merge paths must produce identical canonical state.
    pub fn merge_validating_all(
        &self,
        active: &SandboxState,
        previously_active: &BTreeSet<RegionKey>,
    ) -> Result<Self, SandboxError> {
        let partition =
            self.partition
                .merge_active_validated(active, previously_active, &self.catalog)?;
        let work = RegionValidationWork {
            active_records: state_record_count(active),
            merged_scope_records: partition.global_record_count() + partition.record_count(),
            ..Default::default()
        };
        Ok(Self {
            partition: Arc::new(partition),
            catalog: Arc::clone(&self.catalog),
            validation_work: work,
        })
    }

    /// Validate the changed active state and every global-dependent cold record.
    /// Only immutable environment records in dependency-closed cold groups are
    /// certified without another catalog/record walk. Groups with natural-resource
    /// ecology metadata are deliberately excluded: their configuration and
    /// habitat constraints depend on the current global ecology policy.
    ///
    /// Numeric IDs, ownership, payroll/cargo, habitat counts and cold owner
    /// projections therefore still receive complete merged-scope validation.
    /// Cross-scope string-ID collisions and global capacity are checked before
    /// producing a new proof. Partial or untrusted partitions cannot enter here.
    pub fn merge(
        &self,
        active: &SandboxState,
        previously_active: &BTreeSet<RegionKey>,
    ) -> Result<Self, SandboxError> {
        self.partition.validate_requested(previously_active)?;
        active.validate_with_catalog(&self.catalog)?;
        let cold = |key: &&RegionKey| !previously_active.contains(*key);
        let reserved = DormantReservations::from_pages(
            self.partition
                .pages
                .iter()
                .filter(|(key, _)| cold(key))
                .map(|(_, page)| page),
            &self.partition.global.players,
        );
        if active.dormant != reserved {
            return Err(invalid(
                "regional working-set reservations do not match the previous active set",
            ));
        }
        // A claimed previous residency set must be dependency-closed. A caller
        // cannot use equal counts to substitute unrelated pages or cut a group.
        if previously_active.iter().any(|key| {
            self.partition.pages.get(key).is_some_and(|page| {
                page.dependencies
                    .iter()
                    .any(|key| !previously_active.contains(key))
            })
        }) {
            return Err(invalid("previous active regions are not dependency-closed"));
        }
        let mut reusable: BTreeSet<_> = self
            .partition
            .pages
            .iter()
            .filter(|(key, page)| cold(key) && page.natural_resources.is_empty())
            .map(|(&key, _)| key)
            .collect();
        // Existing canonical links are symmetric. Removing every component
        // touching an ineligible page prevents a retained node's source feature
        // or a policy-dependent node from straddling the validation boundary.
        loop {
            let rejected: Vec<_> = reusable
                .iter()
                .copied()
                .filter(|key| {
                    self.partition.pages[key]
                        .dependencies
                        .iter()
                        .any(|key| !reusable.contains(key))
                })
                .collect();
            if rejected.is_empty() {
                break;
            }
            for key in rejected {
                reusable.remove(&key);
            }
        }
        let mut scope = active.clone();
        let mut environment_reservations = DormantReservations::default();
        let mut certified_records = 0;
        for (key, page) in self.partition.pages.iter().filter(|(key, _)| cold(key)) {
            if reusable.contains(key) {
                // Actual cold bodies and their cargo are validated, never just
                // the independently editable private reference projections.
                page.append_dependents_to(&mut scope)?;
                environment_reservations.resource_nodes += page.resource_nodes.len();
                environment_reservations.world_features += page.world_features.len();
                environment_reservations.feature_edits += page.feature_edits.len();
                certified_records += page.environment_record_count();
                if page
                    .resource_nodes
                    .keys()
                    .any(|id| scope.resource_nodes.contains_key(id))
                    || page
                        .world_features
                        .keys()
                        .any(|id| scope.world_features.contains_key(id))
                    || page
                        .feature_edits
                        .iter()
                        .any(|id| scope.feature_edits.contains(id))
                {
                    return Err(invalid("duplicate certified environment record"));
                }
            } else {
                page.append_to(&mut scope)?;
            }
        }
        scope.dormant = environment_reservations;
        scope.validate_with_catalog(&self.catalog)?;
        let work = RegionValidationWork {
            active_records: state_record_count(active),
            merged_scope_records: state_record_count(&scope),
            certified_environment_records: certified_records,
            certified_environment_groups: environment_groups(&self.partition, &reusable),
        };
        let mut partition = RegionPartition::partition_validated(&scope, self.partition.level)?;
        // Reuse only environmental records. Rebuild their exact links rather
        // than retaining unrelated body/habitat links that a lifecycle change
        // may have removed from a mixed page.
        let feature_keys: BTreeMap<_, _> = reusable
            .iter()
            .flat_map(|key| {
                self.partition.pages[key]
                    .world_features
                    .keys()
                    .map(move |id| (id, *key))
            })
            .collect();
        for key in &reusable {
            let source = &self.partition.pages[key];
            if source.environment_record_count() == 0 {
                continue;
            }
            let target = partition.page(*key);
            append_map(&mut target.resource_nodes, &source.resource_nodes)?;
            append_map(&mut target.world_features, &source.world_features)?;
            for id in &source.feature_edits {
                if !target.feature_edits.insert(id.clone()) {
                    return Err(invalid("duplicate certified feature tombstone"));
                }
            }
            for node in source.resource_nodes.values() {
                if let Some(id) = &node.source_feature {
                    let feature_key = feature_keys.get(id).ok_or_else(|| {
                        invalid("certified environment dependency escaped its group")
                    })?;
                    partition.link(*key, *feature_key);
                }
            }
        }
        if partition.pages.len() > MAX_REGION_PAGES {
            return Err(SandboxError::Capacity);
        }
        partition.global.dormant =
            DormantReservations::from_pages(partition.pages.values(), &partition.global.players);
        Ok(Self {
            partition: Arc::new(partition),
            catalog: Arc::clone(&self.catalog),
            validation_work: work,
        })
    }
}

fn environment_groups(partition: &RegionPartition, keys: &BTreeSet<RegionKey>) -> usize {
    let mut remaining = keys.clone();
    let mut count = 0;
    while let Some(key) = remaining.pop_first() {
        let mut has_environment = false;
        let mut pending = vec![key];
        while let Some(key) = pending.pop() {
            let page = &partition.pages[&key];
            has_environment |= page.environment_record_count() != 0;
            for dependency in &page.dependencies {
                if remaining.remove(dependency) {
                    pending.push(*dependency);
                }
            }
        }
        count += usize::from(has_environment);
    }
    count
}

fn append_map<K: Ord + Clone, V: Clone>(
    target: &mut BTreeMap<K, V>,
    source: &BTreeMap<K, V>,
) -> Result<(), SandboxError> {
    if source.keys().any(|key| target.contains_key(key)) {
        return Err(invalid("duplicate regional record"));
    }
    target.extend(
        source
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    Ok(())
}

fn state_record_count(state: &SandboxState) -> usize {
    state.players.len()
        + state.factions.len()
        + state.plots.len()
        + state.structures.len()
        + state.resource_nodes.len()
        + state.farms.len()
        + state.world_features.len()
        + state.feature_edits.len()
        + state.living.units.len()
        + state.living.accounts.len()
        + state.living.shops.len()
        + state.living.producers.len()
        + state.living.deliveries.len()
        + state.living.roads.len()
        + state.living.traffic.junctions.len()
        + state.living.traffic.vehicle_dynamics.len()
        + state.living.traffic.queued_vehicles()
        + state.ecology.habitats.len()
        + state.ecology.animals.len()
        + state.ecology.resources.len()
        + state.ecology.seeded_regions.len()
        + state.settlements.territories.len()
        + state.settlements.contracts.len()
        + state.settlements.progress.len()
        + state.settlements.reference_prices.len()
        + state.transport.profiles.len()
        + state.transport.drivers.len()
        + state.transport.vehicles.len()
        + state.transport.facilities.len()
        + state.transport.jobs.len()
        + state.transport.trailers.len()
        + state.transport.courses.len()
        + state.transport.races.len()
        + state.transport.personal_bests.len()
        + state
            .transport
            .companies
            .values()
            .map(|company| 1 + company.drivers.len() + company.routes.len())
            .sum::<usize>()
}

fn invalid(message: &str) -> SandboxError {
    SandboxError::Invalid(message.into())
}
