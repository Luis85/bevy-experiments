# Fix wave B report (worktree C:\fl\fixb, branch m0/final-fix-b, 56e3425..8ac5de4)

Commits (oldest first):
- 17f2e4b feat(xtask): item 1 (I5) + item 2 (I8); `.claude/rules/sim.md` line
- 7b17efa feat(xtask): items 3, 4 (gate part), 7 (OWNERS contract files), `owners.rs` test
- 4870337 docs(perf): M0.json head SHA (item 4, first half). **Needs the lead's `Blessed:` trailer** (see Gate)
- 54b50db test(fl_test_utils): items 5, 6
- 12394e6 chore: items 7 (fl_content API `.new`), 8, 9
- 2c18264 fix(fl_game): items 10, 11, 12, 13 (+ M0.md note, debug.ron, DebugConfig default)
- 8ac5de4 style: cargo fmt

## Per item
1. **I5.** Template bans (sim profile): `bevy_ecs::entity::{hash_map::EntityEquivalentHashMap, hash_set::EntityEquivalentHashSet, index_map::EntityEquivalentIndexMap, index_set::EntityEquivalentIndexSet}` (the defining newtypes; the `Entity*` names are aliases that clippy does not match), `hashbrown::{HashMap,HashSet}`, `indexmap::{IndexMap,IndexSet}`, `std::hash::RandomState`, `bevy_platform::hash::RandomState` (hashbrown, indexmap and the RandomStates also on the edge profile; the generator drops paths outside a crate's graph). Regenerated the 5 per-crate `clippy.toml`. `.claude/rules/sim.md`: never key/order sim state by `Entity`, use `SimId`; ban list updated. Workspace clippy `-D warnings`: no new hit anywhere (fl_sim included).
   Pre-existing, not touched: clippy warns in fl_sim that the `rand::rng` ("expected a function, found a module") and `rand::random` ("not reachable") bans do not resolve (rand 0.10 comes in via bevy_ecs without its thread_rng feature). Those two bans are inert; the arch-check change below covers direct `rand` deps. Suggest fixing or dropping them later.
2. **I8.** `arch.rs`: `Role::Sim`/`Feature` trees are an allowlist = `ECS_FAMILY` + `APP_FAMILY` (`bevy_app`, `bevy_derive`; enumerated from `cargo tree -p fl_sim --all-features`, the only Bevy crates beyond the ECS family) plus the existing renderer denylist. `bevy_math/transform/asset/log/diagnostic` are now flagged. `bevy_rand*`, `rapier*`, `avian*` are denied in every pure/content/headless tree. `rand`/`rand_*` are denied as **direct** dependencies only (check_edges, any dep kind): `bevy_ecs` brings `rand` transitively (and proptest `rand_xorshift` in the tools), so a tree-wide rand ban would always fire. Tests: `headless_direct_rand_and_physics_deps_flagged` (fixture, like `headless_dev_dep_on_client_flagged`), `sim_and_feature_trees_are_an_allowlist`, `tree_flags_physics_but_allows_transitive_rand`. `cargo xtask arch-check`: OK (12 packages).
3. **Minor 1.** `gate.rs::no_match_outcome`: Fail for rows `gpu`, `brp` and every nextest row at milestone level; Warn otherwise. Test `empty_match_fails_device_and_milestone_rows`.
4. **Minor 2.** `docs/perf/M0.json` meta gets `"head": "56e3425"` (separate commit); OWNERS `[protected]` gains `docs/perf/M*.json` (owners.rs test: M0.json and M12.json protected, M0.md not). `perf_file` parses `docs/perf/M<n>.json` and fails if there is no `rows` entry with units=3000 or its `mean_ms` > 15. Tests `perf_file_checks_the_3000_unit_mean`, `committed_m0_baseline_passes_g7` (M0.json: 4.43 ms).
5. **Minor 3.** `check_budgets` fails "`<scenario>: <name> has budget N but was not measured`" for a budgeted name missing from `per_tick_max` (merged and name-ordered with the other errors). Test `budgeted_counter_absent_from_the_run_fails`; the round-trip test is extended. perf_counters tests (500/3000/5000) still pass.
6. **Minor 5.** Test renamed `smoke_sim_tick_system_order`; the snapshot name was already explicit (`"sim_tick"`), file `system_order__sim_tick.snap` unchanged, no `.snap.new` produced.
7. **Minor 6.** `API_CRATES` += `fl_content`; `cargo xtask api` wrote `xtask/snapshots/api/fl_content.txt.new` (348 lines, committed so it survives the merge; **not blessed**). fl_math, fl_protocol, fl_view and fl_sim listings are unchanged. OWNERS contract track += `fl_sim/src/{hash,replay,spawn,commands}.rs`.
8. **Minor 13.** `fl_test_utils` moved to fl_cli `[dev-dependencies]` (Cargo.lock unchanged; `cargo build -p fl_cli --all-targets` ok).
9. **Minor 18.** `docs/debugging.md`: the pinned hashes in the hash-dump / fl.sim_state / fl.hash examples are `<hash>` placeholders; the check is now "the game's hash at tick 20 must equal the tick-20 line of `fl_cli hash-dump --mods assets/base --scenario base.testbed --ticks 20 --interval 20`".
10. **T30 wiring.** `GameBuilder::startup_config(shell)` (loads `debug.ron` for the windowed shell, derives `RunConfig.phase_markers = cfg.profiler`) runs first in `build`, before `start_sim(&run)`; new builder method `debug_ron(path)` is the seam. Test `windowed_startup_reads_debug_ron_before_the_sim_starts` (profiler true gives markers on, false gives off, headless gives no cfg and no markers).
11. **T30 logging.** `debug.ron` and `DebugConfig::default` filter: `info,fl_sim=debug,fl_sim::spawn=info` (no fl_sim edit needed); `repo_debug_ron_parses` pins it; debugging.md updated. **Re-measured Lifecycle** (dev build with `debug_remote`, `--stress 3000 --agent`, default debug.ron, `fl.perf` after 421 ticks, machine not idle): tick mean **13.1 ms** (p95 24.1), **Lifecycle mean 0.78 ms** (p50 0.53, max 9.3), versus 38.0 / 29.7 ms before. Recorded in docs/perf/M0.md. (Other phases: Act 2.77 ms, Perceive 7.49 ms mean.)
12. **T30 BRP_EXTRAS_PORT.** `--remote-port` is now `Option<u16>` (still `requires = "remote"`); `remote_port(flag, env)` = flag, else `$BRP_EXTRAS_PORT`, else 15702; an unparsable value is reported and ignored. Test `brp_extras_port_is_the_default_and_the_flag_wins`. Verified live: `BRP_EXTRAS_PORT=15728 fl_game --remote` logged "remote debug endpoint (BRP) ... port=15728" and answered `fl.sim_state`/`fl.perf` there. The debugging.md paragraph is rewritten.
13. **T30 empty run dir.** `observe_setup(logs_dir, cfg)` now runs after `start_sim` succeeds; config loading is separate. Test `content_error_leaves_no_run_directory` (windowed `build` with the bad_mod fixture and a temp logs dir: no entry created). Written test-after (green on first run); I did not run it red against the old order.

## Gate (`cargo xtask gate pr --track contract,tools,client,docs,perf,sim-core`)
Run on the tree before I split commits to isolate M0.json; the final tree is byte-identical (checked with git diff against the pre-split branch).
```
PASS fmt, clippy-drift, claude-md, check-scope
FAIL protected   docs/perf/M0.json: commit changes it without a `Blessed:` trailer (now commit 4870337)
WARN test-guard  6 findings, all intended: remote_port Some(..) assert, 5 log_filter asserts moved to the new default, renamed test
PASS clippy (-D warnings), arch-check, test-headless, test-client, deny, data-check
FAIL api         fl_content: new   (.new committed; needs `cargo xtask bless xtask/snapshots/api --reason ...`)
gate pr: FAIL (protected, api) in 924s
```
Both failures are the protected artifacts the brief says to leave to the lead: bless `fl_content.txt.new`, and give 4870337 a `Blessed:` trailer (it is a standalone commit, so reword it). After that `gate pr` should be PASS. The `cargo fmt --all` fixes found at the start of the gate run are in 8ac5de4; fmt passed in the gate run itself.

## `.new` artifacts
- `xtask/snapshots/api/fl_content.txt.new` (committed in 12394e6).
- No `.snap.new`, no `budgets.ron.new`.

## Concerns for the lead
- Pre-existing inert `rand::rng` / `rand::random` clippy bans in fl_sim (item 1).
- `fl_sim::spawn=info` also hides the other fl_sim::spawn DEBUG record (a despawn request for a missing entity); use `RUST_LOG` or edit debug.ron to see it.
- T30 Important 2 (gate `unclassified_sim_type_fails_build` with `cfg(debug_assertions)`, drop the 212/213 caveat in the checklist) is in fl_sim and the checklist, outside my paths.
