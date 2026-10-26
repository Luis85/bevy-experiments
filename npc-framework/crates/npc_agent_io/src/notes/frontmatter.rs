//! A small reader and writer for the flat YAML frontmatter of the notes
//! files (notes-as-Obsidian-files spec O3), hand-written so no YAML crate is
//! needed: `cargo deny` vets every licence, and `serde_yaml` is unmaintained.
//!
//! The frontmatter is the block between a first line `---` and the next
//! `---` (or `...`) line. Supported, as Obsidian writes it:
//!
//! - `key: value` with a plain, `"double-quoted"` (with `\` escapes) or
//!   `'single-quoted'` scalar; an empty value; a ` #` comment after a plain
//!   value; comment and blank lines;
//! - lists as `key: [a, "b, c"]` or as `- item` lines under `key:`.
//!
//! Everything else (nested maps, block scalars, anchors, multi-line values)
//! is kept as raw text: such a property can be read back verbatim but not
//! as a value ([`Frontmatter::is_complex`]).
//!
//! Every property keeps the lines it was read from; [`Frontmatter::set`]
//! rewrites a property only when its value changes, so properties the game
//! does not know, and the formatting of those it does not change, survive a
//! write byte for byte. Writing quotes every value YAML could misread
//! ([`scalar`]). Pure: no I/O.

/// A property's value as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A scalar, as text (numbers and booleans too); empty for `key:` with
    /// nothing after it.
    Text(String),
    /// A list: `[a, b]`, or `- a` lines under the key.
    List(Vec<String>),
}

/// Why a file's frontmatter could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontmatterError {
    /// The file does not start with a `---` line.
    Missing,
    /// The frontmatter is there but broken (the reason).
    Broken(String),
}

impl std::fmt::Display for FrontmatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrontmatterError::Missing => {
                f.write_str("no frontmatter (the file must start with a `---` line)")
            }
            FrontmatterError::Broken(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for FrontmatterError {}

/// One property, or a comment / blank line (`key` empty).
#[derive(Clone, Debug)]
struct Field {
    key: String,
    value: Value,
    /// Not a value this reader understands (kept as raw text).
    complex: bool,
    /// `key:` with nothing after it so far: `- item` lines may follow.
    open_list: bool,
    /// The lines as read or rendered, line ends included.
    raw: String,
}

/// A file's frontmatter: its properties in file order.
#[derive(Clone, Debug)]
pub struct Frontmatter {
    fields: Vec<Field>,
    eol: &'static str,
}

/// Characters that make a plain scalar mean something else when they come
/// first.
const INDICATORS: &str = "-?:,[]{}#&*!|>'\"%@`";

/// Whether `text` must be quoted to read back as the same string.
pub fn needs_quotes(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.is_empty()
        || text != text.trim()
        || text.starts_with(|c: char| INDICATORS.contains(c))
        // Numbers, dates and the like: read as such by YAML.
        || text.starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '.')
        || text.contains(": ")
        || text.contains(" #")
        || text.ends_with(':')
        || text.chars().any(char::is_control)
        || matches!(
            lower.as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n" | "null" | "~"
        )
}

/// `text` in double quotes, with `\` escapes for quotes, backslashes and
/// control characters.
pub fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` as a YAML scalar: plain when that reads back the same, else
/// [`quoted`].
pub fn scalar(text: &str) -> String {
    if needs_quotes(text) {
        quoted(text)
    } else {
        text.to_string()
    }
}

/// A list item inside `[...]`: also quoted when it holds `,`, brackets or
/// braces.
fn flow_item(text: &str) -> String {
    if needs_quotes(text) || text.contains([',', '[', ']', '{', '}']) {
        quoted(text)
    } else {
        text.to_string()
    }
}

/// `items` as a flow list, `[a, b]`.
pub fn flow_list(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|i| flow_item(i)).collect();
    format!("[{}]", parts.join(", "))
}

/// Whether `date` is RFC 3339 UTC (`2026-10-25T12:32:10Z`, fractions
/// allowed), which is written plain; any other date is quoted.
pub fn is_utc_timestamp(date: &str) -> bool {
    let b = date.as_bytes();
    let digits = |r: std::ops::Range<usize>| r.into_iter().all(|i| b[i].is_ascii_digit());
    if b.len() < 20 || b[b.len() - 1] != b'Z' {
        return false;
    }
    let shape = digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'T'
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19);
    let rest = &b[19..b.len() - 1];
    shape
        && (rest.is_empty()
            || (rest[0] == b'.' && rest.len() > 1 && rest[1..].iter().all(u8::is_ascii_digit)))
}

fn trim_eol(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// A fence line (`---`, or `...` to close), trailing spaces allowed.
fn is_fence(line: &str, closing: bool) -> bool {
    let l = trim_eol(line).trim_end();
    l == "---" || (closing && l == "...")
}

/// A double-quoted scalar from just after its opening quote: the text and
/// what follows the closing quote.
fn read_double(s: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => {
                let Some((_, e)) = chars.next() else { break };
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    ' ' => out.push(' '),
                    'x' | 'u' | 'U' => {
                        let len = match e {
                            'x' => 2,
                            'u' => 4,
                            _ => 8,
                        };
                        let hex: String = chars.by_ref().take(len).map(|(_, h)| h).collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .ok()
                            .filter(|_| hex.len() == len)
                            .and_then(char::from_u32)
                            .ok_or_else(|| format!("bad escape `\\{e}{hex}`"))?;
                        out.push(code);
                    }
                    other => return Err(format!("unknown escape `\\{other}`")),
                }
            }
            c => out.push(c),
        }
    }
    Err("unclosed quote".to_string())
}

/// A single-quoted scalar from just after its opening quote (`''` is one
/// quote): the text and what follows the closing quote.
fn read_single(s: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut rest = s;
    loop {
        let Some(i) = rest.find('\'') else {
            return Err("unclosed quote".to_string());
        };
        out.push_str(&rest[..i]);
        if rest[i + 1..].starts_with('\'') {
            out.push('\'');
            rest = &rest[i + 2..];
        } else {
            return Ok((out, &rest[i + 1..]));
        }
    }
}

/// Nothing, or a comment, after a quoted scalar or a list.
fn check_after(rest: &str) -> Result<(), String> {
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('#') {
        Ok(())
    } else {
        Err(format!("unexpected `{rest}` after the value"))
    }
}

/// A plain scalar: up to a ` #` comment, trimmed.
fn plain(s: &str) -> String {
    let s = match s.find(" #").or_else(|| s.find("\t#")) {
        Some(i) => &s[..i],
        None => s,
    };
    s.trim().to_string()
}

/// One scalar (a list item or a value): quoted or plain. `in_flow` stops a
/// plain scalar at `,` and `]`. Returns the text and the rest.
fn read_scalar(s: &str, in_flow: bool) -> Result<(String, &str), String> {
    let s = s.trim_start();
    if let Some(r) = s.strip_prefix('"') {
        read_double(r)
    } else if let Some(r) = s.strip_prefix('\'') {
        read_single(r)
    } else if in_flow {
        let end = s.find([',', ']']).unwrap_or(s.len());
        Ok((s[..end].trim().to_string(), &s[end..]))
    } else {
        Ok((plain(s), ""))
    }
}

/// A flow list from just after its `[`.
fn read_flow(s: &str) -> Result<(Vec<String>, &str), String> {
    let mut items = Vec::new();
    let mut rest = s.trim_start();
    if let Some(r) = rest.strip_prefix(']') {
        return Ok((items, r));
    }
    loop {
        if rest.trim_start().starts_with(['[', '{']) {
            return Err("nested lists are not supported".to_string());
        }
        let (item, r) = read_scalar(rest, true)?;
        items.push(item);
        let r = r.trim_start();
        if let Some(r) = r.strip_prefix(',') {
            rest = r;
        } else if let Some(r) = r.strip_prefix(']') {
            return Ok((items, r));
        } else {
            return Err("unclosed `[`".to_string());
        }
    }
}

/// A property's value text (after `key:`): the value, whether it is
/// complex, whether `- item` lines may follow.
fn read_value(text: &str) -> Result<(Value, bool, bool), String> {
    let t = text.trim();
    if t.is_empty() || t.starts_with('#') {
        return Ok((Value::Text(String::new()), false, true));
    }
    match t.as_bytes()[0] {
        b'[' => {
            let (items, rest) = read_flow(&t[1..])?;
            check_after(rest)?;
            Ok((Value::List(items), false, false))
        }
        b'"' | b'\'' => {
            let (s, rest) = read_scalar(t, false)?;
            check_after(rest)?;
            Ok((Value::Text(s), false, false))
        }
        b'{' | b'|' | b'>' | b'&' | b'*' | b'!' => Ok((Value::Text(t.to_string()), true, false)),
        _ => Ok((Value::Text(plain(t)), false, false)),
    }
}

/// The key of a `key: value` line and the value text, or `None` when the
/// line is not one (no `:` followed by a space or the line end).
fn split_key(line: &str) -> Option<(&str, &str)> {
    let mut from = 0;
    while let Some(i) = line[from..].find(':') {
        let at = from + i;
        let after = &line[at + 1..];
        if after.is_empty() || after.starts_with([' ', '\t']) {
            let key = line[..at].trim();
            return (!key.is_empty()).then_some((key, after));
        }
        from = at + 1;
    }
    None
}

/// Splits a file into its frontmatter and the body after it (a leading
/// byte order mark is dropped).
///
/// # Errors
/// [`FrontmatterError::Missing`] when the file does not start with `---`;
/// [`FrontmatterError::Broken`] with the reason (and line) otherwise.
pub fn split(text: &str) -> Result<(Frontmatter, &str), FrontmatterError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    if !is_fence(first, false) {
        return Err(FrontmatterError::Missing);
    }
    let eol = if first.ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut fm = Frontmatter {
        fields: Vec::new(),
        eol,
    };
    let mut offset = first.len();
    let broken = |n: usize, why: String| FrontmatterError::Broken(format!("line {n}: {why}"));
    for (i, line) in lines.enumerate() {
        let n = i + 2;
        offset += line.len();
        if is_fence(line, true) {
            return Ok((fm, &text[offset..]));
        }
        let content = trim_eol(line);
        let trimmed = content.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            fm.fields.push(Field {
                key: String::new(),
                value: Value::Text(String::new()),
                complex: false,
                open_list: false,
                raw: line.to_string(),
            });
            continue;
        }
        let indented = content.starts_with([' ', '\t']);
        if indented || trimmed == "-" || trimmed.starts_with("- ") {
            // Blank and comment lines between a property and its nested
            // lines (a blank line in a block scalar, a comment in a list)
            // belong to that property.
            let Some(at) = fm.fields.iter().rposition(|f| !f.key.is_empty()) else {
                return Err(broken(n, format!("`{trimmed}` belongs to no property")));
            };
            let between: String = fm.fields.drain(at + 1..).map(|f| f.raw).collect();
            let last = &mut fm.fields[at];
            last.raw.push_str(&between);
            let item = if trimmed == "-" {
                Some("")
            } else {
                trimmed.strip_prefix("- ")
            };
            match item {
                Some(item) if last.open_list => {
                    let (value, rest) = read_scalar(item, false).map_err(|e| broken(n, e))?;
                    check_after(rest).map_err(|e| broken(n, e))?;
                    match &mut last.value {
                        Value::List(items) => items.push(value),
                        v => *v = Value::List(vec![value]),
                    }
                }
                // A nested map, a block scalar, a continued plain value:
                // kept as it is.
                _ => {
                    last.complex = true;
                    last.open_list = false;
                }
            }
            last.raw.push_str(line);
            continue;
        }
        let Some((key, value)) = split_key(content) else {
            return Err(broken(
                n,
                format!("`{trimmed}` is not a `key: value` property"),
            ));
        };
        if fm.fields.iter().any(|f| f.key == key) {
            return Err(broken(n, format!("property `{key}` appears twice")));
        }
        let (value, complex, open_list) = read_value(value).map_err(|e| broken(n, e))?;
        fm.fields.push(Field {
            key: key.to_string(),
            value,
            complex,
            open_list,
            raw: line.to_string(),
        });
    }
    Err(FrontmatterError::Broken(
        "the frontmatter has no closing --- line".to_string(),
    ))
}

impl Frontmatter {
    /// An empty frontmatter whose lines end with `eol`.
    pub fn new(eol: &'static str) -> Self {
        Self {
            fields: Vec::new(),
            eol,
        }
    }

    /// The line end of its lines.
    pub fn eol(&self) -> &'static str {
        self.eol
    }

    fn field(&self, key: &str) -> Option<&Field> {
        self.fields
            .iter()
            .find(|f| !f.key.is_empty() && f.key == key)
    }

    /// The property keys, in file order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.fields
            .iter()
            .filter(|f| !f.key.is_empty())
            .map(|f| f.key.as_str())
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.field(key).map(|f| &f.value)
    }

    /// Whether `key` holds something this reader keeps only as raw text (a
    /// nested map, a block scalar, ..).
    pub fn is_complex(&self, key: &str) -> bool {
        self.field(key).is_some_and(|f| f.complex)
    }

    /// The scalar value of `key` (`None` when missing or a list).
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Text(t) => Some(t),
            Value::List(_) => None,
        }
    }

    /// The list value of `key`: a list, nothing for an empty value, the one
    /// item of a scalar (`aliases: Old name`).
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        match self.get(key)? {
            Value::List(items) => Some(items.clone()),
            Value::Text(t) if t.is_empty() => Some(Vec::new()),
            Value::Text(t) => Some(vec![t.clone()]),
        }
    }

    /// Sets `key` to `value`, written as `rendered`. A property that
    /// already holds `value` keeps its lines; a changed one is rewritten in
    /// place; a new one is appended.
    pub fn set(&mut self, key: &str, value: Value, rendered: &str) {
        let raw = format!("{key}: {rendered}{}", self.eol);
        match self
            .fields
            .iter_mut()
            .find(|f| !f.key.is_empty() && f.key == key)
        {
            Some(f) if f.value == value && !f.complex => {}
            Some(f) => {
                f.value = value;
                f.complex = false;
                f.open_list = false;
                f.raw = raw;
            }
            None => self.fields.push(Field {
                key: key.to_string(),
                value,
                complex: false,
                open_list: false,
                raw,
            }),
        }
    }

    /// Sets a property written as is, like [`Frontmatter::set_plain`], but
    /// a new one goes first instead of last (an `id` given to a hand-made
    /// file).
    pub fn set_plain_first(&mut self, key: &str, text: &str) {
        if self.field(key).is_some() {
            self.set_plain(key, text);
        } else {
            self.fields.insert(
                0,
                Field {
                    key: key.to_string(),
                    value: Value::Text(text.to_string()),
                    complex: false,
                    open_list: false,
                    raw: format!("{key}: {text}{}", self.eol),
                },
            );
        }
    }

    /// Sets a text property, quoted when needed ([`scalar`]).
    pub fn set_text(&mut self, key: &str, text: &str) {
        self.set(key, Value::Text(text.to_string()), &scalar(text));
    }

    /// Sets a property written as is: a number, `true` / `false`.
    pub fn set_plain(&mut self, key: &str, text: &str) {
        self.set(key, Value::Text(text.to_string()), text);
    }

    /// Sets a date: plain when RFC 3339 UTC ([`is_utc_timestamp`]), any
    /// other text (an old free-form date, nothing) quoted.
    pub fn set_date(&mut self, key: &str, date: &str) {
        let rendered = if is_utc_timestamp(date) {
            date.to_string()
        } else {
            quoted(date)
        };
        self.set(key, Value::Text(date.to_string()), &rendered);
    }

    /// Sets a list property, written `[a, b]`.
    pub fn set_list(&mut self, key: &str, items: &[String]) {
        self.set(key, Value::List(items.to_vec()), &flow_list(items));
    }

    /// The frontmatter block, both `---` lines included.
    pub fn render(&self) -> String {
        let mut out = format!("---{}", self.eol);
        for f in &self.fields {
            out.push_str(&f.raw);
        }
        out.push_str("---");
        out.push_str(self.eol);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(text: &str) -> Frontmatter {
        split(text).unwrap().0
    }

    #[test]
    fn values_of_every_supported_kind() {
        let f = fm(
            "---\na: plain text\nb: \"dq: \\\"x\\\" \\\\ \\u00e9\\n\"\nc: 'it''s'\nd:\ne: [x, \"y, z\", 'w']\n\
                    f:\n  - one\n  - \"two\"\n- three\ng: []\nh: v # comment\npos: [1200, -3400]\n---\nbody\n",
        );
        assert_eq!(f.text("a"), Some("plain text"));
        assert_eq!(f.text("b"), Some("dq: \"x\" \\ é\n"));
        assert_eq!(f.text("c"), Some("it's"));
        assert_eq!(f.text("d"), Some(""));
        assert_eq!(f.list("d"), Some(vec![]));
        assert_eq!(
            f.list("e"),
            Some(vec!["x".into(), "y, z".into(), "w".into()])
        );
        assert_eq!(
            f.list("f"),
            Some(vec!["one".into(), "two".into(), "three".into()])
        );
        assert_eq!(f.list("g"), Some(vec![]));
        assert_eq!(f.text("h"), Some("v"));
        assert_eq!(f.list("pos"), Some(vec!["1200".into(), "-3400".into()]));
        assert_eq!(f.list("a"), Some(vec!["plain text".into()]));
    }

    #[test]
    fn complex_values_are_kept_raw() {
        let text = "---\nnested:\n  a: 1\n  b: 2\nblock: |\n  line one\n  line two\nx: 1\n---\n";
        let mut f = fm(text);
        assert!(f.is_complex("nested") && f.is_complex("block"));
        f.set_plain("x", "2");
        assert_eq!(
            f.render(),
            "---\nnested:\n  a: 1\n  b: 2\nblock: |\n  line one\n  line two\nx: 2\n---\n"
        );
    }

    #[test]
    fn blank_and_comment_lines_inside_nested_values_are_kept() {
        let text = "---\nnested:\n  a: 1\n\n  # about b\n  b: 2\nblock: |\n  one\n\n  two\n\
                    list:\n  - a\n  # not this one\n\n  - b\n# a comment of its own\nx: 1\n---\nbody\n";
        let (mut f, body) = split(text).unwrap();
        assert_eq!(body, "body\n");
        assert!(f.is_complex("nested") && f.is_complex("block"));
        assert_eq!(f.list("list"), Some(vec!["a".into(), "b".into()]));
        assert_eq!(f.text("x"), Some("1"));
        assert_eq!(format!("{}body\n", f.render()), text);
        f.set_plain("x", "2");
        assert_eq!(
            format!("{}body\n", f.render()),
            text.replace("\nx: 1\n", "\nx: 2\n")
        );
        // A blank line before the first property's nested lines too.
        let (f, _) = split("---\nk:\n\n  - v\n---\n").unwrap();
        assert_eq!(f.list("k"), Some(vec!["v".into()]));
    }

    #[test]
    fn broken_frontmatter_says_why() {
        assert_eq!(split("no fence\n").unwrap_err(), FrontmatterError::Missing);
        assert_eq!(split("").unwrap_err(), FrontmatterError::Missing);
        let why = |t: &str| split(t).unwrap_err().to_string();
        assert!(why("---\na: 1\n").contains("closing ---"));
        assert!(why("---\na: 1\nwords\n---\n").contains("line 3"));
        assert!(why("---\na: 1\na: 2\n---\n").contains("appears twice"));
        assert!(why("---\na: \"x\n---\n").contains("unclosed quote"));
        assert!(why("---\na: [x\n---\n").contains("unclosed `[`"));
        assert!(why("---\na: \"x\" y\n---\n").contains("after the value"));
        assert!(why("---\n  - x\n---\n").contains("belongs to no property"));
        assert!(why("---\na: \"\\q\"\n---\n").contains("unknown escape"));
    }

    #[test]
    fn set_keeps_unchanged_lines_and_appends_new_ones() {
        let text = "---\r\ntags:\r\n  - a\r\n# note\r\nn: 1\r\n---\r\nbody\r\n";
        let (mut f, body) = split(text).unwrap();
        assert_eq!(body, "body\r\n");
        f.set_list("tags", &["a".into()]);
        f.set_plain("n", "1");
        assert_eq!(
            f.render(),
            "---\r\ntags:\r\n  - a\r\n# note\r\nn: 1\r\n---\r\n"
        );
        f.set_plain("n", "2");
        f.set_text("t", "a: b");
        assert_eq!(
            f.render(),
            "---\r\ntags:\r\n  - a\r\n# note\r\nn: 2\r\nt: \"a: b\"\r\n---\r\n"
        );
    }

    #[test]
    fn written_values_read_back() {
        for s in [
            "plain",
            "",
            " padded ",
            "a: b",
            "#x",
            "x #y",
            "x#y",
            "-",
            "- x",
            "1",
            "1.5",
            "2026-10-25",
            "yes",
            "No",
            "~",
            "null",
            "[x]",
            "{x}",
            "\"q\"",
            "'q'",
            "back\\slash",
            "tab\tand\nnewline",
            "\u{1}",
            "é ☕",
            "50%",
            "@a",
            "`a`",
            "a:",
            "http://x.y/z",
        ] {
            let mut f = Frontmatter::new("\n");
            f.set_text("k", s);
            f.set_list("l", &[s.to_string(), "b".to_string()]);
            let text = f.render();
            let back = fm(&text);
            assert_eq!(back.text("k"), Some(s), "{text}");
            assert_eq!(
                back.list("l"),
                Some(vec![s.to_string(), "b".to_string()]),
                "{text}"
            );
        }
        assert_eq!(scalar("Found the grove"), "Found the grove");
        assert_eq!(scalar("#ID"), "\"#ID\"");
    }

    #[test]
    fn dates_are_plain_only_in_utc() {
        assert!(is_utc_timestamp("2026-10-25T12:32:10Z"));
        assert!(is_utc_timestamp("2026-10-25T12:32:10.250Z"));
        for d in [
            "",
            "2026-10-25",
            "2026-10-25T12:32:10",
            "2026-10-25T12:32:10+02:00",
            "2026-10-25T12:32:10.Z",
            "d1",
        ] {
            assert!(!is_utc_timestamp(d), "{d}");
        }
        let mut f = Frontmatter::new("\n");
        f.set_date("a", "2026-10-25T12:32:10Z");
        f.set_date("b", "2026-10-24");
        f.set_date("c", "");
        assert_eq!(
            f.render(),
            "---\na: 2026-10-25T12:32:10Z\nb: \"2026-10-24\"\nc: \"\"\n---\n"
        );
    }

    #[test]
    fn a_closing_dots_line_and_a_last_line_without_end() {
        let (f, body) = split("---\na: 1\n...\nbody").unwrap();
        assert_eq!((f.text("a"), body), (Some("1"), "body"));
        let (_, body) = split("---\na: 1\n---").unwrap();
        assert_eq!(body, "");
    }
}
