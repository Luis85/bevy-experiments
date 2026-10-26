---
name: play-actor
description: Control one actor of a running npc-framework world as an external agent, in real time, until a goal is reached - start or attach to a session, loop observe → act (queue) → wait for events with npcctl, then record and verify the session. Use when asked to play, drive or test a unit as an agent ("get the key and open the store with unit 33"), or to run an agent experiment.
argument-hint: "<unit id> <goal> [scenario, default smithy] [--url URL]"
---

# Play an actor: $ARGUMENTS

Arguments: unit id `UNIT`, the goal in words `GOAL`, optionally a scenario (`SCENARIO`, default `smithy`; any
embedded testbed name or a scenario file) and a `--url` of an already running session or game. Guide:
`docs/agent-experiments.md`; method shapes: `docs/debugging.md` §4.

The world runs in real time while you think: plan a few steps ahead with `--queue`, and wait with long polls,
never with `sleep` loops.

## 1. Start or attach

```sh
cargo build -q -p npcctl                       # then N=target/debug/npcctl (npcctl.exe on Windows)
N=target/debug/npcctl
```

- No `--url` given: start a headless session **in the background** (`run_in_background`) with a record, and read
  its first stdout line for the URL:
  `$N serve --scenario SCENARIO --port 0 --record target/play/session.json`
  → `{"listening":"http://127.0.0.1:PORT",..}`. Use `--url http://127.0.0.1:PORT` on every call below.
- A windowed testbed (`cargo run -p actor_testbed -- --scenario SCENARIO --remote`) or another running server:
  use its URL (default `http://127.0.0.1:15702`); `npcctl session info` fails there (BRP has no session methods),
  `npcctl agent list-units` works.

```sh
$N agent list-units --url URL --kind actor     # is UNIT controllable? who controls it?
$N agent attach --url URL --unit UNIT --controller claude --mode direct      # --force if another controller holds it
```

Use `--mode advisor` when the goal is about ordering other units (the unit's own tree keeps running). Note
`applies_at`: the attach takes effect at that tick.

## 2. The loop

Repeat until a stop condition (§3):

1. **Observe:** `$N agent observe --url URL --unit UNIT --controller claude --format text --brief`. Note `tick`
   and `seq` (second line), what you perceive and remember (names and positions), what you carry, and the "You
   can:" list (each `[id]`, `-- unavailable: why`, odds like `[Mining 13 vs 3d6: ~83%]`). Only things the unit
   perceives, remembers or carries are offered: if the target is not listed, look around (move) first, or act by
   its id from the map (`docs/agent-experiments.md` §1 lists the testbed ids).
2. **Decide** 1–3 steps toward GOAL. Prefer an offered affordance id; fix a `why_not` first ("too far" → walk;
   "door is locked: needs key:store" → get the key; "needs tool:pickaxe" / "equip tool:pickaxe first" → pick up,
   then `equip@ID`).
   - Walk to a target and use it, in one act: `--interact pick_up@ID` (any verb: `unlock@12`,
     `use_tool_on@24`).
   - Walk somewhere (Direct): `--steps '[{"verb":"order","args":{"order":{"type":"Move","pos":[x,z]}}},
     {"verb":"execute_orders"}]'` (atomic: both or neither). Chain orders with `"queue":true` *inside* the order's
     args (the CLI's `--queue` is the action queue); a failed order does not stop the ones queued behind it.
   - Village: `--verb walk_to --pos x,z`, `--affordance eat@3`, `sleep`, `work`, `wait`.
3. **Act:** `$N agent act --url URL --unit UNIT --controller claude --affordance ID --expect-tick T --expect-seq S`
   (first step without `--queue`, later steps with it). Exit 1 = rejected: read `reason` / `message`, re-observe,
   adjust. `stale` = re-observe (the message lists what changed; the tick bound is 600 ticks = 30 s); do not
   blindly add `--allow-stale`.
4. **Wait:** `$N agent events --url URL --unit UNIT --since-seq S --until-idle --dedupe` (returns when the unit
   finished everything you gave it; 30 s default timeout), or `--kinds OrderDone,OrderFailed,Interacted,CraftDone,CraftFailed
   --timeout-ms 20000` for the first matching event. Keep `next_seq` as the next `--since-seq`; if `epoch`
   changed, the world was reset: start again from `--since-seq 0`. `timed_out: true` → re-observe (still walking?
   stuck? `busy` says what is still running).
5. Keep a short log for the report: tick, what you did, the result.

## 3. Stop conditions

- **Done:** the observation (or `world/state`, `npc/unit`) shows the goal state (carrying the item, door open,
  `CraftDone`, standing at the place). Confirm with one more observe.
- **Impossible:** the same rejection three times with no new information, or a goal the affordances can never
  offer (wrong faction: "owned by player 2"). Stop and explain.
- **Budget:** about 30 loop iterations or 10 minutes of wall time, unless the user gave another limit.
- Never edit files to make the unit succeed, never use `world.mutate_*`/`npc/command` to bypass validation.

## 4. Record and verify

```sh
$N agent stats --url URL                         # latency (ticks, ms), accepted / rejected / stale, before stopping
$N agent detach --url URL --unit UNIT --controller claude
$N session stop --url URL                        # only for a session you started: writes target/play/session.json
$N replay verify-session target/play/session.json   # must print {"ok":true,..}
```

Journal of the run (optional): extract `replay` from the record and re-run the scenario with it
(`docs/agent-experiments.md` §7): `$N journal --scenario S --commands target/play/replay.json --ticks END
--unit UNIT --kind ExternalAction,OrderDone,OrderFailed,Interacted,ItemMoved`, where `S` is `testbed:NAME` for an
embedded testbed scenario, or the same scenario file the session was served with (a village file, a custom
testbed file).

## 5. Report

- Goal reached or not, in one sentence, with the final tick.
- The steps that mattered (tick, action, outcome) and every rejection with its reason.
- `agent stats` (decisions, rejected, stale, latency p50/p95) and the `verify-session` result.
- Friction worth fixing in the framework or the docs (an unclear `why_not`, a missing affordance).
