# Island Village

For the title-to-cottage player journey, start with [Life on Island Village](island-life.md).
This guide covers the colony ledger, terrain editor and developer entry points.

Run from `npc-framework/`:

```powershell
cargo build --locked -p game_menu -p actor_testbed -p npcctl
.\target\debug\game_menu.exe
```

The normal title offers **New game** and **Continue**. From play, open **Menu** and
choose **Inspect village** or **Landscape tools**. These modes share the same local
island, saved as `local-island.world.json` in the configured agents folder. During
play it saves every minute, on **Ctrl+S** (or **F9**), and when leaving through the
pause menu or closing the window. A second window cannot open the same save. Save
failures appear on screen; a failed close-save leaves the window open so you can retry.

For an inspector-first world, choose **New game**, name the potential visitor and
select **Oversee the village**. The colony runs without beginning that visitor's
mortal life. With an existing save, **Oversee the village** is also available on the
title. **Play character** in the ledger performs the actual arrival when needed.

`cargo run --locked -p game_menu` remains the development entry point. Running the
already-built executable avoids recompiling a different Cargo feature combination.

If the Windows renderer exits with `DeviceLost`, Vulkan was verified during the
November 8 colony iteration. Select it for the current PowerShell session:

```powershell
$env:WGPU_BACKEND = "vulkan"
.\target\debug\game_menu.exe
```

This is a process/session setting, not a global driver or Rust configuration change.

**New game** keeps the previous save as a numbered `.previous-N.json` backup and
uses the world's loaded immutable rules. **Continue** keeps the current island.
Backups are never overwritten. To change the rules of a new world, use a separate
save path with `--island-rules`, or the developer toolbox's fresh-island workflow.

For a disposable world without persistence:

```powershell
cargo run --locked -p actor_testbed -- --scenario island_colony --island-mode inspector
```

Use `--island-mode character` or `editor` for the other entry points. Add
`--island-save C:/NPC/my-island.world.json` to create or resume a separate persistent
island. Existing saves retain their construction rules. Use a new path to start a
fresh world with edited rules; incompatible saves are preserved and refused.
Add `--island-rules C:/NPC/worlds/island_colony.json` to use your edited preset for a
new local island, or `--island-new` to start fresh with a backup at the same path.

## Inspect and manage

**F11** opens the colony ledger. **Village** shows population, beds, wellbeing, stock
and production. Residents shows a person's needs, work, household and current
planning explanation. Click a resident in the world or ledger to inspect them.
Planning explanations distinguish an unfunded goal, missing current prerequisites
and a bounded search limit. A search-limit message does not mean a goal is impossible;
the shared work policy continues to operate.

- Set a household food reserve of one to four days. This feeds funded demand;
  it creates neither goods nor purchasing power.
- Enable or disable social gatherings. Relationships grow when residents meet
  physically and both are available for leisure. Sleep, committed work, its work
  approach and critical needs take priority. Housing, provisions and companionship
  influence morale.
- Assign a resident to a household with an available bed. This changes where
  they go to rest, without teleporting them.
- Commission a home for a resident on an available house plot. The resident must
  procure real logs and planks and complete the shared construction action.
- Cold homes need real fuel. A household steward buys wood and burns it at home;
  heat improves warmth recovery. A living member within 1.5 metres of the door
  can supply one unreserved log for an unlit hearth in cold weather, heating it for
  half a simulation day. The player Pouch shows own-home heat and carried fuel.
  A building alone supplies shelter, not free heat.

The village has twelve professions and connected food, timber, mining, tools,
hunting, leather, thread and clothing chains. Money and goods move through physical
actions. NPCs protect emergency needs, plan funded work and recover at home.

**Space** pauses; **- / =** changes speed; **.** advances one tick while paused.
WASD or arrow keys pan in inspector/editor mode; **Q/E** orbit and the wheel zooms.
Commands queued while paused apply when you resume or step.

## Enter the island

Select **Play character** in the ledger, or enter through the title's New/Continue flow.
The reserved visitor begins a mortal life only on first arrival. Inspector mode
does not leave an unseen visitor starving before you join.

**WASD** moves your character relative to the camera; right-click also orders a
walk. Normal player mode has **Pouch, Craft, Build, Garden, Trade, People and Explore**
workspaces. **F2** opens Pouch or closes a workspace. Gathering, crafting, food,
water, rest, trade, cottage building and grain/berry gardening use real possessions
and the same action validation as NPCs. Direct developer scenario launches retain
the older **Home & farm** panel. Click the world or close a panel to return keyboard
focus to movement.

Returning to Inspector changes the view immediately. Committed gathering, crafting,
construction, hunting or rest keeps its authority and reservations until it settles;
native control then detaches if you are still inspecting. Returning to character
play before completion keeps the activity. **Stop** is the explicit cancellation
action. An existing character remains in the living world with normal needs.

Death remains permanent. **Begin a successor life** starts a successor instead of
resetting the village. **Keep watching the village** dismisses the death panel;
**Life choices · L** reopens it later. Native input will not take a character held
by another controller.

## Shape the landscape

Choose **Terraform**, then use the Landscape tab. Select grass, soil, stone or water,
a height in metres and a brush radius, and click the island. The outline previews the exact
cells and any rejection reason. The ledger reports queued, applied or rejected
commands.

The map uses one-metre cells. Height ranges from -1 to 3 metres; dry ground cannot
be below sea level, and water cannot be above it. Ground above 1.5 metres is
impassable. Editing protects residents, worksites, resources, home entrances and
plot sites, and rejects changes that disconnect passable land. Terrain changes
update navigation and persist in the save; there is currently no undo stack.

## Author the simulation

The shipped rules are in `content/testbed_content/worlds/island_colony.json`.
Open the developer toolbox with `cargo run --locked -p game_menu -- --developer-menu`.
Its World rules editor edits copied presets, including the **Resident AI**
tab for routine priorities, need thresholds and bounded GOAP settings. Saved worlds
keep their original rules. See [NPC AI authoring](island-ai-authoring.md) for exact
units, JSON and validation, and [Living Island economy](living-island-economy.md)
for the production model.

The developer toolbox's local entry buttons use the `island_colony` preset. Edit that preset before
**Start a fresh island** there. Other authored files can be selected explicitly with
`--island-rules`, or through the AI explorer's New-world selection.

The AI explorer workflow remains available for a separately saved agent world.
Choose the Island Village preset; a newly assigned visitor uses `join_island`
before ordinary gameplay. Starting an external AI provider is still explicit.

This iteration provides twelve established residents and housing commissions.
It does not add births, migration, a zoning system, new profession assignment or
an unrestricted object-placement editor. Resident AI authoring edits priorities,
thresholds and planner settings; tree topology remains Rust-authored, with no general
behavior-graph editor. The terrain editor has no undo stack, and an authored audio
soundscape remains separate work. Historical scenarios remain available
through developer CLI commands and tests, outside the showcase launcher.

The [simulation research brief](research/2026-11-09-simulation-research.md) records
why steady-state survival and bounded replenishment recovery do not establish
automatic recovery from a permanent specialist loss. The
[experience](research/2026-11-09-experience-research.md) and
[implementation](research/2026-11-09-implementation-research.md) briefs cover the
UI, discovery, lifecycle, performance and terrain choices. Their proposals are
distinct from the [actual validation record](_notes/2026-11-09-island-player-flow-validation.md).
