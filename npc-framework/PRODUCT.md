# Island Village product

<!-- impeccable:product-schema 1 -->

## Platform

Native desktop game in Rust and Bevy. Windows is the verified development platform.
This is an existing 3D application, not a web or mobile product.

## Users

Players joining a self-sustaining village, and developers inspecting its deterministic
NPC simulation. The ordinary entry experience serves the player first; colony and
terrain tools remain available as explicit advanced modes.

## Product Purpose

Make a home in a living island: arrive, meet residents, gather real materials, craft,
build a cottage, grow crops, trade, and survive the changing day and season. The NPC
economy continues to provide the surrounding world rather than waiting for scripted
player milestones.

## Confirmed direction

On 2026-11-09 the user asked for a coherent Stardew Valley-inspired experience from
starting the game through menus into play and building, with substantial UI/UX work.
They explicitly chose to retain the existing 3D island and current survival and
permanent-death rules. Existing KayKit assets and the native island visual language
are retained. The goal is one continuous application flow.

## Capabilities and constraints

- One title/new/continue/play/pause/save journey in one application window.
- Clear contextual actions and a compact player HUD; inventory, crafting, building,
  farming, trade and residents belong to a consistent player workspace.
- Real inventories, reservations, costs, work duration and NPC trade. Guidance must
  not grant materials, fabricate progress or reveal unknown world objects.
- Integer authoritative state, stable identities, recorded inputs and a host-owned
  schedule remain mandatory. Presentation never owns simulation mechanics.
- Preserve existing saves and developer scenarios. New authoring data is copied
  into a fresh world; a continued world retains its construction rules.
- Keep the framework's general capabilities separate from island content and host UI.

## Operating context

The player starts Island Village, chooses New or Continue, enters the world as a
character and receives a useful next action based on actual conditions. Building and
farming are normal actions, not hidden developer commands. Pause, save and return to
the title have clear outcomes and preserve the world when saving fails.

## Product principles

1. The world is alive before the player arrives.
2. Show the next useful action and its real requirements.
3. Use one vocabulary and control pattern from title to gameplay.
4. Preserve ownership: simulation, input coordination, rendering and files have
   different responsibilities.
5. Prefer visible, recoverable failure over silent loss of progress.

## Evidence on hand

The previous iteration passed the full gate with 1,816 tests and included a 14-day
autonomous economy audit, native captures, and architecture/visual reviews. See
`docs/_notes/2026-11-08-island-colony-validation.md`. Those results are baseline
evidence, not verification of this new increment.

## Accessibility and interaction

Keyboard and mouse must both expose the primary flow. Use readable contrast, visible
focus, comfortably sized controls, explicit disabled reasons and scroll affordances.
Verify both 1280×720 and 1024×768 logical window sizes, including Windows DPI scaling.
