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
mod support;
use horde_core::content::{Mode, Policy};
use horde_core::sim::{Game, smoke_input};
use support::advance;

const TICKS: u32 = 1800;
const SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];

/// `(seed, mode)` -> digest after [`TICKS`] ticks of the headless bot.
const GOLDEN: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0x0b18_ef9e_9f3f_db8f),
    (1, Mode::Npc, 0x13e5_aa1a_f902_4db9),
    (0x1A57_1167, Mode::Personal, 0x2175_368a_9d8d_176a),
    (0x1A57_1167, Mode::Npc, 0x8441_e448_9411_5e96),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x6579_9cfc_9dd3_de72),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x57c7_5e07_a11d_c660),
];

/// The first-stage encounter the `smoke` command plays, for `TICKS` ticks.
fn run(seed: u64, mode: Mode) -> Game {
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
    }
    g
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
