//! Country progression, blueprints, structure upgrades and saved layouts.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub kind: StructureKind,
    pub lane: usize,
    pub column: usize,
}

impl Campaign {
    /// Number of catalogue levels open for deployment: everything up to one
    /// past the furthest cleared level (in catalogue order), at least one.
    /// Derived from the ID-keyed clears, so it is never saved separately.
    pub fn unlocked(&self) -> usize {
        let levels = &content().levels;
        levels
            .iter()
            .rposition(|level| self.first_clears.contains(&level.id))
            .map_or(1, |index| (index + 2).min(levels.len()))
    }
    /// Bosses already beaten join ordinary wave pools; pre-history saves derive their clear flags.
    pub fn unlocked_bosses(&self) -> Vec<crate::content::BossKind> {
        let mut unlocked = self.defeated_bosses.clone();
        for level in &content().levels {
            if self.first_clears.contains(&level.id) && !unlocked.contains(&level.boss) {
                unlocked.push(level.boss);
            }
        }
        unlocked.sort_by_key(|boss| *boss as u8);
        unlocked.dedup();
        unlocked
    }
    /// A victory on catalogue `level` by the mode owner: the campaign
    /// victory, and for Npc Mara's credit. True when it was the level's
    /// first clear.
    pub fn first_clear(&mut self, level: usize, mode: Mode) -> bool {
        let Some(id) = content().levels.get(level).map(|l| l.id.clone()) else {
            return false;
        };
        self.migrate_threat_history();
        let completion_xp = self.completion_xp_preview();
        self.award_player_xp(completion_xp);
        let boss = content().levels[level].boss;
        if !self.defeated_bosses.contains(&boss) {
            self.defeated_bosses.push(boss);
        }
        let first = self.first_clears.insert(id.clone());
        self.victories = self.victories.saturating_add(1);
        if mode == Mode::Npc {
            self.npc.victories += 1;
            self.npc.first_clears.insert(id);
        }
        self.claim_objectives();
        first
    }
    /// The campaign side of a victory on `level`, whoever defended: the
    /// player's completion XP, the boss, the first clear, the victory count
    /// and Mara's objective claims. True when it was the first clear.
    pub(crate) fn commit_victory(&mut self, level: usize) -> bool {
        self.first_clear(level, Mode::Personal)
    }
    /// The defender side of a victory on `level` for an npc other than
    /// Mara (a recruit defending the main run): its victory, first clear and
    /// objective claims against the campaign allowance.
    pub(crate) fn credit_npc_victory(&self, npc: &mut Npc, level: usize) {
        let Some(level) = content().levels.get(level) else {
            return;
        };
        npc.victories += 1;
        npc.first_clears.insert(level.id.clone());
        npc.claim_objectives(self.allowance());
    }
    pub fn blueprint_available(&self, kind: StructureKind) -> bool {
        if self.legacy_features {
            return self.first_clears.len() >= content().structure(kind).unlock_clear;
        }
        skills().nodes.iter().find(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::Blueprint(value) if *value==kind))).is_some_and(|node|self.has_node(&node.id)||(self.victories>=node.minimum_victories&&self.liberated_count()>=node.minimum_liberated&&node.prerequisites.iter().all(|id|self.has_node(id))))
    }
    /// The skill node that grants `kind`'s blueprint.
    fn blueprint_node(kind: StructureKind) -> Option<&'static crate::skill_catalog::SkillNode> {
        skills().nodes.iter().find(|node|node.effects.iter().any(|effect|matches!(effect,crate::skill_catalog::SkillEffect::Blueprint(value) if *value==kind)))
    }
    /// What [`Self::purchase_blueprint`] charges: the price of the
    /// blueprint's skill node, or the content `blueprint_price` on a
    /// legacy-features campaign, which buys the blueprint outside the tree.
    pub fn blueprint_price(&self, kind: StructureKind) -> u64 {
        let legacy = content().structure(kind).blueprint_price;
        if self.legacy_features {
            return legacy;
        }
        Self::blueprint_node(kind).map_or(legacy, |node| node.price)
    }
    pub fn purchase_blueprint(&mut self, kind: StructureKind) -> Result<(), String> {
        if !self.legacy_features {
            let id = Self::blueprint_node(kind)
                .ok_or("Blueprint node missing")?
                .id
                .clone();
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
    pub(super) fn validate_layout(&self, layout: &[Placement]) -> Result<(), String> {
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
}
