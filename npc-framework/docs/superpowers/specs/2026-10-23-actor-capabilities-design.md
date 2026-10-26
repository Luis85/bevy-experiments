# Actor capabilities: perception, interaction, RTS command, vehicles, character rules, progression, crafting (design)

Date: 2026-10-23 · Status: **approved by user** (with improved pathfinding: §4.1 `npc_nav`)
Builds on the baseline-hardening design (crate split, `NpcAppExt` stable-name registration, locomotion seam).

## 1. Intent

Actors become capable game agents in a way that is reusable across colony sims and RTS games. They can:
- perceive their environment through sensors, including vision;
- interact with it: pick up items, open doors, use tools, mount vehicles;
- be commanded by players with RTS click orders, alone or in groups;
- grow through a GURPS-like character sheet with a 3d6 success system, XP and levels, crafting, and learning.

**Every capability is an opt-in plugin.** It is prepared and documented, but off unless the host adds it. One **Actor Testbed** scene enables all of them, as both a showcase and the integration test.

## 2. User requirements (verbatim intent)

- **Perception:** actors perceive their environment, with vision or other sensors.
- **Interaction:** actors interact with the environment, e.g. pick up items, open doors, use tools.
- **Opt-in:** all of these features are opt-in but prepared.
- **Testbed:** one Actor Testscene with every feature and system enabled.
- **RTS command:** RTS systems command single units or groups of units, using **RTS click orders only**. There is no WASD possession.
- **Vehicles:** actors can mount vehicles. A vehicle is commanded either *through its actor*, so orders go to the driver, or *directly* as a unit, depending on setup.
- **Progression:** actors gain XP and progress through levels.
- **Rules:** a GURPS-like character sheet, success system, crafting and learning.

## 3. Non-goals and anti-goals

- **Non-goals:** combat and damage resolution. The rules layer prepares HP/FP and contests, but combat is a later iteration. Also out: dialogue, economy and trade, multiplayer transport, pathfinding beyond the locomotion seam, and RON content loading (registration stays code-first and is name-stable, so RON can follow).
- **Anti-goal: an always-on capability.** Every capability must work only when its plugin is added. With every new plugin removed, the village demo and the existing tests must behave exactly as before.
- **Anti-goal: non-deterministic rules.** Any float, wall clock or order-dependent randomness in perception or rules is a defect. Dice use counter-based RNG keyed by `(seed, sim_id, tick, purpose, roll_index)`.
- **Anti-goal: copying GURPS text or tables.** We implement generic mechanics inspired by GURPS (point-buy, 3d6 roll-under, relative skill levels) under our own names. GURPS is a Steve Jackson Games trademark, and no rulebook text or tables are reproduced.

## 4. Design

### 4.1 Shared foundations (go into the kernel and `npc_jobs`, wave C)

- **Spatial grid:** an integer-cell grid (default 1 m cells) indexing sim positions, with per-cell flags for occluding and blocking. It supports radius and box queries in `SimId` order. Perception, interaction targeting, formations and the testbed all use it.
- **Locomotion seam:** `MoveIntent`/`MoveStatus` and the straight-line mover, as already planned. Closed doors mark cells as blocking; until real pathfinding exists the mover only avoids them (see §5).
- **Pathfinding (`npc_nav`, opt-in, approved for wave C):**
  - **Deterministic grid A\*:** integer costs (orthogonal 10, diagonal 14) and no corner cutting through blocking cells. Ties are broken by `(f, h, cell index)`.
  - **Path smoothing:** integer line-of-sight string-pulling over walkable cells.
  - **Request budget:** a `PathRequest` queue, served in SimId order with a per-tick node budget. Requests not served this tick carry over to the next.
  - **Doors:** closed doors are passable at extra cost when the actor may open them, which inserts an `open` interaction; otherwise they block. Locked doors block unless the actor has the key.
  - **Replanning:** when a cell on the current path becomes blocked (door closed, item dropped), or `MoveStatus::Blocked` persists.
  - **Group moves:** one path for the group leader, plus formation-slot offsets re-validated along it. Flow fields for large crowds remain P2.
  - **Locomotion seam:** the default `GridPathMover` consumes `MoveIntent`, follows the waypoints, and reports `MoveStatus`. Hosts can still replace it.
  - **Debug:** journal `PathFound`/`PathFailed{reason}` entries with node counts, path gizmos, and a `nav.astar` perf mechanic with a budget.
- **Relationships:** Bevy relationships for `CarriedBy`/`Inventory`, `MountedIn`/`Occupants` and `MemberOf`/`Members`, kept deterministic by ordering on SimId.

### 4.2 `npc_perception` (opt-in)

- **Sensors:** a `Sensors` component holding a small list of sensor definitions.
  - **Vision:** range in mm; field of view as an integer half-angle checked with integer dot products and squared comparisons (no trigonometry at runtime); line of sight via an integer DDA over occluding cells.
  - **Hearing:** a radius, triggered by `Noise` stimuli events.
  - **Proximity:** a radius.
- **Stimuli:** `Perceivable { kinds }` on things that can be seen. `Noise` events come from actions such as chopping or doors.
- **Detection:** optionally rolls against the actor's Perception (Per) from the rules plugin (§4.6), for example when detecting a hidden target. Without the rules plugin, detection is automatic.
- **Memory:** `PerceptionMemory`, a bounded `BTreeMap<SimId, Percept { last_seen_tick, pos, via, confidence }>` that forgets after a configurable number of ticks.
- **Integration:** blackboard facts and conditions (`sees(kind)`, `heard_noise`, `remembers(kind)`), plus a `Sense` set that updates on a stagger (`(sim_id + tick) % N`).
- **Debug:** journal `Perceived` and `Lost` entries, perception memory in inspect, and gizmo vision cones and LOS rays in the view helpers.

### 4.3 `npc_interaction` (opt-in)

- **Items:** an `Item { kind, stack, quality }` entity. It lives in the world (with `SimPos`), in an `Inventory`, or in an equipment slot. `Inventory` has capacity in slots and weight.
- **Verbs:** registered by name as `InteractionDef`. Each has:
  - preconditions (item or tool required, reachable, target state);
  - duration in ticks;
  - an optional skill check (§4.6);
  - effects;
  - a reservation.
- **Built-in verbs:** `pick_up`, `drop`, `give`, `open`, `close`, `lock`, `unlock`, `equip`, `unequip`, `use_tool_on` (e.g. chop with an axe), `mount`, `dismount`.
- **Targets:** `Interactable { verbs }` on targets.
- **Doors:** `Door { open, locked_by: Option<ItemKind> }`. Opening or closing a door toggles its grid cell's occluding and blocking flags.
- **One action for every verb:** a generic `Interact { verb, target }` leaf. The same path is used by the BT, `SimCommand::Interact` (orders) and the testbed, so everything is journaled and replayable.

### 4.4 `npc_command`: RTS orders (opt-in)

- **Ownership:** `Owner(PlayerId)` on units. Every order is a `SimCommand` carrying the `PlayerId` and is validated against ownership. This keeps orders deterministic and ready for multiplayer.
- **Orders:**
  - `Move`, `AttackMove` (until combat exists it is a move with a different journal tag), `Stop`, `Hold`, `Follow { target }`, `Patrol { points }`;
  - `Interact { verb, target }`, `Mount { vehicle }`, `Unload`.
  - A per-unit `OrderQueue` supports shift-queueing. Orders feed the existing `FollowOrder` tree branch, generalized.
- **Groups:**
  - **Group orders:** one order to N units gets formation slots computed in integer space (line, box, wedge), spread around the target.
  - **Squads:** a `Squad` entity with `MemberOf`. A squad can carry its own `Brain` for squad-level behaviour later.
- **Host-side helpers** (view helpers; never sim state):
  - selection (click, box, shift-add);
  - control groups (Ctrl+1–9);
  - a right-click context order (ground → Move, item → `pick_up`, door → `open`, vehicle → `Mount`);
  - order and waypoint gizmos.
  - These turn into `SimCommand`s, so a host can swap in its own UI.

### 4.5 `npc_vehicles` (opt-in)

- **Vehicle definition:** `Vehicle { seats: [Seat { role: Driver | Passenger | Gunner, required: bool }], speed, control: VehicleControl }`. A vehicle moves through the locomotion seam.
- **Mounting:** `MountedIn { vehicle, seat }` is a relationship. While mounted, the actor follows the vehicle, its own locomotion is suspended, and its BT runs a `Mounted` branch.
- **Control modes:**
  - **`ByCrew`:** RTS orders to the vehicle are forwarded to the driver's order queue, and the driver's BT drives it through `MoveIntent` on the vehicle. Without a driver, the vehicle cannot move.
  - **`Direct`:** the vehicle is a unit with its own `OrderQueue` (and optionally its own `Brain`) and is commanded like any unit. The crew is cosmetic or used for capacity (e.g. RTS tanks).
- **Commands:** `Mount`, `Dismount`, `Unload` (all occupants), and orders addressed to the vehicle.

### 4.6 `npc_rules`: character sheet and success system (opt-in)

- **`CharacterSheet` component:**
  - **Attributes:** ST, DX, IQ, HT.
  - **Secondary characteristics:** HP, Will, Per, FP, Basic Speed (fixed-point quarters), Basic Move.
  - **Traits:** advantages and disadvantages, with point values and modifiers.
  - **Skills:** each records points spent, its derived relative level and effective level.
- **Point-buy:** attributes and skills cost character points.
- **Skill definitions** are registered by name with:
  - a controlling attribute;
  - a difficulty (Easy, Average, Hard, Very Hard);
  - defaults (e.g. attribute −5, or another skill −3);
  - a cost table, which is our own, configurable, with GURPS-like defaults.
- **Success roll:** 3d6 roll-under against effective skill plus modifiers. The outcome is `{ roll, target, margin, critical: Success | Failure | None }`, where the critical thresholds are a configurable rule.
- **Quick contests** compare the two margins. **Resisted rolls** are prepared for later use.
- **Determinism and debugging:**
  - Dice use counter-based RNG.
  - Every roll is journaled as `Roll { skill, target, roll, margin, crit, purpose }`.
  - That entry answers "why did the craft fail" directly.
- **Hooks into other systems:**
  - `InteractionDef.check`
  - perception detection (Per)
  - crafting
  - learning
- **Mechanics stay pluggable:** a `RuleSet` resource exposes cost tables, crit rules and the default chain. That leaves room for an alternative rule set (e.g. d20) without touching callers.

### 4.7 `npc_progression`: XP, levels and learning (opt-in, needs `npc_rules`)

- **XP:** `Experience { xp, level }`. XP comes from registered sources: successful checks (scaled by difficulty), completed crafts and jobs, and discoveries.
- **Levels:** a configurable curve. Each level grants character points.
- **Spending points** goes through a `SpendPolicy`:
  - **AI policy:** weighted toward the skills the unit uses, so a villager who chops a lot gets better at Axe.
  - **Player policy:** points are banked and spent by a player command.
- **Learning by practice:** using a skill accrues practice ticks, which convert to points in that skill at a configurable rate.
- **Study:** an interaction with a book or trainer item grants study points.
- **Teaching:** an actor-to-actor interaction gated by the teacher's skill.
- **Caps:** configurable, e.g. practice alone cannot raise a skill above its attribute + N.
- **Recording:** XP, level and skill changes are journaled and appear in inspect and the BRP `npc/sheet` method.

### 4.8 `npc_crafting` (opt-in, needs `npc_interaction` and `npc_rules`)

- **Recipes** are registered by name with:
  - inputs (item kinds and counts);
  - required tools (items) and a required workstation (a smart object with `Workstation { kinds }`);
  - a skill, a difficulty modifier and a duration in ticks;
  - outputs.
- **Results:**
  - **Quality** comes from the margin of success.
  - **Critical failure** consumes the inputs and may damage the tool.
  - **Critical success** gives a quality bonus.
- **Running a craft:**
  - **Generic action:** a `Craft { recipe }` action reserves the workstation and inputs.
  - **Job:** colony sims can post a crafting job (smart-object advertisement or job board).
  - **RTS:** use the `Interact`/`Craft` order.
- **Feedback:** grants XP and practice, and is journaled.

### 4.9 Actor Testbed (`apps/actor_testbed` + `content/testbed_content`)

The testbed is a compact walled compound with every plugin on:
- **Units:** about 8 actors in 2 factions/owners, each with sensors and character sheets of different skills.
- **World:**
  - doors (one locked; the key is an item);
  - items (axe, logs, ore, food, a book);
  - trees to chop;
  - a workbench and an anvil for crafting;
  - a cart, `ByCrew`, and a "tank-like" wagon, `Direct`.
- **Input:** RTS selection, control groups, right-click context orders and shift-queues.
- **Panels:**
  - F5 character sheet (selected unit);
  - F6 perception (vision cones, memory);
  - F3/F4 journal and perf, as already built.
- **Scripted scenario tests (headless), one or more per feature:**
  - see through an open door but not a closed one;
  - pick up the key and unlock the door;
  - chop with an axe vs. no axe;
  - craft a plank at the workbench, with quality varying with skill;
  - gain XP, level up and spend points;
  - mount the cart and move it via the driver;
  - command the `Direct` wagon;
  - a group order produces a formation.
- **Every scenario also becomes a replay in the corpus.**
- **Screenshot checks** for the panels and vision cones.

## 5. Sequencing (proposed)

1. **Wave B (running):** crate split.
2. **Wave C:** locomotion seam, spatial grid with occluding/blocking flags, `npc_nav` grid A\* with smoothing, budgets, replanning and door costs, plus the follow-up fixes and the zstd switch.
3. **Wave D:** two parallel streams:
   - D1 perception;
   - D2 rules (sheet and success).
4. **Wave E:** interaction, with items, doors and tools. It needs the grid and rules.
5. **Wave F:** three parallel streams:
   - F1 command (orders, groups, formations, selection helpers);
   - F2 vehicles;
   - F3 progression and learning, plus crafting.
6. **Wave G:** testbed scene and assets, scenario replays, screenshot checks. The RTS units demo is rebuilt on `npc_command`.
7. **Wave H:** CI, `docs/debugging.md`, a "using npc-framework" guide that covers every plugin, and the fresh-agent extension test.

## 6. Calls made by the designer (flag if wrong)

- **XP, levels and points together.** XP and levels sit on top of GURPS-style character points: levels grant points, and practice and study also grant points. An AI spend policy is the default, and a player policy is available.
- **Detection:** automatic without `npc_rules`, and Per-based when `npc_rules` is present.
- **Combat:** not in scope. `AttackMove` is a tagged move until combat exists.
- **Assets:** the testbed uses free CC0 assets (KayKit, plus Kenney for vehicles if KayKit has none). An asset research pass comes before wave G.
- **Pathfinding:** approved by the user and included in wave C as `npc_nav` (§4.1).
