# ADR 0020: Native editor safety and keyboard navigation

- Status: Accepted
- Date: 2026-10-28

## Decision

Worlds reads and parses a selected file from the same byte buffer used for optimistic save checks. Strip
an optional UTF-8 BOM only while parsing; retain complete original bytes. Files changed or made invalid
since listing must not silently become a stale draft with an accepted new baseline. Back, Escape and
window close share explicit discard/keep-editing protection. Escape cancels an existing confirmation.

F7 follows native control navigation: Tab/Shift+Tab traverse controls, including form Save/Cancel;
Enter/Space activate Bevy widget buttons, and Ctrl+Tab cycles books. Open forms retain keyboard access
after focus leaves them. Escape blurs without discarding; only explicit Cancel drops a draft. Use Bevy's
TabNavigation and ScrollIntoView rather than duplicating ordering or scroll geometry.

NotesControl marks keyboard ownership, including focused buttons. NotesInput runs before game input and
must share host modal gates; prompt and close modals gate it explicitly. UI and accessibility state remain
on presentation entities. Native AccessibleLabel and selection markers supply names/state without new
dependencies. Component tests do not establish Windows screen-reader integration.

## Verification

Regressions cover Worlds navigation, external edits/invalid files/BOM baselines; F7 traversal, native button
activation, retained drafts, modal gating, focus styles and world-input shielding; and enabled-state text
contrast. Native capture evidence and the full workspace gate are recorded in
[the follow-up review](../_notes/2026-10-28-review-polish.md). No authoritative model, hash registration,
replay/snapshot baseline, credential protocol or performance budget changes.
