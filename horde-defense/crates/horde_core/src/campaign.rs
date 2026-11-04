use crate::content::{
    ItemDefId, ItemDefinition, Mode, NpcProfile, PersonalProfile, Policy, Slot, StructureKind,
    content,
};
use crate::kind_map::{KindMap, StableKind};
use crate::rules::Stats;
use crate::skill_catalog::{SkillModifiers, skills};
use crate::weapons::{WeaponClass, WeaponSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn legacy_training_default() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BodyAttribute {
    Vitality,
    Strength,
    Coordination,
    Endurance,
    Perception,
}
impl BodyAttribute {
    pub const ALL: [Self; 5] = [
        Self::Vitality,
        Self::Strength,
        Self::Coordination,
        Self::Endurance,
        Self::Perception,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
}
impl StableKind for BodyAttribute {
    const ALL: &'static [Self] = &Self::ALL;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Skill {
    Piercing,
    QuickReload,
    Sweep,
    SecondWind,
    Brace,
    Engineering,
}
impl Skill {
    pub const ALL: [Self; 6] = [
        Self::Piercing,
        Self::QuickReload,
        Self::Sweep,
        Self::SecondWind,
        Self::Brace,
        Self::Engineering,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Piercing => "piercing",
            Self::QuickReload => "quick_reload",
            Self::Sweep => "sweep",
            Self::SecondWind => "second_wind",
            Self::Brace => "brace",
            Self::Engineering => "engineering",
        }
    }
    pub fn name(self) -> &'static str {
        &skills()
            .node(self.id())
            .expect("validated legacy skill")
            .name
    }
    pub fn description(self) -> &'static str {
        &skills()
            .node(self.id())
            .expect("validated legacy skill")
            .description
    }
    pub fn price(self) -> u64 {
        skills()
            .node(self.id())
            .expect("validated legacy skill")
            .price
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NpcTraining {
    Strength,
    Dexterity,
    Intelligence,
    Health,
    Guns,
    Melee,
    HitPoints,
    Fatigue,
}
impl NpcTraining {
    pub const ALL: [Self; 8] = [
        Self::Strength,
        Self::Dexterity,
        Self::Intelligence,
        Self::Health,
        Self::Guns,
        Self::Melee,
        Self::HitPoints,
        Self::Fatigue,
    ];
}
impl StableKind for NpcTraining {
    const ALL: &'static [Self] = &Self::ALL;
}
impl NpcTraining {
    pub fn cost(self) -> u32 {
        match self {
            Self::Strength | Self::Health => 10,
            Self::Dexterity | Self::Intelligence => 20,
            Self::Guns | Self::Melee => 4,
            Self::HitPoints => 2,
            Self::Fatigue => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Strength => "ST +1",
            Self::Dexterity => "DX +1 (skills +1)",
            Self::Intelligence => "IQ +1 (informational)",
            Self::Health => "HT +1",
            Self::Guns => "Guns (Rifle) +1",
            Self::Melee => "Broadsword +1",
            Self::HitPoints => "HP (train ST)",
            Self::Fatigue => "FP (train HT)",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    #[serde(default)]
    pub legacy_graph_points: u32,
    pub level: u32,
    pub xp: u64,
    pub points: u32,
    pub body: KindMap<BodyAttribute, u32>,
    pub loadout: [u64; 4],
}
impl Player {
    pub fn level_cap() -> u32 {
        crate::progression::catalog().player_level_cap
    }
    pub fn is_max_level(&self) -> bool {
        self.level >= Self::level_cap()
    }
    pub fn next_level_xp(&self) -> u64 {
        (20.0 + 15.0 * ((self.level.saturating_sub(1)) as f64).powf(1.3)).round() as u64
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NpcCreation {
    pub archetype: String,
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
    pub ht: i32,
    pub guns: i32,
    pub melee: i32,
    pub budget: u32,
}
impl Default for NpcCreation {
    fn default() -> Self {
        Self {
            archetype: "Marksman".into(),
            st: 10,
            dx: 12,
            iq: 10,
            ht: 12,
            guns: 16,
            melee: 13,
            budget: 76,
        }
    }
}
impl NpcCreation {
    pub fn archetype(index: usize) -> Self {
        match index % 4 {
            0 => Self::default(),
            1 => Self {
                archetype: "Guardian".into(),
                st: 12,
                dx: 11,
                iq: 10,
                ht: 12,
                guns: 13,
                melee: 14,
                budget: 76,
            },
            2 => Self {
                archetype: "Scout".into(),
                st: 10,
                dx: 13,
                iq: 10,
                ht: 10,
                guns: 16,
                melee: 15,
                budget: 76,
            },
            _ => Self {
                archetype: "Steadfast".into(),
                st: 11,
                dx: 11,
                iq: 10,
                ht: 13,
                guns: 14,
                melee: 13,
                budget: 76,
            },
        }
    }
    pub fn points(&self) -> Option<i32> {
        let stats = Stats {
            st: self.st,
            dx: self.dx,
            iq: self.iq,
            ht: self.ht,
        };
        Some(
            stats.point_cost()
                + crate::rules::skill_point_cost(
                    self.guns - self.dx,
                    crate::rules::Difficulty::Easy,
                )? as i32
                + crate::rules::skill_point_cost(
                    self.melee - self.dx,
                    crate::rules::Difficulty::Average,
                )? as i32,
        )
    }
    pub fn valid(&self) -> bool {
        !self.archetype.trim().is_empty()
            && self.archetype.len() <= 48
            && [self.st, self.dx, self.iq, self.ht]
                .into_iter()
                .all(|n| (8..=14).contains(&n))
            && (3..=20).contains(&self.guns)
            && (3..=20).contains(&self.melee)
            && self.guns - self.dx >= 2
            && self.melee - self.dx >= 1
            && self.budget == 76
            && self
                .points()
                .is_some_and(|p| p >= 0 && p as u32 == self.budget)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Npc {
    pub name: String,
    #[serde(default)]
    pub creation: NpcCreation,
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
    pub ht: i32,
    pub guns: i32,
    pub melee: i32,
    pub extra_hp: i32,
    pub extra_fp: i32,
    pub awarded: u32,
    pub unspent: u32,
    pub investments: KindMap<NpcTraining, u32>,
    pub successful_hits: u64,
    pub combat_ticks: u64,
    pub objectives_claimed: [bool; 2],
    pub first_clears: [bool; 5],
    #[serde(default)]
    pub first_clear_claimed: [u32; 5],
    pub policy: Policy,
    pub recall_percent: u32,
    pub prefer_aim: bool,
    pub early_reload: bool,
    pub allow_risk: bool,
    pub loadout: [u64; 4],
    pub attempts: u64,
    pub victories: u64,
}
impl Npc {
    pub fn claim_objectives(&mut self, allowance: u32) -> u32 {
        let before = self.awarded;
        let award = |npc: &mut Self, points: u32| {
            let grant = points.min(allowance.saturating_sub(npc.awarded));
            npc.awarded += grant;
            npc.unspent += grant;
            grant
        };
        for (index, ready) in [self.successful_hits >= 20, self.combat_ticks >= 7200]
            .into_iter()
            .enumerate()
        {
            if ready && !self.objectives_claimed[index] && award(self, 1) > 0 {
                self.objectives_claimed[index] = true;
            }
        }
        for index in 0..5 {
            if self.first_clears[index] && self.first_clear_claimed[index] < 2 {
                let remaining = 2 - self.first_clear_claimed[index];
                let grant = award(self, remaining);
                self.first_clear_claimed[index] += grant;
            }
        }
        self.awarded - before
    }
    pub fn stats(&self) -> Stats {
        Stats {
            st: self.st,
            dx: self.dx,
            iq: self.iq,
            ht: self.ht,
        }
    }
    pub fn max_hp(&self) -> i32 {
        self.st + self.extra_hp
    }
    pub fn max_fp(&self) -> i32 {
        self.ht + self.extra_fp
    }
    pub fn basic_speed(&self) -> f32 {
        (self.dx + self.ht) as f32 / 4.0
    }
    pub fn dodge(&self) -> i32 {
        self.basic_speed().floor() as i32 + 3
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// None preserves the authored base quality and all historical numbers.
    #[serde(default)]
    pub quality: Option<crate::quality::Quality>,
    #[serde(default = "crate::progression::level_one")]
    pub level: u32,
    pub id: u64,
    pub definition: ItemDefId,
    pub locked: bool,
    pub favorite: bool,
    pub protected: bool,
}
impl Item {
    /// Authored definition; saved IDs are validated against the catalogue on load.
    pub fn definition(&self) -> &'static ItemDefinition {
        self.definition
            .definition()
            .expect("validated item definition ID")
    }
    /// Position of this item's definition in the loaded catalogue.
    pub fn definition_index(&self) -> usize {
        self.definition
            .index()
            .expect("validated item definition ID")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub kind: StructureKind,
    pub lane: usize,
    pub column: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub loadout: [u64; 4],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleQuote {
    pub coins: u64,
    pub affected_presets: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadoutMetrics {
    pub damage: i32,
    pub melee: i32,
    pub armor: i32,
    pub health: i32,
    pub stamina: i32,
    pub magazine: u32,
    pub reload: f32,
    pub shots_per_second: f32,
    pub weak_bonus: f32,
    pub guns: i32,
    pub dice: u8,
    pub adds: i32,
    pub dodge: i32,
    pub expected_basic_damage: f32,
    pub baseline_shots_to_kill: Option<u32>,
    pub melee_dice: u8,
    pub melee_adds: i32,
    pub melee_skill: i32,
    pub accuracy: i32,
    pub damage_kind: crate::rules::DamageKind,
    pub melee_kind: crate::rules::DamageKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemComparison {
    pub item_id: u64,
    pub slot: Slot,
    pub before: LoadoutMetrics,
    pub after: LoadoutMetrics,
    pub explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationAction {
    ChooseCountry,
    AllocateBody(BodyAttribute),
    Equip(u64),
    BuyEquipment(usize),
    LearnSkill(Skill),
    LearnNode(String),
    TrainNpc(NpcTraining),
    BuyTraining,
    BuyBlueprint(StructureKind),
    Deploy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodePayment {
    Coins(u64),
    LegacyBodyPoints(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
    pub action: RecommendationAction,
    pub price: u64,
    pub affordable: bool,
}
pub(crate) fn mean_basic_damage(damage: crate::rules::Damage) -> f32 {
    let minimum = damage.minimum();
    if i32::from(damage.dice) + damage.adds >= minimum {
        return 3.5 * f32::from(damage.dice) + damage.adds as f32;
    }
    // Exact bounded convolution for authored negative modifiers: taking the
    // floor after averaging would misrepresent the minimum-damage procedure.
    let mut counts = vec![1u64];
    for _ in 0..damage.dice {
        let mut next = vec![0u64; counts.len() + 6];
        for (sum, count) in counts.iter().enumerate() {
            for face in 1..=6 {
                next[sum + face] += count;
            }
        }
        counts = next;
    }
    let total = counts.iter().sum::<u64>() as f64;
    (counts
        .iter()
        .enumerate()
        .map(|(sum, count)| *count as f64 * (sum as i32 + damage.adds).max(minimum) as f64)
        .sum::<f64>()
        / total) as f32
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    #[serde(default)]
    pub journey_active: bool,
    #[serde(default)]
    pub journey_revision: u8,
    #[serde(default = "legacy_training_default")]
    pub legacy_features: bool,
    #[serde(default)]
    pub balance_settings: crate::balance::BalanceSettings,
    #[serde(default)]
    pub run_history: Vec<crate::stats::RunRecord>,
    #[serde(default = "crate::stats::first_run_id")]
    pub next_run_id: u64,
    /// Zero denotes pre-victory-threat saves; migrate their clear high-water once.
    #[serde(default)]
    pub threat_revision: u8,
    #[serde(default = "crate::survivors::legacy_selection")]
    pub selected_survivor: Option<crate::survivors::SurvivorId>,
    #[serde(default)]
    pub defeated_bosses: Vec<crate::content::BossKind>,
    pub coins: u64,
    pub unlocked: usize,
    pub first_clears: [bool; 5],
    pub player: Player,
    pub npc: Npc,
    pub items: Vec<Item>,
    /// Historical six-node save adapter. New purchases live in skill_nodes.
    pub skills: Vec<Skill>,
    #[serde(default)]
    pub skill_nodes: BTreeSet<String>,
    /// Missing in pre-proficiency saves: grandfather their formerly unrestricted
    /// armory. Fresh campaigns require authored class proficiency and ownership.
    #[serde(default = "legacy_training_default")]
    pub legacy_weapon_training: bool,
    pub blueprints: KindMap<StructureKind, bool>,
    pub structure_upgrades: KindMap<StructureKind, u32>,
    pub layout: Vec<Placement>,
    #[serde(default)]
    pub country_layouts: BTreeMap<String, Vec<Placement>>,
    pub presets: Vec<Preset>,
    pub attempts: u64,
    pub victories: u64,
    pub next_item_id: u64,
    pub next_attempt_id: u64,
    pub reserved: Option<Mode>,
    #[serde(default)]
    pub reserved_items: Vec<u64>,
    pub kills_since_loot: u32,
    #[serde(default)]
    pub world: crate::world::WorldProgress,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefenderProfile {
    #[serde(default)]
    pub equipment_visual: crate::survivors::EquipmentVisual,
    #[serde(default)]
    pub appearance: crate::survivors::Appearance,
    #[serde(default)]
    pub weapon: WeaponSpec,
    #[serde(default)]
    pub modifiers: SkillModifiers,
    pub personal: PersonalProfile,
    pub npc: NpcProfile,
    pub stats: Stats,
    pub ranged_skill: i32,
    pub melee_skill: i32,
    pub max_hp: i32,
    pub max_fp: i32,
    pub npc_melee_damage_dice: u8,
    pub npc_melee_damage_add: i32,
    pub npc_melee_kind: crate::rules::DamageKind,
    pub pierce: bool,
    pub sweep: bool,
    pub heal: bool,
    pub brace: bool,
    pub engineering: bool,
    pub strength: u32,
    pub endurance: u32,
    pub perception: u32,
}
impl Default for Campaign {
    fn default() -> Self {
        let loadout = [1, 2, 3, 4];
        // Keep definitions 0..4 and item IDs 1..4 unchanged for old saves/NPCs.
        let starter_pistol = content().items.iter().position(|i| {
            !i.npc_only
                && i.tier == 0
                && i.slot == Slot::Ranged
                && WeaponSpec::from_item(i).class == WeaponClass::Pistol
        });
        let personal_loadout = if starter_pistol.is_some() {
            [5, 2, 3, 4]
        } else {
            loadout
        };
        Self {
            journey_active: false,
            journey_revision: 1,
            legacy_features: false,
            balance_settings: Default::default(),
            run_history: Vec::new(),
            next_run_id: 1,
            threat_revision: 1,
            selected_survivor: None,
            defeated_bosses: Vec::new(),
            coins: 0,
            unlocked: 1,
            first_clears: [false; 5],
            player: Player {
                legacy_graph_points: 0,
                level: 1,
                xp: 0,
                points: 0,
                body: KindMap::default(),
                loadout: personal_loadout,
            },
            npc: Npc {
                name: "Mara Vale".into(),
                creation: NpcCreation::default(),
                st: 10,
                dx: 12,
                iq: 10,
                ht: 12,
                guns: 16,
                melee: 13,
                extra_hp: 0,
                extra_fp: 0,
                awarded: 0,
                unspent: 0,
                investments: KindMap::default(),
                successful_hits: 0,
                combat_ticks: 0,
                objectives_claimed: [false; 2],
                first_clears: [false; 5],
                first_clear_claimed: [0; 5],
                policy: Policy::Balanced,
                recall_percent: 0,
                prefer_aim: true,
                early_reload: false,
                allow_risk: false,
                loadout,
                attempts: 0,
                victories: 0,
            },
            items: (0..4)
                .map(|n| Item {
                    quality: None,
                    level: 1,
                    id: n + 1,
                    definition: ItemDefId::at(n as usize),
                    locked: true,
                    favorite: false,
                    protected: true,
                })
                .chain(starter_pistol.map(|definition| Item {
                    quality: None,
                    level: 1,
                    id: 5,
                    definition: ItemDefId::at(definition),
                    locked: true,
                    favorite: false,
                    protected: true,
                }))
                .collect(),
            skills: vec![],
            skill_nodes: BTreeSet::from(["pistol_training".into()]),
            legacy_weapon_training: starter_pistol.is_none(),
            blueprints: KindMap::default(),
            structure_upgrades: KindMap::default(),
            layout: vec![],
            country_layouts: BTreeMap::new(),
            presets: vec![Preset {
                name: "Starter".into(),
                loadout: personal_loadout,
            }],
            attempts: 0,
            victories: 0,
            next_item_id: if starter_pistol.is_some() { 6 } else { 5 },
            next_attempt_id: 1,
            reserved: None,
            reserved_items: vec![],
            kills_since_loot: 0,
            world: crate::world::WorldProgress::default(),
        }
    }
}
impl Campaign {
    fn preparation(&self) -> Result<(), String> {
        if self.reserved.is_some() {
            Err("Return to camp before changing equipment or progression".into())
        } else {
            Ok(())
        }
    }
    fn spend(&mut self, price: u64) -> Result<(), String> {
        if self.coins < price {
            return Err(format!("Need {} more coins", price - self.coins));
        }
        self.coins -= price;
        Ok(())
    }
    pub fn npc_available(&self) -> bool {
        self.has_feature(crate::skill_catalog::CampaignFeature::Companion)
    }
    pub fn allowance(&self) -> u32 {
        12 + 8 * self.first_clears.iter().filter(|x| **x).count() as u32
    }
    pub fn body_cap(&self) -> u32 {
        5 + 3 * self.first_clears.iter().filter(|x| **x).count() as u32
    }
    pub fn has_skill(&self, s: Skill) -> bool {
        self.skills.contains(&s) || self.skill_nodes.contains(s.id())
    }
    fn owns_node_definition(&self, node: &crate::skill_catalog::SkillNode) -> bool {
        self.owns_node_with_body(node, &self.player.body)
    }
    fn owns_node_with_body(
        &self,
        node: &crate::skill_catalog::SkillNode,
        body: &KindMap<BodyAttribute, u32>,
    ) -> bool {
        self.skill_nodes.contains(&node.id)
            || self.skills.iter().any(|s| s.id() == node.id)
            || (node.id == "pistol_training" && self.legacy_weapon_training)
            || node.effects.iter().any(|effect| match effect {
                crate::skill_catalog::SkillEffect::BodyDevelopment { attribute, rank } => {
                    body[*attribute] >= *rank
                }
                crate::skill_catalog::SkillEffect::Blueprint(kind) => self.blueprints[*kind],
                crate::skill_catalog::SkillEffect::StructureRank { kind, rank } => {
                    self.structure_upgrades[*kind] >= *rank
                }
                _ => false,
            })
    }
    pub fn has_node(&self, id: &str) -> bool {
        skills()
            .node(id)
            .is_some_and(|node| self.owns_node_definition(node))
    }
    pub fn liberated_count(&self) -> u32 {
        crate::world::world()
            .countries
            .iter()
            .filter(|country| {
                self.world.cleared.contains(&country.id)
                    || self
                        .world
                        .clear_high_water
                        .get(&country.id)
                        .copied()
                        .unwrap_or(0)
                        == country.level_count()
            })
            .count() as u32
    }
    pub fn has_feature(&self, feature: crate::skill_catalog::CampaignFeature) -> bool {
        if self.legacy_features
            && (feature != crate::skill_catalog::CampaignFeature::Companion || self.attempts > 0)
        {
            return true;
        }
        self.skill_nodes.iter().map(String::as_str).chain(self.skills.iter().map(|skill|skill.id())).filter_map(|id|skills().node(id)).any(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::FeatureUnlock(value) if *value==feature)))
    }
    pub fn body_rank(&self, attribute: BodyAttribute) -> u32 {
        self.player.body[attribute].max(self.modifiers().body_development[attribute])
    }
    pub fn modifiers(&self) -> SkillModifiers {
        let mut result = SkillModifiers::default();
        // Catalogue order, rather than purchase order, fixes floating-point stacking.
        for node in &skills().nodes {
            if self.owns_node_definition(node) {
                for effect in &node.effects {
                    result.apply(effect);
                }
            }
        }
        if self.legacy_weapon_training {
            result.weapon_classes.extend(WeaponClass::ALL);
        }
        result
    }
    pub fn can_use_weapon(&self, mode: Mode, definition: usize) -> bool {
        self.weapon_access(mode, definition).is_ok()
    }
    pub fn set_balance_settings(
        &mut self,
        mut settings: crate::balance::BalanceSettings,
    ) -> Result<(), String> {
        settings.validate()?;
        if settings.preset != crate::balance::BalancePreset::Custom
            && settings != crate::balance::BalanceSettings::for_preset(settings.preset)
        {
            settings.preset = crate::balance::BalancePreset::Custom;
        }
        self.balance_settings = settings;
        Ok(())
    }
    pub fn arena_geometry(&self) -> (u8, f32) {
        self.balance_settings.geometry(self.player.level)
    }
    pub(crate) fn clone_for_worker(&self) -> Self {
        Self {
            journey_active: false,
            journey_revision: 1,
            legacy_features: self.legacy_features,
            balance_settings: self.balance_settings.clone(),
            run_history: Vec::new(),
            next_run_id: 1,
            threat_revision: self.threat_revision,
            selected_survivor: self.selected_survivor,
            defeated_bosses: self.defeated_bosses.clone(),
            coins: self.coins,
            unlocked: self.unlocked,
            first_clears: self.first_clears,
            player: self.player.clone(),
            npc: self.npc.clone(),
            items: self.items.clone(),
            skills: self.skills.clone(),
            skill_nodes: self.skill_nodes.clone(),
            legacy_weapon_training: self.legacy_weapon_training,
            blueprints: self.blueprints.clone(),
            structure_upgrades: self.structure_upgrades.clone(),
            layout: self.layout.clone(),
            country_layouts: self.country_layouts.clone(),
            presets: self.presets.clone(),
            attempts: self.attempts,
            victories: self.victories,
            next_item_id: self.next_item_id,
            next_attempt_id: self.next_attempt_id,
            reserved: self.reserved,
            reserved_items: self.reserved_items.clone(),
            kills_since_loot: self.kills_since_loot,
            world: self.world.clone(),
        }
    }
    pub(crate) fn append_run(&mut self, mut record: crate::stats::RunRecord) {
        record.id = self.next_run_id;
        self.next_run_id = self
            .next_run_id
            .checked_add(1)
            .expect("Run history sequence exhausted");
        self.run_history.push(record);
    }
    pub fn choose_survivor(&mut self, id: crate::survivors::SurvivorId) -> Result<(), String> {
        self.preparation()?;
        if self.world.start.is_some() {
            return Err("Choose your survivor before selecting the starting country".into());
        }
        self.selected_survivor = Some(id);
        Ok(())
    }
    pub fn progression_rank(&self) -> u32 {
        let victories = if self.threat_revision == 0 {
            self.victories.max(self.legacy_clear_rank() as u64)
        } else {
            self.victories
        };
        u32::try_from(victories).unwrap_or(u32::MAX)
    }
    fn legacy_clear_rank(&self) -> u32 {
        let mut cleared = self.world.clear_high_water.clone();
        for (id, count) in &self.world.country_levels {
            cleared
                .entry(id.clone())
                .and_modify(|n| *n = (*n).max(*count))
                .or_insert(*count);
        }
        cleared
            .values()
            .copied()
            .sum::<usize>()
            .max(self.first_clears.iter().filter(|v| **v).count()) as u32
    }
    pub fn migrate_threat_history(&mut self) {
        if self.threat_revision == 0 {
            self.victories = self.victories.max(u64::from(self.legacy_clear_rank()));
            self.threat_revision = 1;
        }
        if self.player.is_max_level() {
            self.player.xp = 0;
        }
    }
    pub fn player_level_cap(&self) -> u32 {
        Player::level_cap()
    }
    pub fn lane_count(&self) -> u8 {
        self.balance_settings.geometry(self.player.level).0
    }
    pub fn next_lane_unlock_level(&self) -> Option<u32> {
        (self.player.level.saturating_add(1)..=self.player_level_cap())
            .find(|level| self.balance_settings.geometry(*level).0 > self.lane_count())
    }
    pub fn completion_xp_preview(&self) -> u64 {
        if self.player.is_max_level() {
            return 0;
        }
        let mut needed = self.player.next_level_xp().saturating_sub(self.player.xp);
        for level in self.player.level + 1..self.player_level_cap() {
            let mut p = self.player.clone();
            p.level = level;
            needed = needed.saturating_add(p.next_level_xp());
        }
        needed.min(crate::progression::catalog().victory_player_xp)
    }
    pub fn level_after_victory(&self) -> u32 {
        let mut player = self.player.clone();
        Self::award_xp_to(
            &mut player,
            self.completion_xp_preview(),
            self.legacy_features,
        );
        player.level
    }
    pub fn lane_count_after_victory(&self) -> u8 {
        self.balance_settings.geometry(self.level_after_victory()).0
    }
    fn progression_for(
        &self,
        mode: Mode,
        rank: u32,
        lanes: u8,
    ) -> Result<crate::progression::ProgressionSnapshot, String> {
        let cfg = crate::progression::catalog();
        if rank > cfg.threat_guard_rank {
            return Err("Supported victory-threat numerical limit reached".into());
        }
        let mut result = cfg.snapshot(rank);
        result.enemy_health =
            1.0 + (result.enemy_health - 1.0) * self.balance_settings.threat_growth;
        result.enemy_damage =
            1.0 + (result.enemy_damage - 1.0) * self.balance_settings.threat_growth;
        result.enemy_cap = cfg.population_for_lanes(result.enemy_cap, lanes);
        if mode == Mode::Npc {
            result.arrival_multiplier *= cfg.npc_arrival_factor;
        }
        Ok(result)
    }
    pub fn deployment_progression(
        &self,
        mode: Mode,
    ) -> Result<crate::progression::ProgressionSnapshot, String> {
        self.progression_for(mode, self.progression_rank(), self.lane_count())
    }
    /// Preview includes known completion XP only, not unearned future kill XP.
    pub fn next_victory_progression(
        &self,
        mode: Mode,
    ) -> Result<crate::progression::ProgressionSnapshot, String> {
        self.progression_for(
            mode,
            self.progression_rank().saturating_add(1),
            self.lane_count_after_victory(),
        )
    }
    pub fn shop_item_level(&self) -> u32 {
        crate::progression::catalog()
            .snapshot(self.progression_rank())
            .item_level
    }
    pub fn buy_price(&self, definition: usize) -> Result<u64, String> {
        Ok(crate::progression::price(
            content()
                .items
                .get(definition)
                .ok_or("Unknown equipment")?
                .buy_value,
            self.shop_item_level(),
        ))
    }
    pub fn item_sale_value(&self, id: u64) -> Result<u64, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(
            (crate::progression::price(item.definition().sell_value, item.level) as f64
                * crate::quality::value_ratio(item.definition().base_quality, item.quality))
            .round() as u64,
        )
    }
    pub fn item_quality(&self, id: u64) -> Result<crate::quality::Quality, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(item.quality.unwrap_or(item.definition().base_quality))
    }
    pub fn effective_item(&self, id: u64) -> Result<crate::content::ItemDefinition, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(crate::quality::apply(
            crate::progression::scale_item(item.definition().clone(), item.level),
            item.quality,
        ))
    }
    /// Shared access reason for equipment actions, deployment and presentation.
    pub fn weapon_access(&self, mode: Mode, definition: usize) -> Result<(), &'static str> {
        let Some(item) = content().items.get(definition) else {
            return Err("Unknown equipment");
        };
        // NPCs retain their independent Guns skill/training and attack procedure.
        if mode == Mode::Npc {
            return Ok(());
        }
        if item.npc_only && !self.legacy_weapon_training {
            return Err(
                "NPC issued gear is reserved for NPC defenders. Buy or loot your own weapon; proficiency does not grant equipment.",
            );
        }
        if item.slot == Slot::Ranged
            && !self
                .modifiers()
                .weapon_classes
                .contains(&WeaponSpec::from_item(item).class)
        {
            return Err(
                "Learn this weapon class proficiency in Skills before equipping this owned weapon.",
            );
        }
        Ok(())
    }
    pub fn purchase_node(&mut self, id: &str) -> Result<(), String> {
        self.can_purchase_node(id)?;
        let payment = self.node_payment(id)?;
        let node = skills().node(id).expect("preflight node");
        match payment {
            NodePayment::Coins(price) => self.spend(price)?,
            NodePayment::LegacyBodyPoints(points) => {
                self.player.points -= points;
                self.player.legacy_graph_points += points;
            }
        }
        self.skill_nodes.insert(id.to_owned());
        for effect in &node.effects {
            match effect {
                crate::skill_catalog::SkillEffect::Blueprint(kind) => self.blueprints[*kind] = true,
                crate::skill_catalog::SkillEffect::StructureRank { kind, rank } => {
                    self.structure_upgrades[*kind] = self.structure_upgrades[*kind].max(*rank)
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn node_payment(&self, id: &str) -> Result<NodePayment, String> {
        let node = skills().node(id).ok_or("Unknown skill node")?;
        if self.legacy_features && self.player.points>0 && node.effects.len()==1 && node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::BodyDevelopment{rank,..} if *rank<=self.body_cap())) {{self.player.legacy_graph_points.checked_add(1).ok_or("Legacy graph point ledger exhausted")?;Ok(NodePayment::LegacyBodyPoints(1))}}else{Ok(NodePayment::Coins(node.price))}
    }
    pub fn can_purchase_node(&self, id: &str) -> Result<(), String> {
        self.preparation()?;
        let node = skills().node(id).ok_or("Unknown skill node")?;
        if self.has_node(id) {
            return Err("Already learned".into());
        }
        if node.price == 0 {
            return Err("Starter proficiency is granted only at campaign creation".into());
        }
        if node
            .prerequisites
            .iter()
            .any(|parent| !self.has_node(parent))
        {
            return Err("Learn the required prerequisite skills first".into());
        }
        if self.liberated_count() < node.minimum_liberated {
            return Err(format!(
                "Liberate {} countries first",
                node.minimum_liberated
            ));
        }
        if self.victories < node.minimum_victories {
            return Err(format!("Win {} defenses first", node.minimum_victories));
        }
        if let NodePayment::Coins(price) = self.node_payment(id)?
            && self.coins < price
        {
            return Err(format!("Need {} more coins", price - self.coins));
        }
        Ok(())
    }
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.items
            .binary_search_by_key(&id, |item| item.id)
            .ok()
            .map(|index| &self.items[index])
    }
    pub fn equipped(&self, mode: Mode, slot: Slot) -> &crate::content::ItemDefinition {
        let loadout = match mode {
            Mode::Personal => self.player.loadout,
            Mode::Npc => self.npc.loadout,
        };
        self.item(loadout[slot.index()])
            .map_or(&content().items[slot.index()], Item::definition)
    }
    fn equipment_visual_for(
        &self,
        loadout: [u64; 4],
        class: WeaponClass,
    ) -> crate::survivors::EquipmentVisual {
        let item = |slot: Slot| {
            self.item(loadout[slot.index()]).map(|item| {
                let definition = item.definition();
                crate::survivors::EquipmentVisualItem {
                    definition: item.definition.clone(),
                    level: item.level,
                    quality: item.quality.unwrap_or(definition.base_quality),
                    style: definition.visual_style.clone(),
                }
            })
        };
        crate::survivors::EquipmentVisual {
            ranged: item(Slot::Ranged),
            outfit: item(Slot::Outfit),
            accessory: item(Slot::Accessory),
            melee: item(Slot::Melee),
            weapon_class: Some(class),
        }
    }
    pub fn profile(&self, mode: Mode) -> DefenderProfile {
        let loadout = match mode {
            Mode::Personal => self.player.loadout,
            Mode::Npc => self.npc.loadout,
        };
        self.profile_for_loadout(mode, loadout, &self.npc)
    }
    pub fn profile_for_loadout(
        &self,
        mode: Mode,
        loadout: [u64; 4],
        character: &Npc,
    ) -> DefenderProfile {
        let equipment = |slot: Slot| {
            let definition = self
                .item(loadout[slot.index()])
                .map_or(&content().items[slot.index()], Item::definition);
            let level = self
                .item(loadout[slot.index()])
                .map_or(1, |item| item.level);
            crate::quality::apply(
                crate::progression::scale_item(definition.clone(), level),
                self.item(loadout[slot.index()])
                    .and_then(|item| item.quality),
            )
        };
        let weapon = WeaponSpec::from_item(&equipment(Slot::Ranged));
        let modifiers = self.modifiers();
        let mut personal = equipment(Slot::Ranged).personal.clone();
        let mut npc = equipment(Slot::Ranged).npc.clone();
        let melee = equipment(Slot::Melee);
        let base_melee = crate::rules::Damage {
            dice: melee.npc.damage_dice,
            adds: melee.npc.damage_add,
            kind: melee.npc.damage_kind,
        };
        let npc_melee =
            crate::rules::strength_adjusted_swing(base_melee, character.st).unwrap_or(base_melee);
        personal.melee_damage = melee.personal.melee_damage;
        personal.health = 100;
        personal.stamina = 100;
        for slot in [Slot::Outfit, Slot::Accessory] {
            let i = equipment(slot);
            personal.armor += i.personal.armor;
            personal.health += i.personal.health;
            personal.stamina += i.personal.stamina;
            personal.weak_bonus += i.personal.weak_bonus;
            npc.armor += i.npc.armor;
            npc.accuracy += i.npc.accuracy;
            npc.skill_bonus += i.npc.skill_bonus;
        }
        let b = BodyAttribute::ALL.map(|attribute| {
            self.player.body[attribute].max(modifiers.body_development[attribute])
        });
        if mode == Mode::Personal {
            personal.health += b[0] as i32 * 5;
            personal.melee_damage += b[1] as i32 * 5;
            personal.reload = (personal.reload * (1.0 - 0.025 * b[2] as f32)).max(0.65);
            personal.stamina += b[3] as i32 * 5;
            personal.weak_bonus = (personal.weak_bonus + 0.03 * b[4] as f32).min(1.0);
            personal.reload = (personal.reload * modifiers.reload_multiplier).max(0.65);
            personal.cadence = (personal.cadence * modifiers.cadence_multiplier).max(0.08);
            personal.damage = ((personal.damage + modifiers.damage_add) as f32
                * modifiers.damage_multiplier)
                .round()
                .clamp(1.0, 100_000.0) as i32;
            personal.health += modifiers.health_add;
            personal.stamina += modifiers.stamina_add;
            personal.armor += modifiers.armor_add;
            personal.melee_damage += modifiers.melee_add;
        }
        let is_personal = mode == Mode::Personal;
        DefenderProfile {
            equipment_visual: self.equipment_visual_for(loadout, weapon.class),
            appearance: if mode == Mode::Npc {
                crate::survivors::Appearance::Female
            } else {
                self.selected_survivor
                    .map_or(crate::survivors::Appearance::Male, |id| id.appearance())
            },
            weapon,
            modifiers: modifiers.clone(),
            personal,
            npc,
            stats: character.stats(),
            ranged_skill: character.guns,
            melee_skill: character.melee,
            max_hp: character.max_hp()
                + if mode == Mode::Npc {
                    ([Slot::Outfit, Slot::Accessory]
                        .into_iter()
                        .map(|slot| {
                            self.item(loadout[slot.index()])
                                .map_or(0, |item| item.level.saturating_sub(1))
                        })
                        .sum::<u32>() as f32
                        * crate::progression::catalog().gear.npc_health_per_level)
                        .floor() as i32
                } else {
                    0
                },
            max_fp: character.max_fp(),
            npc_melee_damage_dice: npc_melee.dice,
            npc_melee_damage_add: npc_melee.adds,
            npc_melee_kind: melee.npc.damage_kind,
            pierce: is_personal && modifiers.piercing,
            sweep: is_personal && modifiers.sweep,
            heal: is_personal && modifiers.heal,
            brace: is_personal && modifiers.brace,
            engineering: modifiers.engineering,
            strength: if is_personal { b[1] } else { 0 },
            endurance: if is_personal { b[3] } else { 0 },
            perception: if is_personal { b[4] } else { 0 },
        }
    }
    pub fn compare_item(&self, mode: Mode, id: u64) -> Result<ItemComparison, String> {
        let candidate = self.item(id).ok_or("Item is no longer owned")?;
        let slot = candidate
            .definition
            .definition()
            .ok_or("Unknown item profile")?
            .slot;
        let before = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        let mut after = before;
        after[slot.index()] = id;
        self.validate_loadout(after)?;
        let metrics = |loadout| {
            let p = self.profile_for_loadout(mode, loadout, &self.npc);
            if mode == Mode::Personal {
                let health = content().enemy(crate::content::EnemyKind::Shambler).health as u32;
                let damage = ((p.personal.damage
                    - content().enemy(crate::content::EnemyKind::Shambler).armor)
                    .max(1) as u32
                    * p.weapon.pellets)
                    .min(p.weapon.hit_cap(health as i32) as u32)
                    .max(1);
                LoadoutMetrics {
                    damage: p.personal.damage,
                    melee: p.personal.melee_damage,
                    armor: p.personal.armor,
                    health: p.personal.health,
                    stamina: p.personal.stamina,
                    magazine: p.personal.magazine,
                    reload: p.personal.reload,
                    shots_per_second: 1.0 / p.personal.cadence.max(0.01),
                    weak_bonus: p.personal.weak_bonus,
                    guns: 0,
                    dice: 0,
                    adds: 0,
                    dodge: 0,
                    expected_basic_damage: p.personal.damage as f32,
                    baseline_shots_to_kill: Some(health.div_ceil(damage)),
                    melee_dice: 0,
                    melee_adds: 0,
                    melee_skill: 0,
                    accuracy: 0,
                    damage_kind: p.npc.damage_kind,
                    melee_kind: p.npc_melee_kind,
                }
            } else {
                LoadoutMetrics {
                    damage: 0,
                    melee: 0,
                    armor: p.npc.armor,
                    health: p.max_hp,
                    stamina: p.max_fp,
                    magazine: p.npc.magazine,
                    reload: p.npc.reload_turns as f32,
                    shots_per_second: 1.0,
                    weak_bonus: 0.0,
                    guns: p.ranged_skill + p.npc.skill_bonus,
                    dice: p.npc.damage_dice,
                    adds: p.npc.damage_add,
                    dodge: p.stats.dodge(),
                    expected_basic_damage: mean_basic_damage(crate::rules::Damage {
                        dice: p.npc.damage_dice,
                        adds: p.npc.damage_add,
                        kind: p.npc.damage_kind,
                    }),
                    baseline_shots_to_kill: None,
                    melee_dice: p.npc_melee_damage_dice,
                    melee_adds: p.npc_melee_damage_add,
                    melee_skill: p.melee_skill,
                    accuracy: p.npc.accuracy,
                    damage_kind: p.npc.damage_kind,
                    melee_kind: p.npc_melee_kind,
                }
            }
        };
        Ok(ItemComparison{item_id:id,slot,before:metrics(before),after:metrics(after),explanation:if mode==Mode::Personal{"Personal body-hit profile; Shambler breakpoint excludes weak points and country modifiers."}else{"NPC preparation profile before country modifiers. Weapon mean is basic damage before attack rolls, active defense, armor and wounding; not a chance to injure."}.into()})
    }
    pub fn recommendation(&self, mode: Mode) -> Recommendation {
        let make = |title: String, detail: String, action, price| Recommendation {
            title,
            detail,
            action,
            price,
            affordable: self.coins >= price,
        };
        if self.world.start.is_none() {
            return make("Choose your starting country".into(),"Select any country, then prepare its first defense. Earned coins and loot survive defeat.".into(),RecommendationAction::ChooseCountry,0);
        }
        if self
            .world
            .active
            .as_ref()
            .is_some_and(|id| self.world.cleared.contains(id))
        {
            let next = crate::world::world()
                .countries
                .iter()
                .filter(|c| self.world.can_enter(&c.id) && !self.world.cleared.contains(&c.id))
                .min_by(|a, b| a.difficulty.total_cmp(&b.difficulty).then(a.id.cmp(&b.id)));
            if let Some(country) = next {
                return make(
                    "Open a new neighboring front".into(),
                    format!(
                        "{} is accessible after liberation: {} local defenses. Select an unlocked country on the map; already cleared defenses remain replayable.",
                        country.name,
                        country.level_count()
                    ),
                    RecommendationAction::ChooseCountry,
                    0,
                );
            }
            return make("The configured world is liberated".into(),"Replay cleared defenses, prepare survivors and watch for strategic plague alerts. Earned travel routes remain open.".into(),RecommendationAction::Deploy,0);
        }
        if mode == Mode::Personal
            && self.legacy_features
            && self.player.points > 0
            && let Some(node) = skills().nodes.iter().find(|node| {
                node.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        crate::skill_catalog::SkillEffect::BodyDevelopment { .. }
                    )
                }) && self.can_purchase_node(&node.id).is_ok()
                    && matches!(
                        self.node_payment(&node.id),
                        Ok(NodePayment::LegacyBodyPoints(_))
                    )
            })
        {
            return make("Use a retained legacy body point".into(),"Legacy points remain spendable through body nodes in the same skill graph. No coins are charged for this adapter.".into(),RecommendationAction::LearnNode(node.id.clone()),0);
        }
        let current = self.profile(mode);
        let score = |m: &LoadoutMetrics, slot: Slot| match slot {
            Slot::Ranged => {
                if mode == Mode::Personal {
                    m.damage as f32 * m.shots_per_second + 2.0 * m.magazine as f32 - 5.0 * m.reload
                } else {
                    m.expected_basic_damage * 8.0 + m.guns as f32 * 3.0 + m.magazine as f32
                }
            }
            Slot::Melee => {
                if mode == Mode::Personal {
                    m.melee as f32
                } else {
                    (3.5 * m.melee_dice as f32 + m.melee_adds as f32) * 4.0 + m.melee_skill as f32
                }
            }
            Slot::Outfit => m.armor as f32 * 10.0 + m.health as f32,
            Slot::Accessory => {
                m.health as f32
                    + m.stamina as f32
                    + m.armor as f32 * 10.0
                    + m.weak_bonus * 40.0
                    + m.accuracy as f32 * 4.0
            }
        };
        let owned = self
            .items
            .iter()
            .filter(|i| {
                !self.item_reserved(i.id) && self.can_use_weapon(mode, i.definition_index())
            })
            .filter_map(|item| self.compare_item(mode, item.id).ok())
            .filter(|c| score(&c.after, c.slot) > score(&c.before, c.slot) + 1.0)
            .max_by(|a, b| {
                (score(&a.after, a.slot) - score(&a.before, a.slot))
                    .total_cmp(&(score(&b.after, b.slot) - score(&b.before, b.slot)))
            });
        if let Some(item) = owned {
            return make(
                "Equip an improvement already in your armory".into(),
                format!(
                    "{} improves your selected {:?} loadout. Compare the complete profile before equipping.",
                    self.item(item.item_id)
                        .expect("owned comparison")
                        .definition()
                        .name,
                    mode
                ),
                RecommendationAction::Equip(item.item_id),
                0,
            );
        }
        if mode == Mode::Npc && self.npc.unspent >= NpcTraining::Guns.cost() {
            return make(
                "Improve Mara's ranged skill".into(),
                format!(
                    "Spend 4 unspent NPC points: Guns {} to {}. Awarded points remain inside the campaign allowance.",
                    self.npc.guns,
                    self.npc.guns + 1
                ),
                RecommendationAction::TrainNpc(NpcTraining::Guns),
                0,
            );
        }
        for kind in StructureKind::ALL {
            if self.blueprint_available(kind) && !self.blueprints[kind] {
                let def = content().structure(kind);
                return make(
                    format!("Unlock {}", def.name),
                    format!(
                        "Blueprint ownership is permanent. Placement uses renewed attempt supply, not your wallet. {} coins required.",
                        def.blueprint_price
                    ),
                    RecommendationAction::LearnNode(
                        match kind {
                            StructureKind::Trap => "trap_blueprint",
                            StructureKind::Wall => "wall_blueprint",
                            StructureKind::Turret => "turret_blueprint",
                        }
                        .into(),
                    ),
                    def.blueprint_price,
                );
            }
        }
        if mode == Mode::Personal && !self.has_skill(Skill::QuickReload) {
            let node = skills()
                .node("quick_reload")
                .expect("validated compatibility node");
            return make(
                format!("Learn {}", node.name),
                format!(
                    "{} coins. {} Rewards survive short attempts.",
                    node.price, node.description
                ),
                RecommendationAction::LearnNode(node.id.clone()),
                node.price,
            );
        }
        if mode == Mode::Personal {
            for item in &self.items {
                if self.can_use_weapon(mode, item.definition_index())
                    || item.definition().slot != Slot::Ranged
                    || (item.definition().npc_only && !self.legacy_weapon_training)
                {
                    continue;
                }
                let class = WeaponSpec::from_item(item.definition()).class;
                if let Some(node) = skills().nodes.iter().find(|n| !self.has_node(&n.id) && n.prerequisites.iter().all(|p| self.has_node(p)) && n.effects.iter().any(|e| matches!(e, crate::skill_catalog::SkillEffect::WeaponTraining(c) if *c == class))) {
                    return make(format!("Train for your owned {}", item.definition().name), format!("{} Unlocking proficiency permits equipping the weapon already in your armory.", node.description), RecommendationAction::LearnNode(node.id.clone()), node.price);
                }
            }
        }
        let slot = if (mode == Mode::Personal && current.personal.armor < 3)
            || (mode == Mode::Npc && current.npc.armor < 3)
        {
            Slot::Outfit
        } else {
            Slot::Ranged
        };
        let current_item = self.equipped(mode, slot);
        let current_level = self
            .item(if mode == Mode::Personal {
                self.player.loadout[slot.index()]
            } else {
                self.npc.loadout[slot.index()]
            })
            .map_or(1, |item| item.level);
        let desired = content()
            .items
            .iter()
            .enumerate()
            .filter(|(index, i)| {
                self.can_use_weapon(mode, *index)
                    && i.slot == slot
                    && (i.tier > current_item.tier
                        || (i.tier == current_item.tier && self.shop_item_level() > current_level))
                    && i.tier <= self.unlocked
                    && i.buy_value > 0
            })
            .min_by_key(|(_, i)| i.buy_value);
        if let Some((index, item)) = desired {
            let price = self.buy_price(index).expect("known shop definition");
            return make(
                format!("Save for {}", item.name),
                format!(
                    "Guaranteed quartermaster gear costs {} coins; {} more needed. No rare drop is required.",
                    price,
                    price.saturating_sub(self.coins)
                ),
                RecommendationAction::BuyEquipment(index),
                price,
            );
        }
        if mode == Mode::Npc && self.npc.awarded < self.allowance() {
            return make("Fund Mara's next training point".into(),"Training is immediate in camp. Four Guns points buy another skill level; earned objectives are claimed first.".into(),RecommendationAction::BuyTraining,25);
        }
        let detail=self.world.active_country().map_or_else(||"Choose an unlocked country on the map.".to_owned(),|country|format!("{}: defense {} of {}. Earlier cleared defenses remain replayable; rewards are banked immediately.",country.name,self.world.next_stage()+1,country.level_count()));
        make(
            "Prepare the next defense".into(),
            detail,
            RecommendationAction::Deploy,
            0,
        )
    }
    /// Compatibility entry point: same catalogue price and prerequisite checks.
    pub fn purchase_skill(&mut self, skill: Skill) -> Result<(), String> {
        self.purchase_node(skill.id())?;
        // Old callers may inspect this ledger directly. Mirroring the ID here
        // retains that API; modifiers deduplicate through has_node.
        self.skills.push(skill);
        Ok(())
    }
    pub fn allocate_body(&mut self, attribute: BodyAttribute) -> Result<(), String> {
        self.preparation()?;
        if !self.legacy_features {
            return Err("Purchase body-development nodes in the skill tree".into());
        }
        if self.player.points == 0 {
            return Err("Earn a character point by leveling up".into());
        }
        if self.player.body[attribute] >= self.body_cap() {
            return Err("Attribute at campaign tier cap".into());
        }
        self.player.points -= 1;
        self.player.body[attribute] += 1;
        Ok(())
    }
    pub fn respec_player(&mut self) -> Result<(), String> {
        self.preparation()?;
        if !self.legacy_features {
            return Err("Coin-trained body nodes are permanent".into());
        }
        // Legacy ranks can supply the prerequisite floor for permanent graph
        // purchases. Check the proposed refund without removing that ownership.
        for id in &self.skill_nodes {
            let node = skills()
                .node(id)
                .ok_or_else(|| format!("Unknown owned skill node: {id}"))?;
            if node.prerequisites.iter().any(|prerequisite| {
                !skills()
                    .node(prerequisite)
                    .is_some_and(|required| self.owns_node_with_body(required, &KindMap::default()))
            }) {
                return Err(format!(
                    "Cannot refund legacy body points: learned skill {id} requires those ranks"
                ));
            }
        }
        self.player.points += self.player.body.values().sum::<u32>();
        self.player.body = KindMap::default();
        Ok(())
    }
    pub fn reward(&mut self, coins: u64, xp: u64, mode: Mode) -> u64 {
        self.coins = self.coins.saturating_add(coins);
        if mode == Mode::Personal {
            self.award_player_xp(xp)
        } else {
            0
        }
    }
    fn award_player_xp(&mut self, xp: u64) -> u64 {
        Self::award_xp_to(&mut self.player, xp, self.legacy_features)
    }
    fn award_xp_to(player: &mut Player, mut xp: u64, legacy_points: bool) -> u64 {
        let mut credited = 0;
        while xp > 0 && !player.is_max_level() {
            let needed = player.next_level_xp().saturating_sub(player.xp);
            let grant = xp.min(needed);
            player.xp += grant;
            credited += grant;
            xp -= grant;
            if player.xp >= player.next_level_xp() {
                player.xp = 0;
                player.level += 1;
                if legacy_points {
                    player.points = player.points.saturating_add(1);
                }
            }
        }
        if player.is_max_level() {
            player.xp = 0;
        }
        credited
    }
    pub fn add_item(&mut self, definition: usize) -> Result<u64, String> {
        self.add_item_at_level(definition, 1)
    }
    pub fn add_item_at_level(&mut self, definition: usize, level: u32) -> Result<u64, String> {
        self.add_item_with_quality(definition, level, None)
    }
    pub fn add_item_with_quality(
        &mut self,
        definition: usize,
        level: u32,
        quality: Option<crate::quality::Quality>,
    ) -> Result<u64, String> {
        if !(1..=64).contains(&level) {
            return Err("Unsupported item level".into());
        }
        if definition >= content().items.len() {
            return Err("Unknown item definition".into());
        }
        let id = self.next_item_id;
        let next = id
            .checked_add(1)
            .filter(|_| id > 0)
            .ok_or("Persistent item identity space exhausted")?;
        self.next_item_id = next;
        self.items.push(Item {
            quality,
            level,
            id,
            definition: ItemDefId::at(definition),
            locked: false,
            favorite: false,
            protected: false,
        });
        self.kills_since_loot = 0;
        Ok(id)
    }
    pub fn buy_item(&mut self, definition: usize) -> Result<u64, String> {
        self.preparation()?;
        let def = content().items.get(definition).ok_or("Unknown equipment")?;
        if def.tier == 0 {
            return Err("Protected starter equipment is already owned".into());
        }
        if def.tier > self.unlocked {
            return Err("Clear more stages to unlock this equipment tier".into());
        }
        let price = self.buy_price(definition)?;
        if self.coins < price {
            return Err(format!("Need {} more coins", price - self.coins));
        }
        let id = self.add_item_at_level(definition, self.shop_item_level())?;
        self.coins -= price;
        Ok(id)
    }
    pub fn equip(&mut self, mode: Mode, id: u64) -> Result<(), String> {
        self.preparation()?;
        if self.reserved_items.contains(&id) {
            return Err("This equipment is reserved by another survivor".into());
        }
        let item = self.item(id).ok_or("Item is no longer owned")?;
        let slot = item.definition().slot.index();
        self.weapon_access(mode, item.definition_index())
            .map_err(str::to_owned)?;
        match mode {
            Mode::Personal => self.player.loadout[slot] = id,
            Mode::Npc => self.npc.loadout[slot] = id,
        };
        Ok(())
    }
    pub fn set_lock(&mut self, id: u64, locked: bool) -> Result<(), String> {
        self.preparation()?;
        let item = self
            .items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or("Unknown item")?;
        if item.protected && !locked {
            return Err("Starter equipment remains protected".into());
        }
        item.locked = locked;
        Ok(())
    }
    pub fn set_favorite(&mut self, id: u64, favorite: bool) -> Result<(), String> {
        self.preparation()?;
        let item = self
            .items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or("Unknown item")?;
        item.favorite = favorite;
        Ok(())
    }
    pub fn sale_preview(&self, id: u64) -> Result<SaleQuote, String> {
        self.preparation()?;
        let item = self.item(id).ok_or("Item is no longer owned")?;
        if self.reserved_items.contains(&id) {
            return Err("This item is deployed or assigned to a country guard".into());
        }
        if item.protected || item.locked || item.favorite {
            return Err("Remove item protection before selling".into());
        }
        Ok(SaleQuote {
            coins: self.item_sale_value(item.id)?,
            affected_presets: self
                .presets
                .iter()
                .filter(|p| p.loadout.contains(&id))
                .map(|p| p.name.clone())
                .collect(),
        })
    }
    pub fn sell(&mut self, id: u64) -> Result<u64, String> {
        let quote = self.sale_preview(id)?;
        let wallet = self
            .coins
            .checked_add(quote.coins)
            .ok_or("Sale would exceed the wallet limit")?;
        self.items.retain(|x| x.id != id);
        for slot in 0..4 {
            if self.player.loadout[slot] == id {
                self.player.loadout[slot] = self.personal_starter_id(slot);
            }
            if self.npc.loadout[slot] == id {
                self.npc.loadout[slot] = slot as u64 + 1;
            }
        }
        self.coins = wallet;
        Ok(quote.coins)
    }
    pub fn sell_quote(&self) -> u64 {
        self.items
            .iter()
            .filter(|x| {
                !x.protected
                    && !x.locked
                    && !x.favorite
                    && !self.reserved_items.contains(&x.id)
                    && !self.player.loadout.contains(&x.id)
                    && !self.npc.loadout.contains(&x.id)
                    && !self.presets.iter().any(|p| p.loadout.contains(&x.id))
            })
            .map(|x| self.item_sale_value(x.id).expect("known owned item"))
            .sum()
    }
    pub fn sell_unprotected(&mut self) -> Result<u64, String> {
        self.preparation()?;
        self.coins
            .checked_add(self.sell_quote())
            .ok_or("Batch sale would exceed the wallet limit")?;
        let ids: Vec<_> = self
            .items
            .iter()
            .filter(|x| {
                !x.protected
                    && !x.locked
                    && !x.favorite
                    && !self.reserved_items.contains(&x.id)
                    && !self.player.loadout.contains(&x.id)
                    && !self.npc.loadout.contains(&x.id)
                    && !self.presets.iter().any(|p| p.loadout.contains(&x.id))
            })
            .map(|x| x.id)
            .collect();
        let mut total = 0;
        for id in ids {
            total += self.sell(id)?;
        }
        Ok(total)
    }
    pub fn save_preset(&mut self, name: String, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        if name.trim().is_empty() || name.len() > 48 {
            return Err("Preset name must contain 1-48 bytes".into());
        }
        let loadout = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        if let Some(p) = self.presets.iter_mut().find(|x| x.name == name) {
            p.loadout = loadout;
        } else {
            self.presets.push(Preset { name, loadout });
        }
        Ok(())
    }
    pub fn apply_preset(&mut self, index: usize, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        let loadout = self.presets.get(index).ok_or("Unknown preset")?.loadout;
        self.validate_loadout(loadout)?;
        for id in loadout {
            let item = self.item(id).ok_or("Missing equipment")?;
            self.weapon_access(mode, item.definition_index())
                .map_err(str::to_owned)?;
        }
        if loadout.iter().any(|id| self.reserved_items.contains(id)) {
            return Err("Preset equipment is reserved by another survivor".into());
        }
        match mode {
            Mode::Personal => self.player.loadout = loadout,
            Mode::Npc => self.npc.loadout = loadout,
        };
        Ok(())
    }
    fn personal_starter_id(&self, slot: usize) -> u64 {
        if slot == 0
            && self.items.iter().any(|i| {
                i.id == 5
                    && i.protected
                    && WeaponSpec::from_item(i.definition()).class == WeaponClass::Pistol
            })
        {
            5
        } else {
            slot as u64 + 1
        }
    }
    pub fn validate_loadout(&self, loadout: [u64; 4]) -> Result<(), String> {
        for (slot, id) in loadout.into_iter().enumerate() {
            let item = self.item(id).ok_or("Preset references sold equipment")?;
            if item
                .definition
                .definition()
                .ok_or("Unsupported equipment")?
                .slot
                .index()
                != slot
            {
                return Err("Equipment assigned to an incompatible slot".into());
            }
        }
        Ok(())
    }
    pub fn reserve(&mut self, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        if mode == Mode::Npc && !self.npc_available() {
            return Err(if self.legacy_features {
                "Complete your introductory attempt to meet Mara".into()
            } else {
                "Learn the companion path after liberating a country".into()
            });
        }
        self.validate_loadout(if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        })?;
        let loadout = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        if loadout.iter().any(|id| self.reserved_items.contains(id)) {
            return Err("Equipment is assigned to another deployed survivor or guard".into());
        }
        for id in loadout {
            let item = self.item(id).ok_or("Missing equipment")?;
            self.weapon_access(mode, item.definition_index())
                .map_err(str::to_owned)?;
        }
        self.reserved = Some(mode);
        Ok(())
    }
    pub fn grant_recruit_starters(&mut self) -> Result<[u64; 4], String> {
        self.next_item_id
            .checked_add(4)
            .filter(|_| self.next_item_id > 0)
            .ok_or("Persistent item identity space exhausted")?;
        let mut loadout = [0; 4];
        for (n, id) in loadout.iter_mut().enumerate() {
            *id = self.add_item(n)?;
            let item = self.items.last_mut().expect("just added");
            item.locked = true;
            item.protected = true;
        }
        Ok(loadout)
    }
    pub fn item_reserved(&self, id: u64) -> bool {
        self.reserved_items.contains(&id)
            || match self.reserved {
                Some(Mode::Personal) => self.player.loadout.contains(&id),
                Some(Mode::Npc) => self.npc.loadout.contains(&id),
                None => false,
            }
    }
    pub fn reserve_external(&mut self, loadout: [u64; 4]) -> Result<(), String> {
        self.validate_loadout(loadout)?;
        let main = match self.reserved {
            Some(Mode::Personal) => Some(self.player.loadout),
            Some(Mode::Npc) => Some(self.npc.loadout),
            None => None,
        };
        if loadout.iter().any(|id| {
            self.reserved_items.contains(id) || main.is_some_and(|items| items.contains(id))
        }) {
            return Err("A unique item is already reserved by another deployment".into());
        }
        self.reserved_items.extend(loadout);
        Ok(())
    }
    pub fn release_external(&mut self, loadout: [u64; 4]) {
        self.reserved_items.retain(|id| !loadout.contains(id));
    }
    pub fn finish_attempt(&mut self) {
        if self.reserved == Some(Mode::Npc) {
            self.npc.attempts += 1;
        }
        self.reserved = None;
        self.attempts += 1;
    }
    /// Bosses already beaten join ordinary wave pools; pre-history saves derive their clear flags.
    pub fn unlocked_bosses(&self) -> Vec<crate::content::BossKind> {
        let mut unlocked = self.defeated_bosses.clone();
        for (index, cleared) in self.first_clears.iter().enumerate() {
            let boss = content().levels[index].boss;
            if *cleared && !unlocked.contains(&boss) {
                unlocked.push(boss);
            }
        }
        unlocked.sort_by_key(|boss| *boss as u8);
        unlocked.dedup();
        unlocked
    }
    pub fn first_clear(&mut self, level: usize, mode: Mode) -> bool {
        if level >= 5 {
            return false;
        }
        self.migrate_threat_history();
        let completion_xp = self.completion_xp_preview();
        self.award_player_xp(completion_xp);
        let boss = content().levels[level].boss;
        if !self.defeated_bosses.contains(&boss) {
            self.defeated_bosses.push(boss);
        }
        let first = !self.first_clears[level];
        self.first_clears[level] = true;
        self.unlocked = self.unlocked.max((level + 2).min(5));
        self.victories = self.victories.saturating_add(1);
        if mode == Mode::Npc {
            self.npc.victories += 1;
            if !self.npc.first_clears[level] {
                self.npc.first_clears[level] = true;
            }
        }
        self.claim_objectives();
        first
    }
    pub fn blueprint_available(&self, kind: StructureKind) -> bool {
        if self.legacy_features {
            return self.first_clears.iter().filter(|x| **x).count()
                >= content().structure(kind).unlock_clear;
        }
        skills().nodes.iter().find(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::Blueprint(value) if *value==kind))).is_some_and(|node|self.has_node(&node.id)||(self.victories>=node.minimum_victories&&self.liberated_count()>=node.minimum_liberated&&node.prerequisites.iter().all(|id|self.has_node(id))))
    }
    pub fn purchase_blueprint(&mut self, kind: StructureKind) -> Result<(), String> {
        if !self.legacy_features {
            let id=skills().nodes.iter().find(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::Blueprint(value) if *value==kind))).ok_or("Blueprint node missing")?.id.clone();
            return self.purchase_node(&id);
        }
        self.preparation()?;
        if self.blueprints[kind] {
            return Err("Blueprint already owned".into());
        }
        if !self.blueprint_available(kind) {
            return Err(format!(
                "Clear {} stages to unlock",
                content().structure(kind).unlock_clear
            ));
        }
        self.spend(content().structure(kind).blueprint_price)?;
        self.blueprints[kind] = true;
        Ok(())
    }
    pub fn upgrade_structure(&mut self, kind: StructureKind) -> Result<(), String> {
        if !self.legacy_features {
            let rank = self.structure_upgrades[kind]
                .checked_add(1)
                .filter(|rank| *rank <= 10)
                .ok_or("Structure rank complete")?;
            let id=skills().nodes.iter().find(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::StructureRank{kind:value,rank:value_rank} if *value==kind&&*value_rank==rank))).ok_or("Structure rank complete")?.id.clone();
            return self.purchase_node(&id);
        }
        self.preparation()?;
        if !self.blueprints[kind] {
            return Err("Purchase the blueprint first".into());
        }
        let rank = self.structure_upgrades[kind];
        if rank >= 3 {
            return Err("Structure upgrade complete".into());
        }
        self.spend(40 + 40 * rank as u64)?;
        self.structure_upgrades[kind] += 1;
        Ok(())
    }
    pub fn layout_cost(&self) -> u32 {
        self.layout
            .iter()
            .map(|p| content().structure(p.kind).supply)
            .sum()
    }
    pub fn layout_for(&self, country: &str) -> &[Placement] {
        self.country_layouts.get(country).map_or(&[], Vec::as_slice)
    }
    /// A plan is a persistent intention, so it may exceed a particular stage's supply.
    /// Saving it is legal during combat and does not spend or generate resources.
    pub fn save_country_layout(
        &mut self,
        country: &str,
        layout: Vec<Placement>,
    ) -> Result<(), String> {
        if crate::world::world().country(country).is_none() {
            return Err("Unknown country for defensive layout".into());
        }
        self.validate_layout(&layout)?;
        self.country_layouts.insert(country.to_owned(), layout);
        Ok(())
    }
    pub fn clear_country_layout(&mut self, country: &str) -> Result<(), String> {
        if crate::world::world().country(country).is_none() {
            return Err("Unknown country for defensive layout".into());
        }
        self.country_layouts.remove(country);
        Ok(())
    }
    fn validate_layout(&self, layout: &[Placement]) -> Result<(), String> {
        let mut cells = BTreeSet::new();
        if layout.len() > usize::from(crate::sim::MAX_LANES) * 5
            || layout.iter().any(|p| {
                p.lane >= usize::from(crate::sim::MAX_LANES)
                    || p.column >= 5
                    || !self.blueprints[p.kind]
                    || !cells.insert((p.lane, p.column))
            })
        {
            return Err("Invalid saved defensive layout".into());
        }
        Ok(())
    }
    pub fn save_layout(&mut self, layout: Vec<Placement>, level: usize) -> Result<(), String> {
        self.preparation()?;
        let def = content().levels.get(level).ok_or("Unknown stage")?;
        let mut cells = std::collections::BTreeSet::new();
        let mut cost = 0;
        for p in &layout {
            if p.lane >= usize::from(self.lane_count()) || p.column >= 5 {
                return Err("Build only in the eleven-row, five-column left-half grid".into());
            }
            if !self.blueprints[p.kind] {
                return Err("Blueprint not owned".into());
            }
            if !cells.insert((p.lane, p.column)) {
                return Err("One structure per cell".into());
            }
            cost += content().structure(p.kind).supply;
        }
        if cost > def.supply {
            return Err("Layout exceeds starting supply".into());
        }
        self.layout = layout;
        Ok(())
    }
    fn award_npc_points(&mut self, points: u32) -> u32 {
        let award = points.min(self.allowance().saturating_sub(self.npc.awarded));
        self.npc.awarded += award;
        self.npc.unspent += award;
        award
    }
    pub fn record_npc_progress(&mut self, hits: u64, ticks: u64) {
        self.npc.successful_hits = self.npc.successful_hits.saturating_add(hits);
        self.npc.combat_ticks = self.npc.combat_ticks.saturating_add(ticks);
        self.claim_objectives();
    }
    pub fn choose_start_country(&mut self, id: &str) -> Result<(), String> {
        self.preparation()?;
        self.world.choose_start(id)
    }
    pub fn enter_country(&mut self, id: &str) -> Result<(), String> {
        self.preparation()?;
        self.world.enter(id)
    }
    pub fn record_country_stage_clear(&mut self, stage: usize) -> Result<bool, String> {
        self.world.record_clear(stage)
    }
    /// Records a stage clear for `country` without changing the active country.
    pub fn record_country_stage_clear_in(
        &mut self,
        country: &str,
        stage: usize,
    ) -> Result<bool, String> {
        self.world.record_clear_in(country, stage)
    }
    pub fn claim_objectives(&mut self) {
        let allowance = self.allowance();
        self.npc.claim_objectives(allowance);
    }
    pub fn buy_training_point(&mut self) -> Result<(), String> {
        self.preparation()?;
        self.claim_objectives();
        if self.npc.awarded >= self.allowance() {
            return Err("Advancement allowance reached; clear a new stage".into());
        }
        self.spend(25)?;
        self.award_npc_points(1);
        Ok(())
    }
    pub fn train_npc(&mut self, training: NpcTraining) -> Result<(), String> {
        self.preparation()?;
        if training == NpcTraining::Strength && self.npc.st >= 20 {
            return Err("The supported human melee damage table ends at ST 20".into());
        }
        if training == NpcTraining::Intelligence {
            return Err(
                "IQ is informational in the supported combat scope; training is disabled".into(),
            );
        }
        if matches!(training, NpcTraining::HitPoints | NpcTraining::Fatigue) {
            return Err("Independent HP/FP purchases are outside the pinned Lite subset; train ST/HT instead".into());
        }
        let cost = training.cost();
        if self.npc.unspent < cost {
            return Err(format!("Requires {cost} NPC character points"));
        }
        self.npc.unspent -= cost;
        self.npc.investments[training] += 1;
        match training {
            NpcTraining::Strength => self.npc.st += 1,
            NpcTraining::Dexterity => {
                self.npc.dx += 1;
                self.npc.guns += 1;
                self.npc.melee += 1;
            }
            NpcTraining::Intelligence => {}
            NpcTraining::Health => self.npc.ht += 1,
            NpcTraining::Guns => self.npc.guns += 1,
            NpcTraining::Melee => self.npc.melee += 1,
            NpcTraining::HitPoints => self.npc.extra_hp += 1,
            NpcTraining::Fatigue => self.npc.extra_fp += 1,
        };
        Ok(())
    }
    pub fn respec_npc(&mut self) -> Result<(), String> {
        self.preparation()?;
        self.npc.st = self.npc.creation.st;
        self.npc.dx = self.npc.creation.dx;
        self.npc.iq = self.npc.creation.iq;
        self.npc.ht = self.npc.creation.ht;
        self.npc.guns = self.npc.creation.guns;
        self.npc.melee = self.npc.creation.melee;
        self.npc.extra_hp = 0;
        self.npc.extra_fp = 0;
        self.npc.unspent = self.npc.awarded;
        self.npc.investments = KindMap::default();
        Ok(())
    }
    pub fn validate_npc(&self, npc: &Npc) -> Result<(), String> {
        use NpcTraining::*;
        let i = |training: NpcTraining| npc.investments[training];
        if !npc.creation.valid()
            || npc.investments.values().any(|n| *n > self.allowance())
            || i(Intelligence) != 0
            || i(HitPoints) != 0
            || i(Fatigue) != 0
        {
            return Err("Invalid NPC creation or supported advancement traits".into());
        }
        let base = &npc.creation;
        if !npc.stats().valid()
            || npc.st > 20
            || npc.st != base.st + i(Strength) as i32
            || npc.dx != base.dx + i(Dexterity) as i32
            || npc.iq != base.iq
            || npc.ht != base.ht + i(Health) as i32
            || npc.guns != base.guns + i(Dexterity) as i32 + i(Guns) as i32
            || npc.melee != base.melee + i(Dexterity) as i32 + i(Melee) as i32
            || npc.extra_hp != 0
            || npc.extra_fp != 0
        {
            return Err("NPC traits do not match their advancement ledger".into());
        }
        if npc.name.trim().is_empty()
            || npc.name.len() > 80
            || npc.recall_percent > 75
            || npc
                .first_clear_claimed
                .iter()
                .zip(npc.first_clears)
                .any(|(claimed, clear)| *claimed > 2 || (*claimed > 0 && !clear))
            || (npc.objectives_claimed[0] && npc.successful_hits < 20)
            || (npc.objectives_claimed[1] && npc.combat_ticks < 7200)
        {
            return Err("Invalid NPC identity, policy or achievement ledger".into());
        }
        if npc
            .first_clears
            .iter()
            .zip(self.first_clears)
            .any(|(personal, shared)| *personal && !shared)
        {
            return Err("NPC achievement precedes its campaign milestone".into());
        }
        let spent = npc
            .investments
            .entries()
            .try_fold(0u32, |total, (training, n)| {
                n.checked_mul(training.cost())
                    .and_then(|cost| total.checked_add(cost))
            })
            .ok_or("Invalid advancement investment total")?;
        if npc.awarded > self.allowance() || npc.unspent.checked_add(spent) != Some(npc.awarded) {
            return Err("Invalid advancement ledger".into());
        }
        if npc.first_clear_claimed.iter().any(|n| *n > 2) {
            return Err("Invalid NPC achievement ledger".into());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.balance_settings.validate()?;
        if self.next_run_id == 0 || self.run_history.windows(2).any(|p| p[0].id >= p[1].id) {
            return Err("Invalid run history sequence".into());
        }
        for record in &self.run_history {
            record.validate()?;
            if record.id == 0 || record.id >= self.next_run_id {
                return Err("Invalid persistent run identity".into());
            }
        }
        if self.journey_revision > 1 {
            return Err("Unsupported journey state revision".into());
        }
        if self.threat_revision > 1 {
            return Err("Unsupported victory-threat revision".into());
        }
        self.world.validate()?;
        let mut defeated_bosses = std::collections::BTreeSet::new();
        if self.defeated_bosses.len() > 3
            || self
                .defeated_bosses
                .iter()
                .any(|boss| !defeated_bosses.insert(*boss as u8))
        {
            return Err("Boss defeat history must contain at most three unique boss kinds".into());
        }
        if self.items.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err("Armory identities must remain in increasing creation order".into());
        }
        let frontier = self
            .first_clears
            .iter()
            .rposition(|clear| *clear)
            .map_or(1, |index| (index + 2).min(content().levels.len()));
        if self.unlocked != frontier {
            return Err("Invalid campaign frontier".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for item in &self.items {
            if item.id == 0
                || !(1..=64).contains(&item.level)
                || (item.protected && (item.level != 1 || item.quality.is_some()))
                || item.id >= self.next_item_id
                || item.definition.index().is_none()
                || !ids.insert(item.id)
                || (item.protected
                    && (!item.locked
                        || (item.definition_index() >= 4
                            && !(item.definition().tier == 0
                                && item.definition().slot == Slot::Ranged
                                && WeaponSpec::from_item(item.definition()).class
                                    == WeaponClass::Pistol))))
            {
                return Err("Invalid or duplicate owned item".into());
            }
        }
        for n in 0..4 {
            if !self.items.iter().any(|i| {
                i.id == n + 1 && i.definition_index() == n as usize && i.protected && i.locked
            }) {
                return Err("Protected starter set is missing".into());
            }
        }
        self.validate_loadout(self.player.loadout)?;
        self.validate_loadout(self.npc.loadout)?;
        let mut reserved = std::collections::BTreeSet::new();
        for id in &self.reserved_items {
            if !reserved.insert(*id) || self.item(*id).is_none() {
                return Err("Invalid external equipment reservation".into());
            }
        }
        if self.player.level == 0
            || self.player.body.values().try_fold(
                self.player
                    .points
                    .checked_add(self.player.legacy_graph_points)
                    .ok_or("Invalid legacy body point ledger")?,
                |total, n| total.checked_add(*n),
            ) != Some(if self.legacy_features {
                self.player.level - 1
            } else {
                0
            })
        {
            return Err("Invalid player development ledger".into());
        }
        if self
            .player
            .body
            .values()
            .any(|points| *points > self.body_cap())
            || self.player.xp >= self.player.next_level_xp()
        {
            return Err("Invalid player body cap or unprocessed XP".into());
        }
        let mut seen_skills = std::collections::BTreeSet::new();
        if self
            .skills
            .iter()
            .any(|skill| !seen_skills.insert(*skill as u8))
        {
            return Err("Duplicate skill ownership".into());
        }
        for id in &self.skill_nodes {
            let node = skills()
                .node(id)
                .ok_or_else(|| format!("Unknown owned skill node: {id}"))?;
            if node.prerequisites.iter().any(|p| !self.has_node(p)) {
                return Err(format!("Missing prerequisite for owned skill {id}"));
            }
        }
        for kind in StructureKind::ALL {
            let rank = self.structure_upgrades[kind];
            if rank > 10
                || (rank > 0 && !self.blueprints[kind])
                || (self.blueprints[kind] && !self.blueprint_available(kind))
            {
                return Err("Invalid blueprint milestone or structure upgrade".into());
            }
        }
        self.validate_layout(&self.layout)?;
        for (country, layout) in &self.country_layouts {
            if crate::world::world().country(country).is_none() {
                return Err("Unknown country for defensive layout".into());
            }
            self.validate_layout(layout)?;
        }
        let mut preset_names = std::collections::BTreeSet::new();
        for preset in &self.presets {
            if preset.name.trim().is_empty()
                || preset.name.len() > 48
                || !preset_names.insert(&preset.name)
            {
                return Err("Invalid or duplicate loadout preset name".into());
            }
            for (slot, id) in preset.loadout.into_iter().enumerate() {
                if id == 0
                    || id >= self.next_item_id
                    || self
                        .item(id)
                        .is_some_and(|i| i.definition().slot.index() != slot)
                {
                    return Err("Invalid loadout preset reference".into());
                }
            }
        }
        self.validate_npc(&self.npc)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_purchases_stack_and_reject_unmet_prerequisites_without_spending() {
        let mut c = Campaign {
            coins: 500,
            ..Campaign::default()
        };
        let before = c.clone();
        assert!(c.purchase_node("stopping_power").is_err());
        assert_eq!(c, before);
        assert!(c.purchase_node("pistol_training").is_err());
        assert_eq!(c, before);
        c.purchase_node("steady_hands").unwrap();
        c.purchase_node("stopping_power").unwrap();
        c.purchase_node("quick_reload").unwrap();
        c.purchase_node("rapid_cycle").unwrap();
        c.purchase_node("combat_reload").unwrap();
        c.purchase_node("field_fitness").unwrap();
        let p = c.profile(Mode::Personal);
        assert_eq!(p.personal.damage, 25);
        assert_eq!(p.personal.health, 135);
        assert!((p.personal.reload - 2.1 * 0.8 * 0.85).abs() < 0.001);
        assert!((p.personal.cadence - 0.42 * 0.85).abs() < 0.001);
        let before = c.clone();
        assert!(c.purchase_node("steady_hands").is_err());
        assert_eq!(c, before);
        c.validate().unwrap();
    }
    #[test]
    fn fresh_personal_weapons_require_both_owned_equipment_and_class_training() {
        let mut c = Campaign {
            coins: 500,
            ..Campaign::default()
        };
        assert_eq!(c.profile(Mode::Personal).weapon.class, WeaponClass::Pistol);
        assert!(c.has_node("pistol_training"));
        assert!(!c.can_use_weapon(Mode::Personal, 4));
        let owned = c.add_item(4).unwrap();
        let before = c.clone();
        assert!(c.equip(Mode::Personal, owned).is_err());
        assert_eq!(c, before);
        c.purchase_node("quick_reload").unwrap();
        c.purchase_node("rifle_training").unwrap();
        assert!(c.equip(Mode::Personal, 999_999).is_err());
        c.equip(Mode::Personal, owned).unwrap();
        assert_eq!(c.player.loadout[0], owned);
        c.validate().unwrap();
    }
    #[test]
    fn old_campaigns_keep_equipped_rifles_and_explicit_legacy_armory_compatibility() {
        let mut c = Campaign::default();
        c.items.retain(|i| i.id != 5);
        c.player.loadout[0] = 1;
        c.presets[0].loadout[0] = 1;
        c.next_item_id = 5;
        c.skills = vec![Skill::QuickReload];
        let mut json = serde_json::to_value(&c).unwrap();
        json.as_object_mut().unwrap().remove("skill_nodes");
        json.as_object_mut()
            .unwrap()
            .remove("legacy_weapon_training");
        let mut restored: Campaign = serde_json::from_value(json).unwrap();
        restored.validate().unwrap();
        assert!(restored.legacy_weapon_training);
        assert!(restored.has_node("pistol_training"));
        assert!(restored.has_node("quick_reload"));
        assert_eq!(restored.player.loadout[0], 1);
        assert!(content().items[0].npc_only);
        assert!(restored.can_use_weapon(Mode::Personal, 0));
        restored.equip(Mode::Personal, 1).unwrap();
        assert!(restored.can_use_weapon(Mode::Personal, 4));
        let id = restored.add_item(4).unwrap();
        restored.equip(Mode::Personal, id).unwrap();
        let before = restored.clone();
        assert!(restored.purchase_node("quick_reload").is_err());
        assert_eq!(restored, before);
    }
    #[test]
    fn pistol_cap_survives_damage_upgrades_and_weak_point_bonuses() {
        let mut c = Campaign {
            coins: 500,
            ..Campaign::default()
        };
        c.purchase_node("steady_hands").unwrap();
        c.purchase_node("stopping_power").unwrap();
        let p = c.profile(Mode::Personal);
        for enemy in &content().enemies {
            assert!(p.weapon.hit_cap(enemy.health) < enemy.health);
        }
        assert!(
            p.personal.damage as f32 * (1.0 + p.personal.weak_bonus) > p.weapon.hit_cap(23) as f32
        );
    }
    #[test]
    fn default_campaign_and_profiles_are_valid() {
        let c = Campaign::default();
        assert!(c.validate().is_ok());
        assert_eq!(c.profile(Mode::Personal).personal.magazine, 12);
        assert_eq!(c.profile(Mode::Npc).ranged_skill, 16);
    }
    #[test]
    fn npc_kills_do_not_duplicate_player_development() {
        let mut c = Campaign {
            legacy_features: true,
            ..Campaign::default()
        };
        c.reward(10, 100, Mode::Npc);
        assert_eq!(c.player.xp, 0);
        assert_eq!(c.coins, 10);
        c.reward(0, 20, Mode::Personal);
        assert_eq!(c.player.level, 2);
        assert_eq!(c.player.points, 1);
    }
    #[test]
    fn training_cannot_reset_allowance() {
        let mut c = Campaign {
            coins: 1000,
            ..Campaign::default()
        };
        for _ in 0..12 {
            c.buy_training_point().unwrap();
        }
        c.train_npc(NpcTraining::Guns).unwrap();
        assert_eq!(c.npc.guns, 17);
        c.respec_npc().unwrap();
        assert_eq!(c.npc.awarded, 12);
        assert_eq!(c.npc.unspent, 12);
        assert!(c.buy_training_point().is_err());
        assert!(c.validate().is_ok());
    }
    #[test]
    fn earned_objectives_precede_paid_training() {
        let mut c = Campaign {
            coins: 1000,
            ..Campaign::default()
        };
        for _ in 0..12 {
            c.buy_training_point().unwrap();
        }
        c.record_npc_progress(20, 7200);
        assert_eq!(c.npc.objectives_claimed, [false, false]);
        assert!(c.first_clear(0, Mode::Personal));
        assert_eq!(c.npc.objectives_claimed, [true, true]);
        assert_eq!(c.npc.awarded, 14);
    }
    #[test]
    fn reserved_equipment_cannot_be_sold_or_changed() {
        let mut c = Campaign::default();
        c.reward(65, 0, Mode::Personal);
        c.purchase_node("quick_reload").unwrap();
        c.purchase_node("rifle_training").unwrap();
        let id = c.add_item(4).unwrap();
        c.equip(Mode::Personal, id).unwrap();
        c.reserve(Mode::Personal).unwrap();
        assert!(c.sell(id).is_err());
        assert!(c.equip(Mode::Personal, 1).is_err());
        c.finish_attempt();
        assert_eq!(c.sell(id).unwrap(), 22);
        assert_eq!(c.player.loadout[0], 5);
    }
    #[test]
    fn starter_set_survives_every_sale() {
        let mut c = Campaign::default();
        c.add_item(4).unwrap();
        assert_eq!(c.sell_unprotected().unwrap(), 22);
        assert_eq!(c.items.len(), 5);
        for id in 1..=5 {
            assert!(c.sell(id).is_err());
        }
        assert!(c.validate().is_ok());
    }
    #[test]
    fn presets_reference_owned_items_and_never_duplicate() {
        let mut c = Campaign::default();
        c.reward(65, 0, Mode::Personal);
        c.purchase_node("quick_reload").unwrap();
        c.purchase_node("rifle_training").unwrap();
        let id = c.add_item(4).unwrap();
        c.equip(Mode::Personal, id).unwrap();
        c.save_preset("Rifle".into(), Mode::Personal).unwrap();
        c.apply_preset(1, Mode::Npc).unwrap();
        assert_eq!(c.items.len(), 6);
        assert_eq!(c.sale_preview(id).unwrap().affected_presets, vec!["Rifle"]);
        c.sell(id).unwrap();
        assert!(c.apply_preset(1, Mode::Personal).is_err());
    }
    #[test]
    fn invalid_layout_spends_nothing() {
        let mut c = Campaign {
            blueprints: KindMap::filled(true),
            ..Campaign::default()
        };
        let coins = c.coins;
        assert!(
            c.save_layout(
                vec![Placement {
                    kind: StructureKind::Wall,
                    lane: 11,
                    column: 0
                }],
                0
            )
            .is_err()
        );
        assert!(c.layout.is_empty());
        assert_eq!(c.coins, coins);
        assert!(
            c.save_layout(
                vec![
                    Placement {
                        kind: StructureKind::Wall,
                        lane: 0,
                        column: 0
                    };
                    2
                ],
                0
            )
            .is_err()
        );
    }
    #[test]
    fn all_loot_is_retained_without_capacity_limit() {
        let mut c = Campaign::default();
        for _ in 0..1000 {
            c.add_item(4).unwrap();
        }
        assert_eq!(c.items.len(), 1005);
        assert!(c.validate().is_ok());
    }
    #[test]
    fn npc_creation_budget_uses_pinned_attribute_and_skill_costs() {
        use crate::rules::{Difficulty, skill_point_cost};
        let c = Campaign::default();
        assert_eq!(
            c.npc.stats().point_cost()
                + skill_point_cost(c.npc.guns - c.npc.dx, Difficulty::Easy).unwrap() as i32
                + skill_point_cost(c.npc.melee - c.npc.dx, Difficulty::Average).unwrap() as i32,
            76
        );
    }
    #[test]
    fn npc_first_clear_points_wait_when_capacity_is_full() {
        let mut c = Campaign {
            coins: 10000,
            ..Campaign::default()
        };
        c.first_clears[0] = true;
        c.unlocked = 2;
        for _ in 0..20 {
            c.buy_training_point().unwrap();
        }
        assert!(!c.first_clear(0, Mode::Npc));
        assert_eq!(c.npc.first_clear_claimed[0], 0);
        c.first_clear(1, Mode::Personal);
        assert_eq!(c.npc.first_clear_claimed[0], 2);
        assert_eq!(c.npc.awarded, 22);
        assert!(c.validate().is_ok());
    }
    #[test]
    fn human_hp_and_fp_have_supported_bounds() {
        let mut c = Campaign {
            coins: 10000,
            ..Campaign::default()
        };
        for _ in 0..12 {
            c.buy_training_point().unwrap();
        }
        assert!(c.train_npc(NpcTraining::HitPoints).is_err());
        assert!(c.train_npc(NpcTraining::Fatigue).is_err());
        c.train_npc(NpcTraining::Strength).unwrap();
        assert_eq!(c.npc.max_hp(), 11);
        assert!(c.validate().is_ok());
    }
    #[test]
    fn country_wrappers_preserve_local_clear_transactions() {
        let mut c = Campaign::default();
        let id = crate::world::world().countries[0].id.clone();
        c.choose_start_country(&id).unwrap();
        let count = c.world.active_country().unwrap().level_count();
        for stage in 0..count {
            c.record_country_stage_clear(stage).unwrap();
        }
        assert!(c.world.cleared.contains(&id));
        let progress = c.world.clone();
        assert!(!c.record_country_stage_clear(count - 1).unwrap());
        assert_eq!(c.world, progress);
        assert!(c.validate().is_ok());
    }
    #[test]
    fn preparation_choices_are_reserved_during_deployment() {
        let mut c = Campaign {
            coins: 1000,
            ..Campaign::default()
        };
        c.reserve(Mode::Personal).unwrap();
        assert!(c.purchase_skill(Skill::QuickReload).is_err());
        assert!(c.buy_training_point().is_err());
        let id = crate::world::world().countries[0].id.clone();
        assert!(c.choose_start_country(&id).is_err());
        assert_eq!(c.coins, 1000);
    }
    #[test]
    fn optional_market_covers_each_slot_before_every_gate() {
        for frontier in 1..=5 {
            for slot in Slot::ALL {
                assert!(content().items.iter().any(|i| i.slot == slot
                    && i.tier > 0
                    && i.tier <= frontier
                    && i.buy_value <= 350));
            }
        }
    }
    #[test]
    fn parallel_reservations_are_unique_across_all_defenders() {
        let mut c = Campaign::default();
        let loadout = c.grant_recruit_starters().unwrap();
        c.reserve_external(loadout).unwrap();
        assert!(c.reserve_external(loadout).is_err());
        assert!(c.sell(loadout[0]).is_err());
        assert!(c.equip(Mode::Personal, loadout[0]).is_err());
        c.reserve(Mode::Personal).unwrap();
        assert!(c.reserve_external(c.player.loadout).is_err());
        c.finish_attempt();
        c.release_external(loadout);
        assert!(c.reserved_items.is_empty());
        assert!(c.validate().is_ok());
    }
    #[test]
    fn personal_upgrades_never_transfer_to_npc() {
        let mut c = Campaign::default();
        let before = c.profile(Mode::Npc);
        c.player.body = KindMap::filled(5);
        c.skills = Skill::ALL.to_vec();
        assert_eq!(c.profile(Mode::Npc).npc, before.npc);
        assert_eq!(c.profile(Mode::Npc).ranged_skill, before.ranged_skill);
        assert!(!c.profile(Mode::Npc).heal);
        assert!(c.profile(Mode::Npc).engineering);
    }
    #[test]
    fn recommendations_are_attainable_and_comparison_reports_mode_specific_breakpoints() {
        let mut c = Campaign::default();
        assert_eq!(
            c.recommendation(Mode::Personal).action,
            RecommendationAction::ChooseCountry
        );
        c.choose_start_country("DEU").unwrap();
        c.reward(25, 0, Mode::Personal);
        let next = c.recommendation(Mode::Personal);
        assert_eq!(
            next.action,
            RecommendationAction::LearnNode("quick_reload".into())
        );
        assert!(next.affordable);
        assert_eq!(next.price, 25);
        c.purchase_skill(Skill::QuickReload).unwrap();
        let id = c.add_item(4).unwrap();
        assert_eq!(
            c.recommendation(Mode::Personal).action,
            RecommendationAction::LearnNode("rifle_training".into())
        );
        c.reward(40, 0, Mode::Personal);
        c.purchase_node("rifle_training").unwrap();
        assert_eq!(
            c.recommendation(Mode::Personal).action,
            RecommendationAction::Equip(id)
        );
        let personal = c.compare_item(Mode::Personal, id).unwrap();
        assert_eq!(personal.before.baseline_shots_to_kill, Some(3));
        assert_eq!(personal.after.baseline_shots_to_kill, Some(1));
        let npc = c.compare_item(Mode::Npc, id).unwrap();
        assert!(npc.after.expected_basic_damage > npc.before.expected_basic_damage);
        assert!(npc.after.baseline_shots_to_kill.is_none());
        assert_eq!(c.player.loadout[0], 5);
    }
    #[test]
    fn all_creation_archetypes_have_equal_validated_budgets_and_respec_to_their_own_base() {
        for index in 0..4 {
            let mut c = Campaign {
                coins: 1000,
                ..Campaign::default()
            };
            c.npc.creation = NpcCreation::archetype(index);
            c.respec_npc().unwrap();
            assert!(c.npc.creation.valid());
            assert_eq!(c.npc.creation.points(), Some(76));
            for _ in 0..12 {
                c.buy_training_point().unwrap();
            }
            c.train_npc(NpcTraining::Guns).unwrap();
            c.respec_npc().unwrap();
            assert_eq!(c.npc.guns, c.npc.creation.guns);
            assert_eq!(c.npc.awarded, 12);
            assert!(c.validate().is_ok());
        }
    }
    #[test]
    fn legacy_npc_creation_defaults_preserve_existing_76_point_survivor() {
        let mut json = serde_json::to_value(Campaign::default()).unwrap();
        json["npc"].as_object_mut().unwrap().remove("creation");
        let restored: Campaign = serde_json::from_value(json).unwrap();
        assert_eq!(restored, Campaign::default());
        restored.validate().unwrap();
    }
    #[test]
    fn malformed_development_cannot_overflow_or_bypass_caps() {
        let c = Campaign {
            player: Player {
                level: 7,
                points: 0,
                body: KindMap::from_fn(|a| if a == BodyAttribute::Vitality { 6 } else { 0 }),
                ..Campaign::default().player
            },
            ..Campaign::default()
        };
        assert!(c.validate().is_err());
        let mut npc = Campaign::default().npc;
        npc.investments[NpcTraining::Strength] = u32::MAX;
        let c = Campaign {
            npc,
            ..Campaign::default()
        };
        assert!(c.validate().is_err());
        let c = Campaign {
            skills: vec![Skill::QuickReload; 2],
            ..Campaign::default()
        };
        assert!(c.validate().is_err());
    }
    #[test]
    fn failed_identity_or_wallet_transactions_preserve_owned_items_and_coins() {
        let mut c = Campaign {
            coins: 1000,
            next_item_id: u64::MAX,
            ..Campaign::default()
        };
        let before = c.clone();
        assert!(c.buy_item(4).is_err());
        assert_eq!(c, before);
        c.next_item_id = 6;
        let id = c.add_item(4).unwrap();
        c.coins = u64::MAX;
        let before = c.clone();
        assert!(c.sell(id).is_err());
        assert_eq!(c, before);
    }
    #[test]
    fn npc_strength_improves_hp_and_reference_swing_without_changing_bullets() {
        let mut c = Campaign {
            coins: 1000,
            ..Campaign::default()
        };
        for _ in 0..10 {
            c.buy_training_point().unwrap();
        }
        let before = c.profile(Mode::Npc);
        c.train_npc(NpcTraining::Strength).unwrap();
        let after = c.profile(Mode::Npc);
        assert_eq!(after.max_hp, before.max_hp + 1);
        assert_eq!(after.npc_melee_damage_dice, before.npc_melee_damage_dice);
        assert_eq!(after.npc_melee_damage_add, before.npc_melee_damage_add + 1);
        assert_eq!(after.npc.damage_dice, before.npc.damage_dice);
        assert_eq!(after.npc.damage_add, before.npc.damage_add);
        c.validate().unwrap();
    }
    #[test]
    fn liberated_country_recommends_an_accessible_front_instead_of_repeating_the_final_stage() {
        let mut c = Campaign::default();
        c.choose_start_country("VAT").unwrap();
        for stage in 0..3 {
            c.record_country_stage_clear(stage).unwrap();
        }
        let next = c.recommendation(Mode::Personal);
        assert_eq!(next.action, RecommendationAction::ChooseCountry);
        assert!(next.detail.contains("Italy"));
        assert!(c.world.can_enter("ITA"));
    }
    #[test]
    fn comparison_expectation_applies_minimum_damage_before_averaging() {
        use crate::rules::{Damage, DamageKind};
        assert_eq!(
            mean_basic_damage(Damage {
                dice: 1,
                adds: -3,
                kind: DamageKind::Piercing
            }),
            1.5
        );
        assert_eq!(
            mean_basic_damage(Damage {
                dice: 1,
                adds: -3,
                kind: DamageKind::Crushing
            }),
            1.0
        );
    }
    #[test]
    fn bulk_sale_wallet_overflow_preserves_the_entire_batch() {
        let mut c = Campaign {
            coins: u64::MAX - 30,
            ..Campaign::default()
        };
        c.add_item(4).unwrap();
        c.add_item(4).unwrap();
        let before = c.clone();
        assert!(c.sell_unprotected().is_err());
        assert_eq!(c, before);
        let mut strategy = crate::strategy::StrategyState::default();
        assert!(strategy.sell_extras(&mut c).is_err());
        assert_eq!(c, before);
    }
}
