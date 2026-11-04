---
name: NPC World Viewer
description: A crisp pixel island beside a warm paper observer panel.
colors:
  ink: "#263d38"
  muted: "#62716a"
  line: "#d8dccc"
  paper: "#faf8f0"
  accent: "#315e4e"
  sea: "#235a6e"
  grass: "#88a65e"
  grass-light: "#8eac63"
  sand: "#d2bd83"
  road: "#a78f61"
  road-inset: "#baa675"
  live: "#4c8362"
  warning: "#b58237"
  focus: "#c89740"
  button: "#f4f1e5"
  button-hover: "#e8dfc6"
  button-pressed: "#e5ca8f"
  row-hover: "#eaf0e3"
  row-selected: "#e4eaca"
  field: "#fffef9"
  player-tag: "#e6c88d"
  meter-track: "#e5e7d9"
  meter-good: "#71946d"
  meter-warning: "#b9954d"
  meter-low: "#ac6d53"
typography:
  display:
    fontFamily: '"Silkscreen", sans-serif'
    fontSize: "22px"
    fontWeight: 400
    letterSpacing: "0"
  title:
    fontFamily: '"Segoe UI", sans-serif'
    fontSize: "15px"
    fontWeight: 650
    letterSpacing: "-0.01em"
  entity-title:
    fontFamily: '"Segoe UI", sans-serif'
    fontSize: "22px"
    fontWeight: 600
    letterSpacing: "-0.025em"
  body:
    fontFamily: '"Segoe UI", sans-serif'
    fontSize: "12px"
  label:
    fontFamily: '"Segoe UI", sans-serif'
    fontSize: "11px"
  map-label:
    fontFamily: "monospace"
    fontSize: "10px"
rounded:
  square: "0"
spacing:
  compact: "4px"
  small: "10px"
  control: "12px"
  section: "23px"
  map-inset: "24px"
  wide-section: "27px"
components:
  map-button:
    backgroundColor: "{colors.button}"
    textColor: "#29443c"
    rounded: "{rounded.square}"
    padding: "0 12px"
  map-button-hover:
    backgroundColor: "{colors.button-hover}"
  map-toggle-active:
    backgroundColor: "{colors.button-pressed}"
  search-field:
    backgroundColor: "{colors.field}"
    textColor: "{colors.ink}"
    rounded: "{rounded.square}"
    padding: "10px 9px"
  directory-filter:
    textColor: "{colors.muted}"
    padding: "7px 0 10px"
  directory-filter-active:
    textColor: "{colors.accent}"
  directory-row:
    textColor: "{colors.ink}"
    padding: "12px 3px"
  directory-row-selected:
    backgroundColor: "{colors.row-selected}"
  player-tag:
    backgroundColor: "{colors.player-tag}"
    textColor: "#4b492f"
    padding: "4px 6px"
  need-meter:
    backgroundColor: "{colors.meter-track}"
    height: "9px"
---

# Design System: NPC World Viewer

## Overview

**Creative North Star: "Pixel island, paper observer"**

The user-selected pixel-art world is the visual center: a blue sea, stepped sandy coastline, green land and distinct little village landmarks. Original cached bitmap sprites give people, workplaces and resources recognizable silhouettes. The surrounding interface is a quiet, warm paper surface for reading current simulation facts.

The pixel title establishes the world's character; utility text stays readable in a familiar sans serif. Map controls and inspection are deliberately modest. Status language reports the freshness of the source file, including stale or interrupted states.

**Key Characteristics:**
- Crisp original pixel artwork and square geometry.
- Green, sand and sea world colors beside warm paper and dark green ink.
- Pixel identity paired with compact, readable operator text.
- Map-led desktop layout that becomes a vertical flow on phones.
- Selection and freshness states remain explicit in text.

## Colors

### Primary

Deep green accent marks the identity icon and active directory filters. Golden pressed controls and the played-character tag distinguish local view state. Green live and ochre warning signals accompany connection text; the signal alone does not carry the meaning.

### Secondary

Sea, grass and sand establish the map's broad regions. Earthy road colors connect landmarks. Role-specific roof and clothing colors, small highlights and winter terrain variations remain authored sprite details in `src/art.ts`, rather than a general interface palette.

### Neutral

Warm paper carries the header and sidebar. Green ink supports primary text, muted green supports secondary facts, and pale lines divide sections and directory rows. Row hover and selected fills are deliberately quieter than the map's colors. Need meters use a pale track with green, ochre or clay fills according to the observed value.

**The Status Has Words Rule.** Pair freshness color with the actual connection message; a rendered map can be retained while its snapshot is stale.

## Typography

The display face is self-hosted Silkscreen, with sans-serif fallback. It appears in the island title at the frontmatter display size, falling to (16px) at the narrow breakpoint. Segoe UI with sans-serif fallback handles the rest of the interface. Canvas entity names use monospace.

Section titles use the title role; selected entity names use the larger entity-title role. Body facts and directory names use the body size; metadata uses the label size, with directory subtitles at (10px). The selection hint is (13px) with (1.8) line height and a (30ch) maximum width. Numeric facts and tick values use tabular numerals. The interface uses its observed compact hierarchy, not a modular type scale.

**The Readable Utility Rule.** Keep the pixel face for the identity; retain the sans-serif hierarchy for detailed inspection and controls.

## Layout

Desktop has a (90px) header above a two-column grid: a flexible map and a (334px) sidebar. The main area fills the remaining viewport, with a (540px) minimum height. At widths from (1600px), the sidebar grows to (370px), and section padding increases from (23px) to (27px). The sidebar uses one vertical scroll area so its footer follows the directory.

At widths up to (800px), the header becomes (78px), the map precedes the sidebar, and inspector/directory sit side by side. The map uses (66dvh), with a (440px) minimum height. At widths up to (480px), the sidebar becomes one vertical column and the map uses (62dvh), with a (400px) minimum height. The toolbar spans the map's bottom inset and centers its controls. Supporting read-only text beside the map footer is hidden at the first narrow breakpoint.

Map captions are anchored to the top corners; the toolbar and exploration hint are anchored near the bottom. Panel padding and thin dividers supply the rhythm; this is a continuous operator sidebar, not a collection of floating cards.

## Elevation & Depth

The interface is predominantly flat: paper panels and thin pale dividers provide hierarchy. The map toolbar alone has a soft shadow (`0 5px 16px #142f3b40`) on a dark sea frame. Map headings use a text shadow (`0 2px 4px #183f4d`) to stay readable over terrain. Sprite depth is drawn directly with small shaded blocks and ground shadows. Night uses the observed canvas filter (`brightness(0.7) saturate(0.8)`).

## Shapes

Square edges connect the interface to the stepped shoreline and bitmap silhouettes. Buttons, fields, selection fills and meter tracks have no decorative rounding. The identity is a small stepped tree silhouette. Directory entity symbols repeat simple person, building and resource geometry; sprite landmarks retain their individually authored forms.

## Components

- **Map toolbar:** pale rectangular buttons inside a dark frame, with adjacent (1px) gaps. Controls have a (38px) minimum height and width on desktop; narrow layouts increase height to (40px). Hover darkens the paper; pressed toggles use the golden fill. Zoom uses larger symbols while text controls remain compact. These buttons change the view only.
- **Search field:** a thin outlined paper field with an inline magnifier and compact text. It fills the available directory width. Its native input has the shared focus outline.
- **Directory filters:** text buttons with a pale baseline. Selection uses deep green text, bold weight and a (2px) underline. Hover changes the text to ink.
- **Directory rows:** full-width buttons, at least (54px) high, with a small geometric entity symbol, name, subtitle and optional activity or player tag. Hover uses pale green; selection uses a stronger yellow-green surface and `aria-pressed`.
- **Inspector:** a larger selected name above aligned label/value facts, then thin satisfaction meters and inventory. The empty inspector uses a short selection hint. Values are displayed from the latest accepted snapshot.
- **Carrying and production:** inventory lines group received stacks by friendly good name, with aligned exact totals. Recipe names describe their action and show the real input/output chain. Transport IDs remain unchanged.
- **Cleared sites and trade districts:** producer-supplied pads and corner stakes sit behind paths and actors. Small paper captions identify house/garden sites and trade areas; an Open plots legend provides context. These are map facts, not ownership or building controls. Public speech/actions take priority when space is crowded.
- **Homesteads:** real plot entities show crop beds at the supplied world positions, with different original pixel growth silhouettes for empty, dry, growing and ready stages. Built cottages reuse the village house vocabulary. Ownership and crop times are inspector facts, never browser gameplay controls; real plot/cottage labels replace background site captions.
- **Connection status:** a small square signal beside a strong status line and a muted explanatory line. Missing, stale and interrupted updates use explicit messages while retaining the last available view.
- **Canvas labels and selection:** dark nameplates with light monospace text; selected labels become warm gold with dark text. People are labeled when selected or played, while places/resources remain recognizable landmarks. Selection also gets a small gold pixel outline.
- **Public speech and action feedback:** a read-only Canvas overlay uses paper speech rectangles with stepped pointers and pale selected-row green action rectangles with pixel thought dots. Green ink and 11px monospace inherit the map's palette and utility type. Each rectangle names its actor and wraps at 220px to at most four lines. Hero and selected actors have priority; crowded rectangles yield to names, calendar and controls. Selected records remain available as plain inspector text with their simulation tick. Visibility follows received ticks, including paused or stale snapshots; the browser creates no dialogue or private reasoning.

All buttons, inputs and the canvas use a visible focus outline (3px), offset (3px). Buttons move down (1px) when active and transition background/color over (0.15s). Reduced-motion preference removes those transitions. Disabled buttons reduce opacity to (0.5).

Selecting an entity resets only the desktop sidebar to the inspector heading; mobile selection
anchors that inspector in the page. Polling never scrolls either surface. Narrow headers can grow
to wrap the Living world name and connection explanation instead of clipping those facts.

## Do's and Don'ts

### Do:

- Do preserve crisp nearest-neighbor rendering and original bitmap landmark silhouettes.
- Do keep detailed utility text in Segoe UI and reserve Silkscreen for the identity.
- Do preserve explicit selected, pressed, focus and freshness states.
- Do keep the map first and let inspection flow below it on narrow screens.
- Do keep camera, labels, search and selection controls read-only.

### Don't:

- Don't smooth or blur the pixel artwork when scaling the map.
- Don't imply a stale retained snapshot is live.
- Don't turn view controls into simulation or gameplay commands.
- Don't replace the continuous paper sidebar with decorative floating cards.
