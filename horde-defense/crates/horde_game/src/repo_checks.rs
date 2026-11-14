//! Repository checks that read files, not game state. Test-only.
//!
//! `powershell_scripts_are_51_safe` keeps every script under `scripts/` runnable
//! by the Windows PowerShell 5.1 that ships with Windows: no PowerShell 7 or
//! .NET Core APIs, and native processes only through `scripts/lib/Native.ps1`
//! (an `&` call on a native exe that writes stderr raises NativeCommandError
//! under 5.1 as soon as the caller redirects streams).
//!
//! `docs_invoke_scripts_with_windows_powershell` keeps the documented commands
//! runnable the same way: no Markdown outside `docs/plans/` tells a reader to run
//! `pwsh ... -File scripts/<x>.ps1`, except a PowerShell 7 allowlisted script
//! whose surrounding prose carries [`PS7_DOC_NOTE`]. The root helper
//! (`-File ../scripts/dev.ps1`) is outside this workspace and is not checked.
//!
//! `root_helper_line_is_unannotated` pins that root helper line (ISO-1): README.md
//! and docs/VALIDATION.md each carry it once, byte-identical to 3d492cd.
//!
//! `production_fixtures_stage_features_through_commands` keeps the production
//! review recipes honest (ISO-5, design-qa-captures 1.8): in
//! `ui/review/fixtures/production.rs` the staging seams appear only inside
//! `entitle_*` helpers, so its recipes stage features through `Game::apply`.
//! It scans that file only: `prepare_operations_observer` reuses the shared
//! `fixtures.rs` operations roster (`prepare_progression`,
//! `prepare_operations`), which stages through `staging()` and
//! `deploy_survivor`, as design-qa-captures' operations-observer row allows.
//!
//! `required_docs_sections_exist` is the docs inventory (design-qa-captures
//! 1.13, ruling W-15): every listed doc carries the section headings its
//! owner writes. It checks presence only. `docs_drop_retired_loot_claims`
//! and `docs_drop_retired_progression_claims` keep the claims the production
//! pass made false out of `docs/` (W7-04, W7-05).
//!
//! `playtest_guide_covers_the_production_loop` keeps the playtest guide's
//! loop sections (Loop, Escape, Auto-sell, Takeover, Listening pass,
//! Click-to-photon) and what they must say. `acceptance_ledger_lists_every_production_pass_row`
//! keeps docs/ACCEPTANCE.md listing every criterion of the production-pass
//! acceptance matrix, with each function its Proof names defined in a file
//! its Defined in column lists, and `acceptance_lists_every_pending_human_check`
//! keeps its pending human checks equal to the plan's (W7-08).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// PowerShell 7 only, by decision; their port is a logged follow-up.
const PS7_ALLOWLIST: &[&str] = &[
    "capture_player_models.ps1",
    "trace_npc.ps1",
    "verify_player_models.ps1",
];

/// Not yet ported, with the commit that ports each and removes it from here.
const PENDING: &[(&str, &str)] = &[];

/// Lower-case needles searched on comment-stripped lines, with the reason.
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "getrelativepath",
        "[IO.Path]::GetRelativePath is .NET Core only",
    ),
    (
        "*>",
        "an all-stream redirect of a native exe raises NativeCommandError; use Invoke-Native logs",
    ),
    (
        "compress-archive",
        "Compress-Archive writes backslash entries under 5.1",
    ),
    ("pwsh", "PowerShell 7 is not part of the toolchain"),
    (
        "$scenes = @(",
        "scene names come from the binary (--list-scenes), not a script array",
    ),
];

const REQUIRES: &str = "#requires -version 5.1";
const ZIP_TYPE: &str = "io.compression.zipfile]";
/// The assembly that holds `ZipFile` on .NET Framework 4.x (PowerShell 5.1);
/// `System.IO.Compression` alone has only `ZipArchive`.
const ZIP_ASSEMBLY: &str = "system.io.compression.filesystem";
const LIBRARY: &str = "lib/native.ps1";

/// Native tools a script must not start directly (any `*.exe` word counts too).
const NATIVE_TOOLS: &[&str] = &[
    "cargo",
    "cmd",
    "ffmpeg",
    "git",
    "horde-defense",
    "powercfg",
    "powershell",
    "py",
    "python",
    "robocopy",
    "rustc",
    "rustup",
    "tar",
    "taskkill",
    "xcopy",
];

const NATIVE_REASON: &str =
    "start native processes through Invoke-Native or Invoke-Cargo (lib/Native.ps1)";

fn scripts_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts")
}

/// Every `.ps1` below `directory`, as `/`-separated paths relative to it, sorted.
fn powershell_scripts(directory: &Path) -> Vec<String> {
    fn walk(root: &Path, directory: &Path, found: &mut Vec<String>) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
            {
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

/// Code with comments removed, one entry per source line (1-based numbers).
/// `#` outside a quoted string starts a line comment; `<# ... #>` may span lines.
/// Quote state restarts on every line, so here-string bodies are read as code.
fn code_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut in_block = false;
    for (index, line) in text.lines().enumerate() {
        let characters: Vec<char> = line.chars().collect();
        let mut code = String::new();
        let mut quote: Option<char> = None;
        let mut i = 0;
        while i < characters.len() {
            let c = characters[i];
            let next = characters.get(i + 1).copied();
            if in_block {
                if c == '#' && next == Some('>') {
                    in_block = false;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            match quote {
                Some(open) => {
                    code.push(c);
                    if open == '"' && c == '`' {
                        if let Some(escaped) = next {
                            code.push(escaped);
                            i += 1;
                        }
                    } else if c == open {
                        quote = None;
                    }
                }
                None if c == '<' && next == Some('#') => {
                    in_block = true;
                    i += 2;
                    continue;
                }
                None if c == '#' => break,
                None => {
                    if c == '\'' || c == '"' {
                        quote = Some(c);
                    }
                    code.push(c);
                }
            }
            i += 1;
        }
        lines.push((index + 1, code));
    }
    lines
}

/// `code` (one comment-free line) with every quoted string's contents blanked
/// to spaces, keeping the quotes, so text inside a string is never read as a call.
fn blank_strings(code: &str) -> String {
    let mut blanked = String::with_capacity(code.len());
    let mut quote: Option<char> = None;
    let mut characters = code.chars();
    while let Some(c) = characters.next() {
        match quote {
            Some(open) if c == open => {
                quote = None;
                blanked.push(c);
            }
            Some(open) => {
                blanked.push(' ');
                if open == '"' && c == '`' && characters.next().is_some() {
                    blanked.push(' ');
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                }
                blanked.push(c);
            }
        }
    }
    blanked
}

/// The leading run of `characters` that `allowed` accepts.
fn leading_word(characters: &[char], allowed: impl Fn(char) -> bool) -> String {
    characters.iter().take_while(|&&c| allowed(c)).collect()
}

fn is_variable_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | ':')
}

/// Variables `blanked` declares as scriptblocks: `[scriptblock]$name` or `$name = {`.
fn scriptblock_variables(blanked: &[(usize, String)]) -> Vec<String> {
    let mut names = Vec::new();
    for (_, code) in blanked {
        let characters: Vec<char> = code.chars().collect();
        for (index, &c) in characters.iter().enumerate() {
            if c != '$' {
                continue;
            }
            let name = leading_word(&characters[index + 1..], is_variable_character);
            if name.is_empty() {
                continue;
            }
            let typed = characters[..index]
                .iter()
                .collect::<String>()
                .ends_with("[scriptblock]");
            let rest: String = characters[index + 1 + name.chars().count()..]
                .iter()
                .collect();
            let rest = rest.trim_start();
            let assigned = rest.strip_prefix('=').is_some_and(|value| {
                !value.starts_with('=') && value.trim_start().starts_with('{')
            });
            if typed || assigned {
                names.push(name);
            }
        }
    }
    names
}

/// Every direct native start in `blanked` (lower-cased, string-blanked code):
/// a call operator on anything but a scriptblock (`& cargo`, `& 'x.exe'`,
/// `& $exe`, `& (Resolve-Tool x)`), or a [`NATIVE_TOOLS`] or `*.exe` word at
/// the start of a command (`cargo build`, `$v = git rev-parse HEAD`).
fn native_calls(name: &str, blanked: &[(usize, String)]) -> Vec<String> {
    let scriptblocks = scriptblock_variables(blanked);
    let mut violations = Vec::new();
    for (line, code) in blanked {
        let characters: Vec<char> = code.chars().collect();
        for (index, &c) in characters.iter().enumerate() {
            let previous = index.checked_sub(1).map(|i| characters[i]);
            let starts_command = previous
                .is_none_or(|p| p.is_whitespace() || matches!(p, '(' | '{' | ';' | '|' | '='));
            if c == '&' && starts_command && characters.get(index + 1) != Some(&'&') {
                let rest = &characters[index + 1..];
                let skip = rest.iter().take_while(|c| c.is_whitespace()).count();
                let target = &rest[skip..];
                let problem = match target.first().copied() {
                    None | Some('{') => None,
                    Some('$') => {
                        let variable = leading_word(&target[1..], is_variable_character);
                        (variable.is_empty() || !scriptblocks.contains(&variable)).then(|| {
                            format!(
                                "`& ${variable}`: not a scriptblock in this script; {NATIVE_REASON}"
                            )
                        })
                    }
                    Some(quote) if quote == '\'' || quote == '"' => {
                        Some(format!("`& {quote}...{quote}`: {NATIVE_REASON}"))
                    }
                    Some('(') => Some(format!("`& (...)`: {NATIVE_REASON}")),
                    Some(_) => {
                        let word = leading_word(target, |c| {
                            !c.is_whitespace() && !matches!(c, '(' | ')' | ';' | '|')
                        });
                        Some(format!("`& {word}`: {NATIVE_REASON}"))
                    }
                };
                if let Some(problem) = problem {
                    violations.push(format!("{name}:{line}: {problem}"));
                }
                continue;
            }
            let from = if matches!(c, '(' | '{' | ';' | '|' | '=') {
                if c == '=' && characters.get(index + 1) == Some(&'=') {
                    continue;
                }
                index + 1
            } else if index == characters.iter().take_while(|c| c.is_whitespace()).count() {
                index
            } else {
                continue;
            };
            let rest = &characters[from..];
            let skip = rest.iter().take_while(|c| c.is_whitespace()).count();
            let word = leading_word(&rest[skip..], |c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
            });
            let tool = word.strip_suffix(".exe").unwrap_or(&word);
            let native =
                NATIVE_TOOLS.contains(&tool) || (word.ends_with(".exe") && !tool.is_empty());
            let followed = rest[skip + word.chars().count()..]
                .first()
                .is_none_or(|c| c.is_whitespace() || matches!(c, ')' | '}' | ';' | '|'));
            if native && followed {
                violations.push(format!(
                    "{name}:{line}: `{word}` starts a command: {NATIVE_REASON}"
                ));
            }
        }
    }
    violations
}

/// Every 5.1 rule `name` breaks, as `name:line: reason` (line 0 for whole-file rules).
fn lint_script(name: &str, bytes: &[u8]) -> Vec<String> {
    let mut violations = Vec::new();
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        violations.push(format!("{name}:1: starts with a UTF-8 BOM"));
    }
    if let Some(position) = bytes.iter().position(|byte| !byte.is_ascii()) {
        let line = bytes[..position].iter().filter(|&&b| b == b'\n').count() + 1;
        violations.push(format!(
            "{name}:{line}: non-ASCII byte (5.1 reads BOM-less scripts as ANSI)"
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let first = text.lines().next().unwrap_or_default();
    if !first
        .trim()
        .trim_start_matches('\u{feff}')
        .eq_ignore_ascii_case(REQUIRES)
    {
        violations.push(format!(
            "{name}:1: the first line must be `#requires -Version 5.1`"
        ));
    }
    let code = code_lines(&text);
    let lowered: Vec<(usize, String)> = code
        .iter()
        .map(|(line, code)| (*line, code.to_ascii_lowercase().replace('\\', "/")))
        .collect();
    let mut zip_assembly_loaded = false;
    for (line, code) in &lowered {
        for (needle, reason) in FORBIDDEN {
            if code.contains(needle) {
                violations.push(format!("{name}:{line}: `{needle}`: {reason}"));
            }
        }
        if code.contains("add-type") && code.contains(ZIP_ASSEMBLY) {
            zip_assembly_loaded = true;
        }
        if code.contains(ZIP_TYPE) && !zip_assembly_loaded {
            violations.push(format!(
                "{name}:{line}: [IO.Compression.ZipFile] needs an earlier `Add-Type -AssemblyName System.IO.Compression.FileSystem`"
            ));
        }
    }
    let blanked: Vec<(usize, String)> = lowered
        .iter()
        .map(|(line, code)| (*line, blank_strings(code)))
        .collect();
    violations.extend(native_calls(name, &blanked));
    let runs_native = lowered.iter().any(|(_, code)| {
        let trimmed = code.trim_start();
        ["invoke-native", "invoke-cargo", "start-process", ".exe"]
            .iter()
            .any(|needle| code.contains(needle))
            || trimmed.starts_with("& ")
            || trimmed.starts_with("&'")
            || trimmed.starts_with("&\"")
    });
    let is_library = name.eq_ignore_ascii_case(LIBRARY);
    let names_library = lowered.iter().any(|(_, code)| code.contains(LIBRARY));
    let dot_sources = lowered
        .iter()
        .any(|(_, code)| code.trim_start().starts_with(". "));
    if runs_native && !is_library && !(names_library && dot_sources) {
        violations.push(format!(
            "{name}:0: runs a native process but does not dot-source lib/Native.ps1"
        ));
    }
    violations
}

fn is_listed(name: &str) -> bool {
    PS7_ALLOWLIST.contains(&name) || PENDING.iter().any(|(pending, _)| *pending == name)
}

#[test]
fn powershell_scripts_are_51_safe() {
    let directory = scripts_directory();
    let scripts = powershell_scripts(&directory);
    assert!(
        scripts
            .iter()
            .any(|name| name.eq_ignore_ascii_case(LIBRARY)),
        "scripts/lib/Native.ps1 is missing: {scripts:?}"
    );
    let mut violations = Vec::new();
    for name in scripts.iter().filter(|name| !is_listed(name)) {
        let bytes = std::fs::read(directory.join(name)).expect("read script");
        violations.extend(lint_script(name, &bytes));
    }
    assert!(
        violations.is_empty(),
        "scripts that break Windows PowerShell 5.1 rules:\n{}",
        violations.join("\n")
    );
}

#[test]
fn pending_list_only_names_existing_unported_scripts() {
    let directory = scripts_directory();
    let scripts = powershell_scripts(&directory);
    for (name, commit) in PENDING {
        assert!(
            scripts.iter().any(|script| script == name),
            "PENDING names {name}, which does not exist; remove it"
        );
        assert!(
            !PS7_ALLOWLIST.contains(name),
            "{name} is both pending and allowlisted"
        );
        let bytes = std::fs::read(directory.join(name)).expect("read script");
        assert!(
            !lint_script(name, &bytes).is_empty(),
            "{name} is 5.1-safe now; {commit} must remove it from PENDING"
        );
    }
    for name in PS7_ALLOWLIST {
        assert!(
            scripts.iter().any(|script| script == name),
            "the PowerShell 7 allowlist names {name}, which does not exist; remove it"
        );
    }
}

#[test]
fn lint_reports_each_rule_and_ignores_comments() {
    let clean = "#requires -Version 5.1\n. (Join-Path $PSScriptRoot 'lib\\Native.ps1')\n\
                 # & cargo build and pwsh in a comment are fine\n\
                 <# block: Compress-Archive\n *> #>\n\
                 $r = Invoke-Native -FilePath $exe -LogStem 'x#y' # trailing *>\n\
                 function Run([scriptblock]$Body) { & $Body }\n\
                 $check = { param($x) $x }\n\
                 $null = & $check 1; & { 'x' } | Out-Null\n\
                 $c = Invoke-Native -FilePath (Resolve-Tool 'cmd') -Arguments @('/c', 'echo a & cargo b') -LogStem 'c'\n\
                 Write-Output \"git status\"; $both = $a -and $b\n";
    assert_eq!(
        lint_script("clean.ps1", clean.as_bytes()),
        Vec::<String>::new()
    );

    let cases: &[(&str, &str)] = &[
        ("x = 1\n", "the first line must be"),
        ("#requires -Version 5.1\n& cargo build\n", "`& cargo`"),
        (
            "#requires -Version 5.1\n& './target/debug/x.exe'\n",
            "`& '...'`",
        ),
        // Dot-sourcing the library is not enough: every native start goes through it.
        (
            "#requires -Version 5.1\n. (Join-Path $PSScriptRoot 'lib\\Native.ps1')\n\
             $exe = Get-HordeExecutable\n& $exe --list-scenes\n",
            "`& $exe`",
        ),
        (
            "#requires -Version 5.1\n. (Join-Path $PSScriptRoot 'lib\\Native.ps1')\ncargo build --locked\n",
            "`cargo` starts a command",
        ),
        (
            "#requires -Version 5.1\n. (Join-Path $PSScriptRoot 'lib\\Native.ps1')\nif ($x) { git status }\n",
            "`git` starts a command",
        ),
        (
            "#requires -Version 5.1\n$head = git rev-parse HEAD\n",
            "`git` starts a command",
        ),
        (
            "#requires -Version 5.1\n$t = @(& powercfg.exe /getactivescheme) -join ' '\n",
            "`& powercfg.exe`",
        ),
        (
            "#requires -Version 5.1\n& (Resolve-Tool 'cargo') build\n",
            "`& (...)`",
        ),
        (
            "#requires -Version 5.1\n$p = tool.exe /x\n",
            "`tool.exe` starts a command",
        ),
        ("#requires -Version 5.1\n$x = 'a' *> log\n", "`*>`"),
        (
            "#requires -Version 5.1\nCompress-Archive -Path a\n",
            "compress-archive",
        ),
        ("#requires -Version 5.1\n$p = 'pwsh'\n", "`pwsh`"),
        (
            "#requires -Version 5.1\n[IO.Path]::GetRelativePath($a, $b)\n",
            "getrelativepath",
        ),
        (
            "#requires -Version 5.1\n$Scenes = @('menu')\n",
            "--list-scenes",
        ),
        (
            "#requires -Version 5.1\n[IO.Compression.ZipFile]::OpenRead($p)\n",
            "Add-Type",
        ),
        (
            // ZipFile lives in System.IO.Compression.FileSystem on .NET Framework 4.x.
            "#requires -Version 5.1\nAdd-Type -AssemblyName System.IO.Compression\n\
             [IO.Compression.ZipFile]::OpenRead($p)\n",
            "System.IO.Compression.FileSystem",
        ),
        (
            "#requires -Version 5.1\nStart-Process cmd\n",
            "dot-source lib/Native.ps1",
        ),
        ("#requires -Version 5.1\n$t = 'caf\u{e9}'\n", "non-ASCII"),
        ("\u{feff}#requires -Version 5.1\n", "BOM"),
    ];
    for (script, expected) in cases {
        let violations = lint_script("case.ps1", script.as_bytes());
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expected)),
            "{script:?} should report {expected:?}, got {violations:?}"
        );
    }

    let zip_after_assembly = "#requires -Version 5.1\n\
                              Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem\n\
                              $a = [IO.Compression.ZipFile]::OpenRead($p)\n";
    assert_eq!(
        lint_script("zip.ps1", zip_after_assembly.as_bytes()),
        Vec::<String>::new()
    );
    assert_eq!(
        lint_script(
            "lib/Native.ps1",
            b"#requires -Version 5.1\n$p = Start-Process x\n"
        ),
        Vec::<String>::new(),
        "the library itself need not dot-source itself"
    );
}

/// The note that must sit beside a documented PowerShell 7 allowlisted command.
const PS7_DOC_NOTE: &str = "requires PowerShell 7; not ported in the production pass";

/// The documented form of a workspace script command.
const DOC_COMMAND: &str = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/";

fn workspace_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Folders the doc lint never enters, by `/`-separated workspace path:
/// `docs/plans/` (history) and the untracked local folders from `.gitignore`
/// (`target/`, `saves/`, `screenshots/` and every dot-folder such as `.git/`,
/// `.obsidian/` or `.claude/`), so files outside version control cannot fail it.
fn skipped_directory(name: &str) -> bool {
    matches!(name, "target" | "saves" | "screenshots" | "docs/plans")
        || name
            .rsplit('/')
            .next()
            .is_some_and(|folder| folder.starts_with('.'))
}

/// Every `.md` in the workspace outside [`skipped_directory`] folders, as
/// `/`-separated paths relative to the workspace, sorted.
fn markdown_files(workspace: &Path) -> Vec<String> {
    workspace_files(workspace, workspace, &["md"])
}

/// Every file below `directory` with one of `extensions` (case-insensitive),
/// outside [`skipped_directory`] folders, as `/`-separated paths relative to
/// `workspace`, sorted.
fn workspace_files(workspace: &Path, directory: &Path, extensions: &[&str]) -> Vec<String> {
    fn walk(root: &Path, directory: &Path, extensions: &[&str], found: &mut Vec<String>) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            let relative = path.strip_prefix(root).expect("below the root");
            let parts: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            let name = parts.join("/");
            if path.is_dir() {
                if !skipped_directory(&name) {
                    walk(root, &path, extensions, found);
                }
            } else if path.extension().is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|wanted| extension.eq_ignore_ascii_case(wanted))
            }) {
                found.push(name);
            }
        }
    }
    let mut found = Vec::new();
    walk(workspace, directory, extensions, &mut found);
    found.sort();
    found
}

/// Lower-cased, whitespace-collapsed text, so a wrapped note still matches.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Every `pwsh ... -File scripts/<x>.ps1` line in `text`, as `name:line: reason`.
/// A line inside a fenced block is excused only for a PowerShell 7 allowlisted
/// script when the prose paragraph just before the fence carries the note; a
/// line outside a fence needs the note on the line itself.
fn lint_doc(name: &str, text: &str) -> Vec<String> {
    let note = normalized(PS7_DOC_NOTE);
    let mut violations = Vec::new();
    let mut in_fence = false;
    let mut fence_noted = false;
    let mut paragraph = String::new();
    let mut last_paragraph = String::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if !in_fence {
                if !paragraph.is_empty() {
                    last_paragraph = std::mem::take(&mut paragraph);
                }
                fence_noted = normalized(&last_paragraph).contains(&note);
            }
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if trimmed.is_empty() {
                if !paragraph.is_empty() {
                    last_paragraph = std::mem::take(&mut paragraph);
                }
            } else {
                paragraph.push(' ');
                paragraph.push_str(line);
            }
        }
        let lowered = line.to_ascii_lowercase().replace('\\', "/");
        if !lowered.contains("pwsh") {
            continue;
        }
        let Some(start) = lowered.find("-file scripts/") else {
            continue;
        };
        let script: String = lowered[start + "-file scripts/".len()..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '`' | '"' | '\''))
            .collect();
        let allowlisted = PS7_ALLOWLIST
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&script));
        let noted = if in_fence {
            fence_noted
        } else {
            normalized(line).contains(&note)
        };
        if !(allowlisted && noted) {
            let fix = if allowlisted {
                format!("PowerShell 7 only, so say it {PS7_DOC_NOTE}")
            } else {
                format!("write `{DOC_COMMAND}{script}`")
            };
            violations.push(format!(
                "{name}:{}: `pwsh` for scripts/{script}; {fix}",
                index + 1
            ));
        }
    }
    violations
}

/// [`lint_doc`] on the workspace file `name`; a file that is not UTF-8 is
/// itself a violation.
fn lint_markdown_file(workspace: &Path, name: &str) -> Vec<String> {
    let path = workspace.join(name);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    match String::from_utf8(bytes) {
        Ok(text) => lint_doc(name, &text),
        Err(_) => vec![format!("{name}: not UTF-8; save it as UTF-8")],
    }
}

#[test]
fn docs_invoke_scripts_with_windows_powershell() {
    let workspace = workspace_directory();
    let files = markdown_files(&workspace);
    assert!(
        files.iter().any(|name| name == "README.md"),
        "the workspace README is missing: {files:?}"
    );
    assert!(
        !files.iter().any(|name| name.starts_with("docs/plans/")),
        "docs/plans/ records history and is not checked"
    );
    let mut violations = Vec::new();
    for name in &files {
        violations.extend(lint_markdown_file(&workspace, name));
    }
    assert!(
        violations.is_empty(),
        "documented commands that need PowerShell 7:\n{}",
        violations.join("\n")
    );
}

#[test]
fn doc_lint_ignores_the_root_helper_and_honours_the_ps7_note() {
    let clean = "Run it:\n\n```powershell\n\
                 powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_review.ps1\n\
                 pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense\n\
                 ```\n\n\
                 This checker requires PowerShell 7;\nnot ported in the production pass.\n\n\
                 ```powershell\n\
                 pwsh -NoProfile -File scripts/verify_player_models.ps1 `\n  -Capture a.png\n\
                 pwsh -NoProfile -File scripts\\capture_player_models.ps1 -BindOnly\n\
                 ```\n\n\
                 Inline: `pwsh -File scripts/trace_npc.ps1` (requires PowerShell 7; not ported in the production pass).\n";
    assert_eq!(lint_doc("doc.md", clean), Vec::<String>::new());

    let cases: &[(&str, &str)] = &[
        (
            "```powershell\npwsh -NoProfile -File scripts/capture_review.ps1\n```\n",
            "doc.md:2: `pwsh` for scripts/capture_review.ps1; write `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture_review.ps1`",
        ),
        (
            "Requires PowerShell 7; not ported in the production pass.\n\n\
             ```powershell\npwsh -NoProfile -File scripts/package_playtest.ps1\n```\n",
            "scripts/package_playtest.ps1; write",
        ),
        (
            "```powershell\npwsh -NoProfile -File scripts/verify_player_models.ps1\n```\n",
            "verify_player_models.ps1; PowerShell 7 only, so say it requires PowerShell 7",
        ),
        (
            "Requires PowerShell 7; not ported in the production pass.\n\n\
             ```powershell\nx\n```\n\nRecheck:\n\n\
             ```powershell\npwsh -NoProfile -File scripts/verify_player_models.ps1\n```\n",
            "doc.md:10:",
        ),
        (
            "Run `pwsh -NoProfile -File scripts/trace_npc.ps1` here.\n",
            "doc.md:1:",
        ),
    ];
    for (doc, expected) in cases {
        let violations = lint_doc("doc.md", doc);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expected)),
            "{doc:?} should report {expected:?}, got {violations:?}"
        );
    }
}

/// The root helper line, byte-identical to 3d492cd in README.md and
/// docs/VALIDATION.md (ruling R6/S16, acceptance ISO-1): never annotated.
const ROOT_HELPER_LINE: &str =
    "pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense";

/// Every way `text` breaks ISO-1, as `name[:line]: reason`: a line that names
/// `../scripts/dev.ps1` but is not exactly [`ROOT_HELPER_LINE`], or a count of
/// exact lines other than one.
fn root_helper_violations(name: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut exact = 0;
    for (index, line) in text.lines().enumerate() {
        if line == ROOT_HELPER_LINE {
            exact += 1;
        } else if line.contains("../scripts/dev.ps1") {
            violations.push(format!(
                "{name}:{}: the root dev.ps1 line is changed",
                index + 1
            ));
        }
    }
    if exact != 1 {
        violations.push(format!(
            "{name}: the root dev.ps1 line appears {exact} times"
        ));
    }
    violations
}

#[test]
fn root_helper_line_is_unannotated() {
    let workspace = workspace_directory();
    for name in ["README.md", "docs/VALIDATION.md"] {
        let path = workspace.join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert_eq!(
            root_helper_violations(name, &text),
            Vec::<String>::new(),
            "ISO-1: {name} must keep the root dev.ps1 line byte-identical"
        );
    }

    let annotated = format!("```\n{ROOT_HELPER_LINE} # requires PowerShell 7\n```\n");
    let twice = format!("{ROOT_HELPER_LINE}\n{ROOT_HELPER_LINE}\n");
    let trailing = format!("{ROOT_HELPER_LINE} \n");
    let cases: &[(&str, &str)] = &[
        (
            "# Nothing\n",
            "doc.md: the root dev.ps1 line appears 0 times",
        ),
        (&annotated, "doc.md:2: the root dev.ps1 line is changed"),
        (&twice, "doc.md: the root dev.ps1 line appears 2 times"),
        (&trailing, "doc.md:1: the root dev.ps1 line is changed"),
    ];
    for (doc, expected) in cases {
        let violations = root_helper_violations("doc.md", doc);
        assert!(
            violations.iter().any(|violation| violation == expected),
            "{doc:?} should report {expected:?}, got {violations:?}"
        );
    }
    assert_eq!(
        root_helper_violations("doc.md", &format!("```\r\n{ROOT_HELPER_LINE}\r\n```\r\n")),
        Vec::<String>::new()
    );
}

#[test]
fn doc_lint_skips_ignored_folders_and_reports_non_utf8() {
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let fixture =
        Fixture(std::env::temp_dir().join(format!("horde-doc-lint-{}", std::process::id())));
    let workspace = fixture.0.clone();
    let _ = std::fs::remove_dir_all(&workspace);
    const PWSH: &[u8] = b"pwsh -File scripts/x.ps1\n";
    let files: &[(&str, &[u8])] = &[
        ("README.md", b"# Readme\n"),
        ("docs/bad.md", b"caf\xe9\n"),
        ("docs/plans/old.md", PWSH),
        ("target/debug/x.md", PWSH),
        ("saves/notes.md", PWSH),
        ("screenshots/notes.md", PWSH),
        (".obsidian/x.md", PWSH),
        (".impeccable/x.md", PWSH),
        ("docs/.notes/x.md", PWSH),
    ];
    for (name, bytes) in files {
        let path = workspace.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create fixture folder");
        std::fs::write(&path, bytes).expect("write fixture");
    }
    assert_eq!(markdown_files(&workspace), ["README.md", "docs/bad.md"]);
    assert_eq!(
        lint_markdown_file(&workspace, "docs/bad.md"),
        ["docs/bad.md: not UTF-8; save it as UTF-8"]
    );
}

/// The production review recipes (design-qa-captures 1.8 and Wave 2).
const PRODUCTION_FIXTURES: &str = "src/ui/review/fixtures/production.rs";
/// The `Game` methods that change game state without `Game::apply`: the
/// staging handle, the test-support `*_mut` accessors and setters, and the
/// ungated mutators (deploying, pausing, placing, selling, salvaging,
/// journeys, retries and repeats, expeditions, spawns). Ticking the
/// simulation (`tick`, `drain_events`) is play, not staging.
const STAGING_SEAMS: &[&str] = &[
    "staging",
    "campaign_mut",
    "strategy_mut",
    "attempt_mut",
    "attempt_slot_mut",
    "last_result_mut",
    "battles_mut",
    "pending_runs_mut",
    "game_mut",
    "with_strategy",
    "set_seed",
    "set_next_attempt_id",
    "set_repeat_remaining",
    "set_global_paused",
    "stage_review_enemy_cap",
    "sell_item",
    "sell_extras",
    "sell_unprotected",
    "deploy",
    "deploy_country_stage",
    "deploy_country_stage_as",
    "set_paused",
    "place",
    "save_country_layout",
    "clear_country_layout",
    "salvage",
    "spawn_enemy",
    "spawn_archetype",
    "continue_journey",
    "leave_journey",
    "retry",
    "withdraw",
    "start_repeat",
    "continue_repeat",
    "deploy_survivor",
    "deploy_country_operation",
    "recall_survivor",
];
/// The helpers that may use a staging seam: entitlements (the start country,
/// coins, experience, earlier stage clears and inventory grants).
const ENTITLEMENT_PREFIX: &str = "entitle_";

/// Every [`STAGING_SEAMS`] use in `text` (Rust source) outside an
/// [`ENTITLEMENT_PREFIX`] helper, as `name:line: reason`. A use is a seam
/// name reached through `.` and called (`f.game.staging()`), or named by a
/// path (`Game::staging(&mut f.game)`, `Game::sell_item`). It is allowed only
/// when every enclosing `fn` is an entitlement helper, so an `entitle_*` fn
/// nested in a recipe exempts nothing. Comments and string contents are not
/// code. A closure belongs to the `fn` it is written in.
fn staging_outside_entitlements(name: &str, text: &str) -> Vec<String> {
    let chars = crate::architecture_tests::blank_comments_and_strings(text);
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let starts_word = |at: usize| at == 0 || !is_word(chars[at - 1]);
    let word_at =
        |at: usize| -> String { chars[at..].iter().take_while(|c| is_word(**c)).collect() };
    // The nearest non-whitespace chars before `at`, and the first from `at` on.
    let before = |at: usize| chars[..at].iter().rev().filter(|c| !c.is_whitespace());
    let after = |at: usize| chars[at..].iter().find(|c| !c.is_whitespace()).copied();
    let mut violations = Vec::new();
    // (fn name, brace depth outside its body)
    let mut functions: Vec<(String, usize)> = Vec::new();
    // (fn name, `(`/`[` nesting at its `fn`) until its body opens.
    let mut pending: Option<(String, usize)> = None;
    let mut depth = 0usize;
    let mut nesting = 0usize;
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\n' => line += 1,
            '{' => {
                if let Some((function, _)) = pending.take() {
                    functions.push((function, depth));
                }
                depth += 1;
            }
            '(' | '[' => nesting += 1,
            ')' | ']' => nesting = nesting.saturating_sub(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if functions.last().is_some_and(|(_, outer)| *outer == depth) {
                    functions.pop();
                }
            }
            // A body-less declaration (a trait method). A `;` inside the
            // signature's brackets (`[u8; 4]`) does not end it.
            ';' if pending.as_ref().is_some_and(|(_, at)| *at == nesting) => pending = None,
            _ if starts_word(i) && word_at(i) == "fn" => {
                let mut j = i + 2;
                while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                    j += 1;
                }
                if j > i + 2 {
                    pending = Some((word_at(j), nesting));
                }
            }
            _ => {}
        }
        if starts_word(i) && is_word(chars[i]) {
            let word = word_at(i);
            let mut prior = before(i);
            let used = match (prior.next(), prior.next()) {
                // A method call (or a turbofish one).
                (Some('.'), _) => matches!(after(i + word.chars().count()), Some('(' | ':')),
                // A path: UFCS or a function value.
                (Some(':'), Some(':')) => true,
                _ => false,
            };
            if used && STAGING_SEAMS.contains(&word.as_str()) {
                // The innermost enclosing fn that is not an entitlement.
                let recipe = functions
                    .iter()
                    .rev()
                    .find(|(function, _)| !function.starts_with(ENTITLEMENT_PREFIX));
                let place = match recipe {
                    Some((function, _)) => Some(format!("in fn {function}")),
                    None if functions.is_empty() => Some("outside any fn".to_owned()),
                    None => None,
                };
                if let Some(place) = place {
                    violations.push(format!(
                        "{name}:{line}: `{word}` {place}; stage feature state through \
                         Game::apply(&Command), or entitle it in an `{ENTITLEMENT_PREFIX}*` helper"
                    ));
                }
            }
        }
        i += 1;
    }
    violations
}

#[test]
fn production_fixtures_stage_features_through_commands() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(PRODUCTION_FIXTURES);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(
        text.contains("fn entitle_"),
        "{PRODUCTION_FIXTURES} has no entitle_* helpers; update this check"
    );
    assert_eq!(
        staging_outside_entitlements(PRODUCTION_FIXTURES, &text),
        Vec::<String>::new(),
        "ISO-5: production review recipes stage feature state through Game::apply(&Command)"
    );

    let sample = "\
// f.game.staging() in a comment
fn entitle_coins(f: &mut F) {
    f.game.staging().campaign().reward(1);
    let again = |f: &mut F| f.game.staging();
}
fn prepare_x(f: &mut F) {
    let s = \"staging()\";
    f.game
        .staging()
        .campaign();
    if s.is_empty() { f.game.campaign_mut().coins = 1; }
    fn entitle_inner(f: &mut F) { f.game.staging(); }
    f.game.my_campaign_mut();
    Game::staging(&mut f.game).campaign();
    f.game.deploy(0, Mode::Personal);
    f.game . set_paused (false);
    let sell = Game::sell_item;
    let place = 1; f.game.place(kind, 0, place);
    f.game.tick(Input::default());
    fn deploy(x: u8) {}
}
fn entitle_start(f: &mut F) { f.game.deploy_country_stage(0, Mode::Personal); }
fn prepare_y(f: &mut F) -> impl Fn() { f.game.staging_count(); }
f.game.staging();
fn entitle_bytes(f: &mut F, b: [u8; 4]) { f.game.staging(); }
fn prepare_z(f: &mut F, b: [u8; 4]) { f.game.staging(); }
";
    let rule = "stage feature state through Game::apply(&Command), or entitle it in an \
                `entitle_*` helper";
    let expected: Vec<String> = [
        (9, "staging", "in fn prepare_x"),
        (11, "campaign_mut", "in fn prepare_x"),
        // A nested entitlement helper does not exempt its recipe.
        (12, "staging", "in fn prepare_x"),
        // The UFCS form, the ungated mutators and a path to one.
        (14, "staging", "in fn prepare_x"),
        (15, "deploy", "in fn prepare_x"),
        (16, "set_paused", "in fn prepare_x"),
        (17, "sell_item", "in fn prepare_x"),
        (18, "place", "in fn prepare_x"),
        (24, "staging", "outside any fn"),
        // A `;` inside a signature (`[u8; 4]`) does not end the fn.
        (26, "staging", "in fn prepare_z"),
    ]
    .into_iter()
    .map(|(line, seam, place)| format!("x.rs:{line}: `{seam}` {place}; {rule}"))
    .collect();
    assert_eq!(staging_outside_entitlements("x.rs", sample), expected);
}

/// The docs a playtest package ships (`$CuratedDocs` in
/// `scripts/package_playtest.ps1`), relative to `docs/`.
const CURATED_DOCS: &[&str] = &["PLAYTEST_GUIDE.md"];
/// Upper bound for the package's whole `docs/` folder, in bytes.
const CURATED_DOCS_LIMIT: u64 = 65_536;

/// Targets of the inline Markdown links (`[text](target)`) in `text`, with
/// the 1-based line of each.
fn markdown_link_targets(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else { break };
            found.push((index + 1, after[..end].trim().to_owned()));
            rest = &after[end + 1..];
        }
    }
    found
}

/// Links in a shipped doc that would be dead inside the package: relative
/// targets that are not one of the curated docs. Web links and in-page
/// anchors are fine.
fn links_outside_the_package(name: &str, text: &str) -> Vec<String> {
    markdown_link_targets(text)
        .into_iter()
        .filter(|(_, target)| {
            let lowered = target.to_ascii_lowercase();
            if lowered.starts_with("http://")
                || lowered.starts_with("https://")
                || lowered.starts_with("mailto:")
                || target.starts_with('#')
            {
                return false;
            }
            let file = target.split('#').next().unwrap_or_default();
            !CURATED_DOCS.contains(&file)
        })
        .map(|(line, target)| format!("{name}:{line}: `{target}` is not in the playtest package"))
        .collect()
}

#[test]
fn playtest_guide_has_no_links_outside_the_package() {
    let workspace = workspace_directory();
    let mut violations = Vec::new();
    let mut bytes = 0;
    for name in CURATED_DOCS {
        let path = workspace.join("docs").join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        bytes += text.len() as u64;
        violations.extend(links_outside_the_package(name, &text));
    }
    assert!(
        violations.is_empty(),
        "shipped docs link outside the package ({} links):\n{}",
        violations.len(),
        violations.join("\n")
    );
    assert!(
        bytes <= CURATED_DOCS_LIMIT,
        "the packaged docs are {bytes} B, above {CURATED_DOCS_LIMIT} B"
    );
    // The packaging script ships exactly this list.
    let script = std::fs::read_to_string(scripts_directory().join("package_playtest.ps1"))
        .expect("read package_playtest.ps1");
    let expected = format!(
        "$CuratedDocs = @({})",
        CURATED_DOCS
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        script.contains(&expected),
        "package_playtest.ps1 must declare `{expected}`"
    );
}

#[test]
fn package_link_check_reports_relative_links_only() {
    let text = "See [a](JOURNEYS.md) and [b](audio/x.wav).\n\
                [web](https://example.com/x.md) [top](#loop) \
                [self](PLAYTEST_GUIDE.md#loop) [mail](mailto:a@b.c)\n";
    let violations = links_outside_the_package("PLAYTEST_GUIDE.md", text);
    assert_eq!(
        violations,
        vec![
            "PLAYTEST_GUIDE.md:1: `JOURNEYS.md` is not in the playtest package".to_owned(),
            "PLAYTEST_GUIDE.md:1: `audio/x.wav` is not in the playtest package".to_owned(),
        ]
    );
}

/// The docs inventory (design-qa-captures §1.13): the sections each owner
/// writes into a doc the production pass changes, by heading. Presence only
/// (ruling W-15): the wording is the owner's, so a heading matches a whole
/// line, nothing more. W4-10 lists enemy-balance's four docs, W4-11
/// progression-tree's goal-5 section, W7-04 loot-economy's auto-sell
/// sections, W7-05 progression-tree's, W7-06 campaign-defenders' and W7-07
/// ux-flow's; W7-08 adds the rest of the inventory.
const REQUIRED_DOC_SECTIONS: &[(&str, &[&str])] = &[
    (
        "docs/ENEMY_COMPOSITION.md",
        &[
            "## Composition archetypes",
            "## Elites",
            "## Boss phases",
            "## Regional and Capstone bosses",
        ],
    ),
    (
        "docs/PROGRESSION_SCALING.md",
        &[
            "## Two-axis threat",
            "## Elite rate",
            "## Regional checkpoints and the Capstone",
            "## Expedition threat",
            "## Country difficulty band",
        ],
    ),
    (
        "docs/BALANCE_AND_RUN_STATS.md",
        &[
            "## Goal 5: progression pacing acceptance",
            "## Goal 6: enemy difficulty acceptance",
        ],
    ),
    ("docs/WORLD.md", &["### Country difficulty band"]),
    // W7-04, loot-economy: the auto-sell rule and the loot it settles.
    ("docs/PROGRESSION.md", &["### Auto-sell"]),
    ("docs/GDD.md", &["## Auto-sell"]),
    (
        "docs/ITEM_QUALITY.md",
        &["## Auto-sell threshold", "## Quality markers"],
    ),
    (
        "docs/BALANCE_AND_RUN_STATS.md",
        &["## Goal 4: auto-sell pacing and stash growth"],
    ),
    ("docs/PLAYTEST_GUIDE.md", &["## Auto-sell"]),
    // W7-05, progression-tree: grants, milestones, camp and tree prices, the
    // tree rules, the class presentation and the pacing CLI.
    (
        "docs/PROGRESSION.md",
        &[
            "### Level-up grant and rank milestones",
            "### Camp prices",
            "### Skill tree prices",
        ],
    ),
    (
        "docs/SKILLS_AND_WEAPONS.md",
        &[
            "## Prices and the tree rules",
            "## Weapon class presentation",
        ],
    ),
    ("docs/BALANCE_AND_RUN_STATS.md", &["## Pacing CLI"]),
    // W7-06, campaign-defenders: who defends, the takeover, choosing a
    // country, expedition outcomes and the recruit allowance.
    (
        "docs/STRATEGY.md",
        &[
            "## Defenders and the next defense",
            "## Recruit takeover",
            "## Expedition outcomes",
            "## Defender power and service records",
        ],
    ),
    (
        "docs/JOURNEYS.md",
        &["## Choosing who defends", "## Choosing a country"],
    ),
    (
        "docs/PROGRESSION.md",
        &["### Recruit advancement allowance"],
    ),
    ("docs/PLAYTEST_GUIDE.md", &["## Takeover"]),
    // W7-07, ux-flow: the loop map, the production-pass UI contract, the
    // new views and the Escape table.
    (
        "docs/JOURNEYS.md",
        &["## Loop map", "## Every main round ends on Results"],
    ),
    (
        "docs/UI_REDESIGN.md",
        &[
            "## Production pass — 14 November 2026",
            "### Escape and decision 18",
            "### One amber primary",
            "### Camp home, Results and Upgrades",
            "### Modals and toasts",
            "### Battle layout and the two-row bar",
        ],
    ),
    ("docs/VIEW_HIERARCHY.md", &["## Production pass views"]),
    ("docs/PLAYTEST_GUIDE.md", &["## Escape"]),
    // W7-08, qa-captures: the rest of the inventory. The playtest guide's
    // loop sections (checked more closely by
    // `playtest_guide_covers_the_production_loop`), the acceptance ledger,
    // the audio guide W7-03 rewrote, the later user requirements, the full
    // verify and the ownership table.
    (
        "docs/PLAYTEST_GUIDE.md",
        &["## Loop", "## Listening pass", "## Click-to-photon"],
    ),
    (
        "docs/ACCEPTANCE.md",
        &[
            "## Production pass criteria",
            "## Audio loudness gate",
            "## Pending human checks",
        ],
    ),
    (
        "docs/AUDIO_SURVIVAL.md",
        &[
            "## Loudness gate",
            "## Buses and sliders",
            "## Mix table",
            "## Mastering and headroom",
        ],
    ),
    (
        "docs/EXPANSION.md",
        &["## Production pass requirements — November 2026"],
    ),
    ("docs/VALIDATION.md", &["## Full verify"]),
    ("docs/ARCHITECTURE.md", &["## Ownership"]),
];

/// Interface claims the production pass made false, with what replaced each
/// (W7-07, ux-flow). Matched like [`RETIRED_DOC_CLAIMS`].
const RETIRED_UX_CLAIMS: &[(&str, &str)] = &[
    (
        "Every main round returns to upgrades",
        "every main round ends on Results, one action from Upgrades (JOURNEYS.md, \"Loop map\")",
    ),
    (
        "Each main outcome routes to Journey upgrades",
        "every main outcome ends on Results since W3-03 (JOURNEYS.md, \"Loop map\")",
    ),
    (
        "Choosing that country immediately deploys a live personal defense",
        "Start journey opens the first defense paused; later countries are chosen with SelectCountry (W3-18)",
    ),
    (
        "First-country confirmation immediately creates and enters a live personal attempt",
        "Start journey opens the first defense paused; later countries are chosen with SelectCountry (W3-18)",
    ),
    (
        "In NPC mode open Survivor",
        "the NPC bar's popup is Tactics and names the defender",
    ),
    (
        "Use **Escape** or an explicit Pause action separately",
        "Escape closes construction first and pauses only on the next press (PLAYTEST_GUIDE.md, \"Escape\")",
    ),
];

/// Defender claims the production pass made false, with what replaced each
/// (W7-06, campaign-defenders). Matched like [`RETIRED_DOC_CLAIMS`].
const RETIRED_DEFENDER_CLAIMS: &[(&str, &str)] = &[
    (
        "A survivor has exactly one status: Ready, Deployed or Guarding",
        "RecruitStatus has four since W1-11: Ready, Guarding, Deployed and Defending (STRATEGY.md)",
    ),
    (
        "The advancement allowance is 12 plus 8 for each first-cleared encounter-template milestone",
        "the allowance follows liberations since W2-10 (PROGRESSION.md, \"Recruit advancement allowance\")",
    ),
    (
        "retries the reported stage, preserving control mode",
        "continuing deploys Game::next_defender (JOURNEYS.md, \"Choosing who defends\")",
    ),
];

/// Progression claims the production pass made false, with what replaced
/// each (W7-05, progression-tree). Matched like [`RETIRED_DOC_CLAIMS`].
const RETIRED_PROGRESSION_CLAIMS: &[(&str, &str)] = &[
    (
        "prices are provisional until the wave-4 fit",
        "W4-p fitted them (SKILLS_AND_WEAPONS.md, \"Prices and the tree rules\")",
    ),
    (
        "prices provisional until W4-p",
        "W4-p fitted them (PROGRESSION.md, \"Skill tree prices\")",
    ),
    (
        "the Results screen does not show it yet",
        "Results shows the grant and milestone lines since W3-22",
    ),
    (
        "no shipped structure authors it",
        "the shrapnel trap authors charges_per_rank 1 since W4-03",
    ),
    (
        "keep their prices, so building and companions open on the same milestones",
        "W4-p repriced the blueprints and feature unlocks",
    ),
];

/// Claims the production pass made false, with what replaced each (W7-04,
/// loot-economy). Matched case-insensitively on whitespace-collapsed text,
/// so a rewrapped sentence still fails; `docs/plans/` quotes them as history
/// and is not scanned.
const RETIRED_DOC_CLAIMS: &[(&str, &str)] = &[
    (
        "There is no automatic sale system",
        "the auto-sell rule (PROGRESSION.md, \"Auto-sell\")",
    ),
    (
        "Optional auto-sell is a later",
        "auto-sell shipped, off by default (GDD.md, \"Auto-sell\")",
    ),
    (
        "twenty-nine equipment definitions",
        "the catalogue holds 68 definitions since W2-08",
    ),
    (
        "it never buys, sells or trains automatically",
        "an operation's drops follow the auto-sell rule's expedition toggle",
    ),
];

/// Every claim of `claims` that `text` still makes, as
/// `name: "<claim>" is retired; <replacement>`.
fn retired_claims(name: &str, text: &str, claims: &[(&str, &str)]) -> Vec<String> {
    let text = normalized(text);
    claims
        .iter()
        .filter(|(claim, _)| text.contains(&normalized(claim)))
        .map(|(claim, replacement)| format!("{name}: \"{claim}\" is retired; {replacement}"))
        .collect()
}

/// Every claim of `claims` a Markdown file under `docs/` (outside
/// `docs/plans/`) still makes.
fn retired_claims_in_docs(claims: &[(&str, &str)]) -> Vec<String> {
    let workspace = workspace_directory();
    let mut violations = Vec::new();
    for name in markdown_files(&workspace)
        .iter()
        .filter(|name| name.starts_with("docs/"))
    {
        let path = workspace.join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        violations.extend(retired_claims(name, &text, claims));
    }
    violations
}

#[test]
fn docs_drop_retired_progression_claims() {
    let violations = retired_claims_in_docs(RETIRED_PROGRESSION_CLAIMS);
    assert!(
        violations.is_empty(),
        "docs still make retired progression claims ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn docs_drop_retired_defender_claims() {
    let violations = retired_claims_in_docs(RETIRED_DEFENDER_CLAIMS);
    assert!(
        violations.is_empty(),
        "docs still make retired defender claims ({}):
{}",
        violations.len(),
        violations.join(
            "
"
        )
    );
}

#[test]
fn docs_drop_retired_ux_claims() {
    let violations = retired_claims_in_docs(RETIRED_UX_CLAIMS);
    assert!(
        violations.is_empty(),
        "docs still make retired interface claims ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn docs_drop_retired_loot_claims() {
    let violations = retired_claims_in_docs(RETIRED_DOC_CLAIMS);
    assert!(
        violations.is_empty(),
        "docs still make retired claims ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn retired_claim_check_matches_across_wrapped_lines_and_case() {
    let text = "Sales are strict.\nthere is no\n  AUTOMATIC sale system.\n";
    assert_eq!(
        retired_claims(
            "d.md",
            text,
            &[
                ("There is no automatic sale system", "use the rule"),
                ("never sells", "n/a"),
            ]
        ),
        vec!["d.md: \"There is no automatic sale system\" is retired; use the rule".to_owned()]
    );
}

/// The headings of `required` that no line of `text` equals (trailing
/// whitespace and a CR ignored), as `name: missing "<heading>"`. A heading
/// inside a fenced block does not count.
fn missing_doc_sections(name: &str, text: &str, required: &[&str]) -> Vec<String> {
    let mut fenced = false;
    let mut headings = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            headings.push(line);
        }
    }
    required
        .iter()
        .filter(|heading| !headings.contains(heading))
        .map(|heading| format!("{name}: missing \"{heading}\""))
        .collect()
}

#[test]
fn required_docs_sections_exist() {
    let workspace = workspace_directory();
    let mut missing = Vec::new();
    for (name, required) in REQUIRED_DOC_SECTIONS {
        let path = workspace.join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        missing.extend(missing_doc_sections(name, &text, required));
    }
    assert!(
        missing.is_empty(),
        "docs inventory sections missing ({}):\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn doc_section_check_matches_whole_heading_lines_outside_fences() {
    let text = "# Doc\r\n## Elites  \r\n## Boss phases and more\n```\n## Fenced\n```\n";
    assert_eq!(
        missing_doc_sections("d.md", text, &["## Elites", "## Boss phases", "## Fenced"]),
        vec![
            "d.md: missing \"## Boss phases\"".to_owned(),
            "d.md: missing \"## Fenced\"".to_owned(),
        ]
    );
}

/// The playtest guide's production-loop sections (design-qa-captures 13,
/// W7-08): the loop a tester walks, then the four checks along it.
const PLAYTEST_LOOP_SECTIONS: &[&str] = &[
    "## Loop",
    "## Escape",
    "## Auto-sell",
    "## Takeover",
    "## Listening pass",
    "## Click-to-photon",
];

/// What a loop section must say, matched like [`RETIRED_DOC_CLAIMS`]: the
/// stops of the loop, and for the two human checks a `Status:` line (pending
/// until a pass is logged) and, for click-to-photon, the protocol's camera,
/// shots and targets (design-qa-captures 1.14).
const PLAYTEST_SECTION_FACTS: &[(&str, &[&str])] = &[
    (
        "## Loop",
        &[
            "Results",
            "Upgrade",
            "Back to battle",
            "Back to camp",
            "Camp home",
            "Recruit",
        ],
    ),
    ("## Listening pass", &["Status:"]),
    (
        "## Click-to-photon",
        &[
            "Status:",
            "240 fps",
            "20 single shots",
            "50 ms",
            "34 ms",
            "67 ms",
        ],
    ),
];

/// The body of the section under `heading` (a whole line outside fences, as
/// [`missing_doc_sections`] matches it): the lines after it up to the next
/// heading of its level or higher. None when the heading is absent.
fn section_body(text: &str, heading: &str) -> Option<String> {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let mut fenced = false;
    let mut body: Option<Vec<&str>> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            if hashes > 0 && line[hashes..].starts_with(' ') {
                if body.is_some() && hashes <= level {
                    break;
                }
                if line == heading {
                    body = Some(Vec::new());
                    continue;
                }
            }
        }
        if let Some(lines) = body.as_mut() {
            lines.push(line);
        }
    }
    body.map(|lines| lines.join("\n"))
}

/// The loop sections `text` lacks and the facts its sections leave out.
fn playtest_loop_violations(name: &str, text: &str) -> Vec<String> {
    let mut violations = missing_doc_sections(name, text, PLAYTEST_LOOP_SECTIONS);
    for (heading, facts) in PLAYTEST_SECTION_FACTS {
        let Some(body) = section_body(text, heading) else {
            continue;
        };
        let body = normalized(&body);
        violations.extend(
            facts
                .iter()
                .filter(|fact| !body.contains(&normalized(fact)))
                .map(|fact| format!("{name}: \"{heading}\" does not say \"{fact}\"")),
        );
    }
    violations
}

#[test]
fn playtest_guide_covers_the_production_loop() {
    let name = "docs/PLAYTEST_GUIDE.md";
    let violations = playtest_loop_violations(name, &read_workspace_file(name));
    assert!(
        violations.is_empty(),
        "the playtest guide does not cover the production loop ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn section_body_stops_at_the_next_heading_of_its_level() {
    let text = "# Guide\n## Loop\nResults then\n### Detail\nUpgrade\n```\n## Fenced\n```\n\
                ## Escape\nMenu\n";
    assert_eq!(
        section_body(text, "## Loop").as_deref(),
        Some("Results then\n### Detail\nUpgrade\n```\n## Fenced\n```")
    );
    assert_eq!(section_body(text, "## Escape").as_deref(), Some("Menu"));
    assert_eq!(section_body(text, "## Takeover"), None);
    let violations = playtest_loop_violations("g.md", "## Loop\nResults, Upgrade.\n");
    assert!(violations.contains(&"g.md: missing \"## Click-to-photon\"".to_owned()));
    assert!(violations.contains(&"g.md: \"## Loop\" does not say \"Back to camp\"".to_owned()));
    assert!(!violations.iter().any(|v| v.contains("\"Upgrade\"")));
}

/// The production-pass acceptance matrix, which says every one of its rows
/// is also listed in the acceptance ledger (W7-08).
const PRODUCTION_ACCEPTANCE: &str = "docs/plans/production-pass/acceptance.md";
/// The acceptance ledger and its two production-pass sections.
const ACCEPTANCE_LEDGER: &str = "docs/ACCEPTANCE.md";
const LEDGER_PRODUCTION_SECTION: &str = "## Production pass criteria";
const LEDGER_HUMAN_CHECKS: &str = "## Pending human checks";
/// The production-pass plan and its list of pending human checks.
const PRODUCTION_PLAN: &str = "docs/plans/2026-11-05-production-pass.md";
const PLAN_HUMAN_CHECKS: &str = "## 11. Human checks pending";

/// Backticked names in a ledger Proof cell that are not functions, with
/// what each is.
const PROOF_NAMES_THAT_ARE_NOT_FUNCTIONS: &[(&str, &str)] = &[
    ("boss_phases", "the attempt-scoped saved field"),
    (
        "interface_total_not_above_baseline_v3",
        "a PERF_BUDGET.json gate that measure_performance.ps1 evaluates",
    ),
    (
        "old_campaign_filter_sells_recruit_equipped_item",
        "W1-18's characterisation, inverted and renamed by W1-20",
    ),
    (
        "pacing_targets",
        "the release test target tests/pacing_targets.rs",
    ),
    ("save_clone", "the horde_core example"),
    ("shipped_smoke_seconds", "a frame-report key"),
    (
        "valid_overs",
        "a summary.json key of measure_performance.ps1",
    ),
];

/// The cells of a Markdown table row, trimmed, with an escaped `\|` kept
/// inside its cell as `|`. None for a line that is not a row and for the
/// separator row.
fn table_cells(line: &str) -> Option<Vec<String>> {
    let inner = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' && characters.peek() == Some(&'|') {
            cell.push('|');
            characters.next();
        } else if character == '|' {
            cells.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(character);
        }
    }
    cells.push(cell.trim().to_owned());
    let separator = cells
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':')));
    (!separator).then_some(cells)
}

/// The body rows of every table in `text`: rows whose first cell is not
/// `header`.
fn table_rows(text: &str, header: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter_map(table_cells)
        .filter(|cells| cells[0] != header)
        .collect()
}

/// The functions a Proof cell names: every backticked lower-case token whose
/// last `::` segment is an identifier with an underscore. Paths
/// (`tests/auto_sell.rs`) and keys (`egui.battlefield_primitives.max`) are
/// not names; an elided name (`foo_...`) is kept, so the check reports it.
fn cited_functions(cell: &str) -> Vec<String> {
    let identifier = |name: &str| {
        name.contains('_')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    cell.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            token.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | ':' | '/' | '.')
            })
        })
        .filter_map(|token| token.rsplit("::").next())
        .filter(|name| {
            identifier(name) || (name.contains("...") && identifier(&name.replace('.', "")))
        })
        .map(str::to_owned)
        .collect()
}

/// The functions a source text defines: the name after a line-leading
/// `fn` (past `pub`, `const`, `async` and `unsafe`), `def` or `function`.
fn defined_functions(text: &str) -> Vec<String> {
    const MODIFIERS: &[&str] = &[
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "const ",
        "async ",
        "unsafe ",
    ];
    const KEYWORDS: &[&str] = &["fn ", "def ", "function ", "Function "];
    let mut found = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim_start();
        while let Some(after) = MODIFIERS
            .iter()
            .find_map(|modifier| rest.strip_prefix(modifier))
        {
            rest = after.trim_start();
        }
        let Some(after) = KEYWORDS
            .iter()
            .find_map(|keyword| rest.strip_prefix(keyword))
        else {
            continue;
        };
        let name: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .collect();
        if !name.is_empty() {
            found.push(name);
        }
    }
    found
}

/// Every function defined under `crates/` and `scripts/` (Rust, Python and
/// PowerShell), by name, with the files that define it.
fn function_index(workspace: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for top in ["crates", "scripts"] {
        for name in workspace_files(workspace, &workspace.join(top), &["rs", "py", "ps1"]) {
            for function in defined_functions(&read_workspace_file(&name)) {
                index.entry(function).or_default().insert(name.clone());
            }
        }
    }
    index
}

/// What the ledger's production section gets wrong against the matrix:
/// a criterion without a row, a row that is not a criterion, a row without
/// the five cells (AC, Criterion, Proof, Defined in, Wave), a Proof name
/// that no code defines, a defined name whose files Defined in leaves out,
/// and a listed file that is missing or defines none of the row's names.
/// Defined in is a comma-separated list of backticked workspace paths, or
/// `—` for a row that names no function.
fn ledger_violations(
    matrix: &str,
    section: &str,
    functions: &BTreeMap<String, BTreeSet<String>>,
    file_exists: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let criteria: Vec<String> = table_rows(matrix, "AC")
        .into_iter()
        .map(|cells| cells[0].clone())
        .collect();
    let rows = table_rows(section, "AC");
    let mut violations: Vec<String> = criteria
        .iter()
        .filter(|id| !rows.iter().any(|row| &row[0] == *id))
        .map(|id| format!("{id}: no row in the ledger"))
        .collect();
    for row in &rows {
        let id = &row[0];
        if !criteria.contains(id) {
            violations.push(format!("{id}: not a criterion of {PRODUCTION_ACCEPTANCE}"));
        }
        let [_, _, proof, defined_in, _] = row.as_slice() else {
            violations.push(format!(
                "{id}: {} cells, not AC, Criterion, Proof, Defined in, Wave",
                row.len()
            ));
            continue;
        };
        let files: Vec<String> = if defined_in == "—" {
            Vec::new()
        } else {
            defined_in
                .split(',')
                .map(|file| file.trim().trim_matches('`').to_owned())
                .collect()
        };
        for file in files.iter().filter(|file| !file_exists(file)) {
            violations.push(format!("{id}: `{file}` does not exist"));
        }
        let mut used = BTreeSet::new();
        for name in cited_functions(proof) {
            if PROOF_NAMES_THAT_ARE_NOT_FUNCTIONS
                .iter()
                .any(|(listed, _)| *listed == name)
            {
                continue;
            }
            let Some(defined) = functions.get(&name) else {
                violations.push(format!(
                    "{id}: `{name}` is not a function under crates/ or scripts/"
                ));
                continue;
            };
            let listed: Vec<&String> = files
                .iter()
                .filter(|file| defined.contains(*file))
                .collect();
            if listed.is_empty() {
                violations.push(format!(
                    "{id}: `{name}` is defined in {}, which Defined in does not list",
                    defined.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            used.extend(listed.into_iter().cloned());
        }
        for file in files.iter().filter(|file| !used.contains(*file)) {
            violations.push(format!(
                "{id}: `{file}` defines none of the row's functions"
            ));
        }
    }
    violations
}

/// A workspace file's text, by `/`-separated workspace path.
fn read_workspace_file(name: &str) -> String {
    let path = workspace_directory().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn acceptance_ledger_lists_every_production_pass_row() {
    let workspace = workspace_directory();
    let ledger = read_workspace_file(ACCEPTANCE_LEDGER);
    let section = section_body(&ledger, LEDGER_PRODUCTION_SECTION)
        .unwrap_or_else(|| panic!("{ACCEPTANCE_LEDGER} has no \"{LEDGER_PRODUCTION_SECTION}\""));
    let violations = ledger_violations(
        &read_workspace_file(PRODUCTION_ACCEPTANCE),
        &section,
        &function_index(&workspace),
        &|file| workspace.join(file).is_file(),
    );
    assert!(
        violations.is_empty(),
        "{ACCEPTANCE_LEDGER} does not list the production-pass criteria ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn acceptance_ledger_check_reports_missing_rows_unknown_names_and_unlisted_files() {
    let matrix = "| AC | Criterion | Proof | Wave |\n|---|---|---|---|\n\
                  | G1.1 | a | `x_test` | 1 |\n| G1.2 | b | `y_test` | 1 |\n\
                  | G1.3 | c | d | 7 |\n";
    let section = "| AC | Criterion | Proof | Defined in | Wave |\n|---|---|---|---|---|\n\
                   | G1.1 | a \\| b | `x_test`, `tests/a.rs`, `elided_...` | `a.rs`, `b.rs` | 1 |\n\
                   | G1.3 | c | `z_test`, `w_test` | — | 7 |\n| G9.9 | e | f | — |\n";
    let mut functions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    functions
        .entry("x_test".to_owned())
        .or_default()
        .insert("a.rs".to_owned());
    functions
        .entry("z_test".to_owned())
        .or_default()
        .insert("c.rs".to_owned());
    let violations = ledger_violations(matrix, section, &functions, &|file| file != "b.rs");
    assert_eq!(
        violations,
        vec![
            "G1.2: no row in the ledger".to_owned(),
            "G1.1: `b.rs` does not exist".to_owned(),
            "G1.1: `elided_...` is not a function under crates/ or scripts/".to_owned(),
            "G1.1: `b.rs` defines none of the row's functions".to_owned(),
            "G1.3: `z_test` is defined in c.rs, which Defined in does not list".to_owned(),
            "G1.3: `w_test` is not a function under crates/ or scripts/".to_owned(),
            format!("G9.9: not a criterion of {PRODUCTION_ACCEPTANCE}"),
            "G9.9: 4 cells, not AC, Criterion, Proof, Defined in, Wave".to_owned(),
        ]
    );
    assert_eq!(
        defined_functions(
            "pub(crate) fn a_b() {}\n    async fn c<T>()\ndef d_e(x):\n\
             function Get-X {\n// fn not_this\n"
        ),
        vec!["a_b", "c", "d_e", "Get-X"]
    );
}

/// A check's name without a trailing parenthetical, so the plan's
/// "Stepped damage pop (CF-1-04, W1-64)" and the ledger's "Stepped damage
/// pop (W1-64)" name the same check.
fn check_name(cell: &str) -> String {
    match cell.strip_suffix(')').and_then(|rest| rest.rfind(" (")) {
        Some(open) => cell[..open].trim().to_owned(),
        None => cell.trim().to_owned(),
    }
}

/// Checks pending in one list and absent from the other.
fn human_check_violations(plan: &str, ledger: &str) -> Vec<String> {
    let names = |section: &str| -> BTreeSet<String> {
        table_rows(section, "Check")
            .iter()
            .map(|cells| check_name(&cells[0]))
            .collect()
    };
    let (plan, ledger) = (names(plan), names(ledger));
    let mut violations: Vec<String> = plan
        .difference(&ledger)
        .map(|name| format!("\"{name}\" is pending in the plan but not in {ACCEPTANCE_LEDGER}"))
        .collect();
    violations.extend(
        ledger
            .difference(&plan)
            .map(|name| format!("\"{name}\" is in {ACCEPTANCE_LEDGER} but not in the plan's list")),
    );
    violations
}

#[test]
fn acceptance_lists_every_pending_human_check() {
    let plan = read_workspace_file(PRODUCTION_PLAN);
    let plan = section_body(&plan, PLAN_HUMAN_CHECKS)
        .unwrap_or_else(|| panic!("{PRODUCTION_PLAN} has no \"{PLAN_HUMAN_CHECKS}\""));
    let ledger = read_workspace_file(ACCEPTANCE_LEDGER);
    let ledger = section_body(&ledger, LEDGER_HUMAN_CHECKS)
        .unwrap_or_else(|| panic!("{ACCEPTANCE_LEDGER} has no \"{LEDGER_HUMAN_CHECKS}\""));
    let violations = human_check_violations(&plan, &ledger);
    assert!(
        violations.is_empty(),
        "the pending human checks differ ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn human_check_names_ignore_a_trailing_parenthetical() {
    assert_eq!(
        check_name("Stepped damage pop (CF-1-04, W1-64)"),
        "Stepped damage pop"
    );
    assert_eq!(check_name("Click-to-photon run"), "Click-to-photon run");
    let plan = "| Check | Wave |\n|---|---|\n| A (x) | 1 |\n| B | 2 |\n";
    let ledger = "| Check | Status |\n|---|---|\n| A (y) | pending |\n| C | pending |\n";
    assert_eq!(
        human_check_violations(plan, ledger),
        vec![
            format!("\"B\" is pending in the plan but not in {ACCEPTANCE_LEDGER}"),
            format!("\"C\" is in {ACCEPTANCE_LEDGER} but not in the plan's list"),
        ]
    );
}
