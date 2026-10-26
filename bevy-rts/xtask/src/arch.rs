//! `cargo xtask arch-check`: the allowed workspace edges (spec §5.1, ADR-0001) and the banned external crates
//! per package, read from `cargo tree` one package at a time so feature unification cannot hide an edge.

use anyhow::Result;
use cargo_metadata::{DependencyKind, Metadata, Package};
use std::collections::BTreeSet;
use std::path::Path;

use crate::cmd;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub krate: String,
    pub detail: String,
}

/// A workspace crate's place in the architecture, from its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// `fl_math`, `fl_protocol`, `fl_view`: no Bevy at all.
    Pure,
    /// `fl_content`: the `bevy_ecs` family only.
    Content,
    /// `fl_sim`.
    Sim,
    /// `fl_feat_<name>`, the sim half of a feature.
    Feature(String),
    /// `fl_feat_<name>_client`, the client half of a feature.
    FeatureClient(String),
    /// `fl_catalog`, `fl_observe`, `fl_test_utils`, `fl_cli`.
    Tool,
    /// `fl_client`.
    Client,
    /// `fl_game`, the only crate that sees both halves.
    Game,
    Xtask,
    /// A crate with no row in the allowed-edges table.
    Unknown,
}

impl Role {
    pub fn of(krate: &str) -> Role {
        match krate {
            "fl_math" | "fl_protocol" | "fl_view" => Role::Pure,
            "fl_content" => Role::Content,
            "fl_sim" => Role::Sim,
            "fl_catalog" | "fl_observe" | "fl_test_utils" | "fl_cli" => Role::Tool,
            "fl_client" => Role::Client,
            "fl_game" => Role::Game,
            "xtask" => Role::Xtask,
            _ => match krate.strip_prefix("fl_feat_") {
                Some(rest) => match rest.strip_suffix("_client") {
                    Some(name) => Role::FeatureClient(name.to_owned()),
                    None => Role::Feature(rest.to_owned()),
                },
                None => Role::Unknown,
            },
        }
    }

    /// Headless crates: never the renderer, windowing, input, `Time` or `wgpu`.
    pub fn is_headless(&self) -> bool {
        matches!(self, Role::Sim | Role::Feature(_) | Role::Tool)
    }

    /// Crates whose external dependency tree `arch-check` inspects.
    pub fn is_tree_checked(&self) -> bool {
        matches!(self, Role::Pure | Role::Content) || self.is_headless()
    }

    /// Client-side crates: everything that may see the renderer. Their dev-dependencies count as edges too.
    pub fn is_client(&self) -> bool {
        matches!(self, Role::Client | Role::FeatureClient(_) | Role::Game)
    }
}

/// `cargo tree` arguments for the banned-crate check: every feature and every target, so a banned crate behind a
/// non-default feature or a `[target.'cfg(…)'.dependencies]` table is still seen.
pub const TREE_ARGS: [&str; 5] = ["-e", "normal,build", "--all-features", "--target", "all"];

/// `probes/*` packages are throwaway experiments and go unchecked.
pub fn is_probe(meta: &Metadata, pkg: &Package) -> bool {
    pkg.manifest_path
        .as_std_path()
        .strip_prefix(meta.workspace_root.as_std_path())
        .is_ok_and(|rel| rel.starts_with("probes"))
}

const BASE: [&str; 4] = ["fl_math", "fl_protocol", "fl_view", "fl_content"];

/// Whether `pkg` may depend on the workspace crate `dep`.
fn edge_allowed(pkg: &Package, dep: &str, probes: &BTreeSet<&str>) -> bool {
    let name = pkg.name.as_str();
    let in_list = |list: &[&str]| list.contains(&dep);
    let is_sim_feature = matches!(Role::of(dep), Role::Feature(_));
    match Role::of(name) {
        Role::Pure | Role::Content => match name {
            "fl_protocol" => in_list(&["fl_math"]),
            "fl_view" | "fl_content" => in_list(&["fl_math", "fl_protocol"]),
            _ => false,
        },
        Role::Sim => in_list(&BASE),
        Role::Feature(_) => {
            in_list(&BASE)
                || dep == "fl_sim"
                || sim_deps(pkg).iter().any(|y| dep == format!("fl_feat_{y}"))
        }
        Role::FeatureClient(x) => {
            in_list(&BASE) || dep == "fl_client" || dep == format!("fl_feat_{x}")
        }
        Role::Tool => match name {
            "fl_catalog" => dep == "fl_sim" || is_sim_feature,
            "fl_observe" => in_list(&["fl_math", "fl_protocol", "fl_view"]),
            "fl_test_utils" => in_list(&BASE) || in_list(&["fl_sim", "fl_catalog"]),
            _ => {
                in_list(&BASE)
                    || in_list(&["fl_sim", "fl_catalog", "fl_test_utils", "fl_observe"])
                    || is_sim_feature
            }
        },
        Role::Client => in_list(&BASE),
        Role::Game => dep != "xtask" && !probes.contains(dep),
        Role::Xtask | Role::Unknown => false,
    }
}

/// `[package.metadata.fl] sim_deps = ["y", …]`: the feature crates `fl_feat_y` this feature may depend on.
fn sim_deps(pkg: &Package) -> Vec<String> {
    pkg.metadata["fl"]["sim_deps"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The image crate behind map layers. Only the `Content` role (`fl_content`) may depend on it directly, of any
/// kind; everyone else reads the decoded `MapLayers` (ADR-0014). Transitive uses, such as Bevy's image stack in the
/// client, are not direct dependencies.
const PNG: &str = "png";

/// Checks every workspace edge against the allowed-edges table: normal and build edges, plus dev edges for the
/// client crates. Pure, content and headless crates may not dev-depend on a client crate either, and no crate but
/// `fl_content` may depend on [`PNG`] directly.
pub fn check_edges(meta: &Metadata) -> Vec<Violation> {
    let mut pkgs = meta.workspace_packages();
    pkgs.sort_by(|a, b| a.name.cmp(&b.name));
    let members: BTreeSet<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
    let probes: BTreeSet<&str> = pkgs
        .iter()
        .filter(|p| is_probe(meta, p))
        .map(|p| p.name.as_str())
        .collect();
    let mut out = Vec::new();
    for pkg in pkgs.iter().filter(|p| !is_probe(meta, p)) {
        let name = pkg.name.as_str();
        let role = Role::of(name);
        if role == Role::Unknown {
            out.push(Violation {
                krate: name.to_owned(),
                detail: format!("{name} has no row in the allowed-edges table"),
            });
            continue;
        }
        for dep in &pkg.dependencies {
            let dev = dep.kind == DependencyKind::Development;
            if dep.name == PNG && role != Role::Content {
                out.push(Violation {
                    krate: name.to_owned(),
                    detail: format!(
                        "{name} must not depend on {PNG} directly: only fl_content decodes PNG \
                         map layers (ADR-0014)"
                    ),
                });
                continue;
            }
            if role.is_tree_checked() && denied_direct(&dep.name) {
                out.push(Violation {
                    krate: name.to_owned(),
                    detail: format!("{name} must not depend on {} directly", dep.name),
                });
                continue;
            }
            if !members.contains(dep.name.as_str()) || dep.name == name {
                continue;
            }
            let allowed = if dev && !role.is_client() {
                !(role.is_tree_checked() && Role::of(&dep.name).is_client())
            } else {
                edge_allowed(pkg, &dep.name, &probes)
            };
            if !allowed {
                let kind = if dev { " (dev-dependency)" } else { "" };
                out.push(Violation {
                    krate: name.to_owned(),
                    detail: format!("{name} -> {}{kind} not allowed", dep.name),
                });
            }
        }
    }
    out
}

/// `bevy_ecs` and the crates it brings: the only Bevy `fl_content` may use.
const ECS_FAMILY: [&str; 10] = [
    "bevy_ecs",
    "bevy_ecs_macros",
    "bevy_ecs_macro_logic",
    "bevy_ptr",
    "bevy_utils",
    "bevy_platform",
    "bevy_tasks",
    "bevy_reflect",
    "bevy_reflect_derive",
    "bevy_macro_utils",
];

/// What `fl_sim` and the sim half of a feature may bring besides [`ECS_FAMILY`]: `bevy_app` and the crates it needs
/// (read from `cargo tree -p fl_sim --all-features`; spec §3 and §5.1: `bevy_app` + `bevy_ecs` only).
const APP_FAMILY: [&str; 2] = ["bevy_app", "bevy_derive"];

/// Never in a headless crate's tree.
const HEADLESS_BANNED: [&str; 8] = [
    "bevy",
    "bevy_internal",
    "bevy_render",
    "bevy_window",
    "bevy_input",
    "bevy_time",
    "bevy_winit",
    "wgpu",
];

/// Physics and RNG crates no pure, content or headless crate may use (ADR-0003: randomness is `Rolls` only; no
/// float physics in the sim). `rand` itself is checked on direct dependencies only (see [`denied_direct`]): the
/// `bevy_ecs` family brings it transitively.
fn physics_or_rng_framework(dep: &str) -> bool {
    dep == "bevy_rand"
        || dep.starts_with("bevy_rand_")
        || dep.starts_with("rapier")
        || dep.starts_with("avian")
}

/// A crate no pure, content or headless crate may depend on directly: the `rand` family plus
/// [`physics_or_rng_framework`].
fn denied_direct(dep: &str) -> bool {
    dep == "rand" || dep.starts_with("rand_") || physics_or_rng_framework(dep)
}

fn banned(role: &Role, dep: &str) -> bool {
    match role {
        Role::Pure => dep.starts_with("bevy") || physics_or_rng_framework(dep),
        Role::Content => {
            (dep.starts_with("bevy") && !ECS_FAMILY.contains(&dep)) || physics_or_rng_framework(dep)
        }
        // The sim half of a feature and `fl_sim`: an allowlist, not a denylist.
        Role::Sim | Role::Feature(_) => {
            (dep.starts_with("bevy") && !ECS_FAMILY.contains(&dep) && !APP_FAMILY.contains(&dep))
                || HEADLESS_BANNED.contains(&dep)
                || physics_or_rng_framework(dep)
        }
        r if r.is_headless() => HEADLESS_BANNED.contains(&dep) || physics_or_rng_framework(dep),
        _ => false,
    }
}

/// Checks one package's `cargo tree -p <krate> --prefix none` + [`TREE_ARGS`] output for banned crates.
pub fn check_tree(krate: &str, tree: &str) -> Vec<Violation> {
    let role = Role::of(krate);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for line in tree.lines() {
        let mut words = line.split_whitespace();
        let (Some(dep), version) = (words.next(), words.next().unwrap_or("")) else {
            continue;
        };
        if dep != krate && banned(&role, dep) && seen.insert(dep) {
            out.push(Violation {
                krate: krate.to_owned(),
                detail: format!("{krate} must not depend on {dep} {version}")
                    .trim_end()
                    .to_owned(),
            });
        }
    }
    out
}

/// The full check: edges from the metadata, then one `cargo tree` per pure, content or headless crate. Returns
/// the number of packages checked and the violations.
pub fn check(root: &Path) -> Result<(usize, Vec<Violation>)> {
    let meta = cmd::workspace(root)?;
    let mut violations = check_edges(meta);
    let checked: Vec<&Package> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| !is_probe(meta, p))
        .collect();
    let tree_crates: Vec<String> = checked
        .iter()
        .map(|p| p.name.to_string())
        .filter(|n| Role::of(n).is_tree_checked())
        .collect();
    let trees = cmd::cargo_trees(root, &tree_crates, &TREE_ARGS)?;
    for (krate, tree) in tree_crates.iter().zip(&trees) {
        violations.extend(check_tree(krate, tree));
    }
    Ok((checked.len(), violations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn edges_flag_client_to_sim() {
        let meta = fixture::metadata("metadata_client_sim.json");
        assert_eq!(
            check_edges(&meta),
            [Violation {
                krate: "fl_client".into(),
                detail: "fl_client -> fl_sim not allowed".into(),
            }]
        );
    }

    #[test]
    fn edges_accept_fixture_workspace() {
        let meta = fixture::metadata("metadata_ok.json");
        assert_eq!(check_edges(&meta), []);
    }

    #[test]
    fn feature_edge_needs_declared_dep() {
        // fl_feat_b -> fl_feat_a without sim_deps; fl_feat_c declares sim_deps = ["a"].
        let meta = fixture::metadata("metadata_features.json");
        let v = check_edges(&meta);
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].krate, "fl_feat_b");
        assert_eq!(v[0].detail, "fl_feat_b -> fl_feat_a not allowed");
    }

    #[test]
    fn headless_dev_dep_on_client_flagged() {
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "fl_sim", "fl_client", Some("dev"));
        });
        assert_eq!(
            check_edges(&meta),
            [Violation {
                krate: "fl_sim".into(),
                detail: "fl_sim -> fl_client (dev-dependency) not allowed".into(),
            }]
        );
    }

    #[test]
    fn headless_dev_dep_on_headless_allowed() {
        // fl_observe already dev-depends on fl_sim in the real workspace; a headless dev edge is fine.
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "fl_view", "fl_test_utils", Some("dev"));
        });
        assert_eq!(check_edges(&meta), []);
    }

    #[test]
    fn client_dev_dep_on_sim_flagged() {
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "fl_client", "fl_sim", Some("dev"));
        });
        assert_eq!(
            check_edges(&meta),
            [Violation {
                krate: "fl_client".into(),
                detail: "fl_client -> fl_sim (dev-dependency) not allowed".into(),
            }]
        );
    }

    #[test]
    fn unknown_crate_flagged() {
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_package(v, "fl_mystery", "crates", &["fl_math"]);
        });
        assert_eq!(
            check_edges(&meta),
            [Violation {
                krate: "fl_mystery".into(),
                detail: "fl_mystery has no row in the allowed-edges table".into(),
            }]
        );
    }

    #[test]
    fn probes_are_skipped() {
        // probe_dynlink already depends on fl_sim and fl_content; any edge, even to xtask, goes unchecked.
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "probe_dynlink", "xtask", None);
            fixture::add_package(v, "probe_extra", "probes", &["fl_client", "fl_sim"]);
        });
        assert_eq!(check_edges(&meta), []);
    }

    /// A real `cargo tree` over a scratch workspace: a headless crate whose banned dependencies sit behind a
    /// non-default feature and a target-specific table. Default arguments miss both; [`TREE_ARGS`] sees both.
    #[test]
    fn tree_args_see_feature_gated_and_target_specific_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"fl_sim\", \"bevy_render\", \"wgpu\"]\n",
        );
        for stub in ["bevy_render", "wgpu"] {
            write(
                &format!("{stub}/Cargo.toml"),
                &format!("[package]\nname = \"{stub}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
            );
            write(&format!("{stub}/src/lib.rs"), "");
        }
        write(
            "fl_sim/Cargo.toml",
            "[package]\nname = \"fl_sim\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [features]\nrender = [\"dep:bevy_render\"]\n\n\
             [dependencies]\nbevy_render = { path = \"../bevy_render\", optional = true }\n\n\
             [target.'cfg(target_os = \"android\")'.dependencies]\nwgpu = { path = \"../wgpu\" }\n",
        );
        write("fl_sim/src/lib.rs", "");

        let default_tree = cmd::cargo_tree(root, "fl_sim", &["-e", "normal,build"]).unwrap();
        assert_eq!(check_tree("fl_sim", &default_tree), [], "{default_tree}");
        let full_tree = cmd::cargo_tree(root, "fl_sim", &TREE_ARGS).unwrap();
        let details: Vec<String> = check_tree("fl_sim", &full_tree)
            .into_iter()
            .map(|v| v.detail)
            .collect();
        assert_eq!(
            details,
            [
                "fl_sim must not depend on bevy_render v0.1.0",
                "fl_sim must not depend on wgpu v0.1.0"
            ],
            "{full_tree}"
        );
    }

    #[test]
    fn headless_direct_rand_and_physics_deps_flagged() {
        for (krate, dep) in [
            ("fl_sim", "rand"),
            ("fl_feat_movement", "bevy_rand"),
            ("fl_cli", "rapier2d"),
            ("fl_math", "avian2d"),
            ("fl_content", "rand_core"),
        ] {
            let meta = fixture::metadata_edited("metadata_ok.json", |v| {
                if krate == "fl_feat_movement" {
                    fixture::add_package(v, krate, "crates", &["fl_sim"]);
                }
                fixture::add_dep(v, krate, dep, None);
            });
            let flagged: Vec<String> = check_edges(&meta)
                .into_iter()
                .filter(|v| v.krate == krate)
                .map(|v| v.detail)
                .collect();
            assert_eq!(
                flagged,
                [format!("{krate} must not depend on {dep} directly")],
                "{krate} -> {dep}"
            );
        }
        // A client crate may use rand.
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "fl_client", "rand", None);
        });
        assert_eq!(check_edges(&meta), []);
    }

    #[test]
    fn png_direct_dep_only_in_content() {
        // fl_content decodes the PNG map layers (ADR-0014).
        let meta = fixture::metadata_edited("metadata_ok.json", |v| {
            fixture::add_dep(v, "fl_content", "png", None);
        });
        assert_eq!(check_edges(&meta), []);
        for (krate, kind) in [
            ("fl_sim", None),
            ("fl_cli", None),
            ("fl_client", None),
            ("fl_test_utils", Some("dev")),
        ] {
            let meta = fixture::metadata_edited("metadata_ok.json", |v| {
                fixture::add_dep(v, krate, "png", kind);
            });
            let flagged: Vec<String> = check_edges(&meta)
                .into_iter()
                .filter(|v| v.krate == krate)
                .map(|v| v.detail)
                .collect();
            assert_eq!(
                flagged,
                [format!(
                    "{krate} must not depend on png directly: only fl_content decodes PNG map \
                     layers (ADR-0014)"
                )],
                "{krate} -> png ({kind:?})"
            );
        }
        // The tree rules accept png and its pure-Rust dependencies in the content and sim trees: the rule
        // above is about direct dependencies only.
        for krate in ["fl_content", "fl_sim"] {
            assert_eq!(
                check_tree(krate, "png v0.18.1\nfdeflate v0.3.7").len(),
                0,
                "{krate}"
            );
        }
    }

    #[test]
    fn tree_flags_physics_but_allows_transitive_rand() {
        assert_eq!(check_tree("fl_sim", "rapier3d v0.1.0").len(), 1);
        assert_eq!(check_tree("fl_cli", "avian3d v0.1.0").len(), 1);
        assert_eq!(check_tree("fl_content", "bevy_rand v0.1.0").len(), 1);
        // `bevy_ecs` brings `rand` along.
        assert_eq!(
            check_tree(
                "fl_sim",
                "bevy_ecs v0.20.0
rand v0.10.3"
            )
            .len(),
            0
        );
    }

    #[test]
    fn sim_and_feature_trees_are_an_allowlist() {
        for krate in ["fl_sim", "fl_feat_movement"] {
            for ok in [
                "bevy_app",
                "bevy_ecs",
                "bevy_derive",
                "bevy_platform",
                "bevy_tasks",
            ] {
                assert_eq!(
                    check_tree(krate, &format!("{ok} v0.20.0")).len(),
                    0,
                    "{krate} {ok}"
                );
            }
            for bad in [
                "bevy_math",
                "bevy_transform",
                "bevy_asset",
                "bevy_log",
                "bevy_diagnostic",
            ] {
                assert_eq!(
                    check_tree(krate, &format!("{bad} v0.20.0")).len(),
                    1,
                    "{krate} {bad}"
                );
            }
        }
        // Tools keep the denylist: `fl_cli` may use other Bevy crates, but never the renderer.
        assert_eq!(check_tree("fl_cli", "bevy_math v0.20.0").len(), 0);
        assert_eq!(check_tree("fl_cli", "bevy_render v0.20.0").len(), 1);
    }

    #[test]
    fn tree_flags_render_in_headless_crate() {
        assert_eq!(check_tree("fl_sim", "bevy_render v0.20.0").len(), 1);
        assert_eq!(check_tree("fl_client", "bevy_render v0.20.0").len(), 0);
    }

    #[test]
    fn tree_flags_any_bevy_in_pure_crate() {
        assert_eq!(check_tree("fl_view", "bevy_ecs v0.20.0").len(), 1);
    }

    #[test]
    fn tree_allows_ecs_family_in_content() {
        assert_eq!(
            check_tree("fl_content", "bevy_ecs v0.20.0\nbevy_ptr v0.20.0").len(),
            0
        );
        assert_eq!(check_tree("fl_content", "bevy_app v0.20.0").len(), 1);
    }
}
