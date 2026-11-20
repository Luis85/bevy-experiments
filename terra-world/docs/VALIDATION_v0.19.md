# Validation — Terra World 0.19.0

Executed on 29 October 2026 in Linux x86_64 with `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

## Executed gates and exact test totals

Command: `python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.19-final-acceptance`

- **44 executed gates passed**; the command process returned **0**.
- **946 distinct Rust tests**, **1032 passing executions**; **0 failed**, **0 ignored** in the final run.
- Python desktop-packaging suite: **18 passed**, **0 skips**. These are separate from Rust totals.
- **369 implementation inputs** match their frozen pre-run hashes. Captured logs/reports also match.
- Exact accepted native executable SHA-256: `b86ad4bc44685780b55c6f3eb8f32f88af4330f6f055d9f7d8d71e54339fe704`.
- Save **1**, regional schema **1**, wire **10** / `terra-world/10`, asset schema **1** remain unchanged.

| Crate | Distinct passing tests |
|---|---:|
| terra-bevy | 151 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 184 |
| terra-history | 13 |
| terra-net | 122 |
| terra-persistence | 45 |
| terra-sandbox | 349 |
| terra-stream | 45 |

The gates cover domain/economy/ownership, persistence and real TCP/TLS, finite
warehouse/passenger careers, physical traffic/navigation, continuing guidance,
Bevy headless and render systems, the complete demo/launcher suite, managed and
complete local recovery, original model authoring/generators, source
inspect/export/install/verify and native build. The final regional workload uses
16 real TLS clients, 8 churn cycles, 400 active ticks per cycle, 1,024 cold overlay
regions and 4,096 cold resources. Its receipt records 4,840 fixed steps and exact
offline/resume/restart/backup checks. This is a finite loopback fixture, not WAN
capacity or an MMORPG service qualification.

## Acceptance record provenance

The runner printed `Verification passed: 946 distinct Rust tests` and returned
zero, but the persisted `gates.json` retained its intermediate `running` state
with all 44 successful gate entries. The cause is **unestablished**. The untouched
[original record](verification/v0.19-final-acceptance/gates.json) and its exact
[progress snapshot](verification/v0.19-final-acceptance/runner-progress-snapshot.json)
remain evidence; neither is presented as a completed runner-written receipt.

The release uses the clearly labeled
[independently reconstructed acceptance record](verification/v0.19-final-acceptance/accepted-gates.json).
The integrator reparsed every Rust-test identity from the actual captured logs
using Cargo metadata, reconciled all gate counters, matched the complete
946-identity manifest, verified all log/report hashes and all 369 source hashes,
and recovered the one actual compiler artifact and its executable digest. A
separate reviewer independently checked the same facts and preserved its own
copy of the original snapshot. See the
[republication review](verification/v0.19-review/acceptance-republication-review.md).
The exact one-off path-bound reconstruction procedure is retained and hashed as
an evidence artifact. It is not the normal release runner. Publication time is
explicit; the original runner finish timestamp is unavailable and was not
invented. The reported process exit/terminal summary is attributed to the
integrator separately from independently checked file facts.

No test execution or timing was invented or added during republication. The
first candidate `v0.19-acceptance` stopped while constructing the deliberately
malformed foreign-key test fixture. Disabling FK enforcement only on that
hostile external editor corrected construction; production admission was
unchanged. All 45 focused persistence tests then passed. The final run above
uses the corrected frozen source. Failed-candidate executions are not counted
as final success.

## Scoped regressions and product review

The final run includes all seven dormant settlement families through direct and
wrapped APIs, complete rejected-state conservation, explicit resume, moving and
stationary motion admission; five storage integrity/compatibility regressions;
ordinary/pretty/nested capability redaction with exact wire JSON round trips;
four real-system mesh completion regressions; and a twelve-screen launcher
UI-tree/wrapping/footer-action test. Python publication regressions require the
full canonical roster, strict integer counters and reconciled unique test IDs.

Six dedicated specialist reviews, implementation notes and peer cross-reviews
are under `verification/v0.19-review`. The
[integration review](verification/v0.19-review/integration-review.md),
[implementation plan](IMPLEMENTATION_PLAN_v0.19.md) and
[complete gap report](REVIEW_GAPS_v0.19.md) retain findings, corrections and scope.

## Native and delivery boundary

The frozen-source native candidate builds and its command-line help executes;
the final native gate matches the candidate's exact hash above. Fresh rendered
launcher QA was attempted but **unavailable**: the executor denied Unix-socket
binding and the existing display was inaccessible. **No game process or fresh
screenshot** resulted from that attempt. The
[native availability review](verification/v0.19-native/launcher-review.md) and
receipt record the actual scope and cleanup. UI-tree/action tests do not prove
pixel clipping, actual Tab/controller traversal or a create/play/save/reopen
journey. Historical 0.18 observations remain historical.

Desktop packaging requires the reconstructed complete acceptance record,
frozen source hashes, captured evidence and exact accepted binary. A curated
allowlist retains original runtime data/licenses and current guides. Package
byte/mode checks and the extracted command-line smoke are additional delivery
evidence; they do not qualify graphics or installation on another machine.
Implementation input hashes exclude Markdown/licenses; final archive manifests
record those delivered bytes separately. Local hashes do not authenticate a
publisher or protect against coordinated fabricated receipts.

Original assets, source fixtures and historical evidence are retained. The old
current Product review/Validation are copied byte-for-byte to their 0.18 names.
Source/archive manifests and final delivery receipts record actual preservation.

Windows execution, GPU/loading/residency tails, physical input/assistive access,
uncoached first/returning journeys, long economic balance, dense navigation,
production content/geography, community succession, content migration, trusted
updates, accounts/moderation, WAN/soaks and operational RPO/RTO remain explicit
acceptance work in the G01–G18 register.
