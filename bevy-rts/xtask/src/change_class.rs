//! Change classes and the golden-replay policy (ADR-0021, rulings R146, R151, R153): the gate's
//! `change-class` row (task, pr) and `golden-policy` row (pr, milestone).
//!
//! - Every commit that touches `crates/**`, `features/**`, `assets/**`, the root `scenarios/**`,
//!   or the root `Cargo.toml` or `Cargo.lock` carries a `Change-Class: R|D|F|N|E|X` trailer.
//!   Merge commits are exempt (they are not collected). A class F or D commit carries
//!   `MCR: MCR-nnn`, and every `MCR:` trailer names a `## MCR-nnn` heading of
//!   `docs/design/changelog-mechanics.md`.
//! - A class R or E commit may change a golden replay's bytes only when that same commit changes
//!   `REPLAY_FORMAT_VERSION` (the exemption is per commit), and then only with the golden's hash
//!   stream equal to its hash stream at the commit's parent.
//! - Over the branch (merge base to `HEAD`): a changed golden needs an `MCR:` trailer on the
//!   commits that change it and a changed `SIM_VERSION`, replay format or content; a changed
//!   `SIM_VERSION` needs a changed golden.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::cmd::git;

/// The change classes: Refactor, Data tuning, Formula or rule, New mechanic, Engine or crate
/// upgrade, eXperiment.
pub const CLASSES: [char; 6] = ['R', 'D', 'F', 'N', 'E', 'X'];

/// The MCR changelog, relative to the workspace root.
pub const CHANGELOG: &str = "docs/design/changelog-mechanics.md";
/// The file whose `pub const SIM_VERSION` line the rows read.
const SIM_VERSION_FILE: &str = "crates/fl_sim/src/build.rs";
/// The file whose `pub const REPLAY_FORMAT_VERSION` line the rows read.
const REPLAY_FILE: &str = "crates/fl_sim/src/replay.rs";

/// The trailer block of `message`: its last paragraph, unless that is the subject (a message
/// of one paragraph has no trailers).
fn trailer_block(message: &str) -> &str {
    match message.trim_end().rsplit_once("\n\n") {
        Some((_, last)) => last,
        None => "",
    }
}

/// The value of the trailer line `<key>: <value>` in the trailer block of `message` (the first
/// one), trimmed. A line in the body that quotes a trailer does not count.
fn trailer<'a>(message: &'a str, key: &str) -> Option<&'a str> {
    trailer_block(message).lines().find_map(|line| {
        line.trim_end()
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
            .map(str::trim)
    })
}

/// The class of a `Change-Class: F` trailer: its one character (known or not); `None` without
/// the trailer or when its value is not one character.
pub fn trailer_class(message: &str) -> Option<char> {
    let mut chars = trailer(message, "Change-Class")?.chars();
    match (chars.next(), chars.next()) {
        (Some(class), None) => Some(class),
        _ => None,
    }
}

/// The id of an `MCR: MCR-001` trailer; `None` without one or with an empty value.
pub fn trailer_mcr(message: &str) -> Option<String> {
    trailer(message, "MCR")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// `path` with `/` separators.
fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Whether a commit changing `files` needs a `Change-Class:` trailer: any path under `crates/`,
/// `features/`, `assets/` or the root `scenarios/`, or the root `Cargo.toml` or `Cargo.lock`.
pub fn needs_class(files: &[PathBuf]) -> bool {
    files.iter().any(|file| {
        let path = normalize(file);
        ["crates/", "features/", "assets/", "scenarios/"]
            .iter()
            .any(|dir| path.starts_with(dir))
            || path == "Cargo.toml"
            || path == "Cargo.lock"
    })
}

/// Whether `path` is a golden replay: a `*.replay` directly under a `tests/replays/` directory.
pub fn is_golden(path: &Path) -> bool {
    let path = normalize(path);
    let Some((dir, name)) = path.rsplit_once('/') else {
        return false;
    };
    name.ends_with(".replay") && (dir == "tests/replays" || dir.ends_with("/tests/replays"))
}

/// What the `change-class` row knows of one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFacts {
    pub sha: String,
    pub message: String,
    /// The files the commit changes (empty for a merge commit).
    pub files: Vec<PathBuf>,
    /// `REPLAY_FORMAT_VERSION` in `crates/fl_sim/src/replay.rs` differs between this commit and
    /// its parent (R151).
    pub replay_format_changed: bool,
}

/// What the `golden-policy` row knows of the branch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchFacts {
    /// `SIM_VERSION` differs between the merge base and `HEAD`.
    pub sim_version_changed: bool,
    /// `REPLAY_FORMAT_VERSION` differs between the merge base and `HEAD`.
    pub replay_format_changed: bool,
    /// A file under `assets/**` or the root `scenarios/**` differs between the merge base and
    /// `HEAD`.
    pub content_changed: bool,
    /// The golden replays that differ between the merge base and `HEAD`.
    pub goldens_changed: Vec<PathBuf>,
    /// Per class R or E commit (sha) and each golden it changes: whether the golden's hash
    /// stream at the commit equals its hash stream at the commit's parent (R153).
    pub refactor_goldens: Vec<(String, PathBuf, bool)>,
}

/// The first 7 characters of `sha`.
fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// The rule violations of `commits` (the `change-class` row, without the changelog check of
/// [`mcr_violations`]): a commit that [`needs_class`] has a known class; a class F or D commit
/// has an `MCR:` trailer; a class R or E commit that changes a golden is a violation unless
/// that same commit changes the replay format (ADR-0021, R151).
pub fn class_violations(commits: &[CommitFacts]) -> Vec<String> {
    let mut out = Vec::new();
    for commit in commits {
        let sha = short(&commit.sha);
        let class = trailer_class(&commit.message);
        match class {
            Some(class) if !CLASSES.contains(&class) => out.push(format!(
                "{sha}: unknown change class `{class}` (one of R, D, F, N, E, X)"
            )),
            None if trailer(&commit.message, "Change-Class").is_some() => out.push(format!(
                "{sha}: malformed `Change-Class:` trailer (one letter of R, D, F, N, E, X)"
            )),
            None if needs_class(&commit.files) => out.push(format!(
                "{sha}: touches crates/, features/, assets/, scenarios/ or Cargo.toml/lock but has no \
                 `Change-Class:` trailer"
            )),
            _ => {}
        }
        match class {
            Some(class @ ('F' | 'D')) if trailer_mcr(&commit.message).is_none() => out.push(
                format!("{sha}: a class {class} commit needs an `MCR: MCR-nnn` trailer"),
            ),
            Some(class @ ('R' | 'E')) if !commit.replay_format_changed => {
                for golden in commit.files.iter().filter(|file| is_golden(file)) {
                    out.push(format!(
                        "{sha}: a class {class} commit changes the golden {} without changing \
                         REPLAY_FORMAT_VERSION (R and E keep goldens bit-identical; ADR-0021)",
                        normalize(golden)
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether `changelog` has the heading `## <mcr>` (followed by the end of the line or a
/// character that cannot continue the id).
pub fn has_mcr_heading(changelog: &str, mcr: &str) -> bool {
    changelog.lines().any(|line| {
        line.strip_prefix("## ")
            .and_then(|rest| rest.strip_prefix(mcr))
            .is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '-' && c != '_')
            })
    })
}

/// Every `MCR:` trailer of `commits` that names no heading of `changelog`.
pub fn mcr_violations(commits: &[CommitFacts], changelog: &str) -> Vec<String> {
    commits
        .iter()
        .filter_map(|commit| {
            let mcr = trailer_mcr(&commit.message)?;
            (!has_mcr_heading(changelog, &mcr)).then(|| {
                format!(
                    "{}: `MCR: {mcr}` names no `## {mcr}` heading in {CHANGELOG}",
                    short(&commit.sha)
                )
            })
        })
        .collect()
}

/// The rule violations of the branch (the `golden-policy` row). `golden_commits` are the
/// `(sha, message)` of the branch's commits that change a golden. A changed golden needs an
/// `MCR:` trailer naming a heading of `changelog` on every commit that changes one, and a
/// changed `SIM_VERSION`, replay format or content; a changed `SIM_VERSION` needs a changed
/// golden; and every [`BranchFacts::refactor_goldens`] entry with an unequal hash stream is a
/// violation.
pub fn golden_violations(
    facts: &BranchFacts,
    golden_commits: &[(String, String)],
    changelog: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    if !facts.goldens_changed.is_empty() {
        for (sha, message) in golden_commits {
            let sha = short(sha);
            match trailer_mcr(message) {
                None => out.push(format!(
                    "{sha}: changes a golden replay without an `MCR:` trailer (re-bless only through \
                     `fl_cli rebless --reason MCR-nnn`, R146)"
                )),
                Some(mcr) if !has_mcr_heading(changelog, &mcr) => out.push(format!(
                    "{sha}: `MCR: {mcr}` names no `## {mcr}` heading in {CHANGELOG}"
                )),
                Some(_) => {}
            }
        }
        if !(facts.sim_version_changed || facts.replay_format_changed || facts.content_changed) {
            let goldens: Vec<String> = facts.goldens_changed.iter().map(|g| normalize(g)).collect();
            out.push(format!(
                "golden replays changed ({}) but neither SIM_VERSION, REPLAY_FORMAT_VERSION nor the \
                 content (assets/**, scenarios/**) changed since the merge base",
                goldens.join(", ")
            ));
        }
    }
    if facts.sim_version_changed && facts.goldens_changed.is_empty() {
        out.push(
            "SIM_VERSION changed but no golden replay did: a class F bump must change outcomes"
                .to_owned(),
        );
    }
    for (sha, golden, equal) in &facts.refactor_goldens {
        if !equal {
            out.push(format!(
                "{}: a class R or E commit changes the hash stream of {} (against its parent); R \
                 and E keep hash streams bit-identical (ADR-0021, R153)",
                short(sha),
                normalize(golden)
            ));
        }
    }
    out
}

/// The value of the line `pub const <name>: u32 = <n>;` of `text`.
fn const_value(text: &str, name: &str) -> Option<u32> {
    let prefix = format!("pub const {name}:");
    text.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(&prefix)?;
        let (_, value) = rest.split_once('=')?;
        value.split(';').next()?.trim().parse().ok()
    })
}

/// The `hashes` of a replay as `fl_cli replay dump --json` prints it.
fn hashes_of(json: &serde_json::Value) -> Option<&serde_json::Value> {
    json.get("hashes").filter(|hashes| hashes.is_array())
}

// ---- collecting the facts from git ------------------------------------------------------------------------------

/// The bytes of `path` at `rev`; `None` if it does not exist there.
fn bytes_at(root: &Path, rev: &str, path: &str) -> Result<Option<Vec<u8>>> {
    let out = Command::new("git")
        .args(["show", &format!("{rev}:./{path}")])
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run git show {rev}:{path}"))?;
    Ok(out.status.success().then_some(out.stdout))
}

/// The value of `pub const <name>` in `path` at `rev`.
fn const_at(root: &Path, rev: &str, path: &str, name: &str) -> Result<Option<u32>> {
    Ok(bytes_at(root, rev, path)?
        .and_then(|bytes| const_value(&String::from_utf8_lossy(&bytes), name)))
}

/// Whether `pub const <name>` in `path` differs between `a` and `b`.
fn const_changed(root: &Path, a: &str, b: &str, path: &str, name: &str) -> Result<bool> {
    Ok(const_at(root, a, path, name)? != const_at(root, b, path, name)?)
}

/// `git <args>` as a list of paths, one per line.
fn git_paths(root: &Path, args: &[&str]) -> Result<Vec<PathBuf>> {
    Ok(git(root, args)?
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// The facts of every non-merge commit in `base_rev..HEAD`, oldest first.
pub fn commits(root: &Path, base_rev: &str) -> Result<Vec<CommitFacts>> {
    let range = format!("{base_rev}..HEAD");
    let shas = git(root, &["rev-list", "--no-merges", "--reverse", &range])?;
    let mut out = Vec::new();
    for sha in shas.lines().filter(|line| !line.is_empty()) {
        let message = git(root, &["show", "-s", "--format=%B", sha])?;
        let files = git_paths(
            root,
            &[
                "diff-tree",
                "--relative",
                "--no-commit-id",
                "--name-only",
                "-r",
                "--no-renames",
                "--root",
                sha,
            ],
        )?;
        let replay_format_changed = files.iter().any(|f| normalize(f) == REPLAY_FILE)
            && const_changed(
                root,
                &format!("{sha}^"),
                sha,
                REPLAY_FILE,
                "REPLAY_FORMAT_VERSION",
            )?;
        out.push(CommitFacts {
            sha: sha.to_owned(),
            message,
            files,
            replay_format_changed,
        });
    }
    Ok(out)
}

/// The MCR changelog in the working tree; empty if it does not exist.
fn changelog(root: &Path) -> String {
    std::fs::read_to_string(root.join(CHANGELOG)).unwrap_or_default()
}

/// The `change-class` row's violations for the commits since `base_rev`.
pub fn check_commits(root: &Path, base_rev: &str) -> Result<Vec<String>> {
    let commits = commits(root, base_rev)?;
    let mut out = class_violations(&commits);
    out.extend(mcr_violations(&commits, &changelog(root)));
    Ok(out)
}

/// The replay `bytes` as `fl_cli replay dump --json` prints it (it reads formats 3 and 4),
/// through a file under `target/xtask/golden-policy/`.
fn dump_json(root: &Path, name: &str, bytes: &[u8]) -> Result<serde_json::Value> {
    let dir = root.join("target").join("xtask").join("golden-policy");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let file = dir.join(name);
    std::fs::write(&file, bytes).with_context(|| format!("write {}", file.display()))?;
    let out = Command::new("cargo")
        .args([
            "run", "-q", "-p", "fl_cli", "--", "--json", "replay", "dump",
        ])
        .arg(&file)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .context("run fl_cli replay dump")?;
    if !out.status.success() {
        bail!(
            "fl_cli replay dump {} failed:\n{}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim_end()
        );
    }
    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("fl_cli replay dump {} printed no JSON", file.display()))
}

/// Whether the hash stream of the golden `path` at commit `sha` equals its hash stream at the
/// commit's parent. A golden that does not exist on one side is unequal.
fn hash_stream_kept(root: &Path, sha: &str, path: &Path) -> Result<bool> {
    let path = normalize(path);
    let (Some(after), Some(before)) = (
        bytes_at(root, sha, &path)?,
        bytes_at(root, &format!("{sha}^"), &path)?,
    ) else {
        return Ok(false);
    };
    if after == before {
        return Ok(true);
    }
    let name = path.rsplit('/').next().unwrap_or("golden.replay");
    let after = dump_json(root, &format!("{}-{name}", short(sha)), &after)?;
    let before = dump_json(root, &format!("{}-parent-{name}", short(sha)), &before)?;
    match (hashes_of(&after), hashes_of(&before)) {
        (Some(a), Some(b)) => Ok(a == b),
        _ => bail!("fl_cli replay dump of {path} at {sha} printed no `hashes`"),
    }
}

/// The `golden-policy` row's violations for the branch since `base_rev` (the merge base).
pub fn check_branch(root: &Path, base_rev: &str) -> Result<Vec<String>> {
    let files = git_paths(
        root,
        &[
            "diff",
            "--relative",
            "--name-only",
            "--no-renames",
            base_rev,
            "HEAD",
            "--",
            ".",
        ],
    )?;
    let commits = commits(root, base_rev)?;
    let mut refactor_goldens = Vec::new();
    for commit in &commits {
        if !matches!(trailer_class(&commit.message), Some('R' | 'E')) {
            continue;
        }
        for golden in commit.files.iter().filter(|file| is_golden(file)) {
            let kept = hash_stream_kept(root, &commit.sha, golden)?;
            refactor_goldens.push((commit.sha.clone(), golden.clone(), kept));
        }
    }
    let facts = BranchFacts {
        sim_version_changed: const_changed(
            root,
            base_rev,
            "HEAD",
            SIM_VERSION_FILE,
            "SIM_VERSION",
        )?,
        replay_format_changed: const_changed(
            root,
            base_rev,
            "HEAD",
            REPLAY_FILE,
            "REPLAY_FORMAT_VERSION",
        )?,
        content_changed: files.iter().any(|file| {
            let path = normalize(file);
            path.starts_with("assets/") || path.starts_with("scenarios/")
        }),
        goldens_changed: files.into_iter().filter(|file| is_golden(file)).collect(),
        refactor_goldens,
    };
    let golden_commits: Vec<(String, String)> = commits
        .iter()
        .filter(|commit| commit.files.iter().any(|file| is_golden(file)))
        .map(|commit| (commit.sha.clone(), commit.message.clone()))
        .collect();
    Ok(golden_violations(&facts, &golden_commits, &changelog(root)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str, message: &str, files: &[&str], format: bool) -> CommitFacts {
        CommitFacts {
            sha: sha.to_owned(),
            message: message.to_owned(),
            files: files.iter().map(PathBuf::from).collect(),
            replay_format_changed: format,
        }
    }

    const GOLDEN: &str = "crates/fl_test_utils/tests/replays/testbed_wander.replay";
    const CHANGELOG_000: &str = "# MCRs\n\n## MCR-000 — Replay format 4 (C8, M10)\n\n- Why: x.\n";

    #[test]
    fn trailers_parse() {
        assert_eq!(
            trailer_class("feat: x\n\nChange-Class: F\nMCR: MCR-001\n"),
            Some('F')
        );
        assert_eq!(trailer_class("feat: x\n\nChange-Class:  R  \n"), Some('R'));
        assert_eq!(trailer_class("feat: x"), None);
        assert_eq!(trailer_class("feat: x\n\nChange-Class: RF\n"), None);
        assert_eq!(
            trailer_mcr("feat: x\n\nChange-Class: F\nMCR: MCR-001\n").as_deref(),
            Some("MCR-001")
        );
        assert_eq!(trailer_mcr("feat: x\n\nMCR:\n"), None);
        // Only the trailer block counts: a body line quoting a trailer does not.
        let quoted =
            "fix: x\n\nThe old commit said\nChange-Class: Q\nMCR: MCR-009\n\nChange-Class: R\n";
        assert_eq!(trailer_class(quoted), Some('R'));
        assert_eq!(trailer_mcr(quoted), None);
        assert_eq!(
            trailer_class("Change-Class: F"),
            None,
            "a subject is no trailer"
        );
        assert!(needs_class(&[PathBuf::from("Cargo.lock")]));
        assert!(needs_class(&[PathBuf::from("scenarios/a.scenario.ron")]));
        assert!(needs_class(&[PathBuf::from("features/x/src/lib.rs")]));
        assert!(needs_class(&[PathBuf::from("assets/base/rules.ron")]));
        assert!(!needs_class(&[
            PathBuf::from("xtask/src/gate.rs"),
            PathBuf::from("docs/scenarios/a.md"),
            PathBuf::from("crates.md"),
        ]));
        assert!(is_golden(Path::new(GOLDEN)));
        assert!(!is_golden(Path::new(
            "crates/fl_test_utils/tests/replays/m1_testbed.save"
        )));
        assert!(!is_golden(Path::new(
            "crates/fl_sim/tests/fixtures/replays/x.replay"
        )));
    }

    #[test]
    fn code_commit_needs_class() {
        let v = class_violations(&[commit("a1", "feat: x", &["crates/fl_sim/src/a.rs"], false)]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("a1"), "{v:?}");
        assert!(class_violations(&[commit("b2", "docs: y", &["docs/a.md"], false)]).is_empty());
        assert!(
            class_violations(&[commit(
                "c3",
                "feat: x\n\nChange-Class: N\n",
                &["crates/fl_sim/src/a.rs"],
                false
            )])
            .is_empty()
        );
    }

    #[test]
    fn class_must_be_known() {
        let v = class_violations(&[commit(
            "a1",
            "feat: x\n\nChange-Class: Q\n",
            &["crates/fl_sim/src/a.rs"],
            false,
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains('Q'), "{v:?}");
        let v = class_violations(&[commit(
            "a1",
            "feat: x\n\nChange-Class: RF\n",
            &["crates/fl_sim/src/a.rs"],
            false,
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn data_commit_needs_mcr() {
        let v = class_violations(&[commit(
            "d1",
            "tune: x\n\nChange-Class: D\n",
            &["assets/base/rules.ron"],
            false,
        )]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("MCR"), "{v:?}");
        let f = class_violations(&[commit(
            "f1",
            "feat: x\n\nChange-Class: F\n",
            &["crates/fl_sim/src/a.rs"],
            false,
        )]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            class_violations(&[commit(
                "d1",
                "tune: x\n\nChange-Class: D\nMCR: MCR-003\n",
                &["assets/base/rules.ron"],
                false
            )])
            .is_empty()
        );
    }

    #[test]
    fn mcr_must_exist_in_changelog() {
        let commits = [commit(
            "f4",
            "feat: x\n\nChange-Class: F\nMCR: MCR-004\n",
            &["crates/fl_sim/src/a.rs"],
            false,
        )];
        let v = mcr_violations(&commits, CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("MCR-004") && v[0].contains("f4"), "{v:?}");
        assert!(mcr_violations(&commits, "## MCR-004 — Lanes (C3)\n").is_empty());
        assert!(!has_mcr_heading("## MCR-0040 — x\n", "MCR-004"));
        assert!(has_mcr_heading("## MCR-004\n", "MCR-004"));
        assert!(!has_mcr_heading("### MCR-004 — x\n", "MCR-004"));

        let facts = BranchFacts {
            sim_version_changed: true,
            goldens_changed: vec![PathBuf::from(GOLDEN)],
            ..BranchFacts::default()
        };
        let golden_commits = [("f4".to_owned(), commits[0].message.clone())];
        let v = golden_violations(&facts, &golden_commits, CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("MCR-004"), "{v:?}");
    }

    #[test]
    fn golden_change_needs_mcr_and_version() {
        let goldens = BranchFacts {
            goldens_changed: vec![PathBuf::from(GOLDEN)],
            ..BranchFacts::default()
        };
        // No MCR trailer, though SIM_VERSION changed.
        let facts = BranchFacts {
            sim_version_changed: true,
            ..goldens.clone()
        };
        let no_mcr = [("a1".to_owned(), "feat: x\n\nChange-Class: F\n".to_owned())];
        let v = golden_violations(&facts, &no_mcr, CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("a1") && v[0].contains("MCR"), "{v:?}");
        // An MCR, but no version, format or content change.
        let with_mcr = [(
            "a1".to_owned(),
            "feat: x\n\nChange-Class: F\nMCR: MCR-000\n".to_owned(),
        )];
        let v = golden_violations(&goldens, &with_mcr, CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("SIM_VERSION"), "{v:?}");
        // Each of the three is enough.
        for facts in [
            BranchFacts {
                sim_version_changed: true,
                ..goldens.clone()
            },
            BranchFacts {
                replay_format_changed: true,
                ..goldens.clone()
            },
            BranchFacts {
                content_changed: true,
                ..goldens.clone()
            },
        ] {
            assert_eq!(
                golden_violations(&facts, &with_mcr, CHANGELOG_000),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn sim_version_bump_needs_a_golden_change() {
        let facts = BranchFacts {
            sim_version_changed: true,
            ..BranchFacts::default()
        };
        let v = golden_violations(&facts, &[], CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("SIM_VERSION"), "{v:?}");
        assert!(golden_violations(&BranchFacts::default(), &[], CHANGELOG_000).is_empty());
    }

    #[test]
    fn refactor_class_cannot_change_goldens() {
        let commits = [
            // An earlier commit of the branch changed the format; it does not exempt a1.
            commit(
                "f0",
                "feat!: format\n\nChange-Class: R\nMCR: MCR-000\n",
                &["crates/fl_sim/src/replay.rs"],
                true,
            ),
            commit(
                "a1",
                "refactor: x\n\nChange-Class: R\n",
                &["crates/fl_sim/src/a.rs", GOLDEN],
                false,
            ),
        ];
        let v = class_violations(&commits);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(
            v[0].contains("a1") && v[0].contains("testbed_wander.replay"),
            "{v:?}"
        );
        let e = class_violations(&[commit(
            "e1",
            "chore: bump\n\nChange-Class: E\n",
            &["Cargo.lock", GOLDEN],
            false,
        )]);
        assert_eq!(e.len(), 1, "{e:?}");
        // Class F may change goldens.
        assert!(
            class_violations(&[commit(
                "f1",
                "feat: x\n\nChange-Class: F\nMCR: MCR-001\n",
                &["crates/fl_sim/src/a.rs", GOLDEN],
                false
            )])
            .is_empty()
        );
    }

    #[test]
    fn format_commit_may_rewrite_goldens() {
        let v = class_violations(&[commit(
            "f0",
            "feat!: format\n\nChange-Class: R\nMCR: MCR-000\n",
            &["crates/fl_sim/src/replay.rs", GOLDEN],
            true,
        )]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn format_exemption_needs_equal_hash_streams() {
        let facts = BranchFacts {
            replay_format_changed: true,
            goldens_changed: vec![PathBuf::from(GOLDEN)],
            refactor_goldens: vec![("a1".to_owned(), PathBuf::from(GOLDEN), false)],
            ..BranchFacts::default()
        };
        let golden_commits = [(
            "a1".to_owned(),
            "feat!: format\n\nChange-Class: R\nMCR: MCR-000\n".to_owned(),
        )];
        let v = golden_violations(&facts, &golden_commits, CHANGELOG_000);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(
            v[0].contains("a1") && v[0].contains("testbed_wander.replay"),
            "{v:?}"
        );
    }

    #[test]
    fn later_reblesses_do_not_fail_the_format_commit() {
        // The format commit kept the hash stream of its parent; a later class F commit
        // re-blessed the golden, so HEAD's stream differs from the merge base's.
        let facts = BranchFacts {
            sim_version_changed: true,
            replay_format_changed: true,
            goldens_changed: vec![PathBuf::from(GOLDEN)],
            refactor_goldens: vec![("f0".to_owned(), PathBuf::from(GOLDEN), true)],
            ..BranchFacts::default()
        };
        let changelog = format!("{CHANGELOG_000}\n## MCR-001 — Order queue (C1)\n");
        let golden_commits = [
            (
                "f0".to_owned(),
                "feat!: format\n\nChange-Class: R\nMCR: MCR-000\n".to_owned(),
            ),
            (
                "f1".to_owned(),
                "feat: queue\n\nChange-Class: F\nMCR: MCR-001\n".to_owned(),
            ),
        ];
        let v = golden_violations(&facts, &golden_commits, &changelog);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn const_lines_parse() {
        let text = "/// doc\npub const REPLAY_FORMAT_VERSION: u32 = 4;   // 4: x\n";
        assert_eq!(const_value(text, "REPLAY_FORMAT_VERSION"), Some(4));
        assert_eq!(
            const_value("pub const SIM_VERSION: u32 = 2;\n", "SIM_VERSION"),
            Some(2)
        );
        assert_eq!(
            const_value("const SIM_VERSION: u32 = 2;\n", "SIM_VERSION"),
            None
        );
        assert_eq!(const_value(text, "SIM_VERSION"), None);
    }

    #[test]
    fn hash_streams_compare_the_hashes_only() {
        let a = serde_json::json!({"version": 3, "hashes": [[0, 1], [1, 2]], "semantic": []});
        let b = serde_json::json!({"version": 4, "hashes": [[0, 1], [1, 2]], "header": {}});
        let c = serde_json::json!({"version": 4, "hashes": [[0, 1], [1, 3]]});
        assert_eq!(hashes_of(&a), hashes_of(&b));
        assert_ne!(hashes_of(&a), hashes_of(&c));
        assert_eq!(hashes_of(&serde_json::json!({})), None);
    }
}
