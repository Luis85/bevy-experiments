//! Inheritance: resolving `inherits` chains into flat objects, per component.
//!
//! The resolver is generic over the component representation: [`resolve_objects`] works on
//! typed [`ObjectDef`]s, the loader on documents whose components are still trees, so
//! inheritance runs before typed deserialization (ADR-0020).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::def::DefComponent;
use crate::error::{ContentError, ContentErrors};
use crate::object::ObjectDef;
use crate::registry::DefRegistry;

/// An object with its inheritance chain applied.
#[derive(Debug, Clone)]
pub struct ResolvedObject {
    pub kind: &'static str,
    pub id: String,
    /// The nearest `name` along the chain; falls back to the id.
    pub name: String,
    /// Sorted, deduplicated union of the tags along the chain.
    pub tags: Vec<String>,
    pub components: BTreeMap<String, Arc<dyn DefComponent>>,
    pub file: PathBuf,
}

impl ResolvedObject {
    /// The component registered as `T`, if this object has it.
    pub fn get<T: DefComponent>(&self, reg: &DefRegistry) -> Option<&T> {
        let name = reg.component_name::<T>()?;
        self.components.get(name)?.as_any().downcast_ref::<T>()
    }

    /// Whether this object has the component registered under `name`.
    pub fn has(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }
}

/// The result of inheritance resolution.
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    /// Concrete (non-abstract) objects, sorted by id.
    pub concrete: Vec<ResolvedObject>,
    /// Ids of the abstract templates; they are never spawned.
    pub abstract_ids: BTreeSet<String>,
}

/// Resolve every object's `inherits` chain.
///
/// A child's component replaces the parent's whole; `remove` drops inherited components.
/// Errors (duplicate ids, missing parents, cycles, kind mismatches, removing a component
/// that is not inherited) are all collected before returning.
pub fn resolve_objects(raw: Vec<ObjectDef>) -> Result<Resolved, ContentErrors> {
    let heads = raw.into_iter().map(Head::from).collect();
    let (concrete, abstract_ids, errors) = resolve_partial(heads);
    if !errors.is_empty() {
        return Err(ContentErrors(errors));
    }
    let concrete = concrete
        .into_iter()
        .map(|head| ResolvedObject {
            kind: head.kind,
            id: head.id,
            name: head.name,
            tags: head.tags,
            components: head.components,
            file: head.file,
        })
        .collect();
    Ok(Resolved {
        concrete,
        abstract_ids,
    })
}

/// One object's inheritance-relevant fields, over any component representation: typed
/// ([`ObjectDef`]) or the id of the document that wrote each component (the loader).
#[derive(Debug, Clone)]
pub(crate) struct Head<C> {
    pub(crate) kind: &'static str,
    pub(crate) id: String,
    pub(crate) inherits: Option<String>,
    pub(crate) is_abstract: bool,
    pub(crate) name: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) components: BTreeMap<String, C>,
    pub(crate) remove: Vec<String>,
    pub(crate) file: PathBuf,
}

impl From<ObjectDef> for Head<Arc<dyn DefComponent>> {
    fn from(def: ObjectDef) -> Self {
        Head {
            kind: def.kind,
            id: def.id,
            inherits: def.inherits,
            is_abstract: def.is_abstract,
            name: def.name,
            tags: def.tags,
            components: def.components,
            remove: def.remove,
            file: def.file,
        }
    }
}

/// A concrete object with its chain applied, over any component representation.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedHead<C> {
    pub(crate) kind: &'static str,
    pub(crate) id: String,
    /// The nearest `name` along the chain; falls back to the id.
    pub(crate) name: String,
    /// Sorted, deduplicated union of the tags along the chain.
    pub(crate) tags: Vec<String>,
    pub(crate) components: BTreeMap<String, C>,
    pub(crate) file: PathBuf,
}

/// Resolve every object's chain, collecting every error. Returns the concrete objects that
/// did resolve, sorted by id (an object whose chain failed is missing), the abstract ids, and
/// the errors.
pub(crate) fn resolve_partial<C: Clone>(
    raw: Vec<Head<C>>,
) -> (Vec<ResolvedHead<C>>, BTreeSet<String>, Vec<ContentError>) {
    let mut errors = Vec::new();
    let mut defs: BTreeMap<String, Head<C>> = BTreeMap::new();
    for def in raw {
        if let Some(first) = defs.get(&def.id) {
            errors.push(ContentError {
                file: def.file.clone(),
                line_col: None,
                path: def.id.clone(),
                message: format!(
                    "duplicate id '{}' (first defined in {})",
                    def.id,
                    first.file.display()
                ),
            });
        } else {
            defs.insert(def.id.clone(), def);
        }
    }

    // `None` marks an object whose chain failed; the root cause is reported once.
    let mut memo: BTreeMap<&str, Option<Inherited<C>>> = BTreeMap::new();
    for id in defs.keys() {
        resolve_chain(id, &defs, &mut memo, &mut errors);
    }

    let mut concrete = Vec::new();
    let mut abstract_ids = BTreeSet::new();
    for (id, def) in &defs {
        if def.is_abstract {
            abstract_ids.insert(id.clone());
            continue;
        }
        if let Some(Some(inherited)) = memo.remove(id.as_str()) {
            concrete.push(ResolvedHead {
                kind: def.kind,
                id: id.clone(),
                name: inherited.name.unwrap_or_else(|| id.clone()),
                tags: inherited.tags.into_iter().collect(),
                components: inherited.components,
                file: def.file.clone(),
            });
        }
    }

    (concrete, abstract_ids, errors)
}

/// What a descendant inherits from an object.
#[derive(Clone)]
struct Inherited<C> {
    name: Option<String>,
    tags: BTreeSet<String>,
    components: BTreeMap<String, C>,
}

impl<C> Default for Inherited<C> {
    fn default() -> Self {
        Inherited {
            name: None,
            tags: BTreeSet::new(),
            components: BTreeMap::new(),
        }
    }
}

fn chain_error<C>(def: &Head<C>, field: &str, message: String) -> ContentError {
    ContentError {
        file: def.file.clone(),
        line_col: None,
        path: format!("{}/{field}", def.id),
        message,
    }
}

/// Resolve `id` and every unresolved ancestor, filling `memo`.
fn resolve_chain<'a, C: Clone>(
    id: &'a str,
    defs: &'a BTreeMap<String, Head<C>>,
    memo: &mut BTreeMap<&'a str, Option<Inherited<C>>>,
    errors: &mut Vec<ContentError>,
) {
    // Walk up until a resolved ancestor, the root, or a failure. `chain` is leaf-first.
    let mut chain: Vec<&'a str> = Vec::new();
    let mut cur = id;
    let base: Option<Inherited<C>> = loop {
        if let Some(done) = memo.get(cur) {
            match done {
                Some(inherited) => break Some(inherited.clone()),
                None => break None,
            }
        }
        if let Some(pos) = chain.iter().position(|&c| c == cur) {
            let mut cycle: Vec<&str> = chain[pos..].to_vec();
            cycle.push(cur);
            errors.push(chain_error(
                &defs[cur],
                "inherits",
                format!("inheritance cycle: {}", cycle.join(" -> ")),
            ));
            break None;
        }
        chain.push(cur);
        let def = &defs[cur];
        let Some(parent_id) = def.inherits.as_deref() else {
            break Some(Inherited::default());
        };
        let Some(parent) = defs.get(parent_id) else {
            errors.push(chain_error(
                def,
                "inherits",
                format!("parent '{parent_id}' does not exist"),
            ));
            break None;
        };
        if parent.kind != def.kind {
            errors.push(chain_error(
                def,
                "inherits",
                format!(
                    "parent '{parent_id}' is a {} but '{}' is a {}",
                    parent.kind, def.id, def.kind
                ),
            ));
            break None;
        }
        cur = parent.id.as_str();
    };

    let Some(mut acc) = base else {
        for &c in &chain {
            memo.insert(c, None);
        }
        return;
    };
    // Apply root-most first.
    for &c in chain.iter().rev() {
        let def = &defs[c];
        for name in &def.remove {
            if acc.components.remove(name).is_none() {
                errors.push(chain_error(
                    def,
                    "remove",
                    format!("cannot remove component '{name}': it is not inherited"),
                ));
            }
        }
        for (name, component) in &def.components {
            acc.components.insert(name.clone(), component.clone());
        }
        acc.tags.extend(def.tags.iter().cloned());
        if def.name.is_some() {
            acc.name.clone_from(&def.name);
        }
        memo.insert(c, Some(acc.clone()));
    }
}
