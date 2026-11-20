# Terra World 0.21 player-experience cross-review

Independent reviewer: reliability owner. Reviewed 29 October 2026 after the
player owner's implementation. This was a read-only cross-review of
`accessibility.rs`, `flow.rs`, `interface.rs`, `launcher/ui.rs` and the readiness
change in `interaction.rs`, with adjacent input, renderer and persistence
contracts inspected as needed. The reliability owner's separate output-path
call-site edits in `interaction.rs` are not presented as independent review.

## Disposition

No additional production blocker was found in the selected changes. Source
inspection supports the intended input isolation, Loading navigation, save-policy
copy and geographic readiness boundaries. Execution is coordinated by the
integrator; this review does not turn unexecuted tests into a passing result.

## Modal keyboard and native event ordering

`AccessibleUiPlugin` schedules `capture_keyboard_focus` after sampled player
input and command-context capture, before native `InputFocusSystems::Dispatch`.
An open draft reacquires a keyboard target if focus was lost or redirected to an
underlying field. The overlay's `TabGroup::modal()` contains the draft and five
editing actions. Character-grid buttons retain their existing pointer/controller
operation without adding fifty tab stops.

`keyboard_view` preserves logical focus (`Draft` or a particular editing action)
while replacing the keyboard subtree. It assigns the replacement entity to
`InputFocus`; the chained Update systems apply the deferred tree before styling.
Apply/Discard restore the original field when it is still the reviewed target.
An invalid or disabled target closes the draft rather than applying to another
field. The existing concurrent-edit and bounded-command checks remain in place.

The stronger boundary is at the action observers: launcher, session and world
buttons reject activation while the keyboard is open or its closing frame is
consumed. This also covers a focused event already addressed to an underlying
button. The observer guard does not depend solely on tab traversal. Native Space
activating a focused keyboard action is excluded from draft insertion, and Enter
on such an action is not also interpreted as a second Apply. Closing clears the
pending keyboard actions; the consumed flag prevents the same event reaching a
parent menu. Subsequent navigation resets consumption, so suppression is not
permanent. Command-context capture continues to block world controls during
editing; parent processing follows `ControllerNavigation` in Update.

The main regression uses actual native KeyboardInput dispatch, TabNavigation and
ButtonPlugin paths, checks Tab wrapping and focus repair, applies only the draft,
discards an edited draft, and demonstrates parent activation working after the
close event has been released. Parent-observer fixtures explicitly flush the
deferred ButtonPlugin Activate command before changing their guard state. That
flush is necessary for both negative assertions and positive controls; the
earlier fixture failures did not establish a production defect.

## Loading controls and readiness

`GameFlow::controller_menu_active` now includes Loading while
`GameFlow::blocks_input` continues to block every non-Playing phase. Saving remains
noninteractive. The Loading-control test selects the actual button mapping for
network cancellation, local Save-and-return and terminal-storage Close; it checks
the action queue and continued world-input blocking. Existing storage terminal
admission is reused rather than bypassed.

`loading_terrain_counts` uses the same real clipped/wrapped neighborhood as
population bootstrap, centered on the saved player's position when available.
It counts only relevant resident keys and their intersection with the renderer's
current integrated source keys. The renderer's integrated set excludes stale
fallback geometry from a replaced source. The startup cap remains nine, while
the six-tile polar radius-one and single-tile cases can complete. Missing or
invalid neighborhoods cannot report readiness through a zero-length threshold.

`poll_state` retains the account, connected-network-with-actor, no-pending-storage
and no-blocked-storage gates. Corrected terrain counts alone do not authorize a
world. Status percentages use the same counts and avoid division by zero.
`interaction::verification` intentionally waits for the whole requested display
neighborhood: all relevant source and integrated keys, no source requests in
flight, and no pending/building meshes, after the existing frame threshold.
The distinction between capped gameplay startup and complete screenshot
readiness remains explicit.

## Save contract and presentation

The shared `save_contract_description` matches the inspected implementation:
managed portable saves have `save_managed_json_on_exit`; direct-play JSON needs
F5 or the explicit Save-and-exit path; regional authority uses checkpoints; a
network client cannot save or restore the host's world. The session status and
World drawer consume this same description. The existing regional interval and
dormant-record explanation remain. The table-driven copy test covers all four
contexts, including host authority taking precedence over local flags.

Word-or-character wrapping was added to the session and keyboard text bundles.
This is a structural improvement, not proof of fitting every large-text and
long-path layout. The keyboard's explicit focus borders and lifecycle checks
likewise do not establish native assistive naming or screen-reader behavior.

## Evidence limits

No Cargo command was launched by this cross-reviewer, avoiding competition with
the shared acceptance run. At dispatch, the integrator reported 192 passing demo
tests and three failures in the new deferred-event fixtures; the owner had
corrected those fixtures and the rerun was pending. The final canonical receipt
and focused execution logs determine fresh pass/fail status.

No new native/browser capture, pixel inspection, physical controller exercise,
assistive-device qualification or rendered contrast/clipping measurement was
performed here. Historical captures are historical evidence. The independent
technical-interface assessment and unavailable detector limitation are recorded
in `reliability.md`.
