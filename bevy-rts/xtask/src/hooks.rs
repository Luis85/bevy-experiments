//! `cargo xtask hook <kind>`: the Claude Code hooks in `.claude/settings.json` and the role agents' frontmatter
//! (spec §12.5, §13.3; ADR-0009, ADR-0013). Each reads the hook JSON from stdin and answers on stdout.
//!
//! - `guard-edit` (PreToolUse on Edit/Write/NotebookEdit): implementers never edit protected paths; read-only roles
//!   never edit; `--only <glob>` confines an agent to the given paths (never to `**/snapshots/**`).
//! - `guard-bash` (PreToolUse on Bash/PowerShell): implementers never bless or write protected paths from the
//!   shell; read-only roles never change the repository; everyone is asked before dependency, toolchain and
//!   force-push changes.
//! - `stop` (Stop/SubagentStop): blocks the stop while `gate task` fails, at most [`STOP_ATTEMPTS`] times in a row,
//!   then escalates.
//! - `worktree-remove` (manual, `--worktree <dir>`): deletes a worktree's build dir under [`BUILD_ROOT`].
//!
//! Hooks run the copy of this binary in `target/hooks/` (ruling R36), so they never wait on the build lock;
//! `.claude/hooks/xtask-hook.sh` turns every failure into exit 2 (fail closed). Internal errors exit 2 as well.
//! The workspace is the repository of the hook input's `cwd` or edited file, not the binary's own.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::changed;
use crate::gate::{self, Level, Outcome};
use crate::owners::{self, Owners};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Stop,
    GuardEdit,
    GuardBash,
    WorktreeRemove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Lead,
    Implementer,
    ReadOnly,
}

/// The environment variable that names the session's role (`implementer`, `lead` or `read-only`).
pub const ROLE_ENV: &str = "FL_ROLE";
/// Set to `1` to keep each hook's raw input in `target/xtask/hook-probe.json`.
pub const PROBE_ENV: &str = "FL_HOOK_PROBE";

fn parse_role(s: &str) -> Option<Role> {
    match s.trim() {
        "implementer" => Some(Role::Implementer),
        "lead" => Some(Role::Lead),
        "read-only" | "readonly" => Some(Role::ReadOnly),
        _ => None,
    }
}

/// The role of a subagent type (ruling R37); a plugin prefix (`x:sim-engineer`) is ignored. Unknown subagents are
/// implementers; `test-author` counts as lead because its own `--only` hook confines it.
fn agent_role(agent_type: &str) -> Role {
    match agent_type.rsplit(':').next().unwrap_or(agent_type) {
        "reviewer" | "perf-analyst" => Role::ReadOnly,
        "test-author" => Role::Lead,
        _ => Role::Implementer,
    }
}

/// The hook's role: `--role implementer|lead|read-only` > env `FL_ROLE` > the input's `agent_type` (a subagent)
/// > `Lead` (the main session).
pub fn role_from(input: &Value, flag: Option<&str>) -> Role {
    role_resolve(input, flag, std::env::var(ROLE_ENV).ok().as_deref())
}

/// [`role_from`] with the environment value passed in. `auto` and unknown values fall through.
pub fn role_resolve(input: &Value, flag: Option<&str>, env: Option<&str>) -> Role {
    flag.and_then(parse_role)
        .or_else(|| env.and_then(parse_role))
        .or_else(|| {
            input["agent_type"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .map(agent_role)
        })
        .unwrap_or(Role::Lead)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
    Ask(String),
}

impl Decision {
    /// The PreToolUse answer; `None` for `Allow` (the normal permission flow applies).
    pub fn to_hook_json(&self) -> Option<Value> {
        let (decision, reason) = match self {
            Decision::Allow => return None,
            Decision::Deny(r) => ("deny", r),
            Decision::Ask(r) => ("ask", r),
        };
        Some(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": decision,
                "permissionDecisionReason": reason,
            }
        }))
    }

    fn is_deny(&self) -> bool {
        matches!(self, Decision::Deny(_))
    }
}

const BLOCKED_HINT: &str = "If you believe a protected test is wrong, write `BLOCKED: test X contradicts spec Y \
                            because …` and stop.";

fn is_pending(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("new"))
}

/// Whether `role` may edit `path` (repo-relative): read-only roles never edit; implementers never touch
/// `[protected]` paths (case-insensitive; pending `*.new` files excepted, as in the gate's `protected` row).
pub fn guard_edit(owners: &Owners, role: Role, path: &Path) -> Decision {
    let shown = owners::normalize(path);
    if role == Role::ReadOnly {
        return Decision::Deny(format!(
            "{shown}: this role is read-only and never edits files; report findings instead"
        ));
    }
    if role == Role::Implementer && owners.is_protected(path) && !is_pending(path) {
        return Decision::Deny(format!(
            "{shown} is a protected path (OWNERS.toml [protected]); implementers never change it. Tests and \
             tools write `*.new` files and the lead promotes them with `cargo xtask bless`. {BLOCKED_HINT}"
        ));
    }
    Decision::Allow
}

/// Paths that `--only` never opens up: snapshots are blessed, never written by hand (ruling R37).
const NEVER_ONLY: &str = "**/snapshots/**";

/// `--only` confinement: deny `path` (repo-relative, or `None` outside any repository) unless a glob matches it
/// and it is not a snapshot.
pub fn guard_only(globs: &[String], path: Option<&Path>) -> Result<Decision> {
    if globs.is_empty() {
        return Ok(Decision::Allow);
    }
    let set = owners::glob_set_ci(globs)?;
    let snapshots = owners::glob_set_ci(&[NEVER_ONLY.to_owned()])?;
    let allowed = path.is_some_and(|p| {
        let p = owners::normalize(p);
        set.is_match(Path::new(&p)) && !snapshots.is_match(Path::new(&p))
    });
    Ok(if allowed {
        Decision::Allow
    } else {
        Decision::Deny(format!(
            "{} is outside this agent's paths ({}; never {NEVER_ONLY}); edit only those",
            path.map_or_else(|| "this path".to_owned(), owners::normalize),
            globs.join(", ")
        ))
    })
}

/// One shell command segment's words: quotes dropped, `\` as `/`.
fn words(segment: &str) -> Vec<String> {
    segment
        .chars()
        .filter(|c| !matches!(c, '"' | '\''))
        .map(|c| if c == '\\' { '/' } else { c })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The command split at `;`, `|`, `||`, `&&` and newlines.
fn segments(command: &str) -> Vec<String> {
    command
        .replace("&&", ";")
        .split([';', '|', '\n'])
        .map(str::to_owned)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// The command with quotes dropped, `\` as `/`, `=` unspaced, `cargo-insta` as `cargo insta`, toolchain
/// overrides (`cargo +nightly`) and `git -C <dir>`/`git -c <k=v>` removed and whitespace collapsed: the form the
/// rules match against.
fn normalize_command(command: &str) -> Vec<String> {
    let text = command
        .replace("cargo-insta.exe", "cargo insta")
        .replace("cargo-insta", "cargo insta");
    let mut tokens: Vec<String> = Vec::new();
    let mut drop_next = false;
    for tok in words(&text) {
        if drop_next {
            drop_next = false;
            continue;
        }
        let prev = tokens.last().map(|t| t.to_ascii_lowercase());
        let prev = prev.as_deref().unwrap_or("");
        if prev.ends_with("cargo") && tok.starts_with('+') {
            continue;
        }
        // `git -C <dir>` and `git -c <key=value>` sit between `git` and its verb.
        if prev.ends_with("git") && (tok == "-C" || tok == "-c") {
            drop_next = true;
            continue;
        }
        tokens.push(tok);
    }
    // `A = b`, `A =b` and `A= b` become `A=b`.
    let mut joined: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "=" && !joined.is_empty() && i + 1 < tokens.len() {
            let prev = joined.pop().unwrap_or_default();
            joined.push(format!("{prev}={}", tokens[i + 1]));
            i += 2;
        } else if t.starts_with('=') && t.len() > 1 && !joined.is_empty() {
            let prev = joined.pop().unwrap_or_default();
            joined.push(format!("{prev}{t}"));
            i += 1;
        } else if t.ends_with('=') && t.len() > 1 && i + 1 < tokens.len() {
            joined.push(format!("{t}{}", tokens[i + 1]));
            i += 2;
        } else {
            joined.push(t.clone());
            i += 1;
        }
    }
    joined
}

/// Whether `phrase` occurs in the normalized command with word boundaries on both sides.
fn has(norm: &str, phrase: &str) -> bool {
    let boundary =
        |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
    norm.match_indices(phrase).any(|(i, _)| {
        boundary(norm[..i].chars().next_back()) && boundary(norm[i + phrase.len()..].chars().next())
    })
}

/// `xtask … bless` in any spelling (`cargo xtask bless`, `cargo run -p xtask -- bless`, `…/xtask.exe bless`):
/// a `bless` word after a word naming xtask, within one command segment.
fn is_xtask_bless(command: &str) -> bool {
    segments(command).iter().any(|seg| {
        let w: Vec<String> = words(seg).iter().map(|t| t.to_ascii_lowercase()).collect();
        w.iter()
            .position(|t| t.ends_with("xtask") || t.ends_with("xtask.exe"))
            .is_some_and(|x| w[x + 1..].iter().any(|t| t == "bless"))
    })
}

/// Bless variants: they promote or rewrite protected artifacts.
fn is_bless(command: &str, norm: &str, tokens: &[String]) -> bool {
    let insta_accepts = has(norm, "cargo insta")
        && tokens.iter().any(|t| {
            matches!(t.as_str(), "accept" | "review" | "reject")
                || t.starts_with("--accept")
                || t == "--review"
        });
    let restores_tests = (has(norm, "git checkout") || has(norm, "git restore"))
        && ["tests/", "snapshots/", "scenarios/"]
            .iter()
            .any(|d| norm.to_ascii_lowercase().contains(d));
    insta_accepts
        || restores_tests
        || norm.contains("INSTA_FORCE_PASS")
        || has(norm, "INSTA_UPDATE=always")
        || has(norm, "INSTA_UPDATE=unseen")
        || has(norm, "--bless")
        || is_xtask_bless(command)
}

/// Git verbs that change the repository or its config; read-only roles never run them.
const GIT_WRITES: [&str; 19] = [
    "git commit",
    "git push",
    "git reset",
    "git checkout",
    "git switch",
    "git restore",
    "git stash",
    "git rebase",
    "git merge",
    "git add",
    "git rm",
    "git clean",
    "git cherry-pick",
    "git am",
    "git apply",
    "git tag",
    "git worktree",
    "git config",
    "git update-ref",
];

/// `git branch` options that delete, rename, copy or force-move a branch.
const BRANCH_WRITES: [&str; 9] = [
    "-d", "-D", "--delete", "-m", "-M", "--move", "-c", "-C", "--copy",
];

/// Whether `role` may run `command`: bless variants are denied to every role but the lead, git writes to
/// read-only roles; dependency, toolchain and force-push changes ask every role. Shell writes onto protected
/// paths are [`shell_writes`].
pub fn guard_bash(role: Role, command: &str) -> Decision {
    let tokens = normalize_command(command);
    let norm = tokens.join(" ");
    if role != Role::Lead && is_bless(command, &norm, &tokens) {
        return Decision::Deny(format!(
            "`{}` promotes or rewrites protected test artifacts; only the lead does that, with `cargo xtask bless \
             <paths> --reason …`. {BLOCKED_HINT}",
            command.trim()
        ));
    }
    if role == Role::ReadOnly {
        let branch_write = has(&norm, "git branch")
            && tokens
                .iter()
                .any(|t| BRANCH_WRITES.contains(&t.as_str()) || t == "-f" || t == "--force");
        let verb = GIT_WRITES
            .iter()
            .copied()
            .find(|v| has(&norm, v))
            .or(branch_write.then_some("git branch"));
        if let Some(verb) = verb {
            return Decision::Deny(format!(
                "`{verb}` changes the repository; this role is read-only. Report findings instead"
            ));
        }
    }
    let force_push = has(&norm, "git push")
        && tokens
            .iter()
            .any(|t| t == "-f" || t.starts_with("--force") || (t.starts_with('+') && t.len() > 1));
    let asked = ["cargo update", "cargo add", "rustup default"]
        .into_iter()
        .find(|p| has(&norm, p))
        .or(force_push.then_some("git push --force"));
    match asked {
        Some(what) => Decision::Ask(format!(
            "`{what}` changes dependencies, the toolchain or published history; confirm it is in the task's scope"
        )),
        None => Decision::Allow,
    }
}

/// Shell commands that move, copy or write files (case-insensitive).
const WRITE_VERBS: [&str; 15] = [
    "mv",
    "cp",
    "move",
    "copy",
    "move-item",
    "copy-item",
    "mi",
    "cpi",
    "set-content",
    "sc",
    "add-content",
    "ac",
    "out-file",
    "tee",
    "tee-object",
];

/// Redirect targets that are not files.
fn is_null_target(t: &str) -> bool {
    t.is_empty()
        || t.starts_with('&')
        || t.starts_with('$')
        || t.eq_ignore_ascii_case("/dev/null")
        || t.eq_ignore_ascii_case("nul")
}

/// Denies roles other than the lead a shell write (`mv`, `cp`, `Copy-Item`, `Set-Content`, `Out-File`, `>`, `>>`,
/// …) whose argument is a protected path (pending `*.new` files excepted). `rel` maps a word to a repo-relative
/// path. Every path argument of a write verb counts, so copying *from* a protected path is denied too.
pub fn shell_writes(
    owners: &Owners,
    role: Role,
    command: &str,
    rel: &dyn Fn(&str) -> Option<PathBuf>,
) -> Decision {
    if role == Role::Lead {
        return Decision::Allow;
    }
    for seg in segments(command) {
        let w = words(&seg);
        let mut targets: Vec<String> = Vec::new();
        let verb = w.iter().position(|t| {
            let t = t.trim_start_matches('&').to_ascii_lowercase();
            WRITE_VERBS.contains(&t.as_str())
        });
        if let Some(v) = verb {
            for t in &w[v + 1..] {
                if !t.starts_with('-') {
                    targets.push(t.clone());
                } else if let Some((_, value)) = t.split_once(':') {
                    // `-Path:<file>`
                    targets.push(value.to_owned());
                }
            }
        }
        for (i, t) in w.iter().enumerate() {
            if let Some(pos) = t.rfind('>') {
                let after = &t[pos + 1..];
                let target = if after.is_empty() {
                    w.get(i + 1).cloned().unwrap_or_default()
                } else {
                    after.to_owned()
                };
                if !is_null_target(&target) {
                    targets.push(target);
                }
            }
        }
        for t in targets {
            let Some(path) = rel(&t) else { continue };
            if owners.is_protected(&path) && !is_pending(&path) {
                return Decision::Deny(format!(
                    "`{}` writes or moves the protected path {} (OWNERS.toml [protected]); only the lead changes \
                     it, with `cargo xtask bless`. {BLOCKED_HINT}",
                    seg.trim(),
                    owners::normalize(&path)
                ));
            }
        }
    }
    Decision::Allow
}

/// Blocks per failing streak before the stop hook gives up and escalates.
pub const STOP_ATTEMPTS: u32 = 3;

/// The stop hook's answer and the next attempt count: a passing gate resets the count; a failing one blocks the
/// stop (with `summary` as the reason) until [`STOP_ATTEMPTS`] blocks, then lets the agent stop and escalates
/// (stderr and a `systemMessage`).
pub fn stop_decision(gate_passed: bool, attempts: u32, summary: &str) -> (Option<Value>, u32) {
    if gate_passed {
        return (None, 0);
    }
    if attempts < STOP_ATTEMPTS {
        return (
            Some(json!({ "decision": "block", "reason": summary })),
            attempts + 1,
        );
    }
    let msg = format!("ESCALATE: gate task still failing after {STOP_ATTEMPTS} attempts");
    eprintln!("{msg}");
    (Some(json!({ "systemMessage": msg })), attempts)
}

/// Whether the stop hook runs the gate: SubagentStop only for implementers (ruling R37); Stop for every role
/// but read-only ones, which cannot fix a failing gate.
pub fn stop_gates(event: &str, role: Role) -> bool {
    match event {
        "SubagentStop" => role == Role::Implementer,
        _ => role != Role::ReadOnly,
    }
}

/// The stop hook's answer when it fails itself: one block, then (with `stop_hook_active`) an escalation, so it
/// never loops.
pub fn stop_error(e: &anyhow::Error, stop_hook_active: bool) -> Value {
    if stop_hook_active {
        let msg = format!("ESCALATE: the stop hook failed again: {e:#}");
        eprintln!("{msg}");
        json!({ "systemMessage": msg })
    } else {
        json!({
            "decision": "block",
            "reason": format!(
                "The stop hook could not check `cargo xtask gate task`: {e:#}. Run the gate yourself and fix what it \
                 reports, or report BLOCKED."
            ),
        })
    }
}

pub const CLAUDE_MD: &str = "CLAUDE.md";
pub const CLAUDE_MD_MAX_LINES: usize = 200;

/// The `claude-md` gate row: at most [`CLAUDE_MD_MAX_LINES`] lines (ADR-0013).
pub fn claude_md_check(text: &str) -> Outcome {
    let n = text.lines().count();
    if n <= CLAUDE_MD_MAX_LINES {
        Outcome::Pass
    } else {
        Outcome::Fail(format!(
            "{CLAUDE_MD} has {n} lines; the limit is {CLAUDE_MD_MAX_LINES} (ADR-0013). Move detail into \
             .claude/rules/ or docs/"
        ))
    }
}

/// Build dirs live under this root (`build.build-dir` in `.cargo/config.toml`); nothing else is ever deleted.
pub const BUILD_ROOT: &str = "C:/t/fl/";

/// The build dir to delete for a removed worktree: `build_dir` only if it lies strictly under [`BUILD_ROOT`]
/// (case-insensitive, either separator) and has no `.`/`..` segment.
pub fn removable_build_dir(build_dir: &str) -> Option<PathBuf> {
    let norm = build_dir.trim().replace('\\', "/");
    let rest = norm
        .to_ascii_lowercase()
        .strip_prefix(&BUILD_ROOT.to_ascii_lowercase())?
        .to_owned();
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() || segments.iter().any(|s| *s == "." || *s == "..") {
        return None;
    }
    Some(PathBuf::from(norm))
}

// ---- the hook binary ---------------------------------------------------------------------------------------------

/// Where hooks find this binary (ruling R36).
pub fn hook_binary(root: &Path) -> PathBuf {
    root.join("target")
        .join("hooks")
        .join(format!("xtask{}", std::env::consts::EXE_SUFFIX))
}

/// Copies the running xtask binary to [`hook_binary`]; `None` when it already is that binary. A running copy is
/// renamed aside first (Windows allows renaming, not overwriting, a running executable).
pub fn install_binary(root: &Path) -> Result<Option<PathBuf>> {
    let src = std::env::current_exe().context("locate the running xtask binary")?;
    let dst = hook_binary(root);
    if dst.exists() && src.canonicalize().ok() == dst.canonicalize().ok() {
        return Ok(None);
    }
    let dir = dst.parent().context("hook binary has no parent")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    if dst.exists() {
        let old = dst.with_extension("old");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(&dst, &old);
    }
    std::fs::copy(&src, &dst)
        .with_context(|| format!("copy {} to {}", src.display(), dst.display()))?;
    Ok(Some(dst))
}

// ---- running a hook ----------------------------------------------------------------------------------------------

/// Reads the hook JSON from stdin (unless `worktree` is given for `worktree-remove`) and answers on stdout.
/// `root` is the fallback workspace when the input names none. With `FL_HOOK_PROBE=1` the raw input is kept in
/// `target/xtask/hook-probe.json`.
pub fn run(
    root: &Path,
    kind: Kind,
    role_flag: &str,
    only: &[String],
    worktree: Option<&Path>,
) -> Result<()> {
    if kind == Kind::WorktreeRemove
        && let Some(dir) = worktree
    {
        return run_worktree_remove(dir);
    }
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("read hook input from stdin")?;
    if std::env::var(PROBE_ENV).is_ok_and(|v| v == "1") {
        let dir = root.join("target").join("xtask");
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join("hook-probe.json"), &raw);
        }
    }
    let input: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let role = role_from(&input, Some(role_flag));
    let guard = matches!(kind, Kind::GuardEdit | Kind::GuardBash);
    if guard && input.is_null() && role != Role::Lead {
        // Fail closed: without the input the guard cannot tell what is being changed.
        let deny = Decision::Deny(
            "the hook input is not valid JSON; the guard cannot check this call".into(),
        );
        print_answer(deny.to_hook_json());
        return Ok(());
    }
    let answer = match kind {
        Kind::GuardEdit => run_guard_edit(root, &input, role, only).to_hook_json(),
        Kind::GuardBash => run_guard_bash(root, &input, role).to_hook_json(),
        Kind::Stop => {
            let event = input["hook_event_name"].as_str().unwrap_or("Stop");
            if stop_gates(event, role) {
                let active = input["stop_hook_active"].as_bool().unwrap_or(false);
                run_stop(root, &input).unwrap_or_else(|e| Some(stop_error(&e, active)))
            } else {
                None
            }
        }
        Kind::WorktreeRemove => {
            let Some(dir) = input["worktree_path"]
                .as_str()
                .or_else(|| input["cwd"].as_str())
            else {
                bail!("hook input has neither `worktree_path` nor `cwd`");
            };
            run_worktree_remove(Path::new(dir))?;
            None
        }
    };
    print_answer(answer);
    Ok(())
}

fn print_answer(answer: Option<Value>) {
    if let Some(answer) = answer {
        println!("{answer}");
    }
}

/// The input's `cwd`, else `fallback`.
fn input_cwd(input: &Value, fallback: &Path) -> PathBuf {
    input["cwd"]
        .as_str()
        .filter(|c| !c.is_empty())
        .map_or_else(|| fallback.to_path_buf(), PathBuf::from)
}

/// `OWNERS.toml` of the repository at `repo`; a repository without one protects nothing.
fn owners_of(repo: &Path) -> Result<Owners> {
    if repo.join(owners::OWNERS_PATH).is_file() {
        owners::load_owners(repo)
    } else {
        owners::parse_owners("track = []\n[protected]\npaths = []\n")
    }
}

fn run_guard_edit(root: &Path, input: &Value, role: Role, only: &[String]) -> Decision {
    let tool_input = &input["tool_input"];
    let Some(raw) = tool_input["file_path"]
        .as_str()
        .or_else(|| tool_input["notebook_path"].as_str())
        .filter(|p| !p.is_empty())
    else {
        return if role == Role::Lead {
            Decision::Allow
        } else {
            Decision::Deny(
                "the hook input names no file path; the guard cannot check this edit".into(),
            )
        };
    };
    let abs = lexical_normalize(&input_cwd(input, root).join(raw));
    let located = repo_relative(&abs);
    if let Err(d) = guard_only(only, located.as_ref().map(|(_, rel)| rel.as_path()))
        .map_err(|e| Decision::Deny(format!("bad --only glob: {e:#}")))
        .and_then(|d| if d.is_deny() { Err(d) } else { Ok(()) })
    {
        return d;
    }
    // The edited file's own repository decides what is protected; outside any repository nothing is, but a
    // read-only role still never edits.
    let (repo, rel) = located.unwrap_or_else(|| (root.to_path_buf(), abs.clone()));
    match owners_of(&repo) {
        Ok(owners) => guard_edit(&owners, role, &rel),
        Err(_) if role == Role::Lead => Decision::Allow,
        Err(e) => Decision::Deny(format!(
            "cannot check protected paths ({e:#}); fix OWNERS.toml first"
        )),
    }
}

fn run_guard_bash(root: &Path, input: &Value, role: Role) -> Decision {
    let command = input["tool_input"]["command"].as_str().unwrap_or_default();
    let policy = guard_bash(role, command);
    if policy.is_deny() || role == Role::Lead {
        return policy;
    }
    let cwd = input_cwd(input, root);
    let repo = repo_root(&cwd).unwrap_or_else(|| root.to_path_buf());
    let owners = match owners_of(&repo) {
        Ok(o) => o,
        Err(e) => {
            return Decision::Deny(format!(
                "cannot check protected paths ({e:#}); fix OWNERS.toml first"
            ));
        }
    };
    let rel = |word: &str| -> Option<PathBuf> {
        let abs = lexical_normalize(&cwd.join(word));
        Some(repo_relative(&abs).map_or(abs, |(_, rel)| rel))
    };
    match shell_writes(&owners, role, command, &rel) {
        Decision::Allow => policy,
        deny => deny,
    }
}

/// `path` with `.` and `..` resolved lexically.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The policy workspace containing `dir`: prefer the nearest OWNERS.toml to the
/// enclosing Git root, so hooks also work inside a monorepo.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join("OWNERS.toml").is_file() || d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The git worktree containing the file `path`, and `path` relative to it.
fn repo_relative(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let root = repo_root(path.parent()?)?;
    let rel = path.strip_prefix(&root).ok()?.to_path_buf();
    Some((root, rel))
}

fn counter_path(root: &Path, session: &str) -> PathBuf {
    let safe: String = session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    root.join("target")
        .join("xtask")
        .join(format!("stop-{safe}"))
}

/// The stop hook for the workspace of the input's `cwd`: nothing to check when nothing changed since the gate
/// base; otherwise `gate task`. The attempt counter in `target/xtask/stop-<session_id>` counts consecutive blocks;
/// a stop that no stop hook forced (`stop_hook_active` false) starts a new streak.
fn run_stop(fallback: &Path, input: &Value) -> Result<Option<Value>> {
    let cwd = input_cwd(input, fallback);
    let root = repo_root(&cwd).unwrap_or_else(|| fallback.to_path_buf());
    let session = input["session_id"].as_str().unwrap_or("unknown");
    let counter = counter_path(&root, session);
    let base = changed::resolve_base(&root, None)?;
    if changed::changed_files(&root, &base.rev)?.is_empty() {
        let _ = std::fs::remove_file(&counter);
        return Ok(None);
    }
    let attempts = if input["stop_hook_active"].as_bool().unwrap_or(false) {
        std::fs::read_to_string(&counter)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    let report = gate::run(root.clone(), Level::Task, None, Vec::new(), None);
    let rendered = report.render();
    eprint!("{rendered}");
    let summary = format!(
        "`cargo xtask gate task` fails (block {} of {STOP_ATTEMPTS}). Fix it, then finish again. {BLOCKED_HINT}\n{}",
        attempts + 1,
        rendered
            .lines()
            .filter(|l| !l.starts_with("PASS ") && !l.starts_with("SKIP "))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let (answer, next) = stop_decision(report.passed(), attempts, &summary);
    if next == 0 {
        let _ = std::fs::remove_file(&counter);
    } else {
        if let Some(dir) = counter.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        std::fs::write(&counter, next.to_string())
            .with_context(|| format!("write {}", counter.display()))?;
    }
    Ok(answer)
}

/// Deletes the worktree's build dir (`build_directory` from `cargo metadata` run there, with any inherited
/// build-dir or target-dir override cleared) when it lies under [`BUILD_ROOT`].
fn run_worktree_remove(worktree: &Path) -> Result<()> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(worktree)
        .env_remove("CARGO_BUILD_BUILD_DIR")
        .env_remove("CARGO_TARGET_DIR")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run cargo metadata in {}", worktree.display()))?;
    if !out.status.success() {
        bail!(
            "cargo metadata in {} failed: {}",
            worktree.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: Value = serde_json::from_slice(&out.stdout).context("parse cargo metadata")?;
    let Some(build_dir) = meta["build_directory"].as_str() else {
        eprintln!("worktree-remove: cargo metadata reports no build_directory; nothing deleted");
        return Ok(());
    };
    match removable_build_dir(build_dir) {
        Some(dir) if dir.exists() => {
            std::fs::remove_dir_all(&dir).with_context(|| format!("delete {}", dir.display()))?;
            eprintln!("worktree-remove: deleted {}", dir.display());
        }
        Some(dir) => eprintln!("worktree-remove: {} does not exist", dir.display()),
        None => {
            eprintln!("worktree-remove: {build_dir} is not under {BUILD_ROOT}; nothing deleted")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owners::parse_owners;
    use serde_json::json;

    fn owners() -> Owners {
        parse_owners(include_str!("../../OWNERS.toml")).expect("OWNERS.toml parses")
    }

    #[test]
    fn role_precedence() {
        let sim = json!({ "agent_type": "sim-engineer" });
        let reviewer = json!({ "agent_type": "reviewer" });
        let none = json!({});
        assert_eq!(
            role_resolve(&sim, Some("lead"), Some("implementer")),
            Role::Lead,
            "the flag wins"
        );
        assert_eq!(
            role_resolve(&reviewer, Some("auto"), Some("implementer")),
            Role::Implementer,
            "`auto` defers to FL_ROLE"
        );
        assert_eq!(
            role_resolve(&reviewer, None, Some("lead")),
            Role::Lead,
            "FL_ROLE beats agent_type"
        );
        assert_eq!(role_resolve(&sim, Some("auto"), None), Role::Implementer);
        assert_eq!(
            role_resolve(&json!({ "agent_type": "client-engineer" }), None, None),
            Role::Implementer
        );
        assert_eq!(role_resolve(&reviewer, None, None), Role::ReadOnly);
        assert_eq!(
            role_resolve(&json!({ "agent_type": "perf-analyst" }), None, None),
            Role::ReadOnly
        );
        assert_eq!(
            role_resolve(&none, Some("auto"), Some("bogus")),
            Role::Lead,
            "no agent_type: the main session, the lead (ruling R37)"
        );
        assert_eq!(
            role_resolve(&json!({ "agent_type": "general-purpose" }), None, None),
            Role::Implementer,
            "an unknown subagent is an implementer (ruling R37)"
        );
        assert_eq!(
            role_resolve(&json!({ "agent_type": "test-author" }), None, None),
            Role::Lead,
            "test-author is confined by its own --only hook"
        );
    }

    #[test]
    fn subagent_stop_gates_only_implementers() {
        assert!(stop_gates("Stop", Role::Lead));
        assert!(stop_gates("Stop", Role::Implementer));
        assert!(!stop_gates("Stop", Role::ReadOnly));
        assert!(stop_gates("SubagentStop", Role::Implementer));
        assert!(!stop_gates("SubagentStop", Role::Lead));
        assert!(!stop_gates("SubagentStop", Role::ReadOnly));
    }

    #[test]
    fn stop_error_blocks_once() {
        let first = stop_error(&anyhow::anyhow!("write counter"), false);
        assert_eq!(first["decision"], "block");
        assert!(first["reason"].as_str().unwrap().contains("write counter"));
        let again = stop_error(&anyhow::anyhow!("write counter"), true);
        assert!(again.get("decision").is_none(), "no loop: {again}");
        assert!(
            again["systemMessage"]
                .as_str()
                .unwrap()
                .starts_with("ESCALATE")
        );
    }

    #[test]
    fn edit_without_path_is_denied_to_non_leads() {
        let input = json!({ "tool_input": {} });
        let root = crate::cmd::workspace_root();
        assert!(matches!(
            run_guard_edit(&root, &input, Role::Implementer, &[]),
            Decision::Deny(_)
        ));
        assert_eq!(
            run_guard_edit(&root, &input, Role::Lead, &[]),
            Decision::Allow
        );
    }

    #[test]
    fn workspace_comes_from_the_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".git"), "gitdir: x").unwrap();
        let sub = dir.path().join("crates").join("a");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(repo_root(&sub).as_deref(), Some(dir.path()));
        assert_eq!(
            repo_relative(&sub.join("tests").join("snapshots").join("x.snap")),
            Some((
                dir.path().to_path_buf(),
                PathBuf::from("crates/a/tests/snapshots/x.snap")
            ))
        );
    }

    #[test]
    fn nested_workspace_keeps_its_own_protected_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".git"), "gitdir: x").unwrap();
        let workspace = dir.path().join("bevy-rts");
        std::fs::create_dir_all(workspace.join("crates")).unwrap();
        std::fs::write(workspace.join("OWNERS.toml"), "[protected]\npaths = []\n").unwrap();
        assert_eq!(
            repo_root(&workspace.join("crates")),
            Some(workspace.clone())
        );
        assert_eq!(
            repo_relative(&workspace.join("crates/sim/tests/snapshots/x.snap")),
            Some((
                workspace,
                PathBuf::from("crates/sim/tests/snapshots/x.snap")
            ))
        );
    }

    #[test]
    fn protected_matching_ignores_case() {
        assert!(matches!(
            guard_edit(
                &owners(),
                Role::Implementer,
                Path::new("Crates/fl_sim/Tests/SNAPSHOTS/a.snap")
            ),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn only_never_allows_snapshots() {
        let globs = ["**/tests/acceptance/**".to_owned()];
        assert!(matches!(
            guard_only(
                &globs,
                Some(Path::new("crates/fl_sim/tests/acceptance/snapshots/a.snap"))
            )
            .unwrap(),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn xtask_bless_any_spelling() {
        for cmd in [
            "cargo run -q -p xtask -- bless crates/x --reason y",
            "cargo run -p xtask --release -- bless x --reason y",
            "target/hooks/xtask.exe bless x --reason y",
            r"C:\fl\a\target\hooks\XTASK.EXE bless x --reason y",
            "cargo xtask -- bless x --reason y",
        ] {
            assert!(
                matches!(guard_bash(Role::Implementer, cmd), Decision::Deny(_)),
                "{cmd}"
            );
        }
        assert_eq!(
            guard_bash(Role::Implementer, "cargo xtask gate task; echo bless"),
            Decision::Allow
        );
    }

    #[test]
    fn shell_writes_onto_protected_paths() {
        let o = owners();
        let rel = |t: &str| Some(PathBuf::from(t));
        for cmd in [
            "mv a.snap crates/fl_sim/tests/snapshots/a.snap",
            "cp x crates/fl_view/tests/snapshots/a.snap",
            "echo x > crates/fl_view/tests/snapshots/x.snap",
            "echo x >>scenarios/duel.ron",
            "Set-Content -Path scenarios/duel.ron -Value x",
            "'x' | Out-File docs/perf/budgets.ron",
            "Move-Item a.rs crates/fl_sim/tests/acceptance/a.rs",
            "Copy-Item -Destination crates/fl_sim/tests/holdout/h.rs a.rs",
            "copy a.ron Scenarios\\duel.ron",
        ] {
            assert!(
                matches!(
                    shell_writes(&o, Role::Implementer, cmd, &rel),
                    Decision::Deny(_)
                ),
                "{cmd}"
            );
            assert_eq!(
                shell_writes(&o, Role::Lead, cmd, &rel),
                Decision::Allow,
                "{cmd}"
            );
        }
        for cmd in [
            "cp x crates/fl_view/tests/snapshots/a.snap.new",
            "cat crates/fl_view/tests/snapshots/a.snap",
            "cargo test 2>&1 | tee target/out.log",
            "Get-Content scenarios/duel.ron | Select-String x > target/x.txt",
            "echo x > crates/fl_sim/src/a.rs",
        ] {
            assert_eq!(
                shell_writes(&o, Role::Implementer, cmd, &rel),
                Decision::Allow,
                "{cmd}"
            );
        }
    }

    #[test]
    fn read_only_denies_more_git_writes() {
        for cmd in [
            "git switch main",
            "git branch -D x",
            "git tag v1",
            "git worktree add ../x",
            "git config user.name x",
            "git update-ref refs/heads/x HEAD",
        ] {
            assert!(
                matches!(guard_bash(Role::ReadOnly, cmd), Decision::Deny(_)),
                "{cmd}"
            );
        }
        assert_eq!(
            guard_bash(Role::ReadOnly, "git branch --list"),
            Decision::Allow
        );
    }

    #[test]
    fn guard_denies_protected_for_implementer() {
        let d = guard_edit(
            &owners(),
            Role::Implementer,
            Path::new("crates/fl_sim/tests/snapshots/a.snap"),
        );
        match d {
            Decision::Deny(reason) => assert!(reason.contains("protected path"), "{reason}"),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn guard_allows_protected_for_lead() {
        assert_eq!(
            guard_edit(
                &owners(),
                Role::Lead,
                Path::new("crates/fl_sim/tests/snapshots/a.snap")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn guard_allows_unprotected_for_implementer() {
        assert_eq!(
            guard_edit(
                &owners(),
                Role::Implementer,
                Path::new("crates/fl_sim/src/step.rs")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn guard_denies_bless_variants() {
        for cmd in [
            "cargo insta accept",
            "INSTA_UPDATE=always cargo test",
            "cargo xtask bless x --reason y",
            "git checkout -- tests/",
            "cargo insta review",
            "cargo insta test --accept",
            "INSTA_FORCE_PASS=1 cargo test",
            "$env:INSTA_UPDATE = 'always'; cargo test",
            "cargo test -- --bless",
            "git restore crates/fl_view/tests/snapshots/x.snap",
            "git -C C:/fl/a checkout -- crates/fl_sim/tests/",
        ] {
            assert!(
                matches!(guard_bash(Role::Implementer, cmd), Decision::Deny(_)),
                "`{cmd}` must be denied to an implementer"
            );
            assert_eq!(
                guard_bash(Role::Lead, cmd),
                Decision::Allow,
                "`{cmd}` is the lead's call"
            );
        }
        for cmd in [
            "cargo insta test",
            "INSTA_UPDATE=new cargo test",
            "cargo xtask gate task --track contract",
            "git checkout -b m0/t12",
        ] {
            assert_eq!(
                guard_bash(Role::Implementer, cmd),
                Decision::Allow,
                "`{cmd}` is allowed"
            );
        }
    }

    #[test]
    fn guard_asks_for_cargo_update() {
        for role in [Role::Lead, Role::Implementer, Role::ReadOnly] {
            for cmd in [
                "cargo update -p serde",
                "cargo add serde",
                "rustup default nightly",
                "git push --force origin main",
                "git push -f",
                "git -C C:/fl/a push --force-with-lease",
            ] {
                assert!(
                    matches!(guard_bash(role, cmd), Decision::Ask(_) | Decision::Deny(_)),
                    "`{cmd}` must ask {role:?}"
                );
            }
            assert!(matches!(
                guard_bash(role, "cargo update -p serde"),
                Decision::Ask(_)
            ));
        }
        assert_eq!(
            guard_bash(Role::Lead, "git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn read_only_role_never_edits_or_writes_git() {
        assert!(matches!(
            guard_edit(
                &owners(),
                Role::ReadOnly,
                Path::new("crates/fl_sim/src/step.rs")
            ),
            Decision::Deny(_)
        ));
        for cmd in ["git commit -m x", "git reset --hard", "git stash push"] {
            assert!(
                matches!(guard_bash(Role::ReadOnly, cmd), Decision::Deny(_)),
                "{cmd}"
            );
        }
        assert_eq!(guard_bash(Role::ReadOnly, "git diff main"), Decision::Allow);
    }

    #[test]
    fn stop_blocks_then_escalates() {
        for attempts in 0..3 {
            let (out, next) = stop_decision(false, attempts, "FAIL fmt");
            assert_eq!(
                out,
                Some(json!({ "decision": "block", "reason": "FAIL fmt" })),
                "attempt {attempts}"
            );
            assert_eq!(next, attempts + 1);
        }
        assert_eq!(
            stop_decision(false, 3, "FAIL fmt"),
            (
                Some(
                    json!({ "systemMessage": "ESCALATE: gate task still failing after 3 attempts" })
                ),
                3
            ),
            "escalation is visible in the JSON, and never blocks again"
        );
        assert_eq!(
            stop_decision(true, 2, ""),
            (None, 0),
            "a pass resets the counter"
        );
    }

    #[test]
    fn only_globs_confine_edits() {
        let globs = [
            "**/tests/acceptance/**".to_owned(),
            "**/tests/holdout/**".to_owned(),
        ];
        assert_eq!(
            guard_only(
                &globs,
                Some(Path::new("crates/fl_sim/tests/acceptance/move.rs"))
            )
            .unwrap(),
            Decision::Allow
        );
        assert!(matches!(
            guard_only(&globs, Some(Path::new("crates/fl_sim/src/step.rs"))).unwrap(),
            Decision::Deny(_)
        ));
        assert!(matches!(
            guard_only(&globs, None).unwrap(),
            Decision::Deny(_)
        ));
        assert_eq!(guard_only(&[], None).unwrap(), Decision::Allow);
    }

    #[test]
    fn worktree_remove_only_under_build_root() {
        assert_eq!(
            removable_build_dir(r"C:/t/fl/c5\fdf61b108f2dc9"),
            Some(PathBuf::from("C:/t/fl/c5/fdf61b108f2dc9"))
        );
        assert!(
            removable_build_dir(r"c:\T\FL\ab\cd").is_some(),
            "case and separators"
        );
        for bad in [
            "C:/t/fl/",
            "C:/t/fl",
            "C:/t/flx/a",
            "C:/t/fl/../x",
            "C:/fl/a/target",
            "",
        ] {
            assert_eq!(removable_build_dir(bad), None, "{bad}");
        }
    }

    #[test]
    fn deny_answer_is_pretooluse_json() {
        assert_eq!(Decision::Allow.to_hook_json(), None);
        assert_eq!(
            Decision::Deny("no".into()).to_hook_json(),
            Some(json!({ "hookSpecificOutput": {
                "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "no" } }))
        );
    }

    #[test]
    fn claude_md_limit() {
        let text = |n: usize| "line\n".repeat(n);
        assert!(matches!(claude_md_check(&text(201)), Outcome::Fail(_)));
        assert_eq!(claude_md_check(&text(200)), Outcome::Pass);
    }
}
