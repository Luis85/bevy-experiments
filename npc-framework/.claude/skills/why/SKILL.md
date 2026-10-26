---
name: why
description: Explain why a unit did (or did not do) something in a tick range, from the decision journal and an inspect snapshot, ending in a one-paragraph explanation. Use for questions like "why did unit 5 stop working at tick 3000" or "why doesn't the villager eat".
argument-hint: "<unit id> <from tick> [to tick] [seed]"
---

# Why

Arguments: $ARGUMENTS (unit id, from tick, to tick (default from + 600), seed (default 1)). Below, `UNIT`,
`FROM`, `TO`, `SEED` stand for them; add `--commands FILE` / `--villagers K` if the run used them.

Answer from evidence (journal + snapshot), not from reading the tree code alone. `END` = `TO` + 1.

## 1. Decisions in the range

```sh
cargo run -q -p npcctl -- journal --seed SEED --ticks END --unit UNIT --from FROM --to TO \
  --kind ThinkStart,LeafStart,LeafStop,Abort,NeedLevel,Reserve,Release,Effect,Command,PhaseChange --limit 200
# guard results (CondEval exists only in a verbose journal):
cargo run -q -p npcctl -- journal --seed SEED --ticks END --unit UNIT --from FROM --to TO --kind CondEval --verbose --limit 200
```

## 2. State at the key tick

```sh
cargo run -q -p npcctl -- inspect --seed SEED --tick T --unit UNIT --pretty
```

Read: needs and their levels, `Activity`, tree path (node names), blackboard target, active leaf and
`started_tick`, held reservations, current order. If a place matters, `inspect --all` for stockpile food
and reservations.

Live game instead (BRP, `docs/debugging.md` §3; the windowed apps keep the journal on, `--journal FILE` also
streams it to disk): `npc/journal` with params `{"unit":UNIT,"from":FROM,"to":TO,"kinds":[],"limit":200}` and
`npc/unit` with `{"id":UNIT}`.

Testbed or another content: add `--scenario testbed[:<name>]` (and the run's `--commands`) to every command, and
the capability kinds to `--kind`: `OrderIssued,OrderStarted,OrderDone,OrderFailed,Interacted,ItemMoved,DoorChanged,
Roll,Perceived,Lost,PathFound,PathFailed,CraftStarted,CraftDone,CraftFailed,Mounted,Dismounted,VehicleStalled,
XpGained,LevelUp,SkillImproved,ExternalAction,ActionRejected` (`docs/debugging.md` §5 explains each).
`inspect` then prints every registered component by name (`order_queue`, `perception_memory`, `inventory`,
`character_sheet`, ..).

## 3. Reason it out

- Which branch ran (`LeafStart` path), and what started it (`ThinkStart` reason: LeafDone / Request / NoLeaf)?
- Which higher-priority guards were false (`CondEval`), and why: need value vs threshold (`NeedDef.thresholds`,
  hysteresis), night/day, no free target (`Reserve ok:false`), empty stockpile.
- Was something aborted (`Abort { guard, aborted }`), and by which guard?
- The villager tree order is in `README.md` ("Villager tree") and `villager_tree` in
  `content/village_content/src/villager.rs`; the testbed trees in `content/testbed_content/src/tree.rs`.
- Capability units: was there an order (`OrderStarted`) and how did it end (`OrderFailed.reason`)? A refused
  verb is `Interacted {outcome: "rejected", reason}`, a failed check a `Roll {success: false, purpose}`, a lost
  target `Lost`, an unreachable goal `PathFailed`. An agent-controlled unit (`ControllerAttached`) runs its
  `ExternalAction` queue instead of its own tree.

## 4. Answer

One paragraph: what the unit did in the range, the decisive condition with numbers (e.g. "hunger 18 000 <
urgent 20 000 and stockpile food 0, so UrgentEat failed and Forage won"), and what would have changed the
outcome. Quote the 2–5 journal lines that prove it (tick, kind, node).

If it is a bug, propose a scenario test; the fix follows `/add-mechanic` and ends with `cargo xtask verify`.
