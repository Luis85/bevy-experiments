# Current handoff documentation link audit

**Status: failed.** This checks local destinations and heading anchors in the listed current documents. It does not fetch HTTP destinations, certify linked evidence as current, or replace final source/archive acceptance.

Audited 39 documents and 512 Markdown/HTML links. Existing local destinations: 228. Network requests: 0.

| Link result | Count |
|---|---:|
| external_skipped | 41 |
| missing_destination | 2 |
| valid_anchor | 18 |
| valid_destination | 451 |

## Findings

- docs/VALIDATION.md:78 — missing_destination: verification/v0.23-preflight/reliability-python-polish-tests.json
- docs/VALIDATION.md:78 — missing_destination: verification/v0.23-preflight/reliability-python-polish-tests.log

Optional documents not yet present: none.
Inputs changed during this audit: none.

## Inputs and heading structure

| Document | Headings | Links | Level jumps |
|---|---:|---:|---:|
| README.md | 18 | 69 | 0 |
| PRODUCT.md | 16 | 6 | 0 |
| DESIGN.md | 16 | 5 | 0 |
| CHANGELOG.md | 21 | 8 | 0 |
| docs/PRODUCT_REVIEW.md | 14 | 15 | 0 |
| docs/IMPLEMENTATION_PLAN_v0.23.md | 20 | 0 | 0 |
| docs/RESEARCH_v0.23.md | 10 | 16 | 0 |
| docs/REVIEW_GAPS_v0.23.md | 5 | 3 | 0 |
| docs/COMPATIBILITY.md | 8 | 9 | 0 |
| docs/DESKTOP_DELIVERY.md | 15 | 18 | 0 |
| docs/CONTROLS.md | 12 | 0 | 0 |
| docs/GAME_FLOW.md | 19 | 8 | 0 |
| docs/PLAYTESTING.md | 17 | 7 | 0 |
| docs/OPERATIONS.md | 18 | 12 | 0 |
| docs/RECOVERY_BUNDLES.md | 7 | 5 | 0 |
| docs/REGIONAL_PERSISTENCE.md | 9 | 8 | 0 |
| docs/GAMEPLAY.md | 21 | 17 | 0 |
| docs/LIVING_WORLD.md | 12 | 13 | 0 |
| docs/SETTLEMENTS_AND_ECONOMY.md | 12 | 6 | 0 |
| docs/INTEGRATION.md | 20 | 28 | 0 |
| docs/MULTIPLAYER.md | 13 | 6 | 0 |
| docs/NETWORK_REPLICATION.md | 14 | 11 | 0 |
| docs/VALIDATION.md | 13 | 23 | 0 |
| docs/ARCHITECTURE.md | 12 | 11 | 0 |
| docs/RELEASE_HISTORY.md | 14 | 78 | 0 |
| crates/terra-sandbox/README.md | 8 | 3 | 0 |
| docs/GUIDANCE.md | 10 | 1 | 0 |
| docs/verification/v0.23-native/README.md | 9 | 48 | 0 |
| docs/verification/v0.23-native/state-inspection.md | 2 | 4 | 0 |
| docs/verification/v0.23-delivery/README.md | 5 | 4 | 0 |
| docs/verification/v0.23-review/world-framework.md | 10 | 7 | 0 |
| docs/verification/v0.23-review/player-experience.md | 5 | 12 | 0 |
| docs/verification/v0.23-review/reliability-delivery.md | 10 | 26 | 0 |
| docs/verification/v0.23-review/reliability-cross-review.md | 4 | 1 | 0 |
| docs/verification/v0.23-review/world-reliability-cross-review.md | 7 | 9 | 0 |
| docs/verification/v0.23-review/world-player-cross-review.md | 5 | 0 | 0 |
| docs/verification/v0.23-review/player-reliability-cross-review.md | 6 | 10 | 0 |
| docs/verification/v0.23-review/preservation.md | 10 | 2 | 0 |
| docs/verification/v0.23-review/source-packaging-review.md | 3 | 3 | 0 |

The JSON companion retains every input hash, generated heading anchor and observed link result. Heading-level jumps are informational; duplicate headings receive normal numbered anchors.

## Rerun

```sh
python3 docs/verification/v0.23-review/audit_doc_links.py --self-test
python3 docs/verification/v0.23-review/audit_doc_links.py
```

Current Validation is always included. Current native/delivery READMEs and specialist reviews join the default roster automatically once present. Use --extra RELATIVE_MD for another input and --report-prefix NAME to retain a separately named report. Reports stay under the current review evidence directory.

## Limits

- No HTTP/external fetching and no target content correctness review.
- Local links are checked one hop; linked historical documents are not recursively audited.
- GitHub-style ATX/Setext/duplicate heading slugs and explicit HTML IDs are supported; not a full CommonMark renderer.
- Fenced/inline code and HTML comments are excluded; complex custom HTML, MDX, multiline reference definitions and indented code require manual inspection.
- Non-Markdown fragments other than source line ranges are explicitly reported unchecked.
- Root-leading local paths are resolved relative to the repository root; no read outside that root.
- This active-tree audit is not final native, canonical test or packaged-archive acceptance.

Observation: 2026-10-30T09:08:51.560867+00:00 to 2026-10-30T09:08:51.738396+00:00.
