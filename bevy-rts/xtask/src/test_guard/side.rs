//! One side (base or working tree) of a changed file, lexed whole, with the lines the diff changed marked.
//! Lexing the whole file (not a hunk) means a hunk that starts inside a multi-line or raw string cannot invert
//! the literal state, and a multi-line assert or attribute is always seen whole.

use std::collections::{BTreeMap, BTreeSet};

use super::lex::{Ch, ident_at, idents, is_code, lex, matching_close, normalized, skip_ws};

pub struct Side {
    /// Per line: whether the diff changed it (removed on the old side, added on the new).
    changed: Vec<bool>,
    pub chs: Vec<Ch>,
    /// Per line: its characters (comments removed) as `[start, end)` into `chs`, without the `'\n'`.
    bounds: Vec<(usize, usize)>,
}

/// An attribute `#[…]` or inner attribute `#![…]`.
pub struct Attr {
    /// Index of the `#`.
    pub start: usize,
    /// Index after the closing `]`.
    pub after: usize,
    pub first: usize,
    pub last: usize,
    pub inner: bool,
    /// The attribute path without whitespace (`test`, `tokio::test`, `cfg`, `cfg_attr`).
    pub path: String,
    /// The body without whitespace outside literals (`cfg(test)`, `ignore="flaky"`).
    pub body: String,
    pub idents: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Fn,
    Mod,
}

/// A `fn` or `mod` item: the index of its keyword and its name.
pub struct Item {
    pub kind: ItemKind,
    pub at: usize,
    pub name: String,
}

/// Consecutive outer attributes and the item they annotate, if it is a `fn` or `mod`.
pub struct Stack {
    pub attrs: Vec<Attr>,
    pub item: Option<Item>,
}

pub struct TestFn {
    /// The line of the test attribute, or of the `fn` for an attribute-less proptest function.
    pub line: usize,
    pub name: Option<String>,
    /// The diff changed the test attribute or the `fn` line.
    pub touched: bool,
    pub text: String,
}

/// A macro invocation `name!(…)`.
pub struct MacroCall {
    pub start: usize,
    pub close: usize,
    pub first: usize,
    pub last: usize,
    /// The invocation without whitespace outside literals: the comparison key.
    pub key: String,
}

pub fn is_test_attr(path: &str) -> bool {
    ["test", "rstest", "test_case", "proptest"]
        .iter()
        .any(|t| path == *t || path.ends_with(&format!("::{t}")))
}

impl Side {
    /// `text` lexed whole; `changed` holds the 1-based line numbers the diff changed on this side.
    pub fn new(text: &str, changed: &BTreeSet<u32>) -> Side {
        let lines: Vec<&str> = text.lines().collect();
        let chs = lex(&lines);
        let mut bounds = vec![(0, 0); lines.len()];
        let mut start = 0;
        for (i, ch) in chs.iter().enumerate() {
            if ch.c == '\n' && !ch.lit {
                bounds[ch.line] = (start, i);
                start = i + 1;
            }
        }
        let changed = (0..lines.len())
            .map(|l| changed.contains(&(l as u32 + 1)))
            .collect();
        Side {
            changed,
            chs,
            bounds,
        }
    }

    pub fn changed(&self, line: usize) -> bool {
        self.changed.get(line).copied().unwrap_or(false)
    }

    pub fn line_no(&self, line: usize) -> u32 {
        line as u32 + 1
    }

    pub fn line_chs(&self, line: usize) -> &[Ch] {
        let (s, e) = self.bounds[line];
        &self.chs[s..e]
    }

    /// Index of the first character of a line in `chs`.
    pub fn line_start(&self, line: usize) -> usize {
        self.bounds[line].0
    }

    /// The name of the function (else `proptest!` block, else `""`) around the 1-based `line`.
    pub fn owner_of_line(&self, owners: &Owners, line: u32) -> String {
        match (line as usize).checked_sub(1) {
            Some(l) if l < self.bounds.len() => owners.key(self.line_start(l)),
            _ => String::new(),
        }
    }

    /// The source of a line without comments, trimmed.
    pub fn source(&self, line: usize) -> String {
        self.line_chs(line)
            .iter()
            .map(|ch| ch.c)
            .collect::<String>()
            .trim()
            .to_owned()
    }

    /// The source of lines `first..=last`, trimmed and joined with spaces, at most 160 characters.
    pub fn source_span(&self, first: usize, last: usize) -> String {
        let text = (first..=last)
            .map(|l| self.source(l))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.chars().count() > 160 {
            format!("{}…", text.chars().take(160).collect::<String>())
        } else {
            text
        }
    }

    pub fn span_touched(&self, first: usize, last: usize) -> bool {
        (first..=last).any(|l| self.changed(l))
    }

    pub fn changed_lines(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.changed.len()).filter(|&l| self.changed(l))
    }

    /// Macro invocations whose name satisfies `pred`; nested invocations inside a match are not reported.
    pub fn macro_calls(&self, pred: impl Fn(&str) -> bool) -> Vec<MacroCall> {
        let chs = &self.chs;
        let mut out = Vec::new();
        let mut i = 0;
        while i < chs.len() {
            let Some((name, end)) = ident_at(chs, i) else {
                i += 1;
                continue;
            };
            let bang = skip_ws(chs, end);
            let open = skip_ws(chs, bang + 1);
            let is_macro = is_code(chs, bang, '!')
                && chs
                    .get(open)
                    .is_some_and(|ch| !ch.lit && matches!(ch.c, '(' | '[' | '{'));
            if !is_macro || !pred(&name) {
                i = end;
                continue;
            }
            let close = matching_close(chs, open);
            out.push(MacroCall {
                start: i,
                close,
                first: chs[i].line,
                last: chs[close].line,
                key: normalized(&chs[i..=close]),
            });
            i = close + 1;
        }
        out
    }

    /// Every attribute, outer and inner, in source order.
    pub fn attributes(&self) -> Vec<Attr> {
        let chs = &self.chs;
        let mut out = Vec::new();
        let mut i = 0;
        while i < chs.len() {
            if !is_code(chs, i, '#') {
                i += 1;
                continue;
            }
            let mut open = skip_ws(chs, i + 1);
            let inner = is_code(chs, open, '!');
            if inner {
                open = skip_ws(chs, open + 1);
            }
            if !is_code(chs, open, '[') {
                i += 1;
                continue;
            }
            let close = matching_close(chs, open);
            let body = &chs[open + 1..close.max(open + 1)];
            let path: String = body
                .iter()
                .take_while(|ch| ch.lit || !matches!(ch.c, '(' | '=' | ']'))
                .filter(|ch| !ch.c.is_whitespace())
                .map(|ch| ch.c)
                .collect();
            out.push(Attr {
                start: i,
                after: close + 1,
                first: chs[i].line,
                last: chs[close].line,
                inner,
                path,
                body: normalized(body),
                idents: idents(body),
            });
            i = close + 1;
        }
        out
    }

    /// The `fn` or `mod` item at `i`, after whitespace, attributes and qualifiers.
    pub fn item_after(&self, mut i: usize) -> Option<Item> {
        let chs = &self.chs;
        loop {
            i = skip_ws(chs, i);
            if is_code(chs, i, '#') {
                let open = skip_ws(chs, i + 1);
                if !is_code(chs, open, '[') {
                    return None;
                }
                i = matching_close(chs, open) + 1;
                continue;
            }
            let (word, end) = ident_at(chs, i)?;
            match word.as_str() {
                "pub" => {
                    i = skip_ws(chs, end);
                    if is_code(chs, i, '(') {
                        i = matching_close(chs, i) + 1;
                    }
                }
                "async" | "const" | "unsafe" => i = end,
                "extern" => {
                    i = skip_ws(chs, end);
                    while chs.get(i).is_some_and(|ch| ch.lit) {
                        i += 1;
                    }
                }
                "fn" | "mod" => {
                    let (name, _) = ident_at(chs, skip_ws(chs, end))?;
                    let kind = if word == "fn" {
                        ItemKind::Fn
                    } else {
                        ItemKind::Mod
                    };
                    return Some(Item { kind, at: i, name });
                }
                _ => return None,
            }
        }
    }

    /// Outer attributes grouped into stacks, each with the item it annotates.
    pub fn stacks(&self) -> Vec<Stack> {
        let mut out: Vec<Stack> = Vec::new();
        let mut cur: Vec<Attr> = Vec::new();
        for attr in self.attributes().into_iter().filter(|a| !a.inner) {
            if let Some(prev) = cur.last()
                && skip_ws(&self.chs, prev.after) != attr.start
            {
                out.push(self.close_stack(std::mem::take(&mut cur)));
            }
            cur.push(attr);
        }
        if !cur.is_empty() {
            out.push(self.close_stack(cur));
        }
        out
    }

    fn close_stack(&self, attrs: Vec<Attr>) -> Stack {
        let item = attrs.last().and_then(|a| self.item_after(a.after));
        Stack { attrs, item }
    }

    /// Test functions: `#[test]`-like attributed ones, and proptest ones (`fn f(x in strategy)`).
    pub fn tests(&self) -> Vec<TestFn> {
        let chs = &self.chs;
        let mut out = Vec::new();
        let mut seen_fns = BTreeSet::new();
        for stack in self.stacks() {
            let Some(attr) = stack.attrs.iter().find(|a| is_test_attr(&a.path)) else {
                continue;
            };
            let item = stack.item.filter(|i| i.kind == ItemKind::Fn);
            let fn_line = item.as_ref().map(|i| chs[i.at].line);
            if let Some(i) = &item {
                seen_fns.insert(i.at);
            }
            out.push(TestFn {
                line: attr.first,
                name: item.map(|i| i.name),
                touched: self.span_touched(attr.first, attr.last)
                    || fn_line.is_some_and(|l| self.changed(l)),
                text: match fn_line {
                    Some(l) => self.source_span(attr.first, l),
                    None => self.source_span(attr.first, attr.last),
                },
            });
        }
        for f in self.fns() {
            if seen_fns.contains(&f.at) || !f.proptest_params {
                continue;
            }
            let line = chs[f.at].line;
            out.push(TestFn {
                line,
                name: Some(f.name),
                touched: self.changed(line),
                text: self.source(line),
            });
        }
        out.sort_by_key(|t| t.line);
        out
    }

    /// Every `fn` with its body span.
    fn fns(&self) -> Vec<FnSpan> {
        let chs = &self.chs;
        let mut out = Vec::new();
        let mut i = 0;
        while i < chs.len() {
            let Some((word, end)) = ident_at(chs, i) else {
                i += 1;
                continue;
            };
            if word != "fn" {
                i = end;
                continue;
            }
            let Some((name, name_end)) = ident_at(chs, skip_ws(chs, end)) else {
                i = end;
                continue;
            };
            let mut open = skip_ws(chs, name_end);
            if is_code(chs, open, '<') {
                open = (open..chs.len())
                    .find(|&j| is_code(chs, j, '('))
                    .unwrap_or(chs.len());
            }
            if !is_code(chs, open, '(') {
                i = name_end;
                continue;
            }
            let params_close = matching_close(chs, open);
            let proptest_params = idents(&chs[open..=params_close]).iter().any(|w| w == "in");
            let body = (params_close..chs.len())
                .find(|&j| is_code(chs, j, '{') || is_code(chs, j, ';'))
                .unwrap_or(chs.len().saturating_sub(1));
            let end = if is_code(chs, body, '{') {
                matching_close(chs, body)
            } else {
                body
            };
            out.push(FnSpan {
                at: i,
                end,
                name,
                proptest_params,
            });
            i = name_end;
        }
        out
    }

    /// Owner keys for case counts: per character range, the enclosing function (including its attributes) or
    /// `proptest!` block (keyed by its first function).
    pub fn owners(&self) -> Owners {
        let mut stack_start: BTreeMap<usize, usize> = BTreeMap::new();
        for s in self.stacks() {
            if let (Some(item), Some(first)) = (&s.item, s.attrs.first()) {
                stack_start.insert(item.at, first.start);
            }
        }
        let fns = self.fns();
        let mut regions: Vec<(usize, usize, String)> = fns
            .iter()
            .map(|f| {
                let start = stack_start.get(&f.at).copied().unwrap_or(f.at);
                (start, f.end, f.name.clone())
            })
            .collect();
        let blocks: Vec<(usize, usize, String)> = self
            .macro_calls(|n| n == "proptest")
            .into_iter()
            .map(|m| {
                let first = fns
                    .iter()
                    .find(|f| f.at > m.start && f.at < m.close)
                    .map_or_else(|| format!("proptest@{}", m.first + 1), |f| f.name.clone());
                (m.start, m.close, first)
            })
            .collect();
        regions.sort_by_key(|r| (r.1 - r.0, r.0));
        Owners {
            fns: regions,
            blocks,
        }
    }
}

struct FnSpan {
    at: usize,
    end: usize,
    name: String,
    proptest_params: bool,
}

pub struct Owners {
    /// `(start, end, name)`, smallest first.
    fns: Vec<(usize, usize, String)>,
    blocks: Vec<(usize, usize, String)>,
}

impl Owners {
    /// The innermost function around `i`, else the `proptest!` block around it, else the file (`""`).
    pub fn key(&self, i: usize) -> String {
        let inside = |r: &&(usize, usize, String)| r.0 <= i && i <= r.1;
        self.fns
            .iter()
            .find(inside)
            .or_else(|| self.blocks.iter().find(inside))
            .map(|r| r.2.clone())
            .unwrap_or_default()
    }
}
