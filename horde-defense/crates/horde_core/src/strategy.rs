//! Persistent late-campaign recruitment, garrisons and seeded plague pressure.
//! The caller advances this clock only while the global simulation is running.
use crate::campaign::{Campaign, Npc, NpcTraining, SaleRefs};
use crate::content::{Mode, Slot, content};
use crate::defenders::Archetype;
use crate::rules::{self, Attack, Damage, DamageKind, InjuryState, Rng};
use crate::world::world;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitReadiness {
    pub role: String,
    pub hp: i32,
    pub fp: i32,
    pub guns: i32,
    pub melee: i32,
    pub armor: i32,
    pub dodge: i32,
    pub magazine: u32,
    pub status: RecruitStatus,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecruitStatus {
    Ready,
    Guarding,
    Deployed,
    /// Appended (D5): the depth-0 main-run defender. Never on an expedition.
    Defending,
}
impl RecruitStatus {
    pub const ALL: [Self; 4] = [Self::Ready, Self::Guarding, Self::Deployed, Self::Defending];
    /// Player-facing status text. `Deployed` means away on an expedition;
    /// `Defending` means holding the main defense.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Guarding => "Guarding",
            Self::Deployed => "On expedition",
            Self::Defending => "Defending",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recruit {
    pub id: u64,
    pub name: String,
    pub origin: String,
    pub npc: Npc,
    pub status: RecruitStatus,
    /// Outbreaks this recruit contained while guarding. Saved because the
    /// guard assignment that saw them is removed.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub containments: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuardAssignment {
    pub recruit_id: u64,
    pub country_id: String,
    pub injury: InjuryState,
    pub ammo: u32,
    pub reload_turns: u32,
    #[serde(default)]
    pub ready_turns: u32,
    pub duty_ticks: u64,
    #[serde(default)]
    pub engagement_ticks: u64,
    pub victories: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutbreakOutcome {
    Active,
    Contained,
    Overrun,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outbreak {
    pub id: u64,
    pub country_id: String,
    pub started_tick: u64,
    pub deadline_tick: u64,
    pub strength: u32,
    pub enemy: InjuryState,
    pub outcome: OutbreakOutcome,
    #[serde(default)]
    pub ready_turns: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrategyState {
    pub roster: Vec<Recruit>,
    pub guards: Vec<GuardAssignment>,
    pub outbreaks: Vec<Outbreak>,
    pub tick: u64,
    pub rng: Rng,
    pub next_recruit_id: u64,
    pub next_outbreak_id: u64,
    pub peak_freed: usize,
    pub log: Vec<String>,
    /// Settled expedition outcomes dropped unacknowledged when the battle
    /// list was pruned (saturating).
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub folded_outcomes: u32,
}
/// A fresh game's strategy RNG seed (ASCII "PLAGUE"); [`StrategyState::seeded`]
/// mixes another seed into it.
const STRATEGY_RNG_SALT: u64 = 0x504c41475545;
impl Default for StrategyState {
    fn default() -> Self {
        Self {
            roster: vec![],
            guards: vec![],
            outbreaks: vec![],
            tick: 0,
            rng: Rng::new(STRATEGY_RNG_SALT),
            next_recruit_id: 1,
            next_outbreak_id: 1,
            peak_freed: 0,
            log: vec![],
            folded_outcomes: 0,
        }
    }
}
impl StrategyState {
    /// A fresh strategy layer whose RNG starts at `seed ^ STRATEGY_RNG_SALT`,
    /// so `seeded(0)` is the default. The pacing sweep's seam: seed index 0
    /// keeps the default RNG and every later index varies it.
    pub(crate) fn seeded(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed ^ STRATEGY_RNG_SALT),
            ..Self::default()
        }
    }
    pub fn readiness(&self, campaign: &Campaign, id: u64) -> Result<RecruitReadiness, String> {
        let recruit = self
            .roster
            .iter()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        campaign.validate_loadout(recruit.npc.loadout)?;
        let profile = campaign.profile_for_loadout(
            crate::content::Mode::Npc,
            recruit.npc.loadout,
            &recruit.npc,
        );
        let guard = self.guards.iter().find(|g| g.recruit_id == id);
        let hp = guard.map_or(profile.max_hp, |g| g.injury.hp);
        let fp = guard.map_or(profile.max_fp, |g| g.injury.fp);
        Ok(RecruitReadiness {
            role: recruit.npc.creation.archetype.clone(),
            hp,
            fp,
            guns: profile.ranged_skill + profile.npc.skill_bonus,
            melee: profile.melee_skill,
            armor: profile.npc.armor,
            dodge: profile.stats.dodge(),
            magazine: profile.npc.magazine,
            status: recruit.status,
            summary: format!(
                "{} · {} · HP {hp}/{} · FP {fp}/{} · DR {} · Guns {} · {} rounds",
                recruit.npc.creation.archetype,
                recruit.status.label(),
                profile.max_hp,
                profile.max_fp,
                profile.npc.armor,
                profile.ranged_skill + profile.npc.skill_bonus,
                profile.npc.magazine
            ),
        })
    }
    pub fn respec_recruit(&mut self, campaign: &mut Campaign, id: u64) -> Result<(), String> {
        let recruit = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if recruit.status != RecruitStatus::Ready {
            return Err("Recall this survivor before redistributing advancement".into());
        }
        let main = std::mem::replace(&mut campaign.npc, recruit.npc.clone());
        let result = campaign.respec_npc();
        recruit.npc = std::mem::replace(&mut campaign.npc, main);
        result
    }
    /// Save repair: an idle survivor slot whose item is no longer owned falls
    /// back to the real protected set issued at recruitment. Sales never need
    /// it since ruleset 2 (strict `SellItem` refuses recruit-equipped items);
    /// loading a save still runs it.
    pub fn reconcile_loadouts(&mut self, campaign: &Campaign) -> usize {
        let mut bundles: Vec<u64> = campaign
            .items
            .iter()
            .filter(|i| i.protected && i.definition_index() == 0 && i.id > 4)
            .map(|i| i.id)
            .collect();
        bundles.sort_unstable();
        let mut changed = 0;
        let mut order: Vec<u64> = self.roster.iter().map(|r| r.id).collect();
        order.sort_unstable();
        for recruit in &mut self.roster {
            if recruit.status != RecruitStatus::Ready {
                continue;
            }
            let ordinal = order.binary_search(&recruit.id).expect("known roster ID");
            let first = bundles.get(ordinal).copied().unwrap_or(1);
            for slot in 0..4 {
                if campaign.item(recruit.npc.loadout[slot]).is_none() {
                    let fallback = first + slot as u64;
                    recruit.npc.loadout[slot] = if campaign
                        .item(fallback)
                        .is_some_and(|i| i.protected && i.definition().slot.index() == slot)
                    {
                        fallback
                    } else {
                        slot as u64 + 1
                    };
                    changed += 1;
                }
            }
        }
        changed
    }
    /// The idle recruits' loadouts, the only sale references `Campaign`
    /// cannot see. Built once per sale (`Game::plan_sale`).
    pub fn sale_refs(&self) -> SaleRefs {
        let mut equipped = std::collections::BTreeMap::new();
        for recruit in &self.roster {
            for id in recruit.npc.loadout {
                equipped.entry(id).or_insert_with(|| recruit.name.clone());
            }
        }
        SaleRefs::new(equipped)
    }
    /// The batch quote over every owned item (0 while a defender is deployed).
    pub fn sell_extras_quote(&self, campaign: &Campaign) -> u64 {
        campaign.sell_quote(&self.sale_refs())
    }
    /// Sells every eligible item. While a defender is deployed nothing is
    /// eligible and this returns `Ok(0)`, as it always has.
    pub fn sell_extras(
        &mut self,
        campaign: &mut Campaign,
    ) -> Result<u64, crate::command::CommandError> {
        if campaign.reserved.is_some() {
            return Ok(0);
        }
        campaign.sell_unprotected(&self.sale_refs())
    }
    pub(crate) fn note(&mut self, text: String) {
        self.log.push(text);
        if self.log.len() > 80 {
            self.log.remove(0);
        }
    }
    fn progress(&self, campaign: &Campaign) -> usize {
        self.peak_freed.max(campaign.liberated_count() as usize)
    }
    pub fn recruitment_unlocked(&self, campaign: &Campaign) -> bool {
        campaign.has_feature(crate::skill_catalog::CampaignFeature::Recruitment)
            && self.progress(campaign) >= world().strategy_config.recruitment_unlock_freed
    }
    pub fn parallel_unlocked(&self, campaign: &Campaign) -> bool {
        campaign.has_feature(crate::skill_catalog::CampaignFeature::Parallel)
            && self.progress(campaign) >= world().strategy_config.parallel_unlock_freed
    }
    pub fn grand_strategy_unlocked(&self, campaign: &Campaign) -> bool {
        campaign.has_feature(crate::skill_catalog::CampaignFeature::Operations)
            && self.progress(campaign) >= world().strategy_config.strategy_unlock_freed
    }
    /// The two archetypes the next recruit from `country_id` may join as
    /// (decision 15, ruling E7): first today's rotation, then one of the
    /// other three keyed on `seed`, the next recruit id and the country.
    /// Always distinct; draws no RNG.
    pub fn recruit_offer_for(&self, seed: u64, country_id: &str) -> [Archetype; 2] {
        let first = Archetype::rotation(self.next_recruit_id);
        let key = crate::hash::keyed(
            seed,
            self.next_recruit_id,
            crate::hash::fnv1a(country_id.as_bytes()),
            crate::hash::tags::RECRUIT_OFFER,
        );
        let len = Archetype::ALL.len();
        let step = 1 + (key % (len as u64 - 1)) as usize;
        [first, Archetype::ALL[(first.index() + step) % len]]
    }
    /// Recruits the rotation archetype, as every recruit joined before the
    /// choice existed (`Command::Recruit` takes the same first offer).
    pub fn recruit(&mut self, campaign: &mut Campaign, country_id: &str) -> Result<u64, String> {
        self.recruit_as(
            campaign,
            country_id,
            Archetype::rotation(self.next_recruit_id),
        )
    }
    /// Recruits a survivor from a liberated country as `archetype`: its
    /// creation, starting stats and default policy. `Game::recruit_offer`
    /// decides which archetypes a country offers.
    pub fn recruit_as(
        &mut self,
        campaign: &mut Campaign,
        country_id: &str,
        archetype: Archetype,
    ) -> Result<u64, String> {
        if !self.recruitment_unlocked(campaign) {
            return Err("Liberate your first country to recruit local survivors".into());
        }
        if !campaign.world.cleared.iter().any(|id| id == country_id) {
            return Err("Recruit in a fully liberated country".into());
        }
        if self.roster.len() >= world().strategy_config.max_roster {
            return Err("Survivor roster is full".into());
        }
        if campaign.reserved.is_some() {
            return Err("Return the active defender to camp before recruiting".into());
        }
        let country = world()
            .country(country_id)
            .ok_or("Unknown recruitment country")?;
        let price = campaign.recruit_price();
        if campaign.coins < price {
            return Err(format!("Recruitment requires {price} coins"));
        }
        let next_id = self
            .next_recruit_id
            .checked_add(1)
            .filter(|_| self.next_recruit_id > 0)
            .ok_or("Survivor identity space exhausted")?;
        let loadout = campaign.grant_recruit_starters()?;
        campaign.coins -= price;
        let id = self.next_recruit_id;
        self.next_recruit_id = next_id;
        let name = crate::defenders::recruit_name(id);
        let mut npc = Campaign::default().npc;
        npc.name = name.clone();
        npc.loadout = loadout;
        npc.creation = archetype.creation();
        npc.st = npc.creation.st;
        npc.dx = npc.creation.dx;
        npc.iq = npc.creation.iq;
        npc.ht = npc.creation.ht;
        npc.guns = npc.creation.guns;
        npc.melee = npc.creation.melee;
        npc.policy = archetype.default_policy();
        self.roster.push(Recruit {
            id,
            name: name.clone(),
            origin: country_id.into(),
            npc,
            status: RecruitStatus::Ready,
            containments: 0,
        });
        self.peak_freed = self.peak_freed.max(campaign.liberated_count() as usize);
        self.note(format!(
            "{name} joined from {} · independent protected equipment issued",
            country.name
        ));
        Ok(id)
    }
    pub fn reserve_recruit(&mut self, id: u64) -> Result<Npc, String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Ready {
            return Err("Survivor is already deployed or guarding a country".into());
        }
        r.status = RecruitStatus::Deployed;
        Ok(r.npc.clone())
    }
    pub fn release_recruit(&mut self, id: u64, npc: Npc) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Deployed {
            return Err("Survivor has no active deployment".into());
        }
        r.name = npc.name.clone();
        r.npc = npc;
        r.status = RecruitStatus::Ready;
        Ok(())
    }
    /// Seats a Ready recruit as the main-run defender (Ready to Defending).
    /// `reserve_recruit` / `release_recruit` stay expedition-only (Deployed).
    pub fn reserve_defender(&mut self, id: u64) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Ready {
            return Err("Survivor is already on duty".into());
        }
        r.status = RecruitStatus::Defending;
        Ok(())
    }
    /// Returns the main-run defender to the roster (Defending to Ready). The
    /// roster entry is the battle's npc, so there is nothing to copy back.
    pub fn release_defender(&mut self, id: u64) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Defending {
            return Err("Survivor is not defending the main run".into());
        }
        r.status = RecruitStatus::Ready;
        Ok(())
    }
    pub fn equip_recruit(
        &mut self,
        campaign: &Campaign,
        id: u64,
        item_id: u64,
    ) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Ready {
            return Err("Recall this survivor before changing equipment".into());
        }
        if campaign.item_reserved(item_id) {
            return Err("Equipment is reserved by another deployed survivor or guard".into());
        }
        let item = campaign.item(item_id).ok_or("Item is no longer owned")?;
        r.npc.loadout[item.definition().slot.index()] = item_id;
        Ok(())
    }
    pub fn train_recruit(
        &mut self,
        campaign: &mut Campaign,
        id: u64,
        training: NpcTraining,
    ) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Ready {
            return Err("Recall this survivor before training".into());
        }
        let main = std::mem::replace(&mut campaign.npc, r.npc.clone());
        let result = campaign.train_npc(training);
        r.npc = std::mem::replace(&mut campaign.npc, main);
        result
    }
    pub fn buy_training_point(&mut self, campaign: &mut Campaign, id: u64) -> Result<(), String> {
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if r.status != RecruitStatus::Ready {
            return Err("Recall this survivor before training".into());
        }
        let main = std::mem::replace(&mut campaign.npc, r.npc.clone());
        let result = campaign.buy_training_point();
        r.npc = std::mem::replace(&mut campaign.npc, main);
        result
    }
    pub fn assign_guard(
        &mut self,
        campaign: &mut Campaign,
        id: u64,
        country_id: &str,
    ) -> Result<(), String> {
        let n = self.guard_verdict(campaign, id, country_id)?;
        let r = &mut self.roster[n];
        campaign.reserve_external(r.npc.loadout)?;
        let rifle = item_definition(campaign, &r.npc, Slot::Ranged);
        let profile = campaign.profile_for_loadout(Mode::Npc, r.npc.loadout, &r.npc);
        self.guards.push(GuardAssignment {
            recruit_id: id,
            country_id: country_id.into(),
            injury: InjuryState::new(profile.max_hp, profile.max_fp),
            ammo: rifle.npc.magazine,
            reload_turns: 0,
            ready_turns: 0,
            duty_ticks: 0,
            engagement_ticks: 0,
            victories: 0,
        });
        r.status = RecruitStatus::Guarding;
        let name = r.name.clone();
        self.note(format!("{} assigned to guard {}", name, country_id));
        Ok(())
    }
    /// `assign_guard`'s verdict without the assignment, in its order:
    /// garrisons unlocked, the country liberated, room in its garrison, a
    /// known Ready recruit whose items no other deployment holds. Returns
    /// the recruit's roster index.
    fn guard_verdict(
        &self,
        campaign: &Campaign,
        id: u64,
        country_id: &str,
    ) -> Result<usize, String> {
        if !self.grand_strategy_unlocked(campaign) {
            return Err("Liberate more countries to unlock strategic garrisons".into());
        }
        if !campaign.world.cleared.iter().any(|c| c == country_id) {
            return Err("Guards protect fully liberated countries".into());
        }
        if self
            .guards
            .iter()
            .filter(|g| g.country_id == country_id)
            .count()
            >= world().strategy_config.max_guards_per_country
        {
            return Err("Country garrison is at capacity".into());
        }
        let n = self
            .roster
            .iter()
            .position(|r| r.id == id)
            .ok_or("Unknown survivor")?;
        if self.roster[n].status != RecruitStatus::Ready {
            return Err("Survivor is already on duty".into());
        }
        campaign.check_external(self.roster[n].npc.loadout)?;
        Ok(n)
    }
    /// The roster recruits, by id, that `assign_guard` would accept for
    /// `country_id` now. Pure.
    pub fn eligible_guards(&self, campaign: &Campaign, country_id: &str) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .roster
            .iter()
            .map(|r| r.id)
            .filter(|id| self.guard_verdict(campaign, *id, country_id).is_ok())
            .collect();
        ids.sort_unstable();
        ids
    }
    pub fn release_guard(&mut self, campaign: &mut Campaign, id: u64) -> Result<(), String> {
        let n = self
            .guards
            .iter()
            .position(|g| g.recruit_id == id)
            .ok_or("Survivor is not guarding a country")?;
        self.guards.remove(n);
        let r = self
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("Guard is missing from roster")?;
        campaign.release_external(r.npc.loadout);
        r.status = RecruitStatus::Ready;
        let name = r.name.clone();
        self.note(format!(
            "{name} returned from guard duty; campaign recovery applied"
        ));
        Ok(())
    }
    pub fn active_outbreaks(&self) -> usize {
        self.outbreaks
            .iter()
            .filter(|o| o.outcome == OutbreakOutcome::Active)
            .count()
    }
    pub fn resolve_outbreak(&mut self, country_id: &str) -> bool {
        let mut resolved = false;
        for o in &mut self.outbreaks {
            if o.country_id == country_id && o.outcome == OutbreakOutcome::Active {
                o.outcome = OutbreakOutcome::Contained;
                resolved = true;
            }
        }
        if resolved {
            for guard in &mut self.guards {
                if guard.country_id == country_id {
                    if guard.engagement_ticks > 600 {
                        let ht = self
                            .roster
                            .iter()
                            .find(|r| r.id == guard.recruit_id)
                            .map_or(10, |r| r.npc.ht);
                        guard.injury.lose_fatigue(1, &mut self.rng, ht);
                    }
                    guard.engagement_ticks = 0;
                }
            }
            self.note(format!(
                "Active deployment contained the plague in {country_id}"
            ));
        }
        resolved
    }
    pub fn trigger_outbreak(
        &mut self,
        campaign: &Campaign,
        country_id: &str,
    ) -> Result<u64, String> {
        if !self.grand_strategy_unlocked(campaign) {
            return Err("Grand strategy is not yet unlocked".into());
        }
        if !campaign.world.cleared.iter().any(|id| id == country_id) {
            return Err("Outbreaks occur in liberated countries".into());
        }
        if self
            .outbreaks
            .iter()
            .any(|o| o.country_id == country_id && o.outcome == OutbreakOutcome::Active)
        {
            return Err("Country already has an active outbreak".into());
        }
        let id = self.next_outbreak_id;
        let next = id
            .checked_add(1)
            .filter(|_| id > 0)
            .ok_or("Outbreak identity space exhausted")?;
        let deadline = self
            .tick
            .checked_add(world().strategy_config.outbreak_duration_ticks)
            .ok_or("Strategic clock range exhausted")?;
        self.next_outbreak_id = next;
        let strength = 3 + self.rng.below(5) as u32;
        self.outbreaks.push(Outbreak {
            id,
            country_id: country_id.into(),
            started_tick: self.tick,
            deadline_tick: deadline,
            strength,
            enemy: InjuryState::new(9, 10),
            outcome: OutbreakOutcome::Active,
            ready_turns: 0,
        });
        if self.outbreaks.len() > 64
            && let Some(n) = self
                .outbreaks
                .iter()
                .position(|o| o.outcome != OutbreakOutcome::Active)
        {
            self.outbreaks.remove(n);
        }
        self.note(format!("PLAGUE ALERT · {country_id}: {strength} infected groups; assign guards or win a deployment before the deadline"));
        Ok(id)
    }
    pub fn tick_world(&mut self, campaign: &mut Campaign) {
        let allowance = campaign.allowance();
        for recruit in &mut self.roster {
            // Deployed recruits fight on with a worker copy and Defending ones
            // claim through the main run; neither claims from the world clock.
            if !matches!(
                recruit.status,
                RecruitStatus::Deployed | RecruitStatus::Defending
            ) {
                recruit.npc.claim_objectives(allowance);
            }
        }
        self.peak_freed = self.peak_freed.max(campaign.liberated_count() as usize);
        if !self.grand_strategy_unlocked(campaign) {
            return;
        }
        let Some(next_tick) = self.tick.checked_add(1) else {
            return;
        };
        self.tick = next_tick;
        let interval = world().strategy_config.outbreak_interval_ticks.max(60);
        if self.tick.is_multiple_of(interval) && !campaign.world.cleared.is_empty() {
            let index = self.rng.below(campaign.world.cleared.len() as u64) as usize;
            let id = campaign.world.cleared[index].clone();
            let guards = self.guards.iter().filter(|g| g.country_id == id).count() as u32;
            let chance = world()
                .strategy_config
                .outbreak_chance_percent
                .saturating_sub(guards * world().strategy_config.guard_suppression_percent);
            if self.rng.below(100) < u64::from(chance) {
                let _ = self.trigger_outbreak(campaign, &id);
            }
        }
        if self.tick.is_multiple_of(60) {
            self.guard_turns(campaign);
        }
        let expired: Vec<_> = self
            .outbreaks
            .iter()
            .enumerate()
            .filter(|(_, o)| o.outcome == OutbreakOutcome::Active && self.tick >= o.deadline_tick)
            .map(|(n, _)| n)
            .collect();
        for n in expired {
            self.outbreaks[n].outcome = OutbreakOutcome::Overrun;
            let id = self.outbreaks[n].country_id.clone();
            let _ = campaign.world.reopen(&id);
            self.note(format!("{id} fell to renewed plague pressure; its final stage must be secured again. Earned travel routes remain open."));
            let guards: Vec<_> = self
                .guards
                .iter()
                .filter(|g| g.country_id == id)
                .map(|g| g.recruit_id)
                .collect();
            for guard in guards {
                let _ = self.release_guard(campaign, guard);
            }
        }
    }
    fn guard_turns(&mut self, campaign: &mut Campaign) {
        let mut notes = vec![];
        let mut contained = std::collections::BTreeSet::new();
        // Each guard receives one action. Recovery and reload consume that action,
        // but never consume the hostile's independent action below.
        for n in 0..self.guards.len() {
            let country = self.guards[n].country_id.clone();
            let id = self.guards[n].recruit_id;
            let Some(recruit) = self.roster.iter_mut().find(|r| r.id == id) else {
                continue;
            };
            let Some(outbreak) = self
                .outbreaks
                .iter_mut()
                .find(|o| o.country_id == country && o.outcome == OutbreakOutcome::Active)
            else {
                continue;
            };
            let guard = &mut self.guards[n];
            guard.duty_ticks += 60;
            guard.engagement_ticks += 60;
            recruit.npc.combat_ticks = recruit.npc.combat_ticks.saturating_add(60);
            if !guard.injury.start_turn(&mut self.rng, recruit.npc.ht) {
                continue;
            }
            let shock = guard.injury.shock;
            guard.injury.clear_shock();
            if guard.ready_turns > 0 {
                guard.ready_turns -= 1;
                continue;
            }
            if guard.injury.stunned {
                guard.injury.recover_stun(&mut self.rng, recruit.npc.ht);
                continue;
            }
            if guard.injury.prone {
                guard.injury.prone = false;
                continue;
            }
            if guard.injury.weapon_dropped {
                guard.injury.weapon_dropped = false;
                continue;
            }
            let rifle = item_definition(campaign, &recruit.npc, Slot::Ranged);
            if guard.reload_turns > 0 {
                guard.reload_turns -= 1;
                if guard.reload_turns == 0 {
                    guard.ammo = rifle.npc.magazine;
                }
                continue;
            }
            let melee = (recruit.npc.policy == crate::content::Policy::Guardian || guard.ammo == 0)
                && guard.injury.fp > 1;
            if guard.ammo == 0 && !melee {
                guard.reload_turns = rifle.npc.reload_turns.saturating_sub(1);
                if guard.reload_turns == 0 {
                    guard.ammo = rifle.npc.magazine;
                }
                continue;
            }
            if !guard
                .injury
                .fatigue_allows_action(&mut self.rng, recruit.npc.iq)
            {
                continue;
            }
            if !melee {
                guard.ammo -= 1;
            }
            let profile = campaign.profile_for_loadout(
                crate::content::Mode::Npc,
                recruit.npc.loadout,
                &recruit.npc,
            );
            let weak = guard.injury.fp.saturating_mul(3) < guard.injury.max_fp;
            let melee_damage = Damage {
                dice: profile.npc_melee_damage_dice,
                adds: profile.npc_melee_damage_add,
                kind: profile.npc_melee_kind,
            };
            let attack_damage = if melee {
                if weak {
                    rules::rescale_swing(melee_damage, recruit.npc.st, (recruit.npc.st + 1) / 2)
                        .unwrap_or(melee_damage)
                } else {
                    melee_damage
                }
            } else {
                Damage {
                    dice: rifle.npc.damage_dice,
                    adds: rifle.npc.damage_add,
                    kind: rifle.npc.damage_kind,
                }
            };
            let attack = Attack {
                skill: if melee {
                    recruit.npc.melee - shock
                } else {
                    recruit.npc.guns + rifle.npc.skill_bonus - shock
                },
                damage: attack_damage,
                armor_divisor: 1,
            };
            let result = rules::resolve_attack(
                &mut self.rng,
                attack,
                Some(outbreak.enemy.dodge(7)),
                0,
                &mut outbreak.enemy,
                10,
            );
            if result.requires_ready {
                guard.ready_turns = 1;
            }
            if result.hit && !melee {
                recruit.npc.successful_hits = recruit.npc.successful_hits.saturating_add(1);
            }
            notes.push(format!(
                "{} guarding {} ({}): {}",
                recruit.name,
                country,
                if melee { "melee" } else { "ranged" },
                result.explanation()
            ));
            if outbreak.enemy.terminal() {
                outbreak.strength = outbreak.strength.saturating_sub(1);
                outbreak.enemy = InjuryState::new(9, 10);
                outbreak.ready_turns = 0;
                if outbreak.strength == 0 {
                    outbreak.outcome = OutbreakOutcome::Contained;
                    guard.victories += 1;
                    recruit.npc.victories += 1;
                    // Kept on the recruit: the guard assignment goes.
                    recruit.containments = recruit.containments.saturating_add(1);
                    contained.insert(country.clone());
                    notes.push(format!(
                        "{} contained the outbreak in {}",
                        recruit.name, country
                    ));
                }
            }
        }
        // Each active hostile group receives exactly one action per country,
        // regardless of garrison size. Stable recruit IDs define its target.
        for outbreak in &mut self.outbreaks {
            if outbreak.outcome != OutbreakOutcome::Active {
                continue;
            }
            if !outbreak.enemy.start_turn(&mut self.rng, 10) {
                if outbreak.enemy.terminal() {
                    outbreak.strength = outbreak.strength.saturating_sub(1);
                    outbreak.enemy = InjuryState::new(9, 10);
                    outbreak.ready_turns = 0;
                    if outbreak.strength == 0 {
                        outbreak.outcome = OutbreakOutcome::Contained;
                        contained.insert(outbreak.country_id.clone());
                        // The last group fell to the garrison's wounds:
                        // every guard of the country contained it.
                        for guard in &self.guards {
                            if guard.country_id != outbreak.country_id {
                                continue;
                            }
                            if let Some(r) =
                                self.roster.iter_mut().find(|r| r.id == guard.recruit_id)
                            {
                                r.containments = r.containments.saturating_add(1);
                            }
                        }
                    }
                }
                continue;
            }
            let shock = outbreak.enemy.shock;
            outbreak.enemy.clear_shock();
            if outbreak.ready_turns > 0 {
                outbreak.ready_turns -= 1;
                continue;
            }
            if outbreak.enemy.stunned {
                outbreak.enemy.recover_stun(&mut self.rng, 10);
                continue;
            }
            if outbreak.enemy.prone {
                outbreak.enemy.prone = false;
                continue;
            }
            // Natural hostile attacks do not require retrieving a weapon.
            outbreak.enemy.weapon_dropped = false;
            let target = self
                .guards
                .iter()
                .enumerate()
                .filter(|(_, g)| g.country_id == outbreak.country_id && !g.injury.terminal())
                .min_by_key(|(_, g)| g.recruit_id)
                .map(|(n, _)| n);
            let Some(n) = target else {
                continue;
            };
            let guard = &mut self.guards[n];
            let Some(recruit) = self.roster.iter().find(|r| r.id == guard.recruit_id) else {
                continue;
            };
            let armor = item_definition(campaign, &recruit.npc, Slot::Outfit)
                .npc
                .armor
                + item_definition(campaign, &recruit.npc, Slot::Accessory)
                    .npc
                    .armor;
            let hostile = Attack {
                skill: 9 - shock,
                damage: Damage {
                    dice: 1,
                    adds: 0,
                    kind: DamageKind::Crushing,
                },
                armor_divisor: 1,
            };
            let injury = rules::resolve_attack(
                &mut self.rng,
                hostile,
                Some(guard.injury.dodge(recruit.npc.dodge())),
                armor,
                &mut guard.injury,
                recruit.npc.ht,
            );
            if injury.requires_ready {
                outbreak.ready_turns = 1;
            }
            if injury.injury > 0 {
                notes.push(format!(
                    "{} injured on guard duty: {}",
                    recruit.name,
                    injury.explanation()
                ));
            }
        }
        // Lite's ordinary post-battle exertion: one FP after a fight over ten
        // seconds. Recall applies the campaign's between-attempt recovery rule.
        for guard in &mut self.guards {
            if contained.contains(&guard.country_id) {
                if guard.engagement_ticks > 600 {
                    let ht = self
                        .roster
                        .iter()
                        .find(|r| r.id == guard.recruit_id)
                        .map_or(10, |r| r.npc.ht);
                    guard.injury.lose_fatigue(1, &mut self.rng, ht);
                }
                guard.engagement_ticks = 0;
            }
        }
        for note in notes {
            self.note(note);
        }
        let fallen: Vec<_> = self
            .guards
            .iter()
            .filter(|g| g.injury.terminal())
            .map(|g| g.recruit_id)
            .collect();
        for id in fallen {
            self.note(format!("Guard {id} incapacitated; the outbreak remains active. Campaign recovery restores the survivor."));
            let _ = self.release_guard(campaign, id);
        }
    }
    pub fn validate(&self, campaign: &Campaign) -> Result<(), String> {
        if self.roster.len() > world().strategy_config.max_roster
            || self.next_recruit_id == 0
            || self.next_outbreak_id == 0
            || self.peak_freed > world().countries.len()
            || self.log.len() > 80
            || self.outbreaks.len() > world().countries.len() + 64
        {
            return Err("Invalid strategic capacity or identity counters".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for r in &self.roster {
            if r.id == 0
                || r.id >= self.next_recruit_id
                || !ids.insert(r.id)
                || world().country(&r.origin).is_none()
            {
                return Err("Invalid recruited survivor identity".into());
            }
            campaign.validate_loadout(r.npc.loadout)?;
            campaign.validate_npc(&r.npc)?;
            if r.status == RecruitStatus::Guarding
                && !self.guards.iter().any(|g| g.recruit_id == r.id)
            {
                return Err("Guard roster status has no garrison".into());
            }
        }
        let mut guards = std::collections::BTreeSet::new();
        let mut equipment = std::collections::BTreeSet::new();
        for g in &self.guards {
            let r = self
                .roster
                .iter()
                .find(|r| r.id == g.recruit_id)
                .ok_or("Garrison survivor missing")?;
            if !guards.insert(g.recruit_id)
                || r.status != RecruitStatus::Guarding
                || world().country(&g.country_id).is_none()
                || !campaign.world.cleared.contains(&g.country_id)
                || !g.injury.valid()
                || g.injury.max_hp
                    != campaign
                        .profile_for_loadout(Mode::Npc, r.npc.loadout, &r.npc)
                        .max_hp
                || g.injury.max_fp
                    != campaign
                        .profile_for_loadout(Mode::Npc, r.npc.loadout, &r.npc)
                        .max_fp
                || g.ammo > item_definition(campaign, &r.npc, Slot::Ranged).npc.magazine
                || g.reload_turns
                    > item_definition(campaign, &r.npc, Slot::Ranged)
                        .npc
                        .reload_turns
                || g.ready_turns > 1
                || g.engagement_ticks > g.duty_ticks
                || g.duty_ticks > self.tick
                || r.npc
                    .loadout
                    .iter()
                    .any(|id| !campaign.reserved_items.contains(id) || !equipment.insert(*id))
            {
                return Err("Invalid garrison reservation".into());
            }
        }
        let mut outbreaks = std::collections::BTreeSet::new();
        let mut active_countries = std::collections::BTreeSet::new();
        for o in &self.outbreaks {
            if world().country(&o.country_id).is_none()
                || o.deadline_tick < o.started_tick
                || o.strength > 100
                || o.id == 0
                || o.id >= self.next_outbreak_id
                || !outbreaks.insert(o.id)
                || o.started_tick > self.tick
                || !o.enemy.valid()
                || o.enemy.max_hp != 9
                || o.enemy.max_fp != 10
                || o.ready_turns > 1
                || (o.outcome == OutbreakOutcome::Active
                    && (o.strength == 0
                        || o.deadline_tick <= self.tick
                        || !campaign.world.cleared.contains(&o.country_id)
                        || !active_countries.insert(&o.country_id)))
            {
                return Err("Invalid strategic outbreak".into());
            }
        }
        Ok(())
    }
}
fn item_definition(campaign: &Campaign, npc: &Npc, slot: Slot) -> crate::content::ItemDefinition {
    let definition = campaign.item(npc.loadout[slot.index()]).map_or(
        &content().items[slot.index()],
        crate::campaign::Item::definition,
    );
    campaign
        .effective_item(npc.loadout[slot.index()])
        .unwrap_or_else(|_| definition.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn liberated(count: usize) -> Campaign {
        let mut c = Campaign {
            legacy_features: true,
            ..Campaign::default()
        };
        let ids: Vec<_> = world()
            .countries
            .iter()
            .take(count)
            .map(|c| c.id.clone())
            .collect();
        c.choose_start_country(&ids[0]).unwrap();
        for id in ids {
            let country = world().country(&id).unwrap();
            c.world
                .country_levels
                .insert(id.clone(), country.level_count());
            c.world.cleared.push(id);
        }
        c.coins = 10000;
        c
    }
    #[test]
    fn recruits_receive_independent_unique_equipment() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let id = c.world.cleared[0].clone();
        let a = s.recruit(&mut c, &id).unwrap();
        let b = s.recruit(&mut c, &id).unwrap();
        assert_ne!(a, b);
        assert!(
            s.roster[0]
                .npc
                .loadout
                .iter()
                .all(|item| !s.roster[1].npc.loadout.contains(item))
        );
        // Five protected campaign starters (including the personal pistol),
        // plus four independent items for each of the two recruits.
        assert_eq!(c.items.len(), 13);
        let identities: std::collections::BTreeSet<_> =
            c.items.iter().map(|item| item.id).collect();
        assert_eq!(identities.len(), c.items.len());
        assert!(s.validate(&c).is_ok());
    }
    #[test]
    fn assignment_prevents_parallel_double_use() {
        let mut c = liberated(4);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let id = s.recruit(&mut c, &country).unwrap();
        s.assign_guard(&mut c, id, &country).unwrap();
        assert!(s.reserve_recruit(id).is_err());
        assert!(s.assign_guard(&mut c, id, &country).is_err());
        s.release_guard(&mut c, id).unwrap();
        assert!(s.reserve_recruit(id).is_ok());
        assert!(c.reserved_items.is_empty());
    }
    #[test]
    fn outbreak_expiry_reopens_country_without_reward() {
        let mut c = liberated(4);
        let mut s = StrategyState::default();
        let id = c.world.cleared[0].clone();
        s.trigger_outbreak(&c, &id).unwrap();
        let coins = c.coins;
        s.outbreaks[0].deadline_tick = 1;
        s.tick_world(&mut c);
        assert_eq!(s.outbreaks[0].outcome, OutbreakOutcome::Overrun);
        assert!(!c.world.cleared.contains(&id));
        assert_eq!(c.coins, coins);
        assert_eq!(
            c.world.completed(&id),
            world().country(&id).unwrap().level_count() - 1
        );
    }
    #[test]
    fn seeded_strategic_clock_survives_serialization() {
        let mut c = liberated(4);
        let mut a = StrategyState::default();
        let id = c.world.cleared[0].clone();
        a.trigger_outbreak(&c, &id).unwrap();
        let recruit = a.recruit(&mut c, &id).unwrap();
        a.assign_guard(&mut c, recruit, &id).unwrap();
        let mut b: StrategyState =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        let mut other = c.clone();
        for _ in 0..600 {
            a.tick_world(&mut c);
            b.tick_world(&mut other);
        }
        assert_eq!(a, b);
        assert_eq!(c, other);
    }
    fn guarding(count: usize) -> (Campaign, StrategyState) {
        let mut c = liberated(4);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        s.trigger_outbreak(&c, &country).unwrap();
        for _ in 0..count {
            let id = s.recruit(&mut c, &country).unwrap();
            s.assign_guard(&mut c, id, &country).unwrap();
            let g = s.guards.last_mut().unwrap();
            g.reload_turns = 1000;
            g.injury.max_hp = 1000;
            g.injury.hp = 1000;
        }
        (c, s)
    }
    #[test]
    fn reloading_guards_remain_vulnerable_to_hostile_actions() {
        let (mut c, mut s) = guarding(1);
        for _ in 0..600 {
            s.tick_world(&mut c);
        }
        assert!(s.guards[0].injury.hp < 1000);
        assert!(s.guards[0].reload_turns > 0);
    }
    #[test]
    fn hostile_shock_is_retained_until_the_next_guard_action() {
        let (mut c, mut s) = guarding(1);
        let mut injured = false;
        for _ in 0..60 {
            for _ in 0..60 {
                s.tick_world(&mut c);
            }
            if s.guards[0].injury.shock > 0 {
                injured = true;
                break;
            }
        }
        assert!(
            injured,
            "at least one sourced injury must retain shock at the end of the hostile action"
        );
    }
    #[test]
    fn seeded_zero_is_the_default_and_other_seeds_vary_the_rng() {
        assert_eq!(StrategyState::seeded(0), StrategyState::default());
        let mut a = StrategyState::seeded(1);
        let mut b = StrategyState::seeded(2);
        assert_ne!(a.rng.next_u64(), b.rng.next_u64());
        assert_eq!(
            StrategyState {
                rng: StrategyState::default().rng,
                ..StrategyState::seeded(7)
            },
            StrategyState::default()
        );
    }
    #[test]
    fn adding_a_reloading_guard_does_not_grant_extra_hostile_actions() {
        let (mut c, mut a) = guarding(1);
        let mut b = a.clone();
        let mut other = c.clone();
        let country = c.world.cleared[0].clone();
        let second = b.recruit(&mut other, &country).unwrap();
        b.assign_guard(&mut other, second, &country).unwrap();
        b.guards[1].reload_turns = 1000;
        b.guards[1].injury.max_hp = 1000;
        b.guards[1].injury.hp = 1000;
        for _ in 0..600 {
            a.tick_world(&mut c);
            b.tick_world(&mut other);
        }
        assert_eq!(a.guards[0].injury, b.guards[0].injury);
        assert_eq!(b.guards[1].injury.hp, 1000);
        assert_eq!(a.rng, b.rng);
    }
    #[test]
    fn manual_response_clears_alert_without_free_coins() {
        let c = liberated(4);
        let mut s = StrategyState::default();
        let id = c.world.cleared[0].clone();
        s.trigger_outbreak(&c, &id).unwrap();
        let coins = c.coins;
        assert!(s.resolve_outbreak(&id));
        assert!(!s.resolve_outbreak(&id));
        assert_eq!(c.coins, coins);
    }
    /// Rewritten in W1-20 (was `idle_recruit_sales_warn_and_restore_the_
    /// original_owned_starter_set`): SellItem is strict, so a recruit-equipped
    /// item is no longer sold with a warning and replaced by the recruit's
    /// starter set. The way out is to re-equip the recruit's own starter item;
    /// then the rifle sells and no loadout needs repair.
    #[test]
    fn idle_recruit_equipment_sells_only_after_the_recruit_switches_back() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let id = s.recruit(&mut c, &country).unwrap();
        let starter = s.roster[0].npc.loadout[0];
        let rifle = c.add_item(4).unwrap();
        s.equip_recruit(&c, id, rifle).unwrap();
        let blocked = crate::campaign::SaleBlock::RecruitEquipped {
            name: s.roster[0].name.clone(),
        };
        assert_eq!(
            c.sale_eligibility_for(rifle, crate::campaign::SaleOrigin::Player, &s.sale_refs()),
            Err(blocked.clone())
        );
        assert_eq!(s.sell_extras_quote(&c), 0);
        assert_eq!(
            c.sell(rifle, &s.sale_refs()),
            Err(crate::command::CommandError::SaleBlocked(blocked))
        );
        s.equip_recruit(&c, id, starter).unwrap();
        assert_eq!(c.sell(rifle, &s.sale_refs()), Ok(22));
        assert_eq!(s.reconcile_loadouts(&c), 0, "no loadout needs repair");
        assert!(s.validate(&c).is_ok());
    }
    /// The positive leg of the save repair (W1 review; W1-20 rewrote the old
    /// sale test into a consistent-state check). An old or hand-edited save
    /// can hold an idle recruit slot whose item is no longer owned; loading
    /// it puts that slot back on the recruit's own protected starter bundle,
    /// picked by roster ordinal, and leaves every other recruit alone.
    #[test]
    fn reconcile_loadouts_restores_a_missing_item_to_the_recruits_own_starter() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let first = s.recruit(&mut c, &country).unwrap();
        s.recruit(&mut c, &country).unwrap();
        let starter = s.roster[0].npc.loadout[0];
        let second = s.roster[1].npc.loadout;
        let rifle = c.add_item(4).unwrap();
        s.equip_recruit(&c, first, rifle).unwrap();
        // Stands in for a save whose stash lost the equipped rifle.
        c.items.retain(|i| i.id != rifle);
        assert!(s.validate(&c).is_err(), "the damaged state is invalid");
        assert_eq!(s.reconcile_loadouts(&c), 1);
        assert_eq!(s.roster[0].npc.loadout[0], starter);
        assert_ne!(starter, 1, "the recruit's own bundle, not Mara's");
        assert_ne!(starter, second[0], "the first recruit's bundle");
        assert_eq!(s.roster[1].npc.loadout, second);
        assert!(s.validate(&c).is_ok());
    }
    /// Replaces the W1-18 characterisation test
    /// `old_campaign_filter_sells_recruit_equipped_item` (LE-1, acceptance
    /// G4.2), inverted: every sell path reads the recruit loadouts through
    /// `StrategyState::sale_refs`, so a rifle an idle recruit has equipped is
    /// neither quoted nor sold, by the batch or by the strict single sale, and
    /// the strategic state stays valid.
    #[test]
    fn strict_sell_rejects_recruit_equipped_item() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let id = s.recruit(&mut c, &country).unwrap();
        let rifle = c.add_item(4).unwrap();
        s.equip_recruit(&c, id, rifle).unwrap();
        assert_eq!(s.roster[0].npc.loadout[0], rifle);
        let before = c.clone();
        assert_eq!(s.sell_extras_quote(&c), 0);
        assert_eq!(s.sell_extras(&mut c), Ok(0));
        assert_eq!(
            c.sell(rifle, &s.sale_refs()),
            Err(crate::command::CommandError::SaleBlocked(
                crate::campaign::SaleBlock::RecruitEquipped {
                    name: s.roster[0].name.clone()
                }
            ))
        );
        assert_eq!(c, before);
        assert_eq!(s.roster[0].npc.loadout[0], rifle);
        assert!(s.validate(&c).is_ok());
    }
    #[test]
    fn recruited_backgrounds_and_policies_are_individual_and_training_preserves_creation() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let a = s.recruit(&mut c, &country).unwrap();
        let b = s.recruit(&mut c, &country).unwrap();
        assert_ne!(s.roster[0].npc.creation, s.roster[1].npc.creation);
        assert_ne!(s.roster[0].npc.policy, s.roster[1].npc.policy);
        for _ in 0..4 {
            s.buy_training_point(&mut c, b).unwrap();
        }
        s.train_recruit(&mut c, b, NpcTraining::Guns).unwrap();
        let base = s.roster[1].npc.creation.guns;
        s.respec_recruit(&mut c, b).unwrap();
        assert_eq!(s.roster[1].npc.guns, base);
        assert_eq!(s.roster[1].npc.awarded, 4);
        assert!(s.readiness(&c, a).unwrap().summary.contains("Marksman"));
        assert!(s.validate(&c).is_ok());
    }
    /// Ruling E7: the second offer is keyed on exactly the game seed, the
    /// next recruit id and the country (`keyed(seed, next_recruit_id,
    /// fnv1a(country), RECRUIT_OFFER)`). A recorded `RecruitAs` replays only
    /// while that key holds, and no digest covers it, so each input is shown
    /// to matter and the offers over a grid are pinned.
    #[test]
    fn recruit_offer_is_keyed_on_seed_next_recruit_id_and_country() {
        let seeds = [0, 1, 7, 0xDEAD_BEEF_2026, u64::MAX];
        let countries: Vec<&str> = world()
            .countries
            .iter()
            .take(6)
            .map(|c| c.id.as_str())
            .collect();
        let offer = |seed: u64, id: u64, country: &str| {
            let s = StrategyState {
                next_recruit_id: id,
                ..StrategyState::default()
            };
            s.recruit_offer_for(seed, country)
        };
        let mut table = String::new();
        for &seed in &seeds {
            for id in 1..=8 {
                for &country in &countries {
                    let [first, second] = offer(seed, id, country);
                    let key = crate::hash::keyed(
                        seed,
                        id,
                        crate::hash::fnv1a(country.as_bytes()),
                        crate::hash::tags::RECRUIT_OFFER,
                    );
                    let step = 1 + (key % 3) as usize;
                    assert_eq!(first, Archetype::rotation(id));
                    assert_eq!(
                        second,
                        Archetype::ALL[(first.index() + step) % 4],
                        "seed {seed} id {id} {country}"
                    );
                    table.push_str(&format!(
                        "{seed} {id} {country} {} {}\n",
                        first.id(),
                        second.id()
                    ));
                }
            }
        }
        // Each key input changes some second offer with the others held
        // (ids 1 and 5 share a rotation archetype).
        let second = |seed, id, country| offer(seed, id, country)[1];
        let base = countries[0];
        assert!(
            seeds[1..]
                .iter()
                .any(|&seed| second(seed, 1, base) != second(seeds[0], 1, base)),
            "the seed keys the offer"
        );
        assert!(
            (0..8u64).any(|n| second(1, 1 + 4 * (n + 1), base) != second(1, 1, base)),
            "the next recruit id keys the offer"
        );
        assert!(
            countries[1..]
                .iter()
                .any(|&country| second(1, 1, country) != second(1, 1, base)),
            "the country keys the offer"
        );
        assert_eq!(
            crate::hash::fnv1a(table.as_bytes()),
            0xa2c5_0b35_466f_1d3f,
            "recruit offers moved (ruling E7):\n{table}"
        );
    }
    #[test]
    fn readiness_summary_has_no_debug_names() {
        let mut c = liberated(1);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let id = s.recruit(&mut c, &country).unwrap();
        s.roster[0].status = RecruitStatus::Deployed;
        let summary = s.readiness(&c, id).unwrap().summary;
        assert!(summary.contains("On expedition"), "{summary}");
        assert!(!summary.contains("Deployed"), "{summary}");
    }
    #[test]
    fn strategic_validation_rejects_duplicate_alerts_and_invalid_guard_injury() {
        let (c, mut s) = guarding(1);
        s.guards[0].injury.max_hp = s.roster[0].npc.max_hp();
        s.guards[0].injury.hp = s.roster[0].npc.max_hp();
        s.guards[0].reload_turns = 0;
        s.outbreaks.push(s.outbreaks[0].clone());
        assert!(s.validate(&c).is_err());
        s.outbreaks.pop();
        s.guards[0].injury.shock = 99;
        assert!(s.validate(&c).is_err());
    }
    #[test]
    fn guardian_policy_can_melee_without_spending_ammunition_or_ranged_objective_hits() {
        let mut c = liberated(4);
        let mut s = StrategyState::default();
        let country = c.world.cleared[0].clone();
        let id = s.recruit(&mut c, &country).unwrap();
        s.roster[0].npc.policy = crate::content::Policy::Guardian;
        s.assign_guard(&mut c, id, &country).unwrap();
        s.guards[0].ammo = 0;
        s.trigger_outbreak(&c, &country).unwrap();
        for _ in 0..120 {
            s.tick_world(&mut c);
        }
        assert_eq!(s.guards[0].ammo, 0);
        assert_eq!(s.roster[0].npc.successful_hits, 0);
        assert!(s.log.iter().any(|line| line.contains("(melee)")));
    }
}
