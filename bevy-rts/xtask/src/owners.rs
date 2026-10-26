//! `OWNERS.toml`: which track owns a path, and which paths are protected (spec §12.5, §13.2; ADR-0009, ADR-0011).

use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::path::Path;

pub const OWNERS_PATH: &str = "OWNERS.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnersFile {
    track: Vec<TrackDef>,
    protected: PathList,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackDef {
    name: String,
    paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathList {
    paths: Vec<String>,
}

/// The parsed ownership map. Tracks keep their file order; the first matching track owns a path.
pub struct Owners {
    tracks: Vec<(String, GlobSet)>,
    protected: GlobSet,
}

/// Reads and parses `<root>/OWNERS.toml`.
pub fn load_owners(root: &Path) -> Result<Owners> {
    let path = root.join(OWNERS_PATH);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    parse_owners(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn parse_owners(text: &str) -> Result<Owners> {
    let file: OwnersFile = toml::from_str(text)?;
    let mut tracks: Vec<(String, GlobSet)> = Vec::new();
    for t in file.track {
        if t.name.trim().is_empty() || t.name.trim() != t.name {
            bail!(
                "track name {:?} is empty or has surrounding whitespace",
                t.name
            );
        }
        if tracks.iter().any(|(n, _)| *n == t.name) {
            bail!("track {:?} is defined twice", t.name);
        }
        if t.paths.is_empty() {
            bail!("track {:?} has no paths", t.name);
        }
        let set = glob_set(&t.paths).with_context(|| format!("track {:?}", t.name))?;
        tracks.push((t.name, set));
    }
    let protected = glob_set_ci(&file.protected.paths).context("[protected]")?;
    Ok(Owners { tracks, protected })
}

/// Globs with `/` as a literal separator: `*` stays within one segment, `**` spans segments.
pub fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    glob_set_with(patterns, false)
}

/// [`glob_set`] ignoring case: protected paths and `--only` confinement must hold for any spelling on Windows.
pub fn glob_set_ci(patterns: &[String]) -> Result<GlobSet> {
    glob_set_with(patterns, true)
}

fn glob_set_with(patterns: &[String], case_insensitive: bool) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = GlobBuilder::new(p)
            .literal_separator(true)
            .case_insensitive(case_insensitive)
            .build()
            .with_context(|| format!("glob {p:?}"))?;
        builder.add(glob);
    }
    Ok(builder.build()?)
}

/// A repo-relative path with `/` separators and no leading `./`, as the globs expect.
pub fn normalize(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut s = s.as_str();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    s.to_owned()
}

impl Owners {
    /// The first track whose globs match `path` (repo-relative), or `None` when no track owns it.
    pub fn track_of(&self, path: &Path) -> Option<&str> {
        let p = normalize(path);
        self.tracks
            .iter()
            .find(|(_, set)| set.is_match(Path::new(&p)))
            .map(|(name, _)| name.as_str())
    }

    /// Whether `path` (repo-relative) is a protected test, snapshot, scenario or baseline (case-insensitive).
    pub fn is_protected(&self, path: &Path) -> bool {
        self.protected.is_match(Path::new(&normalize(path)))
    }

    pub fn track_names(&self) -> impl Iterator<Item = &str> {
        self.tracks.iter().map(|(n, _)| n.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owners() -> Owners {
        parse_owners(include_str!("../../OWNERS.toml")).expect("OWNERS.toml parses")
    }

    #[test]
    fn first_matching_track_wins() {
        let o = owners();
        assert_eq!(
            o.track_of(Path::new("crates/fl_sim/src/builder.rs")),
            Some("contract")
        );
        assert_eq!(
            o.track_of(Path::new("crates/fl_sim/src/movement.rs")),
            Some("sim-core")
        );
        assert_eq!(
            o.track_of(Path::new(r"crates\fl_sim\src\builder.rs")),
            Some("contract"),
            "Windows separators are normalized"
        );
        assert_eq!(
            o.track_of(Path::new("crates/fl_sim/clippy.toml")),
            Some("contract")
        );
        assert_eq!(
            o.track_of(Path::new("crates/fl_sim/src/x/clippy.toml")),
            Some("sim-core"),
            "`*` stays within one segment"
        );
        assert_eq!(o.track_of(Path::new("docs/research/x.md")), Some("probes"));
        assert_eq!(
            o.track_of(Path::new("docs/adr/0001-x.md")),
            Some("contract")
        );
        assert_eq!(
            o.track_of(Path::new("docs/superpowers/plans/p.md")),
            Some("docs")
        );
        for repo_config in [".gitignore", ".gitattributes"] {
            assert_eq!(
                o.track_of(Path::new(repo_config)),
                Some("contract"),
                "ruling R35"
            );
        }
    }

    #[test]
    fn protected_globs() {
        let o = owners();
        for p in [
            "crates/fl_sim/tests/acceptance/a.rs",
            "crates/fl_view/tests/snapshots/x.snap",
            "crates/fl_sim/src/snapshots/fl_sim__order.snap",
            "crates/fl_sim/tests/holdout/h.rs",
            "crates/fl_sim/tests/replays/r.replay",
            "scenarios/duel.ron",
            "docs/design/mechanics/fire.md",
            "docs/perf/baselines/m0.json",
            "docs/perf/budgets.ron",
            "docs/perf/M0.json",
            "docs/perf/M12.json",
            "crates/fl_sim/tests/hpa.proptest-regressions",
        ] {
            assert!(o.is_protected(Path::new(p)), "{p} must be protected");
        }
        for p in [
            "assets/base/scenarios/testbed.scenario.ron",
            "crates/fl_sim/tests/fixtures/core_mod/scenarios/duel.scenario.ron",
            "crates/fl_sim/tests/movement.rs",
            "docs/perf/M0.md",
        ] {
            assert!(!o.is_protected(Path::new(p)), "{p} must not be protected");
        }
    }

    #[test]
    fn duplicate_track_is_rejected() {
        let text = "[[track]]\nname = \"a\"\npaths = [\"x/**\"]\n[[track]]\nname = \"a\"\npaths = [\"y/**\"]\n\
                    [protected]\npaths = []\n";
        assert!(parse_owners(text).is_err());
    }
}
