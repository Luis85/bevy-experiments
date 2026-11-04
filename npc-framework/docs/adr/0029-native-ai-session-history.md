# ADR 0029: Native AI session history and output preferences

Status: accepted, 2026-11-01.

The separate AI client already owns Markdown run logs and JSON conversation
metrics. Players need to inspect them without leaving the game or manually
locating output files, and choose the private log folder in Settings.

Expose Sessions inside F10 controls and Logs & metrics from detached-world and
saved-agent menu entries. Preserve the existing native visual language. Provide
logs/metrics tabs, record and page navigation, refresh, live log following and
keyboard scrolling. Back/Escape returns to the previous controls; viewing never
starts inference, takes actor control, steps the world or changes its pause state.

Use one background reader per native view, cached bounded pages and generation
guards. Preserve the pending worker across navigation, discard old results and
old errors, and keep manual reading stable while new data arrives. Display
typed metrics with actual/requested model and effort separated, unknown cost
availability, reported cost basis and accounting coverage. Legacy Codex totals
remain explicitly unverified. Human-readable UTC labels use integer conversion.

The workspace owns a private local history owner ID and append-only JSONL index
of exact client output paths and local metrics links. This is independent of
provider conversation identity. Read only validated indexed files or compatible
workspace-local log/metrics/ledger files. Never scan shared external folders or
read provider history, authentication configuration or private client state.
Bound catalog/index reads and page full logs without deleting original files.

Add optional `ai_logs_dir` beside `ai_metrics_dir`, preserving old settings files
and helper saves. Validate absolute folder preferences before writes. Menu-launched
windows inherit an exact settings-file marker; each normal/fresh Start rereads
that file so saved changes apply without restarting the window. Nonempty native
preferences become explicit client arguments, overriding inherited environment.
Blank preferences retain client environment/default behavior. Existing clients
keep their current destinations; previous indexed outputs remain discoverable.

This remains host/UI data outside simulation state, hashes, snapshots and replay
records. Visual validation uses disposable paused worlds and synthetic session
records, with provider CLIs stopped throughout.
