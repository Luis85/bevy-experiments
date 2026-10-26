# Final review: simulation and capability crates (2026-10-24)

**Determinism:** clean. **ADR 0011/0012:** clean. **Opt-in:** clean.

Proof tests for findings 1–5: `scratchpad/review-sim/tests/findings.rs`.

## Bugs (fix in final wave)
1. **Nav queue starvation** (`npc_nav/src/plan.rs:208, 249-251`).
   - Any `GridChanged` restarts the paused head search.
   - With constant grid churn, the long head search never finishes, so every request behind it stays Pending forever.
   - `decide` re-queues all Failed agents on any change, which makes it worse.
   - Fix (pick one):
     - restart only if a changed cell was seen by the search;
     - cap the work per request;
     - serve the queue round-robin.
2. **Despawned holder orphans its items.**
   - The items keep `CarriedBy{dead}` and have no SimPos.
   - Fix: an interaction leave hook drops the items at the holder's last position.
3. **Despawned carried item leaves a ghost id** in Inventory/Equipment, blocking the slot forever. Fix: the same leave hook removes the id.
4. **`close` checks the doorway against the stale grid index** (`builtin.rs:315-329`).
   - The door can close on a unit that entered this tick, trapping it (StartBlocked).
   - Fix: check live SimPos.
5. **NavAgent with its own GridBlocker can never plan** (`astar.rs:161`, StartBlocked).
   - This affects Direct vehicles with a blocker.
   - Fix: treat the agent's own footprint as open.
6. **Craft re-picks inputs/tools at completion** instead of using the claimed ones (`npc_crafting/src/run.rs:570`).
   - Fix: keep the planned `(id, units)` in `Crafting` and use those.

## Cleanups
1. **CommandLog key collision.** It stores `(tick_applied, original_seq)`, so a mid-tick submit can collide with a later command. Fix: log the original key, or assert that `submit` is not called inside `NpcSimTick`.
2. **CreateSquad.formation spacing is not validated.** Also clamp derived slots to POS_LIMIT.
3. **Despawned squad leaves `MemberOf(dead)` on its members.** Fix: prune them.
4. **per_roll dice key collides** for SimId ≥ 2^30 and for noises without a source. Fix: add the noise's sort index to the key.
5. **Pending `Messages<Noise>` across the tick boundary are not hashed.**
6. **Unhashed state:** the NavQueue hash covers `paused_nodes` only, and `Brain.tree` is not hashed.
7. **PostCraftOrder has no cap** per workstation.
8. **Footprint size is not clamped** to the grid bounds.
