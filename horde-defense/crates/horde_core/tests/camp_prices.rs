//! Threat-scaled camp prices (W2-18; rulings R10 and D15, design
//! progression-tree section 8): recruiting and NPC training charge
//! `camp_price(base, progression_rank)`, and no core purchase path keeps the
//! flat 25 or the raw `recruit_cost`.
use horde_core::{
    campaign::Campaign,
    command::{Command, CommandOutcome},
    sim::Game,
    world::world,
};

/// Threat ranks of the design table; 80 is past the 6x cap.
const RANKS: [u64; 6] = [0, 1, 10, 20, 50, 80];

/// A legacy camp with two liberated countries (recruitment open), plenty of
/// coins, no live attempt and `victories = rank` (threat revision 1, so the
/// progression rank is exactly `rank`).
fn camp(rank: u64) -> Game {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    let ids: Vec<_> = world()
        .countries
        .iter()
        .take(2)
        .map(|c| c.id.clone())
        .collect();
    c.choose_start_country(&ids[0]).unwrap();
    for id in ids {
        let levels = world().country(&id).unwrap().level_count();
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id);
    }
    c.coins = 10_000;
    c.victories = rank;
    let mut game = Game::default();
    *game.staging().campaign() = c;
    assert_eq!(u64::from(game.campaign().progression_rank()), rank);
    game
}

/// Coins `command` charged; panics when it is rejected.
fn charged(game: &mut Game, command: Command) -> (u64, CommandOutcome) {
    let before = game.campaign().coins;
    let outcome = game
        .apply(&command)
        .unwrap_or_else(|error| panic!("{command:?} rejected: {error}"));
    (before - game.campaign().coins, outcome)
}

#[test]
fn recruit_and_training_prices_scale_with_threat() {
    let (mut recruit, mut mara, mut survivor) = (Vec::new(), Vec::new(), Vec::new());
    for rank in RANKS {
        let mut game = camp(rank);
        let country = game.campaign().world.cleared[0].clone();
        let (price, outcome) = charged(&mut game, Command::Recruit(country.clone()));
        let CommandOutcome::Id(id) = outcome else {
            panic!("Recruit returned {outcome:?}");
        };
        assert_eq!(price, game.campaign().recruit_price(), "rank {rank}");
        recruit.push(price);
        let (price, _) = charged(&mut game, Command::BuyNpcPoint);
        assert_eq!(price, game.campaign().npc_training_price(), "rank {rank}");
        mara.push(price);
        let (price, _) = charged(&mut game, Command::BuyRecruitPoint(id));
        survivor.push(price);

        // One coin short of the scaled price is rejected and charges nothing.
        let mut short = camp(rank);
        short.staging().campaign().coins = short.campaign().recruit_price() - 1;
        assert!(
            short.apply(&Command::Recruit(country)).is_err(),
            "rank {rank}"
        );
        short.staging().campaign().coins = short.campaign().npc_training_price() - 1;
        let coins = short.campaign().coins;
        assert!(short.apply(&Command::BuyNpcPoint).is_err(), "rank {rank}");
        assert_eq!(short.campaign().coins, coins, "rank {rank}");
    }
    assert_eq!(recruit, [100, 110, 200, 300, 600, 600]);
    assert_eq!(mara, [25, 30, 50, 75, 150, 150]);
    assert_eq!(survivor, mara, "a recruit's training point costs the same");
}

/// Non-test source of a core purchase path.
fn production(source: &'static str) -> &'static str {
    source.split("#[cfg(test)]").next().unwrap()
}

/// Whether `line` holds an integer literal `25` token (not `25.0`, `250`
/// or `x25`).
fn has_literal_25(line: &str) -> bool {
    let bytes = line.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'.';
    line.match_indices("25").any(|(at, _)| {
        (at == 0 || !word(bytes[at - 1])) && bytes.get(at + 2).is_none_or(|&b| !word(b))
    })
}

/// The one allowed `recruit_cost` read: the base of `Campaign::recruit_price`.
const RECRUIT_PRICE_BASE: &str = "let base = crate::world::world().strategy_config.recruit_cost;";

#[test]
fn camp_price_scan_flags_literals_and_raw_recruit_cost() {
    // Perturbation proof for the scan below: the pre-W2-18 lines are found.
    for line in [
        "        self.spend(25)?;",
        "            && g.campaign.coins >= 25",
        "RecommendationAction::BuyTraining,25);",
    ] {
        assert!(has_literal_25(line), "{line}");
    }
    for line in ["a.stamina >= 25.0", "250", "x25", "1.25"] {
        assert!(!has_literal_25(line), "{line}");
    }
    assert!(!"let price = world().strategy_config.recruit_cost;".contains(RECRUIT_PRICE_BASE));
}

#[test]
fn core_purchase_paths_have_no_literal_camp_prices() {
    let paths = [
        (
            "strategy.rs",
            production(include_str!("../src/strategy.rs")),
        ),
        (
            "campaign/progression.rs",
            production(include_str!("../src/campaign/progression.rs")),
        ),
        (
            "campaign/loadout.rs",
            production(include_str!("../src/campaign/loadout.rs")),
        ),
        (
            "sim/smoke.rs",
            production(include_str!("../src/sim/smoke.rs")),
        ),
    ];
    let mut found = Vec::new();
    let mut bases = 0;
    for (path, source) in paths {
        for (number, line) in source.lines().enumerate() {
            let code = line.trim();
            if code.starts_with("//") {
                continue;
            }
            let at = format!("{path}:{}", number + 1);
            if has_literal_25(code) {
                found.push(format!("{at}: literal 25 in {code:?}"));
            }
            if code.contains("recruit_cost") {
                if path == "campaign/progression.rs" && code == RECRUIT_PRICE_BASE {
                    bases += 1;
                } else {
                    found.push(format!("{at}: raw recruit_cost in {code:?}"));
                }
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(
        bases, 1,
        "Campaign::recruit_price reads the recruit_cost base"
    );
}
