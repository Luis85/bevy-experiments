---
status: "accepted"
date: 2026-10-24
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N20/C9/C14; workflow-and-verification.md §6.3; baseline-recommendations.md A9"
---

# Lockstep command envelope, postcard, `MatchHeader` and the canonical rules hash

## Context and Problem Statement

Revision 1's `PlayerCommand { tick, player, units, kind, queue }` (§5.5) has no per-player sequence and no notion of
input delay; M0 code applies commands in push order. Lockstep needs input delay (commands sampled for a future tick),
waiting for the slowest peer, and a deterministic intra-tick order, and single player should use the same path so that
bugs show up there too. Prior research proposed bincode, which is discontinued (3.0.0 is only a notice). The rules hash
covered content only, so a peer with different code or features would desync late; and "canonical" wasn't defined. M0
code hashes RON text in which `Fx` is printed as `f64`, which loses low bits above 2^21 and depends on float formatting.

## Decision Drivers

* Single player and multiplayer share one command path.
* Deterministic intra-tick order, independent of arrival order.
* A peer, replay or save with different code, features, mods or content is detected up front, not by a late desync.
* A stable, documented, lossless binary format.
* One identity record for handshake, replay and save.

## Considered Options

* Keep the revision 1 command; JSON or RON replays; RON-text rules hash
* A lockstep envelope (`exec_tick`, `seq`), postcard, one `MatchHeader`, a canonical binary rules hash
* An envelope with bincode 1.x pinned

## Decision Outcome

Chosen option: "A lockstep envelope, postcard, one `MatchHeader`, and a canonical binary rules hash".

- **Envelope:** `PlayerCommand { exec_tick: Tick, player: PlayerId, seq: u32, units: Vec<SimId>, kind: CommandKind,
  queue: bool }`, where `CommandKind = (CommandTypeId, postcard payload)`.
  - Within a tick, apply in `(player, seq)` order; a duplicate `(player, seq)` is rejected.
  - `seq` is per player and strictly increasing over the match.
  - Input delay is a driver parameter: 1 in single player, adaptive 2–6 in multiplayer later, 0 in `TestDriver`.
  - A command with `exec_tick` earlier than the next tick to run is rejected as late.
  - Handlers tolerate dead or foreign `SimId`s: filter, then log.
- **Payload data types** implement `fl_protocol::CommandPayload` (a `NAME` and serde data). Core payloads live in
  `fl_protocol`, feature payloads in the feature sim crate. Handlers are registered in the sim. Clients encode by name.
- **Wire, replay and save format: `postcard` 1.x.** bincode is banned in `deny.toml`. JSON exists only as a debug view
  (`fl_cli replay dump --json`).
- **`MatchHeader`** in `fl_protocol`: `format_version`, `build_id`, `sim_version`, `features[(id, version)]`,
  `mods[(id, version)]`, `rules_hash`, `cheats_allowed`, `seed`, `string_tables`. Replays, saves and the future
  handshake all use it.
  - Handshake (later): everything equal, including `build_id`.
  - Replay verification: features, mods, `sim_version` and `rules_hash` equal; a `build_id` mismatch only warns, so
    golden replays keep verifying across commits.
  - Save loading: features and mods equal, or migratable.
  - `build_id` = crate version + git revision (+ dirty flag), provided at build time; `"dev"` when unknown.
  - `sim_version` = `fl_sim::SIM_VERSION`; its bump policy arrives with C8 (M2).
- **Canonical rules hash:** xxh64 (seed 0) over a format-version prefix plus postcard bytes of a canonical tree: mods in
  load order `(id, version)`, each content table sorted by id, components within an object sorted by name, maps sorted.
  postcard is a non-human-readable format, so `Fx` serializes as its raw `i64` (lossless). Presentation `f32` fields
  serialize as IEEE bits. Never `Debug` or RON text, and no `#[serde(flatten)]` in hashed content.

### Consequences

* Good, because single player exercises the exact multiplayer path, and input delay is invisible at RTS cadence.
* Good, because mismatched builds, features, mods or content are caught before tick 0.
* Good, because the rules hash is lossless and independent of formatting.
* Bad, because replays are binary; agents use `fl_cli replay dump --json`.
* Bad, because every command now needs a `seq`; drivers assign it, so test helpers must go through a driver.

### Confirmation

* Tests: `(player, seq)` ordering independent of submit order; duplicate-seq and late rejection; a postcard round trip
  of `PlayerCommand` and `MatchHeader`; rules-hash sensitivity to a one-ULP `Fx` change at a large magnitude;
  insensitivity to field order and whitespace in the source files.
* `serde-reflection` snapshots of `PlayerCommand`, the core payloads and `MatchHeader` (ADR-0011).
* `cargo deny` bans bincode.

## Pros and Cons of the Options

### Keep the revision 1 command, JSON replays, RON-text hash

* Good, because there is no rework.
* Bad, because there is no lockstep ordering, the hash is lossy and formatting-dependent, and code identity is missing.

### Envelope, postcard, header, canonical hash

* Good, because it is lockstep-ready, compact, lossless and documented.
* Bad, because of a binary format and some rework of M0 code.

### bincode 1.x pinned

* Good, because it is familiar.
* Bad, because it is discontinued upstream and has no future fixes.

## More Information

* Spec §5.4 (rules hash), §5.5 (commands, `MatchHeader`, replays).
* Research: `architecture-best-practices.md` N20, C9, C14; SnapNet "Netcode Architectures Part 1: Lockstep";
  `workflow-and-verification.md` §6.3.
* Related: ADR-0002 (drivers), ADR-0004 (string tables), ADR-0008 (`cheats_allowed`).

## Refinements (2026-10-24)

From the final M0 review (`final-review-m0.md`); controller rulings R70, R71, R73.

* **R70: header rules per use.** Handshake: every field equal, `build_id` included. Replay verification: features
  (ids and versions), mods, `sim_version` and `rules_hash` equal; a `build_id` mismatch warns. Save loading: the same
  feature ids, each saved at a version no newer than the build's, and equal mods; the codecs and their migrations
  decide the rest (ADR-0006).
* **`MatchHeader.features` is in load order:** the topological order of the features' dependencies, core first and
  ties by id, not sorted by id. Peers build it the same way and compare it as is.
* **R71: stable reject codes.** `RejectReason` is `#[non_exhaustive]`; a feature rejects with
  `RejectReason::Feature("<feature>.<reason>")` and never adds a variant. `RejectReason::code()` gives every reason a
  stable code (`"core.late"`, …; feature codes verbatim), which `SimEvent::CommandRejected` and the
  `core.command_rejected` journal record carry instead of a `Debug` name.
* **R73 (I1): command type ids are remapped by name.** Command type ids are positions in name order, so a build that
  adds a command name sorting earlier moves them. Loading a save maps its pending commands, and `replay_app` /
  `replay_inputs` map a replay's recorded commands, through the recorded `commands` string table; a name the build
  lacks is an error. An id the recorded table never had stays unknown and is rejected as `UnknownType`.
* **Input caps.** A command names at most `MAX_COMMAND_UNITS` (256) units (`core.too_many_units`), and a queued move
  onto a `MoveOrder` that holds `MAX_WAYPOINTS` (32) is rejected (`core.queue_full`): commands are hashed input a peer
  could otherwise grow at will.
* **`build_id` is real.** `fl_sim`'s build script sets it to the crate version plus `git rev-parse --short HEAD`, with
  `-dirty` when tracked files differed from `HEAD` when the script last ran, which is when `HEAD` or its branch last
  moved; edits made after that build as the clean revision, so the handshake's build-id check tells commits apart,
  not uncommitted edits. `"dev"` only without git.

## Refinements (2026-10-24, M1)

Status: accepted at the M1 Task 1 review (rulings R69, R84), with ADR-0014–0016. Controller rulings R71, R73,
R77, R79.

* **New payloads.** Core payloads `core.rotate { toward: FxVec2 }` and `core.follow { leader: SimId }`, and the debug
  payload `debug.set_terrain { min_cell: (u32, u32), max_cell: (u32, u32), terrain: String }` (`DEBUG_ONLY`, no
  units; `terrain` is a content name, never a dense id). Adding a payload type needs no `FORMAT_VERSION` bump; the
  format guard allows added types. Their dense command ids may shift existing ones (`core.follow` sorts before
  `core.move`), and saves and replays remap them by name (the 2026-10-24 refinement, R73).
* **Input cap over every order kind.** `MAX_WAYPOINTS` (32) bounds `MoveOrder.orders`, every order kind counted. A
  queued move, rotate or follow onto a unit whose orders are full rejects the whole command with `core.queue_full`,
  changing nothing (R77: a code constant until C7). In M1, `MoveOrder` v2 holds a closed `Order` enum (`Move`,
  `Rotate`, `Follow`), an interim that C7's order queue and activity stack replace in the M2 prelude.
* **Forbidden targets.** A target the rules forbid (a blocked cell for `debug.spawn` or `debug.teleport`, a missing
  leader for `core.follow`, a rect with `min > max` or outside the map for `debug.set_terrain`) is rejected with
  `RejectReason::Feature("core.invalid_target")`. That's a stable code, and no new variant (R71, R79). An unknown
  terrain id is `core.unknown_content`.
* **Confirmation.** `payload_names`, `debug_set_terrain_is_debug_only_without_units`,
  `new_payloads_round_trip_postcard` (Task 2); `spawn_on_blocked_cell_is_rejected`,
  `teleport_into_water_is_rejected`, `set_terrain_rejects_bad_input`, `set_terrain_needs_cheats` (Task 5); the ported
  `queued_waypoints_are_capped` (Task 8); `follow_self_or_missing_leader_rejected`, `queued_orders_share_the_cap`
  (Task 9); `console_terrain_issues_debug_set_terrain`, `unknown_command_name_is_reported` (Task 16).

## Refinements (2026-10-25, M2 prelude)

Status: proposed (M2 prelude Task 3); the controller accepts it at that task's review under the user's delegation
(R85), and the plan's final report lists it for ratification. From the M1 final review (`final-review-m1.md` M14) and
controller rulings R71, R73, R77, R148.

* **`MAX_COMMAND_UNITS` moves to `fl_protocol`** (`fl_protocol::MAX_COMMAND_UNITS` = 256), next to the envelope it
  bounds. `fl_sim` re-exports it, and the client imports it instead of keeping its own copy (`fl_client::orders`), so
  the two can no longer drift. `MAX_WAYPOINTS` (32) stays an `fl_sim` code constant: it bounds the save decoder of the
  order queue (ADR-0018), so it can't be content.
* **`core.no_map`.** A handler that needs a map when none is set up rejects with
  `RejectReason::Feature("core.no_map")` instead of `core.no_valid_units` (M14), in `commands.rs` and
  `debug_cmds.rs`. It is a stable code, not a new variant (R71). `core.no_valid_units` keeps its meaning: none of the
  named units can carry out the command.
* **Order kinds by name.** Order kinds are registered (table `"orders"`, ADR-0018); a queued order's kind is saved as
  its dense id, remapped by name through the saved table, and hashed by name, as command type ids are (R73).
* **Confirmation.** `max_command_units_lives_in_protocol`, `no_map_is_its_own_code` (Task 13); the `fl_protocol` API
  and format snapshots.
