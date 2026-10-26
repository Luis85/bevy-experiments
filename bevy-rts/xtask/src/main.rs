//! `cargo xtask`: the workspace's verification entry point (spec §12, ADR-0009).

mod api;
mod arch;
mod bless;
mod change_class;
mod changed;
mod clippy_gen;
mod cmd;
mod gate;
mod hooks;
mod owners;
mod protected;
mod scope;
mod test_guard;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "cargo xtask", about = "Frontline workspace tasks")]
enum Cli {
    /// Run the verification gate at a level.
    Gate {
        level: gate::Level,
        /// The diff base (its merge base with HEAD); else FL_GATE_BASE, else .gate-base, else main.
        #[arg(long)]
        base: Option<String>,
        /// The task's track(s) (see OWNERS.toml); repeat or separate with commas.
        #[arg(long = "track", value_delimiter = ',')]
        tracks: Vec<String>,
        /// The milestone, for `gate milestone` (e.g. M0).
        #[arg(long)]
        milestone: Option<String>,
    },
    /// Check workspace edges and banned external crates.
    ArchCheck,
    /// Generate the per-crate clippy.toml files from xtask/clippy-template.toml.
    GenClippy {
        /// Fail if a generated file is out of date instead of writing it.
        #[arg(long)]
        check: bool,
    },
    /// Fail if the branch touches paths outside its track(s) in OWNERS.toml.
    CheckScope {
        /// The task's track(s); repeat or separate with commas; defaults to FL_TRACK.
        #[arg(long = "track", value_delimiter = ',')]
        tracks: Vec<String>,
        /// The diff base (its merge base with HEAD); else FL_GATE_BASE, else .gate-base, else main.
        #[arg(long)]
        base: Option<String>,
    },
    /// List test-weakening changes since the merge base; the reviewer must accept each.
    TestGuard {
        /// The diff base (its merge base with HEAD); else FL_GATE_BASE, else .gate-base, else main.
        #[arg(long)]
        base: Option<String>,
    },
    /// Promote pending `*.new` protected artifacts (lead or user only).
    Bless {
        /// Directories to search, `*.new` files, or files whose `.new` sibling to promote (relative, no `..`).
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Why the new version is right; it goes into the commit as `Blessed: <reason>`.
        #[arg(long)]
        reason: String,
    },
    /// Snapshot the public API of the contract crates; write `<crate>.txt.new` for every changed or new one.
    Api {
        /// Write nothing; fail if any crate's API differs from its committed snapshot.
        #[arg(long)]
        check: bool,
    },
    /// Run a Claude Code hook: read the hook JSON from stdin, write the hook's answer to stdout.
    Hook {
        kind: hooks::Kind,
        /// Whose call this is; `auto` resolves FL_ROLE, then the input's `agent_type`, then lead.
        #[arg(long, default_value = "auto", value_parser = ["auto", "implementer", "lead", "read-only"])]
        role: String,
        /// guard-edit only: deny every path outside these globs (repeatable); never opens `**/snapshots/**`.
        #[arg(long)]
        only: Vec<String>,
        /// worktree-remove only: the worktree whose build dir to delete, instead of reading hook JSON from stdin.
        #[arg(long)]
        worktree: Option<PathBuf>,
    },
    /// Copy this xtask binary to target/hooks/, where the Claude Code hooks run it (ruling R36).
    InstallHooks,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
    let root = cmd::workspace_root();
    match cli {
        Cli::Gate {
            level,
            base,
            tracks,
            milestone,
        } => {
            // A gate run proves this binary built; hooks use the copy (ruling R36).
            if let Err(e) = hooks::install_binary(&root) {
                eprintln!("warning: hook binary not refreshed: {e:#}");
            }
            let report = gate::run(root, level, base.as_deref(), tracks, milestone);
            print!("{}", report.render());
            Ok(report.passed())
        }
        Cli::Hook {
            kind,
            role,
            only,
            worktree,
        } => {
            if let Err(e) = hooks::run(&root, kind, &role, &only, worktree.as_deref()) {
                // Exit 2 blocks the tool call or the stop: a hook that fails, fails closed (ruling R36).
                eprintln!("error: {e:#}");
                std::process::exit(2);
            }
            Ok(true)
        }
        Cli::InstallHooks => {
            match hooks::install_binary(&root)? {
                Some(path) => println!("install-hooks: {}", path.display()),
                None => println!("install-hooks: already running from the hook binary"),
            }
            Ok(true)
        }
        Cli::Api { check } => {
            let statuses = api::run(&root, !check)?;
            print!("{}", api::render(&statuses, !check));
            Ok(!check || statuses.iter().all(|(_, s)| *s == api::ApiStatus::Same))
        }
        Cli::ArchCheck => {
            let (n, violations) = arch::check(&root)?;
            if violations.is_empty() {
                println!("arch-check: OK ({n} packages)");
                return Ok(true);
            }
            println!("arch-check: FAIL");
            for v in &violations {
                println!("  {}: {}", v.krate, v.detail);
            }
            Ok(false)
        }
        Cli::GenClippy { check: false } => {
            let n = clippy_gen::write(&root)?;
            println!("gen-clippy: wrote {n} files");
            Ok(true)
        }
        Cli::GenClippy { check: true } => {
            let (n, drifted) = clippy_gen::check(&root)?;
            if drifted.is_empty() {
                println!("gen-clippy: up to date ({n} files)");
                return Ok(true);
            }
            println!(
                "gen-clippy: FAIL, {} drifted or stray clippy configs ({n} generated files; run \
                 `cargo xtask gen-clippy` and delete any clippy.toml it does not produce)",
                drifted.len()
            );
            for p in &drifted {
                println!("  {}", p.display());
            }
            Ok(false)
        }
        Cli::CheckScope { tracks, base } => {
            let tracks = scope::effective_tracks(&tracks, scope::fl_track_env().as_deref());
            let base = changed::resolve_base(&root, base.as_deref())?;
            let report = scope::run(&root, &base.rev, &tracks)?;
            print!("{}", report.render());
            Ok(report.outside.is_empty())
        }
        Cli::TestGuard { base } => {
            let base = changed::resolve_base(&root, base.as_deref())?;
            let (findings, suppressed) = test_guard::run_accepted(&root, &base.rev)?;
            for f in &findings {
                println!("{}", f.render());
            }
            println!("{}", test_guard::summary(findings.len(), suppressed));
            Ok(true)
        }
        Cli::Bless { paths, reason } => {
            // Paths are given relative to the current directory; bless takes them relative to the root.
            let cwd = std::env::current_dir()?;
            let prefix = cwd
                .strip_prefix(&root)
                .map(Path::to_path_buf)
                .or_else(|_| {
                    let (cwd, root) = (cwd.canonicalize()?, root.canonicalize()?);
                    anyhow::Ok(cwd.strip_prefix(root)?.to_path_buf())
                })
                .map_err(|_| anyhow::anyhow!("run bless from inside the workspace"))?;
            let paths: Vec<PathBuf> = paths
                .iter()
                .map(|p| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        prefix.join(p)
                    }
                })
                .collect();
            let promoted = bless::bless(&root, &paths, &reason)?;
            println!("bless: {} files promoted", promoted.len());
            for p in &promoted {
                println!("  {}", owners::normalize(p));
            }
            println!("Blessed: {}", reason.trim());
            Ok(true)
        }
    }
}

#[cfg(test)]
mod fixture {
    pub fn metadata(name: &str) -> cargo_metadata::Metadata {
        let json = match name {
            "metadata_ok.json" => include_str!("../tests/fixtures/metadata_ok.json"),
            "metadata_client_sim.json" => {
                include_str!("../tests/fixtures/metadata_client_sim.json")
            }
            "metadata_features.json" => include_str!("../tests/fixtures/metadata_features.json"),
            other => panic!("unknown fixture {other}"),
        };
        serde_json::from_str(json).expect("fixture parses as cargo metadata")
    }

    /// A fixture with its JSON edited first (see [`add_dep`] and [`add_package`]).
    pub fn metadata_edited(
        name: &str,
        edit: impl FnOnce(&mut serde_json::Value),
    ) -> cargo_metadata::Metadata {
        let mut value = serde_json::to_value(metadata(name)).expect("metadata serializes");
        edit(&mut value);
        serde_json::from_value(value).expect("edited fixture parses as cargo metadata")
    }

    fn package<'a>(meta: &'a mut serde_json::Value, name: &str) -> &'a mut serde_json::Value {
        meta["packages"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no package {name} in fixture"))
    }

    /// Adds a path dependency `from -> to` of `kind` (`None` = normal, `Some("dev")`, `Some("build")`).
    pub fn add_dep(meta: &mut serde_json::Value, from: &str, to: &str, kind: Option<&str>) {
        let template = package(meta, "fl_catalog")["dependencies"][0].clone();
        let mut dep = template;
        dep["name"] = to.into();
        dep["kind"] = kind.into();
        dep["path"] = format!("C:\\fl\\a\\crates\\{to}").into();
        package(meta, from)["dependencies"]
            .as_array_mut()
            .unwrap()
            .push(dep);
    }

    /// Adds a workspace member `name` under `<dir>/<name>` depending on `deps`.
    pub fn add_package(meta: &mut serde_json::Value, name: &str, dir: &str, deps: &[&str]) {
        let mut pkg = package(meta, "fl_catalog").clone();
        let id = format!("path+file:///C:/fl/a/{dir}/{name}#0.0.0");
        pkg["name"] = name.into();
        pkg["id"] = id.clone().into();
        pkg["manifest_path"] = format!("C:\\fl\\a\\{dir}\\{name}\\Cargo.toml").into();
        pkg["dependencies"] = serde_json::Value::Array(Vec::new());
        meta["packages"].as_array_mut().unwrap().push(pkg);
        meta["workspace_members"]
            .as_array_mut()
            .unwrap()
            .push(id.into());
        for dep in deps {
            add_dep(meta, name, dep, None);
        }
    }
}
