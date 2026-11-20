"""Native captures of what the second Live pass left unseen, from prepared
saves: the menu tooltips (title, Settings, pause), the cottage evening
(homestead tiers, People with known neighbours, the sleep prompt, the
"Sleeping" veil and the 30x night, waking, the day's account and putting it
away), an upgrade under way (Deliver and Cancel, the chest in the pouch,
Take, the work and its toasts), and hard times (a critical need's pulse and
its Reduce-motion steady edge, the unaffordable build card, a failed save's
"Try again"). Each part at 1280x720 and 1024x768.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p island_content --test island_capture_fixtures -- --ignored --test-threads 1
  cargo build --locked -p island_game
  python scripts/playtest/capture_ui_live3.py [--only menus,cottage,upgrade,hard]

Each part starts the real game on a copy of one save in
target/npc-out/fixtures/ (the fixture itself is never written) and drives it
over BRP: controls by the text on them or their `Name`, keys as KeyboardInput
messages, the pointer through the window's cursor position, the clock run,
paused or stepped. Every change to the world is a recorded command that a
control or key of the game submits. A part that fails is a failed check and
the run goes on. PNGs and summary.json go to target/npc-out/ui-live3/.
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
from capture_player_flow import ALERTS, LEDGER, PRIMARY, TRAY, wait_for
from capture_ui_phase45 import GOAL, HOTBAR, NEEDS, PROMPT, WORKSPACE
from capture_ui_phase67 import Phase67Flow
from capture_ui_redesign import SIZES, launch, stop
from capture_view_checks import DAY

capture_player_flow.SIZES = SIZES
FIXTURES = ROOT / "target" / "npc-out" / "fixtures"
VEIL = "Sleeping veil"
SUMMARY = "Day summary"


class Live3Flow(Phase67Flow):
    def shots(self, label, settle=0.8):
        self.both(label, self.park, settle=settle)

    def resource(self, suffix):
        names = [name for name in self.call("world.list_resources", {})
                 if str(name).endswith(suffix)]
        value = self.call("world.get_resources", {"resource": names[0]})
        return value.get("value", value) if isinstance(value, dict) else value

    def rate(self, seconds=2.0):
        """Simulation ticks per real second while the clock runs (20 at 1x)."""
        start, began = self.state()["tick"], time.monotonic()
        time.sleep(seconds)
        return round((self.state()["tick"] - start) / (time.monotonic() - began), 1)

    def tooltip_cards(self, size):
        return [card for card in self.tooltips_inside(size)]

    def menu_tooltip(self, label, control, word, under=None):
        """Rest the pointer on a menu control at both sizes: its tooltip
        shows, fully inside the window, and says `word`."""
        results = []
        for size in SIZES:
            self.settle(size)
            time.sleep(0.6)
            self.park(size)
            entity = self.control(control, under)
            x, y = self.centre(entity)
            self.cursor(x, y)
            cards = wait_for(lambda: [card for card in self.tooltips_inside(size)] or None, 6) or []
            time.sleep(0.4)
            texts = self.texts()
            self.snap(f"tooltip-{label}-{size[0]}x{size[1]}", size)
            results.append({"size": list(size), "cards": cards,
                            "says": any(word in text for text in texts)})
            self.report["checks"].append({
                "check": f"the {label} tooltip shows inside the {size[0]}x{size[1]} window",
                "ok": bool(cards) and all(card["inside"] for card in cards)
                and any(word in text for text in texts), "cards": cards})
            self.park(size)
        self.settle(SIZES[0])
        return results

    def enter_live(self):
        self.click("Continue")
        wait_for(lambda: self.panel_texts(HOTBAR), 90)
        time.sleep(1.0)
        if self.tip():
            self.click("Island tip > Got it")
            time.sleep(0.4)

    def workspace(self, key):
        self.key(key)
        time.sleep(0.9)
        return self.panel_texts(WORKSPACE)

    def has_text(self, panel, start):
        return [text for text in self.panel_texts(panel) if text.startswith(start)]

    def toast(self, words, seconds):
        """The texts on screen that hold one of `words`, or None after `seconds`."""
        try:
            return wait_for(lambda: [text for text in self.texts()
                                     if any(word in text for word in words)] or None, seconds)
        except RuntimeError:
            return None


def start(binary, base, name, report, fixture=None):
    run = base / name
    save = None if fixture is None else FIXTURES / f"{fixture}.json"
    if save is not None and not save.is_file():
        raise RuntimeError(f"Missing fixture {save}; run island_capture_fixtures first")
    child, url = launch(binary, run, [], save=save)
    flow = Live3Flow(run, child, url, report, time.monotonic())
    flow.deadline = time.monotonic() + 40 * 60
    flow.tick_budget = 8 * DAY
    flow.bootstrap()
    flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
    flow.settle(SIZES[0])
    return run, child, flow


def menus(flow, report, checks):
    """Title, Settings and pause: a tooltip on each, at both sizes."""
    time.sleep(1.0)
    report["menu_tooltips"] = {
        "title_continue": flow.menu_tooltip("title-continue", "Continue", "Load your island"),
        "title_settings": flow.menu_tooltip("title-settings", "Settings", "accessibility"),
    }
    flow.click("Settings")
    time.sleep(0.6)
    report["menu_tooltips"]["settings_back"] = flow.menu_tooltip(
        "settings-back", "Back", "previous screen")
    flow.key("Escape")
    time.sleep(0.5)
    flow.enter_live()
    flow.key("Escape")
    time.sleep(0.4)
    if not flow.has_widget("Save game"):
        flow.click("Menu")
    time.sleep(0.6)
    report["menu_tooltips"]["pause_save"] = flow.menu_tooltip("pause-save", "Save game", "every minute")
    flow.click("Resume")
    time.sleep(0.6)


def click_home(flow, size):
    """Click points of the view until the own cottage opens (Build shows its
    tiers and the cottage becomes the selection, which the homes view cuts
    away). The camera frames the character at the door, so the cottage is
    near the middle; a grid of points around it is tried in turn."""
    flow.settle(size)
    time.sleep(0.6)
    width, height = size
    for dy in (0.0, -0.06, 0.06, -0.12):
        for dx in (0.12, 0.18, 0.06, 0.24, 0.0, -0.06):
            flow.cursor(width * (0.5 + dx), height * (0.48 + dy))
            time.sleep(0.25)
            flow.hold(True)
            time.sleep(0.12)
            flow.hold(False)
            time.sleep(0.6)
            if "Improve your homestead" in flow.panel_texts(WORKSPACE):
                return [round(width * (0.5 + dx)), round(height * (0.48 + dy))]
    return None


def cottage(flow, report, checks):
    flow.call("npc/pause")
    flow.shots("live3-cottage-evening")
    # A click on the own cottage opens it.
    at = click_home(flow, SIZES[0])
    report["home_click"] = at
    checks.append({"check": "a click on your own cottage opens Build and cuts the cottage away",
                   "ok": at is not None, "at": at})
    if at is not None:
        flow.park(SIZES[0])
        flow.snap("live3-home-click-1280x720", SIZES[0])
        flow.key("KeyB")
    report["build"] = flow.workspace("KeyB")
    checks.append({"check": "Build lists the homestead tiers with the storage chest open",
                   "ok": "Improve your homestead" in report["build"]
                   and flow.has_widget("Build a storage chest"), "texts": report["build"][-24:]})
    flow.shots("live3-homestead-tiers")
    flow.key("KeyB")
    report["people"] = flow.workspace("KeyN")
    joined = " ".join(report["people"])
    checks.append({"check": "People knows three neighbours, one a friend",
                   "ok": joined.count("Acquaintance") + joined.count("Friend") >= 3
                   and "Friend" in joined, "texts": report["people"][:30]})
    flow.shots("live3-people-known")
    flow.key("KeyN")
    report["sleep_prompt"] = flow.panel_texts(PROMPT)
    checks.append({"check": "at the own door in the evening the prompt offers sleep",
                   "ok": "Sleep until morning" in report["sleep_prompt"],
                   "texts": report["sleep_prompt"]})
    flow.shots("live3-sleep-prompt")
    flow.call("npc/resume")
    before = flow.rate()
    flow.key("KeyE")
    veil = wait_for(lambda: flow.panel_texts(VEIL) or None, 20)
    during = flow.rate()
    flow.call("npc/pause")
    report["veil"] = veil
    report["ticks_per_second"] = {"awake": before, "asleep": during}
    checks.append({"check": "sleep dims the edges and says until when, with Wake up",
                   "ok": bool(veil) and any(text.startswith("Sleeping") for text in veil)
                   and any("morning at" in text for text in veil)
                   and "Wake up" in veil, "texts": veil})
    checks.append({"check": "while asleep the clock runs many times faster (30x is asked)",
                   "ok": during >= 5 * max(before, 1.0), "rates": report["ticks_per_second"]})
    flow.shots("live3-sleeping-veil", settle=0.4)
    tick = flow.state()["tick"]
    flow.call("npc/resume")
    summary = wait_for(lambda: flow.panel_texts(SUMMARY) or None, 240)
    after = flow.rate()
    flow.call("npc/pause")
    report["woke"] = {"from_tick": tick, "tick": flow.state()["tick"], "ticks_per_second": after}
    report["summary"] = summary
    checks.append({"check": "waking from the night shows the day's account",
                   "ok": bool(summary) and not flow.panel_texts(VEIL), "texts": summary})
    checks.append({"check": "on waking the player's own speed comes back",
                   "ok": after <= 2 * max(before, 1.0), "rates": [before, after]})
    flow.shots("live3-day-summary")
    flow.key("Enter")
    time.sleep(0.8)
    checks.append({"check": "Enter puts the day's account away", "ok": not flow.panel_texts(SUMMARY)})
    flow.shots("live3-morning-after")
    # A gift, from the People workspace: the friend is chosen and a liked
    # good given if they are in reach; otherwise the reason is captured.
    report["people_morning"] = flow.workspace("KeyN")
    flow.shots("live3-people-morning")
    flow.key("KeyN")
    # Repeat orders for recipes and for the garden: the x5 stepper.
    flow.workspace("KeyC")
    # Two steppers now (gather, make): the second is the recipes'.
    steppers = [entity for entity, text in flow.button_rows(WORKSPACE) if text == "\u00d75"]
    flow.activate(steppers[-1], "the recipes' x5")
    time.sleep(0.6)
    report["craft_repeat"] = flow.panel_texts(WORKSPACE)
    flow.shots("live3-craft-repeat")
    flow.key("KeyC")
    report["garden"] = flow.workspace("KeyG")
    if flow.has_widget("Tend your garden"):
        flow.click("Tend your garden")
        time.sleep(0.6)
        report["garden"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "the garden offers repeat orders over the whole plot",
                   "ok": "Work the whole plot" in report["garden"], "texts": report["garden"][:30]})
    flow.shots("live3-garden-repeat")
    flow.key("KeyG")


def upgrade(flow, report, checks):
    flow.call("npc/pause")
    report["build"] = flow.workspace("KeyB")
    checks.append({"check": "the larger cottage is under way with Deliver, Order, Hire and Cancel",
                   "ok": any(text.startswith("Under way") for text in report["build"])
                   and any(text.startswith("Deliver ") for text in report["build"])
                   and any(text.startswith("Order ") for text in report["build"])
                   and "Hire a builder" in report["build"]
                   and "Cancel" in report["build"], "texts": report["build"][:40]})
    flow.shots("live3-upgrade-under-way")
    # Hire a builder: a recorded command; the answer is a toast.
    flow.call("npc/resume")
    flow.click("Hire a builder")
    hired = flow.toast(("is hired to build",), 15)
    flow.call("npc/pause")
    report["hired"] = hired
    checks.append({"check": "hiring a builder is said as a toast", "ok": bool(hired),
                   "texts": hired})
    report["build_hired"] = flow.panel_texts(WORKSPACE)
    flow.shots("live3-upgrade-builder-hired", settle=0.2)
    flow.key("KeyB")
    report["pouch"] = flow.workspace("KeyI")
    checks.append({"check": "the pouch shows the chest with its space and what it holds",
                   "ok": "Your chest" in report["pouch"]
                   and any(text.startswith("Take ") for text in report["pouch"]),
                   "texts": report["pouch"]})
    flow.shots("live3-pouch-chest")
    flow.key("KeyI")
    # Home: the Build workspace's walk to the door; the materials go in by
    # themselves there, the work runs, and the finish is a toast.
    flow.workspace("KeyB")
    flow.call("npc/resume")
    if flow.has_widget("Go home and rest", enabled=True):
        flow.click("Go home and rest")
    supplied = wait_for(lambda: flow.has_text(WORKSPACE, "Under way") and any(
        "materials are in" in text for text in flow.panel_texts(WORKSPACE)) or None, 90)
    report["supplied"] = flow.panel_texts(WORKSPACE)
    checks.append({"check": "at the door the carried materials go in and the work starts",
                   "ok": bool(supplied), "texts": report["supplied"][:20]})
    flow.call("npc/pause")
    flow.shots("live3-upgrade-supplied")
    report["working_pill"] = flow.panel_texts(PROMPT)
    flow.shots("live3-working-pill")
    flow.key("KeyB")
    # Stand at the door (Stop ends the rest) and let the work run at 3x.
    flow.call("npc/resume")
    flow.key("KeyX")
    flow.key("Equal")
    done = flow.toast(("is finished",), 240)
    flow.call("npc/pause")
    report["finished_toast"] = done
    checks.append({"check": "the finished upgrade is said as a toast", "ok": bool(done),
                   "texts": done})
    # Enlarging the cottage is also a goal of the chain: said once, after.
    goal = flow.toast(("Goal done",), 2)
    if not goal:
        flow.call("npc/resume")
        goal = flow.toast(("Goal done",), 8)
        flow.call("npc/pause")
    report["goal_toast"] = goal
    checks.append({"check": "a goal done is said once as a toast", "ok": bool(goal), "texts": goal})
    flow.shots("live3-upgrade-done-toast", settle=0.2)
    flow.key("Minus")
    # The chest: take something out and store it back.
    report["pouch_after"] = flow.workspace("KeyI")
    take = next((text for text in report["pouch_after"] if text.startswith("Take ")), None)
    if take:
        flow.call("npc/resume")
        flow.press_row(take, WORKSPACE)
        time.sleep(1.5)
        flow.call("npc/pause")
        report["after_take"] = flow.panel_texts(WORKSPACE)
        flow.shots("live3-chest-take")
    flow.key("KeyI")
    # The Journal page in Oversee: social and homestead notes of this play.
    flow.click("Oversee")
    time.sleep(1.2)
    flow.open_page("Journal")
    time.sleep(0.8)
    flow.click(LEDGER + " > Everything")
    time.sleep(0.8)
    report["journal"] = flow.panel_texts(LEDGER)
    joined = " ".join(report["journal"])
    checks.append({"check": "the Journal lists social and homestead notes of this play",
                   "ok": "hired a builder" in joined or "finished" in joined,
                   "texts": report["journal"][:40]})
    flow.shots("live3-journal-notes")
    flow.close_ledger()
    flow.click("Live")
    wait_for(lambda: flow.panel_texts(HOTBAR), 30)


def hard(flow, report, checks):
    flow.call("npc/pause")
    report["needs"] = flow.panel_texts(NEEDS)
    flow.shots("live3-critical-need")
    # Two frames a second apart: the pulse changes the outline's strength.
    flow.snap("live3-critical-need-pulse-a-1280x720", SIZES[0])
    time.sleep(1.0)
    flow.snap("live3-critical-need-pulse-b-1280x720", SIZES[0])
    # Reduce motion: a steady edge.
    flow.key("Escape")
    time.sleep(0.4)
    if not flow.has_widget("Settings"):
        flow.click("Menu")
    flow.click("Settings")
    flow.click("Accessibility")
    time.sleep(0.4)
    flow.press("Reduce motion")
    time.sleep(0.4)
    report["reduce_motion"] = flow.stored().get("reduce_motion")
    flow.key("Escape")
    time.sleep(0.4)
    flow.click("Resume")
    time.sleep(0.8)
    flow.call("npc/pause")
    flow.shots("live3-critical-need-reduce-motion")
    checks.append({"check": "Reduce motion is stored", "ok": report["reduce_motion"] is True,
                   "stored": report["reduce_motion"]})
    # The build tray in Oversee: nothing affordable.
    flow.click("Oversee")
    time.sleep(1.2)
    flow.key("KeyB")
    time.sleep(1.0)
    report["tray"] = flow.panel_texts(TRAY)
    checks.append({"check": "an unaffordable build card says why",
                   "ok": any("can raise only" in text for text in report["tray"]),
                   "texts": report["tray"][:20]})
    flow.shots("live3-build-tray-unavailable")
    flow.key("Escape")
    time.sleep(0.4)
    # A failed save: the save file is replaced by a folder of the same name.
    save = flow.run / "living-world.world.json"
    save.unlink()
    save.mkdir()
    try:
        flow.key("KeyS", hold=("ControlLeft",))
        failed = wait_for(lambda: flow.has_widget("Try again") or None, 10)
        report["failed_save"] = [text for text in flow.texts() if "save" in text.lower()][:6]
        checks.append({"check": "a failed save shows a lasting toast with Try again",
                       "ok": bool(failed), "texts": report["failed_save"]})
        flow.shots("live3-save-failed-try-again", settle=0.3)
    finally:
        save.rmdir()
    flow.click("Try again")
    saved = flow.toast(("Island saved",), 10)
    report["retried"] = saved
    checks.append({"check": "Try again saves and says so", "ok": bool(saved) and save.is_file(),
                   "texts": saved})
    flow.shots("live3-save-retried", settle=0.2)
    flow.click("Live")
    wait_for(lambda: flow.panel_texts(HOTBAR), 30)


def empty_states(flow, report, checks):
    """Every Live workspace and ledger page as it opens on hard times."""
    flow.call("npc/pause")
    seen = {}
    for key, name in (("KeyC", "craft"), ("KeyG", "garden"), ("KeyT", "trade"),
                      ("KeyM", "explore")):
        seen[name] = flow.workspace(key)
        flow.shots(f"live3-empty-{name}")
        flow.key(key)
    flow.click("Oversee")
    time.sleep(1.0)
    for page in ("Economy", "People", "Jobs", "Projects", "Journal"):
        try:
            flow.open_page(page)
        except Exception as error:  # noqa: BLE001: a missing page is a finding.
            seen[page] = f"unavailable: {error}"
            continue
        time.sleep(0.8)
        seen[page] = flow.panel_texts(LEDGER)
        flow.shots(f"live3-empty-ledger-{page.lower()}")
    flow.close_ledger()
    report["empty_states"] = seen


PARTS = {"menus": ("cottage-evening", [menus]),
         "cottage": ("cottage-evening", [cottage]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "hard": ("hard-times", [hard, empty_states])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live3")
    parser.add_argument("--only", default="menus,cottage,upgrade,hard")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live3-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor; the "
                       "simulation clock run, paused or stepped; prepared saves copied in",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            run, child, flow = start(binary, base, name, report, fixture)
            try:
                if name != "menus":
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
