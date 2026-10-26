//! Readable, safe file names for notes entries (notes-as-Obsidian-files spec
//! O2): the entry's title, made safe for Obsidian and Windows. Pure: no I/O.

/// Characters dropped from a name: Windows forbids most, Obsidian treats
/// `#`, `^`, `[` and `]` as link syntax.
pub const FORBIDDEN: [char; 13] = [
    '*', '"', '\\', '/', '<', '>', ':', '|', '?', '#', '^', '[', ']',
];

/// Longest name made from a title, in characters (before a diary's date,
/// a ` (2)` and `.md`).
pub const NAME_MAX: usize = 120;

/// Windows device names: `CON`, `NUL.txt`, `COM0`..`COM9`, `COM¹`..`COM³`
/// (and `LPT` alike), `CONIN$`, `CONOUT$` cannot be file names (any case).
fn reserved(stem: &str) -> bool {
    let s = stem.to_lowercase();
    if matches!(
        s.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) {
        return true;
    }
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let rest: Vec<char> = chars.collect();
    (prefix == "com" || prefix == "lpt")
        && matches!(rest.as_slice(), [c] if c.is_ascii_digit() || matches!(c, '¹' | '²' | '³'))
}

fn trim_ends(s: &str) -> &str {
    s.trim_matches(|c: char| c == ' ' || c == '.')
}

/// `title` as a file name (without `.md`): the [`FORBIDDEN`] characters
/// and other control characters are dropped, runs of whitespace become one
/// space, spaces and dots at either end are trimmed, the name is cut to
/// [`NAME_MAX`] characters, and a Windows reserved name gets a `_` after it
/// (`CON_`, `nul_.txt`). Empty when nothing is left: the store then names
/// the file by kind and id (`Memory 12`).
pub fn safe_name(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut space = false;
    for c in title.chars() {
        if c.is_whitespace() {
            space = true;
        } else if !(FORBIDDEN.contains(&c) || c.is_control()) {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    let mut name = trim_ends(&out).to_string();
    if name.chars().count() > NAME_MAX {
        name = name.chars().take(NAME_MAX).collect();
        name = trim_ends(&name).to_string();
    }
    let stem_end = name.find('.').unwrap_or(name.len());
    if reserved(&name[..stem_end]) {
        name.insert(stem_end, '_');
    }
    name
}

/// The `YYYY-MM-DD` a date starts with (the UTC day of an RFC 3339 UTC
/// date), if it does.
pub fn day_of(date: &str) -> Option<&str> {
    let b = date.as_bytes();
    let ok = b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b.get(10).is_none_or(|c| !c.is_ascii_digit());
    ok.then(|| &date[..10])
}

/// `title` without its trailing `.md`s (any case, spaces and dots between
/// them too), so a file name never ends in `.md.md`.
fn without_md(title: &str) -> &str {
    let mut t = title;
    loop {
        let trimmed = trim_ends(t);
        match trimmed.len().checked_sub(3) {
            Some(i) if trimmed.is_char_boundary(i) && trimmed[i..].eq_ignore_ascii_case(".md") => {
                t = &trimmed[..i];
            }
            _ => return t,
        }
    }
}

/// An entry's file name without `.md` or a number: `safe_name(title)` (a
/// trailing `.md` of the title dropped first), or `<kind> <id>` when that is
/// empty, after `YYYY-MM-DD ` (the created day) when `day` is given (the
/// diary).
pub fn entry_name(kind: &str, id: u32, title: &str, day: Option<&str>) -> String {
    // Made safe first, so dropped characters cannot leave a `.md` at the end
    // (`a.md?`); a reserved stem left after the cut still gets its `_`.
    let mut name = safe_name(without_md(&safe_name(title)));
    if name.is_empty() {
        name = format!("{kind} {id}");
    }
    match day {
        Some(d) => format!("{d} {name}"),
        None => name,
    }
}

/// The `n`th file name for `base`: `base.md`, then `base (2).md`, ..
pub fn numbered(base: &str, n: u32) -> String {
    if n <= 1 {
        format!("{base}.md")
    } else {
        format!("{base} ({n}).md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_and_names() {
        assert_eq!(day_of("2026-10-25T12:00:00Z"), Some("2026-10-25"));
        assert_eq!(day_of("2026-10-25"), Some("2026-10-25"));
        assert_eq!(day_of("2026-10-251"), None);
        assert_eq!(day_of("d1"), None);
        assert_eq!(day_of(""), None);
        assert_eq!(
            entry_name("Diary", 3, "Grove: north", Some("2026-10-25")),
            "2026-10-25 Grove north"
        );
        assert_eq!(entry_name("Memory", 12, "???", None), "Memory 12");
        // Never `.md.md`.
        assert_eq!(
            entry_name("Diary", 1, "Kept from diary.md", None),
            "Kept from diary"
        );
        assert_eq!(entry_name("Memory", 2, "notes.MD ", None), "notes");
        assert_eq!(entry_name("Memory", 3, ".md", None), "md");
        assert_eq!(entry_name("Memory", 4, "a.md.md", None), "a");
        assert_eq!(entry_name("Memory", 5, "a.md .md.", None), "a");
        assert_eq!(entry_name("Memory", 6, "a.mdx", None), "a.mdx");
        assert_eq!(entry_name("Memory", 7, "a.md?", None), "a");
        assert_eq!(entry_name("Memory", 8, "CON.md", None), "CON_");
        assert_eq!(numbered("Grove", 1), "Grove.md");
        assert_eq!(numbered("Grove", 3), "Grove (3).md");
    }
}
