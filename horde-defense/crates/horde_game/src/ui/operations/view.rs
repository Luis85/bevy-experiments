//! Pure operations view model: world clock, outbreaks, expeditions and unlocks.
use horde_core::{
    sim::{Game, Outcome},
    strategy::OutbreakOutcome,
    world::world,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutbreakLine {
    pub country_id: String,
    pub country: String,
    pub strength: u32,
    pub seconds_left: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ExpeditionLine {
    pub id: u64,
    pub recruit: String,
    pub country: String,
    pub stage: usize,
    pub hp: i32,
    pub max_hp: i32,
    pub elapsed: f32,
    pub kills: u32,
    pub decision: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ExpeditionReport {
    pub id: u64,
    pub country: String,
    pub stage: usize,
    pub outcome: Outcome,
    pub kills: u32,
    pub coins: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OperationsView {
    pub paused: bool,
    pub field_active: bool,
    /// Active outbreaks only.
    pub outbreaks: Vec<OutbreakLine>,
    /// Unsettled expeditions with a running attempt.
    pub live: Vec<ExpeditionLine>,
    /// Whether any expedition has settled (reports may still be absent).
    pub any_settled: bool,
    pub reports: Vec<ExpeditionReport>,
    pub recruit_unlocked: bool,
    pub parallel_unlocked: bool,
    pub strategy_unlocked: bool,
}

pub(super) fn country_name(id: &str) -> &str {
    world()
        .country(id)
        .map(|country| country.name.as_str())
        .unwrap_or(id)
}

pub(super) fn view_model(game: &Game) -> OperationsView {
    let strategy = game.strategy();
    let outbreaks = strategy
        .outbreaks
        .iter()
        .filter(|o| o.outcome == OutbreakOutcome::Active)
        .map(|o| OutbreakLine {
            country_id: o.country_id.clone(),
            country: country_name(&o.country_id).to_owned(),
            strength: o.strength,
            seconds_left: o.deadline_tick.saturating_sub(strategy.tick) as f32 / 60.0,
        })
        .collect();
    let live = game
        .battles()
        .iter()
        .filter(|b| !b.settled)
        .filter_map(|b| {
            let a = b.game().attempt()?;
            Some(ExpeditionLine {
                id: b.id,
                recruit: strategy
                    .roster
                    .iter()
                    .find(|r| r.id == b.recruit_id)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| "Survivor".into()),
                country: country_name(&b.country_id).to_owned(),
                stage: b.stage,
                hp: a.hp,
                max_hp: a.max_hp,
                elapsed: a.elapsed(),
                kills: a.kills,
                decision: a.decision.clone(),
            })
        })
        .collect();
    let reports = game
        .battles()
        .iter()
        .filter(|b| b.settled)
        .filter_map(|b| {
            let report = b.game().last_result()?;
            Some(ExpeditionReport {
                id: b.id,
                country: country_name(&b.country_id).to_owned(),
                stage: b.stage,
                outcome: report.outcome,
                kills: report.kills,
                coins: report.coins,
            })
        })
        .collect();
    OperationsView {
        paused: game.global_paused(),
        field_active: game.attempt().is_some(),
        outbreaks,
        live,
        any_settled: game.battles().iter().any(|b| b.settled),
        reports,
        recruit_unlocked: strategy.recruitment_unlocked(game.campaign()),
        parallel_unlocked: strategy.parallel_unlocked(game.campaign()),
        strategy_unlocked: strategy.grand_strategy_unlocked(game.campaign()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_world_has_no_operations_and_locked_orders() {
        let game = Game::default();
        let view = view_model(&game);
        assert_eq!(view.paused, game.global_paused());
        assert!(!view.field_active);
        assert!(view.outbreaks.is_empty());
        assert!(view.live.is_empty());
        assert!(!view.any_settled);
        assert!(view.reports.is_empty());
        assert!(!view.recruit_unlocked);
        assert!(!view.parallel_unlocked);
        assert!(!view.strategy_unlocked);
    }

    #[test]
    fn active_outbreaks_report_country_and_remaining_seconds() {
        let mut game = Game::default();
        let country = world().countries[0].clone();
        game.strategy_mut().tick = 120;
        game.strategy_mut()
            .outbreaks
            .push(horde_core::strategy::Outbreak {
                id: 1,
                country_id: country.id.clone(),
                started_tick: 0,
                deadline_tick: 720,
                strength: 3,
                enemy: horde_core::rules::InjuryState::new(10, 10),
                outcome: OutbreakOutcome::Active,
                ready_turns: 0,
            });
        let mut resolved = game.strategy().outbreaks[0].clone();
        resolved.id = 2;
        resolved.outcome = OutbreakOutcome::Contained;
        game.strategy_mut().outbreaks.push(resolved);
        let view = view_model(&game);
        assert_eq!(
            view.outbreaks,
            vec![OutbreakLine {
                country_id: country.id.clone(),
                country: country.name.clone(),
                strength: 3,
                seconds_left: 10.0,
            }]
        );
    }

    #[test]
    fn unknown_country_ids_fall_back_to_the_id() {
        assert_eq!(country_name("XXX"), "XXX");
        let first = &world().countries[0];
        assert_eq!(country_name(&first.id), first.name);
    }
}
