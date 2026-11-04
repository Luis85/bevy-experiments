# Living economy increment — validation evidence

Date: 2026-11-01. **Complete: final full gate `ok:true`, exit 0.** The first gate's legacy-status failure is archived and its fix is green. Final focused Clippy, the refreshed registered Herp rehearsal, all-five release rebuild and unchanged 192,000-tick release-host maturation probe are now green, alongside the simulation and inspected UI evidence below.

Artifacts referenced below are local files under `target/`. The source changes are in the shared checkout; no protected baseline replacement is part of this increment.

## Implemented scope

- Opt-in `EconomyModel::NeedsDriven`, the `living_island` preset and `living_village` scenario. The existing fixed-price island remains available. Living registrations require a fresh living world/content hash.
- Twelve resident professions, including hunter Rhea, tanner Sage and tailor Lina. Needs forecast physical consumption and cold protection; bounded plans propagate funded demand through acyclic recipes, credit stock/committed work once, and fund complete input bundles.
- Market/reference input value ranks jobs, while admission of genuine financed work uses actual missing-input cash cost. Owned materials do not require a second payment at their current book value. This applies to funded sales to other residents as well as self-production, with actual demand and physical inputs still required.
- One physical quote/settlement path for NPCs and clients, adaptive integer prices, bounded public market history, conditional jointly funded bids, explicit player goods listings and application-time price limits. Cash, goods, reach, capacity and ownership remain authoritative.
- Shared gathering, hunting, crafting, equipment, eating, drinking, recovery and trade. Hunting takes 240 ticks, claims a real spear and prey, spends them only on success, and yields two raw meat and one hide. Daily finite habitat renewal uses fresh animal identities and unoccupied grazing positions.
- Joining chains for raw meat → cooked meals and hide → leather plus fiber → thread → warm clothes. Smithing supplies spears; tanning consumes bowls. Meals/berries satisfy Hunger, and equipped clothing protects actual Warmth while wearing out.
- A 96,000-tick default garment lifetime. At one wear per exposed cold tick and 24,000 ticks/day, thirteen actors need 3.25 replacement garments/day in continuous cold; the actual habitat/tanning/tailoring quantities support four/day. The regression checks this with integer cross multiplication. This is a material-capacity bound; tools, other inputs, work and travel still have costs.
- Public native/browser jobs, wildlife, resource/recipe timing, warmth/clothing and market information. Shared `calendar_at` keeps native HUD/weather aligned with the authoritative late-autumn calendar, and both views show the Living title. Native wildlife follows current sensed-unit fog rules, and visual cleanup retains it while the animal exists. Living briefings distinguish ordinary gathering from spear hunting and explain hunting cancellation; fixed-mode briefing text is preserved.

The design is independently inspired by [BazaarBot](https://github.com/larsiusprime/bazaarBot) and [Doran and Parberry, Emergent Economies for Role Playing Games](https://ianparberry.com/pubs/econ.pdf). It uses this framework's integer state, stable ordering and physical settlement. Design/attribution details: [ADR 0030](../adr/0030-needs-driven-living-economy.md), [implementation plan](../superpowers/plans/2026-11-01-living-economy.md), and [operator/player guide](../living-island-economy.md).

## Focused simulation and interface checks

The earlier combined focused run passed **32 tests**, with no failed or ignored tests in those targets. Evidence: [living-survival-finance-final.log](../../target/living-survival-finance-final.log). Its production-admission implementation was subsequently extended by the maturation fix recorded below.

| Target | Passed | Main contracts |
|---|---:|---|
| `island_market` | 12 | Physical listed sales, conservation, jointly backed material limits, affordability recovery, no learning from reads/rejections, application-time limits, snapshots and fixed-mode isolation |
| `living_agent_information` | 3 | Dynamic-price briefing, compact market observations/read-only hashes, own rest-completion receipt |
| `living_production` | 16 | Funded joining demand, no double-crediting, complete-bundle working cash, self-production/internal utility, physical hunting, meal preference, zero unwanted industrial quotas, material-cycle rejection, save/resume, winter production and price-shock recovery |
| `living_survival` | 1 | Cashless NPC self-supply through one finite water gather and one shared drink, without trade/stipend/currency creation |

After the maturation fix, [living-maturation-focused.log](../../target/living-maturation-focused.log) passes **31 tests** across `island_market` (**12**), `living_production` (**17**), `living_survival` (**1**) and the new `living_replacement` (**1**). The unchanged survival, conservation, physical work and zero-unfunded-demand assertions remain in force.

Independent focused runs also passed:

| Check | Passed | Artifact |
|---|---:|---|
| Market price-learning units | 3 | [living-joint-market-unit.log](../../target/living-joint-market-unit.log) |
| Hunting, finite new gathering, cancellation/contention, post-spend capacity, fresh identities/unique positions, snapshots | 9 | [living-joint-hunting.log](../../target/living-joint-hunting.log) |
| Recipe opt-in isolation, joining DAG, all five registered new recipe executions, actual food consumption and clothing capacity | 5 | [living-joint-crafting.log](../../target/living-joint-crafting.log) |
| Climate, actual shared equipment protection/wear, incumbent identities and snapshots | 3 | [living-joint-climate.log](../../target/living-joint-climate.log) |
| Explicit preset mode/old serialization and invalid-layout rejection | 2 | [living-joint-presets.log](../../target/living-joint-presets.log) |
| Closed actor-scoped scheduler outcomes and hash isolation | 4 | [living-joint-scheduler.log](../../target/living-joint-scheduler.log) |
| Living-only briefing correction/fixed text preservation | 1 | [living-briefing-focused.log](../../target/living-briefing-focused.log) |
| Actual native `decide_looks` ECS and picker regression | 1 | [living-final-native-fog.log](../../target/living-final-native-fog.log) |
| Final climate/calendar polish | 4 | [living-calendar-polish.log](../../target/living-calendar-polish.log) |
| Wildlife visual survival through generic cleanup, then removal with the animal | 1 | [living-wildlife-visual-cleanup.log](../../target/living-wildlife-visual-cleanup.log) |

The fog regression places a deer in visible ground with no sensed percept: the visual is hidden and the real picker returns no target. Adding a current sensed percept makes it visible/pickable; stale memory hides it again. This is an ECS/interface check; screenshot inspection is recorded separately below.

The earlier [independent batch index](../../target/living-joint-independent-results.json) retains historical compile failures for survival/information. Its first survival failure was a `JsonObject` test-indexing compile error, not a behavioral RED. The corrected fixture's actual behavioral failure and later passing run are recorded next.

## Three-seed ongoing-economy stability — green

[living-joint-stability.log](../../target/living-joint-stability.log) passed the `living_stability` test in **58.13 seconds**. The test runs seeds **1, 7 and 42**, each for **72,000 simulation ticks** at the unchanged **24,000-tick day**. It does not change needs, wallets, stock or professions after construction.

At 200-tick samples it requires all twelve residents alive and exact conservation of wallets + dropped coin purses + community fund. It counts carried, reserved and dropped trade goods, bounding each kind by its declared stock limit × thirteen actors so discarded output cannot hide a backlog.

The **48,000 → 72,000** late window has its own assertions: at least one actual physical transaction, at least one completed bread/meal production, and at least one sample of unreserved finished food in living residents' inventories. Opening cumulative totals cannot satisfy those checks. The test also measures food-availability samples, longest sampled shortage streak and peak unconsumed goods. The captured success log records the test verdict; it does not include per-seed diagnostic counter output.

The visiting hero is idle in this resident-economy fixture. Its result establishes the bounded NPC-world checks above, not unaided hero survival or an unlimited-duration economic guarantee.

## Causal survival and execution regressions

### Cashless water recovery: actual RED → green

After correcting the initial fixture API compilation issue, [living-survival-runtime-before.log](../../target/living-survival-runtime-before.log) ran the real behavior test and failed: **Thirst 17,000, consumed 0, gathered 0** after the bounded wait. A thirsty cashless farmer at a reachable water source failed to choose physical self-supply.

The fixture preserves Ada's active life tree, removes other resident brains to isolate income, transfers her original water item IDs into the unlisted idle hero's inventory, and transfers her existing 200 coins into the existing community fund. It records `SetNeed(Thirst, 20000)` and leaves the actual source finite. No replacement water or currency is granted.

The corrected AI fallback passes in [living-survival-finance-final.log](../../target/living-survival-finance-final.log). The unchanged assertions require Thirst above 50,000, exactly one successful water gather and one shared water-consumption effect, one unit of actual source depletion/production, no trade, Ada remaining cashless, the original water identities still held by the hero, and conserved total currency.

### Historical pre-maturation internal-work negative control

An earlier execution regression required a cashless tailor with owned materials to spin thread for her own garment even when thread had a negative standalone sale margin. Its declared isolated quotation fixture set the relevant opportunity-cost beliefs. A scoped temporary `ai.rs` variant failed that test after compilation: [living-internal-utility-negative.log](../../target/living-internal-utility-negative.log), exit **101**. Restoring that source version made the targeted test pass: [living-internal-utility-restored-positive.log](../../target/living-internal-utility-restored-positive.log).

This control is **historical**: it exercised the earlier internal-utility disjunct, before the broader owned-input/missing-cash admission fix. It is not causal proof of the final admission policy. That final policy is exercised by the funded bowl-job RED → green and unchanged-world replacement regression below.

[living-internal-utility-negative-control.json](../../target/living-internal-utility-negative-control.json) records:

```text
original SHA-256: 0092fc4cd1b0f604d4cabe34a2ead06257fc23ff42f704cabc0472378ba6c77c
negative SHA-256: 23b639eb18aef3169fd1a6494e209b551810fcd908bfbe1333f60ab1bbcd8a59
restored SHA-256: 0092fc4cd1b0f604d4cabe34a2ead06257fc23ff42f704cabc0472378ba6c77c
behavior_red: true
exact_restoration: true
```

These hashes identify the archived control and its exact restoration at that time. Later maturation edits intentionally change `ai.rs`; the restored hash is not a claim about the final source. No assertion or protected oracle was replaced to accept the negative behavior.

### Mature owned-input work: actual RED → green

The unchanged [192,000-tick pre-fix release rehearsal](../../target/living-mature-economy-9rva05k8/summary.json) exposed a real late stall: twelve NPCs survived and food/general trade continued, but clothing sales stayed at six after tick 72,003 and every equipped garment was gone by tick 168,003. That run advanced three initial host ticks before the operator pause; later clock advances were explicit, with no stock, money, need, job or provider intervention, and the host stopped cleanly.

The captured failure was converted into `a_funded_ready_bowl_job_completes_with_owned_planks_despite_stale_book_cost`. In its isolated quotation fixture, the carpenter already owns the planks and another resident funds a real bowl need. High marked plank value still caused the old job-admission guard to refuse `CARVE`. [living-mature-carve-before.log](../../target/living-mature-carve-before.log) records the actual compiled behavioral RED, preserving the assertion that the ready funded job must start.

The corrected policy retains book/reference margin for ranking, but checks the actual new cash required to acquire missing inputs when admitting genuinely financed work. A held plank does not need to be purchased again at its marked value. This lets the carpenter make and physically sell a needed bowl even when its standalone book margin is negative; it does not create gifts, loans or production quotas.

The same bowl regression is green among **17 production tests** in [living-maturation-focused.log](../../target/living-maturation-focused.log). The run also passes **12 market tests** and the unchanged cashless water recovery test.

### Two garment lifetimes in the unchanged simulation — green

The new `living_replacement` test passes in **27.33 seconds** in [living-maturation-focused.log](../../target/living-maturation-focused.log). It runs seed **7** to **192,000 ticks**, twice the configured 96,000-tick garment lifetime, with the normal 24,000-tick day. After construction it advances only the clock: resident needs, money, stock, prices and jobs are not edited.

It requires all twelve residents alive and conserved total currency at daily checkpoints. Between tick **96,000** and **192,000**, actual garment production and sales must both increase, and food trading must continue. At least one resident's earlier equipped garment must have been destroyed and replaced by a **different real equipped item ID**. Initial stock transfers cannot satisfy the new-production assertion. The corresponding rebuilt physical-host probe is also green, recorded below.

## Registered Herp production → sale → drinking-water rehearsal — green

Command/target: `cargo test --locked -p npcctl --test island_playtest registered_living_raw_materials_to_finished_offer_and_npc_purchase_resume -- --nocapture`.

[living-final-registered-herp.log](../../target/living-final-registered-herp.log) passed the final-candidate registered-HTTP reference-client rerun in **6.79 seconds**. Its summary records:

| Result | Observed |
|---|---|
| World/hero | `living_island`, Herp, seed `1793610091240795440` |
| Opening money | 0 coins |
| Physical chain | Gather ore/log; smelt ingots; saw planks; forge one hunting spear |
| Finished offer/sale | The same physical spear and its listing survive resume; an NPC physically buys the same item |
| First income | 23 coins after 5,062 elapsed simulation ticks |
| Earned-income need cycle | Buy one water for 1 coin, then drink it; 22 coins remain |
| Thirst | 84,328 → 99,997 |
| Full need-cycle budget | 5,224 elapsed ticks, within one normal 24,000-tick day |
| World currency | 2,400, conserved across trade and restart |
| Cleanup/provider | Owned hosts stopped; `real_provider: false` |

Actor choices use registered observations, learned static workplaces, current sensed traders and actual advertised recipes. Operator interventions advance the paused clock and verify final item ownership/saved currency; those privileged reads are not used for actor decisions. No preset coordinates or catalog IDs are supplied to that reference actor. This is a deterministic engineering reference client, not a live AI-provider conversation.

## Browser checks — green

From `npc-world-viewer/`, `npm test` passed **15/15 tests** and `npm run build` passed TypeScript checking and the Vite production build. Evidence: [results](../../target/living-browser-final-results.json), [tests](../../target/living-browser-final-tests.log), [build](../../target/living-browser-final-build.log).

Coverage includes living market/wildlife snapshots, safe numeric/schema validation, backward-compatible optional fields, full replacement/despawn reconciliation, public feedback and narrow-screen bubble placement. The final built JS artifact is `index-DJnPrDzw.js` (607.51 kB; gzip 161.68 kB). Existing Zod pure-annotation and >500-kB chunk advisories remain visible in the successful build log.

## Workspace verification — final full gate green

The archived [first full gate](../../target/living-economy-verify-before-maturation-fix.json) returned **`ok:false`**. Nextest ran **1,691 tests: 1,690 passed, 1 failed, 12 skipped**, with **1 slow / 5 leaky** classifications. The failure was `explorer::ai_client::tests::legacy_status_remains_readable_without_a_failure_field`: an absent legacy status phase was displayed as an explicit disconnected phase. Doc tests passed **1**, with **0 failed / 10 ignored**.

The other gate steps passed: format; Clippy with no warnings; **7 architecture tests**; **8 replays**; **6 golden snapshots, none pending**; and advisories/bans/licenses/sources policy. This is a failed gate, not an overall pass.

The client-status fix preserves the original assertion and passes **8/8** focused status tests in [living-final-client-status.log](../../target/living-final-client-status.log). Final focused Clippy is clean in **55.92 seconds**: [living-final-clippy.log](../../target/living-final-clippy.log). The final `cargo xtask verify --pretty` run returned **`ok:true`, exit 0**: [full JSON verdict](../../target/living-economy-verify.json). All 501 candidate source files still match the frozen final fingerprint.

| Gate | Final result |
|---|---|
| Format | Pass |
| Clippy | Pass, warnings denied |
| Workspace tests | 1,695 run; 1,695 passed; 12 skipped |
| Documentation tests | 1 passed; 0 failed; 10 ignored |
| Architecture | 7 passed |
| Replays | 8 verified: session 1, testbed 3, village 4 |
| Golden snapshots | 6; none pending; `INSTA_UPDATE=no` |
| Dependencies | Advisories, bans, licenses and sources pass |

No optional tool was skipped. Nextest reports **1 slow and 4 leaky outcomes** without failing its verdict. Browser annotation/bundle-size advisories remain as recorded above. Test concurrency was requested as eight threads through the verifier process environment; original environment values were restored afterward, with no test scope, retry, skip or policy changes.

## Final release binaries and candidate identity — green

All five maintained application packages were requested and the final release rebuild passed in **2m46s**, including the status and maturation admission fixes. Evidence: [living-final-release.log](../../target/living-final-release.log).

```powershell
cargo build --locked --release -p actor_testbed -p game_menu -p npcctl -p rts_units_demo -p village --bins
```

The [final candidate manifest](../../target/living-economy-final-candidate.json) records **501 source hashes**, fingerprint `cc24f5fd46140bf0a22ed0ec291194281cda1f613fb14e23b20e6b7ccd3b743a`. Root's shared-checkout coordination confirmed that all **26 refreshed client-source hashes match**. The release `npcctl` used in the successful host probe has SHA-256 `db486698ff2901ca400cb6ee585aae9e101eecdff43d996e3b5dbfa33da2ff39`.

## Unchanged 192,000-tick release-host maturation probe — green

[living-mature-economy-z_ye7cn8/summary.json](../../target/living-mature-economy-z_ye7cn8/summary.json) records **`ok:true`**, seed **7**, the rebuilt binary hash above, and a **29.05-second** unchanged-world probe to **192,000 ticks**. All twelve NPCs are alive at every recorded checkpoint.

| Physical market measure | Tick 96,000 | Tick 192,000 | Late increase |
|---|---:|---:|---:|
| Units actually traded | 417 | 655 | 238 |
| Food units traded | 44 | 77 | 33 |
| Garments sold | 11 | 13 | 2 |

Mira, Ivo and Lina replace destroyed original equipped garments with different real equipped items. There are no state, funds, inventory or provider interventions. The host's initial **three ticks** are recorded; exact checkpoint alignment was corrected only in the harness. The report records `source_changed:false`, `real_provider:false`, `owned_host_stopped:true` and host exit **0**. The earlier red physical-host artifact remains archived as evidence of the defect, rather than being relabeled.

## Native/browser UI inspection — green

Root inspected these persisted native PNGs at **2560×1440, scale 2** (1280×720 logical):

| Capture | Inspected evidence |
|---|---|
| [living-wildlife.png](../../target/living-wildlife-visual-1ecc10466caf/npc-out/shots/living-wildlife.png) | Eight deer visible in a native close-up after the visual-cleanup fix |
| [living-day-work.png](../../target/living-economy-visual-ba3f45f04457/npc-out/shots/living-day-work.png) | Living title, correct Day 20/Autumn native calendar/weather, healthy resident population and public physical jobs |
| [living-night-market.png](../../target/living-economy-visual-ba3f45f04457/npc-out/shots/living-night-market.png) | Native night/market view, continued resident work/recovery and cold-protection state |

The paired run includes all twelve healthy NPCs and public jobs such as gathering fiber/water/logs, buying grain/flour/bowls/fiber and approaching wildlife to hunt. It also records a shared home-rest command that starts resting without charging coins. At night tick **17,001**, clothing protection is represented by actual equipped garments and Warmth values.

The hidden browser was inspected at **1280×720** from the same exported world. [browser-inspection.json](../../target/living-economy-visual-ba3f45f04457/browser-inspection.json) records the matching **Living Island Village** title, **Day 20 / Autumn / Night**, rendered market table, and Lina's inspector with **Warmth 100,000** and warm clothes equipped. It reports **zero console errors**, inspected images and a closed browser tab. Browser images were inspected as tool image attachments.

Both native capture summaries are green: [wildlife run](../../target/living-wildlife-visual-1ecc10466caf/summary.json) and [day/night run](../../target/living-economy-visual-ba3f45f04457/summary.json). They record `provider_started:false`, `physical_input:false` and `owned_resources_stopped:true`. The browser report also records no provider or physical input. Native and browser checks used background capture/debug paths.

Those UI/calendar/wildlife captures precede the final maturation admission fix and establish the inspected presentation states. The separately linked rebuilt release-host probe establishes the later unchanged-world maturation behavior.

## Final status and limits — complete

All six planned increments and the improvement/polishing pass are complete. The final full gate, unchanged 192,000-tick simulation, all-five release rebuild, registered Herp rehearsal and unchanged 192,000-tick release-host probe are green. The first failed gate and red maturation run remain historical evidence of defects that were reproduced and corrected. Manual native/browser/probe resources were stopped; this task made no provider call. Create a fresh `living_island` world for the increment.

The measured scope is a bounded physical spot-market economy with producer professions and shared player actions. It does not establish endless survival under arbitrary destruction/hoarding, salaried employment contracts, loans, property markets, remote auction settlement, automatic bankrupt-agent replacement or a moving predator/herd ecosystem. This increment ran no paid provider conversation and inspected no provider history.
