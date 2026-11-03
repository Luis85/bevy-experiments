pub fn legacy_game() -> horde_core::sim::Game {
    let mut game = horde_core::sim::Game::default();
    game.campaign.legacy_features = true;
    game.campaign.balance_settings = horde_core::balance::BalanceSettings::legacy();
    game
}
