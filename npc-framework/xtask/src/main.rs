//! Repo automation. One command matters: `cargo xtask verify`.
//!
//! ```text
//! cargo xtask verify [--quick] [--pretty]
//! ```
//!
//! Runs, in order: `fmt` (`cargo fmt --check`), `clippy` (`-D warnings`, all
//! targets), `test` (`cargo nextest run --workspace` plus doc tests, or
//! `cargo test --workspace` when nextest is missing), `arch_fitness`
//! (`xtask/tests/arch_fitness.rs`), `replays` (`npcctl replay verify` on every
//! `tests/replays/*.json`), `goldens` (no pending insta snapshots) and `deny`
//! (`cargo deny check`, when installed). Every step runs even when an earlier
//! one failed, so one run shows every problem.
//!
//! Prints exactly one JSON object on stdout:
//! `{"ok":bool,"steps":[{"name","ok","ms","summary","log_tail"}]}`. Progress
//! goes to stderr. Exit code 0 when `ok`, 1 otherwise, 2 on bad usage.
//!
//! `--quick` (used by the Claude Code Stop hook) skips `deny`, `replays` (the
//! corpus is still verified by `content/village_content/tests/corpus.rs` inside `test`)
//! and the slow test binaries in [`SLOW_TESTS`]. A full run fails when the
//! replay corpus is empty.
//!
//! A missing optional tool (cargo-nextest, cargo-deny) is never a failure: the
//! step says `skipped: ...` (or names the fallback) with an install hint.
//! `XTASK_ASSUME_MISSING=nextest,deny` simulates that.
//!
//! This binary deliberately has no dependencies: `cargo test --workspace` also
//! builds it (for its integration tests), and on Windows a running `.exe`
//! cannot be overwritten, so its build must never depend on feature
//! unification across the workspace.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

/// Test binaries `--quick` skips: `(package, package dir, test binary)`.
/// The determinism tests run 20 000 ticks several times; the property and
/// shuffle tests run many randomized / reshuffled schedules.
const SLOW_TESTS: &[(&str, &str, &str)] = &[
    ("village_content", "content/village_content", "determinism"),
    ("village_content", "content/village_content", "properties"),
    ("village_content", "content/village_content", "shuffle"),
];

/// Lines kept in `log_tail` for a failed / passed step.
const TAIL_FAILED: usize = 40;
const TAIL_PASSED: usize = 6;

struct Step {
    name: &'static str,
    ok: bool,
    ms: u128,
    summary: String,
    log_tail: String,
}

/// What a step reports before timing is attached.
struct Outcome {
    ok: bool,
    summary: String,
    log: String,
}

impl Outcome {
    fn new(ok: bool, summary: impl Into<String>, log: impl Into<String>) -> Self {
        Self {
            ok,
            summary: summary.into(),
            log: log.into(),
        }
    }

    fn skipped(why: &str) -> Self {
        Self::new(true, format!("skipped: {why}"), "")
    }
}

struct Ctx {
    root: PathBuf,
    cargo: String,
    quick: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut quick = false;
    let mut pretty = false;
    match args.first().map(String::as_str) {
        Some("verify") => {}
        Some("-h" | "--help" | "help") | None => {
            eprintln!("{}", usage());
            return if args.is_empty() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
        Some(other) => {
            eprintln!("unknown command `{other}`\n\n{}", usage());
            return ExitCode::from(2);
        }
    }
    for arg in &args[1..] {
        match arg.as_str() {
            "--quick" => quick = true,
            "--pretty" => pretty = true,
            "-h" | "--help" => {
                eprintln!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown option `{other}`\n\n{}", usage());
                return ExitCode::from(2);
            }
        }
    }

    let ctx = Ctx {
        root: workspace_root(),
        cargo: std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()),
        quick,
    };
    let steps = verify(&ctx);
    let ok = steps.iter().all(|s| s.ok);
    println!("{}", render_json(ok, &steps, pretty));
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn usage() -> &'static str {
    "usage: cargo xtask verify [--quick] [--pretty]\n\n\
     Runs fmt, clippy, tests, arch_fitness, replays, goldens and cargo-deny and\n\
     prints one JSON verdict {ok, steps:[{name, ok, ms, summary, log_tail}]}.\n\
     --quick   skip cargo-deny, the CLI replay pass and the slow test binaries\n\
     --pretty  indent the JSON"
}

/// `xtask/..`: the workspace root, whatever the current directory is.
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
}

fn verify(ctx: &Ctx) -> Vec<Step> {
    type StepFn = fn(&Ctx) -> Outcome;
    let plan: [(&'static str, StepFn); 7] = [
        ("fmt", step_fmt),
        ("clippy", step_clippy),
        ("test", step_test),
        ("arch_fitness", step_arch_fitness),
        ("replays", step_replays),
        ("goldens", step_goldens),
        ("deny", step_deny),
    ];
    let mut steps = Vec::new();
    for (name, run) in plan {
        eprintln!("verify: {name} ...");
        let start = Instant::now();
        let outcome = run(ctx);
        let ms = start.elapsed().as_millis();
        let verdict = if outcome.ok { "ok" } else { "FAILED" };
        eprintln!("verify: {name} {verdict} in {ms} ms: {}", outcome.summary);
        let tail_len = if outcome.ok { TAIL_PASSED } else { TAIL_FAILED };
        steps.push(Step {
            name,
            ok: outcome.ok,
            ms,
            summary: outcome.summary,
            log_tail: tail(&outcome.log, tail_len),
        });
    }
    steps
}

// ---------------------------------------------------------------------------
// Steps

fn step_fmt(ctx: &Ctx) -> Outcome {
    let out = cargo(
        ctx,
        &["fmt", "--all", "--", "--check", "--files-with-diff"],
        &[],
    );
    if out.spawn_failed() || out.text.contains("no such command") {
        return Outcome::new(
            false,
            "rustfmt is not available (hint: rustup component add rustfmt)",
            out.text,
        );
    }
    if out.ok {
        return Outcome::new(true, "all files formatted", out.text);
    }
    let files: Vec<String> = out
        .text
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with(".rs"))
        .map(|l| {
            // rustfmt prints verbatim Windows paths (`\\?\C:\...`).
            let p = Path::new(l.strip_prefix(r"\\?\").unwrap_or(l));
            rel(&ctx.root, p)
        })
        .collect();
    let summary = if files.is_empty() {
        format!("rustfmt failed: {}", first_error(&out.text))
    } else {
        format!(
            "{} file(s) need formatting (run `cargo fmt --all`): {}",
            files.len(),
            short_list(&files, 8)
        )
    };
    Outcome::new(false, summary, out.text)
}

fn step_clippy(ctx: &Ctx) -> Outcome {
    let out = cargo(
        ctx,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--message-format=short",
            "--",
            "-D",
            "warnings",
        ],
        &[],
    );
    if out.text.contains("no such command") {
        return Outcome::new(
            false,
            "clippy is not available (hint: rustup component add clippy)",
            out.text,
        );
    }
    if out.ok {
        return Outcome::new(true, "no warnings", out.text);
    }
    let diags: Vec<&str> = out
        .text
        .lines()
        .filter(|l| is_short_diagnostic(l))
        .collect();
    let summary = if diags.is_empty() {
        format!("clippy failed: {}", first_error(&out.text))
    } else {
        format!("{} diagnostic(s); first: {}", diags.len(), diags[0].trim())
    };
    Outcome::new(false, summary, out.text)
}

fn step_test(ctx: &Ctx) -> Outcome {
    let envs = [("INSTA_UPDATE", "no")];
    if has_cargo_subcommand(ctx, "nextest") {
        let filter = nextest_filter(ctx.quick);
        let args = [
            "nextest",
            "run",
            "--workspace",
            "--no-fail-fast",
            "--color",
            "never",
            "-E",
            &filter,
        ];
        let nextest = cargo(ctx, &args, &envs);
        // nextest does not run doc tests.
        let doc = cargo(
            ctx,
            &["test", "--workspace", "--doc", "--no-fail-fast"],
            &envs,
        );
        let doc_counts = cargo_test_counts(&doc.text);
        let ok = nextest.ok && doc.ok;
        let mut summary = match nextest_summary(&nextest.text) {
            Some(s) => format!("nextest: {s}"),
            None => format!("nextest failed: {}", first_error(&nextest.text)),
        };
        let _ = write!(summary, "; doc tests: {}", doc_counts.describe());
        let mut failed = nextest_failures(&nextest.text);
        failed.extend(cargo_test_failures(&doc.text));
        if !failed.is_empty() {
            let _ = write!(summary, "; failed: {}", short_list(&failed, 10));
        }
        if ctx.quick {
            let _ = write!(summary, "; --quick skipped {}", slow_test_names());
        }
        // nextest last, so the tail shows its failures and summary.
        let log = format!("{}\n{}", doc.text, nextest.text);
        return Outcome::new(ok, summary, log);
    }

    let hint =
        "cargo-nextest not installed, ran cargo test (hint: cargo install cargo-nextest --locked)";
    let runs: Vec<CmdOutput> = if ctx.quick {
        quick_cargo_test_runs(ctx, &envs)
    } else {
        vec![cargo(
            ctx,
            &["test", "--workspace", "--no-fail-fast"],
            &envs,
        )]
    };
    let ok = runs.iter().all(|r| r.ok);
    let log: String = runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let counts = cargo_test_counts(&log);
    let failed = cargo_test_failures(&log);
    let mut summary = if counts.results == 0 && !ok {
        format!("cargo test failed: {}", first_error(&log))
    } else {
        format!("cargo test: {}", counts.describe())
    };
    if !failed.is_empty() {
        let _ = write!(summary, "; failed: {}", short_list(&failed, 10));
    }
    if ctx.quick {
        let _ = write!(summary, "; --quick skipped {}", slow_test_names());
    }
    let _ = write!(summary, " ({hint})");
    // Several runs: put the failure reports last so the tail shows them.
    let log = format!("{log}\n{}", libtest_failure_excerpt(&log));
    Outcome::new(ok, summary, log)
}

/// libtest's `failures:` sections (panic messages, then the failed names).
fn libtest_failure_excerpt(text: &str) -> String {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim() == "failures:" {
            inside = true;
        }
        if inside {
            out.push(line);
        }
        if line.contains("test result: ") {
            inside = false;
        }
    }
    out.join("\n")
}

fn step_arch_fitness(ctx: &Ctx) -> Outcome {
    let out = cargo(
        ctx,
        &[
            "test",
            "-p",
            "xtask",
            "--test",
            "arch_fitness",
            "--no-fail-fast",
        ],
        &[],
    );
    let counts = cargo_test_counts(&out.text);
    let failed = cargo_test_failures(&out.text);
    let summary = if counts.results == 0 && !out.ok {
        format!("arch_fitness did not run: {}", first_error(&out.text))
    } else if failed.is_empty() {
        counts.describe()
    } else {
        // The assertion messages carry the offending dependency paths.
        let paths: Vec<&str> = out
            .text
            .lines()
            .map(str::trim)
            .filter(|l| l.contains(" -> "))
            .collect();
        format!(
            "{}; failed: {}; {}",
            counts.describe(),
            short_list(&failed, 10),
            short_list(&paths, 4)
        )
    };
    Outcome::new(out.ok, summary, out.text)
}

/// What a corpus file is, from its text (xtask has no JSON parser): an
/// agent session record (`"replay": {..}` next to a scenario document), a
/// testbed replay (`"scenario": "testbed[:<name>]"`) or a village replay.
/// `npcctl replay verify` dispatches on the same thing.
fn corpus_kind(text: &str) -> &'static str {
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if squeezed.contains("\"replay\":{") {
        "session"
    } else if squeezed.contains("\"scenario\":\"testbed") {
        "testbed"
    } else {
        "village"
    }
}

fn step_replays(ctx: &Ctx) -> Outcome {
    if ctx.quick {
        return Outcome::skipped(
            "--quick (tests/replays is still verified by the corpus test in `test`)",
        );
    }
    let dir = ctx.root.join("tests").join("replays");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if files.is_empty() {
        // The corpus is an oracle: a full verify without it proves nothing.
        return Outcome::new(
            false,
            "no tests/replays/*.json: the replay corpus is missing (restore it with git; \
             never regenerate it without the user's approval)",
            format!("{} has no *.json", dir.display()),
        );
    }
    let build = cargo(ctx, &["build", "-p", "npcctl"], &[]);
    if !build.ok {
        return Outcome::new(
            false,
            format!("building npcctl failed: {}", first_error(&build.text)),
            build.text,
        );
    }
    let mut log = String::new();
    let mut failed = Vec::new();
    let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for file in &files {
        let kind = std::fs::read_to_string(file).map_or("unreadable", |t| corpus_kind(&t));
        *kinds.entry(kind).or_insert(0) += 1;
        let file_arg = file.to_string_lossy();
        let out = cargo(
            ctx,
            &[
                "run", "-q", "-p", "npcctl", "--", "replay", "verify", &file_arg,
            ],
            &[],
        );
        let name = file.file_name().map_or_else(
            || file_arg.to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let _ = writeln!(
            log,
            "== {name} ({kind}): {}",
            if out.ok { "ok" } else { "FAILED" }
        );
        log.push_str(&out.text);
        log.push('\n');
        if !out.ok {
            failed.push(name);
        }
    }
    let by_kind = kinds
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let summary = if failed.is_empty() {
        format!("{} replay(s) verified with npcctl ({by_kind})", files.len())
    } else {
        format!(
            "{}/{} replay(s) failed: {}",
            failed.len(),
            files.len(),
            short_list(&failed, 10)
        )
    };
    Outcome::new(failed.is_empty(), summary, log)
}

fn step_goldens(ctx: &Ctx) -> Outcome {
    let mut snaps = Vec::new();
    let mut pending = Vec::new();
    for top in ["tests", "crates", "apps", "content", "xtask"] {
        walk(&ctx.root.join(top), &mut |path| {
            let name = path.to_string_lossy();
            if name.ends_with(".snap.new") || name.ends_with(".pending-snap") {
                pending.push(rel(&ctx.root, path));
            } else if name.ends_with(".snap") {
                snaps.push(rel(&ctx.root, path));
            }
        });
    }
    if !pending.is_empty() {
        return Outcome::new(
            false,
            format!(
                "{} pending snapshot(s) (a golden changed; do not accept without approval): {}",
                pending.len(),
                short_list(&pending, 8)
            ),
            pending.join("\n"),
        );
    }
    if snaps.is_empty() {
        return Outcome::skipped(
            "no insta snapshots found under tests/, crates/, apps/, content/, xtask/",
        );
    }
    Outcome::new(
        true,
        format!(
            "{} snapshot(s), none pending (checked by `test` with INSTA_UPDATE=no)",
            snaps.len()
        ),
        "",
    )
}

fn step_deny(ctx: &Ctx) -> Outcome {
    if ctx.quick {
        return Outcome::skipped("--quick");
    }
    if !has_cargo_subcommand(ctx, "deny") {
        return Outcome::skipped(
            "cargo-deny not installed (hint: cargo install cargo-deny --locked)",
        );
    }
    // `-L error`: drop the (many, Bevy-caused) duplicate-version warnings.
    let out = cargo(
        ctx,
        &["deny", "--color", "never", "-L", "error", "check"],
        &[],
    );
    let verdicts: Vec<&str> = out
        .text
        .lines()
        .map(str::trim)
        .filter(|l| {
            l.starts_with("advisories ")
                || l.starts_with("bans ")
                || l.starts_with("licenses ")
                || l.starts_with("sources ")
        })
        .collect();
    let summary = if verdicts.is_empty() {
        if out.ok {
            "cargo deny check passed".to_owned()
        } else {
            format!("cargo deny failed: {}", first_error(&out.text))
        }
    } else {
        verdicts.join(", ")
    };
    Outcome::new(out.ok, summary, out.text)
}

// ---------------------------------------------------------------------------
// Test helpers

fn nextest_filter(quick: bool) -> String {
    // arch_fitness has its own step.
    let mut excluded = vec!["binary_id(xtask::arch_fitness)".to_owned()];
    if quick {
        excluded.extend(
            SLOW_TESTS
                .iter()
                .map(|(pkg, _, bin)| format!("binary_id({pkg}::{bin})")),
        );
    }
    format!("not ({})", excluded.join(" | "))
}

fn slow_test_names() -> String {
    SLOW_TESTS
        .iter()
        .map(|(pkg, _, bin)| format!("{pkg}::{bin}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Without nextest: the whole workspace minus the packages holding slow
/// tests, then those packages' lib and remaining test binaries.
fn quick_cargo_test_runs(ctx: &Ctx, envs: &[(&str, &str)]) -> Vec<CmdOutput> {
    let mut pkgs: Vec<(&str, &str)> = SLOW_TESTS.iter().map(|(p, d, _)| (*p, *d)).collect();
    pkgs.dedup();
    let mut args: Vec<String> = ["test", "--workspace", "--no-fail-fast"]
        .map(String::from)
        .to_vec();
    for (pkg, _) in &pkgs {
        args.push("--exclude".into());
        args.push((*pkg).into());
    }
    let mut runs = vec![cargo_owned(ctx, &args, envs)];
    for (pkg, dir) in pkgs {
        let mut args: Vec<String> = ["test", "-p", pkg, "--lib", "--no-fail-fast"]
            .map(String::from)
            .to_vec();
        let tests_dir = ctx.root.join(dir).join("tests");
        let mut bins: Vec<String> = std::fs::read_dir(&tests_dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter_map(|e| {
                        let p = e.path();
                        let stem = p.file_stem()?.to_string_lossy().into_owned();
                        let is_test =
                            p.extension().is_some_and(|x| x == "rs") || p.join("main.rs").is_file();
                        is_test.then_some(stem)
                    })
                    .collect()
            })
            .unwrap_or_default();
        bins.sort();
        bins.retain(|b| !SLOW_TESTS.iter().any(|(p, _, s)| *p == pkg && s == b));
        for bin in bins {
            args.push("--test".into());
            args.push(bin);
        }
        runs.push(cargo_owned(ctx, &args, envs));
    }
    runs
}

#[derive(Default)]
struct TestCounts {
    results: usize,
    passed: u64,
    failed: u64,
    ignored: u64,
}

impl TestCounts {
    fn describe(&self) -> String {
        format!(
            "{} passed, {} failed, {} ignored",
            self.passed, self.failed, self.ignored
        )
    }
}

/// Sums libtest's `test result: ok. 3 passed; 0 failed; 1 ignored; ...` lines.
fn cargo_test_counts(text: &str) -> TestCounts {
    let mut counts = TestCounts::default();
    for line in text.lines() {
        let Some(rest) = line.split("test result: ").nth(1) else {
            continue;
        };
        counts.results += 1;
        for part in rest.split(';') {
            let mut words = part.split_whitespace().rev();
            let (Some(label), Some(number)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(n) = number.parse::<u64>() else {
                continue;
            };
            match label {
                "passed" => counts.passed += n,
                "failed" => counts.failed += n,
                "ignored" => counts.ignored += n,
                _ => {}
            }
        }
    }
    counts
}

/// `test some::name ... FAILED` lines.
fn cargo_test_failures(text: &str) -> Vec<String> {
    let mut names: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let name = l.strip_prefix("test ")?.strip_suffix(" ... FAILED")?;
            Some(name.to_owned())
        })
        .collect();
    names.dedup();
    names
}

/// The text after `Summary [ 1.234s] `.
fn nextest_summary(text: &str) -> Option<String> {
    text.lines().rev().find_map(|l| {
        let l = l.trim();
        if !l.starts_with("Summary [") {
            return None;
        }
        l.split_once("] ").map(|(_, rest)| rest.trim().to_owned())
    })
}

/// Names from nextest's `FAIL [ 0.1s] (3/9) crate::binary test_name` lines.
fn nextest_failures(text: &str) -> Vec<String> {
    const BAD: [&str; 6] = [
        "FAIL [",
        "TIMEOUT [",
        "SIGSEGV [",
        "SIGABRT [",
        "ABORT [",
        "LEAK-FAIL [",
    ];
    let mut names: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if !BAD.iter().any(|b| l.starts_with(b)) {
            continue;
        }
        let Some((_, rest)) = l.split_once("] ") else {
            continue;
        };
        let rest = rest.trim();
        // Drop the optional `(3/9)` progress counter.
        let rest = match rest.strip_prefix('(').and_then(|r| r.split_once(") ")) {
            Some((_, name)) => name,
            None => rest,
        };
        let name = rest.trim().to_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

// ---------------------------------------------------------------------------
// Process helpers

struct CmdOutput {
    ok: bool,
    spawn_error: Option<String>,
    /// stderr, then stdout, ANSI escapes removed.
    text: String,
}

impl CmdOutput {
    fn spawn_failed(&self) -> bool {
        self.spawn_error.is_some()
    }
}

fn cargo(ctx: &Ctx, args: &[&str], envs: &[(&str, &str)]) -> CmdOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    cargo_owned(ctx, &owned, envs)
}

fn cargo_owned(ctx: &Ctx, args: &[String], envs: &[(&str, &str)]) -> CmdOutput {
    let mut cmd = Command::new(&ctx.cargo);
    cmd.args(args)
        .current_dir(&ctx.root)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TERM_PROGRESS_WHEN", "never")
        .stdin(Stdio::null());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    match cmd.output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&String::from_utf8_lossy(&out.stdout));
            CmdOutput {
                ok: out.status.success(),
                spawn_error: None,
                text: strip_ansi(&text),
            }
        }
        Err(e) => CmdOutput {
            ok: false,
            spawn_error: Some(e.to_string()),
            text: format!("failed to run `{} {}`: {e}", ctx.cargo, args.join(" ")),
        },
    }
}

/// Whether `cargo <sub>` exists. `XTASK_ASSUME_MISSING=nextest,deny` pretends
/// a tool is missing (to exercise the fallbacks).
fn has_cargo_subcommand(ctx: &Ctx, sub: &str) -> bool {
    let assumed_missing = std::env::var("XTASK_ASSUME_MISSING").unwrap_or_default();
    if assumed_missing.split(',').any(|t| t.trim() == sub) {
        return false;
    }
    Command::new(&ctx.cargo)
        .args([sub, "--version"])
        .current_dir(&ctx.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------------------
// Text helpers

/// rustc/clippy `--message-format=short` lines: `path:line:col: error: ...`.
fn is_short_diagnostic(line: &str) -> bool {
    let l = line.trim_start();
    (l.contains(": error") || l.contains(": warning")) && l.split(':').count() >= 4
}

/// The first line that starts with `error`, else the last non-empty line.
fn first_error(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("error"))
        .or_else(|| text.lines().map(str::trim).rfind(|l| !l.is_empty()))
        .unwrap_or("no output")
        .to_owned()
}

fn short_list<S: AsRef<str>>(items: &[S], max: usize) -> String {
    let shown: Vec<&str> = items.iter().take(max).map(AsRef::as_ref).collect();
    let mut s = shown.join(", ");
    if items.len() > max {
        let _ = write!(s, ", ... ({} more)", items.len() - max);
    }
    s
}

/// Cargo's progress chatter, dropped from `log_tail`.
fn is_noise(line: &str) -> bool {
    const NOISE: [&str; 12] = [
        "Compiling ",
        "Checking ",
        "Blocking waiting",
        "Finished ",
        "Running ",
        "Fresh ",
        "Downloaded ",
        "Downloading ",
        "Locking ",
        "Updating ",
        "Adding ",
        "Doc-tests ",
    ];
    let l = line.trim_start();
    l.is_empty() || NOISE.iter().any(|n| l.starts_with(n))
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !is_noise(l)).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: parameters until a final byte in @..~.
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let skip = path
                .file_name()
                .is_some_and(|n| n == "target" || n == ".git" || n == "node_modules");
            if !skip {
                walk(&path, f);
            }
        } else {
            f(&path);
        }
    }
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------------------
// JSON (hand-written to keep this crate dependency-free)

fn render_json(ok: bool, steps: &[Step], pretty: bool) -> String {
    let (nl, i1, i2, sp) = if pretty {
        ("\n", "  ", "    ", " ")
    } else {
        ("", "", "", "")
    };
    let mut s = String::new();
    let _ = write!(s, "{{{nl}{i1}\"ok\":{sp}{ok},{nl}{i1}\"steps\":{sp}[");
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{nl}{i2}{{\"name\":{sp}{},{sp}\"ok\":{sp}{},{sp}\"ms\":{sp}{},{sp}\"summary\":{sp}{},{sp}\"log_tail\":{sp}{}}}",
            json_str(step.name),
            step.ok,
            step.ms,
            json_str(&step.summary),
            json_str(&step.log_tail),
        );
    }
    if !steps.is_empty() {
        s.push_str(nl);
        s.push_str(i1);
    }
    let _ = write!(s, "]{nl}}}");
    s
}

fn json_str(v: &str) -> String {
    let mut s = String::with_capacity(v.len() + 2);
    s.push('"');
    for c in v.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escapes_control_and_quotes() {
        assert_eq!(json_str("a\"b\\c\nd\u{1}"), r#""a\"b\\c\nd\u0001""#);
    }

    #[test]
    fn json_shape() {
        let steps = [Step {
            name: "fmt",
            ok: true,
            ms: 12,
            summary: "all files formatted".into(),
            log_tail: String::new(),
        }];
        assert_eq!(
            render_json(true, &steps, false),
            r#"{"ok":true,"steps":[{"name":"fmt","ok":true,"ms":12,"summary":"all files formatted","log_tail":""}]}"#
        );
    }

    #[test]
    fn parses_libtest_counts_and_failures() {
        let text = "test a::b ... ok\ntest a::c ... FAILED\n\
                    test result: FAILED. 1 passed; 1 failed; 2 ignored; 0 measured; 0 filtered out\n\
                    test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s\n";
        let c = cargo_test_counts(text);
        assert_eq!((c.results, c.passed, c.failed, c.ignored), (2, 5, 1, 2));
        assert_eq!(cargo_test_failures(text), vec!["a::c".to_owned()]);
    }

    #[test]
    fn parses_nextest_summary_and_failures() {
        let text = "        FAIL [   0.004s] (2/3) npc_sim::core some_test\n\
                    ────────────\n     Summary [   1.234s] 3 tests run: 2 passed, 1 failed, 0 skipped\n\
                            FAIL [   0.004s] (2/3) npc_sim::core some_test\n";
        assert_eq!(
            nextest_summary(text).as_deref(),
            Some("3 tests run: 2 passed, 1 failed, 0 skipped")
        );
        assert_eq!(
            nextest_failures(text),
            vec!["npc_sim::core some_test".to_owned()]
        );
    }

    #[test]
    fn strips_ansi_and_noise() {
        assert_eq!(strip_ansi("\u{1b}[31merror\u{1b}[0m: x\r"), "error: x");
        assert_eq!(
            tail("   Compiling a\nreal 1\n\n   Finished x\nreal 2", 5),
            "real 1\nreal 2"
        );
    }

    #[test]
    fn quick_filter_excludes_slow_binaries() {
        let f = nextest_filter(true);
        assert!(f.contains("binary_id(xtask::arch_fitness)"));
        assert!(f.contains("binary_id(village_content::determinism)"));
        assert!(!nextest_filter(false).contains("determinism"));
    }
}
