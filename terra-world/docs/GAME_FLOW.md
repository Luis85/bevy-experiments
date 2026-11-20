# Native game flow

## Reviewed actions and recovery in 0.23

Nature and Activities explicitly name the active person whose supplies and
needs they affect. Inspecting another body does not retarget these actions.
Command's Eat/Drink still requires that same active person to be the single
owned selection. All three surfaces retain the displayed person, player and
restored-world context. A queued change of body, possession, restore or remote
disconnection requires a fresh review before a consumption command is submitted.

Resource travel/gather and territory travel/contribution/funding/upgrade retain
their displayed target and relevant selection/value context through the native
action queue. A missing explicit patch or territory stays unavailable. **Next
patch** and **Next territory** deliberately choose another available target;
they do not give a queued action permission to use that new target before its
label is displayed. A refresh after activation also cannot silently replace the
identity of the queued action. These checks do not reserve stock, space, money
or access; authority validates the ordinary command again.

The supply noticeboard uses the domain's shared current-condition readiness for
Accept, Deliver and Return. It names the concrete blocker and recovery: carry
the requested goods, approach the recorded destination, wait for receiving
stock space, make wallet space or explicitly return accepted work. Expired
unclaimed requests cannot be accepted. An out-of-view receiving service is not
described as permanently deleted, and a received view cannot reserve delivery
capacity on the server. Returned work spends no carried goods; its existing
escrow rules remain authoritative.

## Actor context in 0.22

The footer now keeps selected or controlled actor context visible while
Activities is open and while panels recede during possession. A single actor
has a name, ownership/control status and biological needs when applicable.
Command uses the same resolved values, full name and current order. No selection
or an unavailable actor is stated directly; neither silently becomes a different
active body. Remote context names its received/partial scope.

Multiple selection names the owned, inspect-only and unavailable counts. Needs
are labeled **Lowest** across selected owned biological bodies, or **Lowest
shown** if a selected body is missing. Each minimum can belong to a different
body; it is not an average or the needs of a single primary actor. Vehicles do
not contribute artificial zero values. Driver needs belong only to the exact
owned vehicle whose possession has been admitted by authority.

Command explains the target difference: eating/drinking affects one selected
active owned person, while Stop/Rest concerns the selected owned group. A native
consume click retains its displayed owner/world-revision/body context and asks
for review if that context changes before submission. Group orders capture
owned IDs when dispatched. Ordinary domain ownership, stock and needs checks
remain authoritative. The unchanged SDK/network Consume operation still uses
the active body at authority processing time.

Long in-world labels wrap at words or characters, compact HUD names are bounded,
and the detailed inspector retains full names. The existing scaled footer and
world interaction boundary remain in use; no extra overlay covers the world.

## Continuation and finances in 0.12

Fleet → Finances shows recorded company receipts, payments, setup and owner
funding, with exact cash reconciliation and recent entries. Compatible older saves
without accounting start with an imported balance and retain earlier revenue and
payroll as historical; the view does not fabricate missing costs. Unknown input and preassignment
vehicle costs remain explicit. Company deposits do not count as earnings.

Before registration, Finances shows one prompt and **Manage company** opens the
Company section immediately. Detailed cash, funding, category, coverage and
recent-entry groups appear only when the player has a company. The shared empty
summary stays short so the registration path remains visible at enlarged text.

Fleet sections show their selected state and reset the inspector to the section
start. At the minimum window or enlarged text, a compact section chooser keeps
content visible. Text scaling itself preserves scroll position. Ordinary walking
orders now remove redundant perimeter legs only when complete resident ground,
source geometry and body clearance support a shorter corridor. A nearby goal can
still require a longer road route when a parked vehicle obstructs the direct
shortcut.

Returning from possession to a body-focused RTS overview restores zero yaw with
the overview offset and pitch. A location/atlas Release preserves its explicit
spectator destination and orientation. Camera framing changes no player identity,
inventory or physical movement.

## Reviewed handoffs retained from 0.11

Activities preserves the exact market recommended by a goal. Market shows the
selected seller, total prices for the chosen quantity, stock and concrete
unavailability reasons. Choose market changes the seller explicitly. Go to
market and Go to production use the same loaded-ground, body-clearance and
vehicle-approach planner as guided travel.

Personal purchases/sales, depot trades and roadside service retain the quote
shown in the panel. A changed price, quantity, selection or restored world asks
for review before spending. The authority also checks the quote after enqueue,
so a valid click cannot bypass later stock, funds or price changes. Successful
local restore retires remaining intents from the replaced world; a failed
restore leaves the current world and its input context intact.

Vehicle exits choose a supported, unoccupied point beside a stopped vehicle.
If none is available, the player remains in the vehicle and receives a reason
to reposition. Purchased and rented vehicles use bounded clear garage parking
before the transaction can spend funds or allocate a body.

Terra now starts at a native desktop menu when launched without arguments. The
same executable remains a command-line integration example: an explicit game
option or `--play` starts the world directly, including existing `--frames`,
`--smoke`, `--screenshot`, configuration and server workflows.

```bash
cargo run -p terra-demo
cargo run -p terra-demo -- --play --camp-view
cargo run -p terra-demo -- --menu --user-data /path/to/my-terra-worlds
```

Run from the repository root so bundled source packs can be found. No first-run
account, external service, live API key or network download is required.

## New local world

Existing worlds have a **Rename** action in **Your worlds**. Creation times help
distinguish copies; a chosen name is retained across refresh and later launches.
**Save name** changes the selected profile's title after checking that its full
displayed state is still current. It does not open the world or save gameplay.
The recovery review also offers **Recovered world name** before creating a new
copy, so repeated restores can have distinct, recognizable names from the start.

1. **New world** opens character creation. Type a name, choose a body silhouette,
   skin tone, hair color and outfit, and rotate the native 3D preview. The same
   palette and silhouette functions drive the in-world character. Appearance has
   no effect on authority, starting money or simulation rules.
2. **World settings** chooses the world name, source preset, WGS84 latitude and
   longitude, seed, requested loaded region, initial hour, time speed, day/night,
   seasonal visuals, weather, vegetation, graphics quality, atmospheric sky and
   storage mode. Names and numbers are real editable text controls. Near, Local,
   Region and Wide show approximate geographic width and depth at the chosen
   latitude; technical chunk counts remain in diagnostics.
3. **Review world** shows the actual source coverage after a heightmap or dated
   pack limits the requested neighborhood. It states the persistence behavior
   and gives the first activities to try.
4. **Create world and play** allocates a new, exclusive folder, writes its
   profile, and starts the game. Two worlds with the same displayed name receive
   different folders; creation never reuses or clears an existing world. Managed
   worlds open with the regional overview, with nearby land and settlement in
   view. The explicit `--camp-view` CLI option still offers a close starting view.
5. **Preparing your world** displays actual source, terrain and population
   readiness. Source and terrain progress use the actual nearby neighborhood
   around the active player, including the smaller neighborhood at either pole.
   Gameplay input is enabled after the required resident terrain has its current
   source rendered, an owned account exists, and regional work is neither pending
   nor blocked. A multiplayer client also waits for its connected actor.
   A source, placement, storage or connection failure remains visible with a
   route back to the menu.

### Loading recovery (0.21)

While **Preparing your world** is open, controller D-pad/stick navigation and
South activation reach the available recovery action. Local worlds offer **Save
and return to main menu**; a joining client offers **Cancel connection · Main
menu**. If the regional worker has stopped and the world must be reopened, the
action becomes **Close failed world**. Gameplay movement and commands remain
blocked during Loading. A requested local save enters Saving and waits for its
acknowledgement before leaving; Saving does not expose controller menu actions.

### Source choices and their limits

| Menu choice | Actual source | Coverage and interpretation |
| --- | --- | --- |
| Living frontier | Deterministic procedural terrain and authored scenario features at real GPS coordinates | The coordinates are geographic; the settlement, jobs, industry and population are authored content. |
| Heightmap valley | Two adjacent georeferenced grayscale PNG bundles and explicitly authored GeoJSON land cover | Bounded authored terrain near `48.137, 11.575`; it is not a survey of Munich. |
| Historical scenario · 1950 | Bundled immutable dated pack and political scenario | Deliberately fictional; a date does not reconstruct real historical conditions. |
| Historical scenario · 1980 | A second dated version of that fictional scenario | Its source identity and coverage differ from the 1950 scenario. |

Source paths are resolved and validated before a world folder is created.
Coordinates outside a bounded pack produce an inline error; the menu does not
quietly move the requested location. The requested neighborhood is reduced only
to the largest complete neighborhood supported by that pack. Live adapters and
game-owned sources remain available through the command line and SDK.

### Storage choices

World settings offers regional storage or portable JSON. The session menu and
World panel describe the save contract for the current session:

| Session mode | Save behavior | Resume behavior |
| --- | --- | --- |
| Regional database | SQLite checkpoints on cadence, source residency changes, F5, session-menu save and clean exit; dormant regions remain in the store | The authoritative checkpoint replaces provisional character and clock state before the world starts. |
| Managed portable JSON | Complete snapshot on F5, session-menu save and clean exit; no periodic autosave | A validated existing snapshot is restored before the world starts. |
| Direct-play manual JSON | Use F5, **Save and return to main menu** or **Save and quit**. Closing the window does not save progress; there is no periodic autosave. | Launch with the same source configuration and `--save FILE`, then use F9 to review restoring that snapshot. |
| Multiplayer client | The host owns progress; the client cannot save or restore the shared world. Leaving closes the client session. | Reconnect to the same server world with the same session name to resume the host-owned identity. |

For local worlds, F9 opens a review of the latest successful checkpoint/snapshot.
Confirming the reviewed restore discards later changes; cancelling preserves the
current world. The action label states this consequence. Settings and character
metadata are not substitutes for a world checkpoint.

The menu's portable JSON mode adds clean-exit saving for managed worlds. Existing
direct-play `--save FILE` retains its manual-save behavior. Regional
storage is still available directly with `--region-store FILE`.

## A continuing player journey

**Activities [J]** starts with a suggested goal. Open the goal chooser to
select directly among the six available goals, including supply work, a home,
community development, driving work and Company. The Company goal follows the live, paid setup and operation of your
own business; see [Company guidance](GUIDANCE.md#company-decisions-after-the-first-jobs).
Each goal gives a concrete next action, the resources or money it requires and
progress derived from the current world. A funded supply job progresses through
acceptance, obtaining actual goods, physical travel and delivery. Delivery uses
the existing escrow and cannot pay twice. Finishing it changes the suggested
next goal; there is no extra tutorial inventory or hidden reward ledger.

The guide also resumes your actual accepted driving assignment, including its
vehicle, rather than whichever offer was last browsed. Home and community goals
respect personal/faction access. Missing local infrastructure is described as
missing from the loaded view. A bounded nearest-resource search refreshes as you
move; it does not claim to search a complete planet.

The source banner retains the next goal when possession hides panels. On a
small window with enlarged text, **Choose activity** opens the inspector's page
chooser while leaving room for the current page's controls. Keyboard management
focus remains available with F6. See [GUIDANCE.md](GUIDANCE.md) for the SDK and
transaction contract.

## Continue and world selection

**Continue** selects the most recently used managed world. **Your worlds** lists
up to 24 recent valid profiles, including a created world that has never started.
Invalid profiles are skipped with a written warning. On launch, the authoritative
restore path checks checkpoint integrity and source identity; file existence
alone does not establish valid progress.

Before a managed runtime can write progress, it publishes `world-started.json`
beside its profile. This records an admitted start, not a successful save.
Subsequent launches require a valid checkpoint, including after an interrupted
first launch that never finished saving. Missing, empty, corrupt or symlinked
required progress, and regional stores without a committed generation, stop for
recovery instead of becoming a fresh world. **Backups and recovery** stays
reachable even when the damaged profile no longer appears in the valid-world
list. Keep the record and restore a verified backup or create a separate new
world. [Compatibility](COMPATIBILITY.md#managed-launch-and-checkpoint-compatibility)
explains legacy limits and why the record belongs with its original managed
folder.

### Back up and recover a world

After saving and returning to **Your worlds**, **Make backup** captures a supported
regional world's committed progress together with its exact source/configuration
dependencies. It uses that world's resolved profile directly. The menu names
the copy in progress and its checkpoint when the copy is verified. Unsaved,
portable-JSON, live and historical worlds explain the separate workflow they
require.

**Backups and recovery** shows recent local copies. **Verify** opens a review of
the named world, checkpoint, backup time, character and graphics settings. Back
cancels; **Recover as new world** restores into a new managed folder, then adds
the recovered world to **Your worlds**. Existing worlds and the retained backup
stay intact, and the recovered world does not launch automatically. Its complete
checkpoint must validate before its launchable profile is published. The optional
folder field also verifies complete native backup copies kept elsewhere.

The copy/verification runs on a single bounded background worker. Navigation stays
available; another operation, launch or close waits for the active result. The
review retains its exact metadata and checkpoint selection, so refreshes, changed
files, cancellation or returning from another screen cannot silently redirect a
confirmation. Restored runtime paths refer to the new managed folder and the
saved political-source choice remains explicit. See
[Recovery bundles](RECOVERY_BUNDLES.md) for supported worlds, private-copy limits
and the operator workflow.

A world profile keeps its source configuration, initial character, title and
presentation settings. On resume, the saved authoritative character, actor IDs,
inventory, economy, ownership and clock take precedence. A chosen initial hour
never resets a resumed clock. Reopening a world does not reseed its existing
starter population or replace its profile choices.

Successful restores advance a runtime-only `SandboxRuntime.restore_revision`.
Once loading finishes, the local controller adopts that restored active actor
once and uses third person if it was possessed. This marker is not saved and
ordinary simulation revisions do not advance it, so choosing RTS with F1 stays
effective until a later explicit restore or new authority session.

Use `--world-profile /path/to/world.json` to launch a managed world directly.
Source or save-path overrides cannot be combined with this argument: create a
separate direct-play configuration for a different world. Graphics and
verification options can still be supplied explicitly.

## Session menu

For repeatable assisted sessions and independent player feedback, start with
[Playtesting](PLAYTESTING.md). Main-menu **Settings → Playtest diagnostics** is
Off by default. Turning it On applies to sessions launched from that menu;
**Save defaults** retains the choice for future menu launches. Reports are local,
bounded and distinct from saves. The path and retained status appear in Settings;
the session menu and F12 show current publication, age and errors. Capture uses
real time even while a local session is paused, and records virtual pause,
calendar pause and host/client context separately.

When the regional storage worker is lost, **Close failed world** replaces
retry/restore/save-and-leave actions that cannot recover that session. It closes
through the normal error/exit pipeline. Reopening validates the actual committed
generation, including an operation that may have committed before its completion
message was lost. Ordinary recoverable storage failures retain retry/restore.
The launcher keeps specific storage and diagnostic export errors separate.


Escape first cancels active text entry, then building placement, then an unfinished
political territory draft. Each cancellation consumes that input frame and stays
in the world. With no edit active, Escape opens the session menu. It offers Resume,
Checkpoint now, Settings, Controls, Restore latest checkpoint, Save and return to
main menu, and Save and quit. Multiplayer substitutes leave-server actions for
local save authority.

The local menu pauses Bevy's virtual time while preserving the saved
`WorldClock.paused` and time scale. A checkpoint made from the menu therefore
does not accidentally resume as a permanently paused world. A clock already
paused by the player stays paused after the menu closes. Multiplayer menus block
only local input; they do not pause the host or the network client.

Saving keeps the session open until the requested operation is acknowledged.
For regional storage, both an advanced committed generation and no pending work
are required. Queueing a save is not presented as success. On a write failure,
the menu reports the error and offers a retry or restore; it does not discard
the live session by returning to the main menu. A final exit checkpoint is also
drained before the world, worker and native application are dropped.

Graphics settings apply immediately. Managed-world settings are stored with that
world; menu defaults apply to newly created worlds and multiplayer clients.
Changing source identity, seed or terrain coverage is a new-world operation.
Performance quality disables atmospheric scattering; toggling the sky does not
override that deliberate quality limit. The saved setting can be used again by
Balanced or High quality.

## Multiplayer join

The **Multiplayer** form accepts an IP address and port, including bracketed IPv6,
plus a session name and an optional local source configuration JSON. Leave the
source blank for a default procedural host; a heightmap or historical host needs
its matching local `WorldConfig` and data files. The source configuration is
checked before creating session state. Live adapters and local regional stores
are not admitted as client source settings. File paths inside the configuration
use the project working directory. The gameplay connection does not transfer
source files. Each server/player pair should use its own session name.
Reuse that name with the same server world to resume the stored identity. The
launcher remembers these connection settings without placing bearer tokens in
the form, logs or profiles; the network adapter manages the separate private
resume identity.

**Verified TLS** adds a trusted CA certificate PEM path and the hostname on the
server certificate. The connection worker performs certificate-chain and server
name verification. PEM input is bounded to 1 MiB. This encrypts the transport
and verifies the configured server; it does not implement an online account or
password service.

Plain TCP is available for localhost. A non-loopback plaintext connection needs
the explicit **Allow unencrypted remote connection** choice. That setting is
remembered as a connection preference. The matching command-line form is:

```bash
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/sessions/driver.json
cargo run -p terra-demo -- --connect 192.0.2.7:7777 --tls-ca /path/to/ca.pem --tls-server-name game.example.org
cargo run -p terra-demo -- --connect 192.0.2.7:7777 --allow-insecure-remote
```

The `192.0.2.7` address is a documentation example. Supply the actual server IP.
Both TLS flags are required together. Remote plaintext without the explicit
choice fails before opening a game window. Authentication, hosting, source
manifest compatibility, account customization and authoritative storage remain
host responsibilities. The join menu does not overwrite a resumed server
character with a local character-creation draft.

If a connected session ends, the native session menu shows the failure and
directs the player to reconnect with the same session name. Last connection
settings survive the return to the launcher.

## Controls

| Context | Controls |
| --- | --- |
| Native menus | Tab / Shift+Tab or controller D-pad/stick moves focus; Enter/Space or South accepts; Escape or East goes back; mouse wheel scrolls overflowing content. |
| Preparing your world | D-pad/stick and South reach the displayed cancel, save-and-return or terminal close action; gameplay stays blocked. |
| Editable forms | Native cursor movement, selection, Backspace/Delete, Home/End and keyboard input. F4 or controller South opens the focused field’s on-screen keyboard. Fields enforce single-line and length limits. |
| Open text keyboard | Tab / Shift+Tab stays among the draft and its five editing actions. Enter on the draft applies it; Enter/Space chooses the focused action. Start applies; Escape/East discards. |
| RTS | F1; click/drag to select; Shift adds selection; right-click loaded terrain to order movement. |
| Possession | F2 third person; F3 first person; C changes view; Tab focuses the actor. |
| On foot | WASD movement; right-drag look; Shift sprint; E contextual interaction. |
| Flight | R/F altitude with a suitable possessed actor. |
| Driving | W/S accelerate/reverse; A/D steer; Shift brake; Space handbrake; P parking brake; L lights; Z/B indicators; H hazards. |
| Work and management | J activities; N natural resources; T territory; V driving jobs; G garage. |
| Inspection | F10 world settings; F11 inspector; F12 performance. |
| Cancel or pause | Escape cancels active text, placement or territory drawing; otherwise opens the session menu. |
| Progress | F5 checkpoint/save; F9 reviews restoring the saved checkpoint/snapshot. Host-owned clients cannot save or restore. |

Menu activation keys are held away from gameplay for the closing frame, so
Space used to resume cannot also activate a handbrake or clock shortcut.
The on-screen keyboard keeps its draft private until Apply. Native focus stays
within the keyboard while it is open, and Apply/Discard restores the original
form field's focus. Parent launcher, session and world buttons reject activation
while the draft is open and during its closing frame. Session and keyboard labels
use the launcher's word-or-character wrapping for long paths and error messages.
Keyboard and mouse are the native review path. Controller menus and calibrated
wheel/joystick input have virtual-device test coverage; physical hardware
compatibility remains to be certified. Platform clipboard
integration is not enabled; the bundled editable widgets retain their native
editing behavior without a claim of system clipboard support.

## Files and lifecycle

By default, `saves/` under the project root contains:

- `worlds/world-<unique-id>/world.json`: bounded, versioned world profile.
- `worlds/world-<unique-id>/world.db`: regional world, when selected.
- `worlds/world-<unique-id>/world-export.json`: portable snapshot, when selected.
- `preferences.json`: presentation defaults.
- `last-connection.json`: server address, session name and transport preferences.
- `sessions/<session-name>.json`: private, address-pinned network resume identity.

`--user-data` chooses a different root. New directories and private metadata use
restricted Unix permissions. Profiles are limited to 256 KiB and are validated
before use. World creation uses exclusive directory/file creation; settings use
temporary-file replacement. Profile, save and database paths are checked against
aliases to prevent a requested export from overwriting authoritative data.
Managed source paths are absolute: when moving a world to another installation,
provide the same source revisions and update source paths deliberately.

The launcher closes and drops its native application before starting the world;
its process supervises the game's fresh winit event loop. The final session
handoff is captured after the checkpoint into a bounded channel outside Bevy's
consumed application. Return to main menu starts a fresh launcher only after a
successful game exit, resource drop and closure of the captured error stream.
On Unix, the supervisor replaces its own process image with that launcher, so
repeated returns retain one supervised lifetime without accumulating old
supervisors. Other platforms use the separate-process fallback. The supervisor
captures a bounded error tail and reopens a visible recovery notice if a child
cannot start or finish successfully. Arguments are passed directly to `Command`,
never to a shell.

## Verification entry points

The launcher exposes deterministic screen selection for the shared native review:

```bash
terra-demo --menu --user-data /tmp/terra-menu-review --menu-screen character --frames 45 --screenshot /tmp/terra-character.png
terra-demo --menu --user-data /tmp/terra-menu-review --menu-screen world --frames 45 --screenshot /tmp/terra-world-settings.png
terra-demo --menu --user-data /tmp/terra-menu-review --menu-screen join --frames 45 --screenshot /tmp/terra-join.png
```

Allowed screen names are `home`, `worlds`, `backups`, `character`, `world`,
`review`, `join`, `settings` and `controls`. Menu screenshot capture is requested at frame 25;
menu automation requires at least 30 frames. PNG output may replace an existing
regular PNG, but never a symlink or a non-image world file. Normal game
automation keeps its existing minimum of 100 frames.

World screenshot capture waits for the complete displayed neighborhood to be
resident and rendered from its current sources, with no in-flight source loads
or pending/building mesh jobs. Unrelated pages or old geometry retained during
source replacement cannot satisfy that readiness check. This capture threshold
is separate from the smaller, capped neighborhood used to admit gameplay.

Focused automated coverage exercises invalid input without writes, unique world
allocation, all bundled presets, profile/DB path protection, session-name
traversal rejection, remote transport opt-in, save acknowledgement, local versus
network pause behavior, and clean portable save/resume preserving the character
and exact clock. Native screenshot evidence and the product review are recorded
in the release verification report.


## Flow foundations introduced in 0.9

Storage selection appears directly below source information in World settings.
The pause screen names the last durable checkpoint once; regional detail remains
in F12. Controls in the main menu and session menu share a persistent movement
and driving binding editor, mouse/gamepad sensitivity, deadzone and invert-Y.
Binding capture consumes Escape and rejects reserved/duplicate keys without
replacing the previous binding. See [CONTROLS.md](CONTROLS.md).

Fleet [Y] adds a complete company flow inside the inspector: registration and
funding, hiring, route drafting, assignment, dispatch, observation, pause and
release. The editable passenger-loop suggestion uses actual eligible stops.
Company cash, personal cash, cumulative fares and employee wages remain distinct.
A parked blocked route can be explicitly resumed. See [COMPANIES.md](COMPANIES.md).

The native gamepad backend supports world movement/look/driving and the main and
pause-menu flows. D-pad or stick moves focus, South accepts, East goes back, and
a bounded on-screen keyboard edits character/world fields. F4 also opens that
keyboard for the focused text field. Controls include 100–150% text scale,
55–100° first-person vertical field of view and optional driving auto-center.
A configurable Bevy/Gilrs wheel profile adds selected axes, calibrated endpoints,
separate/combined pedals, deadzones and neutral rearming.

F6/right-stick click enters visible in-world management focus; enabled actions
scroll into view, names and coordinates use a private draft, and exiting cannot
leak intent into actor movement. A bounded controller terrain cursor supports
RTS selection, orders and building placement. Political atlas drawing remains a
mouse/keyboard path. Virtual input tests do not certify physical hardware, force
feedback or screen readers. See
[CONTROLS.md](CONTROLS.md) for exact bindings and boundaries.

Fleet exposes Company, Finances, Drivers, Routes, Recovery and Depot sections. Loaded work
can be held, physically returned or handed over to a nearby qualified relief
crew. Depot stock transfers and public-service contributions use authoritative
quotes and real inventory/money. Garage presents prepaid rental, renewal and
quoted empty-return confirmation. Expiry preserves the body and obligations;
renewal is required before further driving. Save-and-return preserves those
commitments and the independent control preferences.

## Reviewed restore (0.20)

F9, the in-world Load action and the session-menu Restore action open the same
review. Cancel is initially focused and keeps current progress. The review names
the configured saved snapshot or regional checkpoint generation and explains
that confirmation discards later unsaved changes. Escape cancels back to the
paused session; it does not resume world controls. Confirmations bind a monotonic
review identity, so a queued confirmation from a cancelled review cannot accept
a newly displayed target.

The local engine rechecks source/configured paths, principal and restore context.
Portable JSON must match the exact reviewed bytes used for loading (48 MiB
limit). Regional restore must match the observed generation inside validated
store admission before changing cursor/live state. This is generation-bound
consent under the existing storage integrity contract, not authentication against
an attacker forging valid same-generation database contents. A changed checkpoint
requires review again. Pending/terminal storage and remote-host ownership retain
their existing restrictions. Plain SDK Load remains a trusted host operation.

## 0.24 operational continuity

Continue supply work from the actual saved state. New offers fit useful demand
and receiving capacity. Accepted work keeps its original promise even if another
delivery fills the shortage. Confirmed obsolete public work is removed only
after its reserved reward can be returned in full to its original issuer.

Industry and Fleet show the current operation against the chosen facility,
driver, assignment or depot. If a selected object disappears, choose another
explicitly. An old activation cannot become an operation on a different object.

Building damage uses a named review in the inspector: read the health effect,
then Cancel or Confirm. The initial focus is Cancel. A changed target or
eligibility expires the review, requiring a deliberate fresh action.

