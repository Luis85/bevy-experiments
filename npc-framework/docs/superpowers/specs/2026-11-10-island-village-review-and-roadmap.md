# Island Village: product review and improvement roadmap

Date: 2026-11-10. Scope: `npc-framework/`, living-world profile (three settlements, 42 residents).

**Goal (owner's words):** a Banished / Stardew Valley crossover with a self-sustained,
NPC-driven economy that reacts to the player, the environment and changes. The island
village is the only showcased scenario. On start the player can join as an inspector,
spawn a controllable character, run a Banished-style colony sim from spectator mode,
and terraform the world in a level-editor mode.

**Method.** Five parallel read-only reviews (economy, NPC AI and socials, player modes
and UX, atmosphere, engineering health), each citing code. The orchestrator re-checked
the highest-impact claims in code (marked ✔). Nothing was built, run or seen on screen
during the review; audit numbers quoted from `docs/_notes/` are the project's own.

## 1. Verdict

The foundation is strong and unusual: a deterministic integer sim, a real GOAP planner,
a closed-loop currency, renewable ecology, permanent death, a careful save lifecycle
and a one-window player flow. What exists is a *stable diorama*. It is not yet a colony
sim or a life sim, for five reasons:

1. **The village cannot grow or change shape.** No births, ageing or immigration ✔; NPCs
   never decide to build; the only building is a cottage on pre-authored plots.
2. **The economy is quota-driven, and a daily wealth cap erases price signals** ✔
   (`village/ai.rs:1057-1070` moves every coin above 200 into a fund and tops everyone up
   to 100). Profit cannot accumulate, so scarcity never redirects labour.
3. **Behaviour is Rust, not data.** The behaviour tree is a shell around one mega-action;
   trees, goods, recipes, roles and buildings are compiled constants.
4. **Residents are interchangeable and their social state is write-only** ✔ (morale and
   affinity are never read by a decision).
5. **The player's verbs are thin in every mode**: five colony levers, a single-click
   terrain stamp, no goals after the first cottage, no audio ✔, no settings.

## 2. Findings by perspective

Paths are relative to `npc-framework/`; `V/` = `content/testbed_content/src/village/`,
`W/` = `content/testbed_view/src/`. Severity H/M/L, size S/M/L.

### 2.1 Economy and colony simulation

Works today: 23 goods; food, wood, metal, leather and clothing chains (`V/catalog.rs`);
BazaarBot-style prices moving ≤1/16 per round (`V/market.rs:415-454`); closed-loop money;
a per-settlement production plan rebuilt every 300 ticks (`V/production.rs:1303-1564`);
occupation switching on blockage and vacancy (`V/frontier/labor.rs:582-696`); hearths that
burn real logs; seven-day seasons; logistic regrowth and finite ore (`V/frontier/ecology.rs`).

| # | Gap | Sev | Size | Evidence |
|---|---|---|---|---|
| E1 | No demography: births, ageing, families, immigration. Population can only fall; 84 beds serve 42 people | H | L | `V/colony/model.rs:50-58`, `labor.rs:152` |
| E2 | NPCs never decide to build; one building type; workplaces are immutable | H | M | `V/colony/commands.rs:63,153`, `V/homestead/mod.rs:27` |
| E3 | Daily 100% wealth cap blunts every price signal ✔ | H | M | `V/ai.rs:1057-1070` |
| E4 | Production is need-quota driven; price only gates affordability; 2 of 14 residents per settlement are flexible | H | M | `V/production.rs:501-527,1169`, `labor.rs:172,621` |
| E5 | No stockpiles, barns, market stalls or hauling; trade needs both actors within 1.5 m | H | L | `labor.rs:202`, `V/economy.rs:647` |
| E6 | Shallow chains: axe and pickaxe are priced but have no recipe; gathering needs no tool; hammer's only use is a scripted sink | M | M | `V/catalog.rs:167-201,386,408`, `V/ai.rs:923` |
| E7 | NPC farming is patch gathering; crop plots are player-only with two hardcoded crops | M | M | `V/homestead/crops.rs:21-45` |
| E8 | No external sink/source (merchant boat); player-held coins leave circulation, untested deflation risk | M | M | `V/economy.rs:751-768`, `V/ai.rs:1050` |
| E9 | Morale and health have no consequences; no illness, herbalist or education | M | S–M | `V/colony/social.rs:91-102` |
| E10 | No weather or disasters; cold is only night or winter | M | M | `V/living.rs:155-160` |
| E11 | Heating is cheap; cold never damages health directly | M | S | `V/living.rs:281-311` |
| E12 | Goods, recipes, prices, roles, patches and endowments are Rust constants | M | M–L | `V/catalog.rs:8-452`, `V/model.rs:20-35` |
| E13 | Thin stability evidence: one seed, 14 days, `#[ignore]`, ~37 min | M | S | `tests/regional_economy_soak.rs:20,427` |
| E14 | One global price; no transport cost or inter-village differentials | L–M | M | `V/production.rs:1291` |
| E15 | Scripted remnants (innkeeper daily consume, food preference = index mod 3) | L | S | `V/ai.rs:1072-1083`, `labor.rs:207` |

### 2.2 NPC AI, authoring and socials

Works today: a compact BT runtime with reactive guards and a utility selector
(`crates/npc_ai`); a genuine uniform-cost GOAP search over goods, sources, coins and
position with explainable failures (`V/goap.rs:91-402`); a validated JSON `island_ai`
policy; stuck detours; a single teardown path for reservations.

| # | Gap | Sev | Size | Evidence |
|---|---|---|---|---|
| A1 | The BT is a thin shell: every branch runs one `IslandLifeDecision` mega-action; real logic sits in `ai.rs`, `production.rs`, `labor.rs` | H | L | `V/behavior.rs:205,316-344` |
| A2 | Trees are not data-authorable: `TreeDef` derives serde but nothing loads it | H | M | `crates/npc_ai/src/tree.rs:209-222`, `V/behavior.rs:250-285` |
| A3 | No hot reload; a policy edit needs a new world | H | M | `V/policy.rs:143-153` |
| A4 | Social state is write-only ✔ | H | M | `V/colony/social.rs`, `colony/summary.rs` |
| A5 | No relationships, family, gifts, reputation, festivals or mourning; NPCs never initiate speech | H | L | grep of `V/`; `V/commands.rs:456,480` |
| A6 | No individuality: global thresholds, role-fixed names, no traits | H | M | `V/policy.rs:22-31`, `labor.rs:40-46` |
| A7 | No schedules or shifts; leisure only when no job exists | M | M | `V/behavior.rs:242,311`, `V/ai.rs:444` |
| A8 | GOAP scope is "one more unit of a profession output"; excludes tools, skills, needs, social, construction | M | L | `V/goap.rs:198-237` |
| A9 | Full replan every 20 ticks, no plan cache or heuristic | M | S–M | `V/ai.rs:501-551`, `V/goap.rs:172` |
| A10 | BT lacks decorators, subtrees, parameterised conditions, named blackboard keys | M | M | `crates/npc_ai/src/runtime.rs:58-82,343-370` |
| A11 | Stuck handling dead-ends after three detours | M | S | `V/routing.rs:123` |
| A12 | No AI budget; O(n²) social pairs; full index scans per decision | M | M | `V/colony/social.rs:107-118`, `V/behavior.rs:396` |
| A13 | Two disconnected "why" views; rejected alternatives are not recorded | L | S | `V/goap.rs` explain, `V/ai.rs` |

### 2.3 Player modes, loop and UX

Works today: title → New/Continue/Oversee → play → pause in one window; a seven-workspace
character HUD with guidance; a colony ledger with pause/speed; recorded, validated
terrain edits that update navigation.

| # | Gap | Sev | Size | Evidence |
|---|---|---|---|---|
| P1 | No build menu or free placement in spectator mode | H | L | `W/colony_panel/model.rs:175-205` |
| P2 | Resource overview is two numbers; chains are static text | H | M | `W/colony_panel/model.rs:57-70` |
| P3 | No alerts or event log | H | M | `W/colony_panel.rs:300,510` |
| P4 | No goals after the first cottage; one house and one garden | H | L | `V/player_journey.rs:440-472` |
| P5 | Terraform is a single-click absolute stamp: no drag, relative raise/lower, smooth, undo, prop or resource placement, or "save as map" | H | M→L | `W/colony_panel.rs:386,455,649` |
| P6 | Relationships are greet + advice only | M | M | `W/player_hud/model.rs:440-496` |
| P7 | Three modes share one cramped ledger; no persistent mode switcher | M | M | `W/colony_panel/model.rs:210-438` |
| P8 | No settings (window, UI scale, volume, keybinds); no gamepad | M | M | `apps/actor_testbed/src/frontend/ui.rs:236-547` |
| P9 | Time controls inconsistent across modes; presets only 1/3/10× | M | S | `apps/actor_testbed/src/lib.rs:116,464,763` |
| P10 | No day structure: no sleep-to-morning, no day summary; one item per click | M | M | `docs/island-life.md:66,112` |
| P11 | No overlays, profession table or settlement statistics | M | M | `W/colony_panel/model.rs:43-56` |
| P12 | Debug keys (F1, F3–F8) and "NPC Framework" title leak into the player session | M | S | `W/lib.rs:183`, `apps/actor_testbed/src/lib.rs:420` |
| P13 | Player docs contradict the shipped world (12 vs 42 residents, save name, height range) | L | S | `docs/island-life.md:3`, `docs/island-colony.md:15,121,146` |
| P14 | "Play character" is buried in the ledger; no mouse orbit | L | S | `W/colony_panel.rs:261`, `W/camera.rs:140` |

### 2.4 World, atmosphere and presentation

Works today: chunked 256×256 terrain with five biomes and season tint; orbiting sun,
dusk tint and distance fog; lantern lights; hearth smoke; machines that turn only while
crafting; tree patches that shrink and regrow; resident captions and thought bubbles.

| # | Gap | Sev | Size | Evidence |
|---|---|---|---|---|
| W1 | No audio at all ✔ (feature enabled, zero uses, no sound assets) | H | M | grep; `W/../Cargo.toml` |
| W2 | Settlements are near-identical blue-roofed grids | H | M | `W/living_scene.rs:798-819`, captures |
| W3 | The harbour has no coast identity: no pier, dock or boats | H | M | grep of `V/frontier`, `W/living_scene.rs` |
| W4 | Roads are 6–8 m slabs with stair-stepped diagonals; flat per-cell colour; ridge stone reads as a checkerboard | H | M | `W/living_scene.rs:379-405`, captures |
| W5 | One generic work animation (Work/Craft/PickUp → "Wave"); no sleep, sit, fish, carry | H | M | `W/anim.rs:54-61,90-109` |
| W6 | Weak nights: no lit windows, hard lamp switch | M | S–M | `W/living_scene.rs:892-898,1040` |
| W7 | No post-processing (HDR, bloom, tonemapping) | M | S | `W/camera.rs:86` |
| W8 | No weather; winter is one grass recolour | M | M | `W/village_world.rs:1762` |
| W9 | Wildlife are cuboid deer; no birds or livestock | M | M | `W/village_world.rs:881-967` |
| W10 | Near top-down play camera; hard biome wedges; residents unreadable at settlement zoom | M | S–M | `W/island_camera.rs:184` |
| W11 | Carried goods and stock levels are invisible | M | M | `W/visuals.rs:1069` |
| W12 | Smoke is three faint spheres | L–M | S | `W/living_scene.rs:903-908` |
| W13 | Water mesh is rebuilt on the CPU at 10 Hz; no shoreline | L–M | M | `W/living_scene.rs:1017-1023` |
| W14 | Explore chart is three dots on a blank rectangle | L | S–M | capture |
| W15 | Unmeasured performance; thousands of separate foliage scene roots | M | M | `W/living_scene.rs:622-694` |

### 2.5 Engineering and product health

Solid: enforced determinism and architecture fitness, versioned world spec, atomic saves
with backup, ~1,960 tests.

| # | Risk | Sev | Size | Evidence |
|---|---|---|---|---|
| X1 | The island has no protected replay or golden; all eight replays and six snapshots guard legacy content | H | M | `tests/replays/`, `**/snapshots/` |
| X2 | No measured island performance; the soak implies roughly 7.5× real time for 42 residents (unconfirmed build profile) | H | S→M | `bench/baseline.json`, validation note |
| X3 | Patterns that break at 150–300 residents: O(n²) meetings, full index scans, one global nav budget | H | M–L | `V/colony/social.rs:118`, `V/ai.rs:98,193`, `crates/npc_nav/src/lib.rs:116` |
| X4 | Goods, recipes, roles and buildings are Rust (same as E12) | H | L | `V/catalog.rs` |
| X5 | No save migration: strict version 1; any new good invalidates saves | H | M | `crates/npc_kernel/src/snapshot.rs:45,494` |
| X6 | Profile accretion: five island presets, three registration tiers, runtime branching | M–H | M | `content/testbed_content/src/worlds.rs:41-55` |
| X7 | The game lives in crates named `testbed_*` / `actor_testbed` / `game_menu`; eight view files of 1.3k–2.9k lines | M–H | L | `wc -l` |
| X8 | Roughly 55–60k of 236k lines are legacy demos (explorer, AI client, developer menu, RTS, old village) | M | M | `wc -l` |
| X9 | The gate is slow: full verify ~50 min; quick tier still runs legacy suites | M | S–M | `xtask/src/main.rs:41-45` |
| X10 | Not distributable: compile-time absolute asset path, no release profile or packaging | M | M | `W/assets.rs:16` |
| X11 | 306 markdown files, eight overlapping island docs, no GDD or roadmap | M | M | `docs/` |
| X12 | Native verification relies on BRP widget events; physical input never verified | M | M | validation notes |

## 3. Target experience

- **Pillar 1, the village lives without you.** Residents are born, grow up, pair off,
  work, age and die. They build what they lack, store and haul goods, and change trade
  when margins shift. A soak of four in-game years stays alive across several seeds.
- **Pillar 2, everything is real.** No granted goods, no scripted sinks, no wealth reset.
  A price is a consequence; a shortage has a visible cause and a visible response.
- **Pillar 3, three ways in, one world.** *Oversee* (Banished: build, assign, set
  priorities, read alerts), *Live* (Stardew: a day rhythm, a home that grows, neighbours
  who remember you) and *Shape* (editor: sculpt terrain, place resources and plots).
- **Pillar 4, authored in data.** Goods, recipes, professions, buildings, traits,
  schedules and behaviour trees are versioned data validated at load.
- **Pillar 5, a place with a mood.** Each settlement is recognisable by silhouette and
  colour; nights glow; work has sound; weather changes the day.

## 4. Roadmap

Each phase leaves the game runnable and ends with focused tests, a headless soak and
`cargo xtask verify --quick`. Constraints that apply to every phase:

- All new mechanics are registered in the `living_world` tier only. Legacy profiles,
  the eight protected replays and six snapshots must keep passing unchanged.
- Integer state, `SimId`-sorted iteration, `sim_rand`, recorded commands (`.claude/rules/sim.md`).
- Protected files (`xtask/src/main.rs`, `arch_fitness.rs`, `deny.toml`, `clippy.toml`,
  goldens, replays, CI) are not edited. Items that need them are listed in §5.

### Phase 0: baseline and hygiene (S)

- Build; record a release `living_world` tick-rate measurement at 42 residents
  (scratch evidence in `docs/_notes/`, not `bench/baseline.json`).
- A fast multi-seed island soak test (3 seeds × 2 days, not ignored) plus an ignored
  one-year variant. Assert alive, housed, currency conserved, no negative stock (E13).
- Hide debug keys and legacy panels from the player session; window title "Island
  Village" (P12). Fix contradictory player docs (P13).

### Phase 1: an economy that answers to prices (M)

- Replace the wealth cap with a proportional levy above a threshold that funds civic
  purchasing; keep a destitution floor paid from the fund (E3). Conservation test stays.
- Add realised margin to job ranking and labour scoring; widen flexibility; shorten
  switching inertia when a margin gap persists (E4).
- Tools matter: axe, pickaxe, hoe and hammer recipes with durability; gathering speed
  and eligibility depend on the tool (E6). Remove the scripted innkeeper sink (E15).
- Cold damages health below a threshold; firewood consumption scales with season (E11).
- Morale modulates work duration within a bounded band (E9, A4).
- Plan cache for GOAP with precondition invalidation (A9); stuck exhaustion fails the
  leaf and blacklists the target (A11).
- Tests: glut lowers output; shortage raises it and attracts labour; buy-out and
  coin-hoarding scenarios recover (E8 test half).

### Phase 2: data-driven catalog and behaviour (L)

- `economy` data block: goods, recipes, professions (outputs, wants, tools, aptitude),
  resource patches, buildings. Loaded and validated before freeze, copied into the save's
  construction data like `island_ai` (E12, X4). The shipped file reproduces Phase 1 values.
- Behaviour trees from data: `trees/*.json` in the `TreeDef` format, validated by the
  existing freeze. Split `IslandLifeDecision` into granular leaves (go to, buy, gather,
  craft, eat, drink, rest, socialise, haul, build) and named conditions (A1, A2).
- Per-resident traits (diligence, sociability, chronotype, thrift) and schedule blocks
  (work, meal, leisure, sleep windows) as data read by conditions (A6, A7).
- Decision record in the journal: candidates, scores, rejection reasons (A13).
- Developer-only reload of trees and policy into a new world epoch (A3) is deferred
  unless the content-hash rules allow it without a new world; otherwise "New world from
  edited data" stays the documented loop.

### Phase 3: a village that grows (L)

- **Buildings.** `BuildingDef` (cost, footprint, station or storage kind, capacity, jobs).
  `PlaceBuilding` recorded command validated like terrain edits; construction site with
  delivered materials and staged work (E2, P1).
- **Storage and hauling.** Stockpile and barn entities with inventories; market stall as
  a seller of record so trade no longer needs the producer present; haul jobs (E5).
- **Civic planner.** Per settlement: commissions housing on bed shortage, storage on
  overflow, workplaces on persistent unmet demand; funded by the levy (E2).
- **Demography.** Age and life stage; couples from affinity; births gated on bed and
  food reserve; children consume and learn; old-age death; household inheritance of
  coins and home; arrivals by boat when housing and food allow (E1, A5 family part).
- **NPC fields.** Sown and harvested crop plots with seasonal windows using `CropDef` (E7).
- **Merchant boat.** Periodic visitor with a finite purse and stock: the only external
  source and sink, journaled (E8).
- **Weather and shocks.** Deterministic daily weather (clear, rain, storm, snow)
  affecting growth, warmth and travel; drought and blight as rare seeded events (E10).
- Scale work as population rises: spatial queries for social, per-settlement indices,
  staggered planning (X3, A12).

### Phase 4: three modes worth playing (L)

- Persistent mode switcher **Live / Oversee / Shape** and a shared clock widget
  (pause, 1×, 3×, 10×, and faster if Phase 0 measurements allow) (P7, P9, P14).
- **Oversee:** build menu with placement ghost and rejection reason; per-settlement
  stock table with trend; profession table with +/−; priorities; alert feed with
  click-to-focus; overlays for homes, jobs and resources (P1, P2, P3, P11).
- **Shape:** drag painting; relative raise/lower; smooth and flatten; brush falloff;
  undo/redo as inverse recorded commands; place resource patches, plots and props;
  export the edited world as a new world file (P5).
- **Live:** sleep-to-morning with a day summary; repeat-gather; gifts and friendship
  tiers with real effects (trade terms, help); homestead tiers (cottage upgrade,
  storage, second plot, tool tiers); a season calendar with village gatherings (P4, P6, P10).
- Settings screen: window mode, UI scale, volumes, key remap (P8).

### Phase 5: atmosphere (M)

- HDR, bloom, tonemapping; warm lit windows for occupied homes; lamp fade on the dusk
  curve; stronger chimney smoke; forge sparks (W6, W7, W12).
- Activity poses mapped to existing clips: sleep, sit, carry, chop, hoe, fish (W5);
  carried bulk goods and stock piles that fill visibly (W11).
- Per-settlement roof palette, street shape and one landmark each; harbour pier and
  moored boats; narrower blended paths; clustered ridge stone (W2, W3, W4).
- Weather visuals for Phase 3 states; seasonal foliage tint (W8).
- Audio: ambience by biome and time of day, work one-shots, UI clicks, light music
  (W1). **Needs CC0 sound files: see §5.**
- Foliage instancing and visibility ranges, measured before and after (W15).

### Phase 6: review and polish pass

Fresh multi-perspective review against this document, a four-year multi-seed soak,
native smoke of all three modes at 1280×720 and 1024×768, then a fix pass.

## 5. Decisions that need the owner

**Owner decision, 2026-11-10: all five approved.** The approval covers exactly these
items: recording new island replays/goldens, editing `arch_fitness.rs` and `deny.toml`
for the crate rename and legacy gating, an island quick tier in `xtask/src/main.rs`,
downloading CC0 audio, and breaking existing local saves until migration lands. It does
not cover changing any existing legacy replay, snapshot or bench baseline.

**Owner decision, 2026-11-11:** downloading a CC0 icon set for the UI redesign is
approved ("icons are approved, use a CC0 set"). Attribution-licensed sets are not covered.

1. **Island replays and goldens (X1).** Recording a protected island baseline follows
   `docs/rebaselining.md`: staged by the agent, approved and swapped by the owner.
2. **Crate rename/split (X7) and legacy feature-gating (X8).** These touch
   `arch_fitness.rs` and `deny.toml`, both protected. Until approved, legacy surface is
   hidden at runtime only and modules are split inside the existing crates.
3. **Faster gate (X9).** An island-focused quick tier needs `xtask/src/main.rs`.
4. **Audio assets (W1).** Sound files must be downloaded (CC0 packs); the owner chooses
   or approves the source.
5. **Save compatibility (X5).** Phases 1–3 change the content hash; existing local saves
   will not continue. Proposed: accept this until Phase 3 lands, then add migration.

## 6. Not verified by this review

Frame rate and GPU behaviour; physical keyboard and mouse input; how the game looks in
motion and in seasons other than autumn; whether a player buy-out actually causes a
starvation spiral; real per-tick cost in a release build; save size and time.
