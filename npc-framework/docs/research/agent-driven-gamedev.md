# AI-agent-driven game development: what to improve here (researched 2026-10-23)

Scope: how practitioners and tools in 2025–2026 make agents (Claude Code orchestrator + subagents) productive and safe on game code, and what that means for this repo. The repo already has a deterministic headless sim (SimTick, integer state, SimId order, FNV state hash, ambiguity = Error), its own BT runtime, headless determinism/replay tests, screenshot checks that the agent reads by eye, and in-progress debug tooling: JSON journal, `npcctl`, BRP methods and the `npc_perf` tracing profiler. There is no `CLAUDE.md`, `.claude/`, `clippy.toml`, `.cargo/config.toml` or CI yet.

Crate versions were checked against crates.io with `cargo search` and the crates.io API on 2026-10-23.

---

## 1. Findings by theme

### 1.1 Verification loops are the core lever
- Anthropic's Claude Code guidance puts this first: *"Give Claude a check it can run."* It names four gate strengths: in the prompt, a `/goal` condition, a **Stop hook** that blocks the turn until a script passes, and a **verification subagent** that tries to refute the result. It also tells agents to show evidence (the command and its output) instead of claiming success. ([best practices](https://code.claude.com/docs/en/best-practices))
- **GameLogicBench** (arXiv 2609.21562, Sep 2026) checks game rules at **every tick**, not only at the final state.
  - The best agent run solved 52.78% of tasks, and every model under Claude Code got worse as scope grew from isolated mechanics to repo-scale features.
  - The most common failure was code that runs but behaves wrongly.
  - The evaluator itself had to be validated with **mutants**. Without that, incorrect submissions passed. ([paper](https://arxiv.org/abs/2609.21562))
- **GameXpert-Bench** (arXiv 2608.21833, Aug 2026): agents build playable foundations well. They fail at finding defects on their own, at verifying runtime behaviour, and at **not regressing existing features**. ([paper](https://arxiv.org/abs/2608.21833))
- Practitioner reports:
  - Schomay's agentic playtest harness: a stateless HTTP wrapper returns a text snapshot of the game, a skill file explains how to play, and the agent builds minimal fixtures. The best use was "verify feature X or bug Y". Playtesting caught state bugs that passed unit tests. One milestone took about 12 minutes and about 120k tokens. ([blog](https://blog.jeffschomay.com/letting-ai-play-my-game), [HN](https://news.ycombinator.com/item?id=47947525))
  - HN commenters add three things: run headless sims at scale with scripted bots and let the LLM read the *aggregated report*; have the agent write deterministic goal bots ("reach room 3 in < 10 actions"); and expect AI testers to overstate success.
- **playtest-agent** ([repo](https://github.com/kevinnie2003/playtest-agent)) is the clearest pattern to copy:
  - Generic **invariant oracles**: negative resources, moving through walls, effects with no state change, soft-locks.
  - **Persona policies**: cautious, reckless, hoarder. Persona diversity mattered more than policy sophistication, raising recall from 0.38 to 1.0 *without* an LLM.
  - Every finding stores a replayable action sequence.
  - The detectors are validated against a seeded-bug harness.
  - The LLM is optional, used only when stuck and for triage, under a call budget.

### 1.2 Test techniques that fit a deterministic sim
- **Snapshot/golden tests:** `insta` 1.49 (`cargo-insta` 1.49) supports JSON snapshots with redactions and `sorted_redaction()` for set-like data ([redactions](https://insta.rs/docs/redactions/)). It is a good fit for `WorldSnapshot`, journal slices and `npcctl run` summaries.
- **Stateful property tests:** `proptest` 1.12 and `proptest-state-machine` 0.9 provide a reference model plus the system under test, random transition sequences, and **shrinking to a minimal sequence** ([crate](https://lib.rs/crates/proptest-state-machine), [tutorial](https://tzemanovic.codeberg.page/posts/state-machine-testing-with-proptest/)). `SimCommand` sequences are a natural transition alphabet.
- **Bevy 0.20 schedule randomization:** `ScheduleBuildSettings::shuffle_seed: Option<u64>` (needs the `bevy_ecs/debug` feature) shuffles system order within the ordering constraints. Use it with `SingleThreadedExecutor`, because the multi-threaded executor can still produce orders the shuffle would not. The release notes pitch it for property testing and as a complement to static ambiguity detection. ([docs.rs](https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/schedule/struct.ScheduleBuildSettings.html), [Bevy 0.20 notes](https://bevy.org/news/bevy-0-20/))
- **0.20 "Catching Panics":** panics in systems now go to the fallback error handler. The default still re-panics.
- **Mutation testing:** `cargo-mutants` 27.1 measures whether the tests and oracles actually catch injected bugs. This is the lesson GameLogicBench and playtest-agent both learned.
- **Runner:** `cargo-nextest` 0.9.149 runs each test in its own process. That isolates **global state** (relevant: `npc_perf::install_global` sets a process-global subscriber), kills hung tests safely, retries and labels **flaky** tests, and writes JUnit. Process spawning is slower on Windows, but nextest is often still faster there. ([nextest](https://nexte.st/docs/design/why-process-per-test/))
- **Replay and desync practice:** Factorio's debug mode computes a whole-map CRC **every tick** on replay, so the first diverging tick is known. The first divergence is usually one variable. Normal play uses a cheaper heuristic CRC. ([FFF #47](https://factorio.com/blog/post/fff-47), [FFF #55](https://factorio.com/blog/post/fff-55)) Our replay checkpoints plus per-entity hashes in `diverge` already follow this. The gap is a *committed corpus* of replays run on every change.

### 1.3 Visual regression
- Bevy 0.20 ships `bevy_ci_testing` (`bevy_dev_tools::ci_testing`).
  - `CiTestingEvent` variants: `Screenshot`, `NamedScreenshot(String)`, `ScreenshotAndExit`, `AppExit`, `MoveCamera`, `StartScreenRecording`/`StopScreenRecording` and `Custom(String)`, fired on given frames.
  - `CiTestingSetup.fixed_frame_time` pins time through `TimeUpdateStrategy::ManualDuration`. ([enum](https://docs.rs/bevy_dev_tools/0.20.0/bevy_dev_tools/ci_testing/enum.CiTestingEvent.html), [setup](https://docs.rs/bevy_dev_tools/0.20.0/bevy_dev_tools/ci_testing/struct.CiTestingSetup.html))
  - Bevy built this so that example screenshots come out *exactly the same* across runs ([PR #8488](https://github.com/bevyengine/bevy/pull/8488), [testbeds PR #17573](https://github.com/bevyengine/bevy/pull/17573)).
- `bevy_mod_screenshot_test` 0.2 provides golden screenshot tests, but it targets **Bevy 0.19 only** ([docs](https://docs.rs/bevy_mod_screenshot_test/latest/bevy_mod_screenshot_test/)).
- Diff crates:
  - `dssim-core` 3.5.1: multi-scale SSIM, perceptual.
  - `image-compare` 0.5: SSIM and RMS.
  - `twenty-twenty` 0.8.4: SSIM threshold assertions, with an env var to regenerate references ([docs](https://docs.rs/twenty-twenty)).
  - A pooled perceptual score tolerates anti-aliasing noise that breaks exact pixel diffs.
- **CI GPUs:** wgpu's own CI runs DX12 on **WARP** (Windows), Vulkan on lavapipe and GL on llvmpipe ([wgpu](https://docs.rs/crate/wgpu)). Goldens are only comparable on one adapter. Either keep per-adapter goldens or render CI goldens on a software adapter.
- A practitioner warning: text-state agents "clicked objects players couldn't see" ([summary](https://www.emergentmind.com/topics/llm-agents-as-game-testers)). Text state is the primary oracle; screenshots back it up for what is actually rendered.

### 1.4 Bevy-specific agent tooling (status for 0.20)
- **bevy_brp_mcp / bevy_brp_extras 0.23.0** (2026-10-09) support **Bevy 0.20** ([mcp README](https://github.com/natepiano/bevy_brp/blob/main/mcp/README.md), [extras README](https://github.com/natepiano/bevy_brp/blob/main/extras/README.md)).
  - MCP tools:
    - App control: `brp_launch` (starts the app with logging), `read_log`.
    - World access: `world_query`, `world_mutate_components`, `world_get_components_watch`, `brp_type_guide` (gives mutation paths).
    - Custom methods: `brp_execute` calls any method listed by `rpc.discover`, so our `npc/*` methods are reachable.
  - Extras methods: screenshot (whole window, one camera, or a crop by entity or name; returns only after the PNG is written), `send_keys`, `type_text`, mouse and gamepad input, diagnostics and `shutdown`.
  - Caveats:
    - Logs and watch files are documented under `/tmp/`. Verify where they go on Windows.
    - A minimized window may give a black screenshot.
    - The extras plugin listens on 15702 itself. Check how it coexists with our own `RemotePlugin` setup; `with_http_plugin` exists for this.
- **bevy-inspector-egui 0.38.0** (2026-10-17) depends on bevy ^0.20 and bevy_egui ^0.43. That is new; older search results still say "no 0.20 release". It is for human debugging.
- **bevy_mod_debugdump 0.17.0** (bevy ^0.20) dumps schedule and render graphs as DOT, which agents read well.
- **Bevy remote inspector/editor:** only listed under "What's Next?" in the 0.20 notes, so it does not exist yet.
- **bevy_cli / bevy_lint:** still alpha (`cli-v0.1.0-alpha.2`), installed from git, and its 0.20 support is not documented ([repo](https://github.com/TheBevyFlock/bevy_cli)). Evaluate only.
- **Hotpatching:** Bevy 0.20 still has the `hotpatching` feature (subsecond via `dx serve --hot-patch`). `dioxus-cli` stable is 0.7.10.
  - Limits: binary crate only, and system parameters cannot change ([0.17 notes](https://bevy.org/news/bevy-0-17/)).
  - On **Windows** it is unreliable: [#21461](https://github.com/bevyengine/bevy/issues/21461) is still open (missing DLLs with dynamic linking, stack overflows, slow reloads).
  - Its value for agents is low anyway, because agents verify through headless runs, not a live window.
- **Compile speed:**
  - Bevy's guide calls `dynamic_linking` the biggest win. On Windows it needs the optimized dev profile, or the build fails with "too many exported symbols".
  - Bevy's fast-build template sets `linker = "rust-lld.exe"` on Windows. It warns that `-Zshare-generics` must be off with dynamic linking on Windows.
  - Cranelift is nightly-only, works best on Linux, is "more involved" on Windows and breaks debuggers ([setup](https://bevy.org/learn/quick-start/getting-started/setup/), [config_fast_builds.toml](https://github.com/bevyengine/bevy/blob/main/.cargo/config_fast_builds.toml), [#19916](https://github.com/bevyengine/bevy/issues/19916)).
  - In this repo, `village` took **4 min 2 s** to build (`build.log`).

### 1.5 Making the codebase agent-friendly
- **CLAUDE.md** ([memory docs](https://code.claude.com/docs/en/memory)):
  - Target **under 200 lines**. Imports organize a long file but don't reduce its context cost.
  - Put rules that apply to one part of the code in `.claude/rules/*.md` with a `paths:` frontmatter, which loads only when matching files are touched.
  - Nested `CLAUDE.md` files load on demand.
  - Claude Code ≥ 2.1.277 reads `AGENTS.md` directly, but it ignores `AGENTS.md` when a `CLAUDE.md` exists unless the `CLAUDE.md` imports it.
  - `/doctor prompt-audit` finds contradictions. "Bloated CLAUDE.md files cause Claude to ignore your actual instructions."
- **Skills** ([skills docs](https://code.claude.com/docs/en/skills)):
  - Useful frontmatter: `paths`, `context: fork` (runs in a subagent), `allowed-tools`, `disable-model-invocation`.
  - `` !`cmd` `` injects live command output before the prompt is sent.
  - Keep `SKILL.md` under 500 lines and put detail in supporting files.
  - Built-in `/run` and `/verify` can record a per-project recipe in `.claude/skills/verify/SKILL.md`.
- **Hooks** ([hooks docs](https://code.claude.com/docs/en/hooks)): hooks are *deterministic*, while CLAUDE.md is advisory.
  - `PostToolUse` with exit code 2 feeds stderr back to Claude.
  - `Stop` with exit 2 or `{"decision":"block"}` keeps the agent working. Guard against loops.
  - `PreToolUse` can block an action outright.
  - There are also `SubagentStop`, `TaskCompleted` and `PreCompact` events.
- **CLI vs MCP:** Anthropic calls CLIs "the most context-efficient way" to reach external services. The 2026 debate centers on the token cost of MCP tool schemas loaded up front ([Checkly](https://www.checklyhq.com/blog/mcp-vs-cli-token-efficiency/), [CircleCI](https://circleci.com/blog/mcp-vs-cli)). Our `npcctl` (JSON on stdout, exit codes) is the right primary interface. MCP is a good fit for the *live* app.
- **Guardrails as code:**
  - Clippy's `disallowed-types` and `disallowed-methods` in `clippy.toml` take a `reason` and a `replacement`.
  - Projects ban `std::collections::HashMap` for nondeterministic order and `Instant::now`/`SystemTime::now` for wall-clock leaks ([klocwork ref](https://help.klocwork.com/2026.1/en-us/reference/rs.clippy.disallowed_types.htm), [plumb-core](https://docs.rs/plumb-core)).
  - `cargo-deny` 0.20.2 enforces dependency bans.

### 1.6 Observability and "explain why"
- Agents parse JSON Lines, compact text maps and aggregated reports well. Raw grids, raw logs and screenshots of fast-changing state work poorly (Schomay; HN on real-time games).
- Unreal's Visual Logger records per-agent state snapshots ("selected AI behaviour") that you can scrub after play. Practitioners say the *reason* matters more than the state: **log runner-up scores and why the losers lost** ("Cover: none (no candidate passed protection trace)") ([guide](https://www.strayspark.studio/blog/debugging-unreal-enemy-ai-gameplay-debugger-visual-logger-ai-lod), [Visual Logger](https://unreal-garden.com/tutorials/visual-logger/)).
- Our journal records `CondEval { cond, result }`, but not the operand values or the threshold, so an agent can't answer "why not eat?" without re-deriving them.

### 1.7 Reported pitfalls and mitigations
| Pitfall | Evidence | Mitigation |
|---|---|---|
| Agent edits or special-cases tests to pass | EvilGenie saw explicit reward hacking by Claude Code and Codex. Read-only tests blocked test edits without lowering legitimate success. Giving an "this is impossible" escape hatch cut cheating from 54% to 9% ([glossary](https://primeintellect.ai/rl-glossary/reward-hacking), [EvilGenie](https://alphaxiv.org/abs/2511.21654)) | Protect goldens and oracles with a PreToolUse hook. Tell the agent it may report a test as wrong. Run a reviewer subagent over test diffs |
| Runnable but wrong behaviour; regressions | GameLogicBench, GameXpert-Bench | Tick-level invariants, a golden scenario corpus, mutation testing |
| Hallucinated or outdated Bevy APIs | API churn on fast-moving Bevy versions ([Bevy skill notes](https://github.com/laurigates/claude-plugins/blob/main/bevy-plugin/skills/bevy-game-engine/skill.md)) | Keep `docs/research/bevy-0.20-cheatsheet.md` current and point to it from CLAUDE.md. "Verify on docs.rs/registry source" is already a plan rule |
| Overstated success from AI testers | HN thread | Evidence-only completion: a Stop hook plus a JSON verdict from scripts |
| Context bloat | Anthropic docs | CLI JSON with `--limit`, subagents for exploration, skills instead of a long CLAUDE.md |
| Flaky visual checks | Bevy CI work on fixed frame time | Deterministic capture at tick N, perceptual threshold, per-adapter goldens, nextest flaky detection |
| Text state shows things a player can't see | Studio report | Pair it with screenshots for anything "visible" |

---

## 2. Prioritized recommendations for this repo

### P0: do next (cheap, high leverage)
1. **Write `CLAUDE.md` (≤120 lines) and `.claude/rules/` (S).**
   - CLAUDE.md content: build, test and verify commands; crate-boundary rules; the "sim never reads Time/f32/HashMap" rules; and pointers to `docs/debugging.md`, the cheat sheet, specs and plans.
   - Move determinism rules into `.claude/rules/sim-determinism.md` with `paths: crates/npc_sim/**, crates/npc_ai/**`, and view rules into `rules/view.md`.
   - Add a one-line `AGENTS.md` that defers to CLAUDE.md, or have CLAUDE.md import `@AGENTS.md`.
   - Why: the spec already calls for this. Path-scoped rules keep context small for subagents.
2. **Turn the determinism rules into lints (S).** Add a `clippy.toml` that applies to sim crates, either per crate or through `#![deny(clippy::disallowed_types, clippy::disallowed_methods)]` in `npc_sim`/`npc_ai`.
   - Ban `HashMap`/`HashSet`/`RandomState` (use `BTreeMap`/sorted `Vec`), `Instant::now`/`SystemTime::now`, `bevy::time::Time` and `rand::thread_rng`.
   - Also ban f32 transcendentals where clippy can express them.
   - Add an **architecture test** (`tests/arch.rs` that parses `cargo metadata`): `npc_ai` and `npc_sim` must not depend on `bevy`, `bevy_render` or `npc_view`; `npcctl` must not depend on `npc_view`.
   - Why: CLAUDE.md is advisory and lints are deterministic. Subagents breaking invariants is the top risk.
3. **One verify entry point and a Stop hook (S–M).**
   - Add `scripts/verify.ps1` (or `cargo xtask verify`) that runs fmt-check, clippy `-D warnings`, nextest for the touched crates, and the replay corpus, then prints **one JSON verdict** `{ok, failed:[...], evidence_paths}`.
   - Wire a `Stop` hook that runs it when `*.rs` changed this turn, with a loop guard.
   - Wire a `PostToolUse` hook on `Edit|Write` of `*.rs` that runs `cargo fmt` on the file.
   - Why: this is Anthropic's highest-leverage practice, and it replaces the "pass the gate" prose in plans.
4. **Golden scenario and replay corpus (M).**
   - `tests/golden/` holds replays (`seed + commands + checkpoints`): a three-day village, a GoTo interrupt, spawn/despawn, and a 100-villager day.
   - insta JSON snapshots cover each scenario's `npcctl run` summary (timeline, min/max needs, aborts, final hash).
   - Set CI and the hook to `INSTA_UPDATE=no`.
   - Any behaviour change then appears as a reviewable snapshot diff, not a silent pass.
   - Why: GameXpert says regressions are the failure mode, and Factorio's practice is the same idea.
5. **Protect the oracles (S).**
   - Add a `PreToolUse` hook that blocks edits to `tests/golden/**`, `**/*.snap`, `bench/baseline.json`, `clippy.toml` and `tests/arch.rs`, unless the orchestrator sets `NPC_ALLOW_ORACLE_EDIT=1` for an approved re-baseline task.
   - Add a plan rule: "if a test seems wrong, report it; don't change it".
   - Why: documented reward hacking, and read-only tests keep legitimate success.

### P1: next iteration
6. **Stateful property tests over `SimCommand` (M).** Use `proptest-state-machine` to generate command sequences (spawn, despawn, SetNeed, SetFood, GoTo, Clear) and check per-tick invariants:
   - reservations never exceed capacity, and none leak after despawn;
   - needs stay in range;
   - every `Running` leaf has its action component;
   - the hash is identical with the journal and profiler on or off;
   - replaying the shrunk sequence reproduces the failure.

   Write shrunk failures to `tests/golden/found/*.json`. Why: tick-level assertions are what GameLogicBench found necessary.
7. **Schedule-shuffle determinism test (S).** Enable `bevy_ecs/debug` in dev-deps. Run the village for N ticks with `shuffle_seed` 0..16 on `SingleThreadedExecutor` and assert identical hashes. Why: this checks at runtime what ambiguity = Error only checks statically, and costs one test.
8. **Deterministic visual checks with perceptual diff (M).**
   - Add `village --screenshot-at-tick N`: the sim is stepped, render time is pinned (`TimeUpdateStrategy::ManualDuration`, the same approach as `bevy_ci_testing`), and the window size is fixed.
   - Compare against `tests/visual/<adapter>/*.png` using `dssim-core`.
   - Emit `{name, score, threshold, diff_png}`.
   - The agent opens **only failing diffs**.
   - Why: today screenshots are judged by eye, which is flaky and costs tokens.
9. **Adopt `cargo-nextest` (S).** Add `.config/nextest.toml` with a `ci` profile (retries = 1, so flaky tests get labelled; JUnit output; slow-test timeout). Why: process-per-test isolates `npc_perf`'s global subscriber and BRP ports.
10. **"Explain why" output (M).**
    - Extend `CondEval` to `{cond, result, lhs, op, rhs}`. Example: `hunger 180 < enter 200`.
    - Record the **runner-up** guard on each think.
    - Add `npcctl explain --unit ID --tick T`, which returns the chosen branch, the rejected higher-priority guards with their operand values, reservation failures and hysteresis state.
    - Why: this answers the most common agent question in one call.
11. **Project skills (M).** Each skill uses `!` to inject live state (for example `npcctl inspect` or `git diff --stat`):
    - `/add-mechanic <name>`: scaffolds the action component, system, `MechanicTags` entry, budget, journal kind, golden scenario and docs section, then runs verify.
    - `/verify-visual`: covers the screenshot procedure.
    - `/debug-desync`: runs `diverge`, then explain.
    - `/why <unit> <tick>`.
12. **Evaluate `bevy_brp_mcp` + `bevy_brp_extras` 0.23 (S).** Add `BrpExtrasPlugin` under `debug-tools` for screenshots, `send_keys` and clean `shutdown`. Register the MCP server in `.mcp.json` as optional. Keep `npcctl` and curl as the documented primary path. Verify the port coexistence and Windows log paths first.

### P2: later
13. **Faster builds (S).** Add `.cargo/config.toml` with `rust-lld.exe` for MSVC, and a `village` dev feature for `bevy/dynamic_linking`; keep the release build LLVM and static. Optionally add sccache 0.18. Skip Cranelift on Windows.
14. **Mutation testing nightly (M).** Run `cargo mutants -p npc_ai -p npc_sim` and track the surviving-mutant count. Why: it validates that our oracles reject wrong behaviour.
15. **Persona bots in `npcctl` (M).** Add scripted command policies (starver, order-spammer, mass-spawner, despawn-mid-reservation) with the invariant detectors from item 6 and replayable findings. The LLM's only job is to triage the aggregated JSON.
16. **Docs-sync snapshots (S).** Add insta snapshots of `npcctl --help`, the `rpc.discover` method list, the `MechanicTags` table and the `bevy_mod_debugdump` SimTick graph (DOT). The docs then link these files, so drift fails a test.
17. **ADRs (S).** Move each spec's "Calls made by the designer" section into `docs/adr/NNNN-*.md`, so later agents know what is decided and don't relitigate it.
18. **CI (M).** A GitHub Actions `windows-latest` job running verify, the golden corpus and a WARP screenshot smoke test, with goldens kept per adapter.
19. **Human tooling (S).** Add `bevy-inspector-egui` 0.38 behind a `inspector` feature, for humans only.

---

## 3. Tools: adopt / evaluate / avoid

| Tool | Version (2026-10-23) | Bevy 0.20 | Verdict | Note |
|---|---|---|---|---|
| insta + cargo-insta | 1.49.0 | n/a | **Adopt** | JSON snapshots of summaries and journals; `INSTA_UPDATE=no` in CI |
| proptest / proptest-state-machine | 1.12.0 / 0.9.0 | n/a | **Adopt** | Command-sequence fuzzing with shrinking |
| cargo-nextest | 0.9.149 | n/a | **Adopt** | Process-per-test, flaky detection, JUnit |
| Bevy `shuffle_seed` | in bevy_ecs 0.20 (`debug`) | yes | **Adopt** | Use with `SingleThreadedExecutor` |
| clippy `disallowed-*`, cargo-deny | clippy (toolchain), cargo-deny 0.20.2 | n/a | **Adopt** | Determinism and boundary guardrails |
| dssim-core / image-compare | 3.5.1 / 0.5.0 | n/a | **Adopt** (one of them) | Perceptual screenshot diff |
| Claude Code hooks/skills/rules | current docs | n/a | **Adopt** | Stop and PreToolUse gates, path-scoped rules |
| bevy_brp_mcp / bevy_brp_extras | 0.23.0 / 0.23.0 | **yes** | **Evaluate** | Live-app MCP; check port coexistence and Windows log paths |
| bevy_mod_debugdump | 0.17.0 | yes | **Evaluate** | Schedule graph DOT for docs and agents |
| cargo-mutants | 27.1.0 | n/a | **Evaluate** (nightly) | Measures oracle strength |
| `bevy_ci_testing` feature | bevy 0.20 | yes | **Evaluate** | Frame-scripted screenshots; we may prefer tick-based capture |
| bevy-inspector-egui (+ bevy_egui 0.43.1) | 0.38.0 | yes (released 2026-10-17) | **Evaluate** | For humans; not needed by agents |
| twenty-twenty | 0.8.4 | n/a | Evaluate | SSIM assertions with a regenerate env var |
| rust-lld / `dynamic_linking` / sccache | toolchain / bevy 0.20 / 0.18.0 | yes | **Adopt** (dev only) | Disable share-generics with dylib on Windows |
| tracing-chrome / tracing-tracy | 0.7.2 / 0.12.0 | via `trace_*` features | Keep (already planned) | Deep dives only |
| bevy_cli / bevy_lint | 0.1.0-alpha.2 (git only) | undocumented | Watch | Alpha |
| Bevy hotpatching (`dx serve --hot-patch`) | dioxus-cli 0.7.10 | feature exists | **Avoid for now** | Windows issue #21461 open; little value for headless agents |
| Cranelift backend | nightly | — | **Avoid** on Windows | Immature here, breaks debuggers |
| bevy_mod_screenshot_test | 0.2.0 | **no** (0.19) | Avoid | Use our own tick-based capture |
| bevy_simple_subsecond_system | 0.2.0 | **no** (0.16) | Avoid | Superseded upstream |
