# Terra World 0.3 validation record

Validation date: 2026-10-26. Rust 1.99.1, Bevy 0.20.0, Linux. Native review uses X11 and Mesa software Vulkan. Its render/step timings are not gaming-hardware benchmarks. Windows, macOS, WebAssembly, mobile, public-Internet hosting and distributed MMORPG load were not exercised.

## Automated gates

| Suite | Distinct passing tests | Coverage |
|---|---:|---|
| Core | 12 | Geographic round trips, poles/date line, clocks, solar/seasons, actors and environment contracts |
| History | 13 | Dated packs, missing coverage, provenance, file bounds, political geometry and bundled scenarios |
| CLI | 4 | Parsing, deterministic simulation and the established complete sandbox workflow |
| Stream | 20 | 13 unit, 6 semantic/provider fixtures, 1 real local HTTP/cache integration |
| Sandbox | 77 | 7 faction, 13 established gameplay, 20 living, 14 ecology, 18 settlement and 5 logistics tests |
| Network library / host / TCP | 41 | 17 library, 1 spawn-residency, 17 actual loopback and 6 frontier migration/admission/compatibility tests |
| Bevy library with rendering | 33 | Shared environment/network tests plus terrain, wildlife, resource, ground-cover and territory geometry |
| Bevy integration | 11 | 6 established engine tests, complete living and procedural-map flows, complete frontier flow, 2 real framed native-replica fixtures |
| Native demo | 10 | Controls, cameras, road draping, reachable gathering, partial-upgrade UI and final public-works contribution |
| **Total** | **221** | Shared feature tests and repeated runs counted once |

All 118 domain/network tests pass. The final native demo builds successfully, and both finite headless integration examples run successfully. `cargo fmt --all -- --check` is the formatting gate in the reproduction script.

### Complete engine scenarios

`frontier_world.rs` runs actual Bevy schedules from streamed terrain and population initialization through ecology behavior, berry gathering, named territory founding, supply acquisition, contract acceptance, physical-context fulfillment and exact save/restore. It checks inventory, depleted stock, conserved founding funds, receiving-site goods, reward money, progression and cross-layer validity.

`living_world.rs` retains the full buy → possess another owned body → move → release → RTS order → production/delivery → exact restore workflow. Its trade assertion reads the authoritative current quote, so scarcity pricing cannot silently turn a fixed-price test into a false failure or weaken the exact debit check.

`procedural_settlement.rs` uses the actual source provider, source roads/buildings/land use, resident terrain, physical movement resolver and 3,600 fixed updates. Loading is paused before advancement to prevent worker scheduling from ageing the initial state.

| Recorded procedural scenario | Observed value |
|---|---:|
| Coordinates / seed | Munich / 42 |
| Resident source chunks | 49 |
| Heightfield resolution | 65 × 65 per chunk |
| Fixed movement time / rate | 180 seconds / 20 Hz |
| Calendar scale / elapsed calendar | 60× / 3 hours |
| Living bodies | 107 |
| Managed wildlife | 64 |
| Ecology resource patches | 180 |
| Markets / production sites | 1 / 7 |
| Funded supply requests | 16 |
| Production cycles | 627 |
| Completed physical deliveries | 6 |
| Route replans | 2 |
| Resident purchases | 42 |
| Simulation errors | None |

Those counts describe the recorded deterministic fixture and are not a throughput promise for arbitrary imported regions. Resident pay was zero during this short scenario because the pay threshold was not reached; separate finite-pay tests exercise eligible residents, production gating and treasury conservation.

### New behavioral, conservation and authority regressions

Ecology tests cover five species, height sampling, water access, fleeing, rest, grounded births, population recovery, carrying capacity, no refill on source reload, invalid inputs, bounded view coherence, old-field defaults and custom equipment catalogs. The shoreline fixture gathers actual fish with a rod and proves that unsupported rod/tool paths do not seed unreachable deposits.

Settlement tests execute both neighborhood→village and village→city upgrades through real contribution, Build and Plant commands. They also check founder/faction authority, plot capacity/inheritance, regional gathering, expansion overlap, stock and money conservation, private journals, deadlines/reopening, fulfill replay rejection, scarcity anchors, finite pay and save validation. Logistics fixtures exercise source reservation, blocked-pickup cancellation and physical loaded-cargo return.

Six host fixtures check exact prior catalog recognition, preservation of customized catalogs and existing player/facility state, additive ecology on old living saves, elevated-terrain grounding before placement checks, source exclusion and delayed initialization until terrain is available. A raw protocol-1 Hello is rejected and disconnected before any player, capability, actor or saved command sequence is allocated; current peers use wire protocol 2. Initial market maintenance may change current quotes and move till money into escrow; the tests compare immutable quote anchors and total till+escrow, not incorrect whole-shop equality.

Two native network-adapter fixtures send real framed TCP messages. Valid owned wildlife is accepted; 40 client frames do not advance host ecology, economy or progression. Missing animal bodies, mismatched resource IDs/items and another player's progression each reject atomically.

The UI regressions retain known material/XP/treasury/permission checks while allowing the host to resolve incomplete building counts, and verify that the final eight-tool city requirement accepts a remaining three-tool contribution after five were already donated.

### Graphics integrity and budgets

The default 49-chunk source mesh fixture measures **1,650,487 vertices**, with 6 near, 20 middle and 23 far chunks, below the two-million assertion. Full heightfields remain at every source-detail tier. Source vertices exclude separately cached bodies, player overlays and nearby detail.

All five species and twelve pose states have finite, bounded, grounded geometry. The first pose gate found an ibex grazing beard below the ground; that geometry was corrected. Additional checks cover date-line material continuity, resource depletion stages, ground cover avoiding water/unknown terrain, and settlement lines draped only over resident heights. Ground cover and species poses use separately documented finite caches/budgets.

## Independent executable checks

Fresh v0.3 host/client binaries passed all six real-process groups:

1. Two simultaneous procedural-world clients.
2. Save/restart with both identities and command sequences retained.
3. Historical world pack with matching political data.
4. Rejection of mismatched pack, epoch and politics revisions.
5. Invalid historical startup/resume configuration rejected.
6. FIFO/special-file inputs rejected before they can block startup.

The host/client tests use actual local sockets. A sandbox-only first attempt could not bind loopback; the same gates passed in a localhost-capable execution environment. No rejected or unexecuted socket test is counted as a passing result.

## Native interaction and visual review

The first native batch verified the 49-chunk world with 107 bodies and 64 managed animals, all chunks rendered and no pending source work. It exercised Activities, Nature and Territory through the actual UI. It then saved and resumed that world with the rebuilt polished executable. The interaction confirmation uses a bounded 9-chunk renderer neighborhood while retaining the saved authoritative population and geography.

Observed transaction evidence includes a real travel order to a berry patch, five gathered berries with patch stock changing from 36 to 31, and the creation of **Willow Reach** as a 100 m neighborhood. Founding changed the wallet from 250.00 to 200.00 credits, placed 50.00 credits in its treasury, consumed 20 wood and 10 stone, and recorded 10 community experience. The active actor followed a real RTS order to the supply destination while the player inspected wildlife elsewhere. Delivering four wood removed the offer, changed carried wood from 10 to 6, paid its pre-funded 6.13-credit reward, and raised community experience from 10 to 50 with one completed contract. A later berry consumption changed carried berries from five to four and restored the active body’s needs.

The initial consume attempt correctly rejected already-satisfied food and retained inventory; the final UI disables such actions and displays active-body needs. Native review also prompted balanced ambient daylight/exposure, a complete cooked-fish/water flow, precise public-works remainders, named logistics endpoints, received-count labels and partial-view disclosure. Captures document the final confirmation; command success is checked with saved state rather than inferred from a click or screenshot alone.

The [sanitized native verification record](native-v3-verification.json) records those saved-state checks. Current captures show [activities](screenshots/v3-activities-overview.png), [resource use](screenshots/v3-nature-resources.png), [wildlife](screenshots/v3-wildlife.png), [territory management](screenshots/v3-territory-charter.png), [industry and logistics](screenshots/v3-industry-logistics.png), [third-person](screenshots/v3-third-person.png), [first-person](screenshots/v3-first-person.png) and the [1100 × 700 minimum window](screenshots/v3-minimum-window.png). The final [native multiplayer capture](screenshots/v3-multiplayer-nature.png) also confirms a protocol-2 graphical client connected to the fresh host, with 64 bodies and 28 animals in its received snapshot at the recorded tick. A separate [final heightmap overview](screenshots/v3-heightmap-landscape.png) shows all 49 heightmap chunks rendered with zero pending work, 107 bodies and 64 wildlife. Its approximately 0.21 million source vertices reflect an elevation-only fixture with an authored settlement; it is separate from the procedural source-feature budget above. Private session capabilities, complete host saves and build caches are excluded from the source archive.

## Source-data scope

This pass makes no new claim of a successful public Overpass download. The retained strict public elevation/weather result and the explicit Overpass timeout remain in the provider verification directory and [VALIDATION_v0.2.md](VALIDATION_v0.2.md). Local OSM semantic and real HTTP/cache fixtures pass.

Default terrain, ecology, resources and economy are authored simulation content. The bundled raster is explicitly synthetic, the historical examples are fictional era packs, and the Natural Earth capture is current generalized political geography. A map forest or shop location does not make its simulated wildlife, yields, staff or stock observed fact.

## Reproduce

```sh
./scripts/verify.sh
cargo build -p terra-net --bins --locked
python3 crates/terra-net/tests/process_smoke.py
cargo run -p terra-demo --locked -- --camp-view
cargo run -p terra-demo --locked -- --config configs/heightmap.json
```

`verify.sh` runs the whole workspace gate, renderer/native tests and the two finite headless examples. Socket tests require local TCP capability. Public endpoints are contacted only by explicit live/probe modes. Earlier evidence remains in [VALIDATION_v0.2.md](VALIDATION_v0.2.md) and [VALIDATION_v0.1.md](VALIDATION_v0.1.md), with their release-specific counts and captures.
