# Validation — Terra World 0.21.0

Executed on 29 October 2026 on Linux x86_64 using
`rustc 1.99.1 (58cf8f9c3 2026-10-13)` and Python 3.12.14.

## Direct final acceptance

**44 gates passed · 1,000 distinct Rust tests ·
1,097 passing Rust test executions.** The accepted
run has zero failed or ignored Rust tests and no timed-out gates. The separate
Python delivery suite passed **34 tests, zero skips**; those are not added to
Rust counts. All **373 frozen implementation inputs**, captured logs and
published reports retain their accepted hashes.

Command, run from the project root with the pinned toolchain and target directory:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.21-final-acceptance
```

Exact per-gate commands, environment and timing are recorded in the direct
runner-written [completed receipt](verification/v0.21-final-acceptance/completed-gates.json).
Its progress alias is byte-identical. The run started at 2026-10-29T19:47:09.477889+00:00 and
finished at 2026-10-29T20:06:36.577616+00:00. No receipt reconstruction was used.

Receipt SHA-256:
`fa7a05e0761db19a134af0c0272f79ae2c9c17bcf8323c4bdcffce9e100c898e`.

| Crate | Distinct passing tests |
|---|---:|
| terra-bevy | 176 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 203 |
| terra-history | 13 |
| terra-net | 126 |
| terra-persistence | 48 |
| terra-sandbox | 352 |
| terra-stream | 45 |

Accepted executable SHA-256:
`209b5f2ee0d74d8b4776346297af6838dd9e05cf622af80645f63dedb0bc13d1`.

The [source manifest](verification/v0.21-final-acceptance/source-hashes.json),
[distinct identities](verification/v0.21-final-acceptance/distinct-tests.json),
startup-probe logs, command logs and finite workload reports are beside the receipt.
The default per-command deadline is 3,600 seconds; startup probes are also bounded.
The final run did not need a timeout to complete.

## Executed coverage

The canonical roster covers domain rules, finite economy, ownership, actual
TCP/TLS, durable storage and rejection behavior; warehouse/purchased-stock and
one-hour novice passenger careers; physical traffic, navigation and continuing
guidance; all Bevy headless/render and demo tests; managed recovery and naming;
headless/provider examples; original asset generators, inspection and authoring;
prepared-source inspect/export/install/verify; recovery probes; a finite
16-client TLS workload with eight cycles, 400 ticks and 1,024 cold regions; and
the exact native executable build.

The workload completed successfully with its declared checks. Its detailed
[report](verification/v0.21-final-acceptance/regional-sessions.json) retains
actual measurements and limitations. This finite local workload does not
establish WAN latency, sustained population capacity or public service readiness.

Selected 0.21 regressions cover Loading recovery controls; real dispatched modal
keyboard events, blocked parent activation and successful input release; four
save contexts; heterogeneous shop maintenance with conservation; polar and
ordinary startup using relevant/current source keys; targeted edit remeshing,
shared fragments and stale work; asynchronous full-state regional admission;
private JSON replacement; exclusive first-launch records, abrupt JSON/WAL exits,
required-checkpoint refusal and metadata-safe outputs; bounded verification,
partial logs, descendant cleanup and terminal-receipt package rejection.

The mesh regressions measure one job for a one-page feature edit among 49
resident pages, and two for shared fragments. Global quality changes still use
the two-worker bound and retain pending work. These are exact work-count checks,
not measured frame-rate improvements on a physical GPU.

## Candidate evidence and corrections

Candidate results are retained and excluded from the final totals:

- `verification/v0.21-preflight/demo.log`: 192 passed and three newly added
  positive-control fixtures failed because they asserted before Bevy's deferred
  Activate command was flushed. Owners flushed both rejection and restored-action
  checks. `demo-confirmation.log` then passed all 203 tests; final canonical demo
  also passed 203 after the narrow visual/copy correction.
- `verification/v0.21-preflight/private-snapshot.log`: both new snapshot tests
  passed, while an unrelated selected loopback test was denied socket creation
  by the default sandbox. The normal reviewed execution permission path enabled
  loopback; the full four-test filter and final canonical suites passed.
- The initial native dependency warmup caught a transient system-parameter count
  during active edits. The owner corrected the query grouping before coordinated
  testing; it is not a clean baseline or accepted run.
- `verification/v0.21-release-acceptance` was intentionally interrupted during
  headless compilation after three passing gates, so the desktop allowlist could
  include the current 0.21 plan/gaps/history and preserved 0.20 reports. Its
  terminal receipt is explicitly failed with `interrupted`; its results are not
  used as release acceptance. Final acceptance used the new directory above.

Cross-review also corrected native polar loading after the first living-world
fix, protected startup-record paths from export/directory collisions, and moved
startup probes onto the same bounded process runner as gates. General Windows
process cleanup is qualified as unconfirmed when `taskkill` fails; actual Windows
execution remains untested here.

## Fresh native inspection

The actual compiled native program was inspected in an isolated X11 display with
Vulkan llvmpipe software rendering, at 1440×900 and 1100×700 with 150% text.
The [native record](verification/v0.21-native/README.md) includes images,
per-process/source/binary receipts and per-capture PID/dimension/image hashes.

The launcher/editor retained visible fixed actions and scrolling content. F4
opened the draft, Tab stayed within its editing context and Escape returned to
unchanged setup. A captured Space-key wrap was corrected by widening only that
key; rebuilt confirmation shows it on one line with all actions and instructions
visible at minimum size/maximum text. The corrected review copy reuses the tested
shared helper; its paragraph was below the captured scroll position, so those
images are not cited as visual proof of the wording.

An actual native launcher created a separate radius-one managed world, played,
checkpointed and completed Save and quit. At minimum size, keyboard navigation
scrolled Save and quit fully into view. The final binary reopened that same
world, rendered terrain/actors/activities, paused and saved/exited successfully.
The launch-record hash stayed unchanged, the profile retained checkpoint
presence and the database passed integrity checking at committed generation 15.

Both final confirmation processes exited zero and kept their 373 recorded inputs
unchanged during execution. Their exact binary hash matches the accepted binary.
Only the desktop document allowlist changed afterward; the
[input association](verification/v0.21-native/confirmation-input-association.json)
records that sole difference. Runtime code, configuration and assets did not
change between native confirmation and final acceptance.

One early candidate software-rendered frame was dark/partially drawn despite
logical Region ready; later captured frames rendered the world. The cause was
not determined. First-visible-frame timing and shader warmup remain G07
investigation, rather than an inferred GPU success or regression diagnosis.

## Reviews, documentation and preservation

Three dedicated specialists and the integrator covered the product perspectives
in the [plan](IMPLEMENTATION_PLAN_v0.21.md), then cross-reviewed implementations.
Notes are under `verification/v0.21-review`. The [current review](PRODUCT_REVIEW.md)
and [complete G01–G18 register](REVIEW_GAPS_v0.21.md) retain selected closure
boundaries and follow-up priorities. A local-link audit found no broken
destination/heading anchor among 196 links in 13 current documents.

The independent [preservation audit](verification/v0.21-review/preservation.md)
checked all 2,197 original files and the exact 1,885-file protected union of
assets/data/authoring/licenses/history. Both prior current reports are archived
byte-for-byte as `PRODUCT_REVIEW_v0.20.md` and `VALIDATION_v0.20.md`. Eleven README
release-overview sections survive in `RELEASE_HISTORY.md` with only required
relative-link adjustments.

The prepackaging audit records its own snapshot, before the final current
validation and desktop allowlist changes. Final source packaging rechecks every
original file, accepted evidence and archive member, restores original ZIP Unix
modes and embeds a complete `DELIVERY_MANIFEST.json`. Desktop packaging separately
admits the completed receipt, exact binary and curated payload. Package byte
verification and extracted `--help` smoke records are retained with the source
handoff under `verification/v0.21-delivery`; they are not native hardware tests.

## Platform, compatibility and remaining limits

Domain save 1, regional schema 1, wire protocol 10 / `terra-world/10` and asset
schema 1 remain. `world-started.json` is additive managed lifecycle metadata:
it records admission, not successful saving. A subsequent launch without valid
required progress fails closed. Interrupted first launch before any checkpoint
requires recovery or a separately created world. Already-lost legacy progress
cannot be identified retroactively; older launchers do not enforce the guarantee.
Keep complete archival folders/dependencies and use verified recovery to relocate.

The desktop artifact is a Linux x86_64 optimized dev-profile review build with
ordinary system runtime dependencies. Windows clean-machine execution remains
G02/P1 for the intended PC. Software-rendered native inspection and logical event
tests do not qualify physical controllers/wheels, assistive technologies or GPU
frame tails/residency. Unix private modes do not qualify Windows ACLs.

R4 directory-sync uncertainty and UX04 selected-owned-actor/needs visibility
remain explicit follow-ups. Uncoached sustained play, community governance,
dense navigation, arbitrary hung workers, production content/audio/licensed
geography, migration, trusted updates, public accounts/moderation, WAN soaks,
storage tails and offsite RPO/RTO retain their named criteria. Local hashes and
readback establish consistency, not publisher authentication or power-loss proof.
