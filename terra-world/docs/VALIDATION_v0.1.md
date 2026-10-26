> Archived v0.1 evidence; this is not the v0.2 validation record.

# Validation record

Validation date: 2026-10-26. Toolchain: Rust 1.99.1; Bevy 0.20.0. Tests ran on Linux. Windows, macOS, WebAssembly, mobile hardware and public-Internet deployment were not exercised.

## Domain and data suites

| Suite | Passing tests | What was exercised |
|---|---:|---|
| `terra-core` | 12 | WGS84 round trips, poles, antimeridian, antipodes, tiled geography, environment and actor contracts |
| `terra-sandbox` gameplay | 13 | Transactional inventory/build/craft/farm/gather/damage; catalog-aware persistence; exact GPS JSON round trip |
| `terra-sandbox` factions | 7 | Invitations, role changes, shared property, demotion, invalid saves and tagged-message roster deserialization |
| `terra-cli` | 4 | Command parsing, scripted workflows and complete demo round trip |
| `terra-history` | 13 | Date intervals, missing coverage, ambiguous packs, provenance, path/file budgets, holes, antimeridian, GeoJSON round trips and both bundled eras |
| `terra-stream` unit | 11 | Terrain seams, clipping, cache corruption/age/eviction, bounded work, deduplication, stale completion and retry policy |
| `terra-stream` provider fixtures | 5 | Terrarium decoding, OSM parsing, units, timestamps, offline and polar behavior |
| `terra-stream` HTTP integration | 1 | Actual local HTTP, PNG/JSON decoding, clipping, cache reuse, serialized Overpass access, response spacing and exhausted budgets |
| `terra-net` unit | 12 | Incremental frames, rate/buffer budgets, capabilities, geographic rules, configuration and worst-case snapshot size |
| `terra-net` TCP integration | 13 | Two clients, authoritative movement and commands, interest filtering, replay protection, shared factions, resume, custom catalogs, historical manifests, coverage and isolated malformed peers |
| `terra-bevy` headless unit | 15 | Config bounds, terrain/water/slope semantics, base-world identity, faction replicas, drawing/export, invalid drafts, regional atlas framing, historical grid identity, local saves, antipodal distance, actual TCP routing of late/backlogged commands and bounded recovery after a stalled frame |
| `terra-bevy` headless integration | 6 | Actual plugin scheduling/streaming, fixed clock pause, distant rebasing/date-line jumps, queued atomic commands, disabled weather, paused regional weather refresh and custom-catalog source filtering |
| `terra-bevy` renderer-specific | 3 | Concave polygon triangulation, degenerate geometry rejection and structural mesh fingerprints that ignore inventory-only changes |
| `terra-demo` interaction | 1 | Resource selection skips nearer forbidden and vertically unreachable nodes, selects a permitted node accepted by the domain, and handles depletion |

These counts total **116 distinct tests**. The pure crate tests were independently exercised by their owners and then all 91 passed together in the root Cargo workspace run. The final Bevy headless Cargo target passed all 15 unit and 6 integration tests, including real TCP schedule and stalled-frame regressions discovered during native review. Renderer-specific and demo interaction tests were run with the rendering feature enabled. Do not add repeated runs of the same test to the distinct count.

## Actual executable workflows

The CLI sandbox workflow completed 24 commands with two players, one faction, a transferred plot, three faction-owned structures and a shared home. Save/reload equality was true, including offset GPS values. The offline probe returned a 33×33 heightfield and 22 explicitly synthetic features. The deterministic movement example advanced six seconds and approximately nine metres.

Both finite Bevy examples run successfully with rendering disabled: the standard headless example loaded a chunk and queried terrain; the custom-provider example loaded a chunk through the public provider hook.

The independently built server/client process runner passed six groups:

1. Procedural host with two independent clients; neighbouring source chunks are registered beyond the spawn tile.
2. Clean host save/restart preserving both identities, inventories and command sequences.
3. Historical era-B host with a matching dated political dataset.
4. Wrong epoch, pack or political revision rejected before saved-player allocation.
5. Invalid/missing history configuration, uncovered spawn, conflicting grid and mismatched saved epoch rejected.
6. FIFO/special-file inputs rejected before they can block startup.

The reproducible runner is `crates/terra-net/tests/process_smoke.py`; see `MULTIPLAYER.md` for its command line. Client capability files used by the tests are temporary and are not included in this source bundle.

## Real public data probe

The strict live terrain/weather probe ran with synthetic fallback disabled. It retrieved real terrain samples near Munich, ranging from 519.889 to 530.212 metres, and a current Open-Meteo model sample marked `synthetic: false`. This verifies the implemented HTTPS decoding and unit paths for those providers; it is not a survey comparison or a weather accuracy assessment.

The public Overpass POST timed out at 20 seconds, and a tiny GET diagnostic returned HTTP 406. OSM parsing and the real HTTP/cache/rate-limit path pass against a local fixture. A successful public OSM fetch was **not** observed in this environment. The UI reports this provider condition explicitly and does not silently relabel generated features as OSM.

Exact measurements and attribution are retained in `crates/terra-stream/verification/live-probe-2026-10-26.json`. The final probe may reuse source bytes downloaded earlier in the same verification session; cached retrieval timestamps retain their original age. TLS verification remains enabled.

## Native explorer and renderer

The native application was built and run on a virtual X11 display using Mesa 25.2.8 / LLVM 20.1.2 software Vulkan. Screenshots were captured from the actual application and inspected independently. The 1440×900 layout and the declared 1100×700 minimum were both exercised. See `PRODUCT_REVIEW.md` for findings and limits.

- The finite native smoke run completed 160 frames with nine resident chunks and reported success after claiming land, crafting, building a house/workbench/fence, setting a home and planting wheat.
- An actual UI terrain click placed a fourth structure (another fence), and F5 persisted it. Named faction creation and plot transfer were exercised. F9 restored the earlier personal-plot state, changing the visible shared-plot count from one back to zero.
- An invalid latitude of 91 degrees was rejected with a useful example; geographic focus stayed unchanged.
- The current country atlas rendered with provenance and boundary-policy text. The historical atlas originally appeared empty because its regional polygons were too small at global scale. After the fit/zoom correction, both 1950 regions were visible.
- The native regional editor accepted three clicked vertices and Enter, then exported five total GeoJSON features: four original era features and one new scenario territory. Its closed ring contains exactly four positions, and selection reports overlapping underlying/authored claims. Toolbar clicks do not become accidental map vertices.
- Native public-elevation streaming near 47.421° N, 10.985° E loaded nine real-data chunks with zero synthetic fallback. OSM and live weather were intentionally disabled for this renderer check; terrain colors are heuristic visualization, not observed surface imagery.
- Native multiplayer movement reached the authoritative host and moved player 2 approximately ten metres north while another client observed two actors. This review exposed a queue-consumer race that discarded late UI actions; the corrected schedule regression proves a claim and eight craft commands reach both host and replica while preserving the bounded-send backlog. The corrected native UI then successfully claimed a 40-metre plot and crafted planks; the host save confirmed plot 1 owned by player 2 and inventory changing from 30 wood / 0 planks to 28 wood / 4 planks.
- The native client was deliberately suspended for 12 seconds while its host continued running. After resuming, it consumed queued authoritative updates, remained connected as player 2, and retained its property and crafted inventory. The companion TCP regression also checks that a genuinely silent open peer still times out.
- The host was saved and restarted before the final native review, preserving player 2 and plot 1. After the native crafting and stall checks, reopening the client with the same capability restored player 2, plot 1 and the crafted inventory. The host retained exactly the original player IDs `[1, 2]`; reconnecting did not allocate another owner. The final native capture shows server tick 34462 and both actor replicas. Sanitized state evidence is in [native-multiplayer-verification.json](native-multiplayer-verification.json); no capability secrets or private save files are bundled.

Accepted images are in `screenshots/`: world overview, restored camp, current atlas, regional historical atlas, authored territory, minimum window, multiplayer, and real terrain. `review-before-regional-atlas.png` is retained as before/after evidence, not presented as the final interface.

The final multiplayer review used `day_night: false` to keep its visual lighting readable. Its calendar and environmental readouts continued to follow the authoritative server; fixed visual daylight is not a claim that the server's displayed solar angle was used for that capture.

## Reproduce the gates

```bash
./scripts/verify.sh
cargo build -p terra-net --bins
python3 crates/terra-net/tests/process_smoke.py
cargo run -p terra-demo -- --smoke --camp-view --noon --time-scale 0 \
  --radius 1 --frames 160 --screenshot captures/camp.png
```

A native renderer requires a supported graphics device and window system. In this validation environment, a virtual X11 display and Mesa software Vulkan are used for visual inspection; those timings do not represent gaming hardware performance.

No test contacts a public data endpoint unless it is explicitly a live probe. The included CI definition runs local tests and compiles the native demo; it does not download a planet, stress a public API, or claim Internet-scale load coverage.
