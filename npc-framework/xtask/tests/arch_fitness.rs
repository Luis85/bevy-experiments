//! Architecture fitness: crate-boundary rules checked on the real dependency
//! graph (`cargo metadata --all-features`), so a forbidden dependency fails
//! the build no matter how many crates it hides behind.
//!
//! Rules: docs/superpowers/specs/2026-10-23-baseline-hardening-design.md §2.6
//! and §4 ("hidden dependencies"); plan Tasks P1 and P3. Phase 4 adds the RTS
//! demo. Do not weaken a rule to make a change pass: ask first.
//!
//! Two ways to compute a crate's dependency closure:
//! - [`Resolve::Unified`]: the workspace-wide `--all-features` resolve, i.e.
//!   every optional dependency of every crate on. The strictest view; used for
//!   every core and content crate.
//! - [`Resolve::AsBuilt`]: what `cargo build -p <crate>` compiles. The root
//!   crate gets the given features, every dependency exactly the features
//!   requested along the way (declared features, `default` unless disabled,
//!   `dep:`/`x/feat`/`x?/feat` feature entries). Needed since P3: the facade
//!   `npc_framework` has optional presentation/remote features, and the
//!   unified view would attribute the village app's `remote` feature to the
//!   headless `npcctl` as well. [`as_built_canaries`] proves this resolver
//!   does not under-approximate.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand, PackageId};

/// Crates that mean "rendering / windowing leaked in". `bevy` and
/// `bevy_internal` are the umbrella crates; with default features they pull
/// the renderer, and a core crate must name the `bevy_*` sub-crates it needs.
const RENDER: &[&str] = &[
    "bevy",
    "bevy_internal",
    "bevy_render",
    "bevy_pbr",
    "bevy_core_pipeline",
    "bevy_sprite_render",
    "bevy_ui_render",
    "bevy_gizmos_render",
    "bevy_winit",
    "bevy_remote",
    "wgpu",
    "wgpu-core",
    "wgpu-hal",
    "naga",
    "winit",
];

/// Our presentation crates (the BRP methods are `npc_tools`' `remote`
/// feature, covered by [`RENDER`] through `bevy_remote`).
const VIEW: &[&str] = &["npc_view_helpers", "village_view", "testbed_view"];

/// The framework's generic crates: content reaches them only through the
/// facade `npc_framework`.
const CORE: &[&str] = &[
    "npc_ai",
    "npc_kernel",
    "npc_needs",
    "npc_jobs",
    "npc_locomotion",
    "npc_nav",
    "npc_perception",
    "npc_rules",
    "npc_agent_io",
    "npc_interaction",
    "npc_vehicles",
    "npc_command",
    "npc_progression",
    "npc_crafting",
    "npc_perf",
    "npc_tools",
    "npc_view_helpers",
];

/// Workspace directories whose crates are content or apps. Core crates may
/// never depend on anything under them.
const CONTENT_DIRS: &[&str] = &["content", "apps"];

/// How a rule computes the dependency closure.
#[derive(Clone, Copy, Debug)]
enum Resolve {
    /// Workspace-unified, all features of all crates on.
    Unified,
    /// As `cargo build -p <crate>` builds it, with the root's own features:
    /// `Some(list)` exactly these (plus nothing else), `None` = all of them.
    AsBuilt(Option<&'static [&'static str]>),
}

struct Rule {
    /// The crate under test (a workspace member).
    krate: &'static str,
    /// Crate names forbidden anywhere in its dependency closure.
    forbidden: &'static [&'static [&'static str]],
    /// Also forbid every crate under [`CONTENT_DIRS`].
    no_content: bool,
    /// Also follow the crate's own dev-dependencies (its tests may not cheat).
    include_dev: bool,
    resolve: Resolve,
    why: &'static str,
}

/// The rule table (P1, extended for the P3 split).
const RULES: &[Rule] = &[
    Rule {
        krate: "npc_ai",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "generic BT runtime: bevy_ecs/bevy_app only, no rendering, no content",
    },
    Rule {
        krate: "npc_kernel",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "deterministic headless kernel: no rendering, no view, no content",
    },
    Rule {
        krate: "npc_needs",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "generic needs: no rendering, no view, no content",
    },
    Rule {
        krate: "npc_jobs",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "generic smart objects/reservations: no rendering, no view, no content",
    },
    Rule {
        krate: "npc_locomotion",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "locomotion seam (actor capabilities C1): headless sim code, no view, no content",
    },
    Rule {
        krate: "npc_nav",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in pathfinding (actor capabilities C1): headless sim code, no view, no content",
    },
    Rule {
        krate: "npc_perception",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in perception (actor capabilities D1): headless sim code, no view, no content",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["perception"])),
        why: "perception (opt-in sensors) stays headless",
    },
    Rule {
        krate: "npc_rules",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in character rules (actor capabilities D2): headless sim code, no view, no content",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["rules"])),
        why: "rules (opt-in character sheet) stays headless",
    },
    Rule {
        krate: "npc_agent_io",
        forbidden: &[VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "agent control (actor capabilities D3) is generic: no content, no presentation (any feature)",
    },
    Rule {
        krate: "npc_agent_io",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::AsBuilt(Some(&["default"])),
        why: "headless agent control (session host, clients): BRP only behind the `remote` feature",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["agent-io"])),
        why: "agent-io (what npcctl serve uses) stays headless",
    },
    Rule {
        krate: "npc_interaction",
        forbidden: &[
            RENDER,
            VIEW,
            &["npc_perception", "npc_rules", "npc_agent_io", "npc_tools"],
        ],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in interaction (actor capabilities E1): headless sim code, no view, no content, \
              no other capability crate (glue lives in the facade, ADR 0011)",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["interaction"])),
        why: "interaction (opt-in items, doors, tools, verbs) stays headless",
    },
    Rule {
        krate: "npc_vehicles",
        forbidden: &[
            RENDER,
            VIEW,
            &[
                "npc_perception",
                "npc_rules",
                "npc_agent_io",
                "npc_tools",
                "npc_command",
                "npc_progression",
                "npc_crafting",
            ],
        ],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in vehicles (actor capabilities F2): headless sim code, no view, no content; \
              of the capabilities only npc_interaction (its mount/dismount verbs, ADR 0012)",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["vehicles"])),
        why: "vehicles (opt-in seats, mounting, ByCrew/Direct) stays headless",
    },
    Rule {
        krate: "npc_command",
        forbidden: &[
            RENDER,
            VIEW,
            &[
                "npc_perception",
                "npc_rules",
                "npc_agent_io",
                "npc_interaction",
                "npc_tools",
                "npc_vehicles",
                "npc_progression",
                "npc_crafting",
            ],
        ],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in RTS command (actor capabilities F1): headless sim code, no view, no content, \
              foundations only (ADR 0012; command x interaction/vehicles/agent-io is facade glue)",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["command"])),
        why: "command (opt-in orders, queues, formations, squads) stays headless",
    },
    Rule {
        krate: "npc_progression",
        forbidden: &[
            RENDER,
            VIEW,
            &[
                "npc_perception",
                "npc_agent_io",
                "npc_interaction",
                "npc_tools",
                "npc_vehicles",
                "npc_command",
                "npc_crafting",
            ],
        ],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in progression (actor capabilities F3): headless sim code, no view, no content;               of the capabilities only npc_rules (ADR 0012; study/teach and craft XP are facade glue)",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["progression"])),
        why: "progression (opt-in XP, levels, learning) stays headless",
    },
    Rule {
        krate: "npc_crafting",
        forbidden: &[
            RENDER,
            VIEW,
            &[
                "npc_perception",
                "npc_agent_io",
                "npc_tools",
                "npc_vehicles",
                "npc_command",
                "npc_progression",
            ],
        ],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "opt-in crafting (actor capabilities F3): headless sim code, no view, no content;               of the capabilities only npc_interaction and npc_rules (ADR 0012)",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["crafting"])),
        why: "crafting (opt-in recipes, workstations) stays headless",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["locomotion", "nav"])),
        why: "locomotion + nav (what content enables) stay headless",
    },
    Rule {
        krate: "npc_perf",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "profiler is a library for any host: no bevy render dependency",
    },
    Rule {
        krate: "npc_tools",
        forbidden: &[VIEW],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "tooling is generic: no content, no presentation crates (any feature)",
    },
    Rule {
        krate: "npc_tools",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["default"])),
        why: "headless tooling: renderer only behind the `remote` feature",
    },
    Rule {
        krate: "npc_view_helpers",
        forbidden: &[&["village_view"]],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "generic presentation helpers: no content",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[&["village_view"]],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "the facade is generic: no content under any feature",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["default"])),
        why: "the facade's default features are headless",
    },
    Rule {
        krate: "npc_framework",
        forbidden: &[RENDER, VIEW],
        no_content: true,
        include_dev: false,
        resolve: Resolve::AsBuilt(Some(&["tools", "perf"])),
        why: "tools + perf (what npcctl uses) stay headless",
    },
    Rule {
        krate: "village_content",
        forbidden: &[RENDER, VIEW],
        no_content: false,
        include_dev: true,
        // As built with all its own features: the facade's optional
        // presentation/remote features belong to the apps that enable them.
        resolve: Resolve::AsBuilt(None),
        why: "content is sim code: headless, no rendering, no view",
    },
    Rule {
        krate: "testbed_content",
        forbidden: &[RENDER, VIEW],
        no_content: false,
        include_dev: true,
        resolve: Resolve::AsBuilt(None),
        why: "the testbed's content is sim code: headless; its view is testbed_view",
    },
    Rule {
        krate: "npcctl",
        forbidden: &[RENDER, VIEW],
        no_content: false,
        include_dev: true,
        resolve: Resolve::AsBuilt(None),
        why: "headless CLI: must build and run without a GPU or window",
    },
];

/// Canaries: crates that *must* reach these names (unified graph). If Bevy
/// renames a crate, this fails instead of [`RENDER`] silently matching nothing.
const MUST_REACH: &[(&str, &str)] = &[
    ("village_view", "bevy_render"),
    ("village_view", "wgpu"),
    ("village_view", "bevy_pbr"),
    ("village", "village_view"),
    ("actor_testbed", "testbed_view"),
    ("testbed_view", "bevy_render"),
];

/// Canaries for the as-built resolver: `(crate, root features, must reach)`.
/// They prove it follows optional dependencies through features (it would
/// otherwise pass every `AsBuilt` rule trivially).
const MUST_REACH_AS_BUILT: &[(&str, Option<&[&str]>, &str)] = &[
    ("village", None, "bevy_render"),
    ("village", None, "bevy_remote"),
    ("village", None, "npc_view_helpers"),
    ("npc_tools", Some(&["remote"]), "bevy_remote"),
    ("npc_framework", Some(&["remote"]), "bevy_remote"),
    ("npc_framework", Some(&["view-helpers"]), "npc_view_helpers"),
    ("npc_framework", Some(&["default"]), "npc_perf"),
    ("npc_framework", Some(&["default"]), "npc_tools"),
    ("npc_framework", Some(&["default"]), "npc_locomotion"),
    ("npc_framework", Some(&["nav"]), "npc_nav"),
    ("npc_framework", Some(&["rules"]), "npc_rules"),
    ("npc_framework", Some(&["perception"]), "npc_perception"),
    ("npc_framework", Some(&["agent-io"]), "npc_agent_io"),
    ("npc_framework", Some(&["interaction"]), "npc_interaction"),
    ("npc_framework", Some(&["vehicles"]), "npc_vehicles"),
    ("npc_framework", Some(&["command"]), "npc_command"),
    ("npc_framework", Some(&["progression"]), "npc_progression"),
    ("npc_framework", Some(&["progression"]), "npc_rules"),
    ("npc_framework", Some(&["crafting"]), "npc_crafting"),
    ("npc_framework", Some(&["crafting"]), "npc_interaction"),
    ("npc_agent_io", Some(&["remote"]), "bevy_remote"),
    ("village_content", None, "npc_nav"),
    ("npcctl", None, "village_content"),
    ("village_content", None, "npc_perf"),
    ("npcctl", None, "npc_kernel"),
];

struct Graph {
    meta: Metadata,
    /// Package id -> outgoing edges `(dependency id, lib name, kinds)`.
    edges: BTreeMap<PackageId, Vec<(PackageId, String, Vec<DependencyKind>)>>,
}

fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml");
        let meta = MetadataCommand::new()
            .manifest_path(manifest)
            // Worst case: every optional dependency of every crate enabled.
            .features(CargoOpt::AllFeatures)
            .exec()
            .expect("cargo metadata failed");
        let resolve = meta
            .resolve
            .as_ref()
            .expect("cargo metadata has no resolve graph");
        let edges = resolve
            .nodes
            .iter()
            .map(|n| {
                let deps = n
                    .deps
                    .iter()
                    .map(|d| {
                        (
                            d.pkg.clone(),
                            d.name.clone(),
                            d.dep_kinds.iter().map(|k| k.kind).collect(),
                        )
                    })
                    .collect();
                (n.id.clone(), deps)
            })
            .collect();
        Graph { meta, edges }
    })
}

impl Graph {
    fn name(&self, id: &PackageId) -> &str {
        &self.meta[id].name
    }

    fn member(&self, name: &str) -> &PackageId {
        self.meta
            .workspace_members
            .iter()
            .find(|id| self.name(id) == name)
            .unwrap_or_else(|| panic!("`{name}` is not a workspace member (stale rule table?)"))
    }

    fn is_content(&self, id: &PackageId) -> bool {
        let root = self.meta.workspace_root.as_std_path();
        let manifest = self.meta[id].manifest_path.as_std_path();
        let Ok(rel) = manifest.strip_prefix(root) else {
            return false;
        };
        rel.components()
            .next()
            .is_some_and(|c| CONTENT_DIRS.iter().any(|d| c.as_os_str() == *d))
    }

    /// Breadth-first closure over normal and build edges (plus the root's own
    /// dev edges when `include_dev`) of the unified graph. Returns each
    /// reached package with the package it was first reached from.
    fn closure(&self, root: &PackageId, include_dev: bool) -> BTreeMap<PackageId, PackageId> {
        let mut parent = BTreeMap::new();
        let mut seen = BTreeSet::from([root.clone()]);
        let mut queue = VecDeque::from([root.clone()]);
        while let Some(at) = queue.pop_front() {
            for (dep, _, kinds) in self.edges.get(&at).into_iter().flatten() {
                let follow = kinds.iter().any(|k| match k {
                    DependencyKind::Normal | DependencyKind::Build => true,
                    DependencyKind::Development => include_dev && at == *root,
                    _ => true, // unknown kinds: be strict
                });
                if follow && seen.insert(dep.clone()) {
                    parent.insert(dep.clone(), at.clone());
                    queue.push_back(dep.clone());
                }
            }
        }
        parent
    }

    /// The package a declared dependency `decl_name` (its rename, else its
    /// name) of `pkg` resolves to.
    fn dep_target(&self, pkg: &PackageId, decl_name: &str) -> Option<&PackageId> {
        let lib = decl_name.replace('-', "_");
        self.edges
            .get(pkg)?
            .iter()
            .find(|(_, name, _)| *name == lib)
            .map(|(id, _, _)| id)
    }

    /// The as-built closure (see the module docs), with first-reached
    /// parents like [`Graph::closure`].
    fn closure_as_built(
        &self,
        root: &PackageId,
        root_features: Option<&[&str]>,
        include_dev: bool,
    ) -> BTreeMap<PackageId, PackageId> {
        let mut parent: BTreeMap<PackageId, PackageId> = BTreeMap::new();
        let mut reached: BTreeSet<PackageId> = BTreeSet::from([root.clone()]);
        let mut features: BTreeMap<PackageId, BTreeSet<String>> = BTreeMap::new();
        // Optional dependencies switched on, per package (by declared name).
        let mut active: BTreeMap<PackageId, BTreeSet<String>> = BTreeMap::new();
        let root_set: BTreeSet<String> = match root_features {
            Some(list) => list.iter().map(|s| (*s).to_owned()).collect(),
            None => self.meta[root].features.keys().cloned().collect(),
        };
        features.insert(root.clone(), root_set);

        loop {
            let mut changed = false;
            let snapshot: Vec<PackageId> = reached.iter().cloned().collect();
            for pkg in snapshot {
                let package = &self.meta[&pkg];
                let enabled = features.get(&pkg).cloned().unwrap_or_default();
                // Expand features into (optional deps to activate, dep features).
                let mut activate: BTreeSet<String> = BTreeSet::new();
                let mut dep_feats: Vec<(String, String, bool)> = Vec::new();
                let mut stack: Vec<String> = enabled.iter().cloned().collect();
                let mut seen_f: BTreeSet<String> = BTreeSet::new();
                while let Some(f) = stack.pop() {
                    if !seen_f.insert(f.clone()) {
                        continue;
                    }
                    match package.features.get(&f) {
                        Some(items) => {
                            for item in items {
                                if let Some(dep) = item.strip_prefix("dep:") {
                                    activate.insert(dep.to_owned());
                                } else if let Some((dep, feat)) = item.split_once('/') {
                                    let (dep, weak) = match dep.strip_suffix('?') {
                                        Some(d) => (d, true),
                                        None => (dep, false),
                                    };
                                    if !weak {
                                        activate.insert(dep.to_owned());
                                    }
                                    dep_feats.push((dep.to_owned(), feat.to_owned(), weak));
                                } else {
                                    stack.push(item.clone());
                                }
                            }
                        }
                        // An implicit feature named after an optional dep.
                        None => {
                            activate.insert(f.clone());
                        }
                    }
                }
                let entry = active.entry(pkg.clone()).or_default();
                for a in &activate {
                    changed |= entry.insert(a.clone());
                }
                let on = entry.clone();
                for decl in &package.dependencies {
                    let decl_name = decl.rename.clone().unwrap_or_else(|| decl.name.clone());
                    let kind_ok = match decl.kind {
                        DependencyKind::Normal | DependencyKind::Build => true,
                        DependencyKind::Development => include_dev && pkg == *root,
                        _ => true,
                    };
                    if !kind_ok || (decl.optional && !on.contains(&decl_name)) {
                        continue;
                    }
                    let Some(target) = self.dep_target(&pkg, &decl_name) else {
                        continue; // e.g. a platform-specific dep not in this resolve
                    };
                    if reached.insert(target.clone()) {
                        parent.insert(target.clone(), pkg.clone());
                        changed = true;
                    }
                    let tf = features.entry(target.clone()).or_default();
                    let mut want: Vec<String> = decl.features.clone();
                    if decl.uses_default_features {
                        want.push("default".to_owned());
                    }
                    for (dep, feat, _weak) in &dep_feats {
                        if *dep == decl_name {
                            want.push(feat.clone());
                        }
                    }
                    for w in want {
                        changed |= tf.insert(w);
                    }
                }
            }
            if !changed {
                break;
            }
        }
        parent
    }

    fn path_to(&self, parent: &BTreeMap<PackageId, PackageId>, target: &PackageId) -> String {
        let mut chain = vec![self.name(target).to_owned()];
        let mut at = target;
        while let Some(p) = parent.get(at) {
            chain.push(self.name(p).to_owned());
            at = p;
        }
        chain.reverse();
        chain.join(" -> ")
    }
}

/// Every violation of `rule`, as dependency paths. Only the first forbidden
/// crate on each path is reported (not everything behind it).
fn violations(g: &Graph, rule: &Rule) -> Vec<String> {
    let root = g.member(rule.krate);
    let parent = match rule.resolve {
        Resolve::Unified => g.closure(root, rule.include_dev),
        Resolve::AsBuilt(features) => g.closure_as_built(root, features, rule.include_dev),
    };
    let bad = |id: &PackageId| {
        let name = g.name(id);
        rule.forbidden.iter().any(|list| list.contains(&name))
            || (rule.no_content && g.is_content(id))
    };
    let mut out = Vec::new();
    for id in parent.keys().filter(|id| bad(id)) {
        let mut at = id;
        let mut behind_bad = false;
        while let Some(p) = parent.get(at) {
            behind_bad |= bad(p);
            at = p;
        }
        if !behind_bad {
            out.push(g.path_to(&parent, id));
        }
    }
    out
}

#[test]
fn core_crates_respect_boundaries() {
    let g = graph();
    let mut report = Vec::new();
    for rule in RULES {
        for path in violations(g, rule) {
            report.push(format!(
                "{} [{:?}] ({}): {path}",
                rule.krate, rule.resolve, rule.why
            ));
        }
    }
    assert!(
        report.is_empty(),
        "forbidden dependencies (crate-boundary rules, hardening spec §2.6):\n{}",
        report.join("\n")
    );
}

/// Content reaches the framework only through the facade (ADR 0007): its
/// direct dependencies name no generic crate.
#[test]
fn content_depends_on_framework_only_via_facade() {
    let g = graph();
    let mut report = Vec::new();
    for id in &g.meta.workspace_members {
        if !g.is_content(id) {
            continue;
        }
        for decl in &g.meta[id].dependencies {
            if decl.kind == DependencyKind::Normal && CORE.contains(&decl.name.as_str()) {
                report.push(format!("{} -> {}", g.name(id), decl.name));
            }
        }
    }
    assert!(
        report.is_empty(),
        "content/apps must use npc_framework, not the generic crates directly:\n{}",
        report.join("\n")
    );
}

#[test]
fn rule_table_names_real_crates() {
    let g = graph();
    for rule in RULES {
        g.member(rule.krate);
    }
    for (from, _) in MUST_REACH {
        g.member(from);
    }
    for (from, _, _) in MUST_REACH_AS_BUILT {
        g.member(from);
    }
    for core in CORE {
        g.member(core);
    }
}

#[test]
fn canaries_reach_render_crates() {
    // Proves the walker and the crate names in RENDER/VIEW match reality.
    let g = graph();
    for (from, to) in MUST_REACH {
        let closure = g.closure(g.member(from), false);
        assert!(
            closure.keys().any(|id| g.name(id) == *to),
            "{from} no longer reaches {to}: update RENDER/VIEW in arch_fitness.rs"
        );
    }
}

#[test]
fn as_built_canaries() {
    // Proves the as-built resolver follows features into optional deps.
    let g = graph();
    for (from, features, to) in MUST_REACH_AS_BUILT {
        let closure = g.closure_as_built(g.member(from), *features, false);
        assert!(
            closure.keys().any(|id| g.name(id) == *to),
            "{from} {features:?} (as built) does not reach {to}: the resolver lost an edge"
        );
    }
}

/// The portability proof (hardening spec goal 5, plan Task P4):
/// `apps/rts_units_demo` is a foreign host. Among workspace crates it
/// depends only on the facade `npc_framework`, and nothing under `content/`
/// or `apps/` (village, testbed, ...) is anywhere in its closure, dev
/// dependencies included, under the unified all-features resolve.
#[test]
fn rts_demo_uses_only_facade() {
    let g = graph();
    let demo = g.member("rts_units_demo");
    let members: BTreeSet<&PackageId> = g.meta.workspace_members.iter().collect();
    let direct: Vec<String> = g.edges[demo]
        .iter()
        .filter(|(dep, _, _)| members.contains(dep))
        .map(|(dep, _, _)| g.name(dep).to_owned())
        .filter(|name| name != "npc_framework")
        .collect();
    assert!(
        direct.is_empty(),
        "rts_units_demo may depend on npc_framework only among workspace crates, not {direct:?}"
    );
    let rule = Rule {
        krate: "rts_units_demo",
        forbidden: &[VIEW_CONTENT],
        no_content: true,
        include_dev: true,
        resolve: Resolve::Unified,
        why: "the portability proof uses only the facade",
    };
    let bad = violations(g, &rule);
    assert!(
        bad.is_empty(),
        "rts_units_demo reaches content:\n{}",
        bad.join("\n")
    );
    assert!(
        g.closure(demo, false)
            .keys()
            .any(|id| g.name(id) == "npc_framework"),
        "rts_units_demo no longer reaches the facade"
    );
}

/// Content crates by name (also caught by `no_content`; named so a crate
/// moved out of `content/` still fails the portability rule).
const VIEW_CONTENT: &[&str] = &[
    "village_view",
    "village_content",
    "village",
    "testbed_content",
    "testbed_view",
    "actor_testbed",
    "npcctl",
];

#[test]
fn content_dirs_are_detected() {
    // `village` lives under apps/, so it must count as content for core rules.
    let g = graph();
    assert!(g.is_content(g.member("village")));
    assert!(g.is_content(g.member("village_content")));
    assert!(!g.is_content(g.member("npc_kernel")));
}
