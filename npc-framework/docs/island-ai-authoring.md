# Authoring Island Village life and production

The `island_colony` showcase opts into `island_ai` in its world/scenario JSON.
Older island scenarios omit that field and retain their existing policy. An
empty object selects the defaults below. Unknown fields and unsafe bounds are
rejected during world validation.

```json
"island_ai": {
  "decision_ticks": 20,
  "critical_need": 15000,
  "thirst_threshold": 55000,
  "hunger_threshold": 50000,
  "recovery_threshold": 30000,
  "night_recovery_threshold": 85000,
  "routine": ["thirst", "hunger", "recovery", "home", "work"],
  "goap": {
    "max_expansions": 128,
    "max_steps": 12,
    "buy_cost": 1,
    "gather_cost": 1,
    "craft_cost": 1
  }
}
```

Need values are remaining wellbeing, from zero to 100000. A routine branch runs
below its threshold. The routine is a priority selector: reorder thirst,
hunger, recovery and home, with recovery before home and work always last.
Every goal occurs exactly once. The recovery-before-home rule prevents the
night-time shelter branch from suppressing needed rest. Critical actionable
thirst/hunger, player orders and already-running work
retain their protected positions before this routine. Each guard checks that
its action can actually start; an empty shop does not continuously interrupt
production. The inspector and existing behavior-tree tools show the running
concrete goal, not an invented description of an NPC's thoughts.

Use **Worlds → Resident AI** in the native launcher to edit the selected world's
resident policy. The JSON form above is the equivalent authoring format for
reviewing or sharing exact values. The decision interval is simulation ticks,
not render frames; search limits cap work per planning attempt; action cost
multipliers change preference without changing recipe duration or item prices.

The policy is immutable construction data copied from the selected preset. It
is also registered, hashed and saved as `island_ai_policy`, so an ordinary
save/resume continues the same decisions. Edit a source preset and create a new
world to try a policy; changing that file does not rewrite an existing save.
The shipped source lives in `content/testbed_content/worlds/island_colony.json`
and the direct scenario in `content/testbed_content/scenarios/island_colony.json`.
Source changes are included by the next Cargo build. Explorer installations can
use their existing editable world-preset folder and select the edited preset
when creating a new world. The native editor is a validated priority/policy
form; it does not provide an arbitrary graph canvas or rewrite a running world.

## How production decisions work

The community production ledger forecasts needs, reserves actual stock and
allocates finite purchasing power. The Colony food-reserve setting changes the
forecast horizon from one to four days. Household demand continues to share
cash with viable industrial input bundles; a reserve target does not create
goods or money.

A recorded housing commission is financed as one complete material bundle after
urgent food, water and critical warmth protection, before long-horizon comfort
stock. The planner expands missing planks through the builder's real profession
recipes and accounts for their input logs separately from the cottage's retained
timber. Owned goods need no new payment; missing materials share the builder's
remaining finite purse. This prevents an unaffordable forecast coat bid from
permanently withholding the last construction materials. Cancellation and active
construction still use the existing invalidation and physical reservation rules.
Critical warmth also finances the household's existing one-log fuel target
before a forecast clothing bid, so an unavailable coat cannot withhold accessible
hearth fuel. This target is credited once by the later household forecast.

In colony worlds an unprotected cold resident below the configured recovery
threshold uses the existing home-rest action even when energy is full. The same
read-only warming query governs recovery and fuel procurement. With neither
usable home heat nor carried fuel, the night-home branch yields to ordinary work
so the resident can buy, or finitely gather, a real log before returning home.
Worn coats already restore warmth during activity. This uses the existing rest,
fuel and warmth rates; committed work retains its current interruption contract,
and historical non-colony worlds keep their energy-only recovery policy.

Configured colonies anticipate clothing replacement. The shared clothing
forecast counts cold calendar ticks over the next two simulation days, including
night and winter boundaries. It conservatively assumes exposure during unknown
future activity and subtracts only sheltered rest already scheduled. When an
equipped coat's remaining durability cannot cover that lead time, the household
requests at most one carried spare. A real spare already owned fulfills that
target and is credited once by the material ledger. Residents continue wearing
the usable coat until it breaks; a forecast never creates or prematurely
consumes clothing. Purchases and upstream work still require finite household
cash, real inputs and actual trades. Legacy living worlds retain their previous
replacement policy.

The GOAP planner takes an actor's funded production goal and searches over
inventory quantities, remaining supplier/patch stock, currency and position.
Actions have preconditions and effects: buying consumes coins and finite posted
stock; gathering consumes a patch unit; crafting consumes the registered recipe
inputs and produces its outputs. Candidate recipes come from the actor's
profession and actual workstation. Prices, resource quantities and recipe
durations come from the live simulation. Integer Manhattan travel estimates
and the positive cost multipliers rank alternatives deterministically.

Only the first step is executed, through the ordinary shared buy, gather or
craft implementation. That implementation validates live reach, capacity,
wallets, stock, tool/station reservations and world permissions again. No
hypothetical planner effect modifies the world. The next life decision replans
from actual results; a changed seller or consumed ingredient cannot grant a
free trade. Changed work plans are recorded as `VillagePlanSelected` journal
effects. `village::goap::explain` is a bounded, read-only inspector explanation.
It distinguishes no funded goal, unavailable current prerequisites, and a search
budget limit. Hitting the limit does not claim the goal is impossible. These
diagnostics do not change the goal order, search costs, selected first step, or
the fallback work policy.

Search is bounded to 8–512 expanded states and 1–24 steps. The domain includes
the profession's material prerequisites and concrete purchases/gathers, while
existing shared survival, hunting, equipping and construction actions retain
their specialized execution. If bounded search finds no plan, the resident
uses the established funded production/procurement policy. This prevents a
tuning budget from turning a viable worker into an idle one. Idle workers can
then attend a physical social meeting when colony gatherings are enabled.
Meeting target selection and settlement share `colony::social_available`:
sleeping, committed work, work approaches and critical survival needs make a
participant unavailable. Walking toward a social meeting remains eligible.
Social queries never cancel work or move actors. Existing proximity, cooldown,
pair ordering and affinity amounts still apply; this remains a simple leisure
model rather than a dialogue-driven personality or relationship system.

The pure `colony::home_heating` query describes only a living actor's own home
and carried available fuel. It is exposed as `village.home_heating` to character
clients. In cold weather, a household member within 1.5 metres of its door
automatically supplies one unreserved carried log when the hearth needs lighting;
the existing upkeep heats it for half a simulation day. Inspecting these facts
never spends fuel and does not reveal another member's bag.

Configured residents also recover from an observed integer navigation corner:
a long smoothed line can be clear while a rounded short movement step touches
the adjacent building cell. After forty blocked ticks, resident policy requests
a clear cell-center detour through the existing navigator, then resumes the
physical destination. At most three attempts occur in one stuck area, with a
hundred-tick backoff; two metres of real progress resets the attempt budget.
No position, reach, obstacle or trade rule is bypassed. The recovery waypoint
and attempt state are hashed and saved. An enclosed or genuinely inaccessible
destination can remain blocked after the bounded attempts.

## Validation

`colony_ai` covers authored priority changes, critical-need precedence, real
procurement before crafting, read-only inspection, policy hashing and exact
snapshot continuation. Unit tests in `village/goap.rs` cover a five-step chain,
finite source/cash constraints, bounded search and deterministic tie-breaking.
`colony_economy_soak` checks two seeds over three normal days, reports low-need
and finished-food stockout samples, verifies continuing late production/trade,
and checks resident survival and exact currency conservation every 300 ticks.
These are bounded evidence, not proof of indefinite economic stability.
`colony_route_recovery` reproduces the reported corner at `(4046,-5991)` and
requires the blacksmith to reach the carpenter and pay for a real plank, plus
snapshot equivalence while recovering.
`colony_clothing_forecast` checks early funded replacement while current warmth
is still full, calendar-aware exposure, known rest, a real paid spare that does
not generate another coat request, and unchanged legacy policy.
`colony_life_polish` covers actor-owned heating visibility and exact existing fuel
consumption, and meeting availability without cancellation. `colony_shortage_recovery`
declares a temporary timber-patch exhaustion, waits for ordinary replenishment,
and requires currency conservation, survival, resumed timber/plank output and
trading, including equivalent continuation from a mid-shortage snapshot. This
is a bounded shortage fixture; it does not establish recovery after permanent
specialist loss. See the validation notes for checks actually run.
`colony_commission_funding` isolates the forecast-clothing reservation deadlock,
keeps urgent survival purchases ahead of construction, and checks cashless use of
an already complete owned bundle. The autonomous commission test retains its
two-day completion deadline and continues through early winter to check survival,
real garment output/trade and conserved currency after construction. Its recovery
window spans a complete day after a scheduled dividend, because the clock points
to the next tick and an exact pre-dividend wardrobe snapshot cannot establish
production liveness. `colony_warmth_recovery` covers full-energy cold recovery,
physical cashless fuel acquisition at night, completed rest, preserved committed
gathering and unchanged legacy arbitration.

```text
cargo test --locked -p testbed_content --test colony_ai
cargo test --locked -p testbed_content --test colony_life_polish --test colony_shortage_recovery
cargo test --locked -p testbed_content --test colony_economy_soak -- --nocapture
cargo test --locked -p testbed_content --test colony_economy_soak extended_colony -- --ignored --nocapture
```

The explicitly ignored extended audit runs an unchanged seed-seven colony for
336000 ticks. It reports all four needs, winter clothing coverage, real material
replacement, money and the maximum observed consecutive blocked movement. The
living calendar begins on day nineteen, so the late-winter window is
144000–216000 ticks, followed by a separate final spring window. Running this
audit is explicit; the normal verification gate does not silently incur it.
