//! Reading `rules.ron`: `Rules({ "Section": (…), … })`, one registered type per section.

use std::path::Path;

use crate::error::ContentError;
use crate::kinds::ErasedRules;
use crate::registry::DefRegistry;
use crate::tree::{ContentTree, Node, erased_from_tree};

/// One section of a `rules.ron`: its name, typed value and tree.
pub(crate) struct ParsedSection<'t> {
    pub(crate) name: &'static str,
    pub(crate) value: Box<dyn ErasedRules>,
    pub(crate) tree: &'t Node,
}

/// The `{ … }` section map of a `rules.ron` tree: `Rules({ … })` or `({ … })`.
pub(crate) fn section_map<'t>(
    file: &Path,
    tree: &'t Node,
) -> Result<&'t [(Node, Node)], ContentError> {
    let inner = match &tree.value {
        ContentTree::TupleStruct { name, items }
            if items.len() == 1 && name.as_deref().is_none_or(|n| n == "Rules") =>
        {
            &items[0]
        }
        ContentTree::Tuple(items) if items.len() == 1 => &items[0],
        _ => return Err(ContentError::at_node(file, "", tree, RULES_SHAPE)),
    };
    match &inner.value {
        ContentTree::Map(entries) => Ok(entries),
        _ => Err(ContentError::at_node(file, "", inner, RULES_SHAPE)),
    }
}

const RULES_SHAPE: &str = "expected `Rules({ \"Section\": (…), … })`";

/// Read a `rules.ron` tree, in file order. Unknown and duplicate sections and section values
/// that do not deserialize are errors with the file, line and field path; the first one is
/// returned.
pub(crate) fn parse_rules<'t>(
    reg: &DefRegistry,
    file: &Path,
    tree: &'t Node,
) -> Result<Vec<ParsedSection<'t>>, ContentError> {
    let mut out: Vec<ParsedSection<'t>> = Vec::new();
    for (key, value) in section_map(file, tree)? {
        let ContentTree::Str(name) = &key.value else {
            return Err(ContentError::at_node(
                file,
                "",
                key,
                "rules section names are strings: `\"Section\"`",
            ));
        };
        let Some((name, entry)) = reg.rules_entry(name) else {
            let known: Vec<&str> = reg.rules_section_names().collect();
            return Err(ContentError::at_node(
                file,
                name.as_str(),
                key,
                format!(
                    "unknown rules section '{name}' (registered: {})",
                    known.join(", ")
                ),
            ));
        };
        if out.iter().any(|seen| seen.name == name) {
            return Err(ContentError::at_node(
                file,
                name,
                key,
                format!("duplicate rules section '{name}'"),
            ));
        }
        let typed = erased_from_tree(value, file, "", name.to_owned(), |de| {
            (entry.deserialize)(de)
        })?;
        out.push(ParsedSection {
            name,
            value: typed,
            tree: value,
        });
    }
    Ok(out)
}
