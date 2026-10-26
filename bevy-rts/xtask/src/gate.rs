//! `cargo xtask gate <level>`: the one definition of verified (spec §12.1, ADR-0009).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::arch::{self, Role};
use crate::{
    api, change_class, changed, clippy_gen, cmd, hooks, owners, protected, scope, test_guard,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Level {
    Task,
    Pr,
    Nightly,
    Milestone,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Task => "task",
            Level::Pr => "pr",
            Level::Nightly => "nightly",
            Level::Milestone => "milestone",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Pass,
    Warn(String),
    Skip(String),
    /// Carries the failing command's output; the report shows its tail.
    Fail(String),
}

#[derive(Clone)]
pub struct Ctx {
    pub root: PathBuf,
    pub level: Level,
    /// The diff base as given (`--base`, `FL_GATE_BASE`, `.gate-base` or `main`), for messages.
    pub base: String,
    /// The commit the branch is diffed against: `git merge-base <base> HEAD`.
    pub base_rev: String,
    /// Packages touched since `base_rev`, sorted.
    pub touched: Vec<String>,
    /// Whether the diff since `base_rev` changes game content under `assets/` ([`changed::touches_content`]).
    pub content: bool,
    /// The task's track(s) from `--track`; the check-scope row falls back to `FL_TRACK`.
    pub tracks: Vec<String>,
    pub milestone: Option<String>,
}

pub struct Step {
    pub name: &'static str,
    pub levels: &'static [Level],
    pub run: fn(&Ctx) -> Outcome,
}

const ALL: &[Level] = &[Level::Task, Level::Pr, Level::Nightly, Level::Milestone];
const CHECKS: &[Level] = &[Level::Task, Level::Pr, Level::Milestone];
const MERGE: &[Level] = &[Level::Pr, Level::Milestone];
const DEVICE: &[Level] = &[Level::Nightly, Level::Milestone];
const BRANCH: &[Level] = &[Level::Task, Level::Pr];

/// The ordered gate table. Later tasks append rows.
pub fn steps() -> Vec<Step> {
    vec![
        Step {
            name: "fmt",
            levels: ALL,
            run: fmt,
        },
        Step {
            name: "clippy-drift",
            levels: ALL,
            run: clippy_drift,
        },
        Step {
            name: "claude-md",
            levels: ALL,
            run: claude_md,
        },
        Step {
            name: "check-scope",
            levels: BRANCH,
            run: check_scope,
        },
        Step {
            name: "protected",
            levels: BRANCH,
            run: protected,
        },
        Step {
            name: "test-guard",
            levels: BRANCH,
            run: test_guard,
        },
        Step {
            name: "clippy",
            levels: CHECKS,
            run: clippy,
        },
        Step {
            name: "arch-check",
            levels: CHECKS,
            run: arch_check,
        },
        Step {
            name: "test-headless",
            levels: ALL,
            run: test_headless,
        },
        Step {
            name: "test-client",
            levels: CHECKS,
            run: test_client,
        },
        Step {
            name: "api",
            levels: CHECKS,
            run: api_snapshots,
        },
        Step {
            name: "deny",
            levels: MERGE,
            run: deny,
        },
        Step {
            name: "data-check",
            levels: CHECKS,
            run: data_check,
        },
        Step {
            name: "gpu",
            levels: DEVICE,
            run: gpu,
        },
        Step {
            name: "brp",
            levels: DEVICE,
            run: brp,
        },
        Step {
            name: "task-time",
            levels: &[Level::Nightly],
            run: task_time,
        },
        Step {
            name: "perf-file",
            levels: &[Level::Milestone],
            run: perf_file,
        },
        Step {
            name: "change-class",
            levels: BRANCH,
            run: change_class,
        },
        Step {
            name: "golden-policy",
            levels: MERGE,
            run: golden_policy,
        },
    ]
}

/// The time budget of each level (ADR-0009, ruling M17). A level over budget prints a warning in its footer; only
/// the nightly `task-time` row fails.
const LEVEL_BUDGET: [(Level, Duration); 4] = [
    (Level::Task, Duration::from_secs(300)),
    (Level::Pr, Duration::from_secs(900)),
    (Level::Nightly, Duration::from_secs(5400)),
    (Level::Milestone, Duration::from_secs(3600)),
];
/// The task-level smoke set alone (touched = none) must run within this (the `task-time` row).
const SMOKE_BUDGET: Duration = Duration::from_secs(120);

/// The budget of `level`.
fn budget(level: Level) -> Duration {
    LEVEL_BUDGET
        .iter()
        .find(|(l, _)| *l == level)
        .map_or(Duration::MAX, |(_, d)| *d)
}
/// Lines of a failing command's output shown under its FAIL line.
const FAIL_TAIL: usize = 40;

pub struct GateReport {
    pub level: Level,
    pub rows: Vec<(&'static str, Outcome, Duration)>,
    pub total: Duration,
}

impl GateReport {
    pub fn passed(&self) -> bool {
        !self
            .rows
            .iter()
            .any(|(_, o, _)| matches!(o, Outcome::Fail(_)))
    }

    pub fn render(&self) -> String {
        let mut out: String = self
            .rows
            .iter()
            .map(|(n, o, d)| render_row(n, o, *d))
            .collect();
        out.push_str(&self.footer());
        out
    }

    /// The budget warning (if any) and the summary line.
    fn footer(&self) -> String {
        let secs = self.total.as_secs_f64().round() as u64;
        let mut out = String::new();
        let limit = budget(self.level);
        if self.total > limit {
            out.push_str(&format!(
                "WARN gate {} took {secs}s (budget {}s)\n",
                self.level.name(),
                limit.as_secs()
            ));
        }
        let failed: Vec<&str> = self
            .rows
            .iter()
            .filter(|(_, o, _)| matches!(o, Outcome::Fail(_)))
            .map(|(n, _, _)| *n)
            .collect();
        let level = self.level.name();
        if failed.is_empty() {
            out.push_str(&format!("gate {level}: PASS in {secs}s\n"));
        } else {
            out.push_str(&format!(
                "gate {level}: FAIL ({}) in {secs}s\n",
                failed.join(", ")
            ));
        }
        out
    }
}

/// One step's lines: `PASS fmt (2.1s)`, `WARN …`, `SKIP data-check: <reason>`, or `FAIL clippy (34.0s)` followed
/// by the last 40 lines of the command's output.
fn render_row(name: &str, outcome: &Outcome, d: Duration) -> String {
    let secs = d.as_secs_f64();
    match outcome {
        Outcome::Pass => format!("PASS {name} ({secs:.1}s)\n"),
        Outcome::Skip(reason) => format!("SKIP {name}: {reason}\n"),
        Outcome::Warn(msg) => format!("WARN {name} ({secs:.1}s): {}\n", indent_tail(msg).trim()),
        Outcome::Fail(output) => format!("FAIL {name} ({secs:.1}s)\n{}", indent_tail(output)),
    }
}

fn indent_tail(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let tail = &lines[lines.len().saturating_sub(FAIL_TAIL)..];
    tail.iter().map(|l| format!("    {l}\n")).collect()
}

/// The gate at `level` for the workspace at `root`: resolves the diff base (see [`changed::resolve_base`]) and the
/// touched packages, then runs every step of the level.
pub fn run(
    root: PathBuf,
    level: Level,
    base: Option<&str>,
    tracks: Vec<String>,
    milestone: Option<String>,
) -> GateReport {
    let times_root = root.clone();
    let report = run_with_setup(level, || {
        let meta = cmd::workspace(&root)?;
        let base = changed::resolve_base(&root, base)?;
        let files = changed::changed_files(&root, &base.rev)?;
        Ok(Ctx {
            touched: changed::touched_packages(meta, &files),
            content: changed::touches_content(&files),
            root,
            level,
            base: base.label(),
            base_rev: base.rev,
            tracks,
            milestone,
        })
    });
    let head = cmd::git(&times_root, &["rev-parse", "--short", "HEAD"])
        .map_or_else(|_| "unknown".to_owned(), |h| h.trim().to_owned());
    if let Err(e) = append_gate_times(&times_root, &report, &head) {
        eprintln!("warning: gate-times.jsonl not written: {e:#}");
    }
    report
}

/// Where [`append_gate_times`] writes, relative to the workspace root.
const GATE_TIMES: &str = "target/xtask/gate-times.jsonl";

/// Appends one JSON line `{"level","head","total_secs","rows":[{"name","secs","outcome"}]}` to
/// [`GATE_TIMES`], so gate time can be tracked across runs.
fn append_gate_times(root: &Path, report: &GateReport, head: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    let rows: Vec<serde_json::Value> = report
        .rows
        .iter()
        .map(|(name, outcome, d)| {
            let outcome = match outcome {
                Outcome::Pass => "pass",
                Outcome::Warn(_) => "warn",
                Outcome::Skip(_) => "skip",
                Outcome::Fail(_) => "fail",
            };
            serde_json::json!({"name": name, "secs": d.as_secs_f64(), "outcome": outcome})
        })
        .collect();
    let line = serde_json::json!({
        "level": report.level.name(),
        "head": head,
        "total_secs": report.total.as_secs_f64(),
        "rows": rows,
    });
    let path = root.join(GATE_TIMES);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Builds the context with `setup`, then runs every step of the level in order. A setup failure (cargo metadata,
/// merge base, …) is reported as a failed `setup` row, so the summary line is always printed.
#[expect(
    clippy::disallowed_types,
    reason = "the gate times its own steps; nothing here is sim state"
)]
pub fn run_with_setup(level: Level, setup: impl FnOnce() -> anyhow::Result<Ctx>) -> GateReport {
    let start = std::time::Instant::now();
    let ctx = match setup() {
        Ok(ctx) => ctx,
        Err(e) => {
            let d = start.elapsed();
            return GateReport {
                level,
                rows: vec![("setup", Outcome::Fail(format!("{e:#}")), d)],
                total: d,
            };
        }
    };
    let mut rows = Vec::new();
    for step in steps()
        .into_iter()
        .filter(|s| s.levels.contains(&ctx.level))
    {
        let t = std::time::Instant::now();
        let outcome = (step.run)(&ctx);
        let d = t.elapsed();
        rows.push((step.name, outcome, d));
    }
    GateReport {
        level,
        rows,
        total: start.elapsed(),
    }
}

// ---- package selection ----------------------------------------------------------------------------------------

/// Workspace packages other than probes, sorted, with their roles.
fn packages(root: &Path) -> Result<Vec<(String, Role)>, Outcome> {
    let meta = cmd::workspace(root).map_err(|e| Outcome::Fail(format!("{e:#}")))?;
    let mut out: Vec<(String, Role)> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| !arch::is_probe(meta, p))
        .map(|p| (p.name.to_string(), Role::of(&p.name)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Packages tested without any client crate in the build: pure, content, headless and xtask.
fn is_headless_test(role: &Role) -> bool {
    role.is_tree_checked() || *role == Role::Xtask
}

/// `--workspace --exclude <probe>…`: every package except the probes, as ONE feature-unified build. The task
/// level compiles only this set (ruling R23) so a fresh build dir builds Bevy once; it also checks every reverse
/// dependent of a touched crate.
fn workspace_selection(root: &Path) -> Result<Vec<String>, Outcome> {
    let meta = cmd::workspace(root).map_err(|e| Outcome::Fail(format!("{e:#}")))?;
    let mut sel = vec!["--workspace".to_owned()];
    let mut probes: Vec<String> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| arch::is_probe(meta, p))
        .map(|p| p.name.to_string())
        .collect();
    probes.sort();
    for p in probes {
        sel.extend(["--exclude".to_owned(), p]);
    }
    Ok(sel)
}

/// `package(=a) | package(=b) | …`
fn package_set(pkgs: &[String]) -> String {
    pkgs.iter()
        .map(|p| format!("package(={p})"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The tests a content change (`assets/**`) runs at task level (final review I3): every `fl_catalog` test (the base
/// content's counts and string tables) and `fl_test_utils`' golden replays, whose headers carry the rules hash.
const CONTENT_TESTS: &str = "package(=fl_catalog) | (package(=fl_test_utils) & binary(=golden))";

/// The task-level test filter: every test of the touched packages plus the `smoke_` tests of the smoke set and the
/// `content` tests, minus the exclusions. `None` when nothing is selected.
fn task_filter(
    touched: &[String],
    smoke: &[String],
    content: Option<&str>,
    excl: &str,
) -> Option<String> {
    let mut selected = Vec::new();
    if !touched.is_empty() {
        selected.push(package_set(touched));
    }
    if !smoke.is_empty() {
        selected.push(format!("(({}) & test(/smoke_/))", package_set(smoke)));
    }
    if let Some(content) = content {
        selected.push(content.to_owned());
    }
    if selected.is_empty() {
        return None;
    }
    Some(format!("({}) & not ({excl})", selected.join(" | ")))
}

fn p_args(pkgs: &[String]) -> Vec<String> {
    pkgs.iter()
        .flat_map(|p| ["-p".to_owned(), p.clone()])
        .collect()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// The nextest filter that every test run applies: device and BRP tests have their own rows, and `milestone_`
/// tests run at milestone level only.
fn exclusions(level: Level) -> &'static str {
    if level == Level::Milestone {
        "test(/gpu_/) | test(/brp_/)"
    } else {
        "test(/gpu_/) | test(/brp_/) | test(/milestone_/)"
    }
}

fn nextest_env(level: Level) -> Vec<(&'static str, String)> {
    let seeds = if level == Level::Task { "1" } else { "3" };
    let mut env = vec![("FL_SHUFFLE_SEEDS", seeds.to_owned())];
    if level == Level::Nightly {
        env.push(("PROPTEST_CASES", "4096".to_owned()));
    }
    env
}

/// `cargo nextest run --profile agent <selection> -E <filter>`. A run that matches no test passes with a warning,
/// so an empty filter is visible instead of silently green.
fn nextest(ctx: &Ctx, log: &str, selection: &[String], filter: &str) -> Outcome {
    let mut a = args(&["nextest", "run", "--profile", "agent", "--no-tests=warn"]);
    a.extend_from_slice(selection);
    a.extend(["-E".to_owned(), filter.to_owned()]);
    match cmd::cargo_step(&ctx.root, log, &a, &nextest_env(ctx.level)) {
        Outcome::Pass if no_tests_ran(&ctx.root, log) => no_match_outcome(ctx.level, log, filter),
        other => other,
    }
}

/// What a run that matched no test is: a warning for the task and pr rows, a failure for the device rows (`gpu`,
/// `brp`) and for every row at milestone level, where a renamed or `cfg`'d-out test would otherwise pass silently.
fn no_match_outcome(level: Level, log: &str, filter: &str) -> Outcome {
    let message = format!("no test matched `{filter}`");
    if matches!(log, "gpu" | "brp") || level == Level::Milestone {
        Outcome::Fail(message)
    } else {
        Outcome::Warn(message)
    }
}

fn no_tests_ran(root: &Path, log: &str) -> bool {
    std::fs::read_to_string(root.join("target").join("xtask").join(format!("{log}.log")))
        .is_ok_and(|text| text.contains("no tests to run"))
}

// ---- steps ------------------------------------------------------------------------------------------------------

fn fmt(ctx: &Ctx) -> Outcome {
    cmd::cargo_step(&ctx.root, "fmt", &args(&["fmt", "--all", "--check"]), &[])
}

fn clippy_drift(ctx: &Ctx) -> Outcome {
    match clippy_gen::drift(&ctx.root) {
        Ok(d) if d.is_empty() => Outcome::Pass,
        Ok(d) => Outcome::Fail(format!(
            "clippy configs drifted from {} or not generated by it:\n{}\nrun `cargo xtask gen-clippy` and \
             delete any clippy.toml it does not produce",
            clippy_gen::TEMPLATE_PATH,
            d.iter()
                .map(|p| format!("  {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// `CLAUDE.md` stays within its line limit (ADR-0013).
fn claude_md(ctx: &Ctx) -> Outcome {
    match std::fs::read_to_string(ctx.root.join(hooks::CLAUDE_MD)) {
        Ok(text) => hooks::claude_md_check(&text),
        Err(e) => Outcome::Fail(format!("read {}: {e}", hooks::CLAUDE_MD)),
    }
}

/// Clippy over the whole workspace minus probes at every level: one unified build, and a change to a crate is
/// checked against all its reverse dependents. Skipped at task level when nothing changed.
fn clippy(ctx: &Ctx) -> Outcome {
    if ctx.level == Level::Task && ctx.touched.is_empty() {
        return Outcome::Skip(format!("no package touched since {}", ctx.base));
    }
    let mut a = args(&["clippy"]);
    match workspace_selection(&ctx.root) {
        Ok(sel) => a.extend(sel),
        Err(o) => return o,
    }
    a.extend(args(&["--all-targets", "--", "-D", "warnings"]));
    cmd::cargo_step(&ctx.root, "clippy", &a, &[])
}

/// At most this many detail lines in a row's message, so the header survives the 40-line tail.
const DETAIL_LINES: usize = 38;

/// `header`, then up to [`DETAIL_LINES`] of `lines`, then a pointer to `full` for the rest.
fn capped(header: String, lines: &[String], full: &str) -> String {
    let mut out = header;
    for l in lines.iter().take(DETAIL_LINES) {
        out.push('\n');
        out.push_str(l.trim_end());
    }
    if lines.len() > DETAIL_LINES {
        out.push_str(&format!(
            "\n… and {} more (`{full}` lists all)",
            lines.len() - DETAIL_LINES
        ));
    }
    out
}

/// Fails when the branch touches paths outside its track(s) (`--track`, else `FL_TRACK`); skipped without a
/// track.
fn check_scope(ctx: &Ctx) -> Outcome {
    let tracks = scope::effective_tracks(&ctx.tracks, scope::fl_track_env().as_deref());
    if tracks.is_empty() {
        return Outcome::Skip("no --track given and FL_TRACK is unset".into());
    }
    match scope::run(&ctx.root, &ctx.base_rev, &tracks) {
        Ok(report) if report.outside.is_empty() => Outcome::Pass,
        Ok(report) => {
            let lines: Vec<String> = report.outside_lines().lines().map(str::to_owned).collect();
            Outcome::Fail(capped(
                format!(
                    "{} of {} changed files are outside track(s) {}:",
                    report.outside.len(),
                    report.files,
                    tracks.join(", ")
                ),
                &lines,
                "cargo xtask check-scope",
            ))
        }
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// Changed protected paths (ruling R34): any one fails at task level; at pr level each needs `Blessed:` commits.
fn protected(ctx: &Ctx) -> Outcome {
    match protected::run(&ctx.root, &ctx.base_rev, ctx.level == Level::Pr) {
        Ok((_, v)) if v.is_empty() => Outcome::Pass,
        Ok((n, v)) => {
            let lines: Vec<String> = v
                .iter()
                .map(|v| format!("  {}: {}", owners::normalize(&v.file), v.reason))
                .collect();
            Outcome::Fail(capped(
                format!("{} violations among {n} changed protected files:", v.len()),
                &lines,
                "git diff --name-only",
            ))
        }
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// Lists test-weakening changes since the merge base, minus the accepted ones (`xtask/test-guard-accepted.toml`);
/// any finding left is a warning the reviewer must accept.
fn test_guard(ctx: &Ctx) -> Outcome {
    match test_guard::run_accepted(&ctx.root, &ctx.base_rev) {
        Ok((f, _)) if f.is_empty() => Outcome::Pass,
        Ok((f, suppressed)) => {
            let lines: Vec<String> = f.iter().map(test_guard::Finding::render).collect();
            Outcome::Warn(capped(
                test_guard::summary(f.len(), suppressed),
                &lines,
                "cargo xtask test-guard",
            ))
        }
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

fn arch_check(ctx: &Ctx) -> Outcome {
    match arch::check(&ctx.root) {
        Ok((_, v)) if v.is_empty() => Outcome::Pass,
        Ok((_, v)) => Outcome::Fail(
            v.iter()
                .map(|v| format!("{}: {}", v.krate, v.detail))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// The tests the task level leaves to pr and above (ruling M17): `xtask/heavy-tests.txt`.
const HEAVY_TESTS: &str = "xtask/heavy-tests.txt";

/// `(package(=p) & binary(=b) & test(=t)) | …` over the lines of [`HEAVY_TESTS`] (`<package> <binary> <test>`;
/// `#` starts a comment; blank lines are ignored); `None` when the file lists no test or does not exist.
pub fn heavy_filter(root: &Path) -> anyhow::Result<Option<String>> {
    match std::fs::read_to_string(root.join(HEAVY_TESTS)) {
        Ok(text) => parse_heavy(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context(format!("read {HEAVY_TESTS}"))),
    }
}

/// One line of [`HEAVY_TESTS`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct HeavyEntry {
    package: String,
    binary: String,
    test: String,
}

fn parse_heavy_entries(text: &str) -> anyhow::Result<Vec<HeavyEntry>> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.as_slice() {
            [] => {}
            [package, binary, test] => entries.push(HeavyEntry {
                package: (*package).to_owned(),
                binary: (*binary).to_owned(),
                test: (*test).to_owned(),
            }),
            _ => anyhow::bail!(
                "{HEAVY_TESTS}:{}: expected `<package> <binary> <test>`, found {} field(s)",
                i + 1,
                fields.len()
            ),
        }
    }
    Ok(entries)
}

/// The nextest expression of `entries`; `None` for no entry.
fn heavy_expression(entries: &[HeavyEntry]) -> Option<String> {
    let terms: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "(package(={}) & binary(={}) & test(={}))",
                e.package, e.binary, e.test
            )
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" | "))
}

fn parse_heavy(text: &str) -> anyhow::Result<Option<String>> {
    Ok(heavy_expression(&parse_heavy_entries(text)?))
}

/// The entries (as `<package> <binary> <test>`) that no line of a `cargo nextest list --message-format oneline`
/// output names (`<binary-id> <test>`, the binary id being `<package>::<binary>`, or the package alone for its
/// library tests).
fn stale_heavy(entries: &[HeavyEntry], list: &str) -> Vec<String> {
    let lines: Vec<Vec<&str>> = list
        .lines()
        .map(|l| l.split_whitespace().collect())
        .collect();
    entries
        .iter()
        .filter(|e| {
            let id = format!("{}::{}", e.package, e.binary);
            !lines.iter().any(|l| {
                l.len() == 2
                    && l[1] == e.test
                    && (l[0] == id || (l[0] == e.package && e.binary == e.package))
            })
        })
        .map(|e| format!("{} {} {}", e.package, e.binary, e.test))
        .collect()
}

/// Fails when an entry of [`HEAVY_TESTS`] matches no test (a renamed test would otherwise run at task level again,
/// a typo would exclude nothing). `None` when every entry matches or there is no list.
fn stale_heavy_outcome(ctx: &Ctx) -> Option<Outcome> {
    let entries = match std::fs::read_to_string(ctx.root.join(HEAVY_TESTS)) {
        Ok(text) => match parse_heavy_entries(&text) {
            Ok(e) => e,
            Err(e) => return Some(Outcome::Fail(format!("{e:#}"))),
        },
        Err(_) => return None,
    };
    let expr = heavy_expression(&entries)?;
    let mut a = args(&["nextest", "list", "--message-format", "oneline"]);
    match workspace_selection(&ctx.root) {
        Ok(sel) => a.extend(sel),
        Err(o) => return Some(o),
    }
    a.extend(["-E".to_owned(), expr]);
    match cmd::run_logged(&ctx.root, "heavy-list", "cargo", &a, &[]) {
        Ok((true, text, _)) => {
            let stale = stale_heavy(&entries, &text);
            (!stale.is_empty()).then(|| {
                Outcome::Fail(format!(
                    "{HEAVY_TESTS} lists tests that do not exist (renamed or mistyped):
  {}",
                    stale.join(
                        "
  "
                    )
                ))
            })
        }
        Ok((false, text, path)) => Some(Outcome::Fail(format!(
            "{}
(full log: {})",
            text.trim_end(),
            path.display()
        ))),
        Err(e) => Some(Outcome::Fail(format!("{e:#}"))),
    }
}

/// pr, nightly, milestone: the headless packages built and tested on their own, so the sim proves it builds
/// without client code. task: the unified workspace build, running the touched headless packages' tests plus the
/// `smoke_` tests of `fl_sim` and `fl_test_utils`, and the [`CONTENT_TESTS`] when `assets/` changed (arch-check's
/// per-package trees prove isolation at this level). The task level leaves out the [`HEAVY_TESTS`].
fn test_headless(ctx: &Ctx) -> Outcome {
    let outcome = headless_run(ctx, "test-headless", &ctx.touched);
    if ctx.level == Level::Task
        && !matches!(outcome, Outcome::Fail(_))
        && let Some(stale) = stale_heavy_outcome(ctx)
    {
        return stale;
    }
    outcome
}

/// [`test_headless`] with `touched` standing in for the touched packages, logging to `log`.
fn headless_run(ctx: &Ctx, log: &str, touched: &[String]) -> Outcome {
    let all = match packages(&ctx.root) {
        Ok(p) => p,
        Err(o) => return o,
    };
    let headless: Vec<String> = all
        .into_iter()
        .filter(|(_, r)| is_headless_test(r))
        .map(|(n, _)| n)
        .collect();
    let heavy = match heavy_filter(&ctx.root) {
        Ok(h) => h,
        Err(e) => return Outcome::Fail(format!("{e:#}")),
    };
    let Some(filter) =
        headless_filter(ctx.level, &headless, touched, ctx.content, heavy.as_deref())
    else {
        return Outcome::Skip("no headless package touched and no smoke set".into());
    };
    let selection = if ctx.level == Level::Task {
        match workspace_selection(&ctx.root) {
            Ok(sel) => sel,
            Err(o) => return o,
        }
    } else {
        p_args(&headless)
    };
    nextest(ctx, log, &selection, &filter)
}

/// The `test-headless` filter at `level`: above task every non-excluded test (the heavy ones included); at task
/// [`task_headless_filter`].
fn headless_filter(
    level: Level,
    headless: &[String],
    touched: &[String],
    content: bool,
    heavy: Option<&str>,
) -> Option<String> {
    let excl = exclusions(level);
    if level == Level::Task {
        task_headless_filter(headless, touched, content, excl, heavy)
    } else {
        Some(format!("not ({excl})"))
    }
}

/// The task-level `test-headless` filter over the `headless` packages: the touched ones, the `smoke_` tests of
/// `fl_sim` and `fl_test_utils`, and with a `content` change the [`CONTENT_TESTS`]; then minus the `heavy` tests.
fn task_headless_filter(
    headless: &[String],
    touched: &[String],
    content: bool,
    excl: &str,
    heavy: Option<&str>,
) -> Option<String> {
    let touched: Vec<String> = headless
        .iter()
        .filter(|n| touched.contains(n))
        .cloned()
        .collect();
    let smoke: Vec<String> = ["fl_sim", "fl_test_utils"]
        .into_iter()
        .filter(|s| headless.iter().any(|n| n == s))
        .map(str::to_owned)
        .collect();
    let content = (content && headless.iter().any(|n| n == "fl_catalog")).then_some(CONTENT_TESTS);
    let filter = task_filter(&touched, &smoke, content, excl)?;
    Some(match heavy {
        Some(h) => format!("{filter} & not ({h})"),
        None => filter,
    })
}

/// pr, milestone: the client packages built and tested on their own. task: the unified workspace build, running
/// the touched client packages' tests; skipped when none is touched.
fn test_client(ctx: &Ctx) -> Outcome {
    let all = match packages(&ctx.root) {
        Ok(p) => p,
        Err(o) => return o,
    };
    let client: Vec<String> = all
        .into_iter()
        .filter(|(_, r)| r.is_client())
        .map(|(n, _)| n)
        .collect();
    let excl = exclusions(ctx.level);
    if ctx.level != Level::Task {
        return nextest(
            ctx,
            "test-client",
            &p_args(&client),
            &format!("not ({excl})"),
        );
    }
    let touched: Vec<String> = client
        .into_iter()
        .filter(|n| ctx.touched.contains(n))
        .collect();
    let Some(filter) = task_filter(&touched, &[], None, excl) else {
        return Outcome::Skip(format!("no client package touched since {}", ctx.base));
    };
    match workspace_selection(&ctx.root) {
        Ok(sel) => nextest(ctx, "test-client", &sel, &filter),
        Err(o) => o,
    }
}

/// The public API of every [`api::API_CRATES`] crate against its committed snapshot (ADR-0011). Skipped at task
/// level unless an API crate is touched.
fn api_snapshots(ctx: &Ctx) -> Outcome {
    if ctx.level == Level::Task && !touches_api_crate(&ctx.touched) {
        return Outcome::Skip(format!("no API crate touched since {}", ctx.base));
    }
    match api::run(&ctx.root, false) {
        Ok(statuses) if statuses.iter().all(|(_, s)| *s == api::ApiStatus::Same) => Outcome::Pass,
        Ok(statuses) => Outcome::Fail(api::render(&statuses, false)),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

fn touches_api_crate(touched: &[String]) -> bool {
    touched
        .iter()
        .any(|p| api::API_CRATES.contains(&p.as_str()))
}

fn deny(ctx: &Ctx) -> Outcome {
    cmd::cargo_step(&ctx.root, "deny", &args(&["deny", "check"]), &[])
}

fn data_check(ctx: &Ctx) -> Outcome {
    let needed = ["crates/fl_cli/src/data_check.rs", "assets/base/mod.ron"];
    if let Some(missing) = needed.iter().find(|p| !ctx.root.join(p).exists()) {
        return Outcome::Skip(format!("{missing} does not exist yet"));
    }
    let a = args(&[
        "run",
        "-q",
        "-p",
        "fl_cli",
        "--",
        "data-check",
        "assets/base",
    ]);
    cmd::cargo_step(&ctx.root, "data-check", &a, &[])
}

/// The features the device rows build with: the remote debug endpoint of the client and the game, so the
/// `brp_*` and BRP-driven `gpu_*` tests compile (ruling R68).
const DEVICE_FEATURES: &str = "fl_game/debug_remote,fl_client/debug_remote";

/// The gpu row's selection: the workspace with [`DEVICE_FEATURES`], without the probes, which are
/// historical and may not compile (ruling R43).
fn gpu_selection() -> Vec<String> {
    args(&[
        "--workspace",
        "--exclude",
        "probe_*",
        "--features",
        DEVICE_FEATURES,
    ])
}

/// The brp row's selection: the client and the game with [`DEVICE_FEATURES`].
fn brp_selection() -> Vec<String> {
    args(&[
        "-p",
        "fl_client",
        "-p",
        "fl_game",
        "--features",
        DEVICE_FEATURES,
    ])
}

fn gpu(ctx: &Ctx) -> Outcome {
    nextest(ctx, "gpu", &gpu_selection(), "test(/gpu_/)")
}

fn brp(ctx: &Ctx) -> Outcome {
    nextest(ctx, "brp", &brp_selection(), "test(/brp_/)")
}

/// Nightly: the task-level `test-headless` selection must stay fast (ruling M17). Builds it once (untimed), then
/// times only the test execution twice: with no package touched (the smoke set alone, within [`SMOKE_BUDGET`]) and
/// with `fl_sim` and `fl_test_utils` touched (within the task level's budget). Neither run gets the content
/// tests: the budgets are for code changes.
#[expect(
    clippy::disallowed_types,
    reason = "the gate times its own steps; nothing here is sim state"
)]
fn task_time(ctx: &Ctx) -> Outcome {
    let task_ctx = Ctx {
        level: Level::Task,
        content: false,
        ..ctx.clone()
    };
    let mut build = args(&["nextest", "run", "--profile", "agent", "--no-run"]);
    match workspace_selection(&ctx.root) {
        Ok(sel) => build.extend(sel),
        Err(o) => return o,
    }
    if let Outcome::Fail(m) = cmd::cargo_step(
        &ctx.root,
        "task-time-build",
        &build,
        &nextest_env(Level::Task),
    ) {
        return Outcome::Fail(format!(
            "build failed:
{m}"
        ));
    }
    let run = |log: &str, touched: &[String]| {
        let t = std::time::Instant::now();
        (headless_run(&task_ctx, log, touched), t.elapsed())
    };
    let smoke = run("task-time-smoke", &[]);
    let touched = ["fl_sim".to_owned(), "fl_test_utils".to_owned()];
    let full = run("task-time-touched", &touched);
    judge_task_time(smoke, full)
}

/// A run that failed fails the row; one that ran no test (`Skip`, `Warn`) makes it a `Warn`, since its time says
/// nothing; otherwise [`check_task_times`].
fn judge_task_time(smoke: (Outcome, Duration), full: (Outcome, Duration)) -> Outcome {
    for (label, (outcome, _)) in [("smoke", &smoke), ("touched", &full)] {
        match outcome {
            Outcome::Fail(m) => {
                return Outcome::Fail(format!(
                    "{label} run failed:
{m}"
                ));
            }
            Outcome::Skip(m) | Outcome::Warn(m) => {
                return Outcome::Warn(format!("{label} run ran no tests ({m}); times not judged"));
            }
            Outcome::Pass => {}
        }
    }
    check_task_times(smoke.1, full.1)
}

/// Fails when the smoke set alone took over [`SMOKE_BUDGET`] or the touched run over the task budget; the message
/// names both times.
fn check_task_times(smoke: Duration, touched: Duration) -> Outcome {
    let task = budget(Level::Task);
    if smoke > SMOKE_BUDGET || touched > task {
        Outcome::Fail(format!(
            "task-level tests too slow: smoke set {:.0}s (budget {}s), fl_sim+fl_test_utils touched {:.0}s \
             (budget {}s); move the slowest tests to {HEAVY_TESTS}",
            smoke.as_secs_f64(),
            SMOKE_BUDGET.as_secs(),
            touched.as_secs_f64(),
            task.as_secs()
        ))
    } else {
        Outcome::Pass
    }
}

fn perf_file(ctx: &Ctx) -> Outcome {
    let Some(m) = &ctx.milestone else {
        return Outcome::Fail("the milestone level needs --milestone <M>".into());
    };
    let path = format!("docs/perf/{m}.json");
    match std::fs::read_to_string(ctx.root.join(&path)) {
        Ok(text) => check_perf_json(&path, &text),
        Err(_) => Outcome::Fail(format!("{path} does not exist")),
    }
}

/// Gate G7: the mean tick at 3,000 units must be at most this many milliseconds.
const G7_MEAN_MS: f64 = 15.0;
const G7_UNITS: u64 = 3000;

/// `docs/perf/M<n>.json`: it must have a `rows` entry for 3,000 units whose `mean_ms` is within [`G7_MEAN_MS`].
fn check_perf_json(path: &str, text: &str) -> Outcome {
    let json: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Outcome::Fail(format!("{path} is not valid JSON: {e}")),
    };
    let row = json["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["units"].as_u64() == Some(G7_UNITS)));
    let Some(mean) = row.and_then(|r| r["mean_ms"].as_f64()) else {
        return Outcome::Fail(format!(
            "{path} has no `rows` entry with units = {G7_UNITS} and a numeric `mean_ms`"
        ));
    };
    if mean > G7_MEAN_MS {
        Outcome::Fail(format!(
            "{path}: mean tick at {G7_UNITS} units is {mean:.3} ms, over the {G7_MEAN_MS} ms budget (G7)"
        ))
    } else {
        Outcome::Pass
    }
}

/// Every commit since the base that touches code, content or the lock file carries a known `Change-Class:` trailer,
/// class F and D an `MCR:` naming a changelog heading, and class R and E change no golden replay outside the commit
/// that changes the replay format (ADR-0021, R151).
fn change_class(ctx: &Ctx) -> Outcome {
    match change_class::check_commits(&ctx.root, &ctx.base_rev) {
        Ok(v) if v.is_empty() => Outcome::Pass,
        Ok(v) => Outcome::Fail(capped(
            format!("{} change-class violations since {}:", v.len(), ctx.base),
            &v,
            "git log --format=%B",
        )),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// Golden replays change only with an MCR and a `SIM_VERSION`, format or content change; a `SIM_VERSION` bump changes
/// a golden; a class R or E commit keeps every golden's hash stream equal to its parent's (ADR-0021, R153).
fn golden_policy(ctx: &Ctx) -> Outcome {
    match change_class::check_branch(&ctx.root, &ctx.base_rev) {
        Ok(v) if v.is_empty() => Outcome::Pass,
        Ok(v) => Outcome::Fail(capped(
            format!("{} golden-policy violations since {}:", v.len(), ctx.base),
            &v,
            "git log --stat",
        )),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_rows_build_with_the_remote_feature() {
        let gpu = gpu_selection();
        assert!(
            gpu.windows(2).any(|w| w == ["--exclude", "probe_*"]),
            "{gpu:?}"
        );
        assert!(
            gpu.windows(2).any(|w| w == ["--features", DEVICE_FEATURES]),
            "{gpu:?}"
        );
        let brp = brp_selection();
        assert!(brp.windows(2).any(|w| w == ["-p", "fl_client"]), "{brp:?}");
        assert!(brp.windows(2).any(|w| w == ["-p", "fl_game"]), "{brp:?}");
        assert!(
            brp.windows(2).any(|w| w == ["--features", DEVICE_FEATURES]),
            "{brp:?}"
        );
        for feature in DEVICE_FEATURES.split(',') {
            assert!(feature.ends_with("/debug_remote"), "{feature}");
        }
    }

    #[test]
    fn empty_match_fails_device_and_milestone_rows() {
        let is_fail = |level, log| matches!(no_match_outcome(level, log, "f"), Outcome::Fail(_));
        assert!(is_fail(Level::Nightly, "gpu"));
        assert!(is_fail(Level::Milestone, "brp"));
        assert!(is_fail(Level::Milestone, "test-headless"));
        assert!(is_fail(Level::Milestone, "test-client"));
        assert!(matches!(
            no_match_outcome(Level::Pr, "test-client", "f"),
            Outcome::Warn(_)
        ));
        assert!(matches!(
            no_match_outcome(Level::Task, "test-headless", "f"),
            Outcome::Warn(_)
        ));
    }

    #[test]
    fn perf_file_checks_the_3000_unit_mean() {
        let json = |mean: f64| {
            format!(
                r#"{{"rows":[{{"units":500,"mean_ms":0.5}},{{"units":3000,"mean_ms":{mean}}}]}}"#
            )
        };
        assert_eq!(check_perf_json("M0.json", &json(4.4)), Outcome::Pass);
        assert_eq!(check_perf_json("M0.json", &json(15.0)), Outcome::Pass);
        let Outcome::Fail(m) = check_perf_json("M0.json", &json(15.5)) else {
            panic!("15.5 ms must fail");
        };
        assert!(m.contains("15.500 ms"), "{m}");
        assert!(matches!(
            check_perf_json("M0.json", r#"{"rows":[{"units":500,"mean_ms":1.0}]}"#),
            Outcome::Fail(_)
        ));
        assert!(matches!(
            check_perf_json("M0.json", "nope"),
            Outcome::Fail(_)
        ));
    }

    #[test]
    fn committed_m0_baseline_passes_g7() {
        let text = include_str!("../../docs/perf/M0.json");
        assert_eq!(check_perf_json("docs/perf/M0.json", text), Outcome::Pass);
    }

    #[test]
    fn report_renders_summary_line() {
        let report = GateReport {
            level: Level::Task,
            rows: vec![
                ("fmt", Outcome::Pass, Duration::from_millis(1000)),
                (
                    "clippy",
                    Outcome::Fail("error: x".into()),
                    Duration::from_millis(2000),
                ),
            ],
            total: Duration::from_secs(3),
        };
        assert!(!report.passed());
        let out = report.render();
        assert!(out.contains("PASS fmt (1.0s)\n"), "{out}");
        assert!(out.contains("FAIL clippy (2.0s)\n"), "{out}");
        assert_eq!(out.lines().last(), Some("gate task: FAIL (clippy) in 3s"));
    }

    #[test]
    fn setup_failure_still_prints_summary() {
        let report = run_with_setup(Level::Pr, || {
            anyhow::bail!("git merge-base main HEAD failed")
        });
        assert!(!report.passed());
        let out = report.render();
        assert!(out.starts_with("FAIL setup ("), "{out}");
        assert!(
            out.contains("    git merge-base main HEAD failed\n"),
            "{out}"
        );
        assert_eq!(out.lines().last(), Some("gate pr: FAIL (setup) in 0s"));
    }

    #[test]
    fn task_filter_selects_touched_and_smoke() {
        let excl = exclusions(Level::Task);
        let touched = ["fl_view".to_owned(), "xtask".to_owned()];
        let smoke = ["fl_sim".to_owned(), "fl_test_utils".to_owned()];
        assert_eq!(
            task_filter(&touched, &smoke, None, excl).as_deref(),
            Some(
                "(package(=fl_view) | package(=xtask) | ((package(=fl_sim) | package(=fl_test_utils)) \
                 & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))"
            )
        );
        assert_eq!(
            task_filter(&[], &[], None, excl),
            None,
            "nothing selected means the step is skipped"
        );
    }

    #[test]
    fn content_change_runs_catalog_and_golden_tests_at_task_level() {
        // Final review I3: `assets/**` lies under no package, yet it decides the base content's
        // counts and the golden replays' rules hash.
        let meta = crate::fixture::metadata("metadata_ok.json");
        let files = [PathBuf::from("assets/base/rules.ron")];
        let touched = changed::touched_packages(&meta, &files);
        assert!(touched.is_empty(), "{touched:?}");
        assert!(changed::touches_content(&files));
        let excl = exclusions(Level::Task);
        let headless = ["fl_catalog", "fl_sim", "fl_test_utils", "xtask"].map(str::to_owned);
        assert_eq!(
            task_headless_filter(&headless, &touched, true, excl, None).as_deref(),
            Some(
                "(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/)) | package(=fl_catalog) \
                 | (package(=fl_test_utils) & binary(=golden))) & not (test(/gpu_/) | test(/brp_/) | \
                 test(/milestone_/))"
            )
        );
        // Without a content change only the smoke set runs.
        assert_eq!(
            task_headless_filter(&headless, &touched, false, excl, None).as_deref(),
            Some(
                "(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not \
                 (test(/gpu_/) | test(/brp_/) | test(/milestone_/))"
            )
        );
        let code = [
            PathBuf::from("crates/fl_sim/src/x.rs"),
            PathBuf::from("docs/assets.md"),
        ];
        assert!(!changed::touches_content(&code));
    }

    #[test]
    fn fail_tail_is_capped_and_budget_warned() {
        let output: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        let report = GateReport {
            level: Level::Task,
            rows: vec![("clippy", Outcome::Fail(output), Duration::from_secs(301))],
            total: Duration::from_secs(301),
        };
        let out = report.render();
        assert!(!out.contains("line 60\n"), "{out}");
        assert!(
            out.contains("    line 61\n") && out.contains("    line 100\n"),
            "{out}"
        );
        assert!(
            out.contains("WARN gate task took 301s (budget 300s)\n"),
            "{out}"
        );
        assert_eq!(out.lines().last(), Some("gate task: FAIL (clippy) in 301s"));
    }

    #[test]
    fn capped_detail_keeps_header_in_the_tail() {
        let lines: Vec<String> = (1..=50).map(|i| format!("  f{i}.rs → client")).collect();
        let report = GateReport {
            level: Level::Task,
            rows: vec![(
                "check-scope",
                Outcome::Fail(capped(
                    "50 files outside".into(),
                    &lines,
                    "cargo xtask check-scope",
                )),
                Duration::ZERO,
            )],
            total: Duration::ZERO,
        };
        let out = report.render();
        assert!(out.contains("    50 files outside\n"), "{out}");
        assert!(
            out.contains("      f38.rs → client\n") && !out.contains("f39.rs"),
            "{out}"
        );
        assert!(
            out.contains("    … and 12 more (`cargo xtask check-scope` lists all)\n"),
            "{out}"
        );
    }

    const HEAVY_LINE: &str = "fl_sim flow group_move_uses_one_flow_request";
    const HEAVY_EXPR: &str =
        "(package(=fl_sim) & binary(=flow) & test(=group_move_uses_one_flow_request))";

    #[test]
    fn heavy_filter_lists_each_test() {
        let text = format!("# header\n\n{HEAVY_LINE} # 31 s\n");
        assert_eq!(parse_heavy(&text).unwrap().as_deref(), Some(HEAVY_EXPR));
        let two = format!("{HEAVY_LINE}\nfl_cli cli scale_a\n");
        let want = format!("{HEAVY_EXPR} | (package(=fl_cli) & binary(=cli) & test(=scale_a))");
        assert_eq!(parse_heavy(&two).unwrap().as_deref(), Some(want.as_str()));
        assert_eq!(parse_heavy("# only a comment\n").unwrap(), None);
    }

    #[test]
    fn heavy_line_needs_three_fields() {
        let err = parse_heavy("fl_sim flow\n").unwrap_err().to_string();
        assert!(err.contains("heavy-tests.txt:1"), "{err}");
        let err = parse_heavy("# c\n\na b c d\n").unwrap_err().to_string();
        assert!(err.contains("heavy-tests.txt:3"), "{err}");
    }

    #[test]
    fn committed_heavy_list_parses() {
        let text = include_str!("../heavy-tests.txt");
        assert!(parse_heavy(text).unwrap().is_some());
    }

    #[test]
    fn task_filter_excludes_heavy() {
        let excl = exclusions(Level::Task);
        let headless = ["fl_sim", "fl_test_utils"].map(str::to_owned);
        let f = task_headless_filter(&headless, &[], false, excl, Some(HEAVY_EXPR)).unwrap();
        assert!(f.ends_with(&format!("& not ({HEAVY_EXPR})")), "{f}");
    }

    #[test]
    fn pr_runs_heavy_tests() {
        let headless = ["fl_sim", "fl_test_utils"].map(str::to_owned);
        let f = headless_filter(Level::Pr, &headless, &[], false, Some(HEAVY_EXPR)).unwrap();
        assert!(!f.contains(HEAVY_EXPR), "{f}");
        let t = headless_filter(Level::Task, &headless, &[], false, Some(HEAVY_EXPR)).unwrap();
        assert!(t.contains(HEAVY_EXPR), "{t}");
    }

    #[test]
    fn task_time_row_is_nightly_only() {
        let step = steps().into_iter().find(|s| s.name == "task-time").unwrap();
        assert_eq!(step.levels, [Level::Nightly]);
    }

    #[test]
    fn change_class_and_golden_policy_rows() {
        let level = |name: &str| steps().into_iter().find(|s| s.name == name).unwrap().levels;
        assert_eq!(level("change-class"), [Level::Task, Level::Pr]);
        assert_eq!(level("golden-policy"), [Level::Pr, Level::Milestone]);
    }

    #[test]
    fn task_time_fails_over_either_budget() {
        let secs = Duration::from_secs;
        assert_eq!(check_task_times(secs(100), secs(250)), Outcome::Pass);
        for (smoke, touched) in [(121, 250), (100, 301)] {
            let Outcome::Fail(m) = check_task_times(secs(smoke), secs(touched)) else {
                panic!("{smoke}/{touched} must fail");
            };
            assert!(
                m.contains(&format!("smoke set {smoke}s"))
                    && m.contains(&format!("touched {touched}s")),
                "{m}"
            );
        }
    }

    #[test]
    fn level_budget_warns() {
        let report = GateReport {
            level: Level::Pr,
            rows: Vec::new(),
            total: Duration::from_secs(1000),
        };
        let out = report.render();
        assert!(
            out.contains("WARN gate pr took 1000s (budget 900s)\n"),
            "{out}"
        );
        let ok = GateReport {
            total: Duration::from_secs(900),
            ..report
        };
        assert!(!ok.render().contains("WARN"), "{}", ok.render());
    }

    #[test]
    fn gate_times_appended() {
        let dir = tempfile::tempdir().unwrap();
        let report = GateReport {
            level: Level::Task,
            rows: vec![("fmt", Outcome::Pass, Duration::from_millis(1500))],
            total: Duration::from_secs(2),
        };
        append_gate_times(dir.path(), &report, "abc1234").unwrap();
        append_gate_times(dir.path(), &report, "abc1234").unwrap();
        let text = std::fs::read_to_string(dir.path().join(GATE_TIMES)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for l in lines {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            assert_eq!(v["level"], "task");
            assert_eq!(v["head"], "abc1234");
            assert_eq!(v["rows"][0]["name"], "fmt");
            assert_eq!(v["rows"][0]["outcome"], "pass");
        }
    }

    #[test]
    fn summary_counts_suppressed() {
        let s = test_guard::summary(2, 3);
        assert!(
            s.contains("2 findings (3 accepted findings suppressed)"),
            "{s}"
        );
        assert_eq!(
            test_guard::summary(2, 0),
            "test-guard: 2 findings (reviewer must accept each)"
        );
    }

    #[test]
    fn content_tests_survive_the_heavy_filter() {
        let excl = exclusions(Level::Task);
        let headless = ["fl_catalog", "fl_sim", "fl_test_utils"].map(str::to_owned);
        let f = task_headless_filter(&headless, &[], true, excl, Some(HEAVY_EXPR)).unwrap();
        assert!(f.contains(CONTENT_TESTS), "{f}");
        assert!(f.ends_with(&format!("& not ({HEAVY_EXPR})")), "{f}");
    }

    #[test]
    fn committed_heavy_list_leaves_the_content_tests_alone() {
        let entries = parse_heavy_entries(include_str!("../heavy-tests.txt")).unwrap();
        for e in entries {
            assert_ne!(e.package, "fl_catalog", "{e:?} would drop a content test");
            assert!(
                !(e.package == "fl_test_utils" && e.binary == "golden"),
                "{e:?} would drop a golden replay test"
            );
        }
    }

    #[test]
    fn stale_heavy_entries_are_reported() {
        let entries =
            parse_heavy_entries("fl_sim flow a_test\nfl_sim flow gone_test\nfl_cli cli x\n")
                .unwrap();
        let list = "fl_sim::flow a_test\nfl_sim::flow other_test\nfl_cli::cli x\n";
        let stale = stale_heavy(&entries, list);
        assert_eq!(stale, ["fl_sim flow gone_test"]);
        assert!(stale_heavy(&entries[..1], list).is_empty());
    }

    #[test]
    fn task_time_judges_skips_and_failures() {
        let secs = Duration::from_secs;
        let ok = || (Outcome::Pass, secs(10));
        assert_eq!(judge_task_time(ok(), ok()), Outcome::Pass);
        let skipped = (Outcome::Skip("nothing".into()), secs(0));
        assert!(matches!(
            judge_task_time(skipped, ok()),
            Outcome::Warn(m) if m.contains("nothing")
        ));
        let warned = (Outcome::Warn("no test matched".into()), secs(0));
        assert!(matches!(judge_task_time(ok(), warned), Outcome::Warn(_)));
        let failed = (Outcome::Fail("boom".into()), secs(1));
        assert!(matches!(judge_task_time(failed, ok()), Outcome::Fail(m) if m.contains("boom")));
        let slow = (Outcome::Pass, secs(301));
        assert!(matches!(judge_task_time(ok(), slow), Outcome::Fail(_)));
    }

    #[test]
    fn api_row_runs_only_for_api_crates() {
        assert!(touches_api_crate(&[
            "fl_cli".to_owned(),
            "fl_sim".to_owned()
        ]));
        assert!(!touches_api_crate(&[
            "fl_cli".to_owned(),
            "xtask".to_owned()
        ]));
        let ctx = Ctx {
            root: PathBuf::from("."),
            level: Level::Task,
            base: "main".into(),
            base_rev: "HEAD".into(),
            touched: vec!["fl_cli".into()],
            content: false,
            tracks: Vec::new(),
            milestone: None,
        };
        assert_eq!(
            api_snapshots(&ctx),
            Outcome::Skip("no API crate touched since main".into())
        );
    }

    #[test]
    fn skips_and_warnings_still_pass() {
        let report = GateReport {
            level: Level::Pr,
            rows: vec![
                (
                    "data-check",
                    Outcome::Skip("not yet".into()),
                    Duration::ZERO,
                ),
                (
                    "test-guard",
                    Outcome::Warn("1 finding".into()),
                    Duration::from_millis(240),
                ),
            ],
            total: Duration::from_millis(400),
        };
        assert!(report.passed());
        let out = report.render();
        assert!(out.contains("SKIP data-check: not yet\n"), "{out}");
        assert!(out.contains("WARN test-guard (0.2s): 1 finding\n"), "{out}");
        assert_eq!(out.lines().last(), Some("gate pr: PASS in 0s"));
    }
}
