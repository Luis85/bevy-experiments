# 0036 — One application flow for Island Village

Date: 2026-11-09. Status: accepted for this increment.

## Context

The user requested a coherent life/farming-game experience from menu to play and
building. They confirmed the existing 3D world and survival rules. The previous
launcher/window handoff and separate developer-style panels expose implementation
boundaries to the player.

## Decision

Make the existing actor host an application library with a thin standalone binary.
The normal `game_menu` entry invokes that host directly before creating a launcher
window. Title, new setup, play and pause live in the same process and window. Explicit
developer tooling keeps its existing route. This is an application dependency, not
a new coupling from a generic capability to content or rendering.

The frontend owns local persistence and state transitions. `FrontendGate` is a
view-only contract that suppresses underlying HUD and input while title/pause/setup
owns focus. `PlayerMenuRequest` asks the host to open its menu; the view never saves,
loads or ticks the simulation itself. Player action execution continues through the
existing native coordinator and recorded shared action adapters.

Registries remain frozen. New/Continue uses the active immutable island profile;
fresh worlds are constructed and validated in an isolated headless app before any
replacement. Existing worlds remain compatible. Crop variety uses existing item
kinds and persisted `Crop.kind`/deadlines; player guidance is a pure observation
projection, so this increment does not need a new hashed checklist or silent save
migration.

Shared `game_ui` typography/control primitives provide one visual vocabulary across
the host shell and player workspace. Housing, production, survival, crops, input,
presentation and file ownership retain their documented bounded contexts.

## Research-backed refinement

The title/setup offers inspector-first entry without beginning the reserved visitor's
life. It leaves `AwaitingArrival` intact until explicit character play. After death,
a view-only dismissal allows continued spectating and later successor selection;
neither dismissal nor mode changes can revive or replace an actor.

The native coordinator changes Inspector/Editor presentation immediately but defers
detachment while accepted or active physical work still owns its reservations.
Once that work settles, the latest desired mode determines whether to detach.
Returning to Character early retains the same authority. Explicit cancellation,
resident BT interruption and death remain distinct from this host transition.

The player workspace leads with the selected site's work or selected resident's
conversation. An Explore workspace offers known destinations and short relative
walks, while the content-owned discovery provider retains fixed places through the
actor's existing `SeenStatics`. It never supplies unseen stock, moving-person positions
or foreign plot state. Outcomes come from scoped receipts/events, not button intent.
The HUD names the simulation rate and labels durations in simulation time.

Visibility/selection gates avoid building hidden legacy or unused preview observations.
They affect derived view work only, with attribution in the existing profiler. No
authoritative scheduling, registry identity, snapshot schema or dependency upgrade is
needed for these changes.

These choices apply the
[experience](../research/2026-11-09-experience-research.md),
[simulation](../research/2026-11-09-simulation-research.md) and
[implementation](../research/2026-11-09-implementation-research.md) research to the
existing architecture. The briefs also identify separate future scope, including
general behavior-graph editing, demographics, terrain undo and audio. Verification
is recorded separately in the [validation note](../_notes/2026-11-09-island-player-flow-validation.md).
