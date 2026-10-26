# Task 7 report: `fl_sim` core — commands, spawning, unit components, movement, scenarios, scale scenario

**Status:** DONE_WITH_CONCERNS (the concerns are small deviations, listed below)
**Commit:** `482d66e feat(fl_sim): commands, spawning, core unit components, movement, scenarios, scale scenario`

## What was implemented

Everything lives under `crates/fl_sim`.

- **units.rs**
  - Def components, all with `deny_unknown_fields`:
    - `SelectableDef` (`"Selectable": ()`) inserts `Selectable`.
    - `HullDef { hp, footprint, hitbox_shrink }` inserts `Health { hp, max: hp }` and `Footprint { width: footprint.x, length: footprint.y, hitbox_shrink }`. It validates `hp ≥ 1`, `0 < side ≤ 50` and `0 ≤ shrink ≤ 1`.
    - `MobilityDef { class, road_speed_kmh, terrain_factor, turn_rate_deg_s }` inserts `Mobility` plus an empty `MoveOrder`. It validates `0 < kmh ≤ 200`, `0 ≤ factor ≤ 1` and `0 < turn ≤ 720`.
  - Conversions: speed is `kmh / Fx::from_int(72)`, where 72 is derived from `SIM_TICK_HZ` as `36·20/10`. Turn rate is `Angle::from_degrees(deg_s / 20)`.
  - Runtime components: `Owner`, `UnitType`, `Pose`, `Health`, `Footprint`, `Mobility`, `MoveOrder` (Default), `Wander` and `Selectable`.
    - All of them derive `Serialize`.
    - The six hashed ones implement `SimHash`.
- **spawn.rs**
  - `spawn_unit` checks, in order: unit type (`UnknownUnit`), map (`NoMap`), player (`UnknownPlayer`), then `map.contains(pos)` (`OutsideMap`).
    - It then allocates a SimId and spawns `SimEntity`, `Owner`, `UnitType` and `Pose`.
    - Every def component instantiates in component-name order.
  - `despawn_unit` goes through `SimIndex`.
- **commands.rs**
  - `CommandPayload`, `CommandCtx`, `RejectReason` (thiserror) and `CommandRegistry`.
    - The registry is sorted by `NAME`. A duplicate name or type panics.
    - `seal()` is called in `build_sim_app`. `type_id` and `encode` panic before the seal; registering after it panics.
  - `CommandQueue` is a `BTreeMap<Tick, Vec<PlayerCommand>>` and keeps push order within a tick. `push` returns `Late` if `cmd.tick < now`.
  - `CommandLog`, and `submit`, which also logs Late rejections.
  - `apply_commands` is exclusive. It takes the commands due at or before the clock tick, by tick and then push order; anything older is rejected as Late, as a defence.
    - Checks run in this order: `UnknownType`, `DebugDisabled`, `UnknownPlayer`, then `BadPayload` (postcard `take_from_bytes`; trailing bytes are rejected too).
    - Units are then sorted and deduplicated, and filtered to those that exist, are owned by `cmd.player` and pass `unit_can`. Then comes `NoValidUnits` (when `NEEDS_UNITS`), then `apply`.
  - Core payloads:
    - `MoveCmd` ("core.move"): `unit_can` is "has Mobility". The target is clamped to the map. With `queue` it appends; otherwise it replaces.
    - `StopCmd` ("core.stop"): clears the waypoints.
    - `DebugSpawnCmd` ("debug.spawn"): `NEEDS_UNITS = false`, `DEBUG_ONLY`; calls `spawn_unit` for `ctx.player`.
    - `DebugWanderCmd` ("debug.wander"): `DEBUG_ONLY`. A radius ≤ 0 removes `Wander`; otherwise it inserts `Wander { home: pos, radius }`, with the radius capped at the map's longer side.
- **movement.rs**
  - `MovementSystems` set.
  - `wander_system` (Behaviour, SimIndex order): for `Wander` with an empty `MoveOrder`, pushes `map.clamp(home + (range_fx(-r,r), range_fx(-r,r)))`, drawing x then y.
  - `move_system` (in `MovementSystems`, inside `SimSet::Movement`, SimIndex order): `rotate_towards(desired, turn_rate)`.
    - The unit advances only when `|signed_diff| ≤ 8192`.
    - If `dist ≤ speed` it snaps to the target and pops it; otherwise it moves `normalize_or_zero * speed`. Then `pos = map.clamp(pos)`.
    - A zero-length leg pops at once.
- **scenario.rs**
  - `SetupError` and `setup_map`.
  - `setup_scenario` delegates to `setup_scenario_def`, which works in two phases:
    - It first resolves the map and all player factions, so nothing changes on error.
    - It then inserts `MapInfo` and `Players` (slot i is `PlayerId(i)`, faction is the kind-table index), reseeds `SimRng`, and spawns units in file order.
- **scale.rs**
  - `ScaleScenario` (Default 3000/4/2000/150/1) and `populate`, exactly as the brief describes. Units spawn with heading 0; player i has controller `None`; `k % players` picks the player and `k % types` the unit type.
- **Wiring.**
  - `CoreFeature` registers the 3 defs, 6 hashed components, 4 commands, `CommandQueue`/`CommandLog` and the 3 systems.
  - `SimAppExt::register_command` added.
  - `build_sim_app` seals the registry.
  - `lib.rs` exports everything.
- **R15.**
  - `compute_state_hash` writes `SimIdAllocator::issued()` (new public getter: the last id issued) after the entity count.
  - New tests in `tests/hash.rs`:
    - `hash_independent_of_spawn_order`: a filler entity is spawned and despawned, a component-carrying filler despawned, and units spawned in reverse order. The test asserts that the `Entity` values differ while the hashes are equal.
    - `sim_id_allocator_changes_hash`.
- **Fixtures.**
  - `core_mod/units/{tank,abstract_vehicle}.unit.ron`, `factions/{red,blue}`, `maps/flat` (1000², cell 4) and `scenarios/duel`.
    - duel: seed 42; red is Human, blue is `Ai("default")`. Tanks at (100,100) heading 0 and (900,900) heading 180.
    - The tank's Hull includes `hitbox_shrink: 0.5`, because the field is required.
  - `absurd_speed/`, a copy of core_mod with `road_speed_kmh: 1e12`.

## Tests

- `tests/commands.rs` (11):
  - from the brief:
    - move_command_reaches_target: not there at 199 ticks, exactly there at 200
    - queued_moves_append: also checks that an unqueued move replaces
    - foreign_units_rejected
    - late_command_rejected
    - debug_spawn_requires_flag
    - command_type_ids_sorted_by_name
    - bad_commands_do_not_change_state: 4 rejections, in order Late, UnknownType, BadPayload, NoValidUnits; hashes equal
  - extra:
    - stop_clears_the_move_order
    - duplicate_and_missing_units_are_filtered
    - unknown_player_rejected
    - debug_wander_inserts_and_removes_wander
- `tests/movement.rs` (12):
  - from the brief:
    - kmh_converts_to_m_per_tick
    - tank_turns_before_moving: 10 ticks in place with the heading changing each tick, then arrives facing 180°
    - move_target_outside_map_is_clamped
    - spawn_outside_map_is_error: includes x == width, negative values, the exact last representable edge, UnknownPlayer and NoMap
    - spawn_unknown_unit_is_error
    - mobility_rejects_absurd_speed
    - wander_stays_within_radius_and_map: tolerance radius²·2 + 0.001; also asserts that more than 150 of 200 units moved
    - scale_populate_is_deterministic: also checks that a different seed differs, the 2000 m map, and player → quadrant
    - setup_scenario_spawns_units
  - extra:
    - despawn_unit_removes_it
    - mobility_rejects_out_of_range_values: copies core_mod to `CARGO_TARGET_TMPDIR` with 500 km/h, factor 1.5 and turn 0, and asserts the three validate error paths
    - setup_unknown_ids_are_errors
- Unit tests:
  - commands: queue tick/push order and Late; decode rejects garbage and trailing bytes
  - units: the km/h divisor is 72
- `tests/hash.rs` +2 (R15). `tests/schedule.rs` changed by one line (see deviation 1).

### TDD evidence

- **RED, part 1.** The new test files and fixtures were run against the pre-change sources in a throwaway detached worktree (`C:/fl/red7`, since removed) with `cargo test -p fl_sim`. Expected failure, because the API did not exist:
  - `error[E0432]: unresolved imports fl_sim::CommandLog, fl_sim::CommandPayload, … fl_sim::setup_scenario, fl_sim::submit`
  - `error[E0425]: cannot find function despawn_unit`
  - `error[E0433]: cannot find SetupError`
  - `could not compile fl_sim (test "commands") / (test "movement")`
- **RED, part 2 (R15).** Only the new `tests/hash.rs` against the old sources, with `cargo test -p fl_sim --test hash`:
  - `sim_id_allocator_changes_hash ... FAILED`, `assertion left != right failed`, both hashes `11194255651787437781`
  - `10 passed; 1 failed`
- **GREEN.** `cargo test -p fl_sim`:
  - unit tests: 14 passed
  - commands: 11 passed
  - hash: 11 passed
  - movement: 12 passed
  - schedule: 6 passed
- One failure on the way: `spawn_outside_map_is_error` returned `UnknownPlayer` where `NoMap` was expected on a bare sim. I reordered the checks so the map is checked before the player.

### Gate (from C:\fl\a)
- `cargo fmt --check`: OK
- `cargo clippy --workspace --all-targets -- -D warnings`: no warnings or errors
- `cargo test --workspace`: all pass
  - fl_content 1+24+23
  - fl_math 46+4
  - fl_protocol 4
  - fl_sim 14 (unit) + 11 (commands) + 11 (hash) + 12 (movement) + 6 (schedule)
  - every other crate has 0 tests
- `bash scripts/check_deps.sh`: `deps OK`

## Deviations and decisions (please review)

1. **New public set `CommandSystems`** (in `SimSet::ApplyCommands`, after `ClearSimEvents`) holds `apply_commands`.
   - Because `apply_commands` is exclusive, the Task 6 `sets_run_in_declared_order` trace system in ApplyCommands became ambiguous, and the schedule failed to build.
   - Following the `SimTickEnd` and `MovementSystems` pattern, any other system in `SimSet::ApplyCommands` must order against `CommandSystems`. I changed the trace test by one line (`.after(CommandSystems)`).
2. **`SetupError::TooManyPlayers(usize)`** is a variant the brief does not list. A scenario with more than 256 player slots cannot map onto a `u8` `PlayerId`, and this avoids a panic or a wrap on hostile content.
3. **`DebugWanderCmd::unit_can`** requires `Mobility` and `Pose`. The brief specifies `unit_can` only for `MoveCmd`. A radius ≤ 0 (not just 0) removes `Wander`, and the radius is capped at the map's longer side, so a hostile payload cannot make `range_fx` panic or overflow `home + offset`. `wander_system` also guards with `radius.max(0)`.
4. **`MoveCmd` clamps the target to the map when it applies.** Otherwise a unit with an off-map target would push against the edge forever and never pop the waypoint. `move_system` still clamps the position, as the brief says.
5. **`MobilityDef` inserts an empty `MoveOrder`**, so every mobile unit hashes `MoveOrder` from spawn.
6. **The command registry is sealed in `build_sim_app`.** Ids are kept sorted at every insert; the seal freezes them, and `type_id` and `encode` panic before it. The registry is the `CommandRegistry` resource, so it stays in the sim world for clients to encode with.
7. **R15 hashes `issued()` (the last id handed out)**, not "next id" (that value + 1). The information is the same.
8. **Line endings.** `build.rs` is CRLF in the index (from Task 6); I kept it CRLF so the diff stays small. All other files are LF.

## Concerns

- **`mobility_rejects_absurd_speed` fails at parse time, not in the range validator.** `1e12` is outside `Fx`'s ±2³¹ range, so `Fx` deserialization rejects it: a `ContentError` with file and line, no panic. That satisfies Review Focus #1. The `(0, 200]` range check itself is covered by the extra `mobility_rejects_out_of_range_values` test, which writes a mod copy under `CARGO_TARGET_TMPDIR`.
- **`CommandLog` grows without bound** (`applied` keeps every command). This is fine for M0 tests; a long match or bench will want a cap or a drain. Task 18's journal is the natural owner.
- **`ScaleScenario::populate` panics if `players == 0` while `units > 0`** (documented), and `MapInfo::flat` panics if `map_m ≤ 0`. Both are developer-supplied bench parameters, not content. If `fl_cli bench` exposes them as CLI flags, it should validate them.
- The fixture tank has no `VisualDef`, so loading emits one `tracing::warn!` (there is no subscriber in tests, so nothing is printed).
