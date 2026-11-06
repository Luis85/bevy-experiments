# Horde Defense repository instructions
Independent Rust 1.99.1 / Bevy 0.20 workspace. Read ../AGENTS.md.
Read docs/GDD.md and docs/EXPANSION.md; the latter records later user requirements that supersede the original single-expedition scope.
Simulation in horde_core; presentation only in horde_game. Fixed 60 Hz ticks,
stable IDs, serializable random state; no renderer-driven authoritative outcomes.
Do not bundle proprietary rule tables or rulebook files. Preserve assets/CREDITS.md,
the font OFL notice and geographic source attribution. See docs/GDD.md.
Use cargo fmt --all, cargo clippy --locked --workspace --all-targets -- -D warnings,
the same clippy with -p horde-defense --no-default-features and with --features dev,
and cargo test --locked --workspace (plus -p horde-defense --features dev) inside this directory. Each workspace owns target/ unless CARGO_TARGET_DIR is set; every script under scripts/ honours it.
Until they are ported, package_playtest.ps1 and the PowerShell 7 trace_npc.ps1 still read ./target.
