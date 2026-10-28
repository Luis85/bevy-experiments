# Island Village UI documentation

This records an ordinary extension of the existing Windows Bevy surface, following
`island-village-direction.md`. The user retained the KayKit 3D scene and native controls.
The implementation extends `content/testbed_view/src/village.rs`,
`content/testbed_view/src/village_world.rs` and `apps/game_menu/src/worlds.rs`.
It does not establish a new project visual identity.

`PRODUCT.md`, `DESIGN.md` and `.impeccable/design.json` were absent at this documentation
pass. They remain absent: the ordinary-extension exception in Impeccable new-work §7
requires comparison with the incumbent system and evidence reporting, rather than
inventing a global system for this scene. Existing design sources remain authoritative.

## Incumbent conventions checked

The comparison used `apps/game_menu/src/ui.rs` and
`content/testbed_view/src/explorer_panel.rs`, alongside the implemented island sources.

- **Palette:** the explorer panel supplies the island's near-black ground
  (`Color::srgba(0.05, 0.06, 0.08, 0.88)`), ivory text
  (`Color::srgb(0.93, 0.95, 0.90)`) and gold note/focus color
  (`Color::srgb(0.95, 0.80, 0.45)`). The island uses the same ground as an opaque
  `Color::srgb(0.05, 0.06, 0.08)` for readable HUD and reader text over the world.
  Both incumbent sources and the island use slate controls
  (`Color::srgb(0.20, 0.24, 0.30)`) and the same lighter hover fill
  (`Color::srgb(0.27, 0.32, 0.40)`). These are code values, not a new token layer.
- **Typography:** Bevy's default Fira Mono and printable ASCII continue from the menu.
  The explorer panel already uses compact 11–13 logical-pixel text. The island uses
  17-pixel gold title text, 12-pixel HUD/control text and 13-pixel reader text; it does
  not introduce a display face or a different font family.
- **Controls:** the incumbent uses native `ui_widgets` buttons and text inputs,
  accessible labels, tab navigation, disabled state and a gold focus border. The
  island uses `WidgetButton`, `AccessibleLabel`, `TabIndex`, focus handling and the
  same gold border convention. Its compact 28-pixel minimum-height buttons and square
  HUD/reader surfaces are local scene choices; the menu's rounded panels and larger
  buttons remain intact. The scene does not prescribe these sizes or shapes globally.
- **Layout:** existing menus and notebook panels constrain width, permit wrapping and
  use scroll areas. The island HUD sits at the top right, 370 logical pixels wide with
  a 94% width limit. Its action row wraps. The shared diary/scheduler reader is 92%
  wide, capped at 740 pixels, and 60% high, capped at 650 pixels. Its controls wrap;
  its text area uses `ScrollArea` and vertical overflow. Those measurements document
  this implementation, rather than a project spacing scale.

## Extension behavior recorded

The world remains the primary surface: the existing RTS camera frames KayKit buildings,
characters and props, with connected paths, workplaces, resource labels, water and coast.
`village_world.rs` reads the simulation calendar and night state to change terrain,
directional light, ambient light and sky. These effects are presentation state; this
documentation does not redefine the integer simulation model.

The HUD reports day, season, the played life, coins, health, reserves, supply prices and
the AI scheduler state. Found diaries opens a bounded reader with page and diary
navigation. New life is an explicit action available after permanent death. The queued
successor notice clears when the played actor ID changes. The reconnect hint is derived
only for a living successor in an explorer session without its external controller.

The scheduler panel explains idle timing and the exact conversation-link command;
its pause/resume control changes the current host session. The Worlds editor adds
an AI scheduler page through the existing menu helpers. Its instructions keep the
conversation ID, program and workspace in operator-owned host settings, separate from
the world preset. No real provider continuation is implied by a displayed setup panel.

No shipping raster was created. Existing bundled model/texture attribution remains
the asset provenance authority.

## Evidence and review scope

The documentation pass read `island-village-direction.md`,
`island-village-evidence.md`, the incumbent and extension sources above, and
`target/island-review-fix-native.json`. That native result reports `ok: true` and
`owned_process_exited: true`.

The review packet contains eleven PNGs under
`.impeccable/review/island-village/`: `island-day.png`, `island-narrow-day.png`,
`island-scheduler.png`, `island-narrow-scheduler.png`, `island-permanent-death.png`,
`island-recovered-diary.png`, `island-night.png`, `island-winter.png`, `worlds.png`,
`explorer_new.png` and `testbed.png`. The scene evidence includes a 480-logical-pixel
window. All eleven were opened in the fresh finish review; the eight scene files
were refreshed after the successor-notice correction and opened again.

For the incumbent comparison, this documentation pass additionally opened the final
day scene, narrow scheduler, recovered diary and Worlds captures. They show the
shared Fira Mono, dark grounds, ivory text, slate controls and gold focus treatment;
the scene uses these conventions over the retained 3D village.

The fresh full finish review returned **fix** for the stale successor announcement.
After correction, the same reviewer's verdict pass returned **ship**, scoring that
listed fix resolved. This is approval at the fix-list scope, not a new whole-surface
audit. No detector ran because its HTML/CSS checks do not support native Rust UI.
Runtime checks, exact commands and final gate results are maintained in
[`island-village-evidence.md`](island-village-evidence.md). This note does not
assert a full `cargo xtask verify` result independently of that evidence.

The required recheck after the later preset-installation compatibility correction
read `content/testbed_content/src/worlds.rs` and its regression in
`content/testbed_content/tests/island_village.rs`. An existing folder now receives
the additive island preset only when it already contains a valid preset; invalid-only
folders retain their disabled-start behavior, and existing custom preset bytes are
preserved. The build thread reports the focused regression passed. This correction
changes no native view or menu layout source, so the eleven captures still represent
valid installations and the incumbent comparison remains applicable. No further
capture or visual review was performed for that nonvisual correction.

## Limits and uncanonized differences

Physical keyboard/mouse input, OS screen-reader announcements and authenticated
provider integration remain operator checks. Native widget semantics and automated
activation/focus checks do not establish those outcomes. The scheduler test used a
fake runner; no real AI provider was launched.

The incumbent menu and explorer already differ in panel opacity, type size and control
density. The island follows the explorer's compact overlay conventions while reusing
menu controls in Worlds. This pass records that variation without repairing it or
promoting it to a universal design rule. No further defect hunt, visual redesign or
unrelated workspace change is part of this documentation pass.
