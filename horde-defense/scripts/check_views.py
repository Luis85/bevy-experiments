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


if __name__ == "__main__":
    count, errors = check(Path(__file__).resolve().parent.parent)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print(f"PASS: {count} Views; unique IDs, one root, valid parents, no cycles, existing sources")
