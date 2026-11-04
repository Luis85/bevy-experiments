mod support;
use horde_core::content::Mode;
use horde_core::sim::{BattleId, Game, Input, Outcome, SimEvent, SimEventRecord};
use support::{decode_value, enemy, game, strategic_game};

/// An unconscious boss is reaped on the next tick, finishing the attempt.
fn doom_boss(g: &mut Game) {
    enemy(g, 5, 20.0, true);
    let a = g.attempt.as_mut().unwrap();
    a.boss_spawned = true;
    a.enemies
        .iter_mut()
        .find(|e| e.boss)
        .unwrap()
        .injury
        .unconscious = true;
}
fn ticks(g: &mut Game, n: u32) -> Vec<SimEventRecord> {
    let mut records = Vec::new();
    for _ in 0..n {
        g.tick(Input::default());
        records.extend(g.drain_events());
    }
    records
}
fn assert_monotonic(records: &[SimEventRecord]) {
    assert!(records.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[test]
fn events_on_the_finishing_tick_are_delivered_before_attempt_finished() {
    let mut g = game(Mode::Npc);
    doom_boss(&mut g);
    let records = ticks(&mut g, 1);
    assert!(g.attempt.is_none());
    let last = records.last().expect("finishing tick emits events");
    assert!(matches!(
        last.event,
        SimEvent::AttemptFinished {
            outcome: Outcome::Victory,
            mode: Mode::Npc,
            ..
        }
    ));
    assert!(
        records
            .iter()
            .any(|r| r.tick == last.tick && matches!(r.event, SimEvent::Kill { .. }))
    );
    assert!(records.iter().all(|r| r.battle == BattleId::Root));
    assert_monotonic(&records);
    assert!(g.drain_events().next().is_none(), "records drain once");
}

#[test]
fn withdrawal_outside_a_tick_is_still_delivered() {
    let mut g = game(Mode::Personal);
    ticks(&mut g, 5);
    g.withdraw();
    let records: Vec<_> = g.drain_events().collect();
    assert!(matches!(
        records.as_slice(),
        [SimEventRecord {
            event: SimEvent::AttemptFinished {
                outcome: Outcome::Withdrawal,
                ..
            },
            ..
        }]
    ));
}

#[test]
fn window_events_share_their_stream_sequence_number() {
    let mut g = game(Mode::Npc);
    enemy(&mut g, 5, 8.0, false);
    let records = ticks(&mut g, 240);
    let a = g.attempt.as_ref().unwrap();
    assert!(!a.events.is_empty());
    for event in &a.events {
        let record = records
            .iter()
            .find(|r| r.seq == event.seq)
            .expect("every window event was streamed");
        assert_eq!(record.tick, event.tick);
    }
    assert_monotonic(&records);
}

#[test]
fn npc_resolutions_are_typed_and_inspectable_without_log_prose() {
    let mut g = game(Mode::Npc);
    enemy(&mut g, 5, 8.0, false);
    let records = ticks(&mut g, 240);
    let summary = records
        .iter()
        .rev()
        .find_map(|r| match &r.event {
            SimEvent::RuleResolved { summary, .. } => Some(summary.clone()),
            _ => None,
        })
        .expect("the survivor attacks within four turns");
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.latest_resolution.as_ref(), Some(&summary));
    assert_eq!(
        a.log.iter().rev().find(|l| l.contains("3d6")),
        Some(&summary)
    );
}

#[test]
fn melee_cooldown_readiness_is_an_event() {
    let mut g = game(Mode::Personal);
    g.attempt.as_mut().unwrap().melee_cooldown = 2;
    let first = ticks(&mut g, 1);
    assert!(!first.iter().any(|r| r.event == SimEvent::CooldownReady));
    let second = ticks(&mut g, 1);
    assert!(second.iter().any(|r| r.event == SimEvent::CooldownReady));
}

#[test]
fn expedition_events_carry_their_battle_id_through_the_final_tick() {
    let mut g = strategic_game();
    let country = g.campaign.world.cleared[0].clone();
    let recruit = g.strategy.recruit(&mut g.campaign, &country).unwrap();
    let battle = g.deploy_survivor(recruit, &country, 0).unwrap();
    ticks(&mut g, 3);
    doom_boss(&mut g.battles[0].game);
    let records = ticks(&mut g, 1);
    assert!(g.battles[0].settled);
    let expedition: Vec<_> = records
        .iter()
        .filter(|r| r.battle == BattleId::Expedition(battle))
        .collect();
    assert!(matches!(
        expedition.last().unwrap().event,
        SimEvent::AttemptFinished {
            outcome: Outcome::Victory,
            ..
        }
    ));
    assert!(
        expedition
            .iter()
            .any(|r| matches!(r.event, SimEvent::Kill { .. }))
    );
    assert!(expedition.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[test]
fn sequence_numbers_never_repeat_across_save_resume_and_attempts() {
    let mut original = game(Mode::Npc);
    enemy(&mut original, 5, 8.0, false);
    let before = ticks(&mut original, 300);
    let high = before.iter().map(|r| r.seq).max().unwrap();
    let json = serde_json::to_string(&original).unwrap();
    let mut resumed: Game = serde_json::from_str(&json).unwrap();
    let continued = ticks(&mut original, 300);
    assert_eq!(ticks(&mut resumed, 300), continued);
    assert!(continued.iter().all(|r| r.seq > high));
    original.withdraw();
    let finished = original.drain_events().next_back().unwrap().seq;
    original.campaign.attempts = 1;
    original.deploy(0, Mode::Npc).unwrap();
    original.set_paused(false);
    enemy(&mut original, 5, 8.0, false);
    assert!(ticks(&mut original, 120).iter().all(|r| r.seq > finished));
}

#[test]
fn legacy_saves_without_event_sequences_load_and_renumber_their_window() {
    let mut g = game(Mode::Npc);
    enemy(&mut g, 5, 8.0, false);
    for _ in 0..600 {
        ticks(&mut g, 1);
        if g.attempt.as_ref().unwrap().events.len() > 1 {
            break;
        }
    }
    assert!(g.attempt.as_ref().unwrap().events.len() > 1);
    let mut legacy = serde_json::to_value(&g).unwrap();
    legacy.as_object_mut().unwrap().remove("next_event_seq");
    let attempt = legacy["attempt"].as_object_mut().unwrap();
    attempt.remove("event_seq");
    attempt.remove("latest_resolution");
    for event in attempt["events"].as_array_mut().unwrap() {
        event.as_object_mut().unwrap().remove("seq");
    }
    let mut loaded = decode_value(legacy).unwrap();
    let a = loaded.attempt.as_ref().unwrap();
    assert!(a.latest_resolution.is_none());
    let seqs: std::collections::BTreeSet<_> = a.events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs.len(), a.events.len());
    let high = *seqs.last().unwrap();
    assert!(ticks(&mut loaded, 120).iter().all(|r| r.seq > high));
}
