# Terra World 0.3 product review and polishing record

Review date: 2026-10-26. This review covers the reusable Rust/Bevy framework and its native reference game. Independent ecology, settlement/economy, graphics and product reviewers worked in parallel, followed by root integration, actual-engine tests and native interaction review. Prior release evidence remains in [PRODUCT_REVIEW_v0.2.md](PRODUCT_REVIEW_v0.2.md).

## Product outcome

The new reference loop connects natural resources, equipment and food, real market inventory, funded supply work, player progression, housing/farms, and named territory upgrades. Wildlife uses shared living bodies and movement rules. Resource depletion, economic shortages, deliveries and regional rights are persistent state visible to the player. The source terrain, imported politics and authored gameplay remain distinct.

This release advances a reusable MMORPG foundation. It does not establish production AAA presentation or MMORPG service scale. [MMORPG_FOUNDATION.md](MMORPG_FOUNDATION.md) maps the existing extension seams and the concrete infrastructure, content and performance work still required.

## Review by product perspective

| Perspective | Finding and implemented response | Evidence |
|---|---|---|
| First session and player motivation | A larger world needed a clear purpose. Activities now opens first, explains the next step, and offers funded requests tied to production or retail shortages. | Native Activities flow; complete Bevy frontier integration |
| Wildlife and exploration | Previous animals were mostly decorative. Five species now have habitat, behavior, disturbance, water, rest and bounded recovery metadata on actual moving bodies. | 14 ecology tests; grounded pose tests; native observation |
| Gathering and survival | Resource kinds needed distinct consequences. Finite ore/stone and renewable food/fiber/fish/water share real inventory; tool admission rejects atomically. Food actions expose need levels and do not waste items when satisfied. | Ecology/custom-catalog tests; native berries 36→31 and inventory +5 |
| Crafting and commerce | Cooked fish lacked a native consume action, and the market omitted iron tools/stone blocks. The Nature/Market controls now cover those goods and processing paths. | Final native control build; domain consumable/trade tests |
| Regional ownership | Plot ownership alone could not express a community. Named neighborhood/village/city claims now govern expansion, plot capacity, gathering and construction, with current faction roles. | 18 settlement tests; native named charter and treasury transfer |
| Progression completeness | Fixed five-item UI donations could block the final three tools of a city project. Contributions now clamp to inventory, five units and the project's remaining requirement; irrelevant/finished materials are disabled. | Native command regression applies the remaining three tools |
| Economy and incentives | Unfunded rewards and endlessly repriced goods would break conservation. Work reserves escrow, prices use saved anchors, and resident pay comes from finite market money after new production. | Escrow, deadline, price-anchor, wage and round-trip conservation tests |
| Logistics and failure recovery | Couriers could compete for source stock or strand goods without useful feedback. Dispatch reserves inputs, prioritizes shortages, retries routes and physically returns undeliverable cargo. UI shows named endpoints, waiting, retry and return totals. | Five logistics regressions; actual-map physical deliveries; native Industry |
| Graphics and spatial readability | New geometry disappeared into overly dark daytime shadows. The final pass balances indirect daylight and exposure, retaining directional light and material variation. Species poses, depletion stages, nearby grass and draped charter markers add state-dependent detail. | Native before/after review; 33 renderer-enabled engine tests |
| Controls and interaction context | Inspecting wildlife must not change the account used for trade/gathering. The UI explains inspect-only selection and preserves the active owned body across RTS/FPS/TPS. Name submission closes on the first confirmation. | Control regressions and native interaction checks |
| Multiplayer experience | Bounded snapshots can omit houses/farms, incorrectly disabling valid upgrades. Partial views are disclosed; the UI leaves incomplete building counts to host validation while retaining known costs, reach and permission checks. | Native partial-replica upgrade regression; malformed-view loopback tests |
| Persistence and compatibility | Adding ecology to earlier living saves must not move existing players, reset stock or replace custom policy. Exact previous builtins are recognized; custom catalogs and existing state remain intact. | Five host compatibility/admission tests; actual save/reload |
| Engine integration | A composed domain lifecycle must include crop/resource advancement, not only living/ecology updates. The integration guide now states the complete order and required weather/ground callbacks. | Independent API/documentation review; finite executable examples |
| Performance and scaling | Richer source meshes and more animals require visible budgets. The reference keeps bounded tasks, cached poses, batched ground cover, finite populations and explicit view truncation; road routing uses adjacency lists and a priority queue. | 49-chunk source geometry below two million vertices; documented hard limits |
| Data provenance | Map-derived habitats and service locations are not evidence of real populations, mineral deposits or business stocks. Native source labels and guides retain the authored/source distinction. | Source records, semantic/provider fixtures, fictional historical labels |

## Concrete authority defects closed during review

1. Ecology placement now grounds candidates before altitude-sensitive admission and checks player/facility footprints. Valid hillside terrain can seed without allowing births inside buildings.
2. Loaded earlier living worlds receive idempotent additive ecology even when the host does not run initial living-demo seeding again.
3. Replicas cross-check animal metadata against visible animal bodies and resource metadata against visible matching nodes. Valid owned animals remain accepted.
4. Catalog migration recognizes exact v0.1 and v0.2 reference definitions; a customized catalog is not silently rewritten.
5. Faction-territory plots inherit the regional faction. Cross-boundary or unrelated-faction transfers cannot create an ownership bypass.
6. Regional gathering checks use current membership; permanent source-tree edits retain builder-level authority and existing plot ownership checks.
7. Abandoned accepted contracts reopen on persisted world-time deadlines without duplicating or losing reward escrow.
8. Returning couriers keep both actual cargo and its loaded visual until physical return succeeds.
9. The wire protocol is now version 2. Legacy clients fail at handshake before identity allocation, instead of receiving new delivery phases they cannot decode. Saved-world compatibility remains a separate migration policy.

## Native review method

The first review inspected the actual native window, then exercised navigation to a resource, gathering, attempted consumption, named territory creation, accepting supply work and saving. One combined polishing pass addressed readability and interaction findings. The final confirmation uses the rebuilt native executable at 1440×900 and 1100×700, with saved-state assertions in addition to images. Rendering uses Mesa software Vulkan, so these captures are visual/interaction evidence rather than gaming-hardware performance measurements.

The complete release counts, accepted captures and observed transactions are recorded in [VALIDATION.md](VALIDATION.md) and its linked native verification record. Earlier-release screenshots are labeled separately and are not substituted for current behavior.

## Deliberate next production work

A shipping MMORPG needs prepared and licensed geographic tiles, a production art/animation/audio pipeline, adaptive terrain/visibility systems, full navigation/physics, authenticated encrypted transport with prediction/reconciliation, durable regional state, migrations and account services, distributed simulation ownership, observability and hardware/load testing. Combat/hunting, conquest, taxation, interiors, physical terrain excavation, a complete food web, lane-level traffic and scientific economic equilibrium remain outside this bounded reference release.

Those are explicit extension projects, not hidden claims in the current demo. The implemented systems provide tested data and authority seams on which a game can build them.
