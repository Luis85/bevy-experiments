# Island Village: comprehensive review and polishing followup

> Historical evidence, recovered on 2026-11-20 from the leftover branch
> `audit/island-first-playtest-verified` (commits `63417af`, `dc303f7`, `bf48345`).
> It describes the testbed-hosted Island Village tier that ADR 0044 removed:
> `testbed_content/src/village`, `docs/island-village-playtest.md`,
> `docs/ai-village.md` and `scripts/playtest/live_smoke.py` no longer exist.
> Main took `63417af`'s code in `a3d6fdb` but not `dc303f7`'s owned process
> containment or `bf48345`'s CLI identity handoff; see
> [unported agent-supervision work](2026-11-20-unported-agent-supervision.md).

Base: `63417af`, 2026-10-29. Scope: the existing Island Village implementation,
registered-client/continuation tooling and incumbent native observer UI. Three
independent source reviews preceded disjoint implementation packages; a fresh
reviewer examined their integration. The coordinator owns builds and all process,
native and final verification work. No real provider turn is authorized or run in
this pass; prior paid-smoke limits were consumed.

## Review and implemented corrections

| Priority | Finding and impact | Correction | Evidence |
|---|---|---|---|
| P1 | Ordinary scheduler cleanup depended on root liveness, allowing descendants to outlive a finished CLI and its session lock. This predates the base commit. | Windows suspended creation and owned Job containment; independent descendant handle/accounting verification. POSIX owns a process group and reports unverified cleanup conservatively. Locks remain when cleanup cannot be verified. | Fake root/descendant/sentinel tests cover normal exit, timeout, unlink and actor replacement. |
| P1 | Windows `tasklist` failures could be mistaken for a dead original client or lock owner. This predates the base commit. | Native read-only process handles; tri-state liveness treats unknown as a reason to wait, never permission to dispatch or clear a lock. | Unknown-liveness and retained-lock regressions. |
| P1 | The live harness checked elapsed time around loops but independent blocking operations could outlast its 600-second cap. | Absolute budget begins before authentication; every operation receives remaining time; cleanup is reserved; a serialized owned-pool watchdog stops trees independently of blocked I/O; late binding is refused. | Fake clock, slow-call and independent-watchdog contracts. No provider usage. |
| P2 | Malformed shutdown responses could skip explicit cleanup and summary; queue emptiness could race the provider reader. | Unconditional nested cleanup, truthful results, bounded explicit EOF stream and bounded stdin delivery. | Malformed-response, EOF and cleanup-failure fake tests. |
| P2 | Remembered resource patches advertised live unseen quantities, and distant gather refusals exposed current depletion. | Quantities are shown only in reach; remembered identity/duration remains discoverable. Reach is checked before depletion. | Registered-observation and direct-command privacy regressions. |
| P2 | Seasonal fixtures could pass with only aggregate production while most stages failed. | All ten stages need fresh production in every seed/season fixture. Explicit operator demand removes and records manufactured endowments; raw resources and currency are retained. Fixtures stop after success or six accelerated days, always inside the selected season. | Seeds 1/7/42 × four seasons; currency and resident survival assertions retained. |
| P2 | F7 notebooks obscured observer text and world captions; long error content could push HUD actions off a short screen. | Notebook and village panels coordinate presentation; captions avoid notebook bounds. The HUD body scrolls within its viewport cap while title/actions stay reachable. | Current native notebook/narrow captures and view tests. |
| P2 | Startup Copy/OK and initial HUD controls lacked keyboard entry; F7 rejected HUD/world focus. | Local modal traversal, names and gold focus; enabled control traversal; scoped F7 focus transfer/restoration without weakening other overlays or note typing. | Native Tab/Enter/F7 checks plus integrated keyboard/hash regressions. |
| P2 | ASCII-only font received generated ellipsis/em-dash glyphs; accepting movement erased the retained concrete refusal while its old journal entry was still displayed. | Bounded ASCII suffix/separator; bounded host-only refusal details persist and are matched by actor/controller/tick/verb/reason until superseded. | Truncation, real API refusal → accepted action → completed result regression. |

The native check also caught Copy/OK accessible names creating generic Region
roles before the Button requirement ran. Component ordering now preserves Button
roles, and the native-observer keyboard regression checks both roles explicitly.
The fresh integration review additionally found that Escape could close both F7
and the covered village reader depending on input-system order. Notebook handling
now consumes only its handled Escape; a combined-plugin regression checks both
system orders, retained reader state, restored focus and unchanged sim hash.

## Boundaries retained

- No price, need-decay, currency, production recipe, registry or shipped preset
  tuning. Successful authoritative commands keep their integer state and IDs.
- Current KayKit art, palette, typeface and panel conventions remain. This was a
  bounded refinement, not a new visual direction or dashboard.
- Generic process and telemetry code names no content. Host wall time,
  credentials, process handles and UI state remain outside simulation hashes.
- Oracle files, hooks, architecture tests, generated Clippy policy, dependencies
  and global configuration are unchanged. No private CLI history is consulted.

## Validation and native evidence

Observed focused checks so far:

- Scheduler library: 14 passed, including descendant cleanup and unrelated
  sentinel preservation.
- Agent information: 6 passed; island integration: 24 passed; economy soaks:
  3 passed. Each of twelve seasonal fixtures requires all ten output stages.
- View library: 81 passed / 1 ignored. The final modal-role regression also
  passed separately with 80 other cases filtered.
- Python harness/process contracts: 14 passed. Every provider/RPC operation in
  the harness contracts is fake; containment cases launch only disposable Python
  processes. These do not count as authenticated continuation evidence.
- Focused Clippy passed with warnings denied.

The initial stricter seasonal fixture failed because endowed ingots had not
needed replenishment within 2,400 ticks. The
corrected fixture creates explicit demand, records every removal and uses a hard
14,400-tick cap inside the same season. No assertion or production balance was
weakened to obtain a pass.

All 15 PNGs from `target/playtest-native-258eeab564/npc-out/shots/` were opened.
This debug-profile batch showed the queued-control cue, concrete refusal, F7
without HUD underlay, 480-logical width, 125% scaling, death/new life and recovered
Diary. Its short 1x sample measured 19.936 ticks/s and about 60 FPS; it is not a
release-profile comparison or a long performance soak.

The actual launcher path used a new disposable settings/home directory at
`target/polish-launcher-7e26ab62`: Main → Explore → New → Island Village/human →
Start in window. Windows UI input verified prompt Tab/Enter, HUD Tab entry and
F7 from HUD focus. That fixture reached its independent lifetime limit and its
entire owned tree stopped; closing-F7 focus restoration remains covered by the
integrated view regression. Fast injected text chords did not select/paste the
long-name fixture reliably, so no physical text-entry success is claimed.

The final confirmation at `target/polish-short-ae0ff5db` produced five PNGs, all
opened. PNG headers confirmed 960×720 physical pixels (480×360 logical at 200%)
for the long-name/error, notebook and restored-HUD states. Action controls remained
visible while the explanatory body clipped/scrolled; F7 removed the underlay and
returned the gold focus ring to AI scheduler. The retained concrete refusal stayed
visible during accepted movement. The prompt capture showed the gold Copy focus.
Button roles are verified by the native-widget regression; the attempted final
Windows accessibility read occurred after the automatic fixture had closed, so
updated live accessibility-role confirmation and screen-reader use are not claimed.

Two earlier QA-fixture attempts are retained, not counted as short-viewport passes:
back-to-back reflected resize writes had left the original height in place, and a
newly created PNG was read before its header had been written. Waiting between
view-only resize events and checking the image dimensions fixed the fixture. The
maintained capture now also waits for the PNG terminal chunk, with a fake-file
regression. No extra UI redesign loop was performed.

All owned native trees stopped and no provider ran.
Only current captures and actual checks count; source review is not GUI or
screen-reader validation.

## Final verdict

**OFFLINE READY / LIVE SETUP BLOCKED**, not GO. The independent integration
review approved the source fixes with no remaining actionable P0/P1/P2 issues.
The full gate passed with process-local `NEXTEST_TEST_THREADS=4` (restored after
the command):

```text
cargo xtask verify
{"ok":true}
```

Exact final results: 1,348 nextest tests passed, 12 skipped, 4 leaky
classifications; doc tests 1 passed / 10 ignored; architecture 7 passed;
8 replays verified (1 session, 3 testbed, 4 village); 6 goldens, none pending;
Clippy no warnings; dependency advisories/bans/licenses/sources all passed.
Cargo-nextest and cargo-deny were available; no tools were skipped. Raw verdict:
`target/review-polish/full-verify.json`. All assertions and existing test profiles
remain unchanged. Leaky classifications are reported, not hidden or rebaselined.

Focused commands and logs are under `target/review-polish/`: scheduler tests 14;
the three island suites 6 / 24 / 3; view library 81 passed / 1 ignored;
Python contracts 14; focused Clippy with `-D warnings` passed. Maintained command
examples are in `docs/island-village-playtest.md`. The adjacent sanitized JSON
manifest records exact gate summaries and evidence scope.

## Remaining external gate

Authenticated exact-conversation continuation is still unverified. No Codex or
Claude conversation is started or resumed in this followup. The operator's longer
unaided economic playthrough remains separate. A new explicitly authorized smoke
is required for a GO decision; fake process contracts cannot supply it.
