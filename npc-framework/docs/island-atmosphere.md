# Island atmosphere

Presentation of the regional Island Village world: light, ground, camera, smoke, motion,
settlement identity, visible goods and sound. It implements Phase 5 items W1, W2, W4 to
W7 and W10 to W12 of the
[review and roadmap](superpowers/specs/2026-11-10-island-village-review-and-roadmap.md).

Everything here is view code. It reads simulation components and never writes them,
adds no component to a simulation entity, and changes no hash, replay or schedule. It is
installed only when the regional living world is active (`frontier::enabled`); the old
village, the testbed scenarios and the explorer look and animate as before.

| Concern | Module (`content/island_view/src/`) |
|---|---|
| Day curve shared by sun, lamps, smoke and ambience | `island_atmosphere/daylight.rs` |
| Sun, sky light and haze through the day | `island_atmosphere/sky.rs` |
| HDR, bloom, tonemapping | `island_atmosphere/post.rs` |
| Ground colour: blending, paths, outcrops, shore, grain | `island_atmosphere/ground.rs`, mesh in `living_scene.rs` |
| Outcrop rocks, woodland-edge bushes | `island_atmosphere/dressing.rs` |
| Pitch that follows the zoom, mouse orbit, edge pan | `camera.rs`, `island_camera.rs` |
| Resident markers at settlement zoom, and their key | `island_atmosphere/markers.rs`, `legend.rs` |
| Lanterns and home lights | `island_atmosphere/night.rs` |
| Chimney smoke, forge embers | `island_atmosphere/smoke.rs` |
| Roof colour per settlement | `island_atmosphere/palette.rs` |
| Settlement watchtowers | `island_atmosphere/landmark.rs` |
| Homes and the inn: fitted houses, open doors, who is drawn indoors, the cutaway | `island_homes/` |
| Fitted workplaces, crop fields, the merchant boat, the weather | `island_world/` |
| Carried goods, workplace stock piles | `island_atmosphere/goods.rs` |
| Activity to pose mapping | `island_poses.rs`, clips in `anim.rs` |
| Ambience, work sounds, UI click, volumes | `island_audio.rs` |

## Light

`sky_light` gives the sun, the sky light and the haze for a value of the day curve.
The curve is 0.25 at sunrise and sunset, 1 at noon and 0 at midnight. Light does not
follow it linearly: `sun_strength` is 0.5 at sunrise, 0.8 a tenth of the way into the
day and 1 at noon, so the whole working day is clearly lit.

| Moment | Sun (lux) | Sky light (cd/m2) | Sun colour |
|---|---|---|---|
| Noon | 14,000 | 1,000 | warm white |
| Sunrise, sunset | 8,150 | 725 | gold |
| Midnight | 2,300 | 450 | moon blue |

The first regional curve had the night at 1,500 lux and 340 cd/m2, below the older
island's night (2,300 and 420): away from a lantern the ground was close to black.
The night is now at that older level, a little over. It is still a sixth of the
noon sun, and a lantern still dominates the ground around it.

A night capture taken for the smoke check looked much darker than the earlier
`scene-look/final-harbour-night.png`, with hardly a lit window. No later change had
darkened the night by much: the build before this change, captured at the Harbour
from the same side with the light pinned to midnight, had a median luminance of 50
of 255 in the middle of the frame against 54 in the earlier picture, the ground
having become a little less green in between. The dark capture differed in two other
ways. It showed Stone Ridge, whose ground is grey scree (median 42), and it looked
from behind the houses (the capture scripts place the camera at yaw pi), where a
home then had no lit pane at all. With the changes here the same Harbour view has a
median of 59 and homes show lit panes from every side.

A new game starts at tick 0, which is sunrise. Before this change the sun was linear in
the day curve (3,800 lux at sunrise) and the distance haze reached full strength at 3.5
times the zoom distance, with a 210 m floor; at settlement zoom about half of the
ground's colour was replaced by haze. Together they made the first view of a
"Daylight" morning dim and grey. Haze now reaches full strength at 14 times the zoom
distance (900 m floor), which leaves the settlement under the camera about nine
tenths of its own colour and still pales the far shore.

HDR and bloom do not change scene brightness: the same view captured with and without
them has the same mean luminance to three decimals, at sunrise and at noon.

The island play camera (the one carrying `RtsCamera`) gets `Hdr`, `Bloom` at 0.09
intensity (Bevy's natural preset is 0.15) and `Tonemapping::TonyMcMapface`, which was
already Bevy's implicit default and is now explicit. Preview and editor cameras are
untouched. UI is drawn after tonemapping and `--screenshot` reads the final window
surface, so neither depends on the HDR intermediate.

`lamp_level` turns the day curve into a 0 to 1 lamp strength: zero in daylight, rising
through the last light before sunset, full through the night and fading after sunrise.
Path lanterns follow it in both light intensity and glass emissive, with a range of 10 m.

A home shows a warm door light and six glowing panes when the lamp level is above
zero and the home is heated (`Household::heated_until`) or one of its residents stands
within 5 m of the door. The light eases in and out over about a second. The panes
(`ModelFit::panes` in `island_homes/model.rs`) are the two windows of each side wall,
the gable window over the door and a lantern beside the door, so a lit home shows
from whichever side the camera looks. With one lantern, a settlement seen from the
other side looked unlit. Their emissive is a deep amber (4.6, 1.55, 0.16 linear):
the earlier, paler value came out of bloom and the tonemapper as a cream square.
The door light is 16,000 lumens with a 9 m range (it was 9,000 and 7.5 m) and hangs
outside the door. The panes are thin boxes placed over the cottage mesh's own
windows, from its measured coordinates and the scale the home's body gives it, so
they stay on the glass whatever the body's size. A home that is cut away shows no
panes: it has no walls on the camera's side for them to be in. Its door light
stays.

All of these are shadowless point lights. At most 24 lanterns and 16 home lights are lit,
chosen by distance from the camera focus; the rest are hidden, not merely dimmed. A forge
adds one more light per smithy while it works (three in the authored world).

## Ground

The terrain is still one mesh per 16 x 16 cell chunk with one quad per cell, and the
simulation's cells are only read. What changed is the colour: `ground::corner_color`
computes one colour for every corner of the cell grid, and each quad interpolates its
four corners, so neighbouring cells blend instead of meeting at an edge. Two chunks
compute the same colour for a corner they share. A quad is split into triangles along
the diagonal whose two ends are closer in colour, so the crease runs along a colour
edge and not across it.

- **Roads.** `ground::Roads` rebuilds the centre lines of the authored roads from the
  world description, the same segments the terrain generator laid soil along. A corner
  is road by its distance from the nearest line, so a diagonal road has soft edges
  although its cells are stair-stepped. On average the main roads draw 3 m wide at half
  strength (walkable strip 3.4 m), the spine through a settlement 2 m and the lanes to
  doors and workplaces 1.6 m. A stretch whose cells are no longer soil is not drawn.
  `ground::road_look` makes them trodden paths rather than ruled lines: the drawn path
  wanders up to 0.26 m from its centre line over about 13 m, its width varies by 15 %
  over about 8 m, the edge frays by up to 15 cm, a lane leaving a road meets it in a
  rounded fillet, and a worn verge of trampled ground reaches about a metre beyond the
  edge at a third strength. The path stays centred on the same walkable cells; only its
  soft edge strays past them, as it did before.
- **Other soil.** Soil outside the authored strips is drawn from the cells: a corner is
  path by the share of soil in the 4 x 4 cells around it (binomial weights), passed
  through a soft threshold. Every soil cell stays walkable at its old cost.
- **Biome borders.** Turf colour is the weighted mean of sixteen biome samples around a
  position displaced by low-frequency noise, so a border is an irregular band about
  nine metres wide.
- **Variation.** Turf brightness varies by a few percent over about 23 m and 7 m, and
  broad dry patches shift it toward straw.
- **Ridge stone.** The generator scatters ridge stone as four-cell dashes. `classify`
  recognises that pattern and draws those cells, and the ridge turf between them, from
  `outcrop`: a thresholded noise field that covers about a fifth of a ridge in patches
  several metres across. An outcrop is drawn as grey-brown scree, and
  `dressing::outcrop_rock` stands rocks on it: the five KayKit `rock_single` models
  already shipped (`living/nature/rock_single_{B,D,E}`, `hex/rock_single_{A,C}`), on
  every second cell of an outcrop at most, four in five of those, each offset, turned
  and scaled 2.2 to 4.6 from a hash of its cell coordinates, and flattened to six
  tenths of its height. No rock stands within a cell of a path or of water. Stone or
  turf painted in Terraform is recognised and drawn as painted, without rocks.
- **Shore.** Land within a metre or two of water blends to sand. Every water cell that
  touches land carries a slope from the land's edge down to half a metre under the
  surface a metre out (`ground::beach_corner`, built in `terrain_mesh`): sand up to
  about 0.7 m of bank, the bank's own shaded ground above 2 m, and at most 3 m high,
  above which the cliff keeps its face. The slope is split along the diagonal that
  joins corners of like height, so a stair step of the coast becomes one slanted face.
  A translucent mesh per chunk still tints the water cells within three metres of land.
  The simulation's shore is unchanged: these are water cells, nobody walks on the slope.
- **Grain.** The land material multiplies the vertex colours by a 256 x 256 tiling
  texture generated in code (`ground::detail_image`): three octaves of wrapping value
  noise with features of 6 to 40 cm, between 0.66 and 1 with a mean of 0.83, which the
  material's base colour cancels. One tile covers 4 m and is turned 31 degrees against
  the cell grid. It carries a full mip chain and trilinear, 8x anisotropic sampling, so
  the grain shows up close, thins out at settlement zoom and is gone at the overview.
  No image file is shipped.
- **Woodland edge.** Scenery trees within about six metres of the woodland border are
  thinned (`dressing::tree_thinned`), most of them in the first row, in bays a dozen
  metres across, so the tree line no longer traces the biome border. Bushes scatter
  outside the wood for about seven metres, and a few stand between the thinned trees
  (`dressing::edge_bush`). They are **bushes**: a rounded shrub of four facetted lobes
  about a metre across, built in code, in two muted greens, with no trunk. They are
  not saplings, never grow, are not resource patches and cannot be harvested; like the
  scenery trees they do not block movement.

A chunk is rebuilt when a cell within 4 cells of it changes surface or height
(`ground::NEAR_MARGIN`); biomes are read up to 9 cells away. Both are part of the chunk
signature, so edits still rebuild only the chunks they can affect. Rocks and bushes
follow a terrain edit too: they take the ground's new height, and step aside when
their cell is painted over, paved or flooded.

The autumn palette is muted: each biome's green is dulled a little and takes a tenth
of ochre, and autumn's dry patches are ochre and a third weaker than summer's straw.
The coast, which read as yellow, is olive (sRGB about 0.56, 0.60, 0.37).

## Camera

On the regional island the camera's pitch follows the zoom: 38 degrees up to 30 m, so
doors and facades are visible, easing to 58 degrees from 240 m outward, which is the
overview's pitch (`camera::island_pitch`). A tilt chosen with PageUp or PageDown, or by
dragging, is kept as an offset from that curve while zooming. Focusing a settlement and
entering character play move along the same curve.

| Input | Effect |
|---|---|
| Middle mouse button, drag | orbit (horizontal) and tilt (vertical) |
| Cursor within 14 px of a window edge | pan, in Inspect and Terraform |
| WASD, arrows, Q/E, PageUp/PageDown, wheel, F | as before |

Edge panning is skipped while the cursor is over a panel, while the window is not
focused, in character play and in `--screenshot` runs. `ISLAND_VIEW_EDGE_PAN=off`
disables it at startup. A start pose given without a pitch (`--camera X,Z,DIST`) takes
the curve; `--camera X,Z,DIST,YAW,PITCH` is kept as given, except that a pitch of
exactly 58 counts as not given.

`IslandCameraSettings { edge_pan, pan_speed }` holds the player's preferences; a host
inserts or edits it. The Settings screen does, and keeps them as `edge_pan` and
`pan_speed` (50 to 200 percent, in steps of 25) in `island-settings.json`. Pan speed
scales keyboard and edge panning of the island camera. The startup switch above wins
over the setting.

A camera the viewer has moved (wheel, orbit, pan) is left where it is when a panel
opens or the window resizes; only a view nobody has touched since it was framed is
framed again. `RtsCamera` is reflected, so a capture script can place the view over
the remote protocol (`world.mutate_components`); it is view state.

## Resident markers

At settlement zoom a resident is a few pixels tall. Each living resident with a visible
model gets a dot above the head in the colour of the trade (`markers::role_color`). Dots
keep the same size on screen, fade in between 24 m and 38 m of zoom, where models and
captions stop being readable, and fade out between 190 m and 280 m, before the overview.
At most 96 are drawn, nearest the camera focus first. They are unlit view entities of
their own and follow whatever the fog shows.

Each dot has a thin dark rim: a near-black sphere 1.3 times its size, which leaves
about 15 % of the dot's diameter showing all round and fades with it. The rim is part
of the dot's own mesh (`markers::marker_mesh`), its triangles ahead of the dot's, so
within the one blended draw the dot always lands on it. It used to be a second entity
held behind the dot by a depth bias, which left the order of the pair to the
renderer's transparent sort. In a night capture of the Harbour, five of six markers in
the front row had their rim drawn over the dot and showed as black discs from the
south, while the same markers were right from the north. The cause was not pinned
down (Bevy 0.20 retains transparent items with the mesh centre they were queued
with, which would do it for markers that have moved); one mesh does not depend on it.
 `legend.rs` is the key: a small panel, bottom left, that names the trade of
every colour with a living resident. It is shown while overseeing in the player
session, only while the markers are (opacity of 0.15 or more), and **Hide key**
collapses it to its header. It is not shown in Live or Shape.

## Smoke and embers

A chimney smokes when its home has a fire going (`smoke::hearth_smokes`):

- **Heated homes.** `Household::heated_until` is the simulation's hearth. It is lit
  only in the cold (at night, or in winter) when a resident brings a log to the door,
  and burns for half a day. Measured on the authored world: no hearth is lit before
  the first nightfall (0 of 42 homes through tick 16,000), 34 are lit by 00:00 and all
  42 by 02:00, and the last goes out between 12:00 and 14:00 the next day.
- **Occupied homes.** A new island starts at sunrise with every hearth cold, and a
  night's fire is out by early afternoon, so on heat alone no chimney smoked on the
  first day or at any dusk. A settlement home with a resident within 5 m of its door
  therefore also smokes: a cooking fire. This is a view interpretation, like reading a
  rest as sleep. It is switched off where smoke must remain a truthful sign of warmth:
  in winter, and for a homestead cottage, whose fuel the player manages.

Smoke eases in and out over three seconds. It is never shown for a home the fog does
not show live.

Each chimney has nine puffs, as before. A puff is a quad that always faces the
camera, drawn with a 64 x 64 alpha texture generated in code (`smoke::puff_image`,
no asset file): dense in the middle, falling smoothly to nothing before the quad's
edge, with a lobed outline and a mottled body. It leaves the chimney about a third
of a metre across, rises about 5 m over 7.5 s (slowly at first, so the column has a
body at the mouth), leans downwind more the higher it is, and grows to about a metre
and a half while it thins out. Each puff has its own size (within 18 % either way),
its own wander off the column's line and its own slow turn about the line of sight,
and neighbouring puffs always overlap (`a_column_is_overlapping_puffs_not_a_chain`
checks it), so the column reads as one plume and not as a row of shapes. Puffs fade
through eight shared materials of 10 to 80 % opacity, so batching is kept; one too
faint for the first of them is not drawn. The wind is the same for every building:
a puff's drift is turned back by its building's own rotation.

Smoke is not lit by the scene's lights. A billboard's normal follows the camera, so
real lighting would brighten and darken the smoke as the view orbits. Its colour
comes from the same sky model instead (`smoke::smoke_shade`): a pale grey a little
short of white through the working day (it follows the sun's strength, not the raw
day curve), and at night a blue-grey a third as bright (0.21, 0.23, 0.28 linear):
lighter than the dark ground behind it, so the column still reads, and far below a
lantern or a lit pane. A value three times dimmer, what the moon alone would leave,
could not be seen in a night capture. Lanterns and windows do not light it. A
chimney's mouth is the mouth of the home's fitted cottage model
(`ModelFit::chimney`), about 6.5 m up; a cut-away home, which has lost its roof
and chimney, does not smoke.

While a craft runs at a smithy, its forge chimney shows a low bed of coals in its
mouth that breathes and flickers, seven embers, five smaller puffs of smoke, and a
flickering light at the hearth. An ember is a spark 13 cm across. Each has its
own rhythm: it waits out part of every cycle, then hops out on a new arc (0.7 to
1.6 m up, up to 1.2 m to the side), burning down and glinting unevenly on the way,
so on average three or four are in the air and they never stand in a column. The
coals and embers are unlit and a few times brighter than white, so they stay orange
in sunlight and bloom.

Earlier versions: the first smoke was drawn but could not be seen at settlement
zoom (its densest material was 34 % opaque and mid grey in the morning; an ember was
a 9 cm sphere). The second made both large: nine translucent spheres up to two
metres across, which read as a chain of flat, hard-edged discs, and twelve 24 cm
embers evenly spaced in time, which read as a column of orange balls.

## Poses

`island_poses::island_pose` maps what the simulation reports to a pose; `Pose::clip`
names the clip. Only clips that exist in the shipped files are used, and nothing is
retargeted. `None` leaves the generic testbed mapping in charge.

| Sim state | Pose | KayKit rig (Rogue, Barbarian, Mage, Knight) | Civilian rig (Quaternius) |
|---|---|---|---|
| Asleep (`view::repose`) | Sleep | `Lie_Idle` | `Idle_Neutral`, root laid down (no lying clip) |
| Resting, not asleep | Sit | `Sit_Floor_Idle` | `Idle_Neutral` (no sitting clip) |
| Walking with bulk goods | Carry | `Walking_B` (no carry clip) | `Walk` (no carry clip) |
| Gathering logs | Chop | `1H_Melee_Attack_Slice_Diagonal` | `Sword_Slash` |
| Gathering grain, fibre, berries; planting, watering, harvesting; restoring land | Farm | `2H_Melee_Attack_Chop` | `Interact` |
| Gathering fish | Fish | `2H_Melee_Idle` (no fishing clip) | `Idle_Sword` (no fishing clip) |
| Crafting at a smithy, carpenter, sawmill or mine; building a cottage | Hammer | `1H_Melee_Attack_Chop` | `Punch_Right` |

What is in the hand follows `view::held`, read once a tick into the pose
cache: a rod, an axe or a hoe where the simulation names one, the pose's own
tool otherwise, and nothing when the work goes bare-handed (no usable tool
carried). A stage of work at a homestead upgrade (`live::UpgradeWork`, the
hired builder or a helping friend at the door) is the hammer pose; a
hauler walking ordered materials to the door carries them over the shoulder,
logs from two and planks (their own prop) from one. A fisher at work faces
the nearest water within three cells (`island_poses::face_water`): the
simulation's fishing ground is the dry cell at the edge and says nothing of
where the line goes.

Work tools (`island_poses::WORK_TOOL_SCALE`) are drawn 1.3 times the kit's
scale: at 20 m a hand-sized axe did not read; at 1.3 its blade shows. A tool
the simulation equips (legacy scenarios) keeps its scale.

Sleep is the simulation's own: `Repose::Asleep` is a rest in a bed at night or
in the resident's sleep hours, and anybody asleep lies down on both rigs,
whatever the hour; any other rest sits. The animation pass cannot ask the world,
so `island_poses::cache_repose` reads `view::repose` for every resident once a
tick into `ReposeCache`. A resident the simulation has inside a building is not drawn
outdoors at all (see [Homes](#homes)); only somebody without a bed still rests
in the open.

At settlement zoom (30 m) a resident is about 45 pixels tall, so a pose is read from
motion more than from a still. Between two frames half a second apart, of the 8,000
pixels around the resident, a hammering smith changed 1,866 and a chopping woodcutter
367, against 59 for someone standing idle. The woodcutter's swing is small because
the camera looked at their back. Hoeing was not seen in the open: in three capture
runs every farmer was cutting fibre under trees or behind a building.
`scripts/playtest/pose_sheet.py` lays enlarged crops of the pose captures side by
side for review.

## Settlement identity

Only the blue KayKit building variants ship. The buildings share one texture atlas in
which the team colour is the only saturated blue (hue 200 to 220, saturation about 0.8;
stone is the same hue at saturation 0.1). `palette.rs` copies the atlas once per palette,
shifts only those texels and points building meshes at a material copy. Harbour keeps
blue, Alderwood becomes moss green and Stone Ridge terracotta; meadow and wetland
settlements in edited worlds get ochre and plum. Each extra palette costs about 4 MB of
texture per atlas file.

Each settlement also gets one watchtower in its roof colour at the village edge. The site
is a pure function of the world spec and terrain: at least 11 m from every site, home and
the arrival point, outside plots, on dry level ground, and nearer to its own settlement
than to any other. Live resource patches get a final veto at spawn. The tower has no
simulation entity: it does not block movement and residents can walk through it.

## Goods

A resident walking with two or more units of bulk goods carries a prop: a log bundle
for logs and planks, a crate for ore and ingots, a sack for grain, flour, fibre,
hides, leather, meat, fish and berries. Tools, coins, water, meals and clothing never
count. The prop shows only on the move (`goods::shown_load`): someone standing at
work, idle, resting or asleep has put the load down. Shown always, it covered the
working arm and floated over sleepers, and because every resident keeps two to four
logs of hearth fuel it was on all 42 of them all day.

Each workplace has a pile of up to six props (lumber, crates, sacks or barrels by trade)
sized by the units in its inventory: 1, 5, 11, 19, 29 and 41 units add a prop. A pile is
not updated while the fog shows the place dimmed or remembered.

## Sites, stores and stalls

`island_build::scene` draws what the village builds after the founding. One visual
root per construction site or finished building is spawned when the civic index
gains the entry, replaced when a site becomes its building and despawned with it;
nothing is rebuilt per frame, and the sync does no work at all in a frame in which
neither the civic index nor the entity index changed. Progress and stock compare a
few numbers per site or store each frame and touch transforms only when they
differ. A root the fog shows dimmed or remembered keeps its last look.

A site is a sunk bed of earth, a staked rail at 0.42 m, a doorway of two posts and
a lintel on the entrance side and a plank threshold on the entrance cell. Up to
six props lie beside the doorway for the delivered materials (1, 5, 11, 19, 29 and
41 units add one), shared out between log bundles and crates by what was
delivered. Once supplied, a scaffold of corner posts and rails stands (2.6 m for
a solid building, 1.1 m for an open yard); the plank walls rise to it in
proportion to the work done, and the pile shrinks by the same share.

A finished workplace wears the founding model, fitted to its plot's body like
a founding one (see [Workplaces](#workplaces)); `palette::assign` gives it the
settlement's roof colour and it keeps its turning machinery. A finished home
or inn gets its house from `island_homes` (see [Homes](#homes)), and a field
its plots from `island_world::fields` (see [Fields](#fields)). A
stockpile is a fenced yard with nine prop places filled from the back; a barn is
a plank shed under a two-slope roof in the settlement's roof colour (one plain
material, not the recoloured atlas) with six places by its door; a market stall
is the KayKit market model with six smaller props in front. The share of places
filled is the store's stock over its capacity (anything at all shows one prop);
which props is by what lies there: log bundles, crates, sacks or barrels.

The placement ghost is one translucent unlit box over the body, standing on its
highest cell, a gold door-high box on the entrance cell, and gizmo outlines of
every body cell and the entrance.

## Homes

`island_homes` draws the homes and the inn as places somebody can be inside. It
reads `village::view` (`home`, `homes`, `inside`, `occupants`) and the `Indoors`
component, and writes nothing to the simulation.

### Proportions

The KayKit buildings are tabletop miniatures: a small door on a small house. At
the scale they were placed with, a resident was taller than a home's door and
its eaves, the walls covered a quarter of the body the simulation blocks, and
the model's door faced away from the entrance. Measured from the meshes (model
units times the placed scale; a resident is 1.8 m on the civilian rig and
1.73 m on the adventurer rig):

| Before | Model, scale | Walls drawn | Body in the simulation | Door | Door / resident | Wall top | Ridge |
|---|---|---|---|---|---|---|---|
| Home, one storey | `building_home_A` x 4.0 | 2.24 x 2.52 m | 4 x 5 m | 1.26 m, on the far side | 0.70 | 1.34 m | 3.08 m |
| Home, two storeys | `building_home_B` x 4.0 | 2.80 x 2.38 m | 4 x 5 m | 1.26 m on a 0.84 m base, on the far side | 0.70 | 3.3 m | 4.9 m |
| Inn | `building_tavern` x 2.4 | 2.81 x 3.20 m over all | 5 x 5 m | 0.76 m on a 0.5 m base | 0.42 | 1.3 m | 3.35 m |

Now every home wears the one-storey cottage, whose door is in the middle of
its gable wall, and the inn wears the two-storey house. `HouseModel::fit`
scales the model's three axes separately so that the box of its ground-floor
walls is exactly the body (`HomeView::body_lo..body_hi`, or a plot's cells),
its door is 2.2 m high and stands on the ground, and the door wall is the
entrance side. The two-storey model's stone base is sunk into the ground. A
stone footing under every body closes the gap a level house leaves on a slope.

| Now | Model, scale (x, y, z) | Walls drawn | Door (frame crown), opening | Door / resident | Wall top | Ridge | Chimney |
|---|---|---|---|---|---|---|---|
| Founding home, cottage, 4 x 5 | `home_A` x 7.14, 6.98, 7.94 | 4.00 x 5.00 m | 2.20 m, 1.04 m wide | 1.22 (1.28 for the shorter rig) | 2.35 m | 5.4 m | 6.5 m |
| Family house, 5 x 6 | `home_A` x 8.93, 6.98, 9.52 | 5.00 x 6.00 m | 2.20 m, 1.29 m wide | 1.22 | 2.35 m | 5.4 m | 6.5 m |
| Inn, 5 x 5 | `home_B` x 7.14, 6.98, 8.40, sunk 1.47 m | 5.00 x 5.00 m | 2.20 m, 1.04 m wide | 1.22 | 2.35 m to the upper floor | 7.1 m | 7.5 m |

`RESIDENT_HEIGHT_M`, `RESIDENT_HEIGHT_MIN_M`, `DOOR_HEIGHT_M` and
`WALL_HEIGHT_M` are constants of `island_homes/model.rs`, and a test holds the
door between 1.1 and 1.3 times either resident. Where the door is against the
entrance:

- A founding home: the entrance point is a metre in front of the middle of the
  door wall, and the door is in the middle.
- A family house (5 wide): the entrance cell is in the middle, half a metre out.
- A built cottage (4 wide): the simulation's entrance cell is half a cell to one
  side of the middle, so the door's middle is 0.5 m beside the entrance point;
  the entrance still lies within the door's frame.
- The inn: its model's door is 1.0 m to one side of the entrance point. An inn
  also has a sign on a post by its door.

Workplaces are fitted the same way (see [Workplaces](#workplaces)).

### Doors

A closed house's door is the model's painted, shut one. While somebody steps
in or out, and for 1.0 s after (long enough to read at 1x), the view draws the
door open over it
(`model::door_parts`): the dark of the doorway in two boxes, one the width of
the leaf up to where the arch springs and a narrower one under the crown, and
a timber leaf swung 105 degrees outwards on the jamb. The three boxes are
spawned when the step begins and despawned when the door shuts; a home that is
cut away shows its own open door instead. `ISLAND_VIEW_DOOR_HOLD=6` keeps a door
open that many seconds, for a capture.

### Going in and coming out

A resident whom the simulation has inside a building (`Indoors::building`)
stands on the entrance point. The view walks them the last step into the
doorway over 0.45 s, upright, and then stops drawing them; when they leave, they
step out of the doorway over 0.35 s towards wherever they are walking. Somebody
already inside when the view first sees them is hidden without a step. A hidden
resident's visual is also moved out of a click's reach, so a click on a house
selects the house; they stay selectable from the roster.

A building or a tree between the camera and whoever is followed is taken out
of the way without being opened (`island_homes::unblock`): the played
character in Live, the selected resident in Oversee from 32 m in, as long as
they are outdoors. The test is the straight line from the camera to their
chest against each building's box (the body, 0.2 m wider for the eaves, up
to the roof's ridge, not the chimney's top: a home 5.4 m, not 6.5 m) and each
scenery tree's silhouette (`model::TreeShape`: a trunk, then a crown tapering
from its foot to its top, read off the four tree models, with 0.5 m round it
for the half width of the person looked at; only trees in the box between the
camera and the character are tested). A tree a little off the line stays.
A blocking **home** (a house, the homestead cottage, the inn) is not squashed:
it is cut away (`HomeBlocks` asks `cutaways` for it, after the played
character's and the selection's homes, before the close camera's), so its
floor, furniture and the walls on the far side stay. Every cutaway eases: the
model sinks over 0.2 s, uncovering the room, then is hidden, and rises again
over 0.2 s when the cutaway closes (0.35 s after it last blocked). A home that
is both selected and blocking has one cutaway. A blocking workshop sinks to
its footing over
0.2 s (a vertical squash of its model, eased; no new material), is then
hidden, and rises again over 0.2 s once it has not blocked for 0.35 s. Its
chimney smoke and forge embers go with it (`Unblocked::hides`); its window
panes and door light hang from the squashed frame and go with it too. A tree
is squashed the same way; a tree already cleared for a building stays
cleared. With nobody to follow and nothing out of the way the pass returns at
once; `ISLAND_VIEW_STATS=1` logs `island unblock: N out of the way (T trees)`
whenever the number changes.

On the regional island the played character is drawn as the Knight
(`island_world::cast::player`), whatever the preset's `model` names: the
`human` preset's hooded rogue read as a green blob from the Live camera, and
no resident wears the Knight. The preset is not changed; legacy scenarios
keep the body their preset names (a robot preset there gets the Knight as
before). The poses, the door step, the cutaway and unblocking treat it as a
resident. A wider warm ring at its feet, instead of a faction ring, is the only
mark of the player. `ISLAND_VIEW_PLAYER_BODY` draws it with another body for a
capture (see [Capture and review aids](#capture-and-review-aids)); the preset
is not touched.

### People

`island_world::cast` dresses the regional island's people from one family of
bodies, the KayKit adventurers the buildings belong to: residents wear the
Barbarian, the Mage or the bare-headed Rogue, picked from their `SimId`, with
their hat (three in four) and cape (two in three) on or off and the hat and
cape in one of six cloth colours (a tinted copy of the kit's material, one per
body and colour, shared). The Knight is kept for the played character's
default and the hooded rogue reads as a cloak from above, so residents wear
neither. Before this, a resident's body came from its profile's appearance
through a roster of both kits, the Quaternius villagers at 1.8 m beside
KayKit figures at 1.73 m. Now an adult stands 1.8 m on either rig (KayKit at
0.82 of a model unit a metre instead of 0.75), so the doors and beds tuned for
1.8 m hold, and a child is drawn 0.62 of that, about 1.12 m: `cast::stature`
reads the life stage through `view::age` once a second, so a child grows up in
the drawing the day the simulation says so. Legacy scenarios keep their
bodies and sizes. `ISLAND_VIEW_CAST=mixed` draws the old roster for a
before-and-after, `ISLAND_VIEW_CAST=civilian` the villager kit (and the
traveller for the played character).

A homestead cottage (the Live character's own) is a home like the others on
the body `view::home` gives it. On the living world that is the catalogue
cottage, 4 by 5 m with two beds, and the `larger_cottage` upgrade grows it to
5 by 6 m with four on the same entity: `island_homes::upgrade::refit` notices
the new body within half a second and the cottage is fitted and furnished
again (hearth, table and full beds; the chest once `view::chest` reports
one, in the front corner away from the door, `model::chest_spot`). While
`view::upgrade_site` reports an upgrade, a timber scaffold stands half a
metre round the walls, its poles full height and its rails climbing from
1.2 m to 5 m with the work done, and the delivered logs and planks are
stacked beside the +X wall (at most ten of each drawn). Legacy profiles keep
the 2 by 2 m cottage: the model stretched upwards 1.4 times (door 1.58 m)
and, inside, one bunk of three berths, a brazier and a strip of floor inside
the door (see Limits). Whoever is inside is hidden, and it is cut away when
it is selected or the played character is in it.

### The cutaway

Whoever is up indoors stands on a place (`model::interior`'s `spots`: two by
the hearth, one beside the table, one inside the door) that keeps a 1.8 m
adult's 0.45 m round clear of the walls, the beds, the hearth and the 0.32 m
its fire and slab stand out, the table and stools and the chest's corner,
and 0.7 m from the next place: each wanted place is moved to the nearest
point on a 5 cm grid that is (`clear_spots`). A test checks every home and
inn of the shipped world, the 4 by 5 m and 5 by 6 m cottage and the inn's
5 by 5 m. Only a room fuller than any home is built for lets a place come
closer (feet 0.15 m off the furniture). The chimney breast stands with the
back wall, so it comes down with it when that wall is the camera's. A wall
that comes down is one unbroken stone kerb 0.35 m high, without post stubs.

The cottage model draws its chimney into its one mesh, 0.35 units wide at the
foot against a 0.56-unit wall box: on a 4 m body that is a 2.5 m breast. On
the island each home model gets a copy of the mesh (cached by scale) whose
chimney is narrowed across to a 0.9 m breast, the stack in proportion
(`island_homes::chimney`); the kit's mesh, which legacy scenes draw, is not
changed.

A home or inn is cut away while any of these holds, three at most at once, and
only while the fog shows it live:

- its building card is open (a click on it while overseeing);
- the selected resident is inside it, or it is itself the selection;
- the played character is inside it, in Live;
- in Oversee and Shape, the camera is within 15 m and its focus is on the body
  or within 1.2 m of it; it then stays open until the camera is 18 m out.

The house model is one mesh, so its roof cannot be lifted off. While a home is
cut away its model is hidden and the same body is drawn from boxes in the
atlas's own stone, plaster and timber: a plank floor, a stone footing, plaster
walls with timber posts and a beam at 2.35 m, the door standing open, and no
roof. The walls between the camera and the room (one from straight on, two from
a corner) are lowered to their 0.35 m footing; they follow the camera as it
orbits. Inside (`model::interior`):

- one bed per `HomeView::beds` entry, the shipped `bed_single_A` at 0.66 (1.06
  by 1.98 m), along the two side walls with heads to the back wall, filled from
  the back. Two beds in a row fit a 5 m deep body; more beds than that, or a
  bed beside the door, are drawn smaller, so any capacity fits;
- a child's bed is three quarters of that size, and at night it holds a small
  sleeping figure: a mound under the blanket and a head on the pillow. Children
  are records in the simulation with no position, so by day the bed is empty;
- a hearth against the middle of the back wall, where the cottage's chimney is,
  a table and two stools between the hearth and the door;
- the people inside. Whoever is asleep (`Repose::Asleep`) lies in their own
  bed: the civilians, who have no lying clip, are laid on their backs; the
  adventurers play `Lie_Idle`. Everybody else, resting awake or up, is by the
  hearth, at the table or inside the door; an adventurer resting there sits on
  the floor, which is that rig's rest clip.

The inn is drawn from `view::inn`: the body the simulation blocks for it, one
bed per guest bed (four, `INN_BEDS`; the two beside the door are drawn at 0.85)
and each guest asleep in the bed the simulation gave them.

The hearth's fire glows and lights the room when the home is heated: 5,000
lumens by day rising to 18,000 at night. An unheated home has banked embers and
a candle's worth, 3,500 lumens at night and nothing by day, so sleepers show at
night either way. An inn's fire is always in. The light is a shadowless point
light and also falls on the ground outside the walls.

Nothing of an inside exists while no home is cut away: opening a two-bed
cottage spawned 69 entities in a capture run (its walls twice, standing and
lowered, the furniture and the two bed models) and closing its card despawned
exactly those. The pass that decides returns after reading the camera, the
selection and the open card.

Measured with BRP `npc/perf` in `capture_houses.py`, dev profile, 1280 x 720,
the world paused, over four seconds each (2026-11-11, one machine, one run):

| System (`island_homes`) | Nothing cut away: p50, p95 | One home cut away: p50, p95 |
|---|---|---|
| `residents` (42 residents) | 0.028 ms, 0.044 ms | 0.025 ms, 0.041 ms |
| `keep_hidden` | 0.011 ms, 0.018 ms | 0.007 ms, 0.012 ms |
| `cutaways` | 0.004 ms, 0.007 ms | 0.042 ms, 0.063 ms |
| `dress` | 0.003 ms, 0.006 ms | 0.003 ms, 0.006 ms |

Together 0.05 ms of a frame while nothing is cut away and 0.08 ms with one
home open. The mean frame was 19.4 ms with nothing cut away and 21.3 ms with
one home open; two earlier runs of the same script gave 17.9 and 28.0 ms, and
23.5 and 22.9 ms, on a machine where other builds were running, so the frame
time does not resolve the difference. `ISLAND_VIEW_STATS=1` logged a mean of
21.8 ms for the scene with nothing cut away (18,527 entities).

### What the captures showed

`capture_houses.py`, one run on a new island: at sunrise all 42 residents are
indoors and nobody is drawn; by 08:40, 35 are out. Residents rest around
sunset: of the residents indoors at about 23:00 (ticks 16,920 to 17,080 of a
24,000 tick day), six to nine were resting in bed, each for a rest of about
300 ticks; at 02:24 all 42 were indoors and none was resting, so a home opened
in the small hours shows its people up by the hearth, not in bed. That is the
simulation's rest, which the view only draws. At 23:00 the light is still that
of sunset, so the sleeper was also captured with the light pinned to midnight
(`--pin-night`). Nobody with a home rested outdoors at any of the three times.

Scenery keeps clear of the houses: no scenery tree stands within 2.2 m of the
body of a founding home or inn, and no outcrop rock or bush within a metre
(`model::on_founding_house`). Homes built later are placed by the simulation
on ground it finds free; a scenery tree can still stand against one.

## Workplaces

Solid workplaces are fitted to their bodies (`island_world::workplaces`).
Across the ground a model is scaled until the box of its ground-level walls
fills the body along its longer side, unless that would make it taller than
the cap (7.0 m; the mill 9.0 m), in which case it shrinks evenly. Upwards it
may be stretched by at most 1.25 times its ground scale, only as far as a
door 0.85 of a 1.8 m resident (1.53 m), and never past the cap. Where the
door still falls short it is left short. It is turned so its door or working
side faces the entrance. `WorkModel::across_trim` holds per-model trims
judged from captures (none so far: every model keeps the rule's fit). A
founding workplace's body is the 5 by 5 m blocker behind
its site (the simulation has no accessor for it; `view::inn` covers the inn);
a built one's is its plot. The body stays what the simulation blocks and what
a click picks; where the walls leave some of it, the model's own barrows, logs
and crates and the workplace's stock pile stand there.

| Workplace (model) | One-scale fit (before): scale, height, door / 1.8 m resident | Now: across, up, height, door / resident |
|---|---|---|
| Smithy (`blacksmith`) | 3.88; 3.8 m; 1.22 m, 0.68 | 3.88, 4.85; 4.8 m to the chimney; 1.53 m, 0.85 (0.848) |
| Mill (`windmill`) | 6.0; 8.8 m; 1.89 m, 1.05 | 6.12, 6.12; 9.0 m to the sails' top; 1.93 m, 1.07 |
| Timber yard, carpenter (`lumbermill`) | 4.12; 7.0 m; 1.30 m, 0.72 | 4.12, 4.12; 7.0 m to the hoist; 1.30 m, 0.72 (held by the height cap) |
| Mine head (`mine`) | 2.60; 2.9 m; adit 1.09 m, 0.61 | 2.60, 3.26; 3.7 m; adit 1.37 m, 0.76 (held by the 1.25 lift) |
| Bakery (`tavern`) | 4.55; 6.4 m; 1.43 m, 0.80 | 4.55, 4.86; 6.8 m; 1.53 m, 0.85 |
| Farmstead, tailor, tannery (`market`) | 2.78; 2.7 m; 1.00 m, 0.56 | 2.78, 3.47; 3.4 m; 1.25 m, 0.69 (held by the 1.25 lift) |
| Hunting lodge (`home_B`) | as the inn: 7.5 m; 2.20 m, 1.22 | unchanged |
| Well, fishing hut | 2.4, open | unchanged: the well's wall is 0.92 m high |

The kit's workshops are miniatures of buildings wider than the 5 m body: a
door a resident walks through needs either a stretch or an overhang. The
first fit stretched them by up to 1.5 (the bakery 8.3 m and the timber yard
8.1 m high, over the inn); the second used one scale (doors 0.56 to 0.80 of a
resident). The rule now allows a lift of 1.25 under a 7 m cap: the bakery and
the smithy reach 0.85, the timber yard, the mine and the market stay short.
Door heights are from orthographic renders, good to about 0.02 model units.

`WorkplaceFit` on the root carries the model's place, so the smithy's forge
chimney and fire are found on the fitted model. A workplace's stock pile stands
0.75 m further out than before, clear of the wall (the inn's barrels touched
it). Scenery keeps clear of buildings however they came: no scenery tree
within 2.2 m of a founding body, and trees, rocks and bushes are hidden on the
plot of anything built or being built later, two cells around a solid one
(`sync_dressing`, `dressing::sync`, on a change of the civic state).

## Fields

`island_world::fields` draws a field from `view::fields`, twice a second: the
simulation gives the field's body and each plot's stage, not where a plot lies,
so the view lays the plots out as a grid on the body (four by two on the
shipped 8 by 6 m field), in plot order from the entrance side. A plot is a bed
of soil and four crop rows:

| Stage | Soil | Rows |
|---|---|---|
| fallow, harvested or lost | pale bare soil; frosted in winter, when fields rest | none |
| sown | dark turned soil | pale green lines, 6 cm |
| growing | dark | green, from 16 cm to 76 cm in ten steps of growth |
| ripe | dark | 95 cm: gold for grain, red for any other crop |

`FieldView::lost_plots` and `harvested_units` are totals; a plot that was lost
or harvested is a fallow one. Four corner posts mark the body. The field's own
store of harvest is not drawn.

## Boat

`island_world::boat` draws the merchant boat from `view::boat`, which says
whether it is in, when it leaves and when it next lands, where the trading
post's entrance is and where the boat ties up (`berth`, the sea cell nearest
the post at the waterfront). The boat is moored on that berth when its whole
hull floats there on open water (water cells with no land round them, since
the beach reaches a cell into the sea, half a metre to spare), else straight
out from it the least way that does, up to 14 m; it never lies on the beach,
the land or the post's ground. The view decides which way the sea lies there
(water cells within five pull, land cells push; from the post to the berth
when the shore is even all round) and logs it (`merchant boat berth: x, z; N m from the trading post's
entrance`). No coordinate is the view's own. It sails the boat in over the
500 ticks before a landing from
46 m out, bow first, turns it broadside for the mooring, and sails it out for
500 ticks from the tick it sees it cast off. A boat that
is due and has not landed (a storm keeps it out) stands off most of the way
in. The mooring knows the hull only by `boat::HULL` (6.4 m long, 2.4 m
beam): the boat is one root, bow along +Z and waterline at its origin, with a
`BoatHull` child (a stand-in of blocks with cargo on deck) and a `BoatSail`
child pivoting on the mast, so a ship model drops in as those children. The
root heaves, rolls and pitches on the ambient clock (more in a blow, the bow
more under way) and the sail child swings to the wind's side and shivers;
neither reads any mesh or clip. Under the fog it is drawn only where the
played actor sees now (it moves; a remembered boat would be a lie).

`island_world::harbour` lays a jetty from the shore to the simulation's berth
once per berth (a test checks that the hull floats clear on open water on
the simulation's berth or straight out from it, away from the post, and that
the jetty's foot is on dry land; where the trading post's entrance is
within 6 m of the jetty's foot, as on the living world, the post is the quay
and no quay of planks, stores or unloaded goods is drawn): its head 1.6 m off the hull's landward side, its foot on the edge of
the land cell nearest that head, planks across two stringers on posts every
2.2 m down into the water, deck at least 0.32 m up, in the atlas's timber. At
its foot a quay of planks 4.6 by 3.4 m holds crates, barrels and a sack
(`hex/` models) and a lantern on a post that lights with the path lanterns.
While the boat is moored and in sight, five crates, a sack and a barrel stand
on the quay by the jetty's foot; they go when it sails. The jetty and quay are
fogged as dressing. The host logs `harbour jetty: foot x, z; head x, z; L m
long; deck at y m`.

## Models of the sixth pass

The island draws CC0 models from `assets/living/{ships,animals,nature,props}`
(only paths listed in `assets/living-world-manifest.json`; nothing under
`assets/living/**` is changed, colours are changed in code).

- **The merchant ship** is Kenney's `ship-small.glb`. Its bow is along +Z
  (read from the hull: it narrows to a bowsprit at +Z and the raised stern
  is at -Z) and its keel is at 0. It is drawn 1.35 times its size, 11.9 m
  long, with the keel 1.15 m under the water. Its wood is weathered from
  the kit's light peach to oak through a copy of its palette texture in
  which only the warm wood colours are remapped (`tint::Recolour::Wood`,
  `tint::weathered`): by luminance, the saturated salmon of the hull to a
  dark oak and the pale peach of deck and trim to a mid brown. The texel
  comes out lighter on screen under the noon sun, so the browns are dark in
  the texture: at noon the sunlit hull samples about (146, 122, 102) in
  sRGB and the shaded side about (101, 89, 83)
  (`capture_world.py --only look8`, `hull_sample`). Sails stay off-white,
  iron grey, rope pale. The rowing boats and the pier
  sections take the same copy. The sail node `sail-a` comes down to its yard
  (scaled to 0.14 under the yard) while moored and is set to sail in and
  out. The pennants `flag-a` and `flag-b` stream downwind in the ship's
  own frame and flap. `HULL` is 11.9 x 6.6 m. The mooring floats the
  whole ship on water with no land within two cells (a metre to spare at
  bow and stern) and no land within 10 m of the pier's head
  (`boat::pier_room`, `PIER_WATER_M`), up to 34 m out from the berth, and
  the jetty's head stands 3.6 m off its middle.
- **The harbour.** The jetty is the ship kit's narrow pier sections on
  piles (`structure-platform-dock-small`), end to end from the foot to the
  head (`harbour::dock_sections`), at least ten metres from the strand.
  Mooring posts (`Environment_Dock_Pole`) stand every three metres on
  alternating sides and in a pair at the head (`harbour::pier_posts`). Two
  rowing boats, 3.6 m long: one pulled right up on dry land 6 m to one
  side, one afloat beside the inner half of the jetty, clear of it and of
  the ship. There is an anchor, a bucket of fish and a blue flag on land
  at the foot, on the side away from the beached boat. The quay goods are
  static: no view of the trading post's stock exists. A second flag stands
  beside the trading post's entrance, moved 2.5 m along if it would stand
  on the first. The flags turn downwind (`ambient::WindFlag`). Field
  fences, gates and the wheelbarrow take the same weathered wood, and the
  Kenney signposts and tree bark use oak browns (`kenney_colour`).
- **Deer** are Quaternius' `Deer` and `Stag` (`island_world::wildlife`),
  under each animal's `AnimalBody`, moved and played from the simulation's
  reading (`view::wildlife`, read once a frame into `WildlifeStates`).
  Every fourth animal by SimId is a stag, as the simulation has it. They
  face +Z, so they are turned half round to the body's -Z, and are drawn at
  0.35 (doe) and 0.33 (stag): a doe's shoulder is about a metre.
  - **Position.** The simulation steps a moving animal every five ticks.
    The visual root follows the simulation's position at the animal's own
    ground speed, a little faster the further behind (`wildlife::follow`),
    so it walks on between steps instead of starting and stopping; a load
    far away snaps. It stands on its cell's height. The pause stops it;
    Reduce motion does not, since the movement is the game's state.
  - **Heading.** While walking or fleeing the body turns toward its
    target (target - position) at 2.5 rad/s, 6 fleeing (`turn_toward`,
    `heading_yaw`); it never slides sideways. Standing, it keeps its last
    heading; before it first moves, its own (`herd_place`).
  - **Clips.** Grazing: a 26 s round of `Eating`, `Idle`, `Idle_2` and
    `Idle_Headlow`, each animal at its own phase; `Idle_2` while a hunt
    aims at it. Walking: `Walk`, fleeing: `Gallop`, each played at its
    ground speed over the clip's stride (1.1 m/s walk, 5 m/s gallop;
    `clip_speed`), so 25 mm a tick plays the walk at 0.45 and the 160 mm
    gallop at 0.64. Dead: `Death` once, held on its last frame. Paused,
    every clip holds; with Reduce motion the grazing round holds still.
  - **Spacing.** The simulation keeps its animals 1.5 m apart. The view's
    spacing (`space_herd`, `spread`: does 1.8 m, stags 2.5 m, clear of
    trees, bushes and rocks) is now only a nudge of at most 0.7 m
    (`MAX_NUDGE_M`), eased in while an animal stands and out while it
    moves, so it never fights a moving herd.
- **The woods** are Kenney's faceted pines
  (tall, default, round), oak, broadleaf and thin tree (`living_scene::FOREST`,
  4 to 9 m). The kit's smooth firs and fir clusters read as placeholder
  cones among them and are no longer used. Pines dominate the woodland and
  broadleaves the open land, and the choice is stable in each cell's hash.
  The broadleaves turn gold and rust in autumn by their recoloured
  materials; the pines stay green. The simulation's wood patches are drawn
  as a Kenney pine (one per patch, still selectable, labelled "Fallen
  wood") and berry patches as a Kenney bush. Under them grows
  undergrowth on its own lattice (`UNDERGROWTH`): bushes, grass tufts,
  flowers, mushrooms, stumps, fallen logs and stones.

  All scenery keeps off roads' walkable strips (0.8 m beyond), doors,
  sites, landmarks, founding bodies and a 9 m bank round each fishing spot
  (`scenery_blocked`, tested on the living world). It follows Shape-mode
  edits like the old scenery (`SceneryCell`).

  Kenney's teal greens are recoloured by material name to the island's
  greens (`kenney_colour`): broadleaves turn in autumn, winter dulls the
  leaves and browns the flowers. The colours follow the season the land is
  drawn in. Decorative trees are the view's own and cannot be selected or
  harvested; the simulation's wood patches are drawn as before, one tree
  with a "Fallen wood" label each.
- **Fields** are fenced with the kit's wooden fence, waist high, with a gate
  in the middle of the entrance side and a wheelbarrow outside it.
- **Construction sites** show the kit's building stages, fitted across the
  body: the foundation, the frame, then the scaffolded walls by thirds of
  the work done (`island_build::scene::stage_for`). The delivered lumber
  and stone lie beside them as before.
- **Trades' goods.** Sacks stand by the mill's entrance, and a stack of
  sawn wood by the woodcutter's and the carpenter's.
- **Inns** fly a red flag at the front corner opposite their sign.
- **Worn earth.** The ground is trodden round every door and site
  entrance (1.3 m) and along desire lines from each workplace to its
  nearest neighbour (`ground::Roads` keeps these apart from the roads the
  generator laid). Scenery keeps off them too.
- **Signposts** (Kenney's `sign`) stand where the road from the landing
  enters each settlement, 13 m out from its centre and 2.2 m to the side,
  facing the arrivals (`living_scene::signpost`).
- **Snow lies unevenly** (`ground::snow_cover`). Drifts are a little brighter
  and wind-scoured patches up to 70 % bare, the dull turf showing through,
  so a winter island is not one white sheet.
- **Homes inside** gain a timber mantel over the hearth, firewood beside it,
  a red woven rug before it and a shelf of clay pots on the wall away from
  the door.
- **The fisher** has a line from the rod's tip to the water 2.9 m ahead
  while the fishing clip plays (`island_poses::fishing_lines`).
- **Mills.** The watermill's wheel now turns about its local Z (the
  scout's node inspection), like the windmill's sails; the saw turns
  about X.

Cost of the sixth pass: about 2,100 more mesh entities (7,550 against
5,400; 5,300 against 3,300 in view at island zoom). The new systems add
about 60 us of CPU a frame:
- the deer clips, 19 us;
- the small life, 12 us;
- the flags, 8 us;
- the extra birds, 6 us;
- the crop ripple, 5 us;
- the fishing lines, 5 us;
- the Kenney seasons, 3 us.

The frame time (`target/npc-out/world6/budget-table.md`, ambient off,
pass 5 and pass 6, two rounds each) could not be read to 10 %: the machine
was shared with other agents' builds, and one build's rounds differ by up
to 60 %. The best of each build's rounds at island zoom was 29 ms (pass 6)
against 33 ms (pass 5) at dawn and 26 against 27 at noon.

**Light.** The sun's shadow cascades follow the island camera
(`post::fit_cascades`). The first cascade reaches 0.9 x the camera
distance (10 to 70 m) and the farthest 3.2 x it (60 to 520 m), so Live's
14 m view keeps sharp shadows and the overview keeps all of its shadows.
SSAO is not on: Bevy's SSAO needs MSAA off on the camera, which would
bring back the jagged edges the island relies on MSAA to hide.

**Ponds.** The pond's water is the sea's surface under the land, so the
swells, their light and the breathing foam of the shallows play on it too.

`ISLAND_VIEW_SEASON=spring|summer|autumn|winter` pins the season the view
draws the land, the woods and the small life in, for reviews; the
simulation's calendar is untouched.

### Eighth pass

- **Winter plants** (ninth pass). The evergreens (`leafsDark`) and the
  shrubs (Kenney bushes and berry patches, marked `KenneyShrub`) stay a
  dark, muted evergreen (`kenney_colour_on`); the broadleaf trees go bare
  grey and grass turns to straw. The woodland edge bushes take the same
  evergreen (`dressing::bush_tone`; autumn browns them). In winter all of
  them wear a snow-dusted copy of their mesh: vertex colours that turn the
  flattest tops most of the way to snow (`tint::snow_dusted`,
  `living_scene::dust_with_snow`, `dressing::bush_season`), swapped back
  when the season turns. Tested.
- **Farm timber** (ninth pass). Fences, gates and the barrow take
  `tint::field_wood`, a mid brown lighter than the ship's oak, not the
  hull's weathering.
- **The field gate** (tenth pass) takes `tint::gate_wood`: the farm
  timber, its pale grey crossbars a slightly darker brown.
- **Builders face the house** (tenth pass). A builder or friend working a
  homestead upgrade stage at the door (`UpgradeWork`) plays the hammer and
  turns to the house (`island_poses::face_site`, `site_yaw`). A hired
  builder standing still within 4.5 m of the door between stages (the
  simulation's "waiting") faces the house too, without a work pose
  (`waits_at`).
- **Homestead building site.** While an upgrade is under way, the materials
  delivered are stacked by the cottage and the new part's sill beams lie behind it.
  The scaffold stands once any work is done or everything is in. The new part (1.4 m
  behind the back wall) rises with the work fraction (`work_ticks` against
  `remaining_ticks` of `view::upgrade_site`): frame posts over the first fifth, plank
  walls climbing to four fifths, roof boards last. All of it comes down when the site
  is finished (`island_homes::upgrade`, `extension_parts`, `scaffold_up`).
- **Trades from data.** A trade that only the world's catalog defines (`Role::Trade(key)`) has no special case anywhere in the view:
  - its legend hue is hashed from its key onto `markers::TRADE_PALETTE`, six hues at least 0.15 from every shipped trade's;
  - its workplace is the generic workshop (the market model, `WorkModel::Market`);
  - its worker plays the hammer and is heard as one;
  - its building's caption is the simulation's place label.

  Every match on `Role` or on a good's key in the view has a fallback arm. An unknown good lies as a crate in a store, and is not drawn carried (as coins and tools are not).
- **Placed props.** `island_world::props::sync` follows the simulation's placed props (`frontier::props::props`) twice a second. It draws new or changed props and takes removed ones away. Each prop:
  - stands at its body's middle (`body_min..body_max`) on the cell's height;
  - is turned by its quarter turns;
  - is drawn by its catalog model when the manifest lists it, else by words in its key (`prop_look`: barrel, bench, crate, stone wall, gate, fence, sign, flower bed, log pile, bucket, tent, barrow, ladder, pallet, sack, flag, rock), else as a small crate.

  Fences are turned to run across the front and put back on their middle. A flower bed has a flower on every cell. Fences and barrows take the farm timber, Kenney models their recolouring. `placement_look(world, key)` answers the Shape ghost for a prop (look, footprint, solid) or a farm or garden plot (footprint, plots). Placed fields and gardens are ordinary field buildings, drawn and removed by `fields.rs` like founding fields. A workplace of a data-only trade on an open site is a yard (`island_build::scene::draws_workplace`).
- **Wet buildings.** Rain darkens the buildings' models by up to 26 % and
  makes them glossier (roughness halved) with the ground's wetness
  (`weather::wet_surface`, `wet_buildings`), applied in 5 % steps so the
  materials change a score of times over a shower, not every frame.
- **The harbour's rowing boat afloat** rides the swell on its painter,
  livelier than the ship (`harbour::Afloat`, `small_ride`, `bob`), on the
  view clock.
- **The pier's deck** (ninth pass). The kit's dock section is a frame whose
  own boards lie low, under the sea; seen through the translucent shallows
  at the foot they made a blurred streak. A planked walkway 1.8 m wide now
  lies on the frame (`timbers`, the ship's brown family) and covers it.
- **Civic flags.** A depot barn and a market stall fly a yellow flag at the
  front corner away from the door (`island_build::scene::flag_corner`).
- **Winter smoke.** A heated home smokes in winter with the snow's darker,
  denser plume (`smoke::plume`); a view test feeds a heated home in winter.
- **The grain mill** is drawn as a windmill (the miller's model); its sail
  rotor is the wheel that turns while a miller works it.

## Ambient motion

Something other than people moves at any camera, in any mode, the title's
backdrop included: `island_world::ambient` keeps one `Wind` (direction,
strength with gusts, and the view clock `time`) that every moving thing
reads in the same frame, written once in `PreUpdate`.

- **The wind.** Its strength follows the simulation's weather (0.7 clear,
  1.3 rain, 1.0 snow, 2.6 storm, eased in with the weather's amount) with
  gusts of about a fifth over seconds; its direction swings slowly over the
  day (about 1.8 rad from dawn to dusk), not in seconds.
- **The clocks.** Atmosphere runs on the view clock, real seconds times
  `AmbientMotion::scale`: clouds, their shadows, trees, the sea, rain and
  snow, hearth smoke, the boat's ride and sail, lights' flicker and the
  birds keep moving while the game is paused and are not sped up by 3x or
  10x. Sim activity follows the smoothed sim clock (`interp_alpha`) and stops
  with it: mill sails, water wheels and saws (`Spin`: up to pace over 90
  ticks while a craft is worked there, a run-down after, one turn in 240
  ticks), a forge's smoke, sparks and glow, and the deer's idle.
- **Reduce motion.** `AmbientMotion { scale }` (crate path
  `crate::island_world::ambient::AmbientMotion`; default 1) in front of all
  of it: at 0 the view clock stops, machines and animals stand, and no
  transform changes (tested).
- **Clouds** drift downwind, the high ones faster, and wrap round a square
  2.9 island-radii across, shrinking to nothing near the wrap instead of
  popping. Ten clouds, four of them over the island; the weather's cover
  (0.45 clear to 1.0 storm) brings the later ones in and thins them out.
- **Cloud shadows** are the main effect at island zoom, where nobody sees
  the sky. They are drawn from their own tileable texture of soft-edged
  patches over about a third of the ground (240 m a tile), on a square 14 m
  over the sea. Their core is 50 % of a grey-blue: about a fifth darker on
  grass and clearly grey-blue on snow. They ride downwind at about 1.6 m/s
  in a fresh breeze, so one crosses a house in some five seconds. They fade
  in from 16 m camera height over 22 m. They are fainter as the sun goes
  down and gone at night, and gone under a full overcast or a storm.
- **Trees and bushes** lean downwind with the strength and sway along the
  wind. Each tree has its own phase and its own pace (0.82 to 1.18), so a
  wood shimmers rather than moving as a block. A crown 6 m up moves some 25
  to 40 cm in a fresh breeze. The foot stays and the crown moves.
- **Crops** ripple: each plot's rows lean in a wave that runs downwind over
  the field (`ambient::Ripple`).
- **The sea.** Long swells run downwind over it (`swell_mesh`), with a
  cross-sea and a short chop. Their crests are lighter and their slopes
  catch the light, larger in a blow. The shallows' foam breathes with the
  swell (`foam_breath`): whiter and denser as it runs up the beach, back to
  clear water as it draws off. In winter the beach is wet sand, not snow,
  so the waterline reads.
- **Smoke** bends downwind and stretches with the strength (`smoke::Bend`);
  it rises only from occupied or heated homes and from worked forges, as
  before. **Rain and snow** slant with the wind, tops upwind.
- **Lights.** Path and harbour lanterns, window glows and hearths flicker
  gently (±10 %), each at its own rhythm.
- **Birds.** Eight gulls circle the harbour by day and three follow the
  ship. Two flocks of five small dark birds cross between the trees by day
  when the camera is within 80 m. A bird is a tapered body with two wings
  of two panels each: the inner one swept back a little, the outer one more
  and bent down, grey on a gull (`ambient::bird_meshes`). Gulls glide with
  their wings held a little up and flap a few beats every six to nine
  seconds (`gull_wings`); small birds whirr. Each part shares one mesh per
  kind.
- **Small life** shows only with the camera within 45 m, drawn a little
  larger than life so it reads at 14 m: ten butterflies
  over the grass on summer days, 36 fireflies drifting and blinking on
  summer nights, and 30 leaves falling downwind through autumn.
- **Animals.** A deer's parts hang from one `AnimalBody` under its visual:
  it breathes and now and then dips its head to graze, on the sim clock.

Not done: pond and well shimmer.

Cost. `ISLAND_VIEW_NO_AMBIENT=1` starts the island without the wind, the
cloud shadows and the birds (every consumer falls back to the earlier fixed
breeze), so the same binary measures the motion. Two rounds each, paused, at
1280x720 (`capture_world.py --only budget`,
`target/npc-out/world5/budget-{on,off}-{1,2}/`): the frame was 34.9 ms
without and 37.6 ms with, +8 % on average. Per camera it ranged from -8 % to
+26 %, the storm at island zoom being the worst. The figures swing by up to
40 % between rounds on the same build, because other builds were running on
the machine. The CPU systems add about 0.1 ms in all: `wind_and_water`
+43 us (ten clouds instead of six), `cloud_shadows` 12 us, `birds` 11 us,
`update_wind` 7 us, the deer idle 5 us and smoke 7 us. The rest of the
difference, where it is not noise, is drawing: one translucent
cloud-shadow square over the view at island zoom, four more clouds and seven
birds. The renderer's draw calls were not measured.

## Weather

`island_world::weather` reads `view::weather` twice a second. The simulation
has clear days, rain, storms and snow, and three shocks; it has no fog.

- **Falling.** Rain, a storm and snow are sheets: one mesh of 760 pairs of
  crossed quads scattered through a box of air 64 m square and 26 m high,
  drawn twice, one copy above the other, sliding down round the camera's focus
  and wrapping, so the fall is endless and costs two entities. The box grows
  with the zoom, so the streaks still show from the overview. Rain is thin
  1 m streaks at 19 m/s leaning a little; a storm is longer, faster and leans
  17 degrees; snow is 13 cm flakes at 1.9 m/s that wander a metre sideways.
  They are unlit and follow the sky's brightness, so they do not glow at night.
- **Sky.** `weather::sky_change` multiplies the sun and the sky light, brings
  the haze nearer and mixes sky and haze towards a grey that follows the hour:
  rain 0.4 of the sun (the sky light stays, so shadows soften) and haze at
  0.42 of its clear distance, a storm 0.22 and 0.28 with a double flash of
  lightning every 7.3 s, snow 0.55 and 0.32 with a pale haze. A drought is warm and dusty, a harsh winter cold, a blight
  sallow. It eases in over 2.5 s.

In clear weather nothing is spawned and the falling pass returns at once.

The second pass (`island_world::weather`, `fields`, `living_scene`):

- **Overcast.** Rain takes the sun to 0.14 and lifts the sky light to 1.3,
  a storm 0.06 and 0.95, snow 0.25 and 1.4: shadows all but go and the day
  is carried by the sky light, through the same daylight pipeline, so a night
  and the season's colours still read.
- **Streaks.** 14,000 pairs of quads in a box 150 m square and 26 m high
  centred under the camera, not on the focus. Their size is in world metres
  and never grows with the zoom: rain 2.2 cm by 0.55 m, a storm 2.6 cm by
  0.75 m leaning 18 degrees, snow 10 cm. Full streaks keep 15 m from the
  camera's upright (the sheets lean about a foot moved so the leaning
  upright passes through the camera); between 4.5 and 15 m a second sheet of
  900 streaks at most 0.2 m long falls. So no streak is drawn longer than
  6 % of the picture's height (45 degree view) at any camera height: a
  storm streak 15 m off is 6.0 %, a short one 4.5 m off 5.4 %
  (`weather::screen_share`, tested).
- **From high up.** Above 60 m of camera height over the focus the streaks
  fade out over the next 30 m and a haze fades in: one square over the focus,
  25 m up, wide enough to fill the view, textured with soft patches made of
  whole waves across a 240 m tile (generated in code, no asset; a slow swell
  thins some patches and thickens others), laid twice: a second square 1.5 m
  higher carries the same texture on a 370 m tile turned 41 degrees, each
  with 0.62 of the opacity, so the two never line up into a repeat over open
  sea; both drifting with the wind at 3.5 m/s, grey under rain (0.34
  opaque at most), dark under a storm (0.46), white under snow (0.4). It
  reads as cloud shadow and murk; light, sky and ground tint carry the rest.
- **Lightning.** `ISLAND_VIEW_FLASH=hold` holds a flash at its peak while a
  storm is drawn, for a capture of what a flash does (in play it is a double
  flash every 7.3 s).
- **Snow on roofs.** While snow lies on the ground (from 5 % cover, full at
  60 %), a pale layer lies on both slopes of every home (`island_homes::snow`):
  one mesh of the cottage model's two roof planes, a hair over the tiles,
  hung under each house model so it follows the fit, the cutaway and
  unblocking, and one shared material whose opacity follows the ground. The
  inn, the lodge and the workplaces have roofs of other shapes and get none.
- **Ground.** The land material's colour is multiplied: wet ground darker and
  cooler with a sheen (over 15 s, drying over 60 s); snow settles over 40 s
  as a paler colour and a glow that follows the sky light, and melts over
  90 s; a harsh winter keeps deep snow down without falling snow; a drought
  parches it straw-warm, a blight sours it. Paths darken with the rest: they
  are vertex colours of the same mesh, not a material of their own.
- **Fields.** A drought turns green crops straw-brown and cracks the soil
  pale; a blight withers crops to grey-brown (`fields::shocked_colour`).
- **Wind.** Under a storm scenery trees sway 4.5 times as far, faster, and
  lean with one wind; rain stirs them a little.

### What the world captures showed

`scripts/playtest/capture_world.py`, 1280 by 720, debug build, on a machine
that two other builds were loading at the time.

| Run | `weather::fall_sheets` p50, p95 | World, homes and pose systems, sum of p50 | Mean frame |
|---|---|---|---|
| clear | 5.0, 8.4 us | 77 us | 34.9 ms |
| rain | 9.9, 15.3 us | 73 us | 28.5 ms |
| storm | 10.6, 16.7 us | 122 us | 31.2 ms |
| snow | 6.1, 12.7 us | 53 us | 20.7 ms |

The falling pass costs about 5 us more a frame when something falls. The
storm's higher sum is the homes pass (`island_homes::residents`, 54 us against
about 20). Frame time does not resolve the weather on that machine: the clear
run was the slowest of the four, and the `ISLAND_VIEW_STATS` means of the same
runs (33, 48, 51 and 69 ms) include the night's lights and frames of up to
0.9 s while other builds ran. What the two sheets cost the graphics card
(6,080 blended, unlit triangles) was not measured apart.

- **Workplaces.** The captures in `target/npc-out/world/` (`workplaces-*`,
  `scale-*`) show the earlier fit, stretched upwards, not the one-scale fit
  the table above describes: that one has not been captured. In them a
  carpenter, a tailor, a tanner and a well keeper stand beside their
  workplace; no smith or miller was caught.
- **Door.** A resident in an open doorway at 17 m, and the dark opening in the
  street at 30 m. That run holds the door for six seconds
  (`ISLAND_VIEW_DOOR_HOLD=6`); as shipped it is open for the step and 0.6 s
  after it, which was not caught in a still.
- **Field.** Eight plots growing (green rows), then three ripe (gold) beside
  five fallow, then one growing beside seven fallow, each told apart at 50 m.
  The run's rules let crops be sown in autumn and ripen in half a day.
- **Boat.** Sailing in, moored broadside and sailing out, at its berth.
- **Weather.** Rain, a storm and snow by day at 45 m and 22 m and from the
  overview; rain and snow at night. Night and day still read under each.
- **Sleepers.** At 02:24 all 42 residents are indoors and at rest; one of each
  rig lies in their own bed in a home cut away by a click.

Not seen: a lightning flash; a harsh winter, a drought or a blight; a
workplace or a field built during play (the fitting and the scenery clearing
are covered by tests only); the Live character.

### Second pass (`target/npc-out/world2/`)

`capture_world.py --only scale|live|harbour|rain|storm|snow|harsh`, debug
build, 2026-11-11, a machine shared with two other builders:

- **Workplaces.** Every model from its entrance side with the resident
  nearest its door (0.6 to 2.5 m away), and the three settlements from the
  front at 66 m and 40 m. No workshop dwarfs the inn; the market stalls and
  the mine head are the lowest. No per-model trim was needed.
- **Door at 1x.** A resident stepping out of Harbour cottage 5 with the leaf
  open, and 0.6 s later still open, at the shipped hold (no
  `ISLAND_VIEW_DOOR_HOLD`).
- **Live.** The played character (the `human` preset names `rogue`: it is
  the KayKit hooded rogue, not a robot) with its warm player ring. A home
  between a low camera and the character: in Oversee its chimney hides them;
  in Live, at the same camera, it is down to its footing. Trees between the
  camera and the character: standing in Oversee, gone in Live. Not seen: the
  character at a door, and the homestead cottage (the world has none until
  the player builds one).
- **Harbour.** A 5.9 m jetty and the quay, empty; the boat moored at its head
  with goods on the quay; the quay empty again after it sailed.
- **Weather.** Rain, a storm and snow by day at 45 and 22 m and from the
  island; rain and snow at night; a harsh winter certain from the first winter
  under clear skies. Overcast light, thin streaks, darker wet ground. In the
  harsh winter the season's own winter ground is already pale, so the shock's
  extra snow cannot be told from it in a still. No lightning flash was caught
  in ten captures a quarter second apart; no drought or blight was forced (the
  first season that can have one is two seasons after the start).
- **Poses.** A hammer in a smith's hand at the forge and a hoe in a farmer's
  at 8 m; a chop at 8 m was hidden by a home the close camera cut away. At
  40 m the tools do not read. No fisher was found in twelve tries.

Frame cost, `npc/perf` p50 over four seconds (the machine was loaded; mean
frames 32 to 67 ms): clear Oversee, world + homes + poses 89 us in all,
`unblock` 7 us (returns at once), `weather::ground` 3.7 us; Live with a home
out of the way, `unblock` 14 to 25 us (it tests the trees near the line of
sight only when the camera or the character has moved).

## Sound

`island_audio` (`content/island_view/src/island_audio/`) is the island's one sound
system. It loads only the manifest's `living/audio/{ambience,work,ui,music}/` files that
exist (`catalog`). The older, unlisted `assets/audio/` folder is retired: no code loads
it. Hosts without Bevy's audio plugin (every headless test) and runs with
`ISLAND_AUDIO_OFF=1` load and play nothing. The decisions are pure functions in `mix`,
tested without a device.

- **Volumes.** `AudioLevels { master, ambience, effects, music, ui }`, each 0 to 1,
  defaults 0.8 and 0.7. The older `IslandAudioSettings` (the frontend's settings,
  `--mute`, `--screenshot` runs) is copied into it when it changes: master, ambience
  and effects, with a mute as a master of 0. Each sound also has its own gain in one
  table (`mix::sound_gain`), set from the measured loudness (wind about -37 dBFS raised,
  rain about -22 lowered), to be tuned by ear.
- **Ambience.** Eight layers cross-fade over about 2.5 s, from what the view reads.
  Every half second the view takes the camera focus's distance to open water, the
  trees, residents and lit hearths and forges within 25 m, and the weather, wind and
  lamp curve.

  | Layer | Source | Follows |
  |---|---|---|
  | Waves | made in code | nearness to the coast; louder in wind |
  | Wind | `wind`, two copies half a loop apart | the wind's strength |
  | Rustle | made in code | trees round the focus |
  | Rain | `rain` | rain; a storm is this layer, harder |
  | Birds | `birds_morning` | by day; fewer in rain and on open land |
  | Crickets | made in code | by night; none in rain |
  | Murmur | made in code | residents nearby; quieter at night |
  | Fire | `fire`, two copies | a lit forge or hearth close by |

  In a storm `thunder` sounds at uneven gaps of 12 to 40 s (`mix::thunder_gap`):
  `storm.ogg` is not a clean loop. Local layers (birds, crickets, murmur, fire) fall
  off with the camera's height, to a quarter from the Oversee camera. Paused or behind
  a menu, the ambience goes on at about half its level.
- **Work one-shots.** These are spatial, with the listener at the camera's focus:
  - poses: chop (`chop_*`), hammer (`hammer_*`; the builder counts), pick (`stone_*`),
    hoe (`dig_*`), fish (`splash_*`);
  - machines worked: saw (`saw_loop`), the smith's anvil (`metal_*`); the mill's creak
    has no file and is silent;
  - a galloping deer (`step_dirt_*` as hooves);
  - a door drawn open (`door_open`).

  Only within 45 m of the focus, only what the fog shows live, at most 8 voices at
  once, and each emitter at its own cadence with a pitch of 0.92 to 1.08 from a
  view-side hash. A stopped clock or a menu: none. At 3x and 10x the clock thins
  them (3 and 2 a second, longer waits) rather than pitching them up. Reduce motion
  does not touch sound.
- **Music.** `day` or `night` by the lamp curve, then a gap of 150 s plus 90 to 210 s.
  `jingle_morning` plays on a new day and `jingle_built` when a civic building is
  added. Music holds while a menu is open.
- **Interface.** The UI kit (`game_ui::sound`) writes `PlayUiSound(UiSound)` for
  clicks, hovers, panels opening and closing, refusals, placing, alerts, coins and
  pages. `island_audio` plays each once a frame from `ui/`. There is no second click
  path.
- **Log.** Each layer that becomes audible, each one-shot, each music start and each
  UI sound logs one `island_audio:` line, which `capture_world.py --only sound11`
  counts.

## Capture and review aids

These environment variables are read once at startup by the view. None of them reaches
the simulation, a save or a replay.

| Variable | Effect |
|---|---|
| `ISLAND_VIEW_DAY_PHASE=0.35` | pins sun, sky, lamps and smoke shade to a day phase (0 sunrise, 0.35 noon, 0.7 sunset, 0.85 midnight); the HUD and residents keep the real time |
| `ISLAND_VIEW_POST=off` | no HDR, bloom or explicit tonemapper, for comparison |
| `ISLAND_VIEW_EDGE_PAN=off` | no screen-edge panning |
| `ISLAND_VIEW_STATS=1` | logs entity, mesh, light and chunk counts and the mean frame time between 12 s and 16 s after startup, and one `shape stroke:` line per Shape stroke or undo (see below) |
| `ISLAND_VIEW_SHAPE_BATCH=off` | records uneven Shape edits one cell per command, as before `TerraformCells`, for comparison |
| `ISLAND_VIEW_DRESSING=off` | leaves out the outcrop rocks and woodland-edge bushes, to measure what they cost |
| `ISLAND_VIEW_DOOR_HOLD=6` | keeps a house's door open that many seconds after somebody steps through it (0.6 s in play) |
| `ISLAND_VIEW_PLAYER_BODY=mage` | draws the played character with that body instead of the Knight (`knight`, `rogue`, `unhooded_rogue`, `barbarian`, `mage`, or a villager `village_woman`, `traveller_woman`, `farmer_man`, `traveller_man`); the preset is not changed |
| `ISLAND_VIEW_CAST=mixed` | the residents' family of bodies: `adventurer` (default), `civilian`, or `mixed` (both kits by appearance at their old sizes, as before the one-family pass) |
| `ISLAND_VIEW_FLASH=hold` | holds a lightning flash at its peak (or at a strength 0 to 1) while a storm is drawn |
| `ISLAND_VIEW_SHOCK=drought` | draws a shock's sky, ground and field tints (`drought`, `blight`, `harsh_winter`, `none`) whatever the simulation's shock is |

`scripts/playtest/capture_view_followups.py` drives the player session over the remote
protocol and writes the Oversee, Economy, Shape, Settings and scene captures at
1280 x 720 and 1024 x 768 to `target/npc-out/view-followups/`. It places the camera by
writing `RtsCamera`, takes three captures of a running settlement a few seconds apart
and records how many residents moved between them. `scripts/playtest/measure_view_perf.py`
samples `npc/perf` in a developer launch and prints the systems matching a name.

`scripts/playtest/capture_view_checks.py` takes the unpaused captures in
`target/npc-out/view-checks/`: smoke at the real morning, dusk and night (the clock is
stepped there, the light is not pinned), a forge at work, residents chopping,
hammering, hoeing, carrying and idle at 30 m and 14 m, the save notice and the Live
tip. It finds homes, workers and smithies with operator state reads.

`scripts/playtest/capture_world.py` writes `target/npc-out/world/` in five
runs on copies of the rules with one kind of weather every day: the island at
noon and at night, each settlement's workplaces, a resident beside a workplace,
a door held open, a field early and later (in that run crops may also be sown
in autumn and grow in half a day), a sleeper of each rig; the boat sailing in,
moored and sailing out; and rain, a storm and snow by day, at night and from
the overview, with `npc/perf` for the world, homes and pose systems in each.
`--only main|boat|rain|storm|snow` takes one run and writes
`summary-<run>.json`.

`scripts/playtest/capture_houses.py` writes `target/npc-out/houses/`: a street
at sunrise and at night with a count of who rests indoors and outdoors, a
resident on a doorstep, a home cut away by a click and by a close camera, by day
and at night, the inn outside and cut away, and a cottage placed with one
recorded `PlaceBuilding` on a copy of the rules in which it costs nothing, built
by residents and cut away.

`scripts/playtest/capture_view_wiring.py` writes `target/npc-out/view-wiring/`: smoke
at the real morning and night, a forge from 30 m, 14 m and 10 m, two settlements at
night from the front (yaw 0, the player's default view) and from behind, one home
from three sides, and Shape mode before and after a drag. The other scripts place the
camera at yaw pi, behind the houses. `--mode shape` runs only the drag;
`--mode night --pin-night` is a quick look at the night scene.

## Shape strokes

The Shape drag in that script is real pointer input as far as the view can tell: the
script writes the window's cursor position and `MouseButtonInput` messages over the
remote protocol, and the view's own picking, dab planning, command recording and mesh
rebuild do the rest. With `ISLAND_VIEW_STATS=1` the view times each edit (a dab, or an
undo or redo) from the frame that planned it to the frame whose terrain mesh was
built from a terrain revision that contains all of it, and logs one line per stroke.

Measured on 2026-11-11 on one dev build, world running at 1x, 1280 x 720, nothing
else building: a soft-edged radius-4 brush (49 cells) swept 13 m over open ground in
60 cursor steps, Raise first and Smooth back over it, then two undos. "Cell by cell"
is the same build with `ISLAND_VIEW_SHAPE_BATCH=off`, which records uneven edits as
one `Terraform` per cell, as before `TerraformCells`; it was not measured on the
earlier binary. Each cursor step is a remote call that waits for a frame, so the
sweep lasts as long as the frames take: 7.8 s and 7.2 s batched, 48.6 s and 46.7 s
cell by cell, where a frame stalls on the simulation tick.

| Stroke | Recording | Commands | Dabs dropped | Refused | Plan to mesh: median | 95th | longest |
|---|---|---|---|---|---|---|---|
| Soft Raise | cell by cell | 588 | 28 of 40 | 0 | 36.5 s | 52.2 s | 54.1 s |
| Soft Raise | one batch a dab | 29 | 0 of 29 | 0 | 0.19 s | 0.25 s | 0.26 s |
| Soft Smooth | cell by cell | 573 | 26 of 39 | 12 | 28.4 s | 53.4 s | 54.2 s |
| Soft Smooth | one batch a dab | 23 | 0 of 23 | 0 | 0.17 s | 0.30 s | 1.09 s |
| Undo (Smooth) | cell by cell | 90 | | 0 | 12.6 s | | |
| Undo (Smooth) | one batch | 1 | | 0 | 0.22 s | | |
| Undo (Raise) | cell by cell | 101 | | 29 | 11.1 s | | |
| Undo (Raise) | one batch | 1 | | 0 | 0.17 s | | |

Cell by cell, each command is validated in full by the simulation (occupancy, cliffs
and a connectivity search over the whole island), at most six a tick. The last dab
of the Raise that was kept waited 54 s, which at six commands a tick is about half a
second a tick; the view's own bounds (400 commands queued, 600 a stroke) dropped most
of the dabs, and a dab that was kept reached the mesh half a minute after it was
planned. The simulation refused 12 single-cell
commands of the Smooth and 29 of the 101 of the second undo (the reasons were not
collected), so that undo did not restore the ground. A batch is validated once: every
dab is kept and shows about two tenths of a second after it was planned, and an undo
is exact. That remaining delay is the wait for the next simulation tick, the one
validation and the rebuild of the touched chunk meshes; it was not broken down
further. An earlier batched run, with a build running beside it, gave medians of
0.21 s and 0.17 s. These are one cell-by-cell run and two batched runs on one
machine.


## Scene size and frame time

Phase A (not measured yet): plants farther than 70 m from the camera sway
every fourth frame, in turn, and a sway that does not change (Reduce
motion) is not written, so its model's transforms are not propagated again.
The sea's swell mesh, rebuilt ten times a second, works out each grid point
once instead of once per quad corner. The deer's per-frame systems
(`read_wildlife`, `move_deer`, `play`) look only at the animals, about
0.1 ms together in the pass-11 capture.

Measured with `ISLAND_VIEW_STATS=1` and BRP `npc/perf` on the authored world, dev
profile, paused, camera 62 m above Stone Ridge, 2560 x 1440 window:

- 16,800 entities; 4,570 with a mesh, of which about 1,100 to 1,260 are in view; 1,607
  glTF scene roots, 1,214 of them foliage (one root and one scene each).
- 208 of 256 land chunks and 88 of 256 shore chunks have a mesh.
- 84 point lights exist; 39 are lit at dawn, none at noon.
- Mean frame 31.6 ms over four seconds; `npc/perf` median 38 ms with the profiler and
  the remote interface on. The render schedule takes 24 ms of that and the largest
  view system was the Home and farm panel refresh at 7.9 ms (see below; the panel
  was removed later, ADR 0044). These figures predate the outcrop rocks, bushes
  and shore slopes; see the re-measurement below. All of `island_atmosphere`
  is 0.19 ms and the terrain systems 0.19 ms. Transform propagation, visibility and
  animation over all entities come to about 2.5 ms.

The Home and farm panel (removed since, ADR 0044) belonged to developer launches.
The measurement below is kept as history. Its refresh built a full actor observation every frame, open or closed. It now builds
nothing while closed and, while open, only when one of its inputs changed: a
simulation tick, the control state, the clock's pause state, a pending action or a
choice in the panel, with a rebuild once a second as a bound on staleness. Measured
with `scripts/playtest/measure_view_perf.py` on the same launch at 1280 x 720, over
six seconds each, as `npc/perf` reports `homestead_panel::refresh`:

| State | Before | After |
|---|---|---|
| Closed, world paused | p50 5.35 ms, p95 6.13 ms | p50 0.007 ms, p95 0.009 ms |
| Open, world paused | not measured; the closed cost plus its rows | p50 0.04 ms, p95 0.07 ms, 8 ms once a second |
| Open, world running at 1x | not measured; the closed cost plus its rows | p50 6.2 ms: every frame that follows a tick still rebuilds |

The last row is the remaining cost: an open panel over a running world is rebuilt on
about two frames in three at 30 frames a second. Limiting tick-driven rebuilds to a
few a second would remove it at the price of text that lags the simulation by a
fraction of a second; that was not done.

Foliage was left as scene roots without `VisibilityRange`. The island is 256 m across
and the overview looks at all of it from about 330 m, so a range short enough to cull
anything at settlement zoom would strip the forests from the overview, and frustum
culling already removes three quarters of the meshes. Merging foliage into chunk
meshes is the change that would cut the entity count.

### After the rocks, bushes and shore slopes

Re-measured on 2026-11-10, dev profile, paused, on the same two views, with
`measure_view_perf.py --size 2560x1440` (frames counted over eight seconds) and
`ISLAND_VIEW_DRESSING=off` for the comparison on one build:

| View | With rocks and bushes | Without |
|---|---|---|
| Stone Ridge, 62 m | 16.9 ms (a second run, disturbed: 22.2 ms) | 16.7 ms |
| Overview, 330 m | 16.7 ms | 17.1 ms |

Every undisturbed run sits at the display's 60 Hz interval, so the frame is bound by
vertical sync and the dressing costs nothing that shows. The 31.6 ms above was not
reproduced: the panel fix removed five to eight of those milliseconds, and the rest
is the machine, not the scene. The measurement is only as steady as the machine:
another build was running beside it, and the same view measured between 16.6 and
24 ms from run to run, with and without the dressing alike. At 1280 x 720 the
`ISLAND_VIEW_STATS` means were 18.7 to 23.1 ms at Stone Ridge with the dressing and
16.7 to 23.2 ms without, and 16.9 to 22.1 ms against 18.0 to 29.2 ms at the overview.
No instancing, merging or culling was added.

The dressing is 173 rocks (five entities each: a root, a scene root and the glTF's
own nodes) and 272 bushes (one entity each, one shared mesh, two materials): 17,845
entities against 16,708 without, 5,043 mesh entities against 4,598, and 1,719 meshes
in view at Stone Ridge against 1,375 (3,315 against 2,870 at the overview). The shore
slopes add triangles to the 88 shore chunk meshes and no entities; they cannot be
switched off for a comparison. If the rocks ever need to be cheaper, drawing each as
one mesh entity instead of a scene root would remove about 690 entities.

## Limits

- A site, a yard and a barn are built from boxes and are level: on a slope (up
  to 0.8 m is allowed) the earth bed is sunk into the high side and stands proud
  on the low side. The barn's gables are stepped, not sloped.
- Site and store props do not block movement, and the pile beside a site lies on
  the walkable ring around the body: residents walk through it.
- A workshop's door or opening is lower than a resident: 0.80 of their height
  at the bakery, 0.72 at a timber yard, 0.68 at a smithy, 0.61 at a mine's
  adit, 0.56 at a market's counter. The 5 m body does not leave room for more
  without stretching the model out of shape. The fishing hut keeps its 0.76 m
  door on its 3 m open body.
- A building that stands between the camera and the played character, or the
  selected resident from 32 m in, is hidden down to its footing (a workshop
  to nothing but its yard and its pile) and comes back 0.35 s after it stops
  blocking. It is squashed, not faded, and its smoke and its lit windows go
  with it or stay in the air: a workshop's chimney smoke still rises. A
  workplace is tested to the top of its model (the smithy's stack, the mill's
  sails), not to a ridge.
- A fitted model's props (the mill's barrows, the smithy's anvil yard) stand
  outside the body and do not block movement.
- A field's plots are the view's grid, not places the simulation knows: a
  farmer works a plot from the field's entrance. A field on a slope is drawn
  level.
- The boat has no place in the simulation: it does not block, and it is drawn
  whether or not the fog shows the harbour. Lightning has no thunder.
- Rain does not lie on the roofs, snow lies only on the homes' roofs, and
  there is no fog weather to draw. A home under the fog of war shows its roof
  snow dimmed as it was when the fog dimmed it.
- The falling air is a 150 m box round the camera: between 45 and 60 m of
  camera height its edge can come into view at the top of the picture.
- `ISLAND_VIEW_SHOCK` draws a shock's tints only: a forced blight does not
  take any plots, and a forced drought does not slow growth.
- No child was seen natively: a new island has none, and the capture runs are
  too short for a birth. Their size is covered by tests.
- The upgrade scaffold and stack are a view interpretation: the simulation
  gives the upgrade no place or pile of its own, and they block nothing.
- No fisher was found in any capture run (40 game hours searched in the
  third pass), so the rod has only been seen in tests.
- Every home is the same cottage: the two-storey model, whose door is beside the
  middle of its wall, went to the inn. Homes differ by roof colour and size only.
- A cut-away home is a different drawing from the closed one: the half-timbered
  model with its arched windows gives way to plain walls. The door of a closed
  home is the model's painted, shut door; a resident going in walks into it.
- On a legacy profile the Live character's homestead cottage stands on the
  2 by 2 m the simulation blocks for it, and that is too small for what it holds. Its door is 1.58 m,
  0.88 of a resident, and 0.52 m wide. Between its walls are 1.52 m, a bed is
  1.98 m long, and it has three: they are drawn as three berths of one bunk,
  each 1.52 m long, so somebody asleep in one reaches 14 cm into the wall at
  the head and at the foot. There is no floor for a hearth or a table. Cut
  away, its walls and door are the 2.35 m and 2.2 m of every cut-away home,
  taller than the closed model's. Three full beds, a hearth and a table need
  a body of about 4 by 5 m. Neither the fitted cottage nor its inside has
  been seen in the running game: headless tests cover them.
- On a built home whose body is an even number of cells wide, `HomeView::door`
  is half a cell beside the middle of the wall; the cottage model's door is in
  the middle, so the drawn door is 0.5 m from it and a resident steps to the
  drawn one. The inn's door is a metre from the middle of its wall, as the
  two-storey model has it.
- A selected resident's caption stays over the entrance point, where the
  simulation has them, while they are drawn in bed.
- In a capture taken with the clock stopped, a resident who has just come to
  rest can still show the load they carried: the load is put down by the
  movement the last tick saw.
- The ground is still one quad per metre, so every colour edge is at least a metre
  soft: a path's edge, its wander and its verge are sampled once per metre and read
  as a soft band, not a crisp track. The grain adds texture up close but no new
  shapes: there are no blades, stones or ruts. The terrain's own one-metre height
  steps inland are unchanged.
- The shore slope softens banks up to 3 m. A higher cliff keeps its stepped face
  above the slope, and where the land's edge itself is stepped in height the slope's
  top follows the mean of the cells at each corner, which can leave a lip of a few
  centimetres. The slope is flat-shaded: each water cell's two triangles are visible
  as facets at close zoom.
- Ridge outcrops are a view interpretation. Generated ridge stone cells keep their
  higher walking cost wherever they are, including under drawn turf. Outcrop rocks do
  not block movement: residents walk through them, which is why they are low.
- A road is drawn from its authored line. Soil added beside a road is drawn from its
  cells and can leave a thin turf seam along the strip's outer cells.
- Bushes keep their two greens in every season, winter included, and do not sway.
- The marker key lists trades, not people, and has no counts; clicking an entry does
  nothing.
- Shadows reach 120 m from the camera; the overview has none.
- A pinned day phase changes light only. Residents still sleep and work by the real
  clock, and the HUD names the real time of day.

- Light levels and ground colours were tuned from still captures of the three
  settlements in autumn at four times of day (`target/npc-out/scene-look/`). Bloom
  strength, smoke and sound levels were not revisited, and nothing was reviewed in
  motion or in the other seasons.
- The authored world adds 393 smoke puffs (378 over homes, 15 over forges; drawn only
  where a fire is going), 84 point lights of which at most 43 are lit, up to 78 pile
  props per settlement and four recoloured atlas textures.
- Chimney smoke is nine soft quads, not a particle system or a lit volume: it has no
  shadow, lamps do not light it, where a puff crosses a roof or chimney it is cut
  along a straight line, and the wind never changes.
  Whether a home is "occupied" is a 5 m test around its door, so someone passing the
  door starts a wisp that fades again.
- There is no music, footstep, weather or wildlife sound.
