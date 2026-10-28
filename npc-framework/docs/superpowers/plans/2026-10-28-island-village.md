# Island village implementation plan

1. Add failing construction and economy tests, opt-in world/scene selection, content state and factories.
2. Register actual goods/recipes, shops, finite wallets and renewable resource patches; implement atomic
   trade, consumption and gathering with agent observations/affordances and recorded content commands.
3. Run autonomous residents through production, input purchases, meals, drinks and sleep; tune sustainable
   initial stocks/cadences using multi-day headless tests rather than free recurring subsidies.
4. Add tick calendar/seasons, fatigue, lethal needs, corpse/drop handling and recoverable physical diaries.
5. Bridge F7 Diary pages through recorded inputs; add explicit successor lives, fresh notes/memory and safe
   host credential rebinding. Cover save/resume, headless/windowed hosts and privacy boundaries.
6. Build the island, water, buildings/workplaces, resident names, calendar/needs/economy feedback and new-life
   controls in the native view; expose the new preset in the launcher and document how an agent plays it.
7. Add the user's scene-configurable AI scheduler: observe real-time external-agent inactivity and actor
   busy state; resume an explicitly linked Codex/Claude CLI session in the background, with cooldowns,
   process ownership, bounded attempts/timeouts, status and operator-owned bindings. Verify CLI flags
   against installed help/official sources; test with fake runners, not the user's real AI sessions.
8. Review invariants and failure paths, exercise an agent earning/spending loop and death/succession, inspect
   native captures, then run the full gate. Record evidence, make the local feature commit/merge and build
   all five release apps on main. Preserve the user's pre-existing MCP host; do not push or open a PR.

Completed: all eight steps, including the reviewed/polished native surface and host-reset supervision.
The final full gate returned `ok: true` (1,306 tests passed, 12 skipped); all five locked release binaries
built on local main merge `2fda2f0`. Exact evidence and remaining integration boundaries are recorded in
`docs/_notes/island-village-evidence.md` and `docs/_notes/island-village-ui-documentation.md`.
