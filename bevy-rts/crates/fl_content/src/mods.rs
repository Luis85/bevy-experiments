//! Mods: manifests, load order, file discovery and id checks. [`load_dirs`] is the loader.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};

use crate::db::{ContentDb, KindFiles, Origin, RulesFiles, record_origins};
use crate::def::DefComponent;
use crate::error::{ContentError, ContentErrors, read_text};
use crate::inherit::{Head, ResolvedObject, resolve_partial};
use crate::kinds::{ErasedKind, ErasedRules};
use crate::object::{ObjectDoc, deserialize_component, object_doc};
use crate::registry::DefRegistry;
use crate::rules::parse_rules;
use crate::tree::{Node, PathSeg, parse_ron};

/// A loaded mod's `mod.ron`, plus the directory it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModManifest {
    pub id: String,
    pub version: String,
    /// Mods that must be loaded, and load before this one.
    pub depends: Vec<String>,
    /// Mods that load before this one if present.
    pub load_after: Vec<String>,
    pub dir: PathBuf,
}

/// `Mod(id: "base", version: "0.1.0", depends: [], load_after: [])`.
#[derive(Deserialize)]
#[serde(rename = "Mod", deny_unknown_fields)]
struct ModFile {
    id: String,
    version: String,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    load_after: Vec<String>,
}

/// Licenses an asset may carry. Non-commercial and no-derivatives licenses are rejected.
pub const ALLOWED_LICENSES: &[&str] = &["CC0-1.0", "CC-BY-3.0", "CC-BY-4.0", "OFL-1.1"];

/// One attributed asset from a mod's `credits.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditEntry {
    pub asset: String,
    pub title: String,
    pub author: String,
    /// An SPDX id from [`ALLOWED_LICENSES`].
    #[serde(deserialize_with = "deserialize_license")]
    pub license: String,
    pub url: String,
}

fn deserialize_license<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let license = String::deserialize(deserializer)?;
    if ALLOWED_LICENSES.contains(&license.as_str()) {
        Ok(license)
    } else {
        Err(serde::de::Error::custom(format_args!(
            "license '{license}' is not allowed (allowed: {})",
            ALLOWED_LICENSES.join(", ")
        )))
    }
}

/// `Credits([ (asset: …, title: …, author: …, license: …, url: …), … ])`.
#[derive(Deserialize)]
#[serde(rename = "Credits")]
struct CreditsFile(Vec<CreditEntry>);

/// Load, check, resolve and link the mods in `dirs` (in any order) into a [`ContentDb`].
///
/// The load phases (ADR-0020): discover the mods and order them; parse every file into a
/// content tree ([`parse_ron`]); apply overrides; inherit per component on the object trees
/// (a child's component replaces the parent's whole, `remove` drops one); deserialize every
/// typed value through the registry over a [`TreeDeserializer`](crate::TreeDeserializer);
/// link; freeze and hash. Every leaf of every document records its [`Origin`]
/// ([`ContentDb::origins`]).
///
/// Freezes `registry` first. Every error found is reported, not just the first.
pub fn load_dirs(mut registry: DefRegistry, dirs: &[PathBuf]) -> Result<ContentDb, ContentErrors> {
    registry.freeze();
    let mut errors = Vec::new();
    let mods = order_mods(read_manifests(dirs, &mut errors), &mut errors);

    let mut files = ModFiles::default();
    for (index, manifest) in mods.iter().enumerate() {
        files.load_mod(&registry, index, manifest, &mut errors);
    }
    let mut origins = BTreeMap::new();

    let docs = object_docs(&registry, &files.objects, &mut errors);
    let (docs, replaced) = check_object_ids(&mods, docs, &mut errors);
    // A document an `override` replaced is still checked: its mod is broken without the override.
    typecheck_docs(&registry, &replaced, &mut errors);
    for (mod_index, doc) in &docs {
        let origin = origin(&mods, *mod_index, &doc.file);
        let key = format!("{}:{}", doc.kind, doc.id);
        record_origins(&mut origins, &key, &mut Vec::new(), doc.tree, &origin);
    }
    let abstract_units: BTreeSet<String> = docs
        .iter()
        .filter(|(_, doc)| doc.kind == "unit" && doc.is_abstract)
        .map(|(_, doc)| doc.id.clone())
        .collect();
    let concrete_unit_ids: BTreeSet<String> = docs
        .iter()
        .filter(|(_, doc)| doc.kind == "unit" && !doc.is_abstract)
        .map(|(_, doc)| doc.id.clone())
        .collect();
    let resolved = resolve_docs(&registry, &docs, &mut errors);
    let (units, other_objects): (Vec<_>, Vec<_>) =
        resolved.into_iter().partition(|o| o.kind == "unit");
    // Units whose chain or components failed: the link pass treats them as existing, so a
    // reference to one is not reported again as a missing unit.
    let mut unresolved_units = concrete_unit_ids;
    for unit in &units {
        unresolved_units.remove(&unit.id);
    }
    let max_units = usize::from(u16::MAX);
    let within_cap = units.len() <= max_units;
    if !within_cap {
        let first_over = &units[max_units];
        errors.push(ContentError::at(
            &first_over.file,
            first_over.id.as_str(),
            format!(
                "{} concrete units exceed the limit of {max_units}",
                units.len()
            ),
        ));
    }

    let (kinds, kind_files) =
        build_kind_tables(&registry, &mods, &files.kinds, &mut origins, &mut errors);

    // A later mod replaces a rules section wholesale.
    let mut sections: BTreeMap<&'static str, RulesSource<'_>> = BTreeMap::new();
    for parsed in &files.rules {
        match parse_rules(&registry, &parsed.file, &parsed.tree) {
            Ok(parsed_sections) => {
                for section in parsed_sections {
                    let source = RulesSource {
                        mod_index: parsed.mod_index,
                        file: &parsed.file,
                        value: section.value,
                        tree: section.tree,
                    };
                    sections.insert(section.name, source);
                }
            }
            Err(err) => errors.push(err),
        }
    }
    let mut rules_files = RulesFiles::new();
    let mut rules = BTreeMap::new();
    for (name, source) in sections {
        let origin = origin(&mods, source.mod_index, source.file);
        let mut path = vec![PathSeg::Field(name.to_owned())];
        record_origins(&mut origins, "rules", &mut path, source.tree, &origin);
        rules_files.insert(name, source.file.to_path_buf());
        rules.insert(name, source.value);
    }
    // A missing section belongs in the last-loaded mod's rules.ron.
    let rules_file = mods
        .last()
        .map_or_else(|| PathBuf::from("rules.ron"), |m| m.dir.join("rules.ron"));
    for name in registry.rules_section_names() {
        if !rules.contains_key(name) {
            errors.push(ContentError::at(
                &rules_file,
                name,
                format!("missing rules section '{name}': no mod's rules.ron defines it"),
            ));
        }
    }

    let mut db = ContentDb {
        registry,
        mods,
        units,
        abstract_units,
        unresolved_units,
        kinds,
        rules,
        credits: files.credits,
        warnings: Vec::new(),
        rules_hash: 0,
        origins,
    };
    // Over the cap, unit ids are not all representable, so linking would mislead.
    if within_cap {
        db.link(&other_objects, &kind_files, &rules_files, &mut errors);
    }
    if !errors.is_empty() {
        return Err(ContentErrors(errors));
    }
    db.warnings = db.missing_visual_warnings();
    for warning in &db.warnings {
        tracing::warn!("{warning}");
    }
    db.rules_hash = crate::hash::rules_hash(&db);
    Ok(db)
}

/// The rules section a mod's `rules.ron` defines, with where it came from.
struct RulesSource<'t> {
    mod_index: usize,
    file: &'t Path,
    value: Box<dyn ErasedRules>,
    tree: &'t Node,
}

/// The origin of a value that mod `mod_index` set in `file`.
fn origin(mods: &[ModManifest], mod_index: usize, file: &Path) -> Origin {
    Origin {
        mod_id: mods[mod_index].id.clone(),
        file: file.to_path_buf(),
    }
}

/// Read `mod.ron` from every directory. A missing manifest or a duplicate id is an error.
fn read_manifests(dirs: &[PathBuf], errors: &mut Vec<ContentError>) -> Vec<ModManifest> {
    let mut mods: Vec<ModManifest> = Vec::new();
    for dir in dirs {
        let path = dir.join("mod.ron");
        if !path.is_file() {
            errors.push(ContentError::at(
                &path,
                "",
                "missing mod.ron: every mod directory needs one",
            ));
            continue;
        }
        let file: ModFile = match read_text(&path).and_then(|text| {
            ron::Options::default()
                .from_str(&text)
                .map_err(|err| ContentError::parse(&path, &err))
        }) {
            Ok(file) => file,
            Err(err) => {
                errors.push(err);
                continue;
            }
        };
        if let Some(first) = mods.iter().find(|m| m.id == file.id) {
            errors.push(ContentError::at(
                &path,
                "id",
                format!(
                    "duplicate mod id '{}' (also in {})",
                    file.id,
                    first.dir.display()
                ),
            ));
            continue;
        }
        mods.push(ModManifest {
            id: file.id,
            version: file.version,
            depends: file.depends,
            load_after: file.load_after,
            dir: dir.clone(),
        });
    }
    mods
}

/// Order mods so each comes after everything in its `depends` and `load_after`, breaking
/// ties by id. A missing `depends` target and a cycle are errors; a missing `load_after`
/// target is ignored.
fn order_mods(mods: Vec<ModManifest>, errors: &mut Vec<ContentError>) -> Vec<ModManifest> {
    let index: BTreeMap<&str, usize> = mods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.as_str(), i))
        .collect();
    let mut preds: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); mods.len()];
    for (i, m) in mods.iter().enumerate() {
        for dep in &m.depends {
            if !index.contains_key(dep.as_str()) {
                errors.push(ContentError::at(
                    &m.dir.join("mod.ron"),
                    "depends",
                    format!("mod '{}' depends on mod '{dep}', which is not loaded", m.id),
                ));
            }
        }
        for dep in m.depends.iter().chain(&m.load_after) {
            if let Some(&d) = index.get(dep.as_str()) {
                preds[i].insert(d);
            }
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(mods.len());
    let mut ready: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, p) in preds.iter().enumerate() {
        if p.is_empty() {
            ready.insert(mods[i].id.as_str(), i);
        }
    }
    while let Some((_, next)) = ready.pop_first() {
        order.push(next);
        for (i, p) in preds.iter_mut().enumerate() {
            if p.remove(&next) && p.is_empty() {
                ready.insert(mods[i].id.as_str(), i);
            }
        }
    }
    if order.len() < mods.len() {
        // `index` is sorted by id, so the leftovers are appended in id order.
        let stuck: Vec<usize> = index
            .values()
            .copied()
            .filter(|i| !order.contains(i))
            .collect();
        let ids: Vec<&str> = stuck.iter().map(|&i| mods[i].id.as_str()).collect();
        for &i in &stuck {
            errors.push(ContentError::at(
                &mods[i].dir.join("mod.ron"),
                "depends",
                format!("mod dependency cycle among: {}", ids.join(", ")),
            ));
        }
        order.extend(stuck);
    }

    let mut slots: Vec<Option<ModManifest>> = mods.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| slots[i].take().expect("each mod is ordered once"))
        .collect()
}

/// One content file parsed into a tree.
struct ParsedFile {
    mod_index: usize,
    file: PathBuf,
    /// The object or content kind (`"unit"`, `"terrain"`, …); empty for `rules.ron`.
    ext: &'static str,
    tree: Node,
}

/// Every file of the mods, parsed, in load order.
#[derive(Default)]
struct ModFiles {
    /// Object files (`*.unit.ron`, …).
    objects: Vec<ParsedFile>,
    /// Content-kind files (`*.terrain.ron`, …).
    kinds: Vec<ParsedFile>,
    /// `rules.ron` files.
    rules: Vec<ParsedFile>,
    credits: Vec<CreditEntry>,
}

impl ModFiles {
    fn load_mod(
        &mut self,
        reg: &DefRegistry,
        mod_index: usize,
        manifest: &ModManifest,
        errors: &mut Vec<ContentError>,
    ) {
        for rel in list_files(&manifest.dir, errors) {
            let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".ron") else {
                continue; // Non-RON files are ignored.
            };
            let at_root = rel.components().count() == 1;
            if at_root && name == "mod.ron" {
                continue; // Already read.
            }
            let file = manifest.dir.join(&rel);
            let text = match read_text(&file) {
                Ok(text) => text,
                Err(err) => {
                    errors.push(err);
                    continue;
                }
            };
            if at_root && name == "credits.ron" {
                match ron::Options::default().from_str::<CreditsFile>(&text) {
                    Ok(CreditsFile(entries)) => self.credits.extend(entries),
                    Err(err) => errors.push(ContentError::parse(&file, &err)),
                }
                continue;
            }
            let ext = stem.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            let (list, ext) = if at_root && name == "rules.ron" {
                (&mut self.rules, "")
            } else if let Some(kind) = reg.object_kind(ext) {
                (&mut self.objects, kind)
            } else if let Some(entry) = reg.kind_entry(ext) {
                (&mut self.kinds, entry.ext)
            } else {
                errors.push(ContentError::at(
                    &file,
                    "",
                    format!(
                        "unknown content file type (expected mod.ron, rules.ron, credits.ron or \
                         *.<kind>.ron for a registered kind: {})",
                        known_exts(reg).join(", ")
                    ),
                ));
                continue;
            };
            match parse_ron(&file, &text) {
                Ok(tree) => list.push(ParsedFile {
                    mod_index,
                    file,
                    ext,
                    tree,
                }),
                Err(err) => errors.push(err),
            }
        }
    }
}

/// Registered object and content kind extensions, sorted.
fn known_exts(reg: &DefRegistry) -> Vec<&'static str> {
    let mut exts: Vec<&'static str> = reg.kind_entries().map(|e| e.ext).collect();
    exts.extend(reg.object_kinds());
    exts.sort_unstable();
    exts
}

/// Every file under `dir`, relative to it, sorted by path.
fn list_files(dir: &Path, errors: &mut Vec<ContentError>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        let abs = dir.join(&rel);
        let entries = match std::fs::read_dir(&abs) {
            Ok(entries) => entries,
            Err(err) => {
                errors.push(ContentError::at(&abs, "", format!("cannot list: {err}")));
                continue;
            }
        };
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let child = rel.join(entry.file_name());
                    match entry.file_type() {
                        Ok(t) if t.is_dir() => pending.push(child),
                        Ok(_) => out.push(child),
                        Err(err) => errors.push(ContentError::at(
                            &dir.join(&child),
                            "",
                            format!("cannot stat: {err}"),
                        )),
                    }
                }
                Err(err) => errors.push(ContentError::at(&abs, "", format!("cannot list: {err}"))),
            }
        }
    }
    out.sort();
    out
}

/// Read every object file's document. A file whose document is malformed is reported and
/// left out.
fn object_docs<'t>(
    reg: &DefRegistry,
    parsed: &'t [ParsedFile],
    errors: &mut Vec<ContentError>,
) -> Vec<(usize, ObjectDoc<'t>)> {
    let mut docs = Vec::with_capacity(parsed.len());
    for p in parsed {
        match object_doc(reg, p.ext, &p.file, &p.tree) {
            Ok(doc) => docs.push((p.mod_index, doc)),
            Err(err) => errors.push(err),
        }
    }
    docs
}

/// The surviving object documents and the ones an `override` replaced.
type CheckedDocs<'t> = (Vec<(usize, ObjectDoc<'t>)>, Vec<(usize, ObjectDoc<'t>)>);

/// Apply overrides and reject duplicate or wrongly namespaced ids. Returns the surviving
/// documents in id order, with the index of the mod that defined each, and the documents an
/// `override` replaced.
fn check_object_ids<'t>(
    mods: &[ModManifest],
    docs: Vec<(usize, ObjectDoc<'t>)>,
    errors: &mut Vec<ContentError>,
) -> CheckedDocs<'t> {
    let mut by_id: BTreeMap<String, (usize, ObjectDoc<'t>)> = BTreeMap::new();
    let mut replaced = Vec::new();
    for (mod_index, def) in docs {
        let mod_id = &mods[mod_index].id;
        match by_id.get(&def.id) {
            Some((prev_mod, prev)) if def.is_override => {
                if *prev_mod < mod_index {
                    let id = def.id.clone();
                    replaced.extend(by_id.insert(id, (mod_index, def)));
                } else {
                    errors.push(ContentError::at(
                        &def.file,
                        format!("{}/override", def.id),
                        format!(
                            "override must replace an object from an earlier mod, but '{}' is \
                             defined in the same mod ({})",
                            def.id,
                            prev.file.display()
                        ),
                    ));
                }
            }
            Some((_, prev)) => errors.push(duplicate_id(&def.id, &def.file, &prev.file)),
            None if def.is_override => errors.push(ContentError::at(
                &def.file,
                format!("{}/override", def.id),
                format!(
                    "override: true, but no earlier mod defines '{}' to replace",
                    def.id
                ),
            )),
            None => {
                check_namespace(mod_id, &def.id, &def.file, errors);
                by_id.insert(def.id.clone(), (mod_index, def));
            }
        }
    }
    (by_id.into_values().collect(), replaced)
}

/// Deserialize every component of `docs`, reporting the errors and dropping the values.
fn typecheck_docs(
    reg: &DefRegistry,
    docs: &[(usize, ObjectDoc<'_>)],
    errors: &mut Vec<ContentError>,
) {
    for (_, doc) in docs {
        for (name, tree) in &doc.components {
            if let Err(err) = deserialize_component(reg, &doc.file, &doc.id, name, tree) {
                errors.push(err);
            }
        }
    }
}

/// Inherit per component on the documents' trees, then deserialize each component once,
/// from the document that wrote it; descendants share the typed value. Returns the concrete
/// objects sorted by id. An object whose chain failed, or with a component that did not
/// deserialize, is left out; its error is reported once.
fn resolve_docs(
    reg: &DefRegistry,
    docs: &[(usize, ObjectDoc<'_>)],
    errors: &mut Vec<ContentError>,
) -> Vec<ResolvedObject> {
    // Each component is represented by the id of the document that wrote it.
    let heads = docs
        .iter()
        .map(|(_, doc)| Head {
            kind: doc.kind,
            id: doc.id.clone(),
            inherits: doc.inherits.clone(),
            is_abstract: doc.is_abstract,
            name: doc.name.clone(),
            tags: doc.tags.clone(),
            components: doc
                .components
                .keys()
                .map(|name| (name.clone(), doc.id.clone()))
                .collect(),
            remove: doc.remove.clone(),
            file: doc.file.clone(),
        })
        .collect();
    let (resolved, _, resolve_errors) = resolve_partial(heads);
    errors.extend(resolve_errors);

    let mut typed: BTreeMap<(&str, &str), Arc<dyn DefComponent>> = BTreeMap::new();
    for (_, doc) in docs {
        for (name, tree) in &doc.components {
            match deserialize_component(reg, &doc.file, &doc.id, name, tree) {
                Ok(component) => {
                    typed.insert((doc.id.as_str(), name.as_str()), component);
                }
                Err(err) => errors.push(err),
            }
        }
    }
    resolved
        .into_iter()
        .filter_map(|head| {
            let components = head
                .components
                .iter()
                .map(|(name, owner)| {
                    let component = typed.get(&(owner.as_str(), name.as_str()))?;
                    Some((name.clone(), Arc::clone(component)))
                })
                .collect::<Option<BTreeMap<_, _>>>()?;
            Some(ResolvedObject {
                kind: head.kind,
                id: head.id,
                name: head.name,
                tags: head.tags,
                components,
                file: head.file,
            })
        })
        .collect()
}

/// A content-kind item read from its tree, awaiting its id checks.
struct KindItem<'t> {
    mod_index: usize,
    file: &'t Path,
    item: Box<dyn ErasedKind>,
    tree: &'t Node,
}

/// Deserialize every content-kind file through its registry entry and load its sidecars,
/// reject duplicate and wrongly namespaced ids, record the origins of the items kept, then
/// build each registered kind's table (empty when no file defines one). Also returns each
/// table's files, in table order.
fn build_kind_tables(
    reg: &DefRegistry,
    mods: &[ModManifest],
    parsed: &[ParsedFile],
    origins: &mut BTreeMap<String, Origin>,
    errors: &mut Vec<ContentError>,
) -> (
    BTreeMap<&'static str, Box<dyn crate::kinds::ErasedKindTable>>,
    KindFiles,
) {
    let mut tables = BTreeMap::new();
    let mut files = KindFiles::new();
    for entry in reg.kind_entries() {
        let mut by_id: BTreeMap<String, KindItem<'_>> = BTreeMap::new();
        for p in parsed.iter().filter(|p| p.ext == entry.ext) {
            let mut item = match (entry.parse)(&p.tree, &p.file) {
                Ok(item) => item,
                Err(err) => {
                    errors.push(err);
                    continue;
                }
            };
            if let Err((field, msg)) = item.load_sidecars(&p.file) {
                let path = if field.is_empty() {
                    item.id().to_owned()
                } else {
                    format!("{}/{field}", item.id())
                };
                errors.push(ContentError::at(&p.file, path, msg));
            }
            let id = item.id().to_owned();
            if let Some(prev) = by_id.get(&id) {
                errors.push(duplicate_id(&id, &p.file, prev.file));
                continue;
            }
            if entry.namespaced {
                check_namespace(&mods[p.mod_index].id, &id, &p.file, errors);
            }
            by_id.insert(
                id,
                KindItem {
                    mod_index: p.mod_index,
                    file: &p.file,
                    item,
                    tree: &p.tree,
                },
            );
        }
        if let Some(max) = entry.max_items
            && let Some(first_over) = by_id.values().nth(max)
        {
            errors.push(ContentError::at(
                first_over.file,
                first_over.item.id(),
                format!(
                    "{} {} kinds exceed the limit of {max}",
                    by_id.len(),
                    entry.ext
                ),
            ));
        }
        let mut paths = Vec::with_capacity(by_id.len());
        let mut items = Vec::with_capacity(by_id.len());
        for (id, kept) in by_id {
            let origin = origin(mods, kept.mod_index, kept.file);
            let key = format!("{}:{id}", entry.ext);
            record_origins(origins, &key, &mut Vec::new(), kept.tree, &origin);
            paths.push(kept.file.to_path_buf());
            items.push(kept.item);
        }
        tables.insert(entry.ext, (entry.build_table)(items));
        files.insert(entry.ext, paths);
    }
    (tables, files)
}

fn duplicate_id(id: &str, file: &Path, first: &Path) -> ContentError {
    ContentError::at(
        file,
        id,
        format!("duplicate id '{id}' (first defined in {})", first.display()),
    )
}

fn check_namespace(mod_id: &str, id: &str, file: &Path, errors: &mut Vec<ContentError>) {
    let prefix = format!("{mod_id}.");
    if !id.starts_with(&prefix) || id.len() == prefix.len() {
        errors.push(ContentError::at(
            file,
            id,
            format!("id '{id}' must start with its mod's prefix '{prefix}'"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, depends: &[&str], load_after: &[&str]) -> ModManifest {
        let owned = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect();
        ModManifest {
            id: id.to_owned(),
            version: "1.0.0".to_owned(),
            depends: owned(depends),
            load_after: owned(load_after),
            dir: PathBuf::from(id),
        }
    }

    fn ids(mods: &[ModManifest]) -> Vec<&str> {
        mods.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn load_after_alone_orders_mods() {
        let mut errors = Vec::new();
        let mods = vec![
            manifest("a", &[], &["z"]),
            manifest("m", &[], &[]),
            manifest("z", &[], &["not_installed"]),
        ];
        let ordered = order_mods(mods, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        // Without `load_after`, id order would put "a" first.
        assert_eq!(ids(&ordered), ["m", "z", "a"]);
    }

    #[test]
    fn dependency_cycle_is_error() {
        let mut errors = Vec::new();
        let mods = vec![
            manifest("a", &["b"], &[]),
            manifest("b", &[], &["a"]),
            manifest("c", &[], &[]),
        ];
        let ordered = order_mods(mods, &mut errors);
        // Every mod is still listed: the free one first, then the cycle in id order.
        assert_eq!(ids(&ordered), ["c", "a", "b"]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        for (err, id) in errors.iter().zip(["a", "b"]) {
            assert_eq!(err.file, Path::new(id).join("mod.ron"));
            assert!(
                err.message.contains("mod dependency cycle among: a, b"),
                "{}",
                err.message
            );
        }
    }
}
