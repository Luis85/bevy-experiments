//! The W2-42 plan steps that need no full run: the farm decision and the
//! camp sale.
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
