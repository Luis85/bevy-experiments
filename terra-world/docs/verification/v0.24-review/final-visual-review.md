# Independent final native visual review — 0.24

## Correction and withdrawal — exact pixel comparison

**The earlier claims below that text disappeared in captures 209 and 220 are
incorrect and are withdrawn.** The later statement that these images established
a concrete transition-sensitive rendering problem is also withdrawn. Those
conclusions came from an incorrect full-frame visual assessment, not from a
change in the actual captured text pixels.

An independent byte-level readback of the original PNGs found:

- The named damage-review region in **208 and 209** is exactly identical: all
  48,928 RGB pixels match, including the complete target and consequence text.
- The Build prose region in **219 and 220**, **220 and 310**, and **310 and 311**
  is exactly identical in each comparison: all 52,542 RGB pixels match. The
  original 220 crop contains the complete inventory/home/section text.
- All six original PNG hashes, byte counts and dimensions match their retained
  receipts; the image and receipt files remained unchanged during comparison.

Full coordinates, raw PNG hashes, decoded crop hashes, dimensions and zero-difference
counts are retained in [pixel-comparison.json](pixel-comparison.json), with the
method and interpretation in [pixel-comparison.md](pixel-comparison.md). Exact
original crops of 209 and 220 were also inspected. The evidence does **not**
establish a renderer defect or a visual improvement from the experimental
focus-triggered refresh. That experiment must not be described as fixing a
demonstrated disappearing-text problem.

The integrator directed removal of the three experimental additions while
retaining the independently useful shortened labels. Revert/rebuild and final
native/canonical acceptance must be established by their actual owner receipts;
this correction does not invent those outcomes. The observations below are
preserved as historical review notes, superseded wherever they assert missing
text or an established rendering defect. No original capture, process record or
receipt has been rewritten.

## Original review notes — retained with the correction above

Inspected the actual full-resolution PNGs below on 30 October 2026. Each is
**1100 × 700**; the integrator identifies this session as **150% text**. The
capture receipts establish the pixel dimensions and process association, not the
text-scale setting independently. No UI input, native launch, Cargo run or source
edit was performed by this reviewer.

| Capture | Observed pixels | Review result |
|---|---|---|
| [207-confirmation-minimum-property.png](../v0.24-native/207-confirmation-minimum-property.png) | Property names `fence #2`, with health `15/40`. The shortened `Review damage` control fits within the panel. Save and Load labels are visible and distinct. | The requested target/control copy is readable at this captured size. |
| [208-confirmation-damage-review.png](../v0.24-native/208-confirmation-damage-review.png) | The review names `Damage fence #2?`, shows health `15 → 0 (−15)`, explains destruction, cleared homes and no material refund, and states that Cancel leaves the structure unchanged. Separate `Cancel building damage` and `Confirm damage` buttons are both fully visible. | The named target, consequence and two choices are visible together. The following inventory section continues below the scroll viewport; this image does not show the entire panel at once. |
| [209-confirmation-cancel-focus.png](../v0.24-native/209-confirmation-cancel-focus.png) | A pale outline visibly focuses `Cancel building damage`; Confirm remains separate below it. Command-focus navigation help appears in the bottom panel. However, the named target/consequence text block visible above Cancel in 208 is completely blank in this image. | Safe Cancel focus is visible, but this image is not complete simultaneous evidence of named consequence review plus focused controls. The blank text area was reported promptly for follow-up. |

## Material finding requiring follow-up

**The review explanation is absent in the exact 209 capture while both choices
remain visible.** The text was visible in 208; the review area retains its space
in 209. The pixels alone do not identify whether this is transient rendering,
scroll/focus behavior or another cause. Before treating the focused confirmation
state as fully polished, the integrator should inspect a fresh observation of
that state and determine whether the explanation remains available. Preserve 209
and its original receipt rather than rewriting the observation.

## Provenance readback and limits

For all three PNGs, actual byte counts, SHA-256 hashes and decoded dimensions
matched their separate capture receipts. Each receipt reports window PID
**76362** before and after capture with status `available` and no PID error.
The captures occurred at `12:40:07`, `12:43:53` and `12:44:09` UTC, respectively,
after the recorded direct `confirmation-building` start at `12:36:24` UTC.

The recorded start binary matched both the actual executable bytes and
`native-final-build-01.json`:
`509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`.
The build's 390-input map and retained log hashes were reread successfully. Its
input map equaled current implementation inputs, and its canonical digest
`37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`
matched the process-start record. The input-map file hash differs because it
hashes the formatted JSON bytes; this was checked separately against the build
receipt rather than confused with the canonical map digest.

At this review readback, the native process terminal record was not yet present.
No clean exit, finished-source identity or completed canonical acceptance is
inferred from these screenshots. The final registry audit must establish those
separate conditions from actual completed records.

This review establishes observed visual readability, distinct choices and a
visible Cancel focus in these particular images. It does not execute or qualify
controller behavior, screen readers, all keyboard paths, 200% text/reflow,
physical displays, Windows/GPU behavior, contrast compliance or uncoached user
comprehension. The native/domain regression suites and wider product acceptance
retain their separate scopes.

## Follow-up observation: transition-sensitive text loss

The original finding above is retained unchanged. Subsequently inspected the
actual six additional PNGs, and independently checked their bytes, receipt hashes,
decoded **1100 × 700** dimensions and matching before/after window PID **76362**.
These are observations of the same executable before the targeted correction.

| Follow-up capture | Observed pixels |
|---|---|
| [216-confirmation-repeat-cancel-focus.png](../v0.24-native/216-confirmation-repeat-cancel-focus.png) | The full named fence/health/destruction/no-refund/Cancel paragraph and the Cancel focus outline are visible together. |
| [217-confirmation-focused-stable.png](../v0.24-native/217-confirmation-focused-stable.png) | The same complete paragraph and Cancel outline remain visible. |
| [218-confirmation-focused-readback.png](../v0.24-native/218-confirmation-focused-readback.png) | The paragraph and outline remain visible on another readback. |
| [219-confirmation-repeat-cancelled.png](../v0.24-native/219-confirmation-repeat-cancelled.png) | Cancellation feedback says no structure changed. The Build body's inventory, property counts and home status are readable; command focus is visibly on Choose activity. |
| [220-confirmation-repeat-focus-exited.png](../v0.24-native/220-confirmation-repeat-focus-exited.png) | Command focus is off. The nonbutton Build-body inventory/status/section text visible in 219 is absent while its spaces and action buttons remain. |
| [221-confirmation-retained-fence.png](../v0.24-native/221-confirmation-retained-fence.png) | Following the integrator's scroll action, Property and the named fence target/health `15/40` are readable again. |

The new observations occurred between `12:46:31` and `12:48:07` UTC. Together they
establish a concrete transition-sensitive rendering problem: text can be present,
disappear after focus changes and reappear after scrolling. They do not prove the
precise upstream engine defect. The repeated focused images do not erase the
missing-text observations in 209 and 220.

The `confirmation-building` terminal record is now present: PID 76362 exited 0 at
`12:49:31.188663+00:00`, with unchanged source during that recorded run. The separate
`confirmation-fleet` process, PID 77552, also exited 0 at `12:53:06.638400+00:00`,
but reports `source_inputs_unchanged: false` because the correction landed while
it was active. Both used the earlier binary recorded above. Neither this closure
nor the Fleet exit promotes that earlier executable to acceptance of the new
correction; the source-drift record must remain excluded from final provenance.

## Independent source review of the targeted correction

Read the player's new `refresh_inspector_text_on_focus_transition` system and its
registration/regression fixture in `crates/terra-demo/src/interface.rs`. The
system runs after `UiSystems::PostLayout`, acts only when command-focus active
state changes, and marks the existing `TextColor` change tick of text beneath the
inspector. It walks a bounded ancestor chain, stops at buttons, and excludes
nontext entities and other surfaces. Initial observation and unchanged focus
frames are inert. Authored colors, text, layout, scroll and review state are not
rewritten by this helper.

Checked the pinned primary implementation in `bevy_ui_render 0.20.0`,
`src/lib.rs`: `extract_uinode_changes` includes `Changed<TextColor>` among the
signals that invalidate retained rendering records. `extract_text_sections`
then reads the changed nodes' existing text layout, transform, clipping and
colors. This supports the narrow refresh mechanism; it does not identify or
prove a general engine defect.

The added fixture checks both transition directions and repeated transitions,
invalidation of static headings and review prose, exclusion of buttons/other
surfaces, preserved values/geometry/scroll/review token, and no repeated work on
stable frames. Those assertions address the helper's scope and side effects.
They cannot establish rendered pixels. A fixture-only `String` versus `&str`
compile correction was reported by the owner without changing the production
helper; the failed attempt remains retained by that owner.

**No blocking source finding was found in this minimal correction.** Native
acceptance remains pending a new executable and actual repeated focus entry,
focus exit and idle readback with the explanation still visible. This reviewer
has not launched that executable or run Cargo, and has not created an approval
registry or changed source/helpers. Final closure requires the new observed
pixels and their completed process/canonical evidence.

## Final readback closure after exact reversion

The experimental refresh was removed by its owner. Independent readback of
`native-final-build-03.json`, its input map and retained log, the current binary,
and the final Fleet process confirms the following actual identities:

- The restored `interface.rs` SHA-256 is
  `36c1e855a5e5b93c3809e60b898514601e6fdcb217325b53c250b17d382b22be`.
- All **390** build-03 inputs equal build-01's input map and the current
  implementation map. Their canonical digest is
  `37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`.
- Build 03 completed at `13:09:39.021005+00:00`, exit 0, with no changed inputs.
  Its executable bytes again hash to
  `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`,
  the same executable identity as the original clean Building confirmation.
- The direct `confirmation-final-fleet` process, PID **79904**, ran from
  `13:10:39.172742+00:00` through `13:13:44.633927+00:00`, exited 0, and records
  unchanged source with the same start/finish digest. Its complete log hash,
  start/terminal identity and current executable bytes matched on readback.

Independently inspected the actual following four final PNGs at **1440 × 900**,
then inspected exact unscaled crops of their inspector body text. Their complete
PNG bytes/dimensions match both the capture receipts and the integrator's
`APPROVED-REGISTRY.json`. Each has matching before/after window PID 79904, and
every capture interval falls within that process's completed lifetime.

| Final capture | Observed content |
|---|---|
| [333-confirmation-final-fleet-resumed.png](../v0.24-native/333-confirmation-final-fleet-resumed.png) | Activities names the active actor and assigned departure, shows one completed supply contract/40 community XP, and describes the current parked route and paused payroll. The next-step explanation distinguishes reviewing the assigned service from authorizing travel and payroll. The filename does not mean the service was started; the displayed service is paused. |
| [334-confirmation-final-fleet-company.png](../v0.24-native/334-confirmation-final-fleet-company.png) | `Field Service`, company cash `39.34 cr`, employee/route counts and section choices are readable. The selected transfer amount is `50.00 cr`; Deposit is available and Withdraw is visibly disabled at the displayed lower company balance. The account explanation continues in the existing scroll area. |
| [335-confirmation-final-fleet-drivers.png](../v0.24-native/335-confirmation-final-fleet-drivers.png) | The selected employee `Mira` is named with `Parked — route paused`, zero deliveries, household wallet, next stop/distance and empty-load status. The assigned Trail utility pickup's fuel, condition and capacity are shown. Hiring and unavailable employee actions remain visually distinct. |
| [336-confirmation-final-fleet-assignment.png](../v0.24-native/336-confirmation-final-fleet-assignment.png) | The vehicle-to-route assignment and parked state are named. Before-dispatch text explains the first departure, requested load, funded-delivery receipt, payroll coverage limit, finite issuer funding and variable conditions. `Start / resume`, unavailable `Pause safely`, and `Release parked empty vehicle` are separate controls; the nearby explanation states payroll, pause and release consequences. |

**No new visual blocker was found in this final readback.** The earlier missing-
text assertions remain withdrawn under the exact-pixel correction at the top of
this report. No experimental-renderer-fix claim is reinstated.

This closes the requested independent view and identity readback. It does not
execute a Fleet command, prove every stale-intent rejection, measure contrast,
qualify all keyboard/controller or assistive paths, or certify 200% text,
Windows/GPU/physical-device behavior. The original clean Building process and
the fresh final Fleet process are the integrator's two explicitly reviewed
processes; the earlier Fleet source-drift run remains excluded. Canonical
acceptance was still running at this readback. The full accepted-registry audit
and complete canonical receipt remain separate final gates; this reviewer did
not run accepted mode early or modify any source, helper or native process.
