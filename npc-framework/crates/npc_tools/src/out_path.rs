//! Where remote methods may write files: one function, [`resolve`], that
//! every RPC method taking a client-supplied output path goes through
//! (`npc/record/stop {path}`, `npc/screenshot {path}`).
//!
//! An RPC client is not trusted with the file system: a browser page can
//! reach a localhost server (CSRF, DNS rebinding; `docs/debugging.md` §3.2),
//! so a path that arrives over RPC is confined under one **output root**
//! (default `<cwd>/target/npc-out`, [`DEFAULT_DIR`]; the apps' `--out-dir`
//! changes it). Paths given on the command line (`--record`, `--screenshot`,
//! `--journal`) are the user's own and are not restricted.
//!
//! Rules (checked on the raw string, before any file system access):
//! - relative only: no leading `/` or `\`, no drive (`C:`), no `\\?\`,
//!   `\\.\` or UNC prefix, no `:` anywhere (drives, NTFS streams);
//! - every component is a plain name: not empty, not `.` or `..`, no
//!   trailing dot or space (Windows strips them), no control characters or
//!   `<>"|?*`, not a Windows device name (`CON`, `NUL`, `COM1`, ..);
//! - at most [`MAX_DEPTH`] components and [`MAX_LEN`] bytes;
//! - the extension is forced: unless the name already ends in it, the
//!   expected one is appended (`shot.exe` → `shot.exe.png`);
//! - an existing file is refused unless `overwrite` (never a directory).
//!
//! [`prepare`] then creates the parent directories and checks that the
//! canonical parent is still inside the canonical root (a symlink or
//! junction placed under the root cannot lead out of it).

use std::path::{Path, PathBuf};

/// Default output root, relative to the current directory.
pub const DEFAULT_DIR: &str = "target/npc-out";
/// Most path components.
pub const MAX_DEPTH: usize = 8;
/// Longest accepted relative path, in bytes.
pub const MAX_LEN: usize = 200;

const DEVICE_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// `<cwd>/target/npc-out` (absolute; `target/npc-out` when the current
/// directory is unknown).
pub fn default_root() -> PathBuf {
    std::path::absolute(DEFAULT_DIR).unwrap_or_else(|_| PathBuf::from(DEFAULT_DIR))
}

/// The output root of the remote methods (default [`default_root`]). A host
/// inserts it to change the root (the apps' `--out-dir`).
#[derive(bevy_ecs::resource::Resource, Clone, Debug, PartialEq, Eq)]
pub struct OutputRoot(pub PathBuf);

impl Default for OutputRoot {
    fn default() -> Self {
        Self(default_root())
    }
}

impl OutputRoot {
    /// An absolute root for `dir` (relative to the current directory).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self(std::path::absolute(&dir).unwrap_or(dir))
    }

    /// [`checked`] under this root.
    pub fn file(&self, requested: &str, ext: &str, overwrite: bool) -> Result<PathBuf, String> {
        checked(&self.0, requested, ext, overwrite)
    }
}

fn component_ok(c: &str) -> Result<(), String> {
    if c.is_empty() {
        return Err("empty path component (`//`)".into());
    }
    if c == "." || c == ".." {
        return Err(format!("`{c}` components are not allowed"));
    }
    if c.ends_with('.') || c.ends_with(' ') {
        return Err(format!("component `{c}` ends with a dot or space"));
    }
    if let Some(bad) = c
        .chars()
        .find(|ch| ch.is_control() || matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(format!("component `{c}` contains {bad:?}"));
    }
    let stem = c
        .split('.')
        .next()
        .unwrap_or(c)
        .trim_end()
        .to_ascii_lowercase();
    if DEVICE_NAMES.contains(&stem.as_str()) {
        return Err(format!("`{c}` is a Windows device name"));
    }
    Ok(())
}

/// Checks `requested` (relative, see the module docs) and returns it as a
/// path under `root` with extension `ext` (without the dot, e.g. `"json"`).
/// Does not touch the file system except to check for an existing file.
pub fn resolve(
    root: &Path,
    requested: &str,
    ext: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let why = |m: String| {
        format!(
            "path `{requested}` refused: {m} (give a relative path such as `run/a.{ext}`; \
             it is written under the output root {})",
            root.display()
        )
    };
    if requested.is_empty() {
        return Err(why("empty".into()));
    }
    if requested.len() > MAX_LEN {
        return Err(why(format!("longer than {MAX_LEN} bytes")));
    }
    if requested.starts_with('/') || requested.starts_with('\\') {
        return Err(why(
            "absolute, root-relative, UNC or `\\\\?\\` paths are not allowed".into(),
        ));
    }
    if requested.contains(':') {
        return Err(why("`:` (a drive or a stream) is not allowed".into()));
    }
    if requested.starts_with('~') {
        return Err(why("`~` is not expanded".into()));
    }
    let parts: Vec<&str> = requested.split(['/', '\\']).collect();
    if parts.len() > MAX_DEPTH {
        return Err(why(format!("more than {MAX_DEPTH} components")));
    }
    for c in &parts {
        component_ok(c).map_err(why)?;
    }
    let rel: PathBuf = parts.iter().collect();
    let rel = match rel.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case(ext) => rel,
        // No or another extension: append ours (`run.v2` -> `run.v2.json`,
        // `shot.exe` -> `shot.exe.png`).
        _ => {
            let mut s = rel.into_os_string();
            s.push(".");
            s.push(ext);
            PathBuf::from(s)
        }
    };
    let path = root.join(rel);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_dir() => Err(why("it is a directory".into())),
        Ok(m) if m.file_type().is_symlink() => Err(why("it is a symbolic link".into())),
        Ok(_) if !overwrite => Err(why(
            "the file exists (pass \"overwrite\": true to replace it)".into(),
        )),
        _ => Ok(path),
    }
}

/// Creates the parent directories of `path` (from [`resolve`]) and checks
/// that they are still inside `root` once links are resolved.
pub fn prepare(root: &Path, path: &Path) -> Result<(), String> {
    let parent = path.parent().unwrap_or(root);
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let canon_root =
        std::fs::canonicalize(root).map_err(|e| format!("output root {}: {e}", root.display()))?;
    let canon_parent = std::fs::canonicalize(parent)
        .map_err(|e| format!("cannot resolve {}: {e}", parent.display()))?;
    if !canon_parent.starts_with(&canon_root) {
        return Err(format!(
            "{} leads outside the output root {}",
            parent.display(),
            root.display()
        ));
    }
    Ok(())
}

/// [`resolve`] + [`prepare`]: the path to write.
pub fn checked(
    root: &Path,
    requested: &str,
    ext: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let path = resolve(root, requested, ext, overwrite)?;
    prepare(root, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("npc-out-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        root
    }

    #[test]
    fn accepts_plain_relative_paths_and_forces_the_extension() {
        let root = tmp_root("ok");
        for (req, rel) in [
            ("a.json", "a.json"),
            ("a", "a.json"),
            ("run/a.JSON", "run/a.JSON"),
            ("run\\day1\\b", "run/day1/b.json"),
            ("x.y.json", "x.y.json"),
            ("a~b.json", "a~b.json"),
            ("run.v2", "run.v2.json"),
            ("shot.exe", "shot.exe.json"),
        ] {
            let got = resolve(&root, req, "json", false).unwrap_or_else(|e| panic!("{req}: {e}"));
            let want: PathBuf = rel.split('/').collect();
            assert_eq!(got, root.join(want), "{req}");
        }
        assert_eq!(
            resolve(&root, "shot.json", "png", false).expect("appended"),
            root.join("shot.json.png")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_escapes_and_windows_forms() {
        let root = tmp_root("bad");
        for req in [
            "",
            "/etc/passwd",
            "/tmp/a.json",
            "\\a.json",
            "C:\\Windows\\a.json",
            "C:/a.json",
            "c:a.json",
            "\\\\?\\C:\\a.json",
            "//?/C:/a.json",
            "\\\\.\\pipe\\x",
            "\\\\server\\share\\a.json",
            "//server/share/a.json",
            "..",
            "../a.json",
            "a/../../b.json",
            "a\\..\\b.json",
            "./a.json",
            "a/./b.json",
            "a//b.json",
            "a.json.",
            "a.json ",
            "dir./a.json",
            "dir /a.json",
            "a.json:stream",
            "a.json::$DATA",
            "CON",
            "con.json",
            "dir/NUL.json",
            "COM1.png",
            "lpt9",
            "a<b.json",
            "a|b.json",
            "a*.json",
            "a?.json",
            "a\"b.json",
            "a\nb.json",
            "a\u{0}b.json",
            "~/a.json",
            "a/b/c/d/e/f/g/h/i.json",
        ] {
            assert!(
                resolve(&root, req, "json", false).is_err(),
                "`{}` must be refused",
                req.escape_debug()
            );
        }
        let long = "a".repeat(MAX_LEN + 1);
        assert!(resolve(&root, &long, "json", false).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_overwrite_unless_asked_and_never_a_directory() {
        let root = tmp_root("exists");
        std::fs::write(root.join("a.json"), "{}").expect("write");
        std::fs::create_dir_all(root.join("d.json")).expect("dir");
        let err = resolve(&root, "a.json", "json", false).expect_err("exists");
        assert!(err.contains("overwrite"), "{err}");
        assert_eq!(
            resolve(&root, "a.json", "json", true).expect("overwrite"),
            root.join("a.json")
        );
        assert!(resolve(&root, "d.json", "json", true).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn checked_creates_the_parent_inside_the_root() {
        let root = tmp_root("prepare");
        let p = checked(&root, "run/day1/r", "json", false).expect("checked");
        assert!(p.parent().expect("parent").is_dir());
        assert!(p.starts_with(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_under_the_root_cannot_lead_out() {
        let root = tmp_root("link");
        let outside = tmp_root("link-target");
        std::os::unix::fs::symlink(&outside, root.join("out")).expect("symlink");
        let err = checked(&root, "out/a.json", "json", false).expect_err("escape");
        assert!(err.contains("outside"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
