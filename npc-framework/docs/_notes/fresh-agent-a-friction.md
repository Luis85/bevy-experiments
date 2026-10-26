# Fresh agent A friction log: village thirst + well (2026-10-24)

**Outcome:** the task succeeded content-only. There were no changes under `crates/`, the re-baseline was staged with evidence, and behaviour stayed sane: 1–2 drinks per day and no flip-flop. Lives in the isolated worktree `C:\Projects\.claude\worktrees\agent-a198fa7b0aa7f2f3e`; not merged.

## Fix in FX4 (docs/skills/tests)
1. **There are two re-baseline procedures.** CLAUDE.md and debugging.md §8.2 have the user create the marker and the agent regenerate. The procedure actually used is the stage → evidence → approval flow, which only the plans describe. Unify on stage-then-swap in CLAUDE.md, debugging.md and the `/add-mechanic` skill, with exact commands.
2. **New entities shift SimIds.** Nothing documents this, yet ids 6/7 are hard-coded in about 10 tests and in replay commands, and `corpus_equivalence.rs` requires identical commands.
   - Document the rule: spawn new scenario entities after existing ones.
   - Tests should look ids up via `VillageIds` or by name instead of hard-coding them.
   - Let `corpus_equivalence` accept documented id remaps.
3. **"Content only" isn't quite true.**
   - New `PlaceKind`/`Activity` variants force edits in `village_view` and in app tests.
   - The npcctl run summary hard-codes hunger and energy. It should report all registered needs generically.
4. **Content can't register perf budgets without a core edit.** Village budgets live in `npc_perf`'s `NPC_BUDGET_TABLE`. Add `register_budget` / `NpcPerfPlugin.budget()` for content, and move the village rows out of core.
5. **One golden failure hides the rest.** The test stops at the first failing snapshot, so later `.snap.new` files never get written. Split it into one test per snapshot, or use `insta::with_settings!` + `allow_duplicates` so every snapshot is checked.
6. **Hooks don't load when launched at the repo root.** Also, `village_perception.rs` hard-codes the corpus content hash, and no doc lists it among the tests that fail on re-baseline.
7. **`apps/npcctl/tests/agent.rs::serve()` leaks the server process** when a test fails. It held the target exe locked; I killed PID 38400. Add a kill-on-drop guard.
8. **Smaller items:**
   - Document that need ids are sorted by name.
   - Add a multi-slot reservation helper (`free_for` only handles capacity 1).
   - Document that `SetNeed`'s level updates on the next tick.
   - Document how insta stages `.snap.new` (CI unset).
9. **Document gate timing:** a full verify takes about 40–50 min on a busy machine; for the fast loop use `cargo test -p <crate>`.
