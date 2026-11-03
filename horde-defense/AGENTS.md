# Horde Defense repository instructions
Independent Rust 1.99.1 / Bevy 0.20 workspace. Read ../AGENTS.md.
Read docs/GDD.md and docs/EXPANSION.md; the latter records later user requirements that supersede the original single-expedition scope.
Simulation in horde_core; presentation only in horde_game. Fixed 60 Hz ticks,
stable IDs, serializable random state; no renderer-driven authoritative outcomes.
Do not bundle proprietary rule tables or rulebook files. Preserve assets/CREDITS.md,
the font OFL notice and geographic source attribution. See docs/GDD.md.
Use cargo fmt --all, cargo clippy --locked --workspace --all-targets -- -D warnings,
and cargo test --locked --workspace inside this directory. Each workspace owns target/.
