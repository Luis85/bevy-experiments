# ADR 0019: Stable versions, item capacity and truthful presentation

- Status: Accepted (final review fix wave)
- Date: 2026-10-27

## Context

The final review found concrete gaps in ordinary crafting, optimistic file versions, diagnostics, coordinate
validation and carried-item rendering. Regression tests reproduced them before production code changed.
Corrections must preserve valid simulation inputs and every existing replay/snapshot oracle.
The final performance review also identified repeated LIDAR bitmap preparation and empty Plain-world runtime
systems. Their removal must retain complete sensor scans and the existing world construction contract.

## Decision

- Notes/map versions use a checked monotonically increasing per-entry revision on every mutation. Caller tick
  and wall-clock date can move backward or repeat; neither is a unique version. The revision survives file
  writes/reopens through the existing opaque stamp representation. Existing created stamps stay unchanged;
  exhausted revisions reject rather than repeat. Clients continue returning the exact updated stamp.
- Crafting retains its existing separate-output/no-merge SimId model. A matching stack does not count as space
  for a new separate item: both an actual free inventory slot and weight capacity are required. Equipped items
  retain their existing slot exemption. Overflow outputs land on the ground.
- Carried crafted outputs emit the existing ItemMoved journal kind so ordinary inventory-delta observations
  see them. CraftDone gains optional dropped-item metadata with fixed completion positions; old logs without
  it deserialize and empty metadata stays omitted. Journal enabled/disabled must produce identical state hashes.
- All concrete CLI/server token holders redact their Debug representation, including enclosing CLI output.
  Parsing, defaults, dispatch, HTTP behavior and credential formats stay unchanged.
- Position validation tests the inclusive supported range directly, avoiding signed-minimum absolute-value
  overflow. Valid coordinates preserve their existing behavior; malformed extremes reject without inputs.
- Item visuals follow real inventory/equipment state even when those items no longer have SimPos. Presentation
  entities remain separate. Inventory hides, equipped tools attach to the visible owner's hand/grip, and an
  unseen pickup retains the last-seen ground ghost until that cell is observed empty. Snapshot/startup and drop
  use the same lifecycle.
- LIDAR scratch caches the solid-cell bitmap by scan window, cell size, complete grid bounds, grid revision and
  the obstacle cells after actor exclusion. Subcell movement reuses those solids while tracing every beam from
  its actual origin. One byte holds `SOLID` and transient `VISITED` flags: every fog scan clears `VISITED`, a cache
  miss fills a clean bitmap, and range scans test only `SOLID`. Both paths use the same monomorphized DDA with
  unchanged endpoints, crossing fractions, conservative corner order and noise draws. The existing 9,000-case
  oracle body remains intact; additional regressions cover bitmap invalidation, snapshot replacement and shared
  scratch use across range/fog scans.
- Explorer runtime state and movement systems are scheduled only when a need is enabled or an enabled day/night
  cycle applies a sensing penalty. Plain explorer therefore schedules no empty survival/night update systems.
  A disabled night cycle also creates no unused actor base state merely because its stored range percentage is
  below 100. Snapshot component registration, observation/gate registration and clock setup remain unchanged;
  Plain retains its existing clock and simulation hashes, and active mechanics retain their systems.

## Consequences

Version tokens cannot repeat after a reset, capacity accounting matches actual placement, feedback describes
where outputs went, and visuals match legitimate item states. No dependency, oracle, budget or numerical-model
relaxation is introduced. Full verification and a new release checkpoint follow the bounded fix wave.
The scratch and scheduling changes retain semantic behavior; they do not establish a performance-budget pass.
Fresh timing evidence and its host conditions remain separate from correctness and oracle verification.
