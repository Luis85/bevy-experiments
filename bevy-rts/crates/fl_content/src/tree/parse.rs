//! [`parse_ron`]: the RON subset our content uses, read into a [`Node`] tree with spans.

use std::fmt::Display;
use std::path::Path;

use super::{ContentTree, Node, Span};
use crate::error::ContentError;

/// Containers nest at most this deep, like `ron`'s default recursion limit.
const MAX_DEPTH: usize = 128;

/// Parse `text`, the contents of `file`, into a tree.
///
/// The RON subset our content uses: `//` and `/* */` comments, named and unnamed structs and
/// tuples, lists, maps, strings with `\"` `\\` `\n` `\t` `\u{…}` escapes, decimal integers,
/// floats (digits with `.` or an exponent), `true`/`false`, `Some(..)`/`None`, `()`, bare
/// identifiers and trailing commas. Anything else is an error naming the file, line and
/// column.
pub fn parse_ron(file: &Path, text: &str) -> Result<Node, ContentError> {
    let mut parser = Parser {
        file,
        text,
        pos: 0,
        line: 1,
        col: 1,
        depth: 0,
    };
    parser.skip_ws()?;
    let node = parser.value()?;
    parser.skip_ws()?;
    if let Some(c) = parser.peek() {
        return Err(parser.error(
            parser.span(),
            format!("unexpected '{c}' after the value: one value per file"),
        ));
    }
    Ok(node)
}

#[derive(Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    col: u32,
}

struct Parser<'t> {
    file: &'t Path,
    text: &'t str,
    pos: usize,
    line: u32,
    col: u32,
    depth: usize,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

/// Whitespace as `ron` defines it.
fn is_ws(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\x0B'
            | '\x0C'
            | '\u{85}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

impl Parser<'_> {
    fn span(&self) -> Span {
        Span {
            line: self.line,
            col: self.col,
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            col: self.col,
        }
    }

    fn reset(&mut self, mark: Mark) {
        self.pos = mark.pos;
        self.line = mark.line;
        self.col = mark.col;
    }

    fn error(&self, span: Span, message: impl Display) -> ContentError {
        ContentError {
            file: self.file.to_path_buf(),
            line_col: Some((span.line as usize, span.col as usize)),
            path: String::new(),
            message: message.to_string(),
        }
    }

    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek2(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char, what: &str) -> Result<(), ContentError> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.unexpected(what))
        }
    }

    /// An error at the current position: `expected <what>, found …`.
    fn unexpected(&self, what: &str) -> ContentError {
        match self.peek() {
            Some(c) => self.error(self.span(), format!("expected {what}, found '{c}'")),
            None => self.error(
                self.span(),
                format!("expected {what}, found the end of the file"),
            ),
        }
    }

    /// Skip whitespace and comments.
    fn skip_ws(&mut self) -> Result<(), ContentError> {
        loop {
            match self.peek() {
                Some(c) if is_ws(c) => {
                    self.bump();
                }
                Some('/') if self.peek2() == Some('/') => {
                    while let Some(c) = self.bump() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('/') if self.peek2() == Some('*') => {
                    let start = self.span();
                    self.bump();
                    self.bump();
                    let mut open = 1_usize;
                    while open > 0 {
                        match self.bump() {
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                open -= 1;
                            }
                            Some('/') if self.peek() == Some('*') => {
                                self.bump();
                                open += 1;
                            }
                            Some(_) => {}
                            None => return Err(self.error(start, "unclosed block comment")),
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn enter(&mut self, span: Span) -> Result<(), ContentError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error(span, format!("values nest deeper than {MAX_DEPTH} levels")));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn node(value: ContentTree, span: Span) -> Node {
        Node { value, span }
    }

    fn value(&mut self) -> Result<Node, ContentError> {
        let span = self.span();
        match self.peek() {
            None => Err(self.unexpected("a value")),
            Some('"') => {
                let s = self.string()?;
                Ok(Self::node(ContentTree::Str(s), span))
            }
            Some('[') => self.seq(),
            Some('{') => self.map(),
            Some('(') => self.paren(None, span),
            Some('\'') => Err(self.error(span, "char literals are not supported")),
            Some('#') => Err(self.error(span, "attributes (`#![…]`) are not supported")),
            Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') => self.number(),
            Some(c) if is_ident_start(c) => self.ident_value(),
            Some(c) => Err(self.error(span, format!("unexpected '{c}', expected a value"))),
        }
    }

    fn ident(&mut self) -> Result<String, ContentError> {
        let span = self.span();
        let start = self.pos;
        while self.peek().is_some_and(is_ident_char) {
            self.bump();
        }
        let ident = &self.text[start..self.pos];
        match self.peek() {
            Some('"' | '#') if matches!(ident, "r" | "b" | "br") => Err(self.error(
                span,
                "raw strings, byte strings and raw identifiers are not supported",
            )),
            Some('\'') if ident == "b" => Err(self.error(span, "byte literals are not supported")),
            Some(c) if c.is_alphanumeric() => {
                Err(self.error(span, "non-ASCII identifiers are not supported"))
            }
            _ if ident.is_empty() => Err(self.unexpected("an identifier")),
            _ => Ok(ident.to_owned()),
        }
    }

    fn ident_value(&mut self) -> Result<Node, ContentError> {
        let span = self.span();
        let ident = self.ident()?;
        match ident.as_str() {
            "true" => return Ok(Self::node(ContentTree::Bool(true), span)),
            "false" => return Ok(Self::node(ContentTree::Bool(false), span)),
            "None" => return Ok(Self::node(ContentTree::Option(None), span)),
            "Some" => {
                self.skip_ws()?;
                self.expect('(', "'(' after `Some`")?;
                self.enter(span)?;
                self.skip_ws()?;
                let inner = self.value()?;
                self.skip_ws()?;
                if self.eat(',') {
                    self.skip_ws()?;
                }
                self.expect(')', "')' closing `Some(`")?;
                self.leave();
                return Ok(Self::node(ContentTree::Option(Some(Box::new(inner))), span));
            }
            _ => {}
        }
        let after = self.mark();
        self.skip_ws()?;
        if self.peek() == Some('(') {
            self.paren(Some(ident), span)
        } else {
            self.reset(after);
            Ok(Self::node(ContentTree::Ident(ident), span))
        }
    }

    /// Whether the next tokens are `ident :`, the start of a struct field.
    fn at_field(&mut self) -> Result<bool, ContentError> {
        if !self.peek().is_some_and(is_ident_start) {
            return Ok(false);
        }
        let mark = self.mark();
        while self.peek().is_some_and(is_ident_char) {
            self.bump();
        }
        self.skip_ws()?;
        let field = self.peek() == Some(':');
        self.reset(mark);
        Ok(field)
    }

    /// `( … )` after an optional struct name: unit, struct or tuple.
    fn paren(&mut self, name: Option<String>, span: Span) -> Result<Node, ContentError> {
        self.expect('(', "'('")?;
        self.enter(span)?;
        self.skip_ws()?;
        let value = if self.eat(')') {
            match name {
                None => ContentTree::Unit,
                Some(name) => ContentTree::Struct {
                    name: Some(name),
                    fields: Vec::new(),
                },
            }
        } else if self.at_field()? {
            let mut fields = Vec::new();
            loop {
                let field = self.ident()?;
                self.skip_ws()?;
                self.expect(':', "':' after a field name")?;
                self.skip_ws()?;
                fields.push((field, self.value()?));
                if self.list_end(')')? {
                    break;
                }
                if !self.at_field()? {
                    return Err(self.unexpected("a field name"));
                }
            }
            ContentTree::Struct { name, fields }
        } else {
            let mut items = Vec::new();
            loop {
                items.push(self.value()?);
                if self.list_end(')')? {
                    break;
                }
            }
            match name {
                None => ContentTree::Tuple(items),
                Some(name) => ContentTree::TupleStruct {
                    name: Some(name),
                    items,
                },
            }
        };
        self.leave();
        Ok(Self::node(value, span))
    }

    /// After an item: skip a `,` and report whether `close` ended the list (consuming it).
    fn list_end(&mut self, close: char) -> Result<bool, ContentError> {
        self.skip_ws()?;
        if self.eat(close) {
            return Ok(true);
        }
        if !self.eat(',') {
            return Err(self.unexpected(&format!("',' or '{close}'")));
        }
        self.skip_ws()?;
        Ok(self.eat(close))
    }

    fn seq(&mut self) -> Result<Node, ContentError> {
        let span = self.span();
        self.expect('[', "'['")?;
        self.enter(span)?;
        self.skip_ws()?;
        let mut items = Vec::new();
        if !self.eat(']') {
            loop {
                items.push(self.value()?);
                if self.list_end(']')? {
                    break;
                }
            }
        }
        self.leave();
        Ok(Self::node(ContentTree::Seq(items), span))
    }

    fn map(&mut self) -> Result<Node, ContentError> {
        let span = self.span();
        self.expect('{', "'{'")?;
        self.enter(span)?;
        self.skip_ws()?;
        let mut entries = Vec::new();
        if !self.eat('}') {
            loop {
                let key = self.value()?;
                self.skip_ws()?;
                self.expect(':', "':' after a map key")?;
                self.skip_ws()?;
                entries.push((key, self.value()?));
                if self.list_end('}')? {
                    break;
                }
            }
        }
        self.leave();
        Ok(Self::node(ContentTree::Map(entries), span))
    }

    fn string(&mut self) -> Result<String, ContentError> {
        let start = self.span();
        self.expect('"', "'\"'")?;
        let mut out = String::new();
        loop {
            let at = self.span();
            match self.bump() {
                None => return Err(self.error(start, "unterminated string")),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('u') => out.push(self.unicode_escape(at)?),
                    Some(c) => {
                        return Err(self.error(at, format!("unsupported escape '\\{c}'")));
                    }
                    None => return Err(self.error(start, "unterminated string")),
                },
                Some(c) => out.push(c),
            }
        }
    }

    /// The `{…}` of a `\u{…}` escape: 1 to 6 hex digits naming a Unicode scalar value.
    fn unicode_escape(&mut self, at: Span) -> Result<char, ContentError> {
        let bad = |p: &Self| p.error(at, "a \\u escape is `\\u{…}` with 1 to 6 hex digits");
        if !self.eat('{') {
            return Err(bad(self));
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            self.bump();
        }
        let digits = &self.text[start..self.pos];
        if digits.is_empty() || digits.len() > 6 || !self.eat('}') {
            return Err(bad(self));
        }
        let code = u32::from_str_radix(digits, 16).map_err(|_| bad(self))?;
        char::from_u32(code)
            .ok_or_else(|| self.error(at, format!("\\u{{{digits}}} is not a Unicode scalar value")))
    }

    fn number(&mut self) -> Result<Node, ContentError> {
        let span = self.span();
        let start = self.pos;
        if matches!(self.peek(), Some('-' | '+')) {
            self.bump();
        }
        let mut prev = ' ';
        while let Some(c) = self.peek() {
            let part = c.is_ascii_digit()
                || matches!(c, '.' | 'e' | 'E' | '_')
                || (matches!(c, '+' | '-') && matches!(prev, 'e' | 'E'));
            if !part {
                break;
            }
            prev = c;
            self.bump();
        }
        let text = &self.text[start..self.pos];
        if self.peek().is_some_and(char::is_alphanumeric) {
            return Err(self.error(
                span,
                format!(
                    "unsupported number literal starting `{text}`: only decimal integers and \
                     floats, without suffixes"
                ),
            ));
        }
        if text.contains('_') {
            return Err(self.error(span, "underscores in numbers are not supported"));
        }
        if !text.chars().any(|c| c.is_ascii_digit()) {
            return Err(self.error(span, format!("`{text}` is not a number")));
        }
        let value = if text.contains(['.', 'e', 'E']) {
            ContentTree::Float(float_literal(text).map_err(|msg| self.error(span, msg))?)
        } else {
            let int: i128 = text
                .parse()
                .map_err(|_| self.error(span, format!("integer `{text}` is out of range")))?;
            if int == 0 && text.starts_with('-') {
                return Err(self.error(
                    span,
                    format!("`{text}` is a negative zero integer: write 0, or -0.0 for a float"),
                ));
            }
            ContentTree::Int(int)
        };
        Ok(Self::node(value, span))
    }
}

/// Parse a float literal the way the tree keeps it, or say why it cannot be kept exactly.
pub(super) fn float_literal(text: &str) -> Result<f64, String> {
    let value: f64 = text
        .parse()
        .map_err(|_| format!("`{text}` is not a valid float"))?;
    if !value.is_finite() {
        return Err(format!("float `{text}` is out of range"));
    }
    if !same_as_f32(text, value) {
        return Err(format!(
            "float `{text}` rounds differently when read as f32 and as f64; write it with fewer \
             digits"
        ));
    }
    Ok(value)
}

/// Whether `text` parsed straight to `f32` (as `ron` reads an `f32` field) equals `value`,
/// its `f64` parse, narrowed to `f32`.
fn same_as_f32(text: &str, value: f64) -> bool {
    text.parse::<f32>()
        .is_ok_and(|direct| direct.to_bits() == (value as f32).to_bits())
}
