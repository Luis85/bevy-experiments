# Registered explorer evidence (2026-10-27)

Tasks 30–31 add opt-in registration, assigned-actor control, generated registered MCP clients and token
rotation. ADR 0016 records the transport/lifecycle decisions. Credentials, sessions and authentication
deadlines are host-only; the saved pure explorer context contains no paths or credentials.

## Automated checks

- Initial focused registration suite: 10/10 passed; subsequent regressions cover release/re-take within one
  tick, enabling registration while a legacy request is parked, and preservation of configured-ID cooldowns.
- Broad focused run: 297/302 passed initially. Its five failures exposed old prompt expectations and the
  missing replay context; those were corrected without changing the existing hash constants or oracles.
  Follow-up run passed 50/51; the final fixture initialization correction then passed the remaining replay
  test directly, including a custom preset after deleting its source folder.
- Recipient-scope tests cover rejection of another actor's orders and continued access to legitimate
  interaction targets and driven vehicles. Existing human/scout content and state hash pins stay intact.
- The menu button test rotates the credential, preserves identity and world bytes, and refreshes AGENTS.md.

## Live generated-MCP run

The ignored `target/phase-i-e2e.py` probe starts a real detached rover and uses the exact generated MCP
command/configuration with the agent folder as its working directory. Its agent routine uses MCP tools.
Developer lifecycle/notes operations are separate. Claude Code is unavailable on this machine.

Run `target/phase-i-e2e-7e52d3e35f5c/summary.json` reports `ok:true`:

- Register, enter the assigned robot character and take control; no unit/controller arguments in tool schemas.
- Move 300 mm with act-and-wait, then write diary, memory and proposal entries.
- Add a developer memory through `npcctl notes add`.
- Stop/save detached at tick 35; resume native on the same port. The same MCP process reconnects with its old
  read cursor; the assigned actor and position match, and the resumed running observation is at tick 46.
- Rotate the token. The old MCP session returns explicit revocation and does not reuse its credential.
  Reading the new handbook and registering explicitly restores control. Both memory entries survive.
- No credential echoes in public responses. All probe-owned processes exit; logs are sanitized.

The initial native capture occurred before rendering pipelines were ready and shows only sky/HUD, so it is
not used as visual proof. Repeating after a five-second rendering warmup passed every check again in
`target/phase-i-e2e-40a74f4ae443/summary.json` (saved tick 27, resumed running observation tick 38).
Its `npc-out/shots/phase-i-handover.png` was inspected at tick 138: the orange rover with two sensor domes,
grass, compound walls, cart and props render correctly, with black unexplored fog outside. The Resume menu capture
`target/shots/phase-i-menu-resume.png` was inspected: actor label, Start detached and New token are visible
and readable. No real desktop input was sent.

## Final gate

`cargo xtask verify` reports **`"ok":true`** in `target/verify-phase-i-final.json`: 1,144 tests passed,
12 skipped, four reported leaky; documentation tests 1 passed/10 ignored; architecture 7/7; all eight
replays verified; six snapshots with none pending; format, Clippy and cargo-deny passed. No optional tools
were skipped. Process-local `CARGO_BUILD_JOBS=8` and `NEXTEST_TEST_THREADS=1` bounded the builders/tests;
no retries, filters, oracle changes or assertion removals were used to pass the gate.

The verified checkpoint `28cb8d4` was merged --no-ff to main as `f78702b`. The five-app release build on main
passed in 3m24s (`target/release-phase-i.log`). Binaries are
`C:\Projects\bevy-experiments\npc-framework\target\release\{game_menu,actor_testbed,npcctl,village,rts_units_demo}.exe`.
The user's running old `npcctl.exe` was renamed aside to permit replacement; its process was not stopped.
