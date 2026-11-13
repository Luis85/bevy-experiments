"""Native captures of the fifth UI pass: New game from an exported world (the
export toast's offer, the world picker with two exported worlds and one bad
file, and the first frame of a game started on the shaped world with the
patch placed in Shape); patch chips that never overlap; a gift with its toast
and the tastes it taught; the upgrade card under the physical flow (an order
nobody has taken yet, then a hauler and the hired builder); and the inn card's
Workers and guests tab, its building title and the civic fund chip. Each at
1280x720 and 1024x768.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p testbed_content --test island_capture_fixtures -- --ignored --test-threads 5
  cargo build --locked -p game_menu
  python scripts/playtest/capture_ui_live5.py [--only worlds,summary,gift,upgrade,inn]

Every change to the world is a recorded command that a control, a click or a
key of the game submits. PNGs and summary.json go to target/npc-out/ui-live5/.
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
from capture_player_flow import BUILDING_CARD, LEDGER, wait_for
from capture_ui_live3 import FIXTURES, SUMMARY, VEIL, start
from capture_ui_live4 import RESOURCES, Live4Flow
from capture_ui_phase45 import GOAL, HOTBAR, WORKSPACE
from capture_ui_redesign import SIZES, stop

capture_player_flow.SIZES = SIZES
MENU = "Game menu"
WORLDS = "World list"
PATCH = "testbed_content::village::model::ResourcePatch"
SIMPOS = "npc_sim::math::SimPos"


class Live5Flow(Live4Flow):
    def patch_positions(self):
        """Every resource patch's good and position, read over BRP, or None
        when the types are not reachable."""
        try:
            rows = query(self.url, [PATCH, SIMPOS])
        except Exception as error:  # the report says why
            self.report.setdefault("notes", []).append(f"patch query: {error}")
            return None
        out = []
        for row in rows:
            patch = row["components"].get(PATCH, {})
            pos = row["components"].get(SIMPOS, {})
            out.append((patch.get("kind"), pos.get("x"), pos.get("z")))
        return out

    def export_now(self, rules):
        before = set(rules.glob("*-shaped-*.json")) if rules.is_dir() else set()
        self.press("Export world")
        toast = self.toast(("World exported as",), 15)
        after = set(rules.glob("*-shaped-*.json")) if rules.is_dir() else set()
        return toast, sorted(after - before)

    def wait_texts(self, panel, words, seconds):
        try:
            return wait_for(lambda: [text for text in self.panel_texts(panel)
                                     if any(word in text for word in words)] or None, seconds)
        except RuntimeError:
            return None


def patches_of(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    return {(p["good"], p["x"], p["z"]) for p in data.get("living_world", {}).get("resource_patches", [])}


def worlds(flow, report, checks):
    rules = flow.run / "rules"
    flow.click("Shape")
    time.sleep(1.5)
    flow.call("npc/pause")
    flow.park(SIZES[0])
    # Export the island as it is, then place a patch and export again: the
    # second file differs from the first by the placed patch.
    first_toast, first = flow.export_now(rules)
    report["first_export"] = {"toast": first_toast, "files": [p.name for p in first]}
    flow.key("KeyN")
    time.sleep(0.8)
    flow.press("Richer")
    flow.press("Richer")
    at, tag = flow.hover_until(SIZES[0], lambda tag: len(tag) == 1 and tag[0].startswith("Place"))
    report["place_at"] = {"at": at, "tag": tag}
    placed = None
    # A click can be lost while the window settles: tried twice.
    for _ in range(2):
        if not at or placed:
            break
        flow.cursor(at[0], at[1])
        time.sleep(0.5)
        flow.call("npc/resume")
        flow.click_here()
        placed = flow.toast(("Placed a",), 10)
        flow.call("npc/pause")
    report["placed"] = placed
    # The chips at island zoom: never one under another, none under the tip.
    flow.shots("live5-shape-chips")
    flow.key("Escape")
    time.sleep(0.5)
    offer_toast, second = flow.export_now(rules)
    report["second_export"] = {"toast": offer_toast, "files": [p.name for p in second]}
    offer = flow.has_widget("Start a new game on it")
    checks.append({"check": "after an export the toast offers to start a new game on it",
                   "ok": bool(offer_toast) and bool(offer), "texts": offer_toast})
    # The toast stays 12 s and a resize can take longer: one fresh export
    # for each size, so each shot shows it.
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.4)
        flow.park(size)
        flow.export_now(rules)
        flow.snap(f"live5-export-offer-{size[0]}x{size[1]}", size)
    flow.settle(SIZES[0])
    added = []
    if first and second:
        added = sorted(patches_of(second[0]) - patches_of(first[0]))
    report["placed_patch"] = added
    checks.append({"check": "the second exported world holds the placed patch",
                   "ok": len(added) == 1, "added": added})
    # A file that cannot be used, beside them.
    (rules / "old-island-shaped-1.json").write_text("{ not a world", encoding="utf-8")
    if not offer:
        return
    # The offer stays 12 s; shots at two sizes can outlast it. Export once
    # more to bring it back before choosing it.
    if not flow.has_widget("Start a new game on it"):
        flow.export_now(rules)
        wait_for(lambda: flow.has_widget("Start a new game on it") or None, 10)
    flow.click("Start a new game on it")
    picker = wait_for(lambda: flow.panel_texts(WORLDS) or None, 40)
    time.sleep(1.0)
    report["picker"] = picker
    menu = flow.panel_texts(MENU)
    exported = [text for text in (picker or []) if text.endswith("(shaped)")]
    bad = [text for text in (picker or []) if text.startswith("Cannot be used")]
    checks.append({"check": "the picker lists the shipped world first, two exported worlds and the bad file with its reason",
                   "ok": bool(picker) and "Shipped world" in picker and len(exported) >= 2 and len(bad) == 1,
                   "texts": picker})
    checks.append({"check": "the island was saved and is kept as a backup before the picker opened",
                   "ok": any("kept as a backup" in text for text in menu), "texts": menu[:8]})
    flow.shots("live5-world-picker", settle=0.3)
    # The newest valid export is chosen; begin in Shape.
    begin = flow.controls("Shape the landscape")
    report["begin_controls"] = begin
    if not begin:
        return
    flow.activate(begin[0], "Shape the landscape")
    wait_for(lambda: flow.has_widget("Export world") or None, 90)
    flow.call("npc/pause")
    time.sleep(1.2)
    flow.key("KeyN")
    time.sleep(0.8)
    flow.shots("live5-shaped-new-game", settle=0.3)
    seen = flow.patch_positions()
    report["new_game_patches"] = None if seen is None else len(seen)
    found = None
    if added and seen is not None:
        good, x, z = added[0]
        found = [p for p in seen if p[1] == x and p[2] == z]
    saved = flow.run / "living-world.world.json"
    in_save = False
    if added and saved.is_file():
        text = saved.read_text(encoding="utf-8")
        in_save = str(added[0][1]) in text and str(added[0][2]) in text
    checks.append({"check": "the game started on the exported world has the placed patch",
                   "ok": bool(found) or (seen is None and in_save),
                   "patch": added[:1], "found_live": found, "in_new_save": in_save})
    flow.key("Escape")


def gift(flow, report, checks):
    flow.call("npc/pause")
    prompt = flow.panel_texts("Live prompt")
    report["prompt_first"] = prompt
    if any("Stop" in text for text in prompt):
        flow.key("KeyX")
        time.sleep(0.5)
    flow.call("npc/resume")
    talk = flow.wait_texts("Live prompt", ("Talk to ",), 12)
    flow.call("npc/pause")
    report["prompt"] = talk or flow.panel_texts("Live prompt")
    if talk:
        flow.key("KeyE")
    else:
        flow.workspace("KeyN")
    time.sleep(0.8)
    if flow.has_widget("Give a gift"):
        flow.click("Give a gift")
        time.sleep(0.6)
    before = [text for text in flow.panel_texts(WORKSPACE) if " likes " in text or "what " in text]
    report["tastes_before"] = before
    flow.shots("live5-gift-picker")
    gifts = [(entity, text) for entity, text in flow.button_rows(WORKSPACE)
             if text.splitlines()[-1:] and text.splitlines()[-1] in ("Berries", "Bread")]
    toast = None
    if gifts:
        flow.call("npc/resume")
        flow.activate(gifts[0][0], "a gift slot")
        toast = flow.toast(("thanked you", "loved the", "did not care"), 12)
        flow.call("npc/pause")
    report["gift_toast"] = toast
    checks.append({"check": "a gift answers with a toast", "ok": bool(toast), "texts": toast,
                   "prompt": report["prompt"]})
    flow.shots("live5-gift-toast", settle=0.2)
    if flow.has_widget("Give a gift"):
        flow.click("Give a gift")
        time.sleep(0.6)
    after = [text for text in flow.panel_texts(WORKSPACE) if " likes " in text]
    report["tastes_after"] = after
    checks.append({"check": "after the gift the People tab knows what they like",
                   "ok": bool(after), "before": before, "after": after})
    flow.shots("live5-gift-tastes", settle=0.2)


def upgrade(flow, report, checks):
    flow.call("npc/pause")
    report["goal_ordered"] = flow.panel_texts(GOAL)
    card = flow.workspace("KeyB")
    report["card_unanswered"] = card[:30]
    checks.append({"check": "an order nobody has taken says so plainly",
                   "ok": any("No one has taken this order yet" in text for text in card),
                   "texts": card[:30]})
    checks.append({"check": "the goal card says what is happening at home",
                   "ok": any("meanwhile" in text.lower() for text in report["goal_ordered"]),
                   "texts": report["goal_ordered"]})
    flow.shots("live5-upgrade-unanswered")
    # Time runs at the fastest step until somebody takes the order.
    flow.call("npc/resume")
    for _ in range(3):
        flow.key("Equal")
        time.sleep(0.2)
    started = time.monotonic()
    hauler = flow.wait_texts(WORKSPACE, (" is bringing ", " is buying "), 300)
    report["hauler_after_seconds"] = round(time.monotonic() - started, 1)
    flow.call("npc/pause")
    report["hauler"] = hauler
    checks.append({"check": "the card names a hauler under the physical flow",
                   "ok": bool(hauler), "texts": hauler})
    report["goal_hauler"] = flow.panel_texts(GOAL)
    flow.shots("live5-upgrade-hauler")
    flow.call("npc/resume")
    report["builder_hired"] = [text for text in flow.panel_texts(WORKSPACE)
                               if " is hired and will come" in text]
    builder = flow.wait_texts(WORKSPACE, (" is building at your door", " is on the way to work"), 420)
    flow.call("npc/pause")
    report["builder"] = builder
    report["goal_builder"] = flow.panel_texts(GOAL)
    checks.append({"check": "the card names the hired builder on the way or at work",
                   "ok": bool(builder), "texts": builder, "hired": report["builder_hired"]})
    flow.shots("live5-upgrade-builder")
    flow.key("KeyB")


def inn(flow, report, checks):
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.2)
    bar = [text for text in flow.texts() if text.endswith("fund") or "fund" in text][:6]
    report["fund_chip"] = bar
    flow.open_page("Village")
    time.sleep(0.8)
    inns = [text for text in flow.panel_texts(LEDGER) if "guest beds taken" in text]
    report["inns"] = inns
    checks.append({"check": "the Village page titles inns by the building",
                   "ok": bool(inns) and all("innkeeper" not in text.split(" · ")[0] for text in inns),
                   "texts": inns})
    taken = [text for text in inns if not text.split(" · ")[-1].startswith("0 of")]
    if taken or inns:
        flow.press_row((taken or inns)[0], LEDGER)
        time.sleep(1.5)
    card = flow.panel_texts(BUILDING_CARD)
    report["inn_card"] = card[:20]
    checks.append({"check": "the inn's card is titled by its building",
                   "ok": bool(card) and card[0].endswith(" inn"), "texts": card[:6]})
    if flow.has_widget("Workers and guests"):
        flow.click("Workers and guests")
        time.sleep(0.8)
    people = flow.panel_texts(BUILDING_CARD)
    report["inn_people"] = people[:30]
    checks.append({"check": "the Workers and guests tab names the guest",
                   "ok": any("guest" in text.lower() for text in people) and len(people) > 4,
                   "texts": people[:30]})
    flow.shots("live5-inn-guests")
    flow.key("Escape")
    time.sleep(0.4)
    flow.close_ledger()
    # The civic fund chip and its tooltip.
    flow.shots("live5-oversee-bar")
    chips = [row for row in flow.ui()[0] if str(row.get("text", "")) == "fund"]
    if chips:
        box = flow.rect(int(chips[0]["entity"]))
        if box:
            flow.cursor((box[0] + box[2]) / 2 - 20, (box[1] + box[3]) / 2)
            time.sleep(1.6)
            report["fund_tooltip"] = [text for text in flow.texts()
                                      if "fund" in text.lower() or "coins" in text.lower()][:6]
            flow.snap("live5-fund-tooltip-1280x720", SIZES[0])
            flow.park(SIZES[0])
    checks.append({"check": "the Oversee bar's fund chip reads the civic fund",
                   "ok": bool(bar) and not any(text.strip() == "0 fund" for text in bar),
                   "texts": bar})


def summary(flow, report, checks):
    """Sleep at the door; the morning's account has a Bought row (the
    fixture bought a loaf that day)."""
    flow.call("npc/pause")
    flow.call("npc/resume")
    flow.key("KeyE")
    wait_for(lambda: flow.panel_texts(VEIL) or None, 20)
    report["goal_asleep"] = flow.panel_texts(GOAL)
    lines = wait_for(lambda: flow.panel_texts(SUMMARY) or None, 240)
    flow.call("npc/pause")
    report["summary"] = lines
    checks.append({"check": "the morning's account has a Bought row with what it cost",
                   "ok": bool(lines) and "Bought" in lines
                   and any(" for " in text and "coins" in text for text in lines),
                   "texts": lines})
    flow.shots("live5-day-summary", settle=0.3)
    flow.key("Enter")


PARTS = {"worlds": ("cottage-evening", [worlds]),
         "summary": ("cottage-evening", [summary]),
         "gift": ("village-life", [gift]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "inn": ("village-life", [inn])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live5")
    parser.add_argument("--only", default="worlds,summary,gift,upgrade,inn")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live5-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor and "
                       "MouseButtonInput; the simulation clock run, paused or sped up by its keys; "
                       "prepared saves copied in; one unreadable world file written beside the exports",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            run, child, flow = start(binary, base, name, report, fixture)
            flow.__class__ = Live5Flow
            try:
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
