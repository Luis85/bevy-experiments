# Task 4 report: agent environment

**Status:** DONE_WITH_CONCERNS. **Commit:** `da4b28c chore(agents): CLAUDE.md, path rules, gate and guard hooks, role agents, bevy-api skill, .mcp.json` (trailers `Refs: ADR-0013, ADR-0009`, `Co-Authored-By: Claude Opus 5.5 …`). Base: `.gate-base` = 51254b7.

## What I implemented

- `xtask/src/hooks.rs` (new):
  - `Role { Lead, Implementer, ReadOnly }`, `role_from(input, flag)` (reads `FL_ROLE`) and `role_resolve(input, flag, env)`, which is the testable core. Precedence: `--role` > `FL_ROLE` > `agent_type` > `Lead`. `sim-engineer` and `client-engineer` map to Implementer; `reviewer` and `perf-analyst` map to ReadOnly. A plugin prefix `x:` is ignored. `auto` and unknown values fall through to the next source.
  - `Decision { Allow, Deny, Ask }` and `to_hook_json()`, which returns `hookSpecificOutput.permissionDecision` plus a reason; `Allow` prints nothing.
  - `guard_edit(owners, role, path)`:
    - ReadOnly roles are denied everything.
    - Implementers are denied `[protected]` paths. The reason contains "protected path", points to bless and quotes the `BLOCKED:` hatch.
    - Pending `*.new` files are allowed, matching the gate's `protected` row.
  - `guard_bash(role, command)`:
    - Commands are normalized first: quotes are dropped, `\` becomes `/`, `A = b` becomes `A=b`, `cargo-insta` becomes `cargo insta`, and `cargo +toolchain`, `git -C <dir>` and `git -c k=v` are stripped.
    - Denied to every role except Lead:
      - `cargo insta accept|review|reject|--accept*|--review`
      - `INSTA_FORCE_PASS`
      - `INSTA_UPDATE=always|unseen`, including the PowerShell `$env:` form
      - `--bless` and `xtask bless`
      - `git checkout`/`git restore` on `tests/`, `snapshots/` or `scenarios/`
    - ReadOnly roles are also denied git writes (commit, push, reset, checkout, restore, stash, rebase, merge, add, rm, clean, cherry-pick, am, apply).
    - Every role is asked before `cargo update`, `cargo add`, `rustup default`, and `git push` with `-f`, `--force*` or a `+refspec`.
  - `stop_decision(passed, attempts, summary)`: a pass returns `(None, 0)`. A failure with `attempts < 3` returns a block JSON and `attempts + 1`. A failure at 3 returns `None` and prints `ESCALATE: gate task still failing after 3 attempts` to stderr.
  - `claude_md_check` (at most 200 lines) and `removable_build_dir`: strictly under `C:/t/fl/`, case- and separator-insensitive, and no `.` or `..` segments.
  - `run()` for each hook kind:
    - It writes the raw stdin to `target/xtask/hook-probe.json` on every call.
    - Guards fail closed on input that is not JSON, for any role except Lead.
    - `guard-edit` resolves `file_path`/`notebook_path` against `cwd`, normalizes it lexically, makes it relative to the nearest ancestor that contains `.git`, then applies the `--only` globs and the protected check. If `OWNERS.toml` cannot be loaded it denies non-leads.
    - `stop`:
      - It returns immediately when there are no changed files since the gate base.
      - The counter lives in `target/xtask/stop-<sanitized session_id>`. When `stop_hook_active` is false a new streak starts at 0; when it is true the counter is read.
      - It runs `gate::run(Task)` in-process; this is `cargo xtask gate task` with the same base resolution (R33) and `FL_TRACK` fallback. The report goes to stderr and the block reason lists the non-PASS/SKIP lines.
      - ReadOnly roles are never blocked.
    - `worktree-remove` takes `worktree_path`, falling back to `cwd`. It runs `cargo metadata` there, reads `build_directory`, and deletes it only if `removable_build_dir` accepts it.
- `xtask/src/main.rs`: new subcommand `Hook { kind, --role auto|implementer|lead|read-only, --only <glob>… }`. The Gate arm now calls `gate::run`.
- `xtask/src/gate.rs`: `pub fn run(root, level, base, tracks, milestone)`, extracted from main so the stop hook shares it. New row `claude-md` at every level (`ALL`), after `clippy-drift`.
- `xtask/src/owners.rs`: `glob_set` is now `pub` so `--only` reuses the OWNERS glob semantics. This is a one-word change; the file is not in the brief's list but it is under `xtask/**` (contract).
- Config files:
  - `CLAUDE.md` (124 lines; all 11 sections). It also covers:
    - the sccache wrapper requirement (Task 1 minor)
    - the `--features dynamic` line (Task 26 result: works with `bevy_ecs`-only sim crates; windowed run untested)
    - gate-base precedence and BRP ports
  - `.claude/settings.json`: hooks exactly as in the brief.
  - `.claude/rules/{sim,client,content,shaders}.md` with `paths:` frontmatter.
  - `.claude/agents/*.md` (5 agents).
  - `.claude/skills/bevy-api/SKILL.md`. Step 1b points to the local migration guides, which exist at `~/.cargo/registry/src/*/bevy-0.20.0/_release-content/migration-guides/` along with `release-notes/`. BRP facts come from the local `bevy_brp_extras`/`bevy_brp_mcp` 0.23.0 sources (default port 15702, `BRP_EXTRAS_PORT`, `register_agent_tool`). The API reference on this branch has no BRP section yet (Task 27 is pending), so the skill points to it "once Task 27 lands".
  - `.mcp.json` (verbatim), `rust-analyzer.toml` (verbatim values), `.gitattributes` (`* text=auto eol=lf`).

## Additions beyond the brief's interface (please review)

1. **`--only <glob>`** on `guard-edit`. It is the only way to satisfy "test-author edits only acceptance/holdout, which its own hook allows" with three roles. test-author's frontmatter uses `--role lead --only "**/tests/acceptance/**" --only "**/tests/holdout/**"` for edits and `--role implementer` for Bash. Under `auto`, the global hook maps `test-author` to Lead.
2. **`--role read-only`** CLI value. reviewer and perf-analyst use it in their frontmatter Bash guard so that read-only Bash holds even without `agent_type`.
3. ReadOnly roles are never blocked by the stop hook, because they cannot fix a failing gate and blocking them would only burn a builder slot. This depends on `agent_type` or `FL_ROLE` reaching the hook (see concern 2).
4. `hook-probe.json` is written on every hook call, to support Step 5's "log one hook stdin".

## Tests and TDD evidence

**RED:** I wrote the tests with `todo!()` stubs, then ran `cargo test -p xtask hooks`:
```
failures: hooks::tests::claude_md_limit, guard_allows_protected_for_lead, guard_allows_unprotected_for_implementer,
  guard_asks_for_cargo_update, guard_denies_bless_variants, guard_denies_protected_for_implementer,
  read_only_role_never_edits_or_writes_git, role_precedence, stop_blocks_then_escalates
test result: FAILED. 0 passed; 9 failed; 0 ignored; 0 measured; 60 filtered out
```
Each test panicked with "not yet implemented", which is the expected RED.

**GREEN:** `cargo test -p xtask` gives `test result: ok. 72 passed; 0 failed`. That is 12 hooks tests: the 8 from the brief plus `read_only_role_never_edits_or_writes_git`, `only_globs_confine_edits`, `worktree_remove_only_under_build_root` and `deny_answer_is_pretooluse_json`. `cargo clippy -p xtask --all-targets -- -D warnings` is clean.

## Step 5: hook verification by piping hook JSON into the real binary

| Input | Result |
|---|---|
| `agent_type: sim-engineer`, Write `C:\fl\a\crates\fl_view\tests\snapshots\probe.snap`, `--role auto` | deny, "…probe.snap is a protected path (OWNERS.toml [protected])…" |
| No `agent_type`, `--role implementer`, same path | deny (the frontmatter path) |
| No `agent_type`, `--role auto` (lead) | allow |
| Implementer, `crates/fl_sim/src/step.rs` | allow |
| Implementer, `..\..\scenarios\duel.ron` from cwd `crates\fl_sim` | deny (`scenarios/duel.ron`) |
| test-author `--only`, `crates/fl_sim/src/step.rs` | deny, "outside this agent's paths" |
| test-author `--only`, `crates/fl_sim/tests/acceptance/a.rs` | allow |
| `FL_ROLE=read-only`, any edit | deny |
| sim-engineer, Bash `cargo insta accept` | deny |
| Lead, `cargo update -p serde` | ask |

- **`cargo xtask bless --help`** in the lead session runs and prints usage.
- **Stop hook, failing:** I injected `fn  fmt_probe( ) {}` into hooks.rs, then ran `{"session_id":"t","stop_hook_active":false} | cargo xtask hook stop`. stdout was `{"decision":"block","reason":"`cargo xtask gate task` fails (block 1 of 3)… FAIL fmt (2.2s) … gate task: FAIL (fmt) in 249s"}` with exit 0, and the counter `target/xtask/stop-t` was 1.
- **Stop hook, after reverting:** with `stop_hook_active: true`, the gate passed (`gate task: PASS in 87s`), stdout had 0 bytes, exit was 0, and the counter file was removed.
- **Not done:** the live subagent run (dispatching a `sim-engineer` from a lead session rooted in `C:\fl\a`) and capturing a real hook stdin. My session is rooted in another directory, so this repo's `.claude/settings.json` does not load for me, and the implementer instructions forbid dispatching subagents. The lead should run Step 5's first three bullets. The input of the last hook call is then in `target/xtask/hook-probe.json` (check it for `agent_type`).

## Gate: `cargo xtask gate task --track contract` (final, after the last edit)

```
PASS fmt (2.9s)
PASS clippy-drift (7.3s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.6s)
PASS test-guard (1.9s)
PASS clippy (13.4s)
PASS arch-check (52.9s)
PASS test-headless (54.2s)
SKIP test-client: no client package touched since 51254b7 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 135s
```
An earlier run flagged a variable named `skip_value` as three test-guard `ToleranceChanged` false positives. I renamed it to `drop_next`, after which `test-guard` reports 0 findings.

## Files changed

- Created: `CLAUDE.md`, `.claude/settings.json`, `.claude/rules/{sim,client,content,shaders}.md`, `.claude/agents/{sim-engineer,client-engineer,test-author,reviewer,perf-analyst}.md`, `.claude/skills/bevy-api/SKILL.md`, `.mcp.json`, `rust-analyzer.toml`, `.gitattributes`, `xtask/src/hooks.rs`
- Modified: `xtask/src/main.rs`, `xtask/src/gate.rs`, `xtask/src/owners.rs` (a visibility change only)

## Concerns

1. **Hooks can block on the build-dir lock.** Every hook is `cargo xtask …`, which is `cargo run -p xtask`. While a long cargo build holds the worktree's build-dir lock, that hook waits for the lock. A PreToolUse hook that times out is a non-blocking error, so the guard fails open during long builds, and edits stall until the build finishes or the hook times out. A possible fix, which needs a ruling: run a prebuilt copy of the xtask binary (for example `target/xtask/bin/xtask.exe`, refreshed by `gate`) and fall back to `cargo xtask`.
2. **Whether hook input carries `agent_type` is unverified live** (Step 5, not run; see above). The implementer frontmatter hooks use `--role implementer`, so the denials hold either way. The read-only stop-skip and the `auto` mapping for reviewer and perf-analyst do depend on `agent_type`.
3. **The `.gitattributes` renormalize was not done.** I measured it with a temporary index: renormalizing changes only 2 files committed with CRLF. They are `crates/fl_protocol/src/lib.rs` (contract) and `crates/fl_sim/src/build.rs` (sim-core, outside my track; about 240 lines rewritten each). `git status` does not show them as modified now. To fix: run `git add --renormalize crates/fl_protocol/src/lib.rs crates/fl_sim/src/build.rs` in a commit with `--track contract,sim-core`.
4. **Formats I could not check against the docs:**
   - the frontmatter formats `mcpServers: [brp]`, `skills: [bevy-api]` and tools `mcp__brp__*`
   - the `WorktreeRemove` input field `worktree_path` (with a `cwd` fallback)

   I took them from the research doc and the brief. Pre-approving the project MCP server (`enabledMcpjsonServers`) was left to the user.
5. **The Bash guard is substring-based.** A commit message that mentions `cargo insta accept`, or a branch name containing `tests/` used in `git checkout`, will be denied for implementers. This is a false positive and fails safe.
6. **`worktree-remove` was never run live**, to avoid deleting this worktree's build dir. Its path logic is unit-tested.

---

# Fix round 1

**Commits:**
- `cf93ae9 fix(agents): fail-closed hooks, role resolution, workspace from input` (Refs: ADR-0013, ADR-0009)
- `44661fc chore: renormalize line endings`

Both commits carry the Co-Authored-By trailer.

## Changes by finding

### A1: fail-closed hooks (R36)

- `hooks::install_binary` copies the running xtask binary to `target/hooks/xtask(.exe)`. An existing copy is first renamed to `xtask.old`, because Windows allows renaming a running exe but not overwriting it. The copy is skipped when the running binary already is the hook copy.
  - `cargo xtask gate …` calls it before running, as a best-effort step that warns on stderr.
  - The new `cargo xtask install-hooks` calls it and reports errors.
- New `.claude/hooks/xtask-hook.sh` (bash, the hook shell):
  - It `cd`s to `$CLAUDE_PROJECT_DIR` and runs `target/hooks/xtask(.exe) hook "$@"`. It falls back to `cargo xtask hook "$@"` only when the copy is missing.
  - Any failure becomes `exit 2`.
- Every hook command is `bash "$CLAUDE_PROJECT_DIR/.claude/hooks/xtask-hook.sh" … || exit 2`, with `"onFailure": "block"` and explicit timeouts: guards 30 s, Stop/SubagentStop 1800 s. The agent frontmatter hooks are the same, with `timeout: 30` and `onFailure: block`.
- `main`: any `hook` error exits 2. A clap error for a bad kind also exits 2.
- PreToolUse answers are only `hookSpecificOutput.permissionDecision`, with no top-level `decision`. Stop answers use `decision: "block"` and a reason.
- `stop` turns its own errors (base resolution, changed files, counter write) into one block (`stop_error`). When `stop_hook_active` is true it escalates instead, with stderr and a `systemMessage`, so it never loops. Gate-failure escalation after 3 blocks is now also `{"systemMessage":"ESCALATE: …"}`.

### A2: role resolution (R37)

- No `agent_type` means Lead. An unknown `agent_type` means Implementer. `reviewer` and `perf-analyst` are ReadOnly. `test-author` is Lead, confined by its own `--only` hook.
- `--only` never allows `**/snapshots/**`.
- `stop_gates(event, role)`: SubagentStop runs the gate only for Implementer. Stop runs it for every role except ReadOnly.

### A3: workspace from the hook input

- `stop` and `guard-bash` use the repository of `input.cwd`: the nearest ancestor with `.git`, falling back to the binary's workspace.
- `guard-edit` uses the edited file's own repository and loads that repository's `OWNERS.toml`. A repository without one protects nothing; a ReadOnly role is still denied.

### A4: `xtask bless` in any spelling

Within one command segment, a `bless` word that follows any word ending in `xtask` or `xtask.exe` is denied. This catches `cargo run -q -p xtask -- bless`, `--release -- bless` and `…\XTASK.EXE bless`. `cargo xtask gate task; echo bless` is still allowed.

### B: config corrections

- `MultiEdit` is removed from the matchers.
- Agent `tools:` (and the tool lists in their bodies) include `PowerShell`.
- The `WorktreeRemove` hook is removed. `cargo xtask hook worktree-remove --worktree <dir>` is now a manual command, documented in CLAUDE.md, and its `cargo metadata` child clears `CARGO_BUILD_BUILD_DIR` and `CARGO_TARGET_DIR`.
- CLAUDE.md is updated for roles, the hook binary, `install-hooks` and the manual build-dir cleanup. It is 129 lines.

### C: renormalize (R38)

`44661fc` renormalizes only `crates/fl_protocol/src/lib.rs` and `crates/fl_sim/src/build.rs`. `git diff --cached --ignore-all-space | wc -l` printed 0. I re-checked out both working copies, and they are now `w/lf`.

### D: minors

- Protected and `--only` globs are case-insensitive (`owners::glob_set_ci`). Track globs are unchanged.
- `shell_writes` denies non-lead shell writes onto protected paths, except `*.new` files. It covers the write verbs `mv`, `cp`, `move`, `copy`, `Move-Item`/`mi`, `Copy-Item`/`cpi`, `Set-Content`/`sc`, `Add-Content`/`ac`, `Out-File`, `tee` and `Tee-Object`, plus `>` and `>>` redirect targets.
  - Each segment is checked separately; segments are split at `;`, `|`, `&&` and newlines.
  - Every path argument of a write verb counts, so copying *from* a protected path is also denied. This is conservative.
- Read-only roles are also denied `git switch`, `tag`, `worktree`, `config`, `update-ref`, and `git branch` with `-d`/`-D`/`--delete`/`-m`/`-M`/`--move`/`-c`/`-C`/`--copy`/`-f`.
- `hook-probe.json` is written only when `FL_HOOK_PROBE=1`.
- `guard-edit` denies non-leads when the input has no `file_path`/`notebook_path`.

## TDD

- **RED:** I first added the new and changed tests, then ran `cargo test -p xtask hooks`. The tests were:
  - new: `subagent_stop_gates_only_implementers`, `stop_error_blocks_once`, `edit_without_path_is_denied_to_non_leads`, `workspace_comes_from_the_path`, `protected_matching_ignores_case`, `only_never_allows_snapshots`, `xtask_bless_any_spelling`, `shell_writes_onto_protected_paths`, `read_only_denies_more_git_writes`
  - changed: `role_precedence` (R37 cases) and `stop_blocks_then_escalates` (systemMessage)

  The build failed with 4 compile errors (`E0599` `as_deref` on the new `repo_relative` tuple; `E0308` mismatched types). The new API did not exist yet, so this was the expected RED.
- **GREEN:** `cargo test -p xtask` gives `test result: ok. 81 passed; 0 failed`. `cargo clippy -p xtask --all-targets -- -D warnings` is clean.

## Hook checks through the real wrapper and binary

I set `CLAUDE_PROJECT_DIR='C:\fl\a'` and ran `bash "$CLAUDE_PROJECT_DIR/.claude/hooks/xtask-hook.sh" …` from `/tmp`.

**Before `install-hooks` (fallback to `cargo xtask`):** a sim-engineer Write to a snapshot was denied.

**After `cargo xtask install-hooks` (which printed `install-hooks: C:\fl\a\target\hooks\xtask.exe`):**

| Input | Result |
|---|---|
| sim-engineer, snapshot Write | deny |
| `agent_type: general-purpose`, `cargo run -q -p xtask -- bless x` | deny |
| sim-engineer, cwd `crates\fl_view`, `echo x > tests/snapshots/a.snap` | deny (`crates/fl_view/tests/snapshots/a.snap`) |
| No `agent_type`, `cargo xtask bless …` | allow (lead) |
| Bad kind `guard-nope` | exit 2 |
| `worktree-remove --worktree C:\does\not\exist` | exit 2 |
| SubagentStop, reviewer | no output, exit 0 (not gated) |
| Stop with `FL_GATE_BASE=nonexistent-rev`, `stop_hook_active` false | `{"decision":"block","reason":"The stop hook could not check …"}` |
| Same, `stop_hook_active` true | `{"systemMessage":"ESCALATE: the stop hook failed again: …"}`, no block |
| SubagentStop, sim-engineer, cwd `C:\fl\a\crates`, clean tree | gate ran (`gate task: PASS in 46s`), no stdout, exit 0 |

## Gate

`cargo xtask gate task --track contract`, run on the fix content before the renormalize commit:
```
PASS fmt (3.3s)
PASS clippy-drift (5.6s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
PASS test-guard (0.6s)
PASS clippy (30.1s)
PASS arch-check (6.2s)
PASS test-headless (96.3s)
SKIP test-client: no client package touched since 51254b7 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 143s
```
After the renormalize commit I ran `cargo xtask gate task --track contract,sim-core`, because `crates/fl_sim/src/build.rs` is in the sim-core track:
```
PASS fmt (1.4s)
PASS clippy-drift (3.4s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (0.9s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/build.rs:101 NewSeedOverride: app.insert_resource(SimRng::from_seed(config.seed))
PASS clippy (21.3s)
PASS arch-check (4.8s)
PASS test-headless (40.7s)
SKIP test-client: no client package touched since 51254b7 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 74s
```
The test-guard finding is an artifact of the line-ending rewrite: every line of `build.rs` shows as changed, and this line already existed. The diff is whitespace-only (`--ignore-all-space` is empty), so I recommend accepting it.

## Deferred to the first session rooted in the repo (or Task 30)

- The live `sim-engineer` subagent denial.
- A captured real hook stdin with `FL_HOOK_PROBE=1`, to see whether `agent_type` and `hook_event_name` arrive.
- A real check of `onFailure` and frontmatter `timeout`/`onFailure`. I added these as instructed but could not observe the harness's behaviour.

## Remaining notes

- The hook binary refreshes only on a gate run or `install-hooks`. After xtask source changes, hooks run the previous binary until one of those runs. A new worktree must run `cargo xtask install-hooks` once (documented); until then hooks fall back to `cargo xtask`, which can exceed the 30 s guard timeout and then block (fail closed).
- Bash guard false positives (fail-safe):
  - a commit message that mentions a bless command
  - copying *from* a protected path
  - a `git checkout` of a branch name containing `tests/`
