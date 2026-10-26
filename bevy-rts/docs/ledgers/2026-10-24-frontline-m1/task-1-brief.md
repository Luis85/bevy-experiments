### Task 1: ADRs 0014–0016, refinements to ADR-0007 and ADR-0012, spec sync

**Track:** contract (docs only) · **Lane 1** · No cargo commands.

**Files:**
- Create: `docs/adr/0014-terrain-layers-and-movement-classes.md`, `docs/adr/0015-path-requests-derived-caches-debug-queries.md`, `docs/adr/0016-ai-controller-seam.md` (from `docs/adr/template.md`, MADR 4.0, status `proposed`)
- Modify: `docs/adr/0006-component-classification-and-save-format.md`, `docs/adr/0007-lockstep-command-envelope-and-match-header.md`, `docs/adr/0012-hot-reload-policy.md` (a new `## Refinements (2026-10-24, M1)` section each; ADR-0006 and ADR-0007 already have the final M0 review's `## Refinements (2026-10-24)` sections, which stay as they are), `docs/adr/README.md` (index rows), `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (§5.3, §5.5, §5.7, §6.2, §6.4, §8, §11, as listed below, plus a new revision section)

**Decisions each ADR records** (these are the contracts later tasks implement):
- **ADR-0014, terrain layers and movement classes.**
  - **Terrain kind.** `*.terrain.ron` is a built-in `TerrainDef { id, name, pass, blocks, color }`, rules-hashed.
  - **Map layers.** `MapDef` gains `terrain: Option<MapTerrain>`, naming a 16-bit (or 8-bit) grayscale height PNG and an RGB terrain PNG next to the map file, a height range in metres, and an RGB → terrain-id legend.
    - There is one pixel per cell, and pixel row 0 is the north edge.
    - The loader decodes the layers through a new `ContentKind::load_sidecars` hook into a non-RON `MapDef.layers` field, which the rules hash covers.
    - A map without `terrain` is open ground: pass 1, blocks nothing, height 0.
    - **Where PNG decoding lives.** Only `fl_content` decodes images (the `png` crate: pure Rust, no Bevy, accepted by `arch-check` for content and sim trees). Sim crates and the client read the decoded integer `MapLayers`, never a PNG.
  - **`MoveClass`** moves to `fl_content` (re-exported by `fl_sim`).
  - **Profiles.** A movement profile is `(class, size)`, where `size = ceil(footprint width / cell)` clamped to `1..=Path.max_clearance`.
  - **Blocking.** A cell is blocked for a class if its terrain lists the class in `blocks`, or if the height step to a 4-neighbour exceeds `Movement.max_slope_m[class]`. The map edge counts as blocked.
  - **Clearance.** Clearance is the largest k ≤ `max_clearance` for which the k×k square anchored at the cell (+x, +y) is unblocked.
  - **Speed** = `road_speed × clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)` (spec §7). `terrain_factor` may lie in [−1, 1] (research §1.8: trucks are negative).
  - **Path cost.** Path costs use a per-class `Movement.path_terrain_factor`, because one class's unit types differ and HPA* is shared per class.
  - **Terrain is Hashed state** (`core.terrain`), mutable by `debug.set_terrain` in M1, and later by craters, flattened forest and bridges.
- **ADR-0015, path requests, derived caches and debug queries.**
  - **The queue.** Requests enter a Hashed FIFO `PathQueue`: from command handlers in `(player, seq)` then `SimId` order, and from `advance_orders` in `SimIndex` order.
  - **Processing.** `ThinkSet::Behaviour` pops requests while `spent < Path.budget_per_tick` (at least one per tick). Each one is charged its cold cost, applied to its unit immediately, and so applied in request order. A request whose unit is gone, or no longer waits on it, is discarded at charge 0.
  - **HPA\*.** 16×16-cell clusters per profile, with the entrance rule of Task 7, A* refinement and string-pulling.
  - **Flow fields.** Group moves of `Path.flow_min_group` or more same-profile units use a flow field per `(destination cluster, profile)`. It is an abstract Dijkstra over the HPA* graph plus per-cluster tiles built lazily, in `SimIndex` order, before movement.
  - **Caches.** `NavGrid`, `HpaGraphs` and `FlowCache` are `Derived`. A cache never changes outcomes, because charges are cold costs and every cached value is a pure function of Hashed state. Eviction is therefore free to be any deterministic policy. They are rebuilt by core's one `Derived` rebuild (`rebuild_core`: spatial index, then `NavGrid`, `HpaGraphs`, an empty `FlowCache`; ADR-0006 refinement R74).
  - **Per-tick results.** Steering pushes and similar per-unit, per-tick results live in a Transient scratch resource sorted by `SimId`, never in a Transient component (R72: the archetype move per entity per tick; R76).
  - **Invalidation.** A terrain change updates `NavGrid` for the touched rect, rebuilds only the HPA* clusters it touches, clears the profile's flow entries, and makes units whose remaining path crosses a rebuilt cluster re-request.
  - **Debug queries.** `SimBuilder::register_debug_query(name, fn(&World, &Value) -> Result<Value, String>)`, answered between ticks through `SimQuery::Debug { name, args }` → `SimReply::Debug(Value)`. They are read-only, observe-only and JSON, with `Fx` as raw `i64`. Overlays, BRP `fl.debug_query` and `fl_cli query` use them.
- **ADR-0016, AI controller seam (stub scope).**
  - **Types.** `fl_view::ai::{PlayerView, AiController, NullAi, ai_controller}`.
  - **Building the view.** `PlayerView` is built from the snapshot on the client side until vision exists. M3 moves it into the sim's `Publish` phase.
  - **Hosting.** Players whose controller is `Controller::Ai(id)` are hosted by `fl_client::ai::AiHost`. It thinks once per new snapshot tick, and its drafts go through `LocalDriver` as that player.
  - **Scope.** The full AI placement ADR stays deferred to the first AI faction (§9.3).
- **ADR-0006, M1 refinement (R78).**
  - `Codec::fill_missing(fn(&mut World) -> Result<(), String>)` fills a Hashed or Derived state name that is absent from a save whose owning feature version is older than the build's. Fills run after every saved value is restored and before the Derived rebuilds; an `Err` is `LoadError::Codec`. Without a fill, or in a save of the same version, loading is unchanged.
  - Core becomes version 2 in M1. It registers fills for `core.terrain` (from the content map matching the saved `MapInfo`), `core.path_queue` (empty) and `core.path_follow` (`Idle`).
  - M0 saves therefore load under M1, as the `m0_testbed.save` fixture shows.
- **ADR-0007, M1 refinement.**
  - New core payloads `core.rotate { toward }` and `core.follow { leader }`, and the debug payload `debug.set_terrain { min_cell, max_cell, terrain }`. Adding a payload type needs no `FORMAT_VERSION` bump; the format guard allows added types. Their dense command ids may shift existing ones, and saves and replays remap them by name (the 2026-10-24 refinement, R73).
  - The input cap `MAX_WAYPOINTS` (32) bounds `MoveOrder.orders`, every order kind counted. A queued move, rotate or follow onto a full order is `core.queue_full` (R77: a code constant until C7).
  - A target the rules forbid (a blocked cell, a missing leader, a rect outside the map) is rejected with `RejectReason::Feature("core.invalid_target")`. That's a stable code, and no new variant (R71, R79).
- **ADR-0012, refinement.**
  - **Watcher.** Content is loaded with `std::fs`, not the AssetServer, so the testbed polls mod-file modification times (`debug.ron`: `hot_reload`, `hot_reload_poll_ms`) instead of using Bevy's `file_watcher`.
  - **Value reloads** reach the runner as `ToRunner::Control { id, control: RunnerControl::ReloadContent(OpaqueContent) }`, answered with `SimReply::Reloaded { tick, rules_hash }`. `ToRunner` becomes `#[non_exhaustive]` with that variant, like `SimQuery`/`SimReply` (R80).
  - **Derived state after a value reload.** It runs every feature's `Derived` rebuild in feature load order, as `load_world` does (R74).
  - **ID-set rebuilds** use `load_world_with(.., LoadOptions { drop_unknown_units: true })`.
  - **Replays.** The client captures a replay of the running match (`replay save <path>`), so reloads have a replay to record their `ContentReload` event in.
  - **Def components** gain `DefComponent::reload`, which keeps state ratios.

**Spec edits** (same commit; "an ADR that changes a contract updates the spec"):
- §5.3: `SimQuery::Debug`.
- §5.5: the core payload list names its fixed payloads.
- §5.7: the cold-cost charge, lazy flow tiles, Derived caches.
- §6.2: the terrain type fields and map layer format.
- §6.4: the polling watcher and the runner control message.
- §8: `PlayerView` is a client-built stub until M3.
- §11: overlays read debug queries.

- [ ] **Step 1: Write the three ADRs** with the decisions above, each with Context, Decision Drivers, at least two Considered Options with pros and cons, Consequences and Confirmation. Confirmation bullets name the tests of Tasks 5–20 that check each decision, by the test names this plan gives.
- [ ] **Step 2: Write the three refinement sections** (ADR-0006, ADR-0007, ADR-0012) and the README index rows (status `proposed`).
- [ ] **Step 3: Edit the spec sections listed.** Then add a section `## Revision 2.3 (2026-10-24, M1 contracts)` above `## Revision 2.2`, in 2.2's format: one bullet per contract with its ADR (ADR-0014, ADR-0015, ADR-0016, and the M1 refinements of ADR-0007 and ADR-0012). Revisions 2.1 and 2.2 already exist (M0 implementation and final M0 review), so don't reuse those numbers. Update the status line at the top of the spec to name revisions 2.2 and 2.3.
- [ ] **Step 4: Verify.** Run `git diff --stat`. Expected: only the files above. Then `grep -c "^status: \"proposed\"" docs/adr/001[456]-*.md`. Expected: `1` for each file.
- [ ] **Step 5: Acceptance (lead).** The lead asks the user to accept ADR-0014, ADR-0015 and ADR-0016 and the three refinements (ADR-0006, ADR-0007, ADR-0012). On acceptance, set `status: "accepted"` and the date. On change requests, revise and ask again; no later task starts before acceptance.
- [ ] **Step 6: Commit** `docs(adr): ADR-0014 terrain layers, ADR-0015 path requests and debug queries, ADR-0016 AI seam; refine 0007, 0012` with trailer `Refs: ADR-0011`.

