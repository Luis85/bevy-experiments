//! Object files (`*.unit.ron`, …): an [`ObjectDoc`] is one object's document read from its
//! content tree, with each component still a tree; [`parse_object`] also deserializes the
//! components into an [`ObjectDef`].
//!
//! Each component is deserialized through its [`DefRegistry`] entry, type-erased, over a
//! [`TreeDeserializer`](crate::TreeDeserializer).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::def::DefComponent;
use crate::error::ContentError;
use crate::registry::DefRegistry;
use crate::tree::{ContentTree, Node, erased_from_tree, from_tree_at, parse_ron};

/// One object definition as written in its file, before inheritance.
#[derive(Debug, Clone)]
pub struct ObjectDef {
    pub kind: &'static str,
    pub id: String,
    pub inherits: Option<String>,
    pub is_abstract: bool,
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub is_override: bool,
    pub components: BTreeMap<String, Arc<dyn DefComponent>>,
    pub remove: Vec<String>,
    pub file: PathBuf,
}

const FIELDS: &[&str] = &[
    "id",
    "inherits",
    "abstract",
    "name",
    "tags",
    "override",
    "components",
    "remove",
];

/// Parse one object of `kind` from `text`, the contents of `file`.
///
/// Every field except `id` is optional. An unregistered `kind`, unknown fields, unknown
/// component names (in `components` or `remove`) and unknown fields inside components are
/// errors carrying the file, line/column and field path.
pub fn parse_object(
    reg: &DefRegistry,
    kind: &'static str,
    file: &Path,
    text: &str,
) -> Result<ObjectDef, ContentError> {
    if reg.object_kind(kind).is_none() {
        return Err(ContentError::at(
            file,
            "",
            format!("unknown object kind '{kind}'"),
        ));
    }
    let tree = parse_ron(file, text)?;
    let doc = object_doc(reg, kind, file, &tree)?;
    let mut components = BTreeMap::new();
    for (name, node) in &doc.components {
        let component = deserialize_component(reg, file, &doc.id, name, node)?;
        components.insert(name.clone(), component);
    }
    Ok(ObjectDef {
        kind,
        id: doc.id,
        inherits: doc.inherits,
        is_abstract: doc.is_abstract,
        name: doc.name,
        tags: doc.tags,
        is_override: doc.is_override,
        components,
        remove: doc.remove,
        file: file.to_path_buf(),
    })
}

/// One object's document: its fields as written, each component still a tree.
pub(crate) struct ObjectDoc<'t> {
    pub(crate) kind: &'static str,
    pub(crate) id: String,
    pub(crate) inherits: Option<String>,
    pub(crate) is_abstract: bool,
    pub(crate) name: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) is_override: bool,
    /// Registered component names with their trees.
    pub(crate) components: BTreeMap<String, &'t Node>,
    /// Registered component names.
    pub(crate) remove: Vec<String>,
    pub(crate) file: PathBuf,
    /// The whole document.
    pub(crate) tree: &'t Node,
}

/// Read the object document `tree` of `kind` from `file`.
///
/// The root is a struct, named anything (`Unit(…)`) or not. Unknown and duplicate fields,
/// unregistered or duplicate component names and fields of the wrong type are errors at their
/// field path (`"<id>/<field>"`) and position. Component values are not deserialized here.
pub(crate) fn object_doc<'t>(
    reg: &DefRegistry,
    kind: &'static str,
    file: &Path,
    tree: &'t Node,
) -> Result<ObjectDoc<'t>, ContentError> {
    let ContentTree::Struct { fields, .. } = &tree.value else {
        return Err(ContentError::at_node(
            file,
            "",
            tree,
            "expected an object such as `Unit(id: \"mod.id\", components: { … })`",
        ));
    };
    // The id first, so every other error names it.
    let Some((_, id_node)) = fields.iter().find(|(field, _)| field == "id") else {
        return Err(ContentError::at_node(file, "", tree, "missing field `id`"));
    };
    let id: String = from_tree_at(id_node, file, "", "id".to_owned())?;
    let prefix = format!("{id}/");
    let mut doc = ObjectDoc {
        kind,
        id,
        inherits: None,
        is_abstract: false,
        name: None,
        tags: Vec::new(),
        is_override: false,
        components: BTreeMap::new(),
        remove: Vec::new(),
        file: file.to_path_buf(),
        tree,
    };
    let mut seen = BTreeSet::new();
    for (field, value) in fields {
        let path = format!("{prefix}{field}");
        if !seen.insert(field.as_str()) {
            return Err(ContentError::at_node(
                file,
                path,
                value,
                format!("duplicate field `{field}`"),
            ));
        }
        match field.as_str() {
            "id" => {}
            "inherits" => doc.inherits = from_tree_at(value, file, &prefix, field.clone())?,
            "abstract" => doc.is_abstract = from_tree_at(value, file, &prefix, field.clone())?,
            "name" => doc.name = Some(from_tree_at(value, file, &prefix, field.clone())?),
            "tags" => doc.tags = from_tree_at(value, file, &prefix, field.clone())?,
            "override" => doc.is_override = from_tree_at(value, file, &prefix, field.clone())?,
            "components" => doc.components = component_trees(reg, file, &path, value)?,
            "remove" => doc.remove = remove_list(reg, file, &path, value)?,
            other => {
                let expected: Vec<String> = FIELDS.iter().map(|f| format!("`{f}`")).collect();
                return Err(ContentError::at_node(
                    file,
                    path,
                    value,
                    format!(
                        "unknown field `{other}`, expected one of {}",
                        expected.join(", ")
                    ),
                ));
            }
        }
    }
    Ok(doc)
}

fn unknown_component(reg: &DefRegistry, name: &str) -> String {
    let known: Vec<&str> = reg.component_names().collect();
    format!(
        "unknown component '{name}' (registered: {})",
        known.join(", ")
    )
}

/// The `components: { "Name": (…), … }` map, at field path `path`.
fn component_trees<'t>(
    reg: &DefRegistry,
    file: &Path,
    path: &str,
    value: &'t Node,
) -> Result<BTreeMap<String, &'t Node>, ContentError> {
    let ContentTree::Map(entries) = &value.value else {
        return Err(ContentError::at_node(
            file,
            path,
            value,
            "expected a map of def component names to values: `{ \"Hull\": (…) }`",
        ));
    };
    let mut out = BTreeMap::new();
    for (key, component) in entries {
        let ContentTree::Str(name) = &key.value else {
            return Err(ContentError::at_node(
                file,
                path,
                key,
                "def component names are strings: `\"Hull\"`",
            ));
        };
        let at = format!("{path}.{name}");
        if reg.component(name).is_none() {
            return Err(ContentError::at_node(
                file,
                at,
                key,
                unknown_component(reg, name),
            ));
        }
        if out.insert(name.clone(), component).is_some() {
            return Err(ContentError::at_node(
                file,
                at,
                key,
                format!("duplicate component '{name}'"),
            ));
        }
    }
    Ok(out)
}

/// The `remove: ["Name", …]` list at field path `path`; each name must be registered.
fn remove_list(
    reg: &DefRegistry,
    file: &Path,
    path: &str,
    value: &Node,
) -> Result<Vec<String>, ContentError> {
    let ContentTree::Seq(items) = &value.value else {
        return Err(ContentError::at_node(
            file,
            path,
            value,
            "expected a list of def component names",
        ));
    };
    let mut out = Vec::new();
    for item in items {
        let ContentTree::Str(name) = &item.value else {
            return Err(ContentError::at_node(
                file,
                path,
                item,
                "def component names are strings: `\"Hull\"`",
            ));
        };
        if reg.component(name).is_none() {
            return Err(ContentError::at_node(
                file,
                path,
                item,
                unknown_component(reg, name),
            ));
        }
        out.push(name.clone());
    }
    Ok(out)
}

/// Deserialize component `name` of object `id` from its tree, through its registry entry.
/// Errors carry the path `"<id>/components.<name>.<field…>"` and the failing value's position.
///
/// # Panics
/// If `name` is not registered ([`object_doc`] checks every name).
pub(crate) fn deserialize_component(
    reg: &DefRegistry,
    file: &Path,
    id: &str,
    name: &str,
    tree: &Node,
) -> Result<Arc<dyn DefComponent>, ContentError> {
    let entry = reg
        .component(name)
        .expect("object documents name registered components only");
    erased_from_tree(
        tree,
        file,
        &format!("{id}/"),
        format!("components.{name}"),
        |de| (entry.deserialize)(de),
    )
}
