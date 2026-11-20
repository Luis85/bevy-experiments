# Current handoff documentation link audit

**Status: passed.** This checks local destinations and heading anchors in the listed current documents. It does not fetch HTTP destinations, certify linked evidence as current, or replace final source/archive acceptance.

Audited 44 documents and 498 Markdown/HTML links. Existing local destinations: 236. Network requests: 0.

| Link result | Count |
|---|---:|
| external_skipped | 34 |
| valid_anchor | 20 |
| valid_destination | 444 |

## Findings

No broken local destination, missing supported anchor or unparsed explicit reference was found.

Optional documents not yet present: docs/verification/v0.24-native/state-inspection.md, docs/verification/v0.24-review/world-reliability-cross-review.md, docs/verification/v0.24-review/player-reliability-cross-review.md.
Inputs changed during this audit: none.

## Inputs and heading structure

| Document | Headings | Links | Level jumps |
|---|---:|---:|---:|
| README.md | 18 | 69 | 0 |
| PRODUCT.md | 16 | 6 | 0 |
| DESIGN.md | 17 | 5 | 0 |
| CHANGELOG.md | 22 | 9 | 0 |
| docs/PRODUCT_REVIEW.md | 8 | 16 | 0 |
| docs/IMPLEMENTATION_PLAN_v0.24.md | 24 | 8 | 0 |
| docs/RESEARCH_v0.24.md | 13 | 16 | 0 |
| docs/REVIEW_GAPS_v0.24.md | 5 | 4 | 0 |
| docs/COMPATIBILITY.md | 8 | 9 | 0 |
| docs/DESKTOP_DELIVERY.md | 16 | 18 | 0 |
| docs/CONTROLS.md | 13 | 0 | 0 |
| docs/GAME_FLOW.md | 20 | 8 | 0 |
| docs/PLAYTESTING.md | 18 | 7 | 0 |
| docs/OPERATIONS.md | 18 | 12 | 0 |
| docs/RECOVERY_BUNDLES.md | 7 | 5 | 0 |
| docs/REGIONAL_PERSISTENCE.md | 9 | 8 | 0 |
| docs/GAMEPLAY.md | 21 | 17 | 0 |
| docs/LIVING_WORLD.md | 12 | 13 | 0 |
| docs/SETTLEMENTS_AND_ECONOMY.md | 13 | 6 | 0 |
| docs/INTEGRATION.md | 20 | 28 | 0 |
| docs/MULTIPLAYER.md | 13 | 6 | 0 |
| docs/NETWORK_REPLICATION.md | 14 | 11 | 0 |
| docs/VALIDATION.md | 14 | 25 | 0 |
| docs/ARCHITECTURE.md | 13 | 11 | 0 |
| docs/RELEASE_HISTORY.md | 15 | 87 | 0 |
| crates/terra-sandbox/README.md | 9 | 4 | 0 |
| docs/GUIDANCE.md | 11 | 1 | 0 |
| docs/verification/v0.24-native/README.md | 8 | 35 | 0 |
| docs/verification/v0.24-delivery/README.md | 5 | 3 | 0 |
| docs/verification/v0.24-review/world-framework.md | 6 | 3 | 0 |
| docs/verification/v0.24-review/player-experience.md | 13 | 5 | 0 |
| docs/verification/v0.24-review/reliability-delivery.md | 9 | 7 | 0 |
| docs/verification/v0.24-review/reliability-cross-review.md | 3 | 0 | 0 |
| docs/verification/v0.24-review/world-player-cross-review.md | 8 | 1 | 0 |
| docs/verification/v0.24-review/preservation.md | 10 | 2 | 0 |
| docs/verification/v0.24-review/source-packaging-review.md | 8 | 5 | 0 |
| docs/verification/v0.24-review/final-visual-review.md | 8 | 15 | 0 |
| docs/verification/v0.24-review/pixel-comparison.md | 5 | 2 | 0 |
| docs/verification/v0.24-review/player-docs-closeout.md | 5 | 0 | 0 |
| docs/verification/v0.24-review/canonical-closeout-review.md | 5 | 2 | 0 |
| docs/verification/v0.24-native/candidate-checkpoint-audit.md | 6 | 1 | 0 |
| docs/verification/v0.24-native/confirmation-checkpoint-audit.md | 6 | 5 | 0 |
| docs/verification/v0.24-native/final-checkpoint-audit.md | 3 | 2 | 0 |
| docs/verification/v0.24-delivery/desktop-archive-review.md | 5 | 1 | 0 |

The JSON companion retains every input hash, generated heading anchor and observed link result. Heading-level jumps are informational; duplicate headings receive normal numbered anchors.

## Rerun

```sh
python3 docs/verification/v0.24-review/audit_doc_links.py --self-test
python3 docs/verification/v0.24-review/audit_doc_links.py
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

Observation: 2026-10-30T13:42:46.123695+00:00 to 2026-10-30T13:42:46.293675+00:00.
