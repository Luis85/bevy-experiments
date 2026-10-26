# Task 27 report: bevy_remote probe

Implemented probes/remote/src/main.rs (plus `serde_json.workspace = true` in probes/remote/Cargo.toml, approved as R32; Cargo.lock +1 line). Findings are in docs/research/bevy020-api-reference.md, section "Probe: bevy_remote".

Runs (`cargo run -p probe_remote`, dev GPU NVIDIA RTX 1000 Ada / Vulkan, window hidden):
- Run 1: `remote probe: OK ping=42 screenshot=11217 (non-black px 65536)`
- Run 2 (Notepad opened in front): same OK line. Caveat: the window is hidden by design, so no run with a real focused window was done.
- Delayed ping 700 ms: call 705 ms, main-world frame gap 702 ms (blocking handler stalls the main schedule). 2.5 s delay gives JSON-RPC error -32603 (timeout), no hang; late reply stays in channel (needs request ids).
- `cargo clippy -p probe_remote -- -D warnings`: clean (crate-level `#![expect(clippy::disallowed_types)]` for Instant).

Gate `cargo xtask gate task --track probes`:
```
PASS fmt (1.4s)
PASS clippy-drift (3.5s)
PASS clippy (138.7s)
PASS arch-check (4.9s)
PASS test-headless (1072.5s)
WARN test-client (21.5s): no test matched ...
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
WARN gate task took 1243s (budget 300s)
gate task: PASS in 1243s
```
Concerns: gate took 1243 s (over 300 s budget; test-headless 1072 s, likely cold/contended). Render sub-app BRP server also binds port 15703 (no setter found). Response is chunked; the probe's client extracts the JSON by brace span.

## Fix round 1
Changes (probes/remote/src/main.rs, API reference section):
1. Screenshot check now counts pixels differing from the corner (clear colour) pixel and requires 0 < n < total: 8649 of 65536. Doc wording updated.
2. Added a `default` argument (skips `WinitSettings::continuous()`); the probe measures the idle update rate for 4 s after the screenshot. Results (hidden window): continuous 172 and 180 fps, default 187 and 192 fps, so no throttling difference for a hidden window. A visible unfocused window is untested; the doc says so.
3. Added a watching method: it must be registered as `fl.watch+watch` (plain `fl.watch` fails with -32601 when requested with `+watch`). First SSE event observed. Other items (render variants, panics, catalog rejection, with_port, env var, port 15703) are labelled "source-read, untested".
4. The probe records that the handler runs on the `main()` thread (tested); the `RemoteLast` schedule is labelled source-read.
Minors: asserts -32603 on timeout; sends a second ping, which returned the stale reply 2 (not 11); startup loop retries only the TCP connect; a self-test thread panic now exits non-zero (`Disconnected`); doc headings softened.

Command and output: `cargo run -p probe_remote -- x` / `-- default` (x2 each): `remote probe: OK ping=42 screenshot=11217 (cube px 8649 of 65536)`, exit 0. `cargo clippy -p probe_remote -- -D warnings` clean.
Gate `cargo xtask gate task --track probes`: fmt, clippy-drift, clippy, arch-check, test-headless PASS; `gate task: PASS in 58s`.
