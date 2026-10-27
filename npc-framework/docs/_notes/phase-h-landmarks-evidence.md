# Landmark/map evidence (2026-10-27)

Tasks 21–23 add owned map files, session-bound tools and player methods, landmark navigation, a world switch,
F7 editing and fog-independent marker labels. ADR 0018 records the storage, knowledge and replay boundaries.

## Focused verification

- `npc_agent_io`: 45 library tests, 10 MapStore tests, 5 map API tests and 37 notes tests passed. Coverage
  includes hand edits with restored size/mtime, ID/revision exhaustion, actual locked-file cleanup retries,
  guarded migration cleanup, numeric YAML coordinates, null-kind updates, shared writers, registration scope,
  map-off-before-I/O, nearest/delta/text/act-and-wait output, coordinate extremes and atomic failed batches.
- View: 52 tests passed/1 existing clipboard test ignored initially. After the marker correction, all 5 marker
  tests passed, including coincident/nearby anchor packing, bounded widths, fixed anchors and non-pickability.
  Existing F7 tests cover add/edit/stale saves, retained field/caret state, tab availability, keyboard shielding
  and cancellation of a selection marquee whose release was consumed by the panel.
- CLI/MCP: 2 parser tests and 9 integration tests passed. Map placement is exclusive, signed coordinates stay
  signed, explicit null differs from omission, the player/agent share versions, and stale updates cannot replace
  newer data. Registered MCP map tools work before take_control and expose no identity selectors.
- Content: host 37, landmark integration 3 and world mechanics 17 passed. Thing placement uses the last real
  percept when truth moved or disappeared. Landmark moves arrive and replay after deleting the map folder.
- Workspace/all-target Clippy with warnings denied passed; format and diff checks passed. No dependencies,
  oracle files, generated lint policy or existing assertions were removed/rebaselined.

## Native/generated-MCP probe

`python target/phase-h-e2e.py` uses the actual generated MCP command/configuration and agent working directory.
The scripted agent uses only MCP calls; developer lifecycle/notes actions are separate. The confirmation
run `target/phase-h-e2e-68b438286c24/summary.json` reports `ok:true` for:

- register/enter/take control of a rover; add Here and explicit-position landmarks; list/update/clear kind;
- reject a stale landmark update; move by landmark ID with act-and-wait and receive nearest-map output;
- write diary/memory/proposals and accept a developer memory entry;
- save/stop detached at tick 54, resume native on the same port, retain actor/position and map, and recover a
  stale read cursor transparently (resumed running observation tick 75);
- revoke the old MCP session by rotating its token, reject automatic credential reuse, and accept explicit
  new registration; no credential response echoes; every probe-owned process exits.

The inspected capture is
`target/phase-h-e2e-68b438286c24/npc-out/shots/phase-i-handover.png` (the reusable probe's filename): F7 Map
shows all three entries and stamps; separate Arrival/Return point labels stay readable near the rover;
Beyond wall is visibly labelled over black unexplored fog. Its tick is 176. The initial capture exposed
overlapping/shrink-wrapped labels; one bounded correction gives labels explicit widths and stable collision
placement while retaining exact anchor dots. No real OS input was sent.

## Full gate

The first full run passed 1,216 of 1,217 tests, documentation, architecture, eight replays, six unchanged
snapshots, format, Clippy and deny. Its single failure was an older exact MCP tool-list assertion in
`notes_api.rs` still expecting the pre-map list. The assertion now includes all three new tools and exact
required/property checks for each; all existing notes assertions remain. The focused nextest schema
regression then passed. A complete gate is rerun rather than treating that isolated result as a full pass.

The complete rerun reports **`"ok":true`** (`target/verify-phase-h-final.json`): 1,217 passed, 12 skipped,
one slow and two reported leaky; documentation 1 passed/10 ignored; architecture 7/7; eight replays verified;
six snapshots none pending; format, Clippy and deny passed. No optional tools skipped. Process-local
`CARGO_BUILD_JOBS=8`, `NEXTEST_TEST_THREADS=2`; no retry/filter/ignore changes in this full gate.
The local merge and five-app release result follow in the progress ledger. The final review's separate fix
pass will also address the inherited clock-reset stamp collision discovered after these focused tests.
