# Terra World 0.19 implementation and gap-closing plan

Baseline: attached `terra-world-0.18.0.zip`, reviewed 29 October 2026.

## Product objective and review coverage

Make the regional reference safer to play, extend, recover and hand off. Six fresh dedicated reviewers study the incumbent documentation and implementation before implementation dispatch. Their evidence covers player goals and UX/accessibility; gameplay, economy and progression; persistence/recovery/operations; multiplayer/security/privacy; streaming/rendering/performance/content/data; and delivery, compatibility, QA, licensing and developer handoff. Findings are source-grounded. Historical native captures and acceptance receipts remain historical evidence.

## Prioritized implementation dispatch

| Workstream / owner | Finding and planned closure | Acceptance / gap contribution |
|---|---|---|
| P1 Storage integrity / persistence | Table/column-name admission can overlook missing regional primary-key guarantees and duplicate authoritative records. Validate supported schema contracts and reject duplicate regional identities without silently overwriting rows. | Writable/read-only open and relevant post-open corruption regressions; legitimate existing stores still open. G15/G16 integrity and recovery. |
| P1 Gameplay authority / gameplay | Direct public settlement entry point can bypass dormant-player admission. Apply the incumbent gate before any settlement mutation and protect manual controlled-motion admission, including zero-speed repositioning; audit adjacent public entry points. | Full serialized state unchanged on rejected representative settlement actions; active-player behavior retained. G03/G06 economy and permissions. |
| P2 Player flow / player_experience | Long errors outside the bounded menu scroll area can displace actions, and unbroken paths may overflow. Move notices into the existing scroll region and wrap path/status text. Assess destructive F9 load review as additional scoped polish if it can preserve incumbent action identity. | Layout/input regression checks for long notices, small/high-scale layouts and reachable actions. No claim of physical-device or assistive-access qualification. G04/G08. |
| P2 Multiplayer privacy / network | Derived Debug on protocol enums reveals bearer resume capabilities. Redact credentials while preserving useful diagnostics and unchanged serialized protocol. Review nearby handshake trust boundaries. | Normal/alternate debug redaction, token-free diagnostics, serialization round trips and network suite. G13/G17 foundation; public-service account/moderation scope remains open. |
| P2 Streaming presentation / world_performance | Completed mesh tasks can outlive an evicted/replaced source chunk and be accepted against different authoritative terrain. Bind jobs to their actual source identity and discard/requeue stale completions. | Deterministic task-completion regressions for replacement, matching identity and retry behavior. G07/G10 correctness; hardware residency/frame tails remain open. |
| P2 Release acceptance / delivery | Desktop publication accepts a passed receipt without the complete verification roster or distinct-test identity evidence. Share canonical gate requirements and require complete, consistent evidence before publication. | Positive complete receipt plus missing/duplicate/fabricated-count rejection regressions, packaging suite and canonical runner. G01/G11. |
| Integration / root | Review combined work, resolve integration issues, bump 0.19.0, preserve historical evidence, run fresh frozen-source gates and inspect final archive. | No failed checks described as acceptance; preserve baseline content, record exact tests/input/binary identities, release source and report. |

Owners may narrow or strengthen a fix based on reproduction. Material deviations and additional findings must be recorded in their implementation notes. Source ownership is partitioned; compilation is centrally coordinated to avoid duplicate resource-heavy builds.

## Implementation, review and polishing sequence

1. Retain the six review reports under `docs/verification/v0.19-review/`.
2. Dispatch the above concrete scopes to the dedicated owners. Each adds meaningful regressions and records actual limitations; do not fabricate runtime evidence.
3. Root reviews source changes and runs focused integration checks. Cross-dispatch fresh perspectives among owners for implemented changes; correct findings before freezing source.
4. Freeze implementation inputs, run the canonical verification runner with the pinned toolchain and cached dependencies, and retain command logs, exact test identities and source hashes. Investigate failures and rerun only the necessary gates, then run final acceptance if implementation inputs changed.
5. Package only from an accepted executable. Inspect source/desktop archives for exact bytes, preserved baseline files, portable namespace and absence of new private runtime data. Save updated deliverables and the current report.

## Comprehensive gap backlog and explicit release boundaries

The full G01–G18 register is retained and updated in the final gap report. This increment closes the reproduced conditions above, not entire product areas. Highest remaining acceptance priorities are Windows execution on the intended PC (G02), uncoached Supply/Home/Community/Road/Company playtests and long finite-economy careers (G03/G04), navigation-density/long-run coverage (G05), unavailable-leader succession and dissolution policy (G06), measured GPUs and physical controllers/wheels/accessibility (G07/G08), production animation/audio and licensed geography (G09/G10), trusted signed release/update/rollback (G11), explicit content migration (G12), public enrollment/moderation/audit and WAN soaks (G13/G14), measured storage latency and operational retention/RPO/RTO/crash alerts (G15–G17), and requirements-led global authority partitioning (G18).

Compatibility intent: retain save 1, regional schema 1, wire 10 / `terra-world/10`, and asset schema 1 unless implementation demonstrates an unavoidable contract change. A stricter integrity check must accept valid existing stores, and Debug-only changes must leave wire JSON unchanged. No deployment or publication to a public service is authorized by this plan.
