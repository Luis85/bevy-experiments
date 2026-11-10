//! Choosing a country without deploying (CD6): `Command::SelectCountry` and
//! the pure reads behind the country dossier and an outbreak's "Plan
//! response" (SelectCountry, then the Briefing at the final stage, then
//! `BeginDefense`). Nothing here is saved; every read is derived.
use super::defense::feature_node;
use super::*;
use crate::command::{CommandError, CommandOutcome};
use crate::skill_catalog::{CampaignFeature, skills};
use crate::strategy::OutbreakOutcome;
use crate::world::world;

/// What an active outbreak asks of the player (`Game::outbreak_plan`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutbreakPlan {
    pub country_id: String,
    pub country_name: String,
    /// The stage a response replays: the country's final stage.
    pub stage: usize,
    /// Strategic ticks until the outbreak overruns the country.
    pub ticks_left: u64,
    /// Infected groups still to defeat.
    pub strength: u32,
    /// The recruits guarding the country, in assignment order.
    pub guards: Vec<u64>,
    /// The most guards one country can hold.
    pub guard_capacity: usize,
    /// The recruits `AssignGuard` would accept for this country now, by id.
    pub eligible_guards: Vec<u64>,
    /// The selectable defender with the highest power at the next threat
    /// (`Game::defender_power`). On a tie the earliest in
    /// `Game::available_defenders` order wins (player, Mara, recruits by
    /// id), so Mara wins a Mara/recruit tie; the player is the fallback.
    pub recommended_defender: DefenderId,
}

/// What choosing a country means before it is chosen
/// (`Game::country_choice_preview`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountryChoicePreview {
    /// The defenses that liberate the country.
    pub stage_count: usize,
    /// Mara is not yet in camp, and liberating this country is what meets
    /// her upgrade node's liberation requirement: false once the requirement
    /// is already met, and for a country already counted as liberated.
    /// Always false for a legacy campaign, which meets her after its first
    /// defense instead.
    pub companion_after_liberation: bool,
}

impl Game {
    /// `Command::SelectCountry`: makes `id` the active country between
    /// defenses. It never deploys and never chooses the starting country.
    /// Checked in order: a live defense, no starting country, a country that
    /// cannot be entered (unknown ids included). Nothing changes on an error.
    pub(crate) fn select_country(&mut self, id: &str) -> Result<CommandOutcome, CommandError> {
        if self.attempt.is_some() {
            return Err(CommandError::DefenseInProgress);
        }
        if self.campaign.world.start.is_none() {
            return Err(CommandError::StartNotChosen);
        }
        let country = world()
            .country(id)
            .filter(|_| self.campaign.world.can_enter(id))
            .ok_or_else(|| CommandError::CountryLocked(id.into()))?;
        self.campaign.enter_country(id)?;
        Ok(
            if self.campaign.world.completed(id) >= country.level_count() {
                CommandOutcome::CountryLiberated
            } else {
                CommandOutcome::Applied
            },
        )
    }
    /// The response an active outbreak in `country_id` asks for, or None
    /// when the country has no active outbreak. Pure: no mutation, no RNG.
    pub fn outbreak_plan(&self, country_id: &str) -> Option<OutbreakPlan> {
        let outbreak = self
            .strategy
            .outbreaks
            .iter()
            .find(|o| o.country_id == country_id && o.outcome == OutbreakOutcome::Active)?;
        let country = world().country(country_id)?;
        Some(OutbreakPlan {
            country_id: country.id.clone(),
            country_name: country.name.clone(),
            stage: country.level_count().saturating_sub(1),
            ticks_left: outbreak.deadline_tick.saturating_sub(self.strategy.tick),
            strength: outbreak.strength,
            guards: self
                .strategy
                .guards
                .iter()
                .filter(|g| g.country_id == country_id)
                .map(|g| g.recruit_id)
                .collect(),
            guard_capacity: world().strategy_config.max_guards_per_country,
            eligible_guards: self.strategy.eligible_guards(&self.campaign, country_id),
            recommended_defender: self.recommended_defender(),
        })
    }
    /// What choosing `country_id` means, or None for an unknown country.
    /// Pure: no mutation, no RNG.
    pub fn country_choice_preview(&self, country_id: &str) -> Option<CountryChoicePreview> {
        let country = world().country(country_id)?;
        let stage_count = country.level_count();
        let world = &self.campaign.world;
        // Already in `liberated_count`: liberated now, or every stage once
        // cleared before an outbreak reopened it.
        let counted = world.cleared.iter().any(|c| c == country_id)
            || world
                .clear_high_water
                .get(country_id)
                .is_some_and(|clears| *clears >= stage_count);
        let before = self.campaign.liberated_count();
        let after = before + u32::from(!counted);
        // A legacy campaign meets Mara after its first defense, not through
        // her node (`Campaign::has_feature`), so no liberation brings her.
        // Only the liberation that first meets her node's requirement does:
        // once it is met (her node still unbought), no country brings her.
        let companion_after_liberation = !self.campaign.npc_available()
            && !self.campaign.legacy_features
            && skills()
                .node(&feature_node(CampaignFeature::Companion))
                .is_some_and(|node| {
                    before < node.minimum_liberated && after >= node.minimum_liberated
                });
        Some(CountryChoicePreview {
            stage_count,
            companion_after_liberation,
        })
    }
}
