---
status: "accepted"
date: 2026-10-23
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N4/C5; baseline-recommendations.md A4"
---

# Stable-string registries for every runtime ID

## Context and Problem Statement

Revision 1 interned *content* IDs by sorted string (§6.1) but left the other runtime IDs unspecified:
`CommandTypeId(u16)`, def-component IDs, hashed-component order, and later stats, conditions, tags, effects, RNG streams
and journal kinds. If any of them follows registration order, then adding, reordering or feature-gating a plugin
silently renumbers them. Replays, saves, the network protocol and state hashes all break.

## Decision Drivers

* Every machine and every build with the same feature set assigns the same IDs.
* Adding an unrelated feature must not perturb existing scenarios' state hashes (change class N).
* Replays and saves must survive ID renumbering across builds.
* Dense IDs are still wanted at runtime (array indexing, compact wire format).

## Considered Options

* Registration-order IDs
* Sorted-name dense IDs, with string tables in replays and saves
* Hash-of-name IDs everywhere (no dense IDs)

## Decision Outcome

Chosen option: "Sorted-name dense IDs, with string tables in replays and saves", with one refinement: **anything
hashed uses the stable name key, not the dense ID**.

- One generic `StableRegistry<K>`: register by a namespaced `&'static str` (`"core.move"`, `"infantry.lie_down"`;
  the namespace is the feature id). `freeze()` in `finish()` sorts the names and assigns dense IDs. Registering after
  freeze, or a duplicate name, panics with both registrants named.
- It applies to command kinds, def components, sim components and resources, content kinds, rules sections, RNG
  streams, journal records and view channels; from M2 also stats, conditions, tags, effects and triggers. Content IDs
  (units, factions, …) already follow the same rule.
- The frozen tables form `StringTables` in the `MatchHeader` (ADR-0007). Replays and saves carry them. Loading maps
  names to current IDs; an unknown name is an error, or is migrated (ADR-0006).
- **Hashing never uses dense IDs.** The state hash writes a registry-backed ID as the stable 64-bit hash of its name
  (precomputed per table). RNG streams use the name hash directly (ADR-0003). Otherwise a feature that adds a name
  sorting earlier would change the hash of scenarios that never use it.
- Clients learn the tables from the match header and encode commands by name lookup.

### Consequences

* Good, because IDs are build-independent for a given feature set, and portable across sets through the tables.
* Good, because unrelated features don't perturb existing hashes.
* Bad, because hashing a registry-backed field costs a table lookup. It is negligible at the hash cadence.
* Bad, because every registry needs a freeze step and a table export. One generic type covers all of them.

### Confirmation

* Tests: IDs sorted by name (`core.move < core.stop < debug.spawn < debug.wander`), duplicate and post-freeze
  registration panic, a string-table round trip through the header, and loading a replay recorded with an extra
  registered name.
* A golden-stability test: a dummy feature registering an early-sorting command, component and stream leaves an
  existing scenario's hash stream unchanged.
* Public-API snapshot of the registry types (ADR-0011).

## Pros and Cons of the Options

### Registration order

* Good, because it is trivial.
* Bad, because it is build-dependent and breaks replays, saves and netcode.

### Sorted names to dense IDs plus string tables

* Good, because it is deterministic, compact, and remappable.
* Bad, because of renumbering when names are added, which is handled by the tables and by name-keyed hashing.

### Name hashes everywhere

* Good, because it never renumbers.
* Bad, because 64-bit IDs are bulky in components and on the wire, can't index arrays, and need collision handling.

## More Information

* Spec §5.4 (stable-string registries, state hash), §5.5 (`MatchHeader`, command encoding).
* Research: `architecture-best-practices.md` N4, C5; `baseline-recommendations.md` A4.
* Related: ADR-0003, ADR-0006, ADR-0007, ADR-0012 (an ID-set change on hot reload rebuilds the match).

## Refinements (2026-10-23)

* **R54: name-keyed presence in the state hash.** `compute_state_hash` writes only the `Hashed` state that exists:
  the tick; the number of `Hashed` resources the world has, then each in name order as its name key and data; the
  entity count; then per `SimIndex` entry the `SimId`, the number of `Hashed` components it has, and each in name
  order as its name key and data. Absent state contributes nothing, so a feature that registers a command,
  component, resource or stream a match never uses leaves that match's hashes unchanged (the golden-stability
  test, `fl_sim/tests/golden.rs`). The counts keep the encoding prefix-free: a zero-sized marker's presence (and
  which entity carries it) still changes the hash.
