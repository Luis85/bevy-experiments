#!/usr/bin/env python3
"""Bounded, evidence-only local Markdown audit; no network access or source edits.

Run from any directory with Python 3.11+:
  python docs/verification/v0.23-review/audit_doc_links.py
  python docs/verification/v0.23-review/audit_doc_links.py --self-test

Current Validation is included. The current native/delivery READMEs and specialist reviews are included
automatically once present. --extra adds another repository-relative Markdown input; this does
not recursively audit every linked historical document. Linked Markdown targets
are read only to validate their heading/explicit-ID anchors. Reports go beside
this helper (or another --output-dir contained in the 0.23 review directory).
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import html
import json
from pathlib import Path
import re
import tempfile
import unicodedata
from urllib.parse import unquote, urlsplit


CURRENT_DOCS = (
    "README.md", "PRODUCT.md", "DESIGN.md", "CHANGELOG.md",
    "docs/PRODUCT_REVIEW.md", "docs/IMPLEMENTATION_PLAN_v0.23.md",
    "docs/RESEARCH_v0.23.md", "docs/REVIEW_GAPS_v0.23.md",
    "docs/COMPATIBILITY.md", "docs/DESKTOP_DELIVERY.md", "docs/CONTROLS.md",
    "docs/GAME_FLOW.md", "docs/PLAYTESTING.md", "docs/OPERATIONS.md",
    "docs/RECOVERY_BUNDLES.md", "docs/REGIONAL_PERSISTENCE.md",
    "docs/GAMEPLAY.md", "docs/LIVING_WORLD.md", "docs/SETTLEMENTS_AND_ECONOMY.md",
    "docs/INTEGRATION.md", "docs/MULTIPLAYER.md", "docs/NETWORK_REPLICATION.md",
    "docs/VALIDATION.md", "docs/ARCHITECTURE.md", "docs/RELEASE_HISTORY.md",
    "crates/terra-sandbox/README.md", "docs/GUIDANCE.md",
)
OPTIONAL_CURRENT_DOCS = (
    "docs/verification/v0.23-native/README.md",
    "docs/verification/v0.23-native/state-inspection.md",
    "docs/verification/v0.23-delivery/README.md",
    "docs/verification/v0.23-review/world-framework.md",
    "docs/verification/v0.23-review/player-experience.md",
    "docs/verification/v0.23-review/reliability-delivery.md",
    "docs/verification/v0.23-review/reliability-cross-review.md",
    "docs/verification/v0.23-review/world-reliability-cross-review.md",
    "docs/verification/v0.23-review/world-player-cross-review.md",
    "docs/verification/v0.23-review/player-reliability-cross-review.md",
    "docs/verification/v0.23-review/preservation.md",
    "docs/verification/v0.23-review/source-packaging-review.md",
)
MAX_DOCUMENT_BYTES = 4 * 1024 * 1024
MAX_DOCUMENTS = 64
MAX_LINKS = 8192
MAX_TARGETS = 1024


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def blank(value: str) -> str:
    """Mask syntax without losing the source line/column offsets."""
    return "".join("\n" if char == "\n" else " " for char in value)


def prose(text: str) -> str:
    text = re.sub(r"<!--[\s\S]*?-->", lambda match: blank(match[0]), text)
    output = []
    fence: tuple[str, int] | None = None
    for line in text.splitlines(keepends=True):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})", line)
        if fence is not None:
            output.append(blank(line))
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= fence[1]:
                if not line[marker.end():].strip():
                    fence = None
        elif marker:
            fence = (marker[1][0], len(marker[1]))
            output.append(blank(line))
        else:
            output.append(line)
    return "".join(output)


def mask_code(text: str) -> str:
    return re.sub(r"(`+)([\s\S]*?)\1", lambda match: blank(match[0]), text)


def unescape(value: str) -> str:
    return re.sub(r"\\([\\`*{}\[\]()#+.!_<> -])", r"\1", html.unescape(value))


def close_delimiter(text: str, start: int, opening: str, closing: str) -> int | None:
    depth = 0
    index = start
    while index < len(text):
        char = text[index]
        if char == "\\":
            index += 2
            continue
        if char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return index
        index += 1
    return None


def destination(body: str) -> str:
    body = body.lstrip()
    if body.startswith("<"):
        closing = body.find(">", 1)
        if closing < 0:
            raise ValueError("unterminated angle-bracket link destination")
        return unescape(body[1:closing])
    depth = 0
    for index, char in enumerate(body):
        if char.isspace() and depth == 0:
            return unescape(body[:index])
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
    return unescape(body)


def reference_key(value: str) -> str:
    return re.sub(r"\s+", " ", unescape(value)).strip().casefold()


def links(text: str) -> tuple[list[dict], list[dict]]:
    visible = mask_code(prose(text))
    definitions = {}
    definition_pattern = re.compile(r"^ {0,3}\[([^\]\n]+)\]:[ \t]*(<[^>\n]*>|\S+)(?:[^\n]*)$", re.M)
    for match in definition_pattern.finditer(visible):
        definitions[reference_key(match[1])] = destination(match[2])
    visible = definition_pattern.sub(lambda match: blank(match[0]), visible)
    found: list[dict] = []
    issues: list[dict] = []
    index = 0
    while index < len(visible):
        if visible[index] != "[" or (index > 0 and visible[index - 1] == "\\"):
            index += 1
            continue
        end = close_delimiter(visible, index, "[", "]")
        if end is None:
            index += 1
            continue
        label = visible[index + 1:end]
        line = visible.count("\n", 0, index) + 1
        after = end + 1
        target = None
        kind = "image" if index and visible[index - 1] == "!" else "link"
        if after < len(visible) and visible[after] == "(":
            finish = close_delimiter(visible, after, "(", ")")
            if finish is None:
                issues.append({"line": line, "kind": "unclosed_inline_link", "label": label})
            else:
                try:
                    target = destination(visible[after + 1:finish])
                except ValueError as error:
                    issues.append({"line": line, "kind": "unparsed_destination", "message": str(error)})
                after = finish + 1
        elif after < len(visible) and visible[after] == "[":
            finish = close_delimiter(visible, after, "[", "]")
            if finish is not None:
                key = reference_key(visible[after + 1:finish] or label)
                target = definitions.get(key)
                if target is None:
                    issues.append({"line": line, "kind": "undefined_explicit_reference", "label": key})
                after = finish + 1
        else:
            target = definitions.get(reference_key(label))
        if target is not None:
            found.append({"line": line, "kind": kind, "label": label, "target": target})
        index = after
    for match in re.finditer(r"<(?:a|img)\b[^>]*?\b(?:href|src)\s*=\s*([\"'])(.*?)\1[^>]*>", visible, re.I | re.S):
        found.append({"line": visible.count("\n", 0, match.start()) + 1,
                      "kind": "html", "label": "", "target": html.unescape(match[2])})
    return found, issues


def heading_text(value: str) -> str:
    # Preserve inline code text (including <T>) before stripping actual HTML.
    saved: list[str] = []
    def retain(match: re.Match) -> str:
        saved.append(match[2])
        return f"CODEPLACEHOLDER{len(saved) - 1}TOKEN"
    value = re.sub(r"(`+)(.*?)\1", retain, value)
    value = re.sub(r"!?\[([^\]]+)\]\([^\n]*?\)", r"\1", value)
    value = re.sub(r"<[^>]+>", "", value)
    value = re.sub(r"(?<!\w)_{1,3}([^_\n]+)_{1,3}(?!\w)", r"\1", value)
    for index, code in enumerate(saved):
        value = value.replace(f"CODEPLACEHOLDER{index}TOKEN", code)
    return unescape(value)


def heading_slug(value: str) -> str:
    value = heading_text(value).lower()
    value = "".join(char for char in value
                    if char in "_-" or not unicodedata.category(char).startswith(("P", "S", "C")))
    return re.sub(r"\s", "-", value)


def headings(text: str) -> tuple[list[dict], set[str], list[dict]]:
    visible = prose(text)
    source_lines = visible.splitlines()
    output = []
    used: set[str] = set()
    hierarchy = []
    previous = 0
    for index, line in enumerate(source_lines):
        match = re.match(r"^ {0,3}(#{1,6})(?:[ \t]+(.*)|[ \t]*)$", line)
        if match:
            level = len(match[1])
            title = re.sub(r"[ \t]+#+[ \t]*$", "", match[2] or "").strip()
        elif (index + 1 < len(source_lines) and line.strip()
              and re.fullmatch(r" {0,3}(?:=+|-+)[ \t]*", source_lines[index + 1])
              and not re.match(r"^\s*(?:[-*+] |\d+[.)] |[|>])", line)):
            level = 1 if "=" in source_lines[index + 1] else 2
            title = line.strip()
        else:
            continue
        base = heading_slug(title)
        anchor = base
        suffix = 0
        while anchor in used:
            suffix += 1
            anchor = f"{base}-{suffix}"
        used.add(anchor)
        output.append({"line": index + 1, "level": level, "title": heading_text(title), "anchor": anchor})
        if previous and level > previous + 1:
            hierarchy.append({"line": index + 1, "previous_level": previous, "level": level})
        previous = level
    explicit = {html.unescape(match[2]) for match in re.finditer(
        r"<(?:a|h[1-6])\b[^>]*?\b(?:id|name)\s*=\s*([\"'])(.*?)\1[^>]*>", visible, re.I | re.S)}
    return output, used | explicit, hierarchy


class Audit:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.cache: dict[Path, dict] = {}
        self.destinations: set[Path] = set()

    def read(self, path: Path) -> dict:
        if path not in self.cache:
            if len(self.cache) >= MAX_TARGETS:
                raise ValueError("Markdown/line-target read bound exceeded")
            with path.open("rb") as stream:
                data = stream.read(MAX_DOCUMENT_BYTES + 1)
            if len(data) > MAX_DOCUMENT_BYTES:
                raise ValueError("document exceeds the 4 MiB read bound")
            text = data.decode("utf-8")
            title_list, anchors, hierarchy = headings(text)
            self.cache[path] = {"text": text, "sha256": sha(data), "bytes": len(data),
                                "headings": title_list, "anchors": anchors, "hierarchy": hierarchy}
        return self.cache[path]

    def check(self, source: Path, link: dict) -> dict:
        result = dict(link)
        parsed = urlsplit(link["target"])
        if parsed.scheme or parsed.netloc:
            result.update(status="external_skipped", scheme=parsed.scheme or "protocol-relative")
            return result
        relative = unquote(parsed.path)
        if relative.startswith("/"):
            target = self.root / relative.lstrip("/")
        else:
            target = source.parent / relative if relative else source
        target = target.resolve()
        if not target.is_relative_to(self.root):
            result.update(status="outside_repository")
            return result
        result["resolved_path"] = target.relative_to(self.root).as_posix()
        if not target.exists():
            result.update(status="missing_destination")
            return result
        self.destinations.add(target)
        if len(self.destinations) > MAX_TARGETS:
            raise ValueError("local destination count exceeds its bound")
        fragment = unquote(parsed.fragment)
        if not fragment:
            result.update(status="valid_destination")
            return result
        if target.is_dir():
            candidates = [target / name for name in ("README.md", "readme.md", "README.markdown")]
            target = next((candidate for candidate in candidates if candidate.is_file()), target)
        result["fragment"] = fragment
        if target.suffix.lower() in {".md", ".markdown"} and target.is_file():
            metadata = self.read(target)
            result["anchor_document"] = target.relative_to(self.root).as_posix()
            if fragment in metadata["anchors"]:
                result.update(status="valid_anchor")
            else:
                result.update(status="missing_anchor", available_anchors=sorted(metadata["anchors"]))
        elif (line_range := re.fullmatch(r"L([1-9][0-9]*)(?:-L([1-9][0-9]*))?", fragment)) and target.is_file():
            line_count = len(self.read(target)["text"].splitlines())
            start, end = int(line_range[1]), int(line_range[2] or line_range[1])
            result.update(status="valid_line_anchor" if 1 <= start <= end <= line_count else "missing_line_anchor")
        else:
            result.update(status="non_markdown_anchor_unchecked")
        return result

    def document(self, relative: str) -> dict:
        path = (self.root / relative).resolve()
        if not path.is_relative_to(self.root):
            return {"path": relative, "error": "input escapes the repository"}
        try:
            source = self.read(path)
            found, parsing = links(source["text"])
            if len(found) > MAX_LINKS:
                raise ValueError("document link count exceeds its bound")
            results = []
            for link in found:
                try:
                    results.append(self.check(path, link))
                except (OSError, UnicodeError, ValueError) as error:
                    results.append({**link, "status": "read_or_parse_error", "message": str(error)})
            return {"path": relative, "sha256": source["sha256"], "bytes": source["bytes"],
                    "headings": source["headings"], "heading_level_jumps": source["hierarchy"],
                    "parse_findings": parsing, "links": results}
        except (OSError, UnicodeError, ValueError) as error:
            return {"path": relative, "error": str(error)}


def self_test() -> None:
    text = '# A & B\n## A & B\n## A & B\n\n```md\n[ignored](absent.md)\n```\n`[also ignored](absent.md)`\n[local](<folder/a(b).md#part>)\n[ref][target]\n[target]: real.md#two\n'
    found, issues = links(text)
    assert not issues and [item['target'] for item in found] == ['folder/a(b).md#part', 'real.md#two']
    listed, anchors, _ = headings(text)
    assert [item['anchor'] for item in listed] == ['a--b', 'a--b-1', 'a--b-2']
    assert heading_slug('`Result<T>` and _label_') == 'resultt-and-label'
    assert 'explicit' in headings('<a id="explicit"></a>\n# Other')[1]
    with tempfile.TemporaryDirectory(prefix='terra-doc-link-audit-') as temporary:
        root = Path(temporary)
        (root / 'source.md').write_text('# Source\n')
        (root / 'target.md').write_text('# Target\n## Same\n## Same\n')
        checker = Audit(root)
        for target, status in [('target.md#same-1', 'valid_anchor'), ('target.md#lost', 'missing_anchor'),
                               ('missing.md', 'missing_destination'), ('https://invalid.example/no-fetch', 'external_skipped')]:
            assert checker.check(root / 'source.md', {'target': target})['status'] == status
    print(json.dumps({'self_test': 'passed', 'scope': 'code exclusion, balanced destinations, references, duplicate/inline/explicit anchors, real local target/refusal checks; no HTTP'}))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--extra', action='append', default=[], metavar='RELATIVE_MD')
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--report-prefix', default='doc-links')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    root = args.root.resolve()
    evidence = root / 'docs/verification/v0.23-review'
    output = (args.output_dir or evidence).resolve()
    if not output.is_relative_to(evidence.resolve()) or not re.fullmatch(r'[A-Za-z0-9_-]+', args.report_prefix):
        parser.error('reports must stay inside v0.23-review and use a simple filename prefix')
    optional_present = [path for path in OPTIONAL_CURRENT_DOCS if (root / path).is_file()]
    optional_pending = [path for path in OPTIONAL_CURRENT_DOCS if path not in optional_present]
    roster = list(dict.fromkeys((*CURRENT_DOCS, *optional_present, *args.extra)))
    if len(roster) > MAX_DOCUMENTS:
        parser.error('input document limit exceeded')
    checker = Audit(root)
    started = datetime.now(timezone.utc).isoformat()
    documents = [checker.document(relative) for relative in roster]
    counts = Counter(link['status'] for document in documents for link in document.get('links', []))
    valid = {'valid_destination', 'valid_anchor', 'valid_line_anchor', 'external_skipped', 'non_markdown_anchor_unchecked'}
    errors = [{"document": document['path'], **link} for document in documents
              for link in document.get('links', []) if link['status'] not in valid]
    errors += [{"document": document['path'], "status": 'document_error', "message": document['error']}
               for document in documents if 'error' in document]
    errors += [{"document": document['path'], "status": 'markdown_parse_finding', **finding}
               for document in documents for finding in document.get('parse_findings', [])]
    changed = []
    for document in documents:
        if 'sha256' not in document:
            continue
        try:
            with (root / document['path']).open('rb') as stream:
                current = stream.read(MAX_DOCUMENT_BYTES + 1)
            if sha(current) != document['sha256']:
                changed.append(document['path'])
        except OSError:
            changed.append(document['path'])
    result = {'schema_version': 1, 'scope': 'Bounded current-handoff local Markdown destinations and heading anchors; evidence only.',
              'started_utc': started, 'finished_utc': datetime.now(timezone.utc).isoformat(),
              'status': 'failed' if errors or changed else 'passed',
              'helper_sha256': sha(Path(__file__).read_bytes()), 'network_requests': 0,
              'input_documents': len(documents), 'optional_pending_documents': optional_pending,
              'unique_existing_local_destinations': len(checker.destinations),
              'link_status_counts': dict(sorted(counts.items())), 'broken_or_unparsed': errors,
              'changed_inputs_during_audit': changed, 'documents': documents,
              'limits': ['No HTTP/external fetching and no target content correctness review.',
                         'Local links are checked one hop; linked historical documents are not recursively audited.',
                         'GitHub-style ATX/Setext/duplicate heading slugs and explicit HTML IDs are supported; not a full CommonMark renderer.',
                         'Fenced/inline code and HTML comments are excluded; complex custom HTML, MDX, multiline reference definitions and indented code require manual inspection.',
                         'Non-Markdown fragments other than source line ranges are explicitly reported unchecked.',
                         'Root-leading local paths are resolved relative to the repository root; no read outside that root.',
                         'This active-tree audit is not final native, canonical test or packaged-archive acceptance.']}
    output.mkdir(parents=True, exist_ok=True)
    report = output / f'{args.report_prefix}.json'
    report.write_text(json.dumps(result, indent=2) + '\n')
    lines = ['# Current handoff documentation link audit', '',
             '**Status: ' + result['status'] + '.** This checks local destinations and heading anchors in the listed current documents. It does not fetch HTTP destinations, certify linked evidence as current, or replace final source/archive acceptance.', '',
             f"Audited {len(documents)} documents and {sum(counts.values())} Markdown/HTML links. Existing local destinations: {len(checker.destinations)}. Network requests: 0.", '',
             '| Link result | Count |', '|---|---:|']
    lines += [f'| {name} | {count} |' for name, count in sorted(counts.items())]
    lines += ['', '## Findings', '']
    if errors:
        lines += [f"- {item['document']}:{item.get('line', '?')} — {item['status']}: {item.get('target', item.get('message', item.get('label', '')))}" for item in errors]
    else:
        lines.append('No broken local destination, missing supported anchor or unparsed explicit reference was found.')
    lines += ['', 'Optional documents not yet present: ' + (', '.join(optional_pending) or 'none') + '.',
              'Inputs changed during this audit: ' + (', '.join(changed) or 'none') + '.', '',
              '## Inputs and heading structure', '', '| Document | Headings | Links | Level jumps |', '|---|---:|---:|---:|']
    lines += [f"| {item['path']} | {len(item.get('headings', []))} | {len(item.get('links', []))} | {len(item.get('heading_level_jumps', []))} |" for item in documents]
    lines += ['', 'The JSON companion retains every input hash, generated heading anchor and observed link result. Heading-level jumps are informational; duplicate headings receive normal numbered anchors.', '',
              '## Rerun', '', '```sh', 'python3 docs/verification/v0.23-review/audit_doc_links.py --self-test',
              'python3 docs/verification/v0.23-review/audit_doc_links.py', '```', '',
              'Current Validation is always included. Current native/delivery READMEs and specialist reviews join the default roster automatically once present. Use --extra RELATIVE_MD for another input and --report-prefix NAME to retain a separately named report. Reports stay under the current review evidence directory.', '',
              '## Limits', '']
    lines += ['- ' + limit for limit in result['limits']]
    lines += ['', f"Observation: {started} to {result['finished_utc']}.", '']
    report.with_suffix('.md').write_text('\n'.join(lines))
    print(json.dumps({key: result[key] for key in ('status', 'input_documents', 'optional_pending_documents', 'link_status_counts', 'broken_or_unparsed', 'changed_inputs_during_audit')}, indent=2))
    return 1 if errors or changed else 0


if __name__ == '__main__':
    raise SystemExit(main())
