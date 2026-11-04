# Polish pass plan (2026-11-04)

Follow-up to `docs/REVIEW_FOUNDATION.md` ("Status after hardening", "Tracked follow-ups") and `2026-11-04-foundation-hardening.md`. Branch `claude/horde-polish-pass`, cut from local `main` at `6177aff`.

## Constraints

- Change files only under `horde-defense/`. No push, no merge into `main`.
- Each worktree uses its own `CARGO_TARGET_DIR` inside that worktree.
- `horde_core` stays engine-free. Fixed ticks, stable IDs, serializable RNG.
- Gameplay outcomes for a given seed must not change. Golden digests change only with a stated reason, for example when the serialized form changes but the simulated outcome doesn't.
- Saves from v1 to v5 must keep loading. Any format change bumps the envelope and adds a checked-in fixture.
- Use TDD for core changes. Write the failing test first.

## Brainstorm: options considered

- **Level progress keys.** `first_clears: [bool; 5]`, `first_clear_claimed: [u32; 5]` and `unlocked: usize` all index `content().levels`.
  - Chosen: `BTreeMap<String, _>` / `BTreeSet<String>` keyed by level id, plus `unlocked` as the frontier level id. The v5→v6 migration uses the v5 level order, frozen as a constant.
  - Rejected: a `KindMap`. Levels are catalogue data, not an enum.
- **`BuyItem(usize)`.** Becomes `BuyItem(ItemDefId)`. Item definitions already have stable ids, so this is a command-only change with no save impact.
- **Private `Game` fields.** There are about 640 `.campaign` uses in the frontend, most of them reads.
  - Chosen: read accessors (`campaign()`, `attempt()`, …). Mutation goes through `apply`. Tests use `*_mut` accessors behind a `test-support` feature. This runs last among the foundation tasks because it touches every file.
  - Rejected: a parallel read-only "view" struct. It would duplicate state.
- **Drift warning.** It fires on every decode, including the verify-after-write in the save path.
  - Chosen: warn once per process, keyed by the (saved, current) pair, through a `OnceLock`/atomic in persistence.
- **Crash log tail.**
  - Chosen: the log writer keeps a bounded in-memory ring of the last N lines, shared with the panic hook. This avoids re-reading the file while panicking.
- **Fallback art lookup.**
  - Chosen: a `const` table keyed by `EnemyKind` (or a `KindMap`) holding tint and overlay flags. Rendering looks the entry up instead of matching.

## Waves

Wave 1 runs in parallel, in separate worktrees with disjoint files, and merges into the branch:

1. **Stable level IDs and save v6.** Files: `horde_core` campaign, command, persistence/migrations, and tests/fixtures (`save_v5.json` generated before the change, then a v6 migration test). Also the frontend call sites (`ui/armory.rs` BuyItem, `ui/notices.rs`, `ui/progression.rs`, camp tests). Also includes the drift-once fix, because it lives in `persistence.rs`.
2. **Diagnostics and art cleanup.** `logging.rs` crash tail; orphaned rig assets in `assets/` (verify that nothing references them, including scripts and docs); the data-driven fallback art in `art.rs`.
3. **Battle view model.** Move the remaining direct `attempt` reads in `ui/battle*.rs` onto `BattleViewModel` (`battle_view.rs`).

Wave 2 runs sequentially, after Wave 1 merges:

4. **Private `Game` fields** behind accessors, with a `test-support` feature for test mutators.

Wave 3 is the gameplay and UX polish:

5. **Playtest.** A native run, plus dev tools and F12 captures. Record the friction list below.
6. **Fixes.** Split by file area (battle HUD/feedback, campaign/armory, tooltips/empty states, pacing) and run in parallel where the files don't overlap.

Wave 4 is the full verify:

- fmt
- clippy for default, `--no-default-features` and `--features dev`
- tests for default and `--features dev`
- `--smoke`

After it passes, update `REVIEW_FOUNDATION.md`.

## Deferred (unchanged)

- RNG sub-streams and fixed-point positions.
- `ContentDb` injection.

## Playtest findings

(filled in during Wave 3)
