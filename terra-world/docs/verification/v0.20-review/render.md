# 0.20 world/render/performance review and implementation plan

WP02 is reproducible from current source: render_chunks consumes pending work at dispatch; a disconnected current-source job is discarded without restoring that work when the old mesh already has the requested LOD. A poisoned receiver lock never produces a completion and holds a slot. Visible terrain can therefore retain an old edit indefinitely.

Selected closure: retain failed work with per-source/revision/LOD exponential wall-clock backoff (0.5 seconds to 30 seconds), release poisoned/disconnected tracked jobs, retry through the incumbent two-job queue, clear failure state only after success or context invalidation/eviction, and expose retry/failure diagnostics through F12/local opt-in reports. Existing Arc identity, revision, LOD and physical authority remain unchanged.

Acceptance: deterministic real-system tests inject disconnected and poisoned receivers, prove old valid geometry remains, failure work remains pending, no per-frame retry storm occurs, force only the deadline to permit retry, inject a successful current completion and prove recovery. Additional tests cover repeated failure delay cap and context/eviction reset. No Cargo execution is owned by this specialist.

Broader review: source/provider ingestion remains bounded and retains provenance, seams and unavailable terrain. WP03 region-wide edit invalidation still needs representative measurement and per-chunk signature design. Physical GPU/loading residency, Windows, production animation/audio/assets and licensed shipping geography remain qualification/content gaps. A hung worker thread (rather than closed/poisoned receiver) requires a separately designed cancellation/timeout policy; no thread-preemption guarantee is claimed. Source correctness tests do not qualify actual pixels or hardware.

## Implemented and ready for central validation

Changed `crates/terra-bevy/src/render.rs`, `crates/terra-demo/src/performance.rs`
and renderer guide. The retry ledger is bounded by resident pages and retains
actual source Arc/revision/LOD. Both receiver failures remove the tracked job;
backoff blocks fresh dispatch rather than spinning; last valid meshes remain.
The ledger survives retry dispatch so repeated failures exponentially back off;
source/revision/LOD changes and eviction discard obsolete contexts. Successful
completion clears its active ledger. F12 and local opt-in render JSON expose
cumulative failure count, retrying pages and historical last error.

Three new real-system tests cover disconnect+poison after an actual dirty-edit
dispatch, old geometry conservation/no repeated-frame respawn, forced-deadline
successful recovery, increasing/capped backoff and source/revision/eviction
reset. Existing source identity/revision/LOD tests remain. Tests inject channels
and deadlines without sleeping or GPU/window. Rustfmt passed; **no Cargo test
execution is claimed** before the integrator's run.

Suggested central commands: `cargo test -p terra-bevy --lib --locked --offline
mesh_failure`, then existing `mesh_completion` and complete render/demo suites.
