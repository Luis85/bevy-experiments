# ADR 0003: `SimId` identity and ordering

- Status: Accepted
- Date: 2026-10-23
- Source: villager spec §6.2, `docs/research/ai-prior-art.md` (Determinism)

## Context

Bevy `Entity` ids depend on allocation history and differ across machines. Query iteration order depends on
archetype history: inserting one extra component on a client changes it. Anything order-sensitive (think
order, reservation claims, effect application, hashing) would then differ between peers.

## Decision

- Every sim entity gets a `SimId(u32)` from a sim-owned counter (`SimIdAllocator`), plus a
  `SimId → Entity` index resource (`SimIndex`).
- Anything order-sensitive iterates **sorted by `SimId`**: think, effects, hashing, inspect and the journal.
- Order-sensitive collections are `BTreeMap`s or sorted `Vec`s keyed by `SimId` (e.g. `Reservations`).
- Commands, journal entries, replays, inspect output and BRP refer to units by `SimId`, never by `Entity`.
- No presentation components on sim entities (they would change archetypes and therefore order).

## Consequences

- Order is deterministic regardless of spawn/despawn history or what the view adds.
- A small cost: collect and sort where Bevy would iterate directly.
- Tools and players address units by stable ids that survive replays.
