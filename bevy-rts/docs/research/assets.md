# Free asset research for the WWII isometric RTS (Bevy, glTF)

I didn't download anything. I checked every license on the asset's own page (or through the Sketchfab API) unless a row says "unverified".

## Notes that apply to everything

- **Bevy only loads glTF/GLB natively, animations included.** FBX and OBJ files need a one-time conversion in Blender. Third-party crates exist, but the OBJ ones handle static meshes only.
- **Batch conversion:** `blender -b -P convert_to_glb.py -- in/ out/` converts a whole folder.
- **Formats differ by download site.**
  - Poly Pizza offers glTF for most models, including Quaternius models that are FBX-only on quaternius.com.
  - Sketchfab offers auto-converted glTF/GLB for every downloadable model.
  - Freesound and Sketchfab both need a free login to download.
- **Poly Pizza license labels can be wrong.** Quaternius's "Soldier" (https://poly.pizza/m/oAArCNHjFB) is labelled CC-BY 3.0 there, but quaternius.com says everything is CC0. Download from the original site, or follow whatever label is on the copy you take.
- **Poly counts.** For an RTS showing 100+ units at once, aim for about 3k triangles or fewer per vehicle and 2k or fewer per soldier. Many Sketchfab models tagged "low poly" are 20k–800k faces; I've left most of those out.
- **What CC-BY requires (Sketchfab is CC BY 4.0; Poly Pizza and game-icons are CC BY 3.0):**
  - Credit the author and title.
  - Link to the source and to the license.
  - Say if you modified the asset.
  - A credits screen plus a CREDITS file in the shipped build covers all of this.

## 1. Vehicles

### CC0 (no obligations)

| Asset | URL | Formats | Style / polys | Notes |
|---|---|---|---|---|
| Quaternius Animated Tanks Pack (4 tanks) | https://quaternius.com/packs/animatedtanks.html | FBX/OBJ/Blend (glTF copies on Poly Pizza, e.g. https://poly.pizza/m/FA5daiyZQq, /m/cW3zvvkMOM, /m/Dc4k4CooN3, /m/jWS1CLA0RO, /m/uYHpj7lz1J, /m/j59k7ctnZM) | Stylized and animated; polys not stated | Generic tanks, not WWII-specific (not checked visually). Same style as the other Quaternius packs. |
| OGA "Heavy Tank [extra low poly, RTS]" by 32kda | https://opengameart.org/content/heavy-tankextra-low-poly-rts | .blend + **glb** | About 1,100 faces, 3-colour texture | Generic, Command & Conquer style. Author asks for credit but it's optional. |
| OGA "Lowpoly soviet jeep" (GAZ-67, WWII) by artie31 | https://opengameart.org/content/lowpoly-soviet-jeep | Zip (formats not listed) | Low poly | Genuinely WWII. |
| OGA "T-12 soviet light tank lowpoly" by artie31 | https://opengameart.org/content/t-12-soviet-light-tank-lowpoly | Zip (formats not listed) | Low poly | Same author as the jeep, so the style matches. |
| OGA "soviet military off-road vehicle" by artie31 | https://opengameart.org/content/soviet-military-off-road-vehicle | Zip | Low poly | Listed in OGA's CC0 collection; I didn't open the page itself. |
| OGA "Low poly tank assets" by danitorres567 | https://opengameart.org/content/low-poly-tank-assets-0 | OBJ/FBX/Blend/3DS/STL | Toy-like, 4 colours | Generic. |
| OGA "N64 Style Tank Models" by CptDrunkBear | https://opengameart.org/content/n64-style-tank-models-and-textures | Zip | Retro style | Generic. |
| Kenney Car Kit (45 files) | https://kenney.nl/assets/car-kit | Recent Kenney 3D kits usually include GLB/FBX/OBJ; check the zip | Kenney flat style | Civilian trucks could stand in as supply trucks. |
| "Low Poly German WWII Tanks" by Newc42 (Leichttraktor, Pz III J, Tiger) | https://newc-42.itch.io/german-low-poly-wwii-tanks | .blend/.fbx | Low poly | **Unverified.** A search snippet says CC0, but itch.io blocked automated access behind a Cloudflare bot check, which I didn't bypass. Open it manually. |

### CC-BY (needs credit). Best WWII-specific picks, all on Sketchfab with glTF/GLB downloads

| Asset | URL | Faces | Notes |
|---|---|---|---|
| **panzer III low-poly tank**, alex-toma | https://sketchfab.com/models/2d233db134ec41b29b66180c91d62245 | 830 | Separate turret, gun and hull. Palette PSD included. |
| **t-34 low-poly tank**, alex-toma | https://sketchfab.com/models/fc128adffa18499f85205a6743d1e671 | 925 | Same author, same flat palette style. |
| alex-toma Low poly tree pack / rock pack / bridge | https://sketchfab.com/models/a9b21dc12f504675bd202359e6d37b1a, /d6538804ad894a778739e8343abfdf53, /935237314c954f188e3d0c90719368b5 | 2,683 / 1,610 / 709 | Matches the tanks. This is the most consistent WWII set I found. |
| Panzer 4 D, adrielcz | https://sketchfab.com/models/055f66879bb54e0ba77ad00a61798496 | 1,308 | |
| Panzer 3 Ausf.E, arthur80cristian | https://sketchfab.com/models/4a1e1e6317f244899cc29748723282e5 | 1,488 | |
| Panzerkampfwagen II, wersaus33 | https://sketchfab.com/models/d6c86eeefe3a457caf1825df106c99fc | 1,866 | |
| PS1 low poly Tiger 1, Blueberry12 | https://sketchfab.com/models/e75321d69fd44411a891dcf3a816390a | 886 | |
| Low Poly Sherman, Joris-Tackaert | https://sketchfab.com/models/1daf3b553bd641ed8e621aae325ae575 | 8,634 | |
| m4 sherman low poly, leoxx300 (also has a Panzer III at 2,418) | https://sketchfab.com/models/6deccb9bd148470a817d6983c298353c | 11,032 | |
| Low-poly T-34 and T-34-85, vladrompus | https://sketchfab.com/models/ba0e646348234b62a3ae0c7ce88eb280, /add429640d9a4747960f899c142c5fe7 | about 7.4k each | |
| LowPoly T-34 (Blockbench, pixel-art style), MagmaStudios | https://sketchfab.com/models/1afd685d64cc4baba6ec03c8b6f2cb0a | 476 | |
| [FREE] Low Poly Opel Blitz (truck), brzozaaa | https://sketchfab.com/models/74f021f18d9045fba476372bc9e18081 | 5,241 | Author asks for credit. |
| Low Poly Austin Kegresse (halftrack), Rysky | https://sketchfab.com/models/a3f0dfb1452442bfb07550b3a96d87eb | 288 | |
| Tactical Halftrack, S1Priv | https://sketchfab.com/models/dcfffd3fcc9d47988273940df64390c0 | 2,058 | |
| 37mm AT Gun M3 Low poly, Ronald_Reagan | https://sketchfab.com/models/01a86079cf9a4e6a8f6d042fe89a43ff | 1,106 | |
| FLAK pack (low poly, made for a Hearts of Iron IV mod), vonBerlichingen | https://sketchfab.com/models/1056ea79c3154aadb775e958924fd4c3 | 14,434 (whole pack) | |
| Bf-109 K4 and Me-309 (low poly), vonBerlichingen | https://sketchfab.com/models/53ec8c75306340d49d319a65a2e733a1, /f2eafa09a4364832b6d9a3c67ac5d866 | 1,886 / 1,266 | |
| A6M Zero Low Poly, Waroxed | https://sketchfab.com/models/38311e82157449448f703b53648614a5 | 821 | |
| F4U Corsair Low Poly, wersaus33 | https://sketchfab.com/models/cb11ebb220454ae1b72d913a5bd9b1d0 | 596 | |
| Low Poly Stuka (Ju-87), egedemircan50 | https://sketchfab.com/models/8c7fd2de48d9432585090270bcdf0fa2 | 4,564 | |
| Low Poly Messerschmitt BF-109, DLM96 | https://sketchfab.com/models/d6827fdbdbc447969523d9d22f60a396 | 2,758 | |
| KolosStudios: military truck / tank / fuel truck / barrier | https://sketchfab.com/models/9fd961a22cb84b9383570869b2f0efa9, /308346f4741a48018c93ebe6f8e53905, /ea3594f118d64282b759000e66b5695d, /839aec4819f74240afa84f232e2cff28 | 696 / 760 / 780 / 1,456 | Generic modern vehicles. Also on Poly Pizza as CC-BY 3.0. |

These Sketchfab models come from many authors, so their styles won't match. The fix is in section 7.

## 2. Infantry

| Asset | URL | License | Formats | Notes |
|---|---|---|---|---|
| **Quaternius Toon Shooter Game Kit** (74 models, characters with 17 animations, enemies, environment) | https://quaternius.com/packs/toonshootergamekit.html | CC0 | FBX/OBJ/Blend/**glTF** | Best CC0 option. "Character Soldier" is also on Poly Pizza at https://poly.pizza/m/PpLF4rt4ah. The individual animation names aren't published (itch.io blocked automated access), so check for idle/walk/shoot/death in the download. Cartoony, not WWII-accurate. |
| **KayKit Character Animations** (133 clips: idle, death, walk/run, shoot/aim/reload) | https://kaylousberg.com/game-assets/character-animations | CC0 | FBX/**glTF** | Retargetable to any humanoid. Pair it with your own or another CC0 soldier mesh. |
| Quaternius Universal Animation Library (120+ clips incl. death, combat and gun actions) plus Universal Base Characters | https://quaternius.com/packs/universalanimationlibrary.html, https://quaternius.com/packs/universalbasecharacters.html | CC0 | FBX/glTF/OBJ | Only the "Standard" tier (60–70% of the pack) is free. Base characters average about 13k triangles, which is too heavy for massed infantry. |
| Quaternius Ultimate Animated Character Pack (52 characters) | https://quaternius.com/packs/ultimatedanimatedcharacter.html | CC0 | FBX/OBJ/Blend | Needs Blender conversion. No soldier mentioned. |
| Kenney Animated Characters (Survivors / Protagonists), Mini Characters (25 files, animated), Blocky Characters | https://kenney.nl/assets/animated-characters-survivors, /mini-characters, /blocky-characters | CC0 | The older Animated Characters packs are, as far as I know, FBX-based; check the zips | No weapons confirmed. |
| OGA "Rigged Lowpoly WW2 Soldier" (German) by nisu | https://opengameart.org/content/rigged-lowpoly-ww2-soldier | CC0 | Zip (format not listed) | Rigged, no animations mentioned. Pair it with the KayKit or Quaternius animations. The author's linked site, 3dmodelscc0.com, now redirects to an unrelated domain, so ignore it. |
| Sketchfab "Low Poly Soldier -Free", manoeldarochadeoliveira | https://sketchfab.com/models/739772a9b0f14feeb4a4f1b45dadf4e3 | CC-BY 4.0 (description says no credit needed; credit anyway) | glTF | 1,432 faces, rigged, 5 animations, toon-shaded. |
| Sketchfab "Low Poly WW2 Soldier", Beaine33 | https://sketchfab.com/models/045513909a684d259d59f2a1217f45d9 | CC-BY | glTF | 1,476 faces, not animated. Author asks for credit. |
| Sketchfab "WW2 Wehrmacht - Low Poly", aBraM_ | https://sketchfab.com/models/c98707faf80e413d9001bd61f56558d6 | CC-BY | glTF | 2,428 faces. |
| vonBerlichingen Gebirgsjäger and Volkssturmmann (low poly) | https://sketchfab.com/models/98ede7a312aa45d78ca7508a247474b5, /4af4b7f64eb8415a83be6b341b214b39 | CC-BY | glTF | About 6.5–6.9k faces. |

## 3. Environment

**Models**

| Asset | URL | License | Formats | Notes |
|---|---|---|---|---|
| Kenney Nature Kit (330 files: trees, rocks, foliage) | https://kenney.nl/assets/nature-kit | CC0 | See the general Kenney note | I believe it also has fences, bridges and tents, but the page only tags trees, rocks and foliage, so check the zip. |
| Kenney Survival Kit, Retro Urban Kit, City Kit (Suburban), Modular Buildings, Building Kit, Fantasy Town Kit | https://kenney.nl/assets/survival-kit, /retro-urban-kit, /city-kit-suburban, /modular-buildings, /building-kit, /fantasy-town-kit | CC0 | | Fantasy Town and Retro Urban suit European villages and towns. All share the Kenney style. |
| Quaternius Stylized Nature MegaKit (116 models: 40 trees, 27 rocks, grass) | https://quaternius.com/packs/stylizednaturemegakit.html | CC0 | glTF included | Free tier is about 60–70% of the pack. |
| Quaternius Ultimate Nature Pack (150 models) | https://quaternius.com/packs/ultimatenature.html | CC0 | FBX/OBJ/Blend | Needs conversion. |
| Quaternius Medieval Village MegaKit (300+ grid-snapping walls and roofs) | https://quaternius.com/packs/medievalvillagemegakit.html | CC0 | glTF | Free tier is 60–70%. Half-timbered houses pass for Normandy or German villages. |
| Quaternius Farm Buildings (13) | https://quaternius.com/packs/farmbuildings.html | CC0 | FBX/OBJ/Blend | Needs conversion. |
| Quaternius Ultimate Modular Ruins (90) | https://quaternius.com/packs/ultimatemodularruins.html | CC0 | FBX/OBJ/Blend | More dungeon than WWII rubble. |
| KayKit Forest Nature Pack (100+ free models) | https://kaylousberg.com/game-assets/forest-nature-pack | CC0 | FBX/**glTF**/OBJ | |
| OGA "Destroyed City Assets" by Fleurman (3 ruined buildings, roads, rocks) | https://opengameart.org/content/destroyed-city-assets | CC0 | Zip | Author says "quite not low-poly". |
| Sketchfab "Battlefield Pack", narighillya (trench, bunker, Czech hedgehogs, sandbags, shells) | https://sketchfab.com/models/dcd0ade8c80e46d982a54fe4619f1c87 | CC-BY | glTF | About 28k faces for the whole pack. |
| Sketchfab "Sandbag [Low Poly Realist]", Islide | https://sketchfab.com/models/7d52600a15c747749d845d9f906045cf | CC-BY | glTF | 712 faces. |
| Sketchfab "WW2 Field Bunker", goldenblack4 | https://sketchfab.com/models/6ea5cebdaa82455aa3aad26baa6e0375 | CC-BY | glTF | 286 faces. |
| Sketchfab "Ruin Building - Low Poly", thegobby | https://sketchfab.com/models/b6a7905251f54a419232ae44c5117517 | CC-BY | glTF | 238 faces. |
| Sketchfab "ruined house low poly", ribot02 | https://sketchfab.com/models/9fdc8d1f23a249768c377db0dfcaf42c | CC-BY | glTF | 1,776 faces. |

**Terrain textures and skies** (all CC0, no credit needed)

- **ambientCG** (license: https://docs.ambientcg.com/license/):
  - Grass: Grass001–008
  - Dirt: Ground037, Ground054, Ground003
  - Mud: Ground109, Ground106
  - Snow: Snow006, Snow010A, Snow015
- **Poly Haven** (license: https://polyhaven.com/license; IDs checked through its API):
  - Textures: `brown_mud`, `snow_02`, `forrest_ground_01`, `dry_ground_01`, `sparse_grass`, `aerial_grass_rock`, `rocky_terrain_02`
  - Sky HDRI: `kloofendal_48d_partly_cloudy_puresky`
  - To use an HDRI as Bevy environment lighting, convert it to KTX2 cubemaps first.
- **Style warning:** photo-real textures will clash with flat low-poly models. Tint or blur them, or use vertex-colour or splat terrain with only light texture detail.

## 4. Effects and sounds

**Particles**
- Kenney Particle Pack (80 files) and Kenney Smoke Particles (70 files, tagged smoke/explosion), both CC0: https://kenney.nl/assets/particle-pack, https://kenney.nl/assets/smoke-particles

**Sounds from Freesound** (found with the CC0 filter; I opened three pages individually to confirm)

- Engines:
  - "tank_engine.wav", monosfera, 1:19, confirmed CC0: https://freesound.org/people/monosfera/sounds/572294/
  - "Driving Tank Engine", qubodup, confirmed CC0 (US-government source): https://freesound.org/people/qubodup/sounds/187676/
  - "Military Vehicle Humming": https://freesound.org/people/qubodup/sounds/182793/
  - Truck/APC diesel, kyles: https://freesound.org/people/kyles/sounds/405086/
  - t26_moving_01: https://freesound.org/people/greatmganga/sounds/329800/
- Explosions:
  - "Explosion_01", tommccann, confirmed CC0: https://freesound.org/people/tommccann/sounds/235968/
  - Skip "bad explosion": its uploader asks for it to be removed.
- Gunfire:
  - "Heavy Machine Gun", ShawnyBoy: https://freesound.org/people/ShawnyBoy/sounds/165394/
  - pgi machine-gun loop and triple shot: https://freesound.org/people/pgi/sounds/212602/, /212606/
  - "Gunfire", qubodup: https://freesound.org/people/qubodup/sounds/205588/
  - "Battle, war zone gun fight", alanmcki: https://freesound.org/people/alanmcki/sounds/326442/
- Artillery:
  - "Artillery Gunfire": https://freesound.org/people/qubodup/sounds/239137/
  - "K-55 Artillery": https://freesound.org/people/qubodup/sounds/184365/
  - "Heavy Artillery Shot": https://freesound.org/people/LilMati/sounds/441373/
  - "Incoming mortar 1": https://freesound.org/people/Zagge28/sounds/241840/
- Packs:
  - qubodup "military sounds" (US-government public-domain media): https://freesound.org/people/qubodup/packs/4366/
  - qubodup "military voice samples", useful for unit acknowledgements: https://freesound.org/people/qubodup/packs/13895/
  - qubodup has both CC0 and CC-BY uploads, so check the license on each sound.

**Other sound sources**
- Kenney Impact Sounds, Interface Sounds, UI Audio and Voiceover Pack (all CC0): https://kenney.nl/assets/impact-sounds, /interface-sounds, /ui-audio, /voiceover-pack
- OGA "engine loop heavy vehicle/tank" by Nayckron (dual CC-BY 3.0 / GPL 3.0; take the CC-BY option and credit Nayckron). WAV/MP3/OGG, base loop plus 3 speeds: https://opengameart.org/content/engine-loop-heavy-vehicletank
- OGA "100 cc0 sfx": https://opengameart.org/content/100-cc0-sfx

## 5. UI

**Kenney packs (all CC0)**
- UI Pack (430 files): https://kenney.nl/assets/ui-pack
- UI Pack Sci-fi: https://kenney.nl/assets/ui-pack-sci-fi
- UI Pack RPG Expansion: https://kenney.nl/assets/ui-pack-rpg-expansion
- Cursor Pack (180 files, RTS cursors): https://kenney.nl/assets/cursor-pack
- Crosshair Pack: https://kenney.nl/assets/crosshair-pack
- Board Game Icons: https://kenney.nl/assets/board-game-icons
- Top-down Tanks (2D, minimap or portrait stand-ins): https://kenney.nl/assets/top-down-tanks
- Kenney Fonts: https://kenney.nl/assets/kenney-fonts

**game-icons.net** (CC BY 3.0, credit each icon's author; site suggests "Icons made by {author}. Available on https://game-icons.net")
- SVG source repo: https://github.com/game-icons/icons
- Icon pages follow https://game-icons.net/1x1/{author}/{name}.html
- Command-panel icons confirmed in the repo:
  - Units: lorc/battle-tank, lorc/tank, cathelineau/great-war-tank, skoll/jeep, delapouite/truck, skoll/machine-gun, sbed/rifle, quoting/biplane, lord-berandas/bomber
  - Buildings and positions: quoting/bunker
  - Commands: skoll/halt, delapouite/move, delapouite/crosshair, quoting/bunker-assault, quoting/trench-assault, lorc/bombing-run
  - Resources and support: quoting/artillery-shell, sbed/ammo-box, lorc/auto-repair
  - Defences and gear: lorc/barbed-wire, lorc/land-mine, delapouite/binoculars, lorc/parachute
  - Ranks: skoll/rank-1, skoll/rank-2, skoll/rank-3

**Fonts** (OFL licence confirmed by their location in the Google Fonts repo's `ofl/` folder)
- Military stencil: Black Ops One, Stardos Stencil, Allerta Stencil, Saira Stencil One, Big Shoulders Stencil
- Body text: Oswald, Armata, Russo One, Staatliches, Share Tech Mono, Courier Prime
- Special Elite (typewriter look) is Apache 2.0, which is also fine.
- OFL conditions: ship the licence text with the font files, and don't sell the font on its own.

## 6. Excluded

- **Non-commercial licences:**
  - Sketchfab "Plane" by predator10597, "Low Poly War Hawk" by Fikz3D and "BF 109" by GRIP420 (all CC-BY-NC)
  - leondp's towers (CC-BY-NC-ND)
- **Not a CC licence:** Sketchfab "Free Standard" items, e.g. "Military Sandbag Barrier" and the RB M57 launcher.
- **Share-alike:** OGA "American Tank" (GPL / CC-BY-SA).
- **No licence stated:**
  - IsolatedSoulStudio "Lowpoly Tank Pack 01" (Panzer IV, Sherman, T-34 and others; FBX/DAE)
  - Coynese "low poly ww2 tank"
  - widgeon "3d Low Poly Military Vehicles"
  - tarasov3d "tanks ussr wwii lowpoly pack"
- **Disputed:** OGA "explosion animations" by Jetrel (a commenter says the art is actually CC-BY).
- **Ambiguous label:** Sketchfab "WW2 headwear (CC0)" is titled CC0 but licensed CC-BY; treat it as CC-BY.
- **Paid:** all Unity Asset Store results shown on Poly Pizza.

## 7. Recommended starter set (one style, fewest obligations)

**Style:** flat-coloured, cartoony low poly.

**CC0 core (no attribution needed)**
- Infantry: Quaternius Toon Shooter Game Kit (glTF, animated). Add KayKit Character Animations if you need more clips.
- Armour: Quaternius Animated Tanks (glTF from Poly Pizza) as generic medium and heavy tanks. Add the OGA artie31 GAZ-67 jeep and T-12, and the 32kda Heavy Tank (GLB).
- Trucks and civilian vehicles: Kenney Car Kit.
- Nature: pick either Kenney Nature Kit or Quaternius Stylized Nature MegaKit, not both, so the style stays consistent.
- Villages: Kenney Fantasy Town Kit or Quaternius Medieval Village MegaKit, plus Quaternius Farm Buildings.
- Terrain: ambientCG / Poly Haven, toned down to fit the flat style.
- FX: Kenney particle packs.
- Sound: the Freesound CC0 picks above plus the Kenney audio packs.
- UI: Kenney UI Pack and Cursor Pack. Fonts: Black Ops One and Oswald (OFL).

**Small CC-BY add-on for WWII identity (about 5 credit lines)**
- alex-toma: Panzer III, T-34, trees, rocks, bridge (one author)
- brzozaaa: Opel Blitz
- Ronald_Reagan: 37mm AT gun
- vonBerlichingen: Bf-109 (plus the FLAK pack if you want it)
- game-icons.net: command icons (about 5 author credits: lorc, delapouite, skoll, quoting, sbed)

**Making mixed authors look like one set**
- Ignore the source materials. When a scene finishes loading, swap in your own faction-palette materials with flat shading in Bevy.
- Alternatively, re-texture everything in Blender to one shared palette PNG.
- Rescale everything to one unit scale at the same time.
- Keep `assets/CREDITS` (or `credits.ron`) as the single source for the in-game credits screen.

## 8. Placeholders and data-driven asset paths

**Primitive placeholders** (built-in Bevy shapes plus a plain material in a faction colour)
- Tank: Cuboid hull, smaller Cuboid turret, Cylinder barrel.
- Truck or halftrack: two Cuboids.
- Infantry: Capsule about 0.3 m radius, 1.2 m tall.
- AT gun: Cuboid with a Cylinder barrel.
- Aircraft: crossed Cuboids flying at altitude.
- Trees: Cone on a Cylinder. Buildings: Cuboids. Sandbags: a flattened Cuboid.
- Selection ring: Annulus. Terrain: a Plane mesh with vertex colours.
- Faction colours: German field grey about (0.38, 0.40, 0.36), Soviet olive (0.33, 0.38, 0.22), US olive drab (0.29, 0.32, 0.20).

**Data-driven paths** (keep gameplay stats and visuals in separate files so swapping art never touches code)

Example `assets/data/visuals.ron`:
```ron
{
  "panzer_iii": (
    model: Some("models/vehicles/panzer_iii.glb"),   // None => placeholder
    placeholder: Tank(hull: (2.4, 1.2, 5.4), color: (0.38, 0.40, 0.36)),
    scale: 1.0, yaw_offset_deg: 180.0,
    turret_node: Some("Turret"), barrel_node: Some("Gun"),
    anims: { "move": "Treads" },
    recolor: Faction,                 // override source materials
    icon: "ui/icons/battle-tank.png",
    sounds: { "move": "audio/tank_engine_loop.ogg", "fire": "audio/cannon.ogg" },
    credit: Some("alex-toma-panzer3"),
  ),
  "rifleman_de": ( model: Some("models/infantry/soldier.glb"),
    placeholder: Capsule(radius: 0.3, height: 1.2, color: (0.38,0.40,0.36)),
    anims: { "idle": "Idle", "walk": "Walk", "fire": "Shoot", "die": "Death" }, ... ),
}
```

**How the code uses it**
- **Loading:** load the file as an asset, either through a custom `AssetLoader` or `bevy_common_assets`' RON loader.
- **Spawning:** if `model` is set, spawn the glTF scene (`GltfAssetLabel::Scene(0).from_asset(path)`).
- **Fallback:** if `model` is `None`, or the asset server reports a failed load, spawn the placeholder primitive instead.
- **Animations:** look up clips by name in the loaded glTF's named animations.
- **Recolouring:** apply faction materials in a scene-ready observer.
- **Hot reload:** turn on the `file_watcher` feature so dropping in a new GLB, or editing the RON, reloads live.
- **Credits:** `credits.ron` maps each `credit` id to author, source URL, licence and a modified yes/no flag; the credits screen reads it.
- **API names:** check the Bevy calls above against the 0.20 docs.
- **Optional:** a Blender batch script that converts FBX/OBJ to GLB into `assets/models/` keeps conversion an offline step.

## Unverified items to check by hand

- Which formats are inside the Kenney zips (the pages don't list them).
- The Toon Shooter kit's animation clip names.
- The Newc42 itch.io licence.
- The formats inside the OGA zips.
- How the Quaternius tanks look; I couldn't view images.
