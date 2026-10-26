//! [`to_ron`]: printing a [`Node`] tree as RON that [`parse_ron`](super::parse_ron) reads back.

use std::fmt::Write as _;

use super::parse::float_literal;
use super::{ContentTree, Node};

/// Print `node` as RON that [`parse_ron`] reads back equal, spans aside.
///
/// Every tree the parser produces prints and reads back equal. A few trees it never produces
/// print like another one: an empty [`Tuple`](ContentTree::Tuple) and a nameless struct
/// without fields print as `()` (unit), a nameless
/// [`TupleStruct`](ContentTree::TupleStruct) as a tuple, a named one without items as a named
/// struct without fields, and an identifier `true`, `false`, `None` or `Some` as that keyword.
pub fn to_ron(node: &Node) -> String {
    let mut out = String::new();
    print(node, &mut out);
    out
}

fn print(node: &Node, out: &mut String) {
    match &node.value {
        ContentTree::Unit => out.push_str("()"),
        ContentTree::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ContentTree::Int(i) => write!(out, "{i}").expect("writing to a String cannot fail"),
        ContentTree::Float(f) => out.push_str(&float_text(*f)),
        ContentTree::Str(s) => print_str(s, out),
        ContentTree::Ident(s) => out.push_str(s),
        ContentTree::Option(None) => out.push_str("None"),
        ContentTree::Option(Some(inner)) => {
            out.push_str("Some(");
            print(inner, out);
            out.push(')');
        }
        ContentTree::Seq(items) => {
            out.push('[');
            print_items(items, out);
            out.push(']');
        }
        ContentTree::Tuple(items) | ContentTree::TupleStruct { name: None, items } => {
            out.push('(');
            print_items(items, out);
            out.push(')');
        }
        ContentTree::TupleStruct {
            name: Some(name),
            items,
        } => {
            out.push_str(name);
            out.push('(');
            print_items(items, out);
            out.push(')');
        }
        ContentTree::Map(entries) => {
            out.push('{');
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print(key, out);
                out.push_str(": ");
                print(value, out);
            }
            out.push('}');
        }
        ContentTree::Struct { name, fields } => {
            if let Some(name) = name {
                out.push_str(name);
            }
            out.push('(');
            for (i, (field, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(field);
                out.push_str(": ");
                print(value, out);
            }
            out.push(')');
        }
    }
}

fn print_items(items: &[Node], out: &mut String) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        print(item, out);
    }
}

fn print_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                write!(out, "\\u{{{:x}}}", u32::from(c)).expect("writing to a String cannot fail");
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A float literal that [`parse_ron`] reads back as exactly `value`: the shortest one, or,
/// when that one would round differently as `f32`, the exact decimal expansion.
fn float_text(value: f64) -> String {
    let shortest = format!("{value:?}");
    if float_literal(&shortest).is_ok_and(|back| back.to_bits() == value.to_bits()) {
        return shortest;
    }
    // Every finite f64 has a finite decimal expansion of at most 1074 fractional digits.
    let exact = format!("{value:.1074}");
    let trimmed = exact.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_owned()
    }
}
