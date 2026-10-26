---
status: "accepted"
date: 2026-10-24
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md C16/N4; baseline-recommendations.md A11"
---

# Hot reload: values in place, ID-set changes rebuild, reloads are replay events

## Context and Problem Statement

Revision 1 (§6.4) rebuilt the `ContentDb` on every RON edit and re-instantiated stat components on live entities. But
interning reruns on reload: when the set of IDs changes (a unit added, removed or renamed), dense IDs shift, and live
entities, queued commands and caches hold stale ones. A reload also changes the rules hash mid-match, so a replay of
that session no longer reproduces from its header. Hot reload lands in M1 (the M0 plan moved it there).

## Decision Drivers

* Fast iteration on stats in the testbed (G4).
* No stale dense IDs, ever (ADR-0004).
* Replays stay honest: a session with reloads must not claim to be reproducible.
* Never in competitive or multiplayer play.

## Considered Options

* Always re-instantiate in place (revision 1)
* Values in place; an ID-set change rebuilds the match from a save; reloads recorded as replay events
* Always rebuild the match from a save

## Decision Outcome

Chosen option: "Values in place; an ID-set change rebuilds from a save; reloads are replay events".

- Hot reload is testbed-only: the match must have `cheats_allowed`, and it is never allowed in a lockstep match.
- On a file change the loader builds a new `ContentDb` and compares the ID sets of every registry and content table
  with the running match.
- **Same ID sets (values only):** the driver applies the new content in place at a tick boundary. Stat components on
  live entities are re-instantiated, keeping their state ratios (HP ratio, ammo ratio).
- **ID set changed:** the driver saves the match (ADR-0006), builds a fresh sim with the new content, and loads the save
  through the string tables. Entities whose type disappeared are dropped and logged.
- Every reload appends `ContentReload { tick, rules_hash }` to the replay. Such a replay is marked non-reproducible, and
  `replay-verify` reports that instead of a false divergence. (Storing the reloaded content alongside is a later
  option.)

### Consequences

* Good, because value tweaks stay instant and ID changes are safe.
* Good, because the rebuild path exercises save/load on real sessions, which is extra coverage for ADR-0006.
* Bad, because an ID-set change costs a save and load (sub-second at testbed scale).
* Bad, because reload sessions can't be verified as replays. That is acceptable for a testbed.

### Confirmation

* M1 tests: a value-only reload keeps ID sets and HP ratios; adding a unit triggers a rebuild with identical `SimId`s
  for surviving entities; a replay with a reload event reports non-reproducible.

## Pros and Cons of the Options

### Always in place

* Good, because it is simplest.
* Bad, because of stale dense IDs on any ID-set change.

### Values in place, rebuild on ID change

* Good, because it is fast in the common case and correct in the rare one.
* Bad, because there are two paths to test.

### Always rebuild

* Good, because there is one path.
* Bad, because it is slower for every tweak, and loses transient presentation state.

## More Information

* Spec §6.4, §11 (inspector and hot reload).
* Research: `architecture-best-practices.md` C16, N4.
* Related: ADR-0004, ADR-0006, ADR-0007 (rules hash in the header), ADR-0008 (`cheats_allowed`).

## Refinements (2026-10-24, M1)

Status: accepted at the M1 Task 1 review (rulings R69, R84), with ADR-0014–0016. Controller rulings R69, R70, R74,
R80.

* **Watcher.** Content is loaded with `std::fs`, not the AssetServer, so the testbed polls mod-file modification times
  and lengths (`debug.ron`: `hot_reload`, `hot_reload_poll_ms`) instead of using Bevy's `file_watcher`. The console
  command `reload` forces one. A file that fails to load (a malformed RON saved mid-edit) is reported, and the match
  keeps running; a match without `cheats_allowed` refuses the reload.
* **Value reloads** reach the runner as
  `ToRunner::Control { id, control: RunnerControl::ReloadContent(OpaqueContent) }`, answered with
  `SimReply::Reloaded { tick, rules_hash }` (or `SimReply::Error`). `OpaqueContent` wraps an
  `Arc<dyn Any + Send + Sync>`, because `fl_view` doesn't depend on `fl_content`. `ToRunner` becomes
  `#[non_exhaustive]` with that variant, like `SimQuery`/`SimReply` (R80), and `RunnerControl` is non-exhaustive too.
  The reload needs `cheats_allowed` and equal ID sets, replaces the content and the header's rules hash, and journals
  `core.content_reload`.
* **Derived state after a value reload.** It runs every feature's `Derived` rebuild in feature load order, as
  `load_world` does (R74), so navigation data follows changed terrain values. Then, in `SimIndex` order, every unit
  whose movement profile changed (a new class, or a footprint width across a cell boundary) and that waits on a
  request, follows a path or follows a flow field returns to `Idle`, journaled `replanned`, and re-requests; its old
  request turns stale. No Hashed path state names a profile the rebuilt graphs lack.
* **Map layers don't reach a running match.** `TerrainGrid` is Hashed match state (ADR-0014). An edited map PNG
  changes the content and the rules hash, but neither reload path rewrites the running match's terrain; the edit
  shows when a match next starts from that map. `debug.set_terrain` is the in-match edit.
* **ID-set rebuilds** save the running match and load it with
  `load_world_with(.., LoadOptions { drop_unknown_units: true })`: an entity whose unit type vanished is skipped, its
  `SimId` stays unused, and it is listed in `Loaded.dropped` and logged; survivors keep their `SimId`s. `check_save`
  ignores the rules hash (R70), so the save loads under the edited content. Any load fails with
  `LoadError::Codec { name: "core.terrain", .. }` when the saved `TerrainGrid` palette names a terrain kind the content
  lacks; a rebuild that fails to load reports the error and leaves the running match as it was.
* **Replays.** The client captures a replay of the running match (console `replay save <path>`), so reloads have a
  replay to record their `ContentReload` event in; `replay-verify` reports such a replay as non-reproducible. The
  event comes from the runner's `SimReply::Reloaded`, never from the journal (which may be off). A value reload whose
  answer times out is recorded anyway, at the tick the runner would apply it: a falsely non-reproducible replay is
  safe, a falsely reproducible one is not.
* **Never in a lockstep match (R133).** The client reloads only a match that a `LocalDriver` drives, and refuses
  when the `NetDriven` marker is present, whatever `cheats_allowed` says (both peers may allow cheats, and a reload
  reaching one runner would desync it). A net shell (fl_net, M8) inserts `NetDriven` and never a `LocalDriver`, with
  its own refusal test.
* **Client state after a rebuild.** The rebuild continues the same match, so the client keeps its AI host, replay
  capture, input log, journal and current scenario; every dense unit id it holds (snapshots, unsent backlog, views) is
  mapped to the new content by name, and the dropped units vanish (R131, R135).
* **Def components** gain `DefComponent::reload` (by default `instantiate`), which keeps state ratios: `Hull` keeps
  the HP ratio, and `Mobility` replaces only `Mobility`, keeping orders and paths.
* **Confirmation.** `same_id_sets_true_for_value_change`, `same_id_sets_false_when_unit_added`,
  `opaque_content_eq_is_identity`, `value_reload_keeps_ids_and_hp_ratio`, `value_reload_changes_speed`,
  `value_reload_updates_passability`, `reload_refused_without_cheats`, `reload_refused_when_ids_differ`,
  `reload_journals_content_reload`, `reload_is_deterministic`, `rebuild_drops_vanished_unit_types`,
  `load_without_drop_rejects_unknown_unit`, `threaded_runner_applies_reload`,
  `rebuild_with_vanished_terrain_is_load_error`, `reload_changing_profile_replans`, `value_reload_keeps_terrain_grid`
  (Task 12); `value_edit_reloads_in_place`,
  `adding_a_unit_rebuilds_with_same_sim_ids`, `removing_a_unit_type_drops_its_units`,
  `malformed_ron_keeps_match_running`, `reload_refused_without_cheats`, `reload_while_paused_applies`,
  `visual_only_change_swaps_client_content`, `reload_records_replay_event`, `watcher_detects_change`,
  `failed_rebuild_keeps_match_running`, `reload_refused_in_lockstep_match`, `reload_wait_is_bounded`,
  `rebuild_never_shows_stale_unit_ids` (Task 19).
