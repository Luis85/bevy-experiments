# NPC Framework assistant guide

Read `CLAUDE.md` and the relevant `.claude/rules/` files. Its core/content
boundary and protected oracle rules apply to every assistant, even without
Claude Code hooks. Specs/plans live in `docs/superpowers/`, decisions in
`docs/adr/`, and integration guidance in `docs/using-npc-framework.md`.

- Authoritative state uses integer units, stable `SimId` order and counter-based
  randomness. Sim code never depends on a renderer, wall clock, hash iteration
  order or frame delta. Preserve the host-owned schedule and movement seam.
- Generic capability crates never name content. Register content through the
  facade by stable name; cross-capability glue belongs in the facade.
- Do not alter replay/golden/snapshot baselines, architecture tests, protected
  tooling or lint policy to pass a check. Follow `docs/rebaselining.md`; stage new
  evidence and obtain the documented approval before replacing an oracle.
- Run Cargo inside `npc-framework/`. `cargo run --locked -p game_menu` launches
  the demos; `cargo run --locked -p npcctl -- --help` lists headless tooling.
- Focused tests: `cargo test --locked -p <crate>`. Final gate:
  `cargo xtask verify`; report the JSON `ok` verdict and any skipped tools.
  `cargo xtask verify --quick` is an iteration check, not a full gate.
- Visible changes require a local playtest/screenshot as described in
  `docs/debugging.md`. Debug/agent tooling must not alter simulation outcomes.
- Keep external agent sessions, explorer homes, credentials and personal notes
  outside version control. Existing `.claude/` hooks load when Claude Code starts
  here, not from the monorepo root.
