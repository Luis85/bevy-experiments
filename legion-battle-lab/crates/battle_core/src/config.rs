use crate::MAX_UNITS;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Team {
    #[default]
    Blue,
    Red,
}

impl Team {
    pub const ALL: [Self; 2] = [Self::Blue, Self::Red];

    pub const fn index(self) -> usize {
        match self {
            Self::Blue => 0,
            Self::Red => 1,
        }
    }

    pub const fn opponent(self) -> Self {
        match self {
            Self::Blue => Self::Red,
            Self::Red => Self::Blue,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blaue Armee",
            Self::Red => "Rote Armee",
        }
    }
}

/// Gameplay category, independent of the backward-compatible weapon profile.
/// Categories share a simplified 2.5D combat plane in this first iteration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum UnitCategory {
    #[default]
    Creature,
    Vehicle,
    Building,
    AirUnit,
    SeaUnit,
    UndergroundUnit,
    SpaceUnit,
}

impl UnitCategory {
    pub const ALL: [Self; 7] = [
        Self::Creature,
        Self::Vehicle,
        Self::Building,
        Self::AirUnit,
        Self::SeaUnit,
        Self::UndergroundUnit,
        Self::SpaceUnit,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Creature => "Creature",
            Self::Vehicle => "Vehicle",
            Self::Building => "Building",
            Self::AirUnit => "Air Unit",
            Self::SeaUnit => "Sea Unit",
            Self::UndergroundUnit => "Underground Unit",
            Self::SpaceUnit => "Space Unit",
        }
    }

    /// Gameplay height above the common terrain plane, in metres.
    pub const fn elevation(self) -> f32 {
        match self {
            Self::Creature | Self::Vehicle | Self::Building | Self::SeaUnit => 0.0,
            Self::AirUnit => 8.0,
            Self::UndergroundUnit => -3.0,
            Self::SpaceUnit => 32.0,
        }
    }

    /// Sea units are a mobility proxy, not water-only naval navigation yet.
    pub const fn ignores_ground_obstacles(self) -> bool {
        matches!(
            self,
            Self::AirUnit | Self::SeaUnit | Self::UndergroundUnit | Self::SpaceUnit
        )
    }

    pub fn definition(self, kind: UnitKind) -> UnitDefinition {
        let mut definition = kind.definition();
        let (health, speed, range, damage, radius) = match self {
            Self::Creature => return definition,
            Self::Vehicle => (4.0, 1.5, 12.0, 2.5, 1.1),
            Self::Building => (10.0, 0.0, 20.0, 3.0, 1.5),
            Self::AirUnit => (1.5, 2.4, 16.0, 1.6, 0.55),
            Self::SeaUnit => (6.0, 0.85, 18.0, 3.0, 1.25),
            Self::UndergroundUnit => (2.0, 0.75, 5.0, 1.8, 0.65),
            Self::SpaceUnit => (3.0, 3.0, 40.0, 2.8, 0.65),
        };
        definition.health *= health;
        definition.speed *= speed;
        definition.range = definition.range.max(range);
        definition.damage *= damage;
        definition.radius = radius;
        definition
    }

    pub fn spawn_spacing(self, kind: UnitKind) -> f32 {
        (self.definition(kind).radius * 2.0 + 0.15).max(1.15)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    #[default]
    Infantry,
    Heavy,
    Ranger,
}

/// Shared immutable characteristics; one record per unit type.
#[derive(Clone, Copy, Debug)]
pub struct UnitDefinition {
    pub health: f32,
    pub speed: f32,
    pub range: f32,
    pub damage: f32,
    pub attack_period: f32,
    pub radius: f32,
}

impl UnitKind {
    pub const ALL: [Self; 3] = [Self::Infantry, Self::Heavy, Self::Ranger];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Infantry => "Infanterie",
            Self::Heavy => "Schwere Garde",
            Self::Ranger => "Fernkämpfer",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Infantry => "Ausgewogen · schnell · Nahkampf",
            Self::Heavy => "Robust · hoher Schaden · langsam",
            Self::Ranger => "Große Reichweite · verwundbar",
        }
    }

    pub const fn definition(self) -> UnitDefinition {
        match self {
            Self::Infantry => UnitDefinition {
                health: 100.0,
                speed: 3.8,
                range: 1.65,
                damage: 19.0,
                attack_period: 0.72,
                radius: 0.43,
            },
            Self::Heavy => UnitDefinition {
                health: 185.0,
                speed: 2.65,
                range: 1.95,
                damage: 36.0,
                attack_period: 1.05,
                radius: 0.53,
            },
            Self::Ranger => UnitDefinition {
                health: 65.0,
                speed: 3.3,
                range: 13.5,
                damage: 13.5,
                attack_period: 1.0,
                radius: 0.40,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formation {
    #[default]
    Lines,
    Wedge,
    Block,
}

impl Formation {
    pub const ALL: [Self; 3] = [Self::Lines, Self::Wedge, Self::Block];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Lines => "Breite Linien",
            Self::Wedge => "Keil",
            Self::Block => "Kompakter Block",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapKind {
    #[default]
    Meadow,
    Valley,
    Crossing,
}

impl MapKind {
    pub const ALL: [Self; 3] = [Self::Meadow, Self::Valley, Self::Crossing];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Meadow => "Offene Ebene",
            Self::Valley => "Hügeltal",
            Self::Crossing => "Flussübergang",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Meadow => "Freies Gelände für breite Fronten.",
            Self::Valley => "Sanfte Hügel verlangsamen den Vormarsch.",
            Self::Crossing => "Eine Brücke bündelt die Armeen an einer Engstelle.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmyConfig {
    pub count: u32,
    #[serde(default)]
    pub category: UnitCategory,
    pub kind: UnitKind,
    pub formation: Formation,
}

impl Default for ArmyConfig {
    fn default() -> Self {
        Self {
            count: 600,
            category: UnitCategory::Creature,
            kind: UnitKind::Infantry,
            formation: Formation::Lines,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioConfig {
    pub name: String,
    pub seed: u64,
    pub map: MapKind,
    pub blue: ArmyConfig,
    pub red: ArmyConfig,
    pub time_limit_secs: u32,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            name: "Kräftevergleich".into(),
            seed: 42,
            map: MapKind::Meadow,
            blue: ArmyConfig::default(),
            red: ArmyConfig::default(),
            time_limit_secs: 180,
        }
    }
}

impl ScenarioConfig {
    pub fn total_units(&self) -> u32 {
        self.blue.count.saturating_add(self.red.count)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("Der Szenarioname muss zwischen 1 und 80 Zeichen enthalten.".into());
        }
        if self.blue.count == 0 || self.red.count == 0 {
            return Err("Jede Armee benötigt mindestens eine Einheit.".into());
        }
        let Some(total) = self.blue.count.checked_add(self.red.count) else {
            return Err("Die Einheitenzahl ist zu groß.".into());
        };
        if total > MAX_UNITS {
            return Err(format!(
                "Der CPU-Prototyp unterstützt höchstens {MAX_UNITS} Einheiten insgesamt."
            ));
        }
        if !(30..=3600).contains(&self.time_limit_secs) {
            return Err("Das Zeitlimit muss zwischen 30 und 3600 Sekunden liegen.".into());
        }
        Ok(())
    }

    pub fn preset(index: usize) -> Self {
        let mut config = Self::default();
        match index {
            1 => {
                config.name = "Die letzte Brücke".into();
                config.map = MapKind::Crossing;
                config.blue.count = 900;
                config.red.count = 450;
                config.red.kind = UnitKind::Heavy;
                config.red.formation = Formation::Block;
                config.seed = 87;
            }
            2 => {
                config.name = "Pfeilhagel im Tal".into();
                config.map = MapKind::Valley;
                config.blue.count = 400;
                config.blue.kind = UnitKind::Ranger;
                config.red.count = 900;
                config.red.formation = Formation::Wedge;
                config.seed = 214;
            }
            3 => {
                config.name = "Große Front".into();
                config.blue.count = 5_000;
                config.red.count = 5_000;
                config.seed = 1_024;
                config.time_limit_secs = 300;
            }
            4 => {
                config.name = "Millionenfront".into();
                config.blue.count = 500_000;
                config.red.count = 500_000;
                config.seed = 2_026;
                config.time_limit_secs = 600;
            }
            _ => {}
        }
        config
    }

    /// Validate before encoding; scenario JSON contains configuration, not a savegame.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let config: Self = serde_json::from_str(json)
            .map_err(|error| format!("Ungültiges Szenario-JSON: {error}"))?;
        config.validate()?;
        Ok(config)
    }
}
