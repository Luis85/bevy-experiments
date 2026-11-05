//! One sale rule (LE-1): `Game::sale_eligibility` is the single public
//! eligibility read (W1-19), and every sell path agrees with it. Since W1-20
//! (ruleset 2) `SellItem` is strict: it sells exactly when `sale_eligibility`
//! is Ok, for the quoted coins, and otherwise answers the typed
//! `CommandError::SaleBlocked(block)` without changing anything. No loadout
//! slot is ever replaced by a sale.
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

    // The recruit-equipped label names the survivor.
    assert!(
        SaleBlock::RecruitEquipped {
            name: recruit_name.clone()
        }
        .label()
        .contains(&recruit_name)
    );
}

/// Every block refuses `SellItem` with exactly that typed block and no
/// mutation: no slot falls back to starter equipment and no preset or
/// recruit loadout is broken. The one eligible item sells for its quote.
#[test]
fn strict_sell_rejects_equipped_preset_reserved_recruit_and_npc_only() {
    let Fixture { game, cases, .. } = fixture();
    for (id, expected) in &cases {
        let mut g = game.clone();
        let before = serde_json::to_value(&g).unwrap();
        let result = g.apply(&Command::SellItem(*id));
        match expected {
            Ok(quote) => {
                assert_eq!(result, Ok(CommandOutcome::Amount(quote.coins)));
                assert!(g.campaign().item(*id).is_none());
                assert_eq!(g.campaign().coins, game.campaign().coins + quote.coins);
                assert_eq!(g.campaign().player.loadout, game.campaign().player.loadout);
                assert_eq!(g.campaign().npc.loadout, game.campaign().npc.loadout);
                assert_eq!(g.campaign().presets, game.campaign().presets);
            }
            Err(block) => {
                assert_eq!(
                    result,
                    Err(CommandError::SaleBlocked(block.clone())),
                    "item {id}"
                );
                assert_eq!(serde_json::to_value(&g).unwrap(), before, "item {id}");
            }
        }
        assert!(g.strategy().validate(g.campaign()).is_ok());
    }
    // In battle every owned item is InBattle for SellItem too.
    let mut deployed = game.clone();
    deployed.deploy(0, Mode::Personal).unwrap();
    let before = serde_json::to_value(&deployed).unwrap();
    assert_eq!(
        deployed.apply(&Command::SellItem(cases[0].0)),
        Err(CommandError::SaleBlocked(SaleBlock::InBattle))
    );
    assert_eq!(serde_json::to_value(&deployed).unwrap(), before);
}

/// A blocked sale's error Display is the block's plain label, and the wallet
/// limit is its own typed error that sells nothing, for one item or a batch.
#[test]
fn sale_errors_are_typed_with_plain_text() {
    let Fixture { game, cases, .. } = fixture();
    for (_, expected) in &cases {
        if let Err(block) = expected {
            let error = CommandError::SaleBlocked(block.clone());
            assert_eq!(error.to_string(), block.label());
        }
    }
    assert_eq!(
        CommandError::WalletLimit.to_string(),
        "Sale would exceed the wallet limit"
    );
    let mut full = game.clone();
    full.campaign_mut().coins = u64::MAX;
    let before = serde_json::to_value(&full).unwrap();
    assert_eq!(
        full.apply(&Command::SellItem(cases[0].0)),
        Err(CommandError::WalletLimit)
    );
    assert_eq!(
        full.apply(&Command::SellUnprotected),
        Err(CommandError::WalletLimit)
    );
    assert_eq!(serde_json::to_value(&full).unwrap(), before);
}

/// One item that fails every guard reports the first in declaration order;
/// lifting each guard reveals the next one, and every read (repeated, on a
/// clone, through `SellItem`) gives the same answer.
#[test]
fn sale_block_order_is_deterministic() {
    let mut g = liberated_game(2);
    let country = g.campaign().world.cleared[1].clone();
    let starter = g.campaign().player.loadout;
    let mara_starter = g.campaign().npc.loadout;
    let item = g.campaign_mut().add_item(5).unwrap();
    let slot = g.campaign().item(item).unwrap().definition().slot.index();
    let recruit = id(apply(&mut g, Command::Recruit(country)));
    let recruit_name = g.strategy().roster[0].name.clone();
    let recruit_starter = g.strategy().roster[0].npc.loadout[slot];
    for command in [
        Command::EquipRecruit { id: recruit, item },
        Command::Equip {
            mode: Mode::Personal,
            id: item,
        },
        Command::SavePreset {
            name: "Zulu".into(),
            mode: Mode::Personal,
        },
        Command::SavePreset {
            name: "Alpha".into(),
            mode: Mode::Personal,
        },
        Command::Equip {
            mode: Mode::Npc,
            id: item,
        },
        Command::Favorite {
            id: item,
            favorite: true,
        },
        Command::Lock {
            id: item,
            locked: true,
        },
    ] {
        apply(&mut g, command);
    }
    let first_preset = g.campaign().presets.len() - 2;
    let lifts = [
        (
            SaleBlock::Locked,
            Command::Lock {
                id: item,
                locked: false,
            },
        ),
        (
            SaleBlock::Favorite,
            Command::Favorite {
                id: item,
                favorite: false,
            },
        ),
        (
            SaleBlock::EquippedPersonal,
            Command::Equip {
                mode: Mode::Personal,
                id: starter[slot],
            },
        ),
        (
            SaleBlock::EquippedMara,
            Command::Equip {
                mode: Mode::Npc,
                id: mara_starter[slot],
            },
        ),
        // Preset names are reported in preset order, not sorted.
        (
            SaleBlock::Preset {
                names: vec!["Zulu".into(), "Alpha".into()],
            },
            Command::DeletePreset {
                index: first_preset,
            },
        ),
        (
            SaleBlock::Preset {
                names: vec!["Alpha".into()],
            },
            Command::DeletePreset {
                index: first_preset,
            },
        ),
        (
            SaleBlock::RecruitEquipped {
                name: recruit_name.clone(),
            },
            Command::EquipRecruit {
                id: recruit,
                item: recruit_starter,
            },
        ),
    ];
    for (expected, lift) in lifts {
        for _ in 0..2 {
            assert_eq!(g.sale_eligibility(item), Err(expected.clone()));
            assert_eq!(g.clone().sale_eligibility(item), Err(expected.clone()));
        }
        assert_eq!(
            g.clone().apply(&Command::SellItem(item)),
            Err(CommandError::SaleBlocked(expected))
        );
        apply(&mut g, lift);
    }
    let coins = g.campaign().item_sale_value(item).unwrap();
    assert_eq!(g.sale_eligibility(item), Ok(SaleQuote { coins }));
    assert_eq!(
        apply(&mut g, Command::SellItem(item)),
        CommandOutcome::Amount(coins)
    );
}

/// `DeletePreset` is the way out of the `Preset` block: it removes only that
/// preset, needs preparation and rejects an unknown index without mutation.
#[test]
fn delete_preset_unblocks_sale() {
    let mut g = liberated_game(1);
    let item = g.campaign_mut().add_item(6).unwrap();
    let starter = g.campaign().player.loadout;
    let slot = g.campaign().item(item).unwrap().definition().slot.index();
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Personal,
            id: item,
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
            id: starter[slot],
        },
    );
    let index = g.campaign().presets.len() - 1;
    let blocked = SaleBlock::Preset {
        names: vec!["Vest".into()],
    };
    assert_eq!(g.sale_eligibility(item), Err(blocked.clone()));
    assert_eq!(
        g.clone().apply(&Command::SellItem(item)),
        Err(CommandError::SaleBlocked(blocked))
    );
    // An unknown index and a live defense are refused without mutation.
    let before = serde_json::to_value(&g).unwrap();
    assert!(matches!(
        g.apply(&Command::DeletePreset { index: index + 1 }),
        Err(CommandError::Rejected(_))
    ));
    assert_eq!(serde_json::to_value(&g).unwrap(), before);
    let mut deployed = g.clone();
    deployed.deploy(0, Mode::Personal).unwrap();
    let before_deployed = serde_json::to_value(&deployed).unwrap();
    assert!(deployed.apply(&Command::DeletePreset { index }).is_err());
    assert_eq!(serde_json::to_value(&deployed).unwrap(), before_deployed);

    let others: Vec<_> = g.campaign().presets[..index].to_vec();
    assert_eq!(
        apply(&mut g, Command::DeletePreset { index }),
        CommandOutcome::Applied
    );
    assert_eq!(g.campaign().presets, others);
    let coins = g.campaign().item_sale_value(item).unwrap();
    assert_eq!(g.sale_eligibility(item), Ok(SaleQuote { coins }));
    assert_eq!(
        apply(&mut g, Command::SellItem(item)),
        CommandOutcome::Amount(coins)
    );
    assert!(g.campaign().validate().is_ok());
}

/// A small deterministic generator for the property test below.
struct SplitMix(u64);
impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A random but reproducible stash: bought items, equips for both modes,
/// locks, favourites, presets, recruits with equipment and guards. Rejected
/// commands are skipped; none may leave the strategy state invalid.
fn random_game(seed: u64) -> Game {
    let mut rng = SplitMix(seed);
    let mut g = liberated_game(4);
    let countries = g.campaign().world.cleared.clone();
    let definitions = horde_core::content::content().items.len();
    for _ in 0..4 + rng.below(6) {
        let definition = if rng.below(10) == 0 {
            0
        } else {
            1 + rng.below(definitions - 1)
        };
        g.campaign_mut().add_item(definition).unwrap();
    }
    for step in 0..24 {
        let owned: Vec<u64> = g.campaign().items.iter().map(|i| i.id).collect();
        let item = owned[rng.below(owned.len())];
        let roster: Vec<u64> = g.strategy().roster.iter().map(|r| r.id).collect();
        let country = countries[rng.below(countries.len())].clone();
        let mode = if rng.below(2) == 0 {
            Mode::Personal
        } else {
            Mode::Npc
        };
        let command = match rng.below(10) {
            0..=2 => Command::Equip { mode, id: item },
            3 => Command::Lock {
                id: item,
                locked: rng.below(2) == 0,
            },
            4 => Command::Favorite {
                id: item,
                favorite: rng.below(2) == 0,
            },
            5 => Command::SavePreset {
                name: format!("P{}", rng.below(3)),
                mode,
            },
            6 => Command::DeletePreset {
                index: rng.below(3),
            },
            7 => Command::Recruit(country),
            8 if !roster.is_empty() => Command::EquipRecruit {
                id: roster[rng.below(roster.len())],
                item,
            },
            9 if !roster.is_empty() => Command::AssignGuard {
                id: roster[rng.below(roster.len())],
                country,
            },
            _ => Command::SellItem(item),
        };
        let _ = g.apply(&command);
        assert!(
            g.strategy().validate(g.campaign()).is_ok(),
            "seed {seed} step {step}: {command:?} broke the strategy state"
        );
    }
    g
}

/// The variant name of a block, for the coverage check.
fn block_name(block: &SaleBlock) -> String {
    let debug = format!("{block:?}");
    debug.split([' ', '{']).next().unwrap().to_owned()
}

/// Property over 120 seeds: for every owned id (and one unknown id), in and
/// out of battle, `SellItem` succeeds exactly when `sale_eligibility` is Ok
/// and pays the quote; otherwise it answers that block and changes nothing.
/// The batch quote and `SellUnprotected` realise exactly the Ok set.
#[test]
fn every_sell_path_agrees_with_sale_eligibility() {
    let mut blocks = BTreeSet::new();
    for seed in 0..120u64 {
        let mut g = random_game(seed);
        if seed % 4 == 3 {
            let _ = g.deploy(0, Mode::Personal);
        }
        let mut ids: Vec<u64> = g.campaign().items.iter().map(|i| i.id).collect();
        ids.push(999_999);
        for id in ids {
            let eligibility = g.sale_eligibility(id);
            let mut sold = g.clone();
            let before = serde_json::to_value(&sold).unwrap();
            let result = sold.apply(&Command::SellItem(id));
            match &eligibility {
                Ok(quote) => {
                    assert_eq!(
                        result,
                        Ok(CommandOutcome::Amount(quote.coins)),
                        "seed {seed}"
                    );
                    assert!(sold.campaign().item(id).is_none());
                    assert_eq!(sold.campaign().coins, g.campaign().coins + quote.coins);
                    assert_eq!(sold.campaign().player.loadout, g.campaign().player.loadout);
                    assert_eq!(sold.campaign().npc.loadout, g.campaign().npc.loadout);
                    assert!(sold.campaign().validate().is_ok(), "seed {seed}");
                }
                Err(block) => {
                    blocks.insert(block_name(block));
                    assert_eq!(
                        result,
                        Err(CommandError::SaleBlocked(block.clone())),
                        "seed {seed}"
                    );
                    assert_eq!(serde_json::to_value(&sold).unwrap(), before, "seed {seed}");
                }
            }
            assert!(
                sold.strategy().validate(sold.campaign()).is_ok(),
                "seed {seed}"
            );
        }
        let (ids, coins) = ok_set(&g);
        assert_eq!(g.sell_extras_quote(), coins, "seed {seed}");
        let mut batch = g.clone();
        assert_eq!(
            batch.apply(&Command::SellUnprotected),
            Ok(CommandOutcome::Amount(coins)),
            "seed {seed}"
        );
        assert_eq!(owned(&batch), &owned(&g) - &ids, "seed {seed}");
    }
    // The generator reaches every category a stash can hold.
    for block in [
        "Unknown",
        "InBattle",
        "Reserved",
        "Protected",
        "Locked",
        "Favorite",
        "NpcOnly",
        "EquippedPersonal",
        "EquippedMara",
        "Preset",
        "RecruitEquipped",
    ] {
        assert!(
            blocks.contains(block),
            "no seed produced {block}: {blocks:?}"
        );
    }
}
