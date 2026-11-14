//! Pure reads behind the enemy UI hooks (W3-23; schema v7 §6): what a stage
//! would deploy against, the settled report's run record and the armor the
//! defender's shots meet. No mutation, no RNG.
use super::{Attempt, Enemy, Game, HitSource, enemy_armor_vs};
use crate::content::Mode;
use crate::progression::{BossRank, ProgressionSnapshot};
use crate::stats::RunRecord;

/// What a deploy on one stage of a country's front would face.
#[derive(Clone, Debug, PartialEq)]
pub struct StagePreview {
    /// The difficulty the country plays at (the start clamp included).
    pub difficulty: f32,
    /// The rank of the stage's boss.
    pub boss_rank: BossRank,
    /// The snapshot a deploy of this stage stamps now: the threat rank, the
    /// preset extras, the stage's front ramp and its boss rank.
    pub pressure: ProgressionSnapshot,
    /// The same stage one victory later (rank + 1, the lanes after that
    /// victory), so the two differ only by what the victory changes.
    pub next: ProgressionSnapshot,
}

impl Game {
    /// The preview of 0-based `stage` of country `id` for a defender of
    /// `mode`: [`Self::effective_country_difficulty`],
    /// [`Self::stage_boss_rank`] and `Campaign::progression_for` at the
    /// stage's front position, the derivation `deploy_as` stamps. Errors
    /// outside the catalogue, past the country's last stage and past the
    /// supported threat. Pure: no mutation, no RNG.
    pub fn stage_preview(
        &self,
        id: &str,
        stage: usize,
        mode: Mode,
    ) -> Result<StagePreview, String> {
        let country = crate::world::world()
            .country(id)
            .ok_or_else(|| "Unknown country".to_owned())?;
        if stage >= country.level_count() {
            return Err("This country has no such defense".into());
        }
        let campaign = &self.campaign;
        let front = Some(country.front_position(stage));
        let rank = campaign.progression_rank();
        let pressure = campaign.progression_for(mode, rank, campaign.lane_count(), front)?;
        // Past the supported threat the next victory cannot be previewed.
        let next = campaign
            .progression_for(
                mode,
                rank.saturating_add(1),
                campaign.lane_count_after_victory(),
                front,
            )
            .unwrap_or_else(|_| pressure.clone());
        Ok(StagePreview {
            difficulty: country.effective_difficulty(&campaign.world),
            boss_rank: pressure.boss_rank,
            pressure,
            next,
        })
    }
    /// The run record of the settled report ([`Self::last_result`]): its
    /// main-run record with the report's attempt id, which carries the
    /// run's elite kills and boss time-to-kill. None without a report or
    /// once the history no longer holds the run. Pure.
    pub fn results_run(&self) -> Option<&RunRecord> {
        let attempt = self.last_result.as_ref()?.attempt_id;
        self.campaign
            .run_history
            .iter()
            .rev()
            .find(|run| run.worker_id.is_none() && run.attempt_id == attempt)
    }
}

impl Attempt {
    /// The armor the defender's gunfire meets on `enemy`:
    /// [`Self::enemy_protection`] plus the ranged armor a shield carries
    /// away from the defense line (`HitSource::Shot`). Pure.
    pub fn enemy_shot_protection(&self, enemy: &Enemy) -> i32 {
        enemy_armor_vs(self, enemy, HitSource::Shot)
    }
}
