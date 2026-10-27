//! An actor's owned landmarks, stored in Obsidian-compatible `map/*.md`.
//! This is host data: never registered in simulation hashes or snapshots.
use crate::notes::storage::{self, Fingerprint};
use crate::notes::{NotesError, NotesOwner, Stamp, Unreadable, Writer};
use crate::notes::{filename, format, frontmatter};
use bevy_ecs::prelude::*;
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Resource, Clone, Copy, Debug)]
pub struct MapEnabled(pub bool);
pub fn enabled(world: &World) -> bool {
    world.get_resource::<MapEnabled>().is_none_or(|m| m.0)
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Landmark {
    pub id: u32,
    pub name: String,
    pub pos: [i32; 2],
    pub kind: Option<String>,
    pub thing: Option<SimId>,
    pub note: String,
    pub created: Stamp,
    pub updated: Stamp,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Here,
    Thing(SimId),
    Pos([i32; 2]),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewLandmark {
    pub name: String,
    pub place: Place,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub note: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandmarkChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_kind",
        skip_serializing_if = "Option::is_none"
    )]
    pub kind: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<[i32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
fn present_kind<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}
#[derive(Clone, Debug, PartialEq)]
pub enum MapError {
    TooLong { field: &'static str, max: usize },
    NotFound(u32),
    Stale { id: u32, current: Stamp },
    Invalid(String),
    Exhausted(&'static str),
    Io(String),
}
impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong { field, max } => write!(
                f,
                "{field} is too long: at most {max} {}",
                if *field == "note" {
                    "bytes"
                } else {
                    "characters"
                }
            ),
            Self::NotFound(id) => write!(f, "no landmark with id {id}"),
            Self::Stale { id, current } => write!(
                f,
                "landmark {id} changed since you read it (now tick {}, {}); list it again",
                current.tick, current.date
            ),
            Self::Invalid(s) => f.write_str(s),
            Self::Exhausted(s) => write!(f, "{s} exhausted"),
            Self::Io(s) => write!(f, "map file error: {s}"),
        }
    }
}
impl std::error::Error for MapError {}
impl From<NotesError> for MapError {
    fn from(e: NotesError) -> Self {
        match e {
            NotesError::Io(s) => Self::Io(s),
            NotesError::Exhausted(s) => Self::Exhausted(s),
            NotesError::TooLong { field, max } => Self::TooLong { field, max },
            NotesError::NotFound(id) => Self::NotFound(id),
            NotesError::Stale { id, current } => Self::Stale { id, current },
            NotesError::Forbidden(s) => Self::Invalid(s.into()),
        }
    }
}
pub fn validate_pos(pos: [i32; 2]) -> Result<(), MapError> {
    let limit = npc_kernel::commands::POS_LIMIT as u32;
    if pos.into_iter().any(|v| v.unsigned_abs() > limit) {
        Err(MapError::Invalid(format!(
            "position is outside ±{limit} mm"
        )))
    } else {
        Ok(())
    }
}
fn title(field: &'static str, text: &str) -> Result<String, MapError> {
    let s = format::one_line(text);
    if s.chars().count() > 200 {
        return Err(MapError::TooLong { field, max: 200 });
    }
    if field == "name" && s.is_empty() {
        return Err(MapError::Invalid("name must not be empty".into()));
    }
    Ok(s)
}
fn body(text: &str) -> Result<String, MapError> {
    let s = format::clean_body(text);
    if s.len() > 8192 {
        Err(MapError::TooLong {
            field: "note",
            max: 8192,
        })
    } else {
        Ok(s)
    }
}
fn stamp(at: Stamp) -> Stamp {
    Stamp {
        tick: at.tick,
        date: format::clean_date(&at.date),
    }
}
fn optional_kind(s: Option<String>) -> Result<Option<String>, MapError> {
    s.map(|s| title("kind", &s))
        .transpose()
        .map(|s| s.filter(|s| !s.is_empty()))
}
fn next_id(id: u32) -> Result<u32, MapError> {
    storage::next_id(id).map_err(Into::into)
}

#[derive(Clone)]
struct Entry {
    fm: frontmatter::Frontmatter,
    mark: Landmark,
    body_raw: String,
}
fn number<T: std::str::FromStr>(
    fm: &frontmatter::Frontmatter,
    key: &str,
    default: T,
) -> Result<T, String> {
    match fm.text(key) {
        None | Some("") => Ok(default),
        Some(s) => s.parse().map_err(|_| format!("invalid {key}: {s}")),
    }
}
fn nullable(fm: &frontmatter::Frontmatter, key: &str) -> Option<String> {
    fm.text(key)
        .filter(|s| !s.is_empty() && *s != "null" && *s != "~")
        .map(str::to_owned)
}
impl Entry {
    fn parse(text: &str) -> Result<Self, String> {
        let (fm, raw) =
            frontmatter::split(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        for key in [
            "id",
            "type",
            "name",
            "kind",
            "thing",
            "created",
            "created_tick",
            "updated",
            "updated_tick",
            "revision",
        ] {
            if fm.is_complex(key) {
                return Err(format!("complex {key} is not supported"));
            }
        }
        if fm.text("type") != Some("landmark") {
            return Err("type must be landmark".into());
        }
        let name = title("name", fm.text("name").unwrap_or_default()).map_err(|e| e.to_string())?;
        let coords = fm.list("pos").ok_or("pos must be [x, z]")?;
        if coords.len() != 2 {
            return Err("pos must contain exactly two coordinates".into());
        }
        let pos = [
            coords[0].parse::<i32>().map_err(|_| "invalid pos x")?,
            coords[1].parse::<i32>().map_err(|_| "invalid pos z")?,
        ];
        validate_pos(pos).map_err(|e| e.to_string())?;
        let thing = nullable(&fm, "thing")
            .map(|s| {
                s.strip_prefix('#')
                    .unwrap_or(&s)
                    .parse::<u32>()
                    .map(SimId)
                    .map_err(|_| "thing must be #ID".to_owned())
            })
            .transpose()?;
        let revision = number(&fm, "revision", 1u32)?.max(1);
        let date = fm.text("updated").unwrap_or_default();
        let mark = Landmark {
            id: number(&fm, "id", 0)?,
            name,
            pos,
            kind: optional_kind(nullable(&fm, "kind")).map_err(|e| e.to_string())?,
            thing,
            note: body(raw).map_err(|e| e.to_string())?,
            created: Stamp {
                tick: number(&fm, "created_tick", 0)?,
                date: fm.text("created").unwrap_or_default().into(),
            },
            updated: Stamp {
                tick: number(&fm, "updated_tick", 0)?,
                date: if revision > 1 {
                    format!("{date}#r{revision}")
                } else {
                    date.into()
                },
            },
        };
        Ok(Self {
            fm,
            mark,
            body_raw: raw.into(),
        })
    }
    fn create(mark: Landmark, owner: &NotesOwner, by: Writer) -> Self {
        let mut fm = frontmatter::Frontmatter::new("\n");
        fm.set_plain("id", &mark.id.to_string());
        fm.set_text("type", "landmark");
        fm.set_text("name", &mark.name);
        fm.set_list("aliases", &[]);
        fm.set_list("tags", &["npc/landmark".into()]);
        fm.set_text("agent", &owner.agent);
        fm.set_text("world", &owner.world);
        match &mark.kind {
            Some(kind) => fm.set_text("kind", kind),
            None => fm.set_plain("kind", "null"),
        }
        fm.set_text(
            "thing",
            &mark
                .thing
                .map(|id| format!("#{}", id.0))
                .unwrap_or_default(),
        );
        fm.set_date("created", &mark.created.date);
        fm.set_plain("created_tick", &mark.created.tick.to_string());
        let raw = format!("{}\n", mark.note);
        let mut entry = Self {
            fm,
            mark: mark.clone(),
            body_raw: raw,
        };
        entry.write(mark, Some(by));
        entry
    }
    fn write(&mut self, mark: Landmark, by: Option<Writer>) {
        if mark.name != self.mark.name {
            let mut aliases = self.fm.list("aliases").unwrap_or_default();
            if !aliases.contains(&self.mark.name) {
                aliases.push(self.mark.name.clone());
            }
            self.fm.set_list("aliases", &aliases);
        }
        self.fm.set_plain_first("id", &mark.id.to_string());
        self.fm.set_text("name", &mark.name);
        self.fm.set(
            "pos",
            frontmatter::Value::List(mark.pos.map(|v| v.to_string()).to_vec()),
            &format!("[{}, {}]", mark.pos[0], mark.pos[1]),
        );
        if mark.kind != self.mark.kind {
            match &mark.kind {
                Some(kind) => self.fm.set_text("kind", kind),
                None => self.fm.set_plain("kind", "null"),
            }
        }
        if mark.thing != self.mark.thing {
            self.fm.set_text(
                "thing",
                &mark
                    .thing
                    .map(|id| format!("#{}", id.0))
                    .unwrap_or_default(),
            );
        }
        if let Some(by) = by {
            self.fm.set_text(
                "author",
                match by {
                    Writer::Agent => "agent",
                    Writer::Player => "player",
                },
            );
        }
        let (date, revision) = format::split_rev(&mark.updated.date);
        self.fm.set_date("updated", date);
        self.fm
            .set_plain("updated_tick", &mark.updated.tick.to_string());
        self.fm.set_plain("revision", &revision.to_string());
        if mark.note != self.mark.note {
            self.body_raw = if mark.note.is_empty() {
                String::new()
            } else {
                format!(
                    "{}{}",
                    mark.note.replace('\n', self.fm.eol()),
                    self.fm.eol()
                )
            };
        }
        self.mark = mark;
    }
    fn render(&self) -> String {
        format!("{}{}", self.fm.render(), self.body_raw)
    }
}
#[derive(Clone)]
struct Cached {
    seen: Fingerprint,
    entry: Result<Entry, String>,
    id: Option<u32>,
}
pub struct MapStore {
    dir: PathBuf,
    owner: NotesOwner,
    files: BTreeMap<String, Cached>,
    index: BTreeMap<u32, String>,
    unreadable: Vec<Unreadable>,
    seen_max: u32,
    superseded: BTreeMap<String, Fingerprint>,
}
impl MapStore {
    pub fn open(dir: &Path) -> Result<Self, MapError> {
        Self::open_as(dir, NotesOwner::default())
    }
    pub fn open_as(dir: &Path, owner: NotesOwner) -> Result<Self, MapError> {
        if dir.exists() && !dir.is_dir() {
            return Err(MapError::Io(format!("{} is not a folder", dir.display())));
        }
        let mut s = Self {
            dir: dir.into(),
            owner,
            files: BTreeMap::new(),
            index: BTreeMap::new(),
            unreadable: vec![],
            seen_max: 0,
            superseded: BTreeMap::new(),
        };
        s.refresh()?;
        Ok(s)
    }
    pub fn set_owner(&mut self, owner: NotesOwner) {
        self.owner = owner;
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn list(&mut self) -> Result<Vec<Landmark>, MapError> {
        self.refresh()?;
        Ok(self
            .index
            .values()
            .filter_map(|name| self.files[name].entry.as_ref().ok().map(|e| e.mark.clone()))
            .collect())
    }
    pub fn unreadable(&mut self) -> Result<Vec<Unreadable>, MapError> {
        self.refresh()?;
        Ok(self.unreadable.clone())
    }
    pub fn path_for(&mut self, id: u32) -> Result<PathBuf, MapError> {
        self.refresh()?;
        Ok(self
            .dir
            .join("map")
            .join(self.index.get(&id).ok_or(MapError::NotFound(id))?))
    }
    pub fn add(
        &mut self,
        new: NewLandmark,
        resolved_pos: [i32; 2],
        thing: Option<SimId>,
        at: Stamp,
        by: Writer,
    ) -> Result<Landmark, MapError> {
        let name = title("name", &new.name)?;
        let note = body(&new.note)?;
        let kind = optional_kind(new.kind)?;
        validate_pos(resolved_pos)?;
        self.refresh()?;
        let at = stamp(at);
        let mark = Landmark {
            id: next_id(self.seen_max)?,
            name,
            pos: resolved_pos,
            kind,
            thing,
            note,
            created: at.clone(),
            updated: at,
        };
        let entry = Entry::create(mark.clone(), &self.owner, by);
        let name = self.free_name(&mark, None);
        let seen = storage::write_atomic(&self.dir.join("map").join(&name), &entry.render())?;
        self.files.insert(
            name,
            Cached {
                seen,
                id: Some(mark.id),
                entry: Ok(entry),
            },
        );
        self.reindex();
        Ok(mark)
    }
    pub fn update(
        &mut self,
        id: u32,
        expected: &Stamp,
        change: LandmarkChange,
        at: Stamp,
        by: Writer,
    ) -> Result<Landmark, MapError> {
        let name = change.name.map(|s| title("name", &s)).transpose()?;
        let note = change.note.map(|s| body(&s)).transpose()?;
        let kind = change.kind.map(optional_kind).transpose()?;
        if let Some(pos) = change.pos {
            validate_pos(pos)?;
        }
        self.refresh()?;
        let source = self.index.get(&id).ok_or(MapError::NotFound(id))?.clone();
        let cached = &self.files[&source];
        let mut entry = cached
            .entry
            .as_ref()
            .map_err(|_| MapError::NotFound(id))?
            .clone();
        let original = cached.seen;
        if entry.mark.updated != stamp(expected.clone()) {
            return Err(MapError::Stale {
                id,
                current: entry.mark.updated,
            });
        }
        let mut mark = entry.mark.clone();
        if let Some(name) = name {
            mark.name = name;
        }
        if let Some(note) = note {
            mark.note = note;
        }
        if let Some(kind) = kind {
            mark.kind = kind;
        }
        if let Some(pos) = change.pos {
            mark.pos = pos;
        }
        mark.updated = format::next_stamp(&mark.updated, stamp(at))?;
        let wanted = if mark.name == entry.mark.name {
            source.clone()
        } else {
            self.free_name(&mark, Some(&source))
        };
        let target = if wanted.eq_ignore_ascii_case(&source) {
            source.clone()
        } else {
            wanted
        };
        entry.write(mark.clone(), Some(by));
        let seen = storage::write_atomic(&self.dir.join("map").join(&target), &entry.render())?;
        if source != target {
            match storage::remove_unchanged(&self.dir.join("map").join(&source), original) {
                Ok(true) => {
                    self.files.remove(&source);
                }
                Ok(false) => {}
                Err(_) => {
                    self.superseded.insert(source.clone(), original);
                }
            }
        }
        self.files.insert(
            target,
            Cached {
                seen,
                id: Some(id),
                entry: Ok(entry),
            },
        );
        self.reindex();
        Ok(mark)
    }
    fn free_name(&self, mark: &Landmark, except: Option<&str>) -> String {
        let base = filename::entry_name("Landmark", mark.id, &mark.name, None);
        let taken: BTreeSet<_> = self
            .files
            .keys()
            .filter(|n| !except.is_some_and(|e| n.eq_ignore_ascii_case(e)))
            .map(|n| n.to_lowercase())
            .collect();
        let mut number = 1u32;
        loop {
            let name = filename::numbered(&base, number);
            if !taken.contains(&name.to_lowercase())
                && (!self.dir.join("map").join(&name).exists()
                    || except.is_some_and(|e| name.eq_ignore_ascii_case(e)))
            {
                return name;
            }
            number += 1;
        }
    }
    fn refresh(&mut self) -> Result<(), MapError> {
        let dir = self.dir.join("map");
        self.superseded
            .retain(|name, seen| storage::remove_unchanged(&dir.join(name), *seen).is_err());
        let mut files = BTreeMap::new();
        let mut changed = BTreeSet::new();
        let mut max = self.seen_max;
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => Some(e),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(MapError::Io(format!("{}: {e}", dir.display()))),
        };
        if let Some(entries) = entries {
            for entry in entries {
                let entry = entry.map_err(|e| MapError::Io(e.to_string()))?;
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.starts_with('.')
                    || !name.to_lowercase().ends_with(".md")
                    || !entry.path().is_file()
                {
                    continue;
                }
                let (bytes, seen) = storage::read(&entry.path())?;
                let cached = if let Some(old) = self.files.get(&name).filter(|f| f.seen == seen) {
                    old.clone()
                } else {
                    changed.insert(name.clone());
                    let text = String::from_utf8_lossy(&bytes);
                    let id = format::lenient_id(&text);
                    let parsed = match std::str::from_utf8(&bytes) {
                        Ok(text) => Entry::parse(text),
                        Err(_) => Err("not valid UTF-8".into()),
                    };
                    Cached {
                        seen,
                        entry: parsed,
                        id,
                    }
                };
                max = max
                    .max(cached.id.unwrap_or(0))
                    .max(cached.entry.as_ref().map_or(0, |e| e.mark.id));
                files.insert(name, cached);
            }
        }
        for (name, cached) in &mut files {
            let Ok(entry) = &mut cached.entry else {
                continue;
            };
            if entry.mark.id == 0 {
                let id = next_id(max)?;
                max = id;
                entry.mark.id = id;
                entry.fm.set_plain_first("id", &id.to_string());
                cached.id = Some(id);
                cached.seen = storage::write_atomic(&dir.join(name), &entry.render())?;
            }
            if changed.contains(name) {
                let old = self
                    .index
                    .get(&entry.mark.id)
                    .and_then(|n| self.files.get(n))
                    .and_then(|f| f.entry.as_ref().ok());
                if let Some(old) = old
                    && entry.mark.updated == old.mark.updated
                    && content_differs(&entry.mark, &old.mark)
                {
                    let mut mark = entry.mark.clone();
                    mark.updated = format::next_stamp(&mark.updated, mark.updated.clone())?;
                    entry.write(mark, None);
                    cached.seen = storage::write_atomic(&dir.join(name), &entry.render())?;
                }
            }
        }
        self.files = files;
        self.seen_max = max;
        self.superseded
            .retain(|name, _| self.files.contains_key(name));
        self.reindex();
        Ok(())
    }
    fn reindex(&mut self) {
        let mut index: BTreeMap<u32, String> = BTreeMap::new();
        let mut unreadable = vec![];
        for (name, file) in &self.files {
            let entry = match &file.entry {
                Ok(e) => e,
                Err(why) => {
                    unreadable.push(Unreadable {
                        file: format!("map/{name}"),
                        reason: why.clone(),
                    });
                    continue;
                }
            };
            self.seen_max = self.seen_max.max(entry.mark.id).max(file.id.unwrap_or(0));
            if let Some(old) = index.get(&entry.mark.id) {
                if self.newer(name, old).is_gt() {
                    index.insert(entry.mark.id, name.clone());
                }
            } else {
                index.insert(entry.mark.id, name.clone());
            }
        }
        for (name, file) in &self.files {
            if let Ok(e) = &file.entry
                && index.get(&e.mark.id).is_some_and(|best| best != name)
            {
                unreadable.push(Unreadable {
                    file: format!("map/{name}"),
                    reason: format!("duplicate id {}", e.mark.id),
                });
            }
        }
        unreadable.sort_by(|a, b| a.file.cmp(&b.file));
        self.index = index;
        self.unreadable = unreadable;
    }
    fn newer(&self, a: &str, b: &str) -> std::cmp::Ordering {
        let fa = &self.files[a];
        let fb = &self.files[b];
        let (Ok(ae), Ok(be)) = (&fa.entry, &fb.entry) else {
            return std::cmp::Ordering::Equal;
        };
        let (ad, ar) = format::split_rev(&ae.mark.updated.date);
        let (bd, br) = format::split_rev(&be.mark.updated.date);
        let date = if frontmatter::is_utc_timestamp(ad) && frontmatter::is_utc_timestamp(bd) {
            ad.cmp(bd)
        } else {
            std::cmp::Ordering::Equal
        };
        (!self.superseded.contains_key(a))
            .cmp(&!self.superseded.contains_key(b))
            .then(date)
            .then(ae.mark.updated.tick.cmp(&be.mark.updated.tick))
            .then(ar.cmp(&br))
            .then(fa.seen.modified.cmp(&fb.seen.modified))
    }
}
fn content_differs(a: &Landmark, b: &Landmark) -> bool {
    a.name != b.name || a.pos != b.pos || a.kind != b.kind || a.thing != b.thing || a.note != b.note
}
