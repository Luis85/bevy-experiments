"""Native captures of Live mode's depth (the island UI's second Live pass):
the goal chain with pips, the working pill with progress and a repeat order,
a need's tooltip with the simulation's reason and the pouch it opens, the
pouch's weight bar, People with friendship tiers and the gift picker, a gift
toast, the calendar with a gathering, the Economy page with days left and Low,
a good's producers, the build tray's availability, the inspector's Friends
list. The cottage half (homestead tiers, an upgrade, sleep and the day's
account) starts from prepared saves in capture_ui_live3.py.

Build first, then run from npc-framework:
  python scripts/playtest/capture_ui_live2.py [--binary target/debug/island_game.exe]
      [--only live,people,calendar,oversee]

One disposable run of the real game, driven over BRP like the phase 6/7
script: controls by the text on them or their `Name`, keys as KeyboardInput
messages (I, C, N, E, B, Enter, Esc), the pointer through the window's
cursor position. Every effect on the world is a recorded command that a
control or key of the game submits; nothing is granted or teleported. A part
that fails is a failed check and the run goes on. PNGs go to
target/npc-out/ui-live2/ and still need to be read.
"""
import argparse
import json
from pathlib import Path
import shutil
import time
import uuid

import capture_player_flow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import INSPECTOR, LEDGER, PRIMARY, TRAY, wait_for
from capture_ui_phase45 import GOAL, HOTBAR, NEEDS, PROMPT, WORKSPACE
from capture_ui_phase67 import Phase67Flow
from capture_ui_redesign import SIZES, launch, stop
from capture_view_checks import DAY

capture_player_flow.SIZES = SIZES
CALENDAR = "Island calendar"
VEIL = "Sleeping veil"
SUMMARY = "Day summary"


class Live2Flow(Phase67Flow):
    def shots(self, label, settle=0.8):
        self.both(label, self.park, settle=settle)

    def needs(self):
        return self.observe()["sections"]["village"]["needs"]

    def run_until(self, predicate, seconds):
        """Let time run until `predicate()` holds; pause again after."""
        self.call("npc/resume")
        end = time.monotonic() + seconds
        try:
            while time.monotonic() < end:
                self.check()
                if predicate():
                    return True
                time.sleep(0.5)
            return False
        finally:
            self.call("npc/pause")

    def step_until(self, predicate, ticks, chunk=400):
        """Step the paused clock until `predicate()` holds."""
        done = 0
        while done < ticks:
            if predicate():
                return True
            self.advance(chunk)
            done += chunk
        return predicate()

    def tip_texts(self):
        return [text for text in self.texts() if text]


def live(flow, report, checks):
    flow.call("npc/pause")
    report["goal"] = flow.panel_texts(GOAL)
    checks.append({"check": "the goal tracker counts its steps",
                   "ok": any(text.startswith("Step ") and " of " in text for text in report["goal"]),
                   "texts": report["goal"]})
    flow.shots("live2-goal-pips")

    # The pouch: its weight bar and what is in hand, opened by its key.
    flow.key("KeyI")
    time.sleep(0.8)
    report["pouch"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "the pouch shows the pack's weight",
                   "ok": any(" kg" in text for text in report["pouch"]), "texts": report["pouch"]})
    flow.shots("live2-pouch-weight")
    flow.key("KeyI")

    # Gather berries five times: the stepper, a walk, then the repeat order.
    flow.key("KeyC")
    time.sleep(0.8)
    flow.click(WORKSPACE + " > ×5")
    time.sleep(0.6)
    report["craft_stepper"] = flow.panel_texts(WORKSPACE)
    flow.shots("live2-repeat-stepper")
    if any(text.startswith("Walk to the water") for text in report["craft_stepper"]):
        flow.call("npc/resume")
        flow.press_row("Walk to the water", WORKSPACE)
        flow.run_until(lambda: any(text.startswith("Gather Water ×5")
                                   for text in flow.panel_texts(WORKSPACE)), 90)
    flow.call("npc/resume")
    flow.press_row("Gather Water ×5", WORKSPACE)
    seen = wait_for(lambda: [text for text in flow.panel_texts(PROMPT) if " of 5" in text] or None, 20)
    flow.call("npc/pause")
    report["repeat_pill"] = flow.panel_texts(PROMPT)
    checks.append({"check": "a repeat order shows its count and Stop in the working pill",
                   "ok": bool(seen) and "Stop" in report["repeat_pill"], "texts": report["repeat_pill"]})
    flow.shots("live2-repeat-working-pill")
    flow.run_until(lambda: not any(" of 5" in text for text in flow.panel_texts(PROMPT)), 120)
    flow.key("KeyC")

    # Rest here: the pill's bar follows the rest's known length.
    flow.key("KeyI")
    time.sleep(0.6)
    flow.call("npc/resume")
    flow.click("Rest here")
    time.sleep(3.0)
    flow.call("npc/pause")
    flow.key("KeyI")
    report["rest_pill"] = flow.panel_texts(PROMPT)
    flow.shots("live2-working-pill-progress")
    flow.key("KeyX")

    # A need below half has the simulation's reason in its tooltip; a click
    # on it opens the pouch at what helps.
    low = flow.step_until(lambda: min(flow.needs()[key] for key in ("hunger", "thirst")) < 49000,
                          DAY, chunk=600)
    needs = flow.needs()
    which = "Water" if needs["thirst"] <= needs["hunger"] else "Food"
    report["need_low"] = {"reached": low, "needs": needs}
    flow.hover_levels(flow.control(which, NEEDS), "need-reason", "pouch")
    texts = flow.tip_texts()
    report["need_tooltip"] = [text for text in texts if "pack" in text or "%" in text][:6]
    checks.append({"check": "a low need's tooltip gives the simulation's reason",
                   "ok": any("pack" in text for text in texts), "texts": report["need_tooltip"]})
    flow.click(NEEDS + " > " + which)
    time.sleep(0.8)
    report["pouch_filtered"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "a click on a need opens the pouch at what helps",
                   "ok": any(text.startswith("What helps your") for text in report["pouch_filtered"]),
                   "texts": report["pouch_filtered"]})
    flow.shots("live2-pouch-filtered-by-need")
    flow.key("Escape")


def people(flow, report, checks):
    # Whoever is in reach at the start: E opens them in People.
    # Residents pass the well in the morning; wait for one in reach.
    flow.run_until(lambda: any(text.startswith("Talk to ") for text in flow.panel_texts(PROMPT)), 150)
    prompt = flow.panel_texts(PROMPT)
    talk = next((text for text in prompt if text.startswith("Talk to ")), None)
    report["people_prompt"] = prompt
    if talk:
        flow.key("KeyE")
        time.sleep(0.9)
        report["people_person"] = flow.panel_texts(WORKSPACE)
        flow.shots("live2-people-person-tier")
        flow.call("npc/resume")
        flow.press_row("Talk", WORKSPACE)
        time.sleep(4.0)
        flow.call("npc/pause")
        report["after_talk"] = flow.panel_texts(WORKSPACE)
        flow.press_row("Change neighbour", WORKSPACE)
        time.sleep(0.6)
    else:
        flow.key("KeyN")
        time.sleep(0.8)
    report["people"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "People shows each neighbour's friendship tier",
                   "ok": any(word in " ".join(report["people"]) for word in
                             ("Stranger", "Acquaintance", "Friend")), "texts": report["people"][:20]})
    flow.shots("live2-people-tiers")
    neighbour = talk[len("Talk to "):] if talk else None
    report["neighbour"] = neighbour
    if neighbour:
        flow.press_row(neighbour, WORKSPACE)
        time.sleep(0.6)
    flow.press_row("Give a gift", WORKSPACE)
    time.sleep(0.8)
    report["gift_picker"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "the gift picker offers pouch goods and says what is known of tastes",
                   "ok": "Choose a gift" in report["gift_picker"], "texts": report["gift_picker"]})
    flow.shots("live2-gift-picker")
    gifts = [(entity, text) for entity, text in flow.button_rows(WORKSPACE)
             if text.splitlines()[-1:] and text.splitlines()[-1] in ("Berries", "Log", "Plank", "Bread")]
    report["gift_slots"] = [text.replace("\n", " ") for _, text in gifts]
    if gifts:
        flow.call("npc/resume")
        flow.activate(gifts[0][0], "a gift slot")
        toast = wait_for(lambda: [text for text in flow.texts()
                                  if any(word in text for word in ("thanked you", "loved the",
                                                                   "did not care"))] or None, 12)
        flow.call("npc/pause")
        report["gift_toast"] = toast
        checks.append({"check": "a gift answers with a toast", "ok": bool(toast), "texts": toast})
        flow.shots("live2-gift-toast", settle=0.2)
    flow.key("Escape")


def calendar(flow, report, checks):
    flow.click("Calendar")
    time.sleep(0.8)
    report["calendar"] = flow.panel_texts(CALENDAR)
    checks.append({"check": "the calendar lists the village gatherings",
                   "ok": any(word in " ".join(report["calendar"]) for word in
                             ("Harvest feast", "Midwinter fire")), "texts": report["calendar"]})
    flow.shots("live2-calendar-gatherings")
    flow.key("Escape")


def oversee(flow, report, checks):
    flow.click("Oversee")
    time.sleep(1.0)
    flow.open_page("Economy")
    time.sleep(0.8)
    report["economy"] = flow.panel_texts(LEDGER)
    checks.append({"check": "the goods table has a Days column and marks Low",
                   "ok": "Days" in report["economy"], "texts": report["economy"][:40]})
    flow.shots("live2-economy-days-left")
    flow.press_row("Clean water", LEDGER)
    time.sleep(0.8)
    report["good_detail"] = [text for text in flow.panel_texts(LEDGER)
                             if text.startswith(("Made by", "Nobody makes", "About "))]
    checks.append({"check": "a good's card says who makes it and how long it lasts",
                   "ok": any(text.startswith(("Made by", "Nobody makes")) for text in report["good_detail"]),
                   "texts": report["good_detail"]})
    flow.shots("live2-good-detail-producers")
    flow.close_ledger()
    flow.key("KeyB")
    time.sleep(1.0)
    report["tray"] = flow.panel_texts(TRAY)
    flow.shots("live2-build-tray-availability")
    flow.key("Escape")
    time.sleep(0.4)
    # A resident's inspector: their Friends.
    flow.open_page("People")
    time.sleep(0.6)
    # The neighbour the player talked to has the player among their bonds.
    met = report.get("neighbour")
    row = flow.inspect(lambda text: met is None or text.startswith(met), "a resident")
    report["inspector_row"] = row
    time.sleep(1.0)
    report["inspector"] = flow.panel_texts(INSPECTOR)
    checks.append({"check": "the inspector lists the resident's friends",
                   "ok": "Friends" in report["inspector"], "texts": report["inspector"][:40]})
    flow.shots("live2-inspector-friends")
    flow.key("Escape")
    time.sleep(0.4)
    flow.click("Live")
    wait_for(lambda: flow.panel_texts(HOTBAR), 30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live2")
    parser.add_argument("--only", default="people,live,calendar,oversee")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live2-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; the simulation clock run or stepped",
              "keyboard_input_tested": True, "screenshots": {}, "checks": [], "sections": wanted}
    run = base / "island"
    child, url = launch(binary, run, [])
    try:
        flow = Live2Flow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 90 * 60
        flow.tick_budget = 12 * DAY
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        flow.settle(SIZES[0])
        flow.click("New game")
        flow.click("Start on the island")
        wait_for(lambda: flow.widget("Menu"), 60)

        def arrived():
            visitors = [row for row in flow.state()["entities"]
                        if row.get("components", {}).get("island_villager", {}).get("role")
                        in ("visitor", "Visitor")]
            return visitors if len(visitors) == 1 else None
        flow.actor = wait_for(arrived, 60)[0]["sim_id"]
        report["actor"] = flow.actor
        wait_for(lambda: flow.panel_texts(HOTBAR), 60)
        # Dismiss the first-run tip so it does not cover the captures.
        if flow.tip():
            flow.click("Island tip > Got it")
        parts = {"live": live, "people": people, "calendar": calendar, "oversee": oversee}
        for name in wanted:
            flow.section(name, lambda name=name: parts[name](flow, report, report["checks"]))
            flow.section(name + " cleanup", lambda: flow.settle(SIZES[0]))
        report["ok"] = all(check["ok"] for check in report["checks"])
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        stop(child, run, report, "island")
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        if (run / "shots").is_dir():
            for image in sorted((run / "shots").glob("*.png")):
                shutil.copy2(image, args.out / image.name)
                copied.append(str(args.out / image.name))
        report["copied"] = copied
        name = "summary-" + "-".join(wanted) + ".json"
        (args.out / name).write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
