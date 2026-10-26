# Terra World 0.2 validation record

Validation date: 2026-10-26. Rust 1.99.1, Bevy 0.20.0, Linux. The native review uses X11 and Mesa software Vulkan; its timings are not gaming-hardware benchmarks. Windows, macOS, WebAssembly, mobile and public-Internet deployment were not exercised.

## Automated gates

| Suite | Distinct passing tests | Coverage |
|---|---:|---|
| Core | 12 | Geographic round trips, poles/date line, clocks, solar/seasons, actors and environment contracts |
| History | 13 | Dated packs, missing coverage, provenance, file bounds, political geometry and bundled scenarios |
| CLI | 4 | Parsing, deterministic simulation and existing complete sandbox workflow |
| Stream | 20 | 13 unit, 6 semantic/provider fixtures, 1 real local HTTP/cache integration; PNG heightmaps and OSM tags included |
| Sandbox | 40 | 7 faction, 13 established gameplay and 20 living-world tests |
| Network library / host / TCP | 35 | 17 library, 1 CLI spawn-residency and 17 actual loopback tests |
| Bevy library with rendering | 28 | Includes 20 shared/headless tests and 8 geometry/render-specific checks |
| Bevy integration | 8 | 6 established engine tests, 1 complete living-world workflow and 1 actual procedural-map settlement |
| Native demo | 8 | Camera vectors, control transitions, viewport/hit testing, road draping and reachable gathering |
| **Total** | **168** | Repeated runs and tests shared by headless/render features are counted once |

The complete domain/network gate passes 75 tests together. The renderer-enabled Bevy/demo gate exercises the same public interfaces used by the desktop application. A test that intentionally creates an unresponsive peer prints a timeout diagnostic while passing; it checks genuine silence separately from queued-frame recovery.

### Integrated gameplay proof

`crates/terra-bevy/tests/living_world.rs` creates an actual Bevy app, streams terrain, seeds the population, buys bread, possesses another owned body, moves it, releases it, changes the active RTS body, issues a terrain movement order, advances production and courier deliveries, and restores exact state from a saved world. Principal identity, inventory, cash, active-body position and full-state validity are checked.

`crates/terra-bevy/tests/procedural_settlement.rs` adds the actual map and physical constraints that a flat fixture cannot establish:

| Configuration / result | Observed value |
|---|---:|
| Region / seed | Munich coordinates / 42 |
| Resident source chunks | 49 |
| Heightfield resolution | 65 × 65 per chunk |
| Population | 47 bodies |
| Markets / production sites | 1 / 7 |
| Movement duration / fixed step | 180 seconds / 20 Hz |
| Calendar scale | 60× |
| Production cycles | 626 |
| Completed physical deliveries | 4 |
| Resident purchases | 42 |

Loading is paused before advancing the scenario. Source roads, buildings, forests and farms remain present, and the normal physical resolver stays enabled. The test asserts successful productive activity and physical deliveries, not an exact economic balance forever. These counts describe the recorded run, not a throughput benchmark or a promise for arbitrary imported geography.

The first actual-map run failed with zero deliveries. The final fixes rank nearby roads before applying the route budget, use the full bounded road graph, compare desired heading at turns, and distinguish parked vehicles from moving traffic. Focused regressions retain the failure cases.

### Graphics budget

The default 49-chunk mesh regression measured **1,464,403 source vertices**, with 6 near, 20 middle and 23 far chunks. The assertion keeps this fixture below two million source vertices. This count includes terrain and source features; independently cached actors and player structures are separate. Full heightfields are retained at every detail tier. LOD hysteresis and asynchronous mesh jobs are exercised.

## Independent executable checks

`crates/terra-net/tests/process_smoke.py` ran the freshly built host and client binaries through six passing groups:

1. Procedural host with two independent clients.
2. Save/restart with both identities and inventories preserved.
3. Historical world pack with its matching political dataset.
4. Rejection of mismatched epoch, pack and political revision.
5. Invalid historical startup/resume configuration and missing coverage.
6. Rejection of FIFO/special-file inputs before they can block startup.

A separate 25-tick CLI server smoke opened an actual loopback listener and atomically saved a validated world with 42 NPC bodies, seven production sites, one shop and seven completed production cycles.

## Native review

Actual native captures were inspected at 1440 × 900 and at the declared 1100 × 700 minimum. This review covered the expanded RTS scene, first- and third-person cameras, the market, production, a georeferenced heightmap, and the political atlas. It did not substitute screenshots for command/state assertions.

A native local session verified these actions with the real UI and save output:

- F2/F3 changed possession view; first person hid the controlled exterior body.
- WASD changed the controlled actor's saved GPS position while keeping player ID 1.
- Ordering the actor to the market released direct possession and created a movement order on the same body.
- Buying one bread changed the wallet from 25,000 to 24,750 minor credit units and added one bread to inventory. The visible market showed 247.50 credits and the enabled sell action.
- F5 saved the body, account, inventory, production and logistics state.

The final multiplayer check used the freshly built native client and CLI host.
Two player accounts admitted 52 bodies. Native player 2 selected and possessed
an owned worker rather than the original avatar, changed its host-validated GPS
position with WASD, switched between first and third person, and bought one
bread through the Market UI. The host saved 24,750 minor credit units and one
bread on that same principal. The Industry capture shows 12 completed physical
deliveries and a farm waiting for seed inputs; the minimum-window capture shows
13 deliveries and the farm working again after replenishment.

Closing and reopening the native client with its original session identity
preserved player 2, worker ID 1000063, position, wallet and inventory. Disconnect
correctly released manual possession, so the resumed client began in RTS mode.
The later host sample recorded 14 completed deliveries and 79 resident purchases.
This visual run used a bounded 9-chunk neighborhood; the 49-chunk default is
covered separately by the procedural integration and native landscape capture.
Selected state samples and checked assertions are retained in
[native-v2-verification.json](native-v2-verification.json); private session
capabilities and host saves are excluded.

The native renderer in this environment can take several seconds to present a frame while compilation is also running. Early capture filenames were therefore corrected according to the mode actually visible; a command issued immediately before an image is not evidence that the next frame was already presented.

The final UI fixes include explicit control errors, ownership labels, a visible pause state, consistent spatial interaction distance, production waiting states, shorter labels, and opaque panels. See [the product review](PRODUCT_REVIEW.md) and the accepted captures in `screenshots/`.

The final local overview shows all 49 chunks rendered with zero pending chunks,
47 bodies and all 5 owned bodies selected. Its footer explicitly reports the
paused state, and the native save command completed. This separately confirms
the wider default working set, RTS group selection and pause feedback.

### Accepted native captures

These are unedited captures of the native application. The included terrain and
settlement are authored synthetic content; the political atlas has its own
source attribution. The UI was also independently reviewed after the final
polish fixes.

| View | Evidence |
|---|---|
| Wider default world and RTS selection | [49-chunk overview](screenshots/v2-rts-overview.png) |
| Georeferenced PNG terrain | [Heightmap settlement](screenshots/v2-heightmap-settlement.png) · [Wider relief](screenshots/v2-heightmap-overview.png) |
| Owned-body possession | [Third person](screenshots/v2-third-person.png) · [First person](screenshots/v2-first-person.png) |
| Authoritative trade | [Bread purchase](screenshots/v2-market-purchase.png) |
| Production and physical cargo | [Industry and logistics](screenshots/v2-industry-logistics.png) |
| Small-window layout | [1100 × 700](screenshots/v2-minimum-window.png) |
| Native resume | [Same player, body and wallet](screenshots/v2-multiplayer-resume.png) |
| Political geography | [Attributed country atlas](screenshots/v2-political-atlas.png) |

The final heightmap capture shows 49 rendered chunks with zero pending chunks,
draped roads and the explicit “SYNTHETIC HEIGHTMAP” banner. The final default
landscape, heightmap, possession, purchase, production and minimum-window images
were independently accepted with no remaining blocking visual finding.

## Public data and provenance

The retained strict public elevation/weather probe from 2026-10-26 is recorded in `crates/terra-stream/verification/live-probe-2026-10-26.json`: real Munich elevation samples ranged from 519.889 to 530.212 m and the weather sample was not marked synthetic. This establishes HTTPS decoding and units, not scientific accuracy.

A successful public Overpass download was **not** observed. The v0.2 alternate-endpoint attempt timed out after 20.206 seconds and is recorded in `crates/terra-stream/verification/overpass-v2-2026-10-26.json`. OSM semantic fixtures and the actual local HTTP/cache path pass. Public-data rendering must continue to expose source/fallback status rather than calling generated scenery observed data.

The bundled PNG heightmap is explicitly synthetic. It verifies georeferenced 8/16-bit elevation import and terrain draping, and is not the terrain of Munich. The historical examples are fictional era packs. The bundled Natural Earth countries are a current generalized source capture, not an attested historical border reconstruction.

## Reproduce

```sh
./scripts/verify.sh
cargo build -p terra-net --bins --locked
python3 crates/terra-net/tests/process_smoke.py
cargo run -p terra-demo --locked -- --camp-view
cargo run -p terra-demo --locked -- --config configs/heightmap.json
```

The procedural integration test is part of the normal Bevy integration suite and performs 3,600 fixed updates. Socket tests require a localhost-capable execution environment. Public endpoints are contacted only by explicitly requested live probes or `--live` sessions.

The earlier release's complete evidence is retained in [VALIDATION_v0.1.md](VALIDATION_v0.1.md); its screenshots and counts describe v0.1 and are not substituted for v0.2 checks.
