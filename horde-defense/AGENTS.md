# Horde Defense repository instructions
Independent Rust 1.99.1 / Bevy 0.20 workspace. Read ../AGENTS.md.
Read docs/GDD.md and docs/EXPANSION.md; the latter records later user requirements that supersede the original single-expedition scope.
Simulation in horde_core; presentation only in horde_game. Fixed 60 Hz ticks,
stable IDs, serializable random state; no renderer-driven authoritative outcomes.
Do not bundle proprietary rule tables or rulebook files. Preserve assets/CREDITS.md,
the font OFL notice and geographic source attribution. See docs/GDD.md.
Use cargo fmt --all, cargo clippy --locked --workspace --all-targets -- -D warnings,
the same clippy with -p horde-defense --no-default-features and with --features dev,
and cargo test --locked --workspace (plus -p horde-defense --no-default-features and --features dev) inside this directory. Each workspace owns target/ unless CARGO_TARGET_DIR is set; every script under scripts/ honours it.
Only the PowerShell 7 trace_npc.ps1 still uses ./target.
Performance evidence: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_performance.ps1
-Scene all|peak|progressed-peak|stress -Profile dev|playtest [-VsyncOff] [-Pipelined default|on|off] [-Repeat 3]
[-ShippedSmoke] [-SaveClone] [-NoBuild] [-NoWaitQuiet] [-QuietTimeoutMinutes 240] builds the review exe, runs every scene at both sizes and compares the
frame reports with docs/PERF_BUDGET.json; -SelfTest checks the fixtures only. Exit codes: 0 pass, 1 over budget,
2 invalid evidence (a run invalid twice, or a gated metric missing), 3 build or run failure. Invalid evidence outranks an over,
but every valid gated over prints "VALID OVER: <scene> <size> <metric> <value> > <gate>" and is listed in summary.json
valid_overs whatever the exit code; it stands and is never re-measured away (R34). The script holds the display on for
its whole run, and a run that overlaps a Kernel-Power 506/507 display-off event is invalid (R27). Before it measures it waits for
a quiet machine (no foreign cargo, rustc or cargo-nextest; mean CPU load under 35% over 60 s), logging what it waits for and
exiting 2 after -QuietTimeoutMinutes, and a run with a CPU load sample of 90% or more is invalid (R30).
The interface_total re-gate (R31): powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ab_interface_total.ps1
[-Pairs 5] [-NoBuild -ExecutableA <exe> [-ExecutableB <exe>]] builds A from ab03da9 (git archive into <target>/ab-w1/src) and
B from HEAD, alternates them on progressed-peak (dev, VSync on) and passes when B's median interface_total is at most
1.05 x A's per size (exit codes as measure_performance; evidence in <target>/perf/<stamp>-ab-interface-total/ab.json). Acceptance (W1-61) is
dev with VSync on (interface gates) plus -VsyncOff in dev and playtest (CPU p99), each -Repeat 3; frames over
50 ms leave out driver stalls (PERF_BUDGET.json driver_stall, user ruling). Evidence lands in
<target>/perf/<stamp>-<profile>[-novsync]/summary.json and summary.md. Keep the machine quiet, the window
focused and on mains, and never run a cargo command beside it.
Pacing acceptance (W2-03, W4-11): cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture,
run after the debug builds. Every test in crates/horde_core/tests/pacing_targets.rs is #[ignore = "release-only"];
one OnceLock sweep (expert-deu, Normal, 20 seeds, 60 runs) feeds the goal-5 tests and most goal-6 tests, and a
second (lux-easy, lux-normal, lux-hard, start-aut, start-bgd; 20 seeds, at most 20 runs) feeds the LUX and
start-country tests (W2-04). goal6_defeat_rate_windows and goal6_no_15_streak_after_run_10 assert on goal6-novice
(W4-13, R56: 20 seeds, 60 runs) and print the expert-deu verdicts as information. A third (strategy,
neighbours-solo, neighbours-parallel; 20 seeds) feeds the goal-3 tests (W2-43). The auto-sell gate (W4-l, G4.13)
sweeps auto-sell (20 seeds, 60 runs) against the first sweep, and stash_growth_reported_for_150_runs sweeps it
again at 150 runs (about 6 minutes; name a test to run only its sweeps).
Each prints a PASS or FAIL summary; a goal5 or goal6 filter runs only those rows and their sweeps.
docs/BALANCE_AND_RUN_STATS.md "Goal 5" and "Goal 6" define the rows, the tier definition (milestone-gated nodes
left out) and the milestones (TreeExhausted, OperationsOpen), with their numbers at the wave-4 close: the goal-5
rows pass, G6.11 and G6.12 are red, and of the goal-3 rows G3.11 is red (plan section 9.6). Once the release build exists,
the first two sweeps take about a minute and the strategy sweep several more. The evaluators' synthetic tests
(tests/pacing_harness.rs, tests/strategy_pacing.rs) run in the normal suite. The goal-3 table:
cargo run --locked --release -p horde_core --example pacing -- --strategy --seeds 20 --out PATH.
