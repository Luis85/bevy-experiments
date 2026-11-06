//! Architecture checks that read source files, not game state. Test-only.
//!
//! `presentation_never_reads_core_rng` keeps the frontend off the simulation's
//! random streams. horde_game renders what horde_core decided; audio and VFX
//! variation is keyed by `SimEventRecord.seq` or a frontend counter. A frontend
//! read of `Attempt.rng`, a keyed roll or an extra `Game::tick` would let a
//! renderer move GOLDEN and GAMEPLAY_TRACE.
//!
//! The scan reads `src/` as text. Comments and string contents are blanked and
//! test-only code (`#[cfg(test)]` items and the modules they declare) is skipped,
//! so a needle in docs, UI copy or a test does not count.
use std::path::{Path, PathBuf};

/// Dev-only replay tooling re-runs recordings through the core by design.
const EXCLUDED: &[&str] = &["debug_tools/"];

struct Rule {
    needle: &'static str,
    /// `/`-separated path prefixes below `src/` where the needle is allowed.
    allowed: &'static [&'static str],
    reason: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        needle: ".rng",
        allowed: &[],
        reason: "the core RNG streams (Attempt.rng, Campaign.rng) belong to horde_core",
    },
    Rule {
        needle: "Rng::",
        allowed: &[],
        reason: "the core Rng is never built or called by presentation",
    },
    Rule {
        needle: "hash::keyed",
        allowed: &[],
        reason: "crate::hash keyed rolls decide outcomes and are core-only",
    },
    Rule {
        needle: "keyed::",
        allowed: &[],
        reason: "keyed rolls decide outcomes and are core-only",
    },
    Rule {
        needle: "catalog().roll(",
        allowed: &[],
        reason: "quality rolls are core-only; show the rolled Quality instead",
    },
    Rule {
        needle: ".tick(",
        allowed: &[
            "runner/",
            "ui/review/fixtures.rs",
            "ui/review/fixtures/production.rs",
            "ui/review/matrix.rs",
        ],
        reason: "only the runner advances the simulation (review fixtures stage their own isolated Game before it is shown)",
    },
];

const TEST_ATTRIBUTE: &str = "#[cfg(test)]";
const TEST_FILE_ATTRIBUTE: &str = "#![cfg(test)]";

/// What one source file contributes to the scan.
#[derive(Debug, Default)]
struct Scanned {
    /// `path:line: needle: reason` for every forbidden needle in non-test code.
    hits: Vec<String>,
    /// Modules declared `#[cfg(test)] mod name;`, whose files are skipped.
    test_modules: Vec<String>,
    /// The whole file is `#![cfg(test)]`.
    test_only: bool,
}

fn source_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn is_identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `text` with comments and string or char literal contents replaced by spaces.
/// Newlines are kept, so line numbers survive; lifetimes stay code.
fn blank_comments_and_strings(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                let pair = (chars[i], chars.get(i + 1).copied());
                if pair == ('/', Some('*')) || pair == ('*', Some('/')) {
                    depth += if pair.0 == '/' { 1 } else { -1 };
                    out.extend([' ', ' ']);
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"..", r#".."#, br"..".
        let starts_word = i == 0
            || !is_identifier(chars[i - 1])
            || (chars[i - 1] == 'b' && (i == 1 || !is_identifier(chars[i - 2])));
        if c == 'r' && starts_word {
            let mut j = i + 1;
            while chars.get(j) == Some(&'#') {
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                let hashes = j - i - 1;
                out.extend(&chars[i..=j]);
                i = j + 1;
                while i < chars.len() {
                    let closes = chars[i] == '"'
                        && (1..=hashes).all(|offset| chars.get(i + offset) == Some(&'#'));
                    if closes {
                        out.extend(&chars[i..=i + hashes]);
                        i += hashes + 1;
                        break;
                    }
                    out.push(blank(chars[i]));
                    i += 1;
                }
                continue;
            }
        }
        if c == '"' {
            out.push('"');
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' => {
                        out.push(' ');
                        if let Some(&escaped) = chars.get(i + 1) {
                            out.push(blank(escaped));
                        }
                        i += 2;
                    }
                    '"' => {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    other => {
                        out.push(blank(other));
                        i += 1;
                    }
                }
            }
            continue;
        }
        if c == '\'' {
            // A char literal ('x', '\n', '\u{..}'); otherwise a lifetime or label.
            let end = if next == Some('\\') {
                (i + 2..chars.len().min(i + 12)).find(|&j| chars[j] == '\'')
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(end) = end {
                out.push('\'');
                out.extend(std::iter::repeat_n(' ', end - i - 1));
                out.push('\'');
                i = end + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The index just past the `]` matching the `[` at `open`.
fn skip_brackets(chars: &[char], open: usize) -> usize {
    let mut depth = 0;
    for (index, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    chars.len()
}

/// `name` when `item` is an out-of-line module declaration (`pub mod name;`).
fn module_declaration(item: &str) -> Option<String> {
    let mut rest = item.trim();
    for visibility in ["pub(crate) ", "pub(super) ", "pub "] {
        rest = rest.strip_prefix(visibility).unwrap_or(rest).trim_start();
    }
    let name = rest.strip_prefix("mod ")?.strip_suffix(';')?.trim();
    (!name.is_empty() && name.chars().all(is_identifier)).then(|| name.to_owned())
}

/// Blanks every `#[cfg(test)]` item in `code` (attribute, further attributes and
/// the item up to its closing `}`, `;` or `,`) and returns the declared test modules.
fn remove_test_items(code: &mut [char]) -> Vec<String> {
    let attribute: Vec<char> = TEST_ATTRIBUTE.chars().collect();
    let mut modules = Vec::new();
    let mut i = 0;
    while i + attribute.len() <= code.len() {
        if code[i..i + attribute.len()] != attribute[..] {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + attribute.len();
        loop {
            while j < code.len() && code[j].is_whitespace() {
                j += 1;
            }
            if code.get(j) == Some(&'#') && code.get(j + 1) == Some(&'[') {
                j = skip_brackets(code, j + 1);
            } else {
                break;
            }
        }
        let item_start = j;
        let mut depth = 0;
        while j < code.len() {
            match code[j] {
                '(' | '[' | '{' => depth += 1,
                // The enclosing block closes: the item ended just before.
                ')' | ']' | '}' if depth == 0 => break,
                ')' | ']' => depth -= 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                ';' | ',' if depth == 0 => {
                    j += 1;
                    break;
                }
                _ => {}
            }
            j += 1;
        }
        let item: String = code[item_start..j].iter().collect();
        modules.extend(module_declaration(&item));
        for c in &mut code[start..j] {
            if *c != '\n' {
                *c = ' ';
            }
        }
        i = j;
    }
    modules
}

/// Scans one file's text; `path` is `/`-separated and relative to `src/`.
fn scan_source(path: &str, text: &str) -> Scanned {
    let mut code = blank_comments_and_strings(text);
    let flat: String = code.iter().collect();
    if flat.contains(TEST_FILE_ATTRIBUTE) {
        return Scanned {
            test_only: true,
            ..Scanned::default()
        };
    }
    let test_modules = remove_test_items(&mut code);
    let code: String = code.into_iter().collect();
    let mut hits = Vec::new();
    for (index, line) in code.lines().enumerate() {
        for rule in RULES {
            let allowed = rule.allowed.iter().any(|prefix| path.starts_with(prefix));
            if !allowed && line.contains(rule.needle) {
                hits.push(format!(
                    "{path}:{}: `{}`: {}",
                    index + 1,
                    rule.needle,
                    rule.reason
                ));
            }
        }
    }
    Scanned {
        hits,
        test_modules,
        test_only: false,
    }
}

/// Every `.rs` below `directory`, as `/`-separated paths relative to it, sorted.
fn rust_sources(directory: &Path) -> Vec<String> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<String>) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let relative = path.strip_prefix(root).expect("below the root");
                let parts: Vec<String> = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.push(parts.join("/"));
            }
        }
    }
    let mut found = Vec::new();
    walk(directory, directory, &mut found);
    found.sort();
    found
}

/// Where the children of the module in `path` live, with a trailing `/` ("" at the root).
fn child_directory(path: &str) -> String {
    let (parent, file) = path.rsplit_once('/').unwrap_or(("", path));
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{parent}/")
    };
    match file {
        "main.rs" | "lib.rs" | "mod.rs" => prefix,
        _ => format!("{prefix}{}/", file.trim_end_matches(".rs")),
    }
}

/// The scan of `directory`: hits, and the files it actually read.
fn scan_tree(directory: &Path) -> (Vec<String>, Vec<String>) {
    let sources: Vec<(String, Scanned)> = rust_sources(directory)
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(directory.join(&path))
                .unwrap_or_else(|error| panic!("read {path}: {error}"));
            let scanned = scan_source(&path, &text);
            (path, scanned)
        })
        .collect();
    // A test module's file and every file below it are test-only.
    let mut test_files = Vec::new();
    let mut test_directories = Vec::new();
    for (path, scanned) in &sources {
        let base = child_directory(path);
        for name in &scanned.test_modules {
            test_files.push(format!("{base}{name}.rs"));
            test_directories.push(format!("{base}{name}/"));
        }
    }
    let mut hits = Vec::new();
    let mut read = Vec::new();
    for (path, scanned) in sources {
        let skipped = scanned.test_only
            || test_files.contains(&path)
            || test_directories
                .iter()
                .any(|directory| path.starts_with(directory.as_str()))
            || EXCLUDED.iter().any(|prefix| path.starts_with(prefix));
        if !skipped {
            hits.extend(scanned.hits);
            read.push(path);
        }
    }
    (hits, read)
}

/// The planted strings the scanner must catch, so a clean scan means something.
fn assert_scanner_catches_planted_reads() {
    let planted = [
        (
            "planted.rs",
            "fn draw(a: &Attempt) -> u32 { a.rng.below(3) }\n",
            ".rng",
        ),
        (
            "planted.rs",
            "let roll = crate::hash::keyed(seed, a, b, TAG);\n",
            "hash::keyed",
        ),
    ];
    for (path, text, needle) in planted {
        let hits = scan_source(path, text).hits;
        assert!(
            hits.iter()
                .any(|hit| hit.starts_with("planted.rs:1:") && hit.contains(needle)),
            "the scanner missed a planted `{needle}` in {text:?}: {hits:?}"
        );
    }
}

#[test]
fn presentation_never_reads_core_rng() {
    assert_scanner_catches_planted_reads();
    let (hits, read) = scan_tree(&source_directory());
    for expected in [
        "main.rs",
        "battle_view.rs",
        "runner/runtime.rs",
        "ui/hud.rs",
    ] {
        assert!(
            read.iter().any(|path| path == expected),
            "the scan never read {expected}: {read:?}"
        );
    }
    for skipped in [
        "architecture_tests.rs",
        "debug_tools/replay.rs",
        "ui/tests/battle_tests.rs",
    ] {
        assert!(
            !read.iter().any(|path| path == skipped),
            "the scan read test or dev-only {skipped}"
        );
    }
    assert!(
        hits.is_empty(),
        "presentation reaches core randomness or the simulation step:\n{}",
        hits.join("\n")
    );
}

#[test]
fn scanner_ignores_comments_strings_and_test_code() {
    let clean = "// a.rng.below(3) in a comment\n\
                 /* hash::keyed( in a block\n   comment */\n\
                 let label = \"a.rng and Rng:: are words here {\";\n\
                 let raw = r#\"game.tick(\" \"#;\n\
                 let brace = '{';\n\
                 fn f<'a>(x: &'a str) {}\n\
                 #[cfg(test)]\n\
                 mod tests {\n    fn t() { game.tick(Input::default()); let r = a.rng.below(2); }\n}\n\
                 #[cfg(test)]\n\
                 #[allow(dead_code)]\n\
                 fn helper() -> u32 { Rng::new(1).below(4) }\n\
                 #[cfg(test)]\n\
                 mod fixtures;\n\
                 #[cfg(test)] pub(crate) mod support;\n";
    let scanned = scan_source("clean.rs", clean);
    assert_eq!(scanned.hits, Vec::<String>::new());
    assert_eq!(scanned.test_modules, ["fixtures", "support"]);
    assert!(!scanned.test_only);

    let after =
        "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn draw(a: &Attempt) { a.rng.below(2); }\n";
    let hits = scan_source("after.rs", after).hits;
    assert!(
        hits.iter().any(|hit| hit.starts_with("after.rs:5: `.rng`")),
        "code after a test module is still scanned: {hits:?}"
    );

    let tick = "fn step(game: &mut Game) { game.tick(Input::default()); }\n";
    assert_eq!(
        scan_source("runner/runtime.rs", tick).hits,
        Vec::<String>::new()
    );
    let hits = scan_source("battle_view.rs", tick).hits;
    assert!(
        hits.iter()
            .any(|hit| hit.starts_with("battle_view.rs:1: `.tick(`")),
        "presentation may not step the simulation: {hits:?}"
    );
    assert!(scan_source("x.rs", "#![cfg(test)]\nfn t() {}\n").test_only);
}

#[test]
fn test_module_files_resolve_like_rustc() {
    assert_eq!(child_directory("main.rs"), "");
    assert_eq!(child_directory("ui.rs"), "ui/");
    assert_eq!(child_directory("ui/review/mod.rs"), "ui/review/");
    assert_eq!(child_directory("ui/hud.rs"), "ui/hud/");
}
