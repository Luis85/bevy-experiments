//! Sets `FL_GIT_REV` for [`build_id`](src/build.rs): the short hash of the checked-out
//! commit, with `-dirty` when tracked files differ from it, so `MatchHeader.build_id`
//! tells builds apart (ADR-0007: a lockstep peer with different code fails the handshake; a
//! replay from another build warns). Without git (a source archive) nothing is set and the
//! build id is `"dev"`.
//!
//! The script reruns when `HEAD` or the branch it points to moves (in a worktree, `HEAD`
//! lives in the worktree's git dir and the branch refs in the common dir); it watches only
//! paths that exist. The dirty flag is the state of the tree at that moment: edits after the
//! last commit or checkout do not rerun the script, so they build as the clean revision until
//! `HEAD` next moves. `--path-format=absolute` needs git 2.31 or newer; with an older git the
//! ref is not watched and the revision goes stale until `build.rs` changes.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The trimmed stdout of `git <args>` run in this crate's directory, if it succeeds.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Ask Cargo to rerun this script when `path` changes.
fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

fn main() {
    let Some(rev) = git(&["rev-parse", "--short", "HEAD"]) else {
        // Not a git checkout, or no git: build id "dev". Rerun if this script changes.
        rerun_if_changed(Path::new("build.rs"));
        return;
    };
    // `diff-index` compares tracked files with HEAD without writing the index (unlike
    // `git status`, whose index refresh would make this script rerun on every build).
    let dirty = Command::new("git")
        .args(["diff-index", "--quiet", "HEAD", "--"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .is_ok_and(|status| !status.success());
    let suffix = if dirty { "-dirty" } else { "" };
    println!("cargo:rustc-env=FL_GIT_REV={rev}{suffix}");

    // Only paths that exist: Cargo treats a missing `rerun-if-changed` path as stale and
    // would rerun this script (and rebuild fl_sim and every dependent) on every invocation.
    rerun_if_changed(Path::new("build.rs"));
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        rerun_if_exists(&PathBuf::from(git_dir).join("HEAD"));
    }
    if let Some(common) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        let common = PathBuf::from(common);
        let branch = git(&["symbolic-ref", "-q", "HEAD"]).map(|branch| common.join(branch));
        let packed = common.join("packed-refs");
        match branch {
            // A loose branch ref changes on every commit.
            Some(loose) if loose.exists() => rerun_if_changed(&loose),
            // A packed-only ref: a commit writes a loose ref under refs/heads (scanned
            // recursively), and `git pack-refs` rewrites packed-refs.
            _ => {
                rerun_if_exists(&packed);
                rerun_if_exists(&common.join("refs").join("heads"));
            }
        }
    }
}

/// [`rerun_if_changed`] if `path` exists.
fn rerun_if_exists(path: &Path) {
    if path.exists() {
        rerun_if_changed(path);
    }
}
