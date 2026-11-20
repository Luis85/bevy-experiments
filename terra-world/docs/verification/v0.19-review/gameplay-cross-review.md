# Gameplay owner's independent cross-review — 0.19

Reviewed 29 October 2026. Scope: the network privacy implementation in `crates/terra-net/src/protocol.rs`, and the delivery changes in `scripts/release_contract.py`, `scripts/package_desktop.py`, `scripts/verify.py` and `scripts/test_package_desktop.py`. Source/tests were inspected without compiling. The final integration receipt owns executed acceptance.

## Network privacy

No material blocker found in the implemented scope.

- Client/server serde derives and enum tags/fields remain intact; the wire must retain the credential. Replacing only Debug does not require a protocol bump.
- Both custom matches cover every variant and every currently named field. New variants or fields force an explicit compiler-visible match update rather than silently inheriting credential printing.
- Hello hides the token while retaining its Some/None state. Welcome never formats the underlying credential and retains actor, world, timing and sequence context. Alternate/nested formatting uses the same redaction.
- The regression checks full-token and prefix absence, the redaction marker, guest None, useful actor context, exact Hello/Welcome JSON and both deserialization round trips. These are meaningful leakage/compatibility checks.
- The implementation note correctly limits protection to the typed credential field. Arbitrary user-provided names/errors can contain text; raw JSON intentionally contains the token. This is neither blanket log sanitization nor public account-system qualification.

## Delivery evidence

No material blocker found in the implemented scope.

- A shared immutable gate-name roster is checked by both runner and packager. Exact order rejects omitted, duplicate, reordered and unknown gate identities before publication. Runner commands remain unchanged.
- The packager requires hashed `distinct-tests.json`, validates exact record keys and nonempty strings, rejects duplicate identity triples, and reconciles the total plus per-crate counts. Exact Python integer checks correctly reject booleans posing as counts.
- Existing path-containment, portable namespace, raw evidence-name collision, accepted-source/binary/platform and archive-integrity checks remain intact. Evidence is admitted before the archive is published.
- The successful unit fixture now supplies the full roster and hashed identities. Rejection tests cover native-only/truncated/reordered/duplicate/unknown rosters; missing identity evidence; empty/malformed/duplicate identities; falsified totals, crate counters and boolean counters. Publication absence is asserted on failure.
- `release_contract.py` is a normal scripts input and is included by the runner's Python source freezing. Adding this dependency does not depend on platform-specific runtime behavior.

## Remaining boundaries and maintenance notes

1. These checks establish consistency of a local receipt, not provenance. A malicious author who fabricates gate logs and a matching identity manifest is not authenticated by self-consistent hashes. Do not describe the new checks as proof that arbitrary names were executed. The current source and review descriptions correctly state structural/local acceptance and retain trusted publisher delivery as G11 work.
2. Changing the runner roster requires updating the shared contract in the same change; the runner's own validation catches drift. Strict execution order is intentional and should remain documented when future optional/platform gates are introduced.
3. Native Windows execution, device qualification, uncoached playtesting and deployment readiness remain independent acceptance tasks. Neither custom Debug nor packaging unit fixtures establish those product outcomes.

No peer source files were changed by this review.
