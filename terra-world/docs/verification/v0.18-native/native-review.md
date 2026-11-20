# Fresh native review — 0.18

Executed 29 October 2026 against the final executable SHA-256
`e16c25fabf30d8e96ff6b68cea99766ea1f4cc4d67afc1fbaeaff4433ffa8881`.
The native session began before the canonical gates completed. Their eventual
accepted executable hash matches this session; post-gate execution is not claimed.

## Actual journey and observed results

A new private user-data directory started without saved worlds. The reviewer
opened Controls through the main menu and scrolled normally to the gamepad help.
The added sentence explains releasing sticks, triggers and world buttons after
focus loss/controller changes. It wraps visibly at 1440 × 900; Back, Save controls
and the keyboard footer remain visible. The surface explicitly reports zero
controllers, and the process log reports the sandbox's unavailable udev monitor.
No physical controller behavior is inferred from this visual observation.

The ordinary character → world settings → review flow created **Native Playtest
018** with regional database storage, default Living frontier source and region
coverage. The chosen avatar's actual displayed/persisted name is **AlexaNative
Eighteen**. Initial automated Ctrl+A attempts appended text instead of replacing
it; the world title was corrected with ordinary Backspace/type before creation.
No exact character-name replacement or comprehensive text-editing claim is made.

The preparing surface progressed to a running RTS world with **49 active chunks,
107 bodies and 64 wildlife** in the ready capture. A third-person control switch
and held movement input were sent. The actual world captures show the actor near
the starting junction; no precise travel distance is claimed. After explicit
focus and a deliberate Escape press, the actual session menu opened at **1100 ×
700**. The named world, checkpoint **32**, all seven session actions including
Save and quit, and keyboard footer are visible without clipping.

**Save and return to main menu** returned to a menu offering Continue Native
Playtest 018. The committed database was read only after that return. Continue
reopened the same world through the menu; it completed preparation and paused
with checkpoint **49** displayed. **Save and quit** then closed the supervised
native session with **exit 0**, no forced termination and an unchanged executable.

The bounded read-only committed-state comparison checks title, character choices,
player ID, money, avatar name/owner and inventory hash. All match after reopen;
checkpoint generation advances **34 → 52** and SQLite quick_check returns **ok**.
This is not whole-world equality: simulation time, environment and needs can
advance. Private profiles, databases and player state remain outside delivery.

## Evidence and interpretation

- [Native receipt](native-receipt.json) retains exact command, executable hash,
  supervisor PID, action outcomes, capture hashes/geometries and actual exit.
- [Save/reopen comparison](save-reopen-comparison.json) records only the bounded
  committed summary and explicit matching fields.
- `04-controls-recovery-help.png` is the fresh recovery-help inspection.
- `15-escape-retry.png` is the actual paused 1100 × 700 acceptance capture.
- `16-returned-menu.png` shows the named Continue action after save/return.
- `18-reopened-paused.png` shows the reopened world paused at 1440 × 900.

Capture filenames are original attempted action names, not evidence that those
states occurred. The receipt's `observed_role` identifies each actual image:
02 remains the home menu; 05 remains Controls; 12 and 13 are running third-person
views; 14 is a resized running view. Only 15 and 18 establish visible pause menus.
Initial input attempts before explicit focus and fast taps did not reliably
activate the requested action; later reviewed captures establish the actual path.

## Acceptance boundary

This is a fresh, expert-assisted Linux/X11/Xvfb session using llvmpipe software
Vulkan. It establishes the bounded menu/create/save/reopen/clean-exit journey and
the inspected layouts, not Windows execution, physical controllers, assistive
access, target GPU frame times, uncoached comprehension, completed careers,
worker-poison injection, diagnostics, WAN capacity or long-soak reliability.
The diagnostic choice remained Off. No new high-impact visual blocker was
observed within these states; the wider product gaps remain explicit.
