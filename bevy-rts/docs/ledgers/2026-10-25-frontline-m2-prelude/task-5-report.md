# Task 5 report: golden replay versioning and change classes (C8, M10)

Status: DONE_WITH_CONCERNS (four protected artifacts wait for the lead's bless; see "Lead step")

Commit: `a8cba36 feat(fl_sim)!: replay format 4 with semantic checks, fl_cli rebless, change-class and golden-policy gate rows`
(trailers `Change-Class: R`, `MCR: MCR-000`, `Refs: ADR-0021`; no `Blessed:`, since the commit holds no protected file).

## What I implemented

**fl_sim `replay.rs` (format 4)**
- `REPLAY_FORMAT_VERSION = 4`. `ReplayStart::Scale { version, params }`. I added a helper, `ReplayStart::scale(&ScaleScenario)`.
- `Replay` has two new fields, `semantic: Vec<SemanticCheck>` and `journal_summary: BTreeMap<String, u32>`. `ReplayRecorder::record` counts each tick's journal records by kind.
- `SemanticCheck` is `#[non_exhaustive]`. Its variants are `UnitWithin`, `UnitExists`, `EntityCount` (the `SimIndex` length) and `NeverOnTerrain` (checked after every tick, live listed units only, cell from `NavGrid::cell_of` and terrain from `TerrainGrid::terrain_at`). It has an `id()` method.
- Other new items: `SemanticFailure`, `Replay::with_semantics`, `VerifyOutcome::SemanticFailed` (and `VerifyOutcome` is now `#[non_exhaustive]`), `ReblessEntry` and `rebless_replay`.
- `mod v3` freezes the format-3 `Replay`, `ReplayStart` and `ScaleScenario` exactly as at tag `m1`. `from_bytes` reads format 3 and converts it into the current in-memory form: version 4, scale params at version 1, no checks, an empty summary.
- `verify_replay` and `rebless_replay` share one private `play_back`:
  - Each check records only its first failure.
  - A one-shot check whose tick the replay never runs is a failure.
  - Hashes are judged first: `Diverged` wins over `SemanticFailed`.
- `rebless_replay` runs with the journal on and the replay's hash interval. The new replay has this build's header, the same start, ticks, events and checks, and the inputs remapped to this build's command ids.

**fl_sim `scale.rs`**: `ScaleScenario::VERSION = 1`, plus `encode` and `decode`. `decode` errors name the version ("ScaleScenario version 2 is not supported …") and reject trailing bytes.

**fl_cli `rebless`** (new `src/rebless.rs`, wired into `lib.rs` and `main.rs`)
- Usage: `fl_cli rebless --reason MCR-nnn [--dir …] [--changelog …] [--report target/rebless-report.md] [--mods …]`.
- Exit 1, writing nothing (no `.new` file, no report):
  - when the changelog has no `## MCR-nnn` heading (a whole-id match, so `## MCR-0001` does not match `MCR-000`);
  - when any replay's semantic check fails.
- Otherwise exit 0. It writes `<name>.replay.new` only for replays whose bytes change, and the report. Per replay the report gives: changed, first divergent tick, entities at the start and the end, the semantic results, and a journal summary diff table.
- JSON output: `{"reason", "report", "replays": [{"name", "changed", "first_divergence", "journal_diff": [{"kind", "before", "after"}]}]}`.
- `replay-verify` now reports `semantic-failed` (exit 1). `replay dump` prints the checks and the journal record count.

**xtask `change_class.rs`** (new)
- Pure functions, as specified: `CLASSES`, `trailer_class`, `trailer_mcr`, `needs_class`, `CommitFacts`, `BranchFacts`, `class_violations` and `golden_violations`.
- Extra helpers: `is_golden`, `has_mcr_heading`, and `mcr_violations` (every `MCR:` trailer must name a changelog heading, per ADR-0021's change-class row).
- Git collection works from `git rev-list --no-merges`, which leaves merge commits out.
- The per-commit R/E hash-stream check compares the commit against its parent (R153). It uses `git show <sha>:<path>` bytes, then `fl_cli replay dump --json` on both sides, then compares `hashes`.
- SIM_VERSION and REPLAY_FORMAT_VERSION are read from their `pub const` lines.
- Gate rows: `change-class` (task, pr) and `golden-policy` (pr, milestone), appended to `steps()`.

**Other changes**
- `golden.rs`: the record functions attach the checks. `testbed_wander` gets `ten_units` and `wander_off_water`; `testbed_paths` gets `thirteen_units` and `no_unit_on_water`. New test: `golden_semantics_hold`.
- Changelog: `## MCR-000 — Replay format 4 (C8, M10)` in the template's six-line form.

**Files outside the brief's Files list** (needed for the build):
- `crates/fl_test_utils/src/builder.rs` and `crates/fl_game/src/{builder,replay_capture}.rs`: the `ReplayStart::Scale` shape and the new `Replay` fields.
- `crates/fl_sim/tests/formats.rs`: traces `SemanticCheck` so the reflection registry is complete.
- New fixture `crates/fl_sim/tests/fixtures/replays/testbed_wander_format3.replay`: a byte-identical copy of the tag-`m1` golden. `format_3_replays_still_read` needs a format-3 file that survives the bless; the fixture path is not protected.

## Tests

**TDD evidence**
- RED, xtask: `cargo nextest run --profile agent -p xtask -E 'test(/class|golden_change|mcr_must|sim_version_bump|refactor_class|format_commit|format_exemption|later_reblesses|data_commit|trailers|const_lines|hash_streams/)'` with `todo!()` bodies: `13 tests run: 0 passed, 13 failed` (not yet implemented).
- RED, fl_sim/fl_cli/fl_test_utils (the brief's Step 2 filter): the build failed as expected.
  - `unresolved import fl_sim::SemanticCheck`, `fl_sim::rebless_replay`
  - `no method named with_semantics`, `no field semantic / journal_summary on Replay`
  - `could not compile fl_cli (test "rebless")`, `fl_test_utils (test "golden")`
- GREEN, xtask: the same filter plus `golden_policy`: `14 tests run: 14 passed`.
- GREEN, Step 4 (Step 2 filter plus `mcr_headings`): `14 tests run: 12 passed, 2 failed`.
  - `golden_semantics_hold` failed as expected (it needs the re-recorded goldens).
  - `format_3_replays_still_read` failed on the placeholder hash literals. I filled them from the fixture: tick 0 `0x57ef0ca1ac59fc73`, tick 399 `0xfe273928e11a01d1`.

**Step 5, re-record**: `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden -E 'test(=golden_replays_verify)'`. I re-ran it after the commit on a clean tree, so the headers carry build `a8cba36`, not a `-dirty` id. Comparison through `fl_cli replay dump --json`:

| Golden | Format | Hashes equal | Inputs, start | Header | Checks | Journal summary |
|---|---|---|---|---|---|---|
| testbed_wander | 3 → 4 | 400 vs 400, yes | equal | build_id only (87407dc → a8cba36) | ten_units, wander_off_water | 4 kinds / 57 records |
| testbed_paths | 3 → 4 | 900 vs 900, yes | equal | build_id only (9799309 → a8cba36) | thirteen_units, no_unit_on_water | 6 kinds / 121 records |

- sha256 of `testbed_paths.replay.new`: `3413f1cf8d797fac9bcb27ef31238eed89a1fb013a08db9742cde4e221ceaf42`.
- sha256 of `testbed_wander.replay.new`: `4c44dbff7a268994fa337fbe66ec5b816cd405d545f84acb01b9a33815bfb576`.

**Rebless on copies**: I copied both `.new` files into a scratch directory as `.replay` and ran `cargo run -q -p fl_cli -- rebless --reason MCR-000 --dir <scratch> --report <scratch>/rb-report.md --json`. It exited 0, wrote no `.new`, and both replays came back `changed: false`, `first_divergence: null`, `journal_diff: []`.

```
# Re-bless report: MCR-000
## testbed_paths.replay
- changed: no
- first divergent tick: none
- entities: 10 at the start, 13 at the end
- semantic checks: all 2 hold (thirteen_units, no_unit_on_water)
- journal summary diff: none
## testbed_wander.replay
- changed: no
- first divergent tick: none
- entities: 10 at the start, 10 at the end
- semantic checks: all 2 hold (ten_units, wander_off_water)
- journal summary diff: none
```

The run matches MCR-000's prediction: the bytes change, the hash streams are identical, and there is no first divergent tick.

**Format snapshot**: `formats__save_codec_formats.snap.new`. The format guard passes because the version was bumped. The diff:
- `REPLAY_FORMAT_VERSION = 3 → 4`.
- `Replay` gains `semantic: Seq(SemanticCheck)` and `journal_summary: Map{Str,U32}`.
- `ReplayStart::Scale` becomes `Struct{version: U32, params: Seq(U8)}`.
- The `ScaleScenario` line is removed from the replay section, and a new `SemanticCheck` enum line is added.
- `assertion_line` changes from 71 to 73.

**API snapshot**: `xtask/snapshots/api/fl_sim.txt.new`.
- `ReplayStart::Scale{params, version}`, `ReplayStart::scale`.
- `SemanticCheck` (`#[non_exhaustive]`), `SemanticFailure`, `ReblessEntry`.
- `Replay::{semantic, journal_summary, with_semantics}`.
- `VerifyOutcome` is `#[non_exhaustive]` and gains `SemanticFailed`.
- `ScaleScenario::{VERSION, encode, decode}`, `rebless_replay`.

**Step 7**: I put the four `.new` files temporarily in place of the committed ones, then restored the originals byte for byte and put the `.new` files back. `cargo nextest run --profile agent -p fl_sim -p fl_cli -p fl_test_utils -p xtask`: `649 tests run: 649 passed (1 slow)`.

**Clippy**: `cargo clippy --workspace --all-targets --exclude 'probe_*' -- -D warnings` exits 0. The only warnings are the two existing `crates/fl_sim/clippy.toml` `rand::random` ones.

**Step 8, gate**: `cargo xtask gate task --track contract --track sim-core --track tools --track client --track docs`

```
PASS fmt, clippy-drift, claude-md, check-scope, protected, test-guard, clippy (39.5s), arch-check, test-client (370.2s), data-check, change-class (0.9s)
FAIL test-headless (800.7s): 622 run, 620 passed, 2 failed: fl_sim::formats save_codec_formats (snapshot .new pending), fl_test_utils::golden golden_semantics_hold (committed goldens still format 3)
FAIL api (174.5s): fl_sim changed (fl_sim.txt.new pending)
WARN gate task took 1449s (budget 300s)
gate task: FAIL (test-headless, api) in 1449s
```

Both failures are bless-pending artifacts only. With the `.new` files in place the same tests pass (Step 7).

## Lead step (R146)

```
cargo xtask bless crates/fl_test_utils/tests/replays crates/fl_sim/tests/snapshots xtask/snapshots --reason "MCR-000: replay format 4 with semantic checks; hash streams unchanged"
```

Commit the four files with `Change-Class: R`, `MCR: MCR-000` and `Blessed: …`. That commit is class R, changes goldens, and also needs `replay_format_changed`, so that commit itself must change `REPLAY_FORMAT_VERSION`. If the bless is a separate commit after a8cba36 instead, `change-class` flags it, because the exemption is per commit (R151). Two ways to avoid that:
- amend or squash the bless into a8cba36, as the brief's Step 9 intends (one commit);
- or re-commit with the version line.

Also list MCR-000 for Task 21's ratification, and fill in the `Re-bless:` line of the MCR-000 entry.

## Concerns

1. **`gate pr` will fail `change-class` on an existing commit.** `767c640 chore: bless M1 save fixture and pose trace` touches `crates/fl_test_utils/tests/replays/*` and has no `Change-Class:` trailer. I checked every non-merge commit in main..HEAD, and it is the only one. The controller needs to add a trailer (rewrite it) before `gate pr`.
2. **The bless must land in the same commit as the format bump** (see "Lead step"). Otherwise `change-class` reports a class R commit that changes goldens without a format change. This is the intended R151 behaviour, but it constrains how the bless is committed.
3. **Tracks.** The gate also needed `--track client` (the two `fl_game` edits that the type change forces) and `--track docs` (the changelog). The brief named contract, sim-core and tools only.
4. **Interpretations, all documented in code:**
   - "Entity count before/after" means at the start and at the end of the re-run; old replays store no count.
   - `never_on_terrain_checks_every_tick` moves water under the tank with `debug.set_terrain`, because a teleport onto water is rejected.
   - A format-3 replay reads in memory as version 4.
5. **Journal summary depends on the build.** It is filled only when the journal is on (debug builds, or `rebless`). A golden recorded by a release-mode test run would have an empty summary, and `rebless` would then report it as changed. That is harmless, but worth knowing.
6. **The `golden-policy` row runs `cargo run -p fl_cli` per R/E golden change** (to dump at the commit and at its parent). It has not been exercised end to end yet. It is covered by unit tests on the pure parts, and will first run at `gate pr` once the goldens are blessed.
7. **Gate time** was 1449s against a 300s budget, with a cold build and shared builders.
8. **`replay.rs` grew from 367 to about 860 lines** (format v3 freeze, checks, playback, rebless). It is still one responsibility.

## Fix round 1

Commit: `48a0691 fix(fl_sim): Task 5 review round 1: ScaleScenario format guard, strict NeverOnTerrain, rebless report detail` (`Change-Class: R`, `Refs: ADR-0021`; on top of a8cba36; goldens untouched).

- Important 1: `crates/fl_sim/tests/formats.rs` adds a `SCALE_SCENARIO_VERSION = 1` section tracing `ScaleScenario`; new test `scale_scenario_change_needs_a_version_bump` (an added field without a bump → guard error "wire format of ScaleScenario changed without a version bump"; with `VERSION + 1` → passes). `scale_start_round_trips_versioned_params` now also pins version-1 params as literal bytes `[32 04 a0 1f ac 02 01 02 10 14 0a 00]`.
- Minor 1: `NeverOnTerrain` fails at its first tick when `terrain` is not a `TerrainDef` of the content, or when the match has no `NavGrid`/`TerrainGrid` (doc updated). `never_on_terrain_checks_every_tick` adds the `t.lava` case (failure at tick 0 naming it).
- Minor 2: `fl_cli rebless` lists what changed per replay: `format 3 → 4`, `header <field>` (e.g. `header build_id`), `hash stream`, `inputs (command ids remapped)`, `journal summary` (`encoding` if bytes differ otherwise). In the report line (`- changed: yes (format 3 → 4; journal summary), wrote X.new`), the text output and JSON `"changes"`.
- Minor 3: the report quotes the entry's `- Predicted effect:` (with continuation lines) as `Predicted effect (MCR-nnn): …`; JSON `"predicted_effect"` (null if the entry has none). Unit test `predicted_effect_is_quoted_from_its_entry`; `rebless_writes_new_and_report` asserts both.
- Minor 4: `change_class::trailer` reads only the trailer block (the last paragraph, never the subject); `trailers_parse` covers a body quoting `Change-Class: Q`/`MCR:` above the real `Change-Class: R`, and a subject-only message.
- Minor 6: `format_3_replays_still_read` asserts the fixture's length (6552) and sha256 `921e082a…6cc89e61` (an in-test FIPS 180-4 SHA-256, checked against the empty and "abc" vectors in `sha256_matches_known_digests`; the workspace has no hashing crate). `OWNERS.toml [protected]` gains `crates/fl_sim/tests/fixtures/replays/**`.

Tests:
- `.new` artifacts swapped in, `cargo nextest run --profile agent -p fl_sim -p fl_cli -p fl_test_utils -p xtask` → `652 tests run: 652 passed`; committed files restored byte-for-byte after.
- After a clippy fix (`as_chunks` in the test SHA-256, amended into 48a0691): `cargo nextest run --profile agent -p fl_sim --test replay` → `21 passed`; `cargo clippy --workspace --all-targets --exclude 'probe_*' -- -D warnings` → exit 0.

Gate `cargo xtask gate task --track contract,sim-core,tools,client,docs`:
```
PASS fmt, clippy-drift, claude-md, check-scope, test-guard, clippy (73.0s), arch-check, test-client, data-check, change-class
FAIL protected: crates/fl_sim/tests/fixtures/replays/testbed_wander_format3.replay — now protected, added since the gate base in a8cba36 (lead blesses it in the squash)
FAIL test-headless: 625 run, 623 passed, 2 failed — save_codec_formats (.snap.new pending), golden_semantics_hold (goldens pending)
FAIL api: fl_sim changed (.txt.new pending)
WARN gate task took 710s (budget 300s)
gate task: FAIL (protected, test-headless, api) in 710s
```
All three failures are bless-pending artifacts.

`.new` files:
- `formats__save_codec_formats.snap.new` CHANGED: adds `SCALE_SCENARIO_VERSION = 1` and the `ScaleScenario:` line (assertion_line 73 → 109). sha256 f8fada3f…74475a75.
- `fl_sim.txt.new` unchanged (9ca175f6…84bf12; no public API change this round).
- `testbed_wander.replay.new`, `testbed_paths.replay.new` unchanged (4c44dbff…, 3413f1cf…).
