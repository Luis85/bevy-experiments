//! Unified-diff parsing for test-guard: which lines of each file changed, on the old and the new side.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Context,
    Removed,
    Added,
}

pub struct DiffLine {
    pub tag: Tag,
    pub old: u32,
    pub new: u32,
    pub text: String,
}

pub struct FileDiff {
    pub path: PathBuf,
    /// The file exists at the base (`--- /dev/null` means it was added).
    pub old_exists: bool,
    /// The file exists in the working tree (`+++ /dev/null` means it was deleted).
    pub new_exists: bool,
    pub hunks: Vec<Vec<DiffLine>>,
}

impl FileDiff {
    fn empty() -> FileDiff {
        FileDiff {
            path: PathBuf::new(),
            old_exists: true,
            new_exists: true,
            hunks: Vec::new(),
        }
    }

    fn lines(&self) -> impl Iterator<Item = &DiffLine> {
        self.hunks.iter().flatten()
    }

    /// The 1-based line numbers the diff changed on one side: removed lines (old) or added lines (new).
    pub fn changed(&self, old: bool) -> BTreeSet<u32> {
        let tag = if old { Tag::Removed } else { Tag::Added };
        self.lines()
            .filter(|l| l.tag == tag)
            .map(|l| if old { l.old } else { l.new })
            .collect()
    }

    /// One side's text rebuilt from the hunks alone: the lines the diff shows at their line numbers, blank lines
    /// elsewhere. The fallback when the whole file is not available (unit-test fixtures).
    pub fn reconstruct(&self, old: bool) -> String {
        let skip = if old { Tag::Added } else { Tag::Removed };
        let shown: Vec<(u32, &str)> = self
            .lines()
            .filter(|l| l.tag != skip)
            .map(|l| (if old { l.old } else { l.new }, l.text.as_str()))
            .collect();
        let len = shown.iter().map(|(n, _)| *n).max().unwrap_or(0) as usize;
        let mut lines = vec![""; len];
        for (n, text) in shown {
            if n > 0 {
                lines[n as usize - 1] = text;
            }
        }
        lines.join("\n")
    }
}

fn diff_path(raw: &str, prefix: &str) -> String {
    let raw = raw.split('\t').next().unwrap_or(raw).trim();
    let raw = raw.trim_matches('"');
    raw.strip_prefix(prefix).unwrap_or(raw).to_owned()
}

/// `-12,3` → (12, 3); `-12` → (12, 1).
fn hunk_range(token: &str) -> (u32, u32) {
    let token = &token[1..];
    let (start, count) = token.split_once(',').unwrap_or((token, "1"));
    (start.parse().unwrap_or(0), count.parse().unwrap_or(0))
}

/// Parses `git diff` output (any context size) into per-file hunks.
pub fn parse(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut old_path = String::new();
    let (mut old, mut new) = (0u32, 0u32);
    let (mut old_left, mut new_left) = (0u32, 0u32);
    for raw in diff.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with('\\') {
            continue; // "\ No newline at end of file"
        }
        if old_left > 0 || new_left > 0 {
            let tag = match line.chars().next() {
                Some(' ') | None => Some(Tag::Context),
                Some('-') => Some(Tag::Removed),
                Some('+') => Some(Tag::Added),
                _ => None,
            };
            if let (Some(tag), Some(file)) = (tag, files.last_mut()) {
                let text = line.get(1..).unwrap_or("").to_owned();
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.push(DiffLine {
                        tag,
                        old,
                        new,
                        text,
                    });
                }
                if tag != Tag::Added {
                    old += 1;
                    old_left = old_left.saturating_sub(1);
                }
                if tag != Tag::Removed {
                    new += 1;
                    new_left = new_left.saturating_sub(1);
                }
                continue;
            }
            (old_left, new_left) = (0, 0);
        }
        if line.starts_with("diff --git ") {
            files.push(FileDiff::empty());
            old_path.clear();
        } else if let Some(p) = line.strip_prefix("--- ") {
            if files.is_empty() {
                files.push(FileDiff::empty());
            }
            old_path = diff_path(p, "a/");
            if let Some(file) = files.last_mut() {
                file.old_exists = old_path != "/dev/null";
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            let p = diff_path(p, "b/");
            if let Some(file) = files.last_mut() {
                file.new_exists = p != "/dev/null";
                file.path = PathBuf::from(if file.new_exists { &p } else { &old_path });
            }
        } else if line.starts_with("@@ ") {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() < 3 {
                continue;
            }
            let (o, oc) = hunk_range(tokens[1]);
            let (n, nc) = hunk_range(tokens[2]);
            (old, new, old_left, new_left) = (o, n, oc, nc);
            if let Some(file) = files.last_mut() {
                file.hunks.push(Vec::new());
            }
        }
    }
    files
}

/// A git-style diff that adds `text` as the new file `path`.
pub fn new_file_diff(path: &str, text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for l in lines {
        out.push('+');
        out.push_str(l);
        out.push('\n');
    }
    out
}
