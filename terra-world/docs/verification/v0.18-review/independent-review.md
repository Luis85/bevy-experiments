# Independent product and implementation review — 0.18

Reviewed against the attached 0.17 source and the 0.18 implementation plan on
29 October 2026. This review uses source inspection and the executed checks
explicitly listed below. Historical receipts establish their historical scopes;
they are not fresh 0.18 native, Windows, hardware or human playtest evidence.

## Review across product perspectives

| Perspective | Assessment | Acceptance boundary / remaining work |
|---|---|---|
| Player goals and game design | Keeping the existing first-session, continuing-goal and funded-work loops is appropriate for a defect-closing increment. More content and career balance cannot be inferred from correctness fixes. | Observe uncoached new/returning sessions and measure repeated confusion or stalls (G03/G04). |
| UX and visual design | Preserve the established forest palette, world-first composition, contextual inspectors and explicit save/error states. Input recovery should name the action needed to resume. | Current-source input checks do not qualify a new rendered layout. Native observations must be separately identified. |
| Accessibility and input | Standard-pad focus/reconnect neutral rearm addresses an actual unintended-motion boundary already handled for wheels. Menus and keyboard escape paths must remain usable while world pad intent is blocked. | Real controllers/wheels, assistive technology, contrast, motion comfort and localization remain G08. |
| Economy and ownership | Authoritative purchase failure must preserve the complete pre-command state, including non-spendable business expense records. Expense receipts belong to the same successful command as wallet/treasury movement. | Longer strategies, living/capital costs, production chains and balance remain independent G03 qualification. |
| Simulation/navigation | This increment does not justify raising terrain, population or road-density budgets. Existing physical/corridor and continuation fixtures should remain regression gates. | General dense traffic, richer geometry and long interactions remain G05. |
| Community lifecycle | Permission and existing membership behavior are retained. No unavailable-leader succession or dissolution policy is introduced by these fixes. | G06 remains open; do not describe a lifecycle-complete community product. |
| Content/graphics/data | Original authored assets, source provenance and bounded pack pipeline remain the correct basis. No production-quality art, GPU budget or licensed shipping geography is established by this increment. | G07/G09/G10 and migration G12 remain open. |
| Reliability/recovery | Store mutex poison indicates an uncertain mutation boundary. It must reject continuation, expose reopen guidance and retain last acknowledged status; normal rolled-back SQLite errors should stay retryable. | Storage tail latency, stuck OS I/O, deployment backups and RPO/RTO remain G15/G16. |
| Privacy/operations | Scope contains no new account authority, external telemetry destination or automatic sharing. Diagnostic opt-in and private runtime-state exclusions must remain intact. | Public accounts, moderation, alert ownership, external retention and WAN capacity remain G13/G14/G17. |
| Developer/delivery | Portable archive namespace validation is useful on both producer platforms and must precede publication and dictionary shadowing. New tests should be part of repeatable gates. | Linux validation cannot close Windows clean-machine execution (G02), publisher signing/update trust (G11), or global authority partitioning (G18). |

## Findings sent for correction during implementation

1. **Delivery namespace integration.** A final dictionary-only namespace check
   loses duplicate raw evidence names. Validate reserved evidence names, gate
   logs and evidence-artifact names before insertion, permitting only the exact
   intentional `source-hashes.json` overlap. The delivery owner accepted this
   review finding for correction.
2. **Poison discovery through read-only adapters.** `export(&self)` and
   `backup_to(&self)` can be the first operation to encounter a poisoned store.
   Terminal availability/reopen state must be observable there too, without
   installing default/unknown status as an acknowledged checkpoint.
3. **Input recovery completeness.** Focus regain and selected-device replacement
   must block all standard-pad world paths, including pressed actions and RTS
   fields; menu navigation must remain available and keyboard fallback must
   remain independent. Review re-enable and binding-capture early returns too.

## Final implementation inspection and executed checks

Final source inspection used the stable integration tree. The synchronized
scratch tree was excluded after file restoration was observed.

- **Reliability:** `requires_reopen()` includes persistent mutex poison;
  `require_available()` refuses idle poisoned reads/maintenance; pending result
  acceptance enters the existing terminal latch before installing status. Both
  asynchronous and barrier paths retain last acknowledged status. Ordinary
  non-poisoned SQLite errors keep the prior completion/retry behavior.
- **Economy:** the shared helper prepares optional accounting without mutation,
  performs the checked wallet/treasury charge, then installs the receipt. All
  six service forms call it, and subsequent service mutations have no fallible
  operation. The new 18-case regression compares complete serialized state for
  rejected wallet, treasury and journal-capacity cases. Existing successful
  service and rental coverage supplies the other side of the contract.
- **Input:** process-local ownership resets on focus loss, selected-pad change,
  absence/disable and binding capture. World-only fields are gated; menu and
  keyboard paths remain independent. Independent review found the initial
  trigger threshold could admit raw low throttle; the owner corrected both
  trigger neutral tests to zero and added a focused low-trigger regression.
  Existing RTS consumer paths cancel pointer ownership when pad availability
  disappears. This is a synthetic-input contract, not hardware qualification.
- **Delivery:** raw combined evidence names are validated before dictionary
  insertion, with only the intentional source-manifest overlap. Assembled
  desktop filenames are then checked for case aliases and file/directory
  conflicts. Namespace and containment checks precede archive publication.
  The Python suite is included in the repeatable release runner.

Independently executed:

`python3 -m unittest discover -s scripts -p test_package_desktop.py -v`

Result: **11 tests passed**, zero failures, zero skips, exit 0 on Linux.
These use a synthetic executable and mocked acceptance bindings; they establish
packager behavior, not execution of a release executable. The later Windows-only
symlink-test skip annotation does not change this Linux result.

No further material implementation blocker was found by this source review.
Rust compilation, focused tests, complete release gates and actual native/package
observations must be assessed from [the final gate receipt](../v0.18-acceptance/gates.json)
and the final release Validation, not inferred from this review. Source changes
made after this inspection require their own targeted review.

The remaining G01–G18 register retains all baseline closure criteria outside the
narrow fixes above. In particular, a Linux source/package result cannot close
G02 for the user's Windows PC. No new claims of a production AAA/MMORPG service,
complete Motor Town parity, physical-device support, production content,
long-soak capacity or uncoached player comprehension follow from this pass.
