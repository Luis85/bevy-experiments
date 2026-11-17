# Authoring resident behaviour on the island

Residents of the regional island (`living_world`: Harbour, Alderwood and Stone
Ridge) run a behaviour tree that is data. You change what residents do by
editing JSON: the tree, the daily schedule, the trait ranges and a few
thresholds. No Rust is involved. This guide lists everything a tree can name.

## Where the data lives

| What | File | Block |
|---|---|---|
| The resident tree | `content/island_content/trees/resident.json` | whole file |
| Schedule, traits, places, tree overrides | `content/island_content/worlds/living_world.json` | `living_world.behavior` |
| Need thresholds, decision interval, GOAP limits | same world file | `island_ai` |
| Goods, recipes, professions, prices | same world file | `living_world.economy` ([economy guide](island-economy-authoring.md)) |

The shipped world file has no `behavior` block. An absent block, or an absent
field inside it, means the built-in value: the tree file above and the defaults
shown in this guide. To change the schedule you add only the `schedule` field.

```json
"living_world": {
  "behavior": {
    "version": 1,
    "trees": [{"file": "my_resident.json", "tree": { "name": "island_life", "root": { } }}],
    "traits": { },
    "schedule": { },
    "places": [{"key": "pier", "position": {"x": 500, "z": -43000}}]
  }
}
```

`trees` replaces the built-in list, so it must contain a tree named
`island_life`: the one every resident runs. `file` is only a label used in
error messages. Other trees must be named `island_resident_...`; they are
validated and stored but nothing runs them yet.

## The edit, new world, look loop

1. Edit `trees/resident.json`, or the `behavior` block of the world file.
2. Start a new world. From source: `cargo run --locked -p island_game`, then New.
   Headless: `cargo run --locked -p npcctl -- run --scenario island:living_world --ticks 2400`.
   The tree file is embedded at build time, so Cargo rebuilds the content crate.
   The installed game needs no build: put the tree in the `behavior.trees` of
   its rules file (`<home>/island-village/living_world.json`) and start a new world.
3. A mistake stops the world from loading with a message that names the file,
   the node number and the node name. Node numbers count the nodes in file
   order from 0, the same numbering decision records use.
4. Watch what residents do: the inspector shows the running leaf, and the
   journal holds a decision record for every change (see below).

A running or saved world never changes. The behaviour data is copied into the
world's construction data and hashed with it, so a continued save keeps the
trees it was built with even after the source files change. There is no hot
reload: the tree runtime resolves names once, when the world is built.

## Tree files

A tree file is `{"name": ..., "root": node}`. A node is:

```json
{"name": "Thirst",
 "kind": {"Guard": {"cond": "life.thirsty", "abort": "LowerPriority"}},
 "children": [ node, ... ]}
```

`name` is free text shown in the inspector and in decision records. `children`
are written inline, in priority order; a leaf has none. Moving a branch is
moving its text.

The engine's own flat `TreeDef` shape, `{"name", "nodes": [...]}` with
children as indices into a pre-order list, is also read; that is the shape a
saved world stores. Do not hand-write it: inserting a node renumbers every
later index.

### Node kinds

| `kind` | Children | Behaviour |
|---|---|---|
| `"Selector"` | one or more | Tries children in order; the first that does not fail runs. |
| `"Sequence"` | one or more | Runs children in order; fails when one fails. |
| `{"Guard": {"cond": C, "abort": A}}` | exactly one | Fails when condition `C` is false, otherwise runs its child. |
| `{"Leaf": {"action": X, "param": P}}` | none | Starts action `X`. |
| `{"UtilitySelector": {"scorers": [S, ...]}}` | one per scorer | Tries children from the highest score down. |

`abort` is `"None"` or `"LowerPriority"`. With `"LowerPriority"` the guard is
re-checked while a later branch of the same selector is running (every 40 ticks
and whenever a need level changes); when it passes, that branch is stopped and
the guard's child starts. Use it for needs that must interrupt work.

A leaf parameter is `"None"`, `{"Name": "water"}`, `{"Int": 5}` or
`{"Pos": [x, z]}` (millimetres).

### How leaves run

Every `life.*` leaf takes **one bounded step** when it starts: it walks a
decision's worth towards a seller, or makes the trade if it is already there,
or starts one craft. Then it holds for `island_ai.decision_ticks` (20 ticks),
or until the physical work it started is finished, and succeeds. The tree is
then decided again from the root. A leaf that cannot begin fails immediately,
and its selector tries the next option in the same decision.

So a goal that needs several steps is written as a selector of alternatives,
not as a sequence: "drink, else buy water, else fetch water" is entered again
every decision until the resident has drunk.

### The rule for aborting guards

A guard with `"abort": "LowerPriority"` must be able to start its branch
whenever its condition holds. Otherwise it would stop the resident's work, fail,
and stop it again at the next re-check. The loader enforces this:

- a condition **covers** some leaves (see the table): when it holds, at least
  one of them can start. Every covered leaf must be among the first leaves the
  branch can try. `life.thirsty` covers `life.drink`, `life.buy water` and
  `life.gather water`, so the branch must offer all three;
- or the branch must be able to start a leaf that always starts: `life.idle`,
  `life.sleep`, `life.warm_up` or `life.go_to home`.

A condition that covers nothing, such as a schedule window, cannot abort work
on its own. Give it `"abort": "None"`: it is then checked whenever the
resident decides again, which is at most 20 ticks later unless physical work
is in progress.

## Actions

| Action | Parameter | One step | Fails when |
|---|---|---|---|
| `life.drink` | none | Drink one carried water. | No water is carried. |
| `life.eat` | none, or a food good key | Eat one carried food: the preferred food, else the alternative, else berries. | No food is carried. |
| `life.buy` | `water`, `food`, `fuel`, `provisions`, `clothes`, `reserves`, `inputs`, `tool`, or a good key | Walk to the cheapest reachable seller the resident can afford and buy. `water` and `food` buy one unit to consume. `fuel` buys hearth fuel when the home needs it. `provisions` (water and food), `clothes` and `reserves` (both) stock what the funded plan wants. `inputs` buys work materials. `tool` replaces the profession's tool. A good key buys one more unit. | Nobody affordable sells it, or nothing is wanted. |
| `life.sell` | `surplus`, or a good key | Carry goods offered for sale to the nearest neighbour with funded demand and sell. Not used by the shipped tree. | Nothing is for sale or nobody wants it. |
| `life.gather` | `water`, `food`, `fuel`, `outputs`, or a good key | Walk to the nearest patch and gather one unit. `food` is berries. `fuel` is hearth fuel when the home needs it. `outputs` is the profession's raw good with the most funded demand. | No reachable patch has any left, or the pack is full. |
| `life.craft` | none, or a recipe key | Walk to the workplace and start the best-scoring funded recipe whose inputs are in the pack. | No recipe is ready. |
| `life.help` | none | Living world: lend a played friend (bond at a tier that `helps`) a hand in free time, at most once a day each, with every need above `live.help_needs_above`: walk to the homestead and bring one material its upgrade misses (from the pack, or bought at a store on the way), work one stage of `help_work_ticks` at the door, or harvest a ripe bed of the friend's plot. The shipped tree tries it first in `Free time`. | No friend's homestead needs a hand, the resident is not in its leisure block, a need is too low, or the village is gathering. |
| `life.hunt` | none | A hunter with a spear and funded demand hunts the nearest prey. | Any of those is missing. |
| `life.build` | none | Work on the cottage commission the resident holds, else walk to a building site of the settlement whose materials are all delivered and work one stage; else, when hired for a played character's homestead upgrade, walk to its door and work one paid stage there. | No commission, no supplied site and no homestead hire. |
| `life.pursue_goal` | none | GOAP: plan buy, gather and craft steps towards the funded production goal and take the first. | No funded goal, or no plan within the search limits. |
| `life.change_job` | none | Take a requested or better-paid vacant workplace. | Nothing changes (the usual case). |
| `life.equip` | none | Put on a carried coat. | One is worn or none is carried. |
| `life.sow` | none | A farmer walks to a field of the settlement with a fallow plot and sows it. Seed comes from the field's own seed stock, else from the pack. | Not a farmer, no fallow plot, not a sowing season, too late to ripen before winter, or no seed. |
| `life.tend` | none | A farmer walks to a field and tends a growing plot that has gone the crop's `tend_interval_ticks` without care. | Not a farmer, or no plot needs it. |
| `life.harvest` | none | A farmer walks to a field with a ripe plot and harvests it into the pack; the plot's seed stays in the field. | Not a farmer, no ripe plot, or the pack is full. |
| `life.haul` | `fuel`, `materials`, `surplus`, `restock`, `boat`, `help` | `fuel` carries hearth fuel home; the household lights the hearth at the door. `materials` brings a building site of the settlement what it still needs: spare goods from the pack, else goods bought at a store, sold to the site at the quote; without a village site, it carries an order to a played character's homestead door, paid from that owner's escrow when handed over. `surplus` lays the resident's own surplus in a store or stall. `restock` buys from storage what the stall is short of and lays it on the stall. `boat` (living world) carries unsold stock of a good the merchant boat buys, while the island holds it above its stock targets, to the trading post while the boat is in or due that day; the shipped tree tries it after `materials`. `help` (living world) is the fallback of a resident with no funded work, last in the shipped Work hours: it gathers a raw good an open site of the settlement lacks and no store holds, else a good the resident has no use for that is, or goes into, a staple short in the settlement (grain for bread, fibre for the coat); `materials` or `surplus` then carries it to the site or a store, where the resident is its seller. | Nothing to carry: no fuel needed, no site order this resident can fill, no surplus worth a trip, no stall short of stored goods, or no site or short staple a hand could help. |
| `life.collect_found` | none | Pick up a dropped coin purse or diary within reach. | None is near. |
| `life.sleep` | none | Walk home and rest there. Inside the resident's sleep block this is the sleep of the night: it lasts to the end of the block at `tuning.sleep_energy_per_tick`, and only a critical need of the resident, a starving child of the home or a player order ends it earlier (`VillageWoken`). Outside the block it is a short rest of 300 ticks at `tuning.nap_energy_per_tick`. | Never. |
| `life.warm_up` | none | Walk home and stay by the hearth until warm, at most 1500 ticks. | Never. |
| `life.greet` | `player` | Turn to the played character, if it is within 12 m, awake and not indoors where the resident is not, and say the bond tier's line of `economy.live.greetings` to it (`VillageSpeech` addressed to it), once a day per resident (`Bond.greeted_day`, saved). If the played character sold one of the resident's goods today and stands in its settlement, the line is "You've been busy at my trade, <name>." The bond gains half of `talk_points`; journal `VillageGreeting` (`to`, `text`, `trade`). Guard it with `life.player_within`. | Nobody to greet, too far, asleep, indoors elsewhere, in a meeting, or already greeted today. |
| `life.go_to` | `home`, `work`, `centre`, `meal` (home, else the nearest inn, else the square), a key from `behavior.places`, or `{"Pos": [x, z]}` | Walk there and stay. | The way is blocked (`home` never fails). |
| `life.socialise` | none | Walk to a free neighbour within 15 m for a meeting. | Nobody is free, or the resident met someone in the last 600 ticks. |
| `life.continue` | none | Keep doing the physical work in progress. | Nothing is in progress. |
| `life.idle` | none | Stand still for one decision. | Never. Put it last. |
| `life.sit_down` | none | Sit down to the meal: eats when hunger is below `schedule.meal_hunger_threshold`; in a meal block it marks the day's sit-down meal had (one a day). | The resident is not alive. |
| `ExecuteOrder` | none | The engine's player-order leaf. | No order is queued. |

Field work is timed like gathering: the matching tool (a hoe for grain) and
morale shorten it, an elder takes longer, and `life.busy` holds while it runs.
Fields, crops and their seasons are data: see the
[economy guide](island-economy-authoring.md#crops-and-fields).

Building sites, stores and stalls are described in the
[economy guide](island-economy-authoring.md). A hauler never carries somebody
else's property: goods fetched from a store are bought at the quote and sold on at
the quote, to the site (the civic fund pays) or through the stall.

Buying and selling go through the same validated trade a player uses: both
actors within 1.5 m, real stock, real coins, the buyer's bid limit. `life.buy`
also buys at a store or stall, where the absent owner is credited.

## Conditions

Conditions take no parameter. Thresholds come from the `island_ai` block, the
schedule or the resident's traits.

| Condition | True when | Covers |
|---|---|---|
| `life.critical_thirst` | Thirst is below `critical_need` and water is carried, affordable or gatherable. | `life.drink`, `life.buy water`, `life.gather water` |
| `life.critical_hunger` | Hunger is below `critical_need` and food is carried, affordable or forageable; or a child of the home has nothing left to eat, no adult of the home carries food and this adult can buy some. Wakes a sleeper. | `life.eat`, `life.buy food`, `life.gather food` |
| `life.thirsty` | Not busy, Thirst below `thirst_threshold`, and water is obtainable as above. | same as critical thirst |
| `life.hungry` | Not busy, Hunger below `hunger_threshold`, and food is obtainable as above; or a child of the home has eaten less than 60,000, no adult of the home carries food and this adult can buy some, whatever its own hunger and purse. | same as critical hunger |
| `life.wants_meal` | Not busy, inside a meal block, carrying food, Hunger below `schedule.meal_hunger_threshold`. | `life.eat` |
| `life.tired` | Not busy and Energy below `recovery_threshold`, or inside a sleep block with Energy below `night_recovery_threshold`. | `life.sleep` |
| `life.coat_season` | Living world: the resident neither wears nor carries a coat, and it is winter or the last `tuning.coat_lead_days` of autumn, or a coat is on offer that it (or the civic fund's balance above its reserve) can pay (`living::coat_wanted_now`). Buying one comes before rest and chores; the civic fund pays the price a resident cannot (`VillageCoatGrant`): from its reserve in the coat season, otherwise only from the balance above it. | none |
| `life.freezing` | Living world: Warmth below `tuning.freezing_warmth` without a coat, unless Thirst or Hunger is below `critical_need` and lower still: the lowest critical need is served first. | `life.warm_up` |
| `life.cold` | Not busy, no coat, it is cold in the open (night, winter or snow) and Warmth is below `recovery_threshold`. The home shelters with or without a lit hearth. | `life.warm_up` |
| `life.bedtime` | Not busy, inside a sleep block, and the hearth does not still need fuel fetched. | `life.sleep` |
| `life.busy` | Gathering, crafting, hunting, building, an interaction or a rest is in progress. | `life.continue` |
| `life.company_nearby` | A free neighbour is in walking distance and the meeting cooldown has passed. | `life.socialise` |
| `life.has_food` | Carries something edible. | `life.eat` |
| `life.has_water` | Carries water. | `life.drink` |
| `life.has_surplus` | Holds surplus of an own product worth a trip and a store of the settlement has room. | `life.haul surplus` |
| `life.site_needs_materials` | A building site of the settlement waits for materials this resident carries spare or can buy at a store, and the civic fund can pay; or a homestead upgrade of the settlement has an order this resident can fill. | `life.haul materials` |
| `life.site_needs_builder` | A building site of the settlement has all its materials and no builder. | `life.build` |
| `life.field_needs_harvest` | Not busy, a farmer, and a field of the settlement has a ripe plot nobody else is working, with room in the pack. | `life.harvest` |
| `life.field_needs_sowing` | Not busy, a farmer, and a fallow plot may be sown now: a sowing season, time to ripen before winter, seed in the field or the pack. | `life.sow` |
| `life.field_needs_tending` | Not busy, a farmer, and a growing plot has gone the crop's interval without care. | `life.tend` |
| `life.stall_needs_stock` | A stall of the settlement is short of a good its storage holds and this resident can carry it over. | `life.haul restock` |
| `has_unit_order` | A player order is queued (engine condition). | `ExecuteOrder` |
| `life.in_work_window` | Inside a work block. | nothing |
| `life.in_meal_window` | Inside a meal block. | nothing |
| `life.in_leisure_window` | Inside a leisure block (delayed by diligence). | nothing |
| `life.in_sleep_window` | Inside a sleep block. | nothing |
| `life.wants_leisure` | Not busy, inside a leisure block, and not out in the cold (night or winter) without a coat. Everybody has an evening; sociability decides whether it is spent in company. | nothing |
| `life.family_at_home` | Has a partner or a child, and a home, and the village is not gathering. | nothing |
| `life.gathering` | The village is gathering, the resident is free to attend (no need at or below `gathering_needs_above`), not busy, not in its sleep block, not cold without a coat. | nothing |
| `life.meal_due` | Not busy, inside a meal block on a day without a sit-down meal yet, carrying food, and the village is not gathering. | nothing |
| `life.sociable` | Sociability is at least `traits.sociable_min`. | nothing |
| `life.diligent` | Diligence is 60 or more. | nothing |
| `life.thrifty` | Thrift is 60 or more. | nothing |
| `life.has_coins` | Owns at least one coin. | nothing |
| `life.has_spare_coins` | Owns more coins than the thrift reserve. | nothing |
| `life.needs_fuel` | Cold without a coat, the hearth is out, and too little fuel is carried. | nothing |
| `life.fuel_patch_nearer` | A patch of hearth fuel is nearer than the nearest seller. | nothing |
| `life.carrying_fuel` | Carries hearth fuel. | nothing |
| `life.needs_tool` | The profession uses a tool and no usable one is carried. | nothing |
| `life.workplace_open` | The workplace is reachable and the job is feasible. | nothing |
| `life.stock_available` | An affordable seller offers something the funded plan wants. | nothing |
| `life.food_available` | Food is carried or affordable. | nothing |
| `life.water_available` | Water is carried or affordable. | nothing |
| `life.has_commission` | Holds a building commission. | nothing |
| `life.at_home` | Within 1.5 m of the front door. | nothing |
| `life.at_work` | Within 1.5 m of the workplace. | nothing |
| `life.night` | Night by the world clock, not shifted by chronotype. | nothing |
| `life.winter` | The season is winter. | nothing |
| `life.storm` | Today's weather is a storm. | nothing |
| `life.raining` | Today's weather is rain. | nothing |
| `life.snowing` | Today's weather is snow. | nothing |
| `life.elder` | The resident is an elder (`demography.elder_age_seasons`). | nothing |
| `life.has_partner` | The resident has a living partner. | nothing |

Every schedule window is shifted by the resident's chronotype. There is no
"not": when a tree needs the opposite of a condition, put the branch after a
guard on the condition itself, so it is reached only when that guard fails.

### Scorers

For a `UtilitySelector`. Each is 100000 minus the need, so a higher score is
more urgent: `life.urgency_thirst`, `life.urgency_hunger`, `life.urgency_rest`,
`life.urgency_warmth`.

## The shipped tree

`trees/resident.json`, top to bottom. `LP` marks a guard that aborts
lower-priority work.

| Branch | Guard | Leaves |
|---|---|---|
| Freezing | `life.freezing` LP | get warm now (warm up) |
| Critical thirst | `life.critical_thirst` LP | drink, buy water, fetch water |
| Critical hunger | `life.critical_hunger` LP | eat, buy food, forage |
| Player orders | `has_unit_order` LP | `ExecuteOrder` |
| Current activity | `life.busy` | continue |
| Thirst | `life.thirsty` LP | drink, buy water, fetch water |
| Hunger | `life.hungry` LP | eat, buy food, forage |
| Coat before winter | `life.coat_season` | wear a carried coat, else buy one |
| Gathering | `life.gathering` | meet at the square, else walk to it (a village event: above non-critical tiredness, the meal and the chores) |
| Tired | `life.tired` LP | sleep |
| Cold | `life.cold` LP | warm up |
| Bedtime | `life.bedtime` LP | go to bed (sleep) |
| Meal time | `life.meal_due` | go to the table (home or inn), sit down to the day's meal |
| Chores | none | pick up lost property; put on a coat; fetch hearth fuel when `life.needs_fuel`; stock provisions when `life.has_spare_coins`; replace clothing |
| Storm | `life.storm` | craft during work hours, else shelter at home |
| Sell surplus to the boat | none | while the boat is in or due, carry what it buys to the trading post (above the evening: a long walk to the water is finished, not dropped at dusk) |
| Leisure | `life.wants_leisure` | greet the played character within 6 m (`life.player_within`, once a day), help a friend, a family evening at home, meet a neighbour (sociable residents), else stroll to the square |
| Work hours | `life.in_work_window` | change job, build (a `life.site_needs_builder` guard: a supplied site interrupts the lower work), carry building materials, harvest, sow and tend the field, pursue goal (GOAP), craft, hunt, buy materials, gather, store surplus, restock the stall |
| Night | `life.in_sleep_window` | stay home |
| Player nearby | `life.player_within` 6 m | greet the played character (once a day): a resident with nothing else to do notices it too |
| (last) | none | seek company, idle |

In the living world a resident without a coat out in the cold (night, winter
or snow) with Warmth below `tuning.exposure_warmth` starts no outdoor errand:
`life.build`, `life.craft`, `life.gather`, `life.go_to` (except home),
`life.harvest`, `life.haul`, `life.help`, `life.hunt`, `life.pursue_goal`,
`life.sell`, `life.socialise`, `life.sow` and `life.tend` fail with "too cold
to go out without a coat", and `life.idle` waits at home. Buying, eating,
drinking, resting and going home still start. See
[ADR 0041](adr/0041-cold-first-needs-and-service-trades.md).

The `Storm` branch always starts one of its leaves (`life.go_to home` never
fails), so on a stormy day nobody reaches `Leisure` or `Work hours`: outdoor
work stops and crafters keep working indoors. Needs, chores and bedtime sit
above it and are unaffected. The three field leaves fail at once for everybody
who is not a farmer, so they cost other residents nothing.

Buying goes to a fixed place. `life.buy` picks a store, a stall or a seller
who stands still; a seller who is walking is a target only within six metres
(`WALKING_SELLER_REACH_MM`), so a buyer never trails somebody across the
settlement. `village::view::supplier(world, entity, kind)` names the choice.

Gatherings are everybody's free time. While a gathering of the calendar
(`economy.live.gatherings`) is on, `life.wants_leisure` is true for every
resident who is not busy, not cold without a coat and has every need above
`economy.live.gathering_needs_above` (50,000), whatever its schedule, and
`life.socialise` fails away from the square, so the `Leisure` branch walks
the resident to the square (`life.go_to centre`) and lets it meet people there.
A resident in need is not drawn away: it eats, drinks or warms up first.

Friends help in their free time. A resident at a bond tier marked `helps`
(`economy.live.tiers`) that is in its `Leisure` block, not busy, in the
settlement of a played character's cottage and above
`economy.live.help_needs_above` in every need helps that played character at
most once a day: a material for the open homestead upgrade, work on it, or a
ripe bed harvested (`VillageFriendHelped`). This is an effect of the live tick,
not a tree leaf: the help is instant and the resident does not walk to the
homestead.

Home leaves end indoors. `life.sleep`, `life.warm_up` and `life.go_to home`
walk to the door of the home the resident sleeps in; once there and standing,
the resident is inside (`village::view::inside`) until it walks off. Indoors
the home's hearth decides warmth and passers-by cannot meet the resident, so a
branch that sends a resident home also shelters it. A resident without a home
walks to the settlement centre instead and stays outdoors.

## Schedule

```json
"schedule": {
  "blocks": [
    {"kind": "work",    "from": 0,   "to": 700},
    {"kind": "meal",    "from": 250, "to": 300},
    {"kind": "meal",    "from": 600, "to": 650},
    {"kind": "leisure", "from": 620, "to": 700},
    {"kind": "sleep",   "from": 700, "to": 1000}
  ],
  "chronotype_shift": 25,
  "diligence_shift": 40,
  "meal_hunger_threshold": 60000,
  "chat_ticks": 200,
  "chat_break_mm": 3000,
  "meet_cooldown_ticks": 2400,
  "meet_affinity": 1500
}
```

`from` and `to` are permille of the day: 0 is dawn, 1000 the next dawn. The
shipped day has 24000 ticks and night falls at 700, so one permille is 24
ticks. `from` is included, `to` is not; `from` greater than `to` wraps past
dawn. Blocks may overlap and a kind may appear several times. Kinds are
`work`, `meal`, `leisure` and `sleep`; at least one `work` and one `sleep`
block are required.

Blocks do nothing by themselves. The `life.in_*_window` conditions,
`life.tired`, `life.bedtime`, `life.wants_meal` and `life.wants_leisure` read
them, and the tree decides what a window means.

- `chronotype_shift`: every block moves later by this many permille for a
  resident with chronotype 100, earlier for a negative chronotype.
- `diligence_shift`: leisure starts this many permille later at diligence 100
  and as much earlier at diligence 0. A delay as long as the block removes it.
- `meal_hunger_threshold`: during a meal block a resident carrying food eats
  below this Hunger, without waiting for `hunger_threshold`.
- `chat_ticks`: how long a meeting lasts; it pays its morale and affinity at
  the end. `chat_break_mm`: walking further apart than this ends it, unpaid.
- `meet_cooldown_ticks`: ticks after a meeting before a resident meets again
  (2400: a few meetings a day). `meet_affinity`: the affinity a finished
  meeting pays each party; couples form at `demography.pair_affinity`.

Cold is physical, not scheduled: nights and winter are cold whatever the
schedule says, so a resident whose work block reaches into the night needs a
coat.

## Traits

Each resident gets four integers when the world is founded, drawn from these
ranges with the world seed and never changed afterwards.

```json
"traits": {
  "diligence":   {"min": 20,   "max": 95},
  "sociability": {"min": 5,    "max": 95},
  "chronotype":  {"min": -100, "max": 100},
  "thrift":      {"min": 0,    "max": 100},
  "sociable_min": 35,
  "thrift_reserve_coins": 80
}
```

| Trait | Range | Effect in the shipped tree |
|---|---|---|
| Diligence | 0 to 100 | Delays the leisure block, so a diligent resident works longer. |
| Sociability | 0 to 100 | At `sociable_min` or more the resident takes leisure: meets neighbours or strolls to the square. Below it the resident spends its free time alone: it strolls, but nobody chats with it in its leisure, neither in passing nor by seeking it out. Not in the cold without a coat. |
| Chronotype | -100 to 100 | Shifts the whole schedule. Negative is an early riser. |
| Thrift | 0 to 100 | Keeps `thrift` x `thrift_reserve_coins` / 100 coins back from stocking provisions ahead of need. Buying when actually thirsty or hungry is never held back. |

Set `min` equal to `max` to give everybody the same value. Trait effects come
only from the conditions that read them, so a tree that never asks
`life.wants_leisure` ignores sociability.

## Places

`life.go_to` knows `home`, `work` and `centre` (the resident's settlement
centre). `behavior.places` adds named positions in millimetres. Several
entries may share a key; a resident walks to the nearest one.

## Thresholds: the `island_ai` block

```json
"island_ai": {
  "decision_ticks": 20,
  "critical_need": 15000,
  "thirst_threshold": 55000,
  "hunger_threshold": 50000,
  "recovery_threshold": 30000,
  "night_recovery_threshold": 85000,
  "goap": {"max_expansions": 128, "max_steps": 12,
           "buy_cost": 1, "gather_cost": 1, "craft_cost": 1}
}
```

Need values are remaining wellbeing from 0 to 100000. The conditions above
read these thresholds; an empty object selects the values shown. `routine` is
a leftover of the removed older profiles. The tree file orders the branches,
so a `routine` other than the default is refused.

## Decision records

Whenever a resident's running leaf changes, the journal gets one
`VillageDecision` effect. Nothing is written while the leaf stays the same,
and nothing at all while the journal is off. Records never influence the
simulation: the state hash is identical with the journal on or off.

```sh
cargo run --locked -p npcctl -- journal --scenario island:living_world --seed 7 --ticks 600 --kind Effect --unit 236
```

A record from seed 7, tick 21 (a Harbour carpenter; the nine false need and
schedule guards between `Critical hunger` and `Chores` are left out here):

```json
{"type": "VillageDecision",
 "tree": "island_life",
 "branch": "Resident life > Work hours > Profession > Pursue production goal",
 "node": 62,
 "action": "life.pursue_goal", "param": "",
 "activity": "working to a plan",
 "considered": [
   {"node": "Critical thirst", "rejected": "life.critical_thirst is false"},
   {"node": "Critical hunger", "rejected": "life.critical_hunger is false"},
   {"node": "Chores", "rejected": "life.collect_found: nothing lost within reach; life.equip: a coat is already worn or none is carried; life.needs_fuel is false; life.has_spare_coins holds, but life.buy provisions: nothing wanted is on offer at an affordable price; life.buy clothes: nothing wanted is on offer at an affordable price"},
   {"node": "Storm", "rejected": "life.storm is false"},
   {"node": "Leisure", "rejected": "life.wants_leisure is false"},
   {"node": "Consider a new job", "rejected": "life.change_job: no requested or better-paid vacancy"},
   {"node": "Build", "rejected": "life.build: no building commission and no supplied building site"},
   {"node": "Carry building materials", "rejected": "life.haul materials: no building site of this settlement needs materials this resident can bring"},
   {"node": "Harvest the field", "rejected": "life.harvest: only a farmer works the fields"},
   {"node": "Sow the field", "rejected": "life.sow: only a farmer works the fields"},
   {"node": "Tend the field", "rejected": "life.tend: only a farmer works the fields"}
 ],
 "plan": {"goal": "village_bowl", "steps": 1, "first": "craft village_carve", "cost": 428, "expanded": 2}}
```

- `branch` is the path of node names from the root to the running leaf and
  `node` its number in the tree file.
- `considered` lists, in priority order, every option that was passed over on
  the way: a guard whose condition is false, or a leaf with the reason it could
  not start. "X holds, but ..." means the guard passed and its branch failed.
  Conditions are read at the end of the tick the leaf started in.
- `plan` appears for `life.pursue_goal`: the GOAP goal, the number of steps,
  the first step, the plan cost and the states searched.
- `candidates` appears for `life.craft`: up to three ready recipes with their
  scores, best first.
- A `UtilitySelector` on the path lists every child with its `score`.

The read-only `village::resident::explain(world, entity)` returns the same
shape for the leaf running now, without reasons for failed leaves (those are
only known when the choice is made). `village::resident::label` gives the
short activity text the inspector shows; a resident who has reached home reads
"at home" or "asleep at home", not "going home", and a guest asleep in a
rented inn bed reads "asleep at the inn".

Views should use the structured form, `village::view::latest_decision(world,
entity)`. It returns the record written when the running leaf was chosen
(`recorded: true`, with its `tick`) when the journal was on, else the live
explanation. Each passed-over option carries `does_not_apply` (its guard is
false) instead of a sentence; an option that applied and could not start has
`does_not_apply: false` and, in a recorded decision, a `reason`.

The generic journal also has `LeafStart`, `LeafStop` and `Abort` entries, and
`CondEval` for every guard when the journal is verbose. Each leaf that was
tried and could not start is a `LeafStart` with status `Failure`, about ten per
decision, so filter by kind (`--kind Effect`) when reading decisions; the
journal is a ring buffer of 50,000 entries.

## Worked example: an evening walk to the pier

Goal: sociable residents walk to the pier in the early evening instead of
strolling to the square.

1. Name the place. In the world file:

   ```json
   "behavior": {
     "places": [{"key": "pier", "position": {"x": 500, "z": -43000}}]
   }
   ```

2. Add the branch to `trees/resident.json`, just above the `Leisure` branch,
   so it is tried first:

   ```json
   {"name": "Evening walk",
    "kind": {"Guard": {"cond": "life.wants_leisure", "abort": "None"}},
    "children": [
      {"name": "Walk to the pier",
       "kind": {"Leaf": {"action": "life.go_to", "param": {"Name": "pier"}}}}
    ]},
   ```

   `life.wants_leisure` covers nothing, so the guard uses `"abort": "None"`.
   With `"LowerPriority"` the loader would refuse the file and say why. When
   the way to the pier is blocked the leaf fails and the `Leisure` branch
   below takes over.

3. Start a new world. In the evening the inspector shows "taking a walk" and
   the journal has records whose `branch` is
   `Resident life > Evening walk > Walk to the pier`. Residents below
   `sociable_min` never take the branch.

The test `an_evening_walk_added_in_json_is_walked_by_sociable_residents` in
`tests/island_resident_trees.rs` makes exactly this edit, with an early
leisure block so it runs in seconds, and checks the walk, the arrival, the
record and that unsociable residents stay away.

More ideas that need only JSON:

- Lunch at home: a `life.in_meal_window` guard above `Work hours` leading to
  `life.go_to home`.
- No job-hopping: delete the `Consider a new job` leaf.
- Sellers who deliver: add a `life.sell surplus` leaf to `Profession`.
- Builders first: guard a `life.build` leaf with `life.site_needs_builder` above
  `Chores`, so a supplied site is finished before anything else.
- Shift work: two `work` blocks and a `sleep` block that wraps past dawn.

## How GOAP fits in

`life.pursue_goal` is the planner. The production ledger chooses funded goals
for each profession; the planner searches over carried goods, remaining seller
and patch stock, coins and position for the cheapest sequence of buy, gather
and craft steps that makes one more unit. Only the first step is executed,
through the ordinary validated action, and the plan is cached for up to 600
ticks while its first step stays valid. Search is bounded by
`island_ai.goap`: 8 to 512 expanded states and 1 to 24 steps; the three cost
multipliers change preference without changing durations or prices. When the
search finds nothing the leaf fails and the tree tries `life.craft`,
`life.hunt`, `life.buy inputs` and `life.gather outputs` in turn, so a search
budget never turns a viable worker into an idle one.

## What is still code

- What each action does, and each condition tests. A new action or condition
  is a Rust function plus one entry in `village/resident/actions.rs` or
  `conditions.rs`.
- The production ledger that funds goals, and the GOAP search itself.
- Needs decay, exhaustion, cold and death (`village/ai.rs`, `living.rs`).

## Tests

```text
cargo test --locked -p island_content --test island_resident_trees
cargo test --locked -p island_content --test island_resident_traits
cargo test --locked -p island_content --test island_decision_records
```

`island_resident_trees` covers the shipped file, refusal messages, hashing,
the construction copy and the evening walk. `island_resident_traits` runs the
same world twice with one trait of one resident changed and checks that the
resident behaves differently. `island_decision_records` checks the record
contents, the rate limit and that the journal does not change the state hash.

## Conditions with parameters, `not`, and a tree per resident

A guard may test with parameters, or negate any condition (review fix pass, S3.1):

```json
{"Guard": {"if": "need_below", "need": "Hunger", "value": 40000, "not": false, "abort": "None"}}
{"Guard": {"if": "life.night", "not": true, "abort": "None"}}
{"Guard": {"if": "life.player_within", "value": 6, "abort": "None"}}
```

| `if` | Parameters | True when |
|---|---|---|
| `need_below` | `need` (`Hunger`, `Thirst`, `Energy`, `Warmth`), `value` 0..=100000 | the need is below the value |
| `has_good` | `good` (a good key or a tag), `value` (count, default 1) | the pack holds at least that many |
| `is_trade` | `trade` (a profession key) | the resident works that trade |
| `season` | `season` | it is that season |
| `trait_at_least` | `trait` (`diligence`, `sociability`, `thrift`), `value` 0..=100 | the trait is at least the value |
| `in_block` | `block` (`work`, `meal`, `leisure`, `sleep`) | the resident's shifted schedule is in that block |
| `life.player_within` | `value` in metres, 1..=100 | the played character has arrived, is alive and is within that distance (integer squared distance; one index lookup, no scan) |
| any registered condition | `not: true` | the condition is false |

The loader turns each into a plain registered name (`need_below:Hunger:40000`,
`not:life.night`, `life.player_within:6`); a world may use at most 64 distinct ones. A `not` guard, and any
guard with parameters, must not abort lower-priority work.

Which tree a resident runs is data: `behavior.stage_trees` (`child`, `elder`) and a
profession's `tree`; the order is child, trade, elder, then `island_life`. Any tree
name is allowed; every named tree must exist. `ReplaceBehavior` replaces the whole
behaviour block at a tick boundary (see `village::resident::replace`).
