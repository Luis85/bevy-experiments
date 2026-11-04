# NPC life and the shared island action API

Island residents and clients operate the same physical economy. Residents choose actions through the registered `island_life` behavior tree; human or AI clients choose from the action affordances returned by `agent/observe`. The browser viewer remains a read-only observer. A native human action panel is not part of this increment.

## Daily decisions

The tree prioritizes critical actionable thirst and hunger, explicit player orders, an activity already in progress, normal water and food needs, recovery, returning home at night, and profession work. Gathering and crafting keep their leaf until completion. Critical needs can cancel owned work and release its reservations. Decisions have a 20-tick cadence; the framework's staggered 40-tick reconsideration also detects critical changes between need thresholds. When a supplier is empty, a producer can resume production rather than waiting forever for the supply it creates. The framework's external-control branch suspends autonomous choices; release returns the resident to its tree.

The public `decision` field describes the running branch, such as **find water**, **recover at home** or **supply the village**. Native captions show that goal when there is no more specific gathering/crafting caption. The browser inspector distinguishes **Activity** (what is physically happening) from **Decision** (the active tree goal). These closed labels do not expose model reasoning or private notebooks.

## Shared client actions

Use existing `agent/act` or MCP `act`, then observe the recorded result. Registration/control requirements remain unchanged. Every mutation still goes through recorded commands and application-time checks.
All six island recipe aliases work in headless and native hosts. Canonical and named crafting actions obey the selected world's crafting switch and the actor's crafting permission.

| Task | Action | Shared rule |
|---|---|---|
| Move | `walk`, or Move order plus `execute_orders` | Physical navigation and blocked-water cells |
| Gather | `gather`, target resource patch | Within 1.5 m, finite stock, pack capacity, work duration |
| Manufacture | Advertised `craft:<recipe>` | Actual ingredients, shared workstation/reservation and duration |
| Trade | `buy` / `sell`, target merchant, `kind`, `quantity` | Reach, finite stock, demand, money and pack capacity |
| Eat / drink | `eat` / `drink` | Consume one bread / clean water from the actor's pack |
| Home recovery | `rest_home`, no target | Within 0.8 m of own `home`; free, 300 ticks at 400 energy/tick |
| Ground recovery | `rest_ground`, no target | Free anywhere; 300 ticks at 40 energy/tick |
| Rent a bed | `rest`, target innkeeper | Within 1.5 m; 3 coins, 300 ticks at 400 energy/tick |
| Cancel work/rest | `stop_work` | Cancels work, movement and voluntary rest |
| Recover belongings | `recover_diary` / `collect_coins`, target item | Near, accessible, unheld or owned as appropriate; capacity/overflow checks |
| Speak | `talk`, `text`, optional target | Same speech range, text bounds and living-actor checks |

Home is a resting position rather than purchasable property. Rest does not stop hunger or thirst. Take food and water into account before sleeping. Visitors can carry out every production task without acquiring a profession, but they receive no starting resident stock, shop income or community stipend. NPCs do not author private host notebooks.

## Reading the environment

The `village` observation includes your `home`, `role` and an `environment` object:

- `resources`: known patch ID/kind and gather duration. `current` is true only while sensed or within reach. `remaining`, `capacity` and `replenish_at` are null for stale remote knowledge.
- `workstations`: known public workplace ID/name/role, typical `products` and accepted goods (`accepts`), plus supported recipe names, durations and ingredient/output counts. Wells and inns can have empty recipe lists. This describes a place's possibilities, not its resident's current location, stock, money or availability. Use current affordances to check whether you can trade or craft.

Treat each observation's affordances as a current replacement. A remembered actor position is a place to investigate, not a live tracker. If a merchant is no longer sensed, search a compatible workplace using its learned position and reacquire a visible trader before trading. The registered rehearsal uses these public workplace descriptions without scene coordinates, hidden NPC positions or a larger blind-search budget.

Resources renew at their recorded `replenish_at` simulation tick. Grain yield varies by season. The observer inspector shows resource duration/renewal and workplace production quantities from the same world projection. Neither observation nor rendering advances the world or changes its future state.

## Compatibility and validation

Use a new Island Village world for this increment: registering its tree and action state intentionally changes the island content hash. This is not a migration of older island saves. New-build snapshots must resume deterministically; incumbent testbed and original village replay/golden files remain protected and unchanged.

The implementation plan is [NPC life and environment](superpowers/plans/2026-11-01-npc-life-and-environment.md). Verification evidence is recorded at closeout. No live paid-provider run is necessary to validate this shared simulation/API increment.
