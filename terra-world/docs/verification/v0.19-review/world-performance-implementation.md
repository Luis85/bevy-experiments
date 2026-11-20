# 0.19 source-bound mesh completion implementation

Scope: WP01 from the world/performance review and the dispatched 0.19 plan.

## Change

`crates/terra-bevy/src/render.rs` replaces tuple-shaped mesh job records with a named `ChunkMeshJob` holding revision, LOD, receiver and the actual immutable `Arc<WorldChunk>` used by its worker. Integration now requires pointer identity with the currently resident page in addition to the existing tile-residency, edit/profile revision and desired-detail checks. A stale source completion is discarded and the current page is requeued through the existing two-job, nearest-first dispatch path. Retaining the Arc also prevents allocator address reuse from making an old job appear current.

No authority, source data, save/wire/content schema, profile budget or LOD thresholds change. The additional retained reference does not clone source payload bytes; the worker already retains those bytes while building. Documentation describes the exact admission boundary.

## Meaningful regressions

Four tests exercise the real `render_chunks` system in a Bevy App without a GPU/window. Finished jobs are explicitly injected so admission does not depend on worker timing:

- A completion for the current source integrates and leaves both queues empty.
- A completion for old 10 m terrain cannot publish over current 120 m terrain; the retry uses the current page, and its explicitly ready completion integrates the current height.
- A new page with byte-equivalent source data still requires its own job, proving source-instance identity rather than accidental content/value equality.
- Stale edit revision, stale LOD and actual eviction retain their existing rejection/retry/no-retry behavior.

Run centrally (compilation owned by the integrator):

```sh
cargo test -p terra-bevy --locked --lib mesh_completion
cargo test -p terra-bevy --locked --lib render::cache_tests
```

No Cargo command was run by this implementation owner. Results must be recorded from the integrator's actual run; these tests do not establish native visual or GPU-performance qualification.

## Deliberate remaining scope

WP02 remains open: a disconnected remesh can lose dirty work when existing geometry already has the target detail, and a poisoned receiver can retain a slot. A safe closure needs persistent dirty state plus bounded failure/retry reporting; merely requeuing every frame could create a worker storm. This is separate from rejecting and requeuing a successfully completed stale-source job.

WP03 remains open: a single removed feature still invalidates all resident source meshes. An affected-chunk index or per-chunk edit signature requires measurements and careful treatment of clipped fragments; no performance improvement is claimed here. G07 hardware frame/residency tails and G02/G09/G10 Windows, production art and shipping-data qualification also remain open.
