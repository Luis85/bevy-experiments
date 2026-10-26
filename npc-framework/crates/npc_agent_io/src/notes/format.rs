//! Parsing and rendering of the notes files, without I/O.
//!
//! **Entry files** ([`EntryFile`]): one markdown file per entry, a YAML
//! frontmatter ([`super::frontmatter`]) then the body:
//!
//! ```text
//! ---
//! id: 7
//! type: diary
//! title: Found the grove north of the wall
//! aliases: []
//! tags: [npc/diary]
//! agent: Wren Ashdown
//! world: Hollowmere
//! author: agent
//! created: 2026-10-25T12:32:10Z
//! created_tick: 12345
//! updated: 2026-10-25T12:40:02Z
//! updated_tick: 13020
//! revision: 2
//! ---
//! The body, plain markdown.
//! ```
//!
//! Proposals add `status` and `prompted_by`. The API's `updated` stamp is
//! `{tick: updated_tick, date: updated}` with `#r<revision>` appended from
//! revision 2 on: `revision` counts the writes at the same tick and date
//! ([`next_stamp`]), so a write at a later tick is revision 1 again and its
//! stamp is exactly what the writer passed. An update rewrites only the
//! properties that changed and the body only when it changed; everything
//! else (unknown properties, Obsidian's list style) is kept byte for byte.
//!
//! **Old files, read for migration only**: `diary.md` / `memory.md` ([`Doc`]:
//! a preamble, then `## <title>` entries with
//! `<!-- note id=N created=TICK@DATE updated=TICK@DATE -->`, or the old
//! explorer's `## tick T - title`), and header-block proposals
//! ([`OldProposal`]: `key: value` lines up to the first blank line, then the
//! body).
//!
//! Every format accepts a leading BOM (dropped) and CRLF line ends (kept:
//! a file keeps the line end it was read with, a migrated entry that of its
//! old file).

use super::frontmatter::{self, Frontmatter, FrontmatterError};
use super::{Book, Note, NotesOwner, ProposalStatus, Stamp, Writer};

/// Drops a leading byte order mark.
pub(super) fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// The line end the file uses: that of its first line (`\n` without one).
fn detect_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// The line end of a raw line, or `fallback` for a last line without one.
fn line_eol(line: &str, fallback: &'static str) -> &'static str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        fallback
    }
}

fn trim_eol(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// One line: newlines and carriage returns become spaces, ends trimmed.
pub(super) fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_string()
}

/// A body as the store keeps it: `\n` line ends, no leading blank lines, no
/// trailing whitespace.
pub(super) fn clean_body(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let mut rest = text.as_str();
    while let Some(nl) = rest.find('\n') {
        if rest[..nl].trim().is_empty() {
            rest = &rest[nl + 1..];
        } else {
            break;
        }
    }
    rest.trim_end().to_string()
}

/// A date as the store keeps it: whitespace and `>` become `_` (so a
/// stamp read back from any file compares equal to the one a writer holds).
pub(super) fn clean_date(date: &str) -> String {
    date.chars()
        .map(|c| {
            if c.is_whitespace() || c == '>' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// A date's base and revision: `D#rN` is (`D`, N); a date without a
/// revision suffix is revision 1.
pub(super) fn split_rev(date: &str) -> (&str, u32) {
    if let Some((base, n)) = date.rsplit_once("#r")
        && let Ok(n) = n.parse::<u32>()
        && n >= 2
    {
        return (base, n);
    }
    (date, 1)
}

/// The API stamp of an entry: `date` with `#r<revision>` from revision 2.
fn stamp_of(tick: u64, date: &str, revision: u32) -> Stamp {
    Stamp {
        tick,
        date: if revision >= 2 {
            format!("{date}#r{revision}")
        } else {
            date.to_string()
        },
    }
}

/// The `updated` stamp of a write at `at` over an entry last updated at
/// `prev`: `at` itself, unless it has the same tick and date (revision
/// aside) as `prev` (a paused game, a hand edit): then `prev`'s date with
/// the next revision suffix `#rN`. So every write gives a stamp that differs
/// from the one before it.
pub(super) fn next_stamp(prev: &Stamp, at: Stamp) -> Stamp {
    let (prev_base, prev_rev) = split_rev(&prev.date);
    if at.tick == prev.tick && split_rev(&at.date).0 == prev_base {
        Stamp {
            tick: at.tick,
            date: format!("{prev_base}#r{}", prev_rev.saturating_add(1)),
        }
    } else {
        at
    }
}

/// Whether two versions of a note differ in what a writer sees.
pub(super) fn content_differs(a: &Note, b: &Note) -> bool {
    a.title != b.title || a.body != b.body || a.status != b.status || a.prompted_by != b.prompted_by
}

/// The `type` property of a book's entries.
pub(super) fn type_name(book: Book) -> &'static str {
    match book {
        Book::Diary => "diary",
        Book::Memory => "memory",
        Book::Proposals => "proposal",
    }
}

/// The properties the store manages, in the order a new file has them.
pub(super) const KNOWN: [&str; 15] = [
    "id",
    "type",
    "title",
    "aliases",
    "tags",
    "agent",
    "world",
    "author",
    "created",
    "created_tick",
    "updated",
    "updated_tick",
    "revision",
    "status",
    "prompted_by",
];

fn author_name(by: Writer) -> &'static str {
    match by {
        Writer::Agent => "agent",
        Writer::Player => "player",
    }
}

/// A body as an entry file holds it: its lines with `eol`, ending with one
/// (nothing for an empty body).
fn render_body(body: &str, eol: &str) -> String {
    if body.is_empty() {
        String::new()
    } else {
        format!("{}{eol}", body.split('\n').collect::<Vec<_>>().join(eol))
    }
}

/// Why an entry file is not an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ReadError {
    /// No frontmatter at all (in `proposals/`: an old-style proposal).
    NoFrontmatter,
    /// The reason it cannot be read.
    Bad(String),
}

/// One entry file, parsed.
#[derive(Clone, Debug)]
pub(super) struct EntryFile {
    fm: Frontmatter,
    /// The raw text after the frontmatter.
    body_raw: String,
    eol: &'static str,
    pub(super) note: Note,
    /// The `revision` property.
    pub(super) revision: u32,
}

/// A whole number property: `None` when missing or empty.
fn number<T: std::str::FromStr>(fm: &Frontmatter, key: &str) -> Result<Option<T>, ReadError> {
    match fm.text(key).map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| ReadError::Bad(format!("`{key}` is `{v}`, not a whole number"))),
    }
}

impl EntryFile {
    /// Parses the entry file named `stem` (without `.md`) of `book`.
    ///
    /// # Errors
    /// [`ReadError::NoFrontmatter`], or [`ReadError::Bad`] with the reason:
    /// broken frontmatter, a bad `id`, another `type`, a known property that
    /// is not a plain value, a bad number or status. A file without an `id`
    /// parses with id 0 (see [`EntryFile::set_id`]).
    pub(super) fn parse(book: Book, stem: &str, text: &str) -> Result<Self, ReadError> {
        let text = strip_bom(text);
        let (fm, body) = match frontmatter::split(text) {
            Ok(split) => split,
            Err(FrontmatterError::Missing) => return Err(ReadError::NoFrontmatter),
            Err(e) => return Err(ReadError::Bad(e.to_string())),
        };
        let eol = fm.eol();
        for key in KNOWN {
            let list = matches!(key, "aliases" | "tags");
            if fm.is_complex(key)
                || (!list && matches!(fm.get(key), Some(frontmatter::Value::List(_))))
            {
                return Err(ReadError::Bad(format!("`{key}` must be a plain value")));
            }
        }
        // No id yet (a hand-made file): 0, the store gives it the next one.
        let id =
            match fm.text("id").map(str::trim) {
                None | Some("") => 0,
                Some(v) => v.parse::<u32>().ok().filter(|&n| n > 0).ok_or_else(|| {
                    ReadError::Bad(format!("id `{v}` is not a whole number above 0"))
                })?,
            };
        if let Some(t) = fm.text("type").map(str::trim)
            && !t.is_empty()
            && !t.eq_ignore_ascii_case(type_name(book))
        {
            return Err(ReadError::Bad(format!(
                "type is `{t}`, not `{}`",
                type_name(book)
            )));
        }
        let title = fm
            .text("title")
            .map(one_line)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                // The file name, without a diary's date.
                let s = match super::filename::day_of(stem) {
                    Some(_) if book == Book::Diary => stem[10..].trim_start(),
                    _ => stem,
                };
                s.to_string()
            });
        let created_tick = number::<u64>(&fm, "created_tick")?.unwrap_or(0);
        let created_date = clean_date(fm.text("created").unwrap_or(""));
        let updated_tick = number::<u64>(&fm, "updated_tick")?.unwrap_or(created_tick);
        let updated_date = fm
            .text("updated")
            .map_or_else(|| created_date.clone(), clean_date);
        let revision = number::<u32>(&fm, "revision")?.unwrap_or(1).max(1);
        let (status, prompted_by) = if book == Book::Proposals {
            let status = match fm.text("status").map(str::trim) {
                None | Some("") => ProposalStatus::New,
                Some(s) => ProposalStatus::parse(s).ok_or_else(|| {
                    ReadError::Bad(format!(
                        "status `{s}` is not new, liked, pursuing or rejected"
                    ))
                })?,
            };
            let by = fm
                .text("prompted_by")
                .map(one_line)
                .filter(|p| !p.is_empty());
            (Some(status), by)
        } else {
            (None, None)
        };
        let note = Note {
            id,
            title,
            body: clean_body(body),
            created: Stamp {
                tick: created_tick,
                date: created_date,
            },
            updated: stamp_of(updated_tick, &updated_date, revision),
            status,
            prompted_by,
        };
        Ok(Self {
            fm,
            body_raw: body.to_string(),
            eol,
            note,
            revision,
        })
    }

    /// A new entry file for `note`, written by `by`, with lines ending in
    /// `eol`; `extra` properties (an old proposal's unknown header lines)
    /// follow the known ones.
    pub(super) fn create(
        book: Book,
        note: &Note,
        owner: &NotesOwner,
        by: Writer,
        eol: &'static str,
        extra: &[(String, String)],
    ) -> Self {
        let mut fm = Frontmatter::new(eol);
        let ty = type_name(book);
        let (updated, revision) = split_rev(&note.updated.date);
        fm.set_plain("id", &note.id.to_string());
        fm.set_text("type", ty);
        fm.set_text("title", &note.title);
        fm.set_list("aliases", &[]);
        fm.set_list("tags", &[format!("npc/{ty}")]);
        fm.set_text("agent", &owner.agent);
        fm.set_text("world", &owner.world);
        fm.set_text("author", author_name(by));
        fm.set_date("created", &note.created.date);
        fm.set_plain("created_tick", &note.created.tick.to_string());
        fm.set_date("updated", updated);
        fm.set_plain("updated_tick", &note.updated.tick.to_string());
        fm.set_plain("revision", &revision.to_string());
        if book == Book::Proposals {
            let status = note.status.unwrap_or(ProposalStatus::New);
            fm.set_text("status", status.as_str());
            fm.set_text("prompted_by", note.prompted_by.as_deref().unwrap_or(""));
        }
        for (k, v) in extra {
            fm.set_text(k, v);
        }
        Self {
            fm,
            body_raw: render_body(&note.body, eol),
            eol,
            note: note.clone(),
            revision,
        }
    }

    /// Replaces the note with `note`: the properties that changed are
    /// rewritten (a new title also adds the old one to `aliases`), the body
    /// only when it changed; `by` (the writer, `None` for a revision after a
    /// hand edit) becomes the `author`.
    pub(super) fn write_note(&mut self, note: Note, by: Option<Writer>) {
        let old = &self.note;
        if note.title != old.title {
            let mut aliases = self.fm.list("aliases").unwrap_or_default();
            if !aliases.contains(&old.title) {
                aliases.push(old.title.clone());
            }
            self.fm.set_text("title", &note.title);
            self.fm.set_list("aliases", &aliases);
        }
        if let Some(by) = by {
            self.fm.set_text("author", author_name(by));
        }
        let (updated, revision) = split_rev(&note.updated.date);
        self.fm.set_date("updated", updated);
        self.fm
            .set_plain("updated_tick", &note.updated.tick.to_string());
        self.fm.set_plain("revision", &revision.to_string());
        if note.status != old.status
            && let Some(s) = note.status
        {
            self.fm.set_text("status", s.as_str());
        }
        if note.body != old.body {
            self.body_raw = render_body(&note.body, self.eol);
        }
        self.revision = revision;
        self.note = note;
    }

    /// Gives a file read without an `id` its id: an `id` property first,
    /// everything else kept.
    pub(super) fn set_id(&mut self, id: u32) {
        self.fm.set_plain_first("id", &id.to_string());
        self.note.id = id;
    }

    /// The file text.
    pub(super) fn render(&self) -> String {
        format!("{}{}", self.fm.render(), self.body_raw)
    }
}

/// The largest id an `id:` line of `text` names, read leniently (quotes,
/// spaces), for a file the store cannot read: its id is never handed out
/// again.
pub(super) fn lenient_id(text: &str) -> Option<u32> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("id:"))
        .filter_map(|v| {
            v.trim()
                .trim_matches(['"', '\''])
                .trim()
                .parse::<u32>()
                .ok()
        })
        .filter(|&n| n > 0)
        .max()
}

/// Whether a `proposals/` file without frontmatter is an old header-block
/// proposal: its file name is `NNN-..`, or its header block (up to the
/// first blank line) has a `title:`, `status:` or `written:` line. Anything
/// else is a file the store does not know, left alone.
pub(super) fn looks_like_old_proposal(file: &str, text: &str) -> bool {
    let digits = file.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && file.as_bytes().get(digits) == Some(&b'-') {
        return true;
    }
    strip_bom(text)
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_once(':'))
        .any(|(k, _)| {
            matches!(
                k.trim().to_ascii_lowercase().as_str(),
                "title" | "status" | "written"
            )
        })
}

// ------------------------------------------------- old files (migration)

/// `## title` or a bare `##` (after any line end).
fn is_heading(line: &str) -> bool {
    let l = trim_eol(line);
    l == "##" || l.starts_with("## ")
}

/// A heading behind any number of escaping backslashes.
fn heading_like(line: &str) -> bool {
    is_heading(line.trim_start_matches('\\'))
}

fn unescape_line(line: &str) -> &str {
    if line.starts_with('\\') && heading_like(line) {
        &line[1..]
    } else {
        line
    }
}

/// `TICK@DATE`.
fn parse_tick_at(v: &str) -> Option<Stamp> {
    let (tick, date) = v.split_once('@').unwrap_or((v, ""));
    Some(Stamp {
        tick: tick.parse().ok()?,
        date: date.to_string(),
    })
}

/// The values of an id comment: `<!-- note id=N created=T@D updated=T@D -->`.
fn parse_comment(line: &str) -> Option<(u32, Stamp, Stamp)> {
    let inner = trim_eol(line)
        .trim()
        .strip_prefix("<!-- note ")?
        .strip_suffix("-->")?;
    let (mut id, mut created, mut updated) = (None, None, None);
    for token in inner.split_whitespace() {
        match token.split_once('=') {
            Some(("id", v)) => id = v.parse::<u32>().ok().filter(|&n| n > 0),
            Some(("created", v)) => created = parse_tick_at(v),
            Some(("updated", v)) => updated = parse_tick_at(v),
            _ => {}
        }
    }
    let created = created.unwrap_or_else(|| Stamp {
        tick: 0,
        date: String::new(),
    });
    let updated = updated.unwrap_or_else(|| created.clone());
    Some((id?, created, updated))
}

/// The old explorer heading `tick T - title`.
fn parse_old_heading(h: &str) -> Option<(u64, String)> {
    let rest = h.strip_prefix("tick ")?;
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let tick = rest[..digits].parse().ok()?;
    let rest = rest[digits..].trim_start();
    let rest = rest
        .strip_prefix(['-', ':', '\u{2013}', '\u{2014}'])
        .unwrap_or(rest);
    Some((tick, rest.trim().to_string()))
}

/// A body read from raw entry text (line ends normalised, unescaped).
fn body_from_raw(raw: &str) -> String {
    let cleaned = clean_body(raw);
    cleaned
        .split('\n')
        .map(unescape_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One `## ` entry of an old diary or memory file.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) note: Note,
    /// The entry carried a usable id comment.
    has_id: bool,
    /// The line end of its heading.
    pub(super) eol: &'static str,
}

/// A parsed old `diary.md` or `memory.md`.
#[derive(Clone, Debug)]
pub(super) struct Doc {
    /// Text before the first heading, verbatim.
    pub(super) preamble: String,
    pub(super) entries: Vec<Entry>,
    pub(super) eol: &'static str,
}

impl Doc {
    /// Parses an old diary or memory file and gives every entry without a
    /// usable id (none, or one an earlier entry already has) the next free
    /// id, in file order.
    pub(super) fn parse(text: &str) -> Self {
        let text = strip_bom(text);
        let eol = detect_eol(text);
        let mut preamble = String::new();
        let mut sections: Vec<(&str, String)> = Vec::new();
        for line in text.split_inclusive('\n') {
            if is_heading(line) {
                sections.push((line, String::new()));
            } else if let Some((_, body)) = sections.last_mut() {
                body.push_str(line);
            } else {
                preamble.push_str(line);
            }
        }
        let mut entries: Vec<Entry> = sections
            .into_iter()
            .map(|(heading, rest)| Entry::parse(heading, &rest, eol))
            .collect();
        let mut used = std::collections::BTreeSet::new();
        for e in &mut entries {
            if e.has_id && !used.insert(e.note.id) {
                e.has_id = false;
            }
        }
        let mut next = used.last().copied().unwrap_or(0);
        for e in entries.iter_mut().filter(|e| !e.has_id) {
            next += 1;
            e.note.id = next;
            e.has_id = true;
        }
        Self {
            preamble,
            entries,
            eol,
        }
    }

    /// The preamble, when it holds more than the file's own `# Title` line
    /// (the text migration keeps as an entry of its own).
    pub(super) fn stray_text(&self) -> Option<String> {
        let lines: Vec<&str> = self
            .preamble
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        match lines.as_slice() {
            [] => None,
            [only] if only.starts_with("# ") || *only == "#" => None,
            _ => Some(clean_body(&self.preamble)),
        }
    }
}

impl Entry {
    fn parse(heading: &str, rest: &str, doc_eol: &'static str) -> Self {
        let eol = line_eol(heading, doc_eol);
        let heading_text = trim_eol(heading)[2..].trim().to_string();
        // The id comment is the first non-blank line after the heading
        // (formatters put a blank line under headings, MD022).
        let gap_len: usize = rest
            .split_inclusive('\n')
            .take_while(|l| l.trim().is_empty() && l.ends_with('\n'))
            .map(str::len)
            .sum();
        let after_gap = &rest[gap_len..];
        let first_line = after_gap.split_inclusive('\n').next().unwrap_or("");
        if let Some((id, created, updated)) = parse_comment(first_line) {
            let note = Note {
                id,
                title: heading_text,
                body: body_from_raw(&after_gap[first_line.len()..]),
                created: Stamp {
                    tick: created.tick,
                    date: clean_date(&created.date),
                },
                updated: Stamp {
                    tick: updated.tick,
                    date: clean_date(&updated.date),
                },
                status: None,
                prompted_by: None,
            };
            return Self {
                note,
                has_id: true,
                eol,
            };
        }
        let (tick, title) = parse_old_heading(&heading_text).unwrap_or((0, heading_text));
        let stamp = Stamp {
            tick,
            date: String::new(),
        };
        let note = Note {
            id: 0,
            title,
            body: body_from_raw(rest),
            created: stamp.clone(),
            updated: stamp,
            status: None,
            prompted_by: None,
        };
        Self {
            note,
            has_id: false,
            eol,
        }
    }
}

/// `tick T, DATE` (also `tick T`); anything else is tick 0 with the text as
/// the date.
fn parse_stamp_text(v: &str) -> Stamp {
    let parsed = v.strip_prefix("tick ").and_then(|rest| {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let tick = rest[..digits].parse().ok()?;
        let rest = rest[digits..].trim_start();
        let date = rest.strip_prefix(',').unwrap_or(rest).trim();
        Some(Stamp {
            tick,
            date: clean_date(date),
        })
    });
    parsed.unwrap_or_else(|| Stamp {
        tick: 0,
        date: clean_date(v),
    })
}

/// The file name without `.md` (any case).
pub(super) fn stem(file: &str) -> &str {
    match file.len().checked_sub(3) {
        Some(i) if file.is_char_boundary(i) && file[i..].eq_ignore_ascii_case(".md") => &file[..i],
        _ => file,
    }
}

/// The `NNN` of `NNN-<slug>.md`.
fn file_number(file: &str) -> Option<u32> {
    let s = stem(file);
    let digits = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    s[..digits].parse().ok().filter(|&n| n > 0)
}

/// Whether an old header key can be a frontmatter property as it is.
fn usable_key(key: &str) -> bool {
    key.chars().next().is_some_and(char::is_alphanumeric)
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
}

/// An old header-block proposal (no frontmatter).
#[derive(Clone, Debug)]
pub(super) struct OldProposal {
    /// Its id: the `id:` line's, else the file's number, else 0.
    pub(super) note: Note,
    /// The id came from an `id:` line.
    pub(super) has_id: bool,
    /// Header lines the store does not know, as properties.
    pub(super) extra: Vec<(String, String)>,
    pub(super) eol: &'static str,
}

impl OldProposal {
    /// Parses the old proposal `file`. Header lines that are no property
    /// (no `key:`, an odd key, a repeated or reserved key) start the body,
    /// so nothing is lost.
    pub(super) fn parse(file: &str, text: &str) -> Self {
        let text = strip_bom(text);
        let eol = detect_eol(text);
        let mut header: Vec<&str> = Vec::new();
        let mut tail = "";
        let mut offset = 0;
        let mut in_header = false;
        for line in text.split_inclusive('\n') {
            let blank = line.trim().is_empty();
            if blank && !in_header {
                offset += line.len();
                continue;
            }
            if blank {
                tail = &text[offset..];
                break;
            }
            in_header = true;
            header.push(trim_eol(line));
            offset += line.len();
        }
        let mut known: std::collections::BTreeMap<&str, String> = Default::default();
        let mut extra: Vec<(String, String)> = Vec::new();
        let mut loose: Vec<&str> = Vec::new();
        for line in header {
            let Some((k, v)) = line.split_once(':') else {
                loose.push(line);
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            let lower = k.to_ascii_lowercase();
            let slot = match lower.as_str() {
                "title" => Some("title"),
                "status" => Some("status"),
                "written" => Some("written"),
                "prompted by" => Some("prompted by"),
                "id" => Some("id"),
                "updated" => Some("updated"),
                _ => None,
            };
            match slot {
                Some(s) if !known.contains_key(s) => {
                    known.insert(s, v.to_string());
                }
                None if usable_key(k)
                    && !KNOWN.contains(&lower.as_str())
                    && !extra.iter().any(|(e, _)| e == k) =>
                {
                    extra.push((k.to_string(), v.to_string()));
                }
                _ => loose.push(line),
            }
        }
        let get = |k: &str| known.get(k).map(String::as_str).filter(|v| !v.is_empty());
        let created = get("written").map_or_else(
            || Stamp {
                tick: 0,
                date: String::new(),
            },
            parse_stamp_text,
        );
        let updated = get("updated").map_or_else(|| created.clone(), parse_stamp_text);
        let id_line = get("id")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|&n| n > 0);
        let mut body = clean_body(tail);
        if !loose.is_empty() {
            let lines = loose.join("\n");
            body = if body.is_empty() {
                lines
            } else {
                format!("{lines}\n\n{body}")
            };
        }
        let note = Note {
            id: id_line.or_else(|| file_number(file)).unwrap_or(0),
            title: get("title").map_or_else(|| stem(file).to_string(), one_line),
            body,
            created,
            updated,
            status: Some(
                get("status")
                    .and_then(ProposalStatus::parse)
                    .unwrap_or(ProposalStatus::New),
            ),
            prompted_by: get("prompted by").map(one_line),
        };
        Self {
            note,
            has_id: id_line.is_some(),
            extra,
            eol,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_text_parses() {
        assert_eq!(
            parse_stamp_text("tick 1840, 2026-10-24"),
            Stamp {
                tick: 1840,
                date: "2026-10-24".to_string()
            }
        );
        assert_eq!(parse_stamp_text("tick 5").date, "");
        assert_eq!(parse_stamp_text("yesterday").date, "yesterday");
    }

    #[test]
    fn next_stamp_always_moves() {
        let s = |tick, date: &str| Stamp {
            tick,
            date: date.to_string(),
        };
        assert_eq!(next_stamp(&s(5, "d"), s(6, "d")), s(6, "d"));
        assert_eq!(next_stamp(&s(5, "d"), s(5, "e")), s(5, "e"));
        assert_eq!(next_stamp(&s(5, "d"), s(5, "d")), s(5, "d#r2"));
        assert_eq!(next_stamp(&s(5, "d#r2"), s(5, "d")), s(5, "d#r3"));
        assert_eq!(next_stamp(&s(5, "d#r9"), s(5, "d#r9")), s(5, "d#r10"));
        assert_eq!(next_stamp(&s(5, ""), s(5, "")), s(5, "#r2"));
        // `#r1`, `#r0` and `#rx` are not revisions, just date text.
        assert_eq!(next_stamp(&s(5, "d#r1"), s(5, "d#r1")), s(5, "d#r1#r2"));
        assert_eq!(split_rev("d#rx"), ("d#rx", 1));
        // A stamp splits into date and revision and is rebuilt from them.
        for st in [s(5, "#r3"), s(5, "d"), s(5, "d#r1"), s(5, "d#r12")] {
            let (date, rev) = split_rev(&st.date);
            assert_eq!(stamp_of(st.tick, date, rev), st);
        }
        assert_eq!(parse_tick_at("5@#r3"), Some(s(5, "#r3")));
    }

    #[test]
    fn old_headings() {
        assert_eq!(
            parse_old_heading("tick 40 - Woke up"),
            Some((40, "Woke up".into()))
        );
        assert_eq!(parse_old_heading("tick 7: x"), Some((7, "x".into())));
        assert_eq!(parse_old_heading("tick x - y"), None);
        assert_eq!(parse_old_heading("Grove"), None);
    }

    #[test]
    fn a_malformed_comment_is_body_text() {
        let d = Doc::parse("## A\n<!-- note id=x -->\nbody\n");
        assert_eq!(d.entries[0].note.id, 1);
        assert_eq!(d.entries[0].note.body, "<!-- note id=x -->\nbody");
    }

    #[test]
    fn duplicate_ids_are_reassigned() {
        let c = "<!-- note id=4 created=1@d updated=1@d -->";
        let d = Doc::parse(&format!("## A\n{c}\na\n\n## B\n{c}\nb\n"));
        let ids: Vec<u32> = d.entries.iter().map(|e| e.note.id).collect();
        assert_eq!(ids, [4, 5]);
        assert_eq!(d.entries[1].note.body, "b");
    }

    #[test]
    fn stray_text_is_more_than_a_title() {
        assert_eq!(
            Doc::parse("# Diary of Wren\n\n## A\nx\n").stray_text(),
            None
        );
        assert_eq!(Doc::parse("\n\n## A\nx\n").stray_text(), None);
        assert_eq!(
            Doc::parse("# Diary\nmore\n## A\nx\n")
                .stray_text()
                .as_deref(),
            Some("# Diary\nmore")
        );
        assert_eq!(
            Doc::parse("loose line\n").stray_text().as_deref(),
            Some("loose line")
        );
    }

    #[test]
    fn old_proposal_header_lines_find_a_place() {
        let p = OldProposal::parse(
            "004-x.md",
            "title: T\ntitle: again\nMy Key: v\ntags: a\n: odd\nno colon\n\nbody\n",
        );
        assert_eq!(p.note.id, 4);
        assert!(!p.has_id);
        assert_eq!(p.note.title, "T");
        assert_eq!(p.extra, [("My Key".to_string(), "v".to_string())]);
        assert_eq!(
            p.note.body,
            "title: again\ntags: a\n: odd\nno colon\n\nbody"
        );
    }
}
