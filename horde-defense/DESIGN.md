---
name: Lastlight — Horde Defense
description: An atmospheric pixel survival outpost with a complete deployment-frozen battlefield.
colors:
  amber: "#efb552"
  health: "#92c391"
  danger: "#ea735b"
  ink: "#111718"
  panel: "#1b2323"
  text: "#e6e6d4"
  muted: "#a4b4ac"
  black: "#000000"
  input: "#0d1415"
  faint: "#232d2b"
  control: "#2b3631"
  button: "#080c0d"
  hover-control: "#434d3a"
  active-control: "#585838"
  graph: "#0c1111"
  acquired-node: "#41331a"
  map-water: "#16272a"
  quality-poor: "#a3a6a3"
  quality-common: "#ebede6"
  quality-uncommon: "#4dd661"
  quality-rare: "#529cff"
  quality-epic: "#c275ff"
  quality-legendary: "#ffa63d"
typography:
  display:
    fontFamily: "Pixelify Sans"
    fontSize: "88px"
  result:
    fontFamily: "Pixelify Sans"
    fontSize: "36px"
  page-title:
    fontFamily: "Pixelify Sans"
    fontSize: "28px"
  heading:
    fontFamily: "Pixelify Sans"
    fontSize: "24px"
  wordmark:
    fontFamily: "Pixelify Sans"
    fontSize: "24px"
  body:
    fontFamily: "Atkinson Hyperlegible"
    fontSize: "16px"
  label:
    fontFamily: "Atkinson Hyperlegible"
    fontSize: "12px"
rounded:
  flat: "0px"
spacing:
  item-x: "9px"
  item-y: "7px"
  button-x: "11px"
  button-y: "6px"
  panel: "18px"
  section: "12px"
components:
  button-primary:
    backgroundColor: "{colors.black}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.flat}"
    padding: "6px 11px"
    size: "minimum min(available width, 190px) × 40px"
  button-secondary:
    backgroundColor: "{colors.button}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.flat}"
    padding: "6px 11px"
  panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.flat}"
  input:
    backgroundColor: "{colors.input}"
    textColor: "{colors.text}"
    rounded: "{rounded.flat}"
  health-bar:
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.flat}"
    height: "18px"
  upgrade-node:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.text}"
    rounded: "{rounded.flat}"
    size: "64px × 64px before graph scaling"
  upgrade-node-acquired:
    backgroundColor: "{colors.acquired-node}"
    textColor: "{colors.amber}"
    rounded: "{rounded.flat}"
    size: "64px × 64px before graph scaling"
---

# Design System: Lastlight

## Overview

**Creative North Star: "The pixel survival outpost"**

Lastlight is a grounded survivor/plague world expressed through crisp pixel figures, muted biome ground, worn improvised defenses and small amber signals. The user's seven reference screenshots and explicit pixel-art survival choice establish the visual world. A full-window battlefield and compact corner instruments make the surrounding place visible while every lane of the deployed arena remains readable.

Combat and visual progression use space generously; management remains compact and practical. Scenery belongs around the combat rectangle, while purchases belong beside the selected upgrade. Pixelify Sans titles, square controls and the living checkpoint scene carry the same pixel identity, while Atkinson Hyperlegible keeps body text readable. Reduced effects preserves a static checkpoint pose. This document records the implemented native Rust/Bevy/egui system; dated execution evidence and its limits belong in [docs/VALIDATION.md](docs/VALIDATION.md).

**Key Characteristics:**

- Continuous quiet ground connects the arena to peripheral biome scenery.
- Original pixel sprites retain padded silhouettes, grounded feet and nearest sampling.
- Compact corner health, ammunition, action and boss readouts leave the field open.
- One complete connected progression canvas pairs literal requirements with explicit payments.
- Authoritative state drives timing, threat feedback and progression; artwork never resolves combat.

## Colors

Warm amber and parchment stand against pine/slate neutrals; terrain stays quieter than actors and tactical cues.

### Primary

Amber identifies selection, acquired upgrades, aim marks, boss notices and combat feedback. Primary deployment buttons instead use a black field, a parchment outline and strong parchment text. Keep this distinction: the shared primary helper is not an amber-filled button.

### Secondary

Health green signals a holding line, successful liberation and valid construction. Danger red signals health loss, contact danger, boss health, plague alerts and invalid placement. The compact battle HP bar uses danger red even above one-third health; it is not a green-to-red threshold meter. Labels, lane location, outlines and symbols explain the state alongside color.

### Item quality

Poor gray, Common white, Uncommon green, Rare blue, Epic purple and Legendary orange come from `quality.json`, converted by the native round-to-byte helper. Use the corresponding quality token for item names, icon borders and loot outlines. Always print the quality name: quality, saved item level and definition unlock tier are three separate properties. These semantic colors extend the established palette without changing its original seventeen values. Color does not authorize a purchase, unlock a class or award an item.

History plots reuse labeled red/amber/green for average DPS, coins and threat. Their numeric series/labels carry meaning; color alone is not an outcome or severity verdict.

### Neutral

Ink is the canvas; panel is the slightly lighter management surface. Parchment is primary text and muted sage supports explanations. Input is the inset field; button is the inactive button weak fill. Control, hover-control and active-control are configured native widget background fills; egui's weak button fill and state strokes remain separate properties. Graph is the node-canvas ground and acquired-node is its warm acquired fill. Map water separates geographical land from interface chrome.

**The Quiet Ground Rule.** Keep ground contrast below figures and threat information. Use forward-oriented, feathered material patches; do not restore mirrored terrain carpets or a permanent combat grid.

## Typography

The unmodified bundled **Pixelify Sans** variable font is the named `display` family's sole face, used for page titles, headings, the wordmark and combat numbers. The unmodified bundled **Atkinson Hyperlegible** Regular is registered first in egui's proportional family, so body text, buttons and labels read clearly at small sizes and large UI scales; Pixelify proved hard to read in dense panels. Monospace keeps egui's default fixed-width face. Default egui fallback families remain available for glyph coverage. Barlow Condensed and its license remain retained historical assets; it is not an active font.

The display role gives the menu wordmark its maximum size. The menu initially chooses a size between 52 and 88 logical units, measures `LASTLIGHT` as one unwrapped galley and reduces it further when needed to fit the actual available width. Never wrap the last letter onto a second row. Result titles use the result role; page titles use page-title; native headings and progression headings use heading; the ordinary compact header uses wordmark. Body and buttons use body, while secondary explanations and compact action timing use label. Strong text uses egui's native strong treatment; no fixed font weight or fabricated letter spacing is prescribed.

**The Literal Control Rule.** Action labels name the actual operation: reload, build defenses, recall, train, equip and sell. Keep rules explanations close to the decision they explain.

## Layout

The native window is configured at 1440 × 900, with a configured minimum of 1050 × 700. Interface scaling is 0.75–1.35. Dimensions below are logical UI units after scaling; capture reports record the actual framebuffer and platform scale. These source values describe layout intent, while native captures and interaction checks establish behavior at a particular window/scale.

Battle has no recurring header or permanent management rail. The maximum combat viewport keeps 24-unit side, 124 top and 84 bottom insets, with 276 reserved on the construction side. ArenaProjection maps the attempt’s frozen lane_count and width_yards. Normal starts with three lanes and twelve yards; every three character levels adds two lanes/two yards, capped 17/30. The centered physical clearing scales by width/30 and min(lanes/11,1), with a bounded 160×86 minimum, rather than stretching three rows across the viewport. The original eleven-lane/thirty-yard active save stays frozen. Perimeter dressing follows actual clearing edges with full-sprite clearance, never canvas edges that imply a larger playable path. Shared core helpers own five-column left-half construction, spawn/view bounds, targeting and ordnance cells.

The upper-left HUD reserves 322×108 at(14,12), with portrait, HP, ammo, stamina/FP, action and danger/DPS readouts; boss/time/progression remain opposite. Live DPS uses five simulation seconds and a tooltip for run average/peak/source totals. Bottom controls retain their 71-high wrapped region. Focused unobscured field hover hides the native pointer after egui output; ordinary UI/focus loss restores it. True aim stays at its projected point, while its resource badge/text clamps within the bounded feedback region. Effects may rise through that reserved ambient space, below the HUD and above controls, without expanding playable inputs, collisions or range.

Journey upgrades owns one heading, wallet and Settings action in its flat 14-unit content frame; the global duplicate header is excluded. The already-banked outcome and named continuation/leave actions lead the complete tree. Retreat/rest wording is deduplicated. More progressively reveals map/stats and learned routes. Secondary preparation retains its context without creating an independent purchase system.

The complete graph and selected detail stay side by side: detail width 290 at 1000 units or above, 240 below, with 18 reserved beside the graph and a 180-unit graph minimum. Every 177-node title is shown as a readable one-row truncated name, with a separate state/cost caption and complete hover text; compact mode must not hide titles. Branch focus labels the selected path and only pans/selects. The 0.75 floor keeps icons at least 48 units. Scroll/drag/Focus/Previous/Next reach the complete graph; cached prerequisite routes avoid enlarged title/caption envelopes. Selected purchase remains above its scrolling requirements.

Campaign map/dossier split at 900 available units. Compact selection reserves a 260-unit map-first region including its 220-unit projection and controls, with a collapsible country dossier below; compact search results are height-bounded so the map is not reduced to a strip. Initial selection uses a 98-unit footer rather than the ordinary 118 minimum. Briefing splits at 820, Equipment at 920 and Settings at 800. Menu actions retain their quiet left region, original cabin anchor (0.400,0.807)/21% source height, measured single-line identity and short-window scrolling.

**The Arena Rule.** Fit the arena as a complete tactical surface. Interface scaling may rearrange controls, but it must not crop lane entries or contact positions.

## Elevation & Depth

Management chrome uses flat tonal layering. The floating battle HUD uses translucent black fills, thin parchment strokes and small native margins; it does not introduce rounded dashboard cards. Native egui windows/popups keep library behavior. Actor/prop ground shadows, peripheral occlusion and a quiet atmospheric edge veil create environmental depth without moving the aiming origin. There is no project-defined decorative UI shadow scale.

Scene/effect atlases and the source-owned player-model catalogue use cached managed ColorImage.from_rgba_unmultiplied and NEAREST. Original RGBA and source UVs remain intact. Model sources compile eagerly and upload after the active RenderDevice texture budget is installed; present malformed model content fails explicitly. The shared battle/equipped-preview facade selects real source parts and declared repair/garment/equipment sources. The cohesive low-ready menu sheet remains separate. Preserve source pixels rather than painting fringes, adding a matte or double-premultiplying.

A positive enemy injury refreshes a bounded six-fixed-tick movement halt (about 0.10 seconds) and an alpha-derived silhouette flash. Misses, defended attacks and fully absorbed attacks do not trigger that halt. It is authoritative behavior in `horde_core`, preserved through pause/save/resume, not a renderer-side delay; NPC decisions and attack clocks keep their prior cadence. Reduced effects substitutes a quiet tint and removes decorative motion/flash without removing tactical information. Every fallen hostile has a saved per-attempt corpse record with its resolved appearance and stable identity. Bodies settle over eighteen simulated ticks, remain opaque for the attempt and render in batches after settling. They neither block contact nor act nor pay a second reward; retry creates a fresh field. Reduced effects settles the pose immediately. Pause freezes simulation-based poses. Native widget animation is 0.12 seconds, or zero with reduced effects.

## Shapes

Square silhouettes and crisp stepped pixel clusters carry the identity. Configured native widgets, primary buttons, progress bars and graph nodes use zero corner radius. Flat panels avoid ornamental framing; thin strokes establish controls and selected nodes. Native focus and selection remain visible. Read-only trend plots use the source’s small three-unit corner; this does not replace the square control system.

Enemies differ through silhouette, clothing, size, armor and tools. Faster approaching actors can pass slower peers within their assigned lane; the saved lateral pass_offset and Enemy.y() drive depth, targeting, AoE, corpses and loot together. Walls and shared contact still block and queue; traps cover the full assigned-lane strip. Critical NPC hostiles remain visibly active through CRIT · FIGHTING labels and signed HP inspection. Zero or negative HP alone does not mean defeated under the retained GURPS injury rules. Actual terminal actors stop and settle in the resolving fixed tick. Complete peripheral tree sprites fit their strip or are omitted; no cropped crowns/trunks or tactical obstruction.

## Components

### Buttons and fields

Primary actions are square black buttons with a two-unit parchment outline and strong text, using the minimum size and padding in the frontmatter. Secondary actions inherit the configured native dark weak fill and egui interaction states. Hovered foreground strokes are amber; native focus and disabled-state behavior remain in place. Search and preset names use native text fields; country, loadout and policy choices use combo boxes. Native Rust components are the renderer source of truth; the sidecar records their source and states rather than fictitious browser controls.

### Compact combat instruments

HP and boss health use square 18-high labeled bars; role fills apply egui `gamma_multiply(0.45)` rather than an invented opaque swatch. Ammunition is a strong numeric readout; stamina/FP is compact text. The action row is allocated at 20 high and at most 220 wide, truncates to one row with a complete hover explanation, and uses a thin progress line where meaningful. Values and timers read the attempt directly, without cosmetic easing. Threat text names a recent hit lane, contact count, closing enemies or a holding line; hover explains contact lanes and winding attacks.

Personal combat stays at 1×. NPC observation retains 1×/2×/4× and its inspectable decision/policy popup. The Management menu groups Settings, save, save-and-menu, withdrawal/recall and the log. Saved bindings supply hints and actions; toggle firing consumes a configured press without keyboard-repeat retriggers. Shared-contact damage identifies both its source lane and the defender.

### Cursor resources and impact feedback

The custom crosshair owns only actual focused arena hover without a popup or details surface. Its exact count reads ammo/magazine or selected throwable charges. Up to 24 pips show the fraction remaining (12 with reduced effects); the green arc reads frozen reload or cooldown ticks, not a cosmetic clock. Construction keeps the crosshair while hiding resource rings; standard UI uses its normal pointer.

Resolved damage feedback uses the display face: ordinary hits start at 27 logical units, weak points at 33, incoming loss at 26; armor/dodge/defense/miss have explicit distinct labels. A short punch reaches 1.3×, settles, then rises up to 38 units and fades over 0.9 real UI seconds, with a black outline. The bounded cache keeps at most 24 bubbles per battle namespace, one per target, aggregating resolved positive values within 150 ms. It never causes injury, another hit, rewards or combat randomness. Reduced effects uses 23/27-unit hit/weak text without punch or rise. Damage-number settings still hide these numbers; existing currency feedback remains separate.

### Upgrade canvas and selected details

The authored personal catalogue has 177 nodes rooted at stable pistol_training, displayed as Survivor foundations. Seven path labels focus one connected DAG: Foundations, Body development, Firearms, Survival, Ordnance, Fortification, Companions and world. All prior 32 IDs remain. The same panel explains parent ownership, minimum victories/liberations, Coins or eligible LegacyBodyPoints and the exact unavailable reason. Fresh body development is coin-paid; XP still develops level/geometry without minting personal CP. Legacy allocated ranks remain floors and their eligible points use a conserved spent ledger. A refund that would orphan learned prerequisites is rejected atomically, without free ancestors. NPC rules training and owned weapon acquisition stay independent.

Each node has a square pictogram, written state, native click/focus target and selected outline. Owned fill/strokes retain the warm/amber vocabulary; available/hover uses parchment and locked uses muted sage. Selection/focus only inspects. Blueprints and ten structure ranks per type are nodes in this same kernel, including legacy ownership floors. The action stays visible above bounded requirements text; no Character/body or separate fortification widget bypasses preflight.

### Personal attack selector and ordnance

The personal attack selector is a fixed three-button Gun/Grenade/Molotov row in the reserved space above the arena. A short single-line helper reports firing, charges or readiness; full hover text preserves the gun class and throwable details. Locked hints name the Ordnance branch of Survival skills. Selection clears pending fire and the firing latch, and never equips a different inventory firearm. A throwable uses one Fire press over one-yard cells within the frozen path width and lane count, with a selected cell, radius preview, hand/flight pose and saved charges/cooldowns. The construction grid remains a separate five-column left-half surface. Authoritative projectile/fire-zone records resolve area damage and burning; arcs, flames, sound and reduced-effects feedback never resolve another hit.

### Journey, deployment and retained outcome

New game protects an existing full journey through a named confirmation and archive-before-reset service, then confirms Rook/June and a country before live defense. Every main victory, defeat or withdrawal returns to Journey upgrades. The continuation action names Try again, Next defense or Choose next country as appropriate; Leave journey waits for durable save and never resets earnings. Parallel completions retain their separate orchestration. Continue makes no purchases, keeps the reported mode and restores prior world selection on failure. Later features require their learned nodes and liberation milestones, with legacy capabilities preserved. Existing every-victory threat, character MAX 50, saved arena/balance and quality/level/tier boundaries remain.

### Equipment quality and banked loot

Item names and class-specific icons use six explicit quality colors and framed borders. Keep quality name, saved instance Lv and unlock tier separately labeled in inventory, comparisons, shop, filters and sorting. Effective stats and sale value use the actual quality ratio relative to the definition’s base quality after level scaling; legacy None/base quality preserves old numerical identity. Shops offer fixed base quality rather than a purchase lottery.

A successful drop is already in the armory before its receipt is drawn. A square icon with a quality-colored outline appears at the defeated actor’s actual position, rises and fades over 1.5 real UI seconds, including at 4× NPC observation. At most twelve flyups per battle stream prioritize higher qualities; labels and the HUD/debrief receipt remain readable when effects are dense. Rare and higher receipts may trigger the bounded existing cue. This cache never awards coins/items, consumes combat RNG or replays old receipts as new rewards.

### Gameplay settings and saved analysis

Gameplay · difficulty leads Settings with saved-next, active-field and draft previews. Preset/slider edits remain an unapplied draft until Apply to next run validates and commits; invalid drafts keep their explanation and disabled action. Arena growth and advanced combat tuning disclose detail without changing current fields. Keep this next-run boundary explicit in all summaries.

Run history is a separate read-only screen reached from menu/camp or debrief Run analysis. It pauses fields and restores prior pause on Back. Mode/outcome/front filters and a virtualized newest-first list lead to selected frozen record details. List/detail split at 760 units and stack below; average-DPS/coin/threat plots split at 660 and stack below. Named values and run-ID hover details explain the last 40 matching records; this chart window never truncates the saved ledger. Older finalized runs are not fabricated, and incomplete legacy measurements are identified.

### Settings and return routes

Readability/effects, aiming/firing, audio, controls/profile and next-run gameplay remain native groups. Critical save/return/quit/reset requests gate both model mutations and raw Settings/rebinding while queued/in-flight. Older autosave completion cannot clear a newer busy state. Named return waits for its matching durable acknowledgment; failure remains visible. One storage worker receives immutable snapshots, with one in-flight and one latest pending job. NewJourney uses an epoch barrier across old saves, archive and fresh commit; stale old Quit cannot navigate the new profile. This is a source durability contract, not a measured stutter/FPS claim.

### Pixel art and geographical presentation

The scene/effect loader owns seven atlases; player_model startup separately compiles the manifest-declared model sources. Source-owned masks assemble the original Rook/June still into real parts, with no whole-body image underneath. Both original binds passed native zero-difference comparison; changed poses and garments need separate inspection, rather than a pixel-identity claim. EquipmentVisual selects actual equipped style/slot metadata. resolve_equipped_model returns the shared pixels, sockets, muzzle and throw origin used by battle and read-only previews. Newly exposed surfaces are explicitly authored repairs. The bounded static source/actual-game confirmation was accepted separately. Near-vertical rifles and limited head pitch remain stylized 2D; do not promise natural 3D anatomy. Focused checkpoint performance is documented with its actual frame counts and limits, with six current motion clips independently accepted, including legal grenade/Molotov release and melee recovery. Fixed-step exports are not performance measurements; final package verification remains separate. The retired generic component rig is not the production body. The menu retains its original low-ready scene at foot (0.400,0.807), 21% source height. Preserve attribution/OFL/geography notices; see [CHARACTER_COMPONENTS.md](docs/CHARACTER_COMPONENTS.md) and [PLAYER_MODEL_VALIDATION.md](docs/PLAYER_MODEL_VALIDATION.md).

Country polygons convey selected, freed, active, accessible and locked states, with neighbor connections and a selected-country dossier. Plague notices include country and deadline text. Biome scenery is presentation of authored regional data, not evidence of precise real-world ecology or culture. The declared GURPS Lite NPC subset remains a rules boundary; this 2D pixel presentation does not imply a 3D world or a complete rules implementation.

## Do's and Don'ts

### Do

- **Do** preserve the complete deployment-frozen arena, shared contact line and stable aiming origin.
- **Do** keep continuous ground quiet and peripheral props outside tactical space.
- **Do** use Pixelify Sans through the `display` family for titles, headings and the wordmark, and Atkinson Hyperlegible for body text, buttons and labels; measure the single-line menu wordmark.
- **Do** expose actual upgrade costs, availability and purchase prerequisites beside the selection.
- **Do** pair color with labels, shape or tactical location and honor reduced-effects preferences.
- **Do** keep native capture, interaction, listening and package evidence separate from source and headless checks.

### Don't

- **Don't** restore painted active assets, permanent combat sidebars or spreadsheet lane grids as the default field.
- **Don't** mirror ground patches into a carpet or let scenery obscure an entry, contact position or construction cell.
- **Don't** invent skill dependencies, prices or automatic progression transactions to suit a diagram.
- **Don't** move the aiming origin or let a visual effect resolve an additional hit.
- **Don't** replace durable settings return behavior with a label-only or transient success state.
- **Don't** claim GUI validation from compilation, screenshots from another build or static source extraction.
