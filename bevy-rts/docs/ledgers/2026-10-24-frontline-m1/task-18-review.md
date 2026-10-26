### Spec Compliance
- ✅ Spec compliant (all brief interfaces, queries, F5-F7, throttle and `fl.debug_query` present).
- ⚠️ Gizmo drawing untestable headless (accepted: demo checklist). `docs/debugging.md` deferred to Task 21 (accepted). Headless gate failures are Task 8 blesses (accepted).

### Strengths
- Reads sim truth only via SimQuery::Debug through send_query/PendingReplies; no fl_sim use. One query per overlay, never overlapping, 15-frame cadence; paths use one query for all selected units. Request-id tracking plus abandon on toggle-off. Tests have no threads (try_recv on runner_channel). fl.debug_query uses deny_unknown_fields, forwards query Err as a JSON-RPC error through the existing `query` helper. F5-F7 do not collide (F1 console, F2 perf, F3 grid, F4 cycle player). Terrain height used via ground().

### Issues
#### Critical
- none
#### Important
- none
#### Minor
- remote.rs:140 (diff l.25): the fl.debug_query description is one long line with runs of ~10 literal spaces (missing `\` continuations used by every other entry); it shows up in the agent tool descriptions. Rewrap with `\`.
- F5-F7 toggles are not gated by InputGate: register_overlay (debug/mod.rs:174-194) has no InputGate, same as the existing F3. Pre-existing mechanism, so a cross-cutting follow-up, but it differs from what the lead asked to check.
- overlays.rs `draw_passability`/`blocked_near` (diff l.336-349, 573-595): every frame scans all blocked runs cell by cell (up to the whole map) and emits 2 gizmo lines per cell within 120 m. This is O(blocked cells) per frame on large maps; cache the in-range list per reply or camera move, or iterate rows only within the radius.
- overlays.rs `receive_overlay_replies` (l.495-524): a reply to a query sent before a selection change is applied as-is, so paths/profile can be stale for up to one poll (about 15 frames). Harmless but not discarded by selection; could record the args with the pending id.
- Report: no RED run (the tests were written after the implementation); process only.
- Unrelated Selection-less case: paths with an empty selection clear data silently (fine), but `poll_overlays` skips `since` reset there, so it re-evaluates every frame (cheap).
- Error replies keep the old data, so a stale overlay stays drawn after a persistent error (logged once). Consider clearing it.

### Assessment
**Task quality:** Approved
**Reasoning:** Matches the brief and ADR-0015 with sound bounded polling and id-based reply handling; only minor polish and a per-frame draw cost to watch.
