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
Until they are ported, package_playtest.ps1, verify_playtest.ps1 and the PowerShell 7 trace_npc.ps1 still use ./target.
Performance evidence: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_performance.ps1
-Scene all|peak|progressed-peak|stress -Profile dev|playtest [-VsyncOff] [-Pipelined default|on|off] [-Repeat 3]
[-ShippedSmoke] [-SaveClone] [-NoBuild] builds the review exe, runs every scene at both sizes and compares the
frame reports with docs/PERF_BUDGET.json; -SelfTest checks the fixtures only. Exit codes: 0 pass, 1 over budget,
2 invalid evidence (a run invalid twice, or a gated metric missing), 3 build or run failure. Acceptance (W1-61) is
dev with VSync on (interface gates) plus -VsyncOff in dev and playtest (CPU p99), each -Repeat 3; frames over
50 ms leave out driver stalls (PERF_BUDGET.json driver_stall, user ruling). Evidence lands in
<target>/perf/<stamp>-<profile>[-novsync]/summary.json and summary.md. Keep the machine quiet, the window
focused and on mains, and never run a cargo command beside it.
