# Terra World 0.24 — research and design decisions

Research checked on **30 October 2026**, alongside a source review of the exact delivered 0.23 archive. The [implementation plan](IMPLEMENTATION_PLAN_v0.24.md) maps decisions to owners and acceptance. Research is guidance, not proof that the implementation, platform or user experience has passed.

## Goals and method

The user's goals combine a reusable world SDK, a playable living frontier, connected careers and reliable delivery. The review therefore asks whether a player can find useful work, understand and carry out a selected operation, preserve progress and resume, and whether an integrator can safely reuse the same authority and verification contracts.

Primary sources were preferred: platform accessibility guidance, game-economy API documentation, database transaction principles, standard-library process/archive documentation and platform file semantics. Existing product/design documents and fresh native captures supply Terra-specific evidence. No source prescribes our exact economic policy, no screenshot certifies accessibility, and CI configuration does not establish Windows execution.

## Applied patterns

### 1. Clear objectives and concrete recovery

[Microsoft XAG 109: Objective clarity](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/109) recommends that players can review understandable current objectives and the prerequisites for progress. This supports a continuing supply board whose work is useful and feasible, with a concrete next step when a service or action is unavailable.

**Application:** shared demand/capacity evaluation, known-obsolete public request rejection and preserved accepted commitments. Native absence should explain a deliberate browse/review action instead of silently replacing the target.

**Acceptance:** economic continuation fixtures and fresh Activities/Industry/Fleet journeys. Uncoached first/returning-player comprehension and measured long-career balance remain separate proof.

### 2. Review destructive consequences and preserve action meaning

[Microsoft XAG 115: Error messages and destructive actions](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115) emphasizes preventing/correcting input errors before permanent effects. It supports reviewable consequences and recovery rather than unexplained failure.

**Application:** building damage names the exact structure and health consequence and offers Cancel/Confirm. Routine production/fleet actions bind the displayed operands and intended result until execution, then recheck authority. A newly selected target requires a newly reviewed action.

**Acceptance:** actual Activate-to-queue-to-dispatch tests, cancellation and unchanged-state rejection under selection, ownership, player, restore and eligibility changes. Native review/focus remains in the existing inspector scroll model.

### 3. Consistent navigation and honest scaling scope

[Microsoft XAG 112: UI navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112) informs consistent focus, digital navigation and return paths. [XAG 101: Text display](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/101) makes readability and adjustable presentation explicit.

**Application:** preserve the established palette, chooser, focus reveal, scroll boundaries and 150% text contract while adding action review. Initial focus for damage review goes to Cancel. Current 200% coverage is a deliberate remaining layout requirement, not a setting to unlock without reflow.

**Acceptance:** reference 1440×900 and minimum 1100×700 native journeys at the currently supported scales; tests of focus and action activation. Physical controller/wheel/assistive and product-wide narration/contrast/localization remain open.

### 4. Prepare economic transitions before mutation

[PostgreSQL's transaction tutorial](https://www.postgresql.org/docs/current/tutorial-transactions.html) explains multi-step all-or-nothing changes: partial application must not become a valid state.

**Application:** prepare exact issuer credit before removing an obsolete public contract. A capped or unavailable issuer retains its escrow claim. New work must be useful and physically admissible before funding is reserved. These are pure domain transitions; no PostgreSQL dependency is added.

**Terra policy:** accepted terms remain promises. Unclaimed requests can retire only when demand is confirmed obsolete and the original issuer can receive the exact escrow. Needed expired offers renew under their existing IDs. Unknown destination data is not evidence of deletion.

**Acceptance:** finite-money totals, cap failure/recovery, competition, full-board turnover, no ID reuse/double credit, and disk save/restore continuation.

### 5. Observation and mutation have distinct authority

[PlayFab: ETags and concurrency control](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/etags-and-concurrency-control) describes rejecting writes whose observed version is no longer current. [PlayFab: idempotent transactions and retry patterns](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/idempotent-transactions-and-retries) distinguishes concurrency checks from preventing duplicate effects.

**Application:** a displayed readiness result is an observation, not a reservation. Native operation tokens retain reviewed intent; actual commands validate current authority. Retiring an offer removes its unique identity exactly once, and retries cannot credit its escrow again. No online service or remote reservation protocol is introduced.

**Acceptance:** actual same-frame interaction races, competing acceptances/refunds and unchanged-state failures. Serialized command shapes and wire 11 remain; matching wire numbers do not qualify arbitrary mixed-release behavior.

### 6. File paths are names, not file identity

[Microsoft: Hard Links and Junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions) documents multiple paths to the same file and directory link semantics. [FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info) provides platform identity context. The [same-file crate](https://docs.rs/same-file/latest/same_file/) exposes portable identity comparison.

**Application:** replace the non-Unix hard-link omission with exact direct use of already-locked same-file 1.0.6 and resolve existing ancestors for not-yet-created output paths. Identity/inspection failure must not become a safe-distinct result.

**Acceptance:** protected database, sidecar and metadata aliases retain original bytes; safe distinct outputs work; common tests run locally and Windows junction fixtures are wired to Windows CI. Cooperative validation does not eliminate concurrent administrative replacement, and Linux does not prove Windows filesystems.

### 7. A process's exit is not completion of all owned work

[Python subprocess documentation](https://docs.python.org/3/library/subprocess.html) warns about pipe deadlocks, unlimited buffered output and timeout cleanup. A child can inherit output or redirect it independently of the leader.

**Application:** bound canonical-gate output as it arrives, include EOF in the deadline, bound cleanup waits and require absence of the owned POSIX group before success. Retain partial logs and structured failure evidence. Successful compiler/test output must remain exact for test parsing.

**Acceptance:** real processes for success/failure, overflow, timeouts, held pipes and redirected children. Ordinary process-group ownership is the scope; deliberately detached descendants and platform-specific process controls retain explicit limits.

### 8. Validate the actual extracted artifact before executing it

[Python zipfile documentation](https://docs.python.org/3.13/library/zipfile.html) describes archive extraction concerns. [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use) distinguishes workflow security and dependency provenance from build success.

**Application:** validate bounded archive names/types/namespace and expanded size, extract privately, restore modes, then run passive verification before optional startup smoke. CI must check the actual ZIP before upload and retain failure reports.

**Acceptance:** valid round-trip and malicious namespace/type/limit fixtures; final Linux fresh extraction with exact source/evidence/executable identity. Publisher signing, immutable CI action provenance, authenticated release discovery and rollback remain separate G11 requirements.

## Review decisions across the wider product

| Perspective | Decision for this increment | Evidence still needed |
|---|---|---|
| Product/onboarding/return | Make the existing career and maintenance loop dependable before expanding scope. | Uncoached task completion, time to useful action and return after absence. |
| Gameplay/economy | Useful funded work, conserved escrow and honored commitments. | Sustained balance with labor, living and capital costs. |
| Interface/accessibility | Exact actions, meaningful labels, protected destructive review. | Full 200% reflow, device and assistive-user studies. |
| Simulation/navigation | Preserve existing movement and physical recovery behavior. | Density, obstacles, traffic endurance and real performance budgets. |
| Community/property | Keep conservative existing rules; do not invent abandoned-community takeover. | Explicit succession, populated dissolution, treasury and property policy. |
| SDK/architecture | Reuse domain queries and native adapter boundaries; avoid new saved state. | Integrator usage and eventual partition requirements. |
| Persistence/security | Correct file-identity checks and retain cooperative ownership limits. | Windows/filesystem power-loss, offsite recovery, public-service threat model. |
| Release/operations | Bound evidence production and verify extracted packages. | Signed distribution, real clean machines, monitoring owners and WAN soaks. |
| Data/art/audio | Preserve original licensed inputs and authoring sources. | Shipping assets, ingestion/rights, budgets and source/content migrations. |

## Research limits

The sources are engineering and interaction guidance, not a comparative usability study of Terra. The lifecycle rules are an explicit product design decision grounded in the current finite economy. There are no new claims of physical GPU performance, Windows runtime acceptance, uncoached testing, production AAA assets or public MMORPG capacity. See [the complete gap register](REVIEW_GAPS_v0.24.md) and [actual validation](VALIDATION.md).

