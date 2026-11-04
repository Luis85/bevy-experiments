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
mod support;
use horde_core::content::{Mode, Policy};
use horde_core::sim::{Game, smoke_input};
use support::advance;

const TICKS: u32 = 1800;
const SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];

/// `(seed, mode)` -> digest after [`TICKS`] ticks of the headless bot.
const GOLDEN: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0x2e20_3b0e_cbb0_a2df),
    (1, Mode::Npc, 0x666d_d978_5ba3_8601),
    (0x1A57_1167, Mode::Personal, 0x4da1_2071_d6c2_a94a),
    (0x1A57_1167, Mode::Npc, 0xa0d0_31d1_3edd_1996),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x9900_ad25_4673_28e2),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x72f7_f039_a9dd_7280),
];

/// `(seed, mode)` -> FNV-1a of the per-tick gameplay trace (tick, hp, coins,
/// enemy ids/hp/positions and the event queue; no serialized format) recorded
/// on the v4 build (commit c6f518d) before the ordnance arrays became kind
/// maps. The save-v5 re-bless of `GOLDEN` is format-only: this trace proves
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
    g.seed = seed;
    g.campaign.attempts = 1;
    if mode == Mode::Npc {
        g.campaign.legacy_features = true;
        g.campaign.npc.policy = Policy::BossFocus;
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
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        run_observed(seed, mode, |g| {
            let a = g.attempt.as_ref().unwrap();
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
            hash = line.bytes().fold(hash, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            });
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
            let a = g.attempt.as_ref().expect("battle is still running");
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
