//! Recruitment choice (W2-20; decisions 5 and 15, design-campaign-defenders
//! 1.6, schema-v7 section 6 row 8, ruling E7): `Game::recruit_offer` names
//! two distinct archetypes for the next recruit of a country (the first is
//! today's rotation, the second is keyed on the game seed, the next recruit
//! id and the country, with no RNG draw), the recruit's name, appearance,
//! origin trait and the core price, or the typed reason recruiting is
//! blocked. `Command::RecruitAs` applies one offered archetype;
//! `Command::Recruit` keeps taking the first offer.
mod support;
use horde_core::{
    command::{Command, CommandError, CommandOutcome},
    content::Mode,
    defenders::{Archetype, DefenderId, OriginTrait, RecruitBlocker, recruit_identity},
    sim::Game,
    skill_catalog::{CampaignFeature, SkillEffect, skills},
    world::world,
};
use std::collections::BTreeSet;
use support::legacy_game;

/// A camp with `count` liberated countries, 10k coins, no live attempt and
/// `victories = rank` (so the progression rank is `rank`). `legacy` opens
/// every campaign feature, recruitment included.
fn camp(count: usize, rank: u64, legacy: bool) -> Game {
    let mut game = if legacy {
        legacy_game()
    } else {
        Game::default()
    };
    let ids: Vec<_> = world()
        .countries
        .iter()
        .take(count)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    let c = game.campaign_mut();
    c.choose_start_country(&ids[0].0).unwrap();
    for (id, levels) in ids {
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id);
    }
    c.coins = 10_000;
    c.victories = rank;
    game
}

fn snapshot(game: &Game) -> serde_json::Value {
    serde_json::to_value(game).unwrap()
}

fn id(outcome: Result<CommandOutcome, CommandError>) -> u64 {
    match outcome {
        Ok(CommandOutcome::Id(id)) => id,
        other => panic!("recruiting failed: {other:?}"),
    }
}

/// The node whose effect unlocks recruitment.
fn recruitment_node() -> &'static horde_core::skill_catalog::SkillNode {
    skills()
        .nodes
        .iter()
        .find(|n| {
            n.effects.iter().any(|e| {
                matches!(e, SkillEffect::FeatureUnlock(f) if *f == CampaignFeature::Recruitment)
            })
        })
        .expect("the catalogue unlocks recruitment")
}

#[test]
fn recruit_offer_has_two_distinct_archetypes_and_recruit_as_applies_the_choice() {
    // Over every country the offer is two distinct archetypes, and the keyed
    // second choice reaches all three archetypes the first leaves open.
    let game = camp(4, 0, true);
    let mut seconds = BTreeSet::new();
    for country in &world().countries {
        let mut probe = game.clone();
        if !probe.campaign().world.cleared.contains(&country.id) {
            let c = probe.campaign_mut();
            c.world
                .country_levels
                .insert(country.id.clone(), country.level_count());
            c.world.cleared.push(country.id.clone());
        }
        let offer = probe.recruit_offer(&country.id).unwrap();
        let [first, second] = offer.archetypes.map(|choice| choice.archetype);
        assert_ne!(first, second, "{}", country.id);
        assert_eq!(first, Archetype::Marksman, "recruit 1 rotates to Marksman");
        seconds.insert(second);
    }
    assert_eq!(
        seconds,
        BTreeSet::from([Archetype::Guardian, Archetype::Scout, Archetype::Steadfast]),
        "the second archetype is keyed on the country"
    );

    // RecruitAs applies the second choice: creation, stats and policy, and
    // the offer's name, appearance and origin are the recruit's.
    for next in 1..=8u64 {
        let mut game = camp(4, 0, true);
        let country = game.campaign().world.cleared[1].clone();
        for _ in 1..next {
            id(game.apply(&Command::Recruit(country.clone())));
        }
        let offer = game.recruit_offer(&country).unwrap();
        let choice = offer.archetypes[1];
        assert_ne!(offer.archetypes[0].archetype, choice.archetype);
        let creation = choice.archetype.creation();
        assert_eq!(
            (choice.st, choice.dx, choice.ht, choice.guns, choice.melee),
            (
                creation.st,
                creation.dx,
                creation.ht,
                creation.guns,
                creation.melee
            )
        );
        assert_eq!(choice.policy, choice.archetype.default_policy());
        let recruit = id(game.apply(&Command::RecruitAs {
            country: country.clone(),
            archetype: choice.archetype.id().into(),
        }));
        assert_eq!(recruit, next);
        let r = game
            .strategy()
            .roster
            .iter()
            .find(|r| r.id == recruit)
            .unwrap();
        assert_eq!(r.npc.creation, creation, "recruit {next}");
        assert_eq!(
            (
                r.npc.st,
                r.npc.dx,
                r.npc.iq,
                r.npc.ht,
                r.npc.guns,
                r.npc.melee
            ),
            (
                creation.st,
                creation.dx,
                creation.iq,
                creation.ht,
                creation.guns,
                creation.melee
            )
        );
        assert_eq!(r.npc.policy, choice.archetype.default_policy());
        assert_eq!(r.name, offer.name);
        assert_eq!(r.origin, offer.country_id);
        let card = game.defender_card(DefenderId::Recruit(recruit)).unwrap();
        assert_eq!(card.archetype, Some(choice.archetype));
        assert_eq!(card.appearance, offer.appearance);
        assert_eq!(card.appearance, recruit_identity(recruit).1);
        let origin = card.origin.unwrap();
        assert_eq!(origin.origin_trait, offer.origin_trait);
        assert_eq!(origin.country_name, offer.country_name);
        let biome = &world().country(&country).unwrap().biome;
        assert_eq!(offer.origin_trait, OriginTrait::for_biome(biome));
    }
    // Mara's card names her creation archetype; the player has none.
    let game = camp(1, 0, true);
    assert_eq!(
        game.defender_card(DefenderId::Mara).unwrap().archetype,
        Some(Archetype::Marksman)
    );
    assert_eq!(
        game.defender_card(DefenderId::Player).unwrap().archetype,
        None
    );
}

#[test]
fn legacy_recruit_equals_first_offer() {
    let mut legacy = camp(4, 0, true);
    let mut chosen = legacy.clone();
    let country = legacy.campaign().world.cleared[2].clone();
    let rotation = [
        Archetype::Marksman,
        Archetype::Guardian,
        Archetype::Scout,
        Archetype::Steadfast,
    ];
    for n in 0..9usize {
        let offer = chosen.recruit_offer(&country).unwrap();
        // Today's rotation: recruit `id` is archetype `(id - 1) % 4`.
        assert_eq!(
            offer.archetypes[0].archetype,
            rotation[n % 4],
            "recruit {n}"
        );
        id(legacy.apply(&Command::Recruit(country.clone())));
        id(chosen.apply(&Command::RecruitAs {
            country: country.clone(),
            archetype: offer.archetypes[0].archetype.id().into(),
        }));
        assert_eq!(snapshot(&legacy), snapshot(&chosen), "recruit {n}");
    }
    // The legacy roster keeps today's creation names and policies.
    let names: Vec<_> = legacy
        .strategy()
        .roster
        .iter()
        .map(|r| (r.npc.creation.archetype.clone(), r.npc.policy.label()))
        .collect();
    assert_eq!(
        names[..4],
        [
            ("Marksman".to_string(), "Marksman"),
            ("Guardian".to_string(), "Guardian"),
            ("Scout".to_string(), "Balanced"),
            ("Steadfast".to_string(), "Boss focus"),
        ]
    );
}

#[test]
fn recruit_as_rejects_unoffered_archetype() {
    let mut game = camp(4, 0, true);
    let country = game.campaign().world.cleared[0].clone();
    let offer = game.recruit_offer(&country).unwrap();
    let offered: Vec<_> = offer.archetypes.iter().map(|c| c.archetype).collect();
    let before = snapshot(&game);
    let unoffered: Vec<String> = Archetype::ALL
        .iter()
        .filter(|a| !offered.contains(a))
        .map(|a| a.id().to_string())
        .chain(["Wizard".to_string(), String::new(), "marksman".into()])
        .collect();
    assert_eq!(unoffered.len(), 5, "two of four archetypes are not offered");
    for archetype in unoffered {
        let error = game
            .apply(&Command::RecruitAs {
                country: country.clone(),
                archetype: archetype.clone(),
            })
            .unwrap_err();
        assert_eq!(error, CommandError::ArchetypeNotOffered(archetype.clone()));
        let text = error.to_string();
        assert!(!text.is_empty() && !text.contains("::"), "{text}");
        assert_eq!(snapshot(&game), before, "{archetype:?} changed the game");
    }
    // A blocked offer answers before the archetype is judged.
    let unknown = "Wizard".to_string();
    let error = game
        .apply(&Command::RecruitAs {
            country: "NOWHERE".into(),
            archetype: unknown,
        })
        .unwrap_err();
    assert_eq!(
        error,
        CommandError::RecruitUnavailable(RecruitBlocker::NotLiberated)
    );
    assert_eq!(snapshot(&game), before);
}

#[test]
fn recruit_offer_names_the_missing_recruitment_node() {
    // Without the node (no legacy features) a liberated country names it.
    let mut game = camp(2, 0, false);
    let country = game.campaign().world.cleared[0].clone();
    let node = recruitment_node();
    let needs = RecruitBlocker::NeedsNode {
        id: node.id.clone(),
        name: node.name.clone(),
        price: node.price,
    };
    assert_eq!(game.recruit_offer(&country), Err(needs.clone()));
    let label = needs.label();
    assert!(label.contains(&node.name), "{label}");
    assert!(label.contains(&node.price.to_string()), "{label}");
    let before = snapshot(&game);
    for command in [
        Command::Recruit(country.clone()),
        Command::RecruitAs {
            country: country.clone(),
            archetype: Archetype::Marksman.id().into(),
        },
    ] {
        let error = game.apply(&command).unwrap_err();
        assert_eq!(error, CommandError::RecruitUnavailable(needs.clone()));
        assert_eq!(error.to_string(), label);
        assert_eq!(snapshot(&game), before);
    }

    // The other blockers, each with plain text.
    let mut game = camp(2, 0, true);
    let liberated = game.campaign().world.cleared[0].clone();
    let elsewhere = world()
        .countries
        .iter()
        .find(|c| !game.campaign().world.cleared.contains(&c.id))
        .unwrap()
        .id
        .clone();
    assert_eq!(
        game.recruit_offer(&elsewhere),
        Err(RecruitBlocker::NotLiberated)
    );
    assert_eq!(
        game.recruit_offer("NOWHERE"),
        Err(RecruitBlocker::NotLiberated)
    );
    let price = game.campaign().recruit_price();
    game.campaign_mut().coins = price - 1;
    assert_eq!(
        game.recruit_offer(&liberated),
        Err(RecruitBlocker::NeedCoins(price))
    );
    game.campaign_mut().coins = 10_000;
    let mut defending = game.clone();
    defending.campaign_mut().attempts = 1;
    defending.deploy(0, Mode::Personal).unwrap();
    assert_eq!(
        defending.recruit_offer(&liberated),
        Err(RecruitBlocker::DefenseInProgress)
    );
    let max = world().strategy_config.max_roster;
    for _ in 0..max {
        id(game.apply(&Command::Recruit(liberated.clone())));
    }
    assert_eq!(
        game.recruit_offer(&liberated),
        Err(RecruitBlocker::RosterFull)
    );
    for blocker in [
        needs,
        RecruitBlocker::NotLiberated,
        RecruitBlocker::RosterFull,
        RecruitBlocker::NeedCoins(price),
        RecruitBlocker::DefenseInProgress,
    ] {
        let text = blocker.label();
        let camel = text
            .as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
        assert!(
            text.starts_with(char::is_uppercase) && !camel && !text.contains("::"),
            "{text}"
        );
        assert_eq!(CommandError::RecruitUnavailable(blocker).to_string(), text);
    }
}

#[test]
fn recruiting_draws_no_strategy_rng() {
    let mut game = camp(4, 0, true);
    let country = game.campaign().world.cleared[3].clone();
    for _ in 0..6 {
        let rng = serde_json::to_value(game.strategy().rng).unwrap();
        let before = snapshot(&game);
        let offer = game.recruit_offer(&country).unwrap();
        assert_eq!(snapshot(&game), before, "recruit_offer is pure");
        assert_eq!(game.recruit_offer(&country).unwrap(), offer, "stable");
        id(game.apply(&Command::RecruitAs {
            country: country.clone(),
            archetype: offer.archetypes[1].archetype.id().into(),
        }));
        assert_eq!(
            serde_json::to_value(game.strategy().rng).unwrap(),
            rng,
            "recruiting drew the strategy RNG"
        );
    }
    // The keyed choice never follows the strategy RNG.
    let base = camp(4, 0, true);
    let mut reseeded = base.clone();
    reseeded.strategy_mut().rng = horde_core::rules::Rng::new(99);
    for country in base.campaign().world.cleared.clone() {
        assert_eq!(
            base.recruit_offer(&country),
            reseeded.recruit_offer(&country)
        );
    }
}

/// W2 review (ruling E7): the game's offer is keyed on the game seed and
/// the next recruit id as well as the country. Each changes some second
/// archetype with the others held, the first staying the rotation's.
#[test]
fn recruit_offer_follows_the_game_seed_and_next_recruit_id() {
    let base = camp(4, 0, true);
    let countries = base.campaign().world.cleared.clone();
    let seconds = |game: &Game| -> Vec<Archetype> {
        countries
            .iter()
            .map(|country| {
                let [first, second] = game
                    .recruit_offer(country)
                    .unwrap()
                    .archetypes
                    .map(|choice| choice.archetype);
                assert_eq!(first, Archetype::rotation(game.strategy().next_recruit_id));
                assert_eq!(
                    [first, second],
                    game.strategy().recruit_offer_for(game.seed(), country),
                    "the offer is keyed on the game seed"
                );
                second
            })
            .collect()
    };
    let at_base = seconds(&base);
    let reseeded = [1, 7, 0xDEAD_BEEF_2026, u64::MAX].map(|seed| {
        let mut game = base.clone();
        game.set_seed(seed);
        seconds(&game)
    });
    assert!(
        reseeded.iter().any(|s| *s != at_base),
        "the game seed keys the offer: {at_base:?} {reseeded:?}"
    );
    // Recruit ids 1, 5, 9, ... share the rotation's first archetype.
    let later = (1..=4u64).map(|n| {
        let mut game = base.clone();
        game.strategy_mut().next_recruit_id = 1 + 4 * n;
        seconds(&game)
    });
    assert!(
        later.clone().any(|s| s != at_base),
        "the next recruit id keys the offer: {at_base:?} {:?}",
        later.collect::<Vec<_>>()
    );
}

#[test]
fn recruit_offer_price_reads_core_price() {
    for rank in [0, 10, 20] {
        let mut game = camp(2, rank, true);
        assert_eq!(u64::from(game.campaign().progression_rank()), rank);
        let country = game.campaign().world.cleared[0].clone();
        let offer = game.recruit_offer(&country).unwrap();
        assert_eq!(offer.price, game.campaign().recruit_price(), "rank {rank}");
        let coins = game.campaign().coins;
        id(game.apply(&Command::RecruitAs {
            country: country.clone(),
            archetype: offer.archetypes[1].archetype.id().into(),
        }));
        assert_eq!(coins - game.campaign().coins, offer.price, "rank {rank}");
    }
    let game = camp(2, 10, true);
    let country = game.campaign().world.cleared[0].clone();
    assert_eq!(game.recruit_offer(&country).unwrap().price, 200);
    // The offer is read live: a rank change reprices it at once.
    let mut later = game.clone();
    later.campaign_mut().victories = 20;
    assert_eq!(later.recruit_offer(&country).unwrap().price, 300);
}

#[test]
fn recruit_as_keeps_the_schema_v7_shape() {
    let command = Command::RecruitAs {
        country: "ALB".into(),
        archetype: Archetype::Scout.id().into(),
    };
    let json = serde_json::to_value(&command).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"RecruitAs": {"country": "ALB", "archetype": "Scout"}})
    );
    assert_eq!(serde_json::from_value::<Command>(json).unwrap(), command);
    for archetype in Archetype::ALL {
        assert_eq!(Archetype::from_id(archetype.id()), Some(archetype));
        assert_eq!(
            Archetype::from_creation(&archetype.creation()),
            Some(archetype)
        );
        assert_eq!(archetype.label(), archetype.id());
    }
}
