# Explorer world: profiler budget warnings (2026-10-25)

The dev saw `npc_perf` WARN lines every ~5 s in the release explorer world (`actor_testbed --explorer-home ..
--agent .. --resume`): perception p95 ~260 µs (budget 25), vehicles ~470 µs (12), crafting and interaction ~55 µs
(17), agent_io ~57 µs (17), plus a start-up `redundant edge(s)` schedule warning. This note records what was
measured, the root causes, the fixes and a budget proposal. `bench/baseline.json` is not touched.

## How it was measured

- Release builds of `npcctl` and `actor_testbed`, **built together** (`cargo build --release -p actor_testbed -p
  npcctl`): building `npcctl` alone leaves out `bevy_ecs/multi_threaded`, which the game always has, and gives
  different numbers (see root cause 3). "before" is `ea12bf1` built into the same target dir from a separate
  worktree.
- Headless: `npcctl bench --scenario testbed:<name> --ticks 3000 --warmup 300`, before and after **interleaved**,
  5 rounds, median of each mechanic's p95 (µs). Scenarios: `explorer` (frontier layout, 128×128 grid, one scout,
  two idle vehicles), `testbed` (8 units), `smithy` (3 units).
- Windowed: `actor_testbed --remote` (`--scenario explorer`, and explorer mode `--explorer-home DIR --agent Scout
  --world bench` resumed over BRP), `npc/perf/reset` after 10 s, `npc/perf` 30 s later.
- The machine ran other sessions' cargo builds for part of the day (up to 33 rustc/cargo processes); the final
  tables were taken with 0–6. Only interleaved before/after pairs are compared.

## Root causes

1. **`world.query()` in per-tick exclusive systems.** Each call builds a fresh `QueryState` (~1.1–1.5 µs of
   archetype matching), and with `bevy/trace` Bevy creates a `par_for_each` tracing span inside
   `QueryState::new`. Creating a span goes through every subscriber layer (~0.3 µs with the registry alone,
   ~1 µs with Bevy's `LogPlugin` stack of EnvFilter + ErrorLayer + fmt). The four vehicle systems built seven
   query states per tick with nobody mounted or driving; perception two; nav, command (three), think,
   interaction (four), crafting and the two `ExternalAction` passes one or more each. Profiled, these exclusive
   systems took 5–28 µs each while ordinary param systems took < 1 µs.
2. **Double counting.** The vehicles, perception, interaction, crafting and progression systems open a
   `mechanic` span named after their own mechanic inside the system span that is already tagged with it. The
   collector summed both, so those mechanics read ~2× their real time (vehicles p50 78.8 µs = 2 × the sum of its
   four systems).
3. **Multi-threaded executor for `NpcSimTick`.** With `bevy_ecs/multi_threaded` (every windowed game) the sim
   schedule used Bevy's default multi-threaded executor: ~45 small systems, most of them exclusive, handed
   between threads. It cost ~0.45 ms per tick more than the work itself, and every system body measured ~3×
   slower (thread hops, cold caches). Explorer: 0.60 ms/tick (multi) vs 0.13 ms/tick (single), same code.
4. **The testbed timed none of its sim systems.** `testbed_content::build_app` adds the sim plugins before the
   host adds `DefaultPlugins`, so every `NpcSimTick` system's span (created when the system is added) was created
   before `LogPlugin` set the subscriber and stayed disabled. F4 / `npc/perf` showed only the hand-written
   `mechanic` spans, and nav, think, command, spatial, .. were invisible in the window.
5. **Log spam.** Every breach was a `warn!`, rate-limited to one per mechanic per 5 s, with no way to quiet it.
6. **Redundant edge.** `flush_per_roll_log` was put in `NpcSet::Sense` and in `NpcPerceptionSet`, which is
   already configured inside `Sense`.

Not a cause: work scaling with the 128×128 grid. The spatial queries are bounded by the radius, and perception's
cost is the same in `explorer` and `smithy`.

## Fixes

- Cached `&mut QueryState` / `Local<..>` params in the 14 exclusive systems (no behaviour change: same queries,
  results still sorted by `SimId`, change detection still relative to the system's last run).
- Perception gathers and sorts the perceivables only on a tick where some agent scans or a noise was heard.
- `npc_perf::PerfLayer` skips timing a `mechanic` span whose nearest timed ancestor has the same mechanic.
- `NpcSimTick` uses `SingleThreadedExecutor` (kernel). The order is fixed by the ambiguity check, so no hash
  changes.
- `perf::install_log_plugin_now()`: `actor_testbed` installs the profiler's subscriber before building the sim
  app and disables `DefaultPlugins`' `LogPlugin`. Bevy's `Main::run_main` (now visible, inclusive of the whole
  frame) is tagged `frame` instead of `engine`.
- `BreachLog { Warn, WarnOnce, Debug }`: `NpcPerfPlugin` defaults to `WarnOnce` (one `warn!` per mechanic, then
  `debug!`, which `perf_log_filter`'s `npc_perf=info` keeps off the console); `actor_testbed --perf-warn` gives the
  old behaviour; `npcctl bench` and the F4 panel see every breach as before.
- `flush_per_roll_log` is only `in_set(NpcPerceptionSet)`.

## Results

Headless, release, median p95 µs over 5 interleaved rounds (p50 in brackets):

| mechanic | explorer before | explorer after | testbed before | testbed after | smithy before | smithy after | budget (explorer / testbed / smithy) |
|---|---|---|---|---|---|---|---|
| perception | 75.1 [37.4] | 10.1 [2.3] | 136.3 [75.2] | 24.6 [12.9] | 95.6 [44.1] | 13.3 [4.3] | 25 / 60 / 35 |
| vehicles | 104.1 [50.7] | 4.8 [4.3] | 116.7 [59.4] | 6.4 [4.4] | 113.9 [57.2] | 6.5 [4.4] | 12 / 26 / 16 |
| interaction | 40.3 [21.5] | 2.4 [1.9] | 49.9 [28.7] | 4.4 [2.2] | 42.8 [23.7] | 4.1 [2.0] | 17 / 31 / 21 |
| crafting | 13.8 [7.3] | 1.2 [1.1] | 16.2 [8.8] | 1.6 [1.1] | 14.3 [7.8] | 1.6 [1.1] | 17 / 31 / 21 |
| agent_io | 22.8 [10.5] | 1.0 [0.7] | 21.3 [9.6] | 1.0 [0.4] | 20.6 [10.1] | 1.0 [0.4] | 17 / 31 / 21 |
| nav | 34.2 [14.7] | 3.3 [2.5] | 45.2 [20.1] | 12.9 [4.1] | 37.5 [17.0] | 6.6 [3.0] | 22 / 36 / 26 |
| command | 18.5 [9.7] | 0.5 [0.3] | 24.9 [13.4] | 1.3 [0.5] | 19.9 [10.8] | 1.0 [0.3] | 17 / 31 / 21 |
| think | 12.0 [5.9] | 1.3 [0.8] | 48.6 [10.4] | 30.0 [1.9] | 40.8 [7.4] | 26.9 [1.0] | 60 / 200 / 100 |
| ms per tick (profiler on) | 587 | 124 | 647 | 158 | 644 | 141 | |

Headless budget breaches per run: before 6 (explorer), after 0 in all three scenarios.

Village (`npcctl bench --villagers 1,100,500`, 3 interleaved rounds, ms per tick): 1 villager 0.342 → 0.071,
100 → 0.786 → 0.134, 500 → 0.923 → 0.560; every think/needs/journal p95 is lower or equal. (Against
`bench/baseline.json` both the before and the after binary report the same `think.*` "regressions", so they
predate this change; the baseline was apparently taken with an `npcctl`-only build.)

Windowed explorer mode (resumed over BRP, 30 s, two runs each; p95 µs):

| mechanic | before | after | budget |
|---|---|---|---|
| `driver` (the whole sim tick, inclusive) | 659 / 1088 | 244 / 240 | – |
| vehicles | 60 / 94 | 21 / 22 | 12 |
| perception | 54 / 87 | 36 / 37 | 25 |
| nav | not timed | 21 / 22 | 22 |
| interaction | 8 / 12 (span only) | 15 / 17 | 17 |
| crafting | 8 / 13 | 5 / 6 | 17 |
| agent_io | 1 / 3 | 9 / 8 | 17 |
| WARN lines in 40 s | 12 + redundant edge | 4 (2 per `npc/perf/reset`) | |

"before" windowed per-mechanic values are understated: only the `mechanic` spans were timed (root cause 4), so
nav, think, spatial and most of interaction did not count at all. `driver` is the fair comparison.

## Budget proposal (applied)

After the fixes every mechanic is inside its budget headless, in all three scenarios, with ample room. In the
windowed explorer, vehicles and perception stay over, and nav and interaction sit at their budgets. That is not
work that the fixes left behind:

- Windowed, **every** sim system costs more than headless, also when it has nothing to do: +0.5–1 µs for the
  smallest (facing 0.1 → 0.8, needs 0.2 → 1.2 µs p50), +2–5 µs for systems that read more of the world (vehicle
  gate 1.2 → 5.5, spatial update 0.6 → 5.1, complete interactions 1.0 → 3.9) and +13 µs for nav plan (2.5 → 15.9).
  A tick runs every 50 ms between GPU-bound frames (render p50 ~35 ms on this machine), so each one starts with
  cold caches; and each `mechanic` span costs ~1 µs to create in Bevy's log stack (EnvFilter + ErrorLayer + fmt)
  against ~0.3 µs headless.
- vehicles is four exclusive systems that run every tick whether or not a vehicle moves; perception's p95 is
  its scan tick (`stagger` 4) with line-of-sight traces.

So the windowed budgets need a per-system floor that the headless table does not. Proposed `NPC_BUDGET_TABLE`
rows (fixed µs + µs per agent; about 1.5× the windowed explorer p95, still ≥ 4× the headless p95):

| mechanic | now | proposed | windowed explorer p95 | headless explorer p95 |
|---|---|---|---|---|
| vehicles | 10 + 2 | 30 + 2 | 21–22 | 4.8 |
| perception | 20 + 5 | 50 + 5 | 36–37 | 10.1 |
| nav | 20 + 2 | 30 + 2 | 21–22 | 3.3 |
| interaction | 15 + 2 | 25 + 2 | 15–17 | 2.4 |

Applied to `NPC_BUDGET_TABLE` and the `debugging.md` §7.2 table as proposed (the user chose the rows over the
alternative). Alternative considered: keep the table and give windowed hosts a `windowed` factor like
`DEBUG_BUDGET_SCALE` (×2 for these four, or for all rows) in `NpcPerfPlugin`. Either way the bench baseline is
unaffected. With `BreachLog::WarnOnce` the remaining breaches cost at most one log line per mechanic per run.
