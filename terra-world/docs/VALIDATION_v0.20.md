# Validation — Terra World 0.20.0

Executed on 29 October 2026 in Linux x86_64 using `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

## Direct terminal acceptance

Command: `python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.20-release-acceptance`
(use the actual recorded per-gate commands in the receipt for exact arguments).

**44 gates passed**, **961 distinct Rust tests**, **1050 passing
Rust test executions**, zero failed/ignored in the accepted run. The separate
Python delivery suite passed **28 tests**, with **0 skips**; those
counts are not added to Rust totals. **370 frozen implementation inputs**,
all captured reports and logs match their accepted hashes.

The authority is the direct runner-written
[completed receipt](verification/v0.20-release-acceptance/completed-gates.json), with the
byte-identical legacy `gates.json` alias, source manifest, exact distinct-test
manifest and command logs beside it. Terminal publication is exclusive and read
back before a success announcement. **No receipt reconstruction was used for
0.20.** The 0.19 reconstructed record and its provenance remain historical.

| Crate | Distinct passing tests |
|---|---:|
| terra-bevy | 157 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 184 |
| terra-history | 13 |
| terra-net | 126 |
| terra-persistence | 48 |
| terra-sandbox | 351 |
| terra-stream | 45 |

Accepted executable SHA-256:
`f299f9af1bc2b3669a50de620387a41cd4aa3e9e2bc90648fd2705e7bcbd02f6`.
Save 1, regional schema 1, protocol 10/`terra-world/10`, and asset schema 1 remain.

## Candidate correction and retained evidence

Earlier failed candidates are retained separately: `v0.20-final-acceptance`, `v0.20-acceptance`. Their failures/executions are not counted as final passing acceptance. The restore regression fixtures were corrected to compare against actual initial regional revision/generation. Legacy ecology mock peers now send valid protocol 10 revisioned keyframes, install a valid baseline before each malicious update, and retain exact semantic rejection reasons and complete-state conservation. No production admission was weakened. See the specialist correction notes.

## Executed coverage

The canonical gates cover domain/gameplay/economy/ownership; persistence and
actual TCP/TLS; finite warehouse and one-hour passenger careers; physical traffic
and navigation; continuing guidance; Bevy headless/render and complete demo
suites; managed/full local recovery; original asset generators/authoring;
prepared-source inspect/export/install/verify; regional-session workloads and
native build. Read each finite measurement's workload and limits; loopback
correctness does not establish WAN capacity or an MMORPG deployment.

Selected 0.20 regressions exercise exact F9/Load target review and stale queued
consent; bounded mesh disconnect/poison/backoff/recovery with retained geometry;
shared Welcome/requested-source admission and raw wire snapshot rejection;
expired stale supply offers preserving complete state; full-residency hydration
integrity with valid cache reuse/live-edit preservation; and final-receipt
publication/admission including missing/stale completion evidence.

## Reviews and polishing

Six fresh specialists reviewed before implementing their bounded scopes. Peer
cross-review and corrections are preserved under `verification/v0.20-review`.
A queued restore confirmation initially lacked the unique review identity; the
final actions retain and check their review token. Release review also required
the packager to bind a passed progress alias to its actual terminal sidecar;
missing/different completion now rejects. The final test suite, rather than
source inspection alone, establishes regression outcomes. Cross-review files:
`gameplay-cross-review.md`, `network-cross-review.md`, `release-cross-review.md`, `render-cross-review.md`, `storage-cross-review.md`.

The [implementation plan](IMPLEMENTATION_PLAN_v0.20.md) and
[full gap report](REVIEW_GAPS_v0.20.md) retain selected scopes and all G01–G18
remaining acceptance conditions. Original assets/data/history remain protected;
previous current review and validation are archived byte-for-byte as 0.19.

## Native, platform and delivery limits

No fresh windowed gameplay/launcher journey or screenshot was executed for 0.20.
The 0.19 attempt was blocked before launch by the observed Unix-socket policy;
no bypass or repeat attempt is counted here. Headless UI-tree/action tests do
not establish actual pixel clipping, native Tab/controller traversal, assistive
support, Windows execution or physical GPU/input behavior. The desktop delivery
receipt and extracted `--help` smoke establish exact bytes and bounded Linux
command-line loading only, not create/play/save/reopen or clean-machine support.

The Linux ZIP is an optimized dev-profile review build with ordinary system
runtime dependencies. Source and desktop manifests record delivered bytes/modes;
the implementation freeze excludes Markdown/licenses, whose delivered bytes are
separately recorded. Local integrity checks/readback cannot authenticate a
publisher, prevent later external modification, guarantee power-loss retention
or prove all filesystem/platform support.

Windows/device/assistive and GPU qualification, uncoached sustained careers,
community succession/dissolution policy, dense navigation/hung mesh workers/broad
edit invalidation, production art/audio/licensed geography, content migration,
trusted update/rollback, public accounts/moderation, WAN/soaks, storage tails and
operational offsite retention/RPO/RTO remain open as stated in G01–G18.
