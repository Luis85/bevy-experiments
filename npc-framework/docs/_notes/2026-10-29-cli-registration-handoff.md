# CLI identity handoff during game registration

> Historical evidence, recovered on 2026-11-20 from the leftover branch
> `audit/island-first-playtest-verified` (commits `63417af`, `dc303f7`, `bf48345`).
> It describes the testbed-hosted Island Village tier that ADR 0044 removed:
> `testbed_content/src/village`, `docs/island-village-playtest.md`,
> `docs/ai-village.md` and `scripts/playtest/live_smoke.py` no longer exist.
> Main took `63417af`'s code in `a3d6fdb` but not `dc303f7`'s owned process
> containment or `bf48345`'s CLI identity handoff; see
> [unported agent-supervision work](2026-11-20-unported-agent-supervision.md).

Base: `dc303f7`, 2026-10-29. This implements the request to ask an AI agent
for its own session ID when it registers, so the AI scheduler can target that
conversation. It does not run a real provider or infer any private session.

Registration and the generated handbook now ask for paired `cli_provider`
(`codex` or `claude`) and `cli_session_id` fields. IDs must be exact current CLI
conversation UUIDs supplied by the runtime. If unavailable, the agent can omit
both, report that limitation to the operator, and continue playing. Enabled
supervision returns a reminder when metadata is missing. Existing registration
without metadata remains compatible.

The identity is private host memory attached to the live registration. It is
excluded from authoritative hashes, snapshots, notebooks, Debug and public
acknowledgements. The registered MCP client preserves it across a transparent
host restart and redacts it from results/errors; revocation still forbids
automatic reconnection. A changed offered identity suppresses an older link.

`npcctl scheduler bind --from-registration` lets the operator link the offered
identity without copying its UUID. The active registration must have Direct
control of the selected actor. The operator still supplies the trusted workspace
and runner plus the original one-shot client PID or explicit finished-turn
confirmation. The host writes only its configured bindings file. Registration
and binding do not launch a process; the ordinary supervisor poll dispatches
eligible turns. Manual exact-UUID links remain supported.

The coordinator implemented/integrated the change and owned all builds and
processes. A separate subagent reviewed the complete source diff read-only and
approved it without actionable findings. No art, native UI, dependencies,
protected oracles or simulation encodings were changed.

Focused checks passed: 62 agent-I/O library tests, 13 registration tests, the
three scheduler tests present at that point, and the stateless CLI transport /
identity-redaction test. The full gate also covers the subsequent CLI parser
regression and generated-handbook assertions. Raw gate output is retained in
ignored `target/session-registration/verify.json`.

Final `cargo xtask verify` exited 0 with JSON `ok:true`. All seven steps passed:

- Format and Clippy with warnings denied.
- Nextest: 1,352 passed (3 slow, 3 leaky), 12 skipped. Doc tests: 1 passed,
  10 ignored. `NEXTEST_TEST_THREADS=4` applied only to this verification process,
  matching the previous gate's bounded Windows hook-test concurrency.
- Architecture: 7 passed.
- Replays: 8 verified (1 session, 3 testbed, 4 village).
- Goldens: 6 snapshots, none pending; no baselines changed.
- Dependency policy: advisories, bans, licenses and sources passed.

No verification tools were skipped. This is offline verification; the slow/leaky
test classifications do not establish GUI or authenticated-provider behavior.

The first-playtest readiness decision remains **OFFLINE READY / LIVE SETUP
BLOCKED**. This change improves setup but supplies no new authenticated provider
continuation evidence. The earlier Codex/Claude smoke limitations remain those
recorded in the first-playtest and followup reports. No real provider was called, no
private history was inspected, and no push, PR or merge was performed.
