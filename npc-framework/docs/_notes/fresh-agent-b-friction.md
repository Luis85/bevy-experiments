# Fresh agent B friction log: RTS engineer repair (2026-10-24)

**Outcome.** The task succeeded using only the facade:
- full verify ok (490 tests);
- `rts_demo_uses_only_facade` passes;
- no changes to `crates/` or `content/`.

The work lives in an isolated worktree (`C:\Projects\.claude\worktrees\agent-a6da6124100eba954`) and was not merged.

## Facade/API gaps (fix in FX4)
1. **No custom order kinds.** `UnitOrder` is closed and `register_order_executor` panics outside `KINDS`. Add `UnitOrder::Custom { name, args }` with executors by name, and/or chain `Interact` executors per verb, so a game verb can coexist with the interaction glue.
2. **One executor per kind.** Add per-verb dispatch.
3. **Right-click resolvers can't narrow the selection.** One unit failing `validate` rejects the whole `IssueOrder`. The resolver should return `(units, order)`, or `IssueOrder` should gain a "skip invalid units" mode.
4. **No `AppExt` for resolvers.** Registering a `RightClickOrders` / `ContextOrders` resolver depends on plugin order and is undocumented.
5. **No "pickable but not selectable" marker** for right-click targets such as walls.
6. **Spatial queries ignore footprints.** Add `SpatialGrid::cells_around(footprint)` and a nearest-blocker-by-edge query, plus an "approach footprint" helper that works with nav.
7. **Prelude misses journal items.** Export `JournalKind`, `JsonObject` and `set_journal`, and add a typed `record_effect(world, unit, &T)` helper. Document `CommandConfig` fields.
8. **No generic approach-then-act order helper.** Today it is locked inside `glue::command_interaction`.

## Docs/skills
9. **`/add-unit-type` covers only new types.** It does not cover:
   - adding a behaviour to an existing unit type;
   - the RTS autonomous branch placed between the order guard and idle;
   - executor vs tree.
10. **Executor lifecycle is undocumented.**
    - `validate` runs per unit and rejects the whole command.
    - `cancel` runs on Done and Failed as well.
    - The engine sits between Sense and Think.
11. **Second action systems.** Document that a second action system writing `ActiveLeaf` must be `.chain()`ed. List which framework systems share `NpcSet::Actions` / `Locomotion`.
12. **No small-scenario test harness in the RTS demo.** Point the docs to one.
13. **Gate timing expectations are missing.**
    - Fresh build: about 14 minutes.
    - Full verify: about 50 minutes.
    - Fast inner loop: `cargo test -p <crate>`.
    - Document these in CLAUDE.md.
