# Terra World 0.5 validation record

Validation session: **2026-10-27**. Source release **0.5.0**, wire protocol **3**,
regional database schema **1**, Rust **1.99.1**, Bevy **0.20.0**, Linux x86-64.
The final automated gates contain **320 distinct passing Rust tests**, nine
successful real-process test groups, four compiled-native output-path rejection
cases, two successful finite headless examples and a successful native build.
Repeated runs and shared feature variants are counted once.
The [machine-readable release summary](release-verification-v0.5.json) records
the gate totals and executable checks.

This release adds transactional regional mutable state and its domain, server
and native lifecycle. Review findings and the dedicated follow-up pass are in
[PRODUCT_REVIEW.md](PRODUCT_REVIEW.md); public contracts and operational limits
are in [REGIONAL_PERSISTENCE.md](REGIONAL_PERSISTENCE.md). The prior
[v0.4 validation record](VALIDATION_v0.4.md) remains available. Its network and
graphics measurements are historical evidence, not fresh v0.5 measurements.

## Final automated gates

| Suite | Distinct passing tests | Coverage |
|---|---:|---|
| Core | 12 | Geographic round trips, poles/date line, clocks, solar/seasons, actor and environment contracts |
| History | 13 | Dated packs, coverage/provenance, path/file bounds, political geometry and bundled scenarios |
| CLI | 4 | Argument validation, deterministic simulation, complete sandbox workflow and saved coordinates |
| Stream | 20 | Thirteen unit tests, six provider fixtures and one actual local HTTP/cache integration |
| Sandbox | 107 | Ninety established gameplay/ecology/economy/scaling tests plus seventeen regional invariants |
| Network library / host / TCP | 63 | Fifty-four established tests plus nine regional host and transport regressions |
| Regional storage | 20 | Transactions, crash/restart, stale writers, corruption, backup, schema/path safety and cursor invariants |
| Bevy library with rendering | 40 | Includes twenty-one shared headless library tests plus terrain/material/scene/rendering contracts |
| Bevy integration | 19 | Eleven established complete-world/replica scenarios plus eight regional lifecycle tests |
| Native demo | 22 | Fourteen established controls/UI/export tests, four output guards, one recovery-copy regression and three regional starter-world regressions |
| **Total** | **320** | **Final gates passed; reruns and shared tests counted once** |

The combined non-Bevy gate passed 239 tests. The no-default-feature Bevy run
passed forty: twenty-one shared library tests and nineteen integrations. Its
twenty-one library tests are already included in the forty-test rendering row.
The final rendering-library gate passed forty tests, and the final demo gate
passed twenty-two. The regional recovery integration was rerun after its final
copy assertions were added. Zero-test/doc-test harnesses, examples, process
groups and benchmark iterations do not increase the Rust-test total.

Actual local TCP was permitted for the socket fixtures. Initial restricted
environment attempts are not counted as passes. Formatting and the final
native build are separate release gates.

### Domain correctness under retirement and reactivation

The seventeen new pure-domain tests exercise exact partition/assembly and
active/cold merges, active deletions, migration into an already populated cold
page, reference closure, retained ownership/home/economy state, dormant capacity
reservations, partial-save rejection and invalid placement or dependency links.
They cover poles, date-line adjacency and fine high-latitude tiles where an
angular neighbor ring alone cannot preserve physically nearby records.

Dormant records still consume aggregate domain capacity. Wildlife recovery and
new allocation cannot exceed a world limit by ignoring sleeping records. The
partial-state marker also protects the retained global ledger: neither it nor
an active working set may be written through an API promising a complete world.
Full export recomposes and validates both active changes and dormant data.

Existing suites retain inventory/money/escrow conservation, faction rights,
construction, farming, finite resources, species and habitat relationships,
territory progression, public works, shop/producer stock, reserved delivery
cargo, cancellation and physical return. The prior geographic index and
simulation scheduling regressions also passed in this release.

### Transaction, restore and operational safety

The twenty storage tests use actual temporary SQLite databases. They verify:

- Global state, page insert/update/delete and game-owned metadata commit as one
  generation; failed checkpoints preserve the prior durable state and live
  working set.
- A stale handle cannot overwrite a later generation. Existing stores must be
  restored before mutation, and rejected host metadata does not advance the
  store's cursor.
- Complete export, restart and online backup preserve records and mutations.
  The backup contains committed WAL data and refuses an existing destination.
- Payload corruption, identity/catalog mismatch, wrong region placement and
  invalid dependencies reject before installation.
- Unexpected SQL triggers/views and views-only foreign databases reject before
  Terra can initialize or mutate them.
- Database, journal, relative/canonical, symbolic-link and Unix hard-link output
  aliases are rejected without destroying the original data.
- A real child process exits abruptly with a committed checkpoint and with an
  unfinished transaction; reopening preserves the committed generation and
  excludes the unfinished work.

The shipped crate pins rusqlite 0.40.2 with bundled SQLite 3.53.2. Its WAL/FULL
configuration and optimistic generation check are explicit implementation
choices. These tests cover process failure and transaction boundaries, not
physical power-loss certification or replicated disaster recovery.

### Server authority and Bevy lifecycle

The nine regional host tests preserve private session identities, accepted
command sequences, clock/catalog/actor metadata and complete exports across
restart. They exercise source-feature interests shared by multiple terrain
chunks, corrupt metadata, incompatible restore modes and protected output
paths. A real SQLite write lock verifies that admission/sandbox success messages
wait for their batch commit, further input is held during failure, and retry
commits before releasing those responses.

The eight Bevy integrations run actual schedules. They verify pre-startup
restore, owned-player identity, source registration after hydration, retirement
without replenishment, full-state lookup after an intervening save in the same
intent queue, final checkpoint ordering and complete export. Boundary feature
anchors retain their source edits. Source chunks that finished loading during a
failed persistence barrier are replayed once after recovery. Checkpoint age and
autosave cadence use real time, including paused play and clamped slow frames.
Successful recovery clears the persistence error while intentionally leaving
time paused.

The established headless scenarios still exercise trading, possession, direct
movement, RTS orders, production, delivery, gathering, claims, funded work and
exact save/restore. Real framed replica fixtures continue to reject malformed
or private remote records and avoid running client-side authoritative ecology.
Rendering regressions retain terrain border heights, finite geometry, bounded
materials/detail and the actual Bevy scene-ready replacement lifecycle.

Three final demo tests run the actual startup system with the regional adapter.
A restored possessed helicopter at 850.125 m preserves the assembled world and
clock, including a depleted dormant node and an absent starter node. Blocked
startup leaves state/revision unchanged and later checkpoints a valid fresh
initialization after recovery. A partial fresh world waits for source hydration
and preserves the dormant starter's original position and remaining stock.

## Independent executable checks

Fresh server/client binaries passed these nine process groups:

| Group | Result |
|---|---|
| Two simultaneous procedural-world clients | Passed |
| JSON save/restart preserving two identities and sequences | Passed |
| Regional restart, identity continuity and complete JSON export | Passed |
| Ambiguous JSON/regional restore modes | Rejected as required |
| Regional export aliases | Rejected before database open |
| Historical pack with matching political data | Passed |
| Mismatched epoch, pack and politics revisions | Rejected as required |
| Invalid historical startup and resume configuration | Rejected as required |
| FIFO/special-file startup inputs | Rejected without blocking |

Four additional cases invoked the compiled native demo with telemetry targeting
the database, telemetry targeting its journal, a screenshot targeting the
database, and the derived political export targeting the database. All four
exited with status 1 before window startup, leaving the database intact. These
are executable checks in addition to the four demo unit tests.

Both finite `terra-bevy --no-default-features` examples passed: `headless` loaded
one synthetic chunk and queried its terrain height, and `custom_provider` loaded
one chunk through an installed provider. Process fixtures use local data and
sockets; they do not download a public world dataset.

## Finite regional storage and simulation probe

The complete machine-readable result is
[regional-probe-v0.5.json](regional-probe-v0.5.json). The harness uses a temporary
database, synthetic finite records, zero sockets and a permissive finite-position
motion resolver. Imported collision, rendering and live public data are outside
its scope. It ran on Linux x86-64 with eight visible logical CPUs, SQLite 3.53.2
and the workspace development profile (`opt-level = 1`, debug assertions enabled).

| Workload or measurement | Recorded result |
|---|---:|
| Complete / active resource nodes | 6,400 / 400 |
| Complete / active living bodies | 239 / 59 |
| Active / dormant nonempty geographic pages | 4 / 15 |
| Active / dormant paged records | 448 / 6,180 |
| Retained global records at final checkpoint | 29 |
| Final serialized checkpoint payload | 1,686,302 bytes |
| Repeated checkpoint samples | 16 |
| Checkpoint mean / median | 82.55 / 79.72 ms |
| Checkpoint p95 / maximum | 110.85 / 110.85 ms |
| Initial checkpoint | 39.67 ms |
| Complete-world restore | 38.78 ms |
| Simulation duration | 500 steps; 25 physical / 250 calendar seconds |
| Complete / active simulation pass | 39.93 / 10.86 ms |

Every checkpoint recomposed exactly; full round-trip, reopen and online-backup
comparisons passed. The selected dormant resource stayed unchanged. Paged
records include metadata and tombstones: they are not all NPCs or independent
resources. Payload size is not the database's on-disk or WAL size.

The two simulation passes deliberately perform different workloads: dormant
environment and ambient bodies freeze, while retained economy and owned bodies
advance. The observed CPU reduction is evidence that retirement removes active
work, not equivalence to fully simulating all sleeping actors. It must not be
combined with checkpoint latency into an unsupported overall speedup claim.

Checkpoint/activation still reconstruct and validate the bounded complete world
synchronously. About 83 ms mean checkpoint cost is a concrete frame/tick-path
bottleneck in this fixture. A bounded worker and incremental dependency-group
validation are next structural work before raising population or collection
caps. These finite timings establish neither 20 Hz service guarantees nor
MMORPG concurrent-player capacity.

## Native interaction and visual confirmation

Native checks use the actual application on Linux/X11 with Mesa llvmpipe
software Vulkan, at 1440 × 900 and the 1100 × 700 minimum. The source is the
bundled authored heightmap fixture at real Munich coordinates; its heights,
inhabitants and economy are synthetic scenario content. This environment checks
rendering, lifecycle and UI behavior, not target-GPU frame rate.

The first session loaded 49 terrain chunks and exercised third-person entry,
F12 diagnostics and scrolling, pause, F5 checkpoint and F9 restore. A real second
SQLite connection held a write lock while F5 ran: the durable generation and
saved clock remained unchanged. After unlocking, retry succeeded. The clock was
advanced and then restored to the exact saved paused value. Normal window close
completed the final checkpoint and exported telemetry with no remaining world
or simulation error and exit status 0.

The visual review found a misleading ordinary pause hint during storage failure.
The final patch replaces it with F5 retry / F9 restore controls while blocked;
Space-to-resume returns only after the failure clears. A focused demo regression
and the actual-schedule recovery assertions cover this distinction. The final
confirmation reopens the same database with a smaller terrain radius and checks
the corrected blocked/recovered interface. Full observations and sanitized
reports are in [native-v5-verification.json](native-v5-verification.json).

That confirmation rendered nine source chunks while retaining nine active and
forty dormant mutable pages: 251 active paged records, 244 dormant records and
96 retained global records. Both write-lock attempts preserved the durable
generation, clock and payload; retry committed successfully. The minimum-window
blocked footer displayed F5/F9, and recovered play remained paused with the
Space hint restored.

A final saved-state comparison also exposed a separate nonvisual startup bug:
the demo initializer grounded the saved active body, changing its altitude by
2.5 cm in the ground-actor fixture. This could also ground a restored aircraft.
The initializer now skips restored worlds, defers blocked/partial state and
commits validated fresh initialization through the normal checkpoint path. The
three demo regressions above cover the fix. Native confirmation of that fix uses
a separate paused database with an owned helicopter at 500 m, prepared through
the public domain/store APIs. After 100 actual native updates, its complete
assembled state and metadata matched exactly, both player and aircraft retained
their saved positions, and the process exited with status 0. The screenshot round precedes only this
nonvisual fix; no presentation behavior was changed afterward.

Actual captures:

- [Persistent heightmap world](screenshots/v5-persistent-world.png)
- [Reopened world with dormant regional state](screenshots/v5-resumed-regions.png)
- [Failed checkpoint and actionable recovery controls](screenshots/v5-persistence-blocked.png)
- [Recovered minimum-window interface](screenshots/v5-persistence-recovered.png)

The software adapter and virtual display emitted expected platform warnings.
There were no application panics or render validation errors in the completed
native sessions. Placeholder assets and overlapping/clumped crowds remain
visible product limitations. These captures do not establish production art,
animation, navigation, collision or accessibility quality. Pointer-look raw
motion was not reverified in this iteration. Windows/macOS native operation,
screen-reader/controller coverage and gaming-GPU budgets remain unverified.

## Reproduction and release scope

From the repository root, with the pinned toolchain and documented native build
dependencies installed:

```sh
cargo fmt --all -- --check
cargo test --workspace --exclude terra-demo --exclude terra-bevy --locked
cargo test -p terra-bevy --no-default-features --locked
cargo test -p terra-bevy --lib --locked
cargo test -p terra-demo --locked
cargo build -p terra-net --bins --locked
python3 crates/terra-net/tests/process_smoke.py
cargo run -p terra-persistence --example regional_probe -- --output regional-report.json
cargo run -p terra-demo -- --heightmap crates/terra-stream/data/heightmap-demo/manifest.json --region-store saves/frontier.db --quality balanced --camp-view
```

The process script accepts explicit executable paths; consult `--help` when
using a custom Cargo target directory. Native first-run dependencies and source
configuration are described in the root README. Opening the same regional path
with matching source/catalog identity resumes it. Changing from an old JSON
save to a new database flag does not import that world: use the documented SDK
import workflow before expecting existing client identities to resume.

The release archive contains all nine crates, lockfile, source fixtures, docs,
tests and actual evidence. It excludes compiled targets, private world/session
files, databases and SQLite sidecars. Archive integrity, source-byte parity,
local documentation links and JSON evidence are checked before handoff.

Aggregate gameplay caps, frozen dormant-environment policy, single authority,
trusted reference TCP transport and synchronous barriers remain explicit.
Further work needs storage-worker backpressure, incremental transactions,
principal dormancy, regional authority transfer, account/security operations,
prediction/reconciliation and representative target-hardware/load evidence.
