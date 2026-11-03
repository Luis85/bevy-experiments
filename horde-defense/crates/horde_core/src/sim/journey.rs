//! Explicit main-round continuation. Workers retain their strategic orchestration.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JourneyContinuation {
    Deployed,
    CountryLiberated,
}
impl Game {
    pub fn continue_journey(&mut self) -> Result<JourneyContinuation, String> {
        if self.worker {
            return Err("Use the parent expedition controller".into());
        }
        if !self.campaign.journey_active {
            return Err("Choose a country to begin a journey".into());
        }
        if self.attempt.is_some() {
            return Err("Finish the current round before continuing".into());
        }
        let report = self
            .last_result
            .clone()
            .ok_or("No settled round to continue")?;
        let previous_world = self.campaign.world.clone();
        let operation = if let Some(country) = &report.country_id {
            let count = crate::world::world()
                .country(country)
                .ok_or("Unknown reported country")?
                .level_count();
            let next = if report.outcome == Outcome::Victory {
                self.campaign.world.completed(country)
            } else {
                report.stage
            };
            if next >= count {
                self.campaign.journey_active = false;
                self.campaign.journey_revision = 1;
                self.repeat_remaining = 0;
                self.set_paused(true);
                return Ok(JourneyContinuation::CountryLiberated);
            }
            self.campaign
                .enter_country(country)
                .and_then(|()| self.deploy_country_stage(next, report.mode))
        } else {
            let next = if report.outcome == Outcome::Victory {
                report.level + 1
            } else {
                report.level
            };
            if next >= content().levels.len() {
                self.campaign.journey_active = false;
                self.campaign.journey_revision = 1;
                self.repeat_remaining = 0;
                self.set_paused(true);
                return Ok(JourneyContinuation::CountryLiberated);
            }
            self.deploy(next, report.mode)
        };
        if let Err(error) = operation {
            self.campaign.world = previous_world;
            return Err(error);
        }
        self.repeat_remaining = 0;
        self.set_paused(false);
        Ok(JourneyContinuation::Deployed)
    }
    pub fn leave_journey(&mut self) {
        if self.attempt.is_some() {
            self.withdraw();
        }
        self.campaign.journey_active = false;
        self.campaign.journey_revision = 1;
        self.repeat_remaining = 0;
        self.set_paused(true);
    }
}
