//! Helpers shared by the integration test crates. Each test binary uses a
//! different subset, so unused items are expected.
#![allow(dead_code)]
use horde_core::balance::BalanceSettings;
use horde_core::campaign::Campaign;
use horde_core::content::{EnemyKind, Mode};
use horde_core::sim::{Game, Input};

/// FNV-1a (64-bit) of `bytes`, as used by save checksums and pinned digests.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_extend(0xcbf2_9ce4_8422_2325, bytes)
}

/// Continue an FNV-1a hash over more `bytes`.
pub fn fnv1a_extend(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A default game with the legacy (fixed, unscaled) balance and features.
pub fn legacy_game() -> Game {
    let mut game = Game::default();
    game.campaign.legacy_features = true;
    game.campaign.balance_settings = BalanceSettings::legacy();
    game
}

/// A legacy game with an unpaused attempt deployed in `mode`.
pub fn game(mode: Mode) -> Game {
    let mut g = legacy_game();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    g
}

/// Spawn a Shambler (or boss) in `lane` and move it to `x`.
pub fn enemy(g: &mut Game, lane: u8, x: f32, boss: bool) {
    let mut a = g.attempt.take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, boss);
    a.enemies.last_mut().unwrap().x = x;
    g.attempt = Some(a);
}

/// Tick with default (idle) input `count` times.
pub fn advance(game: &mut Game, count: u32) {
    for _ in 0..count {
        game.tick(Input::default());
    }
}

/// A fresh campaign with `coins` earned and the named skill nodes bought.
pub fn trained_campaign(coins: u64, nodes: &[&str]) -> Campaign {
    let mut campaign = Campaign::default();
    campaign.reward(coins, 0, Mode::Personal);
    for node in nodes {
        campaign.purchase_node(node).unwrap();
    }
    campaign
}

/// A default game that started its campaign in `country`.
pub fn country_game(country: &str) -> Game {
    let mut game = Game::default();
    game.campaign.choose_start_country(country).unwrap();
    game
}

/// Decode a JSON game value as a v1 save envelope with a valid checksum, the
/// way an old build's save would reach migration.
pub fn decode_value(value: serde_json::Value) -> Result<Game, String> {
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = fnv1a(payload.as_bytes());
    Ok(Game::decode_save_generation(
        &serde_json::to_vec(&serde_json::json!({
            "version": 1, "checksum": checksum, "payload": payload,
        }))
        .unwrap(),
    )?)
}

/// A legacy game with four cleared countries, 10k coins and a started campaign,
/// ready to recruit survivors and deploy parallel expeditions.
pub fn strategic_game() -> Game {
    let mut g = legacy_game();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    g.campaign.choose_start_country(&countries[0].0).unwrap();
    for (id, count) in countries {
        g.campaign.world.country_levels.insert(id.clone(), count);
        g.campaign.world.cleared.push(id);
    }
    g.campaign.coins = 10000;
    g.campaign.attempts = 1;
    g
}
