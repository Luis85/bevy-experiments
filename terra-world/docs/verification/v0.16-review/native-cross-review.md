# Independent native candidate review — 0.16

## Evidence and scope

This review inspected fresh screenshots captured by the parent during actual
native play. It used the project's PRODUCT and DESIGN context and the Product
Design audit workflow. The reviewer did not operate the shared display. Candidate
captures were taken before the coordinated final polish batch; the parent owns
post-polish verification and the packaged acceptance evidence.

The reviewed candidate files were under `/tmp/terra-native/`. Filenames below
identify the exact evidence without presenting a static image as a complete
interaction trace. Default-size captures are 1440 × 900. The small-window captures
are 1100 × 700 with 150% text; `resumed-150` is 1440 × 900 with enlarged text.

## Journey review

| Step | Evidence filenames, prefixed `v16-candidate-` | Assessment |
|---|---|---|
| Load the paid Comet and customized Fieldhand | `driver-loaded.png`, `driver-cabin.png`, `driver-parked.png` | The visible seated head/shoulders remain under the roof and the hands sit in the steering area. No protruding feet or head are visible. The garage capture has a support column and deep shade, so it is weak evidence for precise fit or color. The exterior, human and terrain retain the existing low-poly identity. |
| Switch to first-person, then exit | `driver-first.png`, `human-on-foot.png` | First-person shows a clear forward view and cockpit without the exterior/rider. Exit shows an upright customized human beside the Comet. Stills cannot establish animation timing or every transition frame. |
| Retain the car after exit | `human-on-foot.png` | P2: the Driving panel selected a Trail pickup 201 m away while the just-exited Comet remained adjacent, disabling Enter. Source review identified loss of the implicit active-vehicle selection. The correction pins the already-resolved displayed vehicle during Exit, preserving an intentional explicit other selection and existing paid-action guards. |
| Choose a continuing goal | `goal-choices.png`, `goal-supply.png`, `goal-choices-150.png`, `goal-close-150.png`, `goals-top-150.png` | Six direct choices are readable. Closed choices leave the primary action visible. At 1100 × 700 / 150%, the last choice row continues below the fold through the existing inspector scroll; there is no horizontal text overlap. Explicit selection now reveals the chosen next step, preventing the collapsed list from leaving its action above the viewport. This extension was inferred from the source/layout, not claimed as a separately captured failure. |
| Judge the supply commitment | `commitments.png`, `commitments-rows.png`, `second-request-accepted.png` | P2: the acquisition step repeated stock/unit-price/wallet and full-input costs, while the gross reward was visible only farther down the noticeboard. The final guidance batch replaces that duplication with one gross reward/XP and complete missing-input comparison, while retaining deadline, actual task, delivery destination and live-quote/gather/craft alternatives. |
| Open the exact accepted request | `review-exact-request.png`, `noticeboard.png` | P2: review selected the right request but kept the view mid-scroll, leaving the new primary action above the viewport. Successful review now reveals the guide and names the exact request in status. It still performs no travel, purchase, return or delivery. Stale/rejected review preserves scroll and does not announce success. |
| Save and return to guidance | `guidance-saved.png`, `resumed-150.png`, `small-150.png` | Saved feedback and accepted obligations remain visible. The screenshots alone do not prove byte preservation or a complete reopen trace; the parent receipts and domain save/resume regressions provide that separate evidence. |
| Use enlarged text | `small-150.png`, `goals-top-150.png`, `goal-close-150.png`, `human-clear-150.png` | Main navigation, wallet, current action and goal controls remain readable at the minimum window. Content uses vertical scrolling. The focused world scene and the upright human remain visible when the inspector is closed. This is a visual/flow check, not full accessibility qualification. |
| Rename and cancel | `rename-review.png`, `rename-cancelled.png`, `worlds-150.png`, `renamed-world.png` | At 1100 × 700 / 150%, world identity, focused name field, explanatory text and Back/Save actions fit. Cancel returns to the original visible title; the renamed result shows the chosen title. World/profile preservation is established by the parent's separate byte checks, not inferred from these pictures. |
| Name and recover a separate copy | `recovery-name-ready.png`, `named-recovery-complete.png` | The edited recovery name, source title and checkpoint are distinct. The recovered world appears under the chosen name beside the retained copy; no automatic launch is shown. The review body scrolls while Back/Recover actions remain available. No new blocking layout issue was observed. |

## Corrected control interpretation

The candidate footer duplicated a hardcoded movement/parking shortcut list in a
status message above the configured live movement hint. An initial review search
looked for a direct `KeyP` handler and incorrectly suggested that P was not a park
action. Independent follow-up traced the actual input abstraction and withdrew
that claim:

- `Binding::Park` is index 10; default input settings map indices directly to the
  key table, where index 10 is P.
- `driving_keys` dispatches `Binding::Park` to `ParkingBrake`, and its action
  toggles the parked state through the ordinary `SetSignals` command.
- Space is the separate held `Handbrake` binding. Both controls exist.

The parent's P press followed by the visible PARK state is consistent with the
default parking binding. The real polish issue is **remap-insensitive duplicated
shortcuts**, not a nonexistent action. The batch removes the hardcoded shortcut
list and keeps the configured movement hint as the control source. This correction
supersedes the initial review message; the incorrect interpretation was not
published in the project documentation.

## Route wording

P3: `driver-parked.png` shows a destination distance of 186 m and “Turn back · 432 m”.
Source inspection confirms that 432 m is total remaining route length, not distance
until the steering instruction. The agreed correction labels the route value as
metres **remaining**, keeping it distinct from straight-line destination distance.

## Source checks behind the guidance correction

The full-quantity comparison uses positive current prices, sufficient visible
stock, checked multiplication and deterministic cost/distance/ID ordering. It
does not establish reachability, purchase capacity or a reserved quote. The actual
acquisition action can name a nearer partially stocked seller or a recipe
ingredient. The correction preserves those seller/item identities and prerequisite
instructions, including when shop display names match. Live Market quotes remain
responsible for transaction admission. Carried goods and travel are unpriced;
neither gross rewards nor starting inventory are represented as profit.

The review/goal reveal uses a session-only inspector revision. No save field,
authority command, reward, stock reservation or obligation cancellation is added.
Only a successful explicit choice increments it. Tests cover the exact request,
stale/queued context, unchanged automatic refresh, and the inspector's actual
scroll behavior.

## Verification boundaries

The first final-gate attempt is retained at
[v0.16-attempt-1/gates.json](../v0.16-attempt-1/gates.json). Compatibility and format
passed; headless compilation exited 101 because two new guidance fixture
coordinates used a nonexistent `LocalFrame::unproject` method. That failed record
and its log remain intact. The correction changed only those two test fixtures to
`LocalFrame::to_geo(...).unwrap()`; it did not change production behavior.

After `cargo fmt --all` exited 0, the focused command
`cargo test -p terra-sandbox --test guidance_commitments --offline --locked --jobs 3`
exited 0 with **9 passed, 0 failed, 0 ignored, 0 measured and 0 filtered out**.
Test execution took 0.21 seconds after a 47.42-second build. This includes the two
new price/recipe tests and the sustained save/resume journey. The parent has
restarted the complete final gates; this focused pass does not replace the failed
attempt or establish final native acceptance.

The geometric source review of all three body styles is separately recorded in
[content-cross-review.md](content-cross-review.md). These native frames add visible
fit and transition evidence; they do not prove continuous animation clearance,
pixel-identical shading, all body/palette combinations on the GPU, hardware
performance, controller coverage or screen-reader compatibility. No uncoached
first-time or returning-player study was conducted.

This report records candidate observations and the coordinated corrections. Final
test counts, source identity, fresh corrected screenshots and acceptance verdict
belong to the parent's final verification record. **Final release and native
acceptance remain pending.**
