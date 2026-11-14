"""Check the documented native View graph without third-party dependencies."""
from pathlib import Path
import re
import sys
from urllib.parse import unquote


def _front_matter(text: str) -> tuple[dict[str, str], str] | None:
    """A View file's front-matter fields and its body, or None when the file
    does not start with a `---` block."""
    parts = text.split("---", 2)
    if len(parts) != 3 or parts[0].strip():
        return None
    fields = {}
    for line in parts[1].splitlines():
        if ":" in line:
            key, value = line.split(":", 1)
            fields[key.strip()] = value.strip().strip('"').strip("'")
    return fields, parts[2]


def check(workspace: Path) -> tuple[int, list[str]]:
    views: dict[str, tuple[Path, dict[str, str]]] = {}
    errors: list[str] = []
    for path in sorted((workspace / "docs/components").glob("*.md")):
        text = path.read_text(encoding="utf-8-sig")
        parsed = _front_matter(text)
        if parsed is None:
            errors.append(f"{path.name}: missing YAML front matter")
            continue
        fields, body = parsed
        required = {"type", "id", "title", "parent", "status", "source"}
        if missing := required - fields.keys():
            errors.append(f"{path.name}: missing {sorted(missing)}")
            continue
        view_id = fields["id"]
        if fields["type"] != "View" or view_id != path.stem:
            errors.append(f"{path.name}: expected type View and id matching filename")
        if view_id in views:
            errors.append(f"{path.name}: duplicate id {view_id}")
        views[view_id] = (path, fields)
        for reference in fields["source"].split(";"):
            source, _, symbols = reference.strip().partition("::")
            source_path = (workspace / source).resolve()
            if workspace not in source_path.parents or not source_path.is_file():
                errors.append(f"{path.name}: missing/invalid source {source}")
                continue
            source_text = source_path.read_text(encoding="utf-8-sig")
            for symbol in filter(None, (s.strip() for s in symbols.split(","))):
                if not re.search(r"\b" + re.escape(symbol) + r"\b", source_text):
                    errors.append(f"{path.name}: source symbol not found: {symbol}")
        if not body.strip():
            errors.append(f"{path.name}: empty View description")
    roots = [key for key, (_, fields) in views.items() if fields["parent"] == "null"]
    if roots != ["game"]:
        errors.append(f"Expected sole root game, found {roots}")
    for view_id, (path, fields) in views.items():
        parent = fields["parent"]
        if parent != "null" and parent not in views:
            errors.append(f"{path.name}: unknown parent {parent}")
        seen = {view_id}
        while parent in views:
            if parent in seen:
                errors.append(f"{path.name}: parent cycle through {parent}")
                break
            seen.add(parent)
            parent = views[parent][1]["parent"]
    if not views:
        errors.append("No Views documented")
    return len(views), errors


# The doc that draws the View tree (one nested `- `id`` list, two spaces per level).
VIEW_HIERARCHY_DOC = "docs/VIEW_HIERARCHY.md"
# The Views the production pass added (W7-07; design-qa-captures §1.13, the
# VIEW_HIERARCHY.md row; waves.md W7-07): each needs a View file and its place
# in the tree. `journey-upgrades` is the Upgrades screen under its retained id.
PRODUCTION_VIEWS = (
    "journey-upgrades",
    "camp-home",
    "onboarding-callouts",
    "defender-picker",
    "results-loot-block",
    "recruit-modal",
    "equipment-auto-sell",
    "auto-sell-stash-apply",
    "quit-confirmation",
    "return-to-menu-confirmation",
    "storage-error",
    "draft-warning",
    "toasts",
    "battle-layout",
    "battle-retreat-hold",
    "battle-boss-banner",
    "battle-low-hp",
    "menu-build-stamp",
)
# One entry of the tree: indentation, then a backticked View id alone on the line.
TREE_ENTRY = re.compile(r"^( *)- `([^`]+)`\s*$")
INVENTORY_LINE = re.compile(r"Current inventory: \*\*(\d+) View files\*\*")


def _view_parents(workspace: Path) -> dict[str, str]:
    """Each View file's id and front-matter parent (files `check` rejects for
    missing front matter or fields are left out)."""
    parents: dict[str, str] = {}
    for path in sorted((workspace / "docs/components").glob("*.md")):
        parsed = _front_matter(path.read_text(encoding="utf-8-sig"))
        if parsed is not None and {"id", "parent"} <= parsed[0].keys():
            parents[parsed[0]["id"]] = parsed[0]["parent"]
    return parents


def check_view_hierarchy(workspace: Path, required: tuple[str, ...] = PRODUCTION_VIEWS) -> tuple[int, list[str]]:
    """VIEW_HIERARCHY.md's tree lists every View file exactly once, nested
    under the parent its front matter names (the root at the top level);
    lists nothing else; states the View count in its "Current inventory"
    line; and every `required` View has a file. Returns the number of listed
    Views and the errors."""
    doc = Path(VIEW_HIERARCHY_DOC).name
    path = workspace / VIEW_HIERARCHY_DOC
    if not path.is_file():
        return 0, [f"{VIEW_HIERARCHY_DOC}: missing"]
    parents = _view_parents(workspace)
    errors: list[str] = []
    listed: dict[str, str] = {}
    stack: list[str] = []
    text = path.read_text(encoding="utf-8-sig")
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith(("```", "~~~")):
            fenced = not fenced
            continue
        entry = None if fenced else TREE_ENTRY.match(line)
        if entry is None:
            continue
        indent, view_id = len(entry.group(1)), entry.group(2)
        if indent % 2 or indent // 2 > len(stack):
            errors.append(f"{doc}:{number}: `{view_id}` is not nested two spaces under a listed View")
            continue
        del stack[indent // 2:]
        parent = stack[-1] if stack else "null"
        stack.append(view_id)
        if view_id in listed:
            errors.append(f"{doc}:{number}: `{view_id}` is listed twice")
            continue
        listed[view_id] = parent
        if view_id not in parents:
            errors.append(f"{doc}:{number}: lists `{view_id}`, which has no docs/components/{view_id}.md")
        elif parents[view_id] != parent:
            errors.append(f"{doc}:{number}: `{view_id}` is listed under `{parent}`, "
                          f"its View file names parent `{parents[view_id]}`")
    for view_id in sorted(parents.keys() - listed.keys()):
        errors.append(f"{doc}: does not list View `{view_id}` (parent `{parents[view_id]}`)")
    for view_id in required:
        if view_id not in parents:
            errors.append(f"docs/components/{view_id}.md: missing (a production-pass View, W7-07)")
    inventory = INVENTORY_LINE.search(text)
    if inventory is None:
        errors.append(f"{doc}: no 'Current inventory: **<n> View files**' line")
    elif int(inventory.group(1)) != len(parents):
        errors.append(f"{doc}: says {inventory.group(1)} View files, docs/components has {len(parents)}")
    return len(listed), errors


def _module_name(relative: Path) -> str | None:
    """The module path of a crate source entry (`sim/director`), or None."""
    parts = list(relative.parts)
    if relative.suffix == ".rs":
        parts[-1] = relative.stem
    if parts and parts[-1] == "mod":
        parts.pop()
    if not parts or parts == ["lib"] or parts == ["main"]:
        return None
    if any(part == "tests" or part.endswith("_tests") for part in parts):
        return None
    return "/".join(parts)


def architecture_modules(workspace: Path, max_depth: int | None = 2) -> set[tuple[str, str]]:
    """Every module file, or directory holding Rust files, under crates/*/src
    down to `max_depth` path components (lib, main and tests excluded), as
    (crate directory, module path) pairs."""
    modules: set[tuple[str, str]] = set()
    for src in sorted(workspace.glob("crates/*/src")):
        for path in sorted(src.rglob("*")):
            relative = path.relative_to(src)
            if max_depth is not None and len(relative.parts) > max_depth:
                continue
            if path.is_dir() and not any(path.rglob("*.rs")):
                continue
            if path.is_file() and path.suffix != ".rs":
                continue
            if (name := _module_name(relative)) is not None:
                modules.add((src.parent.name, name))
    return modules


# A `#[test]` fn, past any further attributes (`#[ignore]`, `#[should_panic]`) and a visibility.
TEST_FN = re.compile(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")

# The first word of a module row and the crate whose `src/` its paths are relative to.
CRATE_QUALIFIERS = {"core": "horde_core", "game": "horde_game"}

OWNERSHIP_COLUMNS = ["Module (file)", "Owns", "Does not own", "Entry points / key types", "Guard tests"]


def _ownership_rows(text: str) -> tuple[list[list[str]], list[str]]:
    """The cells of each row of the table under the `## Ownership` heading,
    and the errors of rows that do not have its five columns."""
    rows: list[list[str]] = []
    errors: list[str] = []
    in_section = header_seen = False
    for line in text.splitlines():
        if line.startswith("## "):
            in_section = line.strip() == "## Ownership"
            continue
        stripped = line.strip()
        if not in_section or not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if len(cells) != len(OWNERSHIP_COLUMNS):
            errors.append(f"ARCHITECTURE.md: ownership row needs 5 columns: {stripped[:60]}")
        elif cells == OWNERSHIP_COLUMNS:
            header_seen = True
        elif not all(cell and set(cell) <= set("-: ") for cell in cells):
            rows.append(cells)
    if not header_seen:
        errors.append("ARCHITECTURE.md: no ownership table header under '## Ownership': "
                      + " | ".join(OWNERSHIP_COLUMNS))
    return rows, errors


def _module_token(token: str, crate: str | None) -> tuple[str | None, str]:
    """`art/camera.rs`, `ui/battle/mod.rs` and `sim/director` in a row of
    `crate`, or a full `crates/<crate>/src/...` path, as (crate, module path)."""
    name = token.strip().rstrip("/")
    if explicit := re.match(r"crates/([^/]+)/src/(.+)", name):
        crate, name = explicit.groups()
    if name.endswith("/mod.rs"):
        name = name[: -len("/mod.rs")]
    elif name.endswith(".rs"):
        name = name[: -len(".rs")]
    return crate, name


def check_architecture(workspace: Path) -> tuple[int, list[str]]:
    """docs/ARCHITECTURE.md's ownership table has a row for every module to
    depth 2, and every module or file a row names exists.

    A row documents a module when its first cell is `core` or `game` followed
    by the module's backticked path under that crate's `src`
    (`sim/director`, `art/camera.rs`, `ui/battle/mod.rs`). An unqualified
    row's tokens must be workspace-relative paths that exist
    (`scripts/measure_performance.ps1`). A row whose first cell backticks
    nothing, an unknown qualifier, or a path outside the qualifier's crate
    is an error. Every backticked guard test must be
    a Rust `#[test]` fn in crates/ or a workspace path that exists; a
    production fn of the same name does not count.
    """
    text = (workspace / "docs/ARCHITECTURE.md").read_text(encoding="utf-8-sig")
    rows, errors = _ownership_rows(text)
    required = architecture_modules(workspace)
    known = architecture_modules(workspace, max_depth=None)
    tests: set[str] = set()
    for path in workspace.glob("crates/*/**/*.rs"):
        if "target" not in path.relative_to(workspace).parts:
            tests.update(TEST_FN.findall(path.read_text(encoding="utf-8-sig")))
    short = {crate: qualifier for qualifier, crate in CRATE_QUALIFIERS.items()}
    documented: set[tuple[str, str]] = set()
    for cells in rows:
        first = cells[0]
        tokens = re.findall(r"`([^`]+)`", first)
        qualifier = first.split("`", 1)[0].strip()
        crate = CRATE_QUALIFIERS.get(qualifier)
        if not tokens:
            errors.append("ARCHITECTURE.md: ownership row names no backticked module or file: "
                          + (first or "(empty first cell)"))
        elif qualifier and crate is None:
            errors.append(f"ARCHITECTURE.md: unknown crate qualifier {qualifier!r} (use core or game): {first}")
        for token in tokens:
            token_crate, name = _module_token(token, crate)
            if crate is not None and token_crate != crate:
                errors.append(f"ARCHITECTURE.md: {qualifier} row names a "
                              f"{short.get(token_crate, token_crate)} path: {token}")
            elif token_crate is not None and (token_crate, name) in known:
                documented.add((token_crate, name))
            elif crate is not None:
                errors.append(f"ARCHITECTURE.md: {qualifier} row names no module in crates/{crate}/src: {token}")
            elif not (workspace / token).is_file():
                errors.append(f"ARCHITECTURE.md: row names no module or file: {token}")
        for token in re.findall(r"`([^`]+)`", cells[-1]):
            if token not in tests and not (workspace / token).is_file():
                errors.append(f"ARCHITECTURE.md: guard test not found: {token}")
    for crate, missing in sorted(required - documented):
        errors.append(f"ARCHITECTURE.md: no ownership row for {short.get(crate, crate)} module {missing}")
    if not required:
        errors.append("No crate modules found")
    return len(required), errors


# An inline Markdown link's target, as `](target)` or `](<target>)`, with an optional title after it.
INLINE_LINK = re.compile(r"\]\(\s*<?([^)\s>]+)>?[^)]*\)")
# An inline code span; links inside one are text.
CODE_SPAN = re.compile(r"`+[^`]*`+")
# The v1 audition reel (pre-production-pass), kept in place by ruling S19 and never packaged.
AUDITION_DIRECTORY = "docs/audio"
# The docs that record the loudness gate (user decision U3, acceptance G8.5, W7-03).
LOUDNESS_GATE_DOCS = ("docs/ACCEPTANCE.md", "docs/AUDIO_SURVIVAL.md", "docs/plans/2026-11-05-production-pass.md")
LOUDNESS_GATE_PHRASES = ("-23 ±2 LUFS integrated", "true peak ≤ 0.95 FS")


def curated_docs(workspace: Path) -> list[str]:
    """The docs the playtest package ships, relative to docs/, read from
    package_playtest.ps1's `$CuratedDocs = @(...)` line (repo_checks pins the
    same list)."""
    script = workspace / "scripts/package_playtest.ps1"
    if not script.is_file():
        return []
    match = re.search(r"^\$CuratedDocs\s*=\s*@\(([^)]*)\)", script.read_text(encoding="utf-8-sig"), re.MULTILINE)
    return re.findall(r"'([^']+)'", match.group(1)) if match else []


def _prose_lines(text: str):
    """(line number, line) of every line outside a fenced block, with inline
    code spans removed."""
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith(("```", "~~~")):
            fenced = not fenced
            continue
        if not fenced:
            yield number, CODE_SPAN.sub("", line)


def check_doc_links(workspace: Path) -> tuple[int, list[str]]:
    """Every relative inline link in docs/**/*.md resolves to a file or folder
    that exists; no doc links into docs/audio/ (the v1 audition reel, which
    is never packaged); a doc the playtest package ships names nothing in
    docs/audio/; and each of LOUDNESS_GATE_DOCS records the -23 LUFS gate.
    Web, mailto and in-page links are not checked. Returns the number of
    relative links and the errors."""
    errors: list[str] = []
    links = 0
    audition = (workspace / AUDITION_DIRECTORY).resolve()
    shipped = {f"docs/{name}" for name in curated_docs(workspace)}
    for path in sorted(workspace.glob("docs/**/*.md")):
        name = path.relative_to(workspace).as_posix()
        text = path.read_text(encoding="utf-8-sig")
        for number, line in _prose_lines(text):
            if name in shipped and AUDITION_DIRECTORY + "/" in line:
                errors.append(f"{name}:{number}: the playtest package leaves out {AUDITION_DIRECTORY}/; "
                              "a packaged doc names nothing there")
            for target in INLINE_LINK.findall(line):
                if re.match(r"(?i)(https?:|mailto:)", target) or target.startswith("#"):
                    continue
                links += 1
                resolved = (path.parent / unquote(target.split("#", 1)[0])).resolve()
                if not resolved.exists():
                    errors.append(f"{name}:{number}: dead link '{target}'")
                elif resolved == audition or audition in resolved.parents:
                    errors.append(f"{name}:{number}: links the v1 audition reel in {AUDITION_DIRECTORY}/, "
                                  f"which is never packaged: '{target}'")
    for name in LOUDNESS_GATE_DOCS:
        path = workspace / name
        if not path.is_file():
            errors.append(f"{name}: missing (it records the loudness gate)")
            continue
        text = path.read_text(encoding="utf-8-sig").replace("*", "")
        if not all(phrase in text for phrase in LOUDNESS_GATE_PHRASES):
            errors.append(f"{name}: does not record the loudness gate ("
                          + " and ".join(f"'{phrase}'" for phrase in LOUDNESS_GATE_PHRASES) + ")")
    return links, errors


if __name__ == "__main__":
    workspace = Path(__file__).resolve().parent.parent
    count, errors = check(workspace)
    listed, hierarchy_errors = check_view_hierarchy(workspace)
    errors += hierarchy_errors
    modules, architecture_errors = check_architecture(workspace)
    errors += architecture_errors
    links, link_errors = check_doc_links(workspace)
    errors += link_errors
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print(f"PASS: {count} Views; unique IDs, one root, valid parents, no cycles, existing sources")
    print(f"PASS: {VIEW_HIERARCHY_DOC} lists all {listed} Views under their parents, "
          f"the {len(PRODUCTION_VIEWS)} production-pass Views included")
    print(f"PASS: {modules} modules to depth 2 each have an ARCHITECTURE.md ownership row")
    print(f"PASS: {links} relative doc links resolve, none into {AUDITION_DIRECTORY}/; "
          f"the loudness gate is recorded in {len(LOUDNESS_GATE_DOCS)} docs")
