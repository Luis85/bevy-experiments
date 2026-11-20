# 0.20 network, privacy and service review / implementation plan

Baseline: 0.19 source, reviewed 29 October 2026. Source review found NET-02 still
open: NetClient binds a Welcome after only protocol/player checks, while native
and terminal adapters enforce different additional rules. Requested source
identity is sent but not retained/checked by the SDK. Direct wire Snapshot and
SandboxSnapshot bypass revisioned reconstruction through the catch-all branch.

Selected scope: a shared Welcome validator, SDK requested-source admission before
replica binding, and rejection of normalized-only snapshot variants on the wire.
Native adapter uses the shared validator; normalized poll output remains the
existing Snapshot/SandboxSnapshot API. Protocol 10, ALPN and saves unchanged.

Acceptance: malicious loopback peers send malformed scalar/capability/source
handshakes, followed by a valid one proving no premature bind; direct raw actor
and sandbox snapshots reject before/after handshake; actual revisioned update
normalization and all existing resync tests remain. Test pure validator NaN/inf
cases separately because JSON cannot carry nonfinite floats. Full network/TLS
suite is centrally executed, not inferred from inspection.

Public enrollment/moderation/audit, deployment/WAN/soaks, account recovery and
capacity remain G13/G14/G18 work. Structural protocol admission does not establish
a trustworthy server, source truth, business catalog policy or publisher trust.

## Implemented and verification status

Added the shared method in protocol.rs, requested-manifest retention/admission in
client.rs, normalized-only raw snapshot rejection, and shared native Welcome
admission in terra-bevy/network.rs. Added three real loopback SDK tests covering
12 malformed Welcome mutations with subsequent valid retry, source mismatch with
same-data/different-save-name success and repeated rejection, and both raw
snapshot variants before/after Welcome. A pure method test covers NaN/infinite
clocks/scales, uppercase valid token and wrong variant. Existing revisioned
replication/resync tests provide positive event normalization coverage.

Pinned rustfmt completed. No Cargo command was executed by this owner; root owns
actual compilation and final acceptance. Suggested focused commands:
`cargo test -p terra-net --lib --locked --offline` and
`cargo test -p terra-net --test delta_replication --locked --offline`;
then native adapter/headless tests and full TCP/TLS suite.

The change does not centralize every game-policy check or validate arbitrary
application outcomes as a new authority layer. Those remain explicit embedding
responsibilities; no account, moderation or deployment-scale closure is claimed.

## Integration fixture correction and executed checks

The first final candidate exposed two old `frontier_validation` mock peers that
sent normalized SandboxSnapshot directly on the socket. Admission was not
relaxed. The fixture now uses the public SandboxUpdate/encode_frame codec with
explicit full keyframe revisions. The positive case waits for actual normalized
keyframe receipt. Each malformed ecology/private-progress case first installs a
valid baseline, then sends the invalid second keyframe and asserts its exact
semantic failure and complete replica preservation. Thus failure cannot pass
merely by hitting the wrong wire-variant guard.

Audited all Bevy integration peers, network integration framing sites and demo
source for the same assumption: other Snapshot/SandboxSnapshot occurrences are
normalized output assertions or direct adapter tests, while peer sends are
revisioned envelopes (or intentional malformed/legacy ClientMessage tests).
Only `crates/terra-bevy/tests/frontier_validation.rs` required correction.

Executed with pinned toolchain, offline/locked and CARGO_INCREMENTAL=0:

- `cargo test -p terra-bevy --no-default-features --tests --locked --offline -- --skip default_munich_procedural_settlement_produces_and_physically_delivers`: **104 passed**, zero failed/ignored. Only the integrator-authorized unchanged long procedural physics case was filtered; final acceptance runs it normally.
- `cargo test -p terra-bevy --lib --locked --offline`: **121 passed**, zero failed/ignored/filtered.

Logs: `network-bevy-integration.log`, `network-render-integration.log`.
The demo fixture was not changed and its previously passed preflight was not
repeated by this owner. Root owns the final unfiltered frozen-input suite.
