//! Repository checks that read files, not game state. Test-only.
//!
//! `powershell_scripts_are_51_safe` keeps every script under `scripts/` runnable
//! by the Windows PowerShell 5.1 that ships with Windows: no PowerShell 7 or
//! .NET Core APIs, and native processes only through `scripts/lib/Native.ps1`
//! (an `&` call on a native exe that writes stderr raises NativeCommandError
//! under 5.1 as soon as the caller redirects streams).
use std::path::{Path, PathBuf};

/// PowerShell 7 only, by decision; their port is a logged follow-up.
const PS7_ALLOWLIST: &[&str] = &[
    "capture_player_models.ps1",
    "trace_npc.ps1",
    "verify_player_models.ps1",
];

/// Not yet ported, with the commit that ports each and removes it from here.
const PENDING: &[(&str, &str)] = &[
    ("measure_performance.ps1", "W1-36"),
    ("package_playtest.ps1", "W7-02"),
    ("verify_playtest.ps1", "W7-02"),
];

/// Lower-case needles searched on comment-stripped lines, with the reason.
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "getrelativepath",
        "[IO.Path]::GetRelativePath is .NET Core only",
    ),
    ("& cargo", "run cargo through Invoke-Cargo"),
    (
        "& './target",
        "run the game through Get-HordeExecutable and Invoke-Native",
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
const ZIP_ASSEMBLY: &str = "add-type -assemblyname system.io.compression";
const LIBRARY: &str = "lib/native.ps1";

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
        if code.contains(ZIP_ASSEMBLY) {
            zip_assembly_loaded = true;
        }
        if code.contains(ZIP_TYPE) && !zip_assembly_loaded {
            violations.push(format!(
                "{name}:{line}: [IO.Compression.ZipFile] needs an earlier `Add-Type -AssemblyName System.IO.Compression`"
            ));
        }
    }
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
                 $r = Invoke-Native -FilePath $exe -LogStem 'x#y' # trailing *>\n";
    assert_eq!(
        lint_script("clean.ps1", clean.as_bytes()),
        Vec::<String>::new()
    );

    let cases: &[(&str, &str)] = &[
        ("x = 1\n", "the first line must be"),
        ("#requires -Version 5.1\n& cargo build\n", "`& cargo`"),
        (
            "#requires -Version 5.1\n& './target/debug/x.exe'\n",
            "`& './target`",
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
