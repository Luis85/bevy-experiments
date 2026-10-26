//! Shell-level tests for the Claude Code hooks in `.claude/hooks/`.
//!
//! Each test pipes a sample hook JSON payload into a script via `sh` and
//! asserts the exit code (and, for the Stop hook, the `decision` JSON on
//! stdout). Every test uses its own temp workspace through `NPC_HOOK_ROOT`,
//! so the real `.claude/rebaseline-approved` and `target/claude-hooks/` are
//! never touched, and the Stop hook runs a fake verify command
//! (`NPC_VERIFY_CMD`), never the real `cargo xtask verify`.
//!
//! `sh`: `NPC_SH` if set, else Git for Windows' `sh.exe` on Windows (the
//! `bash.exe` on a Windows PATH is often WSL's), else `sh` from PATH.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

fn hooks_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .join(".claude")
        .join("hooks")
}

fn sh() -> PathBuf {
    if let Some(p) = std::env::var_os("NPC_SH") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        let mut candidates = vec![
            PathBuf::from(r"C:\Program Files\Git\bin\sh.exe"),
            PathBuf::from(r"C:\Program Files\Git\usr\bin\sh.exe"),
        ];
        // `git --exec-path` = <git>/mingw64/libexec/git-core
        if let Ok(out) = Command::new("git").arg("--exec-path").output() {
            let exec = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if let Some(root) = exec.ancestors().nth(3) {
                candidates.push(root.join("bin").join("sh.exe"));
            }
        }
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return found;
        }
    }
    PathBuf::from("sh")
}

/// A throwaway workspace root for one test.
struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("npc-hooks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".claude")).unwrap();
        Workspace { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// The root with forward slashes, as an sh-friendly string.
    fn root_slash(&self) -> String {
        self.root.to_string_lossy().replace('\\', "/")
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, content).unwrap();
        p
    }

    fn approve_rebaseline(&self) {
        self.write(".claude/rebaseline-approved", "");
    }

    /// The Stop hook's stamp, `age` seconds old (negative = in the future).
    fn stamp(&self, age_secs: i64) {
        let p = self.write("target/claude-hooks/verify.stamp", "");
        let now = SystemTime::now();
        let t = if age_secs >= 0 {
            now - Duration::from_secs(age_secs as u64)
        } else {
            now + Duration::from_secs((-age_secs) as u64)
        };
        fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_hook(script: &str, ws: &Workspace, input: &str, env: &[(&str, String)]) -> Outcome {
    let mut cmd = Command::new(sh());
    cmd.arg(hooks_dir().join(script))
        .env("NPC_HOOK_ROOT", ws.root_slash())
        .env_remove("NPC_VERIFY_CMD")
        .env_remove("NPC_VERIFY_HOOK")
        .env_remove("NPC_VERIFY_MAX_BLOCKS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap_or_else(|e| {
        panic!("cannot run sh ({e}); the hooks need POSIX sh (Git Bash on Windows, or set NPC_SH)")
    });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Outcome {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A PreToolUse payload for a file tool on `path` (native separators).
fn file_tool(tool: &str, path: &Path) -> String {
    let p = json_escape(&path.to_string_lossy());
    format!(
        r#"{{"session_id":"t","hook_event_name":"PreToolUse","tool_name":"{tool}","tool_input":{{"file_path":"{p}","old_string":"a \"file_path\": \"x\"","new_string":"b"}}}}"#
    )
}

/// A PreToolUse payload for a shell tool running `command`.
fn shell_tool(tool: &str, command: &str) -> String {
    let c = json_escape(command);
    format!(
        r#"{{"session_id":"t","hook_event_name":"PreToolUse","tool_name":"{tool}","tool_input":{{"command":"{c}","description":"d"}}}}"#
    )
}

fn protect(ws: &Workspace, input: &str) -> Outcome {
    run_hook("protect_goldens.sh", ws, input, &[])
}

/// Runs the protect hook on every `(label, payload)` at once (process
/// spawns are slow in Git Bash) and asserts each exit code is `want`.
fn expect_all(ws: &Workspace, cases: &[(String, String)], want: i32) {
    let outcomes: Vec<(String, Outcome)> = std::thread::scope(|s| {
        let handles: Vec<_> = cases
            .iter()
            .map(|(label, input)| s.spawn(move || (label.clone(), protect(ws, input))))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wrong: Vec<String> = outcomes
        .iter()
        .filter(|(_, o)| o.code != want)
        .map(|(l, o)| {
            format!(
                "`{l}` -> {} ({})",
                o.code,
                o.stderr.lines().next().unwrap_or("")
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "expected exit {want} for:\n{}",
        wrong.join("\n")
    );
}

fn shell_cases(tool: &str, commands: &[String]) -> Vec<(String, String)> {
    commands
        .iter()
        .map(|c| (c.clone(), shell_tool(tool, c)))
        .collect()
}

const PROTECTED: &[&str] = &[
    "tests/replays/village_seed1_3days.json",
    "tests/golden/run_seed1.json",
    "crates/npc_sim/tests/snapshots/golden__journal.snap",
    "xtask/tests/snapshots/inspect.snap",
    "bench/baseline.json",
    "crates/npc_sim/clippy.toml",
    "clippy.toml",
    "deny.toml",
    "xtask/tests/arch_fitness.rs",
    "xtask/src/main.rs",
    ".claude/hooks/protect_goldens.sh",
    ".claude/settings.json",
    ".claude/settings.local.json",
];

#[test]
fn hook_blocks_golden_edit() {
    let ws = Workspace::new("blocks");
    let out = protect(
        &ws,
        &file_tool("Edit", &ws.path("tests/replays/village_seed1_3days.json")),
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("ask the user"),
        "stderr: {}",
        out.stderr
    );
    for (i, rel) in PROTECTED.iter().enumerate() {
        let tool = ["Edit", "Write", "MultiEdit"][i % 3];
        let out = protect(&ws, &file_tool(tool, &ws.path(rel)));
        assert_eq!(
            out.code, 2,
            "{tool} {rel} must be blocked; stderr: {}",
            out.stderr
        );
    }
}

#[test]
fn hook_blocks_path_tricks() {
    let ws = Workspace::new("tricks");
    let root = ws.root_slash();
    let cases = [
        // `..` traversal, mixed case, MSYS-style drive.
        format!("{root}/crates/npc_sim/../../tests/replays/a.json"),
        format!("{root}/TESTS/Replays/a.json"),
        format!("{root}/./deny.toml"),
    ];
    for p in &cases {
        let out = protect(&ws, &file_tool("Write", Path::new(p)));
        assert_eq!(out.code, 2, "{p} must be blocked; stderr: {}", out.stderr);
    }
    if let Some(rest) = root
        .strip_prefix("C:/")
        .or_else(|| root.strip_prefix("c:/"))
    {
        let msys = format!("/c/{rest}/bench/baseline.json");
        let out = protect(&ws, &file_tool("Write", Path::new(&msys)));
        assert_eq!(out.code, 2, "{msys} must be blocked");
    }
    let nb = format!(
        r#"{{"tool_name":"NotebookEdit","tool_input":{{"notebook_path":"{}","new_source":"x"}}}}"#,
        json_escape(&ws.path("tests/golden/n.ipynb").to_string_lossy())
    );
    assert_eq!(protect(&ws, &nb).code, 2);
}

#[test]
fn hook_allows_src_edit() {
    let ws = Workspace::new("allows");
    for rel in [
        "crates/npc_sim/src/needs.rs",
        "crates/npc_sim/tests/villager.rs",
        "CLAUDE.md",
        "docs/adr/0001-own-bt-runtime.md",
        "tests/replays_notes.md",
        ".claude/skills/why/SKILL.md",
    ] {
        let out = protect(&ws, &file_tool("Edit", &ws.path(rel)));
        assert_eq!(out.code, 0, "{rel} must be allowed; stderr: {}", out.stderr);
    }
    // Outside the workspace: not this hook's business.
    let outside = ws
        .root
        .parent()
        .unwrap()
        .join("tests")
        .join("replays")
        .join("x.json");
    assert_eq!(protect(&ws, &file_tool("Write", &outside)).code, 0);
    // Other tools pass through.
    let read = r#"{"tool_name":"Read","tool_input":{"file_path":"tests/replays/a.json"}}"#;
    assert_eq!(protect(&ws, read).code, 0);
}

#[test]
fn hook_allows_after_rebaseline_marker() {
    let ws = Workspace::new("marker");
    ws.approve_rebaseline();
    for rel in PROTECTED {
        let out = protect(&ws, &file_tool("Write", &ws.path(rel)));
        assert_eq!(
            out.code, 0,
            "{rel} must be allowed after approval; stderr: {}",
            out.stderr
        );
    }
    let out = protect(&ws, &shell_tool("Bash", "cargo insta accept"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

#[test]
fn hook_never_lets_agent_write_marker() {
    let ws = Workspace::new("nomarker");
    for approved in [false, true] {
        if approved {
            ws.approve_rebaseline();
        }
        let marker = ws.path(".claude/rebaseline-approved");
        assert_eq!(protect(&ws, &file_tool("Write", &marker)).code, 2);
        let stamp = ws.path("target/claude-hooks/verify.stamp");
        assert_eq!(protect(&ws, &file_tool("Write", &stamp)).code, 2);
        for c in [
            "touch .claude/rebaseline-approved",
            "echo ok > .claude/rebaseline-approved",
            "rm -f .claude/rebaseline-approved",
            "touch target/claude-hooks/verify.stamp",
        ] {
            assert_eq!(
                protect(&ws, &shell_tool("Bash", c)).code,
                2,
                "{c} must be blocked"
            );
        }
        let ps = "New-Item -ItemType File .claude\\rebaseline-approved";
        assert_eq!(protect(&ws, &shell_tool("PowerShell", ps)).code, 2);
    }
    // Reading it is fine.
    let out = protect(
        &ws,
        &shell_tool("Bash", "test -f .claude/rebaseline-approved && echo yes"),
    );
    assert_eq!(out.code, 0);
}

#[test]
fn hook_blocks_shell_writes_to_goldens() {
    let ws = Workspace::new("shell");
    for c in [
        "echo {} > tests/replays/a.json",
        "cat x >> tests\\golden\\run.json",
        "cargo run -p npcctl -- replay record --ticks 100 --out tests/replays/a.json",
        "cargo insta accept",
        "INSTA_UPDATE=always cargo test -p npc_sim",
        "sed -i s/a/b/ crates/npc_sim/clippy.toml",
        "cp /tmp/b.json bench/baseline.json",
        "rm -rf crates/npc_sim/tests/snapshots",
        "rm -r tests/replays",
        "git rm .claude/hooks/verify_on_stop.sh",
        "cd crates && mv a.snap snapshots/b.snap",
    ] {
        let out = protect(&ws, &shell_tool("Bash", c));
        assert_eq!(out.code, 2, "`{c}` must be blocked");
    }
    let ps = "Set-Content -Path bench\\baseline.json -Value 1";
    assert_eq!(protect(&ws, &shell_tool("PowerShell", ps)).code, 2);
    for c in [
        "cargo run -p npcctl -- replay verify tests/replays/a.json > out.json 2>&1",
        "cat tests/golden/run.json | head -n 5",
        "git diff --stat tests/replays bench/baseline.json",
        "cargo test -p npc_sim --test corpus",
        "cargo run -p npcctl -- bench --baseline bench/baseline.json --out bench/current.json",
    ] {
        let out = protect(&ws, &shell_tool("Bash", c));
        assert_eq!(out.code, 0, "`{c}` must be allowed; stderr: {}", out.stderr);
    }
}

#[test]
fn hook_blocks_windows_path_forms() {
    let ws = Workspace::new("winforms");
    let root = ws.root_slash();
    let back = root.replace('/', "\\");
    let cases = [
        // Windows drops a trailing dot or space from every component.
        format!("{root}/tests/replays./a.json"),
        format!("{root}/tests/replays /a.json"),
        format!("{root}/bench/baseline.json."),
        format!("{root}/bench/baseline.json "),
        format!("{root}/deny.toml. ."),
        // Verbatim and device prefixes.
        format!("\\\\?\\{back}\\deny.toml"),
        format!("\\\\.\\{back}\\clippy.toml"),
        format!("//?/{root}/bench/baseline.json"),
        // NTFS streams write the file itself.
        format!("{root}/bench/baseline.json::$DATA"),
        // 8.3 short names (name heuristic; cygpath when it exists).
        format!("{root}/BENCH/BASELI~1.JSO"),
        format!("{root}/crates/x/tests/SNAPSH~1/a.json"),
        format!("{root}/xtask/tests/ARCH_F~1.RS"),
        // Newly protected: the verify definition and the CI workflow.
        format!("{root}/xtask/src/main.rs"),
    ];
    let blocked: Vec<(String, String)> = cases
        .iter()
        .map(|p| (p.clone(), file_tool("Write", Path::new(p))))
        .collect();
    expect_all(&ws, &blocked, 2);
    let workflow = ws
        .root
        .parent()
        .unwrap()
        .join(".github")
        .join("workflows")
        .join("npc-framework.yml");
    let out = protect(&ws, &file_tool("Edit", &workflow));
    assert_eq!(out.code, 2, "the CI workflow is protected");
    let other = ws
        .root
        .parent()
        .unwrap()
        .join(".github")
        .join("workflows")
        .join("other.yml");
    assert_eq!(protect(&ws, &file_tool("Edit", &other)).code, 0);
    for rel in [
        "xtask/src/lib.rs",
        "bench/README.md",
        "tests/replays_notes.md",
    ] {
        let out = protect(&ws, &file_tool("Write", &ws.path(rel)));
        assert_eq!(out.code, 0, "{rel} must be allowed; stderr: {}", out.stderr);
    }
}

#[test]
fn hook_blocks_shell_bypasses() {
    let ws = Workspace::new("bypass");
    // A patch that touches an oracle without naming it on the command line.
    ws.write(
        "fix.patch",
        "diff --git a/bench/baseline.json b/bench/baseline.json\n--- a/bench/baseline.json\n+++ b/bench/baseline.json\n@@ -1 +1 @@\n-1\n+2\n",
    );
    ws.write(
        "ok.patch",
        "--- a/crates/x/src/lib.rs\n+++ b/crates/x/src/lib.rs\n@@ -1 +1 @@\n-1\n+2\n",
    );
    let patch = ws.path("fix.patch").to_string_lossy().replace('\\', "/");
    let ok_patch = ws.path("ok.patch").to_string_lossy().replace('\\', "/");
    let bash = [
        // git restoring or importing oracles.
        "git checkout -- tests/replays".to_owned(),
        "git checkout main -- bench/baseline.json".to_owned(),
        "git restore --source=HEAD~3 tests/golden/run.json".to_owned(),
        "git -C . restore tests/replays/a.json".to_owned(),
        "git stash push tests/replays".to_owned(),
        "git reset HEAD~1 -- deny.toml".to_owned(),
        format!("git apply {patch}"),
        format!("patch -p1 < {patch}"),
        // cd, then a relative path.
        "cd bench; rm baseline.json".to_owned(),
        "cd bench && rm -f baseline.json".to_owned(),
        "cd xtask/tests && sed -i s/a/b/ arch_fitness.rs".to_owned(),
        // Deleting a directory that holds oracles.
        "rm -rf tests".to_owned(),
        "rm -r .claude".to_owned(),
        "rm -rf *".to_owned(),
        "rm -rf apps/npcctl/tests".to_owned(),
        // Trailing dot/space, stream, verbatim prefix in a command.
        "echo x > tests/replays./a.json".to_owned(),
        "echo x > \"tests/replays /a.json\"".to_owned(),
        "echo x > bench/baseline.json::$DATA".to_owned(),
        "touch BENCH/BASELI~1.JSO".to_owned(),
        // Wrappers, find, xargs, copy destinations.
        "bash -c 'rm -rf tests/replays'".to_owned(),
        "sudo rm tests/golden/a.json".to_owned(),
        "find tests/golden -name '*.json' -delete".to_owned(),
        "ls tests/replays | xargs rm".to_owned(),
        "cp -t tests/golden x.json".to_owned(),
        "cp a.rs xtask/src/main.rs".to_owned(),
        "echo x >> ../.github/workflows/npc-framework.yml".to_owned(),
        // Inline scripts.
        "python -c \"open('bench/baseline.json','w').write('{}')\"".to_owned(),
        "py -c \"import os; os.remove('tests/replays/a.json')\"".to_owned(),
        "python3 -c \"import pathlib; pathlib.Path('deny.toml').write_text('')\"".to_owned(),
    ];
    expect_all(&ws, &shell_cases("Bash", &bash), 2);
    let ps: Vec<String> = [
        "ri tests\\replays\\a.json",
        "Remove-Item -Recurse -Force tests\\golden",
        "cpi x.json bench\\baseline.json",
        "Copy-Item -Path x.json -Destination tests\\golden\\x.json",
        "mi a.snap b.json",
        "Move-Item tests\\golden\\a.json b.json",
        "rni deny.toml deny.old",
        "[IO.File]::WriteAllText('bench/baseline.json', '{}')",
        "[System.IO.File]::Delete(\"tests/golden/run.json\")",
        "Set-Location tests; Remove-Item replays -Recurse",
    ]
    .map(str::to_owned)
    .to_vec();
    expect_all(&ws, &shell_cases("PowerShell", &ps), 2);
    // Copying FROM an oracle, reading one, or a patch elsewhere is fine.
    let allowed = [
        "cp tests/golden/run.json /tmp/x.json".to_owned(),
        "cp bench/baseline.json bench/old.json".to_owned(),
        "cp -r tests/replays /tmp/corpus".to_owned(),
        "git log -- tests/replays".to_owned(),
        "git show HEAD:npc-framework/bench/baseline.json".to_owned(),
        "git checkout -b fix".to_owned(),
        format!("git apply {ok_patch}"),
        "python -c \"print(open('bench/baseline.json').read())\"".to_owned(),
        "rm -rf target".to_owned(),
        "rm -rf crates/npc_tools/src/old".to_owned(),
        "cd /tmp && rm -rf *".to_owned(),
        "cd crates && cargo test".to_owned(),
    ];
    expect_all(&ws, &shell_cases("Bash", &allowed), 0);
    let ps = "Copy-Item tests\\golden\\run.json C:\\tmp\\x.json";
    assert_eq!(protect(&ws, &shell_tool("PowerShell", ps)).code, 0, "{ps}");
}

// ---------------------------------------------------------------------------
// Stop hook
// ---------------------------------------------------------------------------

const FAIL_JSON: &str = r#"{"ok":false,"steps":[{"name":"fmt","ok":true,"ms":12,"summary":"formatted","log_tail":""},{"name":"clippy","ok":false,"ms":900,"summary":"2 warnings in npc_sim","log_tail":"warning: unused \"x\"\nerror: could not compile"}]}"#;
const OK_JSON: &str =
    r#"{"ok":true,"steps":[{"name":"fmt","ok":true,"ms":12,"summary":"formatted","log_tail":""}]}"#;

fn stop_input(active: bool) -> String {
    format!(
        r#"{{"session_id":"t","hook_event_name":"Stop","stop_hook_active":{active},"last_assistant_message":"done"}}"#
    )
}

/// Runs the Stop hook with a fake verify that prints `verdict` and leaves a
/// `ran` marker (both under the pruned `target/`, so they never count as
/// changes). Returns the outcome and whether the fake verify ran.
fn stop(ws: &Workspace, active: bool, verdict: &str) -> (Outcome, bool) {
    let fake = ws.write("target/fake/verdict.json", verdict);
    let ran = ws.path("target/fake/ran");
    let _ = fs::remove_file(&ran);
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let cmd = format!("cat '{}'; : > '{}'", slash(&fake), slash(&ran));
    let out = run_hook(
        "verify_on_stop.sh",
        ws,
        &stop_input(active),
        &[("NPC_VERIFY_CMD", cmd)],
    );
    (out, ran.exists())
}

fn is_block(out: &Outcome) -> bool {
    out.stdout.contains(r#""decision":"block""#)
}

#[test]
fn stop_hook_blocks_failed_verify() {
    let ws = Workspace::new("stopfail");
    ws.write("crates/a/src/lib.rs", "");
    let (out, ran) = stop(&ws, false, FAIL_JSON);
    assert!(ran);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(is_block(&out), "stdout: {}", out.stdout);
    assert!(
        out.stdout.contains("clippy: 2 warnings in npc_sim"),
        "stdout: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("- fmt"),
        "passing steps are not listed: {}",
        out.stdout
    );
}

#[test]
fn stop_hook_passes_ok_verify() {
    let ws = Workspace::new("stopok");
    ws.write("crates/a/src/lib.rs", "");
    let (out, ran) = stop(&ws, false, OK_JSON);
    assert!(ran);
    assert_eq!(out.code, 0);
    assert!(out.stdout.trim().is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn stop_hook_blocks_when_verify_gives_no_verdict() {
    let ws = Workspace::new("stopcrash");
    ws.write("crates/a/src/lib.rs", "");
    let (out, _) = stop(&ws, false, "error[E0425]: cannot find value");
    assert!(is_block(&out), "stdout: {}", out.stdout);
}

#[test]
fn stop_hook_skips_when_nothing_changed() {
    let ws = Workspace::new("stopskip");
    ws.write("crates/a/src/lib.rs", "");
    ws.write("Cargo.toml", "");
    ws.stamp(-60); // stamp newer than every file
    let (out, ran) = stop(&ws, false, FAIL_JSON);
    assert!(!ran, "verify must not run without changes");
    assert_eq!(out.code, 0);
    assert!(!is_block(&out));
    // A change to a .toml (or .rs/.json) makes it run again.
    ws.stamp(60);
    let (out, ran) = stop(&ws, false, FAIL_JSON);
    assert!(ran);
    assert!(is_block(&out));
}

#[test]
fn stop_hook_loop_guard() {
    let ws = Workspace::new("stoploop");
    ws.write("crates/a/src/lib.rs", "");
    // First stop: verify fails, the stop is blocked.
    let (out, ran) = stop(&ws, false, FAIL_JSON);
    assert!(ran && is_block(&out));
    // Claude continues (stop_hook_active) without editing anything: let it stop.
    let (out, ran) = stop(&ws, true, FAIL_JSON);
    assert!(!ran, "no re-run without new edits");
    assert_eq!(out.code, 0);
    assert!(!is_block(&out), "stdout: {}", out.stdout);
    // With edits it re-verifies and may block again, but at most 3 times in a row.
    let mut blocked = 1;
    for _ in 0..5 {
        ws.stamp(60);
        let (out, _) = stop(&ws, true, FAIL_JSON);
        assert_eq!(out.code, 0);
        if is_block(&out) {
            blocked += 1;
        } else {
            break;
        }
    }
    assert_eq!(blocked, 3, "the block cap must end the loop");
}

#[test]
fn stop_hook_can_be_disabled() {
    let ws = Workspace::new("stopoff");
    ws.write("crates/a/src/lib.rs", "");
    let out = run_hook(
        "verify_on_stop.sh",
        &ws,
        &stop_input(false),
        &[
            ("NPC_VERIFY_HOOK", "off".into()),
            ("NPC_VERIFY_CMD", "exit 1".into()),
        ],
    );
    assert_eq!(out.code, 0);
    assert!(out.stdout.trim().is_empty());
}
