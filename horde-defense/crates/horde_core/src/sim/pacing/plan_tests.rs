//! The W2-42 plan steps that need no full run: the farm decision, the camp
//! sale and who a takeover plan sends next.
use super::*;

#[test]
fn farm_step_replays_the_stage_before_the_frontier_after_n_defeats_then_returns() {
    let farm = PacingFarm::ReplayAfterDefeats(2);
    assert_eq!(farm_step(farm, 3, 1, false), FarmStep::Continue);
    assert_eq!(farm_step(farm, 3, 2, false), FarmStep::Replay(2));
    assert_eq!(farm_step(farm, 3, 5, false), FarmStep::Replay(2));
    // The run after a replay goes back to the frontier, win or lose.
    assert_eq!(farm_step(farm, 3, 0, true), FarmStep::Frontier(3));
    // The first stage has nothing before it to replay.
    assert_eq!(farm_step(farm, 0, 9, false), FarmStep::Continue);
    // Zero defeats never trigger a replay, and no farm plan always continues.
    let never = PacingFarm::ReplayAfterDefeats(0);
    assert_eq!(farm_step(never, 3, 0, false), FarmStep::Continue);
    assert_eq!(farm_step(never, 3, 9, false), FarmStep::Continue);
    assert_eq!(farm_step(PacingFarm::None, 3, 9, true), FarmStep::Continue);
}

#[test]
fn camp_sale_sells_the_unprotected_items_only_under_manual_extras() {
    let mut g = Game::default();
    let extra = g.campaign.add_item(5).expect("item added");
    let quote = g.sell_extras_quote();
    assert!(quote > 0, "the unequipped item is an extra");
    let coins = g.campaign.coins;
    for sell in [PacingSell::Never, PacingSell::Auto] {
        camp_sale(&mut g, sell).expect("no camp sale");
        assert_eq!(g.campaign.coins, coins, "{sell:?} sells nothing in camp");
        assert!(g.campaign.item(extra).is_some());
    }
    camp_sale(&mut g, PacingSell::ManualExtras).expect("extras sold");
    assert_eq!(g.campaign.coins, coins + quote);
    assert!(g.campaign.item(extra).is_none());
    // Nothing left to sell is not an error.
    camp_sale(&mut g, PacingSell::ManualExtras).expect("nothing to sell");
    assert_eq!(g.campaign.coins, coins + quote);
}

/// A legacy game with four liberated countries (recruitment unlocked), two
/// recruits from the first and a finished, withdrawn Personal run. Returns
/// the game and the recruit ids, lowest first.
fn takeover_game() -> (Game, u64, u64) {
    let mut c = crate::campaign::Campaign {
        legacy_features: true,
        ..crate::campaign::Campaign::default()
    };
    let ids: Vec<String> = crate::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| c.id.clone())
        .collect();
    c.choose_start_country(&ids[0]).expect("start country");
    for id in &ids {
        let levels = crate::world::world()
            .country(id)
            .expect("country")
            .level_count();
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id.clone());
    }
    c.coins = 10_000;
    c.attempts = 1;
    let mut g = Game {
        campaign: c,
        ..Game::default()
    };
    let mut recruit = || match g.apply(&Command::Recruit(ids[0].clone())) {
        Ok(crate::command::CommandOutcome::Id(id)) => id,
        other => panic!("recruiting failed: {other:?}"),
    };
    let (first, second) = (recruit(), recruit());
    g.apply(&Command::Deploy {
        stage: 0,
        mode: Mode::Personal,
    })
    .expect("deployed");
    g.apply(&Command::Withdraw).expect("withdrawn");
    (g, first.min(second), first.max(second))
}

/// The takeover plan's survivor-retry rule without a full run: a recruit's
/// lost run (defeat or withdrawal) sends the survivor next; any other last
/// run sends the lowest-id recruit that can defend now, else the survivor.
#[test]
fn takeover_defender_sends_the_survivor_after_a_recruit_loses() {
    fn report(g: &mut Game) -> &mut Report {
        g.last_result.as_mut().expect("a finished run")
    }
    let (mut g, low, high) = takeover_game();
    // The survivor's own loss hands the next run to the lowest-id recruit.
    assert_eq!(report(&mut g).defender(), DefenderId::Player);
    for outcome in [Outcome::Withdrawal, Outcome::Defeat] {
        report(&mut g).outcome = outcome;
        assert_eq!(takeover_defender(&g), DefenderId::Recruit(low));
    }
    // Mara losing is no recruit loss.
    report(&mut g).mode = Mode::Npc;
    assert_eq!(report(&mut g).defender(), DefenderId::Mara);
    assert_eq!(takeover_defender(&g), DefenderId::Recruit(low));
    // A recruit that lost, by defeat or withdrawal, sends the survivor back.
    report(&mut g).seat = DefenderSeat::Recruit(high);
    assert_eq!(report(&mut g).defender(), DefenderId::Recruit(high));
    for outcome in [Outcome::Defeat, Outcome::Withdrawal] {
        report(&mut g).outcome = outcome;
        assert_eq!(
            takeover_defender(&g),
            DefenderId::Player,
            "{outcome:?}: the survivor retries the stage"
        );
    }
    // A recruit's victory keeps the recruits defending.
    report(&mut g).outcome = Outcome::Victory;
    assert_eq!(takeover_defender(&g), DefenderId::Recruit(low));
    // No finished run yet: the lowest-id recruit.
    g.last_result = None;
    assert_eq!(takeover_defender(&g), DefenderId::Recruit(low));
    // Only a recruit that can defend now is sent, else the survivor.
    let country = g.campaign.world.cleared[0].clone();
    g.apply(&Command::AssignGuard { id: low, country })
        .expect("guarding");
    assert_eq!(takeover_defender(&g), DefenderId::Recruit(high));
    g.strategy.roster.clear();
    assert_eq!(takeover_defender(&g), DefenderId::Player);
}
