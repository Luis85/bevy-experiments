# Product review and verification notes

This review covers the implemented reference system and the seams a hosting game
would use. It distinguishes verified behavior from deployment work that remains.
The evidence is executable tests and end-to-end runs in this repository; the
feature list is not a claim that every public dataset, historical period, physical
system, or world-scale multiplayer topology has been implemented.

## Multiplayer review

| Perspective | Finding | Resolution and evidence |
|---|---|---|
| Returning player | A desktop reconnect initially created a different player and could lose access to owned land/housing. | Shared `SessionIdentity` storage and `NetworkSession` load/save preserve the same server-pinned capability. The storage test checks bounded reads, private file permissions, pinning, and atomic rejection. |
| Faction member | Save tests missed a failure parsing numeric member IDs inside an internally tagged network envelope. | A bounded faction-key deserializer accepts valid numeric IDs and rejects aliases/duplicates. The real TCP faction test now traverses invitation, acceptance, promotion, shared building, demotion, save, and resume. |
| Historical player | Offset spawns could start outside a small pack, and geometry versions could differ across host/client. | Spawn candidates are grounded or fall back to the covered host spawn. Missing terrain blocks movement. Pack/grid/epoch/political identities are checked before player allocation and again on the viewer. |
| Game integrator | Replacing the catalog could give new players invalid built-in starter items and create an unsaveable world. | Catalog and starter inventory can be installed atomically and are saved together; resource registration uses supported items. A custom-catalog restart test exercises a fresh join after restore. |
| Operator | A bad packet, oversized frame, or slow peer could otherwise harm an entire session. | Length-limited incremental framing, output and I/O budgets, rate limits, deadlines, and per-peer failure isolation. Corrupt-frame tests prove another client can still join. |
| Authority reviewer | Local clocks/resources or stale actor replicas could appear authoritative during startup or after disconnect. | Multiplayer authority is locked before schedules run, source streaming does not seed local mutable resources, outbound failures are visible, and disconnect clears actor replicas and queued actions. |
| Native multiplayer player | An action arriving after network polling could be discarded by the local sandbox consumer; commands beyond the send budget could also be lost. | The local consumer returns before popping in a network session. A real Bevy/TCP regression schedules nine late commands, verifies the bounded-send backlog survives, and checks the claim and all crafts in host and replica state. |
| Client under rendering load | Checking a snapshot deadline before reading the socket could disconnect a healthy peer after a long frame even though authoritative data was waiting. | Process at most eight nonblocking input batches before evaluating silence. A real TCP regression ages the timestamp, queues 100 snapshots, proves bounded catch-up, and still rejects an open but silent peer. The final native client also survived a deliberate 12-second process suspension while the host continued running. |
| Persistence reviewer | Changed source identity, malformed references, or stale catalog-derived fields could be restored. | Loads validate before replacing state, pin source identity, and validate catalog-derived values. Failed loads retain the existing live world. Sandbox serialization enables precise GPS round trips. |
| Capacity reviewer | Adding full faction rosters could exceed the transport frame budget. | Snapshots cap faction rosters at eight and per-list entries at 128; a maximum-count/long-identifier serialization test checks the complete bounded view fits one 256 KiB frame. |
| Automation user | A rejected CLI command could appear successful to a script. | The reference client returns failure when its requested command is rejected or unconfirmed, and supports an expected manifest for automated checks. |

The protocol suite includes real loopback clients rather than only mocked
messages. The independent `tests/process_smoke.py` runner exercises built host and
client processes, two identities across a host restart, streamed source
registration, a historical/political session, and rejection of mismatched pack,
epoch, and political revisions. Run it with the commands in
[MULTIPLAYER.md](MULTIPLAYER.md).

## Domain and headless gameplay review

The domain review added custom starting equipment registration, catalog-aware
save validation for structures/crops, and exact `f64` GPS JSON restore. A scripted
headless workflow covers gather, craft, build, planting/harvest, demolition,
faction invitation/acceptance, builder roles, land transfer, shared construction,
demotion, and save/reload. Shared ownership follows current roles; the original
creator cannot keep mutation rights after demotion. Source-tree edits are bounded
and distinct from destruction of authored structures.

## Data maintainer and historical-source review

| Finding | Improvement | Evidence |
|---|---|---|
| Reassembling cached terrain could make old raw input appear newly retrieved. | Preserve the oldest underlying retrieval timestamp and enforce source age on assembled-cache hits. | Cache timestamp/TTL regression and strict terrain probe. |
| A request for the past must not quietly receive present-day data. | Explicit half-open coverage, frozen source epochs, missing-tile errors, contained paths, bounded regular files and required provenance. | Historical tests and actual mismatched-era host startup/reconnect tests. |
| Political geometry can include holes, date-line rings and overlapping claims. | Preserve rings/holes and original properties; return all matching claims; normalize wrapped views. | Hole/date-line/round-trip tests and all 177 bundled Natural Earth features. |
| Changed political exports could retain a misleading base revision. | The local editor exports authored scenarios under a new content-derived revision ID while retaining the base identity in local saves. | Bevy political drawing/export regression. The fingerprint is not cryptographic authentication. |
| Public data endpoints are variable and have separate terms. | Expose provenance, age, synthetic/fallback status and failures; bound requests, bodies, concurrency and disk usage. | Local HTTP concurrency/cache tests; strict real terrain/weather success and explicit public OSM failure record. |

The real country snapshot is dated by capture, not asserted historical validity.
The two past-era packs are explicitly fictional examples. Country/source claims
remain separate from gameplay faction permissions.

## Player and integration polishing

- Added desktop tool crafting and wheat-to-seed crafting so the visible farming loop can repeat. Tools are currently catalog/inventory items; no hidden tool bonuses are implied.
- Added named faction creation, invitations, acceptance, builder/member role controls, faction selection and plot transfer to the desktop controls.
- Fixed local action selection to consult current faction permissions instead of checking only the original creator ID.
- Resource targeting now skips inaccessible plots and vertically unreachable nodes before choosing the nearest permitted resource; a UI regression also exercises the resulting domain command and depletion.
- Local saves now include the source-world identity, frozen epoch, clock, factions and edited political geometry. Incompatible worlds fail before state replacement.
- Local Bevy construction/planting/claims require loaded terrain within three metres of the requested surface. Pure domain hosts retain responsibility for environmental checks.
- Exposed a political atlas, selected-coordinate exploration, local border overlay, bounded polygon drawing with undo/cancel, intersection rejection and GeoJSON export.
- Normalized local political views at the date line and clipped/densified long border segments instead of requiring their original endpoints to be resident.
- Added scrollable sidebars, context-specific controls, visible recipe costs, bounded text input, terrain placement previews, a daytime control and explicit source mode labels.
- Moved CPU chunk mesh construction off the main frame, capped concurrent jobs, discarded stale results and reused/removed mesh assets. Geometry fingerprints prevent inventory updates and repeated network snapshots from rebuilding every structure. Text labels update at a bounded cadence and only write changed content.
- Multiplayer actor visuals use the actor's actual archetype; crops and resources follow the appropriate vegetation/terrain layer switches. Disabling weather uses calm, clear conditions; disabling animated daylight now uses a sun above the horizon. Server-owned controls are disabled in network sessions.
- A paused geographic jump now recomputes synthetic weather at the new location without increasing live API request frequency.
- The reusable historical bridge rejects a pack whose actual grid differs from its configured save/network identity. Custom catalogs retain tree metadata without receiving unsupported built-in wood items.
- Native screenshots exposed missing Unicode symbols, downward-facing plot rings and an apparently blank regional atlas. The polish pass embeds a licensed font covering the interface symbols, corrects ring winding, frames valid regional geometry, scales atlas lines/markers/depth with zoom, and places authoring actions above the map. Scroll indicators and a political color legend make the lower controls and boundary meaning more discoverable.

## Native visual and workflow acceptance

The independent visual reviewer inspected actual native captures before reporting findings. Screenshots alone support layout and presentation conclusions; command, identity and persistence conclusions also use saved state, process observations and executable tests.

| Flow | Evidence | Result |
|---|---|---|
| Explore a layered world | [World overview](screenshots/01-world.png) | Terrain, forests, farms, water, buildings and actors are visible together under an explicit synthetic-source label. |
| Build and restore a home | [Restored camp](screenshots/02-camp.png) | House, workbench, two fences, crop and home state render; a real placement click and F5/F9 were verified. |
| Inspect current country data | [Country atlas](screenshots/03-atlas.png) | Map actions, boundary legend and current-source caveats are visible. |
| Inspect dated regional claims | [Before](screenshots/review-before-regional-atlas.png) / [after](screenshots/04-historical-atlas.png) | Regional framing resolves the misleading empty-world view while retaining the fictional/historical labels. |
| Author overlapping territory | [Territory editor](screenshots/05-territory-editor.png) | Three clicked vertices produced a closed exported polygon; selection reports both underlying and authored claims. |
| Use a smaller window | [1100×700 layout](screenshots/06-minimum-window.png) | Header, map actions, sidebars, long claim text and save status fit; scroll indicators remain visible. |
| Join and resume a shared world | [Multiplayer](screenshots/07-multiplayer.png) | The reopened native client shows player 2, its owned plot, crafted inventory, two actor replicas and server tick/calendar. Host state and a deliberate client-stall check independently confirm authority and continuity. |
| Inspect real elevation | [Alpine terrain](screenshots/08-live-terrain.png) | Nine chunks report real elevation with no synthetic fallback; surface colors remain illustrative. |

The local explorer was also checked for invalid GPS entry, named faction creation, plot transfer and restoring earlier ownership. Small-window review led to camera framing that respects sidebar space. Interface symbols now render correctly, plot outlines face upward, atlas actions are above the fold, and scrollbar thumbs are visible.

The final multiplayer flow verified a native claim, crafting, a 12-second client stall, a host restart and reopening the native client with its existing capability. The host retained the same two players and restored player 2's plot and 28 wood / 4 planks inventory. The final capture uses fixed visual daylight for readability while its calendar remains authoritative. See [the validation record](VALIDATION.md) and [sanitized native evidence](native-multiplayer-verification.json).

Accessibility remains partially reviewed: the captures do not establish keyboard-only traversal, screen-reader behavior, touch operation or color-vision accessibility. There is no user-facing UI scale control or distinctive local-player marker. Server-owned disabled controls could receive clearer dimmed text in a future pass. These are documented follow-ups, not claims of compliance.

## Remaining limits visible to users and integrators

- The default global surface and bundled era packs are synthetic. Real current
  and historical datasets require their own licenses, coverage, dated provenance,
  and prepared content. Historical mode never invents absent source data.
- Multiplayer is one bounded process using unencrypted TCP. There is no public
  account service, identity recovery, distributed authority, prediction/rollback,
  interest index, or production availability layer.
- Saved player slots and world collections are finite. There is no automatic
  pruning of inactive owners and no regional persistence pagination. This avoids
  silently deleting property, but a larger service must provide lifecycle policy.
- Reference placement checks terrain support, reach, rights and authored-object footprints. Collision against all sourced geometry is not implemented.
- Movement is a kinematic reference with terrain/coverage constraints. Full body
  collision, locomotion, flight/marine dynamics, navigation, and actor AI remain
  hosting-game systems.
- Crop/resource advancement uses an illustrative weather model at the host's
  configured spawn. It does not model regional climate, agriculture, or ecological
  succession across the globe.
- Political editing is local authoring. Multiplayer peers share an exported
  revision; the reference does not replicate live collaborative polygon edits.
- The flat atlas frames small regional data but does not provide a geodesic globe
  renderer. A dataset spanning the date line retains a global framing extent.
- A bounded client view can be truncated and is never a complete writable save.
  Large worlds need spatial pages/deltas and explicit replication subscriptions.

These boundaries are documented in the runnable guides and extension contracts.
They are not hidden behind successful desktop rendering or passing unit tests.
