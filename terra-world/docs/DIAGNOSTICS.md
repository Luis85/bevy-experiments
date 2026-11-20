# Retained native diagnostics

Diagnostics are an explicit local observation stream. They contain bounded
timing summaries, source identity, rendering and asset-admission counts, network
counters and checkpoint status. They never grant authority, save a world or
export player inventories, account balances or multiplayer resume capabilities.

## Capture from the native menu

Open **Settings → Playtest diagnostics**, turn **Local playtest reports** On,
then choose **Save defaults**. Reports are Off unless you opt in. The choice
applies when you next open a world or join a host from the menu, including an
existing world. Changing it does not alter that world's source, progress or
authority. Turning reports Off keeps any earlier report.

The menu displays the exact report path and a **Refresh report status** action.
The report is `diagnostics/latest.json` under the current user-data folder.
Packaged launchers use the following locations:

| Launch method | Report location |
|---|---|
| Windows desktop launcher | `%LOCALAPPDATA%\TerraWorld\diagnostics\latest.json` |
| Linux desktop launcher | `${XDG_DATA_HOME:-$HOME/.local/share}/terra-world/diagnostics/latest.json` |
| Source menu without `--user-data` | `saves/diagnostics/latest.json` under the project folder |
| Custom `--user-data FOLDER` | `FOLDER/diagnostics/latest.json` |

Use the displayed path in your operating system's file manager to copy the
report. The menu does not open a folder or copy the path to the system
clipboard. The file appears only after a successful capture. Reports stay on
this computer; there is no upload service or automatic bug submission. Review
the file before sharing it.

Menu capture attempts its first sample during the world's first running update,
then at most once every **10 real seconds**, and attempts a final sample on
exit. The pause menu and F12 performance pane show the current report
destination, successful capture age, queued publication and export errors.
Enabled capture is not the same as a successfully written report. A startup
failure before the first update can leave no new report; an older report keeps
its earlier session identifier and capture time.

The fixed path retains the latest **16 successful captures across sessions**,
not a separate file per world or a complete playtest recording. Copy it soon
after reproducing a problem, before a long new session rotates the useful
samples. At the normal menu cadence, a full uninterrupted history spans roughly
the most recent two and a half minutes. Keep your steps, expected and actual
result, and relevant screenshots alongside that copy; see the
[playtest guide](PLAYTESTING.md).

## Direct-play capture

From the extracted project or desktop folder:

```sh
./terra-demo --play --telemetry diagnostics/session.json --telemetry-interval-seconds 10
```

For a saved menu world, substitute `--world-profile path/to/world.json` for
`--play`. Use the actual executable path when building from source. Windows
uses `terra-demo.exe` with the same arguments. These remain direct-play options;
the `--menu` launcher uses its Settings choice and does not accept these output
arguments itself.

`--telemetry FILE` alone records the final exit, preserving the earlier opt-in
behavior. Adding `--telemetry-interval-seconds N` records the first running
update and then at most once per **1–3,600 real seconds**. The cadence continues
while the virtual game clock is paused. It does not catch up missed intervals.
Normal exit adds a final sample after the regional checkpoint barrier.

The latest capture stays at the top level so existing readers can still find
`timings`, `population`, `asset_pack` and `regional_persistence`. The
`history.snapshots` array keeps the **16 most recent successful captures**,
including earlier runs at that file path. Each capture has a run identifier,
sequence, Unix timestamp, elapsed real time and an explicit status:

| Status | Meaning |
|---|---|
| `running` | Captured during an active process. It does not show that the process later exited normally. |
| `graceful_exit` | The application requested successful exit and reached final diagnostic export. Check the captured world and storage error fields too. |
| `exit_error` | The application already reported a failure when the final sample was prepared. |

After a crash or forced termination, the last complete periodic sample remains.
Its time and `running` status show where evidence stopped. A stale or missing
capture is **unknown health**, not proof that the game or storage was healthy.
This is sampled incident context, not a crash dump or continuous trace.

## Bounded work and failure recovery

Periodic writes run on one diagnostic worker, with one in-flight write and one
queued sample. If that queue is busy, the new sample is skipped and
`capture.skipped_busy_captures` records the omission. No unbounded queue builds
up behind slow storage. Final exit waits for its own final publication.

Individual samples are limited to **64 KiB** and the complete file to **2 MiB**.
Publication writes and synchronizes a new same-directory temporary file before
replacing the report. Failed samples preserve the last successful report;
subsequent successful captures count earlier write failures. Errors are also
printed to the process log. Optional periodic failure does not stop gameplay;
a failed requested final export makes the process exit unsuccessfully.

One current writer owns each diagnostic path. An operating-system file lock
prevents a second writer from replacing that session's report. The small,
owner-only lock marker remains beside the report; its existence is not a stale
lease. The lock is released when the writer finishes or its process is
terminated. The final acknowledgement releases it before another menu session
can start. A second session reports the conflict and continues gameplay, but
cannot claim diagnostic success while the first owns the path. Keep the marker
in place; do not delete or replace it while a session is active. Older Terra
executables do not participate in this lock, so do not run an older writer
against the same path.

The marker is published complete without replacing another file. Filesystems
that do not support the required local file lock or hard-link publication report
an error and retain earlier evidence. Unrelated marker files and symbolic links
are rejected. New lock and report files use owner-only Unix permissions;
Windows permissions follow the containing directory's ACLs. Windows execution
and filesystem behavior still need their own acceptance evidence.

Reusing a valid schema-one Terra diagnostic
file preserves its bounded history. An unrelated file, older export without the
retention envelope, malformed history, symlink path or active save/database
alias is rejected; choose a new filename. The report and its lock marker cannot
occupy the active world or multiplayer identity path, including protected
ancestors and SQLite sidecars.

Keep diagnostic destinations on a local trusted filesystem. The checks do not
make concurrent administrator replacement of filesystem ancestors safe, provide
offsite retention, encrypt reports, or guarantee persistence across hardware
failure. Reports may include local error paths and source identifiers; review
them before sharing. They contain no private world export.

## Triage a stalled session

1. Check the last capture's time, status and run identifier. A new run must not
   be mistaken for a continuation of the old timing window.
2. Read `last_world_error`, `last_simulation_error`, and
   `regional_persistence.last_error`. Distinguish a pending checkpoint from a
   failed one. The recorded generation is the committed checkpoint, not every
   visible action.
3. Check the source identity and asset fallback/loading counts. Missing prepared
   coverage and procedural fallback are different conditions.
4. Compare retained frame and CPU-stage summaries with checkpoint submission and
   worker timings. Frame intervals are not GPU timestamps, and a multiplayer
   client's counts are its received view rather than the host's whole world.
5. Preserve the report and process log separately before a long new session
   rotates older samples. For world recovery, use a verified
   [recovery bundle](RECOVERY_BUNDLES.md); diagnostics cannot restore state.

### Distinguish a pause from missing evidence

Each new capture includes only presentation and control-state flags under
`session_state`, plus the existing checkpoint status. It does not include the
text a player typed, a character name, owner obligations or inventory contents.

| Observation | Interpretation |
|---|---|
| `phase` is `paused` and `virtual_time_paused` is true | The local session overlay has paused virtual time. This does not set the saved world clock's pause flag. |
| `world_clock_paused` is true or `world_time_scale` is zero | The world clock is deliberately not advancing. Use the other state fields to understand whether local input or physical simulation can continue. |
| `checkpoint_queued` or `regional_persistence.pending` is true | Checkpoint work has been requested or is running. It is not a durable success receipt. |
| `regional_persistence.blocked` is true | Storage has failed or blocked authority. Read its error and use the in-game retry or restore choices. |
| A multiplayer menu has blocked input while virtual time remains unpaused | The client overlay does not pause its host. Check `network_connected` separately. |
| Capture time stops advancing | The reason is unknown: a stalled update, terminated process or diagnostic-storage problem can all leave old evidence. File existence does not prove health. |

The captured `phase` is one of `loading`, `playing`, `paused` or `saving` when
the native flow is present. `presentation` also records the control mode and
whether the world, inspector, performance or text-edit interface was open,
without its entered text or selection identities. Captured source revisions
remain the authoritative source identifiers; presentation flags never grant
permission to change them.

A successful world checkpoint and a successful diagnostic export are separate
results. The menu supervisor names a final diagnostic-export failure explicitly;
it does not infer that world storage failed merely because the report could not
be replaced. A report marked `graceful_exit` means the final capture was reached
after a successful exit request. Read its recorded world/storage errors before
making a claim about saved progress.

The release [validation record](VALIDATION.md) distinguishes deterministic writer
tests, native forced-termination observations and platform limits. External
alerts, deployment retention policy, crash dumps, operator audit retention and
hardware performance targets remain separate work.
