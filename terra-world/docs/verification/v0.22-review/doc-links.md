# Current handoff documentation link audit

**Status: passed.** This checks local destinations and heading anchors in the listed current documents. It does not fetch HTTP destinations, certify linked evidence as current, or replace final source/archive acceptance.

Audited 23 documents and 287 Markdown/HTML links. Existing local destinations: 125. Network requests: 0.

| Link result | Count |
|---|---:|
| external_skipped | 27 |
| valid_anchor | 17 |
| valid_destination | 243 |

## Findings

No broken local destination, missing supported anchor or unparsed explicit reference was found.

Optional documents not yet present: docs/verification/v0.22-native/README.md.
Inputs changed during this audit: none.

## Inputs and heading structure

| Document | Headings | Links | Level jumps |
|---|---:|---:|---:|
| README.md | 18 | 69 | 0 |
| PRODUCT.md | 16 | 6 | 0 |
| DESIGN.md | 16 | 5 | 0 |
| CHANGELOG.md | 20 | 7 | 0 |
| docs/PRODUCT_REVIEW.md | 13 | 12 | 0 |
| docs/IMPLEMENTATION_PLAN_v0.22.md | 18 | 4 | 0 |
| docs/RESEARCH_v0.22.md | 15 | 24 | 0 |
| docs/REVIEW_GAPS_v0.22.md | 6 | 4 | 0 |
| docs/COMPATIBILITY.md | 8 | 8 | 0 |
| docs/DESKTOP_DELIVERY.md | 15 | 18 | 0 |
| docs/CONTROLS.md | 12 | 0 | 0 |
| docs/GAME_FLOW.md | 18 | 8 | 0 |
| docs/PLAYTESTING.md | 16 | 7 | 0 |
| docs/OPERATIONS.md | 18 | 12 | 0 |
| docs/RECOVERY_BUNDLES.md | 7 | 5 | 0 |
| docs/REGIONAL_PERSISTENCE.md | 9 | 8 | 0 |
| docs/GAMEPLAY.md | 21 | 17 | 0 |
| docs/LIVING_WORLD.md | 12 | 13 | 0 |
| docs/SETTLEMENTS_AND_ECONOMY.md | 11 | 5 | 0 |
| docs/INTEGRATION.md | 20 | 28 | 0 |
| docs/MULTIPLAYER.md | 13 | 6 | 0 |
| docs/NETWORK_REPLICATION.md | 14 | 11 | 0 |
| docs/VALIDATION.md | 7 | 10 | 0 |

The JSON companion retains every input hash, generated heading anchor and observed link result. Heading-level jumps are informational; duplicate headings receive normal numbered anchors.

## Rerun

```sh
python3 docs/verification/v0.22-review/audit_doc_links.py --self-test
python3 docs/verification/v0.22-review/audit_doc_links.py
```

Current Validation is always included. The current native README joins the default roster automatically once present. Use --extra RELATIVE_MD for another input and --report-prefix NAME to retain a separately named report. Reports stay under the current review evidence directory.

## Limits

- No HTTP/external fetching and no target content correctness review.
- Local links are checked one hop; linked historical documents are not recursively audited.
- GitHub-style ATX/Setext/duplicate heading slugs and explicit HTML IDs are supported; not a full CommonMark renderer.
- Fenced/inline code and HTML comments are excluded; complex custom HTML, MDX, multiline reference definitions and indented code require manual inspection.
- Non-Markdown fragments other than source line ranges are explicitly reported unchecked.
- Root-leading local paths are resolved relative to the repository root; no read outside that root.
- This active-tree audit is not final native, canonical test or packaged-archive acceptance.

Observation: 2026-10-29T21:05:02.171352+00:00 to 2026-10-29T21:05:02.297038+00:00.
