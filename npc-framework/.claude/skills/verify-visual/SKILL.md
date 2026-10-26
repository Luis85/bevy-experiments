---
name: verify-visual
description: Check what a windowed app (the village, the Actor Testbed or the RTS units demo) actually renders by taking a screenshot at a fixed sim tick and reading the PNG against a checklist. Use after any change to visuals, animation, lighting, camera, overlay, panels or scene layout, or when text state alone cannot prove something is visible.
argument-hint: "[tick] [seed] [app: village|actor_testbed|rts_units_demo]"
---

# Verify visually

Arguments: $ARGUMENTS (target tick, default 1200; seed, default 1; app, default `village`). Below, `TICK`,
`SEED` and `APP` stand for them. The other apps:

| App | Quick capture | Notes |
|---|---|---|
| `actor_testbed` | `cargo run -p actor_testbed -- --scenario testbed --select 38 --panels sheet,perception --screenshot "$PWD/target/shots/tb.png" --after-secs 8` | `--camera X,Z,DIST[,YAW,PITCH]`, `--paused`; BRP `npc/screenshot` and `npc/step` work as below; cross-check with `npcctl inspect --scenario testbed --tick TICK --unit 38` |
| `rts_units_demo` | `cargo run -p rts_units_demo -- --screenshot "$PWD/target/shots/rts.png" --after-secs 18` | no BRP: time-based only; `--headless-ticks N` for the state hash |

Text state (`npcctl`, BRP) is the primary oracle; the screenshot proves what a player would see.
Write PNGs under `target/shots/` (gitignored). Never commit screenshots as goldens without user approval.

## 1. Build (slow the first time: minutes)

```sh
cargo build -p village
```

## 2a. Quick capture (time-based; enough for "does it render at all")

```sh
mkdir -p target/shots
cargo run -p village -- --seed SEED --screenshot "$PWD/target/shots/quick.png" --after-secs 8
```

## 2b. Capture at an exact tick (deterministic sim state)

Start the game with `run_in_background`, then drive it over BRP (default port 15702; on in dev builds, add
`--remote` to a release build). `npc/screenshot` paths are relative to `target/npc-out/` (the app's `--out-dir`):

```sh
cargo run -p village -- --seed SEED
```

```sh
URL=http://127.0.0.1:15702/
until curl -s -X POST $URL -d '{"jsonrpc":"2.0","id":1,"method":"npc/methods"}' >/dev/null; do sleep 2; done
curl -s -X POST $URL -d '{"jsonrpc":"2.0","id":1,"method":"npc/pause"}'            # result.tick = T
curl -s -X POST $URL -d '{"jsonrpc":"2.0","id":1,"method":"npc/step","params":{"ticks":STEPS}}'  # STEPS = TICK - T
curl -s -X POST $URL -d '{"jsonrpc":"2.0","id":1,"method":"npc/unit","params":{"id":5}}'
curl -s -X POST $URL -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"npc/screenshot","params":{"path":"shots/tick_TICK"}}'
until [ -s target/npc-out/shots/tick_TICK.png ]; do sleep 1; done                # lands 1-2 frames later
```

Stop the background game afterwards. Cross-check headless:
`cargo run -p npcctl -- inspect --seed SEED --tick TICK --unit 5` must match `npc/unit` (same seed, no commands).

## 3. Read the PNG and check

Open it with the Read tool. Report each item (yes/no + what you see):

- [ ] Scene: ground tile, house, bed, stockpile, field and props present; no pink/missing textures, no
      black frame (a minimized window can give black).
- [ ] Villager visible, where the sim says (near the place of his current activity).
- [ ] Animation matches `Activity` (walk / work chop / eat / lie down / idle).
- [ ] Lighting matches the clock: night = last 30 % of the 2400-tick day (dimmer but readable).
- [ ] Your change is visible and looks as intended; nothing else changed.
- [ ] Overlay/panels (F1/F3/F4; testbed F5/F6) if you changed them: `--panels journal,perf` (testbed also
      `sheet,perception`) opens them at start; the F1 overlay needs a key press (say if not checked).
- [ ] Testbed: doors match `Door.open` (`npcctl run --scenario testbed --ticks TICK` → `content_summary.doors`),
      units carry what `content_summary.units[].carries` says, riders sit on their vehicle.

## 4. Gate

- [ ] Report the PNG path and the checklist.
- [ ] `cargo xtask verify` → `"ok":true` (view changes must not change any replay hash).
