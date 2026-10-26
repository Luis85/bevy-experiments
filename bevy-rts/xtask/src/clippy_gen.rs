//! `cargo xtask gen-clippy`: the per-crate `clippy.toml` files, generated from `xtask/clippy-template.toml`
//! (spec §12.2). Clippy reads only the nearest `clippy.toml`, so every crate with a profile gets its own file and
//! every other crate reads the root file.

use anyhow::{Context, Result, bail, ensure};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cmd;

pub const TEMPLATE_PATH: &str = "xtask/clippy-template.toml";

/// Path roots that every crate can resolve.
const ALWAYS_RESOLVABLE: [&str; 3] = ["std", "core", "alloc"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplate {
    profile: BTreeMap<String, RawProfile>,
    #[serde(default)]
    ban: Vec<Ban>,
    #[serde(default)]
    pending: Vec<Pending>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    #[serde(default)]
    crates: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    root: bool,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Type,
    Method,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ban {
    kind: Kind,
    path: String,
    reason: String,
    profiles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    #[serde(rename = "crate")]
    krate: String,
    path: String,
    until: String,
}

struct Profile {
    name: String,
    crates: GlobSet,
    exclude: GlobSet,
    root: bool,
}

/// The parsed `clippy-template.toml`.
pub struct Template {
    profiles: Vec<Profile>,
    bans: Vec<Ban>,
    pending: Vec<Pending>,
}

fn globs(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("bad crate pattern {p:?}"))?);
    }
    Ok(b.build()?)
}

/// The crate a path starts from; `None` for a primitive type such as `f32`.
fn path_root(path: &str) -> Option<&str> {
    path.split_once("::").map(|(root, _)| root)
}

impl Template {
    pub fn parse(text: &str) -> Result<Template> {
        let raw: RawTemplate = toml::from_str(text).context("parse clippy template")?;
        let profiles = raw
            .profile
            .into_iter()
            .map(|(name, p)| {
                Ok(Profile {
                    crates: globs(&p.crates)?,
                    exclude: globs(&p.exclude)?,
                    root: p.root,
                    name,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let t = Template {
            profiles,
            bans: raw.ban,
            pending: raw.pending,
        };
        let roots = t.profiles.iter().filter(|p| p.root).count();
        ensure!(
            roots == 1,
            "the template needs exactly one root profile, found {roots}"
        );
        for ban in &t.bans {
            for name in &ban.profiles {
                let profile = t.profiles.iter().find(|p| &p.name == name);
                let Some(profile) = profile else {
                    bail!("ban {} names unknown profile {name}", ban.path);
                };
                // The root file is shared by crates with different graphs, so it carries std paths only.
                if profile.root
                    && path_root(&ban.path).is_some_and(|r| !ALWAYS_RESOLVABLE.contains(&r))
                {
                    bail!(
                        "root profile {name} can only ban std paths, not {}",
                        ban.path
                    );
                }
            }
        }
        for p in &t.pending {
            let Some(profile) = t.profile_of(&p.krate) else {
                bail!(
                    "pending entry names crate {}, which has no clippy profile",
                    p.krate
                );
            };
            ensure!(
                t.bans
                    .iter()
                    .any(|b| b.path == p.path && b.profiles.contains(&profile.name)),
                "pending entry for {} names {}, which is not a ban in profile `{}`",
                p.krate,
                p.path,
                profile.name
            );
        }
        Ok(t)
    }

    pub fn load(root: &Path) -> Result<Template> {
        let path = root.join(TEMPLATE_PATH);
        Template::parse(
            &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
        )
    }

    /// The profile of a crate with its own file; `None` when it reads the root file.
    fn profile_of(&self, krate: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| !p.root && p.crates.is_match(krate) && !p.exclude.is_match(krate))
    }

    fn root_profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.root)
            .expect("checked in parse")
    }
}

/// The `clippy.toml` for `krate`, or `None` when the crate reads the root file. `resolvable(c)` says whether the
/// crate `c` is in `krate`'s normal dependency graph; a ban on a path outside it is left out.
pub fn render(t: &Template, krate: &str, resolvable: &dyn Fn(&str) -> bool) -> Option<String> {
    let profile = t.profile_of(krate)?;
    Some(render_profile(t, profile, Some(krate), resolvable))
}

/// The root `clippy.toml` (the root profile, std paths only).
pub fn render_root(t: &Template) -> String {
    render_profile(t, t.root_profile(), None, &|_| false)
}

fn render_profile(
    t: &Template,
    profile: &Profile,
    krate: Option<&str>,
    resolvable: &dyn Fn(&str) -> bool,
) -> String {
    let mut out = format!(
        "# Generated by `cargo xtask gen-clippy` from {TEMPLATE_PATH} (profile `{}`). Do not edit.\n",
        profile.name
    );
    let applies = |b: &&Ban| b.profiles.contains(&profile.name);
    let pending: Vec<&Pending> = t
        .pending
        .iter()
        .filter(|p| {
            Some(p.krate.as_str()) == krate
                && t.bans.iter().filter(applies).any(|b| b.path == p.path)
        })
        .collect();
    for p in &pending {
        out.push_str(&format!("# pending: {} until {}\n", p.path, p.until));
    }
    for (kind, key) in [
        (Kind::Type, "disallowed-types"),
        (Kind::Method, "disallowed-methods"),
    ] {
        let entries: Vec<&Ban> = t
            .bans
            .iter()
            .filter(applies)
            .filter(|b| b.kind == kind && !pending.iter().any(|p| p.path == b.path))
            .filter(|b| {
                path_root(&b.path).is_none_or(|r| ALWAYS_RESOLVABLE.contains(&r) || resolvable(r))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{key} = [\n"));
        for b in entries {
            out.push_str(&format!(
                "    {{ path = \"{}\", reason = \"{}\" }},\n",
                escape(&b.path),
                escape(&b.reason)
            ));
        }
        out.push_str("]\n");
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Every generated file (repo-relative path, contents): one per workspace crate with a profile, plus the root.
fn generate(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let t = Template::load(root)?;
    let meta = cmd::workspace(root)?;
    let ws_root = meta.workspace_root.as_std_path();
    let mut crates: Vec<(String, PathBuf)> = Vec::new();
    for pkg in meta.workspace_packages() {
        if t.profile_of(&pkg.name).is_some() {
            let dir = pkg
                .manifest_path
                .parent()
                .context("manifest has a parent")?
                .as_std_path();
            crates.push((
                pkg.name.to_string(),
                dir.strip_prefix(ws_root)?.to_path_buf(),
            ));
        }
    }
    crates.sort();
    if let Some(p) = t
        .pending
        .iter()
        .find(|p| !crates.iter().any(|(n, _)| *n == p.krate))
    {
        bail!(
            "pending entry names crate {}, which is not a workspace package",
            p.krate
        );
    }
    let names: Vec<String> = crates.iter().map(|(n, _)| n.clone()).collect();
    let trees = cmd::cargo_trees(root, &names, &["-e", "normal"])?;
    let mut files = vec![(PathBuf::from("clippy.toml"), render_root(&t))];
    for ((name, dir), tree) in crates.iter().zip(&trees) {
        // Crate names as paths see them: `foo-bar` is `foo_bar`.
        let graph: BTreeSet<String> = tree
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .map(|c| c.replace('-', "_"))
            .collect();
        let text = render(&t, name, &|c| graph.contains(c)).expect("crate has a profile");
        files.push((dir.join("clippy.toml"), text));
    }
    Ok(files)
}

/// Writes every generated file; returns how many.
pub fn write(root: &Path) -> Result<usize> {
    let files = generate(root)?;
    for (path, text) in &files {
        fs::write(root.join(path), text).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(files.len())
}

/// The generated files that differ from the template, plus any other `clippy.toml` or `.clippy.toml` anywhere in
/// the repository (tracked or untracked, not ignored): it would shadow the generated configuration of every crate
/// below it. Empty when everything is up to date.
pub fn drift(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(check(root)?.1)
}

/// The number of generated files and the drifted ones (see [`drift`]).
pub fn check(root: &Path) -> Result<(usize, Vec<PathBuf>)> {
    let files = generate(root)?;
    let mut drifted = Vec::new();
    for (path, text) in &files {
        let on_disk = fs::read_to_string(root.join(path)).unwrap_or_default();
        if on_disk.replace("\r\n", "\n") != *text {
            drifted.push(path.clone());
        }
    }
    let listed = cmd::git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    drifted.extend(stray_configs(
        listed.split('\0').map(Path::new),
        &files,
        root,
    ));
    drifted.sort();
    drifted.dedup();
    Ok((files.len(), drifted))
}

/// Clippy config files among `listed` (repo-relative) that exist on disk and are not generated.
fn stray_configs<'a>(
    listed: impl Iterator<Item = &'a Path>,
    generated: &[(PathBuf, String)],
    root: &Path,
) -> Vec<PathBuf> {
    listed
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n == "clippy.toml" || n == ".clippy.toml")
        })
        .filter(|p| !generated.iter().any(|(g, _)| g == p) && root.join(p).exists())
        .map(Path::to_path_buf)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> Template {
        Template::parse(include_str!("../clippy-template.toml")).unwrap()
    }

    #[test]
    fn render_omits_unresolvable_paths() {
        let t = template();
        let graph = |c: &str| matches!(c, "fl_math" | "fl_protocol" | "serde");
        let view = render(&t, "fl_view", &graph).unwrap();
        assert!(!view.contains("bevy_time::Time"), "{view}");
        let protocol = render(&t, "fl_protocol", &graph).unwrap();
        assert!(protocol.contains("\"f32\""), "{protocol}");
        let sim = render(&t, "fl_sim", &|_| true).unwrap();
        assert!(sim.contains("\"bevy_time::Time\""), "{sim}");
        assert_eq!(render(&t, "fl_client", &|_| true), None);
    }

    #[test]
    fn pending_entry_is_omitted_with_comment() {
        let text =
            format!("{MINI}[[pending]]\ncrate = \"fl_sim\"\npath = \"f32\"\nuntil = \"Task 99\"\n");
        let t = Template::parse(&text).unwrap();
        let sim = render(&t, "fl_sim", &|_| true).unwrap();
        assert!(!sim.contains("\"f32\""), "{sim}");
        assert!(sim.contains("# pending: f32 until Task 99"), "{sim}");
        let without = render(&Template::parse(MINI).unwrap(), "fl_sim", &|_| true).unwrap();
        assert!(without.contains("\"f32\""), "{without}");
    }

    #[test]
    fn sim_bans_pcg32() {
        // Task 10 removed the last sim use of Pcg32 and its pending entry.
        let t = template();
        let sim = render(&t, "fl_sim", &|_| true).unwrap();
        assert!(sim.contains("\"fl_math::Pcg32\""), "{sim}");
        assert!(!sim.contains("# pending"), "{sim}");
        let view = render(&t, "fl_view", &|_| true).unwrap();
        assert!(view.contains("\"fl_math::Pcg32\""), "{view}");
    }

    const MINI: &str = r#"
[profile.sim]
crates = ["fl_sim"]
[profile.edge]
crates = ["fl_math"]
[profile.client]
root = true
[[ban]]
kind = "type"
path = "f32"
reason = "r"
profiles = ["sim"]
"#;

    fn pending_error(krate: &str, path: &str) -> String {
        let text = format!(
            "{MINI}[[pending]]\ncrate = \"{krate}\"\npath = \"{path}\"\nuntil = \"Task 99\"\n"
        );
        match Template::parse(&text) {
            Ok(_) => String::new(),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn pending_entries_are_validated() {
        assert_eq!(pending_error("fl_sim", "f32"), "");
        assert!(
            pending_error("fl_client", "f32").contains("has no clippy profile"),
            "a crate that reads the root file cannot have pending entries"
        );
        assert!(pending_error("fl_nope", "f32").contains("has no clippy profile"));
        assert!(
            pending_error("fl_math", "f32").contains("not a ban in profile `edge`"),
            "f32 is not banned in fl_math's profile"
        );
        assert!(pending_error("fl_sim", "f64").contains("not a ban in profile `sim`"));
    }

    #[test]
    fn stray_clippy_config_anywhere_is_drift() {
        let dir = tempfile::tempdir().unwrap();
        let listed = [
            "clippy.toml",
            "crates/clippy.toml",
            "crates/fl_sim/clippy.toml",
            "tools/.clippy.toml",
            "gone/clippy.toml",
            "README.md",
        ];
        for f in listed.iter().filter(|f| !f.starts_with("gone")) {
            let path = dir.path().join(f);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        let generated = [
            (PathBuf::from("clippy.toml"), String::new()),
            (
                Path::new("crates").join("fl_sim").join("clippy.toml"),
                String::new(),
            ),
        ];
        let stray = stray_configs(listed.iter().map(Path::new), &generated, dir.path());
        assert_eq!(
            stray,
            [
                PathBuf::from("crates/clippy.toml"),
                PathBuf::from("tools/.clippy.toml")
            ]
        );
    }
}
