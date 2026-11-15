"""Check the capture-findings ledger of the production-pass plan (W7-11, design-qa-captures 1.12).

Standard library only. Run:
  python -I -B scripts/check_capture_findings.py docs/plans/2026-11-05-production-pass.md [--final]

The ledger is the table between `<!-- capture-findings:begin -->` and `<!-- capture-findings:end -->`:
  | ID | Wave | Scene | Size | Mode | Finding | Severity | Owner | Status | Fix commit | After evidence | Deferral |

Every row is checked for:
- ID `CF-<wave>-<nn>`, unique, its wave equal to the Wave cell; a Scene, a Finding and an Owner;
- Size `1440x900`, `1050x700@1.35` or `both`; Mode `normal`, `reduced` or `both`;
  Severity blocker, major, minor or polish; Status open, fixed or deferred;
- a fixed row: one or more comma-separated Fix commits of 7-40 hex characters (each must name a commit of
  the repository unless --no-git), and After evidence, comma-separated, each an existing
  `docs/screenshots/production/after-*.png` (an optional `#<sha12>` is checked) or
  `target/production-after/...png#<sha12>`, the first 12 hex characters of the file's SHA-256. A bare
  file name (`frame-0022.png#...`) lies in the folder of the evidence before it. `target/` is the
  workspace target folder or CARGO_TARGET_DIR, whichever holds the file;
- a deferred row: Deferral exactly `approved by user YYYY-MM-DD` (given by the user in chat), a real date;
  no other row carries a Deferral.
--final also fails on any open row, and needs docs/screenshots/production with captures-before.json,
captures-after.json, a before-<x>.png for every after-<x>.png and the reverse, and at most
40,000,000 B in all (the pair budget).

Exit codes: 0 pass, 1 a problem (each printed as a FAIL line).
"""
import argparse
import datetime
import hashlib
import os
import re
import subprocess
import sys
from pathlib import Path

BEGIN = "<!-- capture-findings:begin -->"
END = "<!-- capture-findings:end -->"
COLUMNS = ["ID", "Wave", "Scene", "Size", "Mode", "Finding", "Severity", "Owner", "Status",
           "Fix commit", "After evidence", "Deferral"]
SIZES = {"1440x900", "1050x700@1.35", "both"}
MODES = {"normal", "reduced", "both"}
SEVERITIES = {"blocker", "major", "minor", "polish"}
STATUSES = {"open", "fixed", "deferred"}
ID = re.compile(r"^CF-(\d+)-(\d{2})$")
COMMIT = re.compile(r"^[0-9a-f]{7,40}$")
SHA12 = re.compile(r"^[0-9a-f]{12}$")
DEFERRAL = re.compile(r"^approved by user (\d{4}-\d{2}-\d{2})$")
PAIRS = "docs/screenshots/production"
INDEXES = ["captures-before.json", "captures-after.json"]
PAIR_BUDGET_BYTES = 40_000_000


def default_root(plan: Path) -> Path:
    """The workspace that holds `plan`: the folder above its `docs` folder, else the working folder."""
    plan = plan.resolve()
    for parent in plan.parents:
        if parent.name == "docs":
            return parent.parent
    return Path.cwd().resolve()


def split_row(line: str) -> list[str]:
    text = line.strip()
    if text.startswith("|"):
        text = text[1:]
    if text.endswith("|"):
        text = text[:-1]
    return [cell.strip() for cell in text.split("|")]


def ledger_rows(text: str) -> tuple[list[list[str]], list[str]]:
    """The ledger's data rows as cells, or the problems that stop reading it."""
    if BEGIN not in text or END not in text or text.index(BEGIN) > text.index(END):
        return [], [f"no capture-findings table: the plan needs {BEGIN} ... {END}"]
    body = text[text.index(BEGIN) + len(BEGIN):text.index(END)]
    lines = [line for line in body.splitlines() if line.strip()]
    if len(lines) < 2 or split_row(lines[0]) != COLUMNS or not set("".join(split_row(lines[1]))) <= set("-: "):
        return [], ["ledger header must read | " + " | ".join(COLUMNS) + " | followed by its |---| row"]
    return [split_row(line) for line in lines[2:]], []


def target_folders(root: Path) -> list[Path]:
    folders = []
    if os.environ.get("CARGO_TARGET_DIR"):
        folders.append(Path(os.environ["CARGO_TARGET_DIR"]))
    folders.append(root / "target")
    return folders


def sha12(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()[:12]


def check_evidence(label: str, cell: str, root: Path) -> list[str]:
    problems = []
    folder = None
    for item in [part.strip() for part in cell.split(",") if part.strip()]:
        relative, _, digest = item.partition("#")
        if "/" not in relative:
            if folder is None:
                problems.append(f"{label}: {relative} has no earlier evidence folder")
                continue
            relative = folder + "/" + relative
        folder = relative.rsplit("/", 1)[0]
        committed = re.fullmatch(re.escape(PAIRS) + r"/after-[^/]+\.png", relative) is not None
        built = relative.startswith("target/production-after/") and relative.endswith(".png")
        if not committed and not built:
            problems.append(f"{label}: evidence must be {PAIRS}/after-*.png or "
                            "target/production-after/...png#<sha12>, not " + item)
            continue
        if digest and not SHA12.fullmatch(digest):
            problems.append(f"{label}: {relative} carries #{digest}, not 12 hex characters")
            continue
        if built and not digest:
            problems.append(f"{label}: {relative} needs its #<sha12>")
            continue
        if committed:
            candidates = [root / relative]
        else:
            tail = relative[len("target/"):]
            candidates = [base / tail for base in target_folders(root)]
        found = next((path for path in candidates if path.is_file()), None)
        if found is None:
            problems.append(f"{label}: evidence not found: {relative}")
        elif digest and sha12(found) != digest:
            problems.append(f"{label}: {relative} hashes to {sha12(found)}, not {digest}")
    return problems


def commit_exists(root: Path, commit: str) -> bool | None:
    """Whether git knows `commit`; None when git cannot be asked."""
    try:
        result = subprocess.run(["git", "-C", str(root), "cat-file", "-e", commit + "^{commit}"],
                                capture_output=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    return result.returncode == 0


def check_row(cells: list[str], root: Path, git: bool) -> list[str]:
    (ident, wave, scene, size, mode, finding, severity, owner, status, commits, evidence,
     deferral) = cells
    label = ident or "(no ID)"
    problems = []
    match = ID.fullmatch(ident)
    if match is None:
        problems.append(f"{label}: ID must read CF-<wave>-<nn>")
    elif wave != match.group(1):
        problems.append(f"{label}: Wave {wave} differs from the ID's wave {match.group(1)}")
    for value, name in [(scene, "Scene"), (finding, "Finding"), (owner, "Owner")]:
        if not value:
            problems.append(f"{label}: no {name}")
    for value, allowed, name in [(size, SIZES, "Size"), (mode, MODES, "Mode"),
                                 (severity, SEVERITIES, "Severity"), (status, STATUSES, "Status")]:
        if value not in allowed:
            problems.append(f"{label}: {name} {value!r} is not one of {', '.join(sorted(allowed))}")
    if status == "fixed":
        listed = [part.strip() for part in commits.split(",") if part.strip()]
        if not listed:
            problems.append(f"{label}: a fixed row needs a Fix commit")
        for commit in listed:
            if not COMMIT.fullmatch(commit):
                problems.append(f"{label}: Fix commit {commit!r} is not 7-40 hex characters")
            elif git and commit_exists(root, commit) is False:
                problems.append(f"{label}: Fix commit {commit} is not a commit of this repository")
        if not evidence:
            problems.append(f"{label}: a fixed row needs After evidence")
        else:
            problems.extend(check_evidence(label, evidence, root))
    if status == "deferred":
        if not deferral:
            problems.append(f"{label}: a deferred row needs Deferral 'approved by user YYYY-MM-DD'")
        else:
            approved = DEFERRAL.fullmatch(deferral)
            if approved is None:
                problems.append(f"{label}: Deferral must read 'approved by user YYYY-MM-DD', not {deferral!r}")
            else:
                try:
                    datetime.date.fromisoformat(approved.group(1))
                except ValueError:
                    problems.append(f"{label}: Deferral date {approved.group(1)} is not a date")
    elif deferral:
        problems.append(f"{label}: only a deferred row carries a Deferral")
    return problems


def check_pairs(root: Path) -> list[str]:
    folder = root / PAIRS
    if not folder.is_dir():
        return [f"{PAIRS} is missing (the committed before/after pairs)"]
    problems = []
    for index in INDEXES:
        if not (folder / index).is_file():
            problems.append(f"{PAIRS}/{index} is missing")
    files = [path for path in folder.rglob("*") if path.is_file()]
    total = sum(path.stat().st_size for path in files)
    if total > PAIR_BUDGET_BYTES:
        problems.append(f"{PAIRS} holds {total:,} B, over the {PAIR_BUDGET_BYTES:,} B budget")
    names = {path.name for path in folder.glob("*.png")}
    for name in sorted(names):
        for mine, other in [("after-", "before-"), ("before-", "after-")]:
            if name.startswith(mine) and other + name[len(mine):] not in names:
                problems.append(f"{PAIRS}/{name} has no {other + name[len(mine):]}")
    return problems


def check(plan: Path, root: Path, final: bool = False, git: bool = True) -> tuple[list[str], dict]:
    """Every problem of `plan`'s ledger, and the row count per status."""
    rows, problems = ledger_rows(plan.read_text(encoding="utf-8-sig"))
    counts = {status: 0 for status in sorted(STATUSES)}
    seen = set()
    for cells in rows:
        if len(cells) != len(COLUMNS):
            problems.append(f"{cells[0] or '(no ID)'}: a row needs {len(COLUMNS)} cells, not {len(cells)}")
            continue
        if cells[0] in seen:
            problems.append(f"{cells[0]}: duplicate ID")
        seen.add(cells[0])
        problems.extend(check_row(cells, root, git))
        if cells[8] in counts:
            counts[cells[8]] += 1
        if final and cells[8] == "open":
            problems.append(f"{cells[0]}: open; --final needs every row fixed or deferred with user approval")
    if final:
        problems.extend(check_pairs(root))
    return problems, counts


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("plan", type=Path, help="the plan file holding the ledger")
    parser.add_argument("--final", action="store_true",
                        help="also fail on open rows and check the committed pair folder")
    parser.add_argument("--root", type=Path, help="the workspace (default: the folder above docs/)")
    parser.add_argument("--no-git", action="store_true", help="do not ask git whether each Fix commit exists")
    args = parser.parse_args(argv)
    root = (args.root or default_root(args.plan)).resolve()
    try:
        problems, counts = check(args.plan, root, final=args.final, git=not args.no_git)
    except OSError as error:
        print(f"FAIL: cannot read {args.plan}: {error}")
        return 1
    for problem in problems:
        print(f"FAIL: {problem}")
    if problems:
        print(f"FAIL: {len(problems)} problems in the capture-findings ledger")
        return 1
    total = sum(counts.values())
    print(f"PASS: {total} capture findings ({counts['open']} open, {counts['fixed']} fixed, "
          f"{counts['deferred']} deferred){' and the pairs, --final' if args.final else ''}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
