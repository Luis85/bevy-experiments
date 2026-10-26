//! Running external commands: gate steps, `cargo tree`, `cargo metadata` and `git`.

use anyhow::{Context, Result, bail};
use cargo_metadata::Metadata;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::gate::Outcome;

/// The workspace root: the parent of xtask's manifest directory (`cargo run` sets
/// `CARGO_MANIFEST_DIR` at run time), else the current directory.
pub fn workspace_root() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|dir| dir.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `cargo metadata --no-deps` for the workspace, loaded once per process.
pub fn workspace(root: &Path) -> Result<&'static Metadata> {
    static META: OnceLock<Metadata> = OnceLock::new();
    if let Some(meta) = META.get() {
        return Ok(meta);
    }
    let meta = cargo_metadata::MetadataCommand::new()
        .current_dir(root)
        .no_deps()
        .exec()
        .context("cargo metadata --no-deps")?;
    Ok(META.get_or_init(|| meta))
}

/// Runs a command in `root` with stdout and stderr interleaved into `target/xtask/<log>.log`, and returns whether
/// it succeeded plus the combined output. A file rather than a pipe, so a daemon that a child spawns (the sccache
/// server) cannot keep the read open after the command exits.
pub fn run_logged(
    root: &Path,
    log: &str,
    program: &str,
    args: &[String],
    envs: &[(&str, String)],
) -> Result<(bool, String, PathBuf)> {
    let dir = root.join("target").join("xtask");
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let log_path = dir.join(format!("{log}.log"));
    let file = File::create(&log_path).with_context(|| format!("create {}", log_path.display()))?;
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .status()
        .with_context(|| format!("run {program} {}", args.join(" ")))?;
    let text = String::from_utf8_lossy(&fs::read(&log_path)?).into_owned();
    let rel = log_path
        .strip_prefix(root)
        .unwrap_or(&log_path)
        .to_path_buf();
    Ok((status.success(), text, rel))
}

/// `cargo <args>` as a gate step: `Pass`, or `Fail` with the output and the log path.
pub fn cargo_step(root: &Path, log: &str, args: &[String], envs: &[(&str, String)]) -> Outcome {
    match run_logged(root, log, "cargo", args, envs) {
        Ok((true, _, _)) => Outcome::Pass,
        Ok((false, text, path)) => Outcome::Fail(format!(
            "{}\n(full log: {})",
            text.trim_end(),
            path.display()
        )),
        Err(e) => Outcome::Fail(format!("{e:#}")),
    }
}

/// `cargo tree -p <krate> --prefix none <extra>`, stdout only.
pub fn cargo_tree(root: &Path, krate: &str, extra: &[&str]) -> Result<String> {
    let out = Command::new("cargo")
        .args(["tree", "-p", krate, "--prefix", "none"])
        .args(extra)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run cargo tree -p {krate}"))?;
    if !out.status.success() {
        bail!(
            "cargo tree -p {krate} {} failed:\n{}",
            extra.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs `cargo_tree` for every crate in parallel; results keep the input order.
pub fn cargo_trees(root: &Path, krates: &[String], extra: &[&str]) -> Result<Vec<String>> {
    std::thread::scope(|s| {
        let handles: Vec<_> = krates
            .iter()
            .map(|k| s.spawn(move || cargo_tree(root, k, extra)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("cargo tree thread panicked"))
            .collect()
    })
}

/// `git <args>` in `root`, stdout as text.
pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
