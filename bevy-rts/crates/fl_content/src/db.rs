//! The loaded content database and the link pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use fl_protocol::{StringTables, UnitTypeId, tables};

use crate::builtin::{ScenarioDef, VisualDef};
use crate::error::{ContentError, ContentErrors, read_text};
use crate::inherit::ResolvedObject;
use crate::kinds::{ContentKind, ErasedKindTable, ErasedRules, KindTable, RulesSection};
use crate::link::LinkCtx;
use crate::mods::{CreditEntry, ModManifest};
use crate::registry::DefRegistry;
use crate::tree::{Node, PathSeg, for_each_leaf, from_tree, parse_ron, path_key};

/// The source file of every content-kind item, by extension, in table (id) order.
pub(crate) type KindFiles = BTreeMap<&'static str, Vec<PathBuf>>;

/// The `rules.ron` that defined each rules section, by section name.
pub(crate) type RulesFiles = BTreeMap<&'static str, PathBuf>;

/// Where a content value was last set: the mod and the file (ADR-0020).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub mod_id: String,
    pub file: PathBuf,
}

/// Record `origin` for every leaf of `tree`, document `doc`, under the path `base`.
pub(crate) fn record_origins(
    origins: &mut BTreeMap<String, Origin>,
    doc: &str,
    base: &mut Vec<PathSeg>,
    tree: &Node,
    origin: &Origin,
) {
    for_each_leaf(tree, base, &mut |path| {
        origins.insert(path_key(doc, path), origin.clone());
    });
}

/// All loaded, resolved and linked content. Built by [`load_dirs`](crate::load_dirs).
pub struct ContentDb {
    pub(crate) registry: DefRegistry,
    /// In load order.
    pub(crate) mods: Vec<ModManifest>,
    /// Concrete units sorted by id; the index is the `UnitTypeId`.
    pub(crate) units: Vec<ResolvedObject>,
    pub(crate) abstract_units: BTreeSet<String>,
    /// Concrete units whose `inherits` chain failed. Only non-empty while a failed load runs
    /// its link pass, so references to them are not reported a second time as missing.
    pub(crate) unresolved_units: BTreeSet<String>,
    /// One table per registered kind, by extension.
    pub(crate) kinds: BTreeMap<&'static str, Box<dyn ErasedKindTable>>,
    /// One value per registered section, by name.
    pub(crate) rules: BTreeMap<&'static str, Box<dyn ErasedRules>>,
    pub(crate) credits: Vec<CreditEntry>,
    pub(crate) warnings: Vec<String>,
    pub(crate) rules_hash: u64,
    /// The origin of every leaf of every document, by path key.
    pub(crate) origins: BTreeMap<String, Origin>,
}

impl ContentDb {
    /// Concrete units sorted by id; index = `UnitTypeId`.
    pub fn units(&self) -> &[ResolvedObject] {
        &self.units
    }

    /// The unit with this interned id.
    pub fn unit(&self, id: UnitTypeId) -> Option<&ResolvedObject> {
        self.units.get(usize::from(id.0))
    }

    /// The interned id of the concrete unit `id`.
    pub fn unit_id(&self, id: &str) -> Option<UnitTypeId> {
        let index = self
            .units
            .binary_search_by(|u| u.id.as_str().cmp(id))
            .ok()?;
        Some(UnitTypeId(
            u16::try_from(index).expect("loading caps units at u16::MAX"),
        ))
    }

    /// Whether `id` is an abstract unit template.
    pub(crate) fn unit_is_abstract(&self, id: &str) -> bool {
        self.abstract_units.contains(id)
    }

    /// Whether `id` is a concrete unit whose `inherits` chain failed to resolve.
    pub(crate) fn unit_is_unresolved(&self, id: &str) -> bool {
        self.unresolved_units.contains(id)
    }

    /// The content string tables: [`tables::UNITS`] in [`UnitTypeId`] order, and
    /// `"content.<ext>"` for every registered kind in [`KindTable`] (id) order.
    pub fn string_tables(&self) -> StringTables {
        let mut out = StringTables::default();
        out.insert(
            tables::UNITS,
            self.units.iter().map(|unit| unit.id.clone()).collect(),
        );
        for (ext, table) in &self.kinds {
            out.insert(
                &format!("content.{ext}"),
                table
                    .erased_items()
                    .map(|item| item.id().to_owned())
                    .collect(),
            );
        }
        out
    }

    /// Every content and registry string table: [`string_tables`](Self::string_tables) and
    /// the [`DefRegistry::string_tables`] of the registry it was loaded with.
    fn id_tables(&self) -> StringTables {
        let mut tables = self.string_tables();
        tables.tables.extend(self.registry.string_tables().tables);
        tables
    }

    /// Whether every content and registry string table of `other` holds the same names as
    /// this content's: then every dense id means the same in both, and `other` can replace
    /// this content in a running match (a value-only hot reload, ADR-0012).
    pub fn same_id_sets(&self, other: &ContentDb) -> bool {
        self.id_tables() == other.id_tables()
    }

    /// How `other`'s ID sets differ from this content's, one line per name, sorted:
    /// `"<table>: +<name>"` for a name only `other` has, `"<table>: -<name>"` for one only
    /// this content has (`"content.unit: +base.de.pz4"`, `"content.terrain: -base.sand"`).
    /// Empty exactly when [`same_id_sets`](Self::same_id_sets).
    pub fn id_set_diff(&self, other: &ContentDb) -> Vec<String> {
        let (ours, theirs) = (self.id_tables(), other.id_tables());
        let table_names: BTreeSet<&String> =
            ours.tables.keys().chain(theirs.tables.keys()).collect();
        let mut lines = Vec::new();
        for table in table_names {
            let names = |tables: &StringTables| -> BTreeSet<String> {
                tables
                    .get(table)
                    .unwrap_or_default()
                    .iter()
                    .cloned()
                    .collect()
            };
            let (before, after) = (names(&ours), names(&theirs));
            lines.extend(
                after
                    .difference(&before)
                    .map(|name| format!("{table}: +{name}")),
            );
            lines.extend(
                before
                    .difference(&after)
                    .map(|name| format!("{table}: -{name}")),
            );
        }
        lines.sort_unstable();
        lines
    }

    /// All loaded items of kind `K`.
    ///
    /// # Panics
    /// If `K` is not registered.
    pub fn kind<K: ContentKind>(&self) -> &KindTable<K> {
        self.kind_table::<K>().unwrap_or_else(|| {
            panic!(
                "content kind {} (*.{}.ron) is not registered",
                std::any::type_name::<K>(),
                K::EXT
            )
        })
    }

    pub(crate) fn kind_table<K: ContentKind>(&self) -> Option<&KindTable<K>> {
        self.kinds.get(K::EXT)?.as_any().downcast_ref()
    }

    /// The loaded rules section `S`.
    ///
    /// # Panics
    /// If `S` is not registered.
    pub fn rules<S: RulesSection>(&self) -> &S {
        let type_name = std::any::type_name::<S>();
        let name = self
            .registry
            .rules_section_name::<S>()
            .unwrap_or_else(|| panic!("rules section {type_name} is not registered"));
        self.rules[name]
            .as_any()
            .downcast_ref()
            .unwrap_or_else(|| panic!("rules section '{name}' is not a {type_name}"))
    }

    /// The registry this content was loaded with.
    pub fn registry(&self) -> &DefRegistry {
        &self.registry
    }

    /// Mods in load order.
    pub fn mods(&self) -> &[ModManifest] {
        &self.mods
    }

    /// Every mod's credits, in load order.
    pub fn credits(&self) -> &[CreditEntry] {
        &self.credits
    }

    /// Problems that do not stop loading, such as a unit without a visual.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// xxh64 over the canonical form of all sim-relevant content: mods, units, rules-hashed
    /// kinds and rules sections.
    pub fn rules_hash(&self) -> u64 {
        self.rules_hash
    }

    /// The mod and file that set every leaf of every document, by path key
    /// ([`path_key`](crate::path_key)): objects (abstract ones too), content-kind items and
    /// rules (`"unit:base.de.pz3/components.Hull.hp"`, `"terrain:base.forest/pass"`,
    /// `"rules/Movement.min_speed_frac"`). A leaf is a value without children: a number,
    /// string, identifier, boolean, `()`, `None` or an empty list, map or struct.
    ///
    /// A leaf an object only inherits has no key of its own: its origin is under the
    /// ancestor's key that wrote it. Origins are not content: they never enter the rules hash.
    pub fn origins(&self) -> &BTreeMap<String, Origin> {
        &self.origins
    }

    /// Parse a standalone `*.scenario.ron` (not part of any mod) and run its link checks
    /// against this content.
    pub fn load_scenario_file(&self, path: &Path) -> Result<ScenarioDef, ContentErrors> {
        let text = read_text(path)?;
        let scenario: ScenarioDef = from_tree(&parse_ron(path, &text)?, path)?;
        let mut errors = Vec::new();
        let mut ctx = LinkCtx::new(self, path, scenario.id.clone(), &mut errors);
        ContentKind::link(&scenario, &mut ctx);
        if errors.is_empty() {
            Ok(scenario)
        } else {
            Err(ContentErrors(errors))
        }
    }

    /// The link pass: every component's `validate` (on units and `other_objects`, the
    /// concrete objects of other kinds), every content-kind item's `link` and every rules
    /// section's `link`.
    pub(crate) fn link(
        &self,
        other_objects: &[ResolvedObject],
        kind_files: &KindFiles,
        rules_files: &RulesFiles,
        errors: &mut Vec<ContentError>,
    ) {
        for object in self.units.iter().chain(other_objects) {
            for (name, component) in &object.components {
                let scope = format!("{}/components.{name}", object.id);
                component.validate(&mut LinkCtx::new(self, &object.file, scope, errors));
            }
        }
        for (ext, table) in &self.kinds {
            for (item, file) in table.erased_items().zip(&kind_files[ext]) {
                item.link(&mut LinkCtx::new(self, file, item.id(), errors));
            }
        }
        for (name, section) in &self.rules {
            section.link(&mut LinkCtx::new(self, &rules_files[name], *name, errors));
        }
    }

    /// One warning per concrete unit without a [`VisualDef`] (when visuals are registered).
    pub(crate) fn missing_visual_warnings(&self) -> Vec<String> {
        let Some(visuals) = self.kind_table::<VisualDef>() else {
            return Vec::new();
        };
        self.units
            .iter()
            .filter(|unit| visuals.get(&unit.id).is_none())
            .map(|unit| {
                format!(
                    "{}: unit '{}' has no VisualDef (*.visual.ron)",
                    unit.file.display(),
                    unit.id
                )
            })
            .collect()
    }

    /// An empty database, for unit tests.
    #[cfg(test)]
    pub(crate) fn empty(registry: DefRegistry) -> Self {
        ContentDb {
            registry,
            mods: Vec::new(),
            units: Vec::new(),
            abstract_units: BTreeSet::new(),
            unresolved_units: BTreeSet::new(),
            kinds: BTreeMap::new(),
            rules: BTreeMap::new(),
            credits: Vec::new(),
            warnings: Vec::new(),
            rules_hash: 0,
            origins: BTreeMap::new(),
        }
    }
}
