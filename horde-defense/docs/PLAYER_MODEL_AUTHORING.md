# Authoring source-faithful player models

The current catalogue is `assets/player_models/rook.json`, `june.json` and
`skins.json`. Its strict schema is implemented in
`crates/horde_game/src/player_model/catalogue.rs`. This guide describes the
source ownership data, not a claim that articulated poses have passed review.
The detailed art rationale remains in
[the decomposition research](PLAYER_MODEL_ART_DECOMPOSITION.md).

## Immutable source and coordinates

Both models share `art/pixel/menu-survivors-v1.png`, decoded as 1774 × 887 RGBA.
The original PNG remains unchanged:

- PNG SHA-256: `4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a`.
- Decoded RGBA FNV-1a64: `f6b2cb172a17bb3d`.
- Rook tile: `[0,0,887,887)`; June tile: `[887,0,1774,887)`.
- Rook layout: `[282,42,654,848)`; ground `(478.1,848)`.
- June layout: `[1128,60,1465,848)`; ground `(1306.61,848)`.

The first new Rook parity fixture explicitly uses right edge 654 rather than
the older approximate crop's 652. This retains the faint pistol fringe at the
right edge. The source pixels, 806-pixel source height and existing ground
anchor are unchanged. This is an approved new fixture definition, not a
silent rebaseline of historical captures or a modification to the PNG.

All coordinates are full-image, top-left origin, x right and y down. A run
`[y,x0,x1]` owns texels at that y for `x0 <= x < x1`. Bounds are half-open.
Bone and socket coordinates use this same source frame. At bind all source
points map identically; one shared model-to-display scale handles resizing.

## The actual data contract

Models use schema version 1 and stable string IDs `rook` and `june`. Their
default skins are `rook.original` and `june.original`. The catalogue declares
slots `body`, `outfit`, `accessory`, `ranged`, `hands` and `melee`.

| Parts | Slot | Material / role |
| --- | --- | --- |
| `head`, `hair.back` where present | `body` | Original head/hair identity |
| `neck` | `body` | Original skin at the collar |
| `pelvis`, `leg.near`, `leg.far` | `body` | Original trousers, including visible upper waistband |
| `boot.near`, `boot.far` | `body` | Original independent boots |
| `jacket.torso`, `jacket.hood`, sleeves, cuff, Rook hem | `outfit` | `work_jacket` garment bundle |
| `carriage.*` | `accessory` | `field_webbing`; Rook pack/straps or June webbing/pouches |
| `weapon.original.rear` | `ranged` | The original source pistol, isolated from hands |
| `hand.near.wrist`, `hand.near.palm`, `hand.far.palm` | `hands` | Original proximal near-wrist strip, distal palm/fingers and far hand |
| `source.residue` | `body` | Only isolated faint source artifacts |

Current default hands retain every original finger/wrist pixel in mutually
exclusive masks; they are not generated replacement hands. The bounded action
correction split the authentic proximal wrist strip from the pointing palm so
an action-only closed hand can preserve a connected wrist. The weapon's part
ID is prepared for future
front fragments, but the current source pistol is represented by one visible
weapon fragment. Material names do not grant or imply inventory items.

`mask_mode` is `runs`. Every ordinary part has explicit `source_runs`, empty
`authoring_polygons` and `remainder_max_alpha: null`. Do not mix polygons and
runs in the same model. The current source runs were authored from source
surface contours, refined through isolated part/slot views, then resolved to
one explicit owner per texel. A small exterior fringe was assigned to its
nearest traced surface within three source texels during this authoring pass;
the runtime does not repeat that operation or inspect source hue.

The single `residue` part has empty runs/polygons and a bounded remainder
fallback: maximum alpha 2 for Rook, 1 for June. The compiler assigns only
otherwise-unowned positive-alpha pixels within that bound. Never increase the
cap or move opaque pixels into residue to conceal an incomplete mask.
Alpha-zero source RGB is retained by the source cache for exact raw round trips
and has no drawable ownership geometry.

## Skeleton and sockets

Bones form one graph rooted at `root`: pelvis, torso, head, near/far upper arm,
forearm and hand, near/far leg and boot, plus a weapon attached to the near
hand. Draw order is independent of this parent graph. `bind_origin` and
optional `bind_tip` are source points; derive lengths from these points rather
than maintaining another arm-length configuration.

| Source joint | Rook | June |
| --- | --- | --- |
| Near shoulder | `(392,271)` | `(1221,284)` |
| Near elbow | `(380,377)` | `(1218,384)` |
| Near wrist | `(506,424)` | `(1326,427)` |
| Far shoulder | `(550,276)` | `(1375,284)` |
| Far elbow | `(576,374)` | `(1391,378)` |
| Far wrist | `(571,474)` | `(1388,475)` |

Near pivots were placed from the visibly drawn bend and cuff, rather than
copying the previous rig's elbow coordinates. Far joints are less strongly
evidenced because most of that arm is occluded. These are authored animation
binds requiring pose review, not recovered hidden anatomy. Their exact
positions do not alter the default source-coordinate reconstruction.

Required sockets are `muzzle`, `throw_origin`, `palm.near`, `palm.far`,
`wrist.near` and `wrist.far`. Additional sockets are `barrel_back`, `grip`,
`support`, `shoulder.near`, `shoulder.far`, `elbow.near`, `elbow.far` and `hips`.
Palm centers and weapon grip contacts are separate source positions: an
extended trigger finger does not put the palm center on the gun's grip axis.
The displayed muzzle and barrel direction come from the original source
pistol's own sockets, not the old gun atlas. Socket placement remains subject
to close visual inspection when poses are enabled.

## Skins and real gear

Each original skin includes every visible original owner and residue, with no
hidden fill. Separate compatible bundle skins select:

| Skin suffix | Equipment selector | Parts selected |
| --- | --- | --- |
| `.work_jacket` | `outfit`, style `armor.cloth` | Every `jacket.*` part, including both visible sleeves/cuff/hood/hem |
| `.field_webbing` | `accessory`, style `accessory.pouch` | Every `carriage.*` part |
| `.starter_pistol` | `ranged`, weapon class `Pistol` | Original weapon-owned source pixels |

The original complete appearance is a reference definition. Production gear
resolution still reads the real frozen equipment metadata. It must not invent
an item to obtain the reference costume. A jacket replacement must replace
its whole compatible garment bundle; retaining original sleeves under a new
torso is not a complete jacket swap. Removing a slot must remove that slot's
actual original pixels, including their former baked-in positions.

The single raster cannot supply a clean jacket beneath removed webbing or a
complete straight far sleeve. The accepted version-one model documents have
no hidden fill. The additive catalogue below supplies separately authored
repairs; their fits require their own review. Every repair needs explicit
provenance and an exposure domain, and contributes nothing to the exact
reference bind. Do not reuse a whole original figure behind moving limbs to
hide gaps.

## Editing a mask safely

1. Inspect the immutable source at native size and integer zoom. Show the
   relevant owner by itself, adjacent owners, and the result with its slot
   disabled. Trace the visible surface boundary, including its original
   outline and partial-alpha fringe. Internal black gaps may belong to the
   jacket rather than to a pouch: decide from the actual drawing.
2. Edit the affected row runs. Transferring a texel means removing it from one
   owner and adding it to the other, preserving its source UV. Sort runs by
   `(y,x0)`, merge contiguous runs of the same owner and reject zero-width
   entries. Preserve exact source RGBA; do not change a source pixel's alpha,
   discard colors, or generate visible replacement art.
3. Re-run ownership audit on the complete source tiles, including alpha 1–127.
   Require zero missing, duplicated or changed source pixels. Verify source
   PNG and decoded hashes. Keep alpha-zero RGB available for raw comparison.
4. Inspect isolated `outfit`, `accessory`, `ranged` and `hands` slots. This
   catches meaningful errors that an exact complete bind cannot: a tan strap
   assigned to the jacket still produces a perfect complete picture, but
   survives the wrong slot's removal.
5. Re-run native source-versus-modular bind comparisons through identical
   premultiplied upload, sampling, scale and clip. Review transparent, dark
   and light backgrounds. Any tolerance or new baseline needs the documented
   approval process; do not silently bless the discrepancy.
6. Only then validate affected aim/reload/throw/gear poses. Both arms must
   remain connected, hands must visibly hold their sockets, both boots must
   remain distinct and planted, and effects must use the displayed muzzle or
   release point. Geometry coincidence alone is insufficient visual evidence.

## Current authoring evidence and limits

The source has 195,379 nonzero-alpha Rook pixels and 182,305 June pixels. The
authored masks plus bounded residue preserve all of them exactly once. Rook
has 23 parts and June 24, including residue and the separately owned near-wrist
strip. The bounded residue contains
3,092 / 2,125 pixels, with measured maximum alphas 2 / 1. Source part/slot
isolation was inspected during authoring, and source ownership was checked
without rewriting any PNG.

The independent integration/verification streams accepted the source bind on
3 November 2026. `target/journey-review/source-model-native-1/summary.json`
records zero differing native pixels, zero differing bytes and maximum channel
delta zero for both heroes on black and white backgrounds. Root also inspected
the two semantic part sheets. That acceptance covers the original source bind;
action continuity and changed equipment appearance remain separate gates.
This art-data stream did not perform GUI interaction or a human playthrough.

## Additive equipment and repair catalogue

`assets/player_models/attachments.json` extends the accepted catalogue. The
original source PNG and every original RGBA value remain immutable. The
bounded, Root-approved action correction refined the original documents only
to split a true near-wrist owner and correct draw ordering; the default skin
lists include that owner and still reconstruct every source pixel once.
Its version-one document contains `sources`, new `skins`, `attachments`,
`holds`, `pose_tracks` and `support_tracks`. Exact Rust types are in
`crates/horde_game/src/player_model/attachments.rs`. Original source pixels and
their owners retain their accepted meaning.

Each attachment names its source image and explicit positive-alpha source
runs. `source_to_bind` supplies a source origin, model-space bind origin,
rotation in **radians** and one uniform scale. Aim ranges are **degrees**:
right-facing zero points along +x, positive angles point screen-down. There is
no per-frame texture scan, hue classifier, gun shrinking or limb stretching.

`skin_replacement` attachments are explicit different equipped/unequipped
artwork. `hidden_fill` attachments are separately authored continuity material.
The `provenance` field distinguishes original-source material clones, inferred
far sleeves, the generated underlayer and retained alternative armor artwork.
It must never describe inferred material as recovered original pixels.

### Exposure rules and missing equipment

For a `revealed` attachment, every `required_parts` owner must remain selected.
It then qualifies when **any** `missing_parts` owner is absent, or when its
declared action exposes it. Actions are aim, recoil, reload, throw, melee,
brace and hit. Missing-owner repairs may appear in an equipped static Bind
sample; the accepted `resolve_model` reference path suppresses all fill.
The complete original starter has no missing owners and no nonbind action, so
its static source equality remains unchanged.

Work-jacket repairs require `jacket.torso`. They therefore cannot leak olive or
slate coat material onto the charcoal underlayer. An empty-equipment repair
bundle is unconditional only when it has no original parts, no replacements,
no hold, and exclusively revealed hidden-fill attachments.

An equipment requirement with `slot: "outfit"`, `style: null` and
`weapon_class: null` explicitly means the outfit slot is absent. Positive
styles/classes require the real frozen item. The `.outfit.absent` recipes
select the complete charcoal underlayer. Padded, ballistic and plate outfits
select that same coordinated torso/sleeve/cuff bundle plus their explicit
different armor layer. The real Field-webbing accessory remains independently
selected. Unsupported accessory styles do not silently become Field webbing.

### Repair ledger

The first repair pass tiled original sleeve interiors across old occlusion
footprints. Native causal strips in
`target/journey-review/source-model-repair-cause/` showed that this produced the
stationary third-arm band and June's repeated rectangular grid: removing
HiddenFill removed the band entirely. Original cloth ownership therefore was
not reassigned to hide this repair error.

The corrected torso/carriage repairs sample a single coherent olive/slate coat
panel per model. Three disjoint exposure masks share its exact unit-scale
translation: motion reveals, removed carriage, and old weapon vacancy. A
natural coat-body outline bounds the surface; it does not retain a protruding
old arm/cuff/pack silhouette. Known original torso/hood/hem and protected
identity/trouser pixels are excluded. This avoids duplicate repair coverage
and repeated material tiles while preserving the original visible artwork.

| Logical repair | Target domain and activation |
| --- | --- |
| Rook near/far strap removal | Missing-carriage exposure within the natural coat support domain, sampling one coherent body material |
| June near/far/rear straps and webbing | Missing-carriage exposure within the natural jacket support domain, without repeated sleeve tiles |
| Rook pack removal | No blanket cloth fill: the outer pack mostly reveals the background |
| Torso beneath moving arm/hands/gun | Natural torso unknown-surface domain, excluding known original/protected material; three disjoint exposure masks use one continuous panel while original jacket remains |
| Far upper/fore sleeves | Explicit inferred repair attachments copied from the same survivor's original near-sleeve material, rigidly rotated and uniformly scaled from the source joints; off reference Bind |
| Elbow/wrist joins | Small interior cloth bridges under the visible sleeves/hands, bound to their corresponding forearm |
| Underlayer shoulder/elbow/wrist joins | Small bridges from the same new charcoal garment source, selected with that garment only |

Original cloth donors `[370,255,414,325)` for Rook and
`[1194,273,1239,334)` for June remain only for small genuine joint repairs;
they no longer form tiled torso/strap backings. Do not expand repair geometry
into the former outer arm or pack silhouette just to conceal an exposed hole.

`player-hidden-torsos-v1.png` supplies the single authorized coherent hidden
backing sheet. Its inner material, not its lateral shoulder/stump silhouette,
is used. Rook maps source waist `(490,590)` to `(476,510)` and June maps
`(1316,590)` to `(1304,510)`, both at scale one with no rotation. The exact
prompt, raw/final PNG hashes and unchanged decoded RGBA are retained in
`player-hidden-torsos-source.json`. Every backing remains off the exact
reference Bind; native confirmation of the corrected exposed surface is a
separate acceptance gate.

### Separate charcoal underlayer asset

`assets/art/pixel/player-underlayers-v1.png` is a new explicit alternative
garment, not a regenerated default body. Its twelve atlas cells contain two
arm-free torsos, near/far upper/fore spans for both survivors, and two cuff
sources. Model recipes create separate near/far cuff attachments from those
sources. Torso alignment uses one authored uniform scale; sleeve spans align
their own source pivots to the existing source skeleton.

The built-in image generator used the original still as its sole survivor
style/proportion reference. A second targeted edit removed the first output's
baked short-sleeve lobes from the torso cells. The selected raw output was
copied without raster edits. `player-underlayers-source.json` preserves both
exact prompts, reference hashes, both raw-output paths/hashes, and the final
selected image metadata. Root's prompt-metadata embedding may change the PNG
container hash while preserving its decoded RGBA; record both identities.
The original defaults remain their original source image throughout.

### Weapon and hand contact authoring

Each survivor has a Pistol hold using its original source pistol and five
additional class attachments from `defender-rig-v1.png`. The firearm atlas has
unequal rows: quarter-cell cropping includes actor feet in early gun cells.
The new data uses explicit gun-only source envelopes with transparent gutters.

| Class | Gun-only source envelope | Authored uniform scale |
| --- | --- | --- |
| Pistol | Original per-model weapon owner | Original source scale |
| Smg | `[343,479,583,622)` | 1.15 |
| Shotgun | `[946,502,1248,594)` | 1.35 |
| Rifle | `[635,482,926,614)` | 1.35 |
| Marksman | `[325,740,626,862)` | 1.45 |
| MachineGun | `[8,749,307,892)` | 1.45 |

These fixed scales and source sockets are authored first-pass calibration,
not native-fit approval. Every long gun declares grip, support, stock, muzzle,
barrel-back and magazine sockets. Its stock source origin maps to the source
near shoulder; the pose resolver must retain that physical scale. Pistol
retains its own grip/support/bore/muzzle data and original low-ready reference.

Contacts are not palm centers. Long-gun contacts were refined after inspection
of actual painted grip material; original Pistol/default contacts stay intact:

| Contact | Rook bind point | June bind point |
| --- | --- | --- |
| Original Pistol trigger-hand grasp | `(566,433)` | `(1390,433)` |
| Long-gun palm grasp | `(552,435)` | `(1377,438)` |
| Support-hand grasp | `(570,465)` | `(1391,465)` |

All five atlas grip sockets were inspected on opaque painted handle material;
those gun sockets, stock seats and scales were not moved to disguise a hand
offset error. The resolver derives each hand's local contact offset from its declared hand
bone bind origin and publishes `contact.near`/`contact.far` independently of
the wrist/palm sockets. The original hand images remain selected. A correct
contact equation does not itself establish natural fingers or a convincing
hold; inspect the source pixels at the displayed weapon in native captures.

The first additive gate left action tracks empty. The subsequent authored
motion data below adds tracks after the app gate exposed steep-aim reach and
unsupported-action failures. Native appearance/motion approval remains a
separate gate. Action-only hand replacements still require an explicit
selection contract; drawing a second fist over an unchanged original hand is
not an acceptable substitute.

### Authored aim, support and action tracks

The additive document now contains 24 pose tracks and 10 support tracks.
Neither changes the original bind documents, source owners, weapon scales or
static support sockets. All track samples are immutable and contain source
rotations/translations only.

For the unstocked original pistol, the grip follows an authored safe path from
the midpoint of the actual source shoulders. Native steep-up views showed
the initial circular path sweeping the intact gun/hands behind the head. The
bounded correction keeps the horizontal grip while adding a forward bias and
shorter vertical radius at steep bearings. With `w = sin(theta)^2`, shoulder
midpoint `M`, near shoulder `S`, original grip `G`, original bore `beta`, the
grip is `D = M + [forward*w,0] + R(theta)*[r,0]`, where
`r = horizontal_radius + (steep_radius-horizontal_radius)*w`.

| Model | Horizontal radius | Steep radius | Maximum forward bias |
| --- | --- | --- | --- |
| Rook | 130 | 50 | 130 |
| June | 122 | 45 | 105 |

The weapon's pre-pivot translation is
`S + R(beta-theta) * (D-S) - G`. Keys are authored at
7.5-degree intervals plus exact −90/+90-degree range endpoints. The previous
−89/+89 keys retain their bearings; this bounded correction updates the grip
translations rather than changing the skeleton or firearm scale.
These limits describe the raw target bearing, not an off-axis barrel
correction angle. This
raises the still's low-ready grip into a reachable aiming stance without
resizing the gun or stretching either limb.

Stocked firearms retain their fixed source-to-bind scale and seated stock.
`support_tracks` declare an eligible source-material mask and source-coordinate
grasp keys at aim-normalized phases. Each hold references its own track.
The static attachment support socket remains available as `support.bind`;
`support` represents the effective authored grasp used by the hand. The
eligible mask does not change or remove any rendered gun pixel.

| Class | Steep-aim receiver grasp | Path to nominal fore-end |
| --- | --- | --- |
| Smg | `(420,523)` | Painted receiver to `(490,543)` |
| Shotgun | `(1030,529)` | Receiver/wood fore-end to `(1156,540)` |
| Rifle | `(710,534)` | Receiver/handguard to `(790,548)` |
| Marksman | `(403,817)` | Receiver to `(480,818)` |
| MachineGun | `(83,812)` | Via `(110,778)`, `(175,778)`, `(197,796)` to `(205,813)` |

The machine-gun detour follows its painted top rail and front receiver rather
than crossing the ammunition belt. Track keys include every path corner, so
linear interpolation cannot cut through an ineligible opening. Conservative
material masks and every key/interpolated segment are validated against the
immutable source once; the renderer does not search for a grip at runtime.

The corrected offline contact audit sampled interpolated data every 0.025
degrees through the complete −90 to +90 range for both survivors and all six
classes, performing 172,824 wrist checks. The unchanged support paths had
previously passed 205,648 f32-interpolated source-material mask samples.
Both wrist goals stayed within the minimum/maximum reach of their unchanged
source segments. The smallest far-arm hold margins were 4.32 source pixels
for Rook and 5.37 for June; corrected steep Pistol near-arm margins were
6.61 and 4.19 pixels. This is mathematical/data evidence, not native
visual approval or evidence of every combined action overlay.

All nonbind armed actions layer on the same solved aimed baseline. Action
rotations use bind pivots conjugated through that baseline. Reload keeps the
near grasp on the actual gun; the far arm performs small upper/fore rotations
and hand roll. It uses fixed-length joint FK rather than an arbitrary free
wrist translation, which can otherwise approach the unequal-bone folded
limit. The support hand deliberately leaves its ordinary hold during reload.

| Track | Phase contract and authored behavior |
| --- | --- |
| Aim | Aim-normalized pistol grip path; stocked support tracks are separate |
| Recoil | Intensity zero is identity; intensity one is a restrained class-specific upper-body pulse, with stock/contacts still solved |
| Hit | Intensity zero is identity; intensity one adds a small upper-body recoil and head response |
| Brace | Intensity zero is identity; intensity one adds a small torso lean with head counter-motion |
| Reload | Phases zero and one are exact identity relative to the aimed baseline; intermediate far upper/fore rotations and hand roll |
| Melee | Phase zero is the already-resolved strike/recovery start; phase one is identity relative to the aimed baseline |
| Throw | Phase zero is the actual release/recovery start; phase one is identity relative to the aimed baseline |

Throw/melee tracks use connected upper/fore rotations and hand roll, not
translated floating hands. There is no invented authoritative prethrow
windup. Stocked weapon translations remain zero in action tracks. Reduced
effects may suppress incidental recoil/hit pulses while retaining required
equipment and legal action presentation. Native phase-zero/middle/end views
must still verify visible contacts, seams, feet, tool orientation and recovery
continuity before these tracks are approved for production integration.

### Foreground ordering and source-native closed hand

The original source owners do not overlap at Bind, so refining their draw
order preserves the reference RGBA. Raised near arm/forearm/cuff, weapon and
hands now draw in front of the head. The far forearm draws in front of the
torso, while its upper shoulder remains behind the body. The ordering also
applies to corresponding alternative-garment and genuine repair attachments.
This prevents a solved gun/hand from disappearing under the original head or
looking as though it is held behind the backpack. Rifle stocks remain seated;
no whole-gun translation or scale change is used for that depth correction.

An action-only source-native far fist replaces the distal pointing near palm
during melee. The authentic original `hand.near.wrist` strip remains on the
same hand bone. Rook aligns far anatomical palm `(566,467)` to near palm
`(540,430)`; June aligns `(1386,468)` to `(1366,432)`, at unit scale and zero
rotation. The closed grasp contacts therefore become `(544,428)` and
`(1371,429)`. Tools bind to these actual closed contacts rather than to the
extended original pistol finger. The hand is not enlarged.

Attachment-local `replace_parts` suppression applies only while that
skin-replacement attachment is active; the default/normal pointing palm is
retained otherwise. The wrist strip is never replaced. The cel publishes its
painted anatomical palm as `palm.near` and `throw_origin`, and its painted
closed contact as `contact.near`. The active tool publishes `melee.grip` and
the same `contact.near`; duplicate publishers must agree within the source
socket tolerance. Attachment-qualified socket names remain available for
independent geometry inspection. No skeleton/root/wrist alias is overridden.

At recovery, shared wrist/palm/bone transforms return to their aimed baseline.
An active closed grasp and an original pointing grasp are different material
features: each must be checked on its actual rendered hand/tool, rather than
equating their different contact offsets. This preserves strict contact tests
without publishing a convenient point that is not on the selected cel.

### Additional worn gear and action-only melee tools

`assets/art/pixel/player-equipment-v1.png` supplies nine isolated items in a
three-by-three atlas. Root accepted the first generated artifact for staging
without edits. It contains no bodies or hands. Its exact prompt, original
still reference hash, raw output hash, final metadata-bearing hash and decoded
RGBA identity are recorded in `player-equipment-source.json`.

The catalogue now explicitly covers all current accessory styles: the original
`accessory.pouch` source carriage, plus the new medic, radio, satchel and rig
recipes. The latter replace only the original carriage owners. Their source
mounts and placements are authored independently for each survivor; they do
not select a body or change authoritative equipment. The radio-style item is
the compact worn rangefinder/radio module, matching the content's Rangefinder
style ID. The medic mount is the midpoint between its two belt loops.

| Style | Source envelope | Uniform scale | Placement |
| --- | --- | --- | --- |
| `accessory.medic` | `[121,124,315,330)` | 0.58 | Near hip, bound to pelvis |
| `accessory.radio` | `[542,100,699,335)` | 0.45 | Far upper chest, bound to torso |
| `accessory.satchel` | `[934,26,1145,378)` | 0.85 | Shoulder strap to side bag, torso; rotation −π/12 |
| `accessory.rig` | `[79,474,362,790)` | 0.80 | Chest/waist webbing, torso |
| `melee.crowbar` | `[568,442,688,810)` | 0.87 | Source grip `(607,750)` |
| `melee.axe` | `[977,444,1131,811)` | 0.98 | Source grip `(1007,762)` |
| `melee.machete` | `[186,862,275,1228)` | 0.74 | Source grip `(217,1181)` |
| `melee.hammer` | `[555,862,697,1228)` | 1.00 | Source grip `(625,1181)` |
| `melee.saber` | `[996,856,1095,1228)` | 0.95 | Source grip `(1024,1181)` |

Melee recipes require the real `melee` slot and exact style. They have no
original parts, no `replace_parts`, and no hold override. Their attachments
use bone `hand.near`, slot `melee`, channel `skin_replacement`, activation
`skin_selected` with `actions: ["melee"]`. Each declares `melee.grip` and
`melee.tip`, avoiding collisions with firearm grip/muzzle names. Its source
origin is the painted grip; bind origin is the closed-cel contact `(544,428)`
for Rook or `(1371,429)` for June. Initial +π/2 rotation places the upright source tool
forward; subsequent hand FK/approved action tracks move the same attachment.

The default owned crowbar remains stowed and invisible during Bind and normal
holds, so its mere presence cannot break the original starter appearance.
The renderer hides slot `ranged` only while performing melee/throw, then
restores it. Neither a melee recipe nor an outfit recipe permanently removes
the original hands or equipped ranged weapon; the melee action cel temporarily
replaces only the distal pointing palm. The shared model melee track
is referenced by every ranged hold; the selected melee attachment determines
the actual style. Worn fit, visible finger wrapping and strike motion remain
native-review gates, not established by these data equations.
