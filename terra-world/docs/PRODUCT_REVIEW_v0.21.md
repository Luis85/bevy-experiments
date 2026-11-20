# Product review — Terra World 0.21

Reviewed on 29 October 2026 against the supplied 0.20.0 source archive. Terra
World remains a substantial regional sandbox reference game: persistent
characters and ownership, finite supply and company economies, physical walking
and driving, continuing activities, local recovery, prepared geographic sources,
and an authoritative multiplayer protocol. This release prioritizes player
control, progress safety, valid-world startup and bounded rendering work.

Three dedicated specialists reviewed documentation and source from player,
world/simulation and reliability perspectives. The integrator reviewed network
boundaries, delivery, compatibility and cross-module behavior. Owners implemented
their assignments and cross-reviewed another owner's changes. These are three
multi-perspective specialist reviews plus integration; the six specialists
recorded for 0.20 are historical work.

## Assessment by product perspective

| Perspective | Finding and decision |
|---|---|
| First and returning player | Loading exposed recovery actions without controller navigation. Text editing lacked a native modal focus boundary. Both now have explicit input ownership and behavioral regressions. |
| Information and comprehension | Direct JSON play promised clean-exit saving that only managed worlds implemented. Shared wording now describes managed JSON, direct JSON, regional and server-owned persistence accurately. Long session/editor messages wrap. |
| Game design and finite economy | Valid shops with different assortments could panic during maintenance. Unsupported issuer valuations now skip before consuming IDs, offer capacity or reserved funds. Supported offers and finite funding remain. |
| World and geography | Polar clipping made a fully resident six-tile neighborhood wait for nine. Living initialization and native Loading share the real neighborhood; unrelated or outdated rendered data cannot satisfy readiness. |
| Rendering and performance | One feature edit invalidated every resident mesh. Exact per-chunk context now limits rebuilding while retaining source identity, LOD, bounded workers, backoff and stale-result rejection. |
| Save integrity and recovery | The native adapter bypassed core validation for complete live state. Each new activation now uses the asynchronous barrier. JSON replacement temporaries are private from creation on Unix. |
| First-session failure | A crash after saving but before profile metadata update could leave a played world looking new if progress later disappeared. An immutable pre-launch record now preserves that distinction for newly admitted launches. |
| Multiplayer and service | Existing authoritative commands, Welcome/source admission, TLS and private resume-capability contracts were reviewed and retained. Public accounts/moderation, WAN capacity and distributed authority remain separate requirements. |
| Delivery and operations | Stalled commands or startup probes could hold acceptance indefinitely. Finite deadlines now retain partial logs, classify failure and attempt owned process-tree cleanup. Packaging still requires exact completed acceptance. |
| Documentation | Historical release detail obscured README setup/current scope. It is preserved in `RELEASE_HISTORY.md`; current review, plan, full gap register and validation form one handoff. |

## Implemented improvement pass

The [implementation plan](IMPLEMENTATION_PLAN_v0.21.md) was written before
dispatch and assigns WP01–WP10, including subsequently reviewed WP08b. The
selected changes are implemented; [Validation](VALIDATION.md) records executed
outcomes, final acceptance and native inspection separately.

### Player control and presentation

Loading's Cancel connection, Save and return, and Close failed world actions are
reachable through logical controller navigation. Saving remains noninteractive
and Loading continues to block world commands. The on-screen keyboard owns a
modal native tab group, repairs focus before native dispatch and guards parent
observers through the closing frame. Apply and Discard retain the reviewed
target and release ownership afterward. Tests check both blocked parent
activation while editing and successful activation afterward.

Managed JSON saves on clean exit but has no periodic autosave. Direct JSON
requires F5 or the explicit Save-and-exit action; closing the window is not a
save. Regional checkpoints and server-owned persistence keep their distinct
contracts. Shared wording corrects a player-facing promise without changing
direct-play persistence policy.

### World correctness and efficient presentation

Heterogeneous retail assortments no longer reach an unchecked price lookup.
The central issuer still funds supported requests; unsupported requests receive
no invented prices and consume no escrow or offer capacity. The regression uses
valid catalog data, supported offers, conservation and saved-state continuity.

Startup counts actual neighborhood keys, including clipped polar rows and
longitude wrapping. Native Loading uses the active player's location and current
source-integrated terrain, retaining the nine-page startup cap. Smoke capture
still waits for the full displayed neighborhood. Invalid empty neighborhoods,
unsupported ground and unrelated pages cannot fake readiness.

Mesh jobs retain an exact effective removed-feature set per source chunk. In a
49-page regression a local feature schedules one job; a feature split across two
pages schedules two. Other meshes remain valid. Quality changes still invalidate
all pages under the two-worker limit. This establishes work-count reduction,
not frame-time improvement on a physical GPU.

### Progress safety and bounded verification

New native command/source activation batches use asynchronous hydration even
with complete live state, unless a validated restore already admitted the batch.
Valid live edits survive; one pending request serves a batch; checkpoint
retirement closes it. Corrupt committed state cannot hide behind the native
adapter's fast path.

The managed `world-started.json` record means initialization was admitted, not
that a checkpoint succeeded. It is published exclusively before managed
persistence initialization or writes. Later launches require validated progress,
including a nonzero regional generation. A first launch interrupted before any
checkpoint deliberately requires recovery or a separately created world; no
automatic reset is allowed.

The record binds world/source identity, canonical profile location, progress
path and mode while allowing rename and presentation changes. Valid legacy
progress is checked before adoption. Already-lost legacy progress cannot be
distinguished retroactively, and older launchers do not enforce this guarantee.
Manual archival copies must retain the complete managed folder and external
dependencies; use supported verified recovery for relocation.

Screenshot, political export and diagnostics reserve both managed metadata
paths, including ancestors, locks and existing aliases. Runtime publishers
recheck the boundary. JSON temporary files start at Unix 0600 even under a
permissive umask. General parent-directory synchronization uncertainty remains
the explicit R4 follow-up.

Acceptance commands and startup probes have positive finite deadlines. Timeouts
retain partial logs and fail acceptance; startup evidence is hashed alongside
gate evidence. Real POSIX descendant cleanup is exercised. Windows cleanup
failure is reported as unconfirmed; native Windows execution remains required.

## Cross-review corrections

- The first polar fix left native Loading with a square threshold. The shared
  neighborhood now reaches both admission layers, and only current integrated
  source identity counts toward terrain readiness.
- Startup probes initially retained a helper that could leave children alive
  and lose partial output. They now use the logged bounded process runner.
- Output directory creation could occupy the future launch-record pathname.
  Preflight and relevant runtime publishers now protect managed metadata.
- Three new UI fixtures asserted before Bevy's deferred Activate command was
  flushed. Rejection and positive-control checks now flush the actual queue;
  no production guard was weakened.

Notes are under `verification/v0.21-review`. Failed candidate logs remain
separate from final accepted evidence.

## Remaining priorities and compatibility

The [complete G01–G18 register](REVIEW_GAPS_v0.21.md) carries every inherited
closure criterion forward. Windows execution and a clean-machine
create/play/save/reopen/recovery journey on the user's intended PC remain the
highest delivery priority. Linux software-rendered inspection cannot satisfy it.

Default RTS Activities still needs a persistent selected-owned-actor and needs
summary designed for multi-selection, vehicles and remote partial state.
Uncoached first/returning journeys and sustained finite-economy play are needed
to judge comprehension and balance. Community succession/dissolution and
treasury policy need explicit product decisions.

Physical/assistive devices, GPU tails/residency, arbitrary hung mesh workers,
production art/audio and licensed geography, content migration, trusted updates,
public accounts/moderation, WAN soaks, storage tails and operational recovery
remain open. Close each with its named evidence rather than test-count inference.

Release version is 0.21.0. Domain save 1, regional schema 1, protocol 10 /
`terra-world/10`, and asset schema 1 remain unchanged. The launch record is
additive local lifecycle metadata. External Rust source consumers should compile
against the updated workspace. Historical 0.20 current review and validation are
preserved byte-for-byte as `PRODUCT_REVIEW_v0.20.md` and `VALIDATION_v0.20.md`.
