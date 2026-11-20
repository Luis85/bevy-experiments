# Integration review — Terra World 0.19 candidate

Reviewed 29 October 2026. This record combines six fresh specialist reviews and
peer cross-reviews; it does not replace executed acceptance or native evidence.

## Source-level conclusions

- Gameplay guards run before authority mutation. The seven settlement families
  are genuinely feasible in their fixtures, reject through direct and wrapped
  APIs while dormant, preserve serialized state, and succeed after resume.
  Controlled movement rejects both velocity and stationary repositioning.
  Trusted host placement remains explicit; this is not malicious-host isolation.
- Persistence admission retains valid schema1 but rejects altered xinfo/key/FK
  and automatic-index semantics. The first checkpoint is covered, so a vacuous
  generation0 path cannot silently lose pages via added UNIQUE REPLACE. Decoder
  entry-based admission preserves the first region on duplicate rejection. The
  positive route covers backup, read-only archive, reopen, restore and checkpoint.
- Protocol Debug is exhaustive and does not serialize to hide tokens. Named
  private fields redact in normal/pretty/nested output; serde stays unchanged.
  Explicit JSON is private, and arbitrary user strings are not redacted.
- Mesh tasks retain the source Arc until completion; pointer identity cannot be
  recycled while retained. Replacements requeue through bounded worker limits.
  Tests exercise the real rendering system with deterministic ready completions,
  not a copied comparator. Current-source worker disconnect/poison retry and
  broad edit invalidation remain WP02/WP03, requiring separate implementation.
- Launcher status remains under the incumbent bounded ScrollArea, while footer
  controls keep action closures, TabIndex and retained review revisions. Peer
  review caught duplicate TextLayout bundle construction; the owner removed the
  redundant component, and the actual-button regression covers the failure.
  Tree ancestry/activation is not actual pixel, Tab or device acceptance.
- Release admission shares the full gate roster and Rust-test gate subset with
  execution. Strict integer counters reject booleans/missing values; required
  test gates execute, other gates cannot invent test counts; unique identity,
  per-crate, execution total and distinct<=execution constraints reconcile.
  Existing namespace/source/log/binary/platform safeguards remain. Coordinated
  forged receipts/publisher authentication are explicitly outside this scope.

## Cross-review closure

| Reviewer | Peer work | Findings and disposition |
|---|---|---|
| Gameplay | Network, delivery | No material blocker; retain field-specific privacy and structural-trust wording. |
| Network | Gameplay | No blocker; clarify trusted host-placement exception in its own API doc; implemented. |
| Player experience | Persistence, gameplay | Positive compatibility meaningful; request direct decoder branch proof rather than claiming schema rejection proves decoder; implemented. |
| Delivery | Launcher, meshes | Duplicate TextLayout crash risk; owner fixed and reviewer rechecked. No extra mesh blocker. |
| Persistence | Delivery | Missing counters and bool exits bypassed candidate; owner added strict counters, execution roster/totals and regressions. Reviewer confirmed closure. |
| World/performance | Persistence, launcher | Hidden automatic UNIQUE index could delete pages at generation0; owner added structural index admission and first-checkpoint regressions; reviewer confirmed closure. Also independently caught corrected TextLayout duplication. |

## Product acceptance boundaries

The selected fixes improve a regional playtest foundation without adding prices,
quests, funding, geographic coverage or public-service authority. Full G01–G18
closure remains in the current gap register. Windows, GPU residency/frame tails,
physical controllers/wheels, assistive access, uncoached first/returning sessions,
production content/data, WAN/capacity and deployment recovery need their actual
environments and workloads. Unavailable-leader lifecycle, coordinated F9/Load
review and shared SDK handshake/snapshot admission remain explicit followups.

## Execution

The integrator owns the pinned-toolchain frozen-input gate run and final
packaging. The first candidate stopped in a malformed foreign-key test fixture;
the fixture now disables enforcement only on its hostile external editor before
rewriting the database. Production admission was unchanged. All 45 focused
persistence tests then passed. The first candidate receipt remains historical.
Consult final `docs/VALIDATION.md`, `v0.19-final-acceptance/gates.json` and native
review for final results; a source review alone does not claim tests or rendered
pixels passed.

## Final acceptance addendum

All 44 final commands passed; 946 distinct Rust tests, 1,032 passing executions,
and 18 separate Python packaging tests reconcile. The final persistent runner
receipt retained running/44 despite observed successful process completion.
Use `v0.19-final-acceptance/accepted-gates.json`, the explicitly labeled and
independently checked reconstruction, alongside the untouched original. See the
republication review for provenance, exact independently verified facts and
unavailable original finish timestamp. Native windowed QA remains blocked.
