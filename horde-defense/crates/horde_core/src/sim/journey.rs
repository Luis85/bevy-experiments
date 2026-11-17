//! Explicit main-round continuation. Workers retain their strategic orchestration.
use super::*;
use crate::command::CommandError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JourneyContinuation {
    Deployed,
    CountryLiberated,
}
/// What `ContinueJourney` would do now, once its pre-checks pass.
enum ContinueStep {
    /// The journey's last stage is behind it: nobody deploys.
    Liberated,
    /// Deploy the next defender on `stage` of `country`.
    Country { country: String, stage: usize },
    /// Deploy the next defender on catalogue level `level`.
    Level(usize),
}
impl Game {
    /// Why `Command::ContinueJourney` would be refused now (the same typed
    /// error it returns), or None when it would deploy or liberate. Pure, so
    /// Results can disable Continue and show the picker (ruling D9).
    pub fn continue_blocker(&self) -> Option<CommandError> {
        self.continue_step().err()
    }
    /// `ContinueJourney`'s checks before it changes anything, and the step
    /// it would take. A deploy step also needs [`Self::next_defender`] to
    /// pass [`Self::defender_selectable`]; a liberation deploys nobody.
    fn continue_step(&self) -> Result<ContinueStep, CommandError> {
        let rejected = |message: &str| CommandError::Rejected(message.into());
        if self.worker {
            return Err(rejected("Use the parent expedition controller"));
        }
        if !self.campaign.journey_active {
            return Err(rejected("Choose a country to begin a journey"));
        }
        if self.attempt.is_some() {
            return Err(rejected("Finish the current round before continuing"));
        }
        let report = self
            .last_result
            .as_ref()
            .ok_or_else(|| rejected("No settled round to continue"))?;
        let step = if let Some(country) = &report.country_id {
            let count = crate::world::world()
                .country(country)
                .ok_or_else(|| rejected("Unknown reported country"))?
                .level_count();
            let next = if report.outcome == Outcome::Victory {
                self.campaign.world.completed(country)
            } else {
                report.stage
            };
            if next >= count {
                return Ok(ContinueStep::Liberated);
            }
            ContinueStep::Country {
                country: country.clone(),
                stage: next,
            }
        } else {
            let next = if report.outcome == Outcome::Victory {
                report.level + 1
            } else {
                report.level
            };
            if next >= content().levels.len() {
                return Ok(ContinueStep::Liberated);
            }
            ContinueStep::Level(next)
        };
        self.defender_selectable(self.next_defender())?;
        Ok(step)
    }
    /// [`Self::continue_next_defense`] with the error as its message.
    pub fn continue_journey(&mut self) -> Result<JourneyContinuation, String> {
        self.continue_next_defense()
            .map_err(|error| error.to_string())
    }
    /// `Command::ContinueJourney`: deploys [`Self::next_defender`] on the
    /// journey's next stage (the reported stage again after a loss) and
    /// clears the choice on success (ruling D9). A defender that cannot
    /// defend now is a typed `DefenderUnavailable` with nothing changed.
    pub(crate) fn continue_next_defense(&mut self) -> Result<JourneyContinuation, CommandError> {
        let step = self.continue_step()?;
        let defender = self.next_defender();
        let previous_world = self.campaign.world.clone();
        let operation = match step {
            ContinueStep::Liberated => {
                self.campaign.journey_active = false;
                self.campaign.journey_revision = 1;
                self.repeat_remaining = 0;
                self.set_paused(true);
                return Ok(JourneyContinuation::CountryLiberated);
            }
            ContinueStep::Country { country, stage } => self
                .campaign
                .enter_country(&country)
                .and_then(|()| self.deploy_country_stage_as(stage, defender)),
            ContinueStep::Level(level) => self.deploy_as(level, defender, None),
        };
        if let Err(error) = operation {
            self.campaign.world = previous_world;
            return Err(CommandError::Rejected(error));
        }
        self.campaign.next_defender = None;
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
