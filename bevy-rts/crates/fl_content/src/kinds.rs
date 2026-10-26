//! Content kinds (fixed-schema files such as `*.faction.ron`) and `rules.ron` sections.
//!
//! Both are plain serde types. The loader handles them through small type-erased traits so
//! the [`DefRegistry`](crate::DefRegistry) can hold any number of them.

use std::any::Any;
use std::fmt::Debug;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ContentError;
use crate::link::LinkCtx;
use crate::tree::{ContentTree, Node, from_tree_at};

/// A fixed-schema content type loaded from `*.<EXT>.ron` files, one item per file.
pub trait ContentKind: DeserializeOwned + Serialize + Debug + Send + Sync + 'static {
    /// File suffix: `"faction"` loads `*.faction.ron`.
    const EXT: &'static str;
    /// Whether items of this kind are part of the rules hash (sim-relevant data).
    const IN_RULES_HASH: bool;
    /// Whether ids must start with `"<mod_id>."`.
    const NAMESPACED: bool = true;
    /// The field that holds the item's id in its file (`id`; a [`VisualDef`](crate::VisualDef)
    /// is keyed by its `unit`). Errors name the item by it before it deserializes.
    const ID_FIELD: &'static str = "id";
    /// The item's unique id within its kind.
    fn id(&self) -> &str;
    /// Load files the item names next to `_file`, its own file (a map's PNG layers), into
    /// fields RON cannot set. The loader calls it right after parsing, before the link pass.
    /// An `Err((field, message))` becomes a content error on `_file` at `<id>/<field>`
    /// (at `<id>` when `field` is empty), like an error from [`link`](Self::link).
    fn load_sidecars(&mut self, _file: &Path) -> Result<(), (String, String)> {
        Ok(())
    }
    /// Link-pass checks: cross-references and value ranges, reported through `ctx`.
    fn link(&self, _ctx: &mut LinkCtx<'_>) {}
}

/// A section of `rules.ron` (`Rules({ "Name": (…) })`): global tuning constants.
pub trait RulesSection:
    DeserializeOwned + Serialize + Debug + Clone + Send + Sync + 'static
{
    /// Link-pass checks: value ranges, reported through `ctx` at the field
    /// (`"<Section>/<field>"`), like [`ContentKind::link`].
    fn link(&self, _ctx: &mut LinkCtx<'_>) {}
}

/// All loaded items of one content kind, sorted by id.
#[derive(Debug)]
pub struct KindTable<K> {
    items: Vec<K>,
}

impl<K: ContentKind> KindTable<K> {
    /// `items` must be sorted by id with no duplicates.
    pub(crate) fn from_sorted(items: Vec<K>) -> Self {
        debug_assert!(items.windows(2).all(|w| w[0].id() < w[1].id()));
        KindTable { items }
    }

    /// The item with this id.
    pub fn get(&self, id: &str) -> Option<&K> {
        self.index_of(id).map(|i| &self.items[i as usize])
    }

    /// The item's position in id order: a dense id, the same on every machine.
    pub fn index_of(&self, id: &str) -> Option<u32> {
        let index = self.items.binary_search_by(|k| k.id().cmp(id)).ok()?;
        Some(u32::try_from(index).expect("kind tables hold fewer than 2^32 items"))
    }

    /// Items in id order.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// One parsed content-kind item, type-erased.
pub(crate) trait ErasedKind: erased_serde::Serialize + Send + Sync {
    fn id(&self) -> &str;
    fn load_sidecars(&mut self, file: &Path) -> Result<(), (String, String)>;
    fn link(&self, ctx: &mut LinkCtx<'_>);
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

erased_serde::serialize_trait_object!(ErasedKind);

impl<K: ContentKind> ErasedKind for K {
    fn id(&self) -> &str {
        ContentKind::id(self)
    }
    fn load_sidecars(&mut self, file: &Path) -> Result<(), (String, String)> {
        ContentKind::load_sidecars(self, file)
    }
    fn link(&self, ctx: &mut LinkCtx<'_>) {
        ContentKind::link(self, ctx);
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// A [`KindTable`], type-erased.
pub(crate) trait ErasedKindTable: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn erased_items(&self) -> Box<dyn Iterator<Item = &dyn ErasedKind> + '_>;
}

impl<K: ContentKind> ErasedKindTable for KindTable<K> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn erased_items(&self) -> Box<dyn Iterator<Item = &dyn ErasedKind> + '_> {
        Box::new(self.items.iter().map(|k| k as &dyn ErasedKind))
    }
}

/// Read one `*.<EXT>.ron` file's tree as `K`. An error's path starts with `<id>/` when the
/// tree's [`ContentKind::ID_FIELD`] is a string.
pub(crate) fn parse_kind<K: ContentKind>(
    tree: &Node,
    file: &Path,
) -> Result<Box<dyn ErasedKind>, ContentError> {
    let prefix = match &tree.value {
        ContentTree::Struct { fields, .. } => fields
            .iter()
            .find_map(|(name, value)| match &value.value {
                ContentTree::Str(id) if name == K::ID_FIELD => Some(format!("{id}/")),
                _ => None,
            })
            .unwrap_or_default(),
        _ => String::new(),
    };
    let item: K = from_tree_at(tree, file, &prefix, String::new())?;
    Ok(Box::new(item))
}

/// Build a `KindTable<K>` from items sorted by id, each of which must be a `K`.
pub(crate) fn build_table<K: ContentKind>(
    items: Vec<Box<dyn ErasedKind>>,
) -> Box<dyn ErasedKindTable> {
    let items = items
        .into_iter()
        .map(|item| {
            *item
                .into_any()
                .downcast::<K>()
                .expect("kind items are parsed by their own kind")
        })
        .collect();
    Box::new(KindTable::<K>::from_sorted(items))
}

/// One parsed rules section, type-erased.
pub(crate) trait ErasedRules: erased_serde::Serialize + Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn link(&self, ctx: &mut LinkCtx<'_>);
}

erased_serde::serialize_trait_object!(ErasedRules);

impl<S: RulesSection> ErasedRules for S {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn link(&self, ctx: &mut LinkCtx<'_>) {
        RulesSection::link(self, ctx);
    }
}
