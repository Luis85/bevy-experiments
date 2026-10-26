//! Which files and packages a branch touched since its base.

use anyhow::{Context, Result};
use cargo_metadata::Metadata;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::cmd::git;

/// Files whose change touches every package.
const WORKSPACE_WIDE: [&str; 4] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "clippy.toml",
];

/// `git merge-base <base> HEAD`.
pub fn merge_base(root: &Path, base: &str) -> Result<String> {
    Ok(git(root, &["merge-base", base, "HEAD"])?.trim().to_owned())
}

/// The environment variable that sets the gate's diff base (ruling R33).
pub const GATE_BASE_ENV: &str = "FL_GATE_BASE";
/// The git-ignored file in the worktree root whose first line sets the gate's diff base (ruling R33).
pub const GATE_BASE_FILE: &str = ".gate-base";
/// The base when nothing else sets one.
pub const DEFAULT_BASE: &str = "main";

/// The diff base of a branch: what was asked for, where that came from, and the merge base it resolves to.
pub struct Base {
    pub spec: String,
    pub source: &'static str,
    pub rev: String,
}

/// The base to diff against and its source. Precedence: `--base` > `FL_GATE_BASE` > `.gate-base` (first line)
/// > `main`; empty values are skipped.
pub fn base_spec(
    cli: Option<&str>,
    env: Option<&str>,
    file: Option<&str>,
) -> (String, &'static str) {
    let file_line = file.and_then(|f| f.lines().next());
    [
        (cli, "--base"),
        (env, GATE_BASE_ENV),
        (file_line, GATE_BASE_FILE),
    ]
    .into_iter()
    .find_map(|(v, source)| {
        v.map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| (v.to_owned(), source))
    })
    .unwrap_or_else(|| (DEFAULT_BASE.to_owned(), "default"))
}

/// Resolves the diff base (see [`base_spec`]) to `git merge-base <base> HEAD`. For an ancestor commit (a task's
/// start on an integration branch) that is the commit itself.
pub fn resolve_base(root: &Path, cli: Option<&str>) -> Result<Base> {
    let env = std::env::var(GATE_BASE_ENV).ok();
    let file = std::fs::read_to_string(root.join(GATE_BASE_FILE)).ok();
    let (spec, source) = base_spec(cli, env.as_deref(), file.as_deref());
    let rev =
        merge_base(root, &spec).with_context(|| format!("diff base `{spec}` (from {source})"))?;
    Ok(Base { spec, source, rev })
}

impl Base {
    /// `<spec> (<source>)`, e.g. `97ed4a6 (--base)` or `main (default)`.
    pub fn label(&self) -> String {
        format!("{} ({})", self.spec, self.source)
    }
}

/// Files committed since `base_rev`, plus staged, unstaged and untracked ones; repo-relative and sorted.
pub fn changed_files(root: &Path, base_rev: &str) -> Result<Vec<PathBuf>> {
    let diff = git(
        root,
        &["diff", "--name-only", "--no-renames", "-z", base_rev],
    )?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let files: BTreeSet<PathBuf> = diff
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|f| !f.is_empty())
        .map(PathBuf::from)
        .collect();
    Ok(files.into_iter().collect())
}

/// The workspace packages that own `files` (repo-relative), sorted. A workspace-wide file (the root manifest,
/// the lock file, the toolchain, the root `clippy.toml` or anything under `.cargo/`) touches every package.
pub fn touched_packages(meta: &Metadata, files: &[PathBuf]) -> Vec<String> {
    let root = meta.workspace_root.as_std_path();
    let dirs: Vec<(PathBuf, String)> = meta
        .workspace_packages()
        .iter()
        .filter_map(|p| {
            let dir = p.manifest_path.parent()?.as_std_path();
            Some((
                dir.strip_prefix(root).ok()?.to_path_buf(),
                p.name.to_string(),
            ))
        })
        .collect();
    let mut touched = BTreeSet::new();
    for file in files {
        if is_workspace_wide(file) {
            return dirs
                .into_iter()
                .map(|(_, name)| name)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
        }
        let owner = dirs
            .iter()
            .filter(|(dir, _)| !dir.as_os_str().is_empty() && file.starts_with(dir))
            .max_by_key(|(dir, _)| dir.components().count());
        if let Some((_, name)) = owner {
            touched.insert(name.clone());
        }
    }
    touched.into_iter().collect()
}

/// Whether `files` (repo-relative) change game content: anything under `assets/`, which lies under no package
/// but decides the base content's counts, the rules hash and the golden replays (final review I3).
pub fn touches_content(files: &[PathBuf]) -> bool {
    files.iter().any(|f| f.starts_with("assets"))
}

fn is_workspace_wide(file: &Path) -> bool {
    WORKSPACE_WIDE.iter().any(|w| file == Path::new(w)) || file.starts_with(".cargo")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn touched_maps_files_to_packages() {
        let meta = fixture::metadata("metadata_ok.json");
        let files = [
            PathBuf::from("crates/fl_sim/src/x.rs"),
            PathBuf::from("docs/a.md"),
        ];
        assert_eq!(touched_packages(&meta, &files), ["fl_sim"]);
    }

    #[test]
    fn base_precedence_flag_env_file_default() {
        let file = Some("abc123\nignored second line\n");
        assert_eq!(
            base_spec(Some("f00d"), Some("e11v"), file),
            ("f00d".to_owned(), "--base")
        );
        assert_eq!(
            base_spec(None, Some("e11v"), file),
            ("e11v".to_owned(), "FL_GATE_BASE")
        );
        assert_eq!(
            base_spec(None, Some("  "), file),
            ("abc123".to_owned(), ".gate-base"),
            "an empty value is skipped"
        );
        assert_eq!(
            base_spec(Some(""), None, Some("\n")),
            ("main".to_owned(), "default")
        );
    }

    #[test]
    fn root_manifest_touches_everything() {
        let meta = fixture::metadata("metadata_ok.json");
        let mut all: Vec<String> = meta
            .workspace_packages()
            .iter()
            .map(|p| p.name.to_string())
            .collect();
        all.sort();
        assert!(all.len() > 10, "fixture lists the whole workspace");
        assert_eq!(touched_packages(&meta, &[PathBuf::from("Cargo.toml")]), all);
    }
}
