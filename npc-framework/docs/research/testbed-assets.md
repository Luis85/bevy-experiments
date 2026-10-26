# Actor Testbed assets (researched 2026-10-23)

Everything below is CC0. Each `LICENSE.txt`/`License.txt` reads `License: (Creative Commons Zero, CC0)`, then `This content is free to use in personal, educational and commercial projects.` (KayKit), or `You can use this content for personal, educational, and commercial purposes.` (Kenney). Facts were checked against the live GitHub API, raw GLB/glTF JSON headers and the remote zip directories (range requests only, nothing saved).

**Scale reference:** KayKit Adventurers Knight is about **2.31 units** tall (feet at y=0). `axe_1handed` is 1.24 units long.

## KayKit repos on GitHub (user `KayKit-Game-Assets`; the `/orgs/` API returns 404, `/users/` works)

| Repo | Updated | Testbed use |
|---|---|---|
| Character-Pack-Adventures-1.0 | 2023-09 | faction A (have) |
| **Character-Pack-Skeletons-1.0** | 2024-02 | **faction B** |
| **Dungeon-Remastered-1.0** | 2023-09 | **walls, doors, chest, key, food, crates** |
| Medieval-Hexagon-Pack-1.0 | 2024-04 | trees + stumps, lumber, stone, sack (have) |
| Furniture-Bits-1.0 | 2023-09 | bed (have), book |
| Restaurant-Bits-1.0 | 2023-09 | food, stove |
| Halloween-Bits-1.0 | 2023-10 | full-size dead and pine trees |
| Prototype-Bits-1.0 | 2023-09 | grey-box walls and doors |
| City-Builder-Bits-1.0 | 2023-09 | modern cars with separate wheels |
| Space-Base-Bits-1.0 | 2023-09 | sci-fi trucks with separate wheels |

Raw pattern: `https://raw.githubusercontent.com/KayKit-Game-Assets/<Repo>/main/addons/<addon_dir>/Assets/gltf/<file>`. Most packs ship `.gltf` + `.bin` + one shared atlas PNG (e.g. `furniturebits_texture.png`). **Dungeon Remastered ships self-contained `.gltf.glb` files** with the atlas embedded.

There are **no KayKit vehicle packs** suited to a medieval setting, and none of these repos has a workbench, anvil, forge, pickaxe, hammer, ingot, scroll or cart. KayKit Resource Bits, Forest Nature and Adventurers 2.0 exist only on itch.io (manual download).

## 1. Walls, corners, doors: KayKit Dungeon Remastered (1:1 scale)

Base: `https://raw.githubusercontent.com/KayKit-Game-Assets/KayKit-Dungeon-Remastered-1.0/main/addons/kaykit_dungeon_remastered/Assets/gltf/`

- `wall.gltf.glb` (4 wide × 4 high × 1 deep, 53 KB), `wall_half`, `wall_corner.gltf.glb` (L-shape), `wall_corner_small`, `wall_Tsplit`, `wall_crossing`, `wall_endcap`, `wall_window_open`, `floor_tile_large`, `floor_wood_large`.
- **`wall_doorway.glb`** (note: plain `.glb`, 100 KB) has two nodes: `wall_doorway` (frame) and a child **`wall_doorway_door`** at translation (-0.82, 0, 0). The door mesh is 2.0 wide × 2.75 high and its origin sits at the hinge edge. Rotating that node about Y opens the door. The 2.75-unit opening fits a 2.3-unit character.
- `wall_doorway_sides`, `wall_doorway_Tsplit` and `wall_doorway_scaffold.glb` are variants.
- Grey-box alternative: Prototype Bits `Wall_Doorway.gltf` (4×4) plus a separate `Door_A.gltf`/`Door_B.gltf` (1.6×2.8, origin on the hinge edge).

## 2. Items

| Item | Asset (source) | Scale vs Adventurers |
|---|---|---|
| Axe | `axe_1handed.gltf` (have); `Skeleton_Axe.gltf` | 1:1 |
| Pickaxe, hammer, shovel | Kenney Survival `tool-pickaxe.glb`, `tool-hammer.glb` (+ `-upgraded`) | ×4.7 (0.26 → 1.2) |
| Logs, planks | Hex `resource_lumber.gltf`; Kenney `tree-log.glb`, `resource-wood.glb`, `resource-planks.glb` | hex ×4, Kenney ×4.7 |
| Ore | Hex `resource_stone.gltf`; Kenney `resource-stone(-large).glb` | ×4 / ×4.7 |
| Ingot | **gap**: use Dungeon `coin_stack_*` or a tinted procedural cuboid | |
| Food | Dungeon `plate_food_A/B.gltf.glb` (1:1); Restaurant `food_ingredient_ham_cooked.gltf`, `food_stew`; Kenney Food Kit `bread.glb`, `meat-cooked.glb`, `apple.glb` | restaurant about ×0.4 |
| Book, scroll | Furniture `book_single.gltf` (0.5 tall); Adventurers `spellbook_closed.gltf`. **No scroll anywhere.** | 1:1 |
| Key | Dungeon `key.gltf.glb` (0.9 long), `keyring.gltf.glb` | 1:1 |
| Sack, chest | Hex `sack.gltf` (×4). **Dungeon `chest.glb`** has a separate `chest_lid` node at (0, 0.5, -0.565): its origin is on the hinge, so rotating it about X opens the lid. `chest_gold.glb` is a variant. | 1:1 |

## 3. Workstations: Kenney Survival Kit

`workbench.glb` (child nodes `paper` and `hammer`), `workbench-anvil.glb` (child node `hammer`), `workbench-grind.glb`. Each is 0.24–0.30 tall; scale ×4.5. There is **no CC0 forge or furnace** with a direct link. Options:

- Stand-in: Kenney `campfire-pit.glb` (with a separate `wood` node) next to the anvil bench.
- Restaurant Bits `stove_single.gltf` or `oven.gltf` (kitchen style).
- Hex `building_blacksmith_<color>.gltf`: a whole building at tile scale.
- Quaternius "Blacksmith" on poly.pizza (`https://poly.pizza/m/bV52eTG1Aj`, CC0): needs a manual browser download.

## 4. Trees and stumps

Keep the **matched Hex pair**: `decoration/nature/tree_single_A.gltf` (1.09 tall) with `tree_single_A_cut.gltf` (0.23-tall stump), and the same for B and `trees_A/B_cut`. At ×4 the tree is about 4.4 units (about 2× the character) and the stump about 0.9. Halloween Bits `tree_dead_large` (4.8) and `tree_pine_yellow_large` (6.9) are already at character scale but have no stump. Kenney Survival `tree.glb`/`tree-trunk.glb` is another matched pair at ×4.7.

## 5. Vehicles: Kenney (KayKit has nothing medieval)

| Vehicle | File | Wheels | Scale |
|---|---|---|---|
| **Crew cart** | Fantasy Town Kit 2.0 `cart.glb` / `cart-high.glb` | 2 child nodes named `Group`. Each origin is at a corner of the wheel's bounding box, so wrap each wheel in a pivot entity at about (0, 0.235, 0.235) before spinning it | ×3 (0.54 high, 1.33 long) |
| **Armored wagon** (directly commanded) | Castle Kit 2.0 **`siege-ram.glb`**: a roofed 6-wheel ram cart | 6 `wheel` nodes, origin at the rim, need re-pivoting; separate `ram` node for the attack animation | ×2.5 (1.18 high) |
| Siege, wheel-ready | Castle `siege-catapult.glb` (`catapult` arm node), `siege-tower.glb`, `siege-ballista.glb`, `siege-trebuchet.glb` | 4 `wheel` nodes **centred on the hub** (radius 0.2), spin them directly; `-demolished` variants | ×2.5 |
| Off-style | KayKit City Builder `car_sedan.gltf`, Space Base `spacetruck.gltf` / `spacetruck_large` | 4 centred, named wheel nodes (`*_wheel_front_left` …) | ×5 |
| Manual | Quaternius Medieval Village MegaKit `Prop_Wagon.gltf`: single mesh, PBR textures (a different look). OpenGameArt zip `https://opengameart.org/sites/default/files/medieval_village_megakitstandard.zip` (100 MB, no login) | none | about 1:1 |

**Kenney URLs:** `https://kenney.nl/media/pages/assets/<slug>/<hash>-<ts>/kenney_<slug>[_ver].zip`. The hash changes when a kit is updated, so scrape it from `https://kenney.nl/assets/<slug>` with `grep -oE 'https://kenney\.nl/media/pages/assets/[^"]+\.zip'`. Current links:

- survival-kit `…/survival-kit/4065a8185b-1712149243/kenney_survival-kit.zip` (1.9 MB)
- castle-kit `…/castle-kit/a395102d20-1711543616/kenney_castle-kit.zip` (2.2 MB)
- fantasy-town-kit `…/fantasy-town-kit/efe948d309-1754222374/kenney_fantasy-town-kit_2.0.zip` (3.9 MB)
- food-kit `…/food-kit/83086fa91c-1719418518/kenney_food-kit.zip` (4.6 MB)

GLBs are in `Models/GLB format/`. Kenney GLBs reference an **external `Textures/colormap.png`**, so copy `Models/GLB format/Textures/colormap.png` next to them. Kenney's `chest.glb` has a `lid` node and 3 embedded animations.

## 6. Second faction: KayKit Skeletons (verified)

`https://raw.githubusercontent.com/KayKit-Game-Assets/KayKit-Character-Pack-Skeletons-1.0/main/addons/kaykit_character_pack_skeletons/Characters/gltf/Skeleton_{Minion,Warrior,Rogue,Mage}.glb` (4.8 MB each).

- **Same rig:** bone names are identical (`root`, `hips`, `handslot.r`, `handslot.l`, …).
- **95 embedded clips.** These are a **strict superset of Knight's 76**: every Adventurers clip is present with the same name, including Idle, Walking_A, Interact, Use_Item, Lie_Idle, 1H_Melee_Attack_Chop, PickUp and Sit_*.
- Extras: Idle_B, Idle_Combat, Running_C, Taunt, Spawn_*, Skeletons_Awaken_*, Death_C_*.
- Height 2.17 against Knight's 2.31, i.e. 1:1.
- Weapons are in `.../Assets/gltf/Skeleton_{Axe,Blade,Staff,Crossbow,Shield_*}.gltf` (+ `.bin` + `skeleton_texture.png`).
- Adventurers GLBs embed their weapons as nodes (`1H_Sword`, `Round_Shield`, …), so hide those you don't need.

Fallback: Kenney Mini Dungeon `character-human.glb` / `character-orc.glb`. Their clip names are unverified and will not match.

## Recommended bundle (all raw GitHub plus 2 Kenney zips)

1. **Dungeon Remastered**: `wall`, `wall_corner`, `wall_half`, `wall_endcap`, `wall_doorway.glb` (hinged door), `floor_tile_large`, `chest.glb` (hinged lid), `key`, `plate_food_A`, `box_large`, `barrel_small`. 1:1, self-contained GLBs.
2. **Skeletons**: `Skeleton_Minion.glb` and `Skeleton_Warrior.glb` as faction B, plus `Skeleton_Axe`.
3. **Hex**: `tree_single_A/B` + `_cut`, `resource_lumber`, `resource_stone`, `sack` (×4).
4. **Furniture**: `book_single`.
5. **Kenney Survival**: `workbench`, `workbench-anvil`, `tool-pickaxe`, `tool-hammer`, `campfire-pit` (forge stand-in) (×4.7).
6. **Kenney Castle**: `siege-ram` (armored wagon) and `siege-catapult` (centred wheels) (×2.5). **Kenney Fantasy Town**: `cart` (×3).

Gaps: ingot, scroll and a real forge. Use procedural meshes or tinted stand-ins.

## Fallback

All-Kenney: Castle Kit walls and `wall-doorway` + `door` (×3), Fantasy Town `wall-wood-door.glb` (separate `door` node, origin on the hinge), Survival Kit items and trees, Food Kit, Mini Dungeon characters. The look is consistent with itself but flatter than KayKit, and animation names would need a remap table. For Quaternius (Medieval Village MegaKit doors and wagon), use the OpenGameArt zip above. Other Quaternius packs need manual itch.io clicks.
