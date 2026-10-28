# Island village for AI actors

Build a new opt-in `ai_village` scene and `island_village` explorer world preset. Existing scenarios,
presets, registry manifests and replay/snapshot oracles retain their behavior. Content registers only
when this scene is selected. Implement in testbed content/view and the existing hosts; no core fork.

## Accepted behavior

- A small inhabited island, surrounded by visible water that is blocked by the authoritative navigation
  grid. Buildings, workplaces, resources, roads and named residents form a readable village.
- Human AI actors have Hunger, Thirst and Energy, plus health and accumulating exhaustion. Needs decay
  in simulation ticks. Food, clean water and rented beds are available through actual village supply.
  Hunger/thirst at depletion cause lasting health loss after a grace period. Fatigue slows actors and
  can force a vulnerable recovery on the ground; it does not grant free food or water.
- The world has day/night and four seasons. NPCs work, purchase inputs, produce, trade and satisfy their
  own needs. Winter reduces farm/forage output, with stored goods buffering shortages. No infinite shop
  inventories or automatically refilled wallets. Initial reserves are explicit construction state.
- Chains: grain -> flour -> bread (wood and water as bakery inputs); logs -> planks -> bowls; ore/wood ->
  metal -> tools. A well keeper supplies clean water, and an inn supplies paid lodging. Agents can gather
  and craft using the same material/recipe definitions and sell their output to residents who need it.
- Start the played actor without free spending money; gathering/selling is a viable first action. Prices
  are fixed and visible in this first scene, quantities and purchasing power are finite. Transactions
  check range, ownership, inventory capacity, stock and both wallets again when applied.
- **User decision:** death is permanent for that actor. Its actions stop; possessions and its physical
  diary drop once. The world keeps running. Explicitly begin a new life to join that same world as a new
  SimId, with fresh needs, wallet and memory. Registration/control is rebound by the host, requiring a
  new enter-world handshake, not an automatic resurrection or world reset.
- The diary can be picked up and read by another living actor. NPCs can recover nearby lost diaries;
  another actor can ask for them. A dead actor's diary is immutable after its final publication.
- Existing F7/agent Diary notes remain host data. A host bridge uses the shared NotesStores and records
  only Diary title/body/tick pages as a bounded content command. Simulation/replay never reads files.
  Memory, proposals, credentials, AGENTS.md and arbitrary files are never published. Successor lives
  use fresh notebook folders while prior files remain preserved. Diary publication/custody is saved.
- Merchants have separate finite wallets. Their surplus may enter a finite community fund, which pays
  support to low-cash residents. Those transfers conserve currency; visiting AI actors get no stipend.
- **Added user request:** a scene can enable/configure a background AI scheduler. When the external AI
  stops and its actor remains idle for the configured real-time period, resume that same configured CLI
  session with a short continuation prompt. User selected resumable CLI sessions (e.g. Codex/Claude).
  The scheduler watches activity and existing work, uses cooldowns and bounded concurrency/attempts,
  and never advances the simulation or chooses actor actions itself. Session/executable/workspace links
  are operator-owned host configuration, outside replay descriptors and agent-editable notebook content.

## Boundaries and verification

All authoritative additions use integer state, stable sorted SimIds, recorded commands, registered hashes
and snapshots. Pure calendar calculations use SimClock/SimConfig. Render entities stay separate. Notes,
credential rebinding and persistence run in existing host frame/stop hooks, outside NpcSimTick.

Test construction, physical resource flow, conservation of money/materials, transactional failures,
autonomous survival, seasons, water navigation, lethal needs, cancellation, one-time drops, diary privacy
and custody, new-life identity, snapshots/resume and command replay. Exercise the scene through the
registered agent interface. Inspect native screenshots and menu entry. Run `cargo xtask verify` last,
without concurrent release builds or native apps (the previous work exposed a load-sensitive HTTP test).

These registrations intentionally give this new scene its own content hash. Do not register them in
legacy worlds, change a baseline, replace a replay/snapshot oracle or relax a performance budget.
