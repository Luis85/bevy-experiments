> **License note:** The nival/Blitzkrieg repository (https://github.com/nival/Blitzkrieg) is released under a license that bans commercial use. We reuse its formulas and rules as reference only, never its code or data verbatim.

# Blitzkrieg (Nival, 2003): mechanics research for a Rust/Bevy clone

## Main finding: the original source and data are public, so most numbers below are verified

In 2025 Nival published the single-player **source code, data files, editors and the English manual** at https://github.com/nival/Blitzkrieg. The license bans commercial use, so read `LICENSE.md` before reusing anything; treat the repo as reference, not as code or assets to copy.

Because of this, most of the report comes from primary files, not fan guesses:
- **Combat formulas:** `Sources/src/AILogic/*.cpp` and the stat schema in `Sources/src/Main/RPGStats.h`.
- **Tuning constants:** `Versions/Current/Data/consts.xml`.
- **Per-unit and per-weapon XML:** under `Versions/Current/Data/`.
- **Manual:** `Versions/Current/manual.rtf`. I converted it to text and read the combat, tactics and controls chapters.

Web sources (Wikipedia, reviews, forums) are used only to corroborate. Short paths below are relative to `https://github.com/nival/Blitzkrieg/blob/main/`.

**Units used in the data:** 1 AI tile = 32 AI points. Ranges, dispersion, sight and blast radii are stored in tiles; FireRate is rounds per minute; AimingTime and RelaxTime are in seconds (conversion in `Sources/src/Main/RPGStats.cpp`). The manual says one map unit is about 30 m.

---

## 1. Mechanics catalog

### 1.1 Core loop and mission structure
- **No base building, no resources, no unit production.** You fight with the forces you are given plus scripted reinforcements and limited ammo. Sources: [Wikipedia](https://en.wikipedia.org/wiki/Blitzkrieg_(video_game)), [Worthplaying review](https://worthplaying.com/article/2003/6/24/reviews/10825-pc-review-blitzkrieg/).
- **Campaigns:** three (Allies, Germany, USSR), each split into chronological **chapters** (German example: Poland, France, Barbarossa, Typhoon, Kharkov42, Kharkov43, Africa, Ardennes).
  - Per chapter (manual "Chapter screen"): play a **random intro mission**, which unlocks the one **historical scenario mission**. Finishing that unlocks the next chapter.
  - You can then keep playing **unlimited random missions** in the chapter (easy, normal, hard). Each mission names a reward unit.
- **Chapter definition files:** a chapter is XML plus a `context.xml` plus a Lua script. See `Versions/Current/Data/Scenarios/Chapters/German/Poland/1.xml` and `script.lua`.
  - Lua hooks: `EnterChapter`, `MissionFinished`, `PlayerGainLevel`.
  - Calls used: `EnableMission`, `EnableChapter`, `AddNewSlot` (adds a core-unit slot), `AddBaseUpgrade` (adds an option to the depot), `AddMedal`.
- **How random missions are generated:**
  - Template maps are picked per season and region (`Scenarios/TemplateMissions`, `Settings/*.xml` with weighted `FieldSets`).
  - Objective combos come from `Scenarios/Descriptions/*` (Defend_Area, Escort, Intercept, SA_xx_yy…; for example "Railway bridge and depot").
  - Each chapter's `ChapterUnitsTable` maps placeholder keys ("squad riflemans", "squad machinegunners"…) to weighted concrete units. It also sets which aircraft each side gets (type, number of planes, formation size, recharge time, paratrooper squad).
- **Core ("permanent") units:** named commanders who carry over between missions.
  - They gain experience from kills: experience goes up by the killed unit's `Price` (`AIUnit.cpp` `EnemyKilled`; prices in `Data/Units/AIPrices.xml`, e.g. soldier 1, officer 5, sniper 40, howitzer 60, heavy gun 120).
  - There are 4 ranks (Rookie, Regular, Veteran, Elite), shown as ticks above the unit.
  - Each rank level multiplies speed, relax (reload) time, dispersion, sight, fire range and rotation (`SAIExpLevel::SLevel` in `RPGStats.h`). Only core units level up.
- **Upgrades:**
  - *Bonus:* after a mission, swap one core unit for the reward unit.
  - *Depot:* before a mission, swap one core unit for any currently issued model.
  - Experience and the commander carry over to the new vehicle. Only artillery and armor slots are upgradable.
  - A core unit destroyed in a mission is replaced afterwards at **one rank lower** (manual "Replaceable and irreplaceable losses").
- **Player rank and medals:** the player has a career rank and medals. Rank levels unlock more slots via `PlayerGainLevel`.
  - Six skill ratings: Tactics, Logistics, Caution, Training, Art of War, Sense of duty. Reloading a save lowers the rating.
- **Reinforcements:**
  - Scripted from mission Lua through `LandReinforcement` / `DeleteReinforcement` (function list in `Sources/src/AILogic/Scripts/Scripts.cpp`, which also has `GetNUnitsInArea`, `ObjectiveChanged`, `Win`, `Loose`, `ChangePlayer`, `EnableAviation`, `SwitchWeather`, and more).
  - In multiplayer Flagcontrol, reinforcements arrive on a points threshold (consts `Flags PointsToReinforcement=150`; the manual says 100).
- **Multiplayer:** up to 16 players. Modes: *Sabotage* (attacker vs defender, no reinforcements) and *Flagcontrol* (points per held flag per second, plus a base income).

### 1.2 Unit classes (from the `EUnitRPGType` enum in `RPGStats.h`)
- **Infantry:**
  - Unit types: soldier, engineer, sniper, officer.
  - **Squads** are about 10 men with an officer. Example `Data/Squads/german_rifle_39/1.xml`: 1 officer, 1 AT rifle, 8 Mauser riflemen.
  - Infantry can **disband** into individuals and **re-form**.
  - Each country's gun-crew, HMG, paratrooper and resupply-engineer squads are defined in `Data/partys.xml`.
- **Transport:**
  - carrier (halftrack or truck that tows and carries; e.g. Sdkfz 7: TowingForce 13000 kg, 20 passengers)
  - support (engineering truck)
  - medicine
  - tractor (needed for heavy guns)
  - military auto (staff car: raises morale nearby)
  - civilian auto
- **Artillery:** gun (AT/field), howitzer, heavy gun, AA gun, rocket, super, mortar, heavy MG.
  - All are towed or crew-pushed and **need a crew squad**.
  - A gun with no crew becomes **neutral and capturable** by either side.
- **SPG:** assault, antitank, super, AA.
- **Armor:** light, medium, heavy, super. Armored cars and motorcycles count as armor.
- **Aviation** (called in from the map edge, never directly controlled): scout (recon), bomber (level or dive, auto-detected by dive angle ≥45°), attack ("shturmovik"), fighter (patrols 180 s), lander (paratroopers).
- **Trains:** locomotive, cargo, carrier, armored, super.
- **Scale:** objects.xml registers 288 units, 70 squads, 194 buildings and 18 bridges. Marketing claimed "350+ units and objects".

### 1.3 Combat model (verified in source)
**Firing cycle**
- Wait `AimingTime`, then fire `AmmoPerBurst` rounds at `FireRate` (rpm), then wait `RelaxTime`.
- Aim time and relax time are randomized by ×[1, 1.2] (`CoeffForRandomDelay`).
- Tanks **cannot fire their main gun while moving**; machine guns can (manual).

**Where the shell lands (dispersion, `Shell.cpp` `CExplosion::Init` and `Guns.cpp`)**
- Impact point = aim point + random offset inside an ellipse.
- Radius = `Dispersion × distance / RangeMax`, so the error grows linearly with range and equals `Dispersion` at max range.
- Ellipse shape (length along the trajectory ÷ width across it) depends on trajectory type: cannon 3–3.5, howitzer 1–2, bomb 2, line 1 (`consts.xml` `DispersionRatio`).
- Effective dispersion = base × difficulty coefficient × rank bonus × formation bonus ÷ morale term.

**Hit test (`AIUnit.cpp` `ProcessCumulativeExpl`)**
- It is a hit only if the impact point falls inside the target's rectangle, shrunk by `SmallAABBCoeff` (e.g. 0.5 for tanks, 0.7 for guns). That shrink plays the role of a "silhouette".
- There is **no separate to-hit roll**: hit chance comes purely from geometry plus dispersion.

**Armor side**
- Four sides are chosen from the attack direction relative to the target's rectangle (`unitRect.GetSide(attackDir)`).
- **Top** armor is used when the shooter is higher (aircraft) and for non-line (arcing) trajectories, per `CanBreach` and `CBurstExpl` in `Guns.cpp` and `Shell.cpp`.
- **Bottom** armor is used for mines.
- Each side stores a min and max value. Example, Tiger I (`Units/Technics/German/Tanks/Pz_VI_Tiger_Ausf_A_H/1.xml`): Front 95–151, Sides 59–84, Back 82–111, Top 14–38, Bottom 26–28.
- The manual says the shell's angle is ignored and the info panel shows the average values.

**Penetration**
- Roll piercing uniformly in `Piercing ± PiercingRandom` (× difficulty) and armor uniformly in [min, max] for the struck side.
- It penetrates if piercing ≥ armor **and** the target is not "saved by cover".
- Cover roll: saved if `Random ≥ cover`. Cover values: lying soldier 0.5, tank pit 0.5, formation `CoverBonus`, building slot coverage × 0.3.
- Equal values give 50% (the manual states this).
- Weapon examples:

| Weapon | Piercing | Damage |
|---|---|---|
| 88 mm KwK36 | 136 ± 50 | 110 ± 55 |
| PaK 40 | 106 ± 20 | 77 ± 39 |
| 37 mm | 45 ± 23 | 34 ± 17 |
| Rifle | 3 ± 3 | 7 ± 3 |
| AT grenade | 300 | 5 ± 4 (range 10) |

**Damage**
- On penetration: HP −= `DamagePower ± DamageRandom` (× difficulty).
- Example HP: soldier 10, guns 50, truck 30, Tiger 200, admin building 500.

**HE splash (shells with `Area2` > 0)**
- If there is no exact hit:
  - Units whose rectangle touches the small radius `Area` **and** whose armor on that side is ≤ 15 (`ArmorForAreaDamage`) take Damage × 0.3 (`AreaDamageCoeff`).
  - **Standing** free soldiers inside the big radius `Area2` also take ×0.3; lying soldiers only within `Area`.
  - Static objects inside `Area` are processed too.

**Destroyed vs immobilized**
- Any non-lethal hit on a tank can **break a track**: chance `BrokeTrackProbability`, or always if the shell's special flag 0 is set. Not on Easy.
- A broken track means the tank cannot move or turn its hull until an engineer repairs it (track has 20 HP of repair work).
- A massive hit (≥80% of max HP; 50% chance) plays a "fatality" (ammo explosion) death. Wrecks disappear after 60 s.
- **Tank crew casualties: I found no crew-loss model for vehicles.** `Crew` only appears as a stat (e.g. Tiger 5). Gun crews are real infantry who can be killed and replaced.

**Ammo and shell types**
- Ammo is counted per gun. Main and secondary ammo are shown separately.
- Units call for resupply when ammo drops below 1/3 (`AIUnit::Fired`).
- Guns can carry several shell types: damage (HE/AP), **smoke** (`DamageType 2`: Area = cloud radius, Piercing = transparency, DamagePower = duration), and **leaflet/morale** (`DamageType 1`: lowers morale by Damage/100 within the radius).
- Howitzers carry both a direct-fire shell and an indirect shell (see `Weapons/10_5cmlefh18.xml`).

**Morale**
- Range 0.1–1.
- Effects: dispersion ÷ (m + 0.8(1−m)); relax time ÷ (m + 0.2(1−m)); aim time ÷ m (`AimingCoeff=0`) (`Guns.cpp`; consts `Morale`).
- Per the manual, morale drops while outside depot supply range, faster without an officer (×2 per consts). It recovers back in range or near a staff car. Low morale never causes retreat, only worse performance.
- I could not find the code that ticks morale down, so this decay behavior rests on the manual plus the constants.

**Suppression**
- Soldiers lie down automatically when shots or blasts land within 160 points (5 tiles), for about 5 s (`TimeOfLyingUnderFire`). Lying gives 0.5 cover and 0.4× speed.
- Direct-fire rounds "graze" nearby units, which feeds this behavior.

**Difficulty levels**
- `consts.xml` `<Levels>` multiplies Piercing, Damage, RotateSpeed, Dispersion and Silhouette separately for friends and enemies.
- Example, Easy enemies: Damage ×0.2, Dispersion ×1.2.

### 1.4 Supply, repair, engineers
- **Currency:** everything is priced in "RU" (resource units) carried by trucks (`consts.xml` `Engineers`, `TransportAndResupply`).
  - Truck capacity 35,000 RU.
  - Reload cost is per shell, per gun (`ReloadCost`): Tiger 88 mm 409 RU, leFH18 673 RU, rifle 1 RU.
  - Repair costs `RepairCost` × 175 RU per HP.
  - Replacing one soldier costs 3,500 RU.
  - Mines: AP 1,000, AT 2,000. AT obstacle 2,000. Trench segment 1,000. Wire segment 2,000.
- **Depots:**
  - The main depot is unlimited, indestructible and capturable; capturing it effectively decides the battle.
  - Field depots can be destroyed, captured and repaired. They supply only if linked in a **chain** back to the main depot.
  - Being outside depot radius causes the morale drop.
  - A depot changes owner when only enemy units are within 300 points.
  - I did not find the depot radius values.
- **Supply truck:**
  - Ordered to an area; it then auto-serves every unit **in its sight**, with a 640-point search radius.
  - Its men carry ammo by hand (5,000 RU per trip). It drives back to a depot to reload (3 s load).
  - It also **re-mans squads and gun crews** if at least one man survives, tows guns, and carries infantry.
- **Engineering truck:** carries sappers who work in 160 ms "quanta". They can:
  - repair vehicles and buildings
  - lay AP and AT mines; clear mines (found mines get flagged and units path around them)
  - build trenches, barbed wire and AT obstacles (drawn as lines)
  - build or repair bridges, only at preset spots; this can take several RU trips
  - Lost engineers are replaced automatically while the engineering truck survives.
- **Medical truck:** heals infantry in its sight (radius 1,000 points). Large buildings heal occupants (they have rest and medical slots).
- **Entrenching:**
  - Vehicles and guns can **entrench themselves** into a tank pit: 0.5 cover, but the unit cannot move while dug in. Takes 1.5 s per tile of footprint.
  - Trenches have no HP and cannot be destroyed (manual).
  - Wire and AT obstacles can be destroyed by shells or tanks and cannot be repaired.

### 1.5 Infantry behavior
- Very vulnerable in the open. Strong in buildings and trenches. Fast only in march formation or when riding trucks or tank decks (manual; [GOG forum](https://gog.com/forum/blitzkrieg_series/basic_gameplay)).
- **Garrisoning:**
  - Squads auto-occupy **fire slots** in buildings. Each slot has a direction, firing arc, coverage, sight multiplier and an optional mounted weapon (pillboxes).
  - Occupants are protected by the building's armor (example admin house: 20–40 on the sides, 5–15 on the roof, 500 HP).
  - Everyone exits when the building falls to 10% HP.
  - Buildings can burn. Stormable entrances allow **close combat inside**: occupants fight pairwise every 200 ticks.
  - Players recommend a 2:1 ratio to storm a building.
- **Formations:** 5 slots in the UI (`USER_ACTION_FORMATION_0..4`). The rifle squad defines 4, each with multipliers for speed, dispersion, fire rate, relax time, cover and visibility, plus a lie-down flag:

| # | Speed | Dispersion | FireRate | Relax | Cover | Lie flag |
|---|---|---|---|---|---|---|
| 0 | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 | 0 |
| 1 | 1.5 | 1.5 | 1.5 | 1.5 | 1.0 | 0 |
| 2 | 1.0 | 0.7 | 0.7 | 0.7 | 0.9 | 2 |
| 3 | 1.2 | 1.2 | 1.0 | 1.0 | 0.5 | 1 |

  - The manual names march and combat formations but does not map them to these numbers.
- **Grenades:** thrown from 320 points (10 tiles) or closer, aimed at tracks and side or rear armor.
- **Snipers:** stay camouflaged even while moving. Detection chance rises 0.002 per shot and decays over time. Lost snipers cannot be replaced.
- **Officers:** binoculars (only while stationary) give a 1,600-point (50-tile) radius in a 60° cone. Without an officer, morale decays faster.

### 1.6 Artillery
- **Indirect fire:** a target can be engaged if **any friendly unit sees it**: a sniper, an officer, or a recon plane acting as spotter. Units do **not** auto-fire at targets seen only through binoculars.
- **Suppressive fire** (`ART_BOMBARDMENT`) on unseen ground is possible but less accurate.
- **Ranging** (`RANGE_AREA`, "Zero in"): 4 shots range an area of radius 716 points; dispersion is then ×0.7 inside it. `Alt+Z` shows ranged areas.
- **Counter-battery** (`AntiArtillery.cpp`; consts `Artillery`):
  - Guns with a `RevealRadius` (e.g. the leFH18's equals its range of 159) create "sound circles" visible to the enemy as orange circles on the minimap.
  - The circle starts at 6,400 points and shrinks to 384 points over 13 shots, and stays visible for 20 s.
  - Moving 160 points (5 tiles) resets the information.
- **Towing:**
  - A vehicle's `TowingForce` must be at least the gun's `Weight` (Sdkfz 7: 13,000 vs leFH18: 3,330). Heavy guns need tractors.
  - Hook and unhook take time. Guns have **Install/Uninstall** (deploy and pack) times: `UninstallRotate` and `UninstallTransport`, e.g. leFH18 1.18 s and 5.45 s, PaK 40 0.51 s and 2.84 s.
  - Crews can push light guns at 4–6 km/h.
- **Arc of fire:** turning it ("Turn arc of fire") takes time, and the gun does not fire while turning.
- **Ranges** (tiles): rifle and MG 30, 88 mm 35, mortar 120 mm 91, leFH18 159.

### 1.7 Vision
- **Tile fog of war, shared per side** (`AIWarFog.cpp`).
  - Sight radius = (`Sight` × multiplier × rank bonus + floor(height/10)) tiles. Height adds +1 tile per 10 z.
  - Objects carry per-tile **visibility (absorption) grids** that block sight; units have a sight "power".
  - Vision can be cone-limited (binoculars, building slots).
  - Enemies are forgotten 30 s after leaving view (AA guns after 300 s).
- **Camouflage:**
  - After 5 s idle, units with a `Camouflage` stat (rifleman 40, PaK 40 40, truck 20, Tiger 0) become harder to detect: the detection distance shrinks.
  - **Firing reveals** the unit. Per class, `RevealInfo` sets the reveal probability, re-hide time and the distance to move to hide again (light arm 0.1, super-heavy 0.5).
- **Recon:** recon aircraft, snipers, officers, armored cars. The manual stresses recon before advancing.
- **Line of fire:** I did not verify whether direct fire ray-traces against terrain or buildings beyond the fog-visibility requirement.

### 1.8 Terrain
- **Speed formula:** max speed × (terrain passability + (1 − terrain passability) × unit passability) (`CommonUnit.cpp` `GetMaxSpeedHere`).
  - `Speed` is the road speed. Roads and asphalt have passability 1, so units reach full speed there.
  - Terrain values (`Terrain/sets/1/tileset.xml`): grass and plain 0.8, forest 0.7, sand 0.6, dark ground 0.5, garden 0.3, swamp impassable.
  - Unit passability: soldier 1.0, Tiger 0.49, Sdkfz 7 0.04, truck −0.18 (wheeled vehicles crawl off-road).
  - Each terrain type also has `CanEntrench` and per-AI-class blocking (wheel, halftrack, track, human).
- **Movement types:** impassable (rivers, swamps); infantry-only (thick forest, steep hills); slow (ploughed or snowy fields); normal; roads (manual).
- **Forests** block sight and are good ambush terrain. Tanks and artillery can flatten them.
- **Destructibility:** almost everything can be destroyed, including buildings and **bridges**. Units on a bridge when it falls die.
- **Elevation** increases sight; hollows reduce it.
- **Seasons and climate:** summer, winter, spring, Africa.
- **Weather:** random spells (every 840 ± 200 s, lasting 50 ± 20 s). Fire range ×0.9 and aircraft cannot fly.
- Rivers are crossed only by bridges or by bridges built by engineers.

### 1.9 Controls and UX (manual "Combat screen", "Attachments")
- **Selection:** click; drag box (box-selecting any squad member selects the whole squad); Shift-click to add; double-click selects all of that model.
  - `Ctrl+0-9` assigns a group, `0-9` recalls it, `Alt+0-9` centers on it.
- **Orders:**
  - Right-click issues the context default. `Ctrl+RMB` forces attack; `Alt+RMB` forces move.
  - **Shift queues** commands.
  - **You can pause and still give orders.** Game speed has ±10 steps.
  - `Alt+R` shows ranges and arcs.
- **Command panel:** 12 buttons, some opening sub-menus (shell type, formation). It is mirrored to a **QWER / ASDF / ZXCV** hotkey grid. The top rows keep the same positions for infantry and tanks so mixed groups work.
  - Mixed groups only get commands all members share.
  - User commands (`Common/Actions.h`): Move, Attack, Swarm (aggressive move), Stop, Stand ground (no auto-engage), Guard, Ambush, Follow, Rotate (turn arc), Enter/Leave, Board, Hook/Deploy artillery, Install/Uninstall, Ranging, Suppress, Change shell (damage/smoke/leaflet), Formation 0–4, Disband/Form squad, Entrench self, Binoculars, Capture artillery, Engineer build trench/wire/AT obstacle/bridge, mines AP/AT, clear mines, repair unit/building, Resupply, Human resupply, Fill RU, Call scout/bomber/fighter/attack/paratroopers, Place marker.
  - Each unit's XML has a `Commands` bitfield listing which of these it supports.
- **HUD:**
  - Minimap: fog shown as brightness, objective arrows, artillery sound circles, and the map-edge zone aircraft come from.
  - Info panel: rank, commander name, 4 armor values (front/side/rear/top), main weapon Damage and Piercing.
  - **4 bars:** HP, main ammo, secondary ammo, morale.
  - Icons above units: low ammo (yellow/red), out of supply (house icon), rank ticks, HP bar colored by state.
- **Air support button:** shows a cooldown; choose the plane type, then target.
- **Other:** objectives on Tab; message log with colors; `~` console; F5/F8 quicksave/quickload.

### 1.10 Modding (what the data layer exposes)
- **One folder per object:** `1.xml` (stats), `1.mod` (model), `.dds` textures (with winter/Africa variants), icons, `name.txt`, `desc.txt`.
  - Objects are registered in `Data/objects.xml` (name, type, game_type, path).
  - Packed as `.pak`, which are plain zip files ([HOWTO guide on Scribd](https://www.scribd.com/document/518924214/Blitzkrieg-HOWTO-Tutorial)).
- **Mods:** declared by `mod.xml` (MODName, Version, Desc) plus `modobjects.xml` for added objects. On Steam the mods folder sits under `Run/mods/<mod>/data` ([Steam thread](https://steamcommunity.com/app/313480/discussions/0/1291817837629057185); I only saw this in a search summary, not on the page).
- **Global data files:** `consts.xml` (all tuning constants), `partys.xml` (nations), `Units/AIPrices.xml`, per-type experience-level tables (the code loads them via `GetExpLevels(type)`; I did not locate the file).
- **Weapons** are separate shared XML files referenced by name from units. **Squads** list members plus formation layouts. **Buildings** carry passability and visibility grids, fire slots and entrances.
- **Tools shipped with the game:** `mapeditor.exe` and `reseditor.exe` (with `reshelp.chm`) in `Versions/Current`. Missions are XML with Lua scripts. Community Lua references exist ([Scribd LuaScriptingGuide](https://www.scribd.com/document/518924376/LuaScriptingGuide)) and reinforcement mods exist ([ModDB](https://www.moddb.com/mods/geralds-reinforcement-mod)).
- **Built-in AI opponent:** called the "General" in the code, tuned in `consts.xml`. It launches swarm counterattacks with mobile reserves, fires artillery at weighted map cells, sends bombers, manages resupply and recaptures depots.

---

## 2. Suggested tiering

**MVP: the smallest version that still feels like Blitzkrieg**
- **One hand-built scenario map:** fixed starting forces, 2–3 objectives (capture area, destroy target, hold), and one triggered reinforcement. Win/lose come from data-driven triggers. No base building.
- **Units:** rifle squad (rifle, MG, AT grenade; officer optional), light tank, heavy tank, towed AT gun and howitzer (start pre-deployed; skip towing), supply truck, main depot.
- **Combat:** the full core model from 1.3:
  - linear dispersion ellipse plus geometric hit test with a shrunk hitbox
  - per-side min/max armor (4 sides plus top)
  - piercing ± random against random armor
  - damage ± random
  - HE splash with the ≤15 armor threshold
  - aim, burst and relax cycle; no main gun on the move
  - per-gun ammo counts
  - track breaks (cheap to add, very characteristic)
- **Supply:** the truck refills ammo for units in range from a depot, using RU costs. Without this, limited ammo is just punishment.
- **Vision:**
  - tile fog shared per side, sight radius plus a height bonus
  - forests and buildings block sight
  - idle camouflage, revealed by firing
  - howitzers fire at anything any friendly unit can see (spotting)
- **Terrain:** the passability speed formula, roads, impassable river with bridges, forests.
- **Buildings:** garrisonable via slots, destructible with HP, occupants exit at 10% HP.
- **Infantry:** lying (auto-prone under fire, 0.5 cover) and two formations (march, combat).
- **Commands:** move, attack, swarm, stop, stand ground, rotate (turn arc), enter/leave, load/unload truck, resupply, lie down.
- **UX:** box/double-click/Shift selection, control groups, Shift queues, pause-and-order, game speed, minimap with fog, info panel (4 armor values plus Damage/Piercing), 4 status bars, a hotkey grid.
- **Data:** every unit, weapon, squad and the constants live in external files (RON or JSON), loaded at startup with the schema in section 3.

**v2: depth**
- Engineers: mines AP/AT and clearing, wire, trenches, AT obstacles, repairing units and buildings, rebuilding bridges and building them at preset spots; tank pits (entrench self).
- Towing (TowingForce vs Weight), hook/unhook, install/uninstall times; gun crews as real squads; capturing abandoned guns; re-manning squads from trucks.
- Morale: depot supply radius and depot chain, staff car, officer effect, leaflet shells; smoke shells; medical truck; healing inside buildings.
- Air support: scout, bomber, attack, fighter, paratroopers, with cooldowns, map-edge entry and AA guns (`Ceiling`).
- Artillery: counter-battery sound circles, ranging, suppressive fire on unseen ground.
- Snipers and binoculars (cone vision); building storming (interior close combat); weather.
- Core units with experience and 4 ranks; per-type experience multipliers; post-mission replacement at one rank lower.
- An AI opponent beyond scripted defense: the "General" behaviors (swarm reserves, artillery on weighted cells, recapturing depots).

**Later**
- Campaign layer: chapters, Lua chapter scripts, slots, depot and bonus upgrades, player rank, medals, statistics.
- Random missions from templates plus weighted unit tables, by season and region.
- Multiplayer (Flagcontrol, Sabotage); trains; seasons, winter and desert variants; difficulty multiplier tables.
- Mission scripting API (Lua, or Rhai for Rust); map editor and resource editor; mod folders and a mod registry.
- Encyclopedia, replays, fatality animations, ambient sound systems.

---

## 3. What stats define a unit or weapon (schema hints)

Taken from `RPGStats.h` and the sample XML files.

**Weapon** (`Data/Weapons/*.xml`)
- `Dispersion` (tiles, at max range)
- `AimingTime` (s)
- `AmmoPerBurst`
- `RangeMax`, `RangeMin` (tiles)
- `Ceiling` (AA altitude; 0 means ground only)
- `DeltaAngle` (aim slack before the gun must rotate)
- `RevealRadius` (counter-battery signature)
- `Shells[]`, each with:
  - `TrajectoryType`: LINE, HOWITZER, BOMB, CANNON, ROCKET, GRENADE
  - `DamageType`: HEALTH, MORALE, FOG
  - `Piercing`, `PiercingRandom`
  - `DamagePower`, `DamageRandom`
  - `Area` (small radius), `Area2` (large radius; 0 means no splash)
  - `ProjectileSpeed`, `FireRate` (rpm), `RelaxTime` (s)
  - `BrokeTrackProbability`
  - `Specials` bitset (bit 0 = always breaks tracks)
  - Visual fields: trace probability and speed, detonation shake, effects per hit outcome (direct, miss, reflect, ground, water, air), craters, fire flash.

**Unit base** (all units)
- `MaxHP`, `RepairCost` (RU per HP), `DamagedHPs` (thresholds for damaged visual states)
- `Sight` (tiles), `SightPower`
- `Speed` (road), `RotateSpeed`, `Passability` (can be negative), `Priority` (who yields right of way)
- `Camouflage` (0–100)
- `Weight`, `Price` (AI value and experience award)
- `Type` (the enum above), `AIClass` (wheel, halftrack, track, human)
- `Commands` bitset
- Hitboxes: AABB center and half-size, `SmallAABBCoeff`, per-animation boxes
- Acknowledgement sound sets

**Mechanical units (`Mech`) add**
- Armor per side: `ArmorFront`, `ArmorLeft`, `ArmorRight`, `ArmorBack`, `ArmorTop`, `ArmorBottom`, each `{Min, Max}`
- `Platforms[]` (hull and turret): horizontal and vertical rotation speed, angle limits, which guns they carry
- `Guns[]`: weapon reference, `Ammo`, `Priority`, primary flag, `Direction`, `ReloadCost` (RU per shell), shoot point, recoil
- `TowingForce`, `Crew`, `Passangers`, `TurnRadius`
- `UninstallRotate`, `UninstallTransport` (deploy/pack times)
- Tow, entrance and people points; aircraft fields: `MaxHeight`, dive, climb and tilt angles
- Visual effects, tracks and sounds

**Infantry (`Infa`) adds**
- `Guns[]` (main plus grenade), `RunSpeed`, `CrawlSpeed`, `CanAttackUp`/`CanAttackDown`, a single armor value (0)

**Squad**
- `Members[]` (unit references)
- `Formations[]`: type, LieFlag, Speed/Dispersion/FireRate/RelaxTime/Cover/Visible bonuses, per-member slot positions

**Building**
- `MaxHP`; armor min/max plus silhouette for 6 sides; `Burn`; `RestSlots`, `MedicalSlots`
- `FireSlots[]`: position, direction, angle, coverage, sight multiplier, optional mounted gun and ammo
- `Entrances[]` (stormable flag)
- Passability and visibility grids
- `BuildingType`: house, main depot, temporary depot, pillbox

**Experience level, per unit type**
- `Exp` threshold, `BonusSpeed`, `BonusRelaxTime`, `BonusDispersion`, `BonusSight`, `BonusFireRange`, `BonusRotate`

**Global constants** (`consts.xml`)
- Cover values, splash coefficient and armor cap, dispersion ratios by trajectory, morale coefficients
- Supply and engineering RU prices and rates, artillery ranging and counter-battery parameters
- Camouflage timers and per-class reveal table, weather, flags
- Difficulty multipliers; AI "General" tuning

**Nation** (`partys.xml`)
- Gun-crew squad, howitzer crew, HMG squad, paratrooper unit, resupply-engineer squad

---

## Gaps (sources thin or not located)
- **Vehicle crew casualties:** no model found; `Crew` appears to be just a stat.
- **Morale decay:** the code that ticks morale down for units outside supply was not located. The behavior described comes from the manual plus the `Morale` constants.
- **Depot supply radius:** values not found.
- **Experience-level data file:** not located, so the per-rank multiplier numbers are unknown.
- **Direct-fire line of fire:** not verified whether shots are blocked by terrain or buildings.
- **Formation names:** the mapping of formation slots 0–4 to their UI names is unconfirmed.
- **Unit conversions:** sight conversion is ×1 in tiles; the road-speed unit conversion was not fully traced.
- **Unavailable pages:** GameSpot and TV Tropes returned 403. I found no GameFAQs guide for Blitzkrieg 1; the manual and source replace it.
