"""Native captures of the fourth UI pass: Shape's resources (place, the ghost
and its verdict, richness, remove, markers and labels, Esc) and Export world
(success and a failed write with Try again); menu tooltips beside their
buttons; the day's account with wrapping rows; the upgrade card without
Deliver; the goal card folded while asleep; and from the village-life save a
gift toast, the inn card with its guest and the Departed list. Each at
1280x720 and 1024x768.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p testbed_content --test island_capture_fixtures -- --ignored --test-threads 2
  cargo build --locked -p game_menu
  python scripts/playtest/capture_ui_live4.py [--only menus,shape,cottage,upgrade,village]

Every change to the world is a recorded command that a control, a click or a
key of the game submits. PNGs and summary.json go to target/npc-out/ui-live4/.
"""
import argparse
import json
from pathlib import Path
import shutil
import time
import uuid

import capture_player_flow
from capture_native import ROOT, query
from capture_native_homestead import COMPUTED, TRANSFORM, file_hash, translation, vector
from capture_player_flow import BUILDING_CARD, LEDGER, wait_for
from capture_ui_live3 import (FIXTURES, SUMMARY, VEIL, Live3Flow, start)
from capture_ui_phase45 import GOAL, HOTBAR, WORKSPACE
from capture_ui_redesign import SIZES, stop

capture_player_flow.SIZES = SIZES
RESOURCES = "Shape resources"
TAG = "Resource cursor tag"


class Live4Flow(Live3Flow):
    def rect(self, entity):
        """A node's rectangle in logical pixels."""
        factor = self.scale()
        for row in query(self.url, [COMPUTED, TRANSFORM]):
            if int(row["entity"]) == entity:
                x, y = translation(row["components"][TRANSFORM])[:2]
                w, h = vector(row["components"][COMPUTED]["size"])
                return [(x - w / 2) / factor, (y - h / 2) / factor,
                        (x + w / 2) / factor, (y + h / 2) / factor]
        return None

    def menu_tooltip_beside(self, label, control, word):
        """The tooltip of a menu control stands right of it or above it, and
        never over another control."""
        for size in SIZES:
            self.settle(size)
            time.sleep(0.6)
            self.park(size)
            entity = self.control(control)
            anchor = self.rect(entity)
            x, y = self.centre(entity)
            self.cursor(x, y)
            cards = wait_for(lambda: self.tooltips_inside(size) or None, 6) or []
            time.sleep(0.4)
            self.snap(f"tooltip-{label}-{size[0]}x{size[1]}", size)
            beside = bool(cards) and anchor is not None and all(
                card["rect"][0] >= anchor[2] - 0.5 or card["rect"][3] <= anchor[1] + 0.5
                for card in cards)
            self.report["checks"].append({
                "check": f"the {label} tooltip stands beside or above its button at {size[0]}x{size[1]}",
                "ok": beside and all(card["inside"] for card in cards),
                "anchor": anchor, "cards": cards,
                "says": any(word in text for text in self.texts())})
            self.park(size)
        self.settle(SIZES[0])

    def tag(self):
        return [text for text in self.panel_texts(TAG) if text]

    def hover_until(self, size, accept, points=None):
        """Move the pointer over a grid of ground points until `accept(tag)`."""
        width, height = size
        points = points or [(0.5 + dx, 0.5 + dy) for dy in (0.0, 0.1, -0.1, 0.2, -0.15)
                            for dx in (0.0, 0.08, -0.08, 0.16, -0.16, 0.24)]
        for fx, fy in points:
            self.cursor(width * fx, height * fy)
            time.sleep(0.45)
            tag = self.tag()
            if tag and accept(tag):
                return [round(width * fx), round(height * fy)], tag
        return None, self.tag()

    def click_here(self):
        self.hold(True)
        time.sleep(0.12)
        self.hold(False)
        time.sleep(0.5)


def menus(flow, report, checks):
    time.sleep(1.0)
    flow.menu_tooltip_beside("title-continue", "Continue", "Load your island")
    flow.menu_tooltip_beside("title-new-game", "New game", "Choose a name")
    flow.enter_live()
    if not flow.has_widget("Save game"):
        flow.click("Menu")
    time.sleep(0.6)
    flow.menu_tooltip_beside("pause-save", "Save game", "every minute")
    flow.click("Resume")


def shape(flow, report, checks):
    flow.click("Shape")
    time.sleep(1.5)
    flow.call("npc/pause")
    flow.park(SIZES[0])
    flow.key("KeyN")
    time.sleep(0.8)
    panel = flow.panel_texts(RESOURCES)
    report["resources_panel"] = panel
    checks.append({"check": "N opens the Resources panel with goods from the world and a richness stepper",
                   "ok": "Resources" in panel and any("%" in text for text in panel)
                   and len(flow.button_rows(RESOURCES)) >= 3, "texts": panel})
    flow.shots("live4-shape-resources")
    # Richness: two steps up.
    flow.press("Richer")
    flow.press("Richer")
    time.sleep(0.4)
    report["richness"] = [text for text in flow.panel_texts(RESOURCES) if "%" in text]
    checks.append({"check": "the richness stepper says the word and the number",
                   "ok": any(text.startswith("Rich") and "150%" in text for text in report["richness"]),
                   "texts": report["richness"]})
    # A good: the first tile of the grid is chosen at start; pick the second.
    rows = flow.button_rows(RESOURCES)
    goods = [(entity, text) for entity, text in rows if text and "%" not in text
             and text not in ("−", "+")]
    report["goods"] = [text for _, text in goods]
    if len(goods) > 1:
        flow.activate(goods[1][0], "a good")
        time.sleep(0.4)
    # A place where it can go: the tag without a reason.
    at, tag = flow.hover_until(SIZES[0], lambda tag: len(tag) == 1 and tag[0].startswith("Place"))
    report["place_ok"] = {"at": at, "tag": tag}
    checks.append({"check": "over good ground the ghost is green and the tag says what a click places",
                   "ok": at is not None, "tag": tag})
    flow.snap("live4-shape-place-ghost-1280x720", SIZES[0])
    if at:
        flow.call("npc/resume")
        flow.click_here()
        placed = flow.toast(("Placed a",), 10)
        flow.call("npc/pause")
        report["placed"] = placed
        checks.append({"check": "a click places a patch and says so", "ok": bool(placed),
                       "texts": placed})
        flow.snap("live4-shape-placed-1280x720", SIZES[0])
        # The new patch is marked: the same spot now refuses another (2 m).
        flow.cursor(at[0], at[1])
        time.sleep(0.6)
        report["refused_tag"] = flow.tag()
        checks.append({"check": "where a patch cannot go the tag gives the simulation's reason",
                       "ok": len(report["refused_tag"]) == 2, "tag": report["refused_tag"]})
        flow.snap("live4-shape-place-refused-1280x720", SIZES[0])
        # Remove it again.
        flow.key("KeyE")
        time.sleep(0.6)
        flow.cursor(at[0], at[1])
        time.sleep(0.6)
        report["remove_tag"] = flow.tag()
        checks.append({"check": "the Remove tool names the patch under the pointer",
                       "ok": any(text.startswith("Remove the") for text in report["remove_tag"]),
                       "tag": report["remove_tag"]})
        flow.snap("live4-shape-remove-hover-1280x720", SIZES[0])
        flow.call("npc/resume")
        flow.click_here()
        removed = flow.toast(("Removed the", "Not removed", "Not applied"), 10)
        flow.call("npc/pause")
        report["removed"] = removed
        checks.append({"check": "a click of Remove takes the patch away (or says why not)",
                       "ok": bool(removed), "texts": removed})
        flow.snap("live4-shape-removed-1280x720", SIZES[0])
    # The markers and labels at both sizes, with the panel.
    flow.key("KeyN")
    time.sleep(0.5)
    flow.shots("live4-shape-patch-labels")
    # Esc puts the tool down.
    flow.key("Escape")
    time.sleep(0.6)
    checks.append({"check": "Esc puts the resource tool down", "ok": not flow.panel_texts(RESOURCES)})
    # Export world: a success, then a failed write and Try again.
    rules = flow.run / "rules"
    before = set(rules.glob("*-shaped-*.json")) if rules.is_dir() else set()
    flow.press("Export world")
    exported = flow.toast(("World exported as",), 15)
    after = set(rules.glob("*-shaped-*.json")) if rules.is_dir() else set()
    written = sorted(str(path.name) for path in after - before)
    report["exported"] = {"toast": exported, "files": written, "folder": str(rules)}
    ok = False
    if written:
        try:
            data = json.loads((rules / written[0]).read_text(encoding="utf-8"))
            ok = "living_world" in data and data.get("name", "").endswith("(shaped)")
        except (OSError, ValueError):
            ok = False
    checks.append({"check": "Export world writes a world file and names it in a toast",
                   "ok": bool(exported) and ok, "seen": report["exported"]})
    flow.shots("live4-shape-exported", settle=0.2)
    # A failed write: the next file's partial name is taken by a folder.
    stem = written[0].rsplit("-shaped-", 1)[0] if written else "the-living-island"
    numbers = [int(name.rsplit("-", 1)[1].split(".")[0]) for name in written] or [0]
    blocker = rules / f"{stem}-shaped-{max(numbers) + 1}.json.partial"
    blocker.mkdir(parents=True, exist_ok=True)
    try:
        flow.press("Export world")
        failed = wait_for(lambda: flow.has_widget("Try again") or None, 10)
        report["export_failed"] = [text for text in flow.texts() if "export" in text.lower()][:4]
        checks.append({"check": "a failed export shows a lasting toast with Try again",
                       "ok": bool(failed), "texts": report["export_failed"]})
        flow.shots("live4-shape-export-failed", settle=0.2)
    finally:
        blocker.rmdir()
    flow.click("Try again")
    again = flow.toast(("World exported as",), 10)
    checks.append({"check": "Try again exports and says so", "ok": bool(again), "texts": again})
    flow.snap("live4-shape-export-retried-1280x720", SIZES[0])
    flow.click("Live")
    wait_for(lambda: flow.panel_texts(HOTBAR), 30)


def cottage(flow, report, checks):
    flow.call("npc/pause")
    flow.call("npc/resume")
    flow.key("KeyE")
    wait_for(lambda: flow.panel_texts(VEIL) or None, 20)
    time.sleep(1.0)
    report["goal_asleep"] = flow.panel_texts(GOAL)
    checks.append({"check": "asleep, the goal card folds to its title",
                   "ok": "Asleep until morning" in report["goal_asleep"]
                   and not any("Finish your current activity" in text or text == "Open Pouch"
                               for text in report["goal_asleep"]),
                   "texts": report["goal_asleep"]})
    flow.call("npc/pause")
    flow.shots("live4-asleep-goal-folded", settle=0.3)
    flow.call("npc/resume")
    summary = wait_for(lambda: flow.panel_texts(SUMMARY) or None, 240)
    flow.call("npc/pause")
    report["summary"] = summary
    lowest = [text for text in (summary or []) if text.startswith("Food")]
    checks.append({"check": "the day's account lists only rows with something to say",
                   "ok": bool(summary) and "Nobody today" not in summary, "texts": summary})
    flow.shots("live4-day-summary")
    # The Lowest row wraps inside the card at both sizes.
    rects = {}
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.6)
        texts = [row for row in flow.ui()[0] if str(row.get("text", "")).startswith("Food")]
        # The card is 460 wide and centred in the window.
        panel = [(size[0] - 460) / 2, 0, (size[0] + 460) / 2, size[1]]
        line = flow.rect(int(texts[0]["entity"])) if texts else None
        rects[f"{size[0]}x{size[1]}"] = {"card": panel, "lowest": line}
    flow.settle(SIZES[0])
    report["summary_rects"] = rects
    checks.append({"check": "the Lowest row stays inside the card",
                   "ok": all(seen["lowest"] is None or (seen["card"] and seen["lowest"][2] <= seen["card"][2] + 0.5)
                             for seen in rects.values()) and bool(lowest),
                   "rects": rects})
    flow.key("Enter")


def upgrade(flow, report, checks):
    flow.call("npc/pause")
    report["build"] = flow.workspace("KeyB")
    checks.append({"check": "the upgrade card has no Deliver, says carried goods go in by themselves, and keeps Order, Hire, Cancel",
                   "ok": not any(text.startswith("Deliver ") for text in report["build"])
                   and any(text.startswith("Carry materials to your door") for text in report["build"])
                   and any(text.startswith("Order ") for text in report["build"])
                   and "Hire a builder" in report["build"] and "Cancel" in report["build"],
                   "texts": report["build"][:30]})
    report["workers_before"] = [text for text in report["build"]
                                if any(word in text for word in ("bringing", "is building", "on the way"))]
    flow.shots("live4-upgrade-card")
    flow.call("npc/resume")
    flow.click("Hire a builder")
    flow.toast(("is hired to build",), 15)
    # Who is on the way, in the simulation's words, once it says so.
    try:
        seen = wait_for(lambda: [text for text in flow.panel_texts(WORKSPACE)
                                 if any(word in text for word in
                                        ("bringing", "is building", "on the way", "is coming",
                                         "is fetching", "is walking"))] or None, 40)
    except RuntimeError:
        seen = None
    flow.call("npc/pause")
    report["workers"] = seen
    checks.append({"check": "the upgrade card says who is on the way or at work",
                   "ok": bool(seen) or bool(report["workers_before"]),
                   "texts": seen or report["workers_before"]})
    flow.shots("live4-upgrade-workers")
    flow.key("KeyB")


def village(flow, report, checks):
    flow.call("npc/pause")
    # The inn first: its guest sleeps until morning, but the night runs on.
    flow.click("Oversee")
    time.sleep(1.2)
    flow.open_page("Village")
    time.sleep(0.8)
    inns = [text for text in flow.panel_texts(LEDGER) if "guest beds taken" in text]
    report["inns"] = inns
    taken = [text for text in inns if not text.split(" · ")[-1].startswith("0 of")]
    if taken or inns:
        flow.press_row((taken or inns)[0], LEDGER)
        time.sleep(1.5)
    card = flow.panel_texts(BUILDING_CARD)
    report["inn_card"] = card
    checks.append({"check": "the inn's card lists its guest",
                   "ok": bool(taken) and not any(text.startswith("Guest beds 0/") for text in card),
                   "inns": inns, "texts": card[:30]})
    flow.shots("live4-inn-card")
    flow.key("Escape")
    time.sleep(0.4)
    flow.open_page("People")
    time.sleep(0.8)
    people = flow.panel_texts(LEDGER)
    report["departed"] = [text for text in people if "Departed" in text or "old age" in text][:6]
    checks.append({"check": "the People page lists the departed",
                   "ok": "Departed" in people and any("old age" in text for text in people),
                   "texts": report["departed"]})
    for _ in range(6):
        flow.key("PageDown")
    time.sleep(0.4)
    flow.shots("live4-departed")
    flow.close_ledger()
    flow.click("Live")
    wait_for(lambda: flow.panel_texts(HOTBAR), 30)
    flow.call("npc/pause")
    # The neighbour in reach is named by the prompt; E opens them in People.
    prompt = flow.panel_texts("Live prompt")
    report["prompt"] = prompt
    if any(text.startswith("Talk to ") for text in prompt):
        flow.key("KeyE")
    else:
        flow.workspace("KeyN")
    time.sleep(0.8)
    if flow.has_widget("Give a gift"):
        flow.click("Give a gift")
        time.sleep(0.6)
    flow.shots("live4-gift-picker")
    gifts = [(entity, text) for entity, text in flow.button_rows(WORKSPACE)
             if text.splitlines()[-1:] and text.splitlines()[-1] in ("Berries", "Bread")]
    report["gift_slots"] = [" ".join(text.splitlines()) for _, text in gifts]
    toast = None
    if gifts:
        flow.call("npc/resume")
        flow.activate(gifts[0][0], "a gift slot")
        toast = flow.toast(("thanked you", "loved the", "did not care"), 12)
        flow.call("npc/pause")
    report["gift_toast"] = toast
    report["gift_reason"] = [text for text in flow.panel_texts(WORKSPACE) if "1.5 m" in text][:2]
    checks.append({"check": "a gift answers with a toast", "ok": bool(toast), "texts": toast,
                   "prompt": prompt, "reason": report["gift_reason"]})
    flow.shots("live4-gift-toast", settle=0.2)


PARTS = {"menus": ("cottage-evening", [menus]),
         "shape": ("cottage-evening", [shape]),
         "cottage": ("cottage-evening", [cottage]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "village": ("village-life", [village])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live4")
    parser.add_argument("--only", default="menus,shape,cottage,upgrade,village")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live4-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor and "
                       "MouseButtonInput; the simulation clock run, paused or stepped; "
                       "prepared saves copied in",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            run, child, flow = start(binary, base, name, report, fixture)
            flow.__class__ = Live4Flow
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
