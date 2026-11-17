//! Skills, body attributes, player and survivor development, level and lane progression.

use super::*;

pub(super) fn legacy_training_default() -> bool {
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
    /// Player-facing attribute name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Vitality => "Vitality",
            Self::Strength => "Strength",
            Self::Coordination => "Coordination",
            Self::Endurance => "Endurance",
            Self::Perception => "Perception",
        }
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
    /// The rows a survivor can train, in display order. IQ, HP and FP keep
    /// their variants and serde names (old saves name them), but
    /// `Game::apply` rejects them with `CommandError::TrainingUnsupported`.
    pub const TRAINABLE: [Self; 5] = [
        Self::Strength,
        Self::Dexterity,
        Self::Health,
        Self::Guns,
        Self::Melee,
    ];
    /// Whether this row is in [`Self::TRAINABLE`].
    pub fn is_trainable(self) -> bool {
        Self::TRAINABLE.contains(&self)
    }
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
    /// The highest level whose level-up coin grant has been paid (0 and 1
    /// both mean none). A ledger, at most `level`: settlement pays the levels
    /// above it, at the root only and never in a worker. v6 saves are marked
    /// paid up to their level, so nothing is owed retroactively.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub level_grant_paid: u32,
}

impl Player {
    pub fn level_cap() -> u32 {
        crate::progression::catalog().player_level_cap
    }
    pub fn is_max_level(&self) -> bool {
        self.level >= Self::level_cap()
    }
    pub fn next_level_xp(&self) -> u64 {
        crate::progression::xp_to_advance_from(self.level)
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
    /// Stable IDs of levels this defender cleared first.
    pub first_clears: BTreeSet<String>,
    /// Advancement points already granted per first-cleared level ID (at most
    /// two each); levels with nothing claimed are absent.
    #[serde(default)]
    pub first_clear_claimed: BTreeMap<String, u32>,
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
    /// The advancement level the awarded points reach: the smallest level
    /// `n` in 1..=10 whose allowance `12 + 8(n - 1)` covers `awarded`.
    pub fn advancement_level(&self) -> u8 {
        let max = crate::defenders::MAX_ADVANCEMENT_LEVEL;
        (1..=max)
            .find(|n| 12 + 8 * (u32::from(*n) - 1) >= self.awarded)
            .unwrap_or(max)
    }
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
        // Catalogue order decides who is paid first when the allowance is short.
        for level in &content().levels {
            let claimed = self
                .first_clear_claimed
                .get(&level.id)
                .copied()
                .unwrap_or(0);
            if self.first_clears.contains(&level.id) && claimed < 2 {
                let grant = award(self, 2 - claimed);
                if grant > 0 {
                    self.first_clear_claimed
                        .insert(level.id.clone(), claimed + grant);
                }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodePayment {
    Coins(u64),
    LegacyBodyPoints(u32),
}

/// The advancement cap level (CD5) for `clears` first clears and the
/// high-water liberation count `liberations`:
/// `min(10, max(1 + clears, 1 + ceil(3L/2)))`.
fn cap_level_for(clears: u32, liberations: u32) -> u32 {
    let max = u32::from(crate::defenders::MAX_ADVANCEMENT_LEVEL);
    let by_liberation = (1 + (3 * liberations).div_ceil(2)).min(max);
    let by_clears = (1 + clears).min(max);
    by_clears.max(by_liberation)
}

impl Campaign {
    pub fn npc_available(&self) -> bool {
        self.has_feature(crate::skill_catalog::CampaignFeature::Companion)
    }
    /// The highest advancement level the campaign allows its survivor
    /// defenders (CD5): `min(10, max(1 + first_clears, 1 + ceil(3L/2)))`
    /// with `L` the high-water liberation count. Never below the legacy level
    /// `1 + first_clears` up to the max level 10 (a catalogue of 10 or more
    /// levels stops there too), and equal to it with nothing liberated.
    pub fn advancement_cap_level(&self) -> u8 {
        self.cap_level().min(u32::from(u8::MAX)) as u8
    }
    /// What raises the advancement allowance now, as player text, naming
    /// only what would raise the cap level from here: a new stage clear
    /// while the first-clear term carries the cap, a liberation while the
    /// liberation term does (W2-10), both when they tie; nothing at the max
    /// level. A new clear counts only while some catalogue level is still
    /// uncleared: country stages map onto the catalogue levels, so once all
    /// of them are first-cleared only liberations raise the cap.
    pub fn allowance_raise_hint(&self) -> String {
        let max = crate::defenders::MAX_ADVANCEMENT_LEVEL;
        let cap = self.cap_level();
        if cap >= u32::from(max) {
            return format!("The allowance is at its highest, advancement level {max}.");
        }
        let clears = self.first_clears.len() as u32;
        let liberations = self.liberated_count();
        let new_clear_possible = content()
            .levels
            .iter()
            .any(|level| !self.first_clears.contains(&level.id));
        let clear_raises = new_clear_possible && cap_level_for(clears + 1, liberations) > cap;
        let liberation_raises = cap_level_for(clears, liberations + 1) > cap;
        match (liberation_raises, clear_raises) {
            (true, true) => "Liberate a country or clear a new stage to raise the allowance.",
            (true, false) => "Liberate a country to raise the allowance.",
            (false, true) => "Clear a new stage to raise the allowance.",
            (false, false) => "Liberate more countries to raise the allowance.",
        }
        .to_owned()
    }
    fn cap_level(&self) -> u32 {
        cap_level_for(self.first_clears.len() as u32, self.liberated_count())
    }
    /// Advancement points the campaign allows each survivor defender:
    /// 12 at level 1 and 8 more per level up to the cap level.
    pub fn allowance(&self) -> u32 {
        12 + 8 * (self.cap_level() - 1)
    }
    pub fn body_cap(&self) -> u32 {
        5 + 3 * self.first_clears.len() as u32
    }
    pub fn has_skill(&self, s: Skill) -> bool {
        self.skills.contains(&s) || self.skill_nodes.contains(s.id())
    }
    pub(super) fn owns_node_definition(&self, node: &crate::skill_catalog::SkillNode) -> bool {
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
    /// Catalogue countries currently liberated or whose unique-clear high
    /// water reached every stage (an outbreak reopening keeps the count).
    /// Looks up only the ids the ledgers name, through the country index, so
    /// the per-tick `allowance()` no longer scans the whole world; each
    /// catalogue country counts once, unknown ids never.
    pub fn liberated_count(&self) -> u32 {
        use crate::world::country_stage_count;
        let cleared = &self.world.cleared;
        let freed = cleared
            .iter()
            .enumerate()
            .filter(|(i, id)| country_stage_count(id).is_some() && !cleared[..*i].contains(id))
            .count();
        let by_high_water = self
            .world
            .clear_high_water
            .iter()
            .filter(|(id, count)| country_stage_count(id) == Some(**count) && !cleared.contains(id))
            .count();
        (freed + by_high_water) as u32
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
    pub fn progression_rank(&self) -> u32 {
        let victories = if self.threat_revision == 0 {
            self.victories.max(self.legacy_clear_rank() as u64)
        } else {
            self.victories
        };
        u32::try_from(victories).unwrap_or(u32::MAX)
    }
    /// The threat rank an expedition by `npc` plays at (W4-12, ruling R57):
    /// [`crate::defenders::expedition_threat_rank`] of its advancement level
    /// and the levels of its loadout items in this campaign's stash (a slot
    /// this stash does not hold counts as level 1). It is never above
    /// [`Self::progression_rank`], so an expedition is never harder than
    /// the main run and a saved expedition attempt keeps its rank within
    /// the campaign's clears.
    pub fn expedition_threat_rank(&self, npc: &Npc) -> u32 {
        let levels = npc
            .loadout
            .map(|id| self.item(id).map_or(1, |item| item.level));
        crate::defenders::expedition_threat_rank(npc.advancement_level(), levels)
            .min(self.progression_rank())
    }
    /// The item definition a rank `rank` milestone grants in a game with
    /// `seed` (design-progression-tree §7, ruling E7): a Ranged definition
    /// that is not NPC-only, of a weapon class the campaign is trained in
    /// (Pistol when it is trained in none), at tier `min(unlocked(), 4)`, or
    /// the highest lower tier from 1 up that has one. The pick is
    /// `keyed(seed, rank, 0, MILESTONE)` over the candidates in catalogue
    /// order, so it draws no RNG. None when no tier has a candidate.
    pub fn milestone_item_definition(&self, seed: u64, rank: u32) -> Option<usize> {
        let mut classes = self.modifiers().weapon_classes;
        if classes.is_empty() {
            classes.insert(WeaponClass::Pistol);
        }
        let items = &content().items;
        let key = crate::hash::keyed(seed, u64::from(rank), 0, crate::hash::tags::MILESTONE);
        let top = self.unlocked().min(4);
        (1..=top).rev().find_map(|tier| {
            let candidates: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.tier == tier
                        && item.slot == Slot::Ranged
                        && !item.npc_only
                        && classes.contains(&WeaponSpec::from_item(item).class)
                })
                .map(|(index, _)| index)
                .collect();
            (!candidates.is_empty()).then(|| candidates[(key % candidates.len() as u64) as usize])
        })
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
            .max(self.first_clears.len()) as u32
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
    /// The two-axis encounter snapshot a deploy freezes (design
    /// enemy-balance §1 and §3): threat `rank` scaled by the balance
    /// settings for `lanes` in `mode`, with the preset's extras folded in
    /// and, on a country stage at `front`, the front ramp folded into enemy
    /// health and arrival and the stage's boss rank stamped with its
    /// `boss_ranks` terms folded. `front` None
    /// (no country: GOLDEN and smoke) keeps the literal neutral ramp and a
    /// Sector boss. Pure.
    pub fn progression_for(
        &self,
        mode: Mode,
        rank: u32,
        lanes: u8,
        front: Option<crate::progression::FrontPosition>,
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
        let extras = self.balance_extras();
        result.fold_extras(extras);
        // The neutral ramp writes nothing and multiplies nothing, so a stage
        // without one is the rank's snapshot bit for bit.
        if let Some(ramp) = cfg.front_ramp(extras.front_ramp_pct, front) {
            result.front_ramp = ramp;
            result.enemy_health = (result.enemy_health * ramp).min(4096.0);
            result.arrival_multiplier = (result.arrival_multiplier * ramp).min(8.0);
        }
        // The stage's boss rank and its catalogue terms (rulings W-6 and
        // W-9): Regional health and reward; Capstone health, armor, reward,
        // damage and a transition. Without a country the boss is a Sector
        // boss and nothing is folded.
        if let Some(front) = front {
            let boss_rank = front.boss_rank();
            result.fold_boss_rank(boss_rank, cfg.boss_ranks.of(boss_rank));
        }
        Ok(result)
    }
    /// The next deploy's rank snapshot, before any front position (the
    /// briefing's rank-only forecast).
    pub fn deployment_progression(
        &self,
        mode: Mode,
    ) -> Result<crate::progression::ProgressionSnapshot, String> {
        self.progression_for(mode, self.progression_rank(), self.lane_count(), None)
    }
    /// Preview includes known completion XP only, not unearned future kill XP.
    /// Rank-only, like [`Self::deployment_progression`].
    pub fn next_victory_progression(
        &self,
        mode: Mode,
    ) -> Result<crate::progression::ProgressionSnapshot, String> {
        self.progression_for(
            mode,
            self.progression_rank().saturating_add(1),
            self.lane_count_after_victory(),
            None,
        )
    }
    pub fn shop_item_level(&self) -> u32 {
        crate::progression::catalog()
            .snapshot(self.progression_rank())
            .item_level
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
        self.grant_node(node);
        Ok(())
    }
    /// Record ownership of `node` and mirror its blueprint and structure-rank
    /// effects into the campaign ledgers. No preflight and no payment: the
    /// purchase path and read-only previews (on a clone) share this step.
    pub(super) fn grant_node(&mut self, node: &crate::skill_catalog::SkillNode) {
        self.skill_nodes.insert(node.id.clone());
        for effect in &node.effects {
            match effect {
                crate::skill_catalog::SkillEffect::Blueprint(kind) => self.blueprints[*kind] = true,
                crate::skill_catalog::SkillEffect::StructureRank { kind, rank } => {
                    self.structure_upgrades[*kind] = self.structure_upgrades[*kind].max(*rank)
                }
                _ => {}
            }
        }
    }
    pub fn node_payment(&self, id: &str) -> Result<NodePayment, String> {
        let node = skills().node(id).ok_or("Unknown skill node")?;
        if self.legacy_features && self.player.points>0 && node.effects.len()==1 && node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::BodyDevelopment{rank,..} if *rank<=self.body_cap())) {{self.player.legacy_graph_points.checked_add(1).ok_or("Legacy graph point ledger exhausted")?;Ok(NodePayment::LegacyBodyPoints(1))}}else{Ok(NodePayment::Coins(node.price))}
    }
    /// The first unmet catalogue gate (prerequisites, liberation, victories);
    /// shared by the purchase preflight and `node_status`.
    pub(crate) fn node_requirement_error(
        &self,
        node: &crate::skill_catalog::SkillNode,
    ) -> Option<String> {
        if node
            .prerequisites
            .iter()
            .any(|parent| !self.has_node(parent))
        {
            return Some("Learn the required prerequisite skills first".into());
        }
        if self.liberated_count() < node.minimum_liberated {
            return Some(format!(
                "Liberate {} countries first",
                node.minimum_liberated
            ));
        }
        if self.victories < node.minimum_victories {
            return Some(format!("Win {} defenses first", node.minimum_victories));
        }
        None
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
        if let Some(reason) = self.node_requirement_error(node) {
            return Err(reason);
        }
        if let NodePayment::Coins(price) = self.node_payment(id)?
            && self.coins < price
        {
            return Err(crate::coins::need_more(price - self.coins));
        }
        Ok(())
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
    pub(super) fn award_player_xp(&mut self, xp: u64) -> u64 {
        let credited = Self::award_xp_to(&mut self.player, xp, self.legacy_features);
        self.pay_neutral_level_grants();
        credited
    }
    /// Shim (W2-52, R16), inert once a grant is authored: a level-up whose
    /// grant is 0 coins owes nothing, so the ledger follows the level and a
    /// native v7 save owes no backlog (as migration step (c); level 1 writes
    /// nothing). `Game::settle_progression_rewards` (W2-22) pays the ledger
    /// at settlement; this shim keeps it current mid-battle and in workers
    /// while no grant is authored. The shipped grant is non-zero from level 2
    /// on (W2-54), so GOLDEN no longer exercises it; only
    /// `neutral_level_ups_keep_the_grant_ledger_paid` (a neutral rewards
    /// block) covers it.
    fn pay_neutral_level_grants(&mut self) {
        let player = &mut self.player;
        let mut owed = player.level_grant_paid.max(1).saturating_add(1)..=player.level;
        if !owed.is_empty() && owed.all(|level| crate::progression::level_grant(level) == 0) {
            player.level_grant_paid = player.level;
        }
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
    pub fn claim_objectives(&mut self) {
        let allowance = self.allowance();
        self.npc.claim_objectives(allowance);
    }
    /// Coins for one paid NPC training point (Mara or a recruit), scaled by
    /// threat: `camp_price(camp_prices.npc_training_base, progression_rank)`,
    /// 25 / 30 / 50 / 75 / 150 at rank 0 / 1 / 10 / 20 / 50+.
    pub fn npc_training_price(&self) -> u64 {
        let base = crate::progression::rewards().camp_prices.npc_training_base;
        crate::progression::camp_price(base, self.progression_rank())
    }
    /// Coins to recruit one survivor, scaled by threat from the world's
    /// `recruit_cost` base: 100 / 110 / 200 / 300 / 600 at rank 0 / 1 / 10 /
    /// 20 / 50+.
    pub fn recruit_price(&self) -> u64 {
        let base = crate::world::world().strategy_config.recruit_cost;
        crate::progression::camp_price(base, self.progression_rank())
    }
    pub fn buy_training_point(&mut self) -> Result<(), String> {
        self.preparation()?;
        self.claim_objectives();
        if self.npc.awarded >= self.allowance() {
            return Err(format!(
                "Advancement allowance reached. {}",
                self.allowance_raise_hint()
            ));
        }
        self.spend(self.npc_training_price())?;
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
            || npc.first_clear_claimed.iter().any(|(id, claimed)| {
                *claimed > 2 || (*claimed > 0 && !npc.first_clears.contains(id))
            })
            || (npc.objectives_claimed[0] && npc.successful_hits < 20)
            || (npc.objectives_claimed[1] && npc.combat_ticks < 7200)
        {
            return Err("Invalid NPC identity, policy or achievement ledger".into());
        }
        if !npc.first_clears.is_subset(&self.first_clears) {
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
        if npc.first_clear_claimed.values().any(|n| *n > 2) {
            return Err("Invalid NPC achievement ledger".into());
        }
        Ok(())
    }
}
