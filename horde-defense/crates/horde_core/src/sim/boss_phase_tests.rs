//! W2-36 (design enemy-balance §6 "Bosses: phases only", ruling W-9,
//! acceptance G6.4 and G6.5): the transition count a deploy freezes, one
//! phase profile per boss kind, the 50% and 25% thresholds with their
//! 90-tick hold and `BossPhase` event, and the phase multipliers on speed,
//! armor, the Caller's call interval and the Siege Brute's recovery.

use super::*;
use crate::balance::{BalancePreset, BalanceSettings};
use crate::campaign::Campaign;
use crate::progression::{BossRank, FrontPosition};

/// A campaign on `preset`.
fn on(preset: BalancePreset) -> Campaign {
    let mut c = Campaign::default();
    c.set_balance_settings(BalanceSettings::for_preset(preset))
        .unwrap();
    c
}

/// A stage of an 8-stage front whose boss is `boss_rank`.
fn front(boss_rank: BossRank) -> FrontPosition {
    let f = match boss_rank {
        BossRank::Sector => FrontPosition { stage: 0, count: 8 },
        BossRank::Regional => FrontPosition { stage: 5, count: 8 },
        BossRank::Capstone => FrontPosition { stage: 7, count: 8 },
    };
    assert_eq!(f.boss_rank(), boss_rank);
    f
}

/// The deploy snapshot's transition count on `preset` at threat `rank`, on
/// a stage whose boss is `boss_rank` (None: no country, a Sector boss).
fn phases(preset: BalancePreset, rank: u32, boss_rank: Option<BossRank>) -> u8 {
    on(preset)
        .progression_for(Mode::Personal, rank, 11, boss_rank.map(front))
        .unwrap()
        .boss_phases
}

/// A legacy game with an unpaused attempt in `mode` on level `level`, frozen
/// at threat `rank` on `preset` and a `boss_rank` stage, whose director
/// spawns nothing on its own and whose defender cannot fall. The level's
/// boss stands in the middle lane at the spawn edge.
fn boss_game(
    mode: Mode,
    level: usize,
    preset: BalancePreset,
    rank: u32,
    boss_rank: BossRank,
) -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    // A save holds no encounter above the campaign's rank.
    g.campaign.victories = u64::from(rank);
    let mut a = g.attempt.take().unwrap();
    a.level = level;
    a.progression = on(preset)
        .progression_for(mode, rank, a.lane_count, Some(front(boss_rank)))
        .unwrap();
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    a.spawn_credit = -1.0e9;
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, lane, true);
    g.attempt = Some(a);
    g
}

fn boss(g: &Game) -> &Enemy {
    let a = g.attempt.as_ref().expect("the attempt runs");
    a.enemies.iter().find(|e| e.boss).expect("the boss lives")
}

/// Sets the boss's hit points (NPC: its injury hit points as well).
fn set_boss_hp(g: &mut Game, hp: i32) {
    let a = g.attempt.as_mut().unwrap();
    let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
    e.hp = hp;
    e.injury.hp = hp;
}

fn boss_phase_events(g: &Game) -> Vec<Event> {
    let a = g.attempt.as_ref().unwrap();
    a.events
        .iter()
        .filter(|e| e.kind == EventKind::BossPhase)
        .cloned()
        .collect()
}

#[test]
fn normal_rank_4_boss_has_no_phases_rank_5_has_one() {
    for preset in [BalancePreset::Easy, BalancePreset::Normal] {
        for rank in 0..=4 {
            assert_eq!(phases(preset, rank, None), 0, "{preset:?} rank {rank}");
        }
        for rank in [5, 10, 20, 50] {
            assert_eq!(phases(preset, rank, None), 1, "{preset:?} rank {rank}");
        }
        for boss_rank in [BossRank::Sector, BossRank::Regional] {
            assert_eq!(phases(preset, 4, Some(boss_rank)), 0);
            assert_eq!(phases(preset, 5, Some(boss_rank)), 1);
        }
    }
    // The spawned boss carries what the snapshot froze: nothing at rank 4,
    // one transition of its kind's single profile at rank 5.
    for mode in [Mode::Personal, Mode::Npc] {
        let g = boss_game(mode, 0, BalancePreset::Normal, 4, BossRank::Sector);
        assert!(boss(&g).boss_state.is_none(), "{mode:?} rank 4");
        let g = boss_game(mode, 0, BalancePreset::Normal, 5, BossRank::Sector);
        let state = boss(&g).boss_state.as_ref().expect("a rank-5 boss phases");
        assert_eq!(
            (state.transitions, state.entered, state.hold_until),
            (1, 0, 0)
        );
        assert_eq!(state.profile.name, "Breach", "{mode:?}");
    }
}

#[test]
fn hard_has_two_above_rank_4() {
    for rank in [5, 10, 20, 50] {
        assert_eq!(phases(BalancePreset::Hard, rank, None), 2, "rank {rank}");
        for boss_rank in [BossRank::Sector, BossRank::Regional, BossRank::Capstone] {
            assert_eq!(phases(BalancePreset::Hard, rank, Some(boss_rank)), 2);
        }
    }
    let g = boss_game(Mode::Personal, 0, BalancePreset::Hard, 5, BossRank::Sector);
    assert_eq!(boss(&g).boss_state.as_ref().unwrap().transitions, 2);
}

/// Plan §2.5 "Hard boss phases at ranks 0-4", settled on its default (the
/// W2-26 pin reading): Hard's `boss_phase_bonus` applies only past rank 4,
/// so at ranks 0-4 Hard has Normal's count, 0 on a Sector or Regional
/// stage and 1 on a Capstone.
#[test]
fn hard_ranks_zero_to_four_have_normals_phase_count() {
    for rank in 0..=4 {
        assert_eq!(phases(BalancePreset::Hard, rank, None), 0, "rank {rank}");
        for (boss_rank, count) in [
            (BossRank::Sector, 0),
            (BossRank::Regional, 0),
            (BossRank::Capstone, 1),
        ] {
            assert_eq!(
                phases(BalancePreset::Hard, rank, Some(boss_rank)),
                count,
                "rank {rank} {boss_rank:?}"
            );
        }
    }
}

/// Ruling W-9: a Capstone boss gains one transition on every preset,
/// clamped at two.
#[test]
fn capstone_has_one_extra_transition_clamped_at_two() {
    for preset in [BalancePreset::Easy, BalancePreset::Normal] {
        assert_eq!(phases(preset, 0, Some(BossRank::Capstone)), 1);
        assert_eq!(phases(preset, 4, Some(BossRank::Capstone)), 1);
        assert_eq!(phases(preset, 5, Some(BossRank::Capstone)), 2);
        assert_eq!(phases(preset, 30, Some(BossRank::Capstone)), 2);
    }
    assert_eq!(phases(BalancePreset::Hard, 5, Some(BossRank::Capstone)), 2);
}

#[test]
fn boss_crossing_50_percent_enters_phase_once_with_90_tick_hold_event_and_survives_save() {
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = boss_game(mode, 0, BalancePreset::Normal, 10, BossRank::Sector);
        let (id, max_hp) = (boss(&g).id, boss(&g).max_hp);
        // One hit point above half: no transition.
        set_boss_hp(&mut g, max_hp / 2 + 1);
        g.tick(Input::default());
        assert!(boss_phase_events(&g).is_empty(), "{mode:?}: above half");
        assert_eq!(boss(&g).boss_state.as_ref().unwrap().entered, 0);
        // At half it enters phase 1 on this tick and holds for 90 ticks.
        set_boss_hp(&mut g, max_hp / 2);
        g.tick(Input::default());
        let entered_at = g.attempt.as_ref().unwrap().tick;
        let events = boss_phase_events(&g);
        assert_eq!(events.len(), 1, "{mode:?}: one BossPhase event");
        let ev = &events[0];
        assert_eq!(
            (ev.value, ev.tick, ev.actor_id, ev.duration_ticks),
            (1, entered_at, Some(id), 90),
            "{mode:?}"
        );
        let state = boss(&g).boss_state.clone().unwrap();
        assert_eq!((state.entered, state.hold_until), (1, entered_at + 90));
        assert!(state.held(entered_at) && state.held(entered_at + 89));
        assert!(!state.held(entered_at + 90));
        assert_eq!(
            g.attempt.as_ref().unwrap().telemetry.boss_phase_ticks,
            vec![entered_at],
            "{mode:?}"
        );
        let (x, attack_left) = (boss(&g).x, boss(&g).attack_left);
        let mut resumed = None;
        for step in 1..=89 {
            if step == 45 {
                // A save holds a spawn credit in 0..1; both games go on
                // from the same state.
                g.attempt.as_mut().unwrap().spawn_credit = 0.0;
                let json = g.to_save_json().expect("a holding boss saves");
                let mut loaded =
                    Game::from_payload(serde_json::from_str(&json).unwrap(), SAVE_VERSION).unwrap();
                loaded.set_paused(false);
                assert_eq!(loaded.state_digest(), g.state_digest(), "{mode:?}");
                resumed = Some(loaded);
            }
            g.tick(Input::default());
            if let Some(r) = resumed.as_mut() {
                r.tick(Input::default());
            }
            assert_eq!(
                (boss(&g).x, boss(&g).attack_left),
                (x, attack_left),
                "{mode:?}: the boss holds at step {step}"
            );
        }
        // The hold ends 90 ticks after the transition; the boss walks on.
        g.tick(Input::default());
        assert!(boss(&g).x < x, "{mode:?}: the boss moves after its hold");
        let mut resumed = resumed.expect("saved mid-hold");
        resumed.tick(Input::default());
        assert_eq!(
            resumed.state_digest(),
            g.state_digest(),
            "{mode:?}: resumed"
        );
        assert_eq!(boss(&resumed).boss_state, boss(&g).boss_state);
        // Once: lower still, with only one transition frozen, nothing more.
        // (The presentation window has dropped the 90-tick event by now.)
        set_boss_hp(&mut g, max_hp / 10);
        for _ in 0..3 {
            g.tick(Input::default());
        }
        assert_eq!(boss(&g).boss_state.as_ref().unwrap().entered, 1);
        assert!(boss_phase_events(&g).is_empty(), "{mode:?}: entered once");
        assert_eq!(
            g.attempt.as_ref().unwrap().telemetry.boss_phase_ticks,
            vec![entered_at]
        );
    }
}

/// A Caller in phase 1 calls its reinforcements after `call_interval ×
/// 0.6`: 540 ticks instead of 900.
/// Design enemy-balance §6: for its 90-tick hold a boss in contact at the
/// line neither counts down its attack nor strikes, then resumes on the
/// tick the hold ends. The phase enters on tick 150, so the hold spans the
/// NPC turn at 180 and ends on the turn at 240.
#[test]
fn held_boss_neither_counts_down_nor_strikes() {
    for mode in [Mode::Personal, Mode::Npc] {
        // Some(n): a windup with n ticks left; None: ready to strike now.
        for windup_left in [Some(30), None] {
            let mut g = boss_game(mode, 0, BalancePreset::Normal, 10, BossRank::Sector);
            let max_hp = boss(&g).max_hp;
            let a = g.attempt.as_mut().unwrap();
            a.tick = 149;
            let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
            e.x = DEFENSE_LINE;
            e.contact_since = Some(148);
            e.attack_phase = EnemyPhase::Windup;
            e.attack_left = windup_left.unwrap_or(0);
            e.windup_total = 60;
            set_boss_hp(&mut g, max_hp / 2);
            let strikes = |g: &Game| g.attempt.as_ref().unwrap().telemetry.contact_attacks;
            // The defender's health: hit points, or NPC injury hit points.
            let health = |g: &Game| {
                let a = g.attempt.as_ref().unwrap();
                if mode == Mode::Npc { a.injury.hp } else { a.hp }
            };
            let before = (strikes(&g), health(&g));
            let label = format!("{mode:?} {windup_left:?}");
            for tick in 150..240 {
                g.tick(Input::default());
                let a = g.attempt.as_ref().unwrap();
                assert_eq!(a.tick, tick);
                let e = boss(&g);
                let state = e.boss_state.as_ref().unwrap();
                assert_eq!((state.entered, state.hold_until), (1, 240), "{label}");
                assert!(e.boss_held(tick), "{label}: held at {tick}");
                assert_eq!(
                    (e.attack_phase, e.attack_left, e.x),
                    (EnemyPhase::Windup, windup_left.unwrap_or(0), DEFENSE_LINE),
                    "{label}: no countdown at {tick}"
                );
                assert_eq!(
                    (strikes(&g), health(&g)),
                    before,
                    "{label}: no strike at {tick}"
                );
            }
            // Tick 240 ends the hold: the windup counts down, or the ready
            // boss strikes (on the NPC turn at 240 as well).
            g.tick(Input::default());
            let e = boss(&g);
            assert!(!e.boss_held(240), "{label}");
            match windup_left {
                Some(left) => {
                    assert_eq!(e.attack_left, left - 1, "{label}: counts down");
                    assert_eq!(strikes(&g), before.0, "{label}");
                }
                None => assert_eq!(strikes(&g), before.0 + 1, "{label}: strikes"),
            }
        }
    }
}

#[test]
fn caller_phase_shortens_interval() {
    for mode in [Mode::Personal, Mode::Npc] {
        let interval = |enter: bool| {
            let mut g = boss_game(mode, 1, BalancePreset::Normal, 10, BossRank::Sector);
            let mut a = g.attempt.take().unwrap();
            let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
            assert_eq!(e.boss_kind, Some(BossKind::Caller));
            let state = e.boss_state.as_mut().unwrap();
            assert_eq!(state.profile.name, "Chorus");
            if enter {
                state.entered = 1;
            }
            a.caller_tick = a.tick;
            g.spawn_director(&mut a);
            a.caller_tick - a.tick
        };
        assert_eq!(interval(false), 900, "{mode:?}");
        assert_eq!(interval(true), 540, "{mode:?}");
    }
}

/// Transition n applies each multiplier to the power n, clamped at the
/// profile bounds: speed at most 2, armor, call interval and siege recovery
/// at least 0.25, 0.3 and 0.3. At most one transition enters per tick.
#[test]
fn second_transition_clamped() {
    let profile = BossPhaseProfile {
        name: "Test".into(),
        speed_mult: 1.6,
        armor_mult: 0.4,
        call_interval_mult: 0.5,
        siege_recovery_mult: 0.5,
        hold_ticks: 90,
    };
    let state = |entered| BossState {
        profile: profile.clone(),
        transitions: 2,
        entered,
        hold_until: 0,
    };
    let mults = |s: &BossState| {
        (
            s.speed_mult(),
            s.armor_mult(),
            s.call_interval_mult(),
            s.siege_recovery_mult(),
        )
    };
    assert_eq!(mults(&state(0)), (1.0, 1.0, 1.0, 1.0));
    assert_eq!(mults(&state(1)), (1.6, 0.4, 0.5, 0.5));
    assert_eq!(
        mults(&state(2)),
        (2.0, 0.25, 0.3, 0.3),
        "the second transition clamps at the profile bounds"
    );
    // A Hard Breacher (two transitions) dropped below 25% in one blow enters
    // phase 1 on this tick and phase 2 on the next, and its second hold
    // starts then.
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = boss_game(mode, 0, BalancePreset::Hard, 10, BossRank::Sector);
        let max_hp = boss(&g).max_hp;
        set_boss_hp(&mut g, max_hp / 5);
        g.tick(Input::default());
        let first = g.attempt.as_ref().unwrap().tick;
        assert_eq!(boss(&g).boss_state.as_ref().unwrap().entered, 1, "{mode:?}");
        g.tick(Input::default());
        let state = boss(&g).boss_state.clone().unwrap();
        assert_eq!((state.entered, state.hold_until), (2, first + 1 + 90));
        let values: Vec<_> = boss_phase_events(&g)
            .iter()
            .map(|e| (e.value, e.tick))
            .collect();
        assert_eq!(values, [(1, first), (2, first + 1)], "{mode:?}");
        assert_eq!(
            g.attempt.as_ref().unwrap().telemetry.boss_phase_ticks,
            vec![first, first + 1]
        );
        // Breach twice: speed 0.6 × 1.4², armor × 0.6².
        assert!((boss_speed(boss(&g)) - 0.6 * 1.4 * 1.4).abs() < 1e-6);
        assert!((state.armor_mult() - 0.36).abs() < 1e-6);
    }
}

/// The Breacher's armor multiplier applies to every hit while it is armored
/// (exposure still drops it to 0), and the Siege Brute's recovery shortens.
#[test]
fn phase_armor_and_siege_recovery_follow_the_profile() {
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = boss_game(mode, 0, BalancePreset::Normal, 10, BossRank::Sector);
        let a = g.attempt.as_mut().unwrap();
        let tick = a.tick;
        // Born now: armored for the first 7 s of its cycle.
        let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
        e.armor = 10;
        e.born_tick = tick;
        let e = e.clone();
        assert_eq!(enemy_armor_vs(a, &e, HitSource::Shot), 10, "{mode:?}");
        let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
        e.boss_state.as_mut().unwrap().entered = 1;
        let e = e.clone();
        assert_eq!(enemy_armor_vs(a, &e, HitSource::Shot), 6, "{mode:?}: × 0.6");
        assert_eq!(enemy_armor_vs(a, &e, HitSource::Melee), 6, "{mode:?}");
        // The inspector's protection is the armor hits meet.
        assert_eq!(a.enemy_protection(&e), 6, "{mode:?}: protection");
        // Exposed after its 7 s delay: still 0.
        let mut exposed = a.clone();
        exposed.tick = tick + 7 * 60;
        assert!(e.exposed_at(exposed.tick));
        assert_eq!(enemy_armor_vs(&exposed, &e, HitSource::Shot), 0, "{mode:?}");
        assert_eq!(exposed.enemy_protection(&e), 0, "{mode:?}");
    }
    let recovery = |enter: bool| {
        let mut g = boss_game(
            Mode::Personal,
            2,
            BalancePreset::Normal,
            10,
            BossRank::Sector,
        );
        let mut a = g.attempt.take().unwrap();
        a.tick += 5;
        let tick = a.tick;
        let e = a.enemies.iter_mut().find(|e| e.boss).unwrap();
        assert_eq!(e.boss_kind, Some(BossKind::SiegeBrute));
        assert_eq!(e.boss_state.as_ref().unwrap().profile.name, "Enrage");
        if enter {
            e.boss_state.as_mut().unwrap().entered = 1;
        }
        e.x = DEFENSE_LINE;
        e.contact_since = Some(tick - 1);
        e.attack_left = 0;
        g.enemy_attacks(&mut a);
        let e = a.enemies.iter().find(|e| e.boss).unwrap();
        (e.attack_phase, e.attack_left)
    };
    assert_eq!(recovery(false), (EnemyPhase::Recovery, 240));
    assert_eq!(recovery(true), (EnemyPhase::Recovery, 144));
}
