# World, simulation and performance review — 0.21

Baseline: the supplied 0.20.0 source archive, inspected on 29 October 2026.
The owner reviewed before implementation and then implemented WP04–WP06 from
the root's recorded 0.21 plan. Existing receipts were historical evidence;
this document does not turn archived tests into fresh execution.

## Findings selected before implementation

1. **P1 — valid heterogeneous merchants could panic settlement maintenance.**
   In settlement/economy.rs, post_supply_contracts gathered retail shortages
   from all shops but the unchecked item lookup used only the first issuer's
   prices. The save/catalog validator permits distinct assortments. An unmatched
   retail item could panic during the normal maintenance batch, after other
   clock/price/offer work had already occurred.
2. **P2 — polar bootstrap waited for nonexistent tiles.**
   In living.rs, seed_population required a square count of nine at radius one,
   while the scheduler's TileKey::neighbors clips polar latitude to six
   valid tiles. Valid, fully loaded supported ground could never start living
   population, ecology or transport. A raw resident count also admitted unrelated
   pages instead of checking the startup neighborhood.
3. **P2 — a local feature edit rebuilt the entire resident region.**
   In render.rs, render_chunks advanced its global revision and queued every
   resident chunk for a change to any removed feature. The same revision rejected
   unrelated in-flight completions. This was the retained WP03/G07 work
   amplification gap, separate from already implemented source-pointer and
   worker-failure admission.

## Implementation

### WP04: checked central issuer valuation

The issuer lookup now uses a checked price reference and skips unsupported items
before consuming offer capacity, IDs or reward. Central issuer funding, reward
calculation, producer priority and catalog/save formats remain unchanged.

The new settlement regression constructs catalog-valid merchants with different
assortments. Unsupported item keys sort before and between supported requests.
Four supported requests must still post with consecutive IDs and the original
issuer. The test compares all shop goods, the independent merchant's funds,
and complete money plus escrow. It serializes/restores the world and repeats
maintenance with only unsupported shortages remaining.

### WP05: real neighborhood admission

Startup calculates the same clipped/wrapped tile neighborhood used by streaming,
counts only its resident keys, and retains the existing nine-page startup cap.
The supported-ground check remains authoritative.

Independent cross-review found the native flow's duplicate square threshold.
The shared public living::startup_neighborhood helper now returns the full real
neighborhood (empty for invalid input); population, native loading and narrow
smoke readiness can apply their respective startup or full-region thresholds to
the same keys. The player-experience owner integrates the native consumers.
RenderTelemetry exposes an integrated_chunks key set, backed by the immutable
source instance actually integrated. Old geometry remains visible while a source
is replaced, but that old source cannot falsely satisfy readiness for the new
resident page. The legacy chunks count is retained.

The new population_bootstrap integration target uses real loader threads,
the ordinary stream scheduler and the Bevy population/frontier/transport
schedules. Flat source fixtures cover six-tile north and south polar
neighborhoods and a one-tile world. A gated ninth loader demonstrates that an
unrelated resident cannot make an incomplete ordinary neighborhood ready.
A complete underwater fixture remains unseeded. These are headless supported
terrain cases, not native or real-geography qualification.
An additional helper regression checks full 49-page extent and invalid inputs;
a real renderer-system regression checks current-source readiness through
replacement and eviction.

### WP06: exact source-local edit context

Each resident source retains its exact effective set of removed feature IDs.
Jobs and failure entries carry that set together with the existing immutable
source Arc, quality revision and LOD. Only changed effective sets queue edit
work; real quality changes still invalidate all pages.

The dirty set remains queued if an edit changes while a job runs. A completed
job must match the current source, quality, LOD and exact edit set before any
geometry installs. A current completion also satisfies redundant queued work
when edits changed away and back. Source replacement and eviction discard
obsolete cached contexts. Unrelated edits preserve existing failure deadlines;
changed source/quality/LOD/effective edits invalidate obsolete backoff. The
two-job tracked concurrency cap, exponential delay cap, old visible geometry
and failure diagnostics remain.

The global removed-feature collection still scans bounded overlay records.
When that collection changes, the adapter compares resident feature IDs before
choosing affected pages. This does not claim zero main-thread invalidation work
or containment of a permanently hung worker thread.

GPU-free real-system tests cover:

- A 49-page neighborhood with one local feature edit and a separate global
  quality change, recording actual scheduled/pending job counts.
- Both fragments of a shared source feature during depletion and regrowth.
- Admission of an unrelated completion across another page's edit.
- Concurrent edits and overlay restore while a held job is in progress.
- Returning to the same exact edit context without a redundant rebuild.
- Unrelated edits retaining failure backoff and relevant edits resetting it.
- Existing current/replacement source, revision, LOD, residency, disconnected
  receiver, poisoned receiver and capped retry cases.

## Execution status

Per-file rustfmt (edition 2024, skip_children=true) and git diff --check passed
during implementation. No Cargo execution was started by this owner while the
integrator's shared compilation was active. Fresh Rust outcomes and workload
counts must be taken from the integrator's actual runs; implementation alone
is not a passing receipt.

Suggested coordinated targets:

    cargo test -p terra-sandbox --test settlements --locked --offline
    cargo test -p terra-bevy --test population_bootstrap --locked --offline
    cargo test -p terra-bevy --lib --locked --offline mesh_ -- --nocapture

The existing procedural settlement, living-world, finite career and complete
render/Bevy gates remain integration acceptance.

## Remaining product limits

The changes do not establish long-session economic balance or uncoached goal
comprehension; general or dense-traffic autonomy; unavailable-leader succession
or dissolution policy; licensed complete geographic ingestion; production
character/vehicle/audio coverage; physical GPU/frame-tail/residency budgets;
native Windows/device/assistive qualification; WAN capacity, public-service
operations or distributed authority. Missing terrain remains unavailable and
finite regional caps remain explicit. Job counts are not FPS or GPU-memory
measurements.
