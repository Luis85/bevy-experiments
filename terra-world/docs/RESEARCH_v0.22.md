# Terra World 0.22 — research and design decisions

Research date: **29 October 2026**. This synthesis supports the
[implementation plan](IMPLEMENTATION_PLAN_v0.22.md) and
[product review](PRODUCT_REVIEW.md). Sources are primary documentation and
official examples. They explain applicable patterns; they do not certify Terra
or replace its own tests, native observations or user research.

## User goals and research questions

Terra aims to be a reusable Rust/Bevy framework for georeferenced living worlds,
with a native reference game connecting RTS, possession, vehicles, gathering,
production, finite commerce and community progression. The current increment
asks what improves that complete loop while protecting existing progress and
keeping framework boundaries usable by other games.

The review asked four questions:

1. How can the player understand actor identity, ownership, needs and action
   targets without leaving the recommended activity?
2. How can a reversible early community choice have an honest lifecycle without
   inventing a policy for distributing public assets or excluding other members?
3. What distinguishes visible file replacement, confirmed persistence and an
   actively owned local session?
4. What can release tooling prove independently on an extracted package, and
   which device, trust and deployment questions still need direct evidence?

## Player comprehension and accessibility

### Make current context available at the point of action

Microsoft's [XAG 109: Objective clarity](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/109)
recommends accessible current objectives and progress so players do not have to
remember what to do next. [XAG 114: UI context](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/114)
emphasizes enough context to understand interface elements and their effects.

**Terra decision:** keep the existing live-state Activities guidance and add
consistent actor context in the already reserved HUD. Distinguish selected
bodies from the active interaction body. Display ownership and missing/received
scope explicitly. Do not add a second quest ledger or a separate account model.
A group summary names the lowest relevant needs instead of presenting the first
selected actor as the whole group. Unrelated vehicle selection must not display
the active avatar as that vehicle's driver.

This design follows from Terra's actual source findings and fresh baseline
captures. It is an application of the guidelines, not a claim that Microsoft
prescribes this exact layout or that a developer walkthrough measures novice
comprehension. Uncoached first/returning journeys remain part of G03/G04.

### Preserve readable information and predictable navigation

[XAG 101: Text display](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/101)
addresses readable, configurable text and layout when text grows.
[XAG 103: Additional channels](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/103)
addresses information that should not depend on a single sensory channel or
color distinction. [XAG 112: UI navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112)
emphasizes consistent paths and predictable focus.

**Terra decision:** preserve named Food/Water/Energy percentages alongside bars,
add explicit word-or-character wrapping for long tokens, keep full names in
inspection and bounded names in the compact HUD, and reuse existing focus and
scroll behavior. The new summary is read-only. Existing action authority is
rechecked at activation/dispatch, with reviewed native consumption context where
feasible.

The tested product preference range remains 100–150%. This pass does not claim
200% scaling, measured product-wide contrast, narration, screen-reader support,
physical-controller parity or full XAG compliance. Those require a broader
surface/device design and acceptance effort rather than changing a font number
in isolation.

## Authoritative gameplay and framework compatibility

### Separate repeated requests from changing authority

Microsoft's [PlayFab transaction and retry guidance](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/idempotent-transactions-and-retries)
distinguishes duplicate-operation protection from concurrency checks against
current inventory. Its service-specific implementation is not adopted here.

**Terra decision:** use the existing command sequencing and current-state
authority. A repeated dissolution cannot delete a later faction or create
refunds. A native review retains the exact faction/player/restore/roster context;
the host rechecks the actual ownership ledger before mutation. A spatial client
view cannot prove that no distant public assets exist.

The chosen dissolution policy is a product decision: only an active sole Leader
may dissolve a faction with no shared plot or settlement territory. Outstanding
invitations are revoked, IDs remain monotonic, and no money, stock or property
moves. Occupied communities, public funds, succession, elections and reclaim
remain separate policies. This closes the empty-community dead end without
silently deciding how other players' public assets should be distributed.

### Preserve fixed authority and explicit compatibility boundaries

The [official Bevy fixed-timestep example](https://bevy.org/examples/movement/physics-in-fixed-timestep/)
separates per-frame input, fixed simulation and visual interpolation. The
[Cargo SemVer reference](https://doc.rust-lang.org/cargo/reference/semver.html)
explains that new variants of exhaustive public enums can affect downstream
matches. The [Rust non-exhaustive reference](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute)
describes a deliberate API design choice, not a compatibility-free retrofit.

**Terra decision:** retain existing fixed physical/economic scheduling. Filter
dormant-owned bodies out of native ecology observers to align the adapter with
its active-owner semantics; do not change population rates or saved positions.
Document the new public command variant as a 0.22 SDK source change. Advance wire
protocol to 11 and TLS ALPN to `terra-world/11`, while retaining domain save 1,
regional database 1 and asset schema 1. Do not disguise wire incompatibility as a
patch or infer data migration from a matching file-format number.

## Publication, recovery and writer ownership

### File visibility and synchronization are different boundaries

The [Linux fsync manual](https://man7.org/linux/man-pages/man2/fsync.2.html)
states that file synchronization does not itself necessarily synchronize the
containing directory entry. Directory synchronization is a separate operation
and can fail. Microsoft's [ReplaceFileW documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)
documents unsupported write-through behavior and meaningful failure states.
[Windows buffer-flush documentation](https://learn.microsoft.com/en-us/windows/win32/fileio/flushing-system-buffered-i-o-data-to-disk)
also distinguishes buffered writes from explicitly flushed data.

**Terra decision:** one small shared publisher distinguishes failure before
namespace publication from publication whose durability could not be confirmed.
The first outcome preserves the previous destination; the second retains the
new visible output and reports uncertainty. Private staging, exclusive creation,
file flush and replace/no-clobber intent remain explicit. Unix parent-directory
sync is checked, including relative basenames and newly created parents.

Recovery needs this distinction across layers. A completed bundle must not be
deleted merely because a later final-marker synchronization fails. Conversely,
an inner snapshot is not enough to call an outer managed backup complete. Flush
validated payload trees before the relevant final marker, track that publication
stage structurally and preserve useful completed inner artifacts without
claiming a missing wrapper exists.

Fault injection tests program behavior at these boundaries. They do not prove
power-loss survival on every filesystem, device cache or Windows installation.
The non-Unix contract remains documented separately; an unsupported flag is not
used to manufacture a stronger promise.

### Hold a session lease, not a stale PID marker

[Rust File locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
provides nonblocking exclusive acquisition and handle-lifetime release. Its
documented current mappings use Unix flock and Windows LockFileEx.
[Microsoft LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex)
documents exclusive, fail-immediately locking and handle/process behavior.

**Terra decision:** a stable private per-profile lock file is held through
managed play and final metadata publication. A second participating runtime
fails before mutating progress. Do not delete a lock file on release: replacing
the inode would allow different processes to lock different objects with the
same pathname. An existing unlocked file is harmless; the OS lock decides
ownership. The launch witness continues to mean initialization was admitted,
not that a session is still running or that a checkpoint succeeded.

This is cooperative protection among current managed runtimes. Old binaries,
arbitrary direct SDK writers, administrator path replacement and network
filesystems are outside that guarantee. Windows still needs actual execution
of the same contention/crash-release and native play matrix.

### Preserve transactional backup and complete ownership data

[SQLite's online-backup API](https://sqlite.org/backup.html) provides a consistent
database snapshot through the database engine. [SQLite WAL documentation](https://sqlite.org/wal.html)
describes its sidecars, checkpointing and local-host coordination assumptions.

**Terra decision:** retain the existing SQLite backup API, committed-generation
checks and full saved-state validation. Do not copy only the main file of an
active WAL database or treat a live/remote subset as the complete world. Maintain
source/catalog identity and complete recovery dependencies. No external
dependency upgrade follows merely from generic advice; a concrete versioned
issue must first apply to the locked dependency.

## Delivery integrity and its limits

[Python pathlib](https://docs.python.org/3/library/pathlib.html) documents path
resolution and filesystem metadata operations; path strings alone do not
establish containment or regular-file identity. [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use)
separates least privilege and immutable dependencies from artifact handling.

**Terra decision:** provide a standard-library-only verifier for a clean extracted
desktop package. It checks bounded inventories, portable names, aliases,
links/reparse points, regular files, hashes, binary architecture and the terminal
receipt association. Optional `--help` execution is explicit, host-matched and
time-bounded. Verification does not require Rust, touch saved worlds, install
drivers or download DLLs.

A self-contained hash manifest can detect inconsistency but cannot authenticate
the publisher against a coordinated replacement. Keep signing, trusted release
discovery, authenticated distribution and rollback in G11. Likewise, a checked
Windows workflow or API document is not an executed Windows qualification. The
Linux package must name its actual build profile, host and graphics scope.

## Real-world data and content provenance

[RFC 7946](https://www.rfc-editor.org/rfc/rfc7946.html) specifies GeoJSON
longitude/latitude order, WGS 84 geographic coordinates and the meaning of an
optional height coordinate. [OpenStreetMap's copyright page](https://www.openstreetmap.org/copyright)
and the [OSMF attribution guidance](https://osmfoundation.org/wiki/Attribution_Guidelines)
describe source credit and licensing expectations for the form in which data is
used or distributed.

**Terra decision:** retain source identity, attribution and the visible distinction
between sourced geography and authored/synthetic population. A real GPS location
does not make its businesses, residents or economic activity observed facts.
Keep dataset-specific vertical datum, NoData, topology and redistribution review
in G10. Do not import a new shipping dataset or reinterpret saved source geometry
merely as part of interface polish. Existing original sample assets and their
notices remain in both appropriate deliveries; production art/audio rights and
geography admission need their own content inventory and fixtures.

## Decisions intentionally left open

The research does not choose a production economy balance target, public-service
account model, unavailable-leader succession rule, public treasury distribution,
global partitioning strategy, licensed geographic dataset, signing identity,
offsite owner or physical hardware support matrix. These depend on product and
deployment requirements plus observations not supplied by this increment.
Their complete closure criteria stay in the current G01–G18 register.

Implementation results, cross-review corrections, test counts and accepted
artifact hashes belong to [Validation](VALIDATION.md). The specialist reports
retain detailed evidence paths and rejected hypotheses, including transaction
code that was already protected by an existing candidate-state commit.
