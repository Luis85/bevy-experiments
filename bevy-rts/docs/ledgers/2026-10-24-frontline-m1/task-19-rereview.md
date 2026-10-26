# Task 19 re-review, fix round 1 (ec6930e)

Important 1 (stale dense ids after a rebuild): ADDRESSED. debug/mod.rs `remap_rebuilt` maps SnapshotBuffer prev/curr, backlog tick snapshots and Spawned events, and every UnitView.unit old->new by name. It removes dropped SimIds and units whose type is gone (view, ViewIndex entry), then installs the new ClientContent. Parts rebuild via placeholder.rs:203/214. Test `rebuild_never_shows_stale_unit_ids` is paused, adds `base.de.a_scout` (sorts before every base.de.*), asserts pz3's dense id moved, and checks views, curr snapshot and ViewIndex for #3. M1 AI is `NullAi` (unit struct), so no dense ids are kept. The only dense-id fields are EntityRecord.unit and Spawned.unit; both are covered.
Important 2 (one bounded wait): ADDRESSED. link.rs owns Deadline/WaitError/read_runner/failure_text/wait_for_reply. remote.rs `query` calls wait_for_reply, and `fl.step` uses the shared Deadline/read_runner. The only `recv_timeout` and `Instant::now` in fl_client are in link.rs.
Important 3 (timed-out reload not recorded): ADDRESSED. reload.rs reload_values records `ContentReload{ next_tick at send, new.rules_hash() }` on a wait error. `reload_wait_is_bounded` asserts the event, that the saved replay is non-reproducible, and that the late reply is dropped (abandon, no second record).
Minor 1 (rebuild ContentReload asserted): ADDRESSED. Test: tick == next, hash == the new content's.
Minor 2 (debounce): ADDRESSED. `settled_change` counts scans, not time. Tests are deterministic: the length changes between writes, and nothing depends on a clock.
Minor 3 (spurious change rebuilds everything): ADDRESSED at the watcher level. Per-file byte hash, so a touch is no change; `watcher_ignores_a_touch`. The reload() path still swaps on equal-visuals rules-hash-equal content, but a touch no longer gets there.
Minor 4 (map re-guessed): ADDRESSED. `rebuild_keeps_the_known_map`.
Minor 5 (hardcoded delay): ADDRESSED. Uses input_delay().
Minor 6 (InputLog size): ADDRESSED. `{ticks, inputs with commands, hashes}` via `record`; fl.step also records.
Minor 7 (stale terrain record): ADDRESSED. TerrainCursor plus `install_terrain` in the load hook; `stale_terrain_records_are_dropped_on_load`.
Minor 8 (save-error path): ADDRESSED. `save_error_fails_the_rebuild`.
Minor 9 (process noise): not a code finding; the gate ran in 232 s, under budget.

New findings:
- Minor: `remap_rebuilt` is covered only for views and `curr` in the test. The `prev` buffer and the backlog tick snapshots and spawn events are remapped by the same closure but have no assertion.
- Minor: `reload_wait_is_bounded` does not re-assert after the late reply that the capture still holds exactly one event, so the "no double record" check is implicit.
- Minor: the watcher still skips a same-length edit that keeps the same mtime (coarse filesystem timestamps). That is as before; it should be documented.
- Minor: a Failed (disconnect or runner failure) wait in `reload_values` is also recorded as a reload. That is a safe false non-reproducible.
- Minor: the `fl.*` timeout error text changed ("2 s" to "2000 ms"). No test asserted the old text.
- No wall-clock dependence added to the tests.
