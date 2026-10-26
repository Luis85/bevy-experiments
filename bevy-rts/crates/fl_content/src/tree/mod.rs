//! The lossless content tree (ADR-0020): every content file is parsed into a [`Node`] tree
//! with spans, and typed values are read from trees through [`TreeDeserializer`].
//!
//! ron 0.12's `deserialize_any` reports a bare identifier (`class: Tracked`) as unit, so
//! neither `ron::Value` nor `serde_json::Value` can hold our content without losing enum
//! variants (R148). [`parse_ron`] reads the RON subset our content uses into a tree that keeps
//! identifiers, struct names and positions; [`to_ron`] prints a tree back.
//!
//! Typed values read from a tree equal what `ron` reads from the same text, so the rules hash
//! of content no patch touches is unchanged:
//! - integers become `i128` and convert to the requested width, or to a float type the way
//!   `str::parse` would (a correctly rounded `as` cast);
//! - floats are parsed with `str::parse::<f64>()`. A literal that rounds differently when
//!   parsed straight to `f32` (as `ron` does for an `f32` field) than through `f64` is
//!   rejected, so `Float(f64)` determines both widths exactly. So is a float that overflows
//!   to infinity, and the integer `-0` (whose sign a float field would keep).
//!
//! The tree types and path keys live here; `parse` reads text, `print` writes it and
//! `de` deserializes typed values from a tree.

use std::fmt::{self, Display, Write as _};

mod de;
mod parse;
mod print;

pub use de::{TreeDeserializer, TreeError, from_tree};
pub(crate) use de::{erased_from_tree, from_tree_at};
pub use parse::parse_ron;
pub use print::to_ron;

/// A 1-based position in a content file; `col` counts characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

/// One value of a content file, as written.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentTree {
    /// `()`.
    Unit,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    /// A bare identifier: an enum unit variant (`Tracked`).
    Ident(String),
    /// `None` or `Some(..)`.
    Option(Option<Box<Node>>),
    /// `[a, b]`.
    Seq(Vec<Node>),
    /// `(a, b)`: an unnamed tuple of at least one item.
    Tuple(Vec<Node>),
    /// `{ k: v }`, in written order.
    Map(Vec<(Node, Node)>),
    /// `Name(a: 1)` or `(a: 1)`; `Name()` is a named struct without fields.
    Struct {
        name: Option<String>,
        fields: Vec<(String, Node)>,
    },
    /// `Name(1, 2)`. The parser always sets `name`; [`to_ron`] prints a nameless one like a
    /// [`Tuple`](Self::Tuple).
    TupleStruct {
        name: Option<String>,
        items: Vec<Node>,
    },
}

/// A [`ContentTree`] value with the position it starts at.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub value: ContentTree,
    pub span: Span,
}

impl Node {
    /// The node's children, in written order: sequence and tuple items, map values, struct
    /// fields and the value inside `Some`. Map keys are not children.
    fn has_children(&self) -> bool {
        match &self.value {
            ContentTree::Option(inner) => inner.is_some(),
            ContentTree::Seq(items)
            | ContentTree::Tuple(items)
            | ContentTree::TupleStruct { items, .. } => !items.is_empty(),
            ContentTree::Map(entries) => !entries.is_empty(),
            ContentTree::Struct { fields, .. } => !fields.is_empty(),
            _ => false,
        }
    }
}

/// One step of a path into a document: a struct field or map key, or a sequence index.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathSeg {
    Field(String),
    Index(usize),
}

/// A segment as it appears in a path: a field or map key with `\` and `.` escaped as `\\`
/// and `\.`, an index as its number.
impl Display for PathSeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathSeg::Field(name) => {
                for c in name.chars() {
                    if matches!(c, '.' | '\\') {
                        f.write_char('\\')?;
                    }
                    f.write_char(c)?;
                }
                Ok(())
            }
            PathSeg::Index(i) => write!(f, "{i}"),
        }
    }
}

/// The path key of `path` in document `doc` (ADR-0020): `<doc>/<field path>`, the field path
/// dot-separated with map keys as written and sequence indices as numbers.
/// `doc` is `rules` or `<kind>:<id>`: `"unit:base.de.pz3/components.Hull.footprint.0"`,
/// `"rules/Movement.min_speed_frac"`. An empty path is the document itself: `"<doc>"`.
///
/// A `.` or `\` inside a map key is escaped with a `\` (`{ "a.b": 1 }` is `…/a\.b`), so
/// [`split_path_key`] reads every key back.
pub fn path_key(doc: &str, path: &[PathSeg]) -> String {
    let mut key = doc.to_owned();
    for (i, seg) in path.iter().enumerate() {
        key.push(if i == 0 { '/' } else { '.' });
        write!(key, "{seg}").expect("writing to a String cannot fail");
    }
    key
}

/// Split a [`path_key`] into its document and its segments, unescaped: the inverse of
/// [`path_key`], except that an index and a map key written as the same number read back the
/// same (the document's tree tells them apart). `None` if a `\` escapes anything but `.` or
/// `\`, or ends the key.
pub fn split_path_key(key: &str) -> Option<(&str, Vec<String>)> {
    let Some((doc, path)) = key.split_once('/') else {
        return Some((key, Vec::new()));
    };
    let mut segs = vec![String::new()];
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(escaped @ ('.' | '\\')) => segs.last_mut()?.push(escaped),
                _ => return None,
            },
            '.' => segs.push(String::new()),
            c => segs.last_mut()?.push(c),
        }
    }
    Some((doc, segs))
}

/// The path segment text of a map key: strings and identifiers as written, other keys as RON.
pub(super) fn key_text(key: &Node) -> String {
    match &key.value {
        ContentTree::Str(s) | ContentTree::Ident(s) => s.clone(),
        ContentTree::Int(i) => i.to_string(),
        _ => to_ron(key),
    }
}

/// Call `f` with the path of every leaf under `node`: every node without children (scalars,
/// `None`, empty containers). `Some(..)` and struct names add no path segment.
pub(crate) fn for_each_leaf(node: &Node, path: &mut Vec<PathSeg>, f: &mut impl FnMut(&[PathSeg])) {
    if !node.has_children() {
        f(path);
        return;
    }
    match &node.value {
        ContentTree::Option(Some(inner)) => for_each_leaf(inner, path, f),
        ContentTree::Seq(items)
        | ContentTree::Tuple(items)
        | ContentTree::TupleStruct { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                path.push(PathSeg::Index(i));
                for_each_leaf(item, path, f);
                path.pop();
            }
        }
        ContentTree::Map(entries) => {
            for (key, value) in entries {
                path.push(PathSeg::Field(key_text(key)));
                for_each_leaf(value, path, f);
                path.pop();
            }
        }
        ContentTree::Struct { fields, .. } => {
            for (name, value) in fields {
                path.push(PathSeg::Field(name.clone()));
                for_each_leaf(value, path, f);
                path.pop();
            }
        }
        _ => unreachable!("only containers have children"),
    }
}
