# ADR 0024: Read-only JSON world projection and independent browser viewer

- Status: Accepted
- Date: 2026-10-29

The ExcaliburJS viewer lives in the sibling `npc-world-viewer` TypeScript project. It consumes a
versioned observer projection, rather than loading private resumable WorldSnapshot files. The first
adapter supports island-village content only; unsupported scenes fail explicitly. No generic
capability crate knows about the island or Excalibur.

`testbed_content::projection::world_state` reads visual/entity data between ticks. It exports integer
millimetres (x east, z south), terrain cells and native presentation paths, stable sorted SimIds,
places, resource patches, people, activity/needs/inventory, and ground items. It excludes notebooks,
diary pages, credentials and controller/session metadata. This is an omniscient operator view, not
a perception-scoped agent API or a save format. No projection state is hashed or registered into
content; exporting must preserve current and future state hashes.

`npcctl serve --world-state-out FILE --world-state-interval-ms N` owns scheduling and file I/O.
Export is opt-in, starts before readiness is advertised, and repeats at a host-time interval
(50–60000 ms, default 500), including while paused. Ordinary and explorer hosts publish in their
existing between-tick frame hooks. Writes use a sibling temporary file, sync, close and rename over
the configured file. Startup failures abort; subsequent failures warn and retry. Final stop can leave
the last interval's projection; this file is never used for persistence or recovery.

A loopback Node server reads one operator-configured file every 250 ms and validates schema/version,
numbers, entity IDs and render budgets. The browser polls its same-origin endpoint every 500 ms.
File polling follows Windows atomic replacements and recovers when a missing file appears. Invalid
updates retain the last valid snapshot with an error status. File modification times expose staleness
independently of simulation ticks. There is no gameplay RPC proxy, browser-provided file path, private
save upload or command endpoint.

Snapshots are full replacements. The renderer reconciles actors by ID, removes absent IDs, accepts
lower ticks for reset/resume, and smooths positions only toward received coordinates. Camera, selection,
follow and label settings are browser-local. Pixel-art graphics are original cached Canvas rasters
displayed as Excalibur actors. The browser never predicts decisions or advances the simulation.
Other games can add producer adapters for this contract in later increments.

The Herp stream increment adds the same output flags to `actor_testbed`, with an
initial publication after scenario/resume construction and throttled publication
from `PostUpdate`. A single native host can therefore supply both presentations.
The absolute output path and interval are optional host-only `RunFile` metadata,
preserved through native/detached/window handover; they never enter a world
snapshot or the simulation's registrations. Legacy launches omit the descriptor.

Schema 1 gains optional per-person `bubbles` (default empty): at most two bounded
public thought/action or speech records, with simulation tick and expiry. Content
selects trusted action names and explicit spoken lines; raw command arguments,
rejection inputs, notebooks and recipient-copy events are excluded. Browser
presentation uses text drawing/text nodes and latest received tick, clears on
replacement/reset/despawn, and does not advance lifetimes while the source is
stale or paused. Tails and bodies share viewport/HUD/collision constraints.
