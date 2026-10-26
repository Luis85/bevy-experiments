//! `cargo xtask bless`: the lead promotes pending protected artifacts (spec §12.5, ADR-0009).
//!
//! Tests and tools write protected artifacts as `*.new` files (`insta` writes `*.snap.new`). Blessing renames each
//! to its name without `.new`, replacing the accepted version. The reason goes into the commit as `Blessed: …`.

use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Promotes every `*.new` file under `paths`, which are relative to `root` and may not contain `..`. A directory
/// is searched recursively (skipping `target` and `.git`); a file is promoted if it ends in `.new`, otherwise its
/// `<file>.new` sibling is. The promotion is all-or-nothing: on a failed rename the files already promoted are
/// restored. Returns the promoted files relative to `root`, sorted. Refuses an empty reason and
/// `FL_ROLE=implementer`.
pub fn bless(root: &Path, paths: &[PathBuf], reason: &str) -> Result<Vec<PathBuf>> {
    bless_with_env(&|key| std::env::var(key).ok(), root, paths, reason)
}

/// [`bless`] with the environment read through `env`.
fn bless_with_env(
    env: &dyn Fn(&str) -> Option<String>,
    root: &Path,
    paths: &[PathBuf],
    reason: &str,
) -> Result<Vec<PathBuf>> {
    bless_as(env("FL_ROLE").as_deref(), root, paths, reason)
}

/// [`bless`] with the role given instead of read from `FL_ROLE`.
fn bless_as(
    role: Option<&str>,
    root: &Path,
    paths: &[PathBuf],
    reason: &str,
) -> Result<Vec<PathBuf>> {
    bless_with_rename(role, root, paths, reason, &mut |from, to| {
        std::fs::rename(from, to)
    })
}

fn bless_with_rename(
    role: Option<&str>,
    root: &Path,
    paths: &[PathBuf],
    reason: &str,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<Vec<PathBuf>> {
    if role.is_some_and(|r| r.trim().eq_ignore_ascii_case("implementer")) {
        bail!(
            "FL_ROLE=implementer may not bless protected artifacts; only the lead or the user does (spec §12.5). \
             If a protected test is wrong, write `BLOCKED: test X contradicts spec Y because …` and stop"
        );
    }
    if reason.trim().is_empty() {
        bail!("bless needs a non-empty --reason (it goes into the commit as `Blessed: <reason>`)");
    }
    if paths.is_empty() {
        bail!("bless needs at least one path");
    }
    let mut pending = BTreeSet::new();
    for p in paths {
        let escapes = p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        if escapes || p.as_os_str().is_empty() {
            bail!(
                "{}: bless takes paths relative to the workspace root, without `..`",
                p.display()
            );
        }
        let abs = root.join(p);
        if abs.is_dir() {
            collect_new(&abs, &mut pending)?;
        } else if abs.is_file() && is_new(&abs) {
            pending.insert(abs);
        } else {
            let sibling = PathBuf::from(format!("{}.new", abs.display()));
            if !sibling.is_file() {
                bail!(
                    "{}: no such directory or *.new file, and no {} to promote",
                    p.display(),
                    sibling.display()
                );
            }
            pending.insert(sibling);
        }
    }
    let pending: Vec<PathBuf> = pending.into_iter().collect();
    if let Some(dir) = pending
        .iter()
        .map(|n| n.with_extension(""))
        .find(|d| d.is_dir())
    {
        bail!("{} is a directory; cannot promote over it", dir.display());
    }
    let promoted = promote(&pending, rename)?;
    let mut rel: Vec<PathBuf> = promoted
        .iter()
        .map(|d| {
            d.strip_prefix(root)
                .map_or_else(|_| d.clone(), Path::to_path_buf)
        })
        .collect();
    rel.sort();
    Ok(rel)
}

/// One promotion: the `.new` file, its destination, and where the replaced destination was set aside.
struct Done {
    new: PathBuf,
    dest: PathBuf,
    backup: Option<PathBuf>,
}

/// Renames each `x.new` to `x`, setting an existing `x` aside first. If any step fails, every completed step is
/// undone, so either all files are promoted or none.
fn promote(
    pending: &[PathBuf],
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<Vec<PathBuf>> {
    let mut done: Vec<Done> = Vec::new();
    for new in pending {
        let dest = new.with_extension("");
        let backup = dest
            .exists()
            .then(|| PathBuf::from(format!("{}.bless-old", dest.display())));
        let step = match &backup {
            Some(b) => rename(&dest, b).and_then(|()| {
                rename(new, &dest).inspect_err(|_| {
                    let _ = rename(b, &dest);
                })
            }),
            None => rename(new, &dest),
        };
        if let Err(e) = step {
            for d in done.iter().rev() {
                let _ = rename(&d.dest, &d.new);
                if let Some(b) = &d.backup {
                    let _ = rename(b, &d.dest);
                }
            }
            return Err(e).with_context(|| {
                format!(
                    "promote {} (nothing was promoted; earlier files were restored)",
                    new.display()
                )
            });
        }
        done.push(Done {
            new: new.clone(),
            dest,
            backup,
        });
    }
    for d in &done {
        if let Some(b) = &d.backup {
            let _ = std::fs::remove_file(b);
        }
    }
    Ok(done.into_iter().map(|d| d.dest).collect())
}

fn is_new(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "new") && path.file_stem().is_some_and(|s| !s.is_empty())
}

fn collect_new(dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let path = entry.path();
        if ty.is_dir() {
            if entry.file_name() != "target" && entry.file_name() != ".git" {
                collect_new(&path, out)?;
            }
        } else if ty.is_file() && is_new(&path) {
            out.insert(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn bless_promotes_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("crates/x/tests/snapshots/a.snap"), "old");
        touch(&root.join("crates/x/tests/snapshots/a.snap.new"), "new");
        touch(&root.join("crates/x/tests/replays/b.replay.new"), "replay");
        touch(&root.join("crates/x/tests/replays/c.replay"), "untouched");
        let promoted = bless_as(
            None,
            root,
            &[PathBuf::from("crates/x/tests")],
            "M0 baseline",
        )
        .unwrap();
        assert_eq!(
            promoted,
            [
                PathBuf::from("crates/x/tests/replays/b.replay"),
                PathBuf::from("crates/x/tests/snapshots/a.snap"),
            ]
        );
        let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
        assert_eq!(read("crates/x/tests/snapshots/a.snap"), "new");
        assert_eq!(read("crates/x/tests/replays/b.replay"), "replay");
        assert!(!root.join("crates/x/tests/snapshots/a.snap.new").exists());
        assert!(!root.join("crates/x/tests/replays/b.replay.new").exists());
    }

    #[test]
    fn bless_accepts_a_file_or_its_new_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("s/a.snap.new"), "a");
        touch(&root.join("s/b.snap.new"), "b");
        let promoted = bless_as(
            None,
            root,
            &[PathBuf::from("s/a.snap"), PathBuf::from("s/b.snap.new")],
            "r",
        )
        .unwrap();
        assert_eq!(
            promoted,
            [PathBuf::from("s/a.snap"), PathBuf::from("s/b.snap")]
        );
        assert!(
            bless_as(None, root, &[PathBuf::from("s/missing.snap")], "r").is_err(),
            "a path with nothing to promote is an error"
        );
    }

    #[test]
    fn bless_requires_reason() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("a.snap.new"), "x");
        for reason in ["", "   "] {
            assert!(bless_as(None, dir.path(), &[PathBuf::from("a.snap.new")], reason).is_err());
        }
        assert!(dir.path().join("a.snap.new").exists(), "nothing promoted");
    }

    #[test]
    fn bless_refuses_implementer_role() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("a.snap.new"), "x");
        let err = bless_as(
            Some("implementer"),
            dir.path(),
            &[PathBuf::from("a.snap.new")],
            "looks right",
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("FL_ROLE=implementer"),
            "{err:#}"
        );
        assert!(dir.path().join("a.snap.new").exists(), "nothing promoted");
        assert!(
            bless_as(
                Some("lead"),
                dir.path(),
                &[PathBuf::from("a.snap.new")],
                "ok"
            )
            .is_ok()
        );
    }

    #[test]
    fn bless_reads_role_from_env() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("a.snap.new"), "x");
        let implementer = |key: &str| (key == "FL_ROLE").then(|| "implementer".to_owned());
        let err = bless_with_env(
            &implementer,
            dir.path(),
            &[PathBuf::from("a.snap.new")],
            "r",
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("FL_ROLE=implementer"),
            "{err:#}"
        );
        let unset = |_: &str| None;
        assert_eq!(
            bless_with_env(&unset, dir.path(), &[PathBuf::from("a.snap.new")], "r").unwrap(),
            [PathBuf::from("a.snap")]
        );
    }

    #[test]
    fn bless_rejects_absolute_and_parent_paths() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("s/a.snap.new"), "x");
        let abs = dir.path().join("s/a.snap.new");
        for p in [
            abs,
            PathBuf::from("s/../s/a.snap.new"),
            PathBuf::from("../x"),
        ] {
            assert!(
                bless_as(None, dir.path(), std::slice::from_ref(&p), "r").is_err(),
                "{} must be rejected",
                p.display()
            );
        }
        assert!(dir.path().join("s/a.snap.new").exists(), "nothing promoted");
    }

    #[test]
    fn bless_rolls_back_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("s/a.snap"), "old a");
        touch(&root.join("s/a.snap.new"), "new a");
        touch(&root.join("s/b.snap.new"), "new b");
        let mut rename = |from: &Path, to: &Path| -> io::Result<()> {
            if from.ends_with("b.snap.new") {
                return Err(io::Error::other("disk full"));
            }
            std::fs::rename(from, to)
        };
        let err =
            bless_with_rename(None, root, &[PathBuf::from("s")], "r", &mut rename).unwrap_err();
        assert!(format!("{err:#}").contains("disk full"), "{err:#}");
        let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
        assert_eq!(read("s/a.snap"), "old a", "the replaced file is restored");
        assert_eq!(
            read("s/a.snap.new"),
            "new a",
            "the promoted file is put back"
        );
        assert_eq!(read("s/b.snap.new"), "new b");
        assert!(!root.join("s/b.snap").exists());
        assert!(!root.join("s/a.snap.bless-old").exists());
    }
}
