# Explorer end-to-end run (game menu and AI explorer, Task 14)

Date: 2026-10-25, Windows 11, branch `claude/single-npc-testbed-48eb8e` at `9fafbce` plus the prompt wording
fixes of this task. Done by an automated agent (Claude Code), which played the explorer itself from the
generated prompt. It could not click, so the menu and the modal's buttons were not used by hand (see
"Still needs a human" at the end).

## Commands

```sh
cargo build -p game_menu -p actor_testbed -p village -p rts_units_demo -p npcctl   # ok, 8 m 14 s
# New (what the menu's Explore -> New -> Start runs), started from PowerShell with Start-Process -PassThru (PID kept):
target/debug/actor_testbed.exe --explorer-home <wt>/target/e2e/agents --agent "E2E Wren" --world "E2E Hollowmere"
# the operator standing in for the dev's OK click (the sim waits behind the modal):
curl -s -X POST http://127.0.0.1:15702/ -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"npc/resume","params":null}'
# the agent: only the commands of the start prompt (then start_prompt.md, now prompts/start.md; observe / act / events), ~10 real minutes
taskkill /PID <pid>                              # WM_CLOSE, no /F: a normal window close
# Resume (what the menu's Resume runs after --check-save), with the F7 panel open:
target/debug/actor_testbed.exe --explorer-home <wt>/target/e2e/agents --agent "E2E Wren" --check-save   # {"ok":true}
target/debug/actor_testbed.exe --explorer-home <wt>/target/e2e/agents --agent "E2E Wren" --resume --panels explorer
curl ... '{"method":"npc/screenshot","params":{"path":"e2e/resume_modal_f7","overwrite":true}}'
target/debug/game_menu.exe --settings target/e2e/settings.json --screen explorer_resume \
  --screenshot target/npc-out/e2e/menu_resume.png --after-secs 4
```

## What happened

**New.** The game logged `explorer 'E2E Wren' in 'E2E Hollowmere' (seed 1792825153941685892): new world`, picked
port 15702, and wrote `e2e-wren/{agent.json, diary.md ("# Diary of E2E Wren"), proposals/, e2e-hollowmere.world.json
(tick 0), start_prompt.md}` (the start prompt is now `prompts/start.md`, next to the rest of the prepared folder). The prompt named actor #62, controller `explorer`, the absolute forward-slash
`npcctl.exe` path, the URL and the diary/proposal paths; every placeholder was filled.

Before unpausing I observed as a fresh agent would: tick 0, "You perceive nothing right now", and an `events`
long poll timed out with nothing. Nothing in the prompt said the world could be paused (fixed, see below). After
`npc/resume` (02:44:26 UTC, tick 0) the world ran at 20 ticks/s.

**Play (02:44:26 to 02:54:42 UTC, ticks 0 to ~11 000).** Every action was accepted on the first try with the
prompt's command forms (`--interact verb@ID`, `--affordance`, the `--steps` Move + `execute_orders`, `events
--until-idle --dedupe`):

| Tick | What the agent did |
|---|---|
| 231-268 | picked up the bread #27 in the yard (path, reserve, `Interacted` success) |
| 441-551 | picked up the store key #26 |
| 979-1068 | unlocked the store door #12 (walked there from memory; `DoorChanged locked:false`) |
| 1210-1225 | opened it: hammer #18 inside |
| 1390-2073 | Move to (30 m, 0): A* went through the compound gate #10 (opened by the path) into the east grove |
| 2308-2402 | picked up bread #57 (it merged into the stack #27) |
| 2606-3168 | back into the yard, picked up the axe #25 |
| 3380-3512 | equipped it, chopped tree #23: Woodcutting roll 7 vs 8, success, 13 XP |
| 4054-4073 | picked up 2 logs #63 (only found via the affordance list: see proposal 001) |
| 4433-5491 | Move to (0, 40 m): a six-leg path around the (unseen) ruin |
| 6939-7251 | back to (0, 24 m), then into the ruin at (0, 28 m): scroll #55 seen |
| 7393-7604 | studied the scroll: learned Smithing 9 |
| 8340-8787 | Move to (25, 22 m): the three ore rocks #46-#48 (need a pickaxe; Mining 5, ~4%) |
| 8933-9069 | picked up log #60 |
| 9536-10519 | Move into the store (6.5, 6 m): pickaxe #17 and scroll #19 seen |
| 10721-10757 | picked up the pickaxe #17 |

The agent wrote **6 diary entries** in the prompt's `## tick T - title` form (ticks 552, 2083, 3720, 7828, 9080,
10529) and **3 proposals** in the header format (`001-name-new-items.md` "Say where a harvest went",
`002-paused-world.md` "Tell me when the world is paused", `003-feel-the-walls.md` "Let me sense walls and
openings"). Autosaves came every 60 s of wall clock (ticks 441, 1489, 2585, ... 8953, 10000 in the log).

**Quit.** Before closing, `npc/unit 62` gave pos **(5660, 6630)** with the pickaxe in the inventory; I paused
the game over BRP to read a stable tick (11 033) and closed the window with `taskkill /PID 49508` (no `/F`). The
game logged "No windows are open, exiting" and exited, but **no "save on exit" line**: the save on disk stayed
the 02:53:51 autosave, **tick 10 000**, where the actor was mid-walk at **(16275, -6059)** without the pickaxe.
A probe run (a new agent, unpause, close after 8 s) reproduced it: its save stayed at tick 0. See defect D1.

**Resume.** `--check-save` printed `{"ok":true}`; the menu's Resume screen listed "E2E Wren - E2E Hollowmere -
last saved 2026-10-25 02:59 UTC" (`target/npc-out/e2e/menu_resume.png`). The resumed game logged `resumed`,
wrote `resume_prompt.md` (now `prompts/resume.md`; the plain Resume variant: "read your diary ... and every file in ... proposals ...
first"; the body after line 1 is identical to the start prompt). The first observation, still paused:

```
Tick 10000 (day 4, 16.6% of the day, day time), seq 0.
You carry: the axe #25 (in main_hand), the store_key #26, 4 × bread #27, 3 × log #63.
Position: (16.27 m, -6.05 m).
Doing: #20 Carry out your orders (step 1/1).
Skills: Smithing 9 (IQ-1), Stealth 13 (DX+1).
```

So the actor stood **exactly where the save had it** (16275, -6059 = 16.27 m, -6.05 m), mid-order, with the
saved inventory, skills, XP and the store door still open. After unpausing, the perception events came at the
**same ticks as in the first run** (10054 #32, 10074 #30), until the agent's new command replaced the walk at
10079: the resumed world continued the uninterrupted one bit for bit. The agent read its diary and proposals,
noticed the missing pickaxe ("the save was older than my last memories"), walked into the store again and
picked up the pickaxe at tick 10536 (it had been 10757 in the lost minute), and wrote a 7th diary entry. The
second close lost the last 14 s the same way (last autosave tick 10 964; no save on exit).

**Screenshots** (Read and checked):

- `target/npc-out/e2e/resume_modal_f7.png`: the resumed window, paused, with the "Resume prompt" modal (scrolling
  prompt, notes with the port and file, Copy, OK) and the F7 panel on the right showing the diary entries
  newest first.
- `target/npc-out/e2e/resume_running_f7.png`: a few seconds after the agent appended its 7th entry: the panel
  shows it at the top (picked up from disk within ~2 s); the overlay reads `speed 1x`.
- `target/npc-out/e2e/f7_proposals.png`: a copy of the folder with a one-entry diary; the panel is short, but
  its proposals lines sit under the modal, so the three `[new]` titles are not visible in any screenshot.
- `target/npc-out/e2e/menu_resume.png`: the menu's Resume list with this agent.

## Prompt problems found

| # | Severity | Problem | Done |
|---|---|---|---|
| P1 | medium | The world starts paused behind the modal; an agent pasted before OK sees tick 0, "perceive nothing", and timed-out polls with no explanation (it may give up or repeat actions). | **Fixed** in `prompts/common.md`: "If the tick does not advance ... the world is paused ... wait with `agent events` and observe again instead of repeating actions." |
| P2 | high (because of D1) | The Resume prompt says "continue from where your diary left off", but the save can be older than the diary's last lines (60 s autosave; today always, because of D1). The agent's memory and the world disagree (the pickaxe). | **Fixed** in `prompts/resume.md`: "The world is saved about once a minute ... trust what you observe and note the difference." |
| P3 | medium | "Go and see what lies beyond the walls", but nothing the agent senses names walls or the gate: it found the gate only because a far Move pathed through it, and walked into the ruin without seeing it. | **Fixed** (prompt hint) in `prompts/common.md`: "Your senses report units, items, doors and the like, not walls ... Move to a far point; the path goes through doors and gates and opens them on the way (a locked one only if you carry its key)." Sensing walls stays proposal 003. **Since then fixed** (proposal 003): the `walls` observation section lists wall runs and openings (the gate from the start position) and the prompt says so. |
| P4 | low | Mixed path separators: the diary/proposals paths are backslash Windows paths while `npcctl` is forward-slash; `{PROPOSALS}/NNN-<slug>.md` renders as `...\proposals/NNN-<slug>.md`, and a forward-slash `--explorer-home` gives `C:/.../panelhome\e2e-wren\diary.md`. Every tool accepted them. | Open |
| P5 | low | The proposal header example carries a fixed date (`2026-10-24`); an agent without a clock may copy it. | **Fixed**: `written: tick 1840, <today's date>`. |
| P6 | low | The text observation mixes signals the prompt does not explain: "You perceive nothing right now" while "You can" lists trees 2.9 m away (from memory, at night); "too far: 1.5 m (reach 1.5 m)" (rounding); "You command for player 1: 3 unit(s)" for a lone explorer (the cart and wagon?). | Open (observation text, not the prompt) |
| P7 | low | A harvest's yield is not announced: no event names the 2 logs, "You perceive" omits them; only the affordance list shows "pick up 2 × log #63 (0.0 m away)". | Proposal 001 |

## Defects found (D1 and D2 fixed in the final review, see below)

- **D1 (high, fixed): the on-exit save never runs on a window close.** `apps/actor_testbed/src/explorer.rs` adds
  `save_on_exit` to `Last` with no ordering; Bevy 0.20 adds `exit_on_all_closed` to `Last` in `ExitSystems`
  (`bevy_window` lib.rs:144), so in the closing frame `save_on_exit` can run before the `AppExit` is written,
  and the runner exits after that frame. Evidence: three closes (two E2E runs, one probe), each logged "No windows
  are open, exiting" and no "save on exit"; the probe's save stayed at tick 0. Effect: up to 60 s of play lost
  on every quit, also when the menu's child is closed. Likely fix: `save_on_exit.after(bevy::window::ExitSystems)`
  plus a test that closes the window (despawns it) rather than writing `AppExit` by hand.
- **D2 (medium, fixed): the F7 panel overflows and hides the proposals.** `BODY_LINES = 6` counts source lines; an
  agent's entry is one long paragraph that wraps to 15-20 screen lines, so 7 entries fill the window height and
  the "Proposals" list is pushed off the bottom (`resume_running_f7.png`). Also, while the modal is open it covers
  the panel's lower left (`f7_proposals.png`). Idea: cap by characters or wrapped lines, put Proposals first, or
  scroll.
- **D3 (low, docs): `npcctl snapshot check FILE` without `--scenario` checks against the village** and reports
  "snapshot content hash .. differs" for an explorer save; `--scenario testbed:explorer` gives `{"ok":true}`.
  Documented in `docs/agent-experiments.md` §9.

## Still needs a human (hands-on)

- Real mouse clicks through the menu: Main -> Settings (type or paste a folder, Save) -> Explore with an AI agent
  -> New (prefilled names, Random, Start) and -> Resume (pick the agent, "Checking ...", the game); the mismatch
  dialog's **Load anyway** / **Fresh world, keep the diary** with a save from another build.
- The modal's **Copy** reaching the OS clipboard: `cargo test -p testbed_view os_clipboard -- --ignored` in a
  normal desktop session, then paste into a text editor; **OK** closing the modal and unpausing the sim (here
  `npc/resume` stood in for it).
- **Ctrl+V** into the menu's text fields (settings folder, agent and world names).
- **F7** toggling the panel in a live window (here it was opened with `--panels explorer`).
- The menu hiding while a real game runs and coming back when it is closed (and a non-zero exit code shown).
- A real, separate AI session pasting the prompt (here the same agent that ran the test played it; it knew the
  framework, which a fresh agent would not).
- Quit from the window, then Resume: done by the agent after the D1 fix (see "Final review fixes"); one try with
  a real click on the window's close button is still worth it.

## Final review fixes (2026-10-25)

**D1 fixed** (`apps/actor_testbed/src/explorer.rs`): `save_on_exit` now runs in `bevy::app::OnAppExitSystems`,
which `bevy_window` orders after `ExitSystems`. Test `closing_the_window_saves_the_current_tick` builds the explorer
app with `MinimalPlugins` + `WindowPlugin { primary_window: None, exit_condition: OnAllClosed }` (added before and
after the session), spawns a `Window`, runs 300 ticks, despawns the window and updates: the save's `tick` is 300 and
its `saved` date changed. Without the fix the variant with the window plugin added last failed (`left: 0, right: 300`).

Hands-on (Windows 11, `target/debug` build of this branch):

```sh
# New, from PowerShell Start-Process -PassThru (PID 36324):
target/debug/actor_testbed.exe --explorer-home <wt>/target/e2e-d1/agents --agent "D1 Wren" --world "D1 Hollowmere"
curl ... '{"method":"npc/resume"}'                    # {"paused":false,"tick":0}  (operator standing in for OK)
npcctl agent act --unit 62 --controller explorer --steps '[Move (30000,0), execute_orders]'   # accepted, tick 40
# ~17 s later:
curl ... '{"method":"npc/pause"}'                     # {"paused":true,"tick":375}
curl ... '{"method":"npc/unit","params":{"id":62}}'   # components.pos = {x: 18842, z: -7127}
# the save on disk before closing: tick 0 (the New save), saved 2026-10-25T03:51:03Z
taskkill /PID 36324                                   # no /F
#   "No windows are open, exiting"
#   "actor_testbed: save on exit: tick 375 -> ...\d1-wren\d1-hollowmere.world.json"
# the save on disk now: tick 375, saved 2026-10-25T03:51:57Z
target/debug/actor_testbed.exe ... --agent "D1 Wren" --check-save   # {"ok":true}
target/debug/actor_testbed.exe ... --agent "D1 Wren" --resume       # log: "... resumed"
npcctl agent observe --unit 62 ...    # "Tick 375 ...", "Current order: move to (30000, 0)."
curl ... npc/unit 62                  # components.pos = {x: 18842, z: -7127}: where it was at the close
taskkill /PID <resumed>               # "save on exit: tick 375" again
```

**D2 fixed** (`content/testbed_view/src/explorer_panel.rs`): proposals first (the 12 newest by file name, then
"(+N more)"), then the 5 newest diary entries, each body cut to 280 characters on a char boundary with "...";
the panel is a box (`max_height: percent(85.0)`, `overflow: Overflow::clip_y()`) holding the text as a child (a
node clips its children, not its own text). Screenshots with 8 long diary entries and 15 proposals:

- `target/npc-out/e2e/f7_fixed.png` (`--resume --panels explorer --screenshot .. --after-secs 6`): the prompt modal
  is open and, as designed, dims and covers the middle of the screen, including the panel's proposal lines; the
  panel stops at about 85 % of the window height.
- `target/npc-out/e2e/f7_fixed_running.png` (the same folder after OK, a mouse click posted to the game window,
  then `npc/screenshot`): "Proposals" with 12 lines (`[new] Proposal number 4` .. `[rejected] Proposal number 15`)
  and "(+3 more)" at the top, then "Diary" with entries 8, 7, 6, 5 cut at 280 characters, the rest clipped.

**P3 and P5 fixed** (table above). Also from the final review: examples use `VERB@ID` instead of the real store key
id, the recovery trigger names the real rejection (`not_attached`, or a unit controlled by someone else),
`resume_changed.md` says "another version", a BOM'd `agent.json` / `settings.json` is read, and a Windows
read-only folder is accepted as the agents folder.
