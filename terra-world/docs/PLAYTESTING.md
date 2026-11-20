# Playtesting Terra World 0.24

This guide is the handoff for a repeatable playtest and the next development
iteration. Start with the bundled synthetic world, then introduce a prepared
source or model pack once that baseline works. Read [Validation](VALIDATION.md)
for the exact checks completed on this release, including the build profile and
the scope of any X11/software Vulkan captures. Windows,
physical GPUs, controllers and wheels still need execution on the intended
machines; a configured CI job does not establish that qualification.

## Targeted 0.24 checks

1. Accept supply work, then continue ordinary trade/production. Accepted amount,
   reward and deadline must remain as reviewed; public obsolete work may disappear
   only through conserved issuer refund. Record continued useful work after save/reopen.
2. In Industry, select a facility and read its named next operation. Pause/resume
   near an owned facility. An unavailable old selection must ask for a new choice.
3. Register/fund a company, hire/select a driver, choose a route/vehicle and
   operate its service. Assignment, pause/run, hold/release and depot transfer
   must use the reviewed identities/amounts and explain refusal.
4. Build a permitted structure. Open damage review, check the exact structure
   and health consequence, cancel, then deliberately confirm. Record health and
   property after saving/reopening. Use a separate test world for destruction.
5. Use keyboard management focus for review and cancellation; check reference
   1440×900 and minimum 1100×700 with 150% text. Record clipping or unreachable controls.
6. Extract the delivered ZIP, run its passive verifier, then the optional help
   smoke. Keep its report and distinguish actual Windows/physical-device execution
   from configured CI or Linux software-renderer evidence.

Automated fixtures cover timing/capacity/alias and malicious archive conditions;
these ordinary journeys do not establish uncoached usability or every edge case.

## Targeted 0.23 checks

Use isolated test worlds for these checks. Record the visible target, expected
change, actual result and any obstacle; do not delete lease/witness files to
force a world open.

1. Open Nature and Activities with an owned person, another selected crew member,
   a vehicle and an inspect-only resident/wildlife body. Food/water text must name
   the person who would receive it. A changed reviewed context asks for a new action.
2. Choose a resource or territory, then change the choice before acting. The old
   action must not gather, move, contribute or fund a different destination.
   An unavailable exact target needs an explicit fresh choice.
3. Complete supply work through accept, obtain, travel and delivery. Record the
   exact funded reward and remaining goods. Where an isolated fixture supplies
   full recipient/wallet conditions, the UI must explain recovery and preserve
   goods/escrow; releasing capacity permits deliberate completion.
4. Save, reopen and return to accepted work. Guidance rebuilds from actual state;
   saved goods, money, deadlines and ownership remain consistent.
5. Try Rename and Make backup from a second launcher while the managed world is
   open. They must explain the active session/maintenance conflict. Busy backup
   creates no output. After clean save/exit, refresh and retry; regional backup
   must verify and recover to a separately named world.
6. Check 1440×900 and 1100×700 with 150% text. Reach primary actions and written
   recovery through existing scrolling and keyboard focus. Test physical input
   devices separately; automated dispatch is not their qualification.

These are repeatable acceptance tasks, not a report of uncoached participants.
Human first/returning-player sessions remain required for G03/G04/G08.

## Before opening a world

Extract the entire desktop ZIP. On Linux, run `start-terra.sh`; on a separately
built and accepted Windows package, run `Start-Terra.cmd`. The source ZIP requires
building first; it is not a Windows executable. See [Desktop delivery](DESKTOP_DELIVERY.md)
for prerequisites and the reproducible build commands.

Keep the previously used package and a verified backup when testing an existing
world. Profiles can refer to absolute source paths. Do not move or delete the
source folder while that world depends on it. **Your worlds → Make backup**
supports the local procedural and prepared-source recovery cases described in
[Recovery bundles](RECOVERY_BUNDLES.md). Recover into a new world for a destructive
experiment, and retain the original and backup.

Record the release, operating system, graphics adapter/driver, input device,
window size and text scale before a session. Use **Settings → Controls** to
inspect or change bindings. The default actions below are examples: contextual
Interact hints use the configured key. The reference window is 1440 × 900;
1100 × 700 and text up to 150% are supported review cases. Scroll to controls
below the visible area; keyboard focus should bring them into view.

## Optional local playtest reports

In the main menu, open **Settings**, scroll to **Playtest diagnostics**, and set
**Local playtest reports → On**. Choose **Save defaults** to retain that choice
when the menu is reopened. The choice applies to worlds and host connections
subsequently launched from that menu. It does not change an already running
session. Diagnostics are Off by default, including when older preferences have
no diagnostic setting. Changing graphics defaults in a direct-play session
preserves the saved diagnostic choice.

Version 0.17 added an optional diagnostic field to launcher preferences, separate
from world saves. It reads older preference files with diagnostics Off. If you
return to a pre-0.17 executable with the newer preference file, that older reader
can reject the unfamiliar field and use default graphics with a warning. Keep
the previous preferences with your upgrade backup if retaining those defaults
across a downgrade matters; this does not change the world-save schema.

Capture is attempted as the world opens, then every 10 real seconds and on exit.
The file retains at most 16 samples and is bounded to 2 MiB. Slow writing can
skip a periodic sample; it cannot enqueue unlimited work. Reports contain
timings, version/source/model identifiers, population counts, asset readiness,
pause/input state and checkpoint status. They contain no raw game log, player
inventory, balances, private resume capabilities or full saved world. Error
strings can still contain local paths or source information. Review the file
before sharing it. Nothing is uploaded automatically.

The menu displays the exact report path. Its usual location is:

| Launch method | Report location |
|---|---|
| Linux desktop launcher | `${XDG_DATA_HOME:-$HOME/.local/share}/terra-world/diagnostics/latest.json` |
| Windows desktop launcher | `%LOCALAPPDATA%\TerraWorld\diagnostics\latest.json` |
| Explicit `--user-data FOLDER` | `FOLDER/diagnostics/latest.json` |
| Menu opened from the source root without a user-data argument | `saves/diagnostics/latest.json` under that root |

Use your file manager's address field to open the directory shown in Settings,
then copy `latest.json` into a separately named issue folder. The menu does not
open a file manager or copy a path to the clipboard. Copy the report promptly:
later sessions use the same bounded destination. Turning diagnostics Off leaves
the previous file in place. The report is evidence, not a backup of progress.

**Escape → Session** and **F12 → Performance** show whether capture has actually
succeeded, how old the latest sample is, whether writing is pending and any
export error. **Settings → Refresh report status** rereads the retained file
when you return to the menu. A running sample does not prove a later clean exit.
An old, missing or delayed sample cannot establish current health. An exit sample
describes the state observed at capture; an independent process/exit record is
needed to establish how the process finally terminated.

Only one process can write a particular report at a time. A second process must
use a different user-data directory or explicitly different `--telemetry` path.
The OS releases the diagnostic lock when its owning process exits or crashes;
the small persistent lock file is not itself a running-process indicator. Do not
delete it while a writer might be active. An unrelated or unsafe destination is
refused. A failure before report publication preserves the earlier destination;
a failure after publication retains the new visible complete bytes and reports
uncertain durability. Diagnostic failure and world-save failure are separate
outcomes; inspect checkpoint status before drawing a conclusion about progress.

## First session: observe, act, save, return

For an uncoached usability session, give the player a goal such as “earn enough
to start a small settlement” and observe before showing the following route.
Record hesitation and the player's interpretation in their own words. The
route below is an assisted verification script; completing it does not prove
uncoached clarity or career balance.

1. Choose **New world**. Change the character's name and appearance. Keep the
   bundled synthetic source for the first run, select a region size, and review
   the named source and storage before creating the world.
2. Wait for terrain readiness and control to become available. Identify the
   active control mode, owned actor, source label, wallet and needs. Open
   **Activities [J]**, choose a goal directly, and read its next action.
3. Walk in third person, switch to first person, then return to RTS. Select an
   owned actor and issue a move. Check that context, camera and footer agree.
   Open and close a panel while possessed; it should remain usable.
4. Review a funded supply request. Check the required goods, deadline, gross
   reward and current missing-input cost before accepting. Complete or explicitly
   return it through ordinary play. Insufficient stock, reach, funds or access
   must explain the failure and preserve value.
5. Open **Your commitments**. Confirm accepted work, personal funds, vehicle
   assignments, company obligations and rental bonds are represented only where
   applicable. The briefing is derived from existing world authority.
6. In **Driving [V]** or **Garage [G]**, review an affordable vehicle and its
   rental/bond terms. Enter it, check pedals/steering/braking and change the view.
   Complete an accepted transport stop with the configured Interact action.
   Browsing another job must not redirect that contextual action.
7. If returning a rental, read the vehicle, garage and refund quote. Confirm only
   after it is displayed. A changed quote, vehicle, owner, garage or restored
   state requires a fresh review. Rapid repeated input must not duplicate value.
8. Open the session menu. In local play, simulation presentation pauses; the
   saved calendar pause setting remains distinct. In multiplayer, the host keeps
   running and the menu suppresses local input. Check the diagnostic state if
   enabled. Resume should not reuse the menu activation key as movement.
9. Request a checkpoint, wait for the successful generation, then use **Save
   and return to main menu**. In **Your worlds**, reopen the same named world. Verify the
   character, chosen appearance, funds, inventory, obligations and relevant
   vehicle state. Unsaved changes after a deliberate restore are expected to
   be discarded; do not confuse restore with retry.
10. After a useful milestone, make a backup. Verify it and recover under a
    distinct name into a separate world. Reopen that copy and compare the
    committed state while retaining the original and the backup.

## Focused 0.22 reliability and delivery scenarios

Use disposable managed worlds with verified checkpoints and preserve the
baseline package and recovery source. These are repeatable scenarios and
expected outcomes, not a record that a reviewer has run them on every platform.
Retain the actual process/OS, exact messages, paths, checkpoint generation and
before/after verification results. Never share a private resume identity as an
issue attachment.

### Two managed processes, clean closure and interrupted closure

1. Disable optional local reports, or give each process a distinct report path,
   so the separate diagnostics lock does not obscure the world-lock exercise.
   Open an already saved managed world, request a checkpoint, wait for completion
   and pause the first local session.
2. Start a second copy of the same package using the same user-data directory.
   Attempt to open the same world. It should promptly explain that the world is
   already open and leave the original progress/profile/witness unchanged. The
   first process should remain usable. A different managed world should open
   independently in the second process.
3. Use **Save and return to main menu** in the first process, wait until it has
   returned, and open the original world in the second. Confirm the saved state
   and final presentation settings. The persistent `world-session.lock` should
   still exist; its presence alone must not prevent reopening.
4. Repeat from a confirmed checkpoint. Record which process owns that world,
   then terminate only that disposable session through the operating system's
   process manager. Reopen it after the process is gone. The OS-held lease
   should be available again, and checkpoint validation should determine the
   resumed state. Later unconfirmed gameplay is not promised to survive.
5. Keep first-launch continuity separate: if a disposable world's first process
   is interrupted before it ever produces a valid checkpoint, reopening must
   refuse fresh initialization despite the released lease. Retain
   `world-started.json` and recover a verified backup or create another world.
   Do not delete either marker to force admission.

Retain process exit results as well as the screen messages. A momentary pause,
an existing marker or a stale diagnostic sample is not evidence of ownership.
Run the same matrix on the actual Windows target before recording Windows
qualification; successful Linux execution does not establish it.

### Published output with uncertain durability

A genuine parent-directory synchronization failure happens after file bytes may
already be visible. It cannot be reproduced reliably by merely making a path
unwritable, which commonly fails earlier. Use the explicit host synchronization
boundary or the supplied fault regressions on disposable data; there is no
ordinary player setting that fabricates this error. Keep injected evidence
separate from a real target-filesystem or power-loss rehearsal.

| Injected stage | Expected observation |
|---|---|
| Before final publication | Error reports no publication; any prior destination remains. A new world must not return a profile path if its new containing-directory entry could not be confirmed. |
| After snapshot/profile publication | Error explicitly says the output was published but durability could not be confirmed. Complete new bytes remain readable; no rollback or successful-save acknowledgement is claimed. |
| Native final JSON save | The final-save path reports an error exit and retains dirty state. A readable new file alone does not make that exit a confirmed successful save. |
| Private identity publication during connection | The client does not admit connected state on the failed acknowledgement. Retained identity bytes can be validated for a subsequent confirmed resume; messages must not expose their bearer capability. |

From the matching source checkout and toolchain, the automated companions
execute the actual native final-save system and connection callback, with real
file publication before the injected error:

```sh
cargo test -p terra-demo published_but_uncertain_final_save_emits_error_exit_and_keeps_readable_progress -- --nocapture
cargo test -p terra-bevy --lib welcome_rejects_uncertain_identity_publication_before_admitting_connected_state -- --nocapture
```

Those test results are not screenshots of a production GUI fault. For a manual
fault-adapter run, record the displayed error, process outcome and independently
validated visible file. Preserve the source and retained output before retrying;
an exclusive-create destination must reject a blind overwrite. A reviewed retry
may confirm a replaceable save, but it does not retroactively confirm the first
attempt.

### Recovery completion markers and retained artifacts

Make and verify a normal backup first, then repeat on disposable copies with
faults at the actual named completion boundary. Keep the source backup unchanged
and record the exact retained destination. The automated companion
`uncertain_nested_recovery_publication_retains_verifiable_outputs_without_success`
exercises backend and managed-wrapper stages; new-parent synchronization and
unpublished-backend regressions cover failures before a completion marker.

| Boundary | Expected retained state and product result |
|---|---|
| Before a backend final marker | The operation may remove its owned incomplete tree. It must not offer a completed backup or recovered world. |
| Backend `recovery-bundle.json` publication | A complete backend is retained and independently verifiable even when its directory acknowledgement fails. Without `world-backup.json`, it is not a completed native backup wrapper. |
| Native `world-backup.json` publication | The complete wrapper and backend remain visible on uncertainty, but that operation reports failure rather than a successful backup receipt. Review and verify the retained copy. |
| Backend `restore-receipt.json` publication | The backend restored tree remains; a missing native `world.json` must not be invented or treated as a completed managed world. |
| Native restored `world.json` publication | The complete recovered profile and payload remain on uncertainty. The failing operation does not report successful recovery or automatically open the world. Verify the retained copy before separately choosing to use it. |

Also exercise a newly created backup directory and a fresh recovery user-data
root. Failure to synchronize a newly introduced parent entry must prevent a
success result. Cleanup may remove only this attempt's empty new parents or
owned incomplete tree; existing worlds, backup sources and completed retained
artifacts remain. See [Recovery bundles](RECOVERY_BUNDLES.md) for backend versus
native-wrapper verification, supported sources and identity checks.

### Extracted package: passive inspection and explicit execution

Use the complete extracted desktop package and Python 3.11 or later. From its
root, first run the passive check:

```sh
python scripts/verify_desktop.py
```

Use `python3` where that is the installed command. Confirm the report names the
package, binary and evidence association and reports successful inspection.
Passive inspection must not launch Terra. On a separate copy, changing one
payload byte or adding an unlisted file should produce a failure; preserve the
untouched package for the next step.

On a matching OS/architecture, explicitly request the bounded command-line
execution check:

```sh
python scripts/verify_desktop.py --smoke-help --timeout-seconds 20
```

This starts the packaged executable only for its help output. Capture its exit
status and verifier result. A mismatched host, timeout, failed process or
unexpected output must not be counted as successful execution. The smoke check
does not create/play/save a world, and neither mode proves publisher authenticity
or physical GPU compatibility. Continue with the full native journey above on
the actual target machine. See [Desktop delivery](DESKTOP_DELIVERY.md) for exact
package inspection and qualification boundaries.

## Targeted 0.21 world checks

Use disposable worlds or recovery copies. The custom merchant and polar cases
below need reviewer-provided scenarios with declared source coverage; they do
not imply that those scenarios ship as geographic presets. Keep the ordinary
source label visible and record the scenario revision.

### Different merchant assortments

In a scenario with distinct shop assortments, identify a shortage the central
request issuer can price and one it cannot. Leave enough issuer funds and board
capacity for the supported request, then let normal world time advance through
maintenance. Observe Activities and the relevant Market pages: the world should
remain interactive, the supported funded work should remain available, and the
unsupported shortage should not produce a guessed reward. Review and complete
one supported request through ordinary play, then save and reopen. Retain the
visible offer, seller quotes, world time and observed player outcome. A shop's
own assortment does not guarantee that the central issuer can fund its supply
request; empty-board feedback must be assessed with that constraint in mind.

### Polar loading and supported ground

Use a dry, walkable prepared fixture whose radius-one neighborhood contains six
tiles at a clipped northern or southern row. Check the native loading screen,
then possession and a short walk near the spawn. Source and nearby-terrain
progress should complete for the actual neighborhood, and the local population
and transport controls should become available once their other admission
conditions are met. Retain coordinates, tile level, source coverage, loading
message and a screenshot after entry. Repeat with a genuinely unsupported or
uncovered spawn only on a disposable fixture: the absence of safe ground must
remain visible rather than silently authoring a settlement there. A spectator
view or unrelated loaded pages must not conceal an incomplete player spawn.

### Local edits in a larger resident region

Load a supported larger region and let its initial builds settle. Keep the
camera and quality setting steady while depleting one harvestable source tree,
with the usual reach, equipment and access requirements satisfied. Inspect the
changed feature and nearby unaffected scenery, then allow source-resource
regrowth or restore a checkpoint from before depletion. Once work settles, visible
geometry should agree with the current resource/edit state. If the prepared
source has one feature represented in adjacent chunks, inspect both fragments.
Change graphics quality afterward and verify the edited state survives that
broader rebuild.

Retain the before/after views, actual action, source identity and any F12 build
or failure counts visible during the transition. Short mesh jobs can finish
between diagnostic samples; an unchanged periodic sample does not prove that
no work ran. Exact work-count and concurrent-completion assertions belong to the
automated regression evidence. These visual checks assess continuity and stale
geometry; they do not establish an FPS gain or a hardware performance budget.

## Focused follow-up matrix

| Perspective | Exercise | Evidence to retain |
|---|---|---|
| Goals and economy | Continue after an accepted request, an expiry and a returned request; compare competing costs before a purchase. | Chosen goal, observed blocker, before/after authoritative values and player explanation. |
| Driving | Rebind Interact, browse a different offer while assigned, service the actual stop, review/return a rental. | Visible hint and quote, assignment state, final accepted or rejected action. |
| Autonomous movement | Observe occupied exits, narrow roads and a temporarily blocked route. | Source/route conditions, wait duration, whether physical movement resumes, conserved cargo and obligations. |
| Presentation/content | Run the default pack, then a validated original custom pack; inspect loading/fallback, first person and pause. | Pack identity, ready/rejected/fallback counts, screenshot, actual movement versus clip state. |
| Accessibility | Repeat key tasks at 1100 × 700 and 150% text with keyboard focus; test intended physical devices separately. | Reachability, focus order, clipping and exact device/OS details. |
| Persistence/recovery | Save/reopen, backup/recover, then controlled storage faults on disposable copies only. | Last committed generation, exact error and reopen result; keep the original database and dependencies. |
| Larger working set | Select the larger supported region, travel, revisit and save. | Sample ages, bounded resident/work counts, frame/stage times, storage pending/blocked state. |
| Multiplayer | Use a compatible trusted host; join, disconnect, resume the same identity and inspect local menu behavior. | Host/client versions, encryption state, received-view limits and connection outcome. Never share resume keys. |

The included regressions cover declared finite route/economy/recovery cases.
They do not establish arbitrary dense traffic, hardware performance, public WAN
capacity or infinite worlds. Source coverage is explicit; travel into missing
coverage is not a successful seamless-world test. Intentional procedural fallback
at an instance/distance budget is different from a rejected or failed model.

## When a session goes wrong

A **working checkpoint** needs time to finish; pending is not a failure. A
recoverable storage error keeps authoritative progress blocked and exposes
retry/restore guidance. Retry reattempts saving; Restore discards work after the
last successful checkpoint. Keep the source and database intact.

If the message says the **storage worker stopped** and requires reopening, this
session cannot establish whether its final submitted operation committed. Do not
keep pressing Save or Restore. Close Terra through the displayed recovery exit
path, then reopen the same world so the database can validate its committed
generation. A database commit with a lost acknowledgement may be present on
reopen even when the old session could not report success. Recovery must inspect
that state, not blindly repeat the transaction.

For a hang, record the time, visible screen and last action before ending the
process. A previously written running sample is useful but cannot certify what
happened afterward. Preserve a copy of the diagnostic file and use a disposable
recovery copy for reproduction. Reports, screenshots and a concise step sequence
usually suffice; private session identities and a complete personal save should
not be part of the default issue attachment.

## Issue template

```text
Release / package:
OS / GPU / driver / input device:
Window size / text scale:
World source and preset / region size:
New world, resumed world, or recovered copy:
Expected behavior:
Exact reproduction steps:
Observed behavior and exact visible message:
Time and time zone / last committed checkpoint:
Can it be reproduced in a fresh bundled world?
Diagnostic capture time/session/sequence (or Off / unavailable):
Attachments: reviewed latest.json copy, screenshot, minimal relevant logs
Impact: progress loss / cannot continue / incorrect value / confusing / cosmetic
```

The [0.22 implementation plan](IMPLEMENTATION_PLAN_v0.22.md) records this
iteration's selected corrections and retained limits; [Validation](VALIDATION.md)
records executed acceptance. Keep measured results, assisted review and uncoached
feedback separate so the next iteration can target the actual problem.
