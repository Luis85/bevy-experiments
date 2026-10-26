---
name: debug-desync
description: Find why two sim runs (or a replay and the current code) diverge, down to the first tick, entity and component, then bisect to the cause. Use when a replay verify fails, a determinism test fails, state hashes differ across runs/OSes, or a change unexpectedly alters the village.
argument-hint: "[replay file | seed-a seed-b]"
---

# Debug a desync: $ARGUMENTS

Goal: first divergent tick → which entity/component → which code. Do not "fix" it by re-recording replays.

## 1. Reproduce and locate

```sh
cargo run -p npcctl -- replay verify tests/replays/<name>.json          # {ok, checked, first_mismatch:[tick,expected,actual]}
cargo run -p npcctl -- diverge --a <a.json> --b <b.json> --pretty        # two replays of the same content, or:
cargo run -p npcctl -- diverge --seed-a 1 --seed-b 1 --ticks 4000 --pretty   # same seed twice must NOT diverge
cargo run -p npcctl -- diverge --scenario testbed:smithy --seed-a 1 --seed-b 1 --ticks 2400 --pretty
cargo run -p npcctl -- replay verify-session <session.json>             # an agent session record (also `replay verify`)
```

`diverge` gives `first_divergent_tick`, `entities_differing` and per entity `components_differing` with
`a`/`b` values (every registered component by name). Exit 1 = diverged. Replays name their content
(`village`, `village_n`, `testbed`, `testbed:<name>`); a replay whose `content_hash` differs from the build
(a registered name was added, renamed or removed) is refused: that is a content change, not a desync.

Cross-OS: CI's `determinism` job (`.github/workflows/npc-framework.yml` at the repository root) uploads
`hashes-Linux.json` / `hashes-Windows.json` and the `determinism-compare` job names the run that differs.
Reproduce by recording the same run on each OS (`npcctl replay record ... --out <os>.json`) and
`diverge --a linux.json --b windows.json`.

## 2. Read the diff at that tick

- Which components differ first (`pos`, `needs`, `blackboard`, `move_to`, reservations)? The first one
  usually points to one system.
- Journal both sides around the tick for that unit:

```sh
cargo run -p npcctl -- journal --seed 1 --ticks <T+1> --unit <id> --from <T-50> --to <T> --verbose > target/a.jsonl
# testbed: --scenario testbed:<name> [--commands <replay.json>]; a session record: extract its `replay` first
# second side: same with the other seed/--commands, or the old binary (step 3)
```

- Compare the last matching entry and the first different one (`LeafStart`, `Abort`, `CondEval`, `Reserve`).

## 3. Same inputs, different code? Compare binaries

- Build the old revision in a separate worktree (never `git stash` here):
  `git worktree add ../npc-old <rev>`; run the same `npcctl journal`/`run` there; diff the outputs.
- Bisect when the change is not obvious:
  `git bisect start <bad> <good>` then `git bisect run sh -c 'cargo run -q -p npcctl -- replay verify tests/replays/<name>.json'`.

## 4. Usual causes (check in this order)

- [ ] Iteration in `HashMap`/`HashSet`/query/`Entity` order instead of sorted by `SimId`.
- [ ] Reading `Time`, wall clock, frame count, or float math feeding sim state.
- [ ] Stateful RNG, or a `sim_rand` call reusing another site's `purpose` (rules dice: a reused roll `purpose`).
- [ ] Capability glue (`crates/npc_framework/src/glue/*`, no `clippy.toml`): a clock, a hash container or a float.
- [ ] Two systems without an order (ambiguity); a command applied at a different `(tick, seq)`.
- [ ] Tooling writing sim state (journal/inspect/perf on vs off must give the same hash).
- [ ] A new component missing from `state_hash` (diverges later than it should) or hashed with a
      platform-dependent encoding (`usize`, `DefaultHasher`, float bits).
- [ ] An intentional behaviour change: then the replays are right to fail. Stage, show evidence and ask
      (`docs/rebaselining.md`); never edit `tests/replays/**` yourself.

## 5. Fix and gate

- [ ] Add a regression test that reproduces the divergence (`content/village_content/tests/regressions.rs`,
      `content/testbed_content/tests/`, or the generic crate's unit tests).
- [ ] `cargo xtask verify` → `"ok":true`, replay corpus included. Report tick, entity, component and cause.
