# Task 6 report: `fl_protocol` lockstep envelope, payloads, `MatchHeader`, string tables

**Status:** DONE
**Commit:** `32ec154 feat(fl_protocol)!: lockstep envelope, CommandPayload, core and debug payloads, MatchHeader, StringTables`
(trailers `Refs: ADR-0007, ADR-0004, ADR-0008` and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`)

## What I implemented

`crates/fl_protocol/src/lib.rs` is split into these modules, and `lib.rs` re-exports everything at the crate root. `#![deny(clippy::float_arithmetic)]` is kept (R24).
- `ids.rs`: `SIM_TICK_HZ`, `Tick`, `PlayerId`, `TeamId`, `SimId`, `CommandTypeId`, `UnitTypeId`, `FactionId`, unchanged from rev 1.
- `command.rs`:
  - `PlayerCommand { exec_tick, player, seq, units, kind, queue }` (renamed `tick` → `exec_tick`, added `seq: u32`).
  - `CommandKind` is unchanged.
  - New `CommandPayload` trait with `NAME`, `NEEDS_UNITS = true` and `DEBUG_ONLY = false`.
- `payloads.rs`: `MoveCmd`, `StopCmd`, `DebugSpawnCmd { unit: String, owner, pos, heading }`, `DebugWanderCmd`, `DebugSetOwnerCmd`, `DebugInvulnerableCmd`, `DebugSetHpCmd`, `DebugTeleportCmd`.
  - Every payload has `#[serde(deny_unknown_fields)]` and the name the brief gives.
  - Every `debug.*` payload is `DEBUG_ONLY`, and `DebugSpawnCmd` has `NEEDS_UNITS = false`.
  - The content id is a name (`unit: String`).
- `tables.rs` (`pub mod tables`): the 11 table-name consts and `StringTables { tables: BTreeMap<String, Vec<String>> }` with `insert`, `get`, `id_of` and `remap`.
  - `remap` builds a name→id map of `current` once. When a name is duplicated, the first occurrence wins, which matches `id_of`.
  - `remap` returns an empty Vec when `self` has no such table.
- `header.rs`:
  - `FORMAT_VERSION = 1` and `MatchHeader` (all fields from the brief).
  - `HeaderMismatch`, with a hand-written `Display` and `std::error::Error`.
  - `#[non_exhaustive] Controller`.
  - `check_replay` checks format, features, mods, sim_version and rules_hash. A `build_id` difference is returned as one warning string. Seed, cheats and tables come from the replay and are not compared.
  - `check_handshake` checks every field, in enum order.
  - `check_save` checks `saved.format_version <= ours`, then features, then mods.
- The derives are `Debug, Clone, PartialEq, Serialize, Deserialize` everywhere. The rev-1 `Eq`/`Hash` derives are kept, and `Eq` is added on headers and payloads.

Mechanical `fl_sim` call sites (listed in the brief's Files):
- `src/commands.rs`: `cmd.tick` → `cmd.exec_tick`. The unit-test helper `cmd(tick, seq)` assigns seq 0..4 plus 5 to the late command.
- `tests/commands.rs`: the `command`/`move_to` helpers take a `seq`. Each test numbers its seqs 0, 1, 2, … per player.
- `tests/movement.rs`: `order_move` uses `exec_tick` and `seq: 0`, because each test calls it once.
- There is no behaviour change. `fl_sim` keeps its own rev-1 `CommandPayload` trait and payloads. No other crate uses `PlayerCommand`, and the only uses of `Controller` outside fl_protocol are constructions, so `non_exhaustive` breaks nothing.

## Tests

`crates/fl_protocol/tests/protocol.rs` contains all 11 tests the brief lists, plus a few extra checks:
- a positive RON parse
- a reverse `remap` with a `None`
- `check_save` accepting an older format with a different build_id and rejecting a newer format
- an exact-equality check on `DebugSpawnCmd`

The existing unit tests moved to their modules: `tick_ordering` and `sim_id_display` in ids.rs, `player_command_postcard_roundtrip` in command.rs (updated to `exec_tick` 7 and `seq` 3), and `controller_ron_names` in header.rs. A new unit test, `header_mismatch_display`, is also in header.rs.

### TDD evidence
- RED: `cargo nextest run --profile agent -p fl_protocol` →
  ```
  error[E0432]: unresolved imports `fl_protocol::CommandPayload`, `fl_protocol::DebugInvulnerableCmd`, ... `fl_protocol::StringTables`, `fl_protocol::tables`
  error[E0560]: struct `PlayerCommand` has no field named `exec_tick`
  error[E0560]: struct `PlayerCommand` has no field named `seq`
  error: could not compile `fl_protocol` (test "protocol") due to 5 previous errors
  ```
  This was expected: none of the new API existed yet.
- GREEN: `cargo nextest run --profile agent -p fl_protocol` → `16 tests run: 16 passed, 0 skipped`.
- After the fl_sim call sites: `cargo nextest run --profile agent -p fl_protocol -p fl_sim` → `70 tests run: 70 passed, 0 skipped`.

### Gate
Command: `cargo xtask gate task --track contract --track sim-core`. That is the brief's Step 5. sim-core is needed because the brief's Files list includes the fl_sim call sites.
```
PASS fmt (1.7s)
PASS clippy-drift (3.0s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (0.9s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_protocol/src/lib.rs:97 RemovedTest: #[cfg(test)]
    TEST-GUARD crates/fl_sim/src/commands.rs:450 RemovedAssert: assert_eq!(queue.push(cmd(0), Tick(1)), Err(RejectReason::Late));
PASS clippy (16.1s)
PASS arch-check (10.9s)
PASS test-headless (24.2s)
SKIP test-client: no client package touched since 45415d9 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 58s
```
The first gate run failed clippy with `assertions_on_constants` on the `assert!(X::DEBUG_ONLY)` lines. `debug_payloads_are_debug_only` now compares arrays with `assert_eq!` instead, and the coverage is the same.

### Test-guard findings (both are moves or mechanical rewrites, not weakening)
1. `fl_protocol/src/lib.rs:97 RemovedTest`: the `#[cfg(test)] mod tests` block of the old monolithic lib.rs. All four tests still exist, moved to `ids.rs`, `command.rs` and `header.rs`, and the roundtrip is also in `tests/protocol.rs`.
2. `fl_sim/src/commands.rs:450 RemovedAssert`: the assert is now `assert_eq!(queue.push(cmd(0, 5), Tick(1)), Err(RejectReason::Late));`. The only change is the extra `seq` argument.

## Files changed
- `crates/fl_protocol/src/{lib,ids,command,payloads,header,tables}.rs`
- `crates/fl_protocol/tests/protocol.rs` (new)
- `crates/fl_sim/src/commands.rs`, `crates/fl_sim/tests/commands.rs`, `crates/fl_sim/tests/movement.rs` (mechanical)

All files are LF (`git ls-files --eol` shows `w/lf`). `Cargo.toml`/`Cargo.lock` are not touched; fl_protocol already had the `fl_math`, `serde`, `postcard` and `ron` deps.

## Self-review / concerns
- The dispatch said to gate with `--track contract`. I ran the brief's `--track contract --track sim-core`, because the commit touches fl_sim call sites that the brief lists. I did not run contract alone; check-scope would probably flag the fl_sim files under it.
- The wording of the `check_replay` warning is my own (`replay recorded with build "...", playing back with build "..."`). The brief fixes only its count.
- I chose the field order for mismatch precedence. `check_replay` follows the brief's list (format, features, mods, sim_version, rules_hash). `check_handshake` follows enum order. `check_save` checks format first.
- `HeaderMismatch` also derives `Copy, Eq, Hash` and implements `std::error::Error`, beyond the brief's minimum.
- No API/serde-reflection snapshot exists for fl_protocol yet (architect §2.3 mentions one). It is not in this brief, so I did not add it.
