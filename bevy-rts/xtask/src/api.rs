//! `cargo xtask api`: public API snapshots of the contract crates (spec §12.7, ADR-0011).
//!
//! `cargo-public-api` lists each crate's public API from rustdoc JSON, which needs a nightly
//! toolchain: the nightly named in [`TOOLCHAIN_FILE`], undated (ruling R66), so the listing
//! omits auto-trait impls, which churn with nightly updates. Nothing builds, tests or lints
//! with it. The file's `# cargo-public-api <version>` comment records the tool version the
//! listings were made with; [`public_api`] warns when the installed one differs.
//!
//! The listing is compared with the committed `xtask/snapshots/api/<crate>.txt`; a changed or
//! new listing is written next to it as `<crate>.txt.new`, which only the lead promotes
//! (`cargo xtask bless xtask/snapshots --reason "…"`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};

/// The crates whose public API is snapshotted.
pub const API_CRATES: &[&str] = &["fl_math", "fl_protocol", "fl_view", "fl_content", "fl_sim"];

/// The file naming the nightly toolchain that `cargo-public-api` runs rustdoc with.
pub const TOOLCHAIN_FILE: &str = "xtask/api-toolchain.txt";

/// Where the committed API listings live.
pub const SNAPSHOT_DIR: &str = "xtask/snapshots/api";

/// How a crate's current API listing compares with the committed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiStatus {
    Same,
    Changed,
    /// No listing is committed yet.
    New,
}

impl ApiStatus {
    fn name(self) -> &'static str {
        match self {
            ApiStatus::Same => "unchanged",
            ApiStatus::Changed => "changed",
            ApiStatus::New => "new",
        }
    }
}

/// The toolchain named by the first line of [`TOOLCHAIN_FILE`] that is neither blank nor a
/// `#` comment.
pub fn toolchain(root: &Path) -> Result<String> {
    let path = root.join(TOOLCHAIN_FILE);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .with_context(|| format!("{TOOLCHAIN_FILE} names no toolchain"))
}

/// The `cargo-public-api` version recorded in [`TOOLCHAIN_FILE`] by a
/// `# cargo-public-api <version>` comment line, if any.
pub fn pinned_tool_version(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(TOOLCHAIN_FILE)).ok()?;
    text.lines().find_map(|l| {
        let version = l
            .trim()
            .strip_prefix('#')?
            .trim()
            .strip_prefix("cargo-public-api ")?;
        Some(version.trim().to_owned())
    })
}

/// A warning when the installed `cargo public-api --version` output does not name the
/// `pinned` version.
fn version_warning(pinned: Option<&str>, installed: &str) -> Option<String> {
    let installed = installed.trim();
    let Some(pinned) = pinned else {
        return Some(format!(
            "warning: {TOOLCHAIN_FILE} records no `# cargo-public-api <version>`; installed: \
             {installed}"
        ));
    };
    let found = installed.split_whitespace().last().unwrap_or(installed);
    (found != pinned).then(|| {
        format!(
            "warning: the API listings were made with cargo-public-api {pinned} \
             ({TOOLCHAIN_FILE}), but {installed} is installed; a listing diff may be the tool's"
        )
    })
}

/// `cargo +<toolchain> public-api <args>`. `cargo xtask` runs under the pinned stable
/// toolchain: the rustup and cargo variables it set are dropped, so `+<toolchain>` picks the
/// nightly for the whole run.
fn public_api_command(root: &Path, toolchain: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg(format!("+{toolchain}"))
        .arg("public-api")
        .args(args)
        .current_dir(root)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTC")
        .env_remove("RUSTDOC")
        .env_remove("CARGO")
        .stdin(Stdio::null());
    cmd
}

/// Prints [`version_warning`] to stderr, once per process.
fn warn_on_tool_version(root: &Path, toolchain: &str) {
    static CHECKED: OnceLock<()> = OnceLock::new();
    CHECKED.get_or_init(|| {
        let installed = match public_api_command(root, toolchain, &["--version"]).output() {
            Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
            Err(e) => format!("unknown ({e})"),
        };
        if let Some(warning) = version_warning(pinned_tool_version(root).as_deref(), &installed) {
            eprintln!("{warning}");
        }
    });
}

/// The simplified public API listing of `krate` (`cargo +<toolchain> public-api -p <krate>
/// -ss`: blanket and auto-trait impls omitted), with LF line endings and a final newline.
/// Warns once (stderr) when the installed `cargo-public-api` is not the recorded version.
pub fn public_api(root: &Path, krate: &str, toolchain: &str) -> Result<String> {
    warn_on_tool_version(root, toolchain);
    let out = public_api_command(root, toolchain, &["-p", krate, "-ss", "--color", "never"])
        .output()
        .with_context(|| format!("run cargo +{toolchain} public-api -p {krate}"))?;
    if !out.status.success() {
        bail!(
            "cargo +{toolchain} public-api -p {krate} failed:\n{}",
            String::from_utf8_lossy(&out.stderr).trim_end()
        );
    }
    Ok(normalize(&String::from_utf8_lossy(&out.stdout)))
}

/// LF line endings, no trailing whitespace, one final newline.
fn normalize(text: &str) -> String {
    let mut out: String = text
        .lines()
        .map(|l| format!("{}\n", l.trim_end()))
        .collect::<String>()
        .trim_end()
        .to_owned();
    out.push('\n');
    out
}

/// [`ApiStatus::New`] without a committed listing, else whether the listings are equal
/// (ignoring line endings and trailing whitespace).
pub fn compare(current: &str, committed: Option<&str>) -> ApiStatus {
    match committed {
        None => ApiStatus::New,
        Some(committed) if normalize(committed) == normalize(current) => ApiStatus::Same,
        Some(_) => ApiStatus::Changed,
    }
}

/// The committed listing of `krate`.
fn snapshot_path(root: &Path, krate: &str) -> PathBuf {
    root.join(SNAPSHOT_DIR).join(format!("{krate}.txt"))
}

/// The pending listing of `krate`, for the lead to bless.
fn pending_path(root: &Path, krate: &str) -> PathBuf {
    root.join(SNAPSHOT_DIR).join(format!("{krate}.txt.new"))
}

/// Lists and compares every crate of [`API_CRATES`], in order. With `write`, each changed or
/// new listing is written as `<crate>.txt.new`, and a stale `.txt.new` of an unchanged crate
/// is removed.
pub fn run(root: &Path, write: bool) -> Result<Vec<(&'static str, ApiStatus)>> {
    let toolchain = toolchain(root)?;
    let mut statuses = Vec::new();
    for &krate in API_CRATES {
        let current = public_api(root, krate, &toolchain)?;
        let committed = std::fs::read_to_string(snapshot_path(root, krate)).ok();
        let status = compare(&current, committed.as_deref());
        if write {
            let pending = pending_path(root, krate);
            if status == ApiStatus::Same {
                if pending.is_file() {
                    std::fs::remove_file(&pending)
                        .with_context(|| format!("remove {}", pending.display()))?;
                }
            } else {
                std::fs::create_dir_all(root.join(SNAPSHOT_DIR))?;
                std::fs::write(&pending, &current)
                    .with_context(|| format!("write {}", pending.display()))?;
            }
        }
        statuses.push((krate, status));
    }
    Ok(statuses)
}

/// `api: 4 crates unchanged`, or `api: 2 of 4 crates changed or new` followed by one
/// `  <crate>: changed` line per such crate and what to do next.
pub fn render(statuses: &[(&str, ApiStatus)], wrote: bool) -> String {
    let differing: Vec<_> = statuses
        .iter()
        .filter(|(_, s)| *s != ApiStatus::Same)
        .collect();
    if differing.is_empty() {
        return format!("api: {} crates unchanged\n", statuses.len());
    }
    let mut out = format!(
        "api: {} of {} crates changed or new\n",
        differing.len(),
        statuses.len()
    );
    for (krate, status) in &differing {
        out.push_str(&format!("  {krate}: {}\n", status.name()));
    }
    if wrote {
        out.push_str(&format!(
            "wrote {SNAPSHOT_DIR}/<crate>.txt.new; the lead blesses them with `cargo xtask bless \
             {SNAPSHOT_DIR} --reason \"…\"`\n"
        ));
    } else {
        out.push_str("run `cargo xtask api` to write the .txt.new files for the lead to bless\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_detects_change() {
        assert_eq!(compare("a\nb", Some("a")), ApiStatus::Changed);
        assert_eq!(compare("a", Some("a")), ApiStatus::Same);
        assert_eq!(compare("a", None), ApiStatus::New);
        assert_eq!(compare("a\r\nb  \n", Some("a\nb")), ApiStatus::Same);
    }

    #[test]
    fn render_names_differing_crates() {
        let same = [("fl_math", ApiStatus::Same), ("fl_sim", ApiStatus::Same)];
        assert_eq!(render(&same, false), "api: 2 crates unchanged\n");
        let mixed = [("fl_math", ApiStatus::Same), ("fl_sim", ApiStatus::Changed)];
        let out = render(&mixed, false);
        assert!(
            out.starts_with("api: 1 of 2 crates changed or new\n  fl_sim: changed\n"),
            "{out}"
        );
    }

    #[test]
    fn toolchain_skips_comments() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("xtask")).unwrap();
        std::fs::write(
            dir.path().join(TOOLCHAIN_FILE),
            "# rustdoc JSON only\n\nnightly-2026-10-23\n",
        )
        .unwrap();
        assert_eq!(toolchain(dir.path()).unwrap(), "nightly-2026-10-23");
    }

    #[test]
    fn tool_version_is_read_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("xtask")).unwrap();
        std::fs::write(
            dir.path().join(TOOLCHAIN_FILE),
            "# made with\n# cargo-public-api 0.52.0\nnightly\n",
        )
        .unwrap();
        assert_eq!(pinned_tool_version(dir.path()).as_deref(), Some("0.52.0"));
        assert_eq!(
            version_warning(Some("0.52.0"), "cargo-public-api 0.52.0\n"),
            None
        );
        let warning = version_warning(Some("0.52.0"), "cargo-public-api 0.53.1").unwrap();
        assert!(
            warning.contains("0.52.0") && warning.contains("0.53.1"),
            "{warning}"
        );
        assert!(version_warning(None, "cargo-public-api 0.52.0").is_some());
    }
}
