//! The gate's `protected` row: changed protected paths that bypassed the Write/Edit hook (ruling R34, spec §12.5,
//! ADR-0009). A shell redirect, a tool's output or `git checkout` can change a protected file without the hook
//! seeing it; the diff cannot hide it.
//!
//! - `gate task`: any changed protected file fails; implementers never change them, the lead blesses.
//! - `gate pr`: a changed protected file passes only if it has no uncommitted change and every commit in
//!   `base..HEAD` that touches it carries a `Blessed: <reason>` trailer (written by `cargo xtask bless`).
//!
//! Pending `*.new` files are never protected artifacts themselves.

use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::changed;
use crate::cmd::git;
use crate::owners::{self, Owners};

/// A commit `(sha, message)`.
pub type Commit = (String, String);

pub struct Violation {
    pub file: PathBuf,
    pub reason: String,
}

/// The changed files under `[protected]`, except pending `*.new` files.
pub fn changed_protected(owners: &Owners, files: &[PathBuf]) -> Vec<PathBuf> {
    files
        .iter()
        .filter(|f| owners.is_protected(f) && f.extension().is_none_or(|e| e != "new"))
        .cloned()
        .collect()
}

/// A `Blessed: <reason>` trailer line with a non-empty reason.
pub fn has_blessed_trailer(message: &str) -> bool {
    message.lines().any(|l| {
        l.trim_end()
            .strip_prefix("Blessed:")
            .is_some_and(|r| !r.trim().is_empty())
    })
}

/// The violations among the changed protected `files`. At `pr` level, `uncommitted` holds files with working-tree
/// changes and `commits(file)` lists `(sha, message)` of the range's commits that touch `file`.
pub fn violations(
    pr: bool,
    files: &[PathBuf],
    uncommitted: &BTreeSet<PathBuf>,
    commits: &dyn Fn(&Path) -> Result<Vec<Commit>>,
) -> Result<Vec<Violation>> {
    let mut out = Vec::new();
    for file in files {
        let violation = |reason: String| Violation {
            file: file.clone(),
            reason,
        };
        if !pr {
            out.push(violation(
                "changed protected path; only the lead promotes it with `cargo xtask bless`".into(),
            ));
            continue;
        }
        if uncommitted.contains(file) {
            out.push(violation("uncommitted change to a protected path".into()));
            continue;
        }
        let touching = commits(file)?;
        if touching.is_empty() {
            out.push(violation(
                "changed, but no commit in range touches it".into(),
            ));
        }
        for (sha, message) in &touching {
            if !has_blessed_trailer(message) {
                let short: String = sha.chars().take(7).collect();
                out.push(violation(format!(
                    "commit {short} changes it without a `Blessed:` trailer"
                )));
            }
        }
    }
    Ok(out)
}

/// The changed protected files since `base_rev` (committed, staged, unstaged, untracked) and their violations.
pub fn run(root: &Path, base_rev: &str, pr: bool) -> Result<(usize, Vec<Violation>)> {
    let owners = owners::load_owners(root)?;
    let files = changed::changed_files(root, base_rev)?;
    let protected = changed_protected(&owners, &files);
    if protected.is_empty() {
        return Ok((0, Vec::new()));
    }
    let mut uncommitted = BTreeSet::new();
    if pr {
        let worktree = git(root, &["diff", "--name-only", "--no-renames", "-z", "HEAD"])?;
        let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
        uncommitted = worktree
            .split('\0')
            .chain(untracked.split('\0'))
            .filter(|f| !f.is_empty())
            .map(PathBuf::from)
            .collect();
    }
    let range = format!("{base_rev}..HEAD");
    let commits = |file: &Path| -> Result<Vec<Commit>> {
        let log = git(
            root,
            &[
                "log",
                "--format=%H%x1f%B%x1e",
                &range,
                "--",
                &owners::normalize(file),
            ],
        )?;
        Ok(log
            .split('\x1e')
            .filter_map(|entry| {
                let (sha, message) = entry.trim_start().split_once('\x1f')?;
                Some((sha.to_owned(), message.to_owned()))
            })
            .collect())
    };
    let v = violations(pr, &protected, &uncommitted, &commits)?;
    Ok((protected.len(), v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owners::parse_owners;

    fn owners() -> Owners {
        parse_owners(include_str!("../../OWNERS.toml")).expect("OWNERS.toml parses")
    }

    fn no_commits(_: &Path) -> Result<Vec<Commit>> {
        Ok(Vec::new())
    }

    #[test]
    fn task_level_fails_on_changed_snapshot() {
        let files = [
            PathBuf::from("crates/fl_view/tests/snapshots/x.snap"),
            PathBuf::from("crates/fl_view/src/lib.rs"),
        ];
        let protected = changed_protected(&owners(), &files);
        assert_eq!(
            protected,
            [PathBuf::from("crates/fl_view/tests/snapshots/x.snap")]
        );
        let v = violations(false, &protected, &BTreeSet::new(), &no_commits).unwrap();
        assert_eq!(v.len(), 1);
        assert!(v[0].reason.contains("cargo xtask bless"), "{}", v[0].reason);
    }

    #[test]
    fn pending_new_files_are_ignored() {
        let files = [
            PathBuf::from("crates/fl_view/tests/snapshots/x.snap.new"),
            PathBuf::from("docs/perf/baselines/m0.json.new"),
        ];
        assert!(changed_protected(&owners(), &files).is_empty());
    }

    #[test]
    fn pr_level_needs_blessed_commits() {
        let snap = PathBuf::from("crates/fl_view/tests/snapshots/x.snap");
        let files = [snap.clone()];
        let blessed = |_: &Path| -> Result<Vec<Commit>> {
            Ok(vec![(
                "0123456789abcdef".into(),
                "test(fl_view): new snapshot\n\nBlessed: M0 baseline\nCo-Authored-By: x\n".into(),
            )])
        };
        let none = BTreeSet::new();
        assert!(
            violations(true, &files, &none, &blessed)
                .unwrap()
                .is_empty()
        );

        let mixed = |_: &Path| -> Result<Vec<Commit>> {
            Ok(vec![
                ("aaaaaaa1".into(), "chore: x\n\nBlessed: ok\n".into()),
                ("bbbbbbb2".into(), "feat: tweak snapshot\n".into()),
            ])
        };
        let v = violations(true, &files, &none, &mixed).unwrap();
        assert_eq!(v.len(), 1);
        assert!(v[0].reason.contains("bbbbbbb"), "{}", v[0].reason);

        let dirty = BTreeSet::from([snap]);
        let v = violations(true, &files, &dirty, &blessed).unwrap();
        assert_eq!(v.len(), 1, "an uncommitted change is never blessed");
    }

    #[test]
    fn blessed_trailer_needs_a_reason() {
        assert!(has_blessed_trailer("x\n\nBlessed: M0 baseline"));
        assert!(!has_blessed_trailer("x\n\nBlessed:   \n"));
        assert!(!has_blessed_trailer("x mentions Blessed: inline"));
    }
}
