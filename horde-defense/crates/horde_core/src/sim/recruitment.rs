//! Recruitment choice (decisions 5 and 15, design-campaign-defenders 1.6):
//! the pure `Game::recruit_offer` read and the `Recruit` / `RecruitAs`
//! commands. The offer is derived on every read (schema-v7 section 9): the
//! first archetype is today's rotation and the second is keyed on the game
//! seed, the next recruit id and the country (ruling E7), so recruiting
//! never draws the strategy RNG.
use super::defense::feature_node;
use super::*;
use crate::command::CommandError;
use crate::defenders::{
    Archetype, ArchetypeChoice, OriginTrait, RecruitBlocker, RecruitOffer, recruit_identity,
    recruit_name,
};
use crate::skill_catalog::{CampaignFeature, skills};
use crate::world::world;

impl Game {
    /// The recruit `country_id` offers now, or the first reason it offers
    /// none, checked in the order recruiting always has: the Recruitment
    /// node, a fully liberated country, roster room, no defender out, coins.
    /// Pure: no mutation, no RNG.
    pub fn recruit_offer(&self, country_id: &str) -> Result<RecruitOffer, RecruitBlocker> {
        let campaign = &self.campaign;
        if !campaign.has_feature(CampaignFeature::Recruitment) {
            let id = feature_node(CampaignFeature::Recruitment);
            let node = skills().node(&id);
            return Err(RecruitBlocker::NeedsNode {
                name: node.map_or_else(|| id.clone(), |n| n.name.clone()),
                price: node.map_or(0, |n| n.price),
                id,
            });
        }
        let country = world()
            .country(country_id)
            .filter(|_| self.strategy.recruitment_unlocked(campaign))
            .filter(|_| campaign.world.cleared.iter().any(|id| id == country_id))
            .ok_or(RecruitBlocker::NotLiberated)?;
        if self.strategy.roster.len() >= world().strategy_config.max_roster {
            return Err(RecruitBlocker::RosterFull);
        }
        if campaign.reserved.is_some() {
            return Err(RecruitBlocker::DefenseInProgress);
        }
        let price = campaign.recruit_price();
        if campaign.coins < price {
            return Err(RecruitBlocker::NeedCoins(price));
        }
        let next = self.strategy.next_recruit_id;
        Ok(RecruitOffer {
            country_id: country.id.clone(),
            country_name: country.name.clone(),
            price,
            name: recruit_name(next),
            appearance: recruit_identity(next).1,
            origin_trait: OriginTrait::for_biome(&country.biome),
            archetypes: self
                .strategy
                .recruit_offer_for(self.seed, country_id)
                .map(ArchetypeChoice::of),
        })
    }
    /// `Command::RecruitAs`: recruits from `country_id` as the offered
    /// archetype named `archetype` (its `Archetype::id`). A blocked offer
    /// answers first, then an archetype the offer does not name; nothing
    /// changes on an error.
    pub(crate) fn recruit_as(
        &mut self,
        country_id: &str,
        archetype: &str,
    ) -> Result<u64, CommandError> {
        let offer = self
            .recruit_offer(country_id)
            .map_err(CommandError::RecruitUnavailable)?;
        let chosen = Archetype::from_id(archetype)
            .filter(|a| offer.archetypes.iter().any(|c| c.archetype == *a))
            .ok_or_else(|| CommandError::ArchetypeNotOffered(archetype.into()))?;
        Ok(self
            .strategy
            .recruit_as(&mut self.campaign, country_id, chosen)?)
    }
    /// `Command::Recruit`: `RecruitAs` with the first offer, the rotation
    /// archetype every recruit joined as before the choice.
    pub(crate) fn recruit_first_offer(&mut self, country_id: &str) -> Result<u64, CommandError> {
        let offer = self
            .recruit_offer(country_id)
            .map_err(CommandError::RecruitUnavailable)?;
        let first = offer.archetypes[0].archetype;
        Ok(self
            .strategy
            .recruit_as(&mut self.campaign, country_id, first)?)
    }
}
