"""Native captures of the seventh UI pass: the hired builder seen at work on
the homestead card (the clock stepped in small bounded steps until the
card says so), an alert's tooltip beside the alerts list, and every check
of the sixth pass again. Each at 1280x720 and 1024x768.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p island_content --test island_capture_fixtures -- --ignored --test-threads 6
  cargo build --locked -p island_game
  python scripts/playtest/capture_ui_live7.py [--only settings,worlds,summary,upgrade,builder,alerts]

PNGs and summary.json go to target/npc-out/ui-live7/.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import time
import uuid

os.environ["ISLAND_TIPS_SEEN"] = "all"

import capture_player_flow  # noqa: E402
from capture_native import ROOT  # noqa: E402
from capture_native_homestead import file_hash  # noqa: E402
from capture_player_flow import ALERTS, wait_for  # noqa: E402
from capture_ui_live3 import FIXTURES  # noqa: E402
from capture_ui_live6 import (Live6Flow, overlaps, settings, start, summary,  # noqa: E402
                              upgrade, worlds)
from capture_ui_phase45 import WORKSPACE  # noqa: E402
from capture_ui_redesign import SIZES, stop  # noqa: E402

capture_player_flow.SIZES = SIZES


class Live7Flow(Live6Flow):
    def snap(self, name, size):
        return super().snap(name.replace("live6-", "live7-", 1).replace("live5-", "live7-", 1), size)

    def clock_tick(self):
        return int(self.state()["tick"])


BUILDING = "is building at your door"


def builder(flow, report, checks):
    """From the save where the rest is ordered and a builder hired: time runs
    fast until the materials are in, then the clock is stepped 20 ticks at a
    time (one stage at the door lasts about 180) until the card says the
    builder is at work."""
    flow.call("npc/pause")
    flow.workspace("KeyB")
    card = lambda: flow.panel_texts(WORKSPACE)  # noqa: E731
    flow.call("npc/resume")
    for _ in range(3):
        flow.key("Equal")
        time.sleep(0.2)
    try:
        wait_for(lambda: any("The materials are in" in text for text in card()) or None, 300)
    except RuntimeError:
        pass
    flow.call("npc/pause")
    report["builder_supplied_tick"] = flow.clock_tick()
    seen = []
    found = None
    # At most two island days of 20-tick steps.
    for _ in range(2 * 24_000 // 20):
        flow.call("npc/step", {"ticks": 20})
        # Long enough for the card's rows to be built again from the new tick.
        time.sleep(0.35)
        texts = card()
        left = [text for text in texts if "of work left" in text]
        report.setdefault("builder_samples", []).append(
            [flow.clock_tick(), left[:1], [line for text in texts for line in text.splitlines()
                                           if "Jun" in line or "builder" in line][:2]])
        status = [line for text in texts for line in text.splitlines()
                  if any(word in line for word in (BUILDING, "on the way to work", "is hired and will come"))]
        state = ("working" if any(BUILDING in text for text in texts)
                 else "walking" if any("on the way to work" in text for text in texts)
                 else "hired" if any("is hired and will come" in text for text in texts)
                 else "none")
        if not seen or seen[-1]["state"] != state:
            seen.append({"tick": flow.clock_tick(), "state": state, "texts": status[:2]})
        if state == "working":
            found = {"tick": flow.clock_tick(), "state": state,
                     "line": [text for text in texts if BUILDING in text]}
            break
        if any("Built: a larger cottage" in text for text in texts):
            break
    report["builder_states"] = seen
    report["builder_at_work"] = found
    checks.append({"check": "the card names the hired builder at work",
                   "ok": found is not None, "seen": found, "states": seen[-6:]})
    if found:
        flow.shots("live7-builder-at-work", settle=0.3)


def alerts(flow, report, checks):
    """The first alert hovered: its tooltip stands beside the list, clear of
    it and inside the window, at both sizes."""
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.2)
    if flow.has_widget("Show"):
        flow.click("Show")
        time.sleep(0.6)
    report["tips_in_oversee"] = len(flow.tips_shown())
    checks.append({"check": "with ISLAND_TIPS_SEEN=all no first-run tip shows",
                   "ok": report["tips_in_oversee"] == 0})

    def rows():
        return [(entity, text) for entity, text in flow.button_rows(ALERTS)
                if text.splitlines()[:1] != [] and text.splitlines()[0] not in ("Show", "Hide", "")
                and "journal" not in text]

    seen = {}
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.6)
        flow.park(size)
        found = rows()
        lists = flow.names().get("Game: " + ALERTS, []) or flow.names().get(ALERTS, [])
        list_rect = flow.rect(lists[0]) if lists else None
        key = f"{size[0]}x{size[1]}"
        if len(found) < 2 or list_rect is None:
            seen[key] = {"rows": [text for _, text in found], "list": list_rect}
            continue
        first, second = flow.rect(found[0][0]), flow.rect(found[1][0])
        flow.cursor((first[0] + first[2]) / 2, (first[1] + first[3]) / 2)
        cards = wait_for(lambda: flow.tooltips_inside(size) or None, 6) or []
        time.sleep(0.5)
        flow.snap(f"live7-alert-tooltip-{key}", size)
        clear = bool(cards) and all(
            not overlaps(card["rect"], list_rect) and not overlaps(card["rect"], second)
            and card["inside"] for card in cards)
        seen[key] = {"rows": [text for _, text in found][:4], "list": list_rect, "first_row": first,
                     "next_row": second, "tooltips": cards, "clear": clear}
        flow.park(size)
    flow.settle(SIZES[0])
    report["alert_tooltips"] = seen
    checks.append({"check": "an alert's tooltip stands beside the list, not over the next alert",
                   "ok": len(seen) == len(SIZES) and all(item.get("clear") for item in seen.values()),
                   "seen": seen})


PARTS = {"settings": ("cottage-evening", [settings]),
         "worlds": ("cottage-evening", [worlds]),
         "summary": ("cottage-evening", [summary]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "builder": ("upgrade-underway", [builder]),
         "alerts": ("village-alerts", [alerts])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live7")
    parser.add_argument("--only", default="settings,worlds,summary,upgrade,builder,alerts")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live7-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor and "
                       "MouseButtonInput; the simulation clock run, paused, sped up or stepped "
                       "20 ticks at a time; prepared saves copied in; ISLAND_TIPS_SEEN=all",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            try:
                run, child, flow = start(binary, base, name, report, fixture)
            except RuntimeError as error:
                report["checks"].append({"check": f"section {name} started", "ok": False,
                                         "reason": str(error)})
                continue
            flow.__class__ = Live7Flow
            try:
                if name != "settings":
                    flow.enter_live()
                for step in steps:
                    flow.section(step.__name__, lambda step=step: step(flow, report, report["checks"]))
                    flow.section(step.__name__ + " cleanup", lambda: flow.settle(SIZES[0]))
            finally:
                stop(child, run, report, name)
                if (run / "shots").is_dir():
                    for image in sorted((run / "shots").glob("*.png")):
                        shutil.copy2(image, args.out / image.name)
        report["ok"] = all(check["ok"] for check in report["checks"])
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        report["copied"] = sorted(str(path) for path in args.out.glob("*.png"))
        (args.out / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                               encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
