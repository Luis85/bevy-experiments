use super::*;

pub const MAX_ECOLOGY_HABITATS: usize = 32;
pub const MAX_ECOLOGY_ANIMALS: usize = 64;
pub const MAX_ECOLOGY_RESOURCES: usize = 192;
pub const MAX_ECOLOGY_REGIONS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimalSpecies {
    Deer,
    Boar,
    Hare,
    Fox,
    Ibex,
}
impl AnimalSpecies {
    pub const ALL: [Self; 5] = [Self::Deer, Self::Boar, Self::Hare, Self::Fox, Self::Ibex];
    pub fn label(self) -> &'static str {
        match self {
            Self::Deer => "Deer",
            Self::Boar => "Boar",
            Self::Hare => "Hare",
            Self::Fox => "Fox",
            Self::Ibex => "Ibex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WildlifeBehavior {
    Grazing,
    Roaming,
    Fleeing,
    Drinking,
    Resting,
}
impl WildlifeBehavior {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grazing => "Grazing",
            Self::Roaming => "Roaming",
            Self::Fleeing => "Fleeing",
            Self::Drinking => "Drinking",
            Self::Resting => "Resting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HabitatKind {
    Woodland,
    Meadow,
    Shore,
    Highland,
}
impl HabitatKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Woodland => "Woodland",
            Self::Meadow => "Meadow",
            Self::Shore => "Shore",
            Self::Highland => "Highland",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalResourceKind {
    Timber,
    Stone,
    Berries,
    Fiber,
    Fish,
    IronOre,
    Water,
}
impl NaturalResourceKind {
    pub fn item(self) -> &'static str {
        match self {
            Self::Timber => "wood",
            Self::Stone => "stone",
            Self::Berries => "berries",
            Self::Fiber => "fiber",
            Self::Fish => "fish",
            Self::IronOre => "iron_ore",
            Self::Water => "water",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Timber => "Fallen timber",
            Self::Stone => "Stone outcrop",
            Self::Berries => "Berry thicket",
            Self::Fiber => "Wild flax",
            Self::Fish => "Fishing water",
            Self::IronOre => "Iron deposit",
            Self::Water => "Freshwater source",
        }
    }
    pub fn required_tool(self) -> Option<&'static str> {
        match self {
            Self::Fish => Some("fishing_rod"),
            Self::IronOre => Some("tool"),
            _ => None,
        }
    }
}

/// Game balance, not a biological field estimate. Saved with the world so
/// loading cannot silently change carrying capacity or replenishment policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesDefinition {
    pub species: AnimalSpecies,
    pub habitat: HabitatKind,
    pub group_size: u16,
    pub flee_distance_m: f64,
    pub roaming_radius_m: f64,
    pub decision_seconds: f64,
    pub recovery_seconds: f64,
    pub active_start_hour: f64,
    pub active_end_hour: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NaturalResourceDefinition {
    pub kind: NaturalResourceKind,
    pub capacity: u32,
    /// Zero is a finite deposit. Existing ResourceNode advancement owns growth.
    pub regrow_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EcologyConfig {
    pub enabled: bool,
    pub max_habitats: usize,
    pub max_animals: usize,
    pub max_resources: usize,
    /// Nonresident habitats freeze decisions; hosts still decide actual coverage.
    pub active_radius_m: f64,
    /// One bounded behavior pass per physical second by default.
    pub update_interval_seconds: f64,
    pub grazing_food_per_sim_second: f64,
    pub grazing_water_per_sim_second: f64,
    pub drinking_water_per_sim_second: f64,
    pub rest_energy_per_real_second: f64,
    pub species: BTreeMap<AnimalSpecies, SpeciesDefinition>,
    pub resources: BTreeMap<NaturalResourceKind, NaturalResourceDefinition>,
}
impl Default for EcologyConfig {
    fn default() -> Self {
        let species = [
            (
                AnimalSpecies::Deer,
                HabitatKind::Woodland,
                6,
                24.,
                75.,
                4.,
                7200.,
                5.,
                22.,
            ),
            (
                AnimalSpecies::Boar,
                HabitatKind::Woodland,
                4,
                15.,
                45.,
                5.,
                9000.,
                17.,
                8.,
            ),
            (
                AnimalSpecies::Hare,
                HabitatKind::Meadow,
                6,
                20.,
                38.,
                3.,
                5400.,
                4.,
                23.,
            ),
            (
                AnimalSpecies::Fox,
                HabitatKind::Woodland,
                2,
                30.,
                90.,
                4.,
                14400.,
                18.,
                7.,
            ),
            (
                AnimalSpecies::Ibex,
                HabitatKind::Highland,
                4,
                25.,
                60.,
                5.,
                10800.,
                5.,
                21.,
            ),
        ]
        .into_iter()
        .map(
            |(
                species,
                habitat,
                group_size,
                flee_distance_m,
                roaming_radius_m,
                decision_seconds,
                recovery_seconds,
                active_start_hour,
                active_end_hour,
            )| {
                (
                    species,
                    SpeciesDefinition {
                        species,
                        habitat,
                        group_size,
                        flee_distance_m,
                        roaming_radius_m,
                        decision_seconds,
                        recovery_seconds,
                        active_start_hour,
                        active_end_hour,
                    },
                )
            },
        )
        .collect();
        let resources = [
            (NaturalResourceKind::Timber, 96, 180.),
            (NaturalResourceKind::Stone, 120, 0.),
            (NaturalResourceKind::Berries, 36, 90.),
            (NaturalResourceKind::Fiber, 48, 120.),
            (NaturalResourceKind::Fish, 24, 900.),
            (NaturalResourceKind::IronOre, 72, 0.),
            (NaturalResourceKind::Water, 200, 30.),
        ]
        .into_iter()
        .map(|(kind, capacity, regrow_seconds)| {
            (
                kind,
                NaturalResourceDefinition {
                    kind,
                    capacity,
                    regrow_seconds,
                },
            )
        })
        .collect();
        Self {
            enabled: true,
            max_habitats: MAX_ECOLOGY_HABITATS,
            max_animals: MAX_ECOLOGY_ANIMALS,
            max_resources: MAX_ECOLOGY_RESOURCES,
            active_radius_m: 1800.,
            update_interval_seconds: 1.,
            grazing_food_per_sim_second: 0.0006,
            grazing_water_per_sim_second: 0.00006,
            drinking_water_per_sim_second: 0.002,
            rest_energy_per_real_second: 0.02,
            species,
            resources,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WildlifeHabitat {
    pub id: String,
    pub kind: HabitatKind,
    pub species: AnimalSpecies,
    pub center: GeoPosition,
    pub radius_m: f64,
    pub carrying_capacity: u16,
    pub water_access: Option<GeoPosition>,
    /// At most one recovered body per habitat/update; elapsed credit is capped.
    pub recovery_accumulator_seconds: f64,
    pub initial_population_seeded: bool,
    pub authored: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WildlifeAnimal {
    pub unit_id: u64,
    pub species: AnimalSpecies,
    pub habitat_id: String,
    pub behavior: WildlifeBehavior,
    pub age_seconds: f64,
    pub decision_remaining_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NaturalResource {
    pub node_id: String,
    pub kind: NaturalResourceKind,
    pub habitat_id: Option<String>,
    pub authored: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcologyMetrics {
    pub animals_spawned: u64,
    pub population_recoveries: u64,
    pub flee_events: u64,
    pub behavior_updates: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EcologyWorld {
    pub config: EcologyConfig,
    pub habitats: BTreeMap<String, WildlifeHabitat>,
    pub animals: BTreeMap<u64, WildlifeAnimal>,
    pub resources: BTreeMap<String, NaturalResource>,
    pub seeded_regions: BTreeSet<String>,
    pub elapsed_seconds: f64,
    pub update_accumulator_seconds: f64,
    pub metrics: EcologyMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EcologyView {
    pub animals: Vec<WildlifeAnimal>,
    pub habitats: Vec<WildlifeHabitat>,
    pub resources: Vec<NaturalResource>,
    pub metrics: EcologyMetrics,
    pub truncated: bool,
}

impl EcologyWorld {
    pub fn resource_requirement(&self, node_id: &str) -> Option<&'static str> {
        self.resources
            .get(node_id)
            .and_then(|r| r.kind.required_tool())
    }

    pub fn snapshot(&self, living: &LivingView, nodes: &[ResourceNode]) -> EcologyView {
        let animals: Vec<_> = living
            .units
            .iter()
            .filter_map(|u| self.animals.get(&u.id))
            .cloned()
            .collect();
        let resources: Vec<_> = nodes
            .iter()
            .filter_map(|n| self.resources.get(&n.id))
            .cloned()
            .collect();
        let ids: BTreeSet<_> = animals
            .iter()
            .map(|a| a.habitat_id.clone())
            .chain(resources.iter().filter_map(|r| r.habitat_id.clone()))
            .collect();
        let habitats = ids
            .iter()
            .filter_map(|id| self.habitats.get(id))
            .take(MAX_ECOLOGY_HABITATS)
            .cloned()
            .collect();
        EcologyView {
            truncated: animals.len() > MAX_ECOLOGY_ANIMALS
                || resources.len() > 64
                || ids.len() > MAX_ECOLOGY_HABITATS,
            animals: animals.into_iter().take(MAX_ECOLOGY_ANIMALS).collect(),
            habitats,
            resources: resources.into_iter().take(64).collect(),
            metrics: self.metrics.clone(),
        }
    }

    /// Replica construction only; a partial view is not an authoritative save.
    pub fn from_view(view: EcologyView) -> Self {
        Self {
            animals: view.animals.into_iter().map(|a| (a.unit_id, a)).collect(),
            habitats: view
                .habitats
                .into_iter()
                .map(|h| (h.id.clone(), h))
                .collect(),
            resources: view
                .resources
                .into_iter()
                .map(|r| (r.node_id.clone(), r))
                .collect(),
            metrics: view.metrics,
            ..Self::default()
        }
    }
}
