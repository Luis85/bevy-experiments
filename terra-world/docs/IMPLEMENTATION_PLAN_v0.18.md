# 0.18 implementation plan

Baseline: attached terra-world-0.17.0.zip, reviewed 29 October 2026.

Goal: close reproducible defects within the regional reference while retaining explicit limits on Windows, hardware, content, WAN and human playtest qualification.

1. Fresh reliability agent: inspect persistence/recovery/diagnostic failure boundaries; implement a concrete high-value fix with regression coverage.
2. Fresh gameplay agent: inspect authoritative economy, community and transport lifecycle boundaries; implement a reproducible defect fix with regression coverage.
3. Fresh player-experience agent: inspect input, menus and player handoffs; preserve incumbent design and correct actionable state/interaction defects.
4. Fresh delivery agent: review release scripts and compatibility checks; fix concrete validation or packaging weaknesses with focused executable checks.
5. Integrator: run relevant Rust suites and Python checks, review combined changes, then dispatch independent review of implemented work. Apply necessary corrections in one polishing pass.
6. Produce source release and current review/validation/open-gap report. Existing receipts remain historical; only newly executed commands establish 0.18 acceptance. No inherited native captures count as fresh verification.

Acceptance: all changed code compiles; focused regressions and available broader gates pass; archive excludes private runtime state and preserves baseline project content; all unexecuted checks and remaining product gaps are stated. Windows execution, human playtests and deployment qualification require their actual environments and participants.

## Selected implementation and review results

| Dedicated fresh owner | Delivered scope | Review correction |
|---|---|---|
| Reliability | Poisoned-store terminal recovery and last acknowledged status | Include idle read-only export/backup visibility. |
| Gameplay | Atomic owner-paid assigned-vehicle charges and receipts | Full-state matrix across six services and three rejection classes. |
| Player experience | Neutral standard-pad rearm while preserving menu/keyboard access | Zero trigger neutrality must match raw throttle consumption. |
| Delivery | Portable archive namespace and evidence identities | Check raw duplicates before dictionary insertion. |
| Independent reviewer | Cross-product review of all four implementations | No additional material source blocker after corrections. |
| Native validation | Fresh bounded Linux session observation | Its own receipt names the actual scope; synthetic input does not qualify physical controllers. |

All production changes were frozen before the canonical
`verification/v0.18-acceptance` run. An earlier candidate run began while the
last source refinements were arriving and cannot certify final source identity;
its retained result is historical evidence, not release acceptance. The original
synchronized working copy reverted edits, so integration and testing moved to a
separate stable build directory; exact accepted inputs are hashed by the runner.
