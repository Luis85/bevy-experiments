# 0.19 multiplayer, security, privacy and architecture review

Baseline: attached 0.18 source. Review date: 29 October 2026. This is a source/document review; no WAN, TLS deployment, physical-device or capacity qualification is implied.

## Reviewed scope

Read `docs/REVIEW_GAPS_v0.18.md`, `docs/MULTIPLAYER.md`, `docs/NETWORK_REPLICATION.md`, the capability and operator portions of `docs/OPERATIONS.md`, and the relevant protocol, client, server admission, replication, session, TLS, transport, prediction and native adapter code. Examined operator regression coverage.

## Concrete prioritized findings

| ID | Priority | Finding and evidence | Proposed closure and acceptance |
|---|---|---|---|
| NET-01 | P1 confidentiality | `crates/terra-net/src/protocol.rs:58` and `:99` derive `Debug` on protocol enums whose Hello and Welcome fields include the full bearer `resume_token`. Normal and alternate debug formatting therefore disclose the reusable player capability. This bypasses the deliberate no-Debug `SessionIdentity` policy in `session.rs`, and makes routine embedding logs and assertion failures unsafe even though the current CLI avoids printing tokens. No production log leak was observed; the exposure is in the public formatting contract. | Replace derived Debug with explicit redacted formatting for both enums. Keep variant identity and useful nonsecret fields. Test normal/pretty formatting directly and inside an aggregate; check Hello with Some/None and Welcome. Assert no token or token prefix appears and verify JSON serialization/deserialization still carries exactly the original capability. Keep protocol 10 and ALPN unchanged. |
| NET-02 | P2 SDK boundary follow-up | `NetClient::poll` validates only protocol and actor identity in Welcome before binding its replica. Native `apply_message` performs additional spawn/clock/capability validation; terminal client performs a different subset. Raw normalized `Snapshot`/`SandboxSnapshot` variants also pass through NetClient's catch-all branch. This leaves inconsistent validation responsibility for new embedders. Existing native validation limits the immediate player impact. | In a separate explicitly scoped protocol-admission change, centralize handshake validation and distinguish wire-only update variants from normalized game events, while preserving documented public API behavior. Add malicious-peer loopback cases and prove failed admission does not bind a replica or save an identity. Do not silently remove normalized public message variants in a privacy patch. |
| NET-03 | P1 conditional hosting acceptance | Current tests and conditioned fixtures are finite local workloads. TLS identity and per-peer budgets do not prove real WAN behavior, attack resistance, operating capacity or account recovery. The existing G13/G14/G18 boundaries remain accurate. | Keep public-host deployment gated on intended account/operator policy, target network/soak measurements, retention and operational recovery. Report these as open acceptance work; do not label a debug-redaction fix as public-service readiness. |

## Sound contracts retained

- Hello preflight checks protocol, source compatibility, suspended principals, duplicate live resumes and guest policy before complete-world hydration; the handler repeats those checks.
- Authority supplies player identity and trusted interaction position. Movement and gameplay have independent replay spaces. Movement expires, and sandbox outcomes wait behind the regional commit barrier.
- TLS verifies explicit CA and expected name and negotiates exact ALPN before sending capability-bearing Hello. There is no plaintext fallback after TLS failure.
- Input, frame, queue, poll, snapshot and saved-principal bounds are explicit. Backpressure defers optional view baselines and retains reliable command batches.
- Recipient validation and private view filtering cover owner wallets, cargo, assignments and companies. Operator receipts and audit entries omit capabilities; SessionIdentity has no Debug implementation.
- Recovery/distribution, enrollment, account recovery, distributed authority and deployment capacity remain separate host responsibilities. Adding any requires a product policy and new evidence, not a claim based on reference limits.

## Recommended bounded implementation

Dispatch NET-01 to the network specialist, with ownership of `crates/terra-net/src/protocol.rs`, small protocol privacy tests and this review's final implementation note. Root integrates a short capability-logging note into current operational/release docs if desired. No wire, save, regional schema or asset migration is required.

Required validation: formatting/JSON regression tests, existing protocol unit tests and terra-net tests with normal features and `--no-default-features` when available. Root coordinates all compilation because the shared target and disk are constrained. Retain exact executed command receipts and distinguish not-run checks. Cross-review the final implementation for accidental redaction of actual wire bytes or loss of enum matching behavior.
