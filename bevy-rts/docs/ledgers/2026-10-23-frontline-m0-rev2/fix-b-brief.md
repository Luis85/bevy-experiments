# M0 final fix wave B — guards, tools, client, docs (lane 2, worktree C:\fl\fixb, branch m0/final-fix-b)

Tracks: contract, tools, client, docs, perf, sim-core (`--track contract,tools,client,docs,perf,sim-core`; gate base 56e3425 in `.gate-base`).
Build env: `export CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748` before every cargo/xtask call (shared lane-2 build dir, ruling R31).
Source: C:\fl\a\.superpowers\sdd\2026-10-23-frontline-m0-rev2\final-review-m0.md and task-30-review.md (same dir). Rulings R70–R74 in progress.md are binding.
Another builder works in parallel in C:\fl\a on fl_sim, fl_protocol, fl_catalog, fl_test_utils (except the two files below), docs/adr and the spec. Touch ONLY: xtask/**, OWNERS.toml, .claude/**, CLAUDE.md, the clippy template + generated crates/*/clippy.toml, crates/fl_cli/**, crates/fl_client/**, crates/fl_game/**, crates/fl_observe/**, debug.ron, docs/debugging.md, docs/perf/**, crates/fl_test_utils/src/perf.rs, crates/fl_test_utils/tests/system_order.rs.

Fix, TDD where behavioural:
1. I5 — sim clippy profile bans `bevy_ecs::entity::{EntityHashMap,EntityHashSet,EntityIndexMap,EntityIndexSet}` (use their defining newtype paths — check the bevy_ecs-0.20 sources), `hashbrown::{HashMap,HashSet}`, `indexmap::{IndexMap,IndexSet}`, `std::hash::RandomState`, `bevy_platform::hash::RandomState`; regenerate per-crate clippy.toml; add to `.claude/rules/sim.md`: never key or order sim iteration by `Entity`, use `SimId`. If a new clippy hit lands inside fl_sim, report it instead of fixing (lane 1 owns fl_sim).
2. I8 — arch-check: Sim and Feature roles get an allowlist = ECS_FAMILY + the bevy_app family (enumerate from `cargo tree`); deny `rand*`, `bevy_rand`, `rapier*`, `avian*` in every headless role. Fixture test like `headless_dev_dep_on_client_flagged`.
3. Minor 1 — "no tests matched" is Fail for the gpu, brp and milestone rows.
4. Minor 2 — first add the head SHA field (56e3425) to docs/perf/M0.json; then protect `docs/perf/M*.json` in OWNERS `[protected]`; the `perf_file` row parses M<n>.json and fails if the 3000-unit mean > 15 ms. Test.
5. Minor 3 — `check_budgets` fails when a budgeted name is absent from `per_tick_max`. Test.
6. Minor 5 — rename the system-order test to `smoke_sim_tick_system_order`; keep the snapshot file name stable (explicit insta snapshot name) so nothing needs re-blessing.
7. Minor 6 — add `fl_content` to the API snapshot set (write `.new`, report, do not bless); add `crates/fl_sim/src/{hash,replay,spawn,commands}.rs` to the OWNERS contract track.
8. Minor 13 — fl_cli: `fl_test_utils` → `[dev-dependencies]`.
9. Minor 18 — docs/debugging.md: describe the hash check ("equals `fl_cli hash-dump` at tick 20") instead of the hard-coded `8b3418f4fcb9d9af`.
10. T30 — a test pinning that fl_game loads `debug.ron` before the sim starts and enables phase markers with the profiler.
11. T30 — the default debug.ron must not log every spawn at DEBUG (≈30 ms/tick in debug stress): e.g. a default filter `fl_sim::spawn=info`. Do not edit fl_sim; if the only fix is inside fl_sim, report it. Re-measure the Lifecycle phase in a debug `--stress 3000` run (profiler / F2 panel or `fl_cli`) and report.
12. T30 — fl_game reads `BRP_EXTRAS_PORT` as the default when `--remote-port` is absent (the flag wins). Test.
13. T30 — `observe_setup` must not leave an empty run dir on a content error. Test.

Protected artifacts: write `.new`, never bless; list them in the report.
Done when: `cargo xtask gate pr` PASS (summary in the report), clippy -D warnings, fmt. Commits per item group, conventional messages, `Refs: ADR-NNNN` for contract-path changes, ending `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
