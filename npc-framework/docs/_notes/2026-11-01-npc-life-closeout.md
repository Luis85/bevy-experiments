# NPC life and environment increment — 2026-11-01

**COMPLETE — final full gate `ok:true`.** Implementation, subagent review, end-to-end rehearsals and the final improvement/polish pass are complete. The first gate's registered rehearsal failure is fixed; its original budgets and outcome assertions remain intact.

## Final workspace verdict

`cargo xtask verify --pretty` returned **ok:true**, terminal exit **0**. Authoritative artifact: `target/npc-life-verify-final.json`.

- Formatting: all files formatted. Clippy: no warnings.
- Nextest: **1,585/1,585 passed**, **12 skipped**, **one slow/five leaky classifications** recorded.
- Doc tests: **1 passed, 0 failed, 10 ignored**.
- Architecture: **7 passed**. Protected replays: **8 verified** (session 1, testbed 3, village 4).
- Goldens: **6**, none pending, with `INSTA_UPDATE=no`.
- Deny: advisories, bans, licenses and sources all passed. No optional verification tool was skipped.

Only process-local Nextest concurrency/output variables were set and restored. No protected oracle or generated policy configuration was changed. The five leak classifications remain a recorded test-run warning, not a claimed clean leak audit or a failed gate.

## Delivered

- A real island-only `island_life` TreeDef and nine resident Brains, using the existing runtime and generic external-control/order branches. Saved/hashed staged decisions retain deterministic execution and cooldowns. Critical actionable needs interrupt owned work; ordinary work and recovery finish normally.
- Shared application-time checks for NPC/client consumption, gathering, trade, voluntary recovery and lost property. New `rest_home`, `rest_ground`, `recover_diary`; carried coin redemption removes its inventory slot. Existing generic movement/crafting remain authoritative.
- Six executable named island crafting aliases in headless/native hosts, canonical world/actor permission handling, and the same registered policy gates for NPC manufacture.
- Finite paid ingredient buffering, with smaller valid purchases for tight packs. This resolves a genuine late-year bread shortage without free inputs, money, need overrides or weakened survival assertions.
- Perception-scoped resource renewal/stock and known public workplace role/products/accepted goods and supported recipes with actual ingredient/output counts. These static opportunities never track unseen residents or promise current stock. Stale remote resource values remain unknown. Public current decisions and resource/production information appear in observer exports and views.
- Native captions show life goals when a physical activity is not more specific. Browser selection reveals its inspector; periodic polling never scrolls the page.

Subagents owned the life tree, action parity, and independent environment contracts/audit. The primary integrated the API/observation/view/docs and coordinated verification. The separate authorized AI-client chat owns its session-history UI changes; the final gate covers the combined checkout.

## Focused checks

- `target/npc-life-short-year-buffer-verified.log`: **38/38 passed** — life tree 12, island village 22, economy soaks 4. The original 29-day accelerated survival, all production stages, transaction activity and currency conservation assertions are unchanged. All three-seed stream-length and seasonal soaks passed. Tight-pack regression proves a smaller paid transfer instead of a stalled oversized purchase.
- `target/npc-life-named-production-verified.log`: **3/3 passed** — enabled/disabled named crafting plus all six recipe production paths and canonical/named actor permission negative/positive controls through registered client actions.
- Earlier targeted `target/npc-life-focused.log`: action parity **6/6**, environment **7/7**. Its original tree timing assertion failed; sampling the exact urgent-consumption tick rather than a later legitimate work restart corrected that test. Subsequent tree suites pass.
- Final workplace directory/privacy suite, `target/npc-life-environment-directory.log`: **9/9 passed in 0.19 s**, including no live tracking after unseen resident mutations.
- Viewer: `npm test` **14/14 passed**, `npm run build` passed. Final browser asset `index-1qcIT1-S.js`. Existing Zod annotation and >500 kB bundle advisories remain; no dependency upgrade.
- Release build of all five maintained apps passed in **2m05s**, `target/npc-life-release.log`. The peer subsequently rebuilt actor/menu/npcctl for its CRLF reader fix; its final menu polish build is recorded separately.
- Final release rebuild after the workplace-directory follow-up passed for all five maintained apps in **3m00s**, `target/npc-life-release-final.log`.
- Diff whitespace check passed; existing CRLF-to-LF notices are recorded, not errors.

## Visual and real-host evidence

`target/npc-life-visual-50bf348ad4bd/summary.json` is **ok:true**, with provider_started:false, physical_input:false, owned_resources_stopped:true. This is a new seed-7 standalone island, with an operator-owned paused clock and view-only sizing.

The primary opened both actual native PNGs: `npc-out/shots/npc-day-life.png` at tick 400 and `npc-night-life.png` at tick 17001. All nine residents expose real current tree decisions. Day shows physical gathering/crafting/travel; night shows home recovery. A recorded visitor `rest_home` command produced resting activity without changing coins. All residents were healthy at the night checkpoint.

The hidden browser followed the same exported file, inspected at **1280×720** and **390×844**. NPC decision, bakery recipe quantities and resource renewal timing were visibly verified. After the one browser polish, selection revealed the inspector at both sizes; measured document widths were 1280/1280 and 375/390, with no horizontal overflow and no browser console errors. Temporary viewport overrides and the tab were cleaned up. Browser screenshots are in this chat's tool evidence; native PNGs are retained in the ignored run directory.

Two earlier helper-only attempts (`npc-life-visual-bcd7be3cf478` and `npc-life-visual-d4b83943b964`) stopped before any screenshot: the new helper needed explicit `--remote` in standalone mode and needed the public export file rather than the generic `world/state` inspector. Their failed summaries remain immutable and show owned cleanup. No product defect or provider usage was involved.

The authorized peer reports its native session-viewer UI frozen and visually approved: `target/native-sessions-f2575e04ef37/report.json` covers nine initial states, and `target/native-sessions-confirm-b17e6415a481/report.json` confirms the three menu states after its single bounded footer/scroll polish. Its root opened the images and confirmed desktop/narrow footer visibility, readable glyphs, a paused unchanged world, no provider/private-history access, and cleanup. The combined gate starts from that final source; these peer-owned UI changes are not NPC simulation changes.

## Review-driven failures and corrections

The first broad content run (`target/npc-life-content-tests.log`) failed the unchanged accelerated-year survival check: miner Bram died at tick 53062. Diagnostic `target/npc-life-short-year-diagnostic.json` showed money and water available but bakery stock empty during repeated ingredient trips. Paid buffers of up to eight ingredients fixed the original test (`target/npc-life-short-year-batch8.log`); the final implementation also buys a smaller valid amount when necessary. Temporary diagnostic test source was removed; ignored evidence is retained.

Named crafting tests first exposed an incorrectly available disabled-world affordance (`npc-life-craft-policy-before.log`), then the missing headless recipe handler (`npc-life-craft-policy-after.log`). Explicit aliases and normalized policy checks fixed both. All six recipe paths now have real registered-client production coverage; no generic core crate was special-cased for the island.

The first full gate, `target/npc-life-verify.json`, returned **ok:false**: formatting, Clippy, architecture (7), replays (8), goldens (6) and deny all passed. Nextest ran **1,583 tests: 1,582 passed, one failed, 12 skipped**, with one slow/two leaky classifications; doc tests passed (1, with 10 ignored). The sole failure was `npcctl::island_playtest::registered_cold_income_needs_and_save_resume`.

The unchanged failure was reproduced, then public observation traces (`npc-life-registered-reproduce.log`, `npc-life-registered-trace.log`) showed an expired remembered baker position repeatedly selected from the rehearsal client's historical offer cache. Rebuilding the current offer set removed expired offers, but a still-remembered moving smith exposed the same weak search strategy. A sensed-only filter then correctly required real discovery rather than stale tracking (`npc-life-registered-fresh-offers.log`, `npc-life-registered-live-trader.log`).

The final reference client caches only learned public workplace descriptions as static search destinations, selects a currently sensed merchant, and then validates the actual nearby offer. Public workplace role/products/accepted goods let clients plan that search without hidden actor locations or stock. Original assertions, 25 discovery iterations, 12 blind searches, eight approach/trade attempts, 12,000-tick first-sale and 24,000-tick cycle budgets remain unchanged. No Follow action was used to track an unseen actor.

`target/npc-life-registered-workplace-search.log`: **all three actual HTTP/MCP host rehearsals passed in 7.98 s**. Herp's first sale is tick **3564**, complete income/needs/save-resume cycle **4430**, with one intentional rejected duplicate-gather request. Speech/action/private-note roundtrip and the separately labeled forced-death/successor/privacy rehearsal also passed. All owned hosts stopped; no provider was invoked.

Independent final source review returned **SHIP** for the directory/search correction: only registered observations, learned static positions, stable nearest-place tie-breaking, bounded search and currently sensed trade candidates are used. The final full-gate recheck is `target/npc-life-verify-final.json`.

Independent source audit found no remaining actionable determinism, ownership or privacy issue after those fixes. Protected replay, golden, snapshot, architecture and lint-policy files were not changed to pass checks.

## Compatibility and limits

Use a **fresh Island Village**: the new island tree/action-state registration intentionally changes its content hash. Old strict island saves remain for their matching prior binary; no silent migration is claimed. New-build snapshots resume deterministically. The incumbent non-island replay/golden corpus remains protected.

This increment delivers the shared action API first, as the user selected. It adds no native human action panel or writable browser client. Visitors can perform resident production/survival tasks but retain different starting assets and do not gain passive storefronts or resident community stipends. Private host notebooks remain separate from physical diaries.

No additional paid Codex/Claude run, provider-history read, clipboard check, physical-input QA, or full-stream performance benchmark was performed. Simulation soaks and scripted operator fixtures are not evidence of long unaided live-AI play.
