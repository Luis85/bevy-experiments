#!/bin/sh
# Downloads the CC0 assets used by the village demo and the Actor Testbed
# into ../assets. Re-runnable: existing non-empty files are skipped.
# POSIX sh; requires curl + unzip. Downloads land in temp dirs and are only
# copied, never executed.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/assets"
TB="$ASSETS/testbed"

# Temp dirs (downloads/extractions) are removed even when a step fails.
tmp=""
_tmp=""
trap 'rm -rf ${tmp:+"$tmp"} ${_tmp:+"$_tmp"}' EXIT

KAYKIT="https://raw.githubusercontent.com/KayKit-Game-Assets"
ADV="$KAYKIT/KayKit-Character-Pack-Adventures-1.0/main/addons/kaykit_character_pack_adventures"
SKEL="$KAYKIT/KayKit-Character-Pack-Skeletons-1.0/main/addons/kaykit_character_pack_skeletons"
HEX="$KAYKIT/KayKit-Medieval-Hexagon-Pack-1.0/main/addons/kaykit_medieval_hexagon_pack/Assets/gltf"
FURN="$KAYKIT/KayKit-Furniture-Bits-1.0/main/addons/kaykit_furniture_bits/Assets/gltf"
DUNG="$KAYKIT/KayKit-Dungeon-Remastered-1.0/main/addons/kaykit_dungeon_remastered/Assets/gltf"

fetch() { # url dest
  [ -s "$2" ] && return 0
  mkdir -p "$(dirname "$2")"
  echo "GET $1"
  curl -fsSL --retry 3 -o "$2" "$1"
}

# Fetches a .gltf and every relative buffer/image uri it references.
fetch_gltf() { # base_url rel_path dest_dir
  _name="$(basename "$2")"
  _src_dir="$(dirname "$2")"
  fetch "$1/$2" "$3/$_name"
  grep -o '"uri"[[:space:]]*:[[:space:]]*"[^"]*"' "$3/$_name" | sed 's/.*"\([^"]*\)"$/\1/' | while read -r uri; do
    case "$uri" in
      data:*) continue ;;
    esac
    fetch "$1/$_src_dir/$uri" "$3/$uri"
  done
}

# Copies files out of a Kenney kit zip ("Models/GLB format/<file>"). The kit's
# GLBs reference an external Textures/colormap.png, which is always copied too.
# The zip link carries a hash that changes per release, so it is scraped from
# the kit page. Nothing is downloaded when every file already exists.
kenney_kit() { # slug dest_dir file...
  _slug="$1"
  _dir="$2"
  shift 2
  _missing=""
  for _f in Textures/colormap.png "$@"; do
    [ -s "$_dir/$_f" ] || _missing="$_f"
  done
  [ -z "$_missing" ] && return 0
  echo "GET https://kenney.nl/assets/$_slug"
  _url="$(curl -fsSL --retry 3 "https://kenney.nl/assets/$_slug" |
    grep -oE 'https://kenney\.nl/media/pages/assets/[^"]+\.zip' | head -n 1)"
  if [ -z "$_url" ]; then
    echo "no zip link found on https://kenney.nl/assets/$_slug" >&2
    exit 1
  fi
  _tmp="$(mktemp -d)"
  fetch "$_url" "$_tmp/kit.zip"
  for _f in Textures/colormap.png "$@"; do
    [ -s "$_dir/$_f" ] && continue
    unzip -q -o "$_tmp/kit.zip" "Models/GLB format/$_f" -d "$_tmp/x"
    mkdir -p "$(dirname "$_dir/$_f")"
    cp "$_tmp/x/Models/GLB format/$_f" "$_dir/$_f"
  done
  rm -rf "$_tmp"
  _tmp=""
}

# ---------------------------------------------------------------- village demo

# Character (animations embedded in the glb)
fetch "$ADV/Characters/gltf/Rogue_Hooded.glb" "$ASSETS/characters/Rogue_Hooded.glb"
fetch_gltf "$ADV/Assets" "gltf/axe_1handed.gltf" "$ASSETS/props"
fetch_gltf "$ADV/Assets" "gltf/mug_full.gltf" "$ASSETS/props"

# Buildings / props (Medieval Hexagon). The *_cut stumps and resource_stone are
# used by the Actor Testbed; they share hexagons_medieval.png with the rest.
for rel in \
  buildings/blue/building_home_A_blue.gltf \
  buildings/neutral/building_grain.gltf \
  buildings/neutral/fence_wood_straight.gltf \
  decoration/nature/tree_single_A.gltf \
  decoration/nature/tree_single_B.gltf \
  decoration/nature/tree_single_A_cut.gltf \
  decoration/nature/tree_single_B_cut.gltf \
  decoration/nature/trees_A_medium.gltf \
  decoration/nature/rock_single_A.gltf \
  decoration/nature/rock_single_C.gltf \
  decoration/props/resource_lumber.gltf \
  decoration/props/resource_stone.gltf \
  decoration/props/sack.gltf \
  decoration/props/barrel.gltf \
  decoration/props/crate_A_big.gltf \
  ; do
  fetch_gltf "$HEX" "$rel" "$ASSETS/hex"
done

# Bed + book (Furniture Bits; book is used by the Actor Testbed)
fetch_gltf "$FURN" "bed_single_A.gltf" "$ASSETS/furniture"
fetch_gltf "$FURN" "book_single.gltf" "$ASSETS/furniture"

# Ground texture (ambientCG, CC0)
if [ ! -s "$ASSETS/textures/grass_color.jpg" ]; then
  tmp="$(mktemp -d)"
  fetch "https://ambientcg.com/get?file=Grass004_1K-JPG.zip" "$tmp/grass.zip"
  unzip -q -o "$tmp/grass.zip" -d "$tmp/grass"
  mkdir -p "$ASSETS/textures"
  cp "$tmp/grass/Grass004_1K-JPG_Color.jpg" "$ASSETS/textures/grass_color.jpg"
  rm -rf "$tmp"
  tmp=""
fi

# --------------------------------------------------------------- Actor Testbed

# Faction A second look (Adventurers) + faction B (Skeletons); animations and
# textures are embedded in the glbs.
fetch "$ADV/Characters/gltf/Knight.glb" "$TB/characters/Knight.glb"
fetch "$SKEL/Characters/gltf/Skeleton_Minion.glb" "$TB/characters/Skeleton_Minion.glb"
fetch "$SKEL/Characters/gltf/Skeleton_Warrior.glb" "$TB/characters/Skeleton_Warrior.glb"
fetch_gltf "$SKEL/Assets" "gltf/Skeleton_Axe.gltf" "$TB/props"

# Walls, door, chest, items (Dungeon Remastered; self-contained glbs)
for f in \
  wall.gltf.glb \
  wall_corner.gltf.glb \
  wall_half.gltf.glb \
  wall_endcap.gltf.glb \
  wall_doorway.glb \
  floor_tile_large.gltf.glb \
  chest.glb \
  key.gltf.glb \
  plate_food_A.gltf.glb \
  box_large.gltf.glb \
  barrel_small.gltf.glb \
  coin_stack_small.gltf.glb \
  ; do
  fetch "$DUNG/$f" "$TB/dungeon/$f"
done

# Workstations, tools, materials (Kenney Survival Kit)
kenney_kit survival-kit "$TB/survival" \
  workbench.glb \
  workbench-anvil.glb \
  tool-pickaxe.glb \
  tool-hammer.glb \
  campfire-pit.glb \
  tree-log.glb \
  resource-planks.glb \
  resource-wood.glb \
  bedroll-packed.glb

# Armored wagon + wheel-ready siege engine (Kenney Castle Kit)
kenney_kit castle-kit "$TB/castle" siege-ram.glb siege-catapult.glb

# Crew cart (Kenney Fantasy Town Kit)
kenney_kit fantasy-town-kit "$TB/town" cart.glb

echo "assets ready in $ASSETS"
