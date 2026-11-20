# Menu playtest diagnostics — 0.17 implementation and review

Review date: 2026-10-29. This work starts from the byte-identical accepted 0.16
baseline in the 0.17 working project. Historical evidence was read, not
relabelled as new execution or overwritten.

## Problem and agreed scope

The retained diagnostic writer already bounded samples, retained 16 successful
captures, published complete reports, preserved previous bytes on error, and
checked active save/database/session aliases. The menu could not enable it;
testers needed direct-play arguments. The UI also did not expose successful
publication or failed capture, and reports did not distinguish a deliberate
menu pause from a frozen world clock or pending storage.

The approved repair is default-off local capture from launcher Settings. The
menu forwards the existing direct-play options for
`<user-data>/diagnostics/latest.json` at a 10-second real-time cadence. The
backend never grants authority, changes source identity, saves a world, exports
an inventory or resume capability, collects typed text, retains raw game logs,
uploads a report, or automatically submits a bug.

Ownership: the diagnostics agent owns `performance.rs`, `telemetry_capture.rs`,
`output.rs`, `DIAGNOSTICS.md` and this review note. Root owns launcher preference,
menu, process-handoff and UI integration, plus `GameFlow::diagnostic_phase` and
the player playtest guide. Root coordinates build/native acceptance and release
packaging; this agent changed no gates, version files or package scripts.

## Implementation

- **One bounded destination.** `menu_report_path` returns
  `diagnostics/latest.json`; `MENU_INTERVAL_SECONDS` is 10. There is no directory
  of automatically accumulated sessions. The existing 16-sample, 64 KiB/sample
  and 2 MiB/report limits remain.
- **One writer.** A small validated owner-only Unix marker accompanies each
  report. The worker holds `File::try_lock` for its lifetime. Initial marker
  bytes are written and synchronized in an exclusively created temporary file,
  then published with a non-overwriting hard link while already locked. A
  process interrupted during marker preparation cannot publish an empty marker
  as a permanent lease. Marker existence is not ownership: the operating system
  releases the lock on process termination.
- **Final acknowledgement.** The worker releases its report lock before
  acknowledging its final publication. A new session can therefore start as
  soon as the supervisor returns to the menu, without depending on thread-drop
  scheduling. The fixed marker is not removed or replaced by a finishing
  writer, avoiding a second-inode lock race.
- **Path protection.** Both report and lock are checked against protected world
  paths, ancestors and database sidecars. Session-output preflight also guards
  the lock against the selected private multiplayer identity. Symlink marker
  paths, unrelated marker contents and protected hard-link aliases are rejected.
- **Observed status.** The worker acknowledges the actual published sequence
  and capture time. `CaptureStatus` distinguishes opt-in, queued work, successful
  publication, delay, skipped captures and failure. Sending a sample is never
  displayed as successful recording. A disconnected worker clears pending
  status instead of claiming work can still complete.
- **Menu return.** A bounded JSON string marked `TERRA_DIAGNOSTICS_ERROR ` names
  a final export failure separately from world checkpoint success. The root
  supervisor consumes this explicit marker; no raw error tail is persisted as
  part of a report. The existing final-export failure exit policy remains.
- **Session interpretation.** Samples add flow phase, input-blocking state,
  virtual-clock pause, saved-clock pause, time scale, queued checkpoint/restore,
  unsaved-change and connection booleans. Presentation records control mode and
  panel/text-edit-open flags without actual typed text or selected identities.
  The report includes the existing source identity plus political revision.
- **Read-only menu summary.** `read_summary` validates the bounded retained
  envelope and uses its last sample. It identifies capture time, session,
  running/exit status and committed checkpoint. Missing, stale or malformed
  data cannot become a healthy-session claim, and inspection creates no folder
  or report.

## Independent review corrections

The reliability reviewer identified that a stopped writer could leave a
previous outstanding-job count displayed as pending. The error path now clears
that count and a deterministic regression exercises a completed/disconnected
writer while preserving the earlier report. The player-journey reviewer
identified first-write failure copy that implied a complete report already
existed. The UI and periodic error message now say **any earlier complete
report** is retained.

## Executed verification

The root scheduled one shared Cargo slot. Command:

```sh
source /tmp/terra-build/environment.sh
cargo test --offline --locked -p terra-demo --bin terra-demo
```

Result: **179 passed, 0 failed, 0 ignored, 0 filtered out**, exit 0. Compilation
completed in 1m 43s and the test binary reported 1.82s execution. The full,
unabridged output is retained in
[`diagnostics-native-tests-017.log`](diagnostics-native-tests-017.log). No Rust
warnings or errors occurred in that output. This is a headless execution of the
native-demo test binary, not a native window interaction or hardware benchmark.

Relevant coverage includes:

| Contract | Executed evidence |
|---|---|
| Off creates no diagnostic files | `disabled_diagnostics_create_no_report_or_lock` |
| A menu pause still captures real-time evidence | `paused_running_app_retains_periodic_evidence_before_final_exit`, including separate virtual and saved-clock flags |
| No typed text or private resume capability export | Explicit private sentinel values in the paused-app regression, plus forbidden inventory/player/capability keys |
| Export failures remain visible and can recover | `periodic_export_failure_is_visible_and_recovers_only_after_publication` |
| A stopped worker does not claim pending publication | `stopped_diagnostic_worker_clears_pending_without_replacing_prior_evidence` |
| Second writer preserves the first report and prior history | `one_writer_owns_a_report_and_the_next_run_recovers_its_history` |
| Final handoff releases ownership immediately | `final_acknowledgement_releases_the_lock_before_a_new_session_starts` |
| Forced termination releases the lock | `forced_process_termination_releases_the_operating_system_lock`, which kills a real child test process and reopens the same report |
| Unrelated/private/world paths survive unchanged | Existing output protections plus `lock_paths_preserve_unrelated_files_and_protected_world_outputs`, `diagnostic_lock_symlinks_and_protected_hard_links_are_rejected`, and `diagnostic_lock_cannot_occupy_a_current_or_future_session_identity` |
| Report inspection remains bounded and read-only | `report_summary_is_bounded_read_only_and_uses_the_latest_retained_sample` |
| Publication retains previous successful report on failure | Existing failed-sample and bounded-history regressions |
| Final diagnostic status follows checkpoint failure | Existing `failed_final_checkpoint_is_recorded_as_an_error_exit` and final-window export tests |
| Root menu integration | Launcher opt-in argument, preserved preference and distinct failure-handoff regressions in the same 179-test run |

At this test checkpoint, SHA-256 values are:

| File | SHA-256 |
|---|---|
| `crates/terra-demo/src/performance.rs` | `e95669673d530d838b8d7d7ac39d3f94085d724c979b85507c370173bd387953` |
| `crates/terra-demo/src/telemetry_capture.rs` | `e74036a867f5b1649e33ac57bb99357c4338186ac5d6a372bb6f399171d179d9` |
| `crates/terra-demo/src/output.rs` | `3a319ea64df451115779a9e62f342c3aa068bd9067d55efdbb9b38f5b8bff52e` |
| `diagnostics-native-tests-017.log` | `5f19c677537cc5a940123d7214702a1a9c8f5fa21b1504ec2bac78b05cd2a043` |

These hashes describe this checkpoint. Final acceptance must bind the final
integrated source and executable separately, especially if further UI or
lifecycle fixes are made after this run.

## Limits and remaining evidence

The writer uses local-file lock and hard-link behavior. Unsupported filesystems
fail visibly and preserve prior evidence; network shares, hostile replacement of
filesystem ancestors and Windows execution are not qualified by this Linux test
run. Older binaries do not use the new lock and must not share its report path.
Temporary files interrupted by process termination can remain, just as with the
existing report publication; no arbitrary file cleanup is introduced.

The final requested export waits at the existing maintenance barrier. This is
bounded queued work, not a maximum storage-latency promise. A hard hang before a
new update cannot be diagnosed as healthy from the previous sample. A startup
failure can occur before any current-session sample exists. The captured time
is when the sample was prepared, not a later filesystem modification time.

Native Settings, 1100×700/150% text, pause/F12 status, final error return, and
packaged launcher collection still require the root's fresh native review.
There is no clipboard integration, folder-opening command, continuous trace,
crash dump, public telemetry service, remote retention policy or automatic bug
attachment in this change. A file-manager path and explicit copy workflow are
documented for testers. Agent-operated native checks are not uncoached human
playtest evidence.
