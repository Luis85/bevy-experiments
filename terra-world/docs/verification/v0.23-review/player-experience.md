# Player experience review — 0.23

Reviewed the exact delivered 0.22 source on 30 October 2026. This report separates
source findings from native visual evidence; the integrator retains the fresh
baseline, candidate and confirmation captures in the corresponding native record.
The review read PRODUCT.md, DESIGN.md, README, the current product review,
0.22 gap register and plan, then traced real native label, activation, queue and
domain-dispatch paths. No historical screenshot is counted as current evidence.

## Findings and selected work

| Finding | Evidence in the 0.22 source | Product consequence and selected correction |
|---|---|---|
| Nature consumption bypasses the displayed-person contract | `frontier.rs` produces bare active-body Consume commands for berries, cooked fish and water; `interface.rs` binds only Command Eat/Drink to ConsumeTarget. | An intervening actor switch can spend supplies on another body. Share the active-person identity contract, name that person in Nature and retain the displayed identity through activation and dispatch. |
| Direct Activities consumption compares the item but not its body | `GuideAction::same_intent` compares serialized commands; `ContinueJourney` retains player/restore but two urgent people can yield the same Consume command. | Review of one person's need can silently become consumption for another. Include active-body identity in native guide display context and bind the displayed action instance. |
| Missing resource and territory selection silently chooses another object | `resource()` and `territory()` use fallback after an explicit ID no longer resolves; side-effectful frontier dispatch derives its target again. | A stale gather, contribution, donation or upgrade can act on a target the player did not review. Keep explicit unavailable selection, retain exact target/context and require a fresh display after selection changes. |
| Supply availability and authority disagree | Native `unavailable` checks only selected goods/reach/claim predicates and does not check expiry for acceptance; domain fulfillment has additional receiver/wallet bounds. | A player can be promised completion that cannot execute. Integrate the shared typed readiness seam owned by the world specialist; use it for written recovery and disabled state. |
| Disabled actions do not always name the next corrective step | Nature and territory have general prose, while several detailed blocker states only dim buttons. | Players must infer whether ownership, stock, range, inventory or expiry blocks progress. Retain compact general context and add concrete current-condition messages in the existing scroll body. |

The rest-handoff hypothesis was disproved: opening Command from urgent energy
guidance already selects the current owned active person. It is not counted as a
new bug or fix. Native consumption review is also not a server-side reservation;
the existing active-body command remains authoritative on the host.

## Research and application

- [XAG 109 — objective clarity](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/109)
  supports reviewable goals, current progress and prescriptive next actions,
  including returning players. Apply this to explicit blocked supply recovery.
- [XAG 114 — UI context](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/114)
  supports nearby purpose, target and consequence information. Apply this to the
  named active person and exact resource/territory destination.
- [XAG 115 — errors and destructive actions](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115)
  supports correctable errors and review before consequential actions. Apply
  this to rejecting stale action context before inventory or public funds move.
- [XAG 112 — navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112)
  and [XAG 113 — focus](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/113)
  support consistent digital navigation and visible, reachable focus. Preserve
  the incumbent controls and inspect minimum-window scrolling after new text.
- [XAG 107 — input](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/107)
  supports digital alternatives and avoiding unnecessary timing/hold demands.
  No new held-input confirmation or pointer-only control is introduced.
- Bevy's official [tab navigation](https://bevy.org/examples/ui-user-interface/tab-navigation/)
  and [character creation](https://bevy.org/examples/usage/character-creation/)
  examples demonstrate explicit focus groups and model/controller/view ordering.
  The existing Bevy 0.20 application retains those separation principles; it
  does not change dependencies to imitate a newer online example.

Sources were opened on 30 October 2026. These are design inputs, not a claim of
full XAG compliance. Research recommendations are interpreted for Terra's native
desktop reference game and existing forest-green DESIGN.md direction.

## Implementation and acceptance scope

WP01–02 cover native actor/action identity and resource/territory selection.
The world specialist owns WP03–04's domain readiness and acquisition guidance;
the player specialist integrates its native labels and availability.

Acceptance requires actual Activate → UiQueue → dispatch fixtures, successful
unchanged-context domain execution and unchanged state on rejection. Cases
include changed body, principal, restore, target, pending transitions, explicit
missing selection, queued chooser/action and a refreshed display after a click.
The native pass must inspect Activities, Nature, territory and blocked/ready
supply at reference size and 1100×700 with 150% text. Detailed command outcomes
and value preservation use automated state evidence in addition to screenshots.

Implementation and execution results are recorded in the current validation
report. This review does not establish an uncoached human journey, physical
controller/wheel behavior, assistive-technology support, 200% typography,
Windows qualification or physical-GPU performance. G04 and G08 retain those
separate acceptance criteria.

## Candidate implementation and execution

The integrator formatted the combined source and ran the complete native demo
suite: **238 tests passed, 0 failed, 0 ignored**. The executed candidate command,
output and result are retained in
[native integration log](../v0.23-preflight/native-integration.log) and
[native integration record](../v0.23-preflight/native-integration.json).
This is focused integration evidence; the later frozen canonical release run
remains the authority for final acceptance.

All selected player packages are implemented. Actual Activate/queue/dispatch
regressions cover every Nature/Activities consumption surface, changed active
body/player/restore/owner, pending transitions, dormant owners, disconnected
state and unbound queued actions. Tests also cover a queued patch choice,
replacement display, missing explicit targets and successful unchanged-target
resource/territory transactions. Native supply tests check wallet-capacity
explanation and disabled state, recovery to exact delivery, and expired
acceptance without mutation.

Fresh 0.22 baseline captures `01-baseline-activities.png`,
`02-baseline-nature.png` and `03-baseline-territory.png` in `../v0.23-native`
were opened and inspected before visible changes. Nature showed generic active
actor needs without a name and a dimmed Gather control beside only a general
20 m requirement. The implemented pass names the active supply recipient,
shows carried consumables, explains current gathering distance and gives
explicit unavailable-target recovery. It retains the existing palette,
typography, panel structure, scrolling and viewport bounds.

The reliability specialist independently reviewed the action boundary in
[player reliability cross-review](player-reliability-cross-review.md) and found
no blocking defect after checking the dormant-owner guard and actual activation
fixture. Candidate/confirmation native captures are reviewed separately before
release; baseline observations and candidate tests are not substituted for that
visual acceptance.

The independent [native player review](player-native-review.md) inspected all
52 candidate and all ten final confirmation captures, including the managed-world
supply, Nature, territory, save, rename, backup and separate-recovery journey.
Minimum-window play at 1100×700 and 150% text preserved the named actor,
corrective guidance and reachable actions. Final confirmation shows Performance's
effective sky override with its retained preference, singular contract grammar
and territory item names with spaces. The recovered world visibly retains
181.13 credits, 50 XP, one completed contract and Review23's 75.00-credit treasury;
the independent persisted-state record supports those invariants after resumed
play. Both final native processes exited cleanly with unchanged implementation
inputs. No material confirmation defect was found, and source remains frozen.

The native review explicitly distinguishes rendered journeys from regression
coverage: a full receiver/capacity-blocked supply fixture and busy maintenance
were not exercised as running UI scenarios, and the final sky captures cover
Performance/preference On in pause/global settings. Automated action races and
successful consumption remain fixture evidence. Canonical release acceptance
continues separately and must finish before packaging.
