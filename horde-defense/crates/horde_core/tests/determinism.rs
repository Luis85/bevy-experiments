#![allow(clippy::print_stdout)] // prints digests for re-blessing
//! Golden determinism: fixed seeds driven through the headless bot must reach
//! exactly the pinned `Game::state_digest()` after a fixed number of ticks.
//!
//! The digest hashes the canonical serialized save payload, so ANY change to
//! gameplay outcomes, RNG consumption, or the serialized shape of the game
//! changes it. A failure here means one of those happened.
//!
//! Intentionally changing outcomes (or the save format) is a rebaseline and
//! needs explicit approval. The tooling:
//!
//! - `cargo test -p horde_core --test determinism -- --nocapture` prints the
//!   six `GOLDEN` lines, the six `GAMEPLAY_TRACE` lines and the six outcome
//!   summaries (kills, coins, hp, loot, spawned and `rng.state` at ticks 600,
//!   1200 and 1800) of the current build.
//! - `cargo test -p horde_core --test determinism -- --ignored --nocapture
//!   determinism_anatomy` prints each run's anatomy (golden, trace, key-path,
//!   item, telemetry-stripped and outcome hashes). Run it on the build before
//!   and after a move; `support::classify` labels each move F (format),
//!   C (content), T (telemetry and receipts) or G (gameplay).
//!
//! **Provisional-pin ledger (ruling P3).** A commit that moves a digest does
//! not edit `GOLDEN` or `GAMEPLAY_TRACE`. It sets `PROVISIONAL` to the six new
//! digests of each table (GOLDEN order) and appends its cause to
//! `PROVISIONAL_CAUSES` (commit tag, class, reason), so every commit stays
//! green and the cause is recorded where it happens. The tests then pin the
//! provisional digests, and they accept `PROVISIONAL` only with causes. The
//! wave's dedicated save-determinism re-bless commit folds `PROVISIONAL` into
//! `GOLDEN` and `GAMEPLAY_TRACE`, adds one row per moved digest to the log
//! below, and resets `PROVISIONAL` to None and `PROVISIONAL_CAUSES` to empty.
//! The wave-close critic checks that `PROVISIONAL` is None.
//!
//! `TRACE_RULESET` must equal `RULESET_VERSION`: a ruleset bump states in the
//! same commit what it does to `GAMEPLAY_TRACE`.
//!
//! Save v5 re-bless (format-only, approved): the ordnance counters became
//! kind-keyed maps, which changes the serialized payload of every run but no
//! gameplay; `GAMEPLAY_TRACE` pins the unchanged outcomes of the same runs.
//!
//! Save v6 re-bless (format-only): level progress and level references are
//! keyed by stable level ID; `GAMEPLAY_TRACE` is again unchanged.
//!
//! Re-bless log:
//!
//! | wave | commit | seed/mode | GOLDEN old->new | TRACE old->new | class | cause |
//! |---|---|---|---|---|---|---|
//! | pre-pass | 9b6454e | 0x1 Personal | 0x0b18ef9e9f3fdb8f->0x2e203b0ecbb0a2df | pinned 0xa6caa6de7fd92b1a | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1 Npc | 0x13e5aa1af9024db9->0x666dd9785ba38601 | pinned 0x296610f8ded8a91f | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1a571167 Personal | 0x2175368a9d8d176a->0x4da12071d6c2a94a | pinned 0x8beb3e2e0553347a | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1a571167 Npc | 0x8441e44894115e96->0xa0d031d13edd1996 | pinned 0x85ec9e5263ea48af | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0xdeadbeef2026 Personal | 0x65799cfc9dd3de72->0x9900ad25467328e2 | pinned 0x46a2adc6720f8678 | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0xdeadbeef2026 Npc | 0x57c75e07a11dc660->0x72f7f039a9dd7280 | pinned 0x1fda7a0649d44756 | F | save v5 ordnance kind maps |
//! | pre-pass | 8103a97 | 0x1 Personal | 0x2e203b0ecbb0a2df->0x303ea36fac6a070b | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1 Npc | 0x666dd9785ba38601->0xeede2391290bb381 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1a571167 Personal | 0x4da12071d6c2a94a->0x3be131386490e3b2 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1a571167 Npc | 0xa0d031d13edd1996->0x6a2ba00f7e4018d8 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0xdeadbeef2026 Personal | 0x9900ad25467328e2->0x5aaf1f080eb1da92 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0xdeadbeef2026 Npc | 0x72f7f039a9dd7280->0xc1bb9ef207925afc | unchanged | F | save v6 level IDs |
//! | 1 | W1-69 | 0x1a571167 Personal | 0x3be131386490e3b2->0x2902d75f0238e1a3 | 0x8beb3e2e0553347a->0x5cc8bde4faf09763 | G | W1-69 user decision: enemies no longer queue behind each other (a body that waited 0.65 yd behind another now walks on; outcome at t1800 kills 11, coins 37, loot 1) |
//! | 1 | W1-69 | 0x1a571167 Npc | unchanged | 0x85ec9e5263ea48af->0x20c2f0bd540515c4 | G | W1-69 user decision: enemies no longer queue behind each other (no passing offset, so traced y and event order move; end state unchanged) |
//! | 1 | W1-69 | 0xdeadbeef2026 Npc | unchanged | 0x1fda7a0649d44756->0x9d7678e43c2f560f | G | W1-69 user decision: enemies no longer queue behind each other (no passing offset, so traced y and event order move; end state unchanged) |
//! | 2 | W2-49 | 0x1 Personal | 0x303ea36fac6a070b->0xbed5e2714b6590fe | unchanged | T W2-05, F W2-06, F W2-52, F W2-54; net F | W2-05 telemetry maps; W2-06 drop_tier Some(1); W2-52 level_grant_paid 2 (level 1 -> 2); W2-54 the authored grant makes the shim inert, so level_grant_paid is no longer saved. Anatomy: keys and core move; items, outcome and trace unchanged |
//! | 2 | W2-49 | 0x1 Npc | 0xeede2391290bb381->0xcffe0be4463d46d6 | unchanged | T W2-05, F W2-06; net F | W2-05 telemetry maps; W2-06 drop_tier Some(1). Anatomy: keys and core move; items, outcome and trace unchanged |
//! | 2 | W2-49 | 0x1a571167 Personal | 0x2902d75f0238e1a3->0x3368f18097f7fb5e | unchanged | T W2-05, F W2-06, C W2-08, F W2-52, G W2-09 (support::classify: C), F W2-54; net F | W2-05 telemetry maps; W2-06 drop_tier Some(1) and the one drop in drop_ids; W2-08 the 39 loot_only items enlarge the tier-1 pool (12 -> 18), so the one a.rng draw picks another definition; W2-52 level_grant_paid 2; W2-09 the weighted single draw (R33: a gameplay mechanic, G, though classify labels it C, items only) lands on the wave-1 definition again, so the net items hash is unchanged; W2-54 level_grant_paid no longer saved. Anatomy: keys and core move; items, outcome and trace unchanged (t1800 kills 11, coins 37, loot 1) |
//! | 2 | W2-49 | 0x1a571167 Npc | 0x6a2ba00f7e4018d8->0x97d93e67bd2bed2f | unchanged | T W2-05, F W2-06; net F | W2-05 telemetry maps; W2-06 drop_tier Some(1). Anatomy: keys and core move; items, outcome and trace unchanged |
//! | 2 | W2-49 | 0xdeadbeef2026 Personal | 0x5aaf1f080eb1da92->0x7be5c84d229d8d71 | unchanged | T W2-05, F W2-06, F W2-52, F W2-54; net F | as 0x1 Personal. Anatomy: keys and core move; items, outcome and trace unchanged |
//! | 2 | W2-49 | 0xdeadbeef2026 Npc | 0xc1bb9ef207925afc->0x6dcebbedcc7582c9 | unchanged | T W2-05, F W2-06; net F | W2-05 telemetry maps; W2-06 drop_tier Some(1). Anatomy: keys and core move; items, outcome and trace unchanged |
//! | closing | closing | 0x1 Npc | 0xcffe0be4463d46d6->0x61341ecf1c422fd8 | 0x296610f8ded8a91f->0x77cec20196014f8d | G W4-14 aim, G W4-14 arrival; net G | W4-14 aim (fbedee2): Mara (Balanced) aims only when the aim raises her expected hits per turn, so her turns and shots move; W4-14 arrival (93321b8): `npc_arrival_factor` 0.9 -> 0.5 halves the Npc arrivals. t1800 kills 7 -> 4, spawned 8 -> 4, coins 24 -> 14, hp 13 kept. Anatomy: keys (a sampled hostile holds its hit-stop, `/attempt/enemies/statuses`), core, outcome and trace move; items unchanged |
//! | closing | closing | 0x1a571167 Npc | 0x97d93e67bd2bed2f->0x0118059a899fb437 | 0x20c2f0bd540515c4->0x1fb8dfb5f91248c7 | G W4-14 aim, G W4-14 arrival; net G | as 0x1 Npc. t1800 kills 7 -> 4, spawned 8 -> 4, coins 22 -> 13, hp 13 kept |
//! | closing | closing | 0xdeadbeef2026 Npc | 0x6dcebbedcc7582c9->0x7a662cd4a007adc2 | 0x9d7678e43c2f560f->0x6fedc1ce190ab8e3 | G W4-14 aim, G W4-14 arrival; net G | as 0x1 Npc. t1800 kills 7 -> 4, spawned 8 -> 4, coins 22 -> 14, hp 13 kept |
//!
//! W1-69 is the wave-1 re-bless (ruling R7, folded directly: the user's
//! 2026-11-06 decision that enemies overlap is the only cause, so no
//! `PROVISIONAL` step). The other five GOLDEN and three TRACE digests, the
//! V1-V6 continuation pins, `V6_DROP_TRACE_FNV` and the `save_v7.json` pins
//! did not move.
//!
//! W2-49 is the wave-2 re-bless (rulings P3, R7, R15, R16, R33). It folds the
//! `PROVISIONAL` digests that W2-05, W2-06, W2-08, W2-52, W2-09 and W2-54 set
//! into `GOLDEN`: 6/6 moved, `GAMEPLAY_TRACE` 0/6. The class column lists each
//! cause's class in commit order and the net label `support::classify` gives
//! the wave-1-close anatomy (ab03da9) against the W2-49 anatomy. Every run
//! nets F: its key paths and its telemetry-stripped payload move, while its
//! items, outcome summary and trace do not. W2-09's row is G under R33 (the
//! weighted pick is a gameplay mechanic) though `support::classify` labels its
//! own move C (items only). Anatomy, wave-1 close -> W2-49:
//!
//! | seed/mode | keys | core | items | outcome | trace |
//! |---|---|---|---|---|---|
//! | 0x1 Personal | 0x8a4c8226e2df0f7a->0xbdf9c3b221052103 | 0x2d4354e3a69b4e4c->0x610e5241cbe719a5 | unchanged 0x9148a6bf065cdfaa | unchanged 0x41fef8752e01bcc2 | unchanged |
//! | 0x1 Npc | 0x8a4c8226e2df0f7a->0xbdf9c3b221052103 | 0x9d9eeb61bc22a6a0->0x77d9ff0f18cfcf07 | unchanged 0x9148a6bf065cdfaa | unchanged 0x237989fcf5b0919e | unchanged |
//! | 0x1a571167 Personal | 0x4bbabacd6a590564->0x72f5e366ed803b2a | 0x63f7f6bb7fd129f3->0xa87c9461fccc04a6 | unchanged 0x48f43caeb9493765 | unchanged 0x26d7eb5a5f8d8505 | unchanged |
//! | 0x1a571167 Npc | 0x8a4c8226e2df0f7a->0xbdf9c3b221052103 | 0xdbac444dcac24b75->0x1c1a5cda83ff432c | unchanged 0x9148a6bf065cdfaa | unchanged 0x6552fa80f7948cbe | unchanged |
//! | 0xdeadbeef2026 Personal | 0x8a4c8226e2df0f7a->0xbdf9c3b221052103 | 0xc4ed2aebbeaff2d8->0xf9d3e482239eb995 | unchanged 0x9148a6bf065cdfaa | unchanged 0xeff821037e64ea04 | unchanged |
//! | 0xdeadbeef2026 Npc | 0x8a4c8226e2df0f7a->0xbdf9c3b221052103 | 0x3b5bd46dcb5972b8->0x713699b0faef6131 | unchanged 0x9148a6bf065cdfaa | unchanged 0xabd2f667ea5645d2 | unchanged |
//!
//! `RULESET_VERSION` and `TRACE_RULESET` move 2 -> 3 in W2-49: the bump names
//! W1-69 (no lane queue, R15) and the wave-2 rules, and `GAMEPLAY_TRACE` is
//! unchanged by it. V1-V6 continuation pins, `V6_DROP_TRACE_FNV` and the
//! `save_v7.json` pins did not move. `PROVISIONAL` is None again.
//!
//! W4-08 is the re-bless for waves 3 and 4 up to W4-08 (rulings P3, R7,
//! R36, R56, R57); W4-14 and W4-15 came after it, and the closing re-bless
//! folded them. No commit since W2-49 set `PROVISIONAL`: `PROVISIONAL` stayed
//! None and `PROVISIONAL_CAUSES` empty, and this file's tables are unchanged
//! from e3f52fa to W4-08, so there is nothing to fold and the log gains no
//! row. `GOLDEN` 0/6, `GAMEPLAY_TRACE` 0/6. Why each wave-4 rule leaves the
//! six runs alone (each is one rank-0 deploy with no country, no camp action
//! and no worker game; `support::smoke_run`):
//!
//! | commit | rule | why no digest moves |
//! |---|---|---|
//! | W4-01 | Regional and Capstone boss multipliers (`progression.json` `boss_ranks`) | folded only with a country front position; without a country the boss is a Sector boss and nothing is folded |
//! | W4-03 | steady_hands, hardened, quick_throw, combat_reload retunes; shrapnel_trap `charges_per_rank` 1 | the nodes are never bought and every `structure_upgrades` rank is 0, so the retuned effects and the extra charge are never applied |
//! | W4-p | repriced skill tree, `sim::tier_cost`, `Campaign::blueprint_price` | prices and the tier definition are read only by camp purchases, which these runs never make |
//! | W4-l | auto-sell pacing gate, stash-growth report, `loot::max_drop_tier` in the validator | measurement code and a value-equal refactor; no drop or sale rule changed |
//! | W4-12 | expedition threat rank follows the recruit (R57) | read only by a worker-game Npc deploy; the runs create no expedition (R57's expected Npc GOLDEN move does not happen) |
//! | W4-13 | goal-6 measurement profile | acceptance bot only, no balance data (R56) |
//!
//! W4-02 and W4-04..n changed no rule (presentation lines, and a tuning
//! loop that kept no lever), W4-c shipped only its record, and wave 3
//! changed presentation only. The V1-V6 continuation pins,
//! `V6_DROP_TRACE_FNV`, the `save_v7.json` pins and the frozen-catalogue
//! pin did not move either; no continuation pin moved for W4-03's trap
//! charge (R38), so no charges re-bless reason is owed. Wave 4 changed no
//! named preset's `BalanceSettings`, so no `Campaign::follow_named_preset`
//! reconcile is added (design-save-determinism §4).
//!
//! `RULESET_VERSION` and `TRACE_RULESET` move 3 -> 4 in W4-08: the bump
//! names the wave-2 review rule fixes after W2-49 and the wave-4 rules
//! above, and `GAMEPLAY_TRACE` is unchanged by it. `PROVISIONAL` is None.
//!
//! **Progressed families (W4-09).** `PROGRESSED_GOLDEN` and
//! `PROGRESSED_TRACE` pin four runs the six above never reach: seeds 1 and
//! 0xDEAD_BEEF_2026, each as the player on the Hard preset's Capstone stage
//! and as a recruit taking the main run over (Npc mode), at threat rank 12
//! for 3600 ticks (`support::progressed_determinism_game`; its doc comment
//! lists what is staged and what is played through `Game::apply`). The
//! attempt clock starts 1800 ticks before the boss (3000 for the takeovers
//! since W4-14, which halved the Npc arrivals), so each window holds
//! elites, the new archetypes, the boss and its phases, and the settlement
//! of the win. They were cut at the wave-4 close, after W4-08, so they hold
//! every wave-4 rule. They follow the same ledger: a commit that moves one
//! sets `PROGRESSED_PROVISIONAL` and appends its cause to
//! `PROGRESSED_PROVISIONAL_CAUSES`, and the closing-phase re-bless folds them
//! (rulings P3, R36). Waves 5-7 expect no move (PROGRESSED 0/4).
//!
//! **Closing re-bless (rulings P3, R36, R59, R61).** The closing phase of
//! the production pass folds every provisional pin set after W4-08: W4-14
//! aim (fbedee2) and W4-14 arrival (93321b8) moved the three Npc `GOLDEN`
//! and `GAMEPLAY_TRACE` runs (log rows above), the two recruit takeovers
//! and the v7 continuation (`tests/save_migration.rs`), and W4-15 price
//! (63283a9) moved all four progressed runs. W4-15's `train_companions`
//! (62f90d3) and W4-16 (9f18943) moved nothing, and no wave-5, wave-6 or
//! wave-7 commit set a pin. `GOLDEN` 3/6 and `GAMEPLAY_TRACE` 3/6 (the
//! three Personal runs are unchanged in both), `PROGRESSED_GOLDEN` 4/4,
//! `PROGRESSED_TRACE` 4/4 and `V7_CONTINUATION_FNV` moved; every move nets
//! G (each moved run's trace moves, which `support::classify` ranks first).
//! The V1-V6 continuation pins, `V6_DROP_TRACE_FNV`, the `save_v7.json`
//! pins and the frozen-catalogue pin did not move. Progressed re-bless log:
//!
//! | run | PROGRESSED_GOLDEN old->new | PROGRESSED_TRACE old->new | class | cause |
//! |---|---|---|---|---|
//! | 0x1 HardCapstone | 0x1725164d48e78d12->0xab7276a741312f38 | 0x209f0b4c6c60a4aa->0x7aef2a4298a6356a | G W4-15 price; net G | the staged purse buys every node it affords in catalogue order, and Survivor network at 1,500 coins (was 1,700) leaves 200 more for later nodes, so the node set and the window's play move. Anatomy: core and trace move; keys, items and outcome (Victory, 213 spawned, elites 18/21, phases 16705/17137) unchanged |
//! | 0x1 RecruitTakeover | 0x3d80a50dd3323feb->0x6ffd2abd5ec559b7 | 0x96f568d39deb47d9->0x89869eb54e0b06ea | G W4-14 aim, G W4-14 arrival, G W4-15 price; net G | the recruit aims only when the aim pays; half the arrivals, so the window starts 3000 ticks before the boss (`PROGRESSED_TAKEOVER_BOSS_LEAD`, was 1800) to keep an elite, the new archetypes and the boss phase in it; the price as above. Still a Victory: spawned 59 -> 48, elites 2/2, phase 14640 -> 14400. Anatomy: core, items, outcome and trace move; keys unchanged |
//! | 0xdeadbeef2026 HardCapstone | 0x91697e752793cfc0->0x52782b9ffeef7166 | 0xcaea9c904c7b2910->0xf483cce12e822eaa | G W4-15 price; net G | as 0x1 HardCapstone. Anatomy: core and trace move; keys, items and outcome (Victory, 200 spawned, elites 10/14, phases 16752/17184) unchanged |
//! | 0xdeadbeef2026 RecruitTakeover | 0x5670ca6b321fb553->0xa040f93de56591c4 | 0x6a779762dcfd6acf->0x9e1e5d57ab945f10 | G W4-14 aim, G W4-14 arrival, G W4-15 price; net G | as 0x1 RecruitTakeover. Still a Victory: spawned 62 -> 52, elites 1/1 -> 2/2, phase 14700 -> 14640. Anatomy: core, items, outcome and trace move; keys unchanged |
//!
//! Anatomy, W4-09 (fa616e9, `determinism_anatomy` on an exported tree) ->
//! closing, the moved runs only (the three Personal runs are identical):
//!
//! | run | keys | core | items | outcome | trace |
//! |---|---|---|---|---|---|
//! | 0x1 Npc | 0xbdf9c3b221052103->0x3ce0e643620df24d | 0x77d9ff0f18cfcf07->0x8bc68eb7ffc43ebf | unchanged 0x9148a6bf065cdfaa | 0x237989fcf5b0919e->0xd9e98636939d3490 | moved |
//! | 0x1a571167 Npc | 0xbdf9c3b221052103->0x3ce0e643620df24d | 0x1c1a5cda83ff432c->0x5612de3a687bd9be | unchanged 0x9148a6bf065cdfaa | 0x6552fa80f7948cbe->0x84e925834cd1dbfb | moved |
//! | 0xdeadbeef2026 Npc | 0xbdf9c3b221052103->0x3ce0e643620df24d | 0x713699b0faef6131->0xd1cd92d43eeecfa6 | unchanged 0x9148a6bf065cdfaa | 0xabd2f667ea5645d2->0xcdd66ac45276934e | moved |
//! | 0x1 HardCapstone | unchanged 0x7cc53f4e2adb718e | 0xad40cb3bccf42a3f->0xd10943ed93c3885d | unchanged 0xd77a4c3be9f87058 | unchanged 0xa5da30c3da8f6d36 | moved |
//! | 0x1 RecruitTakeover | unchanged 0x92507d97d5f0305b | 0x7369eb1d3d07fbab->0xf988ad5ab12d1736 | 0xa423fafc4e4e1189->0x6a697b7b5f61e995 | 0x7b788fcc44678727->0xb31b80a582687d21 | moved |
//! | 0xdeadbeef2026 HardCapstone | unchanged 0x7cc53f4e2adb718e | 0xf2d8d02b2cdb5702->0xe0cd126cdf694df8 | unchanged 0xfe8a86e4e51953ee | unchanged 0x9fed1586eb6b5fe0 | moved |
//! | 0xdeadbeef2026 RecruitTakeover | unchanged 0x92507d97d5f0305b | 0xf4f8c3510afef6b0->0x57dbe78de0b5c97a | 0xa5fc27b8dc9c85ca->0x6cf5b81812470c88 | 0x473abb56e8d93edc->0x7a3c2324e5794a38 | moved |
//!
//! `RULESET_VERSION` and `TRACE_RULESET` move 4 -> 5 in the closing
//! re-bless: the bump names W4-14 aim, W4-14 arrival and W4-15 price, and
//! its `GAMEPLAY_TRACE` moves are the three Npc rows above.
//! `PROVISIONAL` and `PROGRESSED_PROVISIONAL` are None again.
mod support;
use horde_core::command::Command;
use horde_core::content::Mode;
use horde_core::defenders::DefenderId;
use horde_core::progression::BossRank;
use horde_core::sim::{CombatTelemetry, Game, RULESET_VERSION, SAVE_VERSION};
use support::{
    Anatomy, MoveClass, OutcomeSummary, PROGRESSED_RANK, PROGRESSED_TICKS, ProgressedRun, advance,
    progressed_run, smoke_run,
};

const TICKS: u32 = 1800;
const SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];

/// `(seed, mode)` -> digest after [`TICKS`] ticks of the headless bot.
/// Re-blessed for save v6: level progress (`first_clears`, NPC claims) is
/// serialized as level-ID sets/maps, the redundant `unlocked` frontier is no
/// longer saved and attempt/report levels serialize as level IDs. The payload
/// of every run changed, no gameplay did: `GAMEPLAY_TRACE` is unchanged.
/// W1-69 (user decision, enemies no longer queue behind each other) moved
/// 0x1A57_1167 Personal. W2-49 (the wave-2 re-bless) folded the wave-2
/// provisional digests into all six (net class F; outcomes and traces
/// unchanged); see the re-bless log. Waves 3 and 4 up to W4-08 (the W4-08
/// re-bless) moved none of them. The closing re-bless folded W4-14 (the Npc
/// aim and arrival pass, a later wave-4 row) into the three Npc runs,
/// class G.
const GOLDEN: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0xbed5_e271_4b65_90fe),
    (1, Mode::Npc, 0x6134_1ecf_1c42_2fd8),
    (0x1A57_1167, Mode::Personal, 0x3368_f180_97f7_fb5e),
    (0x1A57_1167, Mode::Npc, 0x0118_059a_899f_b437),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x7be5_c84d_229d_8d71),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x7a66_2cd4_a007_adc2),
];

/// `(seed, mode)` -> FNV-1a of the per-tick gameplay trace (tick, hp, coins,
/// enemy ids/hp/positions and the event queue; no serialized format) recorded
/// on the v4 build (commit c6f518d) before the ordnance arrays became kind
/// maps. The save-v5 and save-v6 re-blesses of `GOLDEN` are format-only: this trace proves
/// the gameplay outcomes of the same six runs did not move. W1-69 (user
/// decision, enemies no longer queue behind each other) re-blessed three of
/// them; see the re-bless log. Wave 2 (W2-49) moved none of them, nor did
/// waves 3 and 4 up to W4-08. The closing re-bless folded W4-14 (a later
/// wave-4 row) into the three Npc runs, class G.
const GAMEPLAY_TRACE: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0xa6ca_a6de_7fd9_2b1a),
    (1, Mode::Npc, 0x77ce_c201_9601_4f8d),
    (0x1A57_1167, Mode::Personal, 0x5cc8_bde4_faf0_9763),
    (0x1A57_1167, Mode::Npc, 0x1fb8_dfb5_f912_48c7),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x46a2_adc6_720f_8678),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x6fed_c1ce_190a_b8e3),
];

/// The ruleset `GAMEPLAY_TRACE` was recorded under. It follows
/// `RULESET_VERSION` in the commits that bump it (ruling P4).
///
/// - 2 (W1-20, strict `SellItem`): the traced runs never sell, so every
///   `GAMEPLAY_TRACE` digest is unchanged; only the label moves.
/// - 3 (W2-49, the wave-2 re-bless): the bump names W1-69 (no lane queue,
///   R15), whose three `GAMEPLAY_TRACE` moves were re-blessed in wave 1, and
///   the wave-2 rules, which move none of the six traces: the traced runs are
///   rank 0 with no country (no elites, no boss phases, no front ramp, no
///   difficulty band, no new archetype) and the weighted drop pick still
///   makes one draw. Only the label moves.
/// - 4 (W4-08, the wave-3/4 re-bless): the bump names the wave-2 review rule
///   fixes after W2-49 and the wave-4 rules (boss-rank multipliers, the
///   effect retunes and the trap charge, the repriced tree, the expedition
///   threat rank), which move none of the six traces: the traced runs are
///   one rank-0 deploy with no country, no camp purchase and no expedition
///   (the W4-08 table in the file header). Only the label moves.
/// - 5 (the closing re-bless): the bump names W4-14 aim, W4-14 arrival and
///   W4-15 price. W4-14 moves the three Npc traces, folded in the same
///   re-bless (the closing rows of the re-bless log); W4-15 moves none of
///   the six (the traced runs buy no node), and the three Personal traces
///   are unchanged.
const TRACE_RULESET: u32 = 5;

/// Provisional `(GOLDEN, GAMEPLAY_TRACE)` digests, in `GOLDEN` order, set by
/// a digest-moving commit until the wave's re-bless folds them (ruling P3).
/// None since the closing re-bless, which folded W4-14 aim and W4-14
/// arrival into the three Npc runs (their reasons are in the re-bless log).
const PROVISIONAL: Option<([u64; 6], [u64; 6])> = None;

/// Why `PROVISIONAL` moved: `(commit tag, class, reason)`, one per moving
/// commit. Empty exactly when `PROVISIONAL` is None.
const PROVISIONAL_CAUSES: &[(&str, MoveClass, &str)] = &[];

/// The seeds of the progressed runs.
const PROGRESSED_SEEDS: [u64; 2] = [1, 0xDEAD_BEEF_2026];

/// `(seed, run)` -> `Game::state_digest()` after [`PROGRESSED_TICKS`] ticks
/// of the smoke bot on `support::progressed_determinism_game`. Cut at the
/// wave-4 close (W4-09), on the W4-08 ruleset 4. The closing re-bless
/// folded W4-14 (the two takeovers) and W4-15 price (all four), class G.
const PROGRESSED_GOLDEN: [(u64, ProgressedRun, u64); 4] = [
    (1, ProgressedRun::HardCapstone, 0xab72_76a7_4131_2f38),
    (1, ProgressedRun::RecruitTakeover, 0x6ffd_2abd_5ec5_59b7),
    (
        0xDEAD_BEEF_2026,
        ProgressedRun::HardCapstone,
        0x5278_2b9f_feef_7166,
    ),
    (
        0xDEAD_BEEF_2026,
        ProgressedRun::RecruitTakeover,
        0xa040_f93d_e565_91c4,
    ),
];

/// `(seed, run)` -> FNV-1a of the per-tick gameplay trace of the same runs
/// ([`progressed_trace_line`]: the `GAMEPLAY_TRACE` line while the attempt
/// lives, then the settled report and campaign). Cut at W4-09; the closing
/// re-bless folded W4-14 and W4-15 price, as [`PROGRESSED_GOLDEN`].
const PROGRESSED_TRACE: [(u64, ProgressedRun, u64); 4] = [
    (1, ProgressedRun::HardCapstone, 0x7aef_2a42_98a6_356a),
    (1, ProgressedRun::RecruitTakeover, 0x8986_9eb5_4e0b_06ea),
    (
        0xDEAD_BEEF_2026,
        ProgressedRun::HardCapstone,
        0xf483_cce1_2e82_2eaa,
    ),
    (
        0xDEAD_BEEF_2026,
        ProgressedRun::RecruitTakeover,
        0x9e1e_5d57_ab94_5f10,
    ),
];

/// Provisional `(PROGRESSED_GOLDEN, PROGRESSED_TRACE)` digests, in
/// `PROGRESSED_GOLDEN` order (ruling P3, as `PROVISIONAL`). None since the
/// closing re-bless, which folded W4-14 aim, W4-14 arrival and W4-15 price
/// (the progressed re-bless log in the file header).
const PROGRESSED_PROVISIONAL: Option<([u64; 4], [u64; 4])> = None;

/// Why `PROGRESSED_PROVISIONAL` moved: `(commit tag, class, reason)`.
const PROGRESSED_PROVISIONAL_CAUSES: &[(&str, MoveClass, &str)] = &[];

/// The archetypes wave 2 appended (W2-31: six and the splitter's child).
/// They enter at wave tier 5, so no `GOLDEN` run meets one.
const NEW_ARCHETYPES: [&str; 7] = [
    "exploder",
    "splitter",
    "splitter-brood",
    "spitter",
    "shield-bearer",
    "sprinter",
    "juggernaut",
];

/// The ledger rules: provisional digests carry causes, every cause names its
/// commit and reason, and causes never outlive the provisional pin. `tables`
/// are the family's pinned `(GOLDEN, TRACE)` digests; a provisional pin equal
/// to them moves nothing.
fn check_ledger<const N: usize>(
    provisional: Option<([u64; N], [u64; N])>,
    causes: &[(&str, MoveClass, &str)],
    tables: ([u64; N], [u64; N]),
) -> Result<(), String> {
    for (tag, class, reason) in causes {
        if tag.trim().is_empty() || reason.trim().is_empty() {
            return Err(format!(
                "PROVISIONAL_CAUSES entry {tag:?} {class:?} {reason:?} needs a commit tag and a reason"
            ));
        }
    }
    match provisional {
        None if causes.is_empty() => Ok(()),
        None => Err(
            "PROVISIONAL is None but PROVISIONAL_CAUSES is not empty: the re-bless that \
             folded the pins moves the causes into the re-bless log"
                .into(),
        ),
        Some(_) if causes.is_empty() => Err(
            "PROVISIONAL is set without a cause: append (commit tag, class, reason) to \
             PROVISIONAL_CAUSES"
                .into(),
        ),
        Some(pins) if pins == tables => {
            Err("PROVISIONAL equals the pinned tables: reset it to None".into())
        }
        Some(_) => Ok(()),
    }
}

/// The `(GOLDEN, GAMEPLAY_TRACE)` digests of the six smoke runs.
fn golden_tables() -> ([u64; 6], [u64; 6]) {
    (GOLDEN.map(|(_, _, d)| d), GAMEPLAY_TRACE.map(|(_, _, d)| d))
}

/// The `(PROGRESSED_GOLDEN, PROGRESSED_TRACE)` digests of the four
/// progressed runs.
fn progressed_tables() -> ([u64; 4], [u64; 4]) {
    (
        PROGRESSED_GOLDEN.map(|(_, _, d)| d),
        PROGRESSED_TRACE.map(|(_, _, d)| d),
    )
}

/// The digests the tests pin: `PROVISIONAL` when set, the tables otherwise.
fn pinned<T: Copy, const N: usize>(
    table: &[(u64, T, u64); N],
    provisional: Option<[u64; N]>,
) -> [(u64, T, u64); N] {
    let mut pins = *table;
    if let Some(digests) = provisional {
        for (pin, digest) in pins.iter_mut().zip(digests) {
            pin.2 = digest;
        }
    }
    pins
}

fn run(seed: u64, mode: Mode) -> Game {
    smoke_run(seed, mode, TICKS, |_| {})
}

/// One tick of the gameplay trace: tick, hp, coins, enemy ids/hp/positions
/// and the event queue.
fn trace_line(g: &Game) -> String {
    let a = g.attempt().unwrap();
    format!(
        "t{} hp{} coins{} e{:?} ev{:?}\n",
        a.tick,
        a.hp,
        a.coins,
        a.enemies
            .iter()
            .map(|e| (e.id, e.hp, e.x.to_bits(), e.y().to_bits()))
            .collect::<Vec<_>>(),
        a.events
    )
}

/// The smoke run of `seed` and `mode` with its gameplay trace hash and
/// outcome summary, from one pass.
fn observed_run(seed: u64, mode: Mode) -> (Game, u64, OutcomeSummary) {
    let mut hash = support::fnv1a(b"");
    let mut summary = OutcomeSummary::default();
    let g = smoke_run(seed, mode, TICKS, |g| {
        hash = support::fnv1a_extend(hash, trace_line(g).as_bytes());
        summary.observe(g);
    });
    (g, hash, summary)
}

/// One tick of a progressed run's trace: [`trace_line`] while the attempt
/// lives; once it has settled, the report's outcome, kills and coins and the
/// campaign's victories, coins and liberated countries.
fn progressed_trace_line(g: &Game) -> String {
    if g.attempt().is_some() {
        return trace_line(g);
    }
    let r = g
        .last_result()
        .expect("a settled progressed run has a report");
    let c = g.campaign();
    format!(
        "settled {:?} kills{} coins{} | victories{} coins{} cleared{:?}\n",
        r.outcome, r.kills, r.coins, c.victories, c.coins, c.world.cleared
    )
}

/// The telemetry of a progressed run: its live attempt's, or its report's
/// once the attempt settled.
fn progressed_telemetry(g: &Game) -> &CombatTelemetry {
    g.attempt()
        .map(|a| &a.telemetry)
        .or_else(|| g.last_result().map(|r| &r.telemetry))
        .expect("a progressed run has an attempt or a report")
}

/// The progressed run's end: outcome (None while live), kills, spawned,
/// elites, boss phase ticks and new-archetype kills. Its FNV is the
/// anatomy's outcome hash.
fn progressed_outcome(g: &Game) -> String {
    let t = progressed_telemetry(g);
    let new: Vec<_> = t
        .kills_by_archetype
        .iter()
        .filter(|(id, _)| NEW_ARCHETYPES.contains(&id.as_str()))
        .collect();
    format!(
        "outcome {:?} spawned {} elites {}/{} phases {:?} new kills {new:?}",
        g.attempt()
            .is_none()
            .then(|| g.last_result().map(|r| r.outcome))
            .flatten(),
        t.spawned,
        t.elites_killed,
        t.elites_spawned,
        t.boss_phase_ticks
    )
}

/// The progressed run of `seed` and `run` with its trace hash, from one pass.
fn observed_progressed_run(seed: u64, run: ProgressedRun) -> (Game, u64) {
    let mut hash = support::fnv1a(b"");
    let g = progressed_run(seed, run, |g| {
        hash = support::fnv1a_extend(hash, progressed_trace_line(g).as_bytes());
    });
    (g, hash)
}

/// What a progressed run must have exercised: an elite spawned, a boss
/// phase entered and a new archetype killed.
fn progressed_exercise(g: &Game) -> Result<(), String> {
    let t = progressed_telemetry(g);
    let mut missing = Vec::new();
    if t.elites_spawned == 0 {
        missing.push("no elite spawned");
    }
    if t.boss_phase_ticks.is_empty() {
        missing.push("no boss phase entered");
    }
    if !t
        .kills_by_archetype
        .iter()
        .any(|(id, kills)| *kills > 0 && NEW_ARCHETYPES.contains(&id.as_str()))
    {
        missing.push("no new archetype killed");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join(", "))
    }
}

/// A saved game's digest after a save, decode and validation round trip.
fn reloaded_digest(g: &Game) -> u64 {
    let payload: serde_json::Value =
        serde_json::from_str(&g.to_save_json().expect("the game saves")).unwrap();
    Game::from_payload(payload, SAVE_VERSION)
        .expect("the saved game decodes and validates")
        .state_digest()
}

/// Pin (W1-13, R2): the mode owner's seat is never written, so the default
/// game and every GOLDEN run (live, then finished by a withdrawal) carry no
/// `"defender"` key on the attempt or the report and GOLDEN cannot move for it.
#[test]
fn golden_and_default_games_serialize_no_defender_key() {
    fn keyless(g: &Game, slot: &str, what: &str) {
        let value = serde_json::to_value(g).unwrap();
        let object = value[slot]
            .as_object()
            .unwrap_or_else(|| panic!("{what}: {slot} is an object"));
        assert!(
            !object.contains_key("defender"),
            "{what}: {slot} wrote a mode-owner defender key"
        );
    }
    let default = serde_json::to_value(Game::default()).unwrap();
    assert!(default["attempt"].is_null() && default["last_result"].is_null());
    for (seed, mode, _) in GOLDEN {
        let mut g = run(seed, mode);
        let what = format!("seed {seed:#x} {mode:?}");
        keyless(&g, "attempt", &what);
        g.apply(&Command::Withdraw).unwrap();
        keyless(&g, "last_result", &what);
    }
}

#[test]
fn seeded_headless_runs_keep_their_pinned_gameplay_traces() {
    let mut mismatches = Vec::new();
    for (seed, mode, expected) in pinned(&GAMEPLAY_TRACE, PROVISIONAL.map(|p| p.1)) {
        let (_, hash, summary) = observed_run(seed, mode);
        println!("({seed:#x}, Mode::{mode:?}, {hash:#018x}),");
        println!("// {seed:#x} {mode:?}: {summary}");
        if hash != expected {
            mismatches.push(format!(
                "seed {seed:#x} {mode:?} gameplay moved: expected {expected:#018x}, \
                 got {hash:#018x}\n  outcome now: {summary}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Gameplay diverged from the pinned traces:\n{}\n\n\
         If this change is intentional, set PROVISIONAL and append its cause to \
         PROVISIONAL_CAUSES in tests/determinism.rs (see the file header).",
        mismatches.join("\n")
    );
}

#[test]
fn seeded_headless_runs_reach_the_pinned_state_digests() {
    let mut mismatches = Vec::new();
    for (seed, mode, expected) in pinned(&GOLDEN, PROVISIONAL.map(|p| p.0)) {
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
         If this change is intentional, set PROVISIONAL and append its cause to \
         PROVISIONAL_CAUSES in tests/determinism.rs (see the file header). \
         Rebaselining gameplay requires explicit approval.",
        mismatches.join("\n")
    );
}

#[test]
fn progressed_runs_reach_the_pinned_state_digests_and_traces() {
    let golden = pinned(&PROGRESSED_GOLDEN, PROGRESSED_PROVISIONAL.map(|p| p.0));
    let trace = pinned(&PROGRESSED_TRACE, PROGRESSED_PROVISIONAL.map(|p| p.1));
    let mut mismatches = Vec::new();
    for ((seed, run, expected), (_, _, expected_trace)) in golden.into_iter().zip(trace) {
        let (g, hash) = observed_progressed_run(seed, run);
        let digest = g.state_digest();
        println!("golden ({seed:#x}, ProgressedRun::{run:?}, {digest:#018x}),");
        println!("trace  ({seed:#x}, ProgressedRun::{run:?}, {hash:#018x}),");
        println!("// {seed:#x} {run:?}: {}", progressed_outcome(&g));
        if digest != expected {
            mismatches.push(format!(
                "seed {seed:#x} {run:?} state: expected {expected:#018x}, got {digest:#018x}"
            ));
        }
        if hash != expected_trace {
            mismatches.push(format!(
                "seed {seed:#x} {run:?} gameplay: expected {expected_trace:#018x}, got {hash:#018x}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "The progressed runs diverged from their pins:\n{}\n\n\
         If this change is intentional, set PROGRESSED_PROVISIONAL and append its cause \
         to PROGRESSED_PROVISIONAL_CAUSES in tests/determinism.rs (see the file header).",
        mismatches.join("\n")
    );
}

/// W4-09 (design-save-determinism section 9): every progressed run starts
/// from the campaign it names, a valid save, and its window spawns an elite,
/// enters a boss phase and kills a new archetype; the four end states differ
/// and save validly. Perturbation: the same campaigns at rank 2 (below the
/// elite rank 3 and wave tier 5) fail the check, so it is not vacuous: both
/// report no elite and no new archetype. Only the takeover (a Sector boss,
/// below the rank-4 phases) also reports no boss phase, which proves the
/// phase clause; the Hard Capstone keeps its Capstone transition at any
/// rank, so it enters a phase at rank 2 too.
#[test]
fn progressed_golden_runs_exercise_elites_boss_phases_and_new_archetypes() {
    let mut digests = std::collections::BTreeSet::new();
    for (seed, run, _) in PROGRESSED_GOLDEN {
        let what = format!("seed {seed:#x} {run:?}");
        let start = support::progressed_determinism_game(seed, run);
        let a = start.attempt().expect("the progressed defense is live");
        assert_eq!(a.mode, run.mode(), "{what}");
        assert_eq!(u64::from(a.progression.rank), PROGRESSED_RANK, "{what}");
        assert!(!a.boss_spawned && !a.paused, "{what}");
        assert!(!a.structures.is_empty(), "{what}: the defender built");
        match run {
            ProgressedRun::HardCapstone => {
                assert_eq!(a.defender(), DefenderId::Player, "{what}");
                assert_eq!(a.progression.boss_rank, BossRank::Capstone, "{what}");
                assert_eq!(
                    start.campaign().balance_settings.preset,
                    horde_core::balance::BalancePreset::Hard,
                    "{what}"
                );
            }
            ProgressedRun::RecruitTakeover => {
                assert!(
                    matches!(a.defender(), DefenderId::Recruit(_)),
                    "{what}: a recruit holds the main run"
                );
                assert_eq!(a.progression.boss_rank, BossRank::Sector, "{what}");
            }
        }
        assert_eq!(
            reloaded_digest(&start),
            start.state_digest(),
            "{what}: the staged start is a valid save"
        );
        let (end, _) = observed_progressed_run(seed, run);
        progressed_exercise(&end).unwrap_or_else(|missing| panic!("{what}: {missing}"));
        assert_eq!(
            reloaded_digest(&end),
            end.state_digest(),
            "{what}: the end state is a valid save"
        );
        digests.insert(end.state_digest());
    }
    assert_eq!(digests.len(), 4, "every progressed run has its own state");

    for seed in PROGRESSED_SEEDS {
        for run in ProgressedRun::ALL {
            let early = support::smoke_drive(
                support::progressed_determinism_game_at(seed, run, 2),
                PROGRESSED_TICKS,
                |_| {},
            );
            let missing = progressed_exercise(&early).expect_err(&format!(
                "seed {seed:#x} {run:?} at rank 2 exercises everything: the check is vacuous"
            ));
            for clause in ["no elite spawned", "no new archetype killed"] {
                assert!(
                    missing.contains(clause),
                    "seed {seed:#x} {run:?} at rank 2: {clause:?} not reported, got {missing:?}"
                );
            }
            // The Capstone's transition is folded at any rank, so only the
            // Sector-boss takeover (no phases below rank 4) proves the phase
            // clause; the Capstone proves it can also pass.
            assert_eq!(
                missing.contains("no boss phase entered"),
                run == ProgressedRun::RecruitTakeover,
                "seed {seed:#x} {run:?} at rank 2: got {missing:?}"
            );
        }
    }
}

#[test]
fn gameplay_trace_is_recorded_under_the_current_ruleset() {
    assert_eq!(
        TRACE_RULESET, RULESET_VERSION,
        "RULESET_VERSION moved: state what the bump does to GAMEPLAY_TRACE and \
         move TRACE_RULESET in the same commit"
    );
}

#[test]
fn provisional_digests_name_their_causes() {
    check_ledger(PROVISIONAL, PROVISIONAL_CAUSES, golden_tables()).unwrap();
    check_ledger(
        PROGRESSED_PROVISIONAL,
        PROGRESSED_PROVISIONAL_CAUSES,
        progressed_tables(),
    )
    .unwrap();

    let tables = golden_tables();
    let (golden, trace) = tables;
    let mut moved = golden;
    moved[0] ^= 1;
    let cause = [("W9-99", MoveClass::F, "a planted format move")];
    assert!(check_ledger::<6>(None, &[], tables).is_ok());
    assert!(check_ledger(Some((moved, trace)), &cause, tables).is_ok());
    assert!(
        check_ledger(Some((moved, trace)), &[], tables).is_err(),
        "provisional digests without a cause"
    );
    assert!(
        check_ledger::<6>(None, &cause, tables).is_err(),
        "a cause left behind after the re-bless"
    );
    assert!(
        check_ledger(Some((golden, trace)), &cause, tables).is_err(),
        "a provisional pin that moves nothing"
    );
    for blank in [("", MoveClass::F, "a reason"), ("W9-99", MoveClass::F, " ")] {
        assert!(
            check_ledger(Some((moved, trace)), &[blank], tables).is_err(),
            "a cause without a commit tag or reason: {blank:?}"
        );
    }
    // The progressed family keeps the same rules against its own tables.
    let progressed = progressed_tables();
    let mut moved = progressed.1;
    moved[3] ^= 1;
    assert!(check_ledger(Some((progressed.0, moved)), &cause, progressed).is_ok());
    assert!(check_ledger(Some((progressed.0, moved)), &[], progressed).is_err());
    assert!(check_ledger(Some(progressed), &cause, progressed).is_err());
    assert!(check_ledger::<4>(None, &cause, progressed).is_err());
}

/// A small payload with the real shape: campaign items, a receipt list and an
/// attempt with telemetry.
fn planted_payload() -> serde_json::Value {
    serde_json::json!({
        "campaign": {
            "coins": 10,
            "items": [
                {"id": 1, "definition": "pistol", "quality": null, "level": 1, "locked": false},
                {"id": 2, "definition": "bat", "quality": "Rare", "level": 2, "locked": true},
            ],
            "run_history": [{"id": 1, "kills": 3}],
            "first_clear_claimed": {"level-1": 1},
        },
        "attempt": {
            "hp": 30,
            "telemetry": {
                "misses": 2,
                "kills_by_kind": {"Shambler": 3},
                "spawned_by_archetype": {"shambler": 3},
                "kills_by_archetype": {"shambler": 3},
                "damage_taken_by_archetype": {"shambler": 12},
            },
        },
    })
}

fn planted(payload: &serde_json::Value, trace: u64, outcome: u64) -> Anatomy {
    let golden = support::fnv1a(payload.to_string().as_bytes());
    Anatomy::of(payload, golden, trace, outcome)
}

#[test]
fn anatomy_classifier_labels_planted_changes() {
    let base = planted_payload();
    let before = planted(&base, 7, 9);
    let classify = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut after = base.clone();
        edit(&mut after);
        support::classify(&before, &planted(&after, 7, 9))
    };

    assert_eq!(support::classify(&before, &before), None);
    assert_eq!(
        classify(&|p| p["campaign"]["new_counter"] = 0.into()),
        Some(MoveClass::F),
        "an extra key is a format move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["coins"] = 11.into()),
        Some(MoveClass::G),
        "a value-only state move is never format; it is conservatively gameplay"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["first_clear_claimed"]["level-2"] = 1.into()),
        Some(MoveClass::G),
        "a new entry in a data-keyed map is a state move, not a format move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["items"][0]["definition"] = "rifle".into()),
        Some(MoveClass::C),
        "a changed item definition is a content move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["items"][1]["level"] = 3.into()),
        Some(MoveClass::C),
        "a changed item level is a content move"
    );
    assert_eq!(
        classify(&|p| p["attempt"]["telemetry"]["misses"] = 3.into()),
        Some(MoveClass::T),
        "a changed telemetry subtree is a telemetry move"
    );
    assert_eq!(
        classify(&|p| p["attempt"]["telemetry"]["trap_damage"] = 0.into()),
        Some(MoveClass::T),
        "a new telemetry field stays inside the telemetry subtree"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["run_history"][0]["kills"] = 4.into()),
        Some(MoveClass::T),
        "a changed receipt is a telemetry move"
    );
    for after in [planted(&base, 8, 9), planted(&base, 7, 10)] {
        assert_eq!(
            support::classify(&before, &after),
            Some(MoveClass::G),
            "a moved trace or outcome is a gameplay move"
        );
    }
    let mut both = base.clone();
    both["campaign"]["new_counter"] = 0.into();
    assert_eq!(
        support::classify(&before, &planted(&both, 7, 10)),
        Some(MoveClass::G),
        "gameplay outranks every other class"
    );

    // Array indices are stripped: item count does not move the key paths.
    let mut more = base.clone();
    more["campaign"]["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id": 3, "definition": "axe", "quality": null, "level": 1, "locked": false}));
    assert_eq!(
        support::json_key_paths_fnv(&base),
        support::json_key_paths_fnv(&more)
    );
    assert_ne!(support::items_fnv(&base), support::items_fnv(&more));

    // Data-keyed map entries are data, not shape: a newly claimed level ID
    // does not move the key paths.
    let mut claimed = base.clone();
    claimed["campaign"]["first_clear_claimed"]["level-2"] = 2.into();
    assert_eq!(
        support::json_key_paths_fnv(&base),
        support::json_key_paths_fnv(&claimed)
    );
    assert!(support::json_key_paths(&base).contains("/campaign/first_clear_claimed/*"));

    // The per-archetype telemetry maps (W1-27) are keyed by archetype ID: a
    // newly seen archetype does not move the key paths either.
    for map in [
        "spawned_by_archetype",
        "kills_by_archetype",
        "damage_taken_by_archetype",
    ] {
        let mut seen = base.clone();
        seen["attempt"]["telemetry"][map]["spitter"] = 1.into();
        assert_eq!(
            support::json_key_paths_fnv(&base),
            support::json_key_paths_fnv(&seen),
            "a new archetype ID in {map} is data, not shape"
        );
        assert!(support::json_key_paths(&base).contains(&format!("/attempt/telemetry/{map}/*")));
    }
}

#[test]
fn anatomy_reads_the_real_payload_shape() {
    let mut g = support::game(Mode::Personal);
    // The per-archetype telemetry maps are absent while empty; one entry each
    // puts their real serialized names in the payload.
    let mut a = g.attempt_slot_mut().take().unwrap();
    a.telemetry
        .spawned_by_archetype
        .insert("shambler".into(), 1);
    a.telemetry.kills_by_archetype.insert("shambler".into(), 1);
    a.telemetry
        .damage_taken_by_archetype
        .insert("shambler".into(), 4);
    *g.attempt_slot_mut() = Some(a);
    let payload = serde_json::to_value(&g).unwrap();
    let paths = support::json_key_paths(&payload);
    for path in [
        "/campaign/items",
        "/campaign/items/definition",
        "/campaign/items/quality",
        "/campaign/items/level",
        "/campaign/run_history",
        "/attempt/telemetry",
        "/attempt/dps",
        "/attempt/loot_events",
    ] {
        assert!(paths.contains(path), "the save payload has no {path}");
    }
    // Every data-keyed map the key paths collapse is a real payload field, so
    // a rename cannot silently turn its entries back into shape.
    for name in support::DATA_KEYED_MAPS {
        let suffix = format!("/{name}");
        assert!(
            paths.iter().any(|p| p.ends_with(&suffix)),
            "the save payload has no data-keyed map {name}"
        );
    }
    let core = support::json_key_paths(&support::without_telemetry(&payload));
    assert!(!core.iter().any(|p| p.contains("/telemetry")));
    let mut no_items = payload.clone();
    no_items["campaign"]["items"] = serde_json::json!([]);
    assert_ne!(support::items_fnv(&payload), support::items_fnv(&no_items));
}

#[test]
#[ignore = "prints the determinism anatomy of every pinned run"]
fn determinism_anatomy() {
    for (seed, mode, _) in GOLDEN {
        let (g, trace, summary) = observed_run(seed, mode);
        let payload = serde_json::to_value(&g).unwrap();
        let anatomy = Anatomy::of(&payload, g.state_digest(), trace, summary.digest());
        println!("({seed:#x}, Mode::{mode:?}) {anatomy}");
        println!("// {summary}");
    }
    for (seed, run, _) in PROGRESSED_GOLDEN {
        let (g, trace) = observed_progressed_run(seed, run);
        let payload = serde_json::to_value(&g).unwrap();
        let outcome = progressed_outcome(&g);
        let anatomy = Anatomy::of(
            &payload,
            g.state_digest(),
            trace,
            support::fnv1a(outcome.as_bytes()),
        );
        println!("({seed:#x}, ProgressedRun::{run:?}) {anatomy}");
        println!("// {outcome}");
    }
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
fn outcome_summaries_sample_the_whole_run() {
    assert_eq!(
        support::OUTCOME_SAMPLE_TICKS.last().copied(),
        Some(u64::from(TICKS))
    );
    let summary = support::outcome_summary(SEEDS[0], Mode::Personal);
    let ticks: Vec<u64> = summary.samples.iter().map(|s| s.tick).collect();
    assert_eq!(ticks, support::OUTCOME_SAMPLE_TICKS);
    assert_eq!(summary, observed_run(SEEDS[0], Mode::Personal).2);
}

#[test]
fn the_digest_is_a_pure_function_of_state() {
    let g = run(SEEDS[0], Mode::Personal);
    assert_eq!(g.state_digest(), g.clone().state_digest());
    let mut moved = g.clone();
    advance(&mut moved, 1);
    assert_ne!(g.state_digest(), moved.state_digest());
}
