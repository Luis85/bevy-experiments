# World-state projection v1

An omniscient operator projection, separate from resumable WorldSnapshot files and agent
observations. The first producer supports `island_village` only. Unsupported scenes fail explicitly.

```json
{
  "schema_version": 1,
  "tick": 0,
  "world": {
    "id": "island_village", "name": "Island Village", "scene": "island_village",
    "units": "mm", "day": 0, "season": "spring", "night": false, "day_length_ticks": 24000
  },
  "terrain": {
    "origin": { "x": -1000, "z": -1000 }, "cell_size_mm": 1000,
    "width": 2, "height": 2, "cells": [0, 1, 1, 0], "paths": []
  },
  "entities": [
    { "id": 1, "kind": "building", "name": "Example workplace", "role": "farmer", "position": { "x": 0, "z": 0 } }
  ]
}
```

The example is deliberately small. Real island exports contain a 64 × 64 grid, nine places, four
resource patches and initially ten people. `src/state.ts` is the executable contract.

Coordinates are integer millimetres: x east/right and z south/down. Cells are row-major from terrain
origin: `0` water, `1` land. Optional paths are presentation rectangles `{x,z,width_mm,depth_mm}`
centered at x/z, matching the native view; they do not create navigation. The renderer maps 1000 mm
to 16 world pixels. Terrain is bounded to 4096 pixels per side, grids to 256 cells per side, entities
to 10000, and files to 16 MiB.

Terrain may also contain two additive, read-only fields (both default to empty arrays):

- `expansion_plots`: at most 32 `{id,label,x,z,width_mm,depth_mm,use_kind}` records.
  IDs are unique strings of 1–64 characters, labels are 1–128 characters, and
  `use_kind` is `housing` or `garden`. Integer x/z are rectangle **centers**, as with paths.
  Positive extents and the complete rectangle must fit within the supplied terrain grid.
- `districts`: at most 16 unique `{id,label,x,z}` caption points, using the same string
  bounds and integer millimetres. Points must lie within the supplied terrain grid.

The expanded Living profile initially supplies four 6 × 6 m house sites, two 6 × 6 m
gardens and three trade-district captions. These metadata IDs are separate from SimIds;
they do not create residents, ownership, navigation, claims or gameplay actions. The browser
renders received cleared pads and labels behind its public action feedback and remains read-only.
The producer chooses each saved world's geometry profile. Old compact worlds and exports
without these fields remain valid; a changed land footprint refits the presentation camera.
When actual `homestead_plot` entities are present, their names and state replace duplicate
terrain site captions. The metadata remains a background cleared pad.

Tick/day are nonnegative safe JSON integers. UI days display day + 1. Seasons: `spring`, `summer`,
`autumn`, `winter`. Unknown properties are discarded; unsupported versions/scenes are rejected.
Entity IDs are unique u32 SimIds, sorted by the producer. Arrays are full replacements: absence means
despawn; lower ticks are valid after reset/resume. Selection and camera persist across updates;
selection clears if its ID vanishes. A reset snaps movement and refits the camera. Follow resolves
the played actor on every frame.

Every entity has `id`, `kind`, `name`, `position`. Additional fields:

| Kind | Fields |
|---|---|
| `person` | `role`, `home: {x,z}`, `workplace: SimId`, `is_player`, `activity`, `health`, `coins`, `needs`, `inventory`, optional `bubbles` |
| `building` | `role` (the well uses `water_keeper`) |
| `resource` | `remaining`, `capacity` |
| `item` | `count` |
| `homestead_plot` | `plot_id`, `use_kind`, `owner`, `owner_name`, `cottage_built`, `cottage_center`, `front_door`, `beds` |
| `cottage` | `plot_id`, `owner`, `owner_name` |

`needs` maps keys to `{value,level}`. Health/needs are 0–100000 satisfaction (100000 full).
`inventory` contains `{id,kind,count}`; ground items omit carried items. Home is the simulation's
home/rest coordinate; workplace links the place SimId. Graphics depict the native places and their
associated housing; no separate authoritative dwelling entity is implied. Activities come from
the content's read-only inspector. Dead people remain visible until the sim removes them.
The UI sums carrying stacks by kind for readability, using exact integer presentation totals;
the original item IDs and transport records are retained unchanged.

Homestead and cottage `id` values are actual numeric SimIds. Their `plot_id` refers to a
stable terrain site ID; it is never used to manufacture a simulation entity. `owner` is a
nullable u32 actor ID and `owner_name` is a nullable public string. A cottage appears only
when the producer exports its real entity; a cleared pad or the viewer cannot create one.

Each bed is `{index,position,kind,stage,planted_at,watered_at,ready_at}`. Positions use the
same absolute integer millimetres as entities. Housing sites allow at most three beds
(indices 0–2), gardens at most six (0–5); indices must be unique within a plot. `kind` is
nullable; stages are `empty`, `needs_water`, `growing` and `ready`. Empty beds have no crop;
planted beds name their crop. The three time fields are nullable nonnegative safe ticks;
known water/ready times cannot precede their corresponding known planting/watering times.
The inspector displays only supplied timestamps, and remaining ticks use the received
simulation tick. It never infers a ready stage from browser time or invents material costs.

`bubbles` is an additive schema-1 field, defaulting to `[]` for older producers. Each person has at most
two records `{kind: "thought" | "speech", text, tick, expires_tick}`. Text is already public feedback
sanitized by Rust, bounded to 512 Unicode characters. Ticks are nonnegative safe integers and expiry
cannot precede the recorded tick. Thought records describe public actions, never hidden reasoning.
The viewer draws pixel pointers or thought dots using Canvas text; selected feedback also appears as
plain text in the inspector. It never interprets feedback as HTML or gameplay instructions.

Visibility uses only the latest received simulation tick (`tick <= state.tick < expires_tick`). A stale
feed freezes that historical view and keeps its stale warning; browser time never creates or renews
an event. Full replacements, omitted feedback, reset and despawn clear prior feedback. Crowded map
bubbles may be suppressed to protect labels and controls; the selected inspector retains both records.

No credentials, controller identities, sessions, notebook paths, private notebooks or diary pages are
serialized. Export runs between ticks without registering sim state or changing content hashes.
The view reveals positions/wallets across the island; use the existing perception-scoped API for agents.

Both `npcctl serve` and native `actor_testbed` accept `--world-state-out FILE` and
`--world-state-interval-ms N` (default 500), including explorer creation/resume. Native publication runs
initially and in `PostUpdate`, including paused frames. One native host can therefore supply both its
3D window and this browser projection; no second simulation host is needed. The browser's configured
file must be that native output path. These flags publish an operator projection, not a resumable save.

`GET /api/world-state` wraps the projection as `{state?,revision?,updated_at?,error?,source}`.
Revision is SHA-256 of file contents; updated_at is source modification time; source is its basename.
Invalid/missing updates retain state/revision and add error, displayed even when revision is unchanged.
Modification times distinguish a paused publisher (fresh) from a stopped publisher (stale). No request
selects a different file or sends gameplay commands.
