# Task 7 report: `fl_view` snapshot schema, events, journal entry, runner protocol, driver trait

**Status:** DONE_WITH_CONCERNS (one encoding choice for view channels, see Concerns)
**Commit:** `c772e9d feat(fl_view): snapshot schema, events, journal entry, runner protocol, command stamper and TickDriver` (trailers `Refs: ADR-0002`, `Co-Authored-By: Claude Opus 5.5`)
**Worktree / branch:** `C:\fl\a`, `m0/phase-a`, on top of `32ec154` (`.gate-base`).

## What was implemented

All interfaces from the brief, verbatim names and signatures:

- `lib.rs`: `#![deny(clippy::float_arithmetic)]` (R24), `VIEW_FORMAT_VERSION = 1`, module declarations and re-exports of every public type.
- `snapshot.rs`: `SimSnapshot { tick, entities, channels }` with `get` (binary search by `sim_id`), `channel::<C>()` (`None` if absent, `Some(Err(msg))` if it doesn't decode), `set_channel` (replace-or-insert keeping `channels` sorted by name); `EntityRecord` with `hp_frac(hp, max)` = `hp * 65535 / max` in `u64`, rounded down, `max == 0 -> 0`, capped at 65535 when `hp > max`; `flags::{MOVING = 1, INVULNERABLE = 2}`; `ViewChannel` trait.
- `event.rs`: `#[non_exhaustive] SimEvent { Spawned, Despawned, CommandRejected, Custom }`.
- `journal.rs`: `JournalEntry { tick, phase, kind, sim_id, player, emitted, data: serde_json::Value }`.
- `runner.rs`: `TickInput` (serde), `TickOutput` (not serialized: `Debug, Clone, PartialEq` only), `#[non_exhaustive] SimQuery`, `#[non_exhaustive] SimReply`, `ToRunner`, `FromRunner` (not serialized, they carry `TickOutput`), `RunnerFailure` (serde), `RunnerPort`/`RunnerEnd` (`Debug, Clone`), `runner_channel()` over two unbounded crossbeam channels.
- `driver.rs`: `CommandDraft`, `CommandStamper` (`Debug, Clone, Default`; private `next_seq: BTreeMap<PlayerId, u32>`, `held: BTreeMap<Tick, Vec<PlayerCommand>>`), `TickDriver` trait.
  - `stamp`: per-player seq from 0, +1 per stamp; holds a clone and returns the command.
  - `take_due(tick)`: `split_off(tick + 1)` (overflow-safe at `u32::MAX`), returns the due commands sorted `(player, seq)`.
  - `held()`: all held commands sorted `(exec_tick, player, seq)`.
  - `restore(pending, last_applied)`: **resets** the stamper (clears held, rebuilds next seq), holds exactly `pending`, next seq per player = `max(last_applied, max pending seq) + 1`; players in neither map start at 0.

Data types derive `Debug, Clone, PartialEq, Serialize, Deserialize` as required. No `Cargo.toml` change.

## Tests (`crates/fl_view/tests/view.rs`)

The nine tests from the brief, with a few extra assertions inside them: `channel_roundtrip` also checks absent → `None` and that a second `set_channel` replaces (still one channel); `stamper_assigns_increasing_seq_per_player` checks `held().len() == 3`; `stamper_take_due_sorted_and_only_due` checks the tick-6 command remains held and a second `take_due(5)` is empty; `stamper_restore_continues_seq` checks `held() == [pending]` and that another player still starts at 0; `snapshot_postcard_roundtrip` includes a channel.

### TDD evidence

RED — `cargo nextest run --profile agent -p fl_view` against the stub crate:
```
error[E0432]: unresolved imports `fl_view::CommandDraft`, `fl_view::CommandStamper`, `fl_view::EntityRecord`, `fl_view::SimSnapshot`, `fl_view::TickInput`, `fl_view::ToRunner`, `fl_view::ViewChannel`, `fl_view::flags`, `fl_view::runner_channel`
error: could not compile `fl_view` (test "view") due to 1 previous error
```
Expected: the crate was a doc-comment stub.

GREEN — same command after implementation:
```
    Starting 9 tests across 2 binaries
     Summary [   0.314s] 9 tests run: 9 passed, 0 skipped
```

### Gate — `cargo xtask gate task --track contract`
```
PASS fmt (2.1s)
PASS clippy-drift (3.9s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.4s)
PASS test-guard (0.6s)
PASS clippy (13.9s)
PASS arch-check (5.7s)
PASS test-headless (26.9s)
SKIP test-client: no client package touched since 32ec154 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 55s
```
Files are LF (checked for `\r`).

## Files changed

- `crates/fl_view/src/lib.rs` (rewritten from stub)
- `crates/fl_view/src/{snapshot,event,journal,runner,driver}.rs` (new)
- `crates/fl_view/tests/view.rs` (new)

## Self-review

- Signatures match the brief exactly; extra derives only where harmless (`CommandDraft: Eq`, `CommandStamper: Debug, Clone`, ports `Debug, Clone`).
- `hp_frac` capping for `hp > max` is an addition the brief doesn't specify (it would otherwise overflow `u16`); documented on the fn.
- `restore` semantics chosen as a full reset (the brief says "re-holds pending"); documented. If the controller wants it to merge with existing state instead, it is a two-line change.

## Concerns

1. **View-channel encoding is JSON (`serde_json`), not postcard.** `postcard` is only a dev-dependency of `fl_view` (Task 1 table), the brief's Files list excludes `Cargo.toml`, and `serde_json` is a regular dependency, so `set_channel`/`channel` use `serde_json::to_vec`/`from_slice`. Consequences: (a) channel types whose map keys aren't strings/integers fail to encode — `set_channel` panics with a named message (documented `# Panics`), since the brief's signature returns `()`; (b) bytes are larger than postcard. Snapshots are in-process (and never hashed) in M0, so determinism is unaffected. If postcard is wanted, `fl_view` needs `postcard` promoted to `[dependencies]` (a Cargo.toml edit for the controller to rule on).
2. `JournalEntry` / `SimReply` carry `serde_json::Value`, which postcard cannot deserialize (no `deserialize_any`). They derive `Serialize, Deserialize` as specified and work with JSON (CLI/BRP); a postcard round trip of them would fail. Not tested via postcard by design; flagging for Task 28's `formats.rs` snapshots.

## Fix report 1: Ruling R41 (postcard view channels)

**Commit:** `88d5e76 fix(fl_view): postcard view channels` (`Refs: ADR-0002`, Co-Authored-By trailer)

What changed:
- `crates/fl_view/Cargo.toml`: `postcard` moved from `[dev-dependencies]` to `[dependencies]`. `Cargo.lock` needed no change (fl_view's lock entry already listed `postcard`).
- `crates/fl_view/src/snapshot.rs`: `set_channel` encodes with `postcard::to_allocvec` and `channel` decodes with `postcard::from_bytes`. The brief's signature returns `()`, so `set_channel` still panics on an encode failure, with the message ``view channel `<NAME>` failed to encode: <err>``. The `# Panics` doc was updated for postcard's failure cases. `channel` still returns `Some(Err(String))` on a decode failure.
- `crates/fl_view/tests/view.rs`: two new tests.
  - `channel_keeps_fx_exact`: a channel holding `Fx::from_raw((1 << 62) + 1)` round-trips exactly. An f64/JSON round trip would drop the last raw unit.
  - `channel_decode_failure_is_err`: empty bytes under the channel's name give `Some(Err(_))`.
- `JournalEntry` and `SimReply` keep their `serde_json::Value` fields, per the ruling.

Covering tests: `cargo nextest run --profile agent -p fl_view`
```
    Starting 11 tests across 2 binaries
     Summary [   0.431s] 11 tests run: 11 passed, 0 skipped
```

Gate: `cargo xtask gate task --track contract`
```
PASS fmt (2.3s)
PASS clippy-drift (3.7s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.4s)
PASS test-guard (0.5s)
PASS clippy (16.9s)
PASS arch-check (8.5s)
PASS test-headless (46.3s)
SKIP test-client: no client package touched since 32ec154 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 80s
```
Files are LF. Concern 1 is resolved by R41. Concern 2 (`serde_json::Value` can't round-trip through postcard) still applies, as intended, to Task 28's `formats.rs`.
