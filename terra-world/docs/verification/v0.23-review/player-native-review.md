# Independent native player review — 0.23

The player specialist inspected the integrator's fresh native candidate and
confirmation captures on 30 October 2026. The pre-correction candidate is the
0.23 desktop binary with SHA256
`700120e44255afe4c482b3416bdcedaad4200d930012418aa0c36cda0281c7f0`.
Capture receipts associate the launcher window with PID 11234 and the first
managed-world session with PID 12147; each image retains its own receipt. The
integrator owns the running application and the complete action and persistence
record; this independent review inspects saved images, receipts and relevant
source. Final confirmation identities are recorded below.

## Candidate observations

| Evidence in `../v0.23-native` | Observed result |
|---|---|
| `10-candidate-menu.png` through `13-candidate-performance.png` | Menu and settings preserve the established palette, readable hierarchy and visible focus. No blocking geometry defect was observed at 1440×900. Performance exposes one contradictory effective-state label, detailed below. |
| `14-candidate-controls.png` through `18-candidate-controls-saved.png` | The 100%, 115%, 130% and 150% controls views preserve readable labels, visible focus, a scroll affordance and fixed Back/Save controls actions. The final image explicitly confirms that controls were saved. Images alone do not prove all off-screen controls or physical input devices. |
| `19-candidate-settings-return.png` through `21-candidate-defaults-saved.png` | Main-menu and settings content remain legible at 150%. Settings repeats the Performance/sky state mismatch. The content viewport clips at its scrolling boundary while fixed navigation remains visible. |
| `22-candidate-character.png` through `24-candidate-region.png` | Character creation and regional world configuration remain readable at 150%; named fields, preview, source description, regional storage and forward actions fit their intended surfaces. |
| `25-candidate-near-region.png` through `28-candidate-world-review.png` | The selected near-region extent and 1× clock appear in the final review; creation, storage behavior and checkpoint/restore consequences are readable before entering play. |
| `29-candidate-create.png` | The newly created world shows Activities, the named active actor Alex and a written checkpoint result at 150%. The longer panel scrolls. This initial image alone does not establish settled terrain rendering or completion of a gameplay action. |
| `30-candidate-supply.png` through `35-candidate-travel.png` | Terrain is present in the settled frames. The reviewed supply request names Alex, four wood and Riverside sawmill; the acceptance consequence and carried quantity are readable through the scroll body. The accepted state changes the guided action to travel while the displayed wallet remains 250.00 cr. These images do not establish the eventual delivery exchange. |
| `36-candidate-minimum.png` | At 1100×700 with 150% text, activity navigation collapses into Choose activity. The named actor, named destination, travel action, deadline and available goods remain readable within the scroll body. The bottom checkpoint message and core controls remain inside the viewport. |
| `37-candidate-nature-minimum.png` through `39-candidate-consumption-disabled.png` | Nature writes the current distance blocker and the corrective Go to patch action. Supplies name Alex, show carried counts and the 98% needs threshold beside visibly disabled consumption controls. The text remains readable at the minimum window and 150% scale. |
| `40-candidate-delivery-ready.png` and `41-candidate-supply-complete.png` | The guided action reaches Deliver and collect reward. The following image shows 256.13 cr, 40 community XP and one completed contract, matching the preceding 6.13 cr/40 XP promise, then offers a named-neighborhood next step. Exact inventory and escrow conservation remain covered by automated state assertions. |
| `42-candidate-community.png` through `45-candidate-territory.png` | The settlement-name input and submission instructions remain visible in the footer. After naming Review23, the wallet shows 206.13 cr and the new territory shows its name, player owner, 50.00 cr treasury, zero plots and charter requirements. Two minor text inconsistencies are recorded below. |
| `46-candidate-territory-actions.png` through `49-candidate-funded.png` | Scrolling reaches contribution, funding and upgrade controls at the minimum window/150%. Zero owned planks and outstanding requirements explain the dimmed contribution and upgrade actions. The treasury action states 25 credits and the next frame shows the wallet reduced from 206.13 to 181.13 cr. |
| `50-candidate-pause.png` and `51-candidate-save-exit.png` | The pause overlay retains visible focus, a scrolling body, checkpoint identity, restore consequence and accessible save/exit controls at the minimum window/150%. |
| `52-candidate-return-menu.png` through `54-candidate-rename.png` | Returning-player surfaces show Continue with the saved world name; Your worlds identifies the character/source/creation time and exposes Rename and Make backup. Rename retains the current world identity, a focused name field and a written account of its effect. |
| `55-candidate-rename-entered.png` through `61-candidate-menu-final.png` | The entered name appears in the saved-world list and success message. Backup feedback names checkpoint 23; verification presents the world, character, source, checkpoint and separate-world recovery action before mutation. The recovered copy and original are separately visible in Your worlds, and Continue names the recovered copy. All inspected 1440×900/150% surfaces retain readable content and navigation. Reopening and exact saved-state equivalence require the integrator's subsequent acceptance evidence. |

## Candidate findings, corrected in the final batch

**P2 — qualify the effective atmospheric sky state in Performance mode.** In
`13-candidate-performance.png` and `20-candidate-settings-large.png`, the quality
control reads **Performance**, the sky control reads **On**, and nearby help says
Performance disables atmospheric sky. This leaves the player to infer whether
their chosen setting actually applies. It is an observed candidate inconsistency;
this review does not claim it was introduced by 0.23.

Pre-correction source confirmed that `launcher/ui.rs` rendered the raw
`atmospheric_sky` preference on both world configuration and global settings.
`flow.rs` used the same raw preference in pause settings. The main in-world
`interface.rs` label already distinguished **off · Performance** from the
requested preference. Recommended
correction: retain the preference, explicitly name the Performance override in
the affected controls, and keep the effective-state vocabulary consistent.
Acceptance: capture Performance with sky preference both on and off, then a
supported higher preset, and verify the label always describes what applies and
how the retained choice will behave. This is a copy/state clarification; it
does not require changing graphics policy.

Two non-blocking copy inconsistencies are visible in the completed journey:
`41-candidate-supply-complete.png` uses **1 supply contracts completed**, and
`45-candidate-territory.png` exposes **stone_blocks** in upgrade requirements.
The existing contribution selector already renders **stone blocks**. Singular
grammar and applying that same underscore-to-space presentation to the local
inventory formatter are suitable small changes in the consolidated polish
batch. They do not change item identifiers, balances or game rules.

## Consolidated correction and freeze

After inspecting the gameplay and lifecycle captures, the integrator authorized
one consolidated copy correction. The following presentation-only changes were
applied and implementation was frozen:

- `crates/terra-demo/src/launcher/ui.rs`: world configuration and global settings
  now show **Atmospheric sky · Off · Performance** followed by **Sky preference ·
  On/Off** in Performance mode. Balanced/High retain their normal On/Off state.
- `crates/terra-demo/src/flow.rs`: pause settings use the same effective-state
  and retained-preference wording.
- `crates/terra-demo/src/frontier.rs`: one completed supply contract uses
  singular grammar, and the existing inventory formatter presents underscores
  as spaces, including territory upgrade requirements and stored materials.

Action handlers, saved preferences, item identifiers and game rules are
unchanged. No abstraction or tests were added solely for these copy corrections.
The integrator owns formatting, rebuilt-candidate validation and the single
confirmation round. Candidate observations above continue to describe the
pre-correction images rather than treating the proposed wording as visual proof.

## Final confirmation inspected

All ten final confirmation images, **70–76 and 80–82**, were opened and inspected
after the consolidated correction. They use binary SHA256
`432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.
The direct recovered-world process is PID 14073 and the launcher process is
PID 14392. Both completed with exit code 0. Their process receipts record the
same 381 implementation inputs with start/end digest
`dd6d40911a1defed04d39a3a4287ae01ced1cfde27fdcbf0575caa404c39c5d5`
and report those inputs unchanged.

| Final evidence in `../v0.23-native` | Independent observation |
|---|---|
| `70-confirmation-recovered.png`, `71-confirmation-minimum.png` | The reopened recovered world shows 181.13 cr, 50 community XP, the corrected **1 supply contract completed** and active actor Alex. The next building action remains readable at both 1440×900 and 1100×700 with 150% text. |
| `72-confirmation-territory.png` | Review23 retains its player owner and 75.00 cr treasury. Upgrade materials now read **30 planks · 10 stone blocks** and wrap cleanly at the minimum window/150%. |
| `73-confirmation-pause.png` through `76-confirmation-save-quit.png` | The pause surface identifies **My frontier Review23 (recovered)**. Session settings clearly distinguish **Atmospheric sky · Off · Performance** from **Sky preference · On**. Focus, scrolling, restore consequences and Save and quit remain readable and reachable. The separate process receipt establishes clean exit; the final screenshot alone does not. |
| `80-confirmation-menu.png` through `82-confirmation-menu-exit.png` | Continue names the recovered world. Global settings show the same effective sky state and retained preference without the candidate contradiction. Menu navigation remains readable at 150%. The launcher receipt establishes clean exit. |

No material defect was found in this confirmation round. No further
implementation or optional visual iteration was performed.

The review also read the
[recovered-world process record](../v0.23-native/confirmation-recovered-process.json),
[launcher process record](../v0.23-native/confirmation-menu-process.json) and
[persisted-state inspection](../v0.23-native/confirmation-state-inspection.json).
The state record reports exact logical equality between the original and its
checkpoint-23 backup. After resumed play, the recovered world has advanced to
generation 35 and a later clock, while the inspected invariants remain equal:
18113 minor credits, 50 XP, one completed contract, one founded settlement and
Review23 treasury 7500 minor credits. This is preservation of the named
progression and funds; it is not whole-state equality after further simulation.

## Plan coverage and explicit limits

WP09's native journey covers create/continue, Activities, Nature, territory,
successful supply delivery, pause/save/reopen, rename and supported regional
backup/verification/separate recovery. Current baseline, candidate, one
consolidated correction and one confirmation round are retained. The isolated
world, process/source/binary association, minimum-window behavior and visible
focus are supported by actual captures and receipts.

The following planned branches have narrower evidence and must not be described
as executed native UI fixtures:

- **Full receiver/capacity-blocked supply:** domain readiness/conservation and
  native label/disabled-state regression fixtures cover blocked states and
  recovery. The rendered journey demonstrates acceptance, out-of-reach travel
  and successful delivery; it does not reproduce a full receiver in the running
  native window. The native wallet-capacity regression is also a fixture, not a
  screenshot. Therefore the literal planned native capacity-blocked path remains
  visually unexercised even though its source and automated acceptance exist.
- **Busy maintenance:** active-session and maintenance lease refusal are covered
  by process/regression tests. The native backup/rename journey operates on a
  quiescent world; a second simultaneous UI launcher was not used to demonstrate
  the busy message.
- **Sky state matrix:** final captures confirm Performance with retained
  preference On in pause and global settings. The world-creation copy site,
  preference-Off toggle and higher-preset transition were source-reviewed but
  not separately recaptured after the correction. The broader matrix suggested
  in the candidate finding is not claimed as native evidence. The correction
  retains the existing settings handlers and graphics policy.
- **Action races and consumption:** changed-body/target/restore/connection and
  unchanged-context success are tested through actual Activate/queue/dispatch
  fixtures. The native pass shows named supply context, disabled consumption,
  gathering-distance recovery, delivery and treasury funding; it does not turn
  every race or successful consumption fixture into a human-operated journey.

The 238 passing native unit/integration tests recorded in `player-experience.md`
are separate correctness evidence. Canonical release acceptance is still owned
by the integrator and must finish before packaging. Neither these tests nor the
Linux software-rendered native run establish an uncoached human playtest,
physical controller/wheel or GPU qualification, Windows support, 200% text or
assistive-technology compliance.
