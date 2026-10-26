### Task 4: Agent environment — CLAUDE.md, rules, hooks, role agents, bevy-api skill, `.mcp.json`

**Track:** contract · **Refs:** ADR-0013, ADR-0009

**Files:**
- Create: `CLAUDE.md`, `.claude/settings.json`, `.claude/rules/{sim,client,content,shaders}.md`, `.claude/agents/{sim-engineer,client-engineer,test-author,reviewer,perf-analyst}.md`, `.claude/skills/bevy-api/SKILL.md`, `.mcp.json`, `rust-analyzer.toml`, `xtask/src/hooks.rs`
- Modify: `xtask/src/{main,gate}.rs` (subcommand `hook`, gate row `claude-md`)

**Interfaces:**
- Produces:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Role { Lead, Implementer, ReadOnly }
pub fn role_from(input: &serde_json::Value, flag: Option<&str>) -> Role;
//   precedence: --role implementer|lead > env FL_ROLE > input["agent_type"]
//   (sim-engineer|client-engineer → Implementer; reviewer|perf-analyst → ReadOnly) > Lead
pub enum Decision { Allow, Deny(String), Ask(String) }
pub fn guard_edit(owners: &Owners, role: Role, path: &Path) -> Decision;   // protected path + Implementer → Deny
pub fn guard_bash(role: Role, command: &str) -> Decision;
pub fn stop_decision(gate_passed: bool, attempts: u32, summary: &str) -> (Option<serde_json::Value>, u32);
//   failing and attempts < 3 → Some({"decision":"block","reason": summary}), attempts + 1;
//   failing at 3 → None plus stderr "ESCALATE: gate task still failing after 3 attempts"
```
- CLI: `cargo xtask hook <stop|guard-edit|guard-bash|worktree-remove> [--role auto|implementer|lead]` reads the hook JSON from stdin and writes hook JSON to stdout:
  - Guards answer with `hookSpecificOutput.permissionDecision` set to `deny` or `ask` and a reason.
  - **`stop`** returns immediately when `changed_files` is empty. It keeps a per-session attempt counter in `target/xtask/stop-<session_id>` and respects `stop_hook_active`.
  - **`worktree-remove`** deletes the worktree's build dir, read from `cargo metadata` (`build_directory`, Task 1 Step 1), and only if it lies under `C:/t/fl/`.
- **Bash rules:**
  - Denied for `Implementer`: commands containing `cargo insta accept`, `cargo insta review`, `INSTA_FORCE_PASS`, `INSTA_UPDATE=always`, `--bless`, `xtask bless` or `git checkout -- tests/`.
  - `Ask` for every role: `cargo update`, `cargo add`, `rustup default`, `git push --force`, `git push -f`.
- **`.claude/settings.json`:**
  - `Stop` and `SubagentStop` run `cargo xtask hook stop` (timeout 900 s).
  - `PreToolUse` with matcher `Edit|Write|MultiEdit|NotebookEdit` runs `cargo xtask hook guard-edit --role auto`.
  - `PreToolUse` with matcher `Bash|PowerShell` runs `cargo xtask hook guard-bash --role auto`.
  - `WorktreeRemove` runs `cargo xtask hook worktree-remove`.
  - The implementer agents' frontmatter repeats both `PreToolUse` hooks with `--role implementer`. That way the denial holds even when the hook input carries no agent identity.

**Content decisions:**
- **`CLAUDE.md`** (≤ 200 lines, checked by the gate row `claude-md` at every level). Sections:
  1. what Frontline is (two lines)
  2. where the spec, ADRs, plans, research and `docs/debugging.md` live
  3. commands: `cargo xtask gate task --track <t>`, `cargo nextest run --profile agent -p <crate>`, `cargo run -p fl_cli -- data-check assets/base`, `cargo run -p fl_game`
  4. toolchain pin and the nightly rule
  5. worktree rules (edit only under `C:\fl\`, ≤ 2 builders, the build-dir rule from Task 1, long builds in the background)
  6. determinism and dependency rules as a 10-line summary that points to spec §5.1/§5.4
  7. protected paths, `bless` and the `BLOCKED:` escape hatch
  8. contract paths and the ADR rule
  9. observability rules
  10. "resolve Bevy APIs with the `bevy-api` skill; never from memory"
  11. the commit trailer
- **Rules** (frontmatter `paths:`):
  - `sim.md` (`crates/fl_sim/**`, `crates/fl_view/**`, `crates/fl_protocol/**`, `features/*/sim/**`): determinism rules, classification and the hook sub-sets.
  - `client.md` (`crates/fl_client/**`, `crates/fl_game/**`, `features/*/client/**`): no sim access, commands via `LocalDriver`, and `Option<Single<&Window>>`.
  - `content.md` (`assets/**`, `crates/fl_content/**`): strictness and never copying Nival.
  - `shaders.md` (`**/*.wesl`): WESL only.
- **Agents.** Each names its tools, the skill `bevy-api`, hooks and MCP servers.
  - `sim-engineer`: Read, Edit, Write, Bash, Grep, Glob, LSP; implementer hooks.
  - `client-engineer`: the same, plus `mcp__brp__*`.
  - `test-author`: "active from M2 (C3)". Edits only `**/tests/acceptance/**` and `**/tests/holdout/**`, which its own hook allows.
  - `reviewer`: Read, Grep, Glob, and Bash for read-only commands. Its body carries the determinism checklist (§5.4 bullets, classification, ordering, the `(player, seq)` path, no client→sim edges, contract changes cite an ADR, test-guard findings accepted one by one).
  - `perf-analyst`: Read, Grep, Glob, Bash; never edits.
- **`bevy-api` skill.** Resolution order:
  1. Local sources: `~/.cargo/registry/src/*/bevy*-0.20.0/` (`src/`, any shipped `examples/`, and doc comments; grep with `rg -n`)
  2. `docs/research/bevy020-api-reference.md` (including its migration notes)
  3. A compile probe in `probes/`

  Record new facts in the API reference, and never answer from memory. If a local copy of the Bevy migration guides is found during this task, add its path as step 1b.
- **`.mcp.json`:** `{"mcpServers":{"brp":{"command":"bevy_brp_mcp","args":[]}}}`. Ports are per agent, via `--remote-port` or `BRP_EXTRAS_PORT`.
- **`rust-analyzer.toml`:** `[cargo] targetDir = true` (rust-analyzer gets its own subdirectory and never holds the build lock) and `[check] command = "clippy"`.

- [ ] **Step 1: Write the failing tests** (`xtask/src/hooks.rs`):
```rust
#[test] fn role_precedence()                          // flag > FL_ROLE > agent_type > Lead
#[test] fn guard_denies_protected_for_implementer()   // crates/fl_sim/tests/snapshots/a.snap → Deny(reason contains "protected path")
#[test] fn guard_allows_protected_for_lead()
#[test] fn guard_allows_unprotected_for_implementer() // crates/fl_sim/src/step.rs → Allow
#[test] fn guard_denies_bless_variants()              // "cargo insta accept", "INSTA_UPDATE=always cargo test", "cargo xtask bless x --reason y",
                                                      //   "git checkout -- tests/" → Deny for Implementer; Allow for Lead
#[test] fn guard_asks_for_cargo_update()              // "cargo update -p serde" → Ask for every role
#[test] fn stop_blocks_then_escalates()               // attempts 0,1,2 failing → block JSON; attempt 3 → None
#[test] fn claude_md_limit()                          // a 201-line file → Fail; 200 → Pass
```
- [ ] **Step 2: Run** `cargo test -p xtask hooks`. Expected: FAIL.
- [ ] **Step 3: Implement** the hooks module and gate row. Write the config files.
- [ ] **Step 4: Run** `cargo test -p xtask`. Expected: PASS.
- [ ] **Step 5: Verify the live hooks** (lead session).
  - Dispatch a `sim-engineer` subagent to Write `crates/fl_view/tests/snapshots/probe.snap`. Expected: denied with the protected-path reason.
  - From the same subagent, `cargo insta accept`. Expected: denied.
  - Log one hook stdin to `target/xtask/hook-probe.json` and record whether it carries `agent_type`.
  - In the lead session, `cargo xtask bless --help` runs.
  - `echo '{"session_id":"t","stop_hook_active":false}' | cargo xtask hook stop` with a deliberate fmt error in a scratch file prints a block decision. After reverting, it prints nothing and exits 0.
- [ ] **Step 6: Run** `cargo xtask gate task --track contract`. Expected: `PASS claude-md` and `gate task: PASS`.
- [ ] **Step 7: Commit** `chore(agents): CLAUDE.md, path rules, gate and guard hooks, role agents, bevy-api skill, .mcp.json` with trailer `Refs: ADR-0013, ADR-0009`. The controller then runs `cargo xtask gate pr`, fast-forwards `main`, and does the T12 rebase/gate/merge (Completed work).

---

