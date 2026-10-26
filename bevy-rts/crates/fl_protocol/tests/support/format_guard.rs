//! The format guard of the `formats.rs` snapshot tests of fl_protocol, fl_view, fl_sim and
//! fl_test_utils (spec §12.7, ADR-0011), included by each with
//! `#[path = "…/fl_protocol/tests/support/format_guard.rs"] mod format_guard;`.
//!
//! A snapshot text heads each group of `<Type>: <format>` lines with a `<SECTION> = <version>`
//! line, and each codec block of indented lines with `<name>@<version>:`, or
//! `<name>@<version> reads <v>, <v>:` / `<name>@<version> reads older:` when the codec decodes
//! older versions (its migrations, or a custom decode that reads every older version).
//! [`guard`] compares the committed snapshot ([`committed`]) with the current text: a format
//! that changed while its version stayed the same is an error, and so is a codec whose version
//! rose without reading the committed version (R70: save loadability rests on the codecs'
//! migrations).

use std::collections::BTreeMap;
use std::path::Path;

/// The body of the including test's committed snapshot `formats__<name>.snap` (after
/// insta's `---` header), if any.
pub fn committed(name: &str) -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("formats__{name}.snap"));
    let text = std::fs::read_to_string(path).ok()?.replace("\r\n", "\n");
    let (_, body) = text.strip_prefix("---\n")?.split_once("\n---\n")?;
    Some(body.to_owned())
}

/// What one format line or block describes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// A `<Type>: <format>` line under the `<SECTION> = <version>` line above it.
    Type { section: String, name: String },
    /// A `<name>@<version>:` block of indented lines.
    Codec(String),
}

/// One format of a snapshot text.
#[derive(Clone, Debug, Default)]
struct Entry {
    /// The version that governs it.
    version: String,
    /// The older versions a codec reads: `"older"`, or a list such as `"1, 3"`; empty for
    /// none (and for types).
    reads: String,
    format: String,
}

impl Entry {
    /// Whether this codec entry reads saves made at `version`.
    fn reads_version(&self, version: &str) -> bool {
        self.reads == "older" || self.reads.split(", ").any(|v| v == version)
    }
}

/// Every format of a snapshot text, keyed.
fn entries(text: &str) -> BTreeMap<Key, Entry> {
    let mut out: BTreeMap<Key, Entry> = BTreeMap::new();
    let (mut section, mut version) = (String::new(), String::new());
    let mut codec: Option<Key> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("  ") {
            if let Some(entry) = codec.as_ref().and_then(|key| out.get_mut(key)) {
                entry.format.push_str(rest);
                entry.format.push('\n');
            }
            continue;
        }
        codec = None;
        if let Some((name, v)) = line.split_once(" = ")
            && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
        {
            (section, version) = (name.to_owned(), v.to_owned());
        } else if let Some((name, rest)) = line.strip_suffix(':').and_then(|l| l.split_once('@')) {
            let (v, reads) = rest.split_once(" reads ").unwrap_or((rest, ""));
            let key = Key::Codec(name.to_owned());
            let entry = Entry {
                version: v.to_owned(),
                reads: reads.to_owned(),
                format: String::new(),
            };
            out.insert(key.clone(), entry);
            codec = Some(key);
        } else if let Some((name, format)) = line.split_once(": ") {
            let key = Key::Type {
                section: section.clone(),
                name: name.to_owned(),
            };
            let entry = Entry {
                version: version.clone(),
                reads: String::new(),
                format: format.to_owned(),
            };
            out.insert(key, entry);
        }
    }
    out
}

/// Whether version `new` is above `old` (both numbers).
fn rose(old: &str, new: &str) -> bool {
    matches!((old.parse::<u64>(), new.parse::<u64>()), (Ok(old), Ok(new)) if new > old)
}

/// `Err` naming every type or codec of `committed` whose format differs in `current` while its
/// version is the same, and every codec whose version rose in `current` without reading the
/// committed version. Added and removed types pass: their parents' formats change.
pub fn guard(committed: &str, current: &str) -> Result<(), String> {
    let now = entries(current);
    let errors: Vec<String> = entries(committed)
        .into_iter()
        .filter_map(|(key, old)| {
            let new = now.get(&key)?;
            match key {
                Key::Type { name, .. } => (new.version == old.version && new.format != old.format)
                    .then(|| format!("wire format of {name} changed without a version bump")),
                Key::Codec(name) => {
                    if new.version == old.version && new.format != old.format {
                        Some(format!("{name}: format changed without a version bump"))
                    } else if rose(&old.version, &new.version) && !new.reads_version(&old.version) {
                        Some(format!(
                            "{name}: version rose from {} to {} without a migration from {}",
                            old.version, new.version, old.version
                        ))
                    } else {
                        None
                    }
                }
            }
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}
