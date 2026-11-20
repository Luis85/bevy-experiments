# Player experience review and correction — 0.18

Reviewed the 0.17 PRODUCT.md, DESIGN.md, input sampling and control consumers, and historical controls fixture `docs/screenshots/v8-controls-small.png`. The historical image establishes incumbent layout only; it is not fresh 0.18 native evidence. Impeccable polish/craft-floor guidance was read before changes. The existing forest palette, typography, controls screen layout and menus are retained.

## Reproduced source-level issue

The calibrated wheel disarms after focus loss until neutral, but the standard gamepad previously resumed held throttle, movement and actions immediately when focus returned. A different gamepad selected after disconnect could likewise inject already-held world input without a deliberate fresh act. Disabling/re-enabling or leaving binding capture had the same ownership boundary.

## Implemented and independently reviewed

`StandardPadArming` tracks the selected standard-pad entity in process-local state. A newly selected pad, focus loss, absence/disable or binding capture requires a neutral sample before world input resumes. Both sticks must be within their configured radial deadzone, both analog triggers must be zero, and mapped world buttons must be released. Trigger neutrality uses zero because gameplay consumes raw triggers; the independent review caught and corrected an initial stick-deadzone threshold that would have admitted low held throttle.

Until armed, world movement/look, drive, vertical, sprint, bound action edges, RTS reticle/selection fields, zoom and release intent remain absent. Menus retain Start, direction, accept/back/text controls, shoulder page changes and management-focus access. Keyboard controls remain available. The existing controller status explains `release controls to resume world input`; help names the handoff. No preference/save/wire schema changes.

## Validation

Executed `PATH=/tmp/terra-build/rust/bin:$PATH CARGO_HOME=/tmp/terra-build/cargo CARGO_TARGET_DIR=/tmp/terra-build/target cargo test -p terra-demo player_input::tests --offline` in the definitive integration tree: **8 passed, 0 failed, 0 ignored, 175 filtered out**. The tested Cargo package version at this point remained 0.17.0; final 0.18 integration acceptance belongs to the release-level gates. Log: `player-input-tests.log` beside this report. Full-workspace `cargo fmt --all -- --check` also exited 0. An unnecessary queued retry was cancelled after the first focused run passed; it is not counted as another passing gate. The tests include `standard_pad_neutral_rearm_preserves_menus_keyboard_and_device_handoffs` (real Bevy App sampling across focus regain, held triggers/stick, menu Start/South, keyboard availability, neutral rearm, device replacement, disable/enable and binding capture), and `standard_pad_rearm_requires_zero_triggers_instead_of_stick_deadzone`. Existing disconnect and wheel regressions remain; the disconnect fixture now begins neutral before deliberate accelerator use.

## Remaining limits

This is an executable synthetic-input contract, not physical controller, wheel, driver, OS reconnect or assistive-device qualification. Fresh native screenshots were not collected by this subtask. Windows execution, hardware frame tails, localization, assistive access, content breadth, uncoached motivation and career balance remain open. No claim of complete accessibility or gamepad hardware certification follows from these tests.
