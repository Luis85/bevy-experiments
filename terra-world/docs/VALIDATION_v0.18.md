# Validation — Terra World 0.18.0

Executed on 29 October 2026 in Linux x86_64. The authority is the
[canonical gate receipt](verification/v0.18-acceptance/gates.json),
[source hashes](verification/v0.18-acceptance/source-hashes.json) and
[distinct test identities](verification/v0.18-acceptance/distinct-tests.json).

## Frozen-source release gates

Command: `python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.18-acceptance`

- **44 gates passed**; every command exited zero.
- **933 distinct Rust tests**, **1019 passing executions**,
  **0 failed**, **0 ignored**.
- The separate Python desktop-packaging gate ran **11 tests**, all passed on
  Linux, zero skips; those are not added to Rust test totals.
- **368 implementation inputs** unchanged before/after acceptance.
  Captured report and log hashes also remain unchanged.
- Compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; workspace version **0.18.0**.
- Exact accepted native executable SHA-256: `e16c25fabf30d8e96ff6b68cea99766ea1f4cc4d67afc1fbaeaff4433ffa8881`.

The gates include compatibility and formatting, workspace domain/persistence/
network tests, finite careers, physical traffic/navigation cases, goal continuity,
Bevy headless/render tests, the complete demo suite and named recovery journeys,
original model generators/pack builds, connected-source inspect/export/install/
verify, recovery probes, local TLS/regional sessions and native build.
The [regional-session result](verification/v0.18-acceptance/regional-sessions.json)
retains the exact measured workload and outcomes; it is not a WAN capacity claim.

The earlier `verification/v0.18-final` run began while final source refinements
were arriving. It is retained as a candidate record and does not certify final
source identity. Only the canonical frozen-input acceptance above authorizes
packaging. No failed candidate is counted as a passed final gate.

## Focused and independent review

See [reliability](verification/v0.18-review/reliability.md),
[gameplay](verification/v0.18-review/gameplay.md),
[player input](verification/v0.18-review/player-experience.md),
[delivery](verification/v0.18-review/delivery.md) and
[independent review](verification/v0.18-review/independent-review.md).
The gamepad focused run preceded the final version bump; final demo gates above
cover the final version. The design detector returned an empty finding list for
the changed input surface; that is not visual or accessibility certification.

## Fresh native observations

See [native review](verification/v0.18-native/native-review.md) and its receipts
for actual processes, actions, screenshot roles, executable identity and limits.
These observations are separate from automated test counts. Historical images
are not fresh captures. No native controller fault/physical hardware test is
inferred from keyboard-driven smoke checks.

The fresh supervised launcher exited **0** without forced termination. Ordinary
menu creation, play, Save and return, Continue/reopen and Save and quit succeeded.
A read-only comparison of committed stores retained title, character choices,
player, wallet (25,000 minor units), avatar name/owner and inventory hash;
generation advanced **34 → 52**, with SQLite quick check `ok`. This is bounded
field equality, not complete-world equality. The running fixture settled at
49 active chunks, 107 bodies and 64 wildlife. Actual Controls help at 1440 × 900
and pause at 1100 × 700 / 1440 × 900 were inspected. Intermediate attempted
capture filenames are explicitly classified by their actual observed state.

The native session began before the canonical gates completed; their eventual
accepted executable hash matches. Post-gate execution is not claimed. Native
captures do not establish exact movement distance, Ctrl+A text replacement,
physical-device behavior, native fault injection or hardware performance.

## Delivery and preservation

Desktop packaging consumes this exact accepted executable and **156 explicit
payload files**, preserving every one of the prior 152 paths and adding the
current plan/report and archived 0.17 review/validation. Its receipt is
[desktop package](desktop-package-v0.18.json). The package has the ordinary host
runtime dependencies; this is a Linux review package, not a Windows executable.

The source archive retains every baseline path, original assets/source fixtures
and historical evidence. The prior current review/validation are archived byte
for byte as 0.17. The source manifest records member hashes/sizes/modes; external
archive verification checks member bytes, modes, ZIP CRC, baseline preservation
and absence of new runtime databases. The standalone delivered report records
archive checksums without embedding a self-referential source checksum.

## Limits

This establishes a bounded regional reference and the declared test fixtures.
It does not establish Windows clean-machine installation/execution, real GPUs,
physical controllers/wheels, assistive access, uncoached motivation or economic
balance, arbitrary dense geography/traffic, production art/audio/geography,
public accounts/moderation, WAN populations, long soaks, deployment retention,
RPO/RTO or global authority partitioning. Refer to the complete
[G01–G18 register](REVIEW_GAPS_v0.18.md).
