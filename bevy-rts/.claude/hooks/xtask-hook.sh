#!/usr/bin/env bash
# Runs `xtask hook <args>` for a Claude Code hook (ruling R36, ADR-0013).
# - Uses the copy in target/hooks/ (refreshed by every `cargo xtask gate` and by `cargo xtask install-hooks`),
#   so a hook never waits on the build lock or on xtask compiling; falls back to `cargo xtask` only when the
#   copy is missing.
# - Fails closed: any failure (missing binary, crash, internal error) exits 2, which blocks the tool call or stop.
# The hook JSON arrives on stdin and is passed through unchanged.
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 2
bin="target/hooks/xtask"
if [ -f "$bin.exe" ]; then bin="$bin.exe"; fi
if [ -f "$bin" ]; then
  "$bin" hook "$@" || exit 2
else
  cargo xtask hook "$@" || exit 2
fi
