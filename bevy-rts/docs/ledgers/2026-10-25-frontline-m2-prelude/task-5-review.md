# Task 5 review: golden replay versioning and change classes (C8, M10)

Diff: `review-t5.diff` (a8cba36 on d102649, plus the four `.new` files). I read it in four passes. The checks outside the diff are each listed below with the risk they answer.

### Spec Compliance

- ✅ Spec compliant against the brief and ADR-0021, with the deviations the report declares. Each deviation is acceptable:
  - extra files (`fl_game/src/{builder,replay_capture}.rs`, `fl_test_utils/src/builder.rs`, `fl_cli/src/agent.rs`, `fl_sim/tests/formats.rs`) are forced by the type change;
  - the frozen fixture copy;
  - `debug.set_terrain` instead of a teleport;
  - a `--mods` flag and a `"report"` JSON key on `rebless`.
- Checked item by item:
  - **Format 4.** `crates/fl_sim/src/replay.rs:998-1019` adds `REPLAY_FORMAT_VERSION = 4` and `ReplayStart::Scale { version, params }`, with `ReplayStart::scale` added.
  - **Versioned decode.** `from_bytes` dispatches on the leading version at replay.rs:1307-1311. Format 4 decodes as `Replay`. Format 3 decodes through the frozen `mod v3` (replay.rs:1188-1228, field-for-field the m1 layout) and is converted at :1230-1263. Any other version is `ReplayError::Version`, and the error text names both readable versions (:1447).
  - **Semantic checks.**
    - `SemanticCheck` is `#[non_exhaustive]` with four variants and `id()` (replay.rs:1040-1150).
    - The checks run after every tick in `play_back` (:1599-1609). Each check fails at most once. A one-shot check whose tick is outside the replay fails (:1611-1629).
    - Hashes are judged first (:1702-1704).
    - `VerifyOutcome` is `#[non_exhaustive]` with `SemanticFailed`.
  - **`rebless_replay`** (replay.rs:1722-1775) runs with the journal on and the recorded hash interval. The result keeps start, ticks, events and checks; inputs are remapped to this build's command ids; header, hashes and journal summary are new. All as the brief says.
  - **`ScaleScenario`** (`scale.rs:1876-1907`): `VERSION = 1`, `encode` and `decode`. An unknown version errors with "ScaleScenario version N", and trailing bytes are rejected.
  - **`fl_cli rebless`** (`crates/fl_cli/src/rebless.rs:403-498`):
    - A missing MCR heading is refused before any replay is read. The heading match is a whole-id match (:351-361).
    - Semantic failures are collected over every replay before anything is written (:439-456). On refusal there is no `.new` and no report.
    - `.new` is written only for replays whose bytes changed.
    - Per replay, the report has the first divergent tick, the entities at the start and the end, the semantic results and a journal diff table (:523-582). These are the four items ADR-0021 asks for.
    - The JSON keys match the brief.
  - **change_class** (`xtask/src/change_class.rs`):
    - `needs_class` covers crates/, features/, assets/, the root scenarios/ and the root Cargo.toml/Cargo.lock (:2566-2575). These are the R151 minor-9 paths, and :2969-2977 tests them, including the negatives `docs/scenarios/` and `crates.md`.
    - Classes must be known and a malformed trailer is a violation (:2629-2641).
    - F/D need an `MCR:` trailer (:2643).
    - R/E changing a golden is a violation unless the same commit changes `REPLAY_FORMAT_VERSION`, computed per commit from `pub const` at `sha^` vs `sha` (:2812-2819, :2646-2654).
    - `mcr_violations` checks headings (:2676-2689).
    - `golden_violations` implements the branch rules and the `refactor_goldens` inequality (:2697-2743).
    - `check_branch` builds `refactor_goldens` per R/E commit against its parent, never merge base vs HEAD (:2899-2908, :2872-2890). This is R153.
    - All ten brief-named tests are present, plus `trailers_parse`, `const_lines_parse` and `hash_streams_compare_the_hashes_only`.
  - **Gate rows.** `change-class` (BRANCH = task, pr) and `golden-policy` (MERGE = pr, milestone) are in `xtask/src/gate.rs:3295-3304`, and :3376-3381 tests the levels.
  - **Goldens.** `golden.rs:2369-2409` attaches `ten_units`/`wander_off_water` and `thirteen_units`/`no_unit_on_water` exactly as the brief lists them. `golden_semantics_hold` is at :2432-2450.
  - **Changelog.** The `## MCR-000 — Replay format 4 (C8, M10)` entry is at changelog-mechanics.md:2482-2492. Its `Re-bless:` line is still "pending", which the lead step fills in.
- ⚠️ Cannot verify from the diff:
  - The git-collection half of change_class (`commits`, `hash_stream_kept`, `check_branch`, `dump_json`) has no test and has never run end to end (report concern 6). The controller should run `cargo xtask gate pr` (at least the change-class and golden-policy rows) once after the bless squash and the 767c640 rewrite, before closing Task 5.

### Checks run outside the diff (one per named risk)

1. **Is the frozen fixture really format 3 from m1?** `git rev-parse m1:crates/fl_test_utils/tests/replays/testbed_wander.replay`, `git hash-object crates/fl_sim/tests/fixtures/replays/testbed_wander_format3.replay` and `HEAD:…/testbed_wander.replay` all give 2976fe24. The fixture is byte-identical to the m1 golden.
   - At a8cba36 the gate's `golden_replays_verify` passed on the committed format-3 goldens (only `save_codec_formats` and `golden_semantics_hold` failed). So format-3 replays still verify on this build.
2. **Are the `.new` goldens hash-identical?** I ran the existing `target/debug/fl_cli.exe` (built 11:21, after a8cba36) with `--json replay dump` on the old and new files and compared them in Python (outputs in the scratchpad):
   - Both have equal hashes (400/400 and 900/900), equal inputs, start, ticks, hash interval and events.
   - The only header difference is `build_id` (`0.0.0+87407dc` → `0.0.0+a8cba36`, and `0.0.0+9799309` → `0.0.0+a8cba36`).
   - The checks are as the brief lists them. The journal summaries are: wander 4 kinds / 57 records; paths 6 kinds / 121 records, including `core.unit_spawned: 3` and `core.terrain_changed: 1`.
   - The sha256 of each `.new` matches the report.
3. **Are the semantic checks tautological (wrong terrain name, no water on the map)?** `assets/base/maps/testbed_1km.map.ron` has the legend entry `base.water`, the terrain id is `base.water` (`assets/base/terrain/water.terrain.ron`), and `testbed_paths` has a river (`RIVER_ROWS` 122..=127) and turns bridge A into water at tick 120 (`fl_test_utils/tests/support/testbed_paths.rs:51,94`).
   - So `no_unit_on_water` is a strong check: it would catch a unit left on the bridge as it floods.
   - The EntityCount checks pin both goldens' end counts (10, and 13 after 3 spawns), which also makes the "entities before/after" interpretation sufficient (ruling 3).
4. **Why did change-class pass at task level when 767c640 has no trailer?** `.gate-base` holds `d102649`, so the task gate scans only a8cba36.
   - I listed `git rev-list --no-merges main..a8cba36` with trailers and code paths. 767c640 is the only commit that touches a `needs_class` path without a `Change-Class` trailer. Its paths are `m1_testbed.save`, the deleted `.save.new` and `testbed_paths.poses`. None is a `*.replay`, so golden-policy is not affected.
   - Every other class-less commit is docs- or xtask-only.
5. **Where did the 1449 s go?** I read `target/xtask/gate-times.jsonl` and `target/xtask/{test-headless,test-client}.log`. Details under ruling 4.
6. **Does any guard still cover the `ScaleScenario` layout?** I grepped `crates/*/tests/formats.rs` and `fl_protocol/tests/support/format_guard.rs`. Nothing traces `ScaleScenario` any more (Important 1).

### Strengths

- The format split is clean:
  - `mod v3` is a true frozen copy, including the m1 `ScaleScenario`.
  - `from_bytes` is versioned rather than lenient.
  - A unit test covers the v3 scale-start conversion (replay.rs:1799-1855).
  - `decode_all` removes the duplicated trailing-bytes logic.
- `verify_replay` and `rebless_replay` share one `play_back` and do not duplicate the run loop. Hash-before-semantic order is tested (`semantic_checks_run_after_the_hashes`).
- The semantic tests are real behaviour:
  - Passing checks verify, and three distinct failure modes report in replay order with their ticks, including the tick past the end.
  - `never_on_terrain_checks_every_tick` proves the failure tick is 3 (the tick the water arrives), not the end. It also covers a non-matching terrain and a non-existent unit.
- `rebless` validates everything before it writes anything, and its tests assert the absence of `.new` files and of the report on both refusal paths.
- The change_class tests mirror R151/R153 precisely: an earlier format commit does not exempt a later R commit, and a later F re-bless does not fail the format commit. The rows fail closed:
  - a missing blob makes `hash_stream_kept` return false;
  - a missing changelog reads as empty, which produces violations.
- The report is honest about its RED/GREEN runs, the bless-pending gate failures, and the per-commit constraint on how the bless lands.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **The format guard no longer covers `ScaleScenario`.** Evidence: `crates/fl_sim/tests/formats.rs:1952-1961` and the `.new` snapshot at diff :3443-3444.
   - Before, `ScaleScenario`'s layout sat under `REPLAY_FORMAT_VERSION`, so a new field failed the guard until a version bump.
   - Now `ReplayStart::Scale` holds opaque `params: Seq(U8)`, the snapshot drops the `ScaleScenario:` line, and nothing traces it. `format_guard::guard` silently ignores keys that disappear.
   - ADR-0021/M10's rule, "a new field bumps `ScaleScenario::VERSION`", is therefore unenforced. The next bench field changes `encode` while `VERSION` stays 1, and every recorded scale replay fails to decode with "ScaleScenario version 1: …", the failure M10 set out to remove.
   - Fix: add a `SCALE_SCENARIO_VERSION = {ScaleScenario::VERSION}` section to `save_formats_text` that traces `ScaleScenario`, so the guard demands the bump. Regenerate `formats__save_codec_formats.snap.new` (it is blessed in the same squash). Optionally add a literal-bytes test of `ScaleScenario::with_units(50).encode()` at version 1.

#### Minor (Nice to Have)

1. **`NeverOnTerrain` passes silently on a bad terrain name or missing grids** (replay.rs:1135-1146). An unknown terrain string, or a missing `NavGrid`/`TerrainGrid`, makes the check hold forever, so a typo makes a check tautological. Fail when `terrain` is not a content terrain id, and when the grids are absent on a map that has terrain.
2. **`changed` is almost always true in rebless** (rebless.rs:434). Every new commit changes `header.build_id`, so after any commit every golden gets a `.new`, even when its hashes and summary are equal. The report should say what changed (hashes / header only / summary / checks), so that "changed" does not read as a behaviour change.
3. **The rebless report does not quote the MCR's predicted effect** (rebless.rs:523-527). It names the MCR but does not include the entry's "Predicted effect" line. ADR-0021 says the report "must confirm the prediction"; quoting the entry puts the two side by side.
4. **`trailer()` takes the first matching line anywhere in the message** (change_class.rs:2533-2540), not only in the trailer block. A body line that quotes `Change-Class: Q` or `MCR: …` wins over the real trailer. Read the last paragraph only, or the last match.
5. **ADR-0021's other class rules are not enforced.** "N keeps every existing golden bit-identical" and "SIM_VERSION is bumped by class F only" have no check. The ADR's gate-rules section and the brief do not require them, so this is not a spec miss, but a class N golden change is caught only indirectly (MCR/version), and an R commit bumping `SIM_VERSION` is not caught at all. Consider them for Task 21 or later.
6. **The frozen fixture is unprotected and only partly pinned** (`crates/fl_sim/tests/fixtures/replays/`, replay.rs tests :2044-2081). The test pins the version byte, two hashes, the tick count and the input ticks, not the bytes, and `is_golden` excludes the path, so no gate row guards it. Task 2's review rated an unpinned frozen artefact Important. Here the pin is partial, so Minor: add the path to `OWNERS.toml [protected]` (lead) or assert its length and sha256.
7. **`rebless_writes_new_and_report` builds format-3 bytes by patching the format-4 encoding** (rebless test :667-679). The patch is self-checked only through the v3 decoder. The real fixture (or a copy in the temp dir) would anchor it to m1 bytes.
8. **`replay.rs` grew to about 860 lines.** It still has one responsibility, but `SemanticCheck` and its evaluation could move to `replay/semantic.rs`, and the `v3` freeze to `replay/v3.rs`.
9. **`summarize`'s get_mut-then-entry dance** (replay.rs:1386-1394) is an allocation micro-optimisation that reads oddly. Accept or simplify.

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The format, the rebless tool and the change-class/golden-policy rules are correct, versioned and well tested. The new goldens are hash-identical and their semantic checks are meaningful. The one Important is that the M10 decoupling dropped `ScaleScenario` from the format guard, which leaves the new `ScaleScenario::VERSION` unenforced. It is a one-section fix that lands in the same bless.

### Recommended rulings

1. **767c640 without `Change-Class`: rewrite its message.** Add `Change-Class: R`: it adds and blesses test fixtures, changes no `*.replay`, and keeps hash streams unchanged, like its siblings 6d70b57 and 50a4408 (class R).
   - Do not add a cut-off or an allowlist. Both are permanent code for a one-off.
   - Pre-policy history already lies on `main`, outside every `merge-base..HEAD` range. Once this branch merges, 767c640 is never scanned again.
   - The history is local-only, and Task 1's filter-branch rewrite is precedent.
   - Do it in the same rewrite as the bless amend: `git filter-branch --msg-filter` over `main..m2p/phase-a`, touching only 767c640.
   - Then update `.gate-base`: `d102649` is a descendant of 767c640 and gets a new sha.
   - m2p/t07-tree (based on 1eac3cc) is unaffected.
2. **Bless in the same commit as the format bump: sound, keep it.** Amend the bless into a8cba36. It is the branch tip, so a plain `--amend` with `Blessed:` added to `Change-Class: R`, `MCR: MCR-000` and `Refs: ADR-0021` is enough.
   - Do not let a Blessed-only follow-up count as part of the R/E change. R151/R153 made the exemption per commit precisely so that it is mechanical. Pairing a follow-up commit with an earlier format commit needs fuzzy matching (by MCR? adjacency?), and any later R commit carrying `Blessed:` and `MCR: MCR-000` could then rewrite goldens.
   - The goldens' header `build_id` will name the pre-amend sha a8cba36. That is harmless: a different build id is only a verify warning. Do not re-record.
3. **The implementer's assumptions.**
   - **(a) Entity counts at the start and end of the re-run: accept.** Old replays store no count, and both goldens' `EntityCount` checks pin the end count, so a changed old-vs-new count makes rebless refuse anyway.
   - **(b) Water via `debug.set_terrain`: accept.** Teleports onto water are rejected, and the test proves per-tick evaluation (failure at tick 3) plus the negative cases.
   - **(c) Frozen format-3 fixture copy: accept.** It is necessary, since the committed golden becomes format 4, and it is verified byte-identical to `m1:…/testbed_wander.replay` (blob 2976fe24). Protect it or pin its digest (Minor 6).
   - **(d) Not re-verifying the fixture by playback: accept.** A playback test would break legitimately at MCR-001 (Task 13). Reading it and pinning the hashes is the right scope.
4. **Gate time of 1449 s: the trim works. The overrun comes from a sim-core change on a cold, shared build.**
   - Rows from `gate-times.jsonl` at a8cba36:
     - test-headless 800.7 s: the log shows "Finished `test` profile … in 7m 36s", so 456 s is compiling fl_sim and its six dependents in the optimized+debuginfo test profile, and 293 s runs 622 tests with 27 heavy tests skipped;
     - test-client 370.2 s: compile 1m44 s plus 88 s of tests, the rest outside the logged run; the track was added only because fl_game is touched;
     - api 174.5 s (rustdoc JSON);
     - data-check 42.7 s, clippy 39.5 s, and the rest under 10 s.
   - The trim is working as designed:
     - the heavy exclusion is applied (27 skipped);
     - contract-only task gates ran 87–98 s for test-headless and 128–264 s overall (1274ec3, aac327d);
     - but a change to fl_sim touches every reverse dependency, so the task gate runs almost the whole headless suite. Row times still include compile, because R154 moved compile out of the nightly `task-time` row only. Lane 2 (Task 7) was compiling at the same time.
   - Rulings:
     - the over-budget line is a WARN and does not block Task 5;
     - re-measure warm after the bless; I expect about 300 s for test-headless alone, so sim-core tasks will still exceed 300 s;
     - either set a per-track task budget (for example sim-core 600 s), or build untimed before the test rows as `task-time` does and report compile separately;
     - do not narrow `touched` to exclude reverse dependencies, which would weaken the task gate.

### Bless per `.new`

- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: **bless**. Format 4; hashes 400/400 equal; inputs, start, ticks and events equal; only the header `build_id` differs; checks `ten_units` and `wander_off_water`; summary of 57 records. sha256 4c44dbff….
- `crates/fl_test_utils/tests/replays/testbed_paths.replay.new`: **bless**. Format 4; hashes 900/900 equal; inputs, start, ticks and events equal; only `build_id` differs; checks `thirteen_units` and `no_unit_on_water`; summary of 121 records. sha256 3413f1cf….
- `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new`: **bless after Important 1**. Regenerate it with the `ScaleScenario` section. The current content (version 3→4, `semantic`/`journal_summary`, `Scale{version, params}`, `SemanticCheck`) is correct but would drop the `ScaleScenario` guard line. If the lead defers Important 1, bless as is and track the guard as a follow-up.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless**. It matches the brief's interface exactly, plus `ReplayStart::scale`.
- All four go in one bless with `--reason "MCR-000: replay format 4 with semantic checks; hash streams unchanged"`, amended into the format commit (ruling 2). Fill in MCR-000's `Re-bless:` line and list MCR-000 for Task 21's ratification.
