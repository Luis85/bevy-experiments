"""Check the documented native View graph without third-party dependencies."""
from pathlib import Path
import re
import sys


def check(workspace: Path) -> tuple[int, list[str]]:
    views: dict[str, tuple[Path, dict[str, str]]] = {}
    errors: list[str] = []
    for path in sorted((workspace / "docs/components").glob("*.md")):
        text = path.read_text(encoding="utf-8-sig")
        parts = text.split("---", 2)
        if len(parts) != 3 or parts[0].strip():
            errors.append(f"{path.name}: missing YAML front matter")
            continue
        fields = {}
        for line in parts[1].splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                fields[key.strip()] = value.strip().strip('"').strip("'")
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
        if not parts[2].strip():
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


def architecture_modules(workspace: Path, max_depth: int | None = 2) -> set[str]:
    """Every module file, or directory holding Rust files, under crates/*/src
    down to `max_depth` path components (lib, main and tests excluded)."""
    modules: set[str] = set()
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
                modules.add(name)
    return modules


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
        elif not set(cells[0]) <= set("-: "):
            rows.append(cells)
    if not header_seen:
        errors.append("ARCHITECTURE.md: no ownership table header under '## Ownership': "
                      + " | ".join(OWNERSHIP_COLUMNS))
    return rows, errors


def _module_token(token: str) -> str | None:
    """`art/camera.rs`, `ui/battle/mod.rs` and `sim/director` as module paths."""
    name = re.sub(r"^crates/[^/]+/src/", "", token.strip().rstrip("/"))
    if name.endswith("/mod.rs"):
        name = name[: -len("/mod.rs")]
    elif name.endswith(".rs"):
        name = name[: -len(".rs")]
    return name


def check_architecture(workspace: Path) -> tuple[int, list[str]]:
    """docs/ARCHITECTURE.md's ownership table has a row for every module to
    depth 2, and every module or file a row names exists.

    A row documents a module when its first cell backticks the module's path
    under `src` (`sim/director`, `art/camera.rs`, `ui/battle/mod.rs`). Other
    first-cell tokens must be workspace-relative paths that exist
    (`scripts/measure_performance.ps1`). Every backticked guard test must be
    a Rust `fn` in crates/ or a workspace path that exists.
    """
    text = (workspace / "docs/ARCHITECTURE.md").read_text(encoding="utf-8-sig")
    rows, errors = _ownership_rows(text)
    required = architecture_modules(workspace)
    known = architecture_modules(workspace, max_depth=None)
    functions: set[str] = set()
    for path in workspace.glob("crates/*/**/*.rs"):
        if "target" not in path.relative_to(workspace).parts:
            functions.update(re.findall(r"\bfn\s+(\w+)", path.read_text(encoding="utf-8-sig")))
    documented: set[str] = set()
    for cells in rows:
        for token in re.findall(r"`([^`]+)`", cells[0]):
            name = _module_token(token)
            if name in known:
                documented.add(name)
            elif not (workspace / token).is_file():
                errors.append(f"ARCHITECTURE.md: row names no module or file: {token}")
        for token in re.findall(r"`([^`]+)`", cells[-1]):
            if token not in functions and not (workspace / token).is_file():
                errors.append(f"ARCHITECTURE.md: guard test not found: {token}")
    for missing in sorted(required - documented):
        errors.append(f"ARCHITECTURE.md: no ownership row for module {missing}")
    if not required:
        errors.append("No crate modules found")
    return len(required), errors


if __name__ == "__main__":
    workspace = Path(__file__).resolve().parent.parent
    count, errors = check(workspace)
    modules, architecture_errors = check_architecture(workspace)
    errors += architecture_errors
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print(f"PASS: {count} Views; unique IDs, one root, valid parents, no cycles, existing sources")
    print(f"PASS: {modules} modules to depth 2 each have an ARCHITECTURE.md ownership row")
