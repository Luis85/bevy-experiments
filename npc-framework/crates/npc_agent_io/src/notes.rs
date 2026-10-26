//! Agent notes: a diary, a memory and improvement proposals per agent
//! folder (spec `docs/superpowers/specs/2026-10-25-agent-notes-and-workspace-design.md`
//! §2 G1–G4, §8; storage: `2026-10-25-notes-as-obsidian-files-design.md`).
//!
//! Notes are host-side, never sim state: they are not hashed and not in the
//! world snapshot. [`NotesStore`] owns one agent folder, which can be opened
//! as an Obsidian vault: every entry is its own markdown file with a YAML
//! frontmatter ([`frontmatter`]), in a flat folder per book:
//!
//! - `diary/YYYY-MM-DD <title>.md` (the UTC day of `created`, kept when the
//!   title changes), `memory/<title>.md`, `proposals/<title>.md`; names are
//!   made safe ([`safe_name`]; `Memory 12` when nothing is left) and a name
//!   taken in the folder gets ` (2)`, ` (3)`, .. in creation order.
//! - The store finds entries by their `id` property, not by file name: a
//!   file renamed or edited by hand is the same entry. Subfolders and
//!   non-`.md` files are ignored. A file with no or broken frontmatter is
//!   listed as unreadable with the reason ([`NotesStore::unreadable`]) and
//!   never written; so are all but one of the files sharing an `id` (the
//!   newest is the entry: by `updated` when both are UTC timestamps, then
//!   `updated_tick`, then `revision`, then the modification time; ticks
//!   restart after a fresh world and `revision` at every tick). A
//!   hand-made file with frontmatter but no `id` is adopted: it gets the
//!   next id, written back once.
//! - New ids are above a high-water mark of every id seen in the folder
//!   (entries, earlier reads, the `id:` lines of unreadable files), so an
//!   id is never handed out twice.
//! - A title change writes the new file (temp file, rename onto the new
//!   name), then removes the old one, and adds the old title to `aliases`.
//! - Old folders migrate once per book: a `diary.md` / `memory.md` without a
//!   `.bak` next to it becomes one file per entry (text before the first
//!   entry becomes an entry `Kept from diary.md`) and is renamed `.bak`;
//!   entries whose id already has a file are skipped, so a rerun is safe.
//!   A proposal without frontmatter that has the old header block (a
//!   `title:`, `status:` or `written:` line) or an `NNN-` name is rewritten
//!   under its readable name and the old file removed; any other file
//!   without frontmatter is unreadable.
//!
//! Before every operation a file whose modification time (or size) changed
//! is read again, so hand edits are picked up; writes are atomic (temp file,
//! then rename). The caller supplies every [`Stamp`], date included: the
//! store reads no clock. The `agent` and `world` properties of new files are
//! the folder's [`NotesOwner`], handed in by the host.
//!
//! Writers ([`Writer`]): an agent and the player both list, add and update;
//! only the player sets a proposal's status. `update` takes the `updated`
//! stamp the writer read and fails with [`NotesError::Stale`] when the entry
//! changed since, so two writers never overwrite each other silently.
//!
//! In a host: [`NotesDirs`] maps units to agent folders, [`NotesClock`]
//! supplies the date, and [`NotesStores`] keeps one store per folder for
//! everything in the process that touches notes (the `agent/notes/*` and
//! `notes/*` methods of [`crate::api`], an in-game editor) and the folders'
//! owners. None of the three is hashed or snapshotted.
//!
//! `updated` is a version: every change gives a stamp that differs from the
//! previous one. A write at the same tick and date as the entry's `updated`
//! (a paused game) gets the date with a revision suffix, `DATE#r2`, `#r3`,
//! ... (in the file: `updated: DATE`, `revision: N`); a hand edit of an
//! entry's title, body, status or `prompted_by` that leaves its stamp alone
//! (seen when the store re-reads a changed file) gets the next revision
//! too, written back to the file, so writers holding the old stamp get
//! `Stale`. The suffix is part of the date text: pass the stamp back exactly
//! as listed. (An edit made while no store has the file open cannot be told
//! apart from the original and gets no revision.)

pub mod filename;
mod format;
pub mod frontmatter;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy_ecs::prelude::Resource;
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};

pub use filename::safe_name;
use format::{Doc, EntryFile, OldProposal, ReadError};

/// Longest title (and `prompted_by` line), in characters.
pub const TITLE_MAX: usize = 200;
/// Longest body, in bytes.
pub const BODY_MAX: usize = 8192;

/// One of the three notebooks of an agent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Book {
    /// `diary/`: what happened, entry by entry.
    Diary,
    /// `memory/`: long-lasting facts the agent wants to keep.
    Memory,
    /// `proposals/`: improvement ideas, with a status.
    Proposals,
}

impl Book {
    /// The wire name and the book's folder (`diary`, `memory`, `proposals`).
    pub fn as_str(self) -> &'static str {
        match self {
            Book::Diary => "diary",
            Book::Memory => "memory",
            Book::Proposals => "proposals",
        }
    }

    /// The book named `name` (wire name, any case).
    pub fn parse(name: &str) -> Option<Self> {
        [Book::Diary, Book::Memory, Book::Proposals]
            .into_iter()
            .find(|b| b.as_str().eq_ignore_ascii_case(name.trim()))
    }
}

/// Who writes: the agent controlling the unit, or the player (game UI, CLI).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Writer {
    /// An AI agent, through the agent methods.
    Agent,
    /// The player (the dev), through the editor or the player methods.
    Player,
}

/// A proposal's status; only the player sets it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Just written (the status of every new proposal).
    New,
    /// The player likes the idea.
    Liked,
    /// The player is working on it.
    Pursuing,
    /// The player decided against it.
    Rejected,
}

impl ProposalStatus {
    /// The file and wire name (`new`, `liked`, `pursuing`, `rejected`).
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalStatus::New => "new",
            ProposalStatus::Liked => "liked",
            ProposalStatus::Pursuing => "pursuing",
            ProposalStatus::Rejected => "rejected",
        }
    }

    /// The status named `name` (any case).
    pub fn parse(name: &str) -> Option<Self> {
        [Self::New, Self::Liked, Self::Pursuing, Self::Rejected]
            .into_iter()
            .find(|s| s.as_str().eq_ignore_ascii_case(name.trim()))
    }
}

/// When a note was written: the sim tick and a date string the caller
/// supplies (wall-clock RFC 3339 in the games; empty for migrated old
/// entries). The store replaces whitespace and `>` in a date with `_` (in
/// `at` and in `expected` alike) and may append a revision `#rN` to
/// `updated` (see the module docs).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stamp {
    /// Sim tick.
    pub tick: u64,
    /// Date text, as supplied.
    pub date: String,
}

/// One entry of a book.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Note {
    /// Unique within its book, from 1, in the order entries were added.
    pub id: u32,
    /// One line.
    pub title: String,
    /// `\n` line ends, no trailing whitespace.
    pub body: String,
    /// When it was added.
    pub created: Stamp,
    /// When it last changed; pass it to [`NotesStore::update`].
    pub updated: Stamp,
    /// Proposals only.
    pub status: Option<ProposalStatus>,
    /// Proposals only: what prompted the idea.
    pub prompted_by: Option<String>,
}

/// A note to add.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NewNote {
    /// One line (newlines become spaces), at most [`TITLE_MAX`] characters.
    pub title: String,
    /// At most [`BODY_MAX`] bytes.
    pub body: String,
    /// Proposals only.
    #[serde(default)]
    pub prompted_by: Option<String>,
}

/// The fields an update changes (`None` keeps the field).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NoteChange {
    /// A new title.
    #[serde(default)]
    pub title: Option<String>,
    /// A new body.
    #[serde(default)]
    pub body: Option<String>,
    /// A new status (proposals, player only).
    #[serde(default)]
    pub status: Option<ProposalStatus>,
}

/// Why a notes operation failed.
#[derive(Clone, Debug, PartialEq)]
pub enum NotesError {
    /// A field is over its limit ([`TITLE_MAX`] characters, [`BODY_MAX`] bytes).
    TooLong {
        /// `title`, `body` or `prompted_by`.
        field: &'static str,
        /// The limit.
        max: usize,
    },
    /// No entry with this id in the book.
    NotFound(u32),
    /// The entry changed since the writer read it; `current` is its stamp now.
    Stale {
        /// The entry.
        id: u32,
        /// Its `updated` stamp now.
        current: Stamp,
    },
    /// The writer may not do this.
    Forbidden(&'static str),
    /// A file could not be read or written.
    Io(String),
}

impl std::fmt::Display for NotesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotesError::TooLong { field, max } => {
                let unit = if *field == "body" {
                    "bytes"
                } else {
                    "characters"
                };
                write!(f, "{field} is too long: at most {max} {unit}")
            }
            NotesError::NotFound(id) => write!(f, "no note with id {id}"),
            NotesError::Stale { id, current } => write!(
                f,
                "note {id} changed since you read it (now updated at tick {}{}); list it again",
                current.tick,
                if current.date.is_empty() {
                    String::new()
                } else {
                    format!(", {}", current.date)
                }
            ),
            NotesError::Forbidden(why) => f.write_str(why),
            NotesError::Io(e) => write!(f, "notes file error: {e}"),
        }
    }
}

impl std::error::Error for NotesError {}

/// Who an agent folder belongs to: the `agent` and `world` properties of
/// the entries the store writes. The host hands it in
/// ([`NotesStore::open_as`], [`NotesStores::set_owner`]); without one both
/// are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesOwner {
    /// The agent's name.
    pub agent: String,
    /// The world's name.
    pub world: String,
}

/// A file in a book's folder that is not an entry, and why. The store never
/// writes it; fixing it by hand makes it an entry on the next read.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    /// The file, relative to the agent folder, with `/` (`diary/x.md`).
    pub file: String,
    /// Why it is not an entry (`no frontmatter ...`, `line 3: ...`,
    /// `duplicate id 7`, ..).
    pub reason: String,
}

/// What a file looked like when it was last read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

fn fingerprint(meta: &std::fs::Metadata) -> Fingerprint {
    Fingerprint {
        modified: meta.modified().ok(),
        len: meta.len(),
    }
}

/// What a file of a book's folder holds.
#[derive(Clone, Debug)]
enum Parsed {
    /// An entry (it may still lose a duplicate id; id 0 until a hand-made
    /// file without one is given one).
    Entry(Box<EntryFile>),
    /// An old header-block proposal, to migrate (its text).
    Old(String),
    /// Not an entry: the reason, and the id an `id:` line names (read
    /// leniently, never handed out again).
    Broken { reason: String, id: Option<u32> },
}

impl Parsed {
    fn broken(reason: impl Into<String>, text: &str) -> Self {
        Parsed::Broken {
            reason: reason.into(),
            id: format::lenient_id(text),
        }
    }
}

/// A cached file of a book's folder.
#[derive(Clone, Debug)]
struct FileCache {
    seen: Fingerprint,
    parsed: Parsed,
}

/// One book's folder as last read.
#[derive(Default)]
struct BookCache {
    /// By file name.
    files: BTreeMap<String, FileCache>,
    /// The entries: id to the file that holds it.
    index: BTreeMap<u32, String>,
    /// As of the last read, sorted by file.
    unreadable: Vec<Unreadable>,
    /// Old files of a title change that could not be removed (a lock), with
    /// their fingerprint then: they never win their id while another file
    /// has it, and their removal is tried again on every read while the
    /// file is unchanged (a changed one is left, a duplicate).
    superseded: BTreeMap<String, Fingerprint>,
    /// The old one-file book could not be migrated (why).
    old_problem: Option<Unreadable>,
    /// The largest id this store has seen in the folder (high-water mark):
    /// new ids are above it, even when the file that had it broke or went.
    seen_max: u32,
}

impl BookCache {
    fn entry(&self, name: &str) -> Option<&EntryFile> {
        match &self.files.get(name)?.parsed {
            Parsed::Entry(e) => Some(e),
            _ => None,
        }
    }

    fn entries(&self) -> impl Iterator<Item = &EntryFile> {
        self.files.values().filter_map(|f| match &f.parsed {
            Parsed::Entry(e) => Some(&**e),
            _ => None,
        })
    }

    /// Every id a file of the folder has: entries (duplicates included) and
    /// the `id:` lines of unreadable files.
    fn seen_ids(&self) -> BTreeSet<u32> {
        self.files
            .values()
            .filter_map(|f| match &f.parsed {
                Parsed::Entry(e) => Some(e.note.id),
                Parsed::Broken { id, .. } => *id,
                Parsed::Old(_) => None,
            })
            .filter(|&id| id > 0)
            .collect()
    }

    /// The high-water mark: the largest id seen in any file now or earlier
    /// (0 when none). A new id is above it.
    fn high_water(&self) -> u32 {
        self.seen_ids()
            .last()
            .copied()
            .unwrap_or(0)
            .max(self.seen_max)
    }

    /// Which of two files with the same id is the newer, as the entry:
    /// one not superseded; then `updated` when both are RFC 3339 UTC
    /// timestamps (ticks restart after a fresh world, `revision` at every
    /// tick); then `updated_tick`, `revision`, the modification time.
    fn newer(&self, a: &str, b: &str) -> std::cmp::Ordering {
        let (fa, fb) = (&self.files[a], &self.files[b]);
        let (Parsed::Entry(ea), Parsed::Entry(eb)) = (&fa.parsed, &fb.parsed) else {
            return std::cmp::Ordering::Equal;
        };
        let (da, _) = format::split_rev(&ea.note.updated.date);
        let (db, _) = format::split_rev(&eb.note.updated.date);
        let dates = if frontmatter::is_utc_timestamp(da) && frontmatter::is_utc_timestamp(db) {
            da.cmp(db)
        } else {
            std::cmp::Ordering::Equal
        };
        (!self.superseded.contains_key(a))
            .cmp(&!self.superseded.contains_key(b))
            .then(dates)
            .then(ea.note.updated.tick.cmp(&eb.note.updated.tick))
            .then(ea.revision.cmp(&eb.revision))
            .then(fa.seen.modified.cmp(&fb.seen.modified))
    }

    /// The winners' notes by id.
    fn notes(&self) -> BTreeMap<u32, Note> {
        self.index
            .iter()
            .filter_map(|(id, name)| Some((*id, self.entry(name)?.note.clone())))
            .collect()
    }

    /// Rebuilds the index and the unreadable list: per id, the newer file
    /// ([`BookCache::newer`]; the first name of equals) is the entry; raises
    /// the high-water mark.
    fn reindex(&mut self, book: Book) {
        let mut by_id: BTreeMap<u32, Vec<&String>> = BTreeMap::new();
        let mut unreadable = Vec::new();
        for (name, f) in &self.files {
            match &f.parsed {
                Parsed::Entry(e) if e.note.id > 0 => {
                    by_id.entry(e.note.id).or_default().push(name);
                }
                Parsed::Broken { reason, .. } => unreadable.push(Unreadable {
                    file: format!("{}/{name}", book.as_str()),
                    reason: reason.clone(),
                }),
                Parsed::Entry(_) | Parsed::Old(_) => {}
            }
        }
        let mut index = BTreeMap::new();
        for (id, names) in by_id {
            // Names come sorted, so the first of equals wins.
            let mut best = names[0];
            for n in &names[1..] {
                if self.newer(n, best) == std::cmp::Ordering::Greater {
                    best = n;
                }
            }
            for n in names.iter().filter(|n| **n != best) {
                unreadable.push(Unreadable {
                    file: format!("{}/{n}", book.as_str()),
                    reason: format!("duplicate id {id}"),
                });
            }
            index.insert(id, best.clone());
        }
        unreadable.extend(self.old_problem.clone());
        unreadable.sort_by(|a, b| a.file.cmp(&b.file));
        self.index = index;
        self.unreadable = unreadable;
        self.seen_max = self.high_water();
    }
}

/// The notes of one agent folder (see the module docs). Not a sim resource:
/// hosts keep it outside the hashed and snapshotted state.
pub struct NotesStore {
    dir: PathBuf,
    owner: NotesOwner,
    books: BTreeMap<Book, BookCache>,
}

const BOOKS: [Book; 3] = [Book::Diary, Book::Memory, Book::Proposals];

fn io_err(path: &Path, e: &std::io::Error) -> NotesError {
    NotesError::Io(format!("{}: {e}", path.display()))
}

/// Reads `path`: its text, `None` when it is not valid UTF-8.
fn read_text(path: &Path) -> Result<Option<String>, NotesError> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
    Ok(String::from_utf8(bytes).ok())
}

/// Writes `text` to `path` through a synced temp file next to it (a hidden
/// `.name.<pid>.tmp`, never read as an entry) and a rename, and returns the
/// new file's fingerprint.
fn write_atomic(path: &Path, text: &str) -> Result<Fingerprint, NotesError> {
    use std::io::Write as _;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        std::fs::metadata(path)
    })();
    match result {
        Ok(meta) => Ok(fingerprint(&meta)),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(io_err(path, &e))
        }
    }
}

/// The metadata of `path`, `None` when it does not exist.
fn metadata(path: &Path) -> Result<Option<std::fs::Metadata>, NotesError> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path, &e)),
    }
}

/// Whether `name` is an entry file name: `.md` (any case), not hidden.
fn is_entry_name(name: &str) -> bool {
    !name.starts_with('.')
        && name.len() > 3
        && name.is_char_boundary(name.len() - 3)
        && name[name.len() - 3..].eq_ignore_ascii_case(".md")
}

fn check_title(field: &'static str, text: &str) -> Result<String, NotesError> {
    let t = format::one_line(text);
    if t.chars().count() > TITLE_MAX {
        return Err(NotesError::TooLong {
            field,
            max: TITLE_MAX,
        });
    }
    Ok(t)
}

fn check_body(text: &str) -> Result<String, NotesError> {
    let b = format::clean_body(text);
    if b.len() > BODY_MAX {
        return Err(NotesError::TooLong {
            field: "body",
            max: BODY_MAX,
        });
    }
    Ok(b)
}

fn clean_stamp(at: Stamp) -> Stamp {
    Stamp {
        tick: at.tick,
        date: format::clean_date(&at.date),
    }
}

/// The kind in a fallback file name (`Memory 12`).
fn kind_label(book: Book) -> &'static str {
    match book {
        Book::Diary => "Diary",
        Book::Memory => "Memory",
        Book::Proposals => "Proposal",
    }
}

impl NotesStore {
    /// [`NotesStore::open_as`] without an owner (`agent` and `world` empty).
    ///
    /// # Errors
    /// As [`NotesStore::open_as`].
    pub fn open(dir: &Path) -> Result<Self, NotesError> {
        Self::open_as(dir, NotesOwner::default())
    }

    /// Opens the notes of the agent folder `dir`, owned by `owner`, and
    /// reads all three books (migrating an old layout, see the module
    /// docs). A missing folder is an empty one; it is created on the first
    /// write.
    ///
    /// # Errors
    /// [`NotesError::Io`] when `dir` is not a folder or a file cannot be read
    /// or migrated.
    pub fn open_as(dir: &Path, owner: NotesOwner) -> Result<Self, NotesError> {
        if let Some(meta) = metadata(dir)?
            && !meta.is_dir()
        {
            return Err(NotesError::Io(format!("{} is not a folder", dir.display())));
        }
        let mut store = Self {
            dir: dir.to_path_buf(),
            owner,
            books: BOOKS
                .into_iter()
                .map(|b| (b, BookCache::default()))
                .collect(),
        };
        for book in BOOKS {
            store.refresh(book)?;
        }
        Ok(store)
    }

    /// The agent folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Who the folder belongs to.
    pub fn owner(&self) -> &NotesOwner {
        &self.owner
    }

    /// Sets who the folder belongs to (for files written from now on).
    pub fn set_owner(&mut self, owner: NotesOwner) {
        self.owner = owner;
    }

    /// The entries of `book`, oldest (lowest id) first.
    ///
    /// # Errors
    /// [`NotesError::Io`] when a changed file cannot be read or migrated.
    pub fn list(&mut self, book: Book) -> Result<Vec<Note>, NotesError> {
        self.refresh(book)?;
        Ok(self.cache(book).notes().into_values().collect())
    }

    /// The files of `book` that are not entries, with the reason, sorted by
    /// file, as of the last [`NotesStore::list`], `add` or `update` (no I/O).
    pub fn unreadable(&self, book: Book) -> Vec<Unreadable> {
        self.cache(book).unreadable.clone()
    }

    /// Adds an entry to `book` (both writers may; `by` becomes its
    /// `author`), stamped `at` for created and updated, with the next id.
    /// Proposals start with status `new`.
    ///
    /// # Errors
    /// [`NotesError::TooLong`] past a limit; [`NotesError::Forbidden`] for a
    /// `prompted_by` outside proposals; [`NotesError::Io`].
    pub fn add(
        &mut self,
        book: Book,
        by: Writer,
        new: NewNote,
        at: Stamp,
    ) -> Result<Note, NotesError> {
        let title = check_title("title", &new.title)?;
        let body = check_body(&new.body)?;
        let prompted_by = match new.prompted_by.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(_) if book != Book::Proposals => {
                return Err(NotesError::Forbidden(
                    "only proposals have a prompted_by line",
                ));
            }
            Some(p) => Some(check_title("prompted_by", p)?),
        };
        self.refresh(book)?;
        let at = clean_stamp(at);
        let note = Note {
            id: self.cache(book).high_water() + 1,
            title,
            body,
            created: at.clone(),
            updated: at,
            status: (book == Book::Proposals).then_some(ProposalStatus::New),
            prompted_by,
        };
        self.write_new(book, &note, by, "\n", &[], None)?;
        self.cache_mut(book).reindex(book);
        Ok(note)
    }

    /// Changes entry `id` of `book` if its `updated` stamp is still
    /// `expected`; the entry's `updated` becomes `at` and its `author` `by`.
    /// A new title renames the file (the old title joins `aliases`).
    ///
    /// # Errors
    /// [`NotesError::Forbidden`] when an agent sets a status (or anyone sets
    /// one outside proposals); [`NotesError::TooLong`];
    /// [`NotesError::NotFound`]; [`NotesError::Stale`] with the current
    /// stamp when the entry changed since `expected` (nothing is written);
    /// [`NotesError::Io`].
    pub fn update(
        &mut self,
        book: Book,
        by: Writer,
        id: u32,
        expected: &Stamp,
        change: NoteChange,
        at: Stamp,
    ) -> Result<Note, NotesError> {
        if change.status.is_some() {
            if by == Writer::Agent {
                return Err(NotesError::Forbidden(
                    "only the player sets a proposal's status",
                ));
            }
            if book != Book::Proposals {
                return Err(NotesError::Forbidden("only proposals have a status"));
            }
        }
        let title = change
            .title
            .as_deref()
            .map(|t| check_title("title", t))
            .transpose()?;
        let body = change.body.as_deref().map(check_body).transpose()?;
        self.refresh(book)?;
        // Cleaned like `at` was, so a caller echoing its own `at` matches.
        let expected = clean_stamp(expected.clone());
        let cache = self.cache(book);
        let name = cache
            .index
            .get(&id)
            .ok_or(NotesError::NotFound(id))?
            .clone();
        let mut file = cache.entry(&name).ok_or(NotesError::NotFound(id))?.clone();
        let old = file.note.clone();
        if old.updated != expected {
            return Err(NotesError::Stale {
                id,
                current: old.updated,
            });
        }
        let mut note = old.clone();
        if let Some(t) = title {
            note.title = t;
        }
        if let Some(b) = body {
            note.body = b;
        }
        if let Some(s) = change.status {
            note.status = Some(s);
        }
        note.updated = format::next_stamp(&old.updated, clean_stamp(at));
        file.write_note(note.clone(), Some(by));
        let target = if note.title == old.title {
            name.clone()
        } else {
            let wanted = self.free_name(book, &note, Some(&name));
            // Only the case changed: keep the file (renaming a file onto
            // itself is not portable).
            if wanted.to_lowercase() == name.to_lowercase() {
                name.clone()
            } else {
                wanted
            }
        };
        let dir = self.dir.join(book.as_str());
        // The new file first (temp file, rename onto its name), then the
        // old one goes: a crash in between leaves two files with one id,
        // and the newer revision wins.
        let seen = write_atomic(&dir.join(&target), &file.render())?;
        let cache = self.cache_mut(book);
        if target != name {
            match std::fs::remove_file(dir.join(&name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    if let Some(f) = cache.files.get(&name) {
                        cache.superseded.insert(name.clone(), f.seen);
                    }
                }
                _ => {
                    cache.files.remove(&name);
                }
            }
        }
        cache.files.insert(
            target,
            FileCache {
                seen,
                parsed: Parsed::Entry(Box::new(file)),
            },
        );
        cache.reindex(book);
        Ok(note)
    }

    fn cache(&self, book: Book) -> &BookCache {
        &self.books[&book]
    }

    fn cache_mut(&mut self, book: Book) -> &mut BookCache {
        self.books.entry(book).or_default()
    }

    /// A free file name for `note` in `book`'s folder: its readable name,
    /// numbered when taken (case-insensitively: Windows and macOS fold
    /// case), `except` (the file being renamed) not counting as taken.
    fn free_name(&self, book: Book, note: &Note, except: Option<&str>) -> String {
        let day = if book == Book::Diary {
            filename::day_of(&note.created.date)
        } else {
            None
        };
        let base = filename::entry_name(kind_label(book), note.id, &note.title, day);
        let except = except.map(str::to_lowercase);
        let taken: BTreeSet<String> = self
            .cache(book)
            .files
            .keys()
            .map(|n| n.to_lowercase())
            .filter(|n| Some(n) != except.as_ref())
            .collect();
        let dir = self.dir.join(book.as_str());
        let mut n = 1;
        loop {
            let name = filename::numbered(&base, n);
            let lower = name.to_lowercase();
            if !taken.contains(&lower)
                && (Some(&lower) == except.as_ref() || !dir.join(&name).exists())
            {
                return name;
            }
            n += 1;
        }
    }

    /// Writes `note` as a new entry file of `book` and caches it: under its
    /// free readable name, or over the file `replacing` when that is its
    /// readable name. Returns the file name.
    fn write_new(
        &mut self,
        book: Book,
        note: &Note,
        by: Writer,
        eol: &'static str,
        extra: &[(String, String)],
        replacing: Option<&str>,
    ) -> Result<String, NotesError> {
        let file = EntryFile::create(book, note, &self.owner, by, eol, extra);
        let name = self.free_name(book, note, replacing);
        let name = match replacing {
            Some(r) if r.to_lowercase() == name.to_lowercase() => r.to_string(),
            _ => name,
        };
        let path = self.dir.join(book.as_str()).join(&name);
        let seen = write_atomic(&path, &file.render())?;
        self.cache_mut(book).files.insert(
            name.clone(),
            FileCache {
                seen,
                parsed: Parsed::Entry(Box::new(file)),
            },
        );
        Ok(name)
    }

    /// Re-reads `book`'s folder (only changed files), migrates an old
    /// layout, rebuilds the index and gives hand-edited entries a revision.
    fn refresh(&mut self, book: Book) -> Result<(), NotesError> {
        let previous = self.cache(book).notes();
        let reread = self.scan(book)?;
        if book == Book::Proposals {
            self.migrate_old_proposals()?;
        } else {
            self.migrate_old_book(book)?;
        }
        self.adopt(book)?;
        self.cache_mut(book).reindex(book);
        self.bump_hand_edits(book, &previous, &reread)
    }

    /// Gives every hand-made file with frontmatter but no `id` the next id
    /// (above the high-water mark, in file-name order), written back once;
    /// everything else in it is kept.
    fn adopt(&mut self, book: Book) -> Result<(), NotesError> {
        let dir = self.dir.join(book.as_str());
        let new: Vec<String> = self
            .cache(book)
            .files
            .iter()
            .filter(|(_, f)| matches!(&f.parsed, Parsed::Entry(e) if e.note.id == 0))
            .map(|(name, _)| name.clone())
            .collect();
        for name in new {
            let id = self.cache(book).high_water() + 1;
            let cache = self.cache_mut(book);
            let Some(f) = cache.files.get_mut(&name) else {
                continue;
            };
            let Parsed::Entry(file) = &mut f.parsed else {
                continue;
            };
            file.set_id(id);
            f.seen = write_atomic(&dir.join(&name), &file.render())?;
            cache.seen_max = cache.seen_max.max(id);
        }
        Ok(())
    }

    /// Reads the files of `book`'s folder whose fingerprint changed, drops
    /// the ones gone, retries removing superseded files. Returns the names
    /// read.
    fn scan(&mut self, book: Book) -> Result<BTreeSet<String>, NotesError> {
        let dir = self.dir.join(book.as_str());
        let cache = self.cache_mut(book);
        // A superseded file goes only while it is as the rename left it; a
        // file changed since (by hand) stays, a duplicate.
        cache.superseded.retain(|name, was| {
            let path = dir.join(name);
            match std::fs::metadata(&path) {
                Ok(meta) if fingerprint(&meta) == *was => std::fs::remove_file(&path).is_err(),
                _ => false,
            }
        });
        let mut found: BTreeMap<String, Fingerprint> = BTreeMap::new();
        match std::fs::read_dir(&dir) {
            Ok(rd) => {
                for entry in rd {
                    let entry = entry.map_err(|e| io_err(&dir, &e))?;
                    let Ok(name) = entry.file_name().into_string() else {
                        continue;
                    };
                    if !is_entry_name(&name) {
                        continue;
                    }
                    if let Some(meta) = metadata(&dir.join(&name))?
                        && meta.is_file()
                    {
                        found.insert(name, fingerprint(&meta));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&dir, &e)),
        }
        let cache = self.cache_mut(book);
        cache.files.retain(|name, _| found.contains_key(name));
        cache.superseded.retain(|name, _| found.contains_key(name));
        let mut reread = BTreeSet::new();
        for (name, fp) in found {
            if self
                .cache(book)
                .files
                .get(&name)
                .is_some_and(|f| f.seen == fp)
            {
                continue;
            }
            let path = dir.join(&name);
            let parsed = match read_text(&path)? {
                None => {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    Parsed::broken(
                        "not valid UTF-8; fix it by hand (the game never writes it)",
                        &String::from_utf8_lossy(&bytes),
                    )
                }
                Some(text) => match EntryFile::parse(book, format::stem(&name), &text) {
                    Ok(e) => Parsed::Entry(Box::new(e)),
                    Err(ReadError::NoFrontmatter)
                        if book == Book::Proposals
                            && format::looks_like_old_proposal(&name, &text) =>
                    {
                        Parsed::Old(text)
                    }
                    Err(ReadError::NoFrontmatter) => Parsed::broken(
                        "no frontmatter (the file must start with a `---` line)",
                        &text,
                    ),
                    Err(ReadError::Bad(why)) => Parsed::broken(why, &text),
                },
            };
            self.cache_mut(book)
                .files
                .insert(name.clone(), FileCache { seen: fp, parsed });
            reread.insert(name);
        }
        Ok(reread)
    }

    /// Gives every entry read again whose content differs from its previous
    /// read (same id) while its `updated` stamp does not (a hand edit) the
    /// next revision, written back so writers holding the old stamp get
    /// `Stale`.
    fn bump_hand_edits(
        &mut self,
        book: Book,
        previous: &BTreeMap<u32, Note>,
        reread: &BTreeSet<String>,
    ) -> Result<(), NotesError> {
        let dir = self.dir.join(book.as_str());
        let cache = self.cache_mut(book);
        let index = cache.index.clone();
        for (id, name) in index {
            if !reread.contains(&name) {
                continue;
            }
            let Some(old) = previous.get(&id) else {
                continue;
            };
            let Some(f) = cache.files.get_mut(&name) else {
                continue;
            };
            let Parsed::Entry(file) = &mut f.parsed else {
                continue;
            };
            if file.note.updated != old.updated || !format::content_differs(old, &file.note) {
                continue;
            }
            let mut note = file.note.clone();
            note.updated = format::next_stamp(&note.updated, note.updated.clone());
            file.write_note(note, None);
            f.seen = write_atomic(&dir.join(&name), &file.render())?;
        }
        cache.reindex(book);
        Ok(())
    }

    /// Migrates the old one-file `diary.md` / `memory.md` (see the module
    /// docs): once, when it exists and its `.bak` does not.
    fn migrate_old_book(&mut self, book: Book) -> Result<(), NotesError> {
        let old_name = format!("{}.md", book.as_str());
        let old = self.dir.join(&old_name);
        let bak = self.dir.join(format!("{old_name}.bak"));
        self.cache_mut(book).old_problem = None;
        if !old.is_file() || bak.exists() {
            return Ok(());
        }
        let Some(text) = read_text(&old)? else {
            self.cache_mut(book).old_problem = Some(Unreadable {
                file: old_name,
                reason: "not valid UTF-8, so not moved into its folder; fix it by hand".to_string(),
            });
            return Ok(());
        };
        let doc = Doc::parse(&text);
        let mut notes: Vec<(Note, &'static str)> = doc
            .entries
            .iter()
            .map(|e| (e.note.clone(), e.eol))
            .collect();
        let cache = self.cache(book);
        // Every id a file has (unreadable ones too): such entries are
        // skipped (an earlier run wrote them, or the id is taken).
        let have = cache.seen_ids();
        let stray = doc.stray_text().filter(|body| {
            // A rerun: the kept text already is an entry.
            !cache
                .entries()
                .any(|e| e.note.title == format!("Kept from {old_name}") && e.note.body == *body)
        });
        if let Some(stray) = stray {
            let id = notes
                .iter()
                .map(|(n, _)| n.id)
                .max()
                .unwrap_or(0)
                .max(cache.high_water())
                + 1;
            let zero = Stamp {
                tick: 0,
                date: String::new(),
            };
            notes.push((
                Note {
                    id,
                    title: format!("Kept from {old_name}"),
                    body: stray,
                    created: zero.clone(),
                    updated: zero,
                    status: None,
                    prompted_by: None,
                },
                doc.eol,
            ));
        }
        notes.sort_by_key(|(n, _)| n.id);
        for (note, eol) in notes {
            if !have.contains(&note.id) {
                self.write_new(book, &note, Writer::Agent, eol, &[], None)?;
            }
        }
        std::fs::rename(&old, &bak).map_err(|e| io_err(&old, &e))
    }

    /// Rewrites every old header-block proposal (no frontmatter) as an
    /// entry file under its readable name and removes the old file. Ids: an
    /// `id:` line's, else the file's number, else the next free one (in
    /// file-name order); an old file whose entry already exists (same
    /// title, body and created stamp: an earlier run stopped before the
    /// removal) is only removed.
    fn migrate_old_proposals(&mut self) -> Result<(), NotesError> {
        let book = Book::Proposals;
        let old: Vec<(String, OldProposal)> = self
            .cache(book)
            .files
            .iter()
            .filter_map(|(name, f)| match &f.parsed {
                Parsed::Old(text) => Some((name.clone(), OldProposal::parse(name, text))),
                _ => None,
            })
            .collect();
        if old.is_empty() {
            return Ok(());
        }
        let dir = self.dir.join(book.as_str());
        let existing: Vec<Note> = self.cache(book).entries().map(|e| e.note.clone()).collect();
        let cache = self.cache(book);
        let mut used: BTreeSet<u32> = cache.seen_ids();
        let high_water = cache.high_water();
        let mut todo = Vec::new();
        for (name, p) in old {
            let done = existing.iter().any(|n| {
                n.title == p.note.title && n.body == p.note.body && n.created == p.note.created
            });
            if done {
                let _ = std::fs::remove_file(dir.join(&name));
                self.cache_mut(book).files.remove(&name);
            } else {
                todo.push((name, p));
            }
        }
        // `id:` lines first, then file numbers, then the next free ids.
        let mut ids: Vec<u32> = vec![0; todo.len()];
        for from_id_line in [true, false] {
            for (i, (_, p)) in todo.iter().enumerate() {
                if ids[i] == 0
                    && p.has_id == from_id_line
                    && p.note.id > 0
                    && used.insert(p.note.id)
                {
                    ids[i] = p.note.id;
                }
            }
        }
        for id in ids.iter_mut().filter(|id| **id == 0) {
            *id = used.last().copied().unwrap_or(0).max(high_water) + 1;
            used.insert(*id);
        }
        for ((name, mut p), id) in todo.into_iter().zip(ids) {
            p.note.id = id;
            let written =
                self.write_new(book, &p.note, Writer::Agent, p.eol, &p.extra, Some(&name))?;
            if written != name {
                let _ = std::fs::remove_file(dir.join(&name));
                self.cache_mut(book).files.remove(&name);
            }
        }
        Ok(())
    }
}

/// Which units keep notes, and where: the host maps a unit to its agent
/// folder. A unit missing here keeps no notes (the notes methods answer
/// "unit N keeps no notes"). Host-side: not hashed, not in the snapshot.
#[derive(Resource, Clone, Debug, Default)]
pub struct NotesDirs(pub BTreeMap<SimId, PathBuf>);

/// The date of a note's stamps: the host passes a wall-clock RFC 3339
/// function (the library reads no clock), tests a fixed one. Without this
/// resource dates are empty (stamps carry only the tick).
#[derive(Resource, Clone, Copy, Debug)]
pub struct NotesClock(pub fn() -> String);

/// The open notes stores, one per agent folder in this process, opened on
/// first use, and the folders' owners. Everything that writes notes in a process (the agent and
/// player methods, an in-game editor) goes through this one resource, so
/// a folder has one cache and one writer. Host-side: not hashed, not in the
/// snapshot.
#[derive(Resource, Default)]
pub struct NotesStores {
    stores: BTreeMap<PathBuf, NotesStore>,
    owners: BTreeMap<PathBuf, NotesOwner>,
}

impl NotesStores {
    /// The store of the agent folder `dir`, opened (and kept) on first use,
    /// with the owner set for it ([`NotesStores::set_owner`]).
    ///
    /// # Errors
    /// What [`NotesStore::open_as`] returns; a folder that failed to open is
    /// tried again on the next call.
    pub fn store_for(&mut self, dir: &Path) -> Result<&mut NotesStore, NotesError> {
        match self.stores.entry(dir.to_path_buf()) {
            std::collections::btree_map::Entry::Occupied(e) => Ok(e.into_mut()),
            std::collections::btree_map::Entry::Vacant(e) => {
                let owner = self.owners.get(dir).cloned().unwrap_or_default();
                Ok(e.insert(NotesStore::open_as(dir, owner)?))
            }
        }
    }

    /// Who the agent folder `dir` belongs to: the `agent` and `world` of the
    /// entries written there from now on. Set it before the folder's store
    /// opens (the host does, next to [`NotesDirs`]) so a migration writes
    /// them too.
    pub fn set_owner(&mut self, dir: &Path, owner: NotesOwner) {
        if let Some(store) = self.stores.get_mut(dir) {
            store.set_owner(owner.clone());
        }
        self.owners.insert(dir.to_path_buf(), owner);
    }
}
