# Agent feedback increment (from Sorrel Saltmarsh's proposals): design

Date: 2026-10-25. Status: approved by the dev in chat ("proposal is sound"). Source: the eleven
improvement proposals an AI agent wrote while playing the explorer world `duskcombe`
(`C:\Projects\npc-agents\sorrel-saltmarsh\proposals\001`–`011`). Implemented as part of the
notes/workspace/fog/actors plan, split across Phases C and D.

## 1. Intent

Each proposal names a moment where the game told the agent too little, or something contradictory,
and the agent paid for it with a wasted walk or a wrong diary entry. The fixes are small and
local; together they make every refusal and result say what happened and what to do next.

## 2. Goals

**F1. Movement and vehicles** (proposals 001, 003, 010)
- A Move that fails as blocked reports how far it got and why: the nearest reachable point to the goal
  (`nearest_reachable: [x, z]`) and, when known, what blocked it (a wall, a closed or locked door
  `#ID`, outside the world). Shown in the order result, the `OrderFailed` event and the text line.
- Move labels and results speak metres like observations ("move to (0.00 m, -0.03 m)"). A
  fractional or otherwise invalid position is rejected with the unit and an example ("positions are
  integer millimetres: 6.5 m is 6500").
- Bug: `drive_to` refused "riding #30 but not in its driver seat" while the agent held the driver
  seat. Fix the check so a unit in a driver seat can drive. If a wagon is on its own behaviour
  (autopilot), the reason says so and how to take control. The reason always matches the state.

**F2. Items and crafting** (proposals 002, 005, 006, 008)
- A tool or mining yield that does not fit in the pack says where it went ("2 × log dropped at
  (24.08 m, 1.10 m): pack would carry 30950 of 30000 g"), in the result and the `Interacted` event.
  The offer line shows the load after the yield and "will drop" when it will not fit.
- A craft line names every missing input and tool and the inputs it will consume ("needs 1 plank
  (has 0), 1 ingot (has 2), tool hammer (has 1)").
- A refusal for a full pack names what to drop to make room (the lightest item that frees a slot),
  and the observation says once whether the item in hand counts toward the slots, so "9 of 8"
  makes sense (or the count is fixed if it is a bug).
- Craft lines show skill odds like tool uses ("[Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]"),
  and a skill appears on the sheet's Skills line once used, even at its default.

**F3. Sensing what is in reach, and seeing why something was dropped** (proposals 007, 009, 011)
- A short all-round **reach sense** (my call: 1.5 m, the interaction reach): things within reach
  (containers, workstations, doors, items, vehicles) are sensed whatever the facing and inside
  rooms, so their verbs are offered. It is part of perception (deterministic, integer), included in
  the human preset's senses; robots get it as a bump/proximity sensor (my call).
- Everything sensed enters memory and is listed under "You remember" with its last position,
  including vehicles and units (today they went missing). The one-minute rule (map/memory spec M1)
  still applies; longer recall is the map's job. When something leaves memory, the text can say why
  only if the reason is useful (my call: no per-thing "forgotten" lines; the memory span is in
  AGENTS.md).

**F4. Searched ground** (proposal 004)
- `observe` carries a short "searched" summary built from the fog's explored cells (plan Task 9): the
  share of cells explored within a radius of the actor and of the whole map ("12% of the map"; no start position is stored), and the compass sectors
  least explored (my call: 10 m cells, 8 sectors), e.g. "Searched: 34% within 60 m, 12% of the map; least searched:
  north-east, east." JSON (`map_pct`) gives the counts per sector.

Each change adds the test that would have caught the proposal's moment. After the increment the
proposals' status is `pursuing` (set by the dev's go-ahead); the dev moves them on.

## 3. Non-goals

- No change to the sim rules themselves (odds, weights, slot counts) beyond the `drive_to` bug and,
  if it is one, the "9 of 8" slot count.
- No path planning beyond reporting the nearest reachable point.

## 4. Anti-goals

- A new message changes a replay, golden or state hash (messages, labels and observation text are
  not hashed; the reach sense must stay off for scenarios that do not opt in, like fog, unless the
  re-baseline procedure is followed).

## 5. Calls I made

- Reach sense radius 1.5 m, on for explorer actors (human preset) and as a robot proximity sensor.
- No per-thing "forgotten" messages; the one-minute memory span stays.
- Searched summary: 10 m cells, 8 compass sectors, radius 60 m and the share of the whole map (relabelled "of the map" in ruling R16: no start position is stored).
- Split: F1 and F2 land in Phase C (cheap and felt now); F3 opens Phase D (fog follows perception);
  F4 follows the fog's explored cells in Phase D.
