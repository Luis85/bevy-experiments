//! The accepted-findings allowlist (`xtask/test-guard-accepted.toml`, ruling M16, R154): findings a reviewer has
//! accepted stop appearing in the gate, so `gate pr` shows only new ones.
//!
//! An entry's key is the file, the enclosing test function, the kind and the whole whitespace-normalized text of the
//! finding; the line number is ignored, so a moved statement stays accepted. `count` says how many identical
//! findings were accepted: the entry suppresses them only while no more than `count` match, so a new identical
//! finding makes all of them reported again. Every entry names the ruling that accepted it.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

use super::{Finding, FindingKind};
use crate::owners;

/// Where the allowlist lives, relative to the workspace root.
pub const PATH: &str = "xtask/test-guard-accepted.toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub file: String,
    /// The enclosing function; `""` for a finding outside any function.
    pub test: String,
    pub kind: FindingKind,
    pub text: String,
    /// How many identical findings are accepted.
    pub count: usize,
    pub ruling: String,
    /// Free text, e.g. the M1 task review that accepted it.
    pub note: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedFile {
    #[serde(default)]
    accepted: Vec<RawEntry>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    file: String,
    #[serde(default)]
    test: String,
    kind: String,
    text: String,
    #[serde(default = "one")]
    count: usize,
    ruling: String,
    #[serde(default)]
    note: Option<String>,
}

fn one() -> usize {
    1
}

/// Reads [`PATH`]; a missing file is an empty list. An entry with an unknown kind, a zero count or an empty ruling
/// is an error naming its index.
pub fn load_accepted(root: &Path) -> Result<Vec<Accepted>> {
    let path = root.join(PATH);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("read {PATH}")),
    };
    parse_accepted(&text)
}

fn parse_accepted(text: &str) -> Result<Vec<Accepted>> {
    let file: AcceptedFile = toml::from_str(text).with_context(|| format!("parse {PATH}"))?;
    let mut out = Vec::new();
    for (i, e) in file.accepted.into_iter().enumerate() {
        let Some(kind) = FindingKind::from_name(&e.kind) else {
            bail!("{PATH}: entry {i} has unknown kind `{}`", e.kind);
        };
        if e.ruling.trim().is_empty() {
            bail!("{PATH}: entry {i} ({}) has an empty ruling", e.file);
        }
        if e.count == 0 {
            bail!("{PATH}: entry {i} ({}) has a count of 0", e.file);
        }
        out.push(Accepted {
            file: e.file.replace('\\', "/"),
            test: e.test,
            kind,
            text: squash(&e.text),
            count: e.count,
            ruling: e.ruling,
            note: e.note,
        });
    }
    Ok(out)
}

/// Whitespace runs collapsed to one space, ends trimmed.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

type Key = (String, String, FindingKind, String);

fn key(f: &Finding) -> Key {
    (
        owners::normalize(&f.file),
        f.test.clone(),
        f.kind,
        squash(&f.text),
    )
}

/// Drops the findings the entries accept; returns the rest and how many were dropped. A group of identical
/// findings is dropped only while it is no larger than the (summed) `count` of its entries.
pub fn suppress(findings: Vec<Finding>, accepted: &[Accepted]) -> (Vec<Finding>, usize) {
    let mut allowed: BTreeMap<Key, usize> = BTreeMap::new();
    for a in accepted {
        *allowed
            .entry((a.file.clone(), a.test.clone(), a.kind, a.text.clone()))
            .or_default() += a.count;
    }
    let mut found: BTreeMap<Key, usize> = BTreeMap::new();
    for f in &findings {
        *found.entry(key(f)).or_default() += 1;
    }
    let before = findings.len();
    let rest: Vec<Finding> = findings
        .into_iter()
        .filter(|f| {
            let k = key(f);
            !allowed.get(&k).is_some_and(|max| found[&k] <= *max)
        })
        .collect();
    let dropped = before - rest.len();
    (rest, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn finding(file: &str, line: u32, test: &str, text: &str) -> Finding {
        Finding {
            file: PathBuf::from(file),
            line,
            kind: FindingKind::RemovedAssert,
            test: test.to_owned(),
            text: text.to_owned(),
        }
    }

    fn entry(file: &str, test: &str, text: &str, count: usize) -> Accepted {
        Accepted {
            file: file.to_owned(),
            test: test.to_owned(),
            kind: FindingKind::RemovedAssert,
            text: squash(text),
            count,
            ruling: "R154".to_owned(),
            note: None,
        }
    }

    #[test]
    fn accepted_findings_are_suppressed() {
        let findings = vec![
            finding("crates/x/tests/t.rs", 10, "a", "assert_eq!(a, 1);"),
            finding("crates/y/tests/t.rs", 10, "a", "assert_eq!(a, 1);"),
        ];
        let accepted = [entry("crates/x/tests/t.rs", "a", "assert_eq!(a,  1);", 1)];
        let (rest, n) = suppress(findings.clone(), &accepted);
        assert_eq!(n, 1);
        assert_eq!(rest, vec![findings[1].clone()], "another file stays");
        // The line number is ignored.
        let moved = vec![finding("crates/x/tests/t.rs", 99, "a", "assert_eq!(a, 1);")];
        assert_eq!(suppress(moved, &accepted), (Vec::new(), 1));
        // A different kind stays.
        let mut other = finding("crates/x/tests/t.rs", 10, "a", "assert_eq!(a, 1);");
        other.kind = FindingKind::NewIgnore;
        assert_eq!(suppress(vec![other], &accepted).1, 0);
    }

    #[test]
    fn same_file_and_kind_with_other_text_or_test_stays_reported() {
        let accepted = [entry("crates/x/tests/t.rs", "a", "assert_eq!(a, 1);", 1)];
        let other_text = finding("crates/x/tests/t.rs", 10, "a", "assert_eq!(a, 2);");
        assert_eq!(
            suppress(vec![other_text.clone()], &accepted),
            (vec![other_text], 0)
        );
        let other_test = finding("crates/x/tests/t.rs", 10, "b", "assert_eq!(a, 1);");
        assert_eq!(
            suppress(vec![other_test.clone()], &accepted),
            (vec![other_test], 0),
            "the enclosing test is part of the key"
        );
    }

    #[test]
    fn a_new_identical_finding_exceeds_the_count_and_is_reported() {
        let accepted = [entry("crates/x/tests/t.rs", "a", "assert!(p);", 2)];
        let two = vec![
            finding("crates/x/tests/t.rs", 1, "a", "assert!(p);"),
            finding("crates/x/tests/t.rs", 2, "a", "assert!(p);"),
        ];
        assert_eq!(suppress(two.clone(), &accepted), (Vec::new(), 2));
        let one = vec![two[0].clone()];
        assert_eq!(suppress(one, &accepted), (Vec::new(), 1), "fewer is fine");
        let mut three = two;
        three.push(finding("crates/x/tests/t.rs", 3, "a", "assert!(p);"));
        let (rest, n) = suppress(three.clone(), &accepted);
        assert_eq!(
            (rest, n),
            (three, 0),
            "a third identical finding is reported"
        );
    }

    const ENTRY: &str =
        "[[accepted]]\nfile = \"a.rs\"\ntest = \"t\"\nkind = \"RemovedAssert\"\ntext = \"x\"\n";

    #[test]
    fn accepted_entry_needs_ruling() {
        let text = format!("{ENTRY}ruling = \"R1\"\n\n{ENTRY}ruling = \"\"\n");
        let err = parse_accepted(&text).unwrap_err().to_string();
        assert!(err.contains("entry 1"), "{err}");
        assert_eq!(parse_accepted("").unwrap(), Vec::new());
        let bad = format!("{ENTRY}ruling = \"R1\"\n").replace("RemovedAssert", "Nope");
        assert!(
            parse_accepted(&bad)
                .unwrap_err()
                .to_string()
                .contains("entry 0")
        );
    }

    #[test]
    fn count_defaults_to_one_and_must_be_positive() {
        let one = format!("{ENTRY}ruling = \"R154\"\nnote = \"Task 6\"\n");
        let parsed = parse_accepted(&one).unwrap();
        assert_eq!(parsed[0].count, 1);
        assert_eq!(parsed[0].note.as_deref(), Some("Task 6"));
        let zero = format!("{ENTRY}ruling = \"R154\"\ncount = 0\n");
        assert!(
            parse_accepted(&zero)
                .unwrap_err()
                .to_string()
                .contains("entry 0")
        );
    }

    #[test]
    fn every_kind_name_round_trips() {
        for (i, kind) in FindingKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.ordinal(), i, "ALL must follow declaration order");
            assert_eq!(FindingKind::from_name(&format!("{kind:?}")), Some(kind));
        }
        assert_eq!(FindingKind::from_name("Nope"), None);
    }

    #[test]
    fn committed_allowlist_parses() {
        let text = include_str!("../../test-guard-accepted.toml");
        parse_accepted(text).unwrap();
    }
}
