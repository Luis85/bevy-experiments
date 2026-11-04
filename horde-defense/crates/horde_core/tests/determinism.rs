#![allow(clippy::print_stdout)] // prints digests for re-blessing
//! Golden determinism: fixed seeds driven through the headless bot must reach
//! exactly the pinned `Game::state_digest()` after a fixed number of ticks.
//!
//! The digest hashes the canonical serialized save payload, so ANY change to
//! gameplay outcomes, RNG consumption, or the serialized shape of the game
//! changes it. A failure here means one of those happened.
//!
//! Intentionally changing outcomes (or the save format) is a rebaseline and
//! needs explicit approval. To re-bless: run
//! `cargo test -p horde_core --test determinism -- --nocapture`, copy the
//! printed digests into `GOLDEN`, and state in the commit why every seed moved.
//!
//! Save v5 re-bless (format-only, approved): the ordnance counters became
//! kind-keyed maps, which changes the serialized payload of every run but no
//! gameplay; `GAMEPLAY_TRACE` pins the unchanged outcomes of the same runs.
//!
//! Save v6 re-bless (format-only): level progress and level references are
//! keyed by stable level ID; `GAMEPLAY_TRACE` is again unchanged.
mod support;
use horde_core::content::{Mode, Policy};
use horde_core::sim::{Game, smoke_input};
use support::advance;

const TICKS: u32 = 1800;
const SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];

/// `(seed, mode)` -> digest after [`TICKS`] ticks of the headless bot.
/// Re-blessed for save v6: level progress (`first_clears`, NPC claims) is
/// serialized as level-ID sets/maps, the redundant `unlocked` frontier is no
/// longer saved and attempt/report levels serialize as level IDs. The payload
/// of every run changed, no gameplay did: `GAMEPLAY_TRACE` is unchanged.
const GOLDEN: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0x303e_a36f_ac6a_070b),
    (1, Mode::Npc, 0xeede_2391_290b_b381),
    (0x1A57_1167, Mode::Personal, 0x3be1_3138_6490_e3b2),
    (0x1A57_1167, Mode::Npc, 0x6a2b_a00f_7e40_18d8),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x5aaf_1f08_0eb1_da92),
    (0xDEAD_BEEF_2026, Mode::Npc, 0xc1bb_9ef2_0792_5afc),
];

/// `(seed, mode)` -> FNV-1a of the per-tick gameplay trace (tick, hp, coins,
/// enemy ids/hp/positions and the event queue; no serialized format) recorded
/// on the v4 build (commit c6f518d) before the ordnance arrays became kind
/// maps. The save-v5 and save-v6 re-blesses of `GOLDEN` are format-only: this trace proves
/// the gameplay outcomes of the same six runs did not move.
const GAMEPLAY_TRACE: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0xa6ca_a6de_7fd9_2b1a),
    (1, Mode::Npc, 0x2966_10f8_ded8_a91f),
    (0x1A57_1167, Mode::Personal, 0x8beb_3e2e_0553_347a),
    (0x1A57_1167, Mode::Npc, 0x85ec_9e52_63ea_48af),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x46a2_adc6_720f_8678),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x1fda_7a06_49d4_4756),
];

fn run(seed: u64, mode: Mode) -> Game {
    run_observed(seed, mode, |_| {})
}

/// The first-stage encounter the `smoke` command plays, for `TICKS` ticks;
/// `observe` sees the game after every tick.
fn run_observed(seed: u64, mode: Mode, mut observe: impl FnMut(&Game)) -> Game {
    let mut g = Game::default();
    g.set_seed(seed);
    g.campaign_mut().attempts = 1;
    if mode == Mode::Npc {
        g.campaign_mut().legacy_features = true;
        g.campaign_mut().npc.policy = Policy::BossFocus;
    }
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    for _ in 0..TICKS {
        let input = smoke_input(&g);
        g.tick(input);
        observe(&g);
    }
    g
}

#[test]
fn seeded_headless_runs_keep_their_pinned_gameplay_traces() {
    for (seed, mode, expected) in GAMEPLAY_TRACE {
        let mut hash = support::fnv1a(b"");
        run_observed(seed, mode, |g| {
            let a = g.attempt().unwrap();
            let line = format!(
                "t{} hp{} coins{} e{:?} ev{:?}\n",
                a.tick,
                a.hp,
                a.coins,
                a.enemies
                    .iter()
                    .map(|e| (e.id, e.hp, e.x.to_bits(), e.y().to_bits()))
                    .collect::<Vec<_>>(),
                a.events
            );
            hash = support::fnv1a_extend(hash, line.as_bytes());
        });
        assert_eq!(hash, expected, "seed {seed:#x} {mode:?} gameplay moved");
    }
}

#[test]
fn seeded_headless_runs_reach_the_pinned_state_digests() {
    let mut mismatches = Vec::new();
    for (seed, mode, expected) in GOLDEN {
        let g = run(seed, mode);
        let digest = g.state_digest();
        println!("({seed:#x}, Mode::{mode:?}, {digest:#018x}),");
        if digest != expected {
            mismatches.push(format!(
                "seed {seed:#x} {mode:?}: expected {expected:#018x}, got {digest:#018x}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Simulation state diverged from the pinned determinism digests:\n{}\n\n\
         If this change is intentional, re-bless GOLDEN in tests/determinism.rs \
         (see the file header). Rebaselining gameplay requires explicit approval.",
        mismatches.join("\n")
    );
}

#[test]
fn golden_runs_exercise_a_live_battle_and_depend_on_the_seed() {
    let mut digests = std::collections::BTreeSet::new();
    for seed in SEEDS {
        for mode in [Mode::Personal, Mode::Npc] {
            let g = run(seed, mode);
            let a = g.attempt().expect("battle is still running");
            assert_eq!(a.tick, u64::from(TICKS));
            assert!(a.telemetry.spawned > 0);
            digests.insert(g.state_digest());
        }
    }
    assert_eq!(digests.len(), 6, "every seed/mode pair has its own state");
}

#[test]
fn the_digest_is_a_pure_function_of_state() {
    let g = run(SEEDS[0], Mode::Personal);
    assert_eq!(g.state_digest(), g.clone().state_digest());
    let mut moved = g.clone();
    advance(&mut moved, 1);
    assert_ne!(g.state_digest(), moved.state_digest());
}
