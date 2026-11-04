# Real Codex native-session playtest — 2026-11-01

Result: **PASS for live in-game logs, live metrics and the next-session log-folder
preference.** This used the installed, subscription-authenticated Codex CLI through
the game's native Start Codex / New AI session actions and dedicated game client.
No synthetic metrics or transcripts were inserted. No provider history, private
provider state or raw provider process arguments were read.

## Actual runs

Codex CLI 0.145.0 reported the proven cumulative usage scope. The dedicated client
selected `gpt-5.6-sol` with requested effort `high` from the account's advertised
default and CLI effort configuration. Actual execution model/effort were not
reported, so those fields correctly remained unavailable. Reported dollar cost
was also unavailable, not a zero-dollar bill.

| Native client run | Completed turns | Input tokens | Output tokens | Cached input |
|---|---:|---:|---:|---:|
| Start Codex | 1 | 397,086 | 1,250 | 352,128 |
| New AI session | 1 | 266,466 | 1,994 | 235,904 |

The disposable game supplied a bounded engineering objective automatically: enter
the world, observe, speak, write a physical diary page and add a notebook note,
observe again and finish. The saved world contains **two matching physical diary
pages**, and the agent folder contains **two notebook notes**. This is evidence of
real game-tool interaction, not an unaided survival benchmark.

Each client run was limited by a temporary world policy to one dispatch per hour,
with a 240-second turn limit, independent first-failure/wall guards and cooperative
native Stop. Exactly two model turns were used; no failed provider turn was retried.

## Live in-game verification

- Kept F10 Sessions → Session logs open during the first real turn without
  pressing Refresh. Observed **19 native body updates**. The Markdown file grew
  from 782 to 164,538 bytes while the client was still running. Timestamped
  native-body hashes and concurrent file sizes are retained in the summary.
- Kept the second run's Metrics view open from its unfinished state through
  completion, without pressing Refresh. It changed from zero completed turns and
  unavailable usage to **one completed turn, 266,466 input and 1,994 output tokens**.
- Native token values matched the actual exported per-session JSON in both runs.
  The two fresh AI conversations have distinct local metrics IDs.
- Opened all five captured native screenshots. Live/finished log content and
  before/after metrics are readable; actual/unavailable metadata and requested
  settings are distinguished correctly.

## Settings and next-session destination

The test used its own `--settings` file, not the user's normal settings. The first
run wrote one Markdown log under the disposable fixture's `logs-before` folder.

In the real Settings window, navigated to **AI logs folder**, entered
`c:/projects/qa-952948`, and activated **Save**. Confirmed the UI's saved feedback
and the persisted preference. The agents and JSON metrics folders stayed unchanged.
The next native New AI session wrote **one log in the new folder**, left exactly
one log in the original folder, and preserved the original log's SHA-256 unchanged.
Session JSON continued to use the original metrics folder.

Computer Use's direct Value action was unsupported by this custom text control.
Its clipboard shortcut also did not reliably paste under automation. The correct
path was entered with direct native keystrokes using the active German keyboard
layout, then saved through the actual button. This confirms folder persistence;
it does not establish reliable UI Automation Value/Paste support.

The first harness waited for desktop availability and reached its Settings wait
limit after completing the first paid turn. It stopped all owned resources and
preserved that evidence. After the user made the desktop available, Settings was
saved and the same disposable game was resumed for the remaining fresh client
session. Thus this verifies the next client session's destination, including game
resume; it is not a separate live-setting reload test in one uninterrupted game
process. Earlier focused launch tests cover rereading the same settings file.

## Evidence and cleanup

Fixture: `target/real-native-codex-95294857d9b4/`.

- `summary-final.json`: authoritative `ok:true` report, both actual usage records,
  live-update hashes, folder checks and `owned_resources_stopped:true`.
- `settings-ui-proof.json`: native Save result and unchanged other folders.
- `game-artifact-proof.json`: two physical diary pages and saved tick 2,170.
- `evidence-sha256.json`: integrity hashes for the report, configuration, actual
  metrics and both Markdown files.
- `npc-out/shots/session-1-log-live.png`, `session-1-log-finished.png` and
  `session-1-metrics-completed.png`.
- `npc-out-resumed/shots/session-2-metrics-running.png` and
  `session-2-metrics-completed.png`.

The earlier setup-only `real-native-codex-aa6837d0061e` failed while waiting for
the game HTTP server, before a client/model turn started; it was cleaned up.
The first phase's timeout report remains in `summary.json`; it is not overwritten
or presented as a passing full run. The resumed final report is separate.

All owned game, menu and dedicated-client processes were stopped. Existing Codex
processes and global login/configuration were untouched. No maintained Rust/UI
source, dependency, protected oracle or application binary was changed or rebuilt
for this runtime playtest. The pre-existing 1,585-test green gate remains the
application baseline; concurrent NPC economy source work is outside this result.
