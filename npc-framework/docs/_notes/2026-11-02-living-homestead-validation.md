# Living Village polish and homesteads — validation evidence

Date: 2026-11-02. **Implementation and the final improvement/polishing pass are complete.** Focused simulation, registered-client, native-unit, browser-build, release and corrected native/browser visual inspection are green. The final frozen-source full workspace gate exited **0**, with **`ok:true`**.

Artifacts are local files under `target/`. The checks use disposable game fixtures and engineering clients; no provider conversation or provider-history inspection is part of this validation task.

## Delivered scope

- Explicit immutable `Compact`/`Expanded` geography. Fixed and saved copies without an expansion flag keep their original 24×20 m ellipse. Newly selected Expanded Living worlds use a 30×25 m ellipse on the same 64-cell grid: 2,360 land cells versus 1,516 compact cells.
- One shared integer definition supplies land, shoreline, connected roads, resource/pasture positions, three trade-district sign points and six clear 6×6 m plots. All incumbent identities are preserved: ten additional shoreline blockers append after existing construction, followed by six actual homestead plot entities. Districts are wayfinding metadata, not fake shops or ownership.
- Living-only NPC routine polish: actionable bodily water/food recovery precedes optional night rest; ready financed work precedes unnecessary cross-island input buffers; buying, gathering and hunting use the same 1.5 m reach as clients. Fixed behavior and the 0.8 m home-rest rule are preserved.
- Shared recorded `HomesteadAct` actions: claim a plot, build a cottage, plant, water and harvest. Every living villager uses the same ownership, reach, life, busy, material and capacity checks. One housing and one garden plot may be claimed; a dead owner's physical estate can be reclaimed without erasing its house or crops.
- Construction reserves eight real planks and four logs for 600 ticks, spends them only on successful completion, creates an actual blocked/occluding cottage, and changes the owner's home to its reachable door without teleporting. Cancellation/failure/death release work claims and preserve physical materials.
- Three housing beds or six garden beds. One real grain seed and one clean water start 6,000-tick growth; a capacity-checked harvest yields three grain once. Crops and construction survive snapshots. Ready outcomes are emitted once; player choices are not automated.
- Public machine-readable construction/crop rules, approach coordinates, plot/cottage DTOs and actor-scoped ownership observations. Remote foreign ownership/crop changes stay hidden; owned plots remain known. Native/browser presentation uses shared coordinate/stage helpers.
- Generic bounded event-summary repair: an applied actor-attributed content effect joins the own-outcome priority group, so a newer sale/crop receipt is not indefinitely displaced by thirty older actions. Unit filtering, result bounds, chronological ordering and failed-effect/perception handling remain in place.

Design references: [implementation plan](../superpowers/plans/2026-11-02-living-village-homestead.md), [ADR 0032: geography and homesteads](../adr/0032-immutable-island-geography-and-shared-homesteads.md), and [ADR 0033: attributed content effects](../adr/0033-attributed-content-effects-in-agent-event-summaries.md). The existing living economy remains independently inspired by [BazaarBot](https://github.com/larsiusprime/bazaarBot) and [Doran/Parberry's research](https://ianparberry.com/pubs/econ.pdf), as documented in ADR 0030.

## Bounded integration review

The read-only closeout review found no concrete correctness/privacy issue in the requested integration surface:

- Cottage construction participates in activity, busy/work holding and the global work gate. `ai::stop_work` cancels construction before death drops inventory, and homestead ticking follows village vitals/effects.
- Closed scheduler classifications match the emitted plot, cottage and crop outcomes and require the assigned actor and an applied effect. Operator projection exposes actual plot/cottage state; registered observations retain scoped foreign-state rules.
- The generic event-summary fix changes priority for applied content effects without inspecting island names or payloads. The existing caller still filters the requested unit before assigning priority. It does not enlarge buffers or turn the bounded summary into a complete event history.

No product source or Cargo operation was performed for that closeout review.

## Phase A checks and long resident audit — green

[living-polish-phase-a.log](../../target/living-polish-phase-a.log) passes **62 explicitly selected tests**: incumbent economy/life behavior, market/production, geography/profile metadata, replacement, routine polish and cashless survival. The new six routine fixtures have their original runtime failures preserved in [living-routine-polish-before.log](../../target/living-routine-polish-before.log), with corrected behavior in [living-routine-polish-after.log](../../target/living-routine-polish-after.log).

The sole completed fourteen-day debug-host audit is [living-polish-fourteen-day-ixu1glhn/summary.json](../../target/living-polish-fourteen-day-ixu1glhn/summary.json): **`ok:true`**, seed **7**, through **336,000 ticks**, with **12/12 NPCs alive at every half-day checkpoint**. Runtime was **62.14 seconds**. The report records no probe source writes, no real provider, unchanged release binary hash, and a stopped owned host with exit **0**.

| Actual circulation | Tick 96,000 | Tick 336,000 | Late increase |
|---|---:|---:|---:|
| Traded units | 394 | 1,023 | 629 |
| Food units traded | 44 | 153 | 109 |
| Clothing sales | 9 | 15 | 6 |

Nell, Jun, Ivo and Sage replace destroyed equipped garments with different real items. The hunter peaks at **30 raw meat units and 5/64 used inventory slots**. This audit found no demonstrated pack-pressure problem; no new disposal rule or forced stock quota was added on its evidence. One seed and half-day samples do not establish every possible storage failure or continuous duty cycle. Exact currency conservation is covered by the simulation/registered tests rather than inferred from this observer-only audit.

The first bounded audit stopped at tick 120,000 on a Windows observer-file sharing `PermissionError`, with its host stopped: [living-polish-fourteen-day.log](../../target/living-polish-fourteen-day.log), directory `living-polish-fourteen-day-bhw1tbe6`. The later successful run corrected observer-read retry handling within the existing bound; this is not a reclassification of that failed partial run.

## Integrated backend and compatibility checks — green

[living-homestead-focused.log](../../target/living-homestead-focused.log) passes **71 tests**, no failed/ignored tests, session exit **0**:

| Target | Passed |
|---|---:|
| Homestead physical commands, claims, construction/death/cancellation, crop conservation, inheritance and snapshots | 10 |
| Homestead public rules, registered queued claim, approach data and remote-state privacy | 4 |
| Shared geography/navigation/incumbent identities | 6 |
| Geography preset immutability/compatibility | 3 |
| Expanded/compact projection | 2 |
| Existing world projection/read-only feedback/privacy | 9 |
| Living routine polish | 6 |
| Living production | 17 |
| Physical market | 12 |
| 192,000-tick garment replacement | 1 |
| Cashless physical water recovery | 1 |

The geography assertion distinguishes exactly ten appended shoreline blockers from six new plots while retaining incumbent name, inventory and prey-identity assertions. These per-run counts overlap the phase-A checks; they are not a new aggregate workspace-test total.

## Registered Herp homestead loop — green

`cargo test --locked -p npcctl --test island_playtest -- --nocapture --test-threads=1` passes **all five cases** in **35.68 seconds**: [living-homestead-registered-all.log](../../target/living-homestead-registered-all.log), with parsed summaries in [living-homestead-registered-result.json](../../target/living-homestead-registered-result.json).

| Case | Result |
|---|---|
| Existing cold start/income/needs/save-resume | Green; cancellation, purchases, diary and fresh registration remain covered |
| Permanent death/successor/public diary privacy | Green; old control revoked and private books start fresh |
| New homestead/materials/crop/food/income | Green; detailed below |
| Living finished spear/listing/NPC purchase/resume | Green; 22 coins income, earned-income water purchase, same physical item transfer |
| Registered MCP stdio/control/action/notes | Green; actual game transport and note round-trip, no provider |

The new farm case uses `living_homestead`, seed **1793714084139653684**, and **30,476 elapsed simulation ticks**:

- Herp starts with **zero coins**, gathers actual wood and crafts the observed recipe's eight planks, and acquires the four additional logs. Construction consumes the real **8 planks + 4 logs**.
- Plot selection, the cottage rule, the door, bed indices/positions, seed/water costs and deadlines come from registered observations and remembered public places. There are no actor decisions based on hardcoded preset coordinates or privileged targets.
- Herp plants one gathered grain seed, spends one gathered clean water, resumes the world during its **6,000-tick** growth, and retains the same ownership, saved home and crop state.
- A real harvest yields **three grain**. The actor mills/bakes part of that harvest into bread and uses actual public trade/listing opportunities. A `VillageSaleCompleted` receipt records one grain sold for **3 coins**.
- World currency remains **2,400** across the loop and restart. The summary records `crop_became_food:true`, `ownership_crop_home_preserved:true`, owned hosts stopped and `real_provider:false`.

Operator interventions advance the paused simulation clock and independently verify saved estate/home/currency only after actor decisions. There are **no supplied materials, wallet grants, teleports or provider decisions** in this reference-client rehearsal. It is a protocol/gameplay engineering proof, not an unaided live AI-provider run or wall-time performance benchmark.

Earlier failed rehearsal attempts remain in [living-homestead-registered-before-listing.log](../../target/living-homestead-registered-before-listing.log) and [living-homestead-registered.log](../../target/living-homestead-registered.log). The former exhausted its active-sell discovery route; the later work uses actual offered goods and an NPC's physical purchase. The appended log also retains earlier failed attempts before its targeted green result. The clean five-case result is the authoritative final registered run above.

## Applied-effect event receipt regression — actual RED → green

[living-polish-event-summary-before.log](../../target/living-polish-event-summary-before.log) records the compiled behavioral failure: `recent_applied_actor_effect_is_not_starved_by_a_full_old_outcome_window` could not see a newer attributed effect after a long action history.

The generic priority correction in ADR 0033 passes **four event tests** in [living-polish-event-summary-after.log](../../target/living-polish-event-summary-after.log), including foreign-unit/perception and failed-effect safeguards. [living-polish-agent-interface.log](../../target/living-polish-agent-interface.log) also passes **24 tests** across `act_and_wait` (**11**), `control` (**3**) and `interface` (**10**). Detailed clients still need authenticated incremental `agent/events` cursors for complete history; bounded default observations provide the newest significant own outcomes.

## Native/unit/menu and browser build checks — green

- Native unit tests: **97 passed, 1 ignored**, including fog/current perception, privacy/read-only rendering, caption/bubble layout and generic visual cleanup. [living-homestead-native-tests.log](../../target/living-homestead-native-tests.log).
- Menu Worlds checks: **13/13** pass, covering preset identity, complete field round trips, atomic/stale-file safety and editor scroll/focus/footer behavior. [living-polish-menu-worlds.log](../../target/living-polish-menu-worlds.log).
- Expanded homestead briefing: **1/1** pass, teaching the public loop without plot coordinates. [living-homestead-briefing.log](../../target/living-homestead-briefing.log).
- Actor-scoped closed scheduler outcomes: **4/4** pass. [living-homestead-scheduler.log](../../target/living-homestead-scheduler.log).
- Final focused Clippy: clean in **9.40 seconds**. [living-homestead-final-clippy.log](../../target/living-homestead-final-clippy.log). Formatting evidence: [living-homestead-final-fmt.log](../../target/living-homestead-final-fmt.log).
- Final browser tests: **24/24** pass; TypeScript/Vite production build passes after the bounded visual correction. [viewer-homestead-final-tests.log](../../target/viewer-homestead-final-tests.log), [viewer-homestead-final-build.log](../../target/viewer-homestead-final-build.log). Built JS: `index-BdZm_60s.js`, **618.73 kB / gzip 165.30 kB**. Known Zod pure-annotation and >500-kB bundle advisories remain visible.
- The completed mechanical UI scan contains **25 quality advisories, zero non-advisory findings**: [living-homestead-ui-detector.json](../../target/living-homestead-ui-detector.json). It remains the recorded advisory scan and was **not rerun** after the bounded visual correction; actual corrected layout was confirmed separately below.

The earlier [native phase-A compile log](../../target/living-polish-native-phase-a.log) retains its tuple-arity compile error. The later full native unit result above is green; the old compiler failure is not a renderer/GUI verdict.

## All-five release build — green

The maintained applications were requested together and the release build passed in **2m41s**: [living-homestead-release.log](../../target/living-homestead-release.log).

```powershell
cargo build --locked --release -p actor_testbed -p game_menu -p npcctl -p rts_units_demo -p village --bins
```

## Native/browser visual evidence — corrected confirmation green

Root inspected the four real native gameplay states captured under [living-homestead-capture-603cc25a71](../../target/living-homestead-capture-603cc25a71/summary.json), at **2560×1440, scale 2**:

| Native capture | Real state inspected |
|---|---|
| [construction.png](../../target/living-homestead-capture-603cc25a71/npc-out/shots/construction.png) | Cottage construction in progress |
| [dry-two-beds-cottage.png](../../target/living-homestead-capture-603cc25a71/npc-out/shots/dry-two-beds-cottage.png) | Built cottage and planted beds awaiting water |
| [growing-and-dry-cottage.png](../../target/living-homestead-capture-603cc25a71/npc-out/shots/growing-and-dry-cottage.png) | Watered growing crop alongside a dry bed |
| [ready-and-dry-cottage.png](../../target/living-homestead-capture-603cc25a71/npc-out/shots/ready-and-dry-cottage.png) | Mature crop alongside dry/empty state |

Root then moved the hero by an ordinary game action to the real cottage door so the ready crop was unobscured in [ready-clear-cottage.png](../../target/living-homestead-capture-603cc25a71/npc-out/shots/ready-clear-cottage.png). This was a normal actor movement, not a teleport or synthetic state replacement.

The third helper completed its native stage checks and four images, but its 600-second browser review hold timed out during the cross-agent tooling handoff. Its original summary therefore remains honestly **`ok:false`**, with `bounded browser review timed out`, native exit **0**, and both owned native/viewer processes stopped. The later successful confirmation is recorded separately rather than rewriting that historical result.

The child's first six persisted browser PNGs exposed a real `.empty` CSS-class collision. A single bounded correction namespaced bed-stage classes. Root also confirmed and fixed the missing tree favicon and widened caption collision search until all three district captions were visible. Those persisted `browser-*.png` files remain **historical, pre-correction** images; they are not the final browser layout evidence.

[parent-browser-confirmation.json](../../target/living-homestead-capture-603cc25a71/parent-browser-confirmation.json) records **`ok:true`** for Root's hidden-browser confirmation against the preserved real native export:

- Desktop **1440×1000**, scroll width 1440: all three district captions and all six plots visible, without horizontal overflow.
- Mobile **390×844**, scroll width 375: empty status is `position: static` and stays inside its bed; owner/home and ready/dry/empty crop states remain readable, without horizontal overflow.
- Ready, needs-water and empty states inspected; **zero console errors**; the stopped host is shown truthfully as a stale snapshot.
- Corrected browser images were inspected as **CUA tool attachments**. Live confirmation tabs were closed and the viewport reset. No corrected local browser JPG/PNG path is implied.

The new owned confirmation viewer, PID **24968**, is stopped: [homestead-browser-confirm-cleanup.json](../../target/homestead-browser-confirm-cleanup.json). No provider or physical input was used for these captures.

The visual fixtures declare ground supplies to isolate presentation and are not unaided material-acquisition evidence. The actual no-grant registered farm rehearsal above remains separate.

The first helper attempt [living-homestead-capture-145c25fe0b/summary.json](../../target/living-homestead-capture-145c25fe0b/summary.json) stopped before images at tick zero, before initial perception. A follow-up [living-homestead-capture-cb9d5679b1/summary.json](../../target/living-homestead-capture-cb9d5679b1/summary.json) advanced perception but stopped on its bounded plot-discovery requirement before images. Both are retained helper/discovery failures, not renderer failures or GUI passes.

## Final frozen candidate and full workspace gate — green

Product source is frozen. [living-homestead-final-candidate.json](../../target/living-homestead-final-candidate.json) records **514 Rust/content/tooling files and 18 viewer files**, fingerprint `96d1894033362bdf0dad8cc1128474d9aefcc17175360ff8ec0d49b375a879ff`.

`cargo xtask verify --pretty` completed in Root's session **89794**, exit **0**, **`ok:true`**: [living-homestead-verify.json](../../target/living-homestead-verify.json), with its progress transcript in [living-homestead-verify.stderr](../../target/living-homestead-verify.stderr).

| Gate | Exact result |
|---|---|
| Formatting | All files formatted |
| Workspace Clippy | No warnings |
| Nextest | **1,731 passed**, **12 skipped**, **4 slow**, **5 leaky** |
| Documentation tests | **1 passed**, **0 failed**, **10 ignored** |
| Architecture fitness | **7 passed**, **0 failed**, **0 ignored** |
| Replays | **8 verified**: session 1, testbed 3, village 4 |
| Goldens | **6 snapshots**, none pending, `INSTA_UPDATE=no` |
| Dependency policy | Advisories, bans, licenses and sources all pass |

All seven required gate steps ran; no optional-tool skip was reported. Nextest's slow/leaky classifications and skipped/ignored tests remain reported rather than being relabeled as failures or full coverage. Browser bundle/annotation advisories and the mechanical UI scan's 25 advisory findings remain the known non-blocking warnings. No protected oracle or lint policy was changed.

[living-homestead-candidate-confirmation.json](../../target/living-homestead-candidate-confirmation.json) confirms **all 514 recorded Rust/content/tooling files and 18 viewer files are unchanged**, with zero hash mismatches, during the final dependency-check phase. The same frozen product source passed the final gate; only documentation closeout followed.

Final `git diff --check -- npc-framework npc-world-viewer` passes. A broader monorepo check reports extra EOF blank lines in four unrelated Terra documents; those user changes were preserved. Git also prints CRLF normalization notices. [living-homestead-closeout.json](../../target/living-homestead-closeout.json) links the final gate, candidate, gameplay, soak and visual evidence without overwriting failed historical harness summaries.

## Current limits

The measured loop is one cottage and a small hardy-grain farm integrated with the actual village economy. Controls remain shared-API first under the user's established preference; no native human farming action panel is included, and the browser is read-only. The fourteen-day NPC audit is one seed; the registered farm rehearsal is a bounded engineering client. Neither establishes unlimited survival under arbitrary player destruction/hoarding or live-provider decision quality. Physical input and provider usage were not exercised by this validation task.
