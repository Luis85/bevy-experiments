# Task 3 report: fl_protocol

Implemented in crates/fl_protocol/src/lib.rs: SIM_TICK_HZ=20, Tick (next, Display), PlayerId, TeamId, SimId (Display "#N"), CommandTypeId, UnitTypeId, FactionId, CommandKind, PlayerCommand, Controller. `#![deny(clippy::float_arithmetic)]`. Newtypes derive Copy/PartialOrd/Ord/Default; all derive Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize.

TDD: RED = `cargo test -p fl_protocol` with tests only -> 16 compile errors (types not found). GREEN = same command after implementing -> 4 passed.

Gate (full workspace, no fl_math interference): cargo fmt --check OK; clippy --workspace --all-targets -D warnings OK; cargo test --workspace OK (4 fl_protocol tests); check_deps.sh "deps OK".

Files: crates/fl_protocol/src/lib.rs only. Self-review: diff read; no floats, no Cargo edits.
Concerns: none. Tick::next uses plain `+1` (panics on overflow in debug; u32 at 20Hz is ~6.8 years).
