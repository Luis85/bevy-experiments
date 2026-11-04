use crate::content::{
    ItemDefId, ItemDefinition, Mode, NpcProfile, PersonalProfile, Policy, Slot, StructureKind,
    content,
};
use crate::kind_map::{KindMap, StableKind};
use crate::rules::Stats;
use crate::skill_catalog::{SkillModifiers, skills, unlocks};
use crate::weapons::{WeaponClass, WeaponSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod armory;
mod ledger;
mod loadout;
mod progression;
mod tree_view;
mod view;
mod world;
pub use armory::{Item, Preset};
pub use ledger::SaleQuote;
pub(crate) use loadout::mean_basic_damage;
pub use loadout::{
    DefenderProfile, ItemComparison, LoadoutMetrics, Recommendation, RecommendationAction,
};
use progression::legacy_training_default;
pub use progression::{BodyAttribute, NodePayment, Npc, NpcCreation, NpcTraining, Player, Skill};
pub use tree_view::{
    NodePath, NodeState, PreviewStat, Requirement, RequirementKind, StatDelta,
};
pub use view::{NodeStatus, OfferPreview, UpgradeFocus};
pub use world::Placement;

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
    /// Stable IDs of every level cleared at least once. The deployable
    /// frontier ([`Campaign::unlocked`]) is derived from it.
    pub first_clears: BTreeSet<String>,
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
            first_clears: BTreeSet::new(),
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
                first_clears: BTreeSet::new(),
                first_clear_claimed: BTreeMap::new(),
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
            first_clears: self.first_clears.clone(),
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
    pub fn choose_survivor(&mut self, id: crate::survivors::SurvivorId) -> Result<(), String> {
        self.preparation()?;
        if self.world.start.is_some() {
            return Err("Choose your survivor before selecting the starting country".into());
        }
        self.selected_survivor = Some(id);
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
        if self
            .first_clears
            .iter()
            .any(|id| content().level_index(id).is_none())
        {
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
        let first = content().levels[0].id.clone();
        c.first_clears.insert(first.clone());
        assert_eq!(c.unlocked(), 2);
        for _ in 0..20 {
            c.buy_training_point().unwrap();
        }
        assert!(!c.first_clear(0, Mode::Npc));
        assert_eq!(c.npc.first_clear_claimed.get(&first), None);
        c.first_clear(1, Mode::Personal);
        assert_eq!(c.npc.first_clear_claimed[&first], 2);
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
