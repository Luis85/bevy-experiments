# Native controls and input configuration

The native main menu and the in-game session menu both expose **Controls**.
Changes apply immediately and follow the native menu → world → menu process
handoff, including unsaved choices. **Save controls** is fixed below the scroll
area and writes a bounded, validated `controls.json` in the selected user-data
directory for future application launches. World checkpoints never overwrite
input preferences. Old installations without the file use the defaults below.

## Keyboard and mouse

| Action | Default | Configurable here |
|---|---|---|
| Forward / accelerator; backward / reverse | W; S | Yes |
| Move / steer left; right | A; D | Yes |
| Sprint on foot / service brake in a vehicle | Left Shift | Yes |
| Handbrake | Space | Yes |
| Rise / descend for flight and free camera | R; F | Yes |
| Interact / service transport stop | E | Yes |
| Switch first / third person | C | Yes |
| Parking brake; headlights | P; L | Yes |
| Left / right indicators; hazards | Z; B; H | Yes |
| Look | Right mouse drag | Sensitivity and invert-Y |
| RTS / third person / first person | F1 / F2 / F3 | Fixed |
| Select owned crew / stop selected owned actors | Ctrl+A / X | Fixed |
| Focus the selected actor | Tab | Fixed outside management focus |
| Activities / nature / territory / fleet | J / N / T / Y | Fixed |
| Driving / garage / atlas | V / G / M | Fixed |
| World / inspector / diagnostics | F10 / F11 / F12 | Fixed |
| Save / restore | F5 / F9 | Fixed |
| Menu / cancel | Escape | Fixed |

Choose a binding button, then press a key. Duplicate bindings are rejected
without changing either action. Escape cancels capture. Menu shortcuts and
activation keys remain reserved. Assignable keys include the movement/action
letters, arrows, number row, keypad and punctuation exposed by the editor.
Names in RTS, driving and walking guidance follow the chosen keys. Leaving the
Controls screen cancels any unfinished capture. F5/F9 cannot save or restore
while a key is being captured. Space pauses the RTS clock only when it is not
assigned to another RTS action; a possessed vehicle keeps its handbrake behavior.
The World panel always offers the clock toggle.

The camera accepts mouse sensitivity from 0.25× to 3×; the UI cycles useful
presets. Vertical look inversion applies to mouse and gamepad. **Interface text**
cycles 100%, 115%, 130% and 150%; new panels inherit the selection without
repeatedly scaling existing fonts. Launcher, session and on-screen keyboard labels
wrap at word boundaries or within long unbroken paths and error messages.
First-person field of view offers 55°, 65°, 78°, 90° and 100° vertical. Driving
camera auto-center can be disabled for
players who prefer manual look. Mouse/keyboard conventions remain available;
the controller management and RTS paths are described below.

## Gamepad

The native application enables Bevy's gilrs backend. It chooses one connected
ordinary controller deterministically, excluding the device selected as a
wheel. The controls screen reports detected devices and active names. Disable
**Gamepad** to disable ordinary controller movement and menu navigation; the
dedicated wheel profile has its own enable control.

### Menus and character creation

| Input | In a menu or text keyboard |
|---|---|
| D-pad / left stick | Move focus between enabled menu controls, or move the character selection in an open text keyboard; held input repeats at a bounded rate |
| A / South | Activate the focused button; open the keyboard for a focused text field; select a character |
| B / East | Back; discard an open text draft |
| X / West | Delete the final character from an open draft |
| Y / North | Toggle capitals in the text keyboard |
| Start | Apply an open text draft |
| F4 | Open the controller text keyboard for a focused text field |
| Tab / Shift+Tab in the text keyboard | Move among the draft preview, Delete, Clear, Capitals, Discard and Apply; focus stays inside the keyboard |
| Enter / Space in the text keyboard | Enter on the draft applies it; Enter/Space activates the focused editing action |

The controller keyboard edits a private draft and applies it through the same
bounded native editor as physical typing. Numeric and coordinate fields filter
characters. Back discards the draft, and a concurrent external field edit is
reported instead of silently overwritten. The full launcher, character
creation, world settings, Controls and session-menu paths support controller
focus navigation. Physical typing also edits an open keyboard draft; arrows move
its character selection and Escape discards. Space enters a character when the
draft is focused; on an editing action it activates that action without adding
a space. The focused draft/action has an accent outline that persists as the
keyboard refreshes. Apply or Discard restores focus to the native field that
opened it. Keyboard access remains available when ordinary gamepads are disabled.

In 0.21, **Preparing your world** also supports controller focus and activation.
D-pad/stick and South reach **Save and return to main menu** for a local world,
**Cancel connection · Main menu** for a joining client, or **Close failed world**
after a terminal storage-worker failure. Loading continues to block world
movement and commands. Saving waits for acknowledgement and does not expose
controller menu actions.

### In-game management and text

Press **F6** or **right-stick click** to enter management focus. The inspector
opens, the current action gains an accent outline, and world movement/orders
are blocked until focus closes. Arrow keys, Tab/Shift+Tab, D-pad or left stick
move through visible enabled actions. Enter or South activates the real button.
PageUp/PageDown or LB/RB changes inspector pages. The selected action scrolls
into view. Escape/East closes focus; that same input cannot pause, release or
steer an actor behind the interface.

Name and coordinate actions open a private text draft while in management
focus. Type normally, choose characters with the controller, or use the visible
keyboard buttons. Enter on the draft or Start applies through the existing
command queue; Enter/Space on an editing action chooses that action.
Escape/East discards. F4 opens the same keyboard during an ordinary in-world
text edit. Invalid input is still rejected by the domain action. A changed
underlying field or full command queue preserves the draft and explains retry.

### RTS terrain cursor and construction

| Input | In command mode with an active controller cursor |
|---|---|
| Left stick | Move the visible terrain cursor within the usable world view |
| A / South | Select; hold and move to drag-select; confirm a building/crop placement |
| RB held | Add to the existing selection |
| X / West | Move selected owned units to the indicated terrain |
| B / East | Stop selected units; cancel an active placement |
| Y / North | Possess the selected owned actor in third person |
| LB | Cycle eligible actors |
| Right stick | Pan the RTS camera |
| RT / LT | Zoom the RTS camera |
| Right-stick click | Open management focus |

Mouse activity returns to the ordinary mouse path. Disconnect/focus changes
cancel incomplete drag intent; the cursor never synthesizes operating-system
mouse events. Selection, move orders and construction use the same ownership,
reach, terrain and command validation as the mouse. Political atlas polygon
editing remains a mouse/keyboard workflow. Physical controller/wheel hardware,
force feedback, screen readers and localization have not been certified.

### Movement and driving

| Input | In the world |
|---|---|
| Left stick | Walk / steer; radial deadzone with normalized diagonals |
| Right stick | Look; saved sensitivity and invert-Y |
| RT | Accelerator |
| LT | Service brake |
| X / West held with RT | Reverse demand; the shared dynamics solver brakes before changing direction |
| A / South | Interact / transport stop service |
| B / East | Held handbrake |
| Y / North | Switch first / third person |
| RB / LB | Parking brake / lights in a vehicle; ascend / descend in aircraft |
| D-pad left / right / down | Left indicator / right indicator / hazards |
| Left stick click | Sprint |
| Select | Release possession into RTS |
| Start | Session menu |
| Right-stick click | In-game management focus |

## Wheel and joystick driving

The **Wheel / joystick driving** profile maps axes reported by Bevy/Gilrs into
the same authoritative driving intent used by the keyboard and gamepad. It is
disabled by default, and requires a selected connected device plus valid
calibration before it can drive a vehicle.

1. Enable **Wheel / joystick driving**, then use **Wheel device** to select the
   connected device. The editor considers at most 16 connected devices.
2. Choose **Separate axes** for independent pedals, or **Combined axis** for
   hardware where brake and accelerator occupy opposite halves of one axis.
3. Choose each channel's axis while watching its live raw value. Choices include
   the six standard stick axes, two triggers and up to 32 reported additional
   axes. An absent axis remains inactive. Steering and active pedal channels
   must use distinct axes.
4. Calibrate steering by capturing center, full left and full right in that
   order. For each separate pedal, capture released and fully pressed. For
   combined pedals, capture both released, full brake, then full accelerator.
   The control label gives the next step; only a complete valid capture replaces
   the previous range. Cancel preserves the previous valid calibration.
5. Check the live steering, accelerator and brake percentages. Calibration
   detects inverted endpoints; explicit direction controls can invert a channel.
   Deadzone presets span 0–40% and apply after calibration, preserving full
   output at each calibrated endpoint.
6. Release the pedals and center steering to arm the profile. Choose **Save
   controls** to retain the profile for future launches.

The profile supports asymmetric steering ranges and splits a combined pedal
axis into mutually exclusive brake and accelerator demands. A chosen wheel is
excluded from ordinary gamepad walking, look and menu input even while its wheel
profile is disabled. Keyboard or ordinary controller steering and throttle take
priority when actively held; the strongest requested brake remains effective.
When the selected device reports these buttons, West/X held with the accelerator
requests reverse and East/B holds the handbrake. Other vehicle actions retain
their ordinary keyboard/controller bindings.

Focus loss, device disconnection, a missing or nonfinite axis, and configuration
changes release wheel intent. Reconnection or focus return requires released
pedals and centered steering before the wheel can drive again. This prevents a
held accelerator from resuming unexpectedly. Keyboard controls remain available.

The saved identity uses the device name, available vendor/product identifiers
and its ordinal among matching connected devices. Names are bounded to 64
characters; extra-axis identifiers are bounded to 65535. Identical devices do
not expose a stable serial through this adapter, so verify the selected device
after changing their connection order. Saved calibration validates finite
endpoints, distinct channels, a usable range and deadzone bounds. Runtime raw
values, connected entities, arming state and incomplete captures are never saved
or sent through the bounded 4 KiB menu/world settings handoff. Older preferences
load with wheel driving disabled.

This adapter supports devices and axes exposed by the configured Bevy/Gilrs
backend. It does not add a raw HID driver, gear-shifter binding, force feedback
or physical-device certification. Automated virtual-device tests exercise
separate and combined pedals, asymmetric and inverted ranges, invalid
calibration, serialization, disabled input, focus loss and disconnects; actual
hardware and operating-system compatibility still need device testing.

## Input safety and lifecycle

### Actor context and command targets (0.22)

Activities and immersive play keep a compact actor summary in the footer.
Command shows the same selection in detail. Empty selection is stated directly;
an actor absent from the current view is unavailable, not replaced by another
body's identity. A single foreign actor is **Inspect only**. Remote summaries
describe received state and label a partial view; they are not whole-world crew
counts.

For multiple selection, **Lowest** means the minimum Food, Water and Energy
values among selected owned biological bodies. The three minima can come from
different bodies. Vehicles and non-owned bodies are excluded; missing bodies
are named and values become **Lowest shown**. A vehicle displays **Driver** needs
only when authority confirms that exact owned vehicle is currently possessed and
the owned biological avatar is available. Selecting another vehicle does not
borrow that driver's values.

**Eat bread** and **Drink water** require one selected owned person that is also
the account's active body. Their native activation retains the displayed owner,
restored-world revision and body identity, then rechecks them before submission.
A changed body or queued body transition asks for review again. Group **Stop**
and **Rest** recheck ownership and retain the selected owned IDs at native
dispatch. They do not consume food for a group. Park and exit a vehicle before
feeding or resting its driver.

These are native presentation/submission guards. The existing SDK/network
Consume command still applies to the authority's active body when processed;
the native review is not a wire-level reservation across independent clients.

In **0.23**, Nature's berries, cooked fish and water buttons and Activities'
direct Eat/Drink suggestion retain the same active-person identity. Their text
names that person explicitly; unlike Command, these actions do not require the
currently inspected selection to match. A changed active body or a pending
possession/restore transition requires another review. The active vehicle must
still be parked and exited before supplies can be used by its driver.

Native resource, territory and supply actions also retain the target and context
displayed when activated. A missing explicitly chosen destination does not
silently fall back to another one. Use **Next patch** or **Next territory** to
choose an available replacement, review the resulting label and then act.
Supply Accept/Deliver/Return use the same current-condition readiness as domain
authority; their written blockers explain recovery and never reserve capacity.

Input is rebuilt from current state each frame. Missing devices and an
unfocused desktop window produce released input, never cached throttle. A
possessed vehicle receives protective stopped intent during text editing,
menus, the atlas or input suppression. Local pause freezes the authoritative
world; a remote menu suppresses local intent while the host continues.
A rebind-capture frame is consumed so the newly assigned key cannot also move
or close the current screen. Saved input is validated before use; malformed or
unsupported files produce a visible notice and coherent defaults.

The open text keyboard owns native Tab focus. Focus that leaves its draft/actions
is returned to the draft before native key dispatch. Launcher, session and world
buttons reject parent activation while a draft is open and in the frame that
Apply or Discard closes it, so one input cannot both close the editor and choose
an underlying action. Ordinary parent activation is available again after the
closing input is consumed.

Fixed interface shortcuts remain available even when custom movement bindings
are chosen. F5/F9 keep the world's configured persistence semantics. F9 opens the
review described below; only confirmation restores and discards later changes.

| Session mode | Save controls and window-close behavior |
|---|---|
| Regional database | Automatic checkpoints, F5, session-menu save and clean exit preserve progress. F9 reviews the latest checkpoint. |
| Managed portable JSON | F5, session-menu save and clean exit save a snapshot; there is no periodic autosave. F9 reviews the saved snapshot. |
| Direct-play manual JSON | Use F5, **Save and return to main menu** or **Save and quit**. Closing the window does not save, and there is no periodic autosave. F9 reviews the saved snapshot. |
| Multiplayer client | The host owns progress. The client cannot save or restore the shared world; leaving returns control to the launcher or quits. |

### Standard gamepad handoff safety (0.18)

After focus loss, controller replacement, disable/re-enable or binding capture,
release both sticks, both triggers and world-action buttons before resuming.
The sticks use their configured deadzone; triggers must return to zero because
driving consumes their raw values. Until that neutral sample, standard-pad world
movement, driving and action intent is blocked. The controller status explains
what to release. Menus and independent keyboard controls remain available.
This contract has synthetic-input regression coverage; physical devices and OS
reconnection ordering still require qualification.

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

## Reviewed operations in 0.24

Industry names the selected facility and the next Pause or Resume operation.
Use Next facility to deliberately choose a different loaded facility. An explicit
selection that becomes unavailable remains unavailable.

Fleet assignment, run/pause, recovery and depot transfers retain the targets and
amount shown when activated. If the choice or eligibility changed, review the
written state and choose the operation again. Normal movement, wages and rental
countdowns do not require repeatedly reviewing unchanged choices.

In Build, review the named structure and health consequence before confirming
damage. Cancel preserves the structure; management focus starts on Cancel.
Changed health/ownership/reach makes the review expire. Keyboard management and
scrolling use the existing controls; no new hold requirement is introduced.

