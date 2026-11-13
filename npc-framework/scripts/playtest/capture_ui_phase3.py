"""Native captures of the redesigned interface, spec phase 3: the resident
inspector, the building and site cards, the build tray with its ghost tag, and
the ledger pages, at 1280x720 and 1024x768.

Build first, then run from npc-framework:
  python scripts/playtest/capture_ui_phase3.py [--binary target/debug/game_menu.exe]
      [--skip-gallery] [--skip-oversee]

Two disposable runs of the real game, driven over BRP like
`capture_ui_redesign.py` and `capture_build_menu.py`:

1. A new island, entered through "Oversee the village". By day: the People
   page, a resident's inspector on each of its tabs, the cards of their home
   and workplace from the inspector's links, each category of the build tray,
   a ghost with its tag where the simulation accepts and where it refuses the
   building (the real pointer path), one placed site, the Projects page with
   the question before abandoning, the site's card, the Economy page with a
   good's detail card and its Jobs table. By night: the inspector, the tray and
   the Journal. Last, the Compact layout (Interface size 140% in a 1280x720
   window), where the totals have a row of their own.
2. `--ui-gallery data`: the card states, the unavailable one among them. No
   shipped building is unavailable, so that state is seen in the gallery only.

Controls are found by the text on them or by their `Name`; rows that hold
several texts are read as a player reads them. No keyboard input is sent: the
keys are covered by unit tests. PNGs go to target/npc-out/ui-redesign-p3/ and
still need to be read.
"""
import argparse
import json
import math
from pathlib import Path
import shutil
import time
import uuid

import capture_player_flow
from capture_build_menu import INVALID, TAG, VALID, BuildFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import (BUILD_DOCK, BUILDING_CARD, INSPECTOR, LEDGER, PRIMARY, TRAY,
                                 wait_for)
from capture_ui_redesign import SIZES, UiFlow, launch, stop
from capture_view_checks import DAY
from capture_view_followups import Terrain

capture_player_flow.SIZES = SIZES
TABS = ("Overview", "Day", "Why", "Home and job")


class Phase3Flow(UiFlow):
    def mouse(self, button):
        """A real mouse button press and release (UiFlow's `press` is a control)."""
        BuildFlow.press(self, button)

    def slug(self, text):
        return text.lower().replace(" ", "-")

    def inspect(self, pick, label):
        """Open the inspector on the first roster row `pick` accepts."""
        chosen = next(((entity, text) for entity, text in self.rows() if pick(text)), None)
        if chosen is None:
            return None
        self.activate(chosen[0], label)
        wait_for(lambda: self.card(), 8)
        time.sleep(1.0)  # The camera frames the resident; the panel fills.
        return chosen[1]

    def tabs(self, label):
        """The inspector on each of its tabs, at both sizes."""
        seen = {}
        for tab in TABS:
            self.click(INSPECTOR + " > " + tab)
            time.sleep(0.6)
            seen[tab] = self.card()
            self.both(f"{label}-{self.slug(tab)}", self.park, settle=0.8)
        self.click(INSPECTOR + " > Overview")
        time.sleep(0.4)
        return seen


def oversee(binary, base, report):
    run = base / "oversee"
    child, url = launch(binary, run, [])
    save = run / "living-world.world.json"
    checks = report["checks"]
    try:
        flow = Phase3Flow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 50 * 60
        flow.tick_budget = 8 * DAY
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        flow.click("New game")
        flow.click("Oversee the village")
        wait_for(lambda: flow.widget("Oversee"))
        flow.call("npc/pause")
        flow.click("Menu")
        flow.click("Save game")
        flow.terrain = Terrain(json.loads(save.read_text(encoding="utf-8")))
        flow.click("Resume")
        flow.call("npc/pause")
        time.sleep(1.0)
        # A working morning, so residents hold stock and have decided things.
        flow.advance(1500)
        report["ticks"] = {"day": flow.to_phase(0.35)}
        flow.park(SIZES[0])
        flow.whole_island()

        # 1. The People page, then a working resident's inspector on every tab.
        flow.open_page("People")
        time.sleep(0.6)
        report["roster_rows"] = len(flow.rows())
        flow.both("ledger-people", flow.park, settle=0.9)
        row = flow.inspect(lambda text: not text.split("\n")[1].startswith(("sleeping", "resting")),
                           "inspector by day")
        checks.append({"check": "a roster row hands the right column to the inspector",
                       "ok": row is not None and flow.ledger_page() is None, "row": row})
        if row is None:
            raise RuntimeError("The roster had no resident to inspect")
        report["inspector_day"] = flow.tabs("inspector-day")
        checks.append({"check": "the inspector lists rows on each of its four tabs",
                       "ok": all(len(texts) > 2 for texts in report["inspector_day"].values())})
        checks.append({"check": "job and home commands are on the inspector's Home and job tab",
                       "ok": any("Qualified for" in text or "home" in text.lower()
                                 for text in report["inspector_day"]["Home and job"])})

        # 2. The cards of their home and workplace, from the inspector's links.
        name = row.split(" · ")[0]
        cards = {}
        for word in ("Home", "Work"):
            if not flow.card():
                flow.inspect(lambda text: text.startswith(name + " · "), "inspector again")
            links = [text for _, text in flow.button_rows(INSPECTOR) if text.startswith(word + " · ")]
            if not links:
                continue
            flow.press_row(word + " · ", INSPECTOR)
            time.sleep(1.2)
            cards[word] = flow.panel_texts(BUILDING_CARD)
            flow.both(f"building-card-{word.lower()}", flow.park, settle=0.8)
            # Still there after two window sizes: nothing but the player closes it.
            cards[word + " after the captures"] = flow.panel_texts(BUILDING_CARD)
            if flow.has_widget(BUILDING_CARD + " > Close card"):
                flow.click(BUILDING_CARD + " > Close card")
            time.sleep(0.4)
        report["building_cards"] = cards
        checks.append({"check": "a link of the inspector opens a building card in the right column",
                       "ok": bool(cards) and all(cards.values()), "cards": list(cards)})

        # 2b. A real click on a home's roof selects the house: its box is the
        # body the simulation gives it, up to the chimney. The middle of the
        # body first, then higher up the roof.
        home = flow.world().homes("Harbour")[0]
        hx, hz = home["door"][0] / 1000.0, home["door"][1] / 1000.0 - 2.5
        flow.settle(SIZES[0])
        flow.look(hx, hz, 26, "home-roof")
        clicked = []
        for dy in (0, -40):
            flow.pointer_click(640, 360 + dy)
            time.sleep(0.9)
            clicked.append(flow.panel_texts(BUILDING_CARD)[:2])
            if dy:
                flow.snap("home-clicked-on-its-roof-1280x720", SIZES[0])
            if flow.has_widget(BUILDING_CARD + " > Close card"):
                flow.click(BUILDING_CARD + " > Close card")
                time.sleep(0.4)
        report["roof_clicks"] = clicked
        checks.append({"check": "a click on a home's roof opens the home's card",
                       "ok": all(len(texts) == 2 and texts[1].startswith("Home") for texts in clicked),
                       "cards": clicked})

        # 3. The build tray: every category the dock offers.
        flow.whole_island()
        groups = {}
        for group in flow.groups():
            flow.open_group(group)
            groups[group] = [text for _, text in flow.cards()]
            flow.both(f"tray-{flow.slug(group)}", flow.park, settle=0.7)
        report["tray"] = groups
        checks.append({"check": "every category lists cards with a cost and a purpose",
                       "ok": bool(groups) and all(cards and all(card.count("\n") >= 2 for card in cards)
                                                  for cards in groups.values()),
                       "counts": {group: len(cards) for group, cards in groups.items()}})
        # The longest row scrolls sideways under the wheel: the pointer over the
        # cards, then three wheel lines, as the mouse would send them.
        widest = max(groups, key=lambda group: len(groups[group]), default=None)
        if widest is not None and len(groups[widest]) > 3:
            flow.open_group(widest)
            first = flow.cards()[0][0]
            before = flow.centre(first)
            flow.cursor(*before)
            time.sleep(0.4)
            for _ in range(3):
                flow.call("world.write_message", {"message": "bevy_input::mouse::MouseWheel",
                          "value": {"unit": "Line", "x": 0.0, "y": -1.0, "window": flow.window,
                                    "phase": "Moved"}})
                time.sleep(0.15)
            time.sleep(0.5)
            after = flow.centre(first)
            checks.append({"check": "the wheel moves the tray's row of cards sideways",
                           "ok": after[0] < before[0] - 30, "first_card_x": [before[0], after[0]]})
            flow.snap(f"tray-{flow.slug(widest)}-scrolled-1280x720", SIZES[0])

        # 4. A cottage in hand: the ghost and its tag, accepted and refused.
        flow.open_group("Homes")
        flow.press_row("Cottage", TRAY)
        homes = flow.world().homes("Harbour")
        cx = round(sum(home["door"][0] for home in homes) / len(homes) / 1000.0)
        cz = round(sum(home["door"][1] for home in homes) / len(homes) / 1000.0)
        ring = [(0, 0)] + [(round(radius * math.cos(turn)), round(radius * math.sin(turn)))
                           for radius in (8, 13, 18, 24, 30) for turn in
                           [index * math.pi / 6 for index in range(12)]]
        valid, tried = None, []
        for dx, dz in ring:
            x, z = cx + dx, cz + dz
            line, reason = flow.aim(x, z, 34, "search")
            tried.append({"cell": [x, z], "note": line, "reason": reason})
            if line == VALID and reason is None:
                valid = (x, z)
                break
        report["placement_search"] = tried
        if valid is None:
            raise RuntimeError("No buildable spot was found near the Harbour")
        line, reason = flow.aim(valid[0], valid[1], 30, "ghost-valid")
        checks.append({"check": "an accepted ghost has a tag that says so in words",
                       "ok": line == VALID and reason is None, "tag": flow.panel_texts(TAG)})
        flow.shoot("ghost-valid", pointer=True)
        home = homes[0]
        hx, hz = round(home["door"][0] / 1000.0), round(home["door"][1] / 1000.0) - 3
        line, reason = flow.aim(hx, hz, 30, "ghost-invalid")
        checks.append({"check": "a refused ghost has a tag with the simulation's reason",
                       "ok": line == INVALID and bool(reason), "tag": flow.panel_texts(TAG)})
        flow.shoot("ghost-invalid", pointer=True)

        # 5. Place it with a real click, put the building down, close the tray.
        before = set(flow.sites())
        opened = []
        for attempt in range(3):
            line, reason = flow.aim(valid[0], valid[1], 30, "place")
            if line != VALID:
                break
            flow.mouse("Left")
            time.sleep(0.5)
            flow.call("npc/step", {"ticks": 2})
            time.sleep(0.8)
            opened = [sim_id for sim_id in flow.sites() if sim_id not in before]
            if opened:
                break
        checks.append({"check": "a click on an accepted ghost opens one site", "ok": len(opened) == 1})
        flow.mouse("Right")
        time.sleep(0.4)
        flow.close_tray()
        label = flow.sites()[opened[0]]["label"] if opened else None

        # 6. The Projects page, the question before abandoning, the site card.
        flow.open_page("Projects")
        time.sleep(0.6)
        report["projects"] = flow.panel_texts(LEDGER)
        flow.both("ledger-projects", flow.park, settle=0.9)
        if flow.has_widget("What the planner sees"):
            flow.click("What the planner sees")
            time.sleep(0.5)
            flow.both("ledger-projects-planner", flow.park, settle=0.8)
            flow.click("What the planner sees")
        if label is not None:
            flow.click("Abandon " + label)
            time.sleep(0.5)
            asked = flow.has_widget("Abandon it") and flow.has_widget("Keep building")
            flow.both("ledger-projects-abandon", flow.park, settle=0.8)
            flow.click("Keep building")
            checks.append({"check": "abandoning a site asks first", "ok": asked})
            flow.press_row(label, LEDGER)
            time.sleep(1.2)
            report["site_card"] = flow.panel_texts(BUILDING_CARD)
            checks.append({"check": "a project row hands the right column to the site's card",
                           "ok": any(text.startswith("Building site") for text in report["site_card"])
                           and flow.ledger_page() is None})
            flow.both("site-card", flow.park, settle=0.8)
            flow.click(BUILDING_CARD + " > Close card")

        # 7. The Economy page: goods in groups, a good's detail card, the Jobs table.
        flow.whole_island()
        flow.open_page("Economy")
        time.sleep(0.8)
        goods = [text for _, text in flow.button_rows(LEDGER)
                 if text.count("\n") >= 2 and text.split("\n")[1].isdigit()]
        report["economy_goods"] = [text.replace("\n", " | ") for text in goods]
        low = [text for text in goods if text.endswith("Low") or "\nLow" in text]
        report["economy_low"] = [text.split("\n")[0] for text in low]
        if goods:
            chosen = next((text for text in goods if text.startswith("Planks")), goods[0])
            flow.press_row(chosen.split("\n")[0] + "\n", LEDGER)
            time.sleep(0.6)
            texts = flow.panel_texts(LEDGER)
            checks.append({"check": "choosing a good opens its detail card",
                           "ok": any("eference price" in text for text in texts)})
            flow.both("ledger-economy-detail", flow.park, settle=0.9)
        else:
            checks.append({"check": "choosing a good opens its detail card", "ok": False})
        flow.click("Jobs")
        time.sleep(0.6)
        flow.both("ledger-economy-jobs", flow.park, settle=0.8)
        flow.click("Goods")
        flow.close_ledger()

        # 8. Night: the inspector, the tray and the Journal.
        report["ticks"]["night"] = flow.to_phase(0.85)
        flow.whole_island()
        row = flow.inspect(lambda text: True, "inspector by night")
        if row is not None:
            report["inspector_night"] = flow.card()
            flow.both("inspector-night-overview", flow.park, settle=0.9)
            flow.click(INSPECTOR + " > Why")
            time.sleep(0.6)
            flow.both("inspector-night-why", flow.park, settle=0.8)
            flow.click(INSPECTOR + " > Close inspector")
        checks.append({"check": "the inspector was captured at night", "ok": row is not None})
        flow.whole_island()
        flow.open_group("Homes")
        flow.both("tray-homes-night", flow.park, settle=0.7)
        flow.close_tray()
        flow.open_page("Journal")
        time.sleep(0.6)
        report["journal"] = flow.panel_texts(LEDGER)[:30]
        flow.both("ledger-journal", flow.park, settle=0.9)
        flow.click("Everything")
        time.sleep(0.6)
        flow.both("ledger-journal-everything", flow.park, settle=0.9)
        flow.click("Important")
        flow.close_ledger()

        # 9. Compact: Interface size 140% in a 1280x720 window (effective 914x514).
        try:
            flow.to_phase(0.35)
            flow.click("Menu")
            flow.click("Settings")
            flow.click("Display")
            for _ in range(4):
                flow.click("Interface +")
            flow.click("Back")
            flow.click("Resume")
            flow.call("npc/pause")
            time.sleep(1.2)
            flow.park(SIZES[0])
            flow.snap("compact-oversee-1280x720", SIZES[0])
            report["compact_top_bar"] = [text for text in flow.texts() if text][:40]
            # One Ledger tile here; the page is chosen on the ledger's own tabs.
            one_tile = flow.has_widget(capture_player_flow.LEDGER_DOCK + " > Ledger")
            flow.open_page("Economy")
            time.sleep(0.5)
            flow.snap("compact-ledger-1280x720", SIZES[0])
            flow.close_ledger()
            checks.append({"check": "the Compact layout was captured at 140%, with one Ledger tile",
                           "ok": one_tile})
        except (RuntimeError, KeyError) as error:
            checks.append({"check": "the Compact layout was captured at 140%", "ok": False,
                           "reason": str(error)})
        finally:
            try:
                flow.click("Menu")
                flow.click("Settings")
                flow.click("Display")
                for _ in range(4):
                    flow.click("Interface −")
                flow.click("Back")
                flow.click("Resume")
                flow.call("npc/pause")
            except (RuntimeError, KeyError) as error:
                report["compact_restore_error"] = str(error)
    finally:
        stop(child, run, report, "oversee")
    return run


def gallery(binary, base, report):
    """The card states of the kit: the unavailable card is seen here only."""
    run = base / "gallery"
    child, url = launch(binary, run, ["--ui-gallery", "data"])
    try:
        flow = Phase3Flow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 10 * 60
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        time.sleep(1.0)
        flow.press("Data")
        time.sleep(0.6)
        texts = flow.texts()
        report["checks"].append({"check": "the gallery shows an unavailable card with its reason",
                                 "ok": "Needs a carpenter in the village." in texts})
        flow.both("gallery-unavailable-card")
    finally:
        stop(child, run, report, "gallery")
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-redesign-p3")
    parser.add_argument("--skip-gallery", action="store_true")
    parser.add_argument("--skip-oversee", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    base = ROOT / "target" / ("playtest-ui-phase3-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation by text or control name; pointer through window "
                 "cursor position and MouseButtonInput messages; view camera placement; the "
                 "simulation clock stepped while paused",
        "keyboard_input_tested": False, "screenshots": {}, "checks": []}
    try:
        if not args.skip_oversee:
            oversee(binary, base, report)
        if not args.skip_gallery:
            gallery(binary, base, report)
        report["ok"] = all(check["ok"] for check in report["checks"])
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for run in [base / "oversee", base / "gallery"]:
            if (run / "shots").is_dir():
                for image in sorted((run / "shots").glob("*.png")):
                    shutil.copy2(image, args.out / image.name)
                    copied.append(str(args.out / image.name))
        report["copied"] = copied
        (args.out / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                               encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
