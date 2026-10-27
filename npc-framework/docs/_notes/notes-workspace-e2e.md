# Notes/workspace/actors/worlds/map end-to-end evidence

Date: 2026-10-27. Plan Task 18. Workspace: `C:\Projects\bevy-experiments\npc-framework`.

## Result

The real release-binary probe reports **`ok:true`** in
`target/phase-f-e2e-fbd42d058b66/summary.json`. The controller used the actual generated MCP configuration
and agent working directory. Its agent routine used MCP tools only; developer lifecycle/notes/UI calls were
separate. Claude Code is not installed, so this uses the plan's automated-agent fallback.

Command: `python target/phase-f-e2e.py` (local ignored verification driver). Binaries were the five-app H
release artifacts on main, merge `dd645c3`, subsequently documented at `2c8889f`.

| Check | Observed result |
|---|---|
| New rover, detached | Real `npcctl serve`; generated handbook, settings and registered MCP command used |
| Registration/control | Register, enter the assigned robot, take control; no identity selectors in registered tools |
| Agent story | Add/update/list landmarks, reject stale update, move by landmark ID, write diary/memory/proposals |
| Developer memory | `npcctl notes add` writes alongside the agent's memory |
| Open native | Detached stops/saves; native resumes with exact same port and actor position |
| Fog | Raw explored bits retained: 336 explored cells at handoff; native fog and map labels render |
| F7 | Actual Diary/Memory/Proposals/Map buttons activated; all expected entries found in the live UI and captured |
| Rotation | Old session revoked; explicit registration with current handbook succeeds; no secret response echoes |
| Close → Yes | Actual WindowCloseRequested message opens real dialog; actual Yes Activate invokes production handler |
| Keep detached | Native exits; announced child executable, run.json, endpoint and process HANDLE verified on same port |
| Continue | Same MCP process reconnects; actor position, notebooks and landmarks persist through both moves |
| Stop | `session/stop` saves exact final tick 204 (initial detached save tick 40), removes run.json and exits |
| Cleanup | Every driver-owned Popen and verified detached process exited; user's unrelated MCP process was untouched |

No extra production hook or simulated close implementation was added. Existing Bevy BRP reflection drove
the same UI observers as normal interaction. Entity values stayed lossless Python JSON integers, distinct
from simulation SimIds.

## UI procedure and visual review

Primary window query:

```json
{"method":"world.query","params":{"data":{"components":[]},"filter":{"with":["bevy_window::window::PrimaryWindow"]},"strict":true}}
```

F7 queries reflected `bevy_ui::widget::text::Text` plus `bevy_ecs::hierarchy::ChildOf`, scoped beneath the
named `explorer notes panel`. The exact tab label identifies its parent button. `world.trigger_event` sends
`bevy_ui_widgets::Activate` with that button entity. The probe waits for the expected live note titles before
capturing. Closing sends `world.write_message` for `bevy_window::event::WindowCloseRequested`, waits for the
named `close dialog Yes`, then triggers its Activate event once. A lost acknowledgement is never retried
blindly; independently verified handover is required.

All five screenshots under `target/phase-f-e2e-fbd42d058b66/npc-out/shots/` were inspected by both the worker
and controller:

- `phase-f-f7-diary.png`: diary title/body and selected tab readable.
- `phase-f-f7-memory.png`: agent Navigation entry and Developer entry both visible.
- `phase-f-f7-proposals.png`: proposal text, new status and player status controls visible.
- `phase-f-f7-map.png`: all three landmarks/stamps and separated marker labels, including black unexplored fog.
- `phase-f-close-dialog.png`: real question, explanatory text and Yes/No/Cancel fit without clipping.

The sanitized runtime log includes a brief startup/inspection-period profiler warning (perception p95
81.3 µs, fog 59.0 µs versus 55 µs budgets). This run is functional/visual evidence, not a steady-state performance
pass. The final fix verification rechecks representative release performance; no budget is increased.

## Coverage boundaries

- Claude Code's own trust screen and enforcement of its generated permissions need a human with that product;
  the fallback verified the generated settings structurally and kept its scripted agent to MCP calls.
- Physical keyboard, mouse, clipboard and OS titlebar input were not sent, as required by the plan. Synthetic
  widget tests and real reflected UI observers cover the corresponding application paths; OS integration
  checks remain the intentionally ignored/manual family.
- Existing untracked agent folders, tokens and saves were not committed. Probe logs are sanitized and fixtures
  stay under ignored `target/`.

## Gate and final review

Task 18 documentation and full F verification are complete. `target/verify-phase-f.json` reports
**`"ok":true`**: 1,217 passed, 12 skipped, three reported leaky; doc 1 passed/10 ignored; architecture 7/7;
eight replays; six unchanged snapshots; fmt/Clippy/deny passed, no optional tools skipped. Local process settings
were eight build jobs/two test workers, with no filtered or retried full-gate tests. The local merge and
five-app build precede the final review refresh and single bounded source-fix wave. Queued findings remain
in the progress ledger; this document does not declare those defects fixed or the whole plan finished yet.
