//! A small Rust lexer for test-guard: drops comments and marks string and char literals (raw strings
//! included), so `// assert!(x)` is not an assert and `"#[ignore]"` is not an attribute.

/// A source character outside comments, flagged `true` inside a string or char literal.
#[derive(Clone, Copy)]
pub struct Ch {
    pub c: char,
    pub lit: bool,
    pub line: usize,
}

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `'x'`, `'\n'`, `'\u{1F600}'` → its length in chars; a lifetime or label → `None`.
fn char_literal_len(cs: &[char]) -> Option<usize> {
    if cs.get(1) == Some(&'\\') {
        return cs
            .iter()
            .skip(2)
            .take(10)
            .position(|&c| c == '\'')
            .map(|p| p + 3);
    }
    (cs.len() >= 3 && cs[2] == '\'' && cs[1] != '\'').then_some(3)
}

/// Source lines as one character stream without comments; every line ends in a `'\n'`.
pub fn lex(lines: &[&str]) -> Vec<Ch> {
    #[derive(Clone, Copy)]
    enum St {
        Code,
        Block(u32),
        Str,
        Raw(usize),
    }
    let mut st = St::Code;
    let mut out = Vec::new();
    for (line, text) in lines.iter().enumerate() {
        let cs: Vec<char> = text.chars().collect();
        let mut push = |c: char, lit: bool| out.push(Ch { c, lit, line });
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            let next = cs.get(i + 1).copied();
            match st {
                St::Block(depth) => {
                    if c == '*' && next == Some('/') {
                        st = if depth == 1 {
                            St::Code
                        } else {
                            St::Block(depth - 1)
                        };
                        i += 2;
                    } else if c == '/' && next == Some('*') {
                        st = St::Block(depth + 1);
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                St::Str => {
                    push(c, true);
                    i += 1;
                    if c == '\\' {
                        if let Some(n) = next {
                            push(n, true);
                        }
                        i += 1;
                    } else if c == '"' {
                        st = St::Code;
                    }
                }
                St::Raw(hashes) => {
                    push(c, true);
                    i += 1;
                    if c == '"'
                        && cs[i..].iter().take(hashes).filter(|&&h| h == '#').count() == hashes
                    {
                        for _ in 0..hashes {
                            push('#', true);
                        }
                        i += hashes;
                        st = St::Code;
                    }
                }
                St::Code => {
                    if c == '/' && next == Some('/') {
                        break;
                    }
                    if c == '/' && next == Some('*') {
                        st = St::Block(1);
                        i += 2;
                        continue;
                    }
                    if c == '"' {
                        push(c, true);
                        st = St::Str;
                        i += 1;
                        continue;
                    }
                    if c == 'r' && (i == 0 || !is_ident_char(cs[i - 1])) {
                        let hashes = cs[i + 1..].iter().take_while(|&&h| h == '#').count();
                        if cs.get(i + 1 + hashes) == Some(&'"') {
                            for &ch in &cs[i..i + 2 + hashes] {
                                push(ch, true);
                            }
                            i += 2 + hashes;
                            st = St::Raw(hashes);
                            continue;
                        }
                    }
                    if c == '\''
                        && let Some(len) = char_literal_len(&cs[i..])
                    {
                        for &ch in &cs[i..i + len] {
                            push(ch, true);
                        }
                        i += len;
                        continue;
                    }
                    push(c, false);
                    i += 1;
                }
            }
        }
        out.push(Ch {
            c: '\n',
            lit: false,
            line,
        });
    }
    out
}

pub fn is_code(chs: &[Ch], i: usize, c: char) -> bool {
    chs.get(i).is_some_and(|ch| !ch.lit && ch.c == c)
}

pub fn skip_ws(chs: &[Ch], mut i: usize) -> usize {
    while chs.get(i).is_some_and(|ch| !ch.lit && ch.c.is_whitespace()) {
        i += 1;
    }
    i
}

/// The identifier starting at `i` (which must not continue an identifier) and the index after it.
pub fn ident_at(chs: &[Ch], i: usize) -> Option<(String, usize)> {
    let starts = chs
        .get(i)
        .is_some_and(|ch| !ch.lit && (ch.c.is_alphabetic() || ch.c == '_'));
    let continues = i > 0 && !chs[i - 1].lit && is_ident_char(chs[i - 1].c);
    if !starts || continues {
        return None;
    }
    let end = (i..chs.len())
        .find(|&j| chs[j].lit || !is_ident_char(chs[j].c))
        .unwrap_or(chs.len());
    Some((chs[i..end].iter().map(|ch| ch.c).collect(), end))
}

/// The index of the bracket closing the one at `open`, or the last index if it never closes in view.
pub fn matching_close(chs: &[Ch], open: usize) -> usize {
    let mut depth = 0i32;
    for (j, ch) in chs.iter().enumerate().skip(open) {
        if ch.lit {
            continue;
        }
        match ch.c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    chs.len().saturating_sub(1)
}

/// Identifiers outside literals in `chs`.
pub fn idents(chs: &[Ch]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chs.len() {
        if let Some((id, end)) = ident_at(chs, i) {
            out.push(id);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// `chs` without whitespace outside literals: the comparison key of a statement.
pub fn normalized(chs: &[Ch]) -> String {
    chs.iter()
        .filter(|ch| ch.lit || !ch.c.is_whitespace())
        .map(|ch| ch.c)
        .collect()
}
