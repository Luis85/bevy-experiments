# Product review and improvement pass — 0.5

## Outcome and review scope

This iteration advances the persistent-world architecture: real geographic
mutable pages now leave the active simulation, remain in a transactional store,
and return without replenishing depleted resources or discarding ownership,
money, cargo or funded work. Both the native local game and the authoritative
reference server use that lifecycle. The existing gameplay, source and rendering
systems remain integrated.

Three parallel implementation/review tracks covered the domain graph, host
integration and local player/developer experience. The root integration track
built the database boundary, operational APIs, measurement fixture and release
handoff. After implementation, independent cross-reviews challenged each track;
the dedicated improvement pass addressed the concrete failures below. Evidence
is collected in [VALIDATION.md](VALIDATION.md), the automated tests and the
release's actual native captures. Prior review history is retained in
[PRODUCT_REVIEW_v0.4.md](PRODUCT_REVIEW_v0.4.md).

The product remains a reusable, playable foundation. The release does not
establish production AAA asset quality, Internet account operations, elastic
world population or MMORPG concurrency. A working-set reduction and a durable
checkpoint are useful structural improvements with specific limits.

## Review across product perspectives

| Perspective | Review result and improvement | Remaining product boundary |
|---|---|---|
| Player progress | Homes, rights, depleted nodes, inventory, wallet and simulation clock survive region retirement and restart. Explicit restore names the loss of later changes. | Long-term content balance, quests, combat and progression depth remain game-owned. |
| Construction, farming and crafting | Commands hydrate before full-world workbench, housing, claim and infrastructure queries. A mixed save/command queue cannot accidentally query a partial world. | Dormant farms freeze under the reference policy; there is no agronomic/offline harvest model. |
| Economy and logistics | Shops, production, finite tills/stocks, actual courier cargo, reservations, contracts and escrow share the retained ledger and atomic commit. | The small resident economy remains globally bounded; there is no distributed market or offscreen logistics service. |
| Ownership and factions | Principals, faction roles, plots and community claims remain authoritative while environmental records sleep. Owned bodies/homes retain nearby pages. | Logout does not yet retire all persisted principals or release their residency pins. |
| World liveness | Ambient bodies, wildlife and resources have a real residency lifecycle. Relative timers freeze rather than receiving invented catch-up. | A richer game may need a validated coarse offscreen simulation policy. |
| Geographic correctness | Page placement, cross-page references, poles and date line are verified. Metric pins supplement angular neighbors. Visible source-feature anchors preserve boundary tombstones. | Mutable residency does not supply missing terrain or replace continuous collision/navigation. |
| Multiplayer authority | Join identities and successful sandbox responses are held behind their input-batch commit. A failed barrier prevents more input consumption or simulation. | Trusted framed TCP remains the reference transport; authenticated encrypted sessions and prediction remain production work. |
| Data integrity and recovery | Global state, pages and host metadata commit together; stale generations, corrupt payloads and incompatible identities reject. Backups include committed WAL contents. | Recovery evidence covers process failure, not physical power-loss certification or replicated disaster recovery. |
| Privacy and operational safety | New databases/backups are private on Unix; diagnostics omit capabilities and complete inventories. Export/log/image paths cannot overwrite the active database or journals. | Production account management, deployment isolation and backup retention must be supplied by the integrating game. |
| Architecture and reuse | The ninth crate, `terra-persistence`, is independent of Bevy. Pure partitioning, store lifecycle, host methods and Bevy resources have explicit contracts. | Checkpoint/activation still reconstruct the bounded complete world synchronously. |
| Performance and scale | Active versus complete simulation work and checkpoint latency are measured separately. Dormant counts still reserve aggregate capacity. | Existing population limits remain. A bounded storage worker and incremental validation are required before larger aggregate worlds. |
| UX and recovery | F5/F9 follow the selected save mode. World/F12 show generation, residency, age, latency and recovery status. Failed writes pause play; retry clears stale errors while keeping time paused. | Database import from an older JSON save is an explicit SDK workflow; there is no native import wizard. |
| Accessibility and readability | Error/recovery information uses words, not color alone. Existing keyboard shortcuts, forest palette, minimum text size and scrolling panels are retained. Loaded infrastructure is labeled honestly. | Complete keyboard-only, controller and screen-reader coverage is still unverified. |
| Graphics and presentation | Terrain LOD, atmosphere, materials, actor overrides, possession and contextual UI remain available around the new persistence lifecycle. Native checks inspect the real app. | Reference actors/assets, crowd movement, animation, audio, interiors and target-GPU budgets remain below a production AAA bar. |
| Compatibility and delivery | Complete older JSON saves remain readable under matching catalog/source identities; source revision and wire protocol remain stable. Docs distinguish the new database schema and partial-state API. | Downstream Rust struct literals must migrate to `SandboxState::new()`/`Default`; Windows/macOS native builds remain unverified. |

## Defects closed in the dedicated improvement pass

### Preserve complete-world invariants while paging

The first integrated working set exposed a capacity bug: recovery or allocation
could count only active bodies/resources and exceed aggregate limits when cold
records returned. Exact dormant reservations now participate in admission and
validation. The partition's global ledger and active working set carry the
partial-state distinction, and ordinary full-save APIs reject them.

The merge validates that reservations match the previous cold-page inventory.
It appends cold records without overwriting identities, retains deletions from
the prior active set, and supports an active actor moving into an already
populated cold page. The full world is validated against its catalog before a
commit or installation.

### Keep commands and multiplayer success consistent with durability

The local queue originally hydrated only once before a batch. An intervening F5
could retire pages before a later craft or build query. Hydration now occurs at
each gameplay-intent boundary, with a cheap already-materialized path.

The host originally flushed success messages before attempting the input-batch
checkpoint. A failed write could therefore acknowledge an identity or operation
that would disappear on restart. Responses now wait for the successful commit.
A failed barrier is retried before any more input or simulation is processed.
A real SQLite write-lock test verifies the withheld messages and later recovery.

### Make restore and source recovery atomic

Host metadata validation now precedes changes to the store's generation and
residency cursor. Rejected clock/session metadata leaves later merges safe.
Discarding a restore result and retaining an old partial state cannot make
`hydrate` falsely report that the world is complete.

Source geometry could finish loading during a persistence error while mutable
registration was blocked. Recovery now replays those resident source chunks
once, after hydration. This restores their gameplay metadata without refilling
depleted resources. First-point region keys of visible source features also
remain requested, including features exactly on tile clipping boundaries.
Shared source-anchor contributions survive until their last source tile evicts.

### Protect the database from alternate output and schema hazards

Export, telemetry, screenshot and political-output paths could alias the active
database or a SQLite companion file. Shared path guards now run before startup
and at output APIs, including common relative/canonical, symlink and Unix
hard-link aliases. The database remains readable after rejected attempts.

Unexpected persistent SQL triggers/views are rejected before initialization or
writes. A trigger cannot make an apparently successful checkpoint silently delete
its pages. A views-only foreign database cannot receive Terra tables before it
is rejected. Backup creation is exclusive, preventing a concurrent destination
from being overwritten or removed during failed backup handling.

### Correct geographic and player-facing edge cases

An angular tile ring is insufficient at the poles and at fine high-latitude tile
levels. Existing record anchors now receive a 160 m spherical-distance query
around retained players, bodies and homes. Tests cover a building at the same
physical pole but a different longitude, high latitude and date-line proximity.

Restore preserves the saved clock before camera initialization. Automatic save
cadence uses real time, including paused play and slow frames with clamped virtual
time. Recovery removes stale persistence errors, explains that time remains
paused, and identifies the restored-player focus action. Distant territory
infrastructure counts are labeled as loaded counts; the actual upgrade command
checks the complete world.

Actual native review also found that the ordinary paused footer advertised
Space-to-resume during a database write failure. Blocked states now show F5 retry
and F9 restore instead; Space returns only after storage recovers and time remains
intentionally paused. The final documentation review clarified that changing a
server flag creates a new database world: existing JSON identities require the
explicit SDK import before clients can resume them.

The final native saved-state comparison uncovered a separate initialization
defect: the reference demo's starter setup could ground a restored active body
and reseed a missing starter node outside the persistence lifecycle. Restored
regional worlds now skip that setup entirely. Blocked/partial worlds wait for
the normal source hydration barrier; new initialization validates a candidate
before installing it and queues the normal durable checkpoint. Three actual
Bevy-schedule regressions cover an airborne restored actor and dormant depleted
resources, blocked recovery, and a fresh partial world's later hydration.

## Evidence and limits

The [regional fixture](regional-probe-v0.5.json) reports the actual full and active
record/body counts, 500 simulation steps, checkpoint samples, restore cost and
exact preservation results. Its full and active passes deliberately perform
different workloads because dormant environment state freezes. It does not
establish equivalence to continuously simulating every dormant actor.

Automated verification covers domain conservation and references, SQLite
rollback/restart/backup/conflicts, actual Bevy schedules, real TCP identities and
command sequences, and standalone host/client processes. Native verification
covers real input, checkpoint/restore, database locking and recovery, minimum
window presentation and graceful exit. Exact final counts and observations are
in [VALIDATION.md](VALIDATION.md).

The software Vulkan environment verifies rendering and UI behavior. It is not a
gaming-GPU frame-rate benchmark. No new public-data coverage, production asset
quality or concurrent-player capacity is inferred from these checks.

## Next structural priorities and acceptance criteria

| Priority | Work | Acceptance criterion |
|---|---|---|
| 1 | Incremental durable transactions and a bounded storage worker | Only affected dependency groups require validation; backpressure and acknowledgement semantics are explicit; measured checkpoint/activation latency meets a declared frame/tick budget. |
| 2 | Account dormancy and regional authority ownership | Disconnected principals can retire safely; ownership, cargo and escrow cannot duplicate during handoff, failure or restart. |
| 3 | Operational migration and recovery tooling | A user can inspect, back up, restore and deliberately migrate a complete local/server world without writing Rust or overwriting an existing store. |
| 4 | Internet multiplayer boundary | Authenticated encrypted sessions, prediction/reconciliation, abuse controls and representative gameplay load tests preserve existing command/privacy invariants. |
| 5 | Production presentation and movement | Licensed assets, animation/audio, crowd separation, navigation and collision perform within measured budgets on target hardware. |
| 6 | Platform and scale evidence | Windows/macOS native builds and actual target GPUs are exercised; soak/recovery tests include representative regions, clients and storage conditions. |

These are remaining engineering projects. The completed regional lifecycle is a
foundation for them, with current limits and failure semantics made explicit.
