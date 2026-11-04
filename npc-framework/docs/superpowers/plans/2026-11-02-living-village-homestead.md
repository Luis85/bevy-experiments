# Living Village expansion and homestead implementation plan

Date: 2026-11-02. Continue the verified needs-driven village with a modestly larger island, clearer economic districts, smoother NPC routines and a small playable homestead loop. The user selected clear trade districts and prepared house/garden plots. The established shared player/AI action API is the control surface for this increment.

## Working boundaries

- Use the shared checkout and preserve existing changes, protected oracles and the integer simulation/rendering boundary. No dependency upgrades or generic simulation-core changes. The separate generic tooling receipt-priority correction is documented in ADR 0033.
- Coordinate with the separate authenticated client playtest. Its release executables, assets and GPU session remain reserved until its owner reports cleanup. This task starts no provider turn and reads no provider conversation history.
- Keep one logical NPC Cargo chain, with at most two builders across the checkout. Source ownership: geography/construction and homestead backend; NPC routines; browser observer; primary owns metadata, projection/native integration and final verification.
- Preserve fixed-world behavior and incumbent identities. Saved presets without a geography profile remain compact. New expanded worlds explicitly save their construction profile. Install a new `living_homestead` preset additively so existing user world files are not overwritten.
- Complete focused phase-A checks before enabling phase B. Reserve the bounded visual inspection batch for the finished combined implementation, with at most one correction/confirmation round. Run the full Rust gate on the final frozen source.

## A. Expand and polish the village

1. Introduce shared integer geography used by shoreline navigation, native rendering and the exported browser terrain. Expanded radii are 30×25 metres versus 24×20: about 56% more land within the existing 64-cell grid. Keep compact geography exact.
2. Preserve the town, homes, workstations and all incumbent actor/item identities. Reuse the original shoreline IDs and append extra boundary segments last. Move resources and pasture into accessible outskirts, with connected roads and three factual district signs.
3. Prepare six clear 6×6 metre sites: four house plots along the southern lane and two northern gardens. Check corners, clearance and actual navigation to every source, workplace, home and plot. Planned land alone creates no goods or ownership.
4. Improve real NPC decision failures using causal regressions: deliver a ready financed product before shopping for buffer inputs; recognize feasible water/berry subsistence before optional night/recovery routines; use the same 1.5-metre work/trade reach as clients while preserving the 0.8-metre home boundary.
5. Polish both views without replacing their visual language: district/plot readability, friendly good names, compact inventory totals, truthful market and connection states, safe framing and inspector navigation.

## B. Give the player a small homestead

1. Add real saved/hashed plots and a shared recorded `HomesteadAct` surface. Living actors use identical ownership, reach, material, capacity, cancellation and life rules. NPCs do not claim farms automatically.
2. Let an actor claim one house plot and one garden. A living owner's property cannot be taken; a deceased owner's physical estate can be reclaimed. No free seeds, materials or money accompany a claim.
3. Build a small cottage over time using eight planks and four logs. Reserve exact real inputs, spend only on successful completion and release claims on cancellation/death. Add the physical house footprint and set the owner's reachable home without teleporting the actor.
4. Provide three beds beside a cottage site and six at a garden. Plant one real grain, water with one real clean water and grow hardy grain over 6,000 simulation ticks. Harvest three grain only when storage fits, then clear the bed atomically. Keep ready crops and growth timestamps through save/resume.
5. Connect the harvest to ordinary milling, baking, eating, market offers and physical NPC trading. The homestead supplies food/materials through existing economic rules; it is not a separate gold dispenser.
6. Publish perception-scoped plot affordances and honest owner/cottage/crop feedback. Both observers draw the authoritative plot state; the browser remains read-only. AI continuation receives closed claim/build/crop outcomes without private text.

## C. Final improvement and closeout pass

- Check the complete new-world → discover/claim → gather/craft → build → plant/water → mature/harvest → cook/eat or sell → save/resume loop through registered public tools. Privileged reads may verify results but cannot choose the reference actor's actions.
- Cover owner contention, unreachable/unknown plots, missing/reserved materials, busy work, cancellation/death, exact completion times, no double harvest, full inventories, home navigation, currency/item accounting and snapshot determinism.
- Recheck the ordinary economy and garment replacement on expanded routes. A bounded longer probe may establish actual bottlenecks; do not add disposal, quotas, money or arbitrary balances without evidence.
- Build the maintained applications; run browser tests and production build. Inspect native and browser output at representative sizes after all features are integrated. Keep physical Windows input disabled; use the authorized debug and hidden-browser paths.
- Freeze source and run `cargo xtask verify --pretty`, preserving all failures and ending only with `ok:true`. Record exact counts, skips, warnings, compatibility limits and owned-process cleanup in a new closeout note. Preserve earlier increment evidence.

## Acceptance

The expanded map, water blocking, roads and both visual projections agree. Old saved compact worlds stay compact. The player can create and sustain a real little home/garden through shared actions, consume or trade real produce and resume the same estate. NPC production/trade/survival continue on the larger island. No fake construction controls, gifts, remote settlement, hidden provider usage or protected baseline changes substitute for a working loop.

## Executed closeout status

Phase-A 62 tests, the fourteen-day resident audit, integrated backend 71 tests, all five registered gameplay rehearsals, native unit/menu checks, browser 24 tests/build, final focused Clippy and the all-five application release build are green. The public-tool farm rehearsal starts Herp without money, builds from acquired materials, preserves home/crops through resume, turns harvest into food and earns real sale income.

Root inspected four native construction/crop states and a clear cottage-door crop confirmation. Corrected browser output was inspected at desktop 1440×1000 and mobile 390×844, with no overflow or console errors, readable owner/home/crop states and truthful stale-snapshot feedback. The desktop confirmation shows all three district captions and all six plots. The first browser PNGs preserve the `.empty` collision; one namespacing correction and its later CUA confirmation are recorded separately. Original helper summaries remain false for their actual pre-image discovery failures or cross-agent browser-hold timeout; they are not relabeled as successful whole harnesses. All owned capture/confirmation processes and tabs are cleaned up.

All product source is frozen at fingerprint `96d1894033362bdf0dad8cc1128474d9aefcc17175360ff8ec0d49b375a879ff` (514 Rust/content/tooling files plus 18 viewer files), with zero confirmation hash mismatches. The final `cargo xtask verify --pretty` exited **0**, **`ok:true`**: **1,731 tests passed** (12 skipped, 4 slow, 5 leaky), documentation tests 1 passed/10 ignored, architecture checks 7 passed, replays 8 verified and snapshots 6 unchanged. Formatting, warning-denied Clippy and dependency policy all pass; every required gate step ran. The final browser tests remain 24/24 and its production build passes. The mechanical UI scan was not rerun; its 25 advisory-only findings and existing bundle/annotation advisories remain recorded. Implementation and the final polishing pass are complete. Exact artifacts, API-first controls and honest evidence boundaries are in [the validation note](../../_notes/2026-11-02-living-homestead-validation.md).
