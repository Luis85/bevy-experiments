//! Single-line patterns for test-guard: tolerances, case counts, seeds, mocks, test gates and path classes.

use std::path::Path;

use super::lex::{Ch, ident_at, idents, is_code, skip_ws};
use crate::owners;

/// Splits `snake_case` and `CamelCase` into lowercase words.
pub fn words(ident: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in ident.chars() {
        if c == '_' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Words that name a statistical or numeric tolerance.
const LOOSENESS_WORDS: &[&str] = &[
    "alpha",
    "tol",
    "tolerance",
    "eps",
    "epsilon",
    "ulps",
    "rtol",
    "atol",
    "threshold",
    "margin",
    "slack",
    "delta",
    "significance",
    "confidence",
];
/// Multi-word tolerance names, matched inside the lowercase identifier.
const LOOSENESS_FRAGMENTS: &[&str] = &["max_err", "maxerr", "p_value", "pvalue"];

fn is_tolerance_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    words(name)
        .iter()
        .any(|w| LOOSENESS_WORDS.contains(&w.as_str()))
        || LOOSENESS_FRAGMENTS.iter().any(|f| lower.contains(f))
}

/// A value worth tracking: it has a digit (`1e-4`, `Fx::from_num(3)`) or is a plain path (`f64::EPSILON`,
/// `DEFAULT_ALPHA`). Expressions like `target - pos` are not tolerances.
fn is_tolerance_value(v: &str) -> bool {
    !v.is_empty()
        && !v.contains('"')
        && (v.chars().any(|c| c.is_ascii_digit())
            || v.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':'))
}

/// `NAME = value`, `NAME: Type = value` or `NAME: <number>` on a line, for a tolerance-like `NAME`:
/// `(name, value)` pairs.
pub fn tolerance_bindings(chs: &[Ch]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chs.len() {
        let Some((name, end)) = ident_at(chs, i) else {
            i += 1;
            continue;
        };
        i = end;
        if !is_tolerance_name(&name) {
            continue;
        }
        let rest: String = chs[end..]
            .iter()
            .map(|ch| if ch.lit { '"' } else { ch.c })
            .collect();
        let rest = rest.trim_start();
        let assign = |s: &str| -> Option<String> {
            let at = s.char_indices().find(|&(k, c)| {
                c == '='
                    && !s[k + 1..].starts_with(['=', '>'])
                    && !s[..k].ends_with(['=', '<', '>', '!'])
            })?;
            let value = s[at.0 + 1..].split([';', ',']).next()?.trim();
            Some(value.split_whitespace().collect())
        };
        let value = if rest.starts_with('=') {
            assign(rest)
        } else if let Some(after) = rest.strip_prefix(':').filter(|r| !r.starts_with(':')) {
            let head = after.split([';', ',', '}']).next().unwrap_or("");
            if head.contains('=') {
                assign(after)
            } else {
                let v: String = head.split_whitespace().collect();
                v.chars().any(|c| c.is_ascii_digit()).then_some(v)
            }
        } else {
            None
        };
        if let Some(v) = value.filter(|v| is_tolerance_value(v)) {
            out.push((name, v));
        }
    }
    out
}

/// `cases: 256`, `cases = 256` or `with_cases(256)` on a line: the case count.
pub fn proptest_cases(chs: &[Ch]) -> Option<u64> {
    let mut i = 0;
    while i < chs.len() {
        let Some((name, end)) = ident_at(chs, i) else {
            i += 1;
            continue;
        };
        i = end;
        if name != "cases" && name != "with_cases" {
            continue;
        }
        let sep = skip_ws(chs, end);
        let is_sep = chs
            .get(sep)
            .is_some_and(|ch| !ch.lit && matches!(ch.c, ':' | '=' | '('))
            && !is_code(chs, sep + 1, ':')
            && !is_code(chs, sep + 1, '=');
        if !is_sep {
            continue;
        }
        let digits: String = chs[skip_ws(chs, sep + 1)..]
            .iter()
            .take_while(|ch| !ch.lit && (ch.c.is_ascii_digit() || ch.c == '_'))
            .filter(|ch| ch.c != '_')
            .map(|ch| ch.c)
            .collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    None
}

/// A literal seed on a line: an identifier containing `seed` followed by `(`, `:` or `=` and a number
/// (`MatchSeed(7)`, `seed: 7`, `let seed = 7`).
pub fn has_seed_literal(chs: &[Ch]) -> bool {
    let mut i = 0;
    while i < chs.len() {
        let Some((name, end)) = ident_at(chs, i) else {
            i += 1;
            continue;
        };
        i = end;
        if !name.to_ascii_lowercase().contains("seed") {
            continue;
        }
        let sep = skip_ws(chs, end);
        let is_sep = chs
            .get(sep)
            .is_some_and(|ch| !ch.lit && matches!(ch.c, '(' | ':' | '='))
            && !is_code(chs, sep + 1, ':')
            && !is_code(chs, sep + 1, '=');
        let value = skip_ws(chs, sep + 1);
        if is_sep
            && chs
                .get(value)
                .is_some_and(|ch| !ch.lit && ch.c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Identifier fragments of proptest, shuffle and seeded-RNG overrides.
const SEED_OVERRIDE_IDENTS: &[&str] = &[
    "rng_seed",
    "rngseed",
    "shuffle_seed",
    "from_seed",
    "seed_from",
    "with_seed",
    "new_with_rng",
];
/// Environment variables that pin seeds.
pub const SEED_VARS: &[&str] = &["FL_SHUFFLE_SEEDS", "PROPTEST_RNG_SEED"];
/// Environment variables that set the proptest case count.
pub const CASES_VARS: &[&str] = &["PROPTEST_CASES"];

pub fn has_seed_override_ident(chs: &[Ch]) -> bool {
    idents(chs).iter().any(|id| {
        let id = id.to_ascii_lowercase();
        SEED_OVERRIDE_IDENTS.iter().any(|s| id.contains(s))
    })
}

/// The line sets one of `vars` (`.env(…)`, `.envs(…)` or `set_var(…)` with the name in a literal).
pub fn sets_env_var(chs: &[Ch], vars: &[&str]) -> bool {
    let literals: String = chs.iter().filter(|ch| ch.lit).map(|ch| ch.c).collect();
    let code: String = chs.iter().filter(|ch| !ch.lit).map(|ch| ch.c).collect();
    let sets = code.contains(".env(")
        || code.contains(".envs(")
        || idents(chs).iter().any(|i| i == "set_var");
    sets && vars.iter().any(|v| literals.contains(v))
}

/// Sim crates, where tests use the real `build_sim_app` and no mocks (spec §12.4).
pub fn is_sim_path(path: &Path) -> bool {
    let p = owners::normalize(path);
    let parts: Vec<&str> = p.split('/').collect();
    if parts.iter().any(|c| c.ends_with("_client")) {
        return false;
    }
    p.starts_with("features/")
        || parts
            .iter()
            .any(|c| *c == "fl_sim" || *c == "fl_test_utils" || c.starts_with("fl_feat_"))
}

pub fn is_client_path(path: &Path) -> bool {
    owners::normalize(path)
        .split('/')
        .any(|c| c == "fl_game" || c.ends_with("_client"))
}

pub fn is_in_tests_dir(path: &Path) -> bool {
    owners::normalize(path).split('/').any(|c| c == "tests")
}

pub fn is_mock_ident(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    ["mock", "fake", "stub"].iter().any(|m| id.contains(m))
}

/// A `mod name;` declaration of tests, or a `cfg(test)` gate.
pub fn is_test_gate(chs: &[Ch], path: &Path) -> bool {
    let code: String = chs
        .iter()
        .filter(|ch| !ch.lit && !ch.c.is_whitespace())
        .map(|ch| ch.c)
        .collect();
    if code.contains("cfg(test)") {
        return true;
    }
    let ids = idents(chs);
    code.ends_with(';')
        && ids.iter().position(|w| w == "mod").is_some_and(|k| {
            ids.get(k + 1)
                .is_some_and(|name| is_in_tests_dir(path) || name.contains("test"))
        })
}

/// The crate directory of a source path (the part before `src/`, `tests/`, `benches/` or `examples/`), so a test
/// moved within its crate cancels out but one moved to another crate does not.
pub fn crate_key(path: &Path) -> String {
    let p = owners::normalize(path);
    let parts: Vec<&str> = p.split('/').collect();
    match parts
        .iter()
        .position(|c| matches!(*c, "src" | "tests" | "benches" | "examples"))
    {
        Some(k) => parts[..k].join("/"),
        None => parts[..parts.len().saturating_sub(1)].join("/"),
    }
}
