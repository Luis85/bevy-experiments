//! `cargo xtask test-guard`: flags changes that weaken tests, for the reviewer to accept one by one (spec §12.5,
//! ADR-0009).
//!
//! `git diff -U0 <base> -- '*.rs'` (plus untracked `.rs` files) says which lines changed. Each changed file is
//! then lexed whole on both sides — the base version (`git show <base>:<path>`) and the working tree — so string
//! state, multi-line asserts and attribute stacks never depend on how much context a hunk shows. Every `.rs` file
//! is scanned, not only `tests/`, because unit tests live in `src/`. Comments are ignored and literals are
//! opaque. Findings on removed code cite the base line number; findings on added code cite the working-tree line.
//!
//! The checks are textual and err towards reporting:
//! - `RemovedAssert`: an assert-like macro (`assert!`, `assert_eq!`, `prop_assert!`, `insta::assert_*!`, …) on the
//!   old side that the diff touches and that no touched assert in the same crate reproduces. Statements are
//!   compared whitespace-insensitively, so a moved or reformatted assert is quiet and a weakened one is not.
//! - `NewIgnore`: an added `#[ignore]`, also inside `#[cfg_attr(…)]`; an added `#[cfg(…)]` other than
//!   `cfg(test)` on a test function or test module; an added inner `#![cfg(…)]` in a `tests/` file.
//! - `NewShouldPanic`: an added `#[should_panic]`, also inside `#[cfg_attr(…)]`.
//! - `ToleranceChanged`: a changed binding or field whose name is tolerance-like (`alpha`, `tol`, `eps`,
//!   `threshold`, `margin`, `delta`, `p_value`, …) and whose value is a number or a plain path. Not in client
//!   crates.
//! - `FewerProptestCases`: a `cases` value or `with_cases(n)` lowered or removed (paired by the enclosing function
//!   or `proptest!` block); an added count below proptest's default 256 with no removed partner in a file that
//!   existed at the base; a line that sets `PROPTEST_CASES`.
//! - `RemovedTest`: a `#[test]`-like or proptest (`x in strategy`) function on the old side that the diff touches
//!   and no touched test in the same crate has the same name; a removed `#[cfg(test)]` or test `mod` line.
//! - `NewSeedOverride`: an added proptest RNG seed, shuffle seed or seeded-RNG constructor, a line that sets
//!   `FL_SHUFFLE_SEEDS`/`PROPTEST_RNG_SEED`, or a changed literal seed (`MatchSeed(7)` → `MatchSeed(8)`).
//! - `MockInSimTest`: an added identifier containing `mock`, `fake` or `stub` in a sim crate (`fl_sim`,
//!   `fl_test_utils`, `fl_feat_*` other than `*_client`); sim tests use the real `build_sim_app` (spec §12.4).
//! - `NewAssume`: an added `prop_assume!` (or `assume!`) that discards generated inputs.
//! - `NetTestCountDrop`: the diff removes more test functions than it adds. It stands in for the spec's "falling
//!   `nextest list` count", which would need a build of the merge base.

mod accepted;
mod diff;
mod lex;
mod patterns;
mod side;

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cmd::git;
use crate::owners;
use lex::normalized;
use patterns::*;
use side::{ItemKind, Side, is_test_attr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingKind {
    RemovedAssert,
    NewIgnore,
    NewShouldPanic,
    ToleranceChanged,
    FewerProptestCases,
    RemovedTest,
    NewSeedOverride,
    MockInSimTest,
    NewAssume,
    NetTestCountDrop,
}

impl FindingKind {
    /// Every kind, in declaration order. `ordinal` is an exhaustive match, so a new variant stops compiling there
    /// and the `every_kind_name_round_trips` test then fails until the variant is added here.
    pub const ALL: [FindingKind; 10] = [
        FindingKind::RemovedAssert,
        FindingKind::NewIgnore,
        FindingKind::NewShouldPanic,
        FindingKind::ToleranceChanged,
        FindingKind::FewerProptestCases,
        FindingKind::RemovedTest,
        FindingKind::NewSeedOverride,
        FindingKind::MockInSimTest,
        FindingKind::NewAssume,
        FindingKind::NetTestCountDrop,
    ];

    /// The position in [`FindingKind::ALL`]; exhaustive on purpose.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the exhaustive match only guards ALL; tests read it"
        )
    )]
    pub fn ordinal(self) -> usize {
        match self {
            FindingKind::RemovedAssert => 0,
            FindingKind::NewIgnore => 1,
            FindingKind::NewShouldPanic => 2,
            FindingKind::ToleranceChanged => 3,
            FindingKind::FewerProptestCases => 4,
            FindingKind::RemovedTest => 5,
            FindingKind::NewSeedOverride => 6,
            FindingKind::MockInSimTest => 7,
            FindingKind::NewAssume => 8,
            FindingKind::NetTestCountDrop => 9,
        }
    }

    /// The kind named `name` (its `Debug` spelling, as in the allowlist and the rendered findings).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| format!("{k:?}") == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub file: PathBuf,
    pub line: u32,
    pub kind: FindingKind,
    /// The function the finding sits in (for a removed assert, the base-side function); `""` outside any.
    pub test: String,
    pub text: String,
}

impl Finding {
    /// `TEST-GUARD <file>:<line> <kind> [in <test>]: <text>`
    pub fn render(&self) -> String {
        let test = if self.test.is_empty() {
            String::new()
        } else {
            format!(" in {}", self.test)
        };
        format!(
            "TEST-GUARD {}:{} {:?}{test}: {}",
            owners::normalize(&self.file),
            self.line,
            self.kind,
            self.text
        )
    }
}

pub use accepted::{load_accepted, suppress};

/// `test-guard: <n> findings (reviewer must accept each)`, or with `suppressed` allowlisted findings
/// `test-guard: <n> findings (<k> accepted findings suppressed); reviewer must accept each`.
pub fn summary(n: usize, suppressed: usize) -> String {
    if suppressed == 0 {
        format!("test-guard: {n} findings (reviewer must accept each)")
    } else {
        format!(
            "test-guard: {n} findings ({suppressed} accepted findings suppressed); reviewer must accept each"
        )
    }
}

/// [`run`] minus the findings `xtask/test-guard-accepted.toml` accepts; also returns how many it suppressed.
pub fn run_accepted(root: &Path, base_rev: &str) -> Result<(Vec<Finding>, usize)> {
    let findings = run(root, base_rev)?;
    Ok(suppress(findings, &load_accepted(root)?))
}

/// Proptest's default case count; an added lower count with no removed partner is flagged.
const DEFAULT_CASES: u64 = 256;

/// Scans the `.rs` changes since `base_rev` (committed, staged, unstaged and untracked), lexing each changed file
/// whole on both sides.
pub fn run(root: &Path, base_rev: &str) -> Result<Vec<Finding>> {
    let mut diff = git(
        root,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-U0",
            base_rev,
            "--",
            "*.rs",
        ],
    )?;
    let untracked = git(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
        ],
    )?;
    for file in untracked.split('\0').filter(|f| !f.is_empty()) {
        let text = std::fs::read(root.join(file))?;
        diff.push_str(&diff::new_file_diff(file, &String::from_utf8_lossy(&text)));
    }
    // Only sides that exist are read (a file added since the base has no old side), so any read failure here is
    // real: fail closed rather than scan a partial view.
    let source = |path: &Path, old: bool| -> Result<String> {
        if old {
            let spec = format!("{base_rev}:{}", owners::normalize(path));
            git(root, &["show", &spec])
        } else {
            let bytes = std::fs::read(root.join(path))
                .with_context(|| format!("read {}", path.display()))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    };
    scan_with_sources(&diff, &source)
}

/// Scans a unified diff on its own: each side is rebuilt from the lines the hunks show (unit-test fixtures).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the brief's interface; run() lexes whole files")
)]
pub fn scan_diff(diff: &str) -> Vec<Finding> {
    scan_sides(diff, &|file, old| Ok(file.reconstruct(old)))
        .expect("rebuilding sides from the hunks cannot fail")
}

/// Scans a unified diff, lexing each existing file side whole from `source(path, old)`; a source error fails the
/// scan. Findings are sorted by file, line and kind.
pub fn scan_with_sources(
    diff: &str,
    source: &dyn Fn(&Path, bool) -> Result<String>,
) -> Result<Vec<Finding>> {
    scan_sides(diff, &|file, old| source(&file.path, old))
}

fn scan_sides(
    diff: &str,
    text_of: &dyn Fn(&diff::FileDiff, bool) -> Result<String>,
) -> Result<Vec<Finding>> {
    let mut scan = Scan::default();
    let mut sides: BTreeMap<PathBuf, [Option<(Side, side::Owners)>; 2]> = BTreeMap::new();
    for file in diff::parse(diff) {
        if file.hunks.is_empty() {
            continue;
        }
        let side = |old: bool| -> Result<Option<Side>> {
            let exists = if old {
                file.old_exists
            } else {
                file.new_exists
            };
            if !exists {
                return Ok(None);
            }
            let text = text_of(&file, old)?;
            Ok(Some(Side::new(&text, &file.changed(old))))
        };
        let (old, new) = (side(true)?, side(false)?);
        scan.file(&file.path, old.as_ref(), new.as_ref());
        let with_owners = |s: Option<Side>| {
            s.map(|s| {
                let owners = s.owners();
                (s, owners)
            })
        };
        sides.insert(file.path.clone(), [with_owners(old), with_owners(new)]);
    }
    let mut findings = scan.finish();
    for f in &mut findings {
        // Findings on removed code cite the base line, the others the working-tree line.
        let old_first = matches!(
            f.kind,
            FindingKind::RemovedAssert | FindingKind::RemovedTest
        );
        let Some([old, new]) = sides.get(&f.file) else {
            continue;
        };
        let order = if old_first { [old, new] } else { [new, old] };
        if let Some((side, owners)) = order.into_iter().flatten().next() {
            f.test = side.owner_of_line(owners, f.line);
        }
    }
    Ok(findings)
}

/// A flagged or candidate site.
struct Site {
    line: u32,
    text: String,
}

#[derive(Default)]
struct Scan {
    findings: BTreeMap<(PathBuf, u32, FindingKind), String>,
    /// Keys are `<crate>|<normalized statement>` and `<crate>|<test name>`.
    removed_asserts: Vec<(PathBuf, Site, String)>,
    added_asserts: BTreeMap<String, usize>,
    removed_assumes: BTreeMap<String, usize>,
    added_assumes: Vec<(PathBuf, Site, String)>,
    removed_tests: Vec<(PathBuf, Site, Option<String>)>,
    added_tests: BTreeMap<String, usize>,
    added_test_count: usize,
}

#[derive(Default)]
struct FileScan {
    old_exists: bool,
    /// Case counts by owner key (enclosing function or `proptest!` block), in source order.
    cases_removed: BTreeMap<String, Vec<(Site, u64)>>,
    cases_added: BTreeMap<String, Vec<(Site, u64)>>,
    tol_removed: Vec<(Site, String, String)>,
    tol_added: Vec<(Site, String, String)>,
    seeds_removed: Vec<String>,
    seeds_added: Vec<(Site, String)>,
    gates_removed: Vec<(Site, String)>,
    gates_added: Vec<String>,
}

fn is_assert_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("assert")
}

fn is_assume_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with("assume")
}

/// Takes one `key` from a multiset; whether it was there.
fn take(set: &mut BTreeMap<String, usize>, key: &str) -> bool {
    match set.get_mut(key) {
        Some(n) if *n > 0 => {
            *n -= 1;
            true
        }
        _ => false,
    }
}

impl Scan {
    fn flag(&mut self, file: &Path, line: u32, kind: FindingKind, text: String) {
        self.findings
            .entry((file.to_path_buf(), line, kind))
            .or_insert(text);
    }

    fn file(&mut self, path: &Path, old: Option<&Side>, new: Option<&Side>) {
        let ck = crate_key(path);
        let mut fs = FileScan {
            old_exists: old.is_some(),
            ..FileScan::default()
        };
        if let Some(old) = old {
            self.old_side(path, &ck, old, &mut fs);
        }
        if let Some(new) = new {
            self.new_side(path, &ck, new, &mut fs);
        }
        self.finish_file(path, fs);
    }

    fn old_side(&mut self, path: &Path, ck: &str, old: &Side, fs: &mut FileScan) {
        for m in old.macro_calls(is_assert_name) {
            if old.span_touched(m.first, m.last) {
                let site = Site {
                    line: old.line_no(m.first),
                    text: old.source_span(m.first, m.last),
                };
                self.removed_asserts
                    .push((path.to_path_buf(), site, format!("{ck}|{}", m.key)));
            }
        }
        for m in old.macro_calls(is_assume_name) {
            if old.span_touched(m.first, m.last) {
                *self
                    .removed_assumes
                    .entry(format!("{ck}|{}", m.key))
                    .or_default() += 1;
            }
        }
        for t in old.tests().into_iter().filter(|t| t.touched) {
            let site = Site {
                line: old.line_no(t.line),
                text: t.text,
            };
            let key = t.name.map(|n| format!("{ck}|{n}"));
            self.removed_tests.push((path.to_path_buf(), site, key));
        }
        let owners = old.owners();
        for l in old.changed_lines() {
            let chs = old.line_chs(l);
            let site = || Site {
                line: old.line_no(l),
                text: old.source(l),
            };
            if let Some(n) = proptest_cases(chs) {
                fs.cases_removed
                    .entry(owners.key(old.line_start(l)))
                    .or_default()
                    .push((site(), n));
            }
            if !is_client_path(path) {
                for (name, value) in tolerance_bindings(chs) {
                    fs.tol_removed.push((site(), name, value));
                }
            }
            if has_seed_literal(chs) {
                fs.seeds_removed.push(normalized(chs));
            }
            if is_test_gate(chs, path) {
                fs.gates_removed.push((site(), normalized(chs)));
            }
        }
    }

    fn new_side(&mut self, path: &Path, ck: &str, new: &Side, fs: &mut FileScan) {
        for m in new.macro_calls(is_assert_name) {
            if new.span_touched(m.first, m.last) {
                *self
                    .added_asserts
                    .entry(format!("{ck}|{}", m.key))
                    .or_default() += 1;
            }
        }
        for m in new.macro_calls(is_assume_name) {
            if new.span_touched(m.first, m.last) {
                let site = Site {
                    line: new.line_no(m.first),
                    text: new.source_span(m.first, m.last),
                };
                self.added_assumes
                    .push((path.to_path_buf(), site, format!("{ck}|{}", m.key)));
            }
        }
        for t in new.tests().into_iter().filter(|t| t.touched) {
            self.added_test_count += 1;
            if let Some(name) = t.name {
                *self.added_tests.entry(format!("{ck}|{name}")).or_default() += 1;
            }
        }
        self.disabling_attributes(path, new);

        let owners = new.owners();
        for l in new.changed_lines() {
            let chs = new.line_chs(l);
            let site = || Site {
                line: new.line_no(l),
                text: new.source(l),
            };
            if let Some(n) = proptest_cases(chs) {
                fs.cases_added
                    .entry(owners.key(new.line_start(l)))
                    .or_default()
                    .push((site(), n));
            }
            if sets_env_var(chs, CASES_VARS) {
                let s = site();
                self.flag(path, s.line, FindingKind::FewerProptestCases, s.text);
            }
            if !is_client_path(path) {
                for (name, value) in tolerance_bindings(chs) {
                    fs.tol_added.push((site(), name, value));
                }
            }
            if has_seed_override_ident(chs) || sets_env_var(chs, SEED_VARS) {
                let s = site();
                self.flag(path, s.line, FindingKind::NewSeedOverride, s.text);
            } else if has_seed_literal(chs) {
                fs.seeds_added.push((site(), normalized(chs)));
            }
            if is_sim_path(path) && lex::idents(chs).iter().any(|i| is_mock_ident(i)) {
                let s = site();
                self.flag(path, s.line, FindingKind::MockInSimTest, s.text);
            }
            if is_test_gate(chs, path) {
                fs.gates_added.push(normalized(chs));
            }
        }
    }

    /// Added attributes that skip tests: `ignore`, `should_panic`, a `cfg` other than `cfg(test)` on a test
    /// function or test module, and an inner `cfg` in a `tests/` file.
    fn disabling_attributes(&mut self, path: &Path, new: &Side) {
        for attr in new.attributes() {
            if !new.span_touched(attr.first, attr.last) {
                continue;
            }
            let line = new.line_no(attr.first);
            let text = new.source_span(attr.first, attr.last);
            let cfg_attr = attr.path == "cfg_attr";
            let has = |w: &str| cfg_attr && attr.idents.iter().any(|i| i == w);
            if attr.path == "ignore" || has("ignore") {
                self.flag(path, line, FindingKind::NewIgnore, text.clone());
            }
            if attr.path == "should_panic" || has("should_panic") {
                self.flag(path, line, FindingKind::NewShouldPanic, text.clone());
            }
            if attr.inner && attr.path == "cfg" && is_in_tests_dir(path) {
                self.flag(path, line, FindingKind::NewIgnore, text);
            }
        }
        for stack in new.stacks() {
            let Some(item) = &stack.item else { continue };
            let is_test = match item.kind {
                ItemKind::Fn => stack.attrs.iter().any(|a| is_test_attr(&a.path)),
                ItemKind::Mod => {
                    item.name.contains("test")
                        || is_in_tests_dir(path)
                        || stack.attrs.iter().any(|a| a.body == "cfg(test)")
                }
            };
            if !is_test {
                continue;
            }
            for attr in &stack.attrs {
                if attr.path == "cfg"
                    && attr.body != "cfg(test)"
                    && new.span_touched(attr.first, attr.last)
                {
                    let text = new.source_span(attr.first, attr.last);
                    self.flag(path, new.line_no(attr.first), FindingKind::NewIgnore, text);
                }
            }
        }
    }

    /// The per-file pairings: case counts, tolerances, changed seeds and test gates.
    fn finish_file(&mut self, path: &Path, fs: FileScan) {
        // Case counts pair by owner, in order; a lower or missing replacement is flagged, and so is an added
        // count below the default with no partner in a file that existed at the base.
        for (key, removed) in &fs.cases_removed {
            let added = fs.cases_added.get(key);
            for (k, (site, before)) in removed.iter().enumerate() {
                match added.and_then(|a| a.get(k)) {
                    Some((a, after)) if after < before => self.flag(
                        path,
                        a.line,
                        FindingKind::FewerProptestCases,
                        format!("{} (was {before})", a.text),
                    ),
                    Some(_) => {}
                    None => self.flag(
                        path,
                        site.line,
                        FindingKind::FewerProptestCases,
                        format!("{} (removed)", site.text),
                    ),
                }
            }
        }
        if fs.old_exists {
            for (key, added) in &fs.cases_added {
                let paired = fs.cases_removed.get(key).map_or(0, Vec::len);
                for (site, n) in added.iter().skip(paired) {
                    if *n < DEFAULT_CASES {
                        self.flag(
                            path,
                            site.line,
                            FindingKind::FewerProptestCases,
                            format!("{} (below the default {DEFAULT_CASES})", site.text),
                        );
                    }
                }
            }
        }

        // Tolerances: identical (name, value) pairs cancel; any other added or removed binding is a change.
        let mut removed: BTreeMap<String, usize> = BTreeMap::new();
        for (_, name, value) in &fs.tol_removed {
            *removed.entry(format!("{name}={value}")).or_default() += 1;
        }
        let mut changed_names = Vec::new();
        for (site, name, value) in &fs.tol_added {
            if take(&mut removed, &format!("{name}={value}")) {
                continue;
            }
            changed_names.push(name.clone());
            let before = fs
                .tol_removed
                .iter()
                .find(|(_, n, _)| n == name)
                .map(|(_, _, v)| format!(" (was {v})"))
                .unwrap_or_default();
            self.flag(
                path,
                site.line,
                FindingKind::ToleranceChanged,
                format!("{}{before}", site.text),
            );
        }
        for (site, name, value) in &fs.tol_removed {
            if take(&mut removed, &format!("{name}={value}")) && !changed_names.contains(name) {
                self.flag(
                    path,
                    site.line,
                    FindingKind::ToleranceChanged,
                    format!("{} (removed)", site.text),
                );
            }
        }

        // A literal seed added where one was removed is a changed seed, unless the line is unchanged.
        if !fs.seeds_removed.is_empty() {
            let mut removed: BTreeMap<String, usize> = BTreeMap::new();
            for s in &fs.seeds_removed {
                *removed.entry(s.clone()).or_default() += 1;
            }
            for (site, key) in &fs.seeds_added {
                if !take(&mut removed, key) {
                    self.flag(
                        path,
                        site.line,
                        FindingKind::NewSeedOverride,
                        site.text.clone(),
                    );
                }
            }
        }

        // A removed `#[cfg(test)]` or test `mod` line not re-added in the file.
        let mut added: BTreeMap<String, usize> = BTreeMap::new();
        for g in &fs.gates_added {
            *added.entry(g.clone()).or_default() += 1;
        }
        for (site, key) in &fs.gates_removed {
            if !take(&mut added, key) {
                self.flag(path, site.line, FindingKind::RemovedTest, site.text.clone());
            }
        }
    }

    /// The diff-wide pairings (asserts, assumes and tests may move between files of a crate), then the report.
    fn finish(mut self) -> Vec<Finding> {
        let mut added_asserts = std::mem::take(&mut self.added_asserts);
        for (file, site, key) in std::mem::take(&mut self.removed_asserts) {
            if !take(&mut added_asserts, &key) {
                self.flag(&file, site.line, FindingKind::RemovedAssert, site.text);
            }
        }
        let mut removed_assumes = std::mem::take(&mut self.removed_assumes);
        for (file, site, key) in std::mem::take(&mut self.added_assumes) {
            if !take(&mut removed_assumes, &key) {
                self.flag(&file, site.line, FindingKind::NewAssume, site.text);
            }
        }
        let removed_tests = std::mem::take(&mut self.removed_tests);
        let mut added_tests = std::mem::take(&mut self.added_tests);
        for (file, site, key) in &removed_tests {
            if !key.as_ref().is_some_and(|k| take(&mut added_tests, k)) {
                self.flag(file, site.line, FindingKind::RemovedTest, site.text.clone());
            }
        }
        let added = self.added_test_count;
        if removed_tests.len() > added
            && let Some((file, site, _)) = removed_tests.first()
        {
            self.flag(
                file,
                site.line,
                FindingKind::NetTestCountDrop,
                format!(
                    "net test count falls by {} ({} removed, {added} added)",
                    removed_tests.len() - added,
                    removed_tests.len(),
                ),
            );
        }
        self.findings
            .into_iter()
            .map(|((file, line, kind), text)| Finding {
                file,
                line,
                kind,
                test: String::new(),
                text,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FindingKind::*;

    fn fixture(name: &str) -> &'static str {
        match name {
            "removed_assert" => include_str!("../../tests/fixtures/guard/removed_assert.diff"),
            "ignore_should_panic" => {
                include_str!("../../tests/fixtures/guard/ignore_should_panic.diff")
            }
            "proptest_cases" => include_str!("../../tests/fixtures/guard/proptest_cases.diff"),
            "alpha" => include_str!("../../tests/fixtures/guard/alpha.diff"),
            "removed_test" => include_str!("../../tests/fixtures/guard/removed_test.diff"),
            "mock_in_sim" => include_str!("../../tests/fixtures/guard/mock_in_sim.diff"),
            "added_asserts" => include_str!("../../tests/fixtures/guard/added_asserts.diff"),
            "multiline_assert" => include_str!("../../tests/fixtures/guard/multiline_assert.diff"),
            "moved_test" => include_str!("../../tests/fixtures/guard/moved_test.diff"),
            "proptest_fn" => include_str!("../../tests/fixtures/guard/proptest_fn.diff"),
            "seed_override" => include_str!("../../tests/fixtures/guard/seed_override.diff"),
            "raw_string" => include_str!("../../tests/fixtures/guard/raw_string.diff"),
            "cfg_disabled" => include_str!("../../tests/fixtures/guard/cfg_disabled.diff"),
            "proptest_pairing" => include_str!("../../tests/fixtures/guard/proptest_pairing.diff"),
            "proptest_added" => include_str!("../../tests/fixtures/guard/proptest_added.diff"),
            "assume_and_tolerance" => {
                include_str!("../../tests/fixtures/guard/assume_and_tolerance.diff")
            }
            other => panic!("unknown fixture {other}"),
        }
    }

    fn summarize(found: Vec<Finding>) -> Vec<(String, u32, FindingKind)> {
        found
            .into_iter()
            .map(|f| (owners::normalize(&f.file), f.line, f.kind))
            .collect()
    }

    /// `(file, line, kind)` of every finding, in report order.
    fn scan(name: &str) -> Vec<(String, u32, FindingKind)> {
        summarize(scan_diff(fixture(name)))
    }

    fn at(file: &str, line: u32, kind: FindingKind) -> (String, u32, FindingKind) {
        (file.to_owned(), line, kind)
    }

    #[test]
    fn findings_carry_the_enclosing_test() {
        let old = "fn keep() {\n    assert!(true);\n}\n\nfn smoke_truck_moves() {\n    let a = 1;\n    assert_eq!(a, 1);\n}\n";
        let new =
            "fn keep() {\n    assert!(true);\n}\n\nfn smoke_truck_moves() {\n    let a = 1;\n}\n";
        let diff = "diff --git a/crates/x/tests/t.rs b/crates/x/tests/t.rs\n--- a/crates/x/tests/t.rs\n+++ b/crates/x/tests/t.rs\n@@ -7 +6,0 @@\n-    assert_eq!(a, 1);\n";
        let found = scan_with_sources(diff, &|_, is_old| {
            Ok(if is_old { old } else { new }.to_owned())
        })
        .unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].test, "smoke_truck_moves");
    }

    #[test]
    fn guard_flags_removed_assert() {
        let found = scan_diff(fixture("removed_assert"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, RemovedAssert);
        assert_eq!(found[0].line, 12);
        assert_eq!(found[0].text, "assert_eq!(a, b);");
        assert_eq!(
            found[0].render(),
            "TEST-GUARD crates/fl_sim/tests/movement.rs:12 RemovedAssert: assert_eq!(a, b);"
        );
    }

    #[test]
    fn guard_flags_new_ignore_and_should_panic() {
        let f = "crates/fl_sim/tests/combat.rs";
        assert_eq!(
            scan("ignore_should_panic"),
            [at(f, 21, NewIgnore), at(f, 42, NewShouldPanic)],
            "an attribute named in a comment is not code"
        );
    }

    #[test]
    fn guard_flags_fewer_proptest_cases() {
        assert_eq!(
            scan("proptest_cases"),
            [at("crates/fl_sim/tests/props.rs", 8, FewerProptestCases)]
        );
    }

    #[test]
    fn guard_flags_alpha_change() {
        assert_eq!(
            scan("alpha"),
            [at("crates/fl_test_utils/src/stats.rs", 3, ToleranceChanged)]
        );
    }

    #[test]
    fn guard_flags_removed_test_fn() {
        let f = "crates/fl_sim/tests/movement.rs";
        assert_eq!(
            scan("removed_test"),
            [at(f, 30, RemovedTest), at(f, 30, NetTestCountDrop)]
        );
    }

    #[test]
    fn guard_flags_mock_in_sim_test() {
        assert_eq!(
            scan("mock_in_sim"),
            [at("crates/fl_sim/tests/x.rs", 2, MockInSimTest)],
            "mocks at the client boundary are allowed"
        );
    }

    #[test]
    fn guard_quiet_on_added_asserts() {
        assert_eq!(scan("added_asserts"), []);
    }

    #[test]
    fn guard_flags_weakened_multiline_and_commented_out_asserts() {
        let f = "crates/fl_sim/tests/hash.rs";
        assert_eq!(
            scan("multiline_assert"),
            [at(f, 11, RemovedAssert), at(f, 32, RemovedAssert)],
            "a string that mentions assert! is not an assert"
        );
    }

    #[test]
    fn guard_quiet_on_moved_and_reformatted_test() {
        assert_eq!(scan("moved_test"), []);
    }

    #[test]
    fn guard_counts_proptest_fns_in_deleted_file() {
        let f = "crates/fl_sim/tests/props.rs";
        assert_eq!(
            scan("proptest_fn"),
            [
                at(f, 4, FewerProptestCases),
                at(f, 5, RemovedTest),
                at(f, 5, NetTestCountDrop),
                at(f, 7, RemovedAssert),
                at(f, 9, RemovedTest),
                at(f, 10, RemovedAssert),
            ]
        );
        let drop = scan_diff(fixture("proptest_fn"))
            .into_iter()
            .find(|f| f.kind == NetTestCountDrop)
            .unwrap();
        assert_eq!(drop.text, "net test count falls by 2 (2 removed, 0 added)");
    }

    #[test]
    fn guard_flags_seed_overrides() {
        let f = "crates/fl_sim/tests/combat.rs";
        assert_eq!(
            scan("seed_override"),
            [
                at(f, 5, NewSeedOverride),
                at(f, 41, NewSeedOverride),
                at(f, 42, NewSeedOverride)
            ],
            "a fixed seed in a new test is not an override"
        );
    }

    /// The hunk starts inside a raw string whose text has quotes, an unbalanced paren and `#[test]`/`assert!`
    /// look-alikes. Lexed whole, the file still shows exactly the removed assert.
    #[test]
    fn guard_lexes_whole_files_across_raw_strings() {
        let old = include_str!("../../tests/fixtures/guard/raw_string_old.rs.txt");
        let new = include_str!("../../tests/fixtures/guard/raw_string_new.rs.txt");
        let source = |_: &Path, is_old: bool| Ok((if is_old { old } else { new }).to_owned());
        assert_eq!(
            summarize(scan_with_sources(fixture("raw_string"), &source).unwrap()),
            [at("crates/fl_sim/tests/scenario.rs", 13, RemovedAssert)]
        );
        assert_eq!(
            scan("raw_string"),
            [],
            "from the hunk alone (starting mid-string) the literal state inverts and hides the assert"
        );
    }

    /// A side that exists but cannot be read fails the scan; an added file never asks for its (absent) old side.
    #[test]
    fn guard_fails_closed_on_unreadable_source() {
        let unreadable = |_: &Path, old: bool| -> Result<String> {
            if old {
                anyhow::bail!("git show failed")
            }
            Ok(String::new())
        };
        let err = scan_with_sources(fixture("raw_string"), &unreadable).unwrap_err();
        assert!(format!("{err:#}").contains("git show failed"), "{err:#}");
        let added = diff::new_file_diff("crates/fl_sim/tests/new.rs", "struct MockRunner;\n");
        let found = scan_with_sources(&added, &|_, old| {
            assert!(!old, "an added file has no old side to read");
            Ok("struct MockRunner;\n".to_owned())
        })
        .unwrap();
        assert_eq!(
            summarize(found),
            [at("crates/fl_sim/tests/new.rs", 1, MockInSimTest)]
        );
    }

    #[test]
    fn guard_flags_tests_disabled_by_cfg() {
        assert_eq!(
            scan("cfg_disabled"),
            [
                at("crates/fl_sim/src/step.rs", 21, NewIgnore),
                at("crates/fl_sim/src/step.rs", 40, RemovedTest),
                at("crates/fl_sim/src/step.rs", 41, NewIgnore),
                at("crates/fl_sim/tests/combat.rs", 1, NewIgnore),
            ],
            "a cfg on a non-test fn and a new `#[cfg(test)] mod tests` are quiet"
        );
    }

    #[test]
    fn guard_pairs_case_counts_by_owner() {
        assert_eq!(
            scan("proptest_pairing"),
            [],
            "swapping two proptest! blocks keeps each block's count"
        );
    }

    #[test]
    fn guard_flags_low_added_case_counts_and_env() {
        let f = "crates/fl_sim/tests/props.rs";
        assert_eq!(
            scan("proptest_added"),
            [at(f, 22, FewerProptestCases), at(f, 30, FewerProptestCases)],
            "a low count in a new file is quiet"
        );
    }

    #[test]
    fn guard_flags_assume_and_broader_tolerances() {
        let f = "crates/fl_sim/tests/props.rs";
        assert_eq!(
            scan("assume_and_tolerance"),
            [
                at(f, 4, ToleranceChanged),
                at(f, 5, ToleranceChanged),
                at(f, 9, NewAssume),
            ],
            "a moved prop_assume! and a non-numeric `delta` are quiet"
        );
    }

    #[test]
    fn crate_key_groups_src_and_tests() {
        assert_eq!(
            crate_key(Path::new("crates/fl_sim/src/step.rs")),
            "crates/fl_sim"
        );
        assert_eq!(
            crate_key(Path::new("crates/fl_sim/tests/a/b.rs")),
            "crates/fl_sim"
        );
        assert_eq!(crate_key(Path::new("xtask/src/test_guard/mod.rs")), "xtask");
    }
}
