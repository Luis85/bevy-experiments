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

The current increment expands the island into a connected coastal world with several
villages, varied terrain and resident appearances, changing occupations, renewable
ecology, and meaningful exploration and restoration. The existing 3D presentation,
survival rules and continuous native flow remain the foundation. Actual world
changes and readable activities take priority over decorative claims of simulation.

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

The [living-world validation record](docs/_notes/2026-11-09-living-world-validation.md)
records the current seed-7, 336,000-tick audit: all 42 founding residents remained
alive and housed, with 8,400 currency conserved, 2,846 transactions and 94 coats
produced. This covers 14 days through winter and spring, not a full year or an
indefinite guarantee. Native confirmation includes 54 captures at 1280×720 and
1024×768, three actual village journeys, and home visuals retained across Continue
and New. Six additional inspected captures confirm the final terrain shading and
editor diagnostics corrections. Physical input and GPU/FPS performance targets
remain unverified. The [startup follow-up](docs/_notes/2026-11-10-terrain-startup-polish.md)
confirms an opaque preparation screen, coherent terrain reveal and water/land
chunk updates with zero empty-mesh warnings. Its full gate returned `ok: true`:
1,960 tests passed, with all
architecture, replay, snapshot and dependency-policy stages passing. The validation
record retains the skipped/ignored counts and the limits of each check.

The [previous player-flow validation](docs/_notes/2026-11-09-island-player-flow-validation.md)
remains baseline evidence; its passing gate does not verify this increment.

## Accessibility and interaction

Keyboard and mouse must both expose the primary flow. Use readable contrast, visible
focus, comfortably sized controls, explicit disabled reasons and scroll affordances.
Verify both 1280×720 and 1024×768 logical window sizes, including Windows DPI scaling.
