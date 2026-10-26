//! `cargo xtask check-scope`: a branch touches only its track's paths (spec §13.2, ADR-0011).

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use crate::changed;
use crate::owners::{self, Owners};

/// The changed files outside `tracks`, with the track that owns each (`None` = no track owns it).
pub fn check_scope(
    owners: &Owners,
    files: &[PathBuf],
    tracks: &[String],
) -> Vec<(PathBuf, Option<String>)> {
    files
        .iter()
        .filter_map(|f| {
            let owner = owners.track_of(f);
            if owner.is_some_and(|o| tracks.iter().any(|t| t == o)) {
                None
            } else {
                Some((f.clone(), owner.map(str::to_owned)))
            }
        })
        .collect()
}

/// The `--track` values (clap already split `a,b`), or else the comma- or space-separated tracks in `FL_TRACK`
/// (`fl_track`). Values are trimmed and empty ones dropped.
pub fn effective_tracks(cli: &[String], fl_track: Option<&str>) -> Vec<String> {
    let split = |s: &str| -> Vec<String> {
        s.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let from_cli: Vec<String> = cli.iter().flat_map(|t| split(t)).collect();
    if !from_cli.is_empty() {
        return from_cli;
    }
    split(fl_track.unwrap_or_default())
}

/// The `FL_TRACK` environment variable, if set.
pub fn fl_track_env() -> Option<String> {
    std::env::var("FL_TRACK").ok()
}

/// The result of a scope check on a branch.
pub struct ScopeReport {
    pub tracks: Vec<String>,
    pub files: usize,
    pub outside: Vec<(PathBuf, Option<String>)>,
}

impl ScopeReport {
    /// `check-scope: OK (<n> files in <tracks>)`, or `check-scope: FAIL` and one `  <file> → <owner|unowned>`
    /// line per out-of-scope file.
    pub fn render(&self) -> String {
        if self.outside.is_empty() {
            return format!(
                "check-scope: OK ({} files in {})\n",
                self.files,
                self.tracks.join(", ")
            );
        }
        let mut out = String::from("check-scope: FAIL\n");
        out.push_str(&self.outside_lines());
        out
    }

    /// `  <file> → <owner|unowned>` per out-of-scope file.
    pub fn outside_lines(&self) -> String {
        self.outside
            .iter()
            .map(|(f, owner)| {
                format!(
                    "  {} → {}\n",
                    owners::normalize(f),
                    owner.as_deref().unwrap_or("unowned")
                )
            })
            .collect()
    }
}

/// Checks the files changed since `base_rev` (committed, staged, unstaged and untracked) against `tracks`, which
/// must be non-empty and name tracks in `OWNERS.toml`.
pub fn run(root: &Path, base_rev: &str, tracks: &[String]) -> Result<ScopeReport> {
    if tracks.is_empty() {
        bail!("check-scope needs --track <name> (or FL_TRACK)");
    }
    let owners = owners::load_owners(root)?;
    let known: Vec<&str> = owners.track_names().collect();
    if let Some(unknown) = tracks.iter().find(|t| !known.contains(&t.as_str())) {
        bail!(
            "unknown track {unknown:?}; {} defines: {}",
            owners::OWNERS_PATH,
            known.join(", ")
        );
    }
    let files = changed::changed_files(root, base_rev)?;
    Ok(ScopeReport {
        tracks: tracks.to_vec(),
        files: files.len(),
        outside: check_scope(&owners, &files, tracks),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owners::parse_owners;

    fn owners() -> Owners {
        parse_owners(include_str!("../../OWNERS.toml")).expect("OWNERS.toml parses")
    }

    #[test]
    fn unowned_path_reported() {
        assert_eq!(
            check_scope(
                &owners(),
                &[PathBuf::from("foo/bar.rs")],
                &["sim-core".to_owned()]
            ),
            [(PathBuf::from("foo/bar.rs"), None)]
        );
    }

    #[test]
    fn scope_flags_out_of_track_file() {
        let files = [
            PathBuf::from("crates/fl_sim/src/movement.rs"),
            PathBuf::from("crates/fl_client/src/view.rs"),
        ];
        assert_eq!(
            check_scope(&owners(), &files, &["sim-core".to_owned()]),
            [(
                PathBuf::from("crates/fl_client/src/view.rs"),
                Some("client".to_owned())
            )]
        );
        assert!(
            check_scope(
                &owners(),
                &files,
                &["sim-core".to_owned(), "client".to_owned()]
            )
            .is_empty(),
            "several tracks widen the scope"
        );
    }

    #[test]
    fn tracks_fall_back_to_fl_track() {
        assert_eq!(effective_tracks(&["a".to_owned()], Some("b")), ["a"]);
        assert_eq!(
            effective_tracks(&[], Some("sim-core, client")),
            ["sim-core", "client"]
        );
        assert!(effective_tracks(&[], Some("  ")).is_empty());
        assert!(effective_tracks(&[], None).is_empty());
        assert_eq!(
            effective_tracks(&["contract, docs".to_owned()], None),
            ["contract", "docs"],
            "a --track value is split and trimmed like FL_TRACK"
        );
    }
}
