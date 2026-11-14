//! Source scans over the text the player reads (W3-26). Test-only.
//!
//! `no_debug_format_in_player_text` (acceptance G2.4): no production string
//! literal formats a value with `{:?}`. A Debug name is a Rust identifier
//! ("BossFocus", "SmallPiercing", "Deployed"), not a label; the label comes
//! from the core `label()` of the type.
//!
//! `ui_has_no_literal_camp_prices_or_mara_mapping` (acceptance G5.5, R10 and
//! the D-cluster ruling "names come from defender_card"): the UI reads camp
//! prices and defender names from core. It carries no "<N> coins" literal, no
//! coin comparison against a number, no name label "Mara" and no text that
//! picks "Mara" from a `Mode`.
//!
//! Both read `src/` as text through `architecture_tests::production_sources`,
//! so comments and test-only code never count.
use crate::architecture_tests::{
    ProductionSource, is_identifier, production_code, production_sources, source_directory,
};

/// A string literal in production code.
struct Literal {
    /// 1-based line of the opening quote.
    line: usize,
    text: String,
    /// The macro (`format`, `assert`) or call (`label`, `expect`) whose
    /// argument list directly holds the literal.
    context: Option<String>,
}

/// The identifier that ends just before `end` in `code` (skipping spaces).
fn identifier_before(code: &[char], mut end: usize) -> Option<String> {
    while end > 0 && code[end - 1].is_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_identifier(code[start - 1]) {
        start -= 1;
    }
    (start < end).then(|| code[start..end].iter().collect())
}

/// The macro or function whose brackets directly enclose position `at`.
fn enclosing_call(code: &[char], at: usize) -> Option<String> {
    let mut depth = 0;
    for index in (0..at).rev() {
        match code[index] {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' | '[' | '{' => {
                let mut end = index;
                while end > 0 && code[end - 1].is_whitespace() {
                    end -= 1;
                }
                if end > 0 && code[end - 1] == '!' {
                    end -= 1;
                }
                return identifier_before(code, end);
            }
            _ => {}
        }
    }
    None
}

/// Every string literal of `source`'s production code, in order. The blanked
/// code keeps the quotes and blanks what is between them, so a literal is the
/// original text between two consecutive quotes of the code.
fn literals(source: &ProductionSource) -> Vec<Literal> {
    let (text, code) = (&source.text, &source.code);
    assert_eq!(
        text.len(),
        code.len(),
        "{}: the blanking moved text",
        source.path
    );
    let mut found = Vec::new();
    let mut line = 1;
    let mut index = 0;
    while index < code.len() {
        match code[index] {
            '\n' => line += 1,
            '"' => {
                let open = index;
                let close = (open + 1..code.len())
                    .find(|&j| code[j] == '"')
                    .unwrap_or(code.len());
                found.push(Literal {
                    line,
                    text: text[open + 1..close].iter().collect(),
                    context: enclosing_call(code, open),
                });
                line += code[open..close.min(code.len())]
                    .iter()
                    .filter(|&&c| c == '\n')
                    .count();
                index = close;
            }
            _ => {}
        }
        index += 1;
    }
    found
}

fn source(path: &str, text: &str) -> ProductionSource {
    let (code, _) = production_code(text).expect("not a test-only file");
    ProductionSource {
        path: path.to_owned(),
        text: text.chars().collect(),
        code,
    }
}

// ---------------------------------------------------------------------------
// Debug formatting
// ---------------------------------------------------------------------------

/// Macros and calls whose text goes to a developer (a panic, the log, the
/// console), never to a widget.
const DEVELOPER_TEXT: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "panic",
    "unreachable",
    "unimplemented",
    "todo",
    "expect",
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "eprintln",
];

/// Files that write for a tool or a developer, with the reason. Everything
/// else under `src/` is treated as player text.
const NOT_PLAYER_TEXT: &[(&str, &str)] = &[
    (
        "diagnostics/",
        "the frame report names the wgpu backend and present mode for the perf scripts",
    ),
    (
        "launch.rs",
        "launch warnings quote the raw argument or variable into the log and the console",
    ),
    (
        "player_model/",
        "the model manifest keys weapon classes and skins by their Rust names",
    ),
    (
        "visual_assets",
        "manifest keys are Rust names, and asset validation errors quote the broken entry for whoever edits the assets",
    ),
    (
        "ui/review/",
        "review sidecars and review sheets are evidence for captures, not shipped screens",
    ),
];

fn formats_debug(literal: &str) -> bool {
    literal.contains(":?}") || literal.contains(":#?}")
}

/// `path:line: literal` for every Debug-formatting literal that reaches the player.
fn debug_format_hits(source: &ProductionSource) -> Vec<String> {
    if NOT_PLAYER_TEXT
        .iter()
        .any(|(prefix, _)| source.path.starts_with(prefix))
    {
        return Vec::new();
    }
    literals(source)
        .into_iter()
        .filter(|literal| formats_debug(&literal.text))
        .filter(|literal| {
            !literal
                .context
                .as_deref()
                .is_some_and(|context| DEVELOPER_TEXT.contains(&context))
        })
        .map(|literal| format!("{}:{}: \"{}\"", source.path, literal.line, literal.text))
        .collect()
}

#[test]
fn no_debug_format_in_player_text() {
    // The scan catches the shapes the UI used, and only in production text.
    for planted in [
        "fn draw(ui: &mut Ui, slot: Slot) { ui.label(format!(\"{:?}: {}\", slot, name)); }\n",
        "fn draw(ui: &mut Ui, kind: Kind) {\n    ui.selectable_value(&mut k, kind, format!(\"{kind:?}\"));\n}\n",
        "fn text(report: &Report) -> String { format!(\"{} {:#?}\", report.name, report.slot) }\n",
        // A log line built from a Debug `format!` is still counted: the
        // literal's own macro is `format!`, and the string may be shown.
        "fn note(a: A) { warn!(\"{}\", format!(\"{a:?}\")); }\n",
    ] {
        let hits = debug_format_hits(&source("ui/planted.rs", planted));
        assert_eq!(hits.len(), 1, "one hit in {planted:?}: {hits:?}");
        assert!(hits[0].starts_with("ui/planted.rs:"), "{hits:?}");
    }
    let second_line = debug_format_hits(&source(
        "ui/planted.rs",
        "fn a() -> &'static str { \"two\nlines\" }\nfn b(k: K) -> String { format!(\"{k:?}\") }\n",
    ));
    assert_eq!(second_line, ["ui/planted.rs:3: \"{k:?}\""]);
    for clean in [
        "fn draw(ui: &mut Ui, slot: Slot) { ui.label(format!(\"{}: {}\", slot.label(), name)); }\n",
        "// format!(\"{:?}\", slot)\nfn draw() {}\n",
        "fn check(a: A) { assert!(a.ok(), \"{a:?}\"); debug_assert_eq!(a.n, 1, \"{:?}\", a); }\n",
        "fn load(path: &Path) { let Ok(v) = read(path) else { panic!(\"read {path:?}\") }; }\n",
        "fn log(a: A) { warn!(\"dropped {a:?}\"); bevy::log::error!(\"{:?}\", a); }\n",
        "#[cfg(test)]\nmod tests {\n    fn t(k: K) -> String { format!(\"{k:?}\") }\n}\n",
    ] {
        let hits = debug_format_hits(&source("ui/planted.rs", clean));
        assert!(hits.is_empty(), "{clean:?} is clean: {hits:?}");
    }
    let tooling = "fn key(a: Appearance) -> String { format!(\"{a:?}\") }\n";
    for (prefix, _) in NOT_PLAYER_TEXT {
        let file = if prefix.ends_with(".rs") { "" } else { "/x.rs" };
        let path = format!("{}{file}", prefix.trim_end_matches('/'));
        assert!(
            debug_format_hits(&source(&path, tooling)).is_empty(),
            "{path}"
        );
    }
    assert_eq!(debug_format_hits(&source("worldmap.rs", tooling)).len(), 1);

    let sources = production_sources(&source_directory());
    for expected in [
        "ui/armory.rs",
        "ui/character.rs",
        "ui/operations.rs",
        "ui/history.rs",
        "ui/hud/view.rs",
        "inspector.rs",
        "worldmap.rs",
    ] {
        assert!(
            sources.iter().any(|source| source.path == expected),
            "the scan never read {expected}"
        );
    }
    for skipped in [
        "source_scan.rs",
        "ui/tests/battle_tests.rs",
        "debug_tools/mod.rs",
    ] {
        assert!(
            !sources.iter().any(|source| source.path == skipped),
            "the scan read test or dev-only {skipped}"
        );
    }
    let hits: Vec<String> = sources.iter().flat_map(debug_format_hits).collect();
    assert!(
        hits.is_empty(),
        "{} player-facing lines format a value with Debug; use the core label():\n{}",
        hits.len(),
        hits.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Camp prices and the Mara mapping
// ---------------------------------------------------------------------------

/// The screens the price and name scan covers: the shipped UI.
fn is_shipped_ui(path: &str) -> bool {
    path.starts_with("ui/") && !path.starts_with("ui/review/")
}

/// Whether `literal` spells a coin amount ("25 coins", "1,000 coins").
fn has_literal_coin_amount(literal: &str) -> bool {
    let lower = literal.to_ascii_lowercase();
    lower.match_indices(" coin").any(|(at, _)| {
        lower[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// Whether a code line compares a `..coins` value with a number other than 0
/// (`coins >= 25`, `campaign.coins() < 100`). `> 0` only asks "any at all".
fn compares_coins_with_a_number(line: &str) -> bool {
    line.match_indices("coins").any(|(at, needle)| {
        let rest = line[at + needle.len()..]
            .trim_start_matches(|c: char| is_identifier(c) || c == '(' || c == ')')
            .trim_start();
        let Some(rest) = ["<=", ">=", "==", "!=", "<", ">"]
            .iter()
            .find_map(|operator| rest.strip_prefix(operator))
        else {
            return false;
        };
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .filter(char::is_ascii_digit)
            .collect();
        digits.parse::<u64>().is_ok_and(|number| number != 0)
    })
}

/// Lines above a "Mara" text in which a `Mode` makes it a mode-to-name mapping.
const MAPPING_WINDOW: usize = 3;

fn camp_price_and_mara_hits(source: &ProductionSource) -> Vec<String> {
    if !is_shipped_ui(&source.path) {
        return Vec::new();
    }
    let code: String = source.code.iter().collect();
    let lines: Vec<&str> = code.lines().collect();
    let mut hits = Vec::new();
    let mut hit = |line: usize, what: &str, reason: &str| {
        hits.push(format!("{}:{line}: {what}: {reason}", source.path));
    };
    for (index, line) in lines.iter().enumerate() {
        if line.contains("recruit_cost") {
            hit(
                index + 1,
                "`recruit_cost`",
                "the price is Game::recruit_offer or Campaign::recruit_price",
            );
        }
        if compares_coins_with_a_number(line) {
            hit(
                index + 1,
                "coins compared with a number",
                "core decides what is affordable",
            );
        }
    }
    for literal in literals(source) {
        let quoted = format!("\"{}\"", literal.text);
        if has_literal_coin_amount(&literal.text) {
            hit(
                literal.line,
                &quoted,
                "a coin amount is formatted from the core price",
            );
        }
        let lower = literal.text.trim().to_ascii_lowercase();
        if lower == "mara" || lower == "mara vale" {
            hit(
                literal.line,
                &quoted,
                "a name label comes from core (defender_card, the npc record or the feature label)",
            );
        } else if lower.contains("mara") {
            let first = literal.line.saturating_sub(MAPPING_WINDOW + 1);
            let mapped = lines
                .iter()
                .take(literal.line)
                .skip(first)
                .any(|line| line.contains("Mode::"));
            if mapped {
                hit(
                    literal.line,
                    &quoted,
                    "Mode::Npc is not Mara: a recruit defends under it too, so the name comes from defender_card",
                );
            }
        }
    }
    hits
}

#[test]
fn ui_has_no_literal_camp_prices_or_mara_mapping() {
    for (planted, expected) in [
        (
            "fn row(ui: &mut Ui) { ui.button(\"Recruit survivor · 100 coins\"); }\n",
            "coin amount",
        ),
        (
            "fn row(ui: &mut Ui, n: u32) { ui.label(format!(\"Milestone · +1,000 coins · {n}\")); }\n",
            "coin amount",
        ),
        (
            "fn can(c: &Campaign) -> bool { c.coins >= 25 }\n",
            "coins compared",
        ),
        (
            "fn can(c: &Campaign) -> bool { c.coins() < 1_00 }\n",
            "coins compared",
        ),
        (
            "fn price(s: &StrategyState) -> u64 { s.recruit_cost }\n",
            "recruit_cost",
        ),
        (
            "fn who(a: &Attempt) -> &'static str {\n    if a.mode == Mode::Personal {\n        \"You\"\n    } else {\n        \"Mara\"\n    }\n}\n",
            "name label",
        ),
        ("fn hud() -> String { \"MARA\".to_owned() }\n", "name label"),
        (
            "fn title(mode: Mode) -> &'static str {\n    match mode {\n        Mode::Personal => \"Your defense\",\n        Mode::Npc => \"Mara's defense\",\n    }\n}\n",
            "Mode::Npc is not Mara",
        ),
    ] {
        let hits = camp_price_and_mara_hits(&source("ui/planted.rs", planted));
        assert_eq!(hits.len(), 1, "one hit in {planted:?}: {hits:?}");
        assert!(hits[0].contains(expected), "{expected} in {hits:?}");
    }
    for clean in [
        "fn row(ui: &mut Ui, price: u64) { ui.button(format!(\"Recruit survivor · {price} coins\")); }\n",
        "fn paid(r: &Rewards) -> bool { r.level_coins > 0 }\n",
        "fn can(c: &Campaign, price: u64) -> bool { c.coins >= price }\n",
        "fn copy(ui: &mut Ui) { ui.label(\"Mara joins after your introductory attempt.\"); }\n",
        "// \"Recruit survivor · 100 coins\"; Mode::Npc => \"Mara\"\nfn draw() {}\n",
        "#[cfg(test)]\nmod tests {\n    fn t() { assert_eq!(label(), \"Recruit survivor · 200 coins\"); }\n}\n",
        "fn open(f: &mut Ctx) {\n    f.mode = Mode::Npc;\n    a();\n    b();\n    c();\n    f.say(\"Mara's equipment is open.\");\n}\n",
    ] {
        let hits = camp_price_and_mara_hits(&source("ui/planted.rs", clean));
        assert!(hits.is_empty(), "{clean:?} is clean: {hits:?}");
    }
    // Outside the shipped UI the rule does not apply (review fixtures name
    // their scenes; the core owns the prices).
    let price = "fn row(ui: &mut Ui) { ui.button(\"Recruit survivor · 100 coins\"); }\n";
    assert!(camp_price_and_mara_hits(&source("ui/review/fixtures.rs", price)).is_empty());
    assert!(camp_price_and_mara_hits(&source("audio.rs", price)).is_empty());

    let sources = production_sources(&source_directory());
    for expected in [
        "ui/camp_prices.rs",
        "ui/character.rs",
        "ui/operations.rs",
        "ui/navigation.rs",
        "ui/modal.rs",
        "ui/results.rs",
        "ui/hud/view.rs",
    ] {
        assert!(
            sources.iter().any(|source| source.path == expected),
            "the scan never read {expected}"
        );
    }
    let hits: Vec<String> = sources.iter().flat_map(camp_price_and_mara_hits).collect();
    assert!(
        hits.is_empty(),
        "the UI states a camp price or maps a mode to Mara itself:\n{}",
        hits.join("\n")
    );
}
