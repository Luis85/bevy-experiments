# Fix wave A, fix round 1: re-review (1099b76..c4f18f7)

## Finding verdicts

- **Important 1 (build.rs rerun-if-changed on missing paths)**: ADDRESSED. `crates/fl_sim/build.rs:229-250` emits `HEAD` only if it exists, the loose branch ref if it exists, else `packed-refs` and `refs/heads` only if they exist (`rerun_if_exists`). The Cargo FAQ case is gone. The report shows `Fresh fl_sim` on the second `cargo check -v` and no `packed-refs` in the emitted paths. Cases checked: a packed-only branch (a commit writes a loose ref under `refs/heads`, which Cargo scans recursively), a loose ref later packed (the watched file disappears, so the script reruns, then the packed arm applies), and a detached HEAD (`symbolic-ref` fails, so the `_` arm applies and the `HEAD` file still moves).
- **Minor 1 (dirty flag, ADR wording, old git)**: ADDRESSED as a wording fix. ADR-0007 R73 now says `-dirty` reflects the tree when `HEAD` or its branch last moved, and `build.rs:197-205` documents the same plus the git >= 2.31 requirement. The behaviour is unchanged, which was an accepted option. The stat-only false positive from `diff-index` is not mentioned anywhere (negligible).
- **Minor 2 (panic strings lost `\` continuations)**: ADDRESSED. `classify.rs:~311-313` and `~324-326` have the continuations back. The report says the author scanned for other run-together literals and fixed one more in `spawn.rs`. The new `spawn.rs` assert uses `\` correctly.
- **Minor 3 (`remap_command_id` doc/code mismatch)**: ADDRESSED. `commands.rs:~357-362` returns `Err("the save has no string table 'commands'")` when the table is absent, and the doc matches. Because `remap_command_ids` runs per command, a save or replay with no commands is unaffected. `save.rs` test `pending_command_without_a_commands_table_fails_to_load` covers it, and the report shows it red before the fix and green after.
- **Minor 4 (ChildOf/Children/Observer checked only on component growth)**: ADDRESSED. `check_tick` now runs `forbidden_in_world` every tick (`classify.rs:~317-320`). It does 3 `get_id` lookups plus an archetype scan with `any`, and does not allocate when clean (an empty `Vec`). The test `forbidden_component_instanced_mid_match_fails_its_tick` registers both types at build through a query and spawns the child in tick 1. The expected message matches the format string. The test is `cfg(debug_assertions)` like the check.
- **Minor 5 (`Rolls::new` / `stream_key` bypass of `Stream`)**: ADDRESSED. `Rolls::new` and `fl_math::roll` are banned in the `sim` profile (`xtask/clippy-template.toml`, the three regenerated `clippy.toml` files). The rng module doc was reworded to the guarantee that actually holds. `stream_key` stays public with a stated reason. Both paths exist in `fl_math` (`rng.rs:132` and `:152`).
- **Minor 7 (SpawnRequest/DespawnRequest written after the funnel dropped silently)**: ADDRESSED. `spawn::assert_requests_drained` is called in `step_tick` under `cfg!(debug_assertions)`, after the output is built and before `clear_message_buffers` (`step.rs`). The test `spawn_request_after_the_funnel_fails_its_tick` writes in `Publish/Events`. The `spawn_funnel` doc is updated. `Messages::len` counts both buffers, and the funnels `drain()`, so a non-empty buffer there means a lost request.
- **Minor 8 (`SaveError` not `#[non_exhaustive]`)**: ADDRESSED. `save/mod.rs:522`. The only external match is in `tests/save.rs:653`, which is an integration test with a struct-variant pattern and compiles with `non_exhaustive`.
- **Minor 9 (`hash_independent_of_spawn_order` no longer spawns in opposite order)**: ADDRESSED. The new unit test `hash.rs::tests::hash_independent_of_spawn_order` spawns #1,#2 against #2,#1 after 3 filler spawn/despawn cycles. I traced the entity indices (index reuse bumps the generation, and #1 lands at N in one world and N+1 in the other), so its `assert_ne!` on the entities holds. The hashes must then be equal. The integration test is renamed `hash_independent_of_entity_bits`, which is honest.
- **Minors 6 and 10**: parked as instructed, not reviewed.
- **Stale docs (`.claude/rules/sim.md`, `.claude/agents/reviewer.md`)**: ADDRESSED. Randomness, Classification (one `register_rebuild` in feature load order; Transient enforced; `TransientKept`), schedule (`add_message` only, end-of-tick clear, no Housekeeping rotate), `step_tick` order (clear message buffers before `clear_trackers`), and `RejectReason::Feature` all match the wave-A code. I found no remaining stale wording in the grep of both files.

## New breakage in the fix diff

None Critical or Important. Notes:

- Clippy bans and `#[expect]`s (Minor, no action). They are justified and narrowly scoped:
  - `rng.rs:480` on `rolls()`, the single place a `Stream` becomes a cursor.
  - `rng.rs:493` on one unit test that does call `Rolls::new` at line 81, so the `expect` is fulfilled.
  - `scale.rs:548` on the single `let` that calls `Rolls::new`. This is the only other call site in the workspace outside `fl_math` (grep confirmed). The `feature.rs` and `scale.rs` doc mentions are comments.
  - The ban is in the `sim` profile only, so tools and the client are unaffected. `fl_math` itself is not in that profile.
  - The ban does not cover the `Rolls` draw methods or struct construction other than `new`. Acceptable, since the cursor can only be created through `new` or `rolls`.
- The debug-only panic in `step_tick` (Minor, informational): release builds still drop a late request silently, so debug and release behaviour differ on that path. That is the same trade-off as the other debug checks, and the doc says so.
- The `hash.rs` unit test (Minor, informational) does `std::mem::take(world)` on the app's world to return an owned `World`. This is fine for the hash check, but the `App` is left with an empty world.

## Out-of-scope observations

- `.claude/rules/sim.md` "Banned" list (line ~93) does not name the new `Rolls::new`/`roll` bans, although the Randomness bullet does. This is cosmetic.
- The `xtask/snapshots/api/fl_sim.txt.new` is stale by one line (`SaveError` `non_exhaustive`). This is known, and the lead regenerates it.

## Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage. The gate and test failures reported are the unblessed `.new` artifacts only. The covering tests are named in the report with RED/GREEN evidence.
