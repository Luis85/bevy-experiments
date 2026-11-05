//! One upgrade metric (loot-economy §6, W1-22): the recommender's per-slot
//! score and its 1.0 margin, extracted so auto-sell's `keep_upgrades` and the
//! Results upgrade offers judge "better than equipped" exactly as the
//! recommender does.
use horde_core::{
    campaign::{Campaign, Recommendation, RecommendationAction, UPGRADE_MARGIN, UpgradeJudge},
    command::{Command, CommandOutcome},
    content::{Mode, Slot, content},
    defenders::DefenderId,
    sim::Game,
    world::world,
};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

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

/// Fixture 1: a fresh campaign in its first country holding unequipped
/// tier-1 loot (a fire axe, a padded vest, a first-aid rig and a marksman
/// rifle the player is not trained for).
fn fresh_with_loot() -> Game {
    let mut game = Game::default();
    let start = world().countries[0].id.clone();
    let c = game.staging().campaign();
    c.choose_start_country(&start).unwrap();
    for definition in [5, 6, 7, 4] {
        c.add_item(definition).unwrap();
    }
    game
}

/// Fixture 2: a legacy campaign whose start country is still being defended,
/// with three liberated neighbours, Mara holding a fire axe, two recruits
/// (one holding a marksman rifle) and tier-2 loot for both modes.
fn liberated_with_recruits() -> Game {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    let ids: Vec<_> = world()
        .countries
        .iter()
        .take(4)
        .map(|c| c.id.clone())
        .collect();
    c.choose_start_country(&ids[0]).unwrap();
    for id in &ids[1..] {
        let levels = world().country(id).unwrap().level_count();
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id.clone());
    }
    c.coins = 10_000;
    let mut g = Game::default();
    *g.staging().campaign() = c;
    let buy = |g: &mut Game, definition: usize| g.campaign_mut().add_item(definition).unwrap();
    let mara_axe = buy(&mut g, 5);
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Npc,
            id: mara_axe,
        },
    );
    for definition in [9, 10, 8, 11] {
        buy(&mut g, definition);
    }
    let recruit = id(apply(&mut g, Command::Recruit(ids[1].clone())));
    let rifle = buy(&mut g, 4);
    apply(
        &mut g,
        Command::EquipRecruit {
            id: recruit,
            item: rifle,
        },
    );
    id(apply(&mut g, Command::Recruit(ids[2].clone())));
    g
}

/// Fixture 3: the checked-in v6 save (a live attempt holding the player's
/// loadout, a settled Mara report and a stash).
fn v6_save() -> Game {
    Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture loads")
}

fn recommendations(game: &Game) -> [Recommendation; 2] {
    [Mode::Personal, Mode::Npc].map(|mode| game.campaign().recommendation(mode))
}

/// FNV-1a over the debug form of every `compare_item` answer (owned items in
/// id order, Personal then Npc), so the extracted `loadout_metrics` is pinned
/// field by field.
fn comparisons_digest(campaign: &Campaign) -> u64 {
    let mut text = String::new();
    for mode in [Mode::Personal, Mode::Npc] {
        for item in &campaign.items {
            text.push_str(&format!(
                "{:?}
",
                campaign.compare_item(mode, item.id)
            ));
        }
    }
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn equip(mode: &str, item: &str, id: u64) -> Recommendation {
    Recommendation {
        title: "Equip an improvement already in your armory".into(),
        detail: format!(
            "{item} improves your selected {mode} loadout. Compare the complete profile before equipping."
        ),
        action: RecommendationAction::Equip(id),
        price: 0,
        affordable: true,
    }
}

/// Refactor pin (W1-22), written and run green against the recommender
/// before `slot_score`, `loadout_metrics` and `UPGRADE_MARGIN` were
/// extracted from it: the extraction must not change one recommendation or
/// one comparison in any of the three fixture campaigns.
#[test]
fn recommendation_unchanged_after_refactor() {
    let fresh = fresh_with_loot();
    assert_eq!(
        recommendations(&fresh),
        [
            equip("Personal", "Padded vest", 7),
            equip("Npc", "Marksman rifle", 9),
        ]
    );
    let liberated = liberated_with_recruits();
    assert_eq!(
        recommendations(&liberated),
        [
            equip("Personal", "Scavenged ballistic vest", 8),
            equip("Npc", "Patrol rifle", 9),
        ]
    );
    let v6 = v6_save();
    assert_eq!(
        recommendations(&v6),
        [
            Recommendation {
                title: "Prepare the next defense".into(),
                detail: "Belarus: defense 1 of 3. Earlier cleared defenses remain replayable; rewards are banked immediately.".into(),
                action: RecommendationAction::Deploy,
                price: 0,
                affordable: true,
            },
            Recommendation {
                title: "Save for Checkpoint rifle".into(),
                detail: "Guaranteed quartermaster gear costs 459 coins; 0 more needed. No rare drop is required.".into(),
                action: RecommendationAction::BuyEquipment(16),
                price: 459,
                affordable: true,
            },
        ]
    );
    assert_eq!(
        [&fresh, &liberated, &v6].map(|g| comparisons_digest(g.campaign())),
        [
            14_584_140_189_704_006_710,
            6_625_371_839_841_207_539,
            17_223_248_231_927_613_725
        ]
    );
}

/// The owned item of `definition` that no one has equipped.
fn spare(campaign: &Campaign, definition: usize) -> u64 {
    campaign
        .items
        .iter()
        .find(|item| {
            item.definition_index() == definition
                && !campaign.player.loadout.contains(&item.id)
                && !campaign.npc.loadout.contains(&item.id)
        })
        .expect("a spare item of the definition")
        .id
}

#[test]
fn upgrade_gain_true_for_fire_axe_over_crowbar() {
    let game = fresh_with_loot();
    let c = game.campaign();
    let axe = spare(c, 5);
    let crowbar = c.player.loadout[Slot::Melee.index()];
    assert_eq!(c.item(crowbar).unwrap().definition().id.as_str(), "crowbar");
    let comparison = c.compare_item(Mode::Personal, axe).unwrap();
    let melee_gain = (comparison.after.melee - comparison.before.melee) as f32;
    assert!(melee_gain > UPGRADE_MARGIN);
    assert_eq!(
        c.upgrade_gain(Mode::Personal, c.player.loadout, &c.npc, axe),
        Some(melee_gain)
    );
    assert!(
        c.upgrade_gain(Mode::Npc, c.npc.loadout, &c.npc, axe)
            .is_some_and(|gain| gain > UPGRADE_MARGIN)
    );
    let strategy = game.strategy();
    for defender in [DefenderId::Player, DefenderId::Mara] {
        assert!(c.is_upgrade_for(axe, &UpgradeJudge::Defender(defender), strategy));
    }
    // The equipped crowbar is never an upgrade over itself.
    assert_eq!(
        c.upgrade_gain(Mode::Personal, c.player.loadout, &c.npc, crowbar),
        None
    );
}

#[test]
fn upgrade_gain_false_for_untrained_class_in_personal() {
    let game = fresh_with_loot();
    let c = game.campaign();
    let rifle = spare(c, 4);
    assert!(!c.can_use_weapon(Mode::Personal, 4));
    // The rifle out-scores the starter pistol, so only the class rule rejects it.
    let comparison = c.compare_item(Mode::Personal, rifle).unwrap();
    assert!(comparison.after.damage > comparison.before.damage);
    assert_eq!(
        c.upgrade_gain(Mode::Personal, c.player.loadout, &c.npc, rifle),
        None
    );
    assert!(!c.is_upgrade_for(
        rifle,
        &UpgradeJudge::Defender(DefenderId::Player),
        game.strategy()
    ));
    // Mara keeps her independent Guns procedure and may use it.
    assert!(
        c.upgrade_gain(Mode::Npc, c.npc.loadout, &c.npc, rifle)
            .is_some()
    );
}

#[test]
fn upgrade_gain_uses_given_recruit_loadout_and_npc() {
    let mut g = liberated_with_recruits();
    let recruit = g.strategy().roster[0].id;
    let held = spare(g.campaign(), 9);
    apply(
        &mut g,
        Command::EquipRecruit {
            id: recruit,
            item: held,
        },
    );
    let machete = g.campaign_mut().add_item(9).unwrap();
    let c = g.campaign();
    let strategy = g.strategy();
    let r = strategy.roster.iter().find(|r| r.id == recruit).unwrap();
    assert_eq!(r.npc.loadout[Slot::Melee.index()], held);

    // Mara holds a fire axe, so a machete is her upgrade; the recruit already
    // holds an equal machete, so it is not theirs.
    let mara_gain = c.upgrade_gain(Mode::Npc, c.npc.loadout, &c.npc, machete);
    assert!(mara_gain.is_some_and(|gain| gain > UPGRADE_MARGIN));
    assert_eq!(
        c.upgrade_gain(Mode::Npc, r.npc.loadout, &r.npc, machete),
        None
    );
    assert!(c.is_upgrade_for(machete, &UpgradeJudge::Defender(DefenderId::Mara), strategy));
    assert!(!c.is_upgrade_for(
        machete,
        &UpgradeJudge::Defender(DefenderId::Recruit(recruit)),
        strategy
    ));
    // The explicit loadout judge answers with the loadout and NPC it is given.
    let judge = |loadout, npc: &horde_core::campaign::Npc| UpgradeJudge::Loadout {
        mode: Mode::Npc,
        loadout,
        npc: Box::new(npc.clone()),
    };
    assert!(c.is_upgrade_for(machete, &judge(c.npc.loadout, &c.npc), strategy));
    assert!(!c.is_upgrade_for(machete, &judge(r.npc.loadout, &r.npc), strategy));

    // A recruit seat resolves to that recruit's own loadout and NPC: the
    // patrol rifle is judged against the marksman rifle the recruit holds,
    // exactly as the explicit (recruit loadout, recruit NPC) call judges it.
    let marksman = r.npc.loadout[Slot::Ranged.index()];
    assert_eq!(c.item(marksman).unwrap().definition_index(), 4);
    let patrol = spare(c, 8);
    let direct = c
        .upgrade_gain(Mode::Npc, r.npc.loadout, &r.npc, patrol)
        .is_some();
    assert!(direct, "a patrol rifle out-scores a marksman rifle");
    assert_eq!(
        c.is_upgrade_for(
            patrol,
            &UpgradeJudge::Defender(DefenderId::Recruit(recruit)),
            strategy
        ),
        direct
    );
    assert_eq!(
        c.is_upgrade_for(patrol, &judge(r.npc.loadout, &r.npc), strategy),
        direct
    );
}

#[test]
fn unresolvable_recruit_judge_keeps_everything() {
    let game = liberated_with_recruits();
    let c = game.campaign();
    let strategy = game.strategy();
    let unknown = UpgradeJudge::Defender(DefenderId::Recruit(999));
    assert!(strategy.roster.iter().all(|r| r.id != 999));
    assert!(!c.items.is_empty());
    for item in &c.items {
        assert!(
            c.is_upgrade_for(item.id, &unknown, strategy),
            "item {}",
            item.id
        );
    }
    // The player's starter outfit is nobody's upgrade, so a resolvable judge
    // answers differently.
    let starter = c.player.loadout[Slot::Outfit.index()];
    assert_eq!(
        c.item(starter).unwrap().definition().id,
        content().items[2].id
    );
    assert!(!c.is_upgrade_for(starter, &UpgradeJudge::AnyDefender, strategy));
}

#[test]
fn any_defender_judge_counts_every_roster_recruit() {
    let mut g = liberated_with_recruits();
    // Mara and the player both take a machete, so a third one is an upgrade
    // only for the recruits, who still swing their starter crowbars.
    let mara_machete = spare(g.campaign(), 9);
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Npc,
            id: mara_machete,
        },
    );
    let player_machete = g.campaign_mut().add_item(9).unwrap();
    apply(
        &mut g,
        Command::Equip {
            mode: Mode::Personal,
            id: player_machete,
        },
    );
    let machete = g.campaign_mut().add_item(9).unwrap();
    let c = g.campaign();
    let strategy = g.strategy();
    for defender in [DefenderId::Player, DefenderId::Mara] {
        assert!(!c.is_upgrade_for(machete, &UpgradeJudge::Defender(defender), strategy));
    }
    for recruit in &strategy.roster {
        assert!(c.is_upgrade_for(
            machete,
            &UpgradeJudge::Defender(DefenderId::Recruit(recruit.id)),
            strategy
        ));
    }
    assert!(c.is_upgrade_for(machete, &UpgradeJudge::AnyDefender, strategy));
}
