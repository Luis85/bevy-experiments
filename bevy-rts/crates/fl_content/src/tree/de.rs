//! [`TreeDeserializer`]: `serde` deserialization of typed values from a [`Node`] tree.

use std::cell::RefCell;
use std::fmt::{self, Display};
use std::path::Path;
use std::rc::Rc;

use serde::de::value::BorrowedStrDeserializer;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, Unexpected,
    VariantAccess, Visitor,
};

use super::{ContentTree, Node, PathSeg, Span, key_text};
use crate::error::ContentError;

/// The error of a [`TreeDeserializer`]: a message. Where it happened is recorded separately,
/// so it survives type-erased deserialization; [`from_tree`] reports both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeError(String);

impl Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TreeError {}

impl de::Error for TreeError {
    fn custom<T: Display>(msg: T) -> Self {
        TreeError(msg.to_string())
    }
}

/// Where the first (innermost) error of one deserialization happened: the field path and the
/// node's span.
///
/// Only the first error is kept, which is right because serde propagates an error outward
/// unchanged. A type that swallows an error and retries (`#[serde(untagged)]`, `flatten`,
/// `Option` fallbacks through `deserialize_any`) would leave a stale site behind and the
/// reported location would be wrong; content types use none of those (`flatten` is already
/// banned in rules-hashed content).
pub(crate) type ErrorSite = Rc<RefCell<Option<(String, Span)>>>;

/// A `serde` deserializer over a [`Node`].
///
/// Strict like `ron`: a struct reads only a struct (or `()`), a tuple only a tuple, a
/// sequence only `[…]`, a map only `{…}`, a string only a string. An enum reads an
/// identifier or string (a unit variant), a struct or tuple struct named as the variant, or a
/// one-entry map `{ Variant: value }`; `None` and `Some(x)` also name the variants `None` and
/// `Some` (`controller: None`). `is_human_readable` is true, so `Fx` reads numbers.
///
/// Error locations: the first (innermost) failing node is recorded as the error's site, so
/// [`from_tree`] can name it even through type-erased deserialization. A type that swallows a
/// deserialization error and retries (`untagged`, `flatten`) would leave that first site
/// stale; content types use neither.
pub struct TreeDeserializer<'a> {
    node: &'a Node,
    path: String,
    site: ErrorSite,
}

fn join(path: &str, seg: impl Display) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{path}.{seg}")
    }
}

fn unexpected(node: &Node) -> Unexpected<'_> {
    match &node.value {
        ContentTree::Unit => Unexpected::Unit,
        ContentTree::Bool(b) => Unexpected::Bool(*b),
        ContentTree::Int(i) => match i64::try_from(*i) {
            Ok(v) => Unexpected::Signed(v),
            Err(_) => match u64::try_from(*i) {
                Ok(v) => Unexpected::Unsigned(v),
                Err(_) => Unexpected::Other("integer"),
            },
        },
        ContentTree::Float(f) => Unexpected::Float(*f),
        ContentTree::Str(s) => Unexpected::Str(s),
        ContentTree::Ident(_) => Unexpected::Other("identifier"),
        ContentTree::Option(_) => Unexpected::Option,
        ContentTree::Seq(_) => Unexpected::Seq,
        ContentTree::Tuple(_) => Unexpected::Other("tuple"),
        ContentTree::Map(_) => Unexpected::Map,
        ContentTree::Struct { .. } => Unexpected::Other("struct"),
        ContentTree::TupleStruct { .. } => Unexpected::Other("tuple struct"),
    }
}

/// Whether a struct-like value's written name (if any) is `expected`.
fn name_ok(name: Option<&String>, expected: &str) -> bool {
    name.is_none_or(|n| n == expected)
}

fn wrong_name(found: &str, expected: &str) -> TreeError {
    TreeError(format!("expected `{expected}`, found `{found}`"))
}

impl<'a> TreeDeserializer<'a> {
    /// A deserializer over `node`; error paths are relative to it.
    pub fn new(node: &'a Node) -> Self {
        Self::at(node, String::new(), ErrorSite::default())
    }

    /// A deserializer over `node` at field path `path`, recording its first error into `site`.
    pub(crate) fn at(node: &'a Node, path: String, site: ErrorSite) -> Self {
        TreeDeserializer { node, path, site }
    }

    fn child(&self, node: &'a Node, seg: impl Display) -> Self {
        Self::at(node, join(&self.path, seg), Rc::clone(&self.site))
    }

    /// The same path, for a node that adds no segment (`Some(..)`, a newtype's content).
    fn inner(&self, node: &'a Node) -> Self {
        Self::at(node, self.path.clone(), Rc::clone(&self.site))
    }

    /// Record this node as the error site unless an inner node already is.
    fn locate<T>(&self, result: Result<T, TreeError>) -> Result<T, TreeError> {
        if result.is_err() {
            self.record(self.node.span);
        }
        result
    }

    /// Record this path at `span` as the error site unless one is recorded already.
    fn record(&self, span: Span) {
        let mut site = self.site.borrow_mut();
        if site.is_none() {
            *site = Some((self.path.clone(), span));
        }
    }

    fn invalid(&self, expected: &dyn de::Expected) -> TreeError {
        de::Error::invalid_type(unexpected(self.node), expected)
    }

    fn int<T: TryFrom<i128>>(&self, what: &'static str) -> Result<T, TreeError> {
        match self.node.value {
            ContentTree::Int(i) => T::try_from(i)
                .map_err(|_| TreeError(format!("integer {i} is out of range for {what}"))),
            _ => Err(self.invalid(&what)),
        }
    }

    fn seq<V: Visitor<'a>>(&self, items: &'a [Node], visitor: V) -> Result<V::Value, TreeError> {
        let mut access = SeqAcc {
            de: self,
            items,
            next: 0,
        };
        let value = visitor.visit_seq(&mut access)?;
        if access.next < items.len() {
            return Err(TreeError(format!(
                "{} items, but only {} expected",
                items.len(),
                access.next
            )));
        }
        Ok(value)
    }

    fn map<V: Visitor<'a>>(
        &self,
        entries: &'a [(Node, Node)],
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let mut access = MapAcc {
            de: self,
            entries,
            next: 0,
            value: None,
        };
        let value = visitor.visit_map(&mut access)?;
        if access.next < entries.len() {
            return Err(TreeError(format!(
                "{} entries, but only {} expected",
                entries.len(),
                access.next
            )));
        }
        Ok(value)
    }

    fn fields<V: Visitor<'a>>(
        &self,
        fields: &'a [(String, Node)],
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let mut access = StructAcc {
            de: self,
            fields,
            next: 0,
        };
        let value = visitor.visit_map(&mut access)?;
        if access.next < fields.len() {
            return Err(TreeError(format!(
                "{} fields, but only {} expected",
                fields.len(),
                access.next
            )));
        }
        Ok(value)
    }
}

macro_rules! deserialize_int {
    ($($method:ident => $visit:ident: $ty:ty,)*) => {$(
        fn $method<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
            let result = self.int::<$ty>(stringify!($ty)).and_then(|v| visitor.$visit(v));
            self.locate(result)
        }
    )*};
}

impl<'a> de::Deserializer<'a> for TreeDeserializer<'a> {
    type Error = TreeError;

    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Unit => visitor.visit_unit(),
            ContentTree::Bool(b) => visitor.visit_bool(*b),
            ContentTree::Int(i) => match i64::try_from(*i) {
                Ok(v) => visitor.visit_i64(v),
                Err(_) => match u64::try_from(*i) {
                    Ok(v) => visitor.visit_u64(v),
                    Err(_) => visitor.visit_i128(*i),
                },
            },
            ContentTree::Float(f) => visitor.visit_f64(*f),
            ContentTree::Str(s) | ContentTree::Ident(s) => visitor.visit_borrowed_str(s),
            ContentTree::Option(None) => visitor.visit_none(),
            ContentTree::Option(Some(inner)) => visitor.visit_some(self.inner(inner)),
            ContentTree::Seq(items)
            | ContentTree::Tuple(items)
            | ContentTree::TupleStruct { items, .. } => self.seq(items, visitor),
            ContentTree::Map(entries) => self.map(entries, visitor),
            ContentTree::Struct { fields, .. } => self.fields(fields, visitor),
        };
        self.locate(result)
    }

    fn deserialize_bool<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match self.node.value {
            ContentTree::Bool(b) => visitor.visit_bool(b),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    deserialize_int! {
        deserialize_i8 => visit_i8: i8,
        deserialize_i16 => visit_i16: i16,
        deserialize_i32 => visit_i32: i32,
        deserialize_i64 => visit_i64: i64,
        deserialize_i128 => visit_i128: i128,
        deserialize_u8 => visit_u8: u8,
        deserialize_u16 => visit_u16: u16,
        deserialize_u32 => visit_u32: u32,
        deserialize_u64 => visit_u64: u64,
        deserialize_u128 => visit_u128: u128,
    }

    /// Like `ron`: an integer is read as `str::parse::<f32>` would (a correctly rounded cast),
    /// a float is narrowed (exact for every literal [`parse_ron`] accepts).
    fn deserialize_f32<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match self.node.value {
            ContentTree::Int(i) => visitor.visit_f32(i as f32),
            ContentTree::Float(f) => visitor.visit_f32(f as f32),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_f64<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match self.node.value {
            ContentTree::Int(i) => visitor.visit_f64(i as f64),
            ContentTree::Float(f) => visitor.visit_f64(f),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    /// There are no char literals: a one-character string.
    fn deserialize_char<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Str(s) if s.chars().count() == 1 => {
                visitor.visit_char(s.chars().next().expect("one char"))
            }
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_str<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Str(s) => visitor.visit_borrowed_str(s),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_string<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'a>>(self, _visitor: V) -> Result<V::Value, TreeError> {
        let result = Err(TreeError("byte strings are not supported".to_owned()));
        self.locate(result)
    }

    fn deserialize_byte_buf<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Option(None) => visitor.visit_none(),
            ContentTree::Option(Some(inner)) => visitor.visit_some(self.inner(inner)),
            _ => Err(self.invalid(&"`Some(…)` or `None`")),
        };
        self.locate(result)
    }

    fn deserialize_unit<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match self.node.value {
            ContentTree::Unit => visitor.visit_unit(),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    /// `()` or the bare name, as `ron` accepts.
    fn deserialize_unit_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Unit => visitor.visit_unit(),
            ContentTree::Ident(found) if found == name => visitor.visit_unit(),
            ContentTree::Ident(found) => Err(wrong_name(found, name)),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    /// `Name(value)` or `(value)`.
    fn deserialize_newtype_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::TupleStruct { name: found, items } if items.len() == 1 => {
                if name_ok(found.as_ref(), name) {
                    visitor.visit_newtype_struct(self.inner(&items[0]))
                } else {
                    Err(wrong_name(found.as_deref().unwrap_or_default(), name))
                }
            }
            ContentTree::Tuple(items) if items.len() == 1 => {
                visitor.visit_newtype_struct(self.inner(&items[0]))
            }
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_seq<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Seq(items) => self.seq(items, visitor),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_tuple<V: Visitor<'a>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Tuple(items) => self.seq(items, visitor),
            ContentTree::Unit => self.seq(&[], visitor),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_tuple_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::TupleStruct { name: found, items } => {
                if name_ok(found.as_ref(), name) {
                    self.seq(items, visitor)
                } else {
                    Err(wrong_name(found.as_deref().unwrap_or_default(), name))
                }
            }
            ContentTree::Struct {
                name: Some(found),
                fields,
            } if fields.is_empty() => {
                if found == name {
                    self.seq(&[], visitor)
                } else {
                    Err(wrong_name(found, name))
                }
            }
            ContentTree::Tuple(items) => self.seq(items, visitor),
            ContentTree::Unit => self.seq(&[], visitor),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_map<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Map(entries) => self.map(entries, visitor),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    /// `Name(field: …)`, `(field: …)`, or `()` for a struct whose fields all have defaults.
    fn deserialize_struct<V: Visitor<'a>>(
        self,
        name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Struct {
                name: found,
                fields,
            } => {
                if name_ok(found.as_ref(), name) {
                    self.fields(fields, visitor)
                } else {
                    Err(wrong_name(found.as_deref().unwrap_or_default(), name))
                }
            }
            ContentTree::Unit => self.fields(&[], visitor),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_enum<V: Visitor<'a>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        let access = match &self.node.value {
            ContentTree::Ident(variant) | ContentTree::Str(variant) => Some(EnumAcc {
                de: &self,
                variant,
                payload: Payload::None,
            }),
            // `None` and `Some(x)` are also variant names, as in `controller: None`.
            ContentTree::Option(None) => Some(EnumAcc {
                de: &self,
                variant: "None",
                payload: Payload::None,
            }),
            ContentTree::Option(Some(inner)) => Some(EnumAcc {
                de: &self,
                variant: "Some",
                payload: Payload::Items(std::slice::from_ref(&**inner)),
            }),
            ContentTree::Struct {
                name: Some(variant),
                fields,
            } => Some(EnumAcc {
                de: &self,
                variant,
                payload: Payload::Fields(fields),
            }),
            ContentTree::TupleStruct {
                name: Some(variant),
                items,
            } => Some(EnumAcc {
                de: &self,
                variant,
                payload: Payload::Items(items),
            }),
            ContentTree::Map(entries) if entries.len() == 1 => match &entries[0] {
                (
                    Node {
                        value: ContentTree::Str(variant) | ContentTree::Ident(variant),
                        ..
                    },
                    value,
                ) => Some(EnumAcc {
                    de: &self,
                    variant,
                    payload: Payload::Node(value),
                }),
                _ => None,
            },
            _ => None,
        };
        let result = match access {
            Some(access) => visitor.visit_enum(access),
            None => Err(self.invalid(&"an enum variant")),
        };
        self.locate(result)
    }

    fn deserialize_identifier<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        let result = match &self.node.value {
            ContentTree::Str(s) | ContentTree::Ident(s) => visitor.visit_borrowed_str(s),
            _ => Err(self.invalid(&visitor)),
        };
        self.locate(result)
    }

    fn deserialize_ignored_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, TreeError> {
        visitor.visit_unit()
    }
}

struct SeqAcc<'d, 'a> {
    de: &'d TreeDeserializer<'a>,
    items: &'a [Node],
    next: usize,
}

impl<'a> SeqAccess<'a> for SeqAcc<'_, 'a> {
    type Error = TreeError;

    fn next_element_seed<T: DeserializeSeed<'a>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, TreeError> {
        let Some(item) = self.items.get(self.next) else {
            return Ok(None);
        };
        let index = self.next;
        self.next += 1;
        seed.deserialize(self.de.child(item, index)).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len() - self.next)
    }
}

struct MapAcc<'d, 'a> {
    de: &'d TreeDeserializer<'a>,
    entries: &'a [(Node, Node)],
    next: usize,
    /// The value of the entry whose key was just read, with its path segment.
    value: Option<(&'a Node, String)>,
}

impl<'a> MapAccess<'a> for MapAcc<'_, 'a> {
    type Error = TreeError;

    fn next_key_seed<K: DeserializeSeed<'a>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, TreeError> {
        let Some((key, value)) = self.entries.get(self.next) else {
            return Ok(None);
        };
        self.next += 1;
        let seg = PathSeg::Field(key_text(key)).to_string();
        let result = seed.deserialize(self.de.child(key, &seg)).map(Some);
        self.value = Some((value, seg));
        result
    }

    fn next_value_seed<V: DeserializeSeed<'a>>(&mut self, seed: V) -> Result<V::Value, TreeError> {
        let (value, seg) = self
            .value
            .take()
            .expect("serde reads a map value after its key");
        seed.deserialize(self.de.child(value, seg))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len() - self.next)
    }
}

struct StructAcc<'d, 'a> {
    de: &'d TreeDeserializer<'a>,
    fields: &'a [(String, Node)],
    next: usize,
}

impl<'a> MapAccess<'a> for StructAcc<'_, 'a> {
    type Error = TreeError;

    /// An unknown field is reported at the struct's path, on the field's line.
    fn next_key_seed<K: DeserializeSeed<'a>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, TreeError> {
        let Some((name, value)) = self.fields.get(self.next) else {
            return Ok(None);
        };
        let result = seed.deserialize(BorrowedStrDeserializer::<TreeError>::new(name));
        if result.is_err() {
            self.de.record(value.span);
        }
        result.map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'a>>(&mut self, seed: V) -> Result<V::Value, TreeError> {
        let (name, value) = &self.fields[self.next];
        self.next += 1;
        seed.deserialize(self.de.child(value, name))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.fields.len() - self.next)
    }
}

/// What follows an enum's variant name.
enum Payload<'a> {
    None,
    Fields(&'a [(String, Node)]),
    Items(&'a [Node]),
    /// The value of a one-entry map `{ Variant: value }`.
    Node(&'a Node),
}

struct EnumAcc<'d, 'a> {
    de: &'d TreeDeserializer<'a>,
    variant: &'a str,
    payload: Payload<'a>,
}

impl<'d, 'a> EnumAccess<'a> for EnumAcc<'d, 'a> {
    type Error = TreeError;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'a>>(self, seed: V) -> Result<(V::Value, Self), TreeError> {
        let variant = seed.deserialize(BorrowedStrDeserializer::<TreeError>::new(self.variant))?;
        Ok((variant, self))
    }
}

impl<'a> EnumAcc<'_, 'a> {
    /// A deserializer for the payload node of the map form, at path `<path>.<Variant>`.
    fn payload_de(&self, node: &'a Node) -> TreeDeserializer<'a> {
        self.de.child(node, self.variant)
    }

    fn wrong(&self, expected: &str) -> TreeError {
        TreeError(format!(
            "variant `{}` is not written as {expected}",
            self.variant
        ))
    }
}

impl<'a> VariantAccess<'a> for EnumAcc<'_, 'a> {
    type Error = TreeError;

    fn unit_variant(self) -> Result<(), TreeError> {
        match self.payload {
            Payload::None => Ok(()),
            Payload::Node(Node {
                value: ContentTree::Unit,
                ..
            }) => Ok(()),
            _ => Err(self.wrong("a unit variant (a bare name)")),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'a>>(self, seed: T) -> Result<T::Value, TreeError> {
        match self.payload {
            Payload::Items([item]) => seed.deserialize(self.de.inner(item)),
            Payload::Node(node) => seed.deserialize(self.payload_de(node)),
            _ => Err(self.wrong("a newtype variant (`Variant(value)`)")),
        }
    }

    fn tuple_variant<V: Visitor<'a>>(self, _len: usize, visitor: V) -> Result<V::Value, TreeError> {
        match self.payload {
            Payload::Items(items) => self.de.seq(items, visitor),
            Payload::Node(node) => {
                let de = self.payload_de(node);
                let result = match &node.value {
                    ContentTree::Tuple(items) | ContentTree::Seq(items) => de.seq(items, visitor),
                    _ => Err(de.invalid(&visitor)),
                };
                de.locate(result)
            }
            _ => Err(self.wrong("a tuple variant (`Variant(a, b)`)")),
        }
    }

    fn struct_variant<V: Visitor<'a>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TreeError> {
        match self.payload {
            Payload::Fields(fields) => self.de.fields(fields, visitor),
            Payload::Node(node) => {
                let de = self.payload_de(node);
                let result = match &node.value {
                    ContentTree::Struct { name: None, fields } => de.fields(fields, visitor),
                    ContentTree::Map(entries) => de.map(entries, visitor),
                    _ => Err(de.invalid(&visitor)),
                };
                de.locate(result)
            }
            _ => Err(self.wrong("a struct variant (`Variant(field: …)`)")),
        }
    }
}

/// A content error for a failed deserialization of `node`: at the recorded site (or `node`),
/// with the path prefixed by `prefix` (`"<id>/"`, or empty).
pub(crate) fn tree_error(
    file: &Path,
    prefix: &str,
    site: &ErrorSite,
    node: &Node,
    message: impl Display,
) -> ContentError {
    let (path, span) = site
        .borrow_mut()
        .take()
        .unwrap_or_else(|| (String::new(), node.span));
    let path = match (prefix.strip_suffix('/'), path.is_empty()) {
        (Some(id), true) => id.to_owned(),
        _ => format!("{prefix}{path}"),
    };
    ContentError {
        file: file.to_path_buf(),
        line_col: Some((span.line as usize, span.col as usize)),
        path,
        message: message.to_string(),
    }
}

/// Deserialize a `T` from `node`, which came from `file`. An error carries the field path
/// relative to `node` and the span of the node that failed.
pub fn from_tree<T: DeserializeOwned>(node: &Node, file: &Path) -> Result<T, ContentError> {
    from_tree_at(node, file, "", String::new())
}

/// [`from_tree`] for a node at field path `path` of a document, with error paths prefixed by
/// `prefix`.
pub(crate) fn from_tree_at<T: DeserializeOwned>(
    node: &Node,
    file: &Path,
    prefix: &str,
    path: String,
) -> Result<T, ContentError> {
    let site = ErrorSite::default();
    T::deserialize(TreeDeserializer::at(node, path, Rc::clone(&site)))
        .map_err(|err| tree_error(file, prefix, &site, node, err))
}

/// Run a type-erased deserialization (a registry entry) over `node` at field path `path`;
/// errors as in [`from_tree_at`].
pub(crate) fn erased_from_tree<R>(
    node: &Node,
    file: &Path,
    prefix: &str,
    path: String,
    run: impl FnOnce(&mut dyn erased_serde::Deserializer<'_>) -> Result<R, erased_serde::Error>,
) -> Result<R, ContentError> {
    let site = ErrorSite::default();
    let mut erased =
        <dyn erased_serde::Deserializer>::erase(TreeDeserializer::at(node, path, Rc::clone(&site)));
    run(&mut erased).map_err(|err| tree_error(file, prefix, &site, node, err))
}
