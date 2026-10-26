# Agent-Driven Game Development: Environment Improvements for Frontline (researched 2026-10-23)

**Scope.** This report covers concrete tools, configurations and practices that make Claude Code (desktop app, Windows 11, parallel subagents, superpowers workflow) more effective at building Frontline on Bevy 0.20. Frontline is a deterministic fixed-point sim at 20 Hz in its own World, with RON content, `fl_cli`, BRP `fl/*` methods, JSONL journals and Tracy.

**How things were checked.**
- **Versions** come from the crates.io API on 2026-10-23.
- **Bevy 0.20 facts** come from the crate sources already in `~/.cargo/registry/src/index.crates.io-*/bevy*-0.20.0` and from the `v0.20.0` tag on GitHub.
- **Claude Code facts** come from code.claude.com.
- **Status labels.** Items marked **VERIFIED** were read in source or on the official page. **UNVERIFIED** means a third-party claim or something not exercised.
- **Toolchain.** The machine now has `rustc 1.99.1` (2026-10-13). That already satisfies bevy 0.20's MSRV of 1.97.1, so §3 of the design doc ("machine has 1.96.1") is out of date.
- **Nothing was installed** and no project was modified.

---

## Top 10 recommendations (ranked by value/effort)

| # | Recommendation | Effort | Bevy 0.20 status |
|---|---|---|---|
| 1 | Bevy 0.20 "ground truth kit": local source, examples and migration guides, a `bevy-api` skill and a compile-probe crate | S | Native, VERIFIED |
| 2 | Determinism gates inside Bevy: ambiguity detection as error, schedule shuffling, and clippy bans in sim crates | S | VERIFIED in bevy_ecs 0.20 source |
| 3 | Windows build loop: per-worktree short `build-dir`, rust-lld, a debuginfo diet, sccache, Dev Drive and cargo-hakari | S–M | n/a (toolchain) |
| 4 | Claude Code hooks: a Stop/SubagentStop quality gate plus anti-test-cheating guards | M | n/a |
| 5 | `bevy_brp_mcp` + `bevy_brp_extras` 0.23.0, with the `fl/*` methods published as agent tools | S–M | 0.23.0 = Bevy 0.20, VERIFIED |
| 6 | Compact machine-readable feedback: a nextest "agent" profile, short diagnostics and the rust-analyzer LSP plugin | S | n/a |
| 7 | 4–5 role subagents with preloaded skills, path-scoped rules and a CLAUDE.md under 200 lines | M | n/a |
| 8 | Generative sim testing: proptest-state-machine command sequences, golden replays with per-feature hashes, cargo-mutants | M | proptest is engine-independent |
| 9 | Agent-proof perf gating: deterministic work counters, paired A/B wall-clock runs, Tracy CSV export | M | Tracy works via `trace_tracy` |
| 10 | A visual verification ladder: semantic screen asserts, then tolerant goldens via `bevy_ci_testing`/`Screenshot::image`, then an advisory vision rubric | M–L | VERIFIED APIs in 0.20 |

### 1. Bevy 0.20 ground-truth kit (S)

**What it is.** A project skill (`.claude/skills/bevy-api/SKILL.md`) that tells every agent never to write Bevy API from memory. Instead it resolves APIs from the pinned local sources, in this order:

1. **API source:** `~/.cargo/registry/src/index.crates.io-*/bevy_<crate>-0.20.0/src`. All bevy 0.20 sub-crates are already downloaded on this machine.
2. **Examples:** `bevy-0.20.0/examples/**`. The published crate ships its examples, including:
   - `examples/remote/{app_under_test,integration_test}.rs`, a BRP-driven integration test that takes a screenshot and clicks
   - `examples/dev_tools/schedule_data.rs`
   - `examples/app/headless_renderer.rs`
3. **Testing patterns:** `bevy-0.20.0/tests/how_to_test_apps.rs` and `how_to_test_systems.rs`.
4. **Changes since training data:** `bevy-0.20.0/_release-content/migration-guides/*.md` (74 files) and `release-notes/*.md`.
5. **Our verified snippets:** `docs/research/bevy020-api-reference.md` (VERIFIED/UNVERIFIED tags already exist).
6. **Compile probe:** if the API is still uncertain, write a 10-line snippet in a `probes/` workspace member and run `cargo check -p probes`. It is excluded from `default-members` so it never slows normal builds. Record the result in the reference file.

Use `` !`command` `` dynamic injection in the skill to print the registry path, so the agent never has to guess it.

**Why it helps here.**
- Bevy 0.20 shipped after the model's training data. The migration guides list exactly what changed: WESL shaders, `Button`/`Interaction` deprecation, `MeshAabb` rename, the `bevy_shape` split, BSN syntax and more.
- Practitioner reports agree that Bevy's API churn is the main source of agent errors, and that reading installed source beats memory. Godogen's Bevy guide tells agents to "read the installed source instead of relying on memory" ([godogen](https://github.com/htdt/godogen)). Ardan Labs' Bevy Pong code did not compile on first try ([daily.dev summary](https://daily.dev/posts/can-ai-really-build-games-in-rust-agentic-ai-vs-bevy-engine--txjound5e)).
- Local source is pinned to `Cargo.lock`, so it never drifts the way a docs.rs-latest MCP can.

**Optional add-on.** [`kinkirill/bevy_manual_mcp`](https://github.com/kinkirill/bevy_manual_mcp) ("bevy-mcp") offers hybrid search over rustdoc, the Book, migration guides and examples.
- It published a **Bevy 0.20.0 index on Oct 9**.
- It is small: 13 stars.
- It runs via `npx github:...`, which executes code fetched from GitHub. Review it and pin a commit before adopting.

**Tools:** none new. **Bevy 0.20:** native.

### 2. Determinism gates built into Bevy and clippy (S)

**What it is.** Three gates, each cheap to add.

**Ambiguity detection as an error.** In the SimTick schedule, set `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, .. }`. Any two sim systems with conflicting access and no explicit order then fail at schedule build time. This field is VERIFIED in `bevy_ecs-0.20.0/src/schedule/schedule.rs`. Its default is `Ignore`.

**Schedule randomization (new in 0.20).** `ScheduleBuildSettings::shuffle_seed: Option<u64>` sits behind bevy_ecs's `debug` feature (VERIFIED; release note `schedule_randomization.md`).
- Add a test-only feature in `fl_sim` that enables `bevy_ecs/debug`.
- Add a test that runs each golden scenario with N random shuffle seeds and asserts identical per-tick state hashes.
- Use the single-threaded executor for these tests, as the release note advises.
- Caveat from the note: `auto_insert_apply_deferred` limits how far systems get shuffled.

**Clippy bans in sim crates.** Put a `clippy.toml` in each sim-only crate (`fl_math`, `fl_protocol`, `fl_sim`). Clippy looks for `clippy.toml` in the crate directory and its parents. Contents:
- `disallowed-types` (fields `path`, `reason`, `replacement`): `std::collections::HashMap`/`HashSet` (use ordered or indexed maps), `std::time::Instant`/`SystemTime`, `bevy_time::Time`, `rand::rngs::ThreadRng`
- `disallowed-methods`: `rand::random`, `f32::sin` and similar

For feature crates that mix sim and client code, put `#![deny(clippy::float_arithmetic)]` on the `sim` module. Bevy itself uses this exact pattern: its own `clippy.toml` bans `f32::sin`, `powf` and others "for libm determinism" (VERIFIED in `bevy-0.20.0/clippy.toml`).

**Why it helps here.** Determinism bugs are invisible in code review and only appear as desyncs much later. These gates turn "agent added an unordered system" or "agent used `HashMap` iteration or `f32` in the sim" into a compile or test failure that the agent sees immediately.

Two more Bevy 0.20 notes:
- `SerializeSchedulesPlugin` (`bevy_dev_tools::schedule_data`, VERIFIED) dumps the schedule graph to RON, including `SystemConflict` entries. Snapshot the SimTick graph with `insta` 1.49.0 so any change in system order or conflicts shows up in review diffs. This is an architecture-drift detector.
- The "Catching Panics" handler re-panics by default. Never install a "log and continue" fallback error handler in tests or `fl_cli`, or failures get swallowed.

### 3. Windows build loop for parallel agents (S–M)

**What it is.** Six changes:

1. **Per-worktree build-dir on a short path.** `build.build-dir` is stable since Rust 1.91 and supports `{workspace-path-hash}` ([Cargo config](https://doc.rust-lang.org/cargo/reference/config.html)). Set it in the *user* config (`~/.cargo/config.toml`) or as the env var `CARGO_BUILD_BUILD_DIR`, so the repo stays cross-platform: `build-dir = "C:/t/fl/{workspace-path-hash}"`. This gives each Claude worktree under `.claude/worktrees/<name>` its own intermediate directory on a short path, which fixes:
   - LNK1104/MAX_PATH, from deep build-script paths inside the worktree
   - lock contention, because a *single* shared target dir makes parallel subagents block on "waiting for file lock". Cargo's fix for that (`fine-grain-locking`) is still unstable.

   Clean up with a `WorktreeRemove` hook. Each Bevy build dir is about 10 GB; the probe used 9.6 GB.
2. **Linker.** `[target.x86_64-pc-windows-msvc] linker = "rust-lld.exe"` (Bevy setup guide). `rust-lld.exe` already ships in the toolchain at `lib/rustlib/x86_64-pc-windows-msvc/bin`, VERIFIED locally. rust-lld is the *default* only on x86_64 Linux, since 1.90 ([Rust blog](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable)). MSVC still defaults to `link.exe`, so set it explicitly and confirm with a probe link.
3. **Profiles.**
   - `[profile.dev] opt-level = 1` and `[profile.dev.package."*"] opt-level = 3`. These are **required** with `bevy/dynamic_linking` on Windows, otherwise you hit "too many exported symbols".
   - Add `debug = "line-tables-only"` for workspace crates and `debug = false` for `"*"`. PDB generation and linking dominate incremental time on MSVC.
   - Copy `[profile.dev.package.wgpu-types] debug-assertions = false` from [bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d).
4. **Shared dependency cache.** sccache **0.18.0** (2026-09-16) with `SCCACHE_BASEDIRS`. 0.16 fixed Windows path stripping; 0.18 strips basedirs from compiler args ([releases](https://github.com/mozilla/sccache/releases)). With that, the second and later worktrees reuse compiled dependencies. sccache does not cache incremental (workspace) crates, dylibs or linking. That is fine: dependencies are the 5-minute part.
5. **Dev Drive (ReFS) or Defender exclusions** for `CARGO_HOME`, the repo and the build dir. The nextest Windows docs recommend this ([nexte.st](https://nexte.st/docs/installation/windows/)). Microsoft claims up to 30% on non-Rust builds ([Windows blog](https://blogs.windows.com/blog/2023/06/01/dev-drive-performance-security-and-control-for-developers/)). This is a system/security setting, so **the user should apply it**, not an agent.
6. **cargo-hakari 0.9.39** (2026-09-19), a workspace-hack crate. Sim crates use `bevy_ecs` only while the client uses full `bevy`, so `cargo test -p fl_sim` and a workspace build resolve different feature sets and rebuild the whole `bevy_ecs` subtree back and forth. Cargo's built-in `resolver.feature-unification = "workspace"` is **still unstable** (`-Z feature-unification`, [unstable docs](https://doc.rust-lang.org/cargo/reference/unstable.html)), so hakari is still the stable answer.

Also cap concurrency. N subagents × `num_cpus` rustc jobs oversubscribes a 16-core laptop. Allow at most 2 concurrent "builder" agents, keep reviewers read-only or test-only, or set `CARGO_BUILD_JOBS`.

**Unverified risk to probe in M0.** Bevy's `dynamic_linking` plus crates that depend directly on `bevy_ecs`/`bevy_app` (the sim crates) may produce duplicate-crate linkage errors. I found no source either way. Compile-probe it before relying on dynamic linking, or enable it only in `fl_game` dev runs.

### 4. Hooks: a deterministic quality gate plus anti-cheating guards (M)

**What it is.** Hooks in `.claude/settings.json` (and in subagent frontmatter for role-specific ones). Semantics per [hooks docs](https://code.claude.com/docs/en/hooks):
- Exit code 2 blocks.
- `Stop` and `SubagentStop` can block and inject context.
- `stop_hook_active` prevents infinite loops. There is a cap of 8 consecutive continuations (`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`).
- `asyncRewake: true` runs a hook in the background and wakes Claude on exit 2.
- `if: "Bash(cargo *)"` filters by command.
- `"shell": "powershell"` or exec-form `args` avoids Git Bash vs PowerShell quoting differences on Windows.

**Hooks to add:**
- **`SubagentStop`, matched on implementer agent types, plus `Stop`.** If any `.rs` file changed (from `git diff --name-only`), run:
  - `cargo fmt --check`
  - `cargo clippy -p <touched crates> --all-targets --message-format=short -- -D warnings`
  - `cargo nextest run -p <touched crates> --profile agent`

  Block with a compact failure summary. This replaces "remember to run checks" in prose. Anthropic's best-practices guide names a Stop hook as the "deterministic gate" option ([best practices](https://code.claude.com/docs/en/best-practices)).
- **`PreToolUse` on `Edit|Write` (anti test-cheating).** Deny edits to golden and baseline artifacts for implementer agents:
  - `tests/golden/**`, `**/*.snap`, `replays/**/*.hashes`
  - `docs/perf/baselines/**`, `assets/**/scenarios/expected/**`

  Also deny `Bash(cargo insta accept*)` and `Bash(* --bless*)`. Only a designated "baseline-keeper" step, triggered by the user or plan, may re-bless.
- **`PostToolUse` on `Edit|Write`.** Grep the diff for `#[ignore]`, `todo!()`, `unimplemented!()`, new `#[allow(clippy::`/`#[allow(dead_code` without a `// reason:` comment, `assert!(true)`, and tests whose bodies lost their assertions. Inject `additionalContext` telling the agent to justify or revert, and have the Stop gate fail on unjustified ones.
- **`PreToolUse` on Bash.** Ask (or deny) for `cargo update`, `cargo add`, `rustup default`, `git push --force`.

  Run a `cargo tree -p fl_sim -e normal`-based dependency-rule check (design §5.1: no `bevy_render`, `bevy_window`, `bevy_input` or `Time` in sim crates) whenever `Cargo.toml` changes, plus `cargo-deny` 0.20.2 for licenses and advisories.

**Why it helps here.** Benchmarks find both Claude Code and Codex reward-hacking tests by editing or skipping them ([EvilGenie, arXiv 2511.21654](https://arxiv.org/abs/2511.21654)). The standard mitigation is checks the agent cannot modify. CLAUDE.md is advisory; hooks are enforced, and the Claude Code memory docs make the same point.

**Avoid** a `PostToolUse` `cargo check` after *every* edit, the common Rust template ([example](https://thepromptshelf.dev/blog/claude-code-rust-projects-guide-2026/)). With Bevy and parallel subagents it means minutes of lock contention. The LSP plugin (rec. 6) gives per-edit diagnostics instead.

### 5. BRP MCP: `bevy_brp_mcp` 0.23.0 + `bevy_brp_extras` 0.23.0 (S–M)

**What it is.** The most mature Bevy MCP ([natepiano/bevy_brp](https://github.com/natepiano/bevy_brp)).
- **Versions:** 0.23.0 was published on 2026-10-09, one day after Bevy 0.20. The compatibility table pairs **Bevy 0.20 with 0.23.0** (VERIFIED, README and crates.io deps `bevy ^0.20.0`). It is MIT/Apache and has a regular release cadence: 0.22.x weekly through 2026.
- **Tools:**
  - launching apps and examples (`brp_launch`), with logs (`read_log`)
  - `world_query`, `world_mutate_components`, watches
  - `brp_type_guide`
  - screenshots of the window, a camera viewport or a named-entity crop
  - `send_keys`/`type_text`, mouse click/drag/scroll, gamepad
  - diagnostics (FPS and frame time)
- **Agent tools.** `AppAgentToolExt::register_agent_tool` publishes *your own* BRP methods, with descriptions and optional JSON schemas, through `brp_extras/agent_tools`. The agent discovers them with `brp_list_agent_tools` and calls them with `brp_execute` ([docs.rs](https://docs.rs/bevy_brp_extras/0.23.0/bevy_brp_extras/)). This is exactly the hook for `fl/step`, `fl/sim_state`, `fl/issue_command` and the rest. Bevy 0.20's `rpc.discover` lists names only, without schemas, so this metadata matters. Caveat: published methods must be *instant* (not `+watch`) methods.

**Setup:**
- Install with `cargo install bevy_brp_mcp`. The user does this.
- Register it per project in `.mcp.json`, not `~/.claude.json`, so subagents can get it via `mcpServers` frontmatter.
- In the app: `bevy_remote` + `png` features, then `RemotePlugin` + `BrpExtrasPlugin` in a dev-only `fl_client::debug` plugin.

**Frontline-specific caveats:**
- **Built-in `world.*` methods only see the App's main World.** The sim runs in its own World, so agents must reach sim state through `fl/*` methods (already planned). Don't expect `world_query` to show sim entities. For rendering-side inspection, derive `Reflect` on view components.
- **Port collisions.** Parallel agents each running the game need distinct ports. Use `BRP_EXTRAS_PORT` per worktree; the default is 15702.
- **Black screenshots.** Captures of an occluded or minimized window can come back black (README; also noted in Bevy's own `examples/remote/integration_test.rs`). The Claude desktop app will usually sit on top of the game window. Make `fl/screenshot` render from a dedicated camera into an `Image` target and capture it with `Screenshot::image(handle)` (VERIFIED in `bevy_render-0.20.0/src/view/window/screenshot.rs`). That makes it independent of window state.
- **Perf readings while unfocused.** `WinitSettings` defaults to `reactive_low_power(1/60 s)` when unfocused (VERIFIED in `bevy_winit-0.20.0`). In an agent/bench launch mode, set `unfocused_mode: UpdateMode::Continuous`, otherwise `fl/perf` frame times read wrong.
- **Input.** Agents should drive the game through `fl/issue_command` (PlayerCommand), *not* mouse clicks. Use `brp_extras` mouse and keyboard only to test the input→command mapping layer.
- **Readiness gating.** Adopt the "state-gated capture" pattern from the [`bevy-eyes-on`](https://github.com/OilProducts/agent-skills) skill: poll a probe until a condition holds (for example `fl/sim_state.tick >= 600`), then screenshot. No fixed sleeps.

**Alternatives (not recommended):**
- [`Nub/bevy_mcp`](https://github.com/Nub/bevy_mcp) targets Bevy 0.18.
- [`ladvien/bevy_debugger_mcp`](https://github.com/ladvien/bevy_debugger_mcp) was last released 2025-08 (0.1.8).
- [`slc90/bevy_brp`](https://github.com/slc90/bevy_brp) is a git-only fork, 0 stars, although it targets 0.20 and has a "wake-aware" transport.

### 6. Compact, machine-readable feedback: nextest profile, short diagnostics, LSP (S)

**cargo-nextest 0.9.149** (2026-10-20). Add `.config/nextest.toml`:
```toml
[profile.agent]
status-level = "fail"
final-status-level = "fail"
failure-output = "final"
success-output = "never"
slow-timeout = { period = "30s", terminate-after = 4 }
fail-fast = { max-fail = 5 }
[profile.agent.junit]
path = "junit.xml"            # -> target/nextest/agent/junit.xml
[test-groups.gpu]
max-threads = 1               # serialize render/screenshot tests
[[profile.default.overrides]]
filter = "test(/^render_/)"
test-group = "gpu"
```
- JUnit is the stable machine format ([JUnit](https://nexte.st/docs/machine-readable/junit/)). libtest-json is experimental (`NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 --message-format libtest-json`). Experimental record/rerun (`NEXTEST_EXPERIMENTAL_RECORD`) lets agents rerun just the failures ([record/replay](https://nexte.st/docs/features/record-replay-rerun/)).
- Windows caveat: nextest spawns one process per test, and process creation is slow on Windows. Benchmark it against `cargo test` for the many tiny `fl_math` tests ([nexte.st Windows](https://nexte.st/docs/installation/windows/)).

**Compiler output.**
- `cargo clippy --message-format=short` is the cheapest human-readable format.
- For structured use, add `fl_cli dev diag`, or an `xtask`, that runs `--message-format=json`, dedupes, and prints one `level file:line code message` per diagnostic, capped at N.
- The same principle applies to journals: give `fl_cli journal` query filters (tick range, entity, kind, limit) so agents never `cat` a multi-MB JSONL file.

**rust-analyzer LSP plugin** (official, Anthropic-maintained).
- Install with `/plugin install rust-analyzer-lsp@claude-plugins-official`. The `rust-analyzer` component is already installed via rustup.
- Claude then gets diagnostics after each edit and a read-only `LSP` tool for go-to-definition and find-references ([code intelligence docs](https://code.claude.com/docs/en/plugins/code-intelligence)).
- Put `rust-analyzer.toml` at the repo root with `cargo.targetDir = true`, so rust-analyzer's own `cargo check` doesn't take the main build lock ([rust-analyzer config](https://rust-analyzer.github.io/book/configuration.html); `rust-analyzer.toml` support is partial).
- Expect high memory use while it indexes Bevy.

### 7. Role subagents, path-scoped rules and a short CLAUDE.md (M)

**What it is.** Keep `CLAUDE.md` under 200 lines, the docs' target ([memory docs](https://code.claude.com/docs/en/memory)). It should hold:
- build, test and CLI commands
- the toolchain pin
- the five determinism rules
- the dependency rules
- "use the `bevy-api` skill"
- where specs and plans live

Move the rest into **path-scoped rules** in `.claude/rules/`, using `paths:` frontmatter:
- `sim.md` → `crates/fl_sim/**`, `crates/fl_math/**`, `features/*/src/sim/**`: fixed-point only, no `Time`, ordered iteration, every system in a `SimSet`
- `client.md` → `crates/fl_client/**`, `features/*/src/client/**`: input produces commands only, never mutate sim state
- `content.md` → `assets/**/*.ron`: schema notes, run `fl_cli data-check`
- `shaders.md` → `**/*.wesl`

Then add **4–5 custom subagents** in `.claude/agents/*.md`. Supported frontmatter includes `tools`, `disallowedTools`, `model`, `skills` (preloaded), `mcpServers`, `hooks`, `memory: project`, `isolation: worktree`, `effort`, `maxTurns` ([sub-agents docs](https://code.claude.com/docs/en/sub-agents)).

| Agent | Key config |
|---|---|
| `sim-engineer` | skills: `bevy-api`, `determinism`, `superpowers:test-driven-development`; hooks: block client-crate edits; memory: project |
| `client-engineer` | mcpServers: `brp`; skills: `bevy-api`, `visual-check` |
| `test-author` | writes failing tests and scenarios from the spec *before* implementation; it is the only role allowed to create new golden files |
| `reviewer` | tools `Read, Grep, Glob, Bash` (no Edit); model opus; checklist: spec compliance, determinism rules, test-integrity diff (assertions removed, ignores added), dependency rules, schedule-graph snapshot diff |
| `perf-analyst` | runs `fl_cli bench --profile` and Tracy capture/CSV; compares against baselines; never edits code |

These map directly onto superpowers' subagent-driven-development roles (implementer, spec reviewer, code reviewer). Name the agent type in the plan's task blocks so the right tools, hooks and skills load.

**Watch out:** `isolation: worktree` branches from the *default branch*, not the parent's HEAD (docs). Feature-branch work handed to a worktree subagent can start from stale code. Have the plan state the base commit, or let the controller create the worktree.

**Public examples:**
- [Claude Code Game Studios](https://github.com/donchitos/claude-code-game-studios) (27k stars) shows agents, hooks and path-scoped rules for Godot/Unity/Unreal. Copy individual ideas from it, such as the hook that blocks reading oversized design docs and returns headings instead. Don't copy its 49-agent hierarchy: by its author's own account, it produced 30 documents before any code.
- [godogen](https://github.com/htdt/godogen) is a Bevy-capable game generator that judges results from the running game, not from compilation.

### 8. Generative sim testing: command-sequence properties, golden replays, mutation testing (M)

**What it is:**
- **proptest 1.12.0** (2026-10-10) and **proptest-state-machine 0.9.0** (2026-10-10). Generate random but valid `PlayerCommand` sequences against small `ScenarioBuilder` maps. Check invariants every tick:
  - `hp <= max_hp`
  - no unit inside impassable cells
  - ammo and supply conserved except through logged events
  - entity count changes only via spawn and death journal events
  - per-tick hash equal across two runs and across shuffled schedules (rec. 2)

  Shrinking yields a minimal command list, which `fl_cli` can save straight away as a regression scenario RON. That is scenario fuzzing without a fuzzer. cargo-fuzz/libFuzzer support on Windows MSVC is weaker; `bolero` 0.13.7 is an option later.
- **Golden replays.** Store the command stream plus per-tick hashes, and split the hash per feature or subsystem (movement, combat, vision, supply). A desync or regression then names the feature that diverged, not just the tick. Prior art: [tickwise](https://docs.rs/crate/tickwise/0.2.0) (light per-tick hash, rare full hash, field-level diff).
- **cargo-mutants 27.1.0.** Run it weekly or per milestone on `fl_math` and the combat formulas to find tests that pass no matter what the code does. That is the residue of weak or cheated tests. It is slow, so run it outside the per-task loop.

**Why it helps here.** Agent-written example tests tend to mirror the implementation. Properties, mutation testing and golden replays the implementer can't edit (rec. 4) catch that.

### 9. Agent-proof performance gating (M)

**What it is.**
- **Gate on deterministic work counters** (exact or small tolerance) in ordinary tests: pathfinding node expansions, LOS ray steps, spatial-query candidates, target-acquisition evaluations per tick at the 3,000-unit benchmark scenario. These are deterministic, so they never flake, and they catch algorithmic regressions an agent introduces.
- **Gate wall-clock with paired A/B, not against a stored absolute number.** Build the baseline (`main`) and candidate binaries, then run `fl_cli bench` interleaved, ABAB…, taking median-of-N. Fail on more than X% only when the difference is significant. [tango-bench 0.9.0](https://github.com/bazhenov/tango) (2026-10-16, Windows supported) implements paired benchmarking for micro-benches; criterion 0.8.2 is fine for local exploration.
- The dev machine looks like a mobile workstation (RTX 1000 Ada, Core Ultra 9), so thermal and power state vary run to run. Absolute-threshold gates will flake.
- **Agent-readable profiles.** Bevy's `trace_tracy` feature with `tracy-capture -o run.tracy` (Bevy's `docs/profiling.md`, VERIFIED locally), then `tracy-csvexport -e run.tracy`. The CSV has name, total, count, mean, min, max and std per zone ([man page](https://dyn.manpages.debian.org/unstable/tracy-csvexport/tracy-csvexport.1)). The perf-analyst agent diffs CSVs instead of reading a GUI.
- The Tracy GUI version must match `tracy-client`: run `cargo tree --features bevy/trace_tracy | grep tracy` and check the support table, as Bevy's profiling doc says.
- Run benches with `WinitSettings` continuous (rec. 5) or headless.

### 10. A visual verification ladder (M–L)

**What it is.** Rungs ordered by reliability, most reliable first.

1. **Semantic screen asserts (most reliable).** Through `fl/*`, query what *should* be visible (unit world positions, fog state, selection), project with the camera, and assert screen-space facts: on screen, inside the viewport, not occluded by UI. Read the presentation World's components. No pixels involved.
2. **Tolerant golden images on fixed "art test" scenes:**
   - **Capture.** Bevy's `bevy_ci_testing` feature (`CI_TESTING_CONFIG=<file.ron>`; events `Screenshot`, `NamedScreenshot(String)`, `MoveCamera{..}`, `Custom(String)`, `AppExit`; `setup.fixed_frame_time`, all VERIFIED in `bevy_dev_tools-0.20.0/src/ci_testing`). Or a headless render via `Screenshot::image` / the `headless_renderer` example pattern.
   - **Stable scenes.** Disable TAA, particles, time-varying effects and random seeds.
   - **Compare.** [`image-compare` 0.5.0](https://crates.io/crates/image-compare) (MIT, SSIM/RMS) with thresholds.
   - **Baselines.** Keep them **per machine/adapter**.
   - **Artifacts.** Write a single "contact sheet" PNG (expected | actual | diff) for the agent to `Read`, which saves tokens.
   - **CI precedent.** Bevy's own CI runs its examples this way on `windows-latest` with `WGPU_BACKEND=dx12`, which falls back to WARP software rendering on GPU-less runners. On Linux it uses `xvfb-run` plus mesa/lavapipe, and sends screenshots to Pixel Eagle for comparison (VERIFIED in [`example-run.yml` @v0.20.0](https://github.com/bevyengine/bevy/blob/v0.20.0/.github/workflows/example-run.yml)).
3. **Vision-model review, advisory only.**
   - A `visual-check` skill gives the agent the screenshot plus a reference image and a **closed yes/no rubric**, for example: "units on roads have shadows", "fog darker outside sight", "HP bars above every selected unit", "no magenta/missing textures", "camera iso angle about 30°".
   - Findings become tasks, not pass/fail gates. In VideoGameQA-Bench the best VLM scored 53% on visual unit tests and 45% on visual regression ([arXiv 2505.15952](https://arxiv.org/html/2505.15952v1)), and run-to-run variance is significant ([arXiv 2508.04895](https://arxiv.org/html/2508.04895)).
   - "Feels right" (pacing, readability) remains a human checkpoint at milestone close. Practitioners report green tests shipping unplayable games.

---

## Detailed findings by area

### Area 1: MCP servers and tools for driving Bevy

| Tool | Version / date | Bevy 0.20? | Verdict |
|---|---|---|---|
| [`bevy_brp_mcp`](https://crates.io/crates/bevy_brp_mcp) + [`bevy_brp_extras`](https://crates.io/crates/bevy_brp_extras) | 0.23.0 / 2026-10-09 | **Yes** (table: 0.20 → 0.23.0; deps `bevy ^0.20.0`) | **Adopt** (rec. 5) |
| [`slc90/bevy_brp`](https://github.com/slc90/bevy_brp) (fork) | v0.4.0, git only | Yes, per README | Watch; adds a wake-aware transport |
| [`Nub/bevy_mcp`](https://github.com/Nub/bevy_mcp) | — | No (Bevy 0.18) | Skip |
| [`bevy_debugger_mcp`](https://crates.io/crates/bevy_debugger_mcp) | 0.1.8 / 2025-08-23 | No evidence | Skip (stale) |
| [`kinkirill/bevy_manual_mcp`](https://github.com/kinkirill/bevy_manual_mcp) (docs, not runtime) | 0.20.0 index, Oct 9 | Yes | Optional (rec. 1) |
| [`rltvty/bevy-mcp`](https://github.com/rltvty/bevy-mcp) | 1 commit | Unknown | Skip |
| [`bevy-eyes-on` / `bevy-brp-instrumentation` skills](https://github.com/OilProducts/agent-skills) | 2026 | Generic BRP | Borrow the state-gated capture pattern |

**Core Bevy 0.20 BRP surface** (VERIFIED from [`builtin_methods.rs` @v0.20.0](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/builtin_methods.rs)):
- entity and component methods: `world.get_components`, `world.query`, `world.spawn_entity`, `world.insert_components`, `world.remove_components`, `world.despawn_entity`, `world.reparent_entities`, `world.list_components`, `world.mutate_components`
- resource methods: `world.get_resources`, `world.insert_resources`, `world.remove_resources`, `world.mutate_resources`, `world.list_resources`
- `world.trigger_event`, `world.write_message`
- `registry.schema`
- the streaming `+watch` variants, including **`world.observe+watch`**

`world.observe+watch` and `world.write_message` let a test harness stream sim events or inject messages without custom code.

Bevy ships a reference BRP integration test (`examples/remote/integration_test.rs`). It spawns a `Screenshot` entity over BRP, observes `ScreenshotCaptured`, locates a UI button and clicks it. It's a good template for `fl_cli`'s own BRP client.

**Headless and CI rendering:**
- **No window on a dev GPU:** `WindowPlugin { primary_window: None, exit_condition: DontExit }` + `ScheduleRunnerPlugin` + `RenderTarget::Image` (`examples/app/headless_renderer.rs`). It needs a wgpu adapter but no display.
- **Software adapter:** `WGPU_FORCE_FALLBACK_ADAPTER` / `WgpuSettings.force_fallback_adapter`. On Windows that is WARP via DX12; on Linux, lavapipe. Bevy's CI uses `WGPU_BACKEND=dx12` on `windows-latest`.
- **Scripted screenshot runs:** the `bevy_ci_testing` feature. It is frame-based, so pair it with `setup.fixed_frame_time` for determinism.

**Input injection:** brp_extras covers keys, text, mouse and gamepad. For an RTS whose sim takes only commands, command injection via `fl/issue_command` is the right abstraction; keep mouse injection for client-layer tests.

### Area 2: Agent-friendly Rust/Bevy dev loops

- **Linker:** see rec. 3. Mold is not available on Windows. lld is only the default on x86_64 Linux.
- **Cranelift:** nightly-only (`codegen-backend` is still unstable in Cargo). Bevy's guide says Windows setup is "a bit more involved", and the Rust project goal says Windows quality lags Linux ([2025H2 goal](https://rust-lang.github.io/rust-project-goals/2025h2/production-ready-cranelift.html)). It conflicts with the pinned-stable toolchain. **Skip for now.**
- **dynamic_linking:** worthwhile for `fl_game` dev runs. It requires the opt-level profile on Windows, and `cargo test --doc` needs `rustc --print target-libdir` on `PATH`. Probe it against the sim crates' direct `bevy_ecs` dependency (rec. 3).
- **Hot patching:** Bevy 0.20 has a `hotpatching` feature (`bevy_app::hotpatch::HotPatchPlugin`, which connects to the Dioxus CLI; VERIFIED in source). Agents restart processes anyway, and it adds a `dx` dependency. **Low value for agents.**
- **sccache 0.18.0** (rec. 3). cargo-nextest 0.9.149. bacon 3.26.0: useful for the *human*, since it can export `.bacon-locations` (`[exports.locations] auto = true`) that an agent can read ([bacon config](https://dystroy.org/bacon/config/)). Its socket control is Unix-oriented.
- **Workspace-hack:** cargo-hakari 0.9.39 (rec. 3). Cargo `build-dir` has been stable since 1.91; `feature-unification` is unstable.
- **Machine-readable output:**
  - `cargo --message-format=json|short`
  - nextest JUnit (stable) and libtest-json (experimental)
  - `cargo nextest list --message-format json`
  - nextest record/rerun (experimental) ([machine-readable](https://nexte.st/docs/machine-readable/))
- **Clippy:**
  - Workspace `[workspace.lints.clippy]`: `too_many_arguments = "allow"` and `type_complexity = "allow"` (Bevy systems trip both, per the bevy_new_2d template), `nonstandard_macro_braces = "warn"` (Bevy's `children![]`).
  - `-D warnings` in the gate. Deprecated Bevy APIs then fail the build automatically.
  - Sim-crate `clippy.toml` bans (rec. 2).
  - Config fields `path`/`reason`/`replacement`/`allow-invalid` ([lint configuration](https://doc.rust-lang.org/clippy/lint_configuration.html)).
- **Bevy linter (`bevy_lint`):** the compatibility table tops out at **Bevy 0.18** (0.7.0-dev on `nightly-2026-04-16`) ([compat](https://thebevyflock.github.io/bevy_cli/linter/compatibility.html)). **Not usable on 0.20 yet.**
- **rust-analyzer:** rec. 6.

### Area 3: Claude Code configuration for game projects

- **Hook events and semantics:** rec. 4 ([hooks](https://code.claude.com/docs/en/hooks)).
  - Notable events: `SubagentStop` (matcher on `agent_type`), `PostToolBatch`, `TaskCompleted`, `WorktreeCreate`/`WorktreeRemove` (use these to set up and tear down per-worktree build dirs and BRP ports).
  - Notable options: `asyncRewake` for long background checks, and `once: true` in skill frontmatter.
- **Skills:** `.claude/skills/<name>/SKILL.md`. Frontmatter supports `paths`, `context: fork`, `agent`, `allowed-tools`, `disable-model-invocation`, `hooks`, `model`, `effort` and `arguments`; `` !`cmd` `` injects live output. Old `.claude/commands/*.md` still work as slash commands ([skills](https://code.claude.com/docs/en/skills)). Suggested project skills:
  - `bevy-api` (rec. 1)
  - `determinism` (rules, plus how to run shuffle and hash tests)
  - `run-game` (launch with BRP port, wait for readiness, screenshot)
  - `visual-check` (rec. 10)
  - `perf-check` (rec. 9)
  - `new-feature-crate` (scaffold `features/fl_feat_x` with sim/client split and registration)
  - `scenario` (write and run a scenario RON with `fl_cli`)
- **Subagents:** rec. 7.
- **Permissions:**
  - Rules are `Bash(cargo nextest run *)`, the same shape `PowerShell(...)` with aliases canonicalized, and `mcp__brp__*`.
  - Precedence is deny, then ask, then allow.
  - Allow `cargo check/clippy/fmt/nextest/test/run -p fl_cli`, `git status/diff/log`, and `mcp__brp__*`.
  - Ask for `cargo add/update`, `git push`, `rustup`.
  - Deny `cargo publish` ([permissions](https://code.claude.com/docs/en/permissions)).
  - **The OS sandbox does not exist on native Windows**; it needs WSL2 ([sandboxing](https://code.claude.com/docs/en/sandboxing)). Allowlists and hooks are the only guardrails here.
- **Verification as a gate:** the best-practices page lists prompt, `/goal`, Stop hook, and verification subagent, in increasing order of rigor, and recommends a fresh-context adversarial reviewer that "sees only the diff and the criteria" ([best practices](https://code.claude.com/docs/en/best-practices); [verification-loop skills article, 2026-07-22](https://claude.com/resources/articles/building-verification-loops-in-claude-code-with-skills)).
- **Public examples:**
  - [Claude Code Game Studios](https://github.com/donchitos/claude-code-game-studios): hooks such as `validate-read.sh`, which caps design-doc reads at 40 KB, and path-scoped rules per code area.
  - [godogen](https://github.com/htdt/godogen): Bevy option, prove-it-by-running loop.
  - [Liminal Warmth roguelike notes](https://liminalwarmth.substack.com/p/claude-code-context-lessons): a code-reviewer subagent with approval power, a periodic auditor that refreshes the project map in CLAUDE.md, "kill the branch and restart" for unrecoverable drift.

### Area 4: Correct API knowledge for a fast-moving engine

| Option | Pinned to our version? | Cost | Verdict |
|---|---|---|---|
| Local registry source, examples, migration guides and release notes (`bevy-0.20.0/_release-content`) | Yes (Cargo.lock) | Free | **Primary** (rec. 1) |
| Verified-snippets file (`bevy020-api-reference.md`) plus compile probes | Yes | Low | **Keep, institutionalize** |
| `bevy_manual_mcp` 0.20 index | Yes (per index) | npx from GitHub | Optional |
| docs.rs rustdoc-JSON MCPs ([cratedoc](https://github.com/possdog/mcp-rustdoc), [rust-docs-mcp 0.1.1](https://github.com/snowmead/rust-docs-mcp), mcp-docsrs) | Partly; local mode needs **nightly** rustdoc JSON | Medium | Not needed given local source |
| `bevy.org/llms.txt` | — | — | **Does not exist** (404, checked) |
| docs.rs bevy 0.20.0 | Yes | Web fetch | Built OK (1.7 GB of docs); fine for humans and occasional fetches |
| Migration digest | Yes | Low | Generate `docs/research/bevy020-migration-digest.md` from the 74 local migration-guide files: one line per change with old → new |

One more useful practice: keep a small **"known-wrong API" list** (old names the model tends to produce, for example pre-0.20 shader dialects, deprecated `Interaction`, `EventReader`-era names). A `PostToolUse` hook greps new code for them and injects a correction. Deprecations are already caught by `-D warnings`. This list catches the ones that silently compile or were removed outright.

### Area 5: Automated verification without a human

- **Headless sim tests:** `MinimalPlugins` patterns are in `bevy-0.20.0/tests/how_to_test_apps.rs`, VERIFIED. Determinism gates: rec. 2. Properties and fuzzing: rec. 8. Golden replays: rec. 8.
- **Scripted playtests and bots:**
  - `fl_cli run-scenario` with scripted command timelines plus end-state assertions: "attacker captures flag by tick N", "artillery suppresses squad within 30 s".
  - Later, add a scripted AI controller (design §5.5 seam) to play both sides for long soak runs, checking invariants and recording journals.
  - Caleb Leak's agent-built games improved most from a scripted input player plus screenshots, together with format linters and a "game must be winnable" check ([post](https://calebleak.com/posts/dog-game)).
- **Content linters:** Godot users found agent-written structured files were the top silent-failure source, and fixed it with linters ([HN discussion](https://news.ycombinator.com/item?id=47146712)). Our equivalent is `fl_cli data-check` as a hook and gate whenever `assets/**/*.ron` changes.
- **Perf regression:** rec. 9.
- **Visual:** rec. 10. Research numbers: VideoGameQA-Bench and bug-frame retrieval (links above).

### Area 6: Lessons and failure modes from agent-heavy game development

| Failure mode | Evidence | Mitigation here |
|---|---|---|
| Hallucinated or outdated Bevy APIs | Ardan Labs Bevy Pong didn't compile; godogen notes Bevy API churn | Rec. 1 (local source, probes), `-D warnings`, known-wrong-API list |
| Coordinate-convention confusion in Bevy | Caleb Leak: "Claude struggled with its coordinate conventions" | Write a short `conventions.md` rule (sim FxVec2 x/y ↔ world XZ, Y-up, iso camera angles), add a unit test for the mapping, and keep a debug-axes overlay |
| Test cheating / reward hacking | EvilGenie (Claude Code and Codex both observed); [Verification Horizon, arXiv 2606.26300](https://arxiv.org/pdf/2606.26300) | Rec. 4 guards, a separate test-author role, cargo-mutants, a reviewer test-integrity checklist |
| "Green tests, unplayable game" | Practitioner accounts | Scripted playtests with outcome assertions; human feel-check at milestones |
| Architecture drift | Liminal Warmth (static context goes stale) | `cargo tree` dependency gates, schedule-graph snapshot (`SerializeSchedulesPlugin` or `bevy_mod_debugdump` 0.17.0, which supports Bevy 0.20), a periodic auditor agent, path-scoped rules |
| Flaky visual checks | VLM benchmark numbers | Semantic asserts first, tolerant per-machine goldens, VLM advisory only |
| Context bloat from huge logs and journals | Claude Code best practices | `fl_cli` query filters, nextest `failure-output = "final"`, tests run in subagents that return only failures |
| Over-process | Game Studios author's own measurement: 30 docs before code at "standard" rigor | Keep to 4–5 roles; superpowers already supplies process |
| Runtime blindness | Agents "can't press play" without a loop | BRP MCP plus state-gated screenshots (rec. 5) |

---

## Things to avoid

1. **A `cargo check`/`clippy` hook after every Edit** across parallel subagents. It causes lock contention and minutes of stall. Use the Stop/SubagentStop gate and LSP diagnostics instead.
2. **One shared `target-dir` for all concurrent worktrees**, which serializes them on the build lock. Also avoid **per-worktree `target/` in deep `.claude/worktrees/...` paths**, which triggers LNK1104. Use a per-worktree `build-dir` on a short path.
3. **Nightly-only speedups as the default** (Cranelift, `-Zshare-generics`, `-Zthreads`, `feature-unification`). They conflict with the pinned-stable toolchain, and Windows support lags.
4. **`bevy_lint`** until a release lists Bevy 0.20. **Hot patching** for agent loops.
5. **Stale or off-version Bevy MCPs:** `bevy_debugger_mcp` (2025-08), `Nub/bevy_mcp` (0.18), `rltvty/bevy-mcp`, and unreviewed forks.
6. **Vision models as a hard pass/fail gate.** Pixel-exact goldens compared across machines or GPUs. Goldens that include TAA, particles, wall-clock animation or unseeded randomness.
7. **Mouse-click-driven tests of sim behavior.** Drive the sim through `PlayerCommand`s; click tests belong only to the input-mapping layer.
8. **Absolute wall-clock thresholds in unit tests,** or gating perf on a thermally variable laptop without paired A/B runs. **gungraun/iai-callgrind** (Valgrind is Linux-only) on this Windows box.
9. **Letting implementer agents bless snapshots, goldens, replay hashes or perf baselines.**
10. **Assuming BRP `world.*` methods see the sim.** They see the App's main World only. Expose sim state through `fl/*`. Never ship `RemotePlugin`/`RemoteHttpPlugin` in release builds.
11. **A "log and continue" fallback error handler** (new in Bevy 0.20, "Catching Panics") in tests or `fl_cli`. Keep the default re-panic.
12. **`dssim`/`dssim-core`** for image diffs: it is AGPL-3.0. Use `image-compare` (MIT).
13. **Taking perf readings while the game window is unfocused** under default `WinitSettings` (low-power reactive, capped near 60 Hz).
14. **Heavyweight 40–50-agent "studio" templates and long CLAUDE.md files.** Adherence drops above about 200 lines, per the docs.
15. **Expecting `isolation: worktree` subagents to start from your feature branch.** They branch from the default branch.

---

## Version table (crates.io, checked 2026-10-23)

| Crate / tool | Version (date) | Bevy 0.20 | License |
|---|---|---|---|
| bevy_brp_mcp | 0.23.0 (2026-10-09) | yes (`bevy ^0.20.0`) | MIT/Apache-2.0 |
| bevy_brp_extras | 0.23.0 (2026-10-09) | yes | MIT/Apache-2.0 |
| bevy_mod_debugdump | 0.17.0 (2026-10-17) | yes | — |
| bevy-inspector-egui / bevy_egui | 0.38.0 / 0.43.1 (2026-10) | yes | — |
| bevy_assert (Codeberg, `AppTest`) | 0.1.5 (2026-10-19) | yes (`^0.20`) | — (thin docs, unproven) |
| cargo-nextest | 0.9.149 (2026-10-20) | n/a | — |
| sccache | 0.18.0 (2026-09-16) | n/a | — |
| cargo-hakari | 0.9.39 (2026-09-19) | n/a | — |
| bacon | 3.26.0 (2026-09-26) | n/a | — |
| cargo-deny | 0.20.2 (2026-07-09) | n/a | — |
| cargo-mutants | 27.1.0 (2026-06-02) | n/a | MIT |
| proptest / proptest-state-machine | 1.12.0 / 0.9.0 (2026-10-10) | n/a | MIT/Apache-2.0 |
| insta / cargo-insta | 1.49.0 (2026-10-03) | n/a | — |
| image-compare | 0.5.0 (2025-08-18) | n/a | MIT |
| tango-bench | 0.9.0 (2026-10-16) | n/a | MIT |
| criterion | 0.8.2 (2026-02-04) | n/a | — |
| tracing-tracy | 0.12.0 (2026-08-25) | Bevy pins its own via `trace_tracy` | — |
| bevy_lint | max 0.7.0-dev → Bevy 0.18 | **no** | — |
| bevy_debugger_mcp | 0.1.8 (2025-08-23) | no evidence | — |

## Sources (primary)

**Bevy (official, tagged v0.20.0, plus local crate sources):**
- [Bevy 0.20 release notes](https://bevy.org/news/bevy-0-20/) and [0.19→0.20 migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/)
- [Bevy setup / fast compiles](https://bevy.org/learn/quick-start/getting-started/setup/)
- Source files: [`ci_testing/config.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_dev_tools/src/ci_testing/config.rs), [`headless_renderer.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/app/headless_renderer.rs), [`remote/integration_test.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/examples/remote/integration_test.rs), [`example-run.yml`](https://github.com/bevyengine/bevy/blob/v0.20.0/.github/workflows/example-run.yml), [`builtin_methods.rs`](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_remote/src/builtin_methods.rs)

**Claude Code docs:** [hooks](https://code.claude.com/docs/en/hooks), [sub-agents](https://code.claude.com/docs/en/sub-agents), [memory/rules](https://code.claude.com/docs/en/memory), [skills](https://code.claude.com/docs/en/skills), [best practices](https://code.claude.com/docs/en/best-practices), [code intelligence](https://code.claude.com/docs/en/plugins/code-intelligence), [permissions](https://code.claude.com/docs/en/permissions), [sandboxing](https://code.claude.com/docs/en/sandboxing), [rust-analyzer-lsp plugin](https://claude.com/marketplace/plugins/rust-analyzer-lsp)

**Rust tooling docs:**
- Cargo: [config (build-dir)](https://doc.rust-lang.org/cargo/reference/config.html), [unstable features](https://doc.rust-lang.org/cargo/reference/unstable.html)
- [clippy lint configuration](https://doc.rust-lang.org/clippy/lint_configuration.html), [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html)
- nextest: [Windows](https://nexte.st/docs/installation/windows/), [JUnit](https://nexte.st/docs/machine-readable/junit/), [record/rerun](https://nexte.st/docs/features/record-replay-rerun/), [config reference](https://nexte.st/docs/configuration/reference/)
- [sccache releases](https://github.com/mozilla/sccache/releases), [Rust 1.90 lld](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable), [bevy_lint compatibility](https://thebevyflock.github.io/bevy_cli/linter/compatibility.html)

**Agent tooling:** [natepiano/bevy_brp](https://github.com/natepiano/bevy_brp), [bevy_brp_extras docs](https://docs.rs/bevy_brp_extras/0.23.0/bevy_brp_extras/), [kinkirill/bevy_manual_mcp](https://github.com/kinkirill/bevy_manual_mcp), [OilProducts/agent-skills](https://github.com/OilProducts/agent-skills), [tango](https://github.com/bazhenov/tango), [tickwise](https://docs.rs/crate/tickwise/0.2.0), [tracy-csvexport](https://dyn.manpages.debian.org/unstable/tracy-csvexport/tracy-csvexport.1)

**Experience reports:** [godogen](https://github.com/htdt/godogen), [Claude Code Game Studios](https://github.com/donchitos/claude-code-game-studios), [Caleb Leak, "dog game"](https://calebleak.com/posts/dog-game), [HN thread](https://news.ycombinator.com/item?id=47146712), [Liminal Warmth](https://liminalwarmth.substack.com/p/claude-code-context-lessons), [Ardan Labs Bevy test (via daily.dev)](https://daily.dev/posts/can-ai-really-build-games-in-rust-agentic-ai-vs-bevy-engine--txjound5e)

**Research:** [VideoGameQA-Bench](https://arxiv.org/html/2505.15952v1), [VLM bug-frame retrieval](https://arxiv.org/html/2508.04895), [EvilGenie](https://arxiv.org/abs/2511.21654), [Verification Horizon](https://arxiv.org/pdf/2606.26300)
