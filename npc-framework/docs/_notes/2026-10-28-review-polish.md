Method: dual-agent (A: /root/polish_design_review · B: /root/polish_evidence_review)

# Native editor review, 2026-10-28

Baseline: main `641cea4`. Scope: game menu, Worlds editor and explorer F7 notebooks/map. This is an Operate
surface: complete editing tasks safely and efficiently while preserving the established dark panels,
Fira Mono, yellow selection/focus and native controls. The user requested implementation after review.

## Design specificity and strengths

The interface suits a simulation workbench: explicit units and preset identities, grouped forms and readable
notebook metadata. Actor validation, file destinations, conflict recovery and atomic publication already
provide useful protection. Current captures show no clipping at the captured default menu/F7 sizes.
Preserve this visual world; no replacement theme, new information architecture or animation is needed.

## Priorities

1. **P1: Worlds Back/Escape bypass dirty protection.** Drafts disappear on exit. Route Back, Escape and close
   through the existing discard/keep-editing decision. Test actual button and keyboard/message paths.
2. **P1: Worlds selection pairs cached values with fresh disk bytes.** An external edit before selection can
   be overwritten without the expected conflict warning. Parse and baseline one fresh read, retain complete
   original bytes, support existing UTF-8 BOM files and refuse files that became invalid.
3. **P1: F7 keyboard traversal cannot reach actions.** Buttons lack TabIndex; form Tab cycles only text fields.
   Make Tab/Shift+Tab traverse controls, including Save/Cancel, with visible focus and scroll-into-view.
   Keep fast book switching on Ctrl+Tab, protect drafts when focus leaves the form and shield world input.
4. **P2: Native controls lack accessible names.** Widget roles exist, but visible Text is not automatically a
   widget label. Name buttons/fields explicitly, keep names aligned with dynamic labels, expose selection
   state where implemented, and distinguish component-level validation from a real screen-reader test.
5. **P2: Secondary button text lacks enabled-state contrast.** DIM text measures 4.168:1 on normal fill and
   3.030:1 on hover. Use a dedicated readable detail token meeting 4.5:1 in all enabled states.

Supporting polish: restore the shared Worlds frame rather than replacing its Node; add the direct New-agent
action promised by empty Resume copy; bound and wrap F7 in narrower windows. Narrow-window clipping was a
source-only risk in the initial review, not a reproduced initial finding.

## Heuristic assessment

| Heuristic | Score /4 |
|---|---:|
| System status | 3 |
| User terminology | 3 |
| Control/freedom | 2 |
| Consistency | 2 |
| Error prevention | 1 |
| Recognition | 3 |
| Efficiency | 2 |
| Visual clarity | 3 |
| Error recovery | 3 |
| Help/documentation | 3 |
| Total | 25/40 |

This is the independent baseline assessment, not a post-change score. Cognitive load is moderate: seven
Worlds categories remain understandable in this technical tool. New grouping is unnecessary. Confidence
breaks when apparently protected editing loses a draft or overwrites an external change. A keyboard-only
operator encounters unreachable actions; a first-time user in empty Resume has only Back despite the New
instruction; a screen-reader user has roles without meaningful names. These are concrete recovery and
completion problems, not a reason to replace the visual identity.

## Evidence and limitations

Both assessments were isolated. A finished before B's detector findings entered synthesis. A inspected
current release F7 captures plus historical editor/menu context; B and controller inspected all five fresh
incumbent menu captures in `target/review-polish-895af602ff48/`. Current F7 captures were from
`target/phase-f-e2e-96eba0176a1f/`. Parent source review reproduced the Worlds cases with failing regressions.

The detector ran once, exit 0 and JSON `[]`, against the native Rust targets. It emitted no scanned-file or
rule counts, so meaningful native coverage is unestablished. This is not a clean bill from the detector.
Bevy has no DOM: browser visibility, overlays, injection and web live-server steps do not apply. No such
server was started. All owned capture processes exited. No OS keyboard/mouse/clipboard input was sent.

Questions skipped: the user already requested review followed by implementation and polishing. No binding
design decision or missing information requires another question.

## Implemented improvements and regression evidence

The pass addresses all five priorities and the bounded supporting polish. ADR 0020 records the native
keyboard and draft-safety decisions. Four new Worlds regressions failed before production changes;
the two new F7 keyboard/focus regressions also failed before implementation. No assertion or protected
oracle was removed. Existing book-cycle assertions now exercise the documented Ctrl+Tab shortcut.

The matched-read patch review caught BOM compatibility, and the keyboard review caught Escape discarding
drafts and a nested-group focus trap. Each was corrected with regression coverage. A separate native-role
test then caught required-component constructor precedence: AccessibleLabel before the widget produced
an Unknown role. Widget-first construction now preserves Button/TextInput roles; the regression passed.

- Menu unit suite: 118 passed, one existing ignored (`target/polish-menu-tests.log`). Includes Back/Escape/
  window-close safety, external edit/invalid file/BOM selection, empty Resume routing, focus and contrast.
- Menu/actor/view nextest pass: 235 passed, two skipped, one reported leaky
  (`target/polish-focused-tests.log`), followed by the focused native-role regression passing.
- Enabled secondary-text contrast: 7.117:1 normal, 5.174:1 hover, 9.213:1 pressed; regression enforces 4.5:1.
- All five release apps rebuilt; final role-correct build passed in 3m06s
  (`target/polish-release-final.log`).

## Native confirmation batch

Final menu captures: `target/review-polish-939b95d81cf2/summary.json`, `ok:true`, five PNGs. Independent
review confirmed brighter descriptions, focused fields, the direct empty-Resume New action, restored
Worlds frame, contained actions and unchanged overall identity. Controller also inspected Worlds.

Native/MCP confirmation: `target/polish-native-25f4a3fb601b/summary.json`, `ok:true`. In addition to the
entire registration/notebook/map/rotation/two-handover/Stop workflow, existing BRP KeyboardInput messages
typed "Keyboard waypoint", traversed all five landmark fields to Save, returned with Shift+Tab, retained
the draft with Escape, re-entered it with Tab and activated explicit Cancel with Enter. Border-color
inspection confirmed the actual focused buttons. Window reflection resized to 480 logical pixels for the
narrow check, then restored its width. Keyboard focus requested Bevy ScrollIntoView for the focused action.

All eight F7/form/close PNGs were inspected by the independent reviewer; the controller independently
inspected the typed Name and narrow Save images. Focus, Save/Cancel, wrapped hints/stamps and book content
were readable, with offscreen form content clipped by its intentional scroll viewport. Native handovers
preserved 336 explored cells; saves advanced tick 28→361. All owned processes exited; user MCP PID 57336
was preserved. No real OS keyboard/mouse/clipboard input or Narrator session was used.

This was one incumbent capture batch and one completed-implementation confirmation batch. No additional
cosmetic iteration is queued. Native screen-reader announcements and physical-device integration remain
manual boundaries.

## Full gate and review closure

`cargo xtask verify` passed: `target/verify-polish.json` reports **`ok:true`**, 1,256 tests passed, 12 skipped,
two slow and two reported leaky; doc tests 1 passed/10 ignored; architecture 7/7; all eight CLI replays;
six snapshots with none pending; fmt/Clippy/deny passed, no optional tools skipped. Existing protected
oracles and dependencies remain unchanged. Source stayed frozen through the final native confirmation
and complete gate; later documentation records evidence only.

All five assessed priorities are addressed. The exact local critique snapshot
`2026-10-28T08-15-17Z__apps-game-menu-src-main-rs.md` was read and closed after verification. Its 25/40 is
the independent baseline score, not an invented post-change rating; this was the first run for the target.
The local archive is ignored, the temporary snapshot body was removed, and this repository report remains
the durable evidence. No browser overlay or live server was applicable to the Bevy UI.
