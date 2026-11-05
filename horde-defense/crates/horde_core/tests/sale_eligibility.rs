//! One sale rule (LE-1, W1-19): `Game::sale_eligibility` is the single public
//! eligibility read, and every batch sell path (`SellUnprotected`,
//! `Game::sell_extras` and its quote) sells exactly its Ok set. `SellItem`
//! keeps its pre-strict behaviour until W1-20: it still sells through the
//! equipped, preset, recruit and NPC-issue blocks (replacing the slot with
//! protected starter equipment) and refuses every other block with the
//! block's label.
use horde_core::{
    campaign::{Campaign, SaleBlock, SaleQuote},
    command::{Command, CommandError, CommandOutcome},
    content::Mode,
    sim::Game,
    world::world,
};
use std::collections::BTreeSet;

/// A legacy game with `count` liberated countries, recruitment and garrisons
/// unlocked, plenty of coins and no live attempt (so preparation is open).
fn liberated_game(count: usize) -> Game {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    let ids: Vec<_> = world()
        .countries
        .iter()
        .take(count)
        .map(|c| c.id.clone())
        .collect();
    c.choose_start_country(&ids[0]).unwrap();
    for id in ids {
        let levels = world().country(&id).unwrap().level_count();
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id);
    }
    c.coins = 10_000;
    let mut game = Game::default();
    *game.staging().campaign() = c;
    game
}

fn apply(game: &mut Game, command: Command) -> CommandOutcome {
    game.apply(&command)
        .unwrap_or_else(|e| panic!("{command:?} failed: {e}"))
}

fn id(outcome: CommandOutcome) -> u64 {
    match outcome {
        CommandOutcome::Id(id) => id,
        other => panic!("expected an id, got {other:?}"),
    }
}

/// The ids of the fixture, one per `SaleBlock` category plus one sellable
/// extra and one id that was never owned.
struct Fixture {
    game: Game,
    recruit_name: String,
    cases: Vec<(u64, Result<SaleQuote, SaleBlock>)>,
}

fn fixture() -> Fixture {
    let mut g = liberated_game(4);
    let countries = g.campaign().world.cleared.clone();
    let buy = |g: &mut Game, definition: usize| g.campaign_mut().add_item(definition).unwrap();

    let sellable = buy(&mut g, 5);
    let locked = buy(&mut g, 6);
    apply(
        &mut g,
        Command::Lock {
            id: locked,
            locked: true,
        },
    );
    let favorite = buy(&mut g, 7);
    apply(
        &mut g,
        Command::Favorite {
            id: favorite,
            favorite: true,
        },
    );
    // A legacy unprotected copy of the NPC-issue carbine (definition 0).
    let npc_only = buy(&mut g, 0);
    let personal = buy(&mut g, 5);
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Personal,
            id: personal,
        },
    );
    let mara = buy(&mut g, 5);
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Npc,
            id: mara,
        },
    );
    // Save a preset holding a vest, then put the starter outfit back on.
    let preset = buy(&mut g, 6);
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Personal,
            id: preset,
        },
    );
    apply(
        &mut g,
        Command::SavePreset {
            name: "Vest".into(),
            mode: Mode::Personal,
        },
    );
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Personal,
            id: 3,
        },
    );
    // One idle recruit holds a rifle; a second guards a country with an axe.
    let equipped_recruit = id(apply(&mut g, Command::Recruit(countries[1].clone())));
    let recruit_rifle = buy(&mut g, 4);
    apply(
        &mut g,
        Command::EquipRecruit {
            id: equipped_recruit,
            item: recruit_rifle,
        },
    );
    let guard = id(apply(&mut g, Command::Recruit(countries[2].clone())));
    let reserved = buy(&mut g, 5);
    apply(
        &mut g,
        Command::EquipRecruit {
            id: guard,
            item: reserved,
        },
    );
    apply(
        &mut g,
        Command::AssignGuard {
            id: guard,
            country: countries[2].clone(),
        },
    );
    let recruit_name = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == equipped_recruit)
        .unwrap()
        .name
        .clone();
    let value = |g: &Game, id: u64| g.campaign().item_sale_value(id).unwrap();
    let cases = vec![
        (
            sellable,
            Ok(SaleQuote {
                coins: value(&g, sellable),
            }),
        ),
        (999_999, Err(SaleBlock::Unknown)),
        (reserved, Err(SaleBlock::Reserved)),
        (1, Err(SaleBlock::Protected)),
        (locked, Err(SaleBlock::Locked)),
        (favorite, Err(SaleBlock::Favorite)),
        (npc_only, Err(SaleBlock::NpcOnly)),
        (personal, Err(SaleBlock::EquippedPersonal)),
        (mara, Err(SaleBlock::EquippedMara)),
        (
            preset,
            Err(SaleBlock::Preset {
                names: vec!["Vest".into()],
            }),
        ),
        (
            recruit_rifle,
            Err(SaleBlock::RecruitEquipped {
                name: recruit_name.clone(),
            }),
        ),
    ];
    Fixture {
        game: g,
        recruit_name,
        cases,
    }
}

/// The owned ids `sale_eligibility` accepts, and their summed quote.
fn ok_set(game: &Game) -> (BTreeSet<u64>, u64) {
    let mut ids = BTreeSet::new();
    let mut coins = 0;
    for item in &game.campaign().items {
        if let Ok(quote) = game.sale_eligibility(item.id) {
            ids.insert(item.id);
            coins += quote.coins;
        }
    }
    (ids, coins)
}

fn owned(game: &Game) -> BTreeSet<u64> {
    game.campaign().items.iter().map(|i| i.id).collect()
}

#[test]
fn sale_eligibility_is_pure() {
    let Fixture { game, cases, .. } = fixture();
    let mut deployed = game.clone();
    deployed.deploy(0, Mode::Personal).unwrap();
    for g in [game, deployed] {
        let before = serde_json::to_value(&g).unwrap();
        let strategy_rng = g.strategy().rng.state;
        let attempt_rng = g.attempt().map(|a| a.rng.state);
        for (id, _) in &cases {
            let _ = g.sale_eligibility(*id);
        }
        for item in &g.campaign().items {
            let _ = g.sale_eligibility(item.id);
        }
        let _ = g.sell_extras_quote();
        assert_eq!(serde_json::to_value(&g).unwrap(), before);
        assert_eq!(g.strategy().rng.state, strategy_rng);
        assert_eq!(g.attempt().map(|a| a.rng.state), attempt_rng);
    }
}

#[test]
fn sell_paths_share_one_eligibility_function() {
    let Fixture {
        game,
        recruit_name,
        cases,
    } = fixture();
    assert!(game.strategy().validate(game.campaign()).is_ok());
    for (id, expected) in &cases {
        assert_eq!(&game.sale_eligibility(*id), expected, "item {id}");
    }
    let (ids, coins) = ok_set(&game);
    assert_eq!(ids, BTreeSet::from([cases[0].0]));
    assert!(coins > 0);

    // The quote and both batch sales realise exactly the Ok set.
    assert_eq!(game.sell_extras_quote(), coins);
    let mut command = game.clone();
    assert_eq!(
        apply(&mut command, Command::SellUnprotected),
        CommandOutcome::Amount(coins)
    );
    let mut direct = game.clone();
    assert_eq!(direct.sell_extras(), Ok(coins));
    for sold in [&command, &direct] {
        assert_eq!(owned(sold), &owned(&game) - &ids);
        assert_eq!(sold.campaign().coins, game.campaign().coins + coins);
        assert!(sold.strategy().validate(sold.campaign()).is_ok());
    }

    // While a defender is deployed every owned item is InBattle: the quote is
    // 0 and SellUnprotected still answers Amount(0), as before.
    let mut deployed = game.clone();
    deployed.deploy(0, Mode::Personal).unwrap();
    for item in &deployed.campaign().items {
        assert_eq!(deployed.sale_eligibility(item.id), Err(SaleBlock::InBattle));
    }
    assert_eq!(deployed.sale_eligibility(999_999), Err(SaleBlock::Unknown));
    assert_eq!(deployed.sell_extras_quote(), 0);
    let before = owned(&deployed);
    assert_eq!(
        apply(&mut deployed, Command::SellUnprotected),
        CommandOutcome::Amount(0)
    );
    assert_eq!(owned(&deployed), before);

    // Old SellItem behaviour, kept until W1-20 makes it strict: the equipped,
    // preset, recruit-equipped and NPC-issue blocks are sold through (the
    // slot falls back to protected equipment); every other block is refused
    // with its label and nothing changes.
    for (id, expected) in &cases {
        let mut g = game.clone();
        let before = serde_json::to_value(&g).unwrap();
        let result = g.apply(&Command::SellItem(*id));
        match expected {
            Ok(quote) => assert_eq!(result, Ok(CommandOutcome::Amount(quote.coins))),
            Err(
                SaleBlock::NpcOnly
                | SaleBlock::EquippedPersonal
                | SaleBlock::EquippedMara
                | SaleBlock::Preset { .. }
                | SaleBlock::RecruitEquipped { .. },
            ) => {
                let coins = game.campaign().item_sale_value(*id).unwrap();
                assert_eq!(result, Ok(CommandOutcome::Amount(coins)), "item {id}");
                assert!(g.campaign().item(*id).is_none());
                assert!(g.strategy().validate(g.campaign()).is_ok());
            }
            Err(block) => {
                assert_eq!(result, Err(CommandError::Rejected(block.label())));
                assert_eq!(serde_json::to_value(&g).unwrap(), before);
            }
        }
    }
    // The recruit-equipped label names the survivor.
    assert!(
        SaleBlock::RecruitEquipped {
            name: recruit_name.clone()
        }
        .label()
        .contains(&recruit_name)
    );
}
