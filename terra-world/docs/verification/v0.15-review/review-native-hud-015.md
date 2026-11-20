# Native driving HUD polish — 0.15

## Observed issue

Inspected `/tmp/terra-native/v15-candidate-comet-world-controls.png` before editing. At 150% text scale, the open World drawer covered most of the left driving HUD, while the wider underlying HUD exposed chopped speed/navigation text along the drawer's right edge.

`driving::route_map` already hid its floating card whenever World, Inspector or Performance was open. `driving::hud` checked only vehicle possession and menu state, so it kept rendering underneath those panels.

## Scoped fix

Only `crates/terra-demo/src/driving.rs` changed:
- A shared `floating_huds_allowed` rule checks that all three drawers are closed.
- Both floating driving cards apply that rule.
- Closing the drawers restores the same HUD entities.
- The footer, driving input, panel controls, source banner and world markers remain untouched.

This follows existing route-map behavior and removes the inconsistent competing overlay without introducing a new layout or authority rule.

## Verification

A new ECS regression runs the real HUD and route-map systems, opens every drawer individually and in combination, closes them again, and checks both first- and third-person possession as well as RTS. It asserts that hiding HUDs does not alter the requested panel states.

Focused driving suite passed all 9 tests; the exact existing `scaled_footer_reserves_the_same_world_panel_and_floating_hud_bounds` assertion also passed. Both runs had zero failures or ignored tests. The running candidate executable was not rebuilt or replaced during the parent's native review; final native screenshot confirmation requires the parent's next candidate build.


Receipts:

- `/tmp/terra-build/content-015-hud-polish.log` — SHA-256 `97c9ace9fa35adc6bbe57d295ad656cc1914e718dd73893923b51da5cc4874cb`
- `/tmp/terra-build/content-015-hud-layout.log` — SHA-256 `369a0a77805c7448159810456ba52301f85f18c7193aaa6f24a2dd5dd8cfd352`

Shared Cargo slot released to parent. Source formatted with rustfmt; the normal test build compiled all native code without replacing the running executable.
