# Terra World 0.2 product review and polishing pass

This pass reviewed the native game and the reusable system together: player onboarding, actor control, construction/farming, economic behavior, map fidelity, historical provenance, multiplayer authority, persistence, performance, integration and maintainability. Independent agents implemented separate subsystems and then reviewed their combined behavior. The acceptance evidence is the executable suite, actual native operation, saved-state inspection and screenshots; feature declarations alone were not treated as proof.

## Outcome

The reference game now has a connected loop: select an owned actor in RTS mode, order it to a service or possess it in first/third person, buy or sell finite stock with the same principal inventory and wallet, consume supplies, gather/craft/build/farm, and watch producers and couriers move actual materials through the settlement. The world clock drives environmental conditions and metabolism; physical locomotion remains measured in real simulation seconds. The same rules and commands run in the authoritative host.

The implementation remains a bounded, stylized demonstration and a reusable SDK. It is not presented as a complete planetary digital twin, a photorealistic game, or a production MMO service.

## Significant findings and fixes

| Perspective | Finding | Resolution and evidence |
|---|---|---|
| Player / logistics | Production worked in isolated tests, but every courier stopped at a turn. | The queue check combined a new waypoint direction with an old actor heading. It now compares the desired heading. A turning-queue regression and the real Bevy workflow complete deliveries. |
| Actual map gameplay | The flat fixture passed while the real procedural settlement still delivered nothing. | Nearby roads are ranked before the 32-route bound; routing uses the full bounded graph. Parked idle vehicles no longer masquerade as moving traffic queues. The 49-chunk source-world test completes 4 deliveries and 42 resident purchases after 180 seconds. |
| Multiplayer fairness | The host initially enforced less geography than local play. | A bounded resident-surface adapter now governs terrain support, water, slopes, source buildings, bridges and raised underpasses. Authored structures and fallback facilities participate in swept collision checks. Actual TCP regressions cover hostile placement and physical movement. |
| New player / operator | The new helicopter increased the starter crew to 5 while admission still reserved 4 slots. | Shared capacity constants, prepared crew allocation and atomic commit/rollback prevent partially allocated principals, units and tokens. ID/capacity and seed-failure cases are tested. |
| Returning player | Disconnect could leave an offline body possessed with stale velocity, draining walking energy indefinitely. | Disconnect releases manual control and clears velocity while preserving body/account state. RTS orders continue on the host. Real-socket lifecycle and resume tests cover the change. |
| World operator | Distant players could evict spawn terrain and prevent later joins. | Spawn remains in the CLI's bounded source interest union. A residency regression retains the admission surface. |
| Data integrator | Duplicate imported road IDs could pass complete-state validation but fail public-view validation. | Identical segments deduplicate; distinct segments receive stable bounded identities. Validation and snapshot tests cover reusable provider/historical inputs. |
| Data fidelity | Physical road width normalization could be discarded during living-world seeding. | Normalized source width is retained when a raw tag uses physical units. The host also accepts centimetre-tagged raised underpasses consistently. |
| Replica consumer | Partial living views could silently omit roads/sites, and faster pose updates could erase their truncation flag. | All supported reductions mark incompleteness. Actor and sandbox completeness are tracked independently and combined across their different update rates. A mixed-message regression checks both directions. |
| Player / error recovery | Rejected possession and invalid RTS input wrote an internal status that no UI displayed. | The HUD publishes changed control status, while domain/server outcomes retain precedence. Ownership text distinguishes a controllable actor from an inspected NPC. |
| Player / pause state | Pausing with the World drawer closed could look like broken movement. | The footer gives a persistent pause/resume hint. |
| Airborne player | Horizontal market distance could imply eligibility while 3D reach checks rejected trading. | Market display, interaction feedback and production management use spatial distance. |
| Production manager | Enabled sites were always called running, even when blocked on input or storage. | The inspector reports working, missing inputs, depleted source, full storage, disabled production or paused time, and explains management reach. |
| Visual usability | Long labels wrapped awkwardly, and translucent panels revealed moving geometry behind text. | Shorter mode/quantity labels and opaque surfaces improve scanning and contrast. The minimum window retains bounded scrolling instead of allowing content to overlap the footer. |
| Source honesty | An imported sample heightmap could appear to be observed Munich terrain. | The included fixture is labeled synthetic in its manifest, source records, documentation and visible source banner. Authored economy/people remain distinct from mapped features. |

## Player and creator experience

The main interaction model is explicit. F1 gives RTS control; click, Shift selection, drag selection and group commands operate owned bodies. F2 and F3 request possession through the domain and wait for confirmation. WASD movement is camera-relative; R/F controls aircraft altitude. Release preserves the current active body, so trading/building reach does not jump to another avatar. Explicit geographic camera navigation releases control before moving the view.

The inspector separates Command, Build, Market, Industry and Factions tasks. The World drawer and inspector can be hidden independently, leaving more landscape visible. The interface shows the player ID needed by faction invitations. Existing housing, workbench access, plot/faction roles, gathering, farming, destruction, saves and the atlas remain available alongside the new controls.

Native operation verified physical movement, mode switching, a market movement order and an actual bread purchase. Saved state confirmed player identity remained 1 and the wallet changed from 250.00 to 247.50 credits while inventory gained one bread. The market exposed the matching buy/sell prices, shop stock, own quantity and till. The player did not receive a separate inventory merely by possessing a worker or vehicle.

The final multiplayer pass repeated possession and the purchase as player 2.
The authoritative host retained the same owned worker, 247.50 credits and one
bread after closing and reopening the native client. The Industry panel showed
12 completed deliveries and a producer waiting for inputs; the next minimum-size
capture showed replenished inputs, resumed work and 13 completed deliveries.
This confirms that the visible state follows the authoritative economy. The
[native verification record](native-v2-verification.json) includes the selected
state evidence and explicitly records its smaller 9-chunk visual working set.

## Graphics, geography and provenance

The default requested neighborhood grew to 49 chunks with 65×65 heightfields. Smooth indexed meshes, shared normals, UVs/tangents and authored ground-detail textures improve terrain presentation. Source tags affect building height/parts, roofs, windows, colors/materials, shops, road width/lanes/bridges, water, fields, orchards and tree types. Rendering has finite feature budgets, asynchronous mesh construction and hysteresis between detail tiers. Full terrain remains present in every tier.

The default scene regression measured 1,464,403 source vertices. That is a bounded geometry result, not a claim of a particular frame rate on gaming hardware. Actual native captures show the wider RTS landscape, first/third-person body visibility, a heightmap settlement with draped roads, and the revised political atlas.

The PNG importer preserves declared WGS84 bounds, elevation scaling and source attribution, validates regular contained files, and refuses unsupported coverage instead of silently inventing missing elevation. An elevation-only import does not imply observed buildings, roads or businesses; its fallback game settlement is authored. The bundled heightmap and dated example packs remain explicitly fictional. Current Natural Earth country data retains capture-date and boundary-policy text.

## Economy and simulation integrity

Money uses integer minor credit units. Transactions validate quantity, price direction, stock, funds, reach and overflow before committing. Wallets and tills are finite; production consumes actual inputs or finite extraction reserves. Couriers collect stock into cargo and deposit it at their destination. Production may stop when deliveries or resources are unavailable. Tests verify that failed commands remain atomic and materials/money are not silently duplicated.

The actual-map test was the decisive integration gate: with loaded roads/buildings/farms/forests and collision enabled, 47 bodies and 7 sites produced 626 cycles, 4 completed deliveries and 42 resident purchases in the recorded 180-second run. It exposed defects that the initial isolated economy tests and flat-terrain integration did not reveal.

Needs are attached to bodies, while inventory/wallet/permissions belong to the principal. Food, water and energy influence movement limits. The host clamps otherwise valid delayed movement intent against current needs, avoiding false rejection when exhaustion changes between input and processing.

## Reuse, authority and operations

Pure geographic, streaming, history and sandbox/economy crates remain independent of the renderer. `TerraLivingPlugin` is opt-in for embedding applications, and its settings can disable reference population seeding or advancement. Games can provide their own data provider, population, renderer, catalog and environment resolver. The domain modules were split by responsibility rather than kept as one large economy implementation.

Clients submit intent. The host owns mutation, collision, interaction reach, identity, sequencing and persistence. A public partial replica is not a writable complete save. Views are bounded under the transport frame limit, privacy-sensitive account data is scoped to the requesting player, and capability-based resume retains the principal across reconnects. The new living state participates in the existing atomic save/restore envelope.

The independent host/client process runner passed procedural two-client operation, restart/resume, a matching historical/political session, mismatched world rejection, invalid historical configuration and special-file input checks. The final source verification has 168 distinct passing tests; see [VALIDATION.md](VALIDATION.md) for the counts and commands.

## Native visual acceptance

The independent reviewer inspected the real application at 1440×900 and 1100×700. No blocking overlap or unreadable primary controls were found in the reviewed Command, Market, Industry, heightmap and atlas flows. The source banner, mode controls, wallet, quantities, numeric needs, source attribution and navigation guidance remain visible. Overflow stays inside scrollable panels.

The completed-purchase capture has state evidence behind it. Earlier Industry images with zero deliveries are retained only as review-before evidence; the final release image uses the corrected simulation. Early screenshots also lagged the issued action under software rendering, so final images are labeled by their actual visible mode.

The final [RTS overview](screenshots/v2-rts-overview.png) confirms all 49 chunks
rendered, five selected owned bodies and the persistent pause hint. The final
[heightmap capture](screenshots/v2-heightmap-settlement.png) confirms the synthetic
source banner and terrain-draped roads. The independent reviewer accepted these
along with the final opaque-panel possession, trade, logistics and minimum-size
captures without requesting another blocking change.

Accessibility was partially reviewed for visual contrast, text/numeric cues, viewport exclusion and the minimum window. Keyboard-only traversal, screen readers, user UI scaling and color-vision testing remain unverified. The intended accepted target is a mouse-and-keyboard desktop demonstration.

## Current limits

- The default world, population and economic behavior are authored simulation content. Public inputs have bounded coverage and separate update/licensing policies. A successful live Overpass fetch was not observed in this environment; its local HTTP and semantic paths are tested.
- Movement is a kinematic surface controller with bounded routing, following and local obstacle handling. It is not a complete navmesh, signal/lane-change system, rigid-body vehicle model or aerodynamic simulation. RTS aircraft orders are horizontal; direct possession provides altitude input.
- Prices are fixed reference prices. There are no wages, taxation, dynamic price discovery or comprehensive national economies. Production ownership is personal; existing factions govern shared plots/housing/farms/workbenches.
- One reference settlement is seeded around the scenario start. Continuous regional population generation, ecology and economic partitioning across the planet are hosting-game extensions.
- Source geometry is immutable input. Mutable construction, crops, resources and supported source-feature edits are an overlay; arbitrary sourced-building fracture is not implemented.
- Multiplayer is bounded single-process TCP for trusted local development/LAN use. Public accounts, encrypted production transport, predictive movement and distributed authority remain host concerns.
- The graphical atlas is a flat geographic view. Source claims and authored faction territories do not arbitrate sovereignty. Current captures are not historical truth, and missing past data is not synthesized as fact.
- Save envelopes pin world/source/catalog assumptions. The v2 procedural revision deliberately rejects incompatible v1 native-world geometry; deliberate migration is required for such worlds.

The [earlier product review](PRODUCT_REVIEW_v0.1.md) remains available as historical evidence for the foundation. It is not used to claim that the new workflows were already verified in that release.
