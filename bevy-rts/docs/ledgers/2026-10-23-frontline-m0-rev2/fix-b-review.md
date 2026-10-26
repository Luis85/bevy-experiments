# Fix wave B review (56e3425..8ac5de4)

### Spec Compliance
- Spec compliant. All 13 items have a hunk. Items 10 and 13 have tests that are weaker than the brief implies (see Minor 1, 2).
- Cannot verify from the diff: `gate pr` result and the Lifecycle re-measure (0.78 ms mean vs 29.7 ms before). Both are taken from the report.

### Checks run
- Clippy bans resolve. In `bevy_ecs-0.20.0/src/entity/` the structs `EntityEquivalent{HashMap,HashSet,IndexMap,IndexSet}` are defined in `hash_map.rs`, `hash_set.rs`, `index_map.rs` and `index_set.rs`. The modules are `pub mod` and ungated. `EntityHashMap`, `EntityHashSet`, `EntityIndexMap` and `EntityIndexSet` are plain `type` aliases, so banning the newtypes is the right choice.
- `bevy_platform::hash::RandomState` is a `pub use foldhash::fast::RandomState` re-export. Clippy resolves it to the underlying item, so the ban works. `std::hash::RandomState` exists.
- The generator skips paths outside a crate's dependency graph. The root `clippy.toml` is unchanged, and the new bans are on the `sim` and `edge` profiles only.
- `arch.rs::banned` handles `Role::Sim | Role::Feature(_)` before `is_headless`. The allowlist is a bevy-prefixed crate that is in `ECS_FAMILY` or `APP_FAMILY` (`bevy_app`, `bevy_derive`). `HEADLESS_BANNED` and the physics/RNG crates are denied on top of that. Tests:
  - `sim_and_feature_trees_are_an_allowlist`
  - `tree_flags_physics_but_allows_transitive_rand`
  - fixture `headless_direct_rand_and_physics_deps_flagged` (like `headless_dev_dep_on_client_flagged`), which covers a direct `rand`/`rand_*`/`bevy_rand`/`rapier*`/`avian*` dep in fl_sim, a feature, fl_cli, fl_math and fl_content, plus a client crate that is allowed.
- The `rand` ban is enforced on direct dependencies only, because `bevy_ecs` brings `rand` transitively. The report states this. It is a reasonable reading of the brief's "deny `rand*`".
- `no_match_outcome` fails `gpu`, `brp` and every row at milestone level. `gpu` and `brp` run only at `DEVICE` levels (nightly, milestone), so they cannot affect `gate pr` or `gate task`. Task and pr still warn. Test `empty_match_fails_device_and_milestone_rows` covers both sides.
- `check_perf_json` finds the `rows` entry with `units == 3000` and fails if `mean_ms` > 15, if the entry is missing, or if the JSON is invalid. The committed M0.json has rows 500/3000/5000 with 3000 = 4.43 ms, and `committed_m0_baseline_passes_g7` uses `include_str!`. The boundary at exactly 15.0 passes (matches "> 15").
- `OWNERS`: `docs/perf/M*.json` is protected. The test checks M0 and M12 are protected and M0.md is not. The contract track now lists the four fl_sim files, and all four exist.
- `check_budgets` now takes the union of the two name sets. The round-trip test has the `c.n ... not measured` expectation, and there is a dedicated test.
- The system-order test is renamed, and the explicit `"sim_tick"` name in `insta::assert_snapshot!` is unchanged, so the snapshot file is unchanged.
- The `fl_content.txt.new` listing is sane to bless. It has 348 lines in `cargo public-api` format and covers `ContentDb`, `DefRegistry`, `ContentKind`, `DefComponent`, `RulesSection`, the loaders and the `Placeholder` enum.

### Strengths
- Banning the defining newtypes rather than the aliases was verified against the sources.
- The `rand` tradeoff is explained and tested both ways.
- `startup_config` returns `(cfg, run)`, so `run.phase_markers` is derived from the loaded config by construction. `observe_setup` now takes `&DebugConfig` and runs only after `start_sim` succeeds.
- The flag-over-env precedence test covers empty, garbage and out-of-range values.
- The report discloses honestly that the empty-run-dir test was written after the fix.

### Issues

#### Critical
None.

#### Important
None.

#### Minor
1. `crates/fl_game/src/builder.rs`, test `windowed_startup_reads_debug_ron_before_the_sim_starts`: it exercises `startup_config`, not `build`. Reversing the order inside `build` or passing `RunConfig::default()` to `start_sim` would not fail it. Ordering is only implied by the data dependency `run` -> `start_sim`. It pins the config-to-markers mapping, not "loaded before the sim starts". A stronger pin would observe `build(Windowed)` itself, for example through a recorded `phase_markers`.
2. `builder.rs`, test `content_error_leaves_no_run_directory`: it was never run red. By inspection it would fail on the old order, because the old code created `logs/<run-id>` before `start_sim`. `map_or(0, ...)` means a missing logs dir also passes, which is correct here.
3. `crates/fl_game/src/main.rs`: `remote_port` is unit-tested, but the wiring in `Cli::builder` (`std::env::var(PORT_ENV)` passed in) is not tested end to end. The report says it was verified live. The `rustfmt` shape of the `GameBuilder::new()` chain also changed noisily.
4. `docs/debugging.md:299` still shows the concrete `0x4c4acd04ce1d032a` in the plain-text bisect output. The JSON example just above now uses `<hash>`, so the doc is inconsistent. `docs/m0-demo-checklist.md:44` still pins both hashes, but that file is out of the fix-b paths and is historical evidence.
5. `xtask/src/arch.rs`, `denied_direct` runs for dev-dependencies too. This is intended ("any dep kind"), but a future legitimate `rand` dev-dep in a tool or test crate will need an arch exception. The direct-`rand` ban for headless roles is the only RNG guard, since the `rand::rng` and `rand::random` bans in the clippy template are inert (reported).
6. `xtask/snapshots/api/fl_content.txt.new` (for the lead before blessing): `DefRegistry` is exposed as a bevy `Component` as well as a `Resource` (lines around 1321). Check whether the `Component` derive is intended before pinning it.
7. `fl_sim::spawn=info` also silences the despawn-of-missing-entity DEBUG record. This is disclosed in the report and noted in the `debug.ron` comment. Acceptable.
8. Gate evidence: the report's `gate pr` run was on the pre-split tree. The failures `protected` and `api` are the expected ones. 4870337 needs a `Blessed:` trailer, and `fl_content.txt.new` needs a bless.

### Assessment
**Task quality:** Approved
**Reasoning:** Every requested guard, tool change and doc fix is implemented with sound, source-verified clippy paths and tested arch-check, gate and budget logic. The only gaps are two tests that pin helper functions rather than the `build` wiring, and doc leftovers, all Minor.
