# Player specialist — independent world and delivery cross-review

Reviewed 30 October 2026 after the initial implementation, while native source
was frozen for the other specialists' review. This is source/API/tooling review;
it does not claim a native candidate, extracted real release or Windows run.

## Supply careers and operator recovery

Reviewed `crates/terra-sandbox/src/settlement/{demand,economy,supply,commands}.rs`,
the supply branch of `guidance.rs`, their integration with native
`crates/terra-demo/src/frontier.rs`, and the world specialist's focused results.

No blocking issue found in the selected supply policy:

- New posting uses actual recipe/retail need and the largest currently fitting
  quantity from 1 through 12. The posting-only inventory operation is kept out of
  ordinary read-only demand classification.
- Accepted, unexpired work remains deliverable after demand changes. Its goods,
  reward and deadline are preserved. Expiry releases ownership before public
  maintenance; Return does not consume carried goods or award progress.
- Known obsolete public offers prepare the original issuer's exact credit before
  removal. Missing issuer or full wallet preserves the request and escrow. The
  contract-ID order makes competing refunds deterministic and retirement does
  not reuse old identities.
- Missing or position-replaced receivers are unknown. They are not evidence
  authorizing a refund or renewed promise. A still-needed request can wait for
  receiving capacity without losing its published terms.
- The additive `ObsoleteOffer` is consumed through the existing public readiness
  API. Native request labels display the same explanation and Accept availability
  uses that API. Authority prepares acceptance again. Guidance excludes a known
  obsolete public request but still prioritizes the player's accepted work.

The copy explains the condition, waiting/alternative work and protected money.
It does not promise an immediate refund when an issuer is unavailable or full.
The existing narrow inspector remains responsible for wrapping the explanation;
its actual candidate/final appearance is checked by the integrator.

One boundary remains deliberate: readiness is not a reservation. Other deliveries
can consume capacity after a request is published or accepted, so a player may
need to wait or return an accepted request. The implementation and API comments
do not imply otherwise. Missing local data is not treated as globally absent.

## Extracted desktop usability and failure behavior

Reviewed `scripts/verify_desktop_archive.py`, its integration with
`scripts/verify_desktop.py`, generated `START_HERE.txt`, the payload allowlist,
`.github/workflows/ci.yml` and `docs/DESKTOP_DELIVERY.md`.

No blocking issue found in the implemented archive check:

- CLI arguments distinguish passive inspection from explicitly requested
  `--smoke-help`. Payload execution occurs only after exact package admission.
- Central-directory count/size and compressed/uncompressed budgets are checked
  before extraction. Portable names, case aliases, roots, file/directory
  conflicts, file types and permissions are admitted as a complete inventory.
- Each file is copied exclusively into a fresh private temporary tree; the tool
  never delegates archive paths to `extractall`. The expected release/platform
  folder and exact byte inventory are checked before startup.
- Startup is bounded and its scope is stated. A subsequent exact-tree/mode check
  detects modified or added payloads. The original ZIP identity and digest are
  checked again. The temporary extraction is cleaned up.
- Failure results are printable JSON with an error and limited scope. A requested
  report uses a new path, preserves previous proof and retains failed results.
- Both configured CI platforms run fresh extraction and bounded startup before
  the desktop upload; evidence upload remains `always()`. Windows configuration
  is not represented as a completed Windows run.

### Independent focused execution

Ran from the repository root:

```sh
python3 -m unittest discover -s scripts -p test_verify_desktop_archive.py
```

Observed exit 0: **11 tests passed**, reported suite duration **0.147 seconds**.
Those fixtures exercise normal passive extraction, passive-before-smoke ordering,
corrupt payloads, namespace/root/type/mode rejection, bounded metadata/payloads,
post-start mutation and failure-report preservation. This focused execution
does not replace final verification of the actual delivered archive.

### Documentation integration request

At review time, `docs/DESKTOP_DELIVERY.md` still omitted the new archive CLI and
described CI upload as following acceptance and packaging. Requested the
integrator add the fresh extraction/startup prerequisite, exact command and
new-path failure-report workflow during the owned current-document pass.
This was communicated to the integrator; no shared current docs were edited by
this reviewer.

## Remaining release acceptance

The native full suite, candidate/final visual inspection, canonical source-bound
acceptance and fresh verification of the real packaged desktop remain with their
assigned owners. This review supplies independent source and focused fixture
evidence, without inferring unmeasured device, hardware or user qualification.
