//! Trainable NPC rows (W2-23; design progression-tree section 9, acceptance
//! G5.4): only ST, DX, HT, Guns and Melee are trainable. IQ, HP and FP keep
//! their variants and serde names (old saves name them), but
//! `Game::apply(TrainNpc | TrainRecruit)` rejects them with the typed
//! `CommandError::TrainingUnsupported` before the campaign is reached.
use horde_core::{
    campaign::{Campaign, NpcTraining},
    command::{Command, CommandError, CommandOutcome},
    sim::Game,
    world::world,
};

const UNSUPPORTED: [NpcTraining; 3] = [
    NpcTraining::Intelligence,
    NpcTraining::HitPoints,
    NpcTraining::Fatigue,
];

/// A legacy camp with two liberated countries (recruitment open), plenty of
/// coins and no live attempt.
fn camp() -> Game {
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
    c.coins = 100_000;
    let mut game = Game::default();
    *game.staging().campaign() = c;
    game
}

#[test]
fn npc_trainable_rows_exclude_iq_hp_fp() {
    assert_eq!(
        NpcTraining::TRAINABLE,
        [
            NpcTraining::Strength,
            NpcTraining::Dexterity,
            NpcTraining::Health,
            NpcTraining::Guns,
            NpcTraining::Melee,
        ]
    );
    for training in NpcTraining::ALL {
        assert_eq!(
            training.is_trainable(),
            NpcTraining::TRAINABLE.contains(&training),
            "{training:?}"
        );
        assert_eq!(
            training.is_trainable(),
            !UNSUPPORTED.contains(&training),
            "{training:?}"
        );
    }
    // The variants stay: old saves name them.
    assert_eq!(NpcTraining::ALL.len(), 8);
}

#[test]
fn apply_train_npc_intelligence_returns_training_unsupported() {
    let mut game = camp();
    for _ in 0..30 {
        game.apply(&Command::BuyNpcPoint).unwrap();
    }
    let country = game.campaign().world.cleared[0].clone();
    let CommandOutcome::Id(id) = game.apply(&Command::Recruit(country)).unwrap() else {
        panic!("Recruit returned no id");
    };
    for _ in 0..30 {
        game.apply(&Command::BuyRecruitPoint(id)).unwrap();
    }
    let before = game.clone();
    for training in UNSUPPORTED {
        assert_eq!(
            game.apply(&Command::TrainNpc(training)),
            Err(CommandError::TrainingUnsupported(training))
        );
        assert_eq!(
            game.apply(&Command::TrainRecruit { id, training }),
            Err(CommandError::TrainingUnsupported(training))
        );
        let text = CommandError::TrainingUnsupported(training).to_string();
        let camel = text
            .as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
        assert!(
            text.starts_with(char::is_uppercase)
                && !camel
                && !text.contains("::")
                && !text.contains('{'),
            "{text}"
        );
    }
    assert_eq!(game.campaign(), before.campaign(), "nothing changed");
    assert_eq!(game.strategy(), before.strategy(), "nothing changed");
    // The trainable rows still reach the campaign: each trains once, while
    // 30 points cover it, and is otherwise refused by the campaign's
    // point check, never by the typed verdict.
    for training in NpcTraining::TRAINABLE {
        let unspent = game.campaign().npc.unspent;
        let result = game.apply(&Command::TrainNpc(training));
        if unspent >= training.cost() {
            assert_eq!(result, Ok(CommandOutcome::Applied), "{training:?}");
            assert_eq!(game.campaign().npc.investments[training], 1);
        } else {
            assert!(
                matches!(result, Err(CommandError::Rejected(_))),
                "{training:?}"
            );
        }
        let recruit = |game: &Game| {
            let r = game.strategy().roster.iter().find(|r| r.id == id).unwrap();
            (r.npc.unspent, r.npc.investments[training])
        };
        let (unspent, invested) = recruit(&game);
        let result = game.apply(&Command::TrainRecruit { id, training });
        if unspent >= training.cost() {
            assert_eq!(result, Ok(CommandOutcome::Applied), "{training:?}");
            assert_eq!(recruit(&game).1, invested + 1);
        } else {
            assert!(
                matches!(result, Err(CommandError::Rejected(_))),
                "{training:?}"
            );
        }
    }
}
