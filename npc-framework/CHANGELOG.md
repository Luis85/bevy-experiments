# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) once the `npc_framework` facade is published.

Bevy compatibility: `0.1.x` (unreleased) targets Bevy 0.20, Rust ≥ 1.97.1.

## [Unreleased]

### Island review fix pass (sim)

- Shape placements are built: `PlacePlot`/`PlaceProp` take `mode` (`build` opens a
  site with the catalog cost and work, `editor` needs `SetEditorMode`); prop sites
  (`SiteKind::Prop`, at most 8 open). Spears, rods and hammers wear (`durability` in
  data; 150% build time without a hammer). The boat's prices and hold drift
  (`merchant.drift`) and its order follows island shortages. Food spoils
  (`shelf_life_days`, `village::spoilage`). A market stall takes any traded good.
  Any good with a `food_value` is eaten; every `fuel` burns.
- Views for the interface: `price_line(s)`, `good_flow`, `need_alert`, `outlook`,
  `season_summary` (season ledger), `consigned`, `merchant::outlook`, `editor_mode`.
- Saves: stricter identity (`OtherBuild`, stamps checked, state hash checked), the
  world rules travel with the save (`saved_rules`), `ISLAND_SAVE_VERSION` 4.
- Behaviour: a tree per trade or life stage, guards with parameters and `not`,
  everybody has an evening, family evenings; `ReplaceBehavior` replaces the
  behaviour document, trees included, at a tick boundary (ADR 0045, additive
  `npc_ai::AiRegistry::replace_trees`).
- Day rhythm: one sit-down meal a day at home or the inn (`life.meal_due`,
  `life.sit_down`, `DayRhythm`); meetings last `chat_ticks` and pay at their end
  (`Chatting`); buyers keep their seller (`Errand`); a building stage a critical
  need interrupts is resumed (`SuspendedWork`). Hired builders use the hammer
  factor; drinks and garments are found by tag.
- Replays stamp the content hash of their first tick (`InitialContentHash`), so a
  replay with a `ReplaceBehavior` verifies; existing replay files are unchanged.
- Fix: a world chosen for New Game now replaces the window's construction (the
  construction is saved and loaded with its economy and roles), so its save
  continues.
- Closing fixes: a gathering of the calendar ranks above non-critical
  tiredness, the meal and the chores (tree data `Gathering`, `life.gathering`),
  and wakes a daytime rest; selling to the boat ranks above the evening; a
  meeting starts only between residents not on an errand; the boat orders
  only what the island can deliver by its next call (made by a recipe or a
  crop, or a gathered good nobody eats, not spoiling before then, and not
  lacking entirely) and the island lays the order at the post ahead of the
  call. The Rust economy tables the data replaced are removed (`price`,
  `gather_tool`, `role_tool`, `FRONTIER_TRADE_KINDS`).
- Soak fixes: a household decides on a child from the food it can count on
  (its members' packs and its share of its settlement's stores); every granted
  unit of a durable good carries its durability (founders' spears, rods and
  hammers wear); the last tailor of a village stays while somebody lacks a coat;
  the boat sells only goods that are dear on the island, within a trade balance
  (`merchant.trade_balance_percent`, 5), and does not buy grain while flour or
  bread is dear; a thirsty resident fetches water at a nearer well instead of
  walking to a far seller; meetings come every `meet_cooldown_ticks` (2400) and
  pay `meet_affinity` (1500); the season ledger closes before the new day's
  flows and counts the days a village ran short of food, water or fuel.
- Last sim round: a staple's need about to go unmet is demand at the makers' cost
  (`market::price_for`, `stock::reservation`), so bakers bake for residents short
  of coin; floor grants never exceed the day's levies; the island is founded
  with couples (`demography.founding_couples_percent`, 40); an arriving
  character's needs stay full; Jobs reasons are plain sentences; the ledger
  turns on the calendar's season; a save layout this build cannot read is
  another build's; a blocked hunter re-paths.

### Island game: review fixes (saving, startup, keys, words)

- Saving: Save game and Ctrl+S write your save; the minute timer writes
  `<stem>.autosave.json` beside it, removed on a clean exit. After a crash
  the title offers "Continue from the autosave" when it is newer. The pause
  menu says how old both are and offers "Quit without saving", which asks
  first, with Back as the default. When saving on the way out fails, the
  game offers to quit without saving, and a second close asks for it. A
  failed autosave stays as a toast until one succeeds.
- Backups: the next backup is numbered after the highest, and the five
  newest are kept. An island from an older version, another build or a
  newer version is kept as `<stem>.older-<n>.json` and never deleted.
- Startup: a save that cannot open, older or other-build or newer saves,
  and unreadable island rules each get a card on the title with the file
  name, the backup it will become and "Copy the folder path". If the
  game's own files cannot build the island, a window says so instead of
  `exit(2)`. Continue uses the sim's `load_island_save` as its single gate,
  built from the rules the save carries.
- New game: once the new island exists the player stays on it; if Live
  cannot start, Oversee opens with a notice. A failed rollback stops every
  save and asks for a restart.
- Sleep: pausing while asleep keeps the player's own speed, and Resume
  never restores 30x.
- Keys: the RTS order keys, `.`, F9, F2 and F11 are developer keys or
  removed. In Shape, Remove patch is X (E turns the camera), R turns a held
  plot or prop without changing the tool, and the brush keys wait while
  one is held. Ctrl chords never move the camera, and Ctrl+S works on the
  pause menu. Patches join Undo and Redo.
- Shape placement goes through building sites, with cost and work shown
  before placing. "Place for free" is the recorded `SetEditorMode`, and
  Undo cancels a site.
- Behaviour: the host saves `behaviour.json` beside the save and reloads it
  through `ReplaceBehavior` after the sim's own check (`island_view::reload_from_text`).
- Words: a New screen with Live first and Shape as a link, "Continue in
  Oversee", plain host errors with a "Details:" line, and the glossary
  sweep across the panels.

### Island view: review fixes (fog stub, audio, wildlife, assets)

- **Fog:** the island's `fog.rs` is a stub. The fog is always off, so the mask plane, its per-tick walks and the dimmed materials are gone. What you see does not change.
- **Audio:** `island_audio` logs at debug level, so a sound capture needs `RUST_LOG=info,island_view::island_audio=debug`.
  - The work-sound pass no longer allocates every frame.
  - The Morning and Built jingles play only during play, never on a load.
- **Wildlife:** it is read once per sim tick, and the fallback box-deer assets are made only when they are used.
- **Assets:**
  - The skeleton model kinds and the testbed prop models (walls, doors, trees, siege ram, cart) are removed from the island view, together with their visuals. The island spawns no such props.
  - `assets/hex`, `assets/furniture`, the axe prop and the grass texture are now listed in `living-world-manifest.json`.

### Island Village in its own crates; older island tiers removed

No backwards compatibility: read the "not kept" bullets before updating.

- The island is three new crates: `content/island_content` (the
  `living_world` simulation, its scenario, world rules, resident tree and
  tests), `content/island_view` (presentation) and `apps/island_game` (the
  shipped game). They were split out of `testbed_content`, `testbed_view`
  and `actor_testbed`. No island crate depends on a testbed or village
  crate, and the reverse; `arch_fitness` enforces both directions.
  ADR 0044.
- `island_game` is the game binary: `cargo run --locked -p island_game`
  opens the title (New/Continue, play, pause in one window). `--scenario`,
  a bare `--island-save`, `--island-new`, `--commands` or `--record` select
  the developer host; `--front-end` keeps the player flow with a custom
  save. `scripts/package_island.ps1` ships `island_game.exe` and leaves out
  the retired, untracked `assets/audio/` folder.
- `game_menu` is the developer menu only (Actor Testbed, village, RTS demo,
  editors, AI explorer). With no flag it opens that menu; `--developer-menu`
  is still accepted. It no longer depends on the `actor_testbed` library and
  has no island entry, island quit path or Resident AI page.
- `actor_testbed` hosts the testbed only: default scenario `testbed`; the
  flags `--front-end`, `--dev-keys`, `--island-*`, `--ui-gallery`, `--mute`
  and `--world-state-*` are gone.
- `npcctl` knows three contents: `village`, `testbed[:<name>]` and
  `island:<name>` (a bare `living_world` qualifies to `island:living_world`,
  also in `npcctl replay record`).
- Island saves live in the island's own folder:
  `<home>/island-village/island.world.json`, rules beside it as
  `living_world.json` (`island_game::{ISLAND_HOME_DIR, SAVE_FILE,
  RULES_FILE, island_home}`).
- One construction: `island_content::construction::shipped()` (the
  embedded `living_world` scenario with the built-in world's descriptor) is
  what `try_new_sim_app`, every test and the replay factory build; New Game
  builds `construction::from_preset` of the chosen world. Headless
  perception memory goes from 128 to 256 entries and the headless world
  gains `RevealsFog`. `worlds/living_island.json` and the scenario file's
  inline `living_world` block are removed; an island scenario without its
  construction is refused.
- The island's content hash and every island state hash change once.
  Dropped inherited registrations: the `TestbedDrive` command, the wander
  and worker trees, the walk/wander/tool-work actions and `TbWalking`, the
  `book`, `scroll` and `store_key` item kinds and the study table, the
  skeleton sheet templates, the fog-statics and LIDAR-obstacle shaping.
  Renamed: `ExplorerContext` to `construction::Construction` (JSON
  `explorer_context` to `construction`); `TestbedPlugin`,
  `TestbedAgentPlugin`, `TestbedViewPlugin`, `TestbedPanels`, `TestbedIds`,
  `TestbedUnit`, `TbIdling` to `IslandBasePlugin`, `IslandAgentPlugin`,
  `IslandViewPlugin`, `IslandPanels`, `IslandIds`, `IslandUnit`,
  `IslandIdling`; stable names `testbed_unit`, `testbed_prop`,
  `testbed_ids`, `tb_idling` to `island_unit`, `island_prop`, `island_ids`,
  `island_idling`; action `TbIdle` to `IslandIdle`, tree `testbed_idle` to
  `island_idle`; verb gates `testbed.allowed_actions` and
  `testbed.world_rules` to `island.allowed_actions` and
  `island.world_rules`. `ISLAND_SAVE_VERSION` is 3.
- Removed, older island tiers: scenarios `ai_village`, `living_village`,
  `island_colony`; world presets `island_village`, `living_island`,
  `living_homestead`, `island_colony`; the profile gates
  (`frontier::enabled`, `needs_driven`, `geography::is_expanded`,
  `colony::enabled`, `market::enabled`, `catalog::frontier_profile`) and
  the code only they reached: the fixed economy, the Rust `island_life`
  tree, the fixed spawn tables, compact geography, the Rust recipe and item
  tables, the legacy 64x64 terrain brush, the legacy hunting, repose and
  production paths. `living_world` is the only island construction.
- Removed from the island view: the Home & farm panel (`homestead_panel`,
  F2), the legacy terrain picking, robot bodies, the key and scroll models
  and the fog toggle (F8, `--fog`; the island's fog is always off).
  `island_game` does not accept `--panels explorer|map`.
- Removed: islands in the explorer and AI-agent hosts. `actor_testbed
  --explorer-home`, `npcctl serve --explorer-home` and the developer menu's
  explorer serve testbed worlds only (default `plain_explorer`). The island
  briefing, the village frame and notes bridge in `explorer::host`, and the
  world projection file (`--world-state-out`, `projection.rs` in both hosts,
  the run-file handover) are gone. An agent reaches the island through
  `island_game --remote` (`npc/*`, `agent/*`, `world/state`).
- Removed from `testbed_content`: `village/**`, `Layout::Island`, the island
  fields of `WorldSpec` and `WorldPreset` (`living_world`, `colony`,
  `island_ai`, `economy`, `island_geography`, `scene`), `EconomyModel`,
  `IslandGeography`, `WorldScene`. Explorer documents serialize as before.
- Not kept: saves. A save of an older tier or an earlier build, including
  any save made before the renames above, is refused with one sentence and
  never migrated (ADR 0042). `living-world.world.json` and
  `local-island.world.json` in the agents folder are not read.
- Not kept: names. The removed scenario and world names load nowhere;
  `testbed:living_world` is now `island:living_world`; island reflect type
  paths and perf tags start with `island_view::` / `island_content::`.
  Explorer workspaces created on an island world cannot be resumed.
- Tests: the island tests moved to `content/island_content/tests`. Deleted
  with no replacement: `world_projection`, `island_scheduler_events`,
  `island_notes_lifecycle`, `npcctl`'s `island_playtest`, the island
  briefing tests, explorer credential succession on an island, the
  explorer-built short-year and three-seed seasonal fixtures. Older-tier
  tests were swapped to `living_world` by name; the 228 that then failed
  (or were older-tier by fixture) were deleted, emptying 18 test files; the
  lost coverage is listed in ADR 0044. New:
  `content/island_view/tests/plugin.rs` (the whole island view, headless).
  The 8 replays, 6 snapshots, proptest regressions and `bench/baseline.json`
  of the village and the Actor Testbed are unchanged. The island has no
  protected replay or golden yet.
- Docs and scripts: removed `docs/ai-village.md`, `living-homestead.md`,
  `living-island-economy.md`, `island-village-playtest.md`,
  `island-village-stream.md` and seven older capture and smoke scripts in
  `scripts/playtest/` (`capture_island_colony`, `capture_colony_menu`,
  `capture_menu`, `capture_sessions`, `client_smoke`, `live_smoke`,
  `test_client_smoke`); `capture_native.py` and
  `capture_native_homestead.py` stay as the kept scripts' helper modules.
  The kept capture scripts start `island_game.exe`;
  `capture_player_flow.py` now passes its own `--island-save` so it never
  opens the player's real save.

### Living world: Oversee targets and priorities, Shape plots and props, trades from data

- Profession targets: `SetProfessionTarget { settlement, profession, wanted }`
  (`wanted: null` clears) sets a wanted headcount that the labour review
  moves toward through the usual trade-change rules; view
  `frontier::oversee::profession_table` (current, wanted, free seats and a
  reason when they differ). ADR 0043.
- Village priorities: `SetVillagePriority { settlement, topic, profession?,
  level }` over `housing`, `storage`, `workplace`, `food_reserve`,
  `fuel_reserve`, `coats`, `boat_orders` (`low`, `normal`, `high`); normal is
  the old behaviour; view `frontier::oversee::priorities` with one sentence
  per level. ADR 0043.
- Shape: `PlacePlot` / `RemovePlot` (farm `field` or the new `garden`
  plot, finished and empty) and `PlaceProp` / `RemoveProp` (props from the
  new catalog list `economy.props`: barrel, crate stack, fence piece, flower
  bed, signpost, flag, wheelbarrow, each with its model); exported worlds
  carry the plots in `buildings` and the props in `props`. Views
  `frontier::props::{catalog, props}`. ADR 0043.
- A trade is its data key: `Role::Trade(key)` for a trade only the catalog
  defines, `Role::from_key`/`key`; every role reads and writes as its key
  string. Living-world logic no longer branches on a named trade
  (`economy_spec::{trades, is_service, open_site, trade_label}`); a trade,
  its goods, recipe, patch and workplace can be added in data alone
  (`tests/island_data_trade.rs`). The planner may build the first workplace
  of a catalog trade a settlement lacks. `RegionalOccupationChanged` adds
  `from_key`, `to_key`, `from_trade`, `to_trade`. ADR 0042.
- `ISLAND_SAVE_VERSION` is 2.

### Living world: coats from the start, stalking hunters, save identity (pass 5)

- A resident who carries a coat and wears none puts it on at once, in any
  season (`VillageCoatWorn`). Two founders in three come wearing a coat
  (`tuning.founder_coats_per_three`, 2); each settlement is founded with its
  tailors' workshop built and each tailor holding one batch of coat
  materials (`tuning.found_coat_workshops`, true).
- The boat orders 3 goods a call (`merchant.order_goods`); its purchases
  against an order no longer move the island's quotes or trade margins.
- A hunter keeps its prey and re-paths only once it moved 3 m; the stalked
  animal does not flee from that hunter. `tuning.wildlife_motion` (true).
- New view data: `view::plan_lines` (raw and readable planner lines, no raw
  keys in `readable`), `view::activity_progress_permille`,
  `view::workplace_name`; `goap::explain_readable`.
- Goods carry a `drink` tag; `catalog::good_with_tag`, `catalog::has_tag`
  (ADR 0042).
- Island saves carry an identity: `village::island_save` (`save_island`,
  `check_island_save`, `load_island_save`, `IslandSaveCheck`); only a save of
  this build loads, every other is refused with a typed check (ADR 0042).
- The roles of goods and trades are catalog data (`village::roles`): new good
  tags (`timber`, `lumber`, `metal`, `ore`, `meat`, `hide`, `leather`, `yarn`,
  `fiber`, `baked`, `cooked`, `foraged`, `grain`), profession fields
  `hunt { tool, yields }`, `builds`, `sows`, `display`; a living world missing
  a role is refused at load naming it. The Actor Testbed keeps
  `roles::TESTBED`.
- `view::upgrade_stage(world, unit) -> Option<(ticks_left, ticks_total)>`:
  the stage of work under way at a homestead upgrade's door.

### Living world: coats, boat orders, a fund for building, deer that live

- Goods a resident has no use for (`market::no_use`) are carried whole to a
  store, which sells them (`tuning.unused_goods_to_market`).
- `hunting::available_prey` and the habitat count query the wild animals
  instead of every entity (same animals, same order).
- A resident moved to another home while resting in the old one wakes.
- Tuning `grants_spare_sites` (true) switches the floor-grant rule.
- Coat makers keep a shelf of `max(2, residents / tuning.coat_shelf_residents)`
  (15) coats while anybody lacks one, in every season, from their own purse.
- Outside the coat season a resident without a coat is granted its price only
  from the fund's balance above its reserve (`tuning.coat_grants_any_season`);
  `life.coat_season` holds then while a coat is on offer it or the fund can
  pay (`living::coat_wanted_now`, `living::coat_grant_room`).
- The boat announces at each call what it will buy at the next
  (`merchant.order_goods` 2, `order_units` 10, `order_price_percent` 130):
  financed demand in the port's plan, paid from its purse, expiring unfilled
  when it leaves (`VillageBoatOrder`, `VillageBoatOrderClosed`);
  `BoatView.next_order: Vec<merchant::BoatOrder>`.
- Floor grants no longer take the coins the fund holds for open sites; sites
  spend `civic::site_budget`.
- Deer graze, wander inside their range, keep 1.5 m apart, gallop from a
  resident who comes close and settle again (`wildlife::WildlifeMotion`,
  `view::wildlife()`); births keep 1.5 m from every animal.
- A hired builder starts work anywhere within the door's 4 m reach (the owner
  may stand on the door point), its claim is renewed while it walks, and a
  builder on its claim who stands still reads `waiting`, never `walking`.

### Living world: nobody freezes, raw prices, service trades

- Nobody freezes (ADR 0041): the resident tree opens with `life.freezing`
  (Warmth below `tuning.freezing_warmth`, 30000, without a coat, unless a lower
  critical Thirst or Hunger), so the lowest critical need is served first; a
  coatless resident in the cold below `tuning.exposure_warmth` (50000) starts
  no outdoor errand and idles at home; from `tuning.coat_lead_days` (3) before
  winter `life.coat_season` makes a coat a priority purchase, the civic fund
  pays the price a resident cannot (`VillageCoatGrant`) and tailors stock a
  coat for everybody without one.
- Raw goods have stock cover (`tuning.raw_stock_cover`): every traded good's
  island target comes from its daily use, so stock on offer up to it is not a
  glut and a gathered good's quote no longer walks down to 1 coin while supply
  only matches demand.
- The inn is a service trade (`service.wage_floor` in data): an inn without a
  keeper is staffed from a trade with workers to spare, its keeper stays, and
  the civic fund tops the day's takings up to the floor (`VillageServiceWage`).
- The trading post is no longer restocked from storage: it is the boat's
  stall, and restocking carried the harbour's food 90 m from its homes.
- The boat calls every 3 days, pays the island's base price for what it buys
  (berries and hides added) and asks 140% for what it sells. Its imports land at
  a market stall founded near the harbour centre (`merchant.market`), not at the
  waterfront post: shopping at the water had cut meetings, couples and births.
- A tired or cold resident far from home rents a bed at a nearer staffed inn
  (`tuning.inn_detour_m`), at the inn itself: in the living world `rest` is
  valid at the inn's entrance and pays the keeper wherever it is.
- A within-cover quote below base climbs back to base, as an above-base quote
  with nothing short already fell back to it.
- `tuning.idle_trade_days` (0, off by default): when set, a worker whose trade
  has had no work (goods above target, none short) for that many days may take
  a vacancy elsewhere, down to one worker of the trade. With 2 days the 14-day
  soak saw 319-364 job changes against 62-95 off, so it ships off. A keeper is
  staffed only from somebody whose switching cooldown has run out.
- Imports the civic fund cannot pay for are laid on the market stall on
  consignment (`merchant::MERCHANT_OWNER`) and paid to the boat's purse when
  bought; the boat no longer buys fiber or thread (it took a spring's fiber
  from the tailors).
- The production plan no longer counts a resident's hearth fuel reserve as
  supply for a building site, which never receives it: a site's logs are cut.
- A player's open homestead orders are production demand like a civic site's
  (`live::order_demand`): a carpenter saws ordered planks nobody has to spare.
- A coat grant is paid only when the coat purchase starts (journaled then; the
  coins go back otherwise), may draw on the fund's destitution reserve (not on
  building-levy coins), and in the coat season a resident the plan listed no
  coat for buys one directly. A guest asleep at an inn reads "asleep at the
  inn".
- `UpgradeSiteView.ordered_at: Option<u64>` (tick of the last order).
- `DaySummary` gains `bought`, `sold`, `bought_coins` and `sold_coins`.
- Births say why they wait (`VillageBirthDeferred` in the journal).

### Living world: physical homestead work, the boat as an outlet

- Island view: props placed with Shape are drawn from their catalog model (or a model matched by key, or a small crate), turned and on the ground, and go when removed; placed fields and gardens are drawn like founding fields; an open workplace of a data-only trade is a yard.
- Island view: a trade only the world's data defines gets a legend hue of its own (hashed from its key onto a palette kept apart from the shipped trades), the generic workshop (market) model, the hammer pose and its sound; an unknown good is stored as a crate. A prop table (`island_world::props`) maps placed props' data keys to manifest models, an unknown prop to a small crate.
- Island sound: one system (`island_audio`) on the manifest's CC0 files. Ambience layers cross-fade with the coast, woods, wind, rain and storm (thunder at gaps), birds by day, crickets by night, a village murmur and fires nearby; waves, leaf rustle, crickets and murmur are made in code. Work one-shots come from the poses and machines on screen (capped, thinned at speed, pitch varied), with quiet day and night music, jingles on a new morning and a finished building, and UI sounds through `PlayUiSound`. `AudioLevels` holds the volumes; `ISLAND_AUDIO_OFF=1` silences a run.
- Island view: a homestead upgrade's new part rises behind the cottage with the work (frame, walls, roof), its scaffold up once materials are in.
- Island view, phase A: rain darkens and glosses the buildings with the ground; a hired builder waiting at the door between stages faces the house; the harbour's rowing boat rocks on the swell; trees and grass far from the camera sway a quarter as often and an unchanged sway is not rewritten; the sea's swell is worked out once per grid point (a quarter of the wave sums).
- Island view, eleventh pass: the deer move as the simulation moves them: they follow its positions smoothly between its steps, turn toward where they go, walk and gallop at playback speeds matched to the ground, graze out of step, and fall once and lie still when dead; the view's herd spacing is now a small nudge that fades while they move.
- Island view, tenth pass: the field gate's crossbars are farm-timber brown; stags keep 2.5 m from each other and 2.15 m from does, so their antlers no longer run into each other; a builder working an upgrade stage at the door faces the house.
- Island view, ninth pass: farm fences, gates and the barrow are a mid brown; winter evergreens and shrubs stay dark green with snow on their tops (broadleaves bare, grass straw); the pier has a planked walkway, which also hides the blurred streak at its foot.
- Island view, eighth pass:
  - Ship, rowing boats, pier, field fences, gates and the barrow are dark oak with a mid-brown deck and trim (the noon hull samples R 146, R-B 44); signposts and bark are oak brown.
  - The ship lies at least 10 m out with a real pier to it, mooring posts along it; both harbour flags stand on land at its foot.
  - Deer keep 1.6 m apart and clear of trees, bushes and rocks; winter bushes and undergrowth are dark and grey, grass straw; depot barns and market stalls fly a yellow flag.
- Island view, seventh pass:
  - The ship, rowing boats and pier sections show their kit colours (brown wood, pale sails) through a recoloured palette.
  - The ship floats clear with room at bow and stern, the jetty is the kit's pier sections, the rowing boats are 3.6 m long (one ashore, one afloat), and the deer stand apart as a loose herd.
  - Gulls have swept, bent wings and glide.
  - The plain cone trees are gone: the woods and the wood patches are Kenney's faceted pines and broadleaves, and the broadleaves turn in autumn.
  - Sites show the kit's building stages, mills have sacks and woodcutters sawn wood, and inns fly flags.
  - The ground is worn at doors and along desire lines.
  - Shadow cascades follow the camera.
- Island view, sixth pass (models and motion):
  - The merchant ship is Kenney's `ship-small` (sail furled at the berth and set to sail in and out, pennants streaming downwind), with mooring posts, rowing boats, an anchor, fish and flags at the harbour.
  - Deer and stags are the Quaternius models, with clips chosen from the simulation.
  - The woods mix KayKit and Kenney trees (Kenney's greens recoloured by season) with undergrowth kept off roads, doors, bodies and fishing banks.
  - Fields are fenced, settlements have signposts, and homes have a mantel, firewood, a rug and a shelf.
  - Cloud shadows now read at island zoom (and go under an overcast), trees sway visibly each at their own pace, swells run over the sea, the shore's foam breathes, crops ripple, gulls follow the ship, small birds fly in flocks, and butterflies, fireflies and falling leaves come in their seasons.
  - Winter snow lies unevenly, the fisher's line reaches the water, and the watermill's wheel turns about its own axle.
- Island view: ambient motion. One wind for the whole island (from the weather, gusting, turning over the day) on a view clock that runs while paused: clouds drift and wrap, their shadows slide over the land at island zoom, trees sway, smoke and rain bend, the boat rides the swell and its sail shivers, lanterns and hearths flicker, gulls circle the harbour. Mills, wheels and saws spin up and down on the sim clock only while worked; deer breathe and graze. `AmbientMotion { scale }` stops all of it at 0. The boat floats clear of the beach and the post at the sim's berth or straight out from it; the winter beach is wet sand, not snow.
- Homestead upgrades are physical (ADR 0040). An order is carried to the
  cottage door by a resident of the settlement (from its spare stock, or bought
  at a store on the way) and paid from the owner's escrow only when it is handed
  over there; a hired builder walks to the door in working hours and builds in
  paid stages (`UpgradeWork`); a friend walks over in free time through the new
  `life.help` leaf to bring a material, work a stage or harvest a ripe bed. The
  civic planner never sees these sites. Cancelling returns goods and the whole
  escrow; a hauler keeps what it was carrying.
- `view::upgrade_site` reports `workers` ("Oak is bringing 2 log.").
- The trading post is founded at the waterfront beside the berth, the sea cell
  nearest to the harbour's centre (`view::boat().berth`); `placement.coast` lets
  a waterfront building stand on the coast.
- The merchant boat buys the island's surplus above the stock targets (residents
  of every settlement carry it to the post while the boat is in) and sells what
  the island is short of; purse 6,000, larger limits and logs on its list.
- The soak's journal reader read `(last, now]` and dropped every entry of a tick
  that is a multiple of the sample interval, which is when the boat sails: the
  `VillageBoatDuty` counter now reads `[last, now)`.
- Repeat gathering walks to its patch when out of reach; `view::activity_progress`
  gives done/total for gathering, crafting, a rest on the ground and a stage of
  homestead work; `view::catalogue` adds `reasons` (materials short, open-site
  allowance, homestead tier, placement); `RelationView.tastes_known`/`tastes`;
  `view::held` and an activity-aware `view::held_tool`; the homestead
  observation reports the real cottage body in the living world.
- Fishing grounds lie at the water: a fish patch is placed on the dry cell beside
  water nearest to its fisher's site (it stood on dry land up to 28 m from the
  pond). A resident gathering there reads "fishing" (`view::activity_label`), and
  `view::held` reports the fishing rod.
- Fix: a living-world save made after a hunt, a burial or a removed resource patch
  no longer fails to Continue ("Required island identity N is missing"). The
  structure check now requires only permanent construction identities and lets
  mortal ones go only the way the simulation removes them
  (`tests/island_save_continue.rs`).
- `WorkplaceView.solid` is now true for founding workplaces with a body (all but
  the well and the fishing spot), whose blocker is a separate entity.
- New resident leaf parameter `life.haul boat`: while the boat is in, residents
  carry stock it buys to the trading post (tried after `materials` in work hours).
- Acceptance: `regional_economy_soak` reports the 5th percentile of the
  per-sample lowest resident need on seeds 7, 11 and 23
  (`fourteen_days_on_three_seeds_meet_the_acceptance_bars`).

### Island interface redesign, phases 1 and 2

- Add one design system for the island interface in `testbed_view::game_ui`:
  design tokens (`tokens`), a widget kit with every state (`widgets`), two-level
  tooltips (`tooltip`), icons looked up by concept (`icon`) and an automatic
  interface size with layout classes (`scale`). Parchment panels over dark chrome;
  the optional Dusk panel style is a table swap. A contrast test recomputes the
  spec's pairs. Guide: `docs/island-ui.md`; decision: ADR 0039.
- Add 92 CC0 interface pictograms (Maki 8.0.0, Temaki 5.13.0) as two embedded
  atlases, with `scripts/fetch_ui_icons.py`, `assets/ui-icons-manifest.json`, the
  two licence texts and a credits section. Nine further glyphs are drawn in code.
- Rebuild the Oversee shell: a top bar with the mode switch, seven village totals
  with tooltips, weather, date and a segmented clock; an alert stack of three
  rows; bottom docks for Overlays and Places, the build categories and the
  ledger's pages (Village, Economy, People, Projects, Journal). The ledger is
  closed by default and shares one right column with the inspector.
- New Oversee keys: F1 to F5 (ledger pages), O (overlays), M (places), F (follow),
  Alt (tooltip detail). Esc undoes one step per press and no longer also opens
  the pause menu when it closed something.
- Settings: **Interface size: automatic** (the default; 150% at 1080p, 200% at
  1440p, times the existing percentage) and **Panels: parchment / dusk**, stored
  in `island-settings.json` as `ui_scale_auto` and `panel_style`.
- Remove the permanent hint text of the Oversee ledger and the keys written into
  bar labels ("Live · 1", "Menu · Esc", "Pause · Space").
- Add a developer widget gallery (`--ui-gallery PAGE`) and
  `scripts/playtest/capture_ui_redesign.py`.

### Island interface redesign, phase 3

- Rebuild the resident inspector on the parchment kit in the right column: a
  header with the trade's colour and icon, a status badge in words, and four
  tabs (Overview, Day, Why, Home and job). A resident chosen from the roster
  has a People button that leads back to it. The job and home commands move from
  the People page to the inspector's **Home and job** tab; the roster row still
  leads there.
- Replace the build palette with a build tray above the docks: cards with icon,
  cost as icon and count pairs and one line of purpose, the island's stock of
  the materials at the side, and a tag beside the cursor that gives the verdict
  in words, the simulation's reason, the cost, the footprint and the keys. The
  palette's hint lines and its Turn, Put down and Close buttons are gone (R,
  right-click or Esc, and B or the close button do those). Build categories are
  derived from the catalogue; a world with a crop-plot building gets **Farms**.
- Rebuild the building and site card in the right column, with a status badge
  and a progress bar for a site.
- Ledger pages: Economy groups goods, marks low ones, opens a detail card for a
  chosen good and moves professions to a **Jobs** view; Projects shows the civic
  fund as two numbers, sites as rows with a progress bar and material chips,
  folds the planner's view and decisions, and says **Abandon** where it said
  Cancel (the safe answer comes first in the question).
- Compact layout (less than 1024 by 700 after the interface size): the top
  bar's totals move to a second row, the weather folds into the date and the
  ledger dock becomes one Ledger tile.
- Choosing an already selected resident or building from a ledger row now hands
  the right column to its panel again.
- A click anywhere on a home or an inn, roof and outer walls included, selects
  it: the click box is the building's real body up to its chimney. An inn's
  card says who is inside now. The caption of a resident drawn inside a
  cut-away home sits over them, in bed or on the floor, not over the entrance.
  The Trades key hides while no marker is drawn (at night, with everybody
  indoors).
- Capture scripts: `scripts/playtest/capture_ui_phase3.py` is new; the older
  `capture_*.py` scripts find the renamed controls ("Menu", "Live", the ledger
  dock, the Journal page, the build tray) through shared helpers in
  `capture_player_flow.py`.

### Island interface redesign, phases 4 and 5, and phase 3 follow-ups

- **Live** is rebuilt on the design system. The goal tracker (top left) shows
  the journey, the next step, its progress and one button that names what it
  does; it folds to its title by its arrow, and by itself while a workspace is
  open. The needs (bottom left) are icon, word and bar, with a triangle below
  45% and a diamond below 20%; percentages are in the tooltips. The hotbar
  (bottom centre) has the seven workspaces and Stop with key caps. A prompt
  pill appears only for something in reach ("E Gather Water") or for work
  under way ("Walking, then: gather one item", with Stop). Outcomes and
  requested actions are toasts under the top bar; the permanent feedback line
  and the hint line are gone.
- Live workspaces open in the right column on the parchment kit, with an icon,
  a name, a status badge and Close. Craft and Trade are two panes wide, with
  the pouch beside the recipes or offers, where the window allows. A pouch
  item is chosen first; Eat, Drink, Wear and Offer for sale then appear for
  it. Recipes show their inputs as chips with have/need, the workshop and the
  seconds at 1x; a recipe that cannot be made keeps its place, disabled, with
  the reason beneath, and **Can make / All** narrows the list. The end of a
  life is a modal in the same style; the spectator's life choices are a chip
  with the key L.
- Tab from the top bar now goes on to the goal and the needs, then the hotbar,
  in Live.
- **Shape** is rebuilt: a tool rail (Raise, Lower, Flatten, Smooth, Paint, Undo,
  Redo and the history depth "12 of 32"), one brush bar (a Size slider with [
  and ], Edge Hard or Soft, four Surface swatches) and a tag beside the cursor
  ("Raise · ground at 6.3 m"). A refusal turns the outline red and dashed and
  adds the simulation's reason to the tag. Cell coordinates, the hint lines
  and "Landscape updated · N edits applied" are gone; what still needs saying
  is a short notice above the brush bar. All keys are unchanged.
- Projects: "Homes residents build themselves" replaces the count of "active
  construction projects" that disagreed with the sites listed above it.
- The tag over a selected resident is the design system's world tag: name,
  what they are doing and a status shape.
- The inspector says spans in time words ("They will look at their trade again
  in about 2 hours."), reads the day plan, the latest decision and the
  activity from the simulation's own view functions, and gains **Family and
  bed** on Overview (partner, parents, children, the bed and whether they are
  asleep, resting or awake).
- The Village page gains **Families**: households, couples, children, elders,
  and the births, arrivals and deaths since the island was founded (a young
  island says that nothing has changed yet).
- The calendar tells tomorrow's weather and the supply boat where the world
  has them.
- The goods table sorts by any header (again to reverse) within its groups; at
  the Compact size its last header is "Change" and a good's detail card is two
  lines. With totals on the bar and little room (1024 wide), the bar gives up
  the weather chip first and then the words of the modes.
- Building cards have tabs where there is data: Overview, Residents or Workers
  (at an inn, who is inside), Stock.
- The Paused pill and the notices keep clear of a two-pane Live workspace
  (they stand between the goal and the workspace then) and the pill gives way
  to the open calendar.
- An inn's card lists its guest beds: how many are taken, and each bed with
  its guest and when the rented rest ends.
- The People page ends with **Departed**: who has died, as one sentence each.
  An inspector open on somebody who has been laid to rest closes with a note;
  alerts name them from the record of the departed. The Journal notes burials
  and sleepers woken before morning.
- The developer homestead panel takes its colours from the tokens.
- `scripts/playtest/capture_ui_phase45.py` is new; it also sends Tab, Esc, B,
  F1 to F5 and the Live and Shape keys as key messages and reads back what the
  interface did.

### Island interface redesign, phases 6 and 7

- The title is a column on a bark gradient over the live island: Continue (with
  who and when on its second line) leads when there is a save, New game when
  there is none; Settings and How to play sit side by side; Quit is quiet; a
  credits line stands at the bottom.
- A new beginning asks for a name and then how to begin: **Live** (start on the
  island), **Oversee** (the villages first) or **Shape** (the land first), each
  with one sentence. Beginning in Shape is new.
- The pause menu is a panel over a scrim: who and when and how long ago the
  island was saved, Resume (Esc), Save game (Ctrl+S), Settings, How to play, and
  under a rule "Save and go to title" and "Save and quit". Its mode buttons are
  gone: the modes are in the top bar.
- Settings has four tabs. Sound: three sliders and Mute. Display: Interface size
  (Auto or Fixed, a slider, a line drawn at the chosen size) and Windowed or
  Borderless. Controls: camera pan speed, edge pan and the list of keys.
  Accessibility: Reduce motion, Tooltip delay, Show tips with "Show the
  first-run tips again", and Panel style (Parchment or Dusk). Sliders keep a
  step button at each end. Changes still apply at once.
- How to play is a Guide with five pages: Controls, Oversee, Live, Shape, The
  economy.
- Saving and loading answer on the island's toast lane in every mode: "Island
  saved" with a tick, "Island loaded. Welcome back, …" after Continue, and a
  failed save as a toast that stays, with its reason and "Try again". The old
  notice chip at the bottom left is gone.
- First-run tips: four in Oversee (totals, build dock, a resident, alerts), one
  in Live, one in Shape. One small card at a time beside what it explains;
  "Got it" dismisses it for good, the settings file remembers, and Settings can
  show them again. Show tips off hides them and the hint under the Live goal.
- Tooltips: workspace rows (what it does; then what it takes and why not),
  recipe input chips, pouch slots and the chosen item's buttons.
- The chosen pouch item is one card with a named button per thing to do. The
  Explore chart is drawn in the interface's own colours and no longer repeats
  the destinations as buttons.
- The tag over your own character reads like a resident's: a status shape, the
  name with "(you)", and what they are doing.
- A planned purchase says where it will be made on the inspector's Why tab
  ("They will buy bread at Harbour market stall 1.").
- With a two-pane workspace open in a 1024 wide window, toasts stand under the
  folded goal tracker instead of over it.
- Fixed: a Shape stroke in which the simulation refused a dab could be undone
  but not redone. A refused dab is now taken back out of its stroke, and a
  stroke of refused dabs alone is no history.
- `scripts/playtest/capture_ui_phase67.py` is new.
- Seen natively and fixed: the first-run tip card spawned with two hover
  components and stopped the game at start; the Oversee totals and alerts tips
  stood over the Paused pill and the journal line and now stand beside the
  alert column under the toast lane; tooltips now stand above toasts; the
  title's gradient keeps its darkness to the column's edge; "Island loaded"
  survives the loading hitch; the played character's tag says "ready" (not
  "waiting for you") and "asleep until morning".
- Plain ledger headers and the inspector's sections have tooltips; menu
  buttons carry one too, but it does not show natively yet; under a host menu only the menu's own controls show one. A failed
  save's "Try again" is on the Tab ring. Empty states of the building card and
  a good's card say what to do next. The inspector's Home and job tab says the
  work plan in words instead of the planner's trace.
- Reduce motion now has a consumer: a critical need's row pulses once every two
  seconds, a steady edge with the setting on.

### Island interface: Live depth

- The goal tracker counts the goal chain ("Step 2 of 7") with pips, says what
  the current goal unlocks, and says a finished goal once as a toast.
- The working pill has a bar where the simulation knows the work's length (a
  rest, a night, a homestead upgrade) and "3 of 10" for a repeat order; Stop
  ends the order too. Gather & craft has a ×1 / ×5 / ×10 stepper.
- A need's tooltip gives the simulation's reason it is low; a click on a need
  opens the pouch at what helps it. The pouch shows the pack's weight and, where
  there is one, the tool in hand and its uses left.
- At your own door from evening on the prompt offers "Sleep until morning"; while
  asleep a quiet veil with Wake up covers the world and the host runs the clock
  at 30x, then gives your speed back; on waking a card gives the day's account
  (coins, goods gathered, made and received, people met, the lowest point of
  each need, the next goal). Enter or Esc puts it away.
- People shows each neighbour's friendship tier with pips and what it gives; a
  gift picker offers the pouch's giftable goods and names a resident's taste
  once you can know it. Gifts, tier changes, greetings, invitations and help are
  toasts and Journal notes.
- Build lists the homestead's tiers with their cost (have/need), work time, what
  each gives and why one is not available yet; an upgrade under way is a card
  with deliveries, work left and Cancel.
- The calendar lists the village gatherings (today first) and who is in
  mourning. The Economy page has a Days column for food and water and marks
  under two days Low; a good's card says who makes it. The build tray takes
  availability and its reason from the simulation's catalogue. Ages read as the
  simulation words them ("Adult · 34"); the Village page has this year's births,
  arrivals and deaths; the inspector lists a resident's Friends.
- `scripts/playtest/capture_ui_live2.py` is new.

### Island interface: Oversee targets and priorities, Shape plots and props

- Oversee, Economy, Jobs for one village: each trade from the catalog with
  Now, Wanted and Open, then −, + and "Village decides" under it
  (`SetProfessionTarget`). When now and wanted differ, the simulation's
  reason is shown under the row. A service trade has no controls and says why.
- Oversee, Economy, Priorities for one village: Low, Normal and High for
  each topic (`SetVillagePriority`), with the simulation's sentence on what
  the level changes.
- Shape, Place (L): the catalog's plots (buildings with a field) and props,
  with a ghost, a cursor tag with the refusal reason, R to turn, and
  Remove placed. Placements share Undo and Redo with strokes; the inverse is
  `RemovePlot`/`RemoveProp` or the placement again.
- Trades from data show by display name in the roster, the resident and
  workplace cards and the marker legend (with the world's colour). An
  unknown trade gets a generic picture.

### Island interface: two weights, a quiet load

- Barlow SemiBold joins Regular (OFL, credited in `assets/CREDITS.md`):
  headings, panel titles, key figures and the selected tab are SemiBold,
  through the type scale.
- The alert sound stays quiet for what a world's first scan finds after a
  load or a new game.
- The Jobs table names trades from the world's data.

### Island interface: phase A (old saves, the colony's day, Shape strength)

- An island saved by an older version is named on the title ("This island
  was saved by an older version and cannot be opened. It is kept as a
  backup; start a new island."), New game is offered, and the file is kept;
  every save now records its island version.
- The Economy page opens with "Today on the island": the civic fund, where
  the merchant boat is, what its call traded and what it wants next.
- Shape's brush bar has a Strength switch (Gentle, Normal, Strong); the brush
  outline is drawn thicker and in front of the ground.
- The Craft badge says "At the …" when the character stands at a known
  workshop; Shift+Tab from the world in Live enters the top bar.
- Sound: Settings has Music and Interface sounds sliders beside Master,
  Ambience and Effects, saved and applied as `AudioLevels`. The shared
  widgets ask for interface sounds (click, page, open and close, confirm and
  deny, place, alert, coin, a rate-limited hover).

### Island interface: the boat's next order, builder between stages (pass 8)

- The merchant boat's next order shows as rows: in the calendar's outlook
  (Oversee and Live) and in the Live Trade tab: "Next call wants 10 planks
  at 14 coins each.", and in port "In port, it wants …: 3 sold so far."
  Nothing shows when it wants nothing.
- The journal and alerts say the boat's order and its end in plain words
  (`VillageBoatOrder`, `VillageBoatOrderClosed`), and a resident's trade
  says its goods with the plural helper.
- The homestead card says a builder between stages "is between stages of
  work on your homestead", and a friend on the way "is on the way to lend a
  hand" (apart from the builder's "on the way to work").
- Fixture `upgrade-underway-at-door`: the same upgrade with the owner at the
  door, where the builder joins the owner's work.

### Island interface: builder at work, alerts, inn sleepers (pass 7)

- The resident inspector keeps the simulation's "asleep at the inn" like
  "asleep at home", instead of falling back to "sleeping".
- A view test feeds a homestead site with its hired builder working and
  checks the card says "… is building at your door." and not "on the way".
- `capture_ui_live7.py` steps the clock 20 ticks at a time after the
  materials are in, so the builder's short stage at the door (about 180
  ticks) is caught on screen; the `village-alerts` fixture has residents
  starving and thirsty so the alerts list has rows to hover.

### Island interface: polish (pass 6)

- Reduce motion also stills the island's ambient motion (`AmbientMotion`
  scale 0); it is kept with the other settings.
- `ISLAND_TIPS_SEEN=all` starts a run with every first-run tip seen; the
  capture scripts set it.
- The upgrade card's status is short rows, one sentence each: where the
  materials stand, what is ordered and not yet taken and since when, who is
  bringing what, who is hired, what to do meanwhile.
- An alert's tooltip stands beside the list instead of over the next alert.
- A world file that cannot be used says so in plain words; the parser's
  detail goes to the log.
- With Shape's Resources panel open the toast lane is centred beside it, so
  an export's toast no longer covers the goods.
- The `cottage-evening` fixture also sells to a resident, so the morning
  account shows a Sold row.

### Island interface: New game on an exported world

- New game lists the window's world first, then every world exported from
  Shape beside the world rules, newest first, with its date, settlements and
  patches by good; an unreadable or invalid file is listed, disabled, with
  the reason. The chosen file goes through the same staging, validation,
  backup and rollback as the shipped world, and the save keeps the whole
  construction.
- After an export the toast offers "Start a new game on it" while that is
  safe; it saves the island (kept as a backup), then opens New game with the
  export chosen.
- Counts of goods agree with their number ("Order 2 logs", "1 plank"); a
  workplace card is titled by its building ("Harbour inn"); the civic fund
  chip reads the fund itself; the morning account adds Bought and Sold.
- The upgrade card says when nobody has taken an order yet and how long ago
  it was placed (`UpgradeSiteView::ordered_at`), names haulers and the hired
  builder in its own words, and drops "carry it to your door" once
  everything missing is ordered. The goal card says what is happening at
  home while residents fetch and build, or adds it after a water, food or
  garden step.
- The morning account no longer lists a purchase twice (Bought, not
  Received). An empty civic fund says its sites wait for coins instead of
  "held". A tooltip whose text wraps takes its full width at once, so its
  last line is no longer clipped. Shape's first-run tip stands under the
  toast lane, clear of an export's toast.
- Shape's patch labels are chips: a picture at island zoom, the name on hover
  or close up, never one under another or under the first-run tip.
- Fixtures: `upgrade-underway` orders the rest and hires a builder by the
  physical flow; `village-life` keeps the neighbour awake.

### Island interface: Shape resources and world export

- Shape has a Resources group beside the terrain brushes: Resource (N) places
  a patch of a good from the world's patch list at a richness from 25% to 400%
  ("Poor / Normal / Rich / Abundant"), with a ghost that is green where it can
  go and red with the simulation's reason where not; Remove (E) takes the
  patch under the pointer away, or says why not. Existing patches carry a
  marker and a label while either tool is in hand; Esc puts the tool down.
  Both are recorded commands (`PlaceResourcePatch`, `RemoveResourcePatch`) and
  are not part of Undo (the tooltips and the hint say so).
- "Export world" in the Shape rail asks the host to write the shaped island
  as `<world>-shaped-N.json` in the worlds folder: the running world's rules
  with the `living_world` block of `frontier::shape::export_world`, checked as
  a world preset before it is written. A toast names the file; a failed
  write offers Try again.
- The morning account leaves out empty rows, wraps long ones inside the card
  and adds the day's friendship changes and homestead steps from the journal.
- The upgrade card drops Deliver (carried goods go in by themselves at the
  door) for one line that says so; Order, Hire and Cancel stay, and who is on
  the way or at work is said in the simulation's words.
- Menu tooltips stand beside their button, or above it, never over the next.
- Asleep, the goal card folds to its title.
- The Village page lists the inns with their guest beds; a row opens the
  inn's card.
- Prepared saves `village-life` (a death this morning, a guest at the inn, a
  neighbour beside the played character) and `larger-cottage-done` (every
  homestead tier built, a stocked chest, the character outside by day), and
  `scripts/playtest/capture_ui_live4.py`.

### Island interface: Live depth, seen natively

- Fix menu tooltips: the card of a title, pause or Settings button was built
  and hidden in the same frame, because the player HUD hides every island root
  while a host menu is open and the card is a root. Tooltip cards are exempt
  (`TooltipRoot`); a unit test runs both systems in frame order.
- Gather & craft and Trade: an empty pouch is one line above the list, and the
  list takes the whole width; with goods the two panes stay.
- The title's Continue button has its tooltip (it is built apart from the other
  buttons and had none); a test checks every title, pause and Settings control.
- A tooltip stands above the panel of its anchor: the day's account card hid
  its own button's tooltip. The critical-need pulse is drawn after the kit's
  paint, which had put the need row's outline back every frame.
- The upgrade card offers Order (per missing good, price with the carrying fee)
  and Hire a builder (its cost) beside Deliver and Cancel; orders and hiring are
  toasts and Homestead notes in the Journal. Recipes and garden work take the
  ×1 / ×5 / ×10 repeat stepper ("Work the whole plot"); a stopped order says why.
- People lists every neighbour the game keeps a bond with, not only faces seen
  in the last minute. The title and pause say the day of the season, as the bar.
- A failed save's toast reads as sentences without the key; first-run tips
  stand under the Paused pill and one toast.
- Notices said together are each shown in turn (the lane had one slot, so an
  upgrade's "is finished" was replaced by the next outcome in the same frame).
  With an upgrade under way, Build shows its card first.
- Sleep no longer covers the sleeper: the view's edges dim (centre clear, no
  animation) and a strip where the prompt stands says "Sleeping… · morning at
  06:00" with Wake up (E); the bar's clock stays visible through the 30x night.
- In Live a click on your own cottage opens it: Build with its tiers, upgrade
  and chest, and the cottage becomes the selection, which the homes view cuts
  away.
- Empty states: the Journal's Important filter says to choose Everything when
  only routine notes exist; the calendar says what it lists on an island with
  no gathering or mourning; Gather & craft says why no recipe is listed.
- Add prepared island saves for captures,
  `content/testbed_content/tests/island_capture_fixtures.rs` (`#[ignore]`d):
  `cottage-evening`, `upgrade-underway` and `hard-times` in
  `target/npc-out/fixtures/`, loadable with `--island-save`.
- Add `scripts/playtest/capture_ui_live3.py` (menu tooltips, sleep from prompt
  to the day's account, the upgrade and chest, the critical need and Reduce
  motion, the unaffordable build card, a failed save's Try again);
  `capture_ui_redesign.launch` takes a `save` to copy in; `capture_player_flow`
  resolves the old "Word · K" labels to the redesigned controls and anchors
  exploration at the actor's first own observation; the natural-play cottage
  part of `capture_ui_live2.py` is replaced by the saves.


### Regional living world

- Add an opt-in 256×256 metre coastal world with three settlements, 42 residents,
  explicit homes, persistent identities and bounded occupation changes.
- Add finite ecology and delayed restoration, fishing and renewable woven clothing;
  distribute regional production and use reachable physical sourcing.
- Expand character, workplace and nature assets with recorded CC0 provenance,
  regional terrain chunks, atmosphere and a published settlement chart.
- Hold regional entry behind an opaque preparation screen until terrain GPU
  buffers and render pipelines are ready. Retain water-only chunk identities
  without uploading empty meshes, including terrain edits back to land.
- Preserve old saves; normal play uses a separate living-world save and frozen
  construction. ADR 0037 describes ownership; the coastal-world guide covers play.
- Replace the regional daily wealth cap with a 10% levy on coins above 200, a
  40-coin destitution floor and an equal dividend of the fund above its reserve;
  remove the scripted innkeeper sink there. Other profiles keep the cap.
- Rank regional labour by realised margin: three flexible residents per settlement,
  and a persistent margin gap moves surplus workers with a shorter cooldown.
- Read the coastal world's goods, recipes, professions, resource patches, founding
  endowments and economy tuning from a validated, hashed `economy` data block in the
  world file (`docs/island-economy-authoring.md`); a file without the block loads the
  built-in economy. This changes the `living_world` content hash: older local
  coastal-world saves do not continue.
- Make the world seed matter in the coastal world: founders get seeded food tastes,
  one extra seeded aptitude and a seeded band of starting stores.
- Keep coastal homes warm through winter: stewards stock hearth fuel from two days
  before winter, cold residents fetch fuel from a nearer patch, rest by the hearth
  until warm and shelter when freezing; the last essential worker stays while demand
  exists and a settled specialist may cover a vacant essential trade.
- Damp coastal labour thrash: no return to a profession left less than two days ago
  without a doubled pay gap, no refill of a seat vacated within 2,400 ticks, and no
  move away from goods in demand except towards a larger shortage. An empty
  essential workplace overrides these rules.
- Add the regional `TerraformCells` batch command (one atomic, recorded command per
  painted stroke; `Terraform` is unchanged) and journal effects
  `VillageStarvationOnset`, `VillageThirstOnset` and `VillagePatchDepleted`.
- Bound coastal market prices at five times the base price and let an unfunded,
  unsold quote above base drift back down.
- Add axe, pickaxe and hoe recipes with 120-use durability; the matching tool speeds
  log, ore and grain gathering and bare hands slow it, for residents and players.
- Drain health in deep cold, burn hearth logs faster in winter and let morale scale
  work time between 90% and 115% (regional profile only).
- Cache each resident's GOAP plan until its first step completes or fails, and
  abandon a destination after three failed detours, avoiding it for 1,200 ticks.
- Add a three-seed island soak, price-economy tests and explicit player buy-out and
  coin-hoarding shock tests. The regional content hash changes: existing
  `living-world` saves do not load.
- Make the developer toggles (F1 overlay, F3 journal, F4 perf, F5 sheet, F6 perception,
  F7 notebooks, F8 fog) inert in the player session unless the host gets `--dev-keys`.
- Title the player session window "Island Village"; developer launches keep theirs.
- Correct the island guides: 42 residents in three settlements, the `living-world`
  save, profession assignment and the 0 to 12 metre living-world height range.
- Author coastal residents' behaviour in data: the tree is
  `content/testbed_content/trees/resident.json` (or `living_world.behavior.trees`),
  validated before the freeze with messages naming the file and node, hashed and
  kept in the world's construction data. The one decision leaf is split into 19
  `life.*` leaves (go to, buy, sell, gather, craft, eat, drink, sleep, warm up,
  socialise, haul, build, hunt, GOAP as `life.pursue_goal`, ...) and 35 named
  conditions; a guard that aborts lower-priority work is refused unless its branch
  can start. Other island profiles keep their built-in tree. This changes the
  `living_world` content hash again: older local coastal-world saves do not continue.
- Give every coastal resident four seeded traits (diligence, sociability,
  chronotype, thrift) and read work, meal, leisure and sleep windows from data;
  residents take evening leisure, eat at meal times and shift their day accordingly.
- Journal a `VillageDecision` record when a resident's activity changes: the chosen
  branch, every higher-priority option passed over with its failing condition or
  reason, and for GOAP the goal and plan length. Records never change the sim;
  `village::resident::explain` and `label` expose the same facts to views.
- Fix slow bread recovery after a buy-out. The cause was a made-to-order chain with
  no buffers: ordered bread waited for a farmer to cut grain behind a long fibre
  order book, a miller to fetch and mill it and a baker to fetch the flour. Bakers
  now keep four flour and millers four grain (economy data), producers rank
  ingredients of food with food, and units delivered in a market round answer as
  many unfilled orders in the coastal price rule. The buy-out test now asserts bread
  is back within twice its base price two days after the buy-out.
- Stop cloning the whole world preset per resident per tick in the island vitals pass.
- Rewrite `docs/island-ai-authoring.md` as the behaviour authoring guide.
- Let the coastal village grow: building types are data (`economy.buildings`: cost,
  work, footprint, kind, placement rules), the recorded `PlaceBuilding` and
  `CancelBuilding` commands open and abandon construction sites, residents carry
  the materials there and build in stages, and the finished building keeps the
  site's identity. `village::civic::placement_check` is the pure placement query.
- Add stores and market stalls as sellers of record: goods lie there on their
  owner's behalf and are sold at the quote while the owner is elsewhere, crediting
  them at once. `life.haul` gains `materials`, `surplus` and `restock`; `life.build`
  also works building sites; four conditions (`life.has_surplus`,
  `life.site_needs_materials`, `life.site_needs_builder`, `life.stall_needs_stock`)
  join the list. Buying and selling at a store, and delivering to a site, use the
  shared `buy`/`sell` action.
- Add a civic planner per settlement: it houses the homeless and commissions one
  home, store or workplace at a time on a lasting shortage, journaled as
  `VillageCivicProject`. The civic fund pays for delivered materials, and the daily
  dividend returns only what open sites do not need. New `economy.tuning` fields
  hold every threshold. `labor::settle_newcomer` settles a homeless arrival.
  This changes the `living_world` content hash again: older local coastal-world
  saves do not continue. Other profiles and the recorded replays are unchanged.
- Add demography to the coastal world: every resident has a seeded age and life
  stage (`economy.demography`, ages in seasons). Housed adults with lasting mutual
  affinity become couples; a couple with food and a bed (a free one, or a home the
  planner can pay for) has children, who are household members that eat and do not
  work; a child comes of age into a free workplace seat with one aptitude of a
  parent; elders slow down and die of old age on a seeded day. A dead resident's
  coins and plain goods pass to the household, else to the civic fund. Journal
  effects: `VillageBirth`, `VillageComingOfAge`, `VillagePairing`, `VillageElder`,
  `VillageInheritance`, `VillageChildDeath`, `VillageAdoption`, `VillageFamilyMoved`,
  `VillageArrival`; `VillageDeath` gains `cause` and `name`.
- Add crop fields: a building with the `field` attribute (an open storage building
  with plots) is founded beside each farm and may be commissioned by the planner on
  a lasting grain shortage. Farmers work it through `life.sow`, `life.tend` and
  `life.harvest` (conditions `life.field_needs_sowing`, `life.field_needs_tending`,
  `life.field_needs_harvest`); crops (`economy.crops`) have seasonal windows and lie
  fallow in winter; farmers sow only while the settlement is short of grain per
  resident. Wild grain becomes a finite founding reserve.
- Add the merchant boat (`economy.merchant`): a periodic visitor at the Harbour
  trading post with a finite purse and finite stock, the only external source and
  sink. Island coins plus `village::merchant::purse` are constant. Settlers land
  with it when housing, seats and food allow.
- Add deterministic daily weather and rare seasonal shocks (`economy.weather`):
  rain speeds crops and refills water, a storm halts outdoor work and the boat
  (`life.storm` and the tree's `Storm` branch), snow raises warmth loss; drought,
  blight and harsh winter are seeded events.
- Make the civic reserve follow the recent shortfall of the levy against
  destitution grants, let the fund feed a hungry child from a store, and hold a planner's
  unaffordable project back from the daily dividend. The planner also moves crowded
  families, builds family houses, fields and workplaces for grown children.
- Fund building: when open sites need more than the civic fund may spend, a
  building levy (`tuning.building_levy_percent`, at most a twentieth of each
  resident's coins above the destitution floor, only what is needed) is raised at
  once and earmarked for materials, so a placed building is supplied without a
  subsidy; grants are still paid first. `civic::funding` and `SiteView.funding`
  say in words whether a site is funded. `SiteView` gains `placed_at`,
  `last_delivery_at` and `paid`, `BuildingView` gains `crop_plots`, `PlannerState`
  gains `last_decision_text`, and `VillageSiteCancelled` carries the label and
  position. Journal effect `VillageBuildingLevy`.
- Rest market prices on a stormy day and let residents lay in a third day of
  provisions the day before; a field's granary stores only its crop.
- Let a gatherer whose good sells above its base price keep a few units beyond
  its orders (`tuning.stall_restock_level`): makers worked to order, so a price
  that rose in a storm had nothing to bring it down again.
- Add a relief levy (`tuning.relief_levy_percent`): when the fund cannot pay the
  destitution grants of the day, the shortfall is raised from coins above the
  floor. Journal effect `VillageReliefLevy`; `VillageCivicLevy` gains
  `relief_levy`.
- Add `tuning.days_per_season` and `tuning.founding_day`; `village::calendar` uses
  them (`calendar_at` keeps the seven-day year of the other profiles).
- Put residents inside their homes: a resident standing at the door of the home it
  sleeps in is indoors (`village::indoors::Indoors`) until it walks off. Indoors
  the hearth decides warmth, a coat does not wear and only somebody inside the
  same home can be met. Everybody with a bed sleeps in it; a rented bed beside the
  inn is inside the inn. Journal effect `VillageWentIndoors` (after 600 ticks
  outdoors).
- Add `village::view`: read accessors for the activity label, the latest decision
  as data, effective schedule blocks, season effects, household and family, weather
  and forecast, boat status, field plots, who is inside which building, each
  home's entrance, footprint, facing and beds, and awake/resting/asleep. A
  resident at home no longer reads "going home".
- Replace whole-index scans on resident hot paths with component queries, bucket
  social meetings by position and memoise searched route estimates by grid
  revision; results are unchanged in every profile.
- Sleep through the night: in its sleep block a resident with a bed goes to bed
  and sleeps to the end of the block (`tuning.sleep_energy_per_tick`); a critical
  need, a starving child of the home or a player order wakes it (`VillageWoken`).
  A rest at home by day is a short nap (`tuning.nap_energy_per_tick`). The
  `Bedtime` branch now runs `life.sleep`.
- Buy at a fixed place: a seller who is walking is a target only within six
  metres, so a slow buyer no longer trails a seller across the settlement.
  `view::supplier` names where a resident would buy.
- Add `view::inn`/`view::inns` (an inn as a place with four guest beds; a full
  inn turns guests away) and `door`, `body_min`, `body_max` on `HomeView`; a
  homestead cottage reports the two by two metres the simulation blocks.
- Remove the body of a dead resident a day after death and keep the life in
  `Demography::departed` (`view::departed`, journal `VillageLaidToRest`).
- Send an adult for food when a child of the home is hungry and nobody at home
  carries any.
- Move the gatherer's shelf to its own switch, `tuning.gatherer_shelf_units`.
- Add live mode for the coastal world (`economy.live`, command `LiveAct`): sleep
  to the morning with a day summary, repeat-gather orders, bonds with residents
  (talk, trade, gifts with seeded tastes, neglect; a discount, a gift back and
  invitations by tier), residents who speak first, a goal chain that opens
  homestead upgrades paid in goods and work, calendar gatherings the residents
  walk to, and mourning when somebody is laid to rest. Views: `day_summary`,
  `day_so_far`, `asleep_until`, `repeat_order`, `relationships`, `gift_tastes`,
  `goals`, `homestead_tiers`, `gatherings`, `mourning`.
- Add a price floor in percent of the base price (`tuning.price_floor_percent`).
- Let an inn guest sleep the night and read the guest beds from data
  (`tuning.inn_guest_beds`).
- Add `view::daily_use`, `view::producers` and `view::catalogue`.
- This changes the `living_world` content hash again: older local coastal-world
  saves do not continue. Other profiles and the recorded replays are unchanged.
- Replace the flat price floor with reservation prices and stock targets
  (`tuning.reservation_prices`, `maker_margin_percent`, `raw_floor_percent`,
  `stock_target_days`, `stock_cover_pricing`, `trade_window_rounds`): makers do not
  sell below input cost plus a margin, start no batch while the settlement
  offers more than two days of its use, and stock within that target no longer
  lowers a price (for goods made in batches or grown; gathered goods have no
  cover). A quote below every seller's reservation climbs towards it.
  `margin_persist_ticks` and `margin_cooldown_ticks` default to 6,000 (were
  2,400): with real margins the shorter hysteresis moved workers about 200 times
  in the 14-day audit, now 118. `price_floor_percent` defaults to 0. New hashed resource
  `regional_trade_volume_v1` (`village::stock::TradeVolume`); `view::daily_use`
  lists every traded good.
- Let a resident go to a gathering only while every need is above
  `live.gathering_needs_above` (50,000): the 14-day audit showed the midwinter
  fire behind the Hunger, Thirst and Warmth regression of the live-mode pass.
- Add a boat duty (`tuning.boat_duty_percent`, 5): each merchant-boat trade pays
  a share of its value from the purse into the civic fund (`VillageBoatDuty`).
- Finish live mode: repeat orders of crafting (`recipe`) and garden work (`task`)
  with stop reasons; homestead upgrades as private sites with deliveries, orders
  filled by residents for a fee in escrow, a hired builder and `cancel_upgrade`;
  a storage chest (`store`, `take`, `view::chest`, `view::upgrade_site`); the
  garden plot allowance enforced; friends at a helping tier who help
  (`VillageFriendHelped`); goods in the day summary booked by the action; a goal
  chain over the new tiers. The living-world homestead cottage is the catalogue
  cottage (4 x 5 m, two beds) and the larger cottage grows into the family house
  (5 x 6 m, four beds). Add `view::workplace` and `view::workplaces`.
- Add Shape commands `PlaceResourcePatch` and `RemoveResourcePatch`; a settlement
  keeps at least one patch of every good (up to four of one). Add
  `frontier::export_world` and the optional `terrain`, `resource_patches` and
  `buildings` blocks of `living_world`, built again from an exported file.
- The `living_world` content hash changes again (new tuning and live fields,
  new commands and resource, `CivicState.founded`, `Merchant.duty_paid`).

### Three ways to play (Live / Oversee / Shape)

- Add the Oversee build menu (**Build · B**): a palette of the world's buildings
  by kind with cost and purpose, a placement ghost that snaps to cells, turns with
  R and is green or red by the simulation's own placement check with its reason
  beside the cursor; a click records `PlaceBuilding`, right-click or Esc puts the
  building down.
- Draw what the village builds: construction sites that fill in with deliveries
  and work, finished homes and workplaces in the settlement's roof colour with
  window light and chimney smoke, stockpiles and barns with visible stock, market
  stalls with goods on display. Roots are spawned and despawned on change only.
- Add **Building projects** to the ledger's Village tab: the civic fund, open
  sites with material progress and a confirmed Cancel (`CancelBuilding`), each
  planner's last review and its recent decisions with reasons.
- Add project started / finished / cancelled notes and a "site waiting for
  materials for N days" warning to the alert feed.
- Add building cards for sites, homes, workplaces, stores and stalls, and name the
  site, store or stall in resident activities ("building Harbour cottage 3",
  "carrying planks to …", "selling bread at …") in place of "building a cottage".

- Add a persistent island session bar: Menu, a Live / Oversee / Shape mode switcher
  (keys 1 / 2 / 3, active mode lit) and one shared clock with day, season, time of
  day and Pause / 1× / 3× / 10×. Space, `-` and `=` now act the same in every mode,
  including character play; the character HUD no longer asks for the menu to resume.
- Add village alerts from journal effects and existing components: a scrollable,
  folded, 40-row, click-to-focus feed while overseeing and fading notices in
  character play. Session-scoped view state; nothing is saved or hashed.
- Add an Economy ledger tab: per-settlement stock, reference price and a view-side
  trend for every tradeable good, plus workers and open workplaces per profession.
- Replace the ledger's landscape list with a Shape palette: Raise, Lower, Flatten,
  Smooth and Paint with radius (`[` / `]`), hard or soft edge, drag painting, and
  session undo / redo (Ctrl+Z / Ctrl+Y). All of it records the existing absolute
  `Terraform` command; refusals show beside the cursor. Developer launches keep a
  Landscape tab and their ledger mode and clock rows.
- Add a Settings screen (title and pause): volumes, mute, interface size and
  windowed or borderless display, kept in `island-settings.json` beside the save.
- Rename the pause menu's mode entries to "Oversee the village" and "Shape the
  landscape"; playtest scripts use the new bar labels.
- Give village alerts a severity: the Oversee feed shows critical and warning
  rows and folds each day's routine notes (job changes, trades, arrivals, levy
  day) into one line that unfolds on click; a filter offers Important, Critical
  or Everything, the panel collapses to its header, and Live toasts fire for
  emergencies only. Homelessness and food or water shortage are new warnings.
- Put the session bar and the Shape palette in the Tab order with a visible
  focus ring; a click still leaves keyboard focus alone, so Space keeps pausing.
- Add Edge pan and Pan speed to Settings (`edge_pan`, `pan_speed` in
  `island-settings.json`); name middle-mouse orbit and edge panning in the
  on-screen hints and How to play.
- Outline the resident markers and add a collapsible key of the trades, shown
  while overseeing at the zooms where markers are drawn.
- Stop the developer Home & farm panel from building an actor observation every
  frame: nothing while closed, and only on changed inputs while open.
- Keep the Economy tab's settlement selector on one row, size the Shape palette
  to its content, and leave a camera the player has moved where it is when a
  panel opens or the window resizes.
- Record a Shape dab in the coastal world as one command: the `Terraform` disc
  when the brush ends at one value, otherwise one atomic `TerraformCells` batch;
  undo and redo are one batch each and a refused one stays in the history. A
  stroke is bounded to 4,096 cells. A soft radius-4 Raise drag went from a median
  36.5 s between planning a dab and its mesh (588 commands, 28 of 40 dabs
  dropped) to 0.19 s (29 commands, none dropped); `ISLAND_VIEW_SHAPE_BATCH=off`
  keeps the old recording for comparison. The older island world still records
  uneven dabs cell by cell.
- Report starvation, dangerous thirst and emptied resource patches in the coastal
  world from the simulation's `VillageStarvationOnset`, `VillageThirstOnset` and
  `VillagePatchDepleted` journal effects. The component scan remains for
  homelessness and shortages, reads needs once when a session opens, and is the
  whole source where no onsets are journaled; nothing is reported by both.
- Announce a save only when the player asked for it (Ctrl+S, F9, the pause menu)
  or when it failed. The minute autosave no longer shows "Island saved".
- Add a resident inspector to Oversee: selecting a resident in the world or in
  the roster opens a card with their work, home, activity, need bars, coins and
  carried goods, traits in plain words, the day's schedule with the present
  moment marked, and a "Why" section built from the latest `VillageDecision`
  record (what they chose, what would have come first and why it did not, the
  plan and its steps left). Follow keeps the camera on them; Esc or a click on
  empty ground closes it. View state only; nothing is recorded or saved.
- Make the Residents tab a sortable roster (name, settlement, profession, mood,
  wealth) with a settlement filter, the activity label and a need indicator per
  row; the selected resident's job and home commands are one step away.
- Open a read-only calendar from the clock: season and day within it, days to
  the next season, sunrise and sunset, and what the season changes for growth
  and cold, read from the running world.
- Use one activity wording (the running leaf's label, made concrete by the
  physical action) in the inspector, the roster and the caption over a
  resident; a resident meeting a neighbour in free time greets them.
- Correct the guides: the minute autosave is silent, and ADR 0038 describes
  terrain strokes as one batch command per dab.

### Island world

- Fit every solid workplace to the 5 m body the simulation blocks for it, door
  or working side to the entrance, by one scale and without stretching it: a
  mill's door is 1.89 m (it was 0.76 m), a smithy's 1.22 m, and no workshop
  but the mill stands taller than the inn. Open workplaces keep their scale.
  Piles stand clear of the walls; scenery gives way to anything built later.
- Take a building out of the way while it stands between the camera and the
  played character, or the selected resident at a close camera, without
  opening it.
- Draw a house's door open while somebody steps through it.
- Draw crop fields plot by plot from `view::fields`: fallow, sown, growing in
  ten steps, ripe; frosted in winter.
- Draw the merchant boat from `view::boat`: sailing in, moored off the shore
  nearest the trading post, sailing out.
- Draw today's weather from `view::weather`: rain, storm with lightning and
  snow as falling sheets round the camera, with a dimmer, hazier sky; shocks
  tint the air. Clear weather costs nothing.
- Key the sleep pose on `view::repose`: anybody asleep lies down on both rigs,
  any other rest sits; in a cut-away home only sleepers are in bed.
- Draw the inn from `view::inn`: its body, its four guest beds and each guest
  in their own.
- Fit the Live character's homestead cottage to the 2 by 2 m body the
  simulation gives it (door 1.58 m) and cut it away like any home: its three
  beds are one bunk of three berths, the only way they fit.
- Stop tracking a resident whose entity the simulation has removed.
- Add `island_world` (`testbed_view`) and `scripts/playtest/capture_world.py`.
- Refit workplaces: fill the body across the ground, lift up to 1.25 times
  towards a door 0.85 of a resident, cap at 7 m (the mill 9 m). The bakery and
  smithy reach 0.85; the timber yard, mine and market stay short.
- Give the Live character the adventurer rig with a warm player ring instead
  of the robot capsule; it steps through doors, sleeps and is unblocked like a
  resident.
- Ease a blocking building down to its footing over 0.2 s and back up, take
  its smoke and lights with it, and treat scenery trees the same way.
- Dress the regional island's residents, children and played character from
  one family of bodies (the KayKit adventurers), varied by body, hat, cape and
  cloth colour picked from the `SimId`; adults stand 1.8 m and children 0.62
  of that. Legacy scenarios keep their bodies.
- Keep rain, storm and snow streaks their size in metres at every zoom, and
  short within 15 m of the camera, so none is longer than 6 % of the picture;
  from a camera above 60 m fade them out for two crossed layers of drifting
  haze.
- Draw the played character as the Knight on the regional island, whatever
  its preset's model; `ISLAND_VIEW_PLAYER_BODY` still overrides for captures.
- Cut a home in the camera's way away instead of squashing it to its
  footing, and ease every cutaway's roof and walls down and up over 0.2 s.
- Keep everybody standing indoors 0.45 m clear of the furniture and the
  walls, and bring the chimney breast down with the back wall.
- Moor the boat on the simulation's berth and lay the jetty from it; draw
  homestead builders hammering, haulers carrying logs or planks, tools from
  `view::held` (none when bare-handed), fishers facing the water, lowered
  cutaway walls as one kerb, and the cottage's chimney 0.9 m wide.
- Test a tree against its trunk and tapering crown with a 0.5 m margin, and a
  home up to its ridge, before taking it out of the camera's way.
- Lay snow on the homes' roofs while it lies on the ground, and put the
  homestead chest in the cottage's cutaway once the character has one.
- Draw a scaffold round the homestead cottage while its upgrade is open, the
  delivered logs and planks beside it, and refit the cottage when the
  simulation grows its body (4 by 5 m to 5 by 6 m).
- Draw work tools 1.3 times larger so they read at settlement zoom.
- Add capture overrides `ISLAND_VIEW_PLAYER_BODY`, `ISLAND_VIEW_CAST`,
  `ISLAND_VIEW_FLASH` and `ISLAND_VIEW_SHOCK` (view only) and the third
  `capture_world.py` pass (`target/npc-out/world3/`).
- Build a jetty and quay from the nearest shore to the boat's berth, with
  crates, barrels and a lantern; stack unloaded goods on the quay while the
  boat is in; draw the boat only where the played actor sees.
- Make wet weather overcast (sun 0.06 to 0.25, more sky light), draw many thin
  short streaks with none near the lens, wet and snow the ground over seconds,
  bend trees in a storm, and show drought, blight and a harsh winter on the
  ground and the fields.
- Put an axe, a hoe, a rod or a hammer in the hand while a chop, farm, fish or
  hammer clip plays.
- Keep a door open 1.0 s after a step so it reads at 1x.

### Island homes

- Fit every home's house to the body the simulation gives it: the walls cover
  its cells, the door faces the entrance and is 2.2 m high, 1.22 times a
  resident (it was 1.26 m on a 2.2 by 2.5 m house, on the far side). Built
  cottages and family houses are fitted the same way; the inn becomes a
  two-storey house with a sign.
- Stop drawing residents the simulation has indoors: they step into the doorway
  and are hidden, and step out again when they leave. Nobody with a bed is
  drawn asleep outdoors.
- Cut a home or inn away when its card is open, an occupant is selected or the
  camera is close: no roof, the walls on the camera's side lowered, one bed per
  bed of the home, a table, a hearth that burns when the home is heated and
  lights the room at night, and the occupants in bed or by the hearth. Interior
  entities exist only while a building is cut away.
- Remove the bed that stood outdoors beside the regional inn.
- Add `island_homes` (`testbed_view`) and `scripts/playtest/capture_houses.py`;
  document proportions, rules and limits in `docs/island-atmosphere.md`.

### Island atmosphere

- Give the island play camera an HDR target, restrained bloom and an explicit
  TonyMcMapface tonemapper; UI and window captures are unaffected.
- Fade path lanterns along the dusk curve and add a warm door light to homes that
  are heated or occupied at night, capped at 24 lantern and 16 home lights.
- Replace hearth smoke with nine rising, fading puffs at each cottage's real
  chimney; add embers and a flickering glow over a forge while a craft runs there.
- Map island work to distinct shipped clips: sleep, sit, carry, chop, farm, fish and
  hammer. Legacy scenarios keep their previous animation and lay-down pose.
- Recolour roofs per settlement (harbour blue, Alderwood moss, Stone Ridge
  terracotta) and add one decorative, non-colliding watchtower beside each.
- Show a sack, log bundle or crate on residents hauling bulk goods and a stock pile
  beside each workplace that grows with its inventory.
- Add CC0 island audio: crossfaded ambience by biome and time of day, rate-limited
  spatial chop, hammer and mill sounds, a UI click and an `IslandAudioSettings`
  resource. `--mute` and `--screenshot` runs are silent; headless hosts play nothing.
  Sources and hashes are recorded in `assets/CREDITS.md`. See `docs/island-atmosphere.md`.
- Light the coastal world by a faster day curve: sunrise now has half of noon's light
  instead of a quarter, noon is 14,000 lux with a brighter sky light, dawn and dusk are
  gold and night is a readable moon blue. Distance haze reaches full strength at 14
  times the zoom distance instead of 3.5, so a settlement keeps its own colours.
  HDR and bloom were measured not to change brightness.
- Colour the coastal ground per grid corner instead of per cell: neighbouring cells
  blend, authored roads are drawn from their centre lines (3 m main roads, 1.6 m lanes,
  no stair steps), biome borders are feathered, turf varies gently, ridge stone gathers
  into outcrops, and the shore gets a sand edge and a shallow-water tint. Walkable
  cells and their costs are unchanged; chunks still rebuild only near an edit.
- Lower the coastal camera to 38 degrees at settlement zoom, steepening to 58 degrees
  at the overview. Dragging with the middle mouse button orbits and tilts, and the
  cursor at a window edge pans (`ISLAND_VIEW_EDGE_PAN=off` to disable).
- Show a trade-coloured dot above each resident at settlement zoom, at most 96,
  fading out up close and at the overview.
- Add view-only review aids: `ISLAND_VIEW_DAY_PHASE`, `ISLAND_VIEW_POST=off` and
  `ISLAND_VIEW_STATS=1` (scene size and frame time in the log).
- Give the ground a fine grain up close (a tiling texture generated in code,
  with a mip chain), stand low rocks on the ridge outcrops, slope a beach from
  the land's edge into the water instead of a stepped bank, let roads wander,
  vary in width, wear at the verge and round their junctions, mute the autumn
  palette to olive and ochre, and fray the woodland edge with thinned trees and
  bushes. All view-only: cells, costs, hashes and replays are unchanged.
- Add `scripts/playtest/capture_view_followups.py` and `measure_view_perf.py`;
  `RtsCamera` is reflected so a capture script can place the view over BRP.
- Make chimney smoke and forge fire readable from settlement zoom. Smoke was gated
  on a lit hearth alone, which no home has on a new island's first day, and was too
  faint to see when it did show. A settlement home with someone in now also shows a
  cooking fire (not in winter, and not for a homestead cottage, where smoke keeps
  meaning heat); puffs are larger, denser, pale by day and lean with one wind. A
  working forge shows a fire in its chimney mouth, larger embers and its own smoke.
- Show a carried load only while its resident walks; a load on a still figure hid
  the working arm and, with everyone holding hearth logs, was on every resident.
- Say a save once: one "Island saved · Ctrl+S" notice at the bottom left that fades
  after a few seconds, instead of a permanent label and a second line beside it. A
  failed save stays until the next attempt.
- Show the Live tip under the session bar for eight seconds the first time, then only
  while the Live button is hovered or focused.
- Resolve the asset folder at run time: `ISLAND_ASSETS`, then an `assets` folder
  beside the executable, then the developer fallback; the choice is logged once.
  Add `scripts/package_island.ps1` and `docs/island-packaging.md`.
- Add `ISLAND_VIEW_DRESSING=off`, `scripts/playtest/capture_view_checks.py` and
  `pose_sheet.py`; `measure_view_perf.py` takes `--camera`, `--size` and
  `--no-dressing`. Rocks, bushes and shore slopes measured no slower.
- Draw chimney smoke as camera-facing quads with a soft alpha texture made in
  code: nine overlapping puffs a chimney that start a third of a metre across,
  grow, lean and thin out, each with its own size, wander and turn. The spheres
  read as a chain of hard-edged discs. Smoke takes its colour from the sky model
  (a blue-grey a third as bright at night) and is not lit by lamps.
- Make forge embers seven 13 cm sparks with their own rhythm, arc and flicker
  over a low bed of coals, in place of twelve evenly spaced 24 cm balls.
- Raise the coastal night to the older island's level (moon 2,300 lux, sky
  450 cd/m2; it had been 1,500 and 340), give a lit home an amber lantern on both
  long walls and a lit pane on each side wall, and strengthen its door light.
  Before, a settlement seen from behind showed no lit pane at all.
- Build a resident marker's dark rim into the dot's own mesh. As a second entity
  behind a depth bias, some rims were drawn over their dots: black discs from
  one side of the island.
- Add `scripts/playtest/capture_view_wiring.py`, which also drives a Shape drag
  through the window's cursor and mouse-button input and reports the view's
  plan-to-mesh time (`ISLAND_VIEW_STATS=1`).


### One-window island life

- Connect the title, named New game, Continue, play, pause, save and return to title
  in one native host/window. Keep authoring and AI explorer tools behind the explicit
  developer menu, and preserve the existing 3D style and permanent-life survival rules.
- Add a shared player HUD and Pouch, Craft, Build, Garden, Trade, People and Explore workspaces,
  with contextual actions, real ingredient counts, work feedback and cottage previews.
- Project first-homestead guidance from the character's known world and possessions;
  add face-to-face advice on homes, gardens, water and trade without granting rewards.
- Define crops in homestead content, retaining grain and adding hardy berry beds to
  Island Colony with real seeds, watering, growth and harvest; preserve saved crop kinds.
- Stage and validate new worlds before replacement, back up previous saves, recover
  from write failures and preserve committed work across title/continue transitions.
- Keep host lifecycle, view input, actor-scoped projections and authoritative actions
  separate; document the player flow and its ownership in ADR 0036.
- Add inspector-first entry without visitor arrival, post-death spectating with later
  successor choices, an Explore workspace, and selected-site/conversation controls
  ahead of long selectors. Show speed and simulation-time estimates explicitly.
- Retain learned immovable island destinations through existing per-actor discovery
  data without disclosing unseen stock or moving residents. Filter conversation
  targets to known residents and nearby contextual actions.
- Preserve committed work during inspector/editor transitions by deferring native
  detachment until it settles. Report requests, progress and actual outcomes with
  actor/epoch-scoped feedback instead of exposing internal action names.
- Expose own-home heating and fuel rules, share leisure-meeting eligibility between
  planning and settlement, acknowledge active garden beds and distinguish planner
  search limits from missing prerequisites.
- Skip hidden legacy observations and unselected build-preview projections; attribute
  island UI work in the existing profiler. Record research, bounded recovery checks
  and the remaining authoring, demographic, ecology, terrain-undo and audio scope.

### Island colony showcase

- Make Island Village the launcher's showcase, with inspector, controllable character and
  landscape editor modes sharing an atomic, locked local save.
- Add opt-in JSON-authored resident routines and bounded GOAP planning over actual cash,
  inventory, finite resources and registered production recipes; expose decisions in the ledger.
- Add finite households, physical social relationships, civic food reserves, material-backed
  resident housing commissions and real household heating fuel.
- Add guarded, recorded terrain brushes with navigation updates, persistent height/surface
  state, native previews, seasonal ground, lamps and village atmosphere.
- Preserve legacy scenario registrations and protected replay/golden files.
- Separate colony contexts, read-only construction intent, resident action execution,
  native control coordination and UI projections; document ownership and extension seams.
- Make persistent launcher shutdown require an acknowledged successful save, with explicit
  refusal handling; validate restored content references before replacing any saved file.

### Native homestead play

- Add Home & farm (F2): explicit human control, plot claiming, material-backed cottage building,
  individual crop beds, gathering, workshop recipes, food/rest and actual market actions.
- Walk to the selected door, bed or remembered worksite before revalidating shared actions;
  preserve foreign control, fog knowledge and cancellation across actor/world changes.
- Keep saved growing crops while discarding the hero's stale queued homestead input on host resume.

### Expanded Living Village and homesteads

- Add immutable compact/expanded island profiles. New Living worlds have roughly 56% more
  land, connected trade routes, three districts and six clear house/garden plots; saved compact
  worlds retain their shoreline. Add the `living_homestead` preset without overwriting user files.
- Let all living actors claim plots, build cottages from real reserved planks/logs, plant and water
  grain, harvest it and use the ordinary food/market chains. Preserve ownership, home and crop
  deadlines through save/resume; release construction claims on stop, failure or death.
- Prefer ready NPC work over buffer shopping, enable actionable emergency subsistence during
  night/recovery, and share client work reach. Verify continued survival and trade over fourteen days.
- Draw actual homes/crop stages in native and browser views, preserve fog knowledge, improve
  names/inventory/market/inspector feedback, and notify agents of closed homestead outcomes.
- Keep newer applied actor effects visible after long action histories in the generic bounded
  observation summary; detailed clients retain incremental event-stream consumption.

### Living Island Village economy

- Add the opt-in `living_island` preset and `living_village` scenario, with hunter,
  tanner and tailor professions. Hunger, thirst and cold generate demand that
  propagates through real recipes to gathering, hunting, production and trade.
- Add saved deterministic market prices, bounded trading history and cash-backed
  purchasing plans. Protect urgent survival and complete manufacturing input bundles;
  keep conditional affordability bids within the buyer's working capital.
- Let clients offer actual carried goods for physical NPC purchases, withdraw offers,
  and set buy/sell price limits through the shared action API. New chains make meals,
  spears, leather, thread and equipped clothing from finite resources and wildlife.
- Show jobs, wildlife, warmth, clothing and market prices in the native/browser views;
  publish closed hunting, listing, rest and clothing events for AI continuation.
  This mode requires a fresh world. See `docs/living-island-economy.md` for commands,
  the independently BazaarBot-inspired design and its scope.

### NPC life and shared environment actions

- Run island residents through the existing reactive behavior-tree runtime, with
  inspectable water/food/recovery/night/profession goals, critical need interruption,
  shared order execution, external-control suspension and deterministic saved decisions.
- Give NPCs and clients the same validated consumption, trade, gather and recovery paths.
  Add `rest_home`, `rest_ground` and `recover_diary`, and clean up redeemed coin purses
  from their holder's inventory. NPC manufacturing respects world crafting settings.
- Register every named island recipe for headless clients and share crafting policy
  between aliases and canonical actions. Improve paid ingredient logistics with
  bounded batches and smaller purchases for tight packs, sustaining the food chain.
- Add perception-scoped resource stock/renewal and workstation input/output information;
  expose public decisions, production recipes and renewal timing in the observer views.
  Island content hashes change; this increment requires a fresh island rather than
  silently migrating old island saves. See `docs/npc-life-and-environment.md`.

### Dedicated AI Agent Game Client

- Add native Sessions views for logs and metrics in explorer controls and the
  game menu, with history/page navigation, live following, keyboard scrolling and
  read-only background loading. Add an independent AI logs folder in Settings;
  normal/fresh starts reread saved folder preferences. Keep workspace-owned links
  to earlier output folders and show actual/requested metadata and cost availability.

- Automatically save a private Markdown log for every client run, covering prompts,
  CLI conversation/tool output, game notifications, usage and lifecycle. Redact
  credentials before writing, preserve previous runs, and support `--logs-dir` /
  `NPC_AI_LOGS_DIR` independently of JSON metrics. Logging failure stops dispatch
  while preserving accepted identity and accounting.

- Add `ai-agent-game-client` alongside `npcctl` to create and resume Codex or Claude
  CLI conversations using existing subscription login. The client supplies the
  initial mission and registered game tools automatically, with bounded event/idle
  continuation, cancellation, private session persistence and explicit fresh-session recovery.
- Add native explorer and detached-world Start Codex/Start Claude/Stop agent/status
  controls; F10 reopens windowed controls. Prompt copying and manual session binding
  remain optional compatibility tools.
- Add authenticated host leases, shared turn capacity and bounded actor notification
  feeds outside simulation state. Reuse observation acknowledgments and prevent
  legacy/client duplicate dispatch. Cover both provider protocols with real-host,
  fake-provider process rehearsals, including Windows crash cleanup and recovery.
- Resolve Codex gameplay model and effort from its effective workspace configuration
  and visible catalog before dispatch, preserving supported choices and otherwise
  using only validated advertised defaults through process-local arguments. Record
  selection intent separately from provider-reported execution metrics; workspace
  preflight performs metadata requests without starting a model turn.
- Add explicit Codex `--approve-game-tools` consent for only the 16 validated
  registered game tools on owned processes, with native Start scope text and
  refusal of conflicting CLI approval settings. Report canonical game-tool
  cancellation as a closed failure, stop without retry, and preserve reported
  usage and private session continuity without retaining tool payloads.
- Stop on provider identity faults across normal polling and reconnect cleanup,
  preserve the original private conversation, and exclude foreign-session usage
  from its accounting even when a cancellation precedes the identity fault.
- Correct Codex CLI 0.145.0 token accounting across resumed conversations. Metrics
  format 2 preserves legacy preimages, labels unverified history and partial
  coverage, and keeps reported session snapshots separate from tracked deltas.

### Browser world observer

- Add opt-in `npcctl serve --world-state-out FILE --world-state-interval-ms N` for ordinary and explorer
  island hosts. Publish versioned read-only JSON between ticks through atomic replacement, preserving
  simulation hashes and private agent/diary data. The sibling `npc-world-viewer` TypeScript project
  follows the file with an interactive pixel-art ExcaliburJS island and entity inspection.

### Island Village and AI continuation

- Publish fresh successor notebooks during new-life registration, preserving
  previous-life files and removing the delay before notebook access.

- Clean generated Start/Resume prompt text and remove the BOM-prefixed Windows copy transport;
  prompt copies use UTF-8 stdin with no text in executable arguments. Preserve Unicode spelling and
  meaningful spacing, and align the briefing with speech and saved event-continuation settings.

- Add recorded actor dialogue, an explicit session-bound MCP `talk` tool, nearby addressed/aloud
  hearing and bounded resident replies. Read-only thought/action and speech bubbles explain public
  actions and dialogue for stream viewers; incoming speech can notify its connected AI recipient.

- Make AI continuation event-aware: route bounded, deduplicated actor outcomes and public phase
  changes to the exact connected conversation, acknowledge observed cursors, retain pending events
  through busy/paused/cooldown states, and expose event delivery controls/status in the native UI.
  Content summaries stay behind a host adapter; raw journal payloads and private data are excluded.

- Prepare Herp's stream with a dedicated operator run sheet, clearer remaining-reserve and queued-action
  guidance, registered cancellation/restart/Diary regressions, and three-day economy/season-boundary
  fixtures. Throttled actor captions explain residents' current work; the observer HUD reports actual
  gathering, purchases, sales, meals and lodging, including the visiting actor's seller perspective.

- Expose perceived merchant demand, concrete trade refusals and actor activity without revealing
  merchant wallets. Add island onboarding and MCP-only Codex workspace configuration, native activity
  and recovery feedback, read-only continuation preflight, and maintained first-playtest fixtures/runbook.

- Polish world captions with measured, bounded placement and actor priority; stabilize the diary reader's
  selected book, page boundaries and scroll position. Scheduler controls name their next action and status
  reflects current pause/control/death eligibility. Successors keep the scenario's selected actor preset
  even when unrelated presets are registered earlier; future births remain consistent across resume.

- Add the opt-in `ai_village` scenario and `island_village` explorer preset: nine named residents operate
  physical food, wood, metal and tool chains with finite inventories/wallets and a cash-conserving community
  fund. Visiting actors gather/craft/sell to buy food, clean water and lodging. Empty supplies allow
  producers to restart work rather than waiting on their own missing output.
- Add day/night and seasonal supply, blocked island water, fatigue/health and permanent death. Death drops
  possessions/currency and a recoverable diary; a new recorded life creates a different actor in the same
  continuing world, with fresh books/senses and reassigned registration. Original files remain preserved.
- Publish only deceased actors' Diary pages through a recorded content command. Private Memory/Proposals,
  map knowledge and credentials stay outside the dropped book; unreadable/oversized exports report errors.
- Add scene-configurable host AI supervision and `npcctl scheduler bind|unbind|status`: named Codex/Claude
  CLI resumes, idle/work/control checks, cooldowns, bounded concurrency/rates, owned-process timeouts and
  cleanup. Operator session links remain outside scene/replay data; status queries never dispatch.
- Extend the native view with the island/coast, workplaces, calendar/reserves/currency, diary reading,
  explicit successor controls and supervisor status. World presets gain an AI scheduler settings page.

### Performance explorer

- F4 groups measured work by ownership domain, then mechanic, system and nested execution path. It exposes
  self/inclusive timings, deferred/condition costs, full selected names and bounded frozen captures.
- Separate actual frame rate, achieved simulation ticks/s, configured speed/rate, per-tick wall cost and a
  diagnostic frame target. Effective budgets show inclusive p95, usage, warmup and unobserved states.
- History charts retain 240 half-second samples of frame mean/peak/target, actual/target simulation throughput,
  selected self cost or inclusive budget p95, and per-tick p95. Recording continues with the panel hidden;
  frozen captures retain history, missing/inactive budget measurements stay gaps, and reset clears diagnostics.
- Add explicit tree/selected/capture queries to `npc/perf` while preserving flat reports and existing limits.
  ECS trace/debug support and targeted span filters keep system measurement available with quiet logging.
- Runtime parent tracking handles cached parentless Bevy spans, concurrency/reentrancy and resets; bounded
  depth/cardinality warnings expose incomplete capture. Overlay controls shield world input and respect modals.

### Editor safety and polish

- Worlds now protects unsaved edits on Back, Escape and window close; selected presets and their optimistic
  save baseline come from one fresh read, including BOM-prefixed files and files changed externally.
- F7 gains complete Tab/Shift+Tab navigation, Enter/Space actions, visible focus and scroll-into-view.
  Ctrl+Tab switches books; Escape preserves drafts. Focused controls shield world shortcuts and respect modals.
- Menu/F7 controls have native accessible names; F7 selection is exposed to accessibility. Menu fields show
  focus, secondary button text meets enabled-state contrast, action rows wrap, Worlds keeps the shared frame,
  and empty Resume offers a direct New-agent action.

### Fixed (final review)

- Crafting no longer overfills a full pack merely because a matching stack exists. Separate output IDs and
  equipment's slot exemption remain; carried outputs appear in inventory deltas and dropped outputs report
  their fixed completion positions.
- Notes/map update revisions stay unique across tick/date rollback and reopen, preventing stale-token reuse.
- CLI and HTTP server Debug representations redact all token fields without changing parsing or dispatch.
- Picked-up/equipped/restored items follow real ownership even without SimPos; hidden inventory stays hidden,
  hand/grip attachments work, and unseen pickups retain their last-seen ground appearance until observed empty.
- Position validation safely rejects signed integer extremes while retaining the closed supported bounds.
- LIDAR reuses solid-cell scratch bitmaps across subcell movement and combines `SOLID`/`VISITED` flags, clearing
  fog coverage on every scan. All beams, endpoints, corner traversal and noise remain unchanged; the original
  9,000-case oracle and additional cache/snapshot/shared-scratch regressions retain full-scan coverage.
- Plain explorer skips empty survival/night runtime systems and unused actor base state. Snapshot registration
  and clock setup remain unchanged; worlds with active needs or night sensing penalties retain their updates.

### Documentation and end-to-end verification (plan Task 18)

- Updated contributor/user guidance for the actor and world editors, registered MCP lifecycle, persistent
  notes/map, configurable perception memory, fog and exact-port detached/window handovers. See the
  end-to-end evidence in `docs/_notes/notes-workspace-e2e.md`.

### Added (landmarks, plan Tasks 21–23)

- Actors keep an owned map of fixed landmarks: agent/player list/add/update methods, session-bound MCP tools
  and developer CLI commands. Positions can come from here, perception memory or explicit coordinates.
- Observe includes the five nearest landmarks. Move orders can target a landmark; recorded inputs resolve to
  positions and replay without map files. The world map switch defaults on and can disable the feature.
- F7 has a Map tab with stale-save protection; noninteractive landmark labels remain visible through fog.
  The shared notes/map store detects byte changes, guards source deletion and reports exhausted IDs/revisions.

### Added (world presets, plan Tasks 15–17)

- World presets configure needs, battery, day/night sensing, detection, perception memory, systems and
  inhabitants. New explorers select actor and world together; saves retain immutable copies of both presets.
- Optional survival mechanics provide sources, recovery actions, movement penalties and exhaustion; night
  vision/camera ranges scale from their base values. Plain explorer preserves its existing world and hashes.
- The game menu's Worlds editor exposes preset fields with live validation and guarded atomic saves; settings
  includes a worlds folder and Resume identifies both selected presets.

### Added (registered explorer agents, plan Tasks 30–31)

- Explorer agents receive a persistent public ID and secret token, register with the server, enter their world
  and explicitly take control of their assigned actor. Sessions and credentials are host-only; plain scenarios
  retain their unlocked interface. Token rotation revokes the old session and refreshes the generated handbook.
- Generated explorer MCP configuration uses session-bound registered mode; legacy MCP clients remain available
  for ordinary scenarios. Generated handbooks and dungeon-master briefings teach only the MCP workflow.

### Added (actor editor, plan Task 14)

- **Actors** in the game menu opens the Actor Testbed's standalone `--actor-editor` mode: a preset list,
  New/Duplicate/Revert/Save, a rotating 3D model preview and model picker, and explicit forms for identity,
  attributes, skills, traits, actions and every sensor parameter (including reach). Live validation disables
  invalid saves; the character-point total stays visible. Body and Sensor ranges views share real metre units.
- Preset writes are atomic. Another file at the chosen slug, or a source changed on disk, requires explicit
  replacement confirmation. Ordinary edits retain the selected file's stable preset ID; renamed/new copies
  use the name's slug and preserve the original. Unsaved navigation and window close require discard approval.
- Settings now exposes the optional actors folder. CLI capture helpers: `--editor-preset ID`, `--screenshot`
  and `--after-secs`; the editor runs without a simulation driver. Shared preset validation safely rejects
  extreme numeric values before core point-buy arithmetic; `sheet_point_total` exposes a checked `i64` total.

### Added (robots in the explorer, plan Task 13)

- The New-agent screen lists valid actor presets with their descriptions and passes the chosen preset to both
  windowed and detached worlds. Resume rows show each agent's preset. The Rover and Sentinel have procedural
  bodies, wheels, sensor domes and tool grips; the view honors every preset's model selection.
- Robot fog follows its sensors: camera cones with line of sight, LIDAR geometry through the first beam hit,
  and radar target cells. LIDAR alone reveals no named items or live object state. `VisibleCells.identified`
  keeps that coverage separate from geometry (snapshot-only, default absent for older saves). Robot fog caches
  invalidate on position, facing, sensor settings, obstacle cells, radar detections and grid revision.
- LIDAR traversal keeps scaled crossing times, converting the distance only at a hit, and uses checked bitmap
  indexing within the scan window. Range scans and fog share the bitmap beam walker; moving robots retain their
  cache entries and buffers. Full scans and the existing conservative corner behavior are preserved.

### Security

- `cargo deny` ignores RUSTSEC-2026-0391 (zstd 0.13.3), RUSTSEC-2026-0398 and RUSTSEC-2026-0403 (zstd-safe 7.3.0), published 2026-10-26: all come in through Bevy 0.20's `bevy_image` (`zstd_c`); the fixes need zstd 0.14 / zstd-safe 8.2, which Bevy 0.20 cannot take, and the other backend (ruzstd) has RUSTSEC-2026-0344. The flawed paths are not reached (no reused context, no `seekable` feature, only trusted embedded current-format LUT frames). The dev's decision; drop the entries when Bevy moves to zstd 0.14.

### Changed (robot sensors fix round 1, plan Task 12)

- **npc_perception**: an idle robot reuses its traced LIDAR ranges (keyed by entity, position, facing, LIDAR,
  `SpatialGrid::revision` and the obstacle cells of its scan square) and only draws its noise again; the LIDAR
  scans half a stagger after the camera/radar pass (`is_lidar_due`; `SensorReadings::lidar_tick`), never in the
  same tick. `LidarObstacles` (set with `PerceptionAppExt::set_lidar_obstacles`, not hashed; default: every kind)
  says which perceivable things stop a beam in their cell. Camera track ids skip ids still in use.
- **testbed_content**: `world::lidar_obstacles()`: units, vehicles, trees, ore rocks, workstations and chests stop
  a beam; items do not, and a door only through its cell (closed: blocking; open: the beam passes).
- **npc_framework** (`perception_agent`): the LIDAR, camera and radar text lines come together right after `You
  perceive` (their sections carry only the JSON); `1 degree`, not `1 degrees`.

### Added (LIDAR, camera and radar with ROS-shaped readings, spec G10, plan Task 12)

- **npc_perception**: robot sensors, appended to `Sensor` (`Lidar`, `Camera`, `Radar`; builders `Sensors::lidar`,
  `camera`, `radar`; hash tags 3–5, so existing encodings are unchanged) and `SenseKind::{Camera, Radar}` (tags 4, 5).
  On an agent's scan tick the perception pass also runs them and writes `SensorReadings` (snapshot-only
  `sensor_readings`, added to agents with a robot sensor; never hashed): the LIDAR's `LaserScan` (ROS
  `sensor_msgs/LaserScan` in mm / millidegrees: per beam the distance to the first blocking or occluding cell or
  cell holding a perceivable thing, a miss = `range_max_mm + 1`, `±noise_ppm` noise from `sim_rand` purpose
  `LIDAR_NOISE_PURPOSE`; no identities, no memory), the camera's `CameraDetection`s (`vision_msgs` style: class =
  kind name, track id stable while in view, bearing + = left, distance, size, confidence 990000 → 600000 ppm over
  its range; cone + line of sight) and the radar's `RadarTrack`s (`radar_msgs` style: bearing and range rounded to
  its resolution, radial velocity from the range's change since its previous scan, amplitude higher for things that
  moved; line of sight). Camera and radar feed `PerceptionMemory` **automatically** (no detection check: a sensor
  reports what is there); priority vision, camera, radar, proximity, reach. Integer angle helpers (`atan2_mdeg`,
  `dir_ppm`, `bearing_mdeg`, `facing_angle_mdeg`, offline sine/tangent tables). A 360-beam, 20 m LIDAR marks the
  reachable square once per scan in a reused bitmap and walks it with the same supercover as `trace_segment`
  (proven equal by a unit test); nothing is allocated per scan once warm. Fog of war ignores the new sensors until
  Task 13.
- **npc_kernel**: `SpatialGrid::for_each_solid` (the blocking or occluding cells of a rectangle, read by row like
  `for_each_occluding`).
- **npc_framework** (`perception_agent`): observation sections `lidar`, `camera` and `radar` (the readings as
  JSON; text: nearest obstacle per 8 compass sectors, one line per detection / track, capped at 8 with a per-class
  / moving-still summary). `fog_agent::{SECTORS, sector_of}` are public (the glue's compass). `per_roll` never
  rolls for camera or radar (own dice purposes if ever asked).
- **testbed_content** (`actors`): the rover (LIDAR 20 m / 360° / 360 beams + camera 15 m / 35°) and the sentinel
  (radar 30 m / 60° + camera 15 m / 35°), both with reach 1.5 m, are valid and get their sensors from
  `apply_preset`; `ROBOT_SENSORS_LATER` is gone; at most one `lidar`, `camera` and `radar` per preset. No hash of a
  world without robots moved (testbed, village, the human explorer world: `existing_hashes_unchanged`).

### Changed (quitting the game stops every process it started, plan Task 29)

- **game_menu**: Quit (Main's button, the window's close button, or Ctrl+C on the menu's console through Bevy's
  Ctrl+C handler) now stops every process the menu started or adopted before it exits, in parallel on worker threads,
  on a new **Quitting...** screen that lists each one and its state: tracked detached worlds get `session/stop` (they
  save first; an unanswered stop is asked again within the wait, for a world still taking its port over after F9)
  and up to 15 s to exit (one that saved but still runs is then killed); an explorer window gets `app/quit {pid}` (it
  saves; once it answered it gets up to 10 s more) and is killed through the menu's child handle when it has not quit
  in time; the Village, testbed scenarios and the RTS units demo have nothing to save and are killed at once (ruling
  R17); a world still starting is killed after 15 s; a build or a save check is killed at once (`QuitWait`). Every
  kill removes the `run.json` it leaves behind. An adopted world (a `run.json` pid) is killed only after
  `run::kill_adopted` checked the pid is still its `npcctl`/`actor_testbed` and did not start after the `run.json`; a
  reused pid is never killed (the world is forgotten, its stale `run.json` removed). An Open in window or Stop in flight
  is waited for first (nothing opens or restarts after it). A world that cannot be stopped keeps the menu on that
  screen with the world and the reason (refused: its save failed; or no answer: `StopError`), **Quit anyway** (kill;
  the save stays at the last autosave, `run.json` removed) and **Back** (it keeps running; not after Ctrl+C). This supersedes spec G7's "a detached world outlives the menu" for the menu's Quit;
  Quit is no longer disabled while a Stop or Open in window runs (`BUSY_QUIT` is gone). Processes the menu did not
  start or adopt, and `npcctl mcp` servers (the AI tool's), are never touched.
- **npc_view_helpers** (`remote`, feature `remote`): `app/quit {pid?}` (`install_app_quit`, `AppQuitPlugin`,
  `app_quit`, `quit_when_due`, `QUIT_DELAY`): answers `{quitting, pid}`, then the app exits 0 200 ms later through an
  ordinary `AppExit`, so its exit systems run; a pid that is not the app's is refused. Registered by apps only.
- **actor_testbed**, **village**: register `app/quit` with their BRP. An explorer window's `app/quit` takes the
  close dialog's "No" path: save, remove `run.json`, exit 0.
- **testbed_content** (`explorer::host`): `remote_call(port, method, params, timeout)` (`session_call` uses it),
  `remote_call_checked` (`RemoteError::Refused` / `NoAnswer`), `parse_rfc3339` (the inverse of `rfc3339`).

### Added (actor presets as data, spec G9–G11, plan Task 11)

- **testbed_content** (`actors`): actor presets, one JSON file per preset in the actors folder (`Settings::actors_dir`,
  default `npc-actors` beside the agents folder; the built-in `human`, `rover` and `sentinel` are copied into a folder
  that has no preset). `ActorPreset { name, description, body, model, sheet, speed_mm_per_tick, carry_capacity,
  actions, sensors, perception }` (unknown fields and sensor types refused); `load_presets`, `find_preset`,
  `validate` (one reason per problem: the sheet's traits, skills and limits within a 100-point budget, speed, carrying
  capacity, sensor ranges 1..=50 000 mm, reach 0..=5 000 mm and at most one, fields of view, beams, known actions
  and models, the world's detection check), `apply_preset` (sheet, speed, carrying capacity, vision/hearing, `ReachSense`,
  `AllowedActions`, `ActorModel`; both new components snapshot-only, never hashed). Until Task 12 the robot sensors
  (LIDAR, camera, radar) are refused ("this sensor arrives with the robot sensors (Task E2)"), so the rover and the
  sentinel show as invalid. A scenario actor may name a preset (`"preset": "<id>"`, applied at spawn).
- **testbed_content** (`explorer`): the explorer's actor is built from the chosen preset (`HostArgs.preset`, default
  `human`; `HostArgs.actors_dir`); an unknown or invalid preset is refused with its reasons before anything is
  written. `agent.json` stores the preset id (`AgentMeta.preset`, `human` for older folders) and the save header
  the preset itself (`preset`, `actor_preset`), so a resume and `world/reset` rebuild the same actor even after the
  file changed. A resume keys the preset by the save (a save from before presets is the built-in human); an
  `agent.json` that disagrees is corrected and the host says so (`ExplorerSession.notes`, also printed to stderr);
  resuming with another `--preset` is an error. `--fresh-world` reads the preset file again (an edit applies) and,
  when the file is gone, uses the old save's copy with a note. The preset's `reach` replaces the fixed 1.5 m
  (`EXPLORER_REACH_MM` stays the built-ins' value); every preset keeps `RevealsFog`. `AGENTS.md`'s "Your character"
  comes from the preset (`ActorSummary::from_preset`): the human renders as before, its description now the preset's
  sentence followed by speed and load. The `human` preset's world has today's content and state hashes.
- **npc_agent_io**: verb gates (`register_verb_gate`, `verb_gate`, not in the content hash): a refused verb's
  affordances say why (`why_not`) and are marked structural (`Affordance::structural`, a new unserialized flag that
  `structurally_impossible` honours whatever the reason's words), so `brief` hides them; `agent/act` rejects the
  verb (`not_allowed`). The testbed's gate refuses an actor's known actions outside its `AllowedActions` ("this actor
  cannot pick_up").
- **npc_framework** (`glue::command_agent`): `order_verb` names the verb an order makes a unit do: `Interact` its
  verb; with `vehicles`, `Mount` `mount`, `Unload` `unload`, and `Move`/`AttackMove`/`Patrol`/`Follow` `drive_to`
  for a vehicle or a unit in a driver seat (`ByCrew` or `Direct`). An `order` or `interact` act is refused
  (`not_allowed`) when a verb gate refuses that verb to an ordered unit or, for orders to another unit, to the acting
  unit itself.
- **actor_testbed** / **npcctl serve**: `--preset ID` (New) and `--actors-dir DIR`; **game_menu** passes the
  settings' `actors_dir`, when set, as `--actors-dir` to every explorer host it starts (window and detached).

### Added (searched ground in observe, spec F4, plan Task 28, proposal 004)

- **npc_framework** (`glue::fog_agent`): the perception section of an actor with an `ExploredCells` record carries
  `searched {cell_mm: 10000, radius_mm: 60000, within_radius_pct, map_pct, sectors: [{dir, explored, total}; 8]}`
  and the text line `Searched: 34% within 60 m, 12% of the map; least searched: north-east, east.`. The record
  is coarsened to 10 m cells (explored when at least half of the fine cells are); sectors are N..NW (`+z` north) with
  ties in that order (sectors of fewer than 2 coarse cells, edge slivers, are never named); `map_pct` is the explored share of the whole grid (no start position is stored). `cell_mm` is the real coarse edge; a cell of more than 400 fine cells is sampled (at most 20 x 20 tests). Read-only, computed per observe, never
  hashed: no replay, golden or state hash moves.
- **testbed_content**: `AGENTS.md` says that observations show how much nearby ground the actor has searched.

### Added (fog of war in the window, spec G8, plan Task 10)

- **testbed_view** (`fog`): the Actor Testbed window draws the fog of war of the `RevealsFog` actor (the explorer),
  like Age of Empires: never-explored ground black, explored ground not visible now greyed, visible ground clear.
  A mask plane over the ground holds one texel per grid cell (`mask_texel`: black, dark grey at alpha 200, clear;
  linearly filtered), rewritten only when a cheap hash of the explored and visible bits changes (checked once per
  sim tick). Statics in ground not visible now are drawn dimmed **as last seen** (`SeenStatics`: a door seen open
  stays open, a tree seen standing stays standing, an item where it was seen) by swapping their meshes to cached
  darkened materials; a static never seen is not drawn; wall runs are fogged piece by piece by their cells, and the
  view-only dressing by its cell. **Units and vehicles are drawn only while the actor senses them** (Ruling R14:
  `sensed` in its `PerceptionMemory`, never by the fog cells); the actor and the vehicle it rides always. Pure
  functions `unit_look`, `static_look`, `area_look`, `mask_texels`, `mask_hash`. Read-only on the sim: no state
  hash, replay or golden changes.
- **F8** toggles fog; resource `FogOfWar { enabled }`. **actor_testbed `--fog on|off`** (default on in explorer
  mode, off elsewhere; without a `RevealsFog` actor everything shows anyway). The F1/F5/F6/F7 panels are not fogged.
- **Settings** `fog_of_war: bool` (default true; older files load as true) in
  `testbed_content::explorer::settings`; the game menu's Settings screen has a **Fog of war** checkbox (saved at
  once) and explorer windows (New, Resume, Open in window) get `--fog on|off`; detached worlds have no view.
  `launch::save_settings` takes the fog flag.
- **Review fixes (Task 10 round 1).** `npc_view_helpers::ViewHidden` (new, generic): sim entities the view hides;
  the crew and path gizmos skip them, they cannot be clicked, box-selected or right-click picked, and a selected
  one leaves `Selected` / `CommandSelection` (so no selection ring or order gizmo follows it into the fog). The fog
  fills it once per tick with every visual whose look is hidden (units it does not sense, statics never seen).
  **Ruling R15:** the F1 item markers follow the item's look under fog (none when hidden, at the last-seen spot
  when drawn as last seen); the rest of F1 and F5/F6/F7 stay unfogged. A mesh whose material was not loaded when
  it should have been dimmed is retried until it is. A headless test builds and runs the whole
  `TestbedViewPlugin` (`content/testbed_view/tests/plugin.rs`).

### Added (fog of war, sim side, spec G8, plan Task 9)

- **npc_perception** (`fog`): opt-in marker `RevealsFog` brings `ExploredCells { width, bits }`, `VisibleCells { bits }`
  (one bit per grid cell, `u64` words over `GridBounds`) and `SeenStatics(BTreeMap<SimId, SeenState { kind, pos, open,
  felled, footprint }>)`. On the actor's scan ticks, right after the perception systems (`fog_system`), visible =
  the cells its vision cones (`reveal_cells`: centre in range and cone, with `line_of_sight`) and its `ReachSense`
  (`reveal_reach_cells`) reach, plus its own cell; explored |= visible; statics standing in visible cells are recorded
  as seen now, and records whose cell is seen empty are dropped. A record made for another grid size starts over.
  All four are snapshot-only (`reveals_fog`, `explored_cells`, `visible_cells`, `seen_statics`, ADR 0013): the
  explored ground survives save/resume, and no state, content hash, replay or golden changes.
- **`FogStatics`** (`PerceptionAppExt::set_fog_statics`, not hashed): content says which `Perceivable` kind bits are
  static, the kind for `GridOccluder` entities without `Perceivable` (wall runs), and `fn(EntityRef) -> Option<bool>`
  readers for "open" and "felled". Default: nothing recorded.
- **Testbed**: `world::fog_statics()` (doors with their open state, trees felled or standing, ore, stations, items,
  chests; walls as the new `kinds::WALL`, `kinds::FOG_STATICS`), registered by `TestbedPlugin`. **Explorer worlds**:
  the actor gets `RevealsFog` in the host's `apply_senses` (New, Fresh, Resume, reset); the shared scenarios are
  unchanged.
- **Fog cost (review fix round)**: the fog update is its own exclusive system, `fog_system` in the new
  `NpcFogSet` (`NpcSet::Sense`, after `NpcPerceptionSet`), with its own perf mechanic `fog` (tag
  `npc_perception::fog`) and a budget row `fog` 55 + 0 µs (approved by the dev), so `perception`
  keeps its budget. Fog is per cell: seen from the centre of the actor's cell, with lines of sight traced once per
  range into a table and the scan area's occluders read once (`SpatialGrid::for_each_occluding`, new); a scan
  reuses the visible cells while the actor's cell, facing, sensors and the grid's cells (`SpatialGrid::revision`,
  new, not state) are unchanged, and touches only statics that can be in view, allocating nothing once warm.
  Release explorer world, perception p95: windowed 132 µs before, 38–46 µs after (fog p95 16 µs idle, 38 µs
  walking); headless idle 62 → 6.5 µs (fog 2.1), walking 79 → 8.1 µs (fog 13.6).
- `ExploredCells` records the grid's `min` too (`serde` default: older records still load) and a record that does
  not fit the loaded grid starts over at the load: a `warn!`, `FogResets` and, in explorer worlds, a line in the
  load report's `skipped`. `VisibleCells::fits`; `is_visible` is false for bits of another size. `SeenState` keeps
  a wall run's `footprint`, so a wall seen gone at any of its cells is dropped.

### Added (reach sense; vehicles and units are remembered, spec F3, plan Task 27)

- **npc_perception**: opt-in `ReachSense { radius_mm }` (next to `Sensors`): every `Perceivable` thing within the
  radius, whatever the facing, with the same grid line of sight as vision (never through a wall), on the agent's scan
  ticks into `PerceptionMemory` as `SenseKind::Reach` (`"reach"`, hash tag 3; vision, then proximity win). It is
  registered snapshot-only (`reach_sense`, ADR 0013): no state- or content-hash entry, so worlds without it keep every
  hash. `perception_json` shows it. **per_roll** rolls reach with the proximity modifier (dice index sense 3).
- **Perception text** (`perception_agent`): a `Within reach: chest #54 (behind you), forge #15.` line for units with
  a reach sense (sensed things within reach, nearest first; `behind you` / `beside you` for one only reach senses),
  JSON `reach_mm` and `within_reach`.
- **Explorer worlds**: the actor gets `ReachSense { radius_mm: 1500 }` (`EXPLORER_REACH_MM`; also on resume of an
  older save and on reset), and chests become `Perceivable` (new kind bit `kinds::CONTAINER`), so a chest in reach is
  sensed and remembered (proposals 007, 011); in explorer worlds chests are perceivable to every NPC, not only the
  agent's actor, and a chest spawned at runtime stays imperceptible until the next resume or reset. AGENTS.md's senses
  list "reach: 1.5 m all around, through nothing solid". A missing explorer actor is now an error, not a world
  without reach.

### Fixed (proposal 009)

- **"You remember"** lists the 24 most recently seen things (`REMEMBER_TEXT_LIMIT`) plus every vehicle and unit
  (a `Brain`, `Commandable` or `Vehicle`, whatever its age), then `; and N more (40 tree, 12 item)` by kind name
  (count descending, then name); the JSON `remembered` stays complete. It used to stop after the eight lowest
  `SimId`s ("and N more"), so vehicles and units, spawned after the world and so with the highest ids, were hidden
  whenever more was remembered. Memory itself never dropped them by kind; the one-minute rule still applies.
- **"You perceive"** follows the same rule: the 8 nearest (`PERCEIVE_TEXT_LIMIT`, was the 8 lowest ids) plus every
  vehicle and unit, then the per-kind summary.

### Added (the menu runs detached worlds, spec G7, plan Task 8)

- **game_menu**: New has **Start in window** and **Start detached**; each Resume row starts in the window and has
  **Start detached** (both after the save check; the mismatch dialog keeps the choice). Detached runs `npcctl serve
  --explorer-home .. --agent ..` (its own console and process group: it outlives the menu) and shows the new
  **Detached** screen: "<agent> in <world> running detached on port N, tick T" (`session/info` every 2 s on a
  background thread), the URL, the prompt without its frontmatter with **Copy** (the shared clipboard module), **Open
  in window**, **Stop** and Back. Stop and Open in window run on a worker thread: `session/stop` (the world saves
  first), then the exit is waited for up to 15 s. Stop returns to Resume ("<agent> stopped at tick T and was
  saved."). Open in window aborts on a refused stop (a failed save) or a process still running, and the world keeps
  running detached; a process that exited non-zero or wrote no newer save is restarted detached from its last save
  on the same port; else `actor_testbed --resume --exact-port N` opens it, and a window that exits 4 or 2 restarts it
  detached on `N` with the reason. The Resume list adopts live `run.json` worlds ("running detached on port N - Open /
  Stop"; one running in a window is disabled). `--detached AGENT` opens an agent's Detached screen. The menu never
  kills a detached world; it only stops it over `session/stop`.
- **actor_testbed** (explorer mode): closing the window asks **"Keep it running detached?"** (`close_when_requested`
  is off there): Yes saves, starts `npcctl serve --explorer-home .. --agent .. --resume --port N --wait-port` on the
  window's port (detached from the window), prints `{"detached":{"pid","port","agent","world"}}` for the menu (which
  then shows the Detached screen) and exits; No saves and exits; Escape / Cancel goes back. The game's input does not
  run while it is open and no text field keeps the focus (Y / N / Escape answer it). **F9** moves the world to
  detached without asking. `--close-dialog` opens the dialog at start (visual checks).
- **`npcctl serve --wait-port`** (with `--resume --port N`): a handover from the closing window: waits up to 10 s, in
  one budget, for `N` to come free and for the window (its `run.json` pid) to exit; never another port (exit **4**,
  "port N is still in use"). The window spawns the detached host before it exits, so the host must wait: this flag is
  that wait (the window cannot drop its BRP server and still spawn after `App::run`).
- `testbed_content::explorer::host`: the clock-free handover waits `wait_for_port_until` / `wait_for_handover_until`
  (the caller passes "is the budget spent?": actor_testbed and npcctl own the clock, this crate reads none),
  `Handover`, `running_elsewhere` and `HANDOVER_WAIT` (moved from actor_testbed), plus `detached_resume_args`,
  `detach_command`, `session_call` (the menu's JSON-RPC to a detached world), `DetachedLine`, `latest_prompt`,
  `prompt_text`, `pid_dead` (a confident "gone" only) and `RunFile::read_any`.
- Review fixes: the Resume screen asks the OS about `run.json` pids on a thread ("checking whether it runs..." until
  it answers) and deletes a `run.json` whose pid is clearly dead; a Stop or Open in window that aborts from a Resume
  row shows its reason on the world's Detached screen; Quit (and the menu window's close button) waits while a Stop
  or Open in window is in flight; Stop of a found (adopted) world counts as clean only with a newer save; a failed
  move to detached puts the window's earlier pause state back; the "moved to detached" line never panics on a closed
  stdout; the prompt modal is frozen while the close dialog is open.
- Known limits: a detached world the menu started keeps its stdout/stderr piped to the menu (no log file yet), so
  after the menu quits its later log lines are lost and its final stdout line may fail (it has saved by then). A
  window-to-detached handover whose `npcctl serve --wait-port` cannot take the port (still taken after 10 s) fails
  silently: the window has exited and saved, nothing runs, and the save is intact (Resume it).

### Added (detached explorer worlds, spec G7, plan Task 7)

- **`npcctl serve --explorer-home DIR --agent NAME [--world W | --resume [--load-anyway | --fresh-world]]
  [--preset human] [--port N] [--paused]`** hosts an explorer world without a window (npcctl never renders):
  built or resumed by `testbed_content::explorer::host`, served with the session's agent methods. It prints
  `{"listening","port","agent","world","prompt"}`, writes the agent folder's workspace for its URL and
  `run.json` (mode `detached`), autosaves every 60 s of wall time and saves on `session stop` or Ctrl+C (the new
  `ctrlc` dependency of npcctl), then removes `run.json` and exits 0. A failed save on `session stop` refuses the
  stop (the world keeps running). Without `--port` it takes the first port in 15702..=15721 whose next port is
  free too; a second host for a running agent is refused. `world/reset` rebuilds the explorer world with its
  notes resources (`host::reset_world`); `world/load_scenario` is refused there.
- **`npc_agent_io::session`**: `Session::start_app(app, options)` / `start_app_on(app, server, options)` host a
  world the host built (no `--record`); `SessionOptions` gains `on_frame` (after every frame and step slice),
  `on_stop` (a final save; an `Err` refuses `session/stop`), `rebuild` (`world/reset` of such a session) and `stop`
  (an `AtomicBool` a Ctrl+C handler sets). `SessionOptions` is no longer `Clone` (it holds the hooks).
- **`actor_testbed --explorer-home .. --resume --exact-port N`**: moves a stopped detached world into the window
  on exactly port `N`, retried for up to 10 s, never another port (exit **4**, "port N is still in use"); no prompt
  modal on this path, the sim runs and a 5 s notice says "moved here from detached (port N)". Every explorer window
  writes `run.json` (mode `window`) and removes it on exit. `pick_port` / `PORT_SPAN` moved to
  `testbed_content::explorer::host` (re-exported by actor_testbed).
- Review fixes: once a stop is accepted, the session runs nothing more: calls queued behind `session/stop`,
  parked long-polls and steps are answered "the session is stopping" (`session::STOPPING`), so the final save is
  the world's last state; `on_stop` extras never replace `stopped` / `record`; an app finished without a driver
  is refused with a message. The window refuses an agent whose `run.json` names another live process (exit 2,
  "agent X is already running (pid P, mode M, port N)"); `--exact-port` waits within its 10 s for the detached
  host to exit first. A detached `world/reset` keeps the old save as `<world>.world.<date>.json`. `npcctl serve`
  exits 3 for a save from another build, as `actor_testbed` does. ctrlc's `termination` feature is on (SIGTERM /
  SIGHUP on Unix; on Windows closing the console window is not a graceful stop and can lose up to the last 60 s
  autosave interval: use `session stop` or Ctrl+C). A reset whose save fails and whose backup cannot be put back
  says so and names the backup.

### Fixed (item and crafting feedback, agent proposals 002, 005, 006, 008)

- **A tool yield says where it went** (proposal 005): `use_tool_on` yields are always placed at the actor's feet (a
  sim rule, unchanged; they never went into the pack). `JournalKind::Interacted` gains `dropped: [{item, kind,
  stack, pos: [x, z]}]` (`npc_kernel::journal::DroppedItem`; left out of the JSON when empty) for a success that put
  new items on the ground, and `reason` then says why the pack could not take them (`"pack would carry 30950 of
  30000 g"`, `npc_interaction::items::PackFit::refusal`). Event lines and the `changes` text read "use_tool_on #42
  success, 2 × log dropped at (24.08 m, 1.10 m): pack would carry 30950 of 30000 g" (`observe::dropped_text`). The
  `use_tool_on` offer shows where the yield lands and the load after picking it up: "yields 2 × log at your feet
  (pack 24950 -> 30950 of 30000 g, too heavy to pick up)" (or ", no free slot to pick up";
  `glue::interaction_agent::yield_load`). Journal and observation only: computed while the journal records.
- **A craft names every input and tool** (proposal 006): `npc_crafting::can_craft` refuses a craft with missing
  inputs or tools with the new `CraftError::Missing { needs: [CraftNeed {key, tool, need, have}] }`: every recipe
  input and tool with what the crafter has, the missing ones first ("needs 1 plank (has 0), 1 ingot (has 2), tool
  hammer (has 1)"; `tool:` prefixes are dropped in text, `npc_crafting::tool_name`). It used to name only the first
  gap (`MissingTool` / `MissingInput`, still used by the completion check). The plan of a craft that can start is
  unchanged. The `craft:<recipe>` label says what it consumes and uses ("Craft forge_axe at workstation #14:
  consumes 1 ingot, 1 plank; uses hammer").
- **Craft odds, and defaulted skills on the sheet** (proposal 002): a craft line carries its roll's odds like a tool
  use (`check`, and "[Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]": the effective skill plus the recipe's
  modifier and the planned tools' quality; `glue::interaction_agent::odds::skill_check`). With `progression`, the
  `sheet` section lists a skill the unit has rolled without buying it at its default, in the new `used_defaults`
  (`[{"name","level","default"}]`) and on the Skills line ("Carpentry 7 (default DX-5)").
- **A full pack says what to drop** (proposal 008): a refusal for the actor's full inventory (`pick_up`,
  `unequip`, and `equip` into a taken slot) names the lightest items that free a slot, by weight then `SimId`, at
  most three: "inventory is full (8 slots): drop 1 of [plank #66, ingot #89] to make room", "main_hand holds #17
  (unequip it first; inventory is full (8 slots): drop 1 of [..] to make room)"
  (`npc_interaction::items::with_room_hint`), in `why_not`, the action rejection, the `Interact` command refusal
  and the journal. The verbs' own reasons stay the bare rule, because an `Interact` order keeps its failure reason
  in hashed state. "9 of 8 slots" was not a counting bug: equipped items take no slot, and the agent counted the
  main-hand item from "You carry". The load line now says so when something is equipped ("8 of 8 slots used;
  equipped items (pickaxe #17 in main_hand) take no slot, but unequipping one needs a free slot"), and the
  `inventory` section has `"equipped_use_slots": false`.

No hashed state, recorded command, replay or golden changes. `JournalKind::Interacted` has a new field, so code
that builds it needs `dropped: Vec::new()`.

### Fixed (movement and vehicle feedback, agent proposals 001, 003, 010)

- **`drive_to` from the driver seat of a `Direct` vehicle** (proposal 010): the testbed's wagon is `Direct` (a unit of
  its own), and `steered_vehicle` accepted only `ByCrew` driver seats (`npc_vehicles::driven_vehicle`), so a unit
  shown "Mounted {role: Driver, seat: 0}" was refused "riding #30 but not in its driver seat". A unit in the driver
  seat of a `Direct` vehicle now drives it (the recorded `DriveTo`, in `Advisor` and `Direct` mode; busy "driving
  vehicle #30 to (x, z)" while it moves) unless the vehicle is on autopilot (`glue::vehicles_agent::autopilot`:
  another controller, its own behaviour tree, or with `command` its orders drive it); then `drive_to` says so and how
  to take control ("wagon #30 is on autopilot: its own behaviour drives it; attach a controller to #30 in Direct mode
  to take control"). The `vehicle` section reads "self-propelled, steered from its driver seat" or "on autopilot"
  instead of "moving on its own". Passengers keep "riding #N but not in its driver seat". The driver seat of a
  `Direct` vehicle now also offers `unload@<vehicle>` (the `unload` handler uses the same `steered_vehicle`), so
  that unit can get everyone off. The `drive_to` label and the busy reasons speak metres: "Drive to (4.50 m,
  0.50 m)", "driving vehicle #4 to (4.50 m, 0.50 m)", "driving to (..)", "blocked on the way to (..)".
- **A blocked walk says how far it got and why** (proposal 001): `JournalKind::OrderFailed` gains the optional
  `nearest_reachable: [x, z]` (mm) and `blocker` (`npc_kernel::spatial::Blocker`: `{"kind":"Wall","cell":[x,z],
  "pos":[x,z]}`, `{"kind":"Door","id":12,"locked":true}`, `{"kind":"OutOfBounds"}`), filled by `npc_command` for
  `reason: "blocked"` from the new read-only `npc_nav::explain_blocked` (its own flood fill, no nav budget or queue
  state; computed only while the journal records). It floods with the planner's rules first (a door the agent may
  open is passable), so behind an unlocked outer door and a locked inner one it names the inner door and a point in
  the hallway; only when the goal is reachable under those rules does a strict pass (closed doors block) name the
  door the walk stalled at. Its work is capped at `npc_nav::EXPLAIN_CELL_BUDGET` (2^18) flooded cells
  (`explain_blocked_within` takes another budget), and at most `npc_command::BLOCKED_REPORTS_PER_TICK` (8) reports
  are built per tick (`BlockedReports`, not hashed; units in `SimId` order); past either cap a failure carries only
  its `reason`. Doors are named through the new `NavAppExt::register_cell_blocker`
  (`npc_interaction::door::door_blocker`). Event lines and the `changes` text read "Move failed: blocked by locked
  door #12; nearest reachable (4.50 m, 3.50 m)"; `changes.orders[]` carries the two fields. Both are left out of the
  JSON when `None`, so other events are unchanged; `JournalKind::order_failed(order, reason)` builds one without them.
- **Move orders speak metres** (proposal 003): `describe_order` labels, `order` results, the `command` section and
  the busy reason say "move to (0.00 m, -0.03 m)" (`observe::metres_pos`). A `pos` (or `points` entry) that is not
  integer millimetres is refused with "positions are integer millimetres: 6.5 m is 6500" (`api::POS_HINT`): the
  act's own `pos` and a step's `pos` as invalid params, one in `args` as `invalid_args`.

### Changed (notes as Obsidian files)

- **One markdown file per notes entry** (spec `2026-10-25-notes-as-obsidian-files-design.md`): `NotesStore` keeps
  the diary, memory and proposals as `diary/YYYY-MM-DD <title>.md`, `memory/<title>.md` and `proposals/<title>.md`
  in the agent folder, each with a flat YAML frontmatter (`id`, `type`, `title`, `aliases`, `tags: [npc/<type>]`,
  `agent`, `world`, `author` (last writer), `created`, `created_tick`, `updated`, `updated_tick`, `revision`;
  proposals add `status` and `prompted_by`), so the folder opens as an Obsidian vault. File names are made safe
  (`notes::safe_name`: `* " \ / < > : | ? # ^ [ ]` dropped, spaces collapsed, ends trimmed, Windows reserved names
  (`CON`, `NUL`, `COM0`-`COM9`, `COM¹`-`COM³`, `LPT` alike, `CONIN$`, `CONOUT$`) get `_`, 120 characters, a trailing
  `.md` of the title dropped, `Memory 12` when nothing is left), a taken name gets ` (2)`, ` (3)`; a title change
  renames the file (temp file, rename, then the old file removed) and adds the old title to `aliases`. Entries are
  found by `id`, so a file renamed or edited by hand (in Obsidian) is the same entry; subfolders and non-`.md` files
  are ignored; unknown properties and untouched lines survive an update byte for byte. A file with no or broken
  frontmatter, or a second file with the same `id` (the newest wins: `updated` when both are UTC timestamps, then
  `updated_tick`, `revision`, the modification time), is listed in the new `NotesStore::unreadable` / `notes::Unreadable` and never written; the notes `list` results (methods, MCP
  `notes_list`, `npcctl`) gain the additive `unreadable: [{file, reason}]`. A hand-made file with frontmatter but no
  `id` is adopted (the next id, written back once). New ids are above a high-water mark of every id seen in the
  folder (entries, earlier reads, the `id:` lines of unreadable files), so no id is handed out twice. The
  frontmatter reader/writer is hand-written (`notes::frontmatter`; no YAML crate). The API is unchanged: the `updated` stamp keeps its
  `{tick, date}` shape with `#rN` in `date`, rebuilt from `updated_tick`, `updated` and `revision`.
- **Old folders migrate once**: a `diary.md` / `memory.md` without a `.bak` becomes one file per entry (ids, titles,
  stamps and text kept; text before the first entry becomes an entry `Kept from diary.md`) and is renamed
  `diary.md.bak` / `memory.md.bak`; entries whose id already has a file are skipped, so a rerun is safe. A header-block
  proposal (no frontmatter, but a `title:`, `status:` or `written:` header line or an `NNN-` name; any other file
  without frontmatter is unreadable) is rewritten under its readable name (unknown header lines become properties) and the old
  file removed. Old free-form dates are kept verbatim, quoted.
- **Owners**: `notes::NotesOwner {agent, world}` fills the `agent` / `world` properties; `NotesStore::open_as(dir,
  owner)` and `NotesStores::set_owner(dir, owner)` (the testbed's `insert_notes` sets it from the agent's
  `agent.json` / session), so the generic store never reads content files.
- **Testbed**: new agent folders get empty `diary/`, `memory/` and `proposals/` (no `diary.md`); `AgentDir::diary()`
  is the folder, `AgentDir::memory()` is new. `AGENTS.md` and `prompts/*.md` start with a frontmatter (`type:
  agent-guide` / `prompt`, `agent`, `world`, `generated: true`; no date, so an unchanged folder regenerates byte for
  byte; `workspace::{generated_frontmatter, agents_md_file, prompt_file}`); `CLAUDE.md` stays exactly `@AGENTS.md`
  and the prompt text the modal shows and copies has no frontmatter. The F7 editor lists the unreadable files of the
  shown book with their reason.

### Changed (explorer memory, spec M1)

- **Explorer actors remember for a minute**: explorer worlds set `PerceptionConfig` to `forget_after_ticks = EXPLORER_FORGET_AFTER` (1200, 60 s) and `memory_capacity = EXPLORER_MEMORY_CAPACITY` (128) on every start path, old saves included (the host applies them after loading). Other scenarios, the village and the framework defaults are unchanged; the `perception_config` hash of explorer worlds changes (no replay or golden covers them).
### Changed (explorer prompts: an Epoch 1 mission)

- **The AI explorer's prompts are a game master's mission brief** (`content/testbed_content/prompts/`): Epoch 1,
  survive, learn, explore and map the world, find out how it works and keep what you learn in your notebooks.
  `AGENTS.md` shrinks from a manual to a character sheet: mission, body, how the agent reaches the world, its three
  notebooks (memory is its map and its manual), the rules, and `npcctl agent --help` for tools without MCP. It no
  longer teaches the one-call loop, tool fields, coordinates or the walls and openings lines; the MCP tools'
  descriptions are the agent's reference.

### Added (prepared agent folder, spec G5)

- **`testbed_content::explorer::workspace`**: `write_workspace(dir, kind, vars)` writes the agent folder's generated
  files atomically (`GENERATED`: `AGENTS.md` from the new `prompts/agents.md`, `CLAUDE.md` = `@AGENTS.md`,
  `prompts/<kind>.md`, `.claude/settings.json` (MCP-only: allow `mcp__npc__*` and `Read(./**)`, deny shell, edits
  and web, `dontAsk`, no reads outside the folder), `.claude/settings.local.json` (enables `npc`) and `.mcp.json`
  (`npcctl mcp --url URL --controller explorer`, the Windows path JSON-escaped)); it never touches the notes,
  `agent.json`, saves or `run.json`, and deletes the old `start_prompt.md` / `resume_prompt.md`. `WorkspaceVars`,
  `ActorSummary` (`human_scout()` until actor presets exist). `AGENTS.md` teaches the one-call loop (`act_and_wait`,
  `observe` with `since_seq`), the notes tools and, for tools without MCP, the `npcctl` lines. The start/resume
  prompts shrink to who you are, read `AGENTS.md`, (resume) list memory, diary and proposals with `notes_list`,
  observe; they no longer tell the agent to write files. `AgentDir::prompt(kind)` (`prompts/<kind>.md`) replaces
  `start_prompt()` / `resume_prompt()`; actor_testbed writes its prompt there (the whole workspace: Task B2).
  Template filling is now one pass, so a name that looks like a placeholder stays as typed.
- **The explorer writes the prepared folder on every start**: actor_testbed's explorer calls `write_workspace` (actor
  `ActorSummary::human_scout()`) on every New, Resume, Load anyway and Fresh world, so an older folder gets its
  `AGENTS.md`, `CLAUDE.md`, `prompts/`, `.claude/` settings and `.mcp.json` on its next resume and a changed port is
  rewritten everywhere; the notes, `agent.json` and the saves stay untouched. Its own `write_prompts` / `prompt_path`
  are gone; the modal's file is `prompts/<kind>.md`. A folder that cannot be written no longer stops the start: it
  is logged and named in the modal's notes (`ExplorerPrompt::write_error`). The menu's New screen names the folder
  ("After Start, open your AI tool in <agents folder>/<slug(agent)> and paste the prompt.", following the typed name;
  `launch::agent_folder_line`).

### Added (fewer round trips for agents, spec G13)

- **`agent/act_and_wait`** (BRP and `npcctl serve`): every `agent/act` field plus `until_idle` (default true),
  `kinds`, `timeout_ms` (default 30000, clamped to 120000) and `observe: {format, brief}`; acts at once, waits until
  the unit is idle (or, with `kinds`, the first such event; without `until_idle`, the first matching event) or the
  timeout, and returns `{act, events, observation}` (`events`: the `agent/events` result for the seqs after the act,
  `timed_out` at the timeout; `observation`: `agent/observe` with `changes` since the act). A rejected act answers at
  once. A parked poll in the session and a watching method on BRP, where it **needs a `nonce`** (invalid params
  without one): state is keyed by the params, nonce included, and every finished call (also a rejected act or
  `timeout_ms: 0`) keeps its answer as a tombstone for 60 s after its last run, since `bevy_remote` re-runs an answered
  watcher until the HTTP side drops it; re-runs get that answer and never act twice. It never blocks a frame. API:
  `api::act_and_wait_start`, `act_wait_ready`, `act_wait_result`, `ActWait`. Host-side: the same commands as
  `agent/act`, so the state hash is the same.
- **`agent/observe {since_seq}`**: adds `changes` (`sensed_new`, `lost`, `inventory_gained`, `inventory_lost`,
  `orders`, `interactions`, `actions`, `truncated` when the ring buffer already dropped events after `since_seq`),
  built from the agent event buffer only (`observe::changes`), and in text a first line "Since your last look: .."
  or "Nothing changed since your last look." (`observe::since_line`). A `since_seq` beyond the latest seq is invalid
  params (the world was rebuilt), as in `agent/events`.
- CLI: `npcctl agent act .. --wait [--kinds K,..] [--timeout-ms N]` (`--format text`: an act line, a wait line and
  the observation text; a rejection exits 1) and `npcctl agent observe .. --since-seq S`. MCP: tool `act_and_wait`
  (every `act` field plus the wait fields; text form like the CLI's, `observe.format` default text) and
  `observe.since_seq`. The HTTP client and server wait `act_and_wait`'s default/cap plus the usual grace. The CLI's
  `nonce` is now unique per call (process id plus wall clock), not per process.

### Added (agent notes)

- **`npc_agent_io::notes`**: a host-side `NotesStore` per agent folder with three books, `diary.md`, `memory.md` and
  `proposals/NNN-<slug>.md` (`list` / `add` / `update`; ids per book; title <= 200 chars, body <= 8192 bytes). Updates
  carry the `updated` stamp the writer read and are rejected as stale ("changed since you read it") when it moved
  (`updated` is a version: a same-tick write or a hand edit gets a `#rN` revision on the date);
  only `Writer::Player` sets a proposal's status. Old explorer folders (`## tick T - title` headings, proposals
  without `id:`) get ids on first read, written back once; text before the first heading, unknown header lines,
  BOM and CRLF are tolerated and untouched entries are written back byte for byte. Files changed on disk are re-read
  before each operation; writes are atomic. Not sim state: nothing is hashed or snapshotted.
- **Notes methods** (BRP and `npcctl serve`): `agent/notes/list|add|update` for the unit's controller (the `agent/act`
  check, now `api::check_controller`: `not_attached`, `not_controller`, `read_only`; `list` is a read and works for an
  Observer) and `notes/list|add|update` for
  the player (no controller; only they set a proposal's `status`). The host maps units to agent folders in
  `NotesDirs` and dates stamps with `NotesClock`; one `NotesStores` resource holds a store per folder. A stale update
  is the rejection `reason: "stale"` with `current`. CLI `npcctl agent notes ..` and `npcctl notes ..`; MCP tools
  `notes_list`, `notes_add`, `notes_update` (`npcctl mcp --controller`, default `explorer`), and the MCP schemas
  now list `act.steps`, `observe.brief` and `events.until_idle|min_events|dedupe`. `mcp::serve`, `handle` and
  `call_tool` take the default controller. MCP tool errors keep the method's `data.reason` (text and
  `structuredContent.error.reason`), and the `act` tool says how to walk to a thing and use it (`interact`).

- **F7 notes editor** (`testbed_view`, explorer mode): the F7 panel edits the explorer's notes through the same
  `NotesStores` as the notes methods, as `Writer::Player`: tabs Diary, Memory, Proposals (Tab cycles), a scrollable
  list newest first with ids, created/updated stamps, bodies and statuses; **Add** / **Edit** open a title and a
  multi-line body `TextInput`, **Save** passes the `updated` stamp read when editing began. A stale save writes
  nothing: the form keeps the typed text, shows the entry's current title and body above the fields ("Now in the
  file:") and says "#N was changed meanwhile - its current text is shown above; Save again to replace it with
  yours, or Cancel"; only that second Save may replace it (stale again if it changed once more). Status buttons
  per proposal ("changed meanwhile - reloaded" when stale); the player's own status click on the proposal open in
  the form keeps the form current. Re-read every 1 s.
  While a panel field has focus `ViewSystems::Input` is gated (`editor_typing`), Tab moves between the fields, and
  copy/cut/paste use the OS clipboard; clicks and the wheel over the panel do not reach the world. `actor_testbed`'s
  explorer maps its actor to the agent folder (`NotesDirs`) and dates notes with the wall clock (`NotesClock`).
  `testbed_content::explorer::notes` (the old diary/proposal parsers) is gone: `read_lossy` moved to `home`, and
  `clipboard::paste_text` is shared by the menu and the editor.

### Fixed (profiler numbers and sim tick cost; `docs/_notes/explorer-perf-budgets.md`)

- **`NpcSimTick` runs on Bevy's `SingleThreadedExecutor`** (set by the kernel): with `bevy_ecs/multi_threaded` (every
  windowed game) the default executor cost ~0.45 ms per tick more than the work of its small, mostly exclusive
  systems and made each one measure ~3x slower. Explorer tick 0.59 → 0.12 ms, village_n 500 0.87 → 0.45 ms
  (release, profiler on). System order is unchanged (ambiguities are errors): no state hash changes.
- **Per-tick `world.query()` rebuilt a `QueryState` in 14 exclusive systems** (perception, think, nav plan, command
  engine, the four vehicle systems, interaction completions and doors, crafting completions, the `ExternalAction`
  passes): ~1.5 µs each, plus one `par_for_each` tracing span per build with `bevy/trace`. They take cached
  `&mut QueryState` / `Local` params now. Perception gathers the perceivables only on a tick where an agent scans
  or a noise was heard.
- **`npc_perf`**: a `mechanic` span inside a timed span of the same mechanic is no longer counted twice (vehicles,
  perception, interaction, crafting and progression were doubled). New `BreachLog` (`Warn`, `WarnOnce`, `Debug`) and
  `set_breach_interval`; `NpcPerfPlugin` defaults to `WarnOnce` and `perf_log_filter` adds `npc_perf=info`, so
  normal play logs one line per slow mechanic instead of one every 5 s (`actor_testbed --perf-warn` restores it).
  Bevy's `Main::run_main` is tagged `frame` (inclusive, no budget) instead of `engine`.
- **`actor_testbed` profiled none of its sim systems**: they were added before `LogPlugin` set the subscriber, so
  their spans stayed disabled and F4 showed only the hand-written `mechanic` spans. New
  `perf::install_log_plugin_now()` installs the subscriber first (`DefaultPlugins.disable::<LogPlugin>()`).
- **The testbed's sim systems were `untagged`**: new `testbed_content::perf_tags` (`walk_cancel_system`,
  `walk_arrival_system` → `actions.move`; `idle_system` → `actions.idle`, like the village's), added by
  `actor_testbed` and `npcctl bench`; `TestbedPlugin` registers the `actions.idle` budget (10 + 2, the village's row:
  `TESTBED_BUDGETS`). No sim change.
- **World snapshots lost `GridBlocker` / `GridOccluder` without spatial hashing**: they were saved only when a
  capability called `register_spatial_hash`, so a host with `SpatialPlugin` alone lost them on load and the
  post-load re-sync cleared their cells. `SpatialPlugin` now always saves them (`grid_blocker`, `grid_occluder`).
  No content-hash or replay change (ADR 0013 addendum).
- **A driving unit counted as idle for agents**: nothing registered a vehicles busy predicate, so `agent/events
  {until_idle}` and `agent/act_and_wait` answered while an `Advisor` driver's vehicle (the `DriveTo` command) or a
  `Direct` vehicle was still on its way. New busy predicate `npc_vehicles` (`glue::vehicles_agent::vehicles_busy`):
  busy while `DriveVehicle` runs, or while a `Direct` vehicle's own intent is `Moving` / `Blocked`. Agent tooling
  only: no sim or hash change.
- **The agent HTTP server could lose its refusals** (`npc_agent_io::http`, the flaky `caps_concurrent_connections`):
  a refused connection was closed without reading the client's request, and on Windows a close with unread input,
  or a request arriving after the close, resets the connection, which can destroy the `503`/`4xx` before the client
  reads it (16 of 330 loaded runs got status 0 and no body; 0 of 325 after the fix). A refusal now reads the
  connection until the client closes it (at most 1 s, `REFUSE_LINGER`), and the `503`s are answered by their own
  thread (a queue of 64, dropped beyond) so the listener never waits on a client. The test also counted 200 retries
  (~4 s) for a free slot; it now watches the server's slot count with a 60 s clock deadline and a 1 h request
  deadline, so a freed slot can only come from a close (slots return 6–470 ms after close under load). Private test
  seam `HttpServer::bind_counted`; no public API change.
- **Docs**: per-component and per-system reference pages (`docs/entities/components/`, `docs/entities/systems/`);
  `disable::<StraightLineMover>()` is shown on `NpcPlugins::new().build()`; README F7 (notes editor) and profiler
  log-plugin text updated.
- **Schedule warning**: `flush_per_roll_log` was put in `NpcSet::Sense` and `NpcPerceptionSet` (already in
  `Sense`); the redundant edge warning at start-up is gone.
- **Budgets** (`NPC_BUDGET_TABLE`, fixed µs + µs per agent): `vehicles` 10 → 30 + 2, `perception` 20 → 50 + 5, `nav`
  20 → 30 + 2, `interaction` 15 → 25 + 2, from the windowed explorer p95 (~1.5× headroom); `bench/baseline.json`
  unchanged.

### Changed (explorer and snapshot polish)

- **`testbed_content::explorer::clipboard`**: one std-only module for the OS clipboard (`copy`, `paste`), with a
  5 s timeout per tool (a hung tool is killed) and `wl-paste --type text/plain`; the modal and the menu switch to it
  next. `home::name_problem` explains an unusable agent name (`create_agent` refuses one), `backup_save` never
  overwrites a backup (`-2`, `-3`, ..). Prompt: the "npcctl not built" note has its own line, the events step keeps
  `next_seq`, `--kinds` lists the craft events, and every path uses forward slashes.
- **Snapshots**: `write_atomic` syncs the temp file before the rename, `snapshot_check` prints the entity diff for a
  saved-vs-never-saved divergence too, a test pins the load order the `brain` and `spatial_grid` loaders rely on, and
  a test pins that `Reservations` round-trips hash-equal. No state hash changes.
- **In-game explorer**: the prompt modal copies through the shared `clipboard::copy` (its own clipboard code is
  gone); a copy that has not reported after 7 s, or whose thread panicked, shows the failure note and Copy works
  again; a "scroll for more - Copy takes all of it" hint shows while the prompt is taller than its scroll area.
  The F7 panel reads the files in the frame it opens, and a file it saw but could not read (a sharing violation
  while the agent writes) keeps the old text and is retried at the next poll instead of being cached
  (`notes::try_read_lossy`, `try_read_proposals`). `actor_testbed` explorer mode: `pick_port` needs `p + 1` free
  too (the render server), `--remote-port N` is the first port tried (`N..=N+19`), a port taken before BRP could
  bind it exits 2 ("busy: try again") instead of advertising a dead URL, `--world` with `--resume` is a usage
  error, and a failed tick-0 save removes the new agent folder so the name can be used again.
- **Game menu**: Ctrl+C / Ctrl+X / Ctrl+V in a text field use the shared `clipboard::copy` / `paste` off the
  frame (its own clipboard code is gone; Cut still removes the selection). A game that exits non-zero shows
  "<game> exited with code N: <its last stderr line>" (stderr is piped through a thread that still copies every
  line to the menu's terminal). `--check-save` exiting neither 0 nor 1 reports "the save check failed (exit N):
  <line>" on the Resume screen instead of the mismatch dialog; a check is killed after 30 s ("the save check
  timed out") or when the Checking screen is left, and an in-flight check blocks a second launch like a running
  game. The Building screen has **Cancel** (kills cargo, back to Missing); closing the menu kills a build too.
  Settings' Save creates no folder when there is no settings file to write; New checks both names with
  `home::name_problem` (inline reason). The menu renders reactively (low-power winit modes: a frame on window
  events, 2 per second while a text field has the focus for the caret, polling only while a child, a clipboard
  tool or `--screenshot` needs it) instead of continuously. **Build** falls back to the workspace the menu was
  compiled in when none is above its folder (`CARGO_TARGET_DIR` outside the workspace).

### Added (explorer senses walls)

- **`walls` observation section** (`testbed_content::walls`, registered by `TestbedAgentPlugin`): the wall runs
  (`Prop { Wall }` footprints and the `SpatialGrid` static layer's blocking cells) and openings (a gap of at most
  4 cells between two runs on one line; a `Prop { Doorway }` in it makes it a `door` with its state and testbed
  name, e.g. `gate`) within the unit's vision range, sorted by distance, with compass directions; no field of
  view or line of sight. Text: "Walls within 15.00 m: .." and "Openings within 15.00 m: the gate (closed door
  #10) .., 3.04 m south". Integer geometry, read-only (no state or content hash change). The explorer prompt
  (`prompts/common.md`) now says walls and openings are sensed (proposal 003, finding P3 of the explorer E2E run).

### Added (game menu and AI explorer)

- **`apps/game_menu`**: `cargo run -p game_menu` opens a menu (Explore with an AI agent, Actor Testbed with one
  button per embedded scenario, Village, RTS units demo, Settings, Quit) that starts the chosen game as a child
  process with its flags, hides its window while the game runs (polling `try_wait`, low-power updates) and comes
  back when it exits, showing a non-zero exit code on the main screen. A game not built next to the menu shows its
  `cargo build -p <name>` command and a **Build** button (when `cargo` is on `PATH` and the workspace is found)
  with a "building..." state; Launch and Build ignore a second activation while a child runs. Settings edits the
  agents folder in a Bevy `TextInput` (validated inline, created on Save, saved to the per-user settings file or
  `--settings FILE`); the explorer entry is disabled until the folder is valid. Ctrl+V / Cmd+V in every menu text
  field pastes from the OS clipboard (`powershell Get-Clipboard`, `pbpaste`, `wl-paste`/`xclip`/`xsel` on a
  thread, first line for single-line fields) in place of Bevy's in-app paste. `--screen NAME --screenshot PATH
  --after-secs S` and `--resume AGENT` (presses Resume at start) for visual checks.
  `testbed_content::scenario::embedded_scenarios()` lists every embedded scenario with a one-line description.
- **Explorer New / Resume screens**: New has the agent and world names prefilled with `random_name` (wall-clock
  seed), a **Random** button per field, and **Start** (or Enter in a field), disabled with an inline reason while
  a name is blank or the agent's folder exists ("an agent with this name exists: resume it instead"). Resume lists
  the agents folder's agents with their world and last save date (from the save header), "no save yet" ones
  disabled; it runs `actor_testbed --check-save` off the frame ("Checking ..." screen) and starts the game when it
  fits, else shows "This world was saved by another version of the game: <error>" with **Load anyway** /
  **Fresh world, keep the diary** (an unreadable save: the error and Fresh world only).
- **Explorer mode in `actor_testbed`**: `--explorer-home DIR --agent NAME [--world NAME] [--resume
  [--load-anyway | --fresh-world]] [--check-save]`. New creates the agent folder and an `explorer` world (actor
  named after the agent, wall-clock seed, saved at tick 0); Resume rebuilds from the save header's scenario and
  seed and loads the snapshot before the first frame; a save from another build exits 3 without a choice;
  `--check-save` prints `{"ok":..}` JSON and exits 0/1 without a window. The sim starts paused. Explorer mode
  picks the first free BRP port in 15702..=15721 (exit 2 when none, or without `debug-tools`), writes
  `start_prompt.md` / `resume_prompt.md` into the agent folder, exposes the prompt in an `ExplorerPrompt`
  resource, and saves every 60 s (wall clock, after the driver set) and on exit, also when the window is closed
  (the final save runs in `OnAppExitSystems`, after the window's exit); `--record` / `--commands` are refused
  with `--resume`.
- **Explorer prompt modal**: `testbed_view::{ExplorerModalPlugin, PromptModal, PromptModalClosed}` show a centred
  panel (max 900 px, scrollable monospace 13 px prompt, notes) with **Copy** (pipes the text into the OS copy tool
  on a background thread: `clip.exe` fed UTF-16LE+BOM, `pbcopy`, or `wl-copy`/`xclip`/`xsel`; "Copied" for 2 s
  only when the tool exits 0, else a note naming the prompt file) and **OK**; the game's input
  (`ViewSystems::Input`: command UI, order/camera keys, speed keys) waits while it is open. Explorer mode shows the
  start/resume prompt there and unpauses the sim on OK. No new crates and no Bevy `system_clipboard` feature (its
  `clipboard-win` dependency is BSL-1.0, which `deny.toml` does not allow).
- **F7 explorer panel** (`testbed_view`, `ExplorerFiles`, `--panels explorer`; explorer mode only): the agent's
  proposals first (`[status] title`, the 12 newest, then "(+N more)"), then its 5 newest diary entries, each cut
  to 280 characters; re-read within ~2 s of a change on disk; the box is capped at 85% of the window height.
- **Explorer prompt templates**: `testbed_content::explorer::prompt::{PromptKind, PromptVars, render_prompt}`
  render the paste-into-an-AI prompt (start and three resume variants) from `content/testbed_content/prompts/*.md`.
  The common part says what a paused world looks like (wait rather than repeat actions), that walls are not sensed
  and a far Move paths through doors and gates, and how to recover a lost controller; the plain resume says the
  save may be up to a minute older than the diary, so trust the senses. Examples use `VERB@ID`, not real ids.
- **Explorer agent folders**: `testbed_content::explorer::{names, home, notes, settings}` give random agent/world
  names, per-agent folders (`create_agent`, `list_agents`, `slug`), diary/proposal parsing and the agents-folder
  `Settings` (std + serde only). A leading BOM in `agent.json`, `settings.json`, the diary or a proposal is
  tolerated; the Windows read-only attribute on a folder (not enforced there) does not reject it.
- **`testbed:explorer`**: a new embedded scenario on the `frontier` layout (the compound plus groves, ore, a ruin
  and scattered items outside, on a 128-cell grid) with one `scout` under a `Direct` `explorer` controller.
  Existing scenarios are unchanged (64-cell grid, same spawn order).
- **Docs**: README quick start starts with `cargo run -p game_menu` and shows the AI explorer
  (`docs/img/explorer.jpg`); `docs/agent-experiments.md` §9 "The AI explorer" (the flow, the folder layout, the
  prompt variants, resume, **Load anyway** / **Fresh world, keep the diary**); the explorer support code in
  `content/testbed_content/README.md`; `CLAUDE.md` crate map (`apps/game_menu`, snapshots in `npc_kernel`).
  An end-to-end run with its findings: `docs/_notes/explorer-e2e.md`.

### Added (world snapshots, ADR 0013)

- **World snapshots** (`npc_kernel::snapshot`): `SnapshotRegistry` (save/load per component and
  resource by stable name, rebuild functions), `SnapshotAppExt`, `save(world) -> WorldSnapshot` and
  `load(world, &snap, LoadMode::{Strict, Lenient}) -> LoadReport` (version and content-hash checks, skipped
  and defaulted names). `register_sim_component[_with]` also registers the component (bound `+ DeserializeOwned`,
  `_with` also `+ Serialize`); `SimHashResource::snapshot_fns` opts a resource in; `SimClock` and
  `SimIdAllocator` derive serde and are saved as `sim_clock` / `sim_id_allocator`. `Journal::clear`. Snapshot
  entries are not in the content hash: state hashes, content hashes and replays are unchanged.
- **Every tick-deciding piece of state is in world snapshots**: the command queue,
  `Brain` (by tree name), `BrainState`/`ActiveLeaf`/`Blackboard`/`BrainStats`/`ThinkRequest`, the spatial grid
  (static and dynamic layers, unreported changes; index rebuilt, one re-sync on the next update), the nav queue
  (requests, paused A* search and its scratch stamps), pending noises, `Provides`/`Approach`/`Reservations`,
  `PauseDecay`, `MoveIntent`/`MoveStatus`/`ExternalMover` (saved even when not hashed), `HostPosition`, agent
  controllers/queues and the agent id counters, the village places/villager/`VillageIds` and `TestbedIds`;
  `DoorCells` is rebuilt from the saved synced cells (doors removed after the saved sync are cleared next tick),
  and `key_replan_pending` carries `Inventory`/`Equipment` changes made after `key_replan_system` ran. `load`
  settles change detection (loaded components look unchanged to every system; rebuild functions run after it),
  never re-hands a loaded `SimId`, and range-checks ids before changing anything.
  `npc_tools::snapshot_check::assert_resume_equivalent` proves save → load → run equals an uninterrupted run
  (three runs: never saved, saved, loaded); the testbed, every embedded scenario, mid-walk/craft/key/mounted
  saves and the village pass. **Changed:** snapshot entries take explicit names
  (`register_snapshot_component_named`, `register_snapshot_resource_named`, new `register_snapshot_component_with`;
  the type-name variants are gone); re-registering the same type under the same name is a no-op. State hashes,
  content hashes, replays and goldens are unchanged.
- **Snapshot files**: `npc_kernel::snapshot::{write_atomic, read, check}` (re-exported as
  `npc_framework::snapshot`; `check` makes `load`'s version and content-hash checks without loading) and
  `npcctl snapshot save | check` (`docs/debugging.md` 2.6b).

### Changed

- **Testbed `doors` and `agent` scenarios**: the scout `eye` no longer has `fixed_facing`, so its vision cone
  follows its movement like its model does (it started facing +Z and stayed there). No replay, golden or
  snapshot uses either scenario; `perception` keeps its fixed facings.

### Added (FX4: facade/API gaps from the fresh-agent extensions, `docs/_notes/fresh-agent-{a,b}-friction.md`)

All additive: apps that use none of it keep their state hash, content hash, journal and replays byte for byte.

- **Custom orders**: `UnitOrder::Custom { name, args: JsonObject }` (`UnitOrder::custom_with(name, &args)`,
  `order.custom()`, `order.journal_name()`), run by `CommandAppExt::register_custom_order(name, OrderExecutor)`
  (content hash `order_custom`); the journal's `order` field is the name. **Per-verb `Interact` executors**:
  `register_interact_verb_executor(verb, name, exec)` (content hash `order_verb`), consulted before the
  `"Interact"` kind executor, so game verbs coexist with the interaction glue (`OrderExecutors::for_order`).
- **`IssueOrder::skip_invalid`** (`.skip_invalid(true)`, JSON `"skip_invalid":true`, not serialized when false):
  units that fail a per-unit check are skipped and journaled `OrderFailed { reason: "skipped: .." }`;
  `IssueOrder::partition`, `submit_order_report` → `IssueReceipt { tick, seq, units, skipped }`, `issue_to`.
- **Approach engine** `npc_command::approach`: `ApproachSpec { reach: Reach::{Point, Area}, goal, tolerance_mm }`
  (`to_point`, `to_footprint`), `approach_step` → `InReach`/`Walking`/`Blocked`, `approach_then`. The command ×
  interaction glue's walk into reach now runs on it (same behaviour: testbed replays unchanged).
- **Footprint queries** (`npc_kernel::spatial`): `SpatialGrid::footprint_area`, `footprint_cell_rect`,
  `cells_around(pos, footprint, ring)`, `nearest_free_cell_around`, `nearest_blocker_by_edge(world, from, max_mm,
  filter)` → `BlockerHit`, `area_dist2`.
- **`command_ui`**: `CommandUiAppExt` (`add_right_click_order`, `add_context_order` and `_subset` variants; any
  plugin order; the interaction glue uses it), subset resolvers returning `ClickOrder { units, order }` (old
  resolver signatures unchanged), `right_click_order_for`, `context_order_for`, `issue_to_units`; right-click
  orders go out with `skip_invalid` (`SkipInvalidOrders`, default on); `PickTarget` marks visuals that right
  clicks pick but selection does not.
- **Journal**: `record_effect(world, unit, kind, &payload)` and `JournalKind::effect(kind, &payload)`; the prelude
  now exports `JournalKind`, `JsonObject`, `set_journal`, `record_effect`, the approach and receipt types, the
  footprint queries, `free_for`/`free_slots`/`try_reserve_slot` and (with `perf`) `PerfAppExt`. `CommandConfig`
  fields are documented.
- **`npc_jobs`**: multi-slot reservations `free_slots` (also `Reservations::free_slots`), `try_reserve_slot`,
  `Reservations::slot_of`; the village's `available_food_for` uses `free_slots`.
- **Perf budgets from content**: `PerfAppExt::register_budget(mechanic, fixed_us, per_agent_us)` into the app's
  `PerfBudgetTable`, `NpcPerfPlugin::budget(..)` / `budgets_with`, `PerfBudgets::default_npc_with`,
  `add_rows`, `default_npc_for_world` (used by `npcctl bench`). `actions.eat/work/sleep/idle` moved out of
  `NPC_BUDGET_TABLE` into `village_content::VILLAGE_BUDGETS` (same effective budgets for the village).
- **`npcctl run --all-needs`**: adds `needs_all` (every registered need by key, every entity with `Needs`, any
  content); the default summary is unchanged.

### Changed (FX4: docs, skills and test hygiene from the same friction logs; no oracle changed)

- **One re-baseline procedure**: `docs/rebaselining.md` (stage → evidence → user approval → swap, with exact
  commands, the list of checks that fail on a re-baseline, the evidence note template and the swap commands);
  `scripts/rebaseline/stage_replays.sh` re-records corpus files into `tests/replays.next/`. CLAUDE.md,
  `debugging.md` §8.2, `.claude/rules/{content,sim}.md` and the skills point at it.
- **SimId stability** documented (`.claude/rules/content.md`, `/add-mechanic`, `rebaselining.md` §5); tests look
  ids up (`VillageIds`, `TestbedIds`, `next_sim_id`, the agent scenario loaded in process) instead of hard-coding
  them; `corpus_equivalence.rs` (village, testbed) accepts a documented id remap
  `tests/replays.next/<name>.idmap.json`.
- `golden_inspect_unit5` split into one test per snapshot (`_t600`, `_t1800`, `_t4000`; snapshot files
  unchanged), so one failure no longer hides the others.
- `npcctl` tests kill spawned `serve`/`mcp` processes on drop (no leaked server holding the exe).
- `/add-mechanic` and `/add-unit-type` rewritten (what "content only" touches, perf tag + budget, need-id order,
  `SetNeed` timing, multi-slot reservations, tree branch vs order executor, the executor lifecycle, the RTS
  order → autonomous → idle pattern, `.chain()` for action systems); a small-scenario harness
  `apps/rts_units_demo/tests/small_scenario.rs`; `using-npc-framework.md` §4.1/§4.3 (need-id order, `SetNeed`
  timing, what shares `NpcSet::Actions`/`Locomotion`); gate timings in CLAUDE.md.

### Fixed (FX1: final simulation review, `docs/_notes/final-review-sim.md`)

- **Nav queue starvation**: a paused head search restarts on `GridChanged` only when a changed cell is one it
  opened or is next to one it expanded (`Search::touches`); one request's work over all restarts is capped at
  `SEARCH_LIMIT_BUDGETS` (8) × `NavBudget` (at least the grid's cell count, so an uninterrupted search is never
  capped), past which it fails with the new `PathFailReason::SearchLimit` and the requests behind it are served.
- **Nav agents with their own `GridBlocker`** (e.g. `Direct` vehicles) plan and move: their own footprint is open
  to their search, smoothing and mover (`SpatialGrid::cell_without_one_blocker`), and their own cell changes do
  not make them replan.
- **Interaction leave hook** (`npc_interaction.items`): a holder that leaves the sim drops its items where it stood
  (`SimId` order, journaled `ItemMoved`, back in the grid); a carried item that leaves is removed from its holder's
  inventory/equipment. `OnLeaveFn` now also gets the leaving `Entity` (components still readable).
- **`close`** checks the doorway against live `SimPos`, not the start-of-tick grid index.
- **Crafting** completes with the claimed items: `Crafting { tools, inputs: (id, units) }`, checked by
  `check_claimed` (a shrunk claimed input makes the craft `invalid`), instead of re-picking inputs and tools.
- **Commands inside a tick**: `submit` inside `NpcSimTick` (after the tick's commands applied) is `Unavailable`
  without `at_tick`; `submit_deferred` queues for the next tick; the running tick is `TickInPast`
  (`CommandQueue::applied_through`). `CommandLog` keeps each command's own `(tick, seq)`: no key collisions.
- **Formations**: spacing is validated (`1..=POS_LIMIT`) for `IssueOrder` and `CreateSquad`; world slots are
  clamped to ±`POS_LIMIT`. A squad that leaves the sim no longer leaves `MemberOf(dead)` on its members.
- **`per_roll` dice key** (`detect_key`): unique per observer and tick (noises carry `Detection::noise`
  ordinals; targets 2^30 apart and sourceless noises no longer share dice); unambiguous cases keep their old dice.
- **Hashing**: pending `Messages<Noise>` (`pending_noises`, new name: changes the content hash of perception
  apps), the paused nav search's whole progress (`nav_queue`; unchanged encoding while no search is paused),
  crafting's claimed tools/inputs; `Brain.tree` is hashed only with `npc_kernel::hash::register_brain_hash` (opt-in:
  by default it would change every village hash).
- **Limits**: `PostCraftOrder` at most `MAX_CRAFT_ORDERS` (64) open orders per workstation; footprints are
  clamped to the grid's size (`footprint_cells`, `footprint_indices`).

### Security (FX2: tooling, transport and guardrails after the final review)

- **Session server** (`npc_agent_io::http`, `npcctl serve`): refuses requests with an `Origin` header (403), a
  `Host` other than `127.0.0.1|localhost|[::1]:PORT` (421) or a non-JSON `Content-Type` (415); optional bearer
  token (`serve --token`, clients and `mcp --token`, 401); limits: 64 KiB headers, a 10 s request deadline, 64
  connections, 1 MiB bodies, 64 calls per batch, an empty batch is a `-32600` error. `world/load_scenario` takes
  the scenario inline only (no `path`). Transport errors name the URL.
- **Client file paths** (`npc_tools::out_path`, one checked function): `npc/record/stop {path}` and
  `npc/screenshot {path}` are relative, confined under the output root (`--out-dir`, default
  `target/npc-out`), extension forced, no overwrite unless `overwrite: true`.
- **BRP off by default in release builds** of `village` / `actor_testbed` (`--remote` turns it on); the shared
  remote setup moved to `npc_view_helpers::remote` (`RemoteArgs`, `install_remote`, `npc/screenshot`,
  `record_from_startup`). `bevy_remote` 0.20 cannot check `Origin`/`Host`: documented residual risk
  (`docs/debugging.md` §3.2).
- **Hook** `protect_goldens.sh`: trailing dots/spaces, `\\?\` prefixes, NTFS streams and 8.3 names folded;
  `git checkout|restore|reset|stash|apply ..` (and patch contents), PowerShell `ri|cpi|mi|rni`, `[IO.File]::`,
  `python -c` writes, `cd` + relative paths and deleting a directory that holds oracles are blocked; copying *from*
  an oracle is allowed; `xtask/src/main.rs` and the CI workflow are protected. Docs: launch Claude Code in
  `npc-framework/` (CLAUDE.md, README, debugging.md §8.2).

### Changed (FX2)

- Session host `npc/step` runs long steps in 50-tick slices, serving requests and long-polls in between.
- MCP `events` sends a per-call `nonce`; BRP watching requests with identical params get separate state.
- CI: `verify` is the only fmt/clippy/test/deny job; clippy on feature combinations, an MSRV (1.97.1) job.
- `cargo xtask verify` (full) fails on an empty replay corpus; stale "Task P2" texts fixed.
- npcctl's empty `debug-tools` feature removed; scenario lookup is file-first in `npcctl` and `actor_testbed`;
  `npc_agent_io::scenario::attach_controllers` is public and used by the testbed app and content.
- Removed `build.log` and the `docs/_notes/{npcctl,rpc}-reference.md` pointer stubs (their content is in
  `docs/debugging.md`); `scripts/p3/capture.sh` handles testbed replays and session records.

### Changed (FX3: agent interface after the first fresh-agent play-test)

- **Perception gate for affordances**: with a perception provider, `agent/observe` offers only affordances whose
  target the unit perceives, remembers, carries or rides (central filter in `npc_agent_io`; new registries
  `register_known` / `register_possessions` / `register_describer` / `register_unit_info` / `register_busy`), with
  no hidden radius for known targets (`why_not: "too far: 12.0 m .."`). Text: item kinds and positions for
  percepts, no "remembered" own items, a night line, tool targets' yield and uses, skill-check odds
  (`check`, `[Mining 13 vs 3d6: ~83%]`), the auto-spend vs bank explained; recent events drop perception rolls
  and keep the agent's own outcomes; `observe {brief}` hides structurally impossible affordances.
- **Acts**: atomic multi-step `agent/act {steps}` (all or nothing; plans' commands are validated before any is
  submitted), the `interact` verb (walk into reach and act, one act), wrongly typed args are `invalid_args`,
  staleness counts only relevant events (listed in the rejection, `interrupting`) and `stale_ticks` defaults to
  600, acts skipped at apply time are journaled (`ExternalAction {status:"rejected"}` + `ActionRejected`) and the
  queue limit counts same-tick acts; `execute_orders` ends `done` when the order queue runs dry.
- **Events**: `epoch` on every response, `since_seq` beyond the latest seq is an error, `oldest_seq` / `dropped`,
  `until_idle`, `min_events`, `dedupe`, unknown kinds rejected with the valid list, binary-searched scans;
  `list_units` rows carry the `owner` player. Journal JSON: `Roll.target` is `target_number` (`target` still read).
  CLI: `observe --brief`, `act --interact VERB@ID | --steps JSON` (a note when `--queue` is used with `order`),
  `events --until-idle --min-events --dedupe`.

### Added (RTS demo: engineers repair walls)

- `apps/rts_units_demo`: walls carry a hashed `WallHealth` (three start damaged); idle engineers repair the nearest
  damaged wall within 12 m (guard `rts_damaged_wall_near` → action `RtsRepair`: walk to a free cell next to it,
  then +4 hp/tick, journal `Effect {type: "WallRepaired"}`); the order `Interact { verb: "repair", target }`
  (executor `rts_units_demo.repair` for kind `Interact`) and R + right click (`RightClickKind::Custom
  ("rts.repair")`) send engineers anywhere; damaged walls are tinted; the headless summary has `damaged_walls`.
  Content only: no facade change. Tests: `apps/rts_units_demo/tests/repair.rs`.

### Added (village: thirst and the well; behaviour change, re-baseline staged)

- **Thirst + well** (content only, `village_content` / `village_view`): need `Thirst` (decays 45/tick awake,
  paused asleep like energy), a `Well` smart object per plot (`PlaceKind::Well`, `Provides Thirst` 2500/tick,
  `Approach`, 2 drinking slots via `Reservations` and `jobs::free_slots`; SimId 6 in `village`, spawned after
  the plot so the older SimIds stay, so runtime spawns start at 7), actions `ReserveWell` / `DrinkWater` (`Drink` hashed component, `Activity::Drink`, slot
  released in the tick the drink completes), conditions `thirst_urgent_well` / `thirsty_well`, guards
  `UrgentDrink` (below `UrgentEat`, abort lower) and `CasualDrink` (below `NightSleep`), the agent verb `drink`,
  perf mechanic `actions.drink` (budget row in `VILLAGE_BUDGETS`, like eat's), a well model and a thirst bar
  in the F1 overlay; `npcctl run --all-needs` reports `thirst`. Test: `content/village_content/tests/thirst.rs`.
  New names change the content hash: the village replays and goldens are re-staged (`tests/replays.next/`
  with `village_spawn_despawn.idmap.json` (6 → 7), `.snap.new`), evidence in
  `docs/_notes/thirst-merge-rebaseline-evidence.md` (the pre-merge branch note: `thirst-well-evidence.md`).

### Changed (wave H2: portability and API gaps from the RTS demo)

- **`command_ui` right-click order kind** (additive): `RightClickMode { kind, once }` (host-set from its own
  hotkeys; `held` / `once`), `RightClickKind::{Move (default: the context order), AttackMove, Patrol,
  Custom(name)}`, custom kinds by name in `RightClickOrders` (`RightClickOrderFn(&World, &RightClick)`),
  `PatrolDraft` (Shift+right click collects patrol points, a plain click issues `Patrol`, leaving the mode
  issues the draft; drawn as gizmos), the pure `right_click_order` / `patrol_click`, and the system set
  `NpcCommandUiSet` (hosts setting the mode run `.before` it). `apps/rts_units_demo` no longer unbinds the
  plugin's right click: A / P set the mode, S / H use `issue_to_selection`.
- **Public picking and selection**: `npc_view_helpers::{pick, PICK_HEIGHT, PICK_RADIUS}`,
  `command_ui::{pick_unit, select_units, select_in_rect, unit_screen_positions, issue_to_selection}` and the
  public `DragBox` (the drag in progress or a scripted marquee, drawn by the plugin). The RTS demo's
  screenshot mode uses them instead of its own picker and marquee; the testbed's `--select` uses
  `select_units`.
- **Profiler setup in two calls**: `npc_framework::perf` is now a module (`npc_perf::*` re-exported as
  before) adding, with `view-helpers`, `log_plugin()` / `perf_layer` / `perf_log_filter()` and
  `NpcPerfPlugin` (`tag`, `tags_with`, `budgets`, `end_frame`; default tags, budgets scaled for the build
  profile, `PerfHandle`, `end_frame_system` in `Last`), implemented in `npc_view_helpers::perf_setup`.
  `apps/village`, `apps/actor_testbed` and `apps/rts_units_demo` drop their ~20-line `LogPlugin
  { custom_layer }` modules.
- **Perception scan** (same results, cheaper): candidates outside every vision cone and outside
  proximity are dropped before the nearest-first sort, line of sight is traced only for visited cone
  candidates, the scan returns a sorted `Vec`, agents no longer clone `Sensors` every tick, hearing finds
  listeners by binary search. 200-agent dense crowd (`npc_perception` `tests/perf.rs`, release, same
  machine): stagger 1 ~2.50 → ~1.87 ms/tick, stagger 4 ~0.72-0.79 → ~0.56-0.59 ms/tick, identical
  percept counts; the `("perception", 20, 5)` budget (1.02 ms at 200 agents) is unchanged.
- **Key after a failed path**: `npc_nav::retry_failed_path(world, agent)` re-queues one agent's failed
  request; `npc_interaction`'s `door::key_replan_system` (in `NpcInteractionSet::Complete`, after the
  completions) calls it for a `NavAgent` + `Interactor` whose inventory/equipment changed and who now
  carries the key of a locked door (a per-agent pass cost, so no `GridChanged` came). Tests
  `failed_path_replans_when_the_key_is_picked_up`, `retry_failed_path_requeues_one_failed_request`.
- **Prelude** (additive): `state_hash`, `content_hash`, and with `command` `create_squad` (the RTS demo no
  longer reaches into `kernel::hash` / `kernel::registry` / `command`).
- `docs/adding-a-host-app.md`: the `deny.toml` `bevy`-wrappers rule for new host apps (a protected file:
  ask the user) and an open proposal. No oracle changed: village and testbed replays, goldens and the RTS
  demo's headless summary (seed 1 / 200 units: state `2091ecc0bdb3c7b6`, all arrived at 1043) are byte-identical.

### Added (wave H1: CI, documentation, agent skills)

- **CI** (`.github/workflows/npc-framework.yml` at the repository root; `working-directory: npc-framework`, runs on
  changes under `npc-framework/**`): `lint` (fmt + clippy `-D warnings`), `test` (nextest + doc tests), `verify`
  (`cargo xtask verify` on ubuntu-latest and windows-latest, with cargo-nextest and cargo-deny installed), `deny`
  (cargo-deny-action), `features` (`cargo hack check --each-feature -p npc_framework --no-dev-deps`, plus the 23
  capability-glue and view-helper feature pairs alone; a full powerset would be 2^15 builds), `determinism` (on
  both OSes: `npcctl replay verify` over `tests/replays/*.json`, `npcctl run --seed 1 --ticks 7200`,
  `--scenario testbed --ticks 4800`, `rts_units_demo --headless-ticks 1500`; hashes uploaded as artifacts) and
  `determinism-compare` (fails when Linux and Windows disagree). Bevy's Linux libraries (ALSA, udev, X11,
  xkbcommon, Wayland) are installed where the workspace builds; cargo is cached (`Swatinem/rust-cache`).
- **`docs/debugging.md`**: the debugging guide (every npcctl subcommand and BRP/agent/world/session method with
  examples and result shapes, error codes, all 39 journal kinds grouped by crate, mechanics and budgets, Tracy and
  Chrome traces, `cargo xtask verify`, the hooks, and recipes: why did unit X do Y, reproduce and bisect a desync,
  the slowest mechanic, the live game with curl, a screenshot at tick N, a recorded agent session).
  `docs/_notes/npcctl-reference.md` and `rpc-reference.md` are now pointers to it.
- **`docs/using-npc-framework.md`**: integrating the framework into another Bevy game (the dependency and
  features, `NpcPlugins` and every `with_*`, a complete unit type registered by name, the host-owned schedule,
  tick and movement seam, Relaxed mode, every capability with a snippet and its hooks, the glue table,
  determinism rules, view helpers incl. `RightClickMode` and the perf setup, versions). Its snippets were
  compile-checked and run in a scratch crate against the facade.
- **`docs/agent-experiments.md`**: running AI-agent experiments (headless `serve` or the windowed testbed, attach
  modes, the real-time observe → act → events loop, staleness and idle policy, a prompt template, MCP setup for
  Claude Code, analysing and verifying a recorded session, a worked key → store → pickaxe → ore example).
- **Skill `/play-actor <unit> <goal>`** (`.claude/skills/play-actor/SKILL.md`); `/add-mechanic`,
  `/add-unit-type`, `/verify-visual`, `/debug-desync` and `/why` updated for the content crates, the testbed, the
  RTS demo, capability kinds and the CI determinism job.
- README rewritten as a front page (screenshots, features per capability, quick start, crate map, guides);
  `CLAUDE.md` lists every crate and app and points to the guides.

### Added (wave G2a: the windowed Actor Testbed)

- **`content/testbed_view`** (presentation only; never writes sim state or inserts components on sim entities):
  Dungeon wall runs tiled from `wall` pieces; doors from `wall_doorway` squeezed along the wall (`DOOR_SX` 0.5) so the
  leaf fits the sim's 1 m door cell, the leaf re-parented onto its own hinge pivot (non-uniform scale without
  shear), swinging smoothly with `Door.open`; trees that turn into stumps when their `ToolTarget` is used up, the ore
  rock shrinking with its uses, workbench / anvil / forge, chest; items lying on the ground, equipped tools in the
  holder's hand bone, other carried items hidden; the cart (turned handle-first) and the siege-ram wagon with
  re-pivoted, spinning wheels; Knight / Rogue (player 1) and skeletons (player 2) with faction rings, animation
  from sim state (`Pose`: idle, walk, work, interact, craft, pick up, ride) and riders seated on their vehicle;
  RTS camera; F1 overlay (pose, interaction, crafting, ride, needs, tree path, order queue, equipment,
  inventory), **F5** character sheet (attributes, secondary, traits, skills with effective level, points, XP,
  level, policy), **F6** perception panel and gizmos (cone, line-of-sight rays, memory markers, last noise); the
  RTS command UI (`CommandUiPlugin`) with the glue's item / door / vehicle resolvers plus `testbed.use_tool`
  (tree, ore → `use_tool_on`), Z/X/C/V formations, H/B/U hold / stop / unload.
- **`apps/actor_testbed`**: `--scenario NAME|FILE`, `--seed`, `--speed`, `--paused`, `--player N`, `--select
  ID[,ID..]`, `--camera X,Z,DIST[,YAW,PITCH]`, `--commands FILE` (submitted when each tick comes up),
  `--screenshot PATH --after-secs S`, `--panels journal,perf,sheet,perception`, `--journal`, `--record`,
  `--remote-port` / `--no-remote` / render port flags; BRP `npc/*` + `npc/screenshot` + **`AgentRemotePlugin`**
  (`agent/*`, `world/state` against the window). Speed keys Space / `.` / `-` `=` (digits are control groups).
  The windowed sim matches the headless one (testbed seed 1, tick 117: the same state hash).
- **`npc_view_helpers::command_ui`**: `CommandFormation` (the formation group moves carry; hosts bind keys) and
  `order_command_with` (additive).
- **`apps/village`**: `AgentRemotePlugin` behind `debug-tools` (read-only there: no `AgentIoPlugin`, the content
  hash is unchanged; `tests/rpc.rs` `rpc_agent_methods_read_the_village`).
- **arch_fitness** (tightening): `testbed_view` is a `VIEW` crate, `testbed_content` must stay headless, canaries
  `actor_testbed → testbed_view → bevy_render`.
- README "Actor Testbed" section, `docs/img/testbed_{overview,panels,door}.jpg`.

### Added (wave G2b: Actor Testbed glue scenarios, npcctl testbed support, testbed corpus)

- **npcctl** takes the testbed in `run`, `inspect`, `journal`, `replay record|verify`, `diverge` and `bench`:
  `--scenario testbed` / `testbed:<name>` (an embedded scenario; a bare `smithy` is qualified) or a scenario file
  (`*.json`, built by its content's `npc_agent_io` loader). A factory registry (`apps/npcctl/src/sim.rs`,
  `CONTENTS`) resolves names; the village is last and takes every other name, so village output, replay headers
  and errors are byte-identical (the `golden__run_seed1_7200` snapshot and the village corpus are unchanged).
  Additive: `run` adds `content_summary` (absent for the village; testbed: units, doors, vehicles, ground items)
  and counts every brain's aborts/leaf starts for other contents; `inspect` prints the kernel's installed
  inspector for other contents; `bench --scenario S` (default `village_n`; fixed-size scenarios run once);
  `--seed` defaults to 1 (a scenario file: its own seed). `replay record --scenario FILE` prints a session record.
  `replay verify` dispatches: a session record re-runs from its document (as `verify-session`), a replay by the
  content claiming its `scenario` name. `diverge` checks both files are of the same content.
- **`testbed_content`**: `replay` (`try_new_replay_app`: the testbed's replay `AppFactory` for `testbed` /
  `testbed:<name>`, `ScenarioInfo.name` qualified; `verify`, `try_diverge`, `is_testbed`, `qualified`),
  `summary::summary_json`, scenario `smithy.json` (smith + joiner with the `player` policy + apprentice); books
  and scrolls `Teaches` (Carpentry / Smithing, max 12, 200 ticks), `smith`-template units are a `Teacher` of
  Smithing (100 ticks) (`content::TEACHES` / `TEACHERS`). Components only (already registered by the glue): the
  testbed content hash is unchanged by them; the testbed state hash changes (no earlier testbed oracle existed).
- **Tests**: `content/testbed_content/tests/glue.rs` (cart via its driver + `Unload`, `Mount` orders, the wagon
  with crew, queued `Interact` orders key → store → pickaxe, mine → smelt → forge with quality from Smithing,
  crafting XP → level-up → AI spend, Player policy + `SpendPoints`, study and teaching gated by level, a scripted
  `Direct` agent doing key → store → pickaxe → ore by `observe`/`act` only with `why_not` checks and its session
  replayed without the agent, an Advisor ordering a squad in formation, faction ownership rejections);
  `content/testbed_content/tests/corpus.rs`; `apps/npcctl/tests/testbed.rs` (CLI + insta golden
  `testbed__run_testbed_4800`).
- **Corpus** (new oracle files; existing ones untouched): `tests/replays/testbed_default_2days.json` (4800
  ticks), `testbed_orders_craft.json` (`testbed:smithy`, 2400 ticks, 18 commands), `testbed_vehicles.json`
  (`testbed:vehicles`, 1200 ticks, 8 commands), `testbed_agent_session.json` (session record, 600 ticks, 16
  commands). The village corpus test skips other contents' files; `cargo xtask verify`'s replays step counts
  files per kind (village / testbed / session).

### Added (wave G2c: `apps/rts_units_demo`, the portability proof)

- **`apps/rts_units_demo`**: a self-contained RTS host depending only on `npc_framework` (`nav`, `command`,
  `view-helpers`, `perf`) + `bevy`, no `content/*` crate. Its own content in the app (`sim.rs`): unit types
  `Infantry` / `Engineer` registered by name (trees `rts_infantry` / `rts_engineer`: `has_unit_order` →
  `ExecuteOrder` over the `RtsIdle` leaf; components `rts_unit`, `rts_wall`, `rts_idling`; kind namer), a 96 × 96 m
  map with `GridBlocker` walls, 200 `NavAgent` units of 2 players in 4 squads, a scripted opening (squads cross the
  map around the walls; `AttackMove` + a shift-queued `Move`). Host (`host.rs`): `Time<Fixed>` at 60 Hz with its own
  physics-like system in `FixedUpdate`; the sim at 10 Hz `run_in(FixedUpdate)`. View (`view.rs`): capsule/cube
  visuals per player and squad, RTS camera, HUD, `command_ui` selection and control groups, right click with
  A (`AttackMove`) / P (`Patrol`) / Shift (queue), Z/X/C/V formations (`CommandFormation`), S/H, F2/F3/F4.
  Flags `--units`, `--seed`, `--idle`, `--screenshot PATH --after-secs S`, `--headless-ticks N` (JSON summary:
  arrived, ticks, `all_arrived_at`, state/content hash, wall time). Screenshots `screenshots/rts_demo_*.png`,
  `docs/img/rts_demo.jpg`; README "Portability" with the findings.
- Tests: `apps/rts_units_demo/tests/demo.rs` (`rts_demo_orders_move_squads`: all 200 arrive by tick 1500,
  identical summary and state hash across two runs; `headless_flag_prints_the_summary`;
  `host_fixed_timestep_is_untouched_in_demo`); arch_fitness `rts_demo_uses_only_facade` (direct workspace deps ⊆
  {`npc_framework`}, no content/app crate anywhere in the unified closure, dev deps included).
- **Facade prelude** (additive): `GridBounds`.

### Added (actor capabilities F4: facade glue for command, vehicles, progression, crafting)

- **Glue modules** (`npc_framework::glue`, ADR 0011/0012; each behind both facade features, its own plugin, added by
  `NpcPlugins` when both capabilities are on; none is registered otherwise):
  - `command_interaction` (`CommandInteractionPlugin`): order executor `npc_interaction.interact` for
    `Interact { verb, target }` (walk into the verb's reach through `MoveIntent`/nav, then `begin`; `OrderDone` on
    success, `OrderFailed` with the verb's own reason; location-independent checks fail before walking; queued
    *pick up key → unlock → open* works); hashed `InteractOrder`; finished hook `npc_command.interact_order`; with
    `view-helpers` the `ContextOrders` resolvers `npc_interaction.item` (→ `pick_up`) and `npc_interaction.door`
    (→ `open` / `unlock`); shared engine `interaction_order_step`.
  - `command_vehicles` (`CommandVehiclesPlugin`): redirect `npc_vehicles.crew` (a `ByCrew` vehicle's orders go to its
    `Commandable` driver), executor `npc_vehicles.drive` for `Move`/`AttackMove` (a driver steers the vehicle's
    `MoveIntent`; a vehicle ordered itself goes through `command_vehicle_to`, i.e. its AI driver's `DriveGoal`;
    riders fail "dismount first"; `Direct` vehicles and everyone else use the built-in move), `npc_vehicles.mount`
    (walk into `mount_reach_mm`, then `mount`), `npc_vehicles.unload`; with `view-helpers` resolver
    `npc_vehicles.mount`. Content gives a `Direct` vehicle `Commandable` + `Owner` (it is a unit); a `ByCrew` vehicle
    needs them too (orders name it), its driver `Commandable` to receive them.
  - `crafting_interaction` (`CraftingInteractionPlugin`, feature `crafting`): interaction busy predicate
    `npc_crafting`, so no interaction starts while a unit crafts.
  - `progression_crafting` (`ProgressionCraftingPlugin`): craft hook `npc_progression` (`award_xp(.., XP_CRAFT)` on
    success, `grant_practice` of the recipe's duration).
  - `progression_interaction` (`ProgressionInteractionPlugin`): verbs `study` (target with `Teaches { skill,
    max_level, session_ticks }`, e.g. a book; `grant_study` capped by the book) and `teach` (actor with
    `Teacher { skills, session_ticks }` and a sheet, target a student with `Experience`; gated like
    `grant_teaching`); components hashed as `teaches` / `teacher` (they live in the glue, so `npc_progression`'s
    content hash is unchanged and it stays off `npc_interaction`).
  - `command_agent` (`CommandAgentPlugin`): section `command`, actions `order {order, units, queue, formation}`
    (validated `IssueOrder` for any owned units/squads, Direct and Advisor) and `execute_orders` (Direct units with
    a `Brain` queue the `ExecuteOrder` leaf).
  - `vehicles_agent` (`VehiclesAgentPlugin`): `mount@v` / `dismount@v` affordances (interaction verbs, `why_not`),
    `drive_to {pos}` (Direct driver: `DriveVehicle` leaf; else the recorded `DriveTo` command), `unload`, section
    `vehicle`.
  - `progression_agent` (`ProgressionAgentPlugin`): section `progression` (XP, level, next level, learning).
  - `crafting_agent` (`CraftingAgentPlugin`): `craft:<recipe>` affordances per nearby workstation with
    `CraftError` `why_not` (handlers per recipe and `craft {recipe}`; Direct: `Craft` leaf, Advisor: `Craft`
    command), section `crafting`.
- **`npc_interaction`** (additive): `InteractionListeners` with busy predicates (`add_busy_check`, consulted by
  `busy_with`, so `begin`, the `Interact` command and affordances) and finished hooks (`add_finished_hook`, called
  when an interaction ends: success/failed/invalid/cancelled), by name; names join the content hash only when
  registered (none by default).
- **`npc_agent_io`**: `SIGNIFICANT_KINDS` lists `OrderIssued/Started/Done/Failed`, `Mounted`, `Dismounted`,
  `VehicleStalled`, `XpGained`, `LevelUp`, `SkillImproved`, `CraftStarted/Done/Failed`.
- **Architecture fitness**: rows for `npc_progression` (rules only among capabilities) and `npc_crafting`
  (interaction + rules only), both in `CORE`, facade as-built rules and canaries for `progression` / `crafting`.
- Tests: `crates/npc_framework/tests/glue_command.rs`, `glue_learning.rs`, `glue_agent.rs`.

### Added (actor capabilities F3: `npc_progression` and `npc_crafting`)

- **`npc_rules` roll feed** (additive): `RollCounters.feed: RollFeed { enabled, records }` with `RollRecord { tick,
  unit, skill, outcome, purpose }`, filled by every roll (`roll_against` and up, `roll_with`) only when enabled;
  `RollCounters::drain_feed`. Off by default (nothing changes without a consumer); never fed from the journal.
- **`npc_progression`** (opt-in, facade feature `progression`, `ProgressionPlugin`; `NpcPlugins::with_progression()`):
  - `Experience { xp, level }` (requires `Learning` and `SpendPolicy`, default `Ai`); XP sources by name
    (`XpSourceDef`, defaults `roll`/`craft`/`job`/`discovery`); roll XP from the feed (successes of registered skills:
    `5 + clamp(16 - target, 0, 10) + 2 * difficulty step + 5 on a crit`); `award_xp(world, unit, amount, source)`.
  - Default curve: triangular, base 100 (levels at 100, 300, 600, 1000 .. total XP), 5 banked points per level, max
    level 50. `SpendPolicy::Ai` spends at once, one skill point at a time on the highest `uses / (points + 1)`;
    `Player` banks for `SpendPoints`.
  - Learning: practice (30 ticks per roll, 300 per point, cap attribute + 2), study (200 per point, cap attribute + 4
    and the book's max), teaching (100 per point, teacher ≥ student + 1, cap the teacher's level): `grant_practice`,
    `grant_study`, `grant_teaching` (the `study`/`teach` verbs are facade glue, wave F4).
  - Journal `XpGained`, `LevelUp`, `SkillImproved`; perf mechanic `progression`; inspect `experience_json`; state hash
    (`experience`, `learning`, `spend_policy`, `progression_config`) only with the plugin.
- **`npc_crafting`** (opt-in, facade feature `crafting`, `CraftingPlugin`; `NpcPlugins::with_crafting()`):
  - `RecipeDef` by name (inputs by kind or tag from the inventory, tools carried or equipped, workstation kinds,
    skill + modifier, duration, outputs, xp), validated at freeze (`validate_recipe`); `Workstation { kinds }`.
  - One path for the `Craft` leaf (recipe, or the next order on the workstation), the `Craft` command and orders:
    claims the workstation (capacity 1) and the items, rolls `npc_rules::roll_skill` (purpose `craft.<recipe>`) at the
    end; quality `min(margin / 2, 5)`, +2 on a critical success; a failure keeps the inputs; a critical failure
    consumes them and damages the tools (`Durability` −10, or −1 quality without it); every attempt wears tools by 1.
  - `CraftingOrders` + `PostCraftOrder` command, `crafting_jobs` (job board), `next_order_for`; completion hooks by
    name with `CraftCompleted` (the progression glue is wave F4); `CraftError` reasons usable as `why_not`.
  - Journal `CraftStarted`, `CraftDone`, `CraftFailed`; perf mechanic `crafting`; inspect `crafting_json`; state hash
    (`workstation`, `crafting_orders`, `durability`, `crafting`, `craft_leaf`, `crafting_config`) only with the plugin.
    Village oracles unchanged (the village adds neither plugin).

### Added (actor capabilities F2: `npc_vehicles`, seats, mounting, ByCrew/Direct vehicles)

- **`npc_vehicles`** (opt-in, facade feature `vehicles`, `VehiclesPlugin`; `NpcPlugins::with_vehicles()`, which
  also turns on interaction; ADR 0012: depends on `npc_interaction` only among the capabilities):
  - `Vehicle { seats: [Seat { role: Driver | Passenger | Gunner, required, offset }], speed_mm_per_tick, control:
    ByCrew | Direct }`; `MountedIn { vehicle, seat }` on the actor and `Occupants` (one `SeatSlot` per seat:
    `Free` / `Claimed` / `Taken`) on the vehicle, both by `SimId`, changed only by the crate.
  - Mounting is the interaction verbs `mount` / `dismount` (reserved by `npc_interaction`, registered here): one
    path for the `Interact` leaf/command, affordances and the new commands `Mount { unit, vehicle, role? }`,
    `Dismount { unit }`, `Unload { vehicle }`. Seat = requested role (strict), else `SeatPreference`, else first
    free; seats are claimed per mount (lower `SimId` wins a tie, the loser is cancelled with the reason); reach
    `VehicleConfig::mount_reach_mm`. Dismount/unload place actors on free cells next to the vehicle (ring search,
    nearest first, never through walls or wall corners, unoccupied cells preferred).
  - While mounted: `SimPos` = vehicle + seat offset every tick (after the movers), the actor's own `MoveIntent`
    is dropped, BT conditions `Mounted`, `MountedAsDriver`, `HasDriveGoal`.
  - `ByCrew`: the driver's `DriveVehicle` action writes the vehicle's `MoveIntent` toward its `DriveGoal`, its
    standing order (`SimCommand::Order` to the driver) or blackboard slot `goal_slot`; without a driver (or with a
    required seat empty) the intent is dropped, `MoveStatus::Blocked`, journal `VehicleStalled`. `Direct`: the
    vehicle moves through locomotion/nav like any agent with its crew aboard. Glue hooks:
    `command_vehicle_to`, `driver_of`, `can_move`, `set_drive_goal`, `mount`/`dismount`/`unload`.
  - Journal kinds `Mounted`, `Dismounted`, `VehicleStalled` (NAMES, journal panel); perf mechanic `vehicles`
    (tag + budget); inspect `vehicle_json`; crew gizmo `npc_view_helpers::vehicle_gizmos` (feature `vehicles`);
    state hash (`vehicle`, `occupants`, `mounted_in`, `seat_preference`, `drive_goal`, `driving`,
    `vehicle_config`, plus locomotion state) only with the plugin. Village oracles unchanged (the village does not
    add the plugin).

### Added (actor capabilities F1: `npc_command`, RTS orders, queues, formations, squads)

- **`npc_command`** (opt-in, facade feature `command`, `CommandPlugin`; `NpcPlugins::with_command()`):
  - `Owner(PlayerId)` and `Commandable { speed_mm_per_tick, arrive_mm }` on units; every order is the recorded
    content command `IssueOrder { player, units, order, queue, formation? }`, validated against ownership on submit
    and again at its tick. `UnitOrder`: `Move`, `AttackMove` (tagged move), `Stop`, `Hold`, `Follow { target }`,
    `Patrol { points }`, and the data-only `Interact { verb, target }`, `Mount { vehicle }`, `Unload` (rejected
    unless an `OrderExecutor` is registered for the kind by name: capability glue).
  - Per-unit `OrderQueue` (current `ActiveOrder` with progress + shift-queue): `queue: false` replaces (journal
    `OrderFailed { reason: "replaced" }`), `queue: true` appends. Journal kinds `OrderIssued`, `OrderStarted`,
    `OrderDone`, `OrderFailed` (in `JournalKind::NAMES`, journal panel).
  - Execution in `NpcCommandSet::Execute` (after `Sense`, before `Think`) in `SimId` order through the locomotion
    seam; `Brain` units run orders via the BT condition `has_unit_order` + leaf `ExecuteOrder` (aborting pauses).
    Hooks by name: `register_order_executor(kind, name, ..)`, `register_order_redirect(name, ..)`.
  - Group moves: integer formation slots (`Line`, `Box`, `Wedge`, spacing in mm) facing centroid → target, blocked
    slots relocated to the nearest free cell (deterministic), one leader whose nav path followers reuse shifted by
    their offset. `Squad { members, formation }` + `MemberOf`, `CreateSquad` / `DisbandSquad` commands.
  - Perf mechanic `command` (tag + budget); inspect `command_json`; state hash (`command_owner`, `commandable`,
    `order_queue`, `executing_order`, `squad`, `squad_member`, `command_config`, plus locomotion and spatial
    state) only with the plugin. Village oracles unchanged (the village does not add the plugin).
- `npc_view_helpers` feature `command`: opt-in `CommandUiPlugin` (click/box/shift selection, Ctrl+1-9 control
  groups, right-click context orders with `ContextOrders` resolvers by name, order/waypoint gizmos), configurable
  `CommandBindings`; it only emits `SimCommand`s.

### Added (actor capabilities E1: `npc_interaction`, items, inventory, doors, tools, verbs)

- **`npc_interaction`** (opt-in, facade feature `interaction`, `InteractionPlugin`; `NpcPlugins::with_interaction()`):
  - `Item { kind, stack, quality }` entities (in the world with `SimPos`, or carried: `CarriedBy { holder, slot }`),
    `ItemKindDef` by name (weight g, stacking, tags such as `"tool:axe"` / `"key:gate"`, equip slot);
    `Inventory { slots, max_weight_g, items }` and `Equipment { slots }` as sorted `SimId` lists; `Interactor`.
  - `InteractionDef` by name (`register_interaction` / `override_interaction`): offer filter, preconditions with
    reasons, duration (fixed or per target), optional skill check, effects, target reservation, noise, item arg.
    Built-ins `pick_up`, `drop`, `give`, `open`, `close`, `lock`, `unlock`, `equip`, `unequip`, `use_tool_on`
    (`ToolTarget { required_tag, yields, duration, check, remaining }`); `mount`/`dismount` reserved for vehicles.
  - One path: the `Interact` BT leaf (`ParamValue::Name(verb)`, target/item from blackboard slots 7/6), the
    `Interact { unit, verb, target, item? }` content command and doors on nav paths all call `begin`; completions
    run in `NpcInteractionSet::Complete` in `SimId` order. Hooks by name: skill check (none: checks pass) and noise.
  - `Door { open, locked_by, key }`: the door cell's static grid data (tag bit 0, blocking + occluding while closed,
    extra cost 1 while locked); nav pass-cost fn (closed door the agent may open: extra cost + path marker; locked
    without the key: blocked); a `NavAgent` + `Interactor` at a marked door unlocks (with the key) and opens it.
  - Journal kinds `Interacted { verb, target, item, outcome, reason }`, `ItemMoved`, `DoorChanged` (in
    `JournalKind::NAMES`, journal panel); perf mechanic `interaction` (tag + budget); inspect `interaction_json`;
    state hash (`item`, `inventory`, `equipment`, `carried_by`, `interactor`, `door`, `interactable`,
    `tool_target`, `interacting`, `interact_leaf`, `interaction_config`, the spatial static layer) only with the plugin.
- **`npc_framework::glue`** (ADR 0011): `interaction_rules` (`InteractionRulesPlugin`: checks are `roll_skill`
  with purpose `interaction.<verb>`; a failed roll spends the time, no effect), `interaction_perception`
  (`InteractionPerceptionPlugin`: door and tool noises become `Noise`), `interaction_agent`
  (`InteractionAgentPlugin`: affordances for every verb with `why_not`, handlers (Direct: the leaf; Advisor: the
  command), section `inventory`). Village oracles untouched (the village adds no interaction plugin).

### Added (actor capabilities E2: glue between the capability crates, ADR 0011)

- **`npc_framework::glue`** (facade modules, each compiled only when both features are on; capability
  crates still never depend on each other):
  - `perception_rules` (`perception` + `rules`): `PerceptionRulesPlugin { activate }` registers detection
    check `"per_roll"`: 3d6 under Per + distance band + sense + target modifiers by name
    (`PerRollAppExt::register_per_roll_modifier`; built-in `"stealthy"` reads the new hashed `Stealthy
    { penalty }`), tuned by the hashed `PerRollConfig` (no sheet: automatic; spotted targets tracked
    without re-rolls unless `reroll_sensed`). Dice keyed `(seed, observer, tick, "perception.detect",
    target·4+sense)`; every roll journaled as `Roll { skill: "Per", purpose: "perception.detect" }`.
  - `perception_agent` (`perception` + `agent-io`): `PerceptionAgentPlugin`, perception provider
    `perception` (`sensed` / `remembered` with last-seen ticks and ages, `last_noise`): `agent/observe`
    shows only what the unit perceives and reports `omniscient: false`; `world/state` still shows all.
  - `rules_agent` (`rules` + `agent-io`): `RulesAgentPlugin`, section `sheet` (attributes, secondary,
    traits, bought skills with effective levels, points), affordance/handler `spend_points {purchase}`
    (Direct and Advisor; `why_not` without points / for observers), registers `SpendPoints`.
- **`NpcPlugins`**: opt-in `with_perception()`, `with_rules()`, `with_agent_io()`,
  `with_per_roll_detection()`; they add the capability plugins and the glue of every pair that is on.
- **`npc_rules`**: recorded command `SpendPoints { unit, purchase }` (validated at submit and apply; not
  registered by `RulesPlugin`, so its content hash is unchanged).
- **`npc_tools`**: feature `rules` (forwarded by the facade's `rules`): BRP `npc/sheet {unit}` with `remote`.
- Docs: ADR 0011, CLAUDE.md crate map rows, `.claude/rules/sim.md` paths (new core crates and the glue),
  README architecture. Village oracles untouched (the village adds no glue plugin).

### Added (actor capabilities D3: `npc_agent_io`, external agent control in real time)

- **`npc_agent_io`** (opt-in, facade feature `agent-io`, `AgentIoPlugin`): controllers attached by recorded
  commands (`AttachController` / `DetachController`, modes `Direct` / `Advisor` / `Observer`, idle policy
  `Idle` / `Autopilot`); in `Direct` mode a reserved top-priority `ExternalControl` guard (injected into
  every tree at freeze time through the new kernel `TreeTransforms` hook) runs an `ExternalAction` leaf over
  a per-unit queue of steps that name registered actions (`AgentAct` / `AgentCancel` / `AgentClear`);
  provider registries by name (`AgentIoAppExt`: observation sections, perception, affordances with
  `why_not`, action handlers); observations as JSON (`omniscient: true` until a perception provider exists)
  and deterministic text; staleness guard (`expect_tick` / `expect_seq`, `AgentIoConfig`); per-controller
  latency stats; an event buffer harvested from the journal. Methods `agent/attach|detach|observe|act|
  cancel|clear|events|stats|list_units`, `world/state|load_scenario|reset`, `session/info|stop`. Hosts: a
  headless real-time session (std JSON-RPC over HTTP, BRP wire format; `agent/events` long-polls without
  blocking the loop, ADR 0010) and, with feature `remote`, BRP (`AgentRemotePlugin`, watching methods).
  Versioned scenario files with per-content loaders; session records that verify without the agent.
  Clients: `RpcClient`, a stdio MCP server, the `greedy` reference controller
  (`examples/greedy_controller`). Journal kinds `ControllerAttached`, `ControllerDetached`,
  `ExternalAction`, `ActionRejected`; perf mechanic `agent_io`. Without the plugin nothing changes (village
  oracles untouched); with it the content hash changes.
- **Village:** `village_content::agent` (`VillageAgentPlugin`: `needs` / `village` sections, `walk_to`,
  `eat`, `sleep`, `work` affordances and handlers; the `village` scenario loader),
  `try_new_sim_app_with`, `scenarios/agent_village.json`.
- **npcctl:** `serve --scenario s.json [--tick-hz N] [--record f] [--port N] [--seed N]`, `agent
  attach|detach|observe|act|cancel|clear|events|stats|list-units`, `world load-scenario|reset|state`,
  `session info|stop` (`--url`, `--format json|text`), `mcp --url`, `replay verify-session`.
- **npc_tools:** `RemoteAppExt::add_remote_watching_method`; re-exports `BrpError`, `BrpResult`,
  `error_codes`.

### Added (actor capabilities D1: `npc_perception`)

- **`npc_perception`** (opt-in, facade feature `perception`, `PerceptionPlugin`): `Sensors` (vision cone with an
  integer half angle checked by dot products against a cos² table, no runtime trig; hearing; proximity),
  `Facing` (follows movement unless `FixedFacing`), `Perceivable { kinds }`, `Noise` stimuli (`emit_noise` /
  `EmitNoise`, no-ops without the plugin), line of sight over occluding grid cells (`trace_segment`), bounded
  `PerceptionMemory` (forget after N ticks, oldest evicted), staggered scans `(sim_id + tick) % N` in
  `NpcSet::Sense` (`NpcPerceptionSet`), `DetectionChecks` hook by name (default `automatic`), conditions
  `sees_kind` / `remembers_kind` / `heard_noise` (+ generic `sees::<M>` etc.), `PerceptionFacts` blackboard
  mirror, `perception_json` inspect section. Journal kinds `Perceived` / `Lost`, perf mechanic `perception`,
  vision-cone / LOS gizmos in `npc_view_helpers` (feature `perception`). Without the plugin nothing is
  registered (village unchanged); adding it changes the content hash.

### Added (actor capabilities D2: `npc_rules`)

- **`npc_rules`** (opt-in, facade feature `rules`, `RulesPlugin`): point-buy `CharacterSheet` (ST/DX/IQ/HT,
  derived HP/Will/Per/FP/Basic Speed in quarters/Basic Move, traits, skill points, banked points); `SkillDef`,
  `TraitDef`, `SheetTemplate` registered by stable name (`RulesAppExt`, checked at freeze); a pluggable
  `RuleSet` resource (own default cost tables, doubling-then-linear skill curve, difficulty offsets, crit
  thresholds, default chain, dice and resolution functions); 3d6 roll-under `roll_skill` / `roll_stat` /
  `roll_against`, `quick_contest`, `resisted_roll` with a counter-based dice key `(seed, unit, tick, purpose,
  roll index)`; validated `spend_points`; `SheetView` JSON in inspect. Journal kind `Roll` (`Critical` in the
  kernel), perf mechanic `rules`. Without the plugin nothing is registered (village unchanged).

### Changed (actor capabilities C2: tooling follow-ups, eat claim, zstd)

- **Profiler:** budgets are checked only once a mechanic has `min_samples` samples since the last reset
  (`DEFAULT_MIN_SAMPLES` 60, `PerfCollector::set_min_samples`; `over_budget` waits too);
  `PerfBudgets::scaled_for_profile()` (×10 with `debug_assertions`) is what the village uses. `report()`
  selects percentiles in O(window) instead of sorting; `report_top` / `report_limited` summarise only the
  rows kept; the F4 panel caches its report and refreshes it at most every 0.5 s.
- **BRP ports:** `NpcRemotePlugin { port, render_port: RenderPort }` probes both ports and logs a taken one
  (with its number) instead of failing silently (`RemoteStatus`); the render-app server follows the main
  port (`port + 1`) or is off. Village flags `--render-remote-port N`, `--no-render-remote`.
- **Journal:** `JournalKind::NAMES` / `parse_name` (npcctl uses them); `with_journal_from_build` and
  `npc_tools::new_app_journaled` keep the scenario's tick-0 `Spawn` entries; `npcctl journal` uses it by
  default (`--journal-from-build false` for the old output), `inspect` on request. Library default unchanged.
- **One replay recorder:** `npc_kernel::replay::Recorder` (aligned or live cadence), re-exported as
  `npc_tools::recorder`, behind `replay::record`, `npcctl run --record` / `replay record` and BRP
  `npc/record/*` (`village --record`). Recorded replays are byte-identical to before.
- **Deferred command validation:** `submit_deferred` validates at apply time only (skips recorded in
  `SkippedCommands`); npcctl command files use it, so a command can target a unit spawned earlier in the
  same tick. Rejections still exit 2 with the same JSON.
- **zstd:** the windowed apps decode zstd KTX2 LUTs with Bevy's `zstd_c` (C `zstd`) instead of `ruzstd`;
  Bevy's default features are spelled out in `village`/`village_view` without `zstd_rust`, the libraries
  depend on Bevy without default features. `ruzstd` is gone from the graph and `deny.toml` no longer
  ignores RUSTSEC-2026-0344.
- **Eat claim (behaviour change, re-baseline staged):** `eat_system` releases the stockpile claim in the
  tick the meal completes, with its `TakeFood`, instead of when the leaf stops a tick later
  (`docs/_notes/c2-eat-claim-evidence.md`).

### Added (actor capabilities C1: spatial grid, locomotion seam, `npc_nav`)

- **Spatial grid** in the kernel (`npc_kernel::spatial`, `SpatialPlugin` in `NpcPlugins`): integer cells
  (default 1 m, 128 × 128), static `CellInfo` (blocking/occluding/tags + extra cost) merged with
  `GridBlocker`/`GridOccluder` footprints, a `SimId` index updated in `NpcSet::Inputs`, sorted
  `in_radius`/`in_box` queries, a conservative integer `trace_segment`, and `GridChanged` messages. Not hashed
  unless a capability calls `register_spatial_hash`.
- **Locomotion seam** (`npc_locomotion`, facade feature `locomotion`, default): `MoveIntent`/`MoveStatus`,
  `NpcSet::Locomotion` (between Actions and Effects) with `NpcMoverSet`, and the `StraightLineMover` in
  `NpcPlugins`. The village's `MoveToDest`/`FollowOrder` now write a `MoveIntent` and end on `MoveStatus`;
  the village stays byte-identical (corpus and goldens unchanged; the intent is not hashed unless
  `register_locomotion_hash` is called).
- **Pathfinding** (`npc_nav`, facade feature `nav`, opt-in `NavPlugin`/`GridPathMover` + `NavAgent` per
  unit): deterministic grid A* (10/14, no corner cutting, `(f, h, cell)` ties), LOS smoothing, a per-tick
  node budget (`NavBudget`, 4000) served in `(tick, SimId)` order with carry-over, replanning on
  `GridChanged` or `Blocked`, a per-tag `PassCost` hook with path markers (doors, wave E), journal
  `PathFound`/`PathFailed`, perf mechanics `spatial`, `locomotion`, `nav`, `nav.astar`, and a path gizmo
  (`npc_view_helpers`, feature `nav`). New scenario `village_nav` (wall per plot, `NavAgent` villagers).

### Changed (baseline hardening P3: crate split, stable-name registration)

- **Crate split** behind the facade `npc_framework` (`NpcPlugins`, `NpcAppExt`, `prelude`; features `tools`,
  `remote`, `perf`, `view-helpers`): `npc_sim` became `npc_kernel` + `npc_needs` + `npc_jobs` + the example
  content `content/village_content`; `npc_view` became the generic `npc_view_helpers` + `content/village_view`;
  `npc_remote` became `npc_tools` (feature `remote`), which also holds the CLI helpers and bench harness.
- **Registration by stable name** (ADR 0009): actions, conditions, scorers, trees, needs (`NeedDef`), sim
  components (`SimHash`), hashed resources and content commands; ids frozen sorted by name in
  `Plugin::finish`; trees reference names; typed leaf params (`ParamValue`); content hash in replays
  (format version 2; v1 files still load).
- **State hash** is registry-driven (registered components in name order). The encoding changed once;
  journals, inspect snapshots and run summaries are byte-identical before and after
  (`docs/_notes/p3-rebaseline-evidence.md`).
- **Commands**: `SpawnVillager` and `SetFood` are content commands (`ContentCommand`), same JSON on the wire.
- **Host-safe driver** (ADR 0008): `NpcPlugins::new().tick_hz(n).run_in(schedule)` with its own accumulator
  (`SimTime`); never writes `Time<Fixed>`; pause/speed/step in the kernel's `SimControl`, keys in
  `apps/village`; `DeterminismMode::{Strict, Relaxed}` with a `HostPosition` adapter; public labels
  `NpcSimTick`/`NpcSet`.
- `arch_fitness` covers the new crates (incl. an as-built feature resolver and a facade-only rule for content).


### Added

- **Behaviour-tree runtime (`npc_ai`)**: flat serializable `TreeDef` with a content hash, shared by all
  agents; per-agent `BrainState`/`Blackboard`/`ActiveLeaf`; `Selector`, `Sequence`, `Guard` with
  `Abort::LowerPriority`, `Leaf`, `UtilitySelector`; event-driven think for dirty agents only; actions as
  `SparseSet` components ticked by ordinary systems (ADRs 0001, 0005).
- **Deterministic headless sim (`npc_sim`)**: `SimTick` schedule at 20 Hz with ambiguity detection = Error,
  integer state (mm positions, milli-unit needs), `SimId` ordering, counter-based RNG, FNV state hash
  (ADRs 0002–0004).
- **Villager content (iteration 1)**: hunger and energy needs with hysteresis, smart objects (house, bed,
  stockpile, field) with `Provides`/`Approach`, `Reservations`, a food economy loop, day/night cycle, and
  the villager tree (urgent eat/sleep, forage, follow order, night sleep, casual eat, day work, wander).
- **Presentation (`npc_view`, `apps/village`)**: KayKit CC0 assets, separate visual entities (`VisualOf`),
  interpolated movement, animation from `Activity`, day/night lighting, RTS camera, F1 debug overlay,
  `--screenshot PATH --after-secs S`.
- **Debug tooling**: decision/event journal (JSON Lines, ring buffer, filters), `SimCommand` input layer
  with `GoTo`/`Clear` orders, replays (record, verify, diverge with per-entity diffs), inspect snapshots;
  all of it never changes sim results (ADR 0006).
- **`npcctl` headless CLI**: `run`, `inspect`, `journal`, `replay record|verify`, `diverge`, `bench`, JSON
  on stdout with exit codes 0/1/2.
- **BRP remote interface (`npc_remote`)**: `npc/pause|resume|step|speed|state|unit|command|journal|perf|
  perf/reset|record/start|record/stop|screenshot|methods` on `127.0.0.1:15702` (feature `debug-tools`).
- **Profiler (`npc_perf`)**: per-system timing via a tracing layer, mechanic tags, rolling p50/p95/max,
  per-mechanic budgets with rate-limited warnings, F3 journal and F4 perf panels, `bench/baseline.json`;
  optional `tracy` and `chrome-trace` features.
- **Agent workflow**: `CLAUDE.md`; path-scoped rules in `.claude/rules/` (sim determinism, view
  presentation-only, content registration); project skills `/add-mechanic`, `/add-unit-type`,
  `/verify-visual`, `/debug-desync`, `/why`; Claude Code hooks: a PreToolUse guard that protects goldens,
  snapshots, replays, baselines, lint config and the hooks unless the user creates
  `.claude/rebaseline-approved`, and a Stop hook that runs `cargo xtask verify --quick` and blocks while it
  fails (tests in `xtask/tests/hooks.rs`).
- **Architecture decision records** `docs/adr/0001`–`0008`, and this changelog.
- In progress (baseline hardening, `docs/superpowers/plans/2026-10-23-baseline-hardening.md`): the
  locomotion seam (ADR 0008) and `apps/rts_units_demo` (P4), CI (P5).
- Actor capabilities G1 (Actor Testbed assets + content): `content/testbed_content` (walled compound with every
  capability on; item kinds, skills, traits, sheet templates, recipes, unit trees with the order branch,
  `TestbedDrive`, `testbed` observation section, scenario loader `content: "testbed"` + 9 embedded scenario files,
  `try_new_sim_app`, headless scenario tests; `npcctl serve --scenario testbed`), CC0 testbed assets in
  `assets/testbed/` (POSIX `scripts/fetch_assets.sh`, `assets/CREDITS.md`, `docs/_notes/testbed-assets-measured.md`).
  New content only: village oracles unchanged.
