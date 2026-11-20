# 0.19 world, content, architecture and performance review

Baseline: attached 0.18 source; reviewed 29 October 2026. This is source inspection, not a new GPU benchmark or a native visual acceptance session.

## Current strengths

- `ChunkStreamer` has bounded residency, queued work and worker count; validates source payloads on a worker; isolates provider panics; rejects wrong tile keys; uses exponential failure backoff. Requests and completion integration remain separate.
- Prepared heightmaps retain immutable content identity, explicit provenance, real union coverage and seam checks. Missing geometry remains missing. Existing support/feature records stay authoritative independently of render LOD.
- Imported presentation has per-model/per-instance reservations, loader limits, fallback meshes and explicit estimated-memory language. Quality presets already express cost budgets instead of unsupported FPS promises.
- The 0.18 report correctly leaves Windows/GPU/input-device qualification, production content and real geography in G02/G07/G08/G09/G10. Existing finite measurements cannot close those areas.

## Findings

| Priority / ID | Finding and evidence | Consequence | Proposed acceptance |
| --- | --- | --- | --- |
| P1 / WP01 | `crates/terra-bevy/src/render.rs:245-249` keys mesh jobs by tile, global edit/profile revision and LOD. `render_chunks` checks current residency/revision/LOD at lines 344-350, but retains no source instance identity. A tile can be evicted and reloaded while its detached worker is running, with the new `Arc<WorldChunk>` under the same key. | An old completion can replace visible meshes even though current terrain/feature queries use the new source. Reloading after fallback/live recovery or a custom provider update can briefly display terrain/buildings from a different source instance. | Retain the immutable chunk Arc in the job and require pointer identity with the currently resident Arc before integration. Requeue replacements through the existing bounded queue. Inject deterministic completed jobs into a GPU-free render-system regression: replaced source must never integrate; unchanged source must integrate; revision and LOD rejection must remain. |
| P2 / WP02 | `render_chunks:336-353` removes a disconnected worker completion, then silently discards it. An edit/profile rebuild with an already integrated matching LOD has no persistent dirty state after the job was dispatched. | A failed remesh can leave an edited source feature visible indefinitely; a poisoned receiver lock instead occupies a worker slot indefinitely. | Keep dirty work until successful integration and expose failure; use bounded retry/backoff rather than an unbounded per-frame respawn loop. This deserves a separately scoped reliability change with deterministic disconnect/poison tests. |
| P2 / WP03 | `render_chunks:281-285` invalidates every resident source mesh when any removed feature changes, even for a single harvested tree. | The work is bounded by current residency, but a large region can remesh many unaffected chunks. No measurement currently establishes frame or loading tails for repeated edits under the large preset. | First measure affected-chunk count and mesh work during representative harvest/build sessions. Then index source feature IDs to resident chunks or maintain per-chunk edit signatures. Preserve boundary-clipped fragments and complete invalidation on world replacement. |
| P1 qualification / WP04 | G07 remains open: model estimates are explicit reservations, source vertices are geometry counts, and frame intervals are main-app timings. | Neither counts nor CPU timing establish GPU residency, GPU frame time, clean-driver behavior or production asset loading peaks. | Name a supported GPU/driver/display configuration and collect a steady-state plus loading/quality-switch journey; retain p50/p95/p99/max intervals and allocator/OS GPU memory observations with workload and native artifact hash. |
| P1 qualification / WP05 | G02/G09/G10 remain open for the intended Windows player machine, production art and licensed shipping geography. | Linux fixtures cannot certify Windows rendering, artist workflows or source-data suitability. | Keep those as explicit release acceptance work; preserve original fixtures and content identity. Add content/OS qualification only when that actual input/environment is available. |

## Recommended bounded implementation

Implement WP01 now: source-bound mesh completion admission plus GPU-free real-system regressions. Keep the two-job concurrency ceiling, configured integration budget, LOD hysteresis, procedural/source data, authority and save/wire/content schemas unchanged. Do not bundle a broad renderer rewrite into this correctness fix. WP02 and WP03 need their own clearly accepted scope and tests/measurements; WP04/WP05 require the actual deployment hardware/content.

## Review limits

No Rust command or fresh native/GPU run was executed by this review at the time of writing; the toolchain was not visible in the initial PATH. Historical 0.18 screenshots and receipts were read as baseline evidence only. Findings identify code paths and reproducible test constructions; implementation acceptance must be recorded after the source is changed and actual checks run.
