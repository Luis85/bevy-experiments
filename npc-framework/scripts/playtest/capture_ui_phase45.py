"""Native captures of the redesigned interface, spec phases 4 and 5, and of the
phase 3 follow-ups: the Live HUD and its workspaces, Shape's rail, brush bar
and cursor tag, and the Oversee pages that changed, at 1280x720 and 1024x768.

Build first, then run from npc-framework:
  python scripts/playtest/capture_ui_phase45.py [--binary target/debug/game_menu.exe]
      [--only oversee,keys,shape,live,compact,night,pouch,save]

One disposable run of the real game, driven over BRP like
`capture_ui_phase3.py`:

1. Oversee: a resident's inspector (family and bed, the world tag over them),
   the Village page with its census, the Projects page, the calendar with the
   days ahead, the goods table sorted by a header, a building card on its
   tabs, a building in hand at 1024 wide, and the Compact layout (Interface
   size 140%).
2. Keys: Tab, Esc, B and F1 to F5 are sent as `KeyboardInput` messages, the
   same messages the window sends, and what the interface did is read back.
3. Shape: each tool with its cursor tag, a stroke while the button is held
   (the real pointer path), undo, a refusal over a building, and Shape's keys.
4. Live: the HUD idle, a walk under way, something in reach, each of the
   seven workspaces, Live's keys, the pouch with an item chosen, recipes, the
   Compact layout, and the HUD at night.
5. Not part of the default run: `--only compact` (the Economy page at the
   Compact size alone), `--only pouch` (water drawn and chosen in the pouch)
   and `--only save` (Ctrl+S in Shape and in Live, for where the host's save
   notice stands) and `--only night` (Oversee at night: a sleeper's inspector
   and the inn's card with its guest beds). They can be combined, in this
   order: compact,night,pouch,save.

Controls are found by the text on them or by their `Name`. A section that
fails is recorded as a failed check and the run goes on. PNGs go to
target/npc-out/ui-redesign-p45/ and still need to be read.
"""
import argparse
import json
from pathlib import Path
import shutil
import time
import uuid

import capture_player_flow
from capture_build_menu import TAG
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import (BUILD_DOCK, BUILDING_CARD, INSPECTOR, LEDGER, PAGES, PRIMARY,
                                 TRAY, wait_for)
from capture_ui_phase3 import Phase3Flow
from capture_ui_redesign import SIZES, launch, stop
from capture_view_checks import DAY
from capture_view_followups import Terrain
from capture_view_wiring import WiringFlow

capture_player_flow.SIZES = SIZES
KEYBOARD = "bevy_input::keyboard::KeyboardInput"
RAIL = "Shape tools"
BRUSH = "Shape brush"
SHAPE_TAG = "Shape cursor tag"
GOAL = "Live goal"
NEEDS = "Live needs"
HOTBAR = "Live hotbar"
PROMPT = "Live prompt"
WORKSPACE = "Player workspace"
CALENDAR = "Island calendar"
WORKSPACES = (("Pouch", "I"), ("Craft", "C"), ("Build", "B"), ("Garden", "G"),
              ("Trade", "T"), ("People", "N"), ("Explore", "M"))


def logical(code):
    """The logical key that goes with a physical one on a US layout."""
    if code.startswith("Key"):
        return {"Character": code[3:].lower()}
    return {"BracketLeft": {"Character": "["}, "BracketRight": {"Character": "]"},
            "Equal": {"Character": "="}, "Minus": {"Character": "-"},
            "ShiftLeft": "Shift", "ControlLeft": "Control"}.get(code, code)


class Phase45Flow(Phase3Flow):
    def key(self, code, hold=()):
        """One key press and release as the window reports it; `hold` are
        modifier keys held around it."""
        def send(key, state):
            self.call("world.write_message", {"message": KEYBOARD, "value": {
                "key_code": key, "logical_key": logical(key), "state": state,
                "text": None, "repeat": False, "window": self.window}})
            time.sleep(0.12)

        for modifier in hold:
            send(modifier, "Pressed")
        send(code, "Pressed")
        send(code, "Released")
        for modifier in reversed(hold):
            send(modifier, "Released")
        time.sleep(0.45)
        self.report.setdefault("keys_sent", []).append("+".join([*hold, code]))

    def focused(self):
        """What holds the keyboard focus: the control's name, or None."""
        try:
            value = self.call("world.get_resources", {"resource": "bevy_input_focus::InputFocus"})
        except (RuntimeError, KeyError, OSError) as error:
            return f"unreadable: {error}"
        value = value.get("value", value) if isinstance(value, dict) else value
        while isinstance(value, (list, dict)) and value:
            value = value[0] if isinstance(value, list) else next(iter(value.values()))
        if not isinstance(value, int):
            return None
        return next((name for name, entities in self.names().items() if value in entities),
                    f"entity {value}")

    def hold(self, pressed):
        self.call("world.write_message", {"message": "bevy_input::mouse::MouseButtonInput",
                  "value": {"button": "Left", "state": "Pressed" if pressed else "Released",
                            "window": self.window}})

    def sweep(self, start, end, seconds=1.6, pause_at=None, then=None):
        """Press at `start`, move to `end`, release. With `pause_at` (a share
        of the way), `then()` runs there while the button is still held."""
        steps = max(8, int(seconds / 0.05))
        self.cursor(*start)
        time.sleep(0.5)
        self.hold(True)
        try:
            for step in range(1, steps + 1):
                share = step / steps
                self.cursor(start[0] + (end[0] - start[0]) * share,
                            start[1] + (end[1] - start[1]) * share)
                time.sleep(seconds / steps)
                if pause_at is not None and then is not None and share >= pause_at:
                    then()
                    then = None
        finally:
            self.hold(False)
        time.sleep(0.8)

    def section(self, name, body):
        """Run one part of the capture; a failure is a failed check."""
        try:
            body()
            return True
        except Exception as error:  # noqa: BLE001: every failure is reported, none hidden.
            self.report["checks"].append({"check": f"section {name} ran to its end", "ok": False,
                                          "reason": f"{type(error).__name__}: {error}"})
            print(f"section {name} failed: {error}", flush=True)
            try:
                self.snap(f"failed-{name}-1280x720", SIZES[0])
            except Exception:  # noqa: BLE001
                pass
            return False


def oversee(flow, report, checks):
    # 1. A working resident: the family and bed lines, and the tag over them.
    flow.open_page("People")
    time.sleep(0.6)
    row = flow.inspect(lambda text: not text.split("\n")[1].startswith(("sleeping", "resting")),
                       "inspector by day")
    card = flow.card()
    report["inspector_overview"] = card
    checks.append({"check": "the inspector's Overview has the family and bed lines",
                   "ok": "Family and bed" in card, "row": row})
    flow.both("gap-inspector-family-bed", flow.park, settle=0.9)
    # Closer, so the tag over the selected resident can be read.
    entity, _ = flow.camera_entity()
    flow.call("world.mutate_components", {"entity": entity,
              "component": "testbed_view::camera::RtsCamera", "path": "distance", "value": 16.0})
    time.sleep(1.0)
    flow.both("gap-world-tag", flow.park, settle=0.9)
    report["inspector_day_tab"] = flow.card("Day")
    report["inspector_why_tab"] = flow.card("Why")
    flow.both("gap-inspector-why", flow.park, settle=0.8)
    report["inspector_home_tab"] = flow.card("Home and job")
    flow.both("gap-inspector-home-and-job", flow.park, settle=0.8)
    checks.append({"check": "no line of the inspector counts ticks",
                   "ok": not any(" ticks" in text for tab in ("inspector_overview", "inspector_day_tab",
                                 "inspector_why_tab", "inspector_home_tab") for text in report[tab])})
    flow.card("Overview")

    # 2. The home's card on its tabs.
    links = [text for _, text in flow.button_rows(INSPECTOR) if text.startswith("Home · ")]
    if links:
        flow.press_row("Home · ", INSPECTOR)
        time.sleep(1.2)
        tabs = {"Overview": flow.panel_texts(BUILDING_CARD)}
        flow.both("gap-card-overview", flow.park, settle=0.8)
        for tab in ("People", "Stock"):
            if flow.has_widget(BUILDING_CARD + " > " + tab):
                flow.click(BUILDING_CARD + " > " + tab)
                time.sleep(0.6)
                tabs[tab] = flow.panel_texts(BUILDING_CARD)
                flow.both(f"gap-card-{tab.lower()}", flow.park, settle=0.8)
        report["card_tabs"] = tabs
        checks.append({"check": "the building card has tabs and each shows its own lines",
                       "ok": len(tabs) >= 2 and len({tuple(lines) for lines in tabs.values()}) == len(tabs),
                       "tabs": list(tabs)})
        flow.click(BUILDING_CARD + " > Close card")
    else:
        checks.append({"check": "the building card has tabs and each shows its own lines",
                       "ok": False, "reason": "the resident has no home link"})
    time.sleep(0.4)
    flow.whole_island()

    # 3. Village: the census. Projects: the wording.
    flow.open_page("Village")
    time.sleep(0.6)
    village = flow.panel_texts(LEDGER)
    report["village_page"] = village
    checks.append({"check": "the Village page has the census",
                   "ok": "Families" in village and any(text.startswith(("Since the island was founded", "Nobody has been born"))
                                                       for text in village)})
    flow.both("gap-village-census", flow.park, settle=0.9)
    flow.open_page("Projects")
    time.sleep(0.6)
    projects = flow.panel_texts(LEDGER)
    report["projects_page"] = projects
    checks.append({"check": "the Projects page counts what it lists, in words",
                   "ok": "Homes residents build themselves" in projects
                   and not any("active construction projects" in text for text in projects)})
    flow.both("gap-projects-wording", flow.park, settle=0.9)

    # 4. Economy: a header sorts its column.
    flow.open_page("Economy")
    time.sleep(0.8)

    def goods():
        return [text.split("\n")[0] for _, text in flow.button_rows(LEDGER)
                if text.count("\n") >= 2 and text.split("\n")[1].isdigit()]

    before = goods()
    flow.click(LEDGER + " > Sort by price")
    time.sleep(0.6)
    upwards = goods()
    flow.both("gap-economy-sorted-by-price", flow.park, settle=0.8)
    flow.click(LEDGER + " > Sort by price")
    time.sleep(0.6)
    downwards = goods()
    report["economy_sort"] = {"before": before, "price": upwards, "price_reversed": downwards}
    checks.append({"check": "a header of the goods table sorts, and again reverses",
                   "ok": sorted(before) == sorted(upwards) == sorted(downwards)
                   and upwards != downwards})
    flow.click(LEDGER + " > Sort by price")
    flow.close_ledger()

    # 5. The calendar: the days ahead and the boat.
    flow.press("Calendar")
    time.sleep(0.6)
    calendar = flow.panel_texts(CALENDAR)
    report["calendar"] = calendar
    checks.append({"check": "the calendar tells the weather ahead and the boat",
                   "ok": any("omorrow" in text or "boat" in text.lower() for text in calendar)})
    flow.both("gap-calendar-forecast", flow.park, settle=0.8)
    flow.press("Calendar")
    time.sleep(0.4)

    # 6. A building in hand: the totals at 1024 wide.
    flow.open_group("Homes")
    flow.press_row("Cottage", TRAY)
    time.sleep(0.5)
    flow.both("gap-building-in-hand", flow.park, settle=0.8)
    flow.mouse("Right")
    time.sleep(0.4)
    flow.close_tray()

    compact_economy(flow, report, checks)


def compact_economy(flow, report, checks):
    """The Economy page at the Compact size: Interface size 140% in a
    1280x720 window."""
    flow.settle(SIZES[0])
    try:
        flow.click("Menu")
        flow.click("Settings")
        flow.click("Display")
        for _ in range(4):
            flow.click("Interface +")
        flow.click("Back")
        flow.click("Resume")
        flow.call("npc/pause")
        time.sleep(1.2)
        flow.open_page("Economy")
        time.sleep(0.6)
        flow.park(SIZES[0])
        flow.snap("gap-compact-economy-1280x720", SIZES[0])
        rows = [text for _, text in flow.button_rows(LEDGER)
                if text.count("\n") >= 2 and text.split("\n")[1].isdigit()]
        if rows:
            flow.press_row(rows[0].split("\n")[0] + "\n", LEDGER)
            time.sleep(0.6)
            flow.snap("gap-compact-economy-detail-1280x720", SIZES[0])
        flow.close_ledger()
        checks.append({"check": "the Compact economy page was captured", "ok": True})
    finally:
        flow.click("Menu")
        flow.click("Settings")
        flow.click("Display")
        for _ in range(4):
            flow.click("Interface −")
        flow.click("Back")
        flow.click("Resume")
        flow.call("npc/pause")
        time.sleep(0.8)


def keys(flow, report, checks):
    """Tab, Esc, B and F1 to F5 as key messages, in Oversee."""
    seen = {}
    flow.park(SIZES[0])
    for index, page in enumerate(PAGES, start=1):
        flow.key(f"F{index}")
        seen[f"F{index}"] = flow.ledger_page()
    checks.append({"check": "F1 to F5 open the ledger's five pages",
                   "ok": [seen[f"F{index}"] for index in range(1, 6)] == list(PAGES), "seen": dict(seen)})
    flow.snap("keys-f5-journal-1280x720", SIZES[0])
    flow.key("F5")
    seen["F5 again"] = flow.ledger_page()
    checks.append({"check": "the page's key closes it again", "ok": seen["F5 again"] is None})
    # Tab walks the ring: the bar first, then on.
    ring = []
    for _ in range(4):
        flow.key("Tab")
        ring.append(flow.focused())
    seen["Tab"] = ring
    checks.append({"check": "Tab moves the keyboard focus from control to control",
                   "ok": len({name for name in ring if name}) >= 2, "focus": ring})
    flow.snap("keys-tab-focus-1280x720", SIZES[0])
    flow.key("Tab", hold=("ShiftLeft",))
    seen["Shift+Tab"] = flow.focused()
    flow.key("Escape")
    seen["Esc with focus"] = {"focus": flow.focused(), "menu": flow.has_widget("Resume")}
    # B: the build tray; Esc puts it away; Esc with nothing open is the menu.
    flow.key("KeyB")
    seen["B"] = bool(flow.panel_texts(TRAY))
    flow.snap("keys-b-build-tray-1280x720", SIZES[0])
    flow.key("Escape")
    seen["Esc on the tray"] = {"tray": bool(flow.panel_texts(TRAY)), "menu": flow.has_widget("Resume")}
    checks.append({"check": "B opens the build tray and Esc closes it without opening the menu",
                   "ok": seen["B"] and not seen["Esc on the tray"]["tray"]
                   and not seen["Esc on the tray"]["menu"], "seen": seen["Esc on the tray"]})
    flow.key("F3")
    flow.key("Escape")
    seen["Esc on the ledger"] = {"page": flow.ledger_page(), "menu": flow.has_widget("Resume")}
    flow.key("Escape")
    time.sleep(0.6)
    seen["Esc on the island"] = flow.has_widget("Resume")
    checks.append({"check": "Esc closes the ledger first and opens the menu last",
                   "ok": seen["Esc on the ledger"] == {"page": None, "menu": False}
                   and seen["Esc on the island"], "seen": seen})
    flow.snap("keys-esc-menu-1280x720", SIZES[0])
    if seen["Esc on the island"]:
        flow.click("Resume")
        flow.call("npc/pause")
    report["keys_oversee"] = seen


def shape(flow, report, checks):
    x, z, clearance = WiringFlow.open_ground(flow)
    report["shape_ground"] = {"cell": [x, z], "clearance_m": clearance}
    flow.click("Shape")
    wait_for(lambda: flow.panel_texts(RAIL), 10)
    flow.look(x, z, 40, "shape")
    flow.call("npc/resume")
    time.sleep(0.6)
    centre = {size: (size[0] / 2, size[1] / 2) for size in SIZES}

    def over_ground(size):
        flow.cursor(*centre[size])
        time.sleep(0.7)

    tags = {}
    for tool in ("Raise", "Lower", "Flatten", "Smooth", "Paint"):
        flow.click(RAIL + " > " + tool)
        over_ground(SIZES[0])
        tags[tool] = flow.panel_texts(SHAPE_TAG)
        flow.both(f"shape-{tool.lower()}", over_ground, settle=0.6)
    report["shape_tags"] = tags
    checks.append({"check": "the cursor tag names the tool and the ground, never a cell",
                   "ok": all(lines and lines[0].startswith(tool + " · ") and "cell" not in lines[0]
                             for tool, lines in tags.items()), "tags": tags})
    report["shape_rail"] = flow.panel_texts(RAIL)
    report["shape_brush"] = flow.panel_texts(BRUSH)
    flow.click(BRUSH + " > Stone")
    over_ground(SIZES[0])
    flow.both("shape-paint-stone", over_ground, settle=0.6)

    # A stroke in progress, by the real pointer: Raise, soft, a little wider.
    flow.click(RAIL + " > Raise")
    flow.click(BRUSH + " > Soft")
    flow.key("BracketRight")
    flow.key("BracketRight")
    reach = max(40.0, min(140.0, (clearance - 9.5) / 0.046))
    depth_before = [text for text in flow.panel_texts(RAIL) if " of " in text]
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.6)
        middle = centre[size]
        flow.sweep((middle[0] - reach, middle[1]), (middle[0] + reach, middle[1]), pause_at=0.6,
                   then=lambda size=size: flow.snap(f"shape-stroke-in-progress-{size[0]}x{size[1]}", size))
        time.sleep(1.2)
        flow.snap(f"shape-after-stroke-{size[0]}x{size[1]}", size)
    flow.settle(SIZES[0])
    depth_after = [text for text in flow.panel_texts(RAIL) if " of " in text]
    checks.append({"check": "two strokes show as history depth under Undo",
                   "ok": depth_before == ["0 of 32"] and depth_after == ["2 of 32"],
                   "depth": [depth_before, depth_after]})
    # Undo, once by its tile and once by Ctrl+Z.
    flow.click(RAIL + " > Undo")
    time.sleep(0.7)
    report["shape_undo_notice"] = flow.panel_texts("Shape status")
    flow.snap("shape-undo-notice-1280x720", SIZES[0])
    time.sleep(1.0)
    report["shape_after_undo"] = flow.panel_texts(RAIL)
    flow.both("shape-after-undo", over_ground, settle=0.8)
    flow.key("KeyZ", hold=("ControlLeft",))
    time.sleep(1.5)
    depth_undone = [text for text in flow.panel_texts(RAIL) if " of " in text]
    # A quick stroke on fresh ground beside the first: no dab of it is
    # refused, so it can be taken back and laid again. (A stroke held still
    # until the simulation refuses a dab keeps the refused dab in its history,
    # and the simulation then refuses its redo.)
    middle = centre[SIZES[0]]
    flow.sweep((middle[0] - reach, middle[1] + 90), (middle[0] + reach, middle[1] + 90), seconds=0.7)
    time.sleep(1.5)
    depth_quick = [text for text in flow.panel_texts(RAIL) if " of " in text]
    flow.key("KeyZ", hold=("ControlLeft",))
    time.sleep(1.5)
    depth_back = [text for text in flow.panel_texts(RAIL) if " of " in text]
    flow.key("KeyY", hold=("ControlLeft",))
    time.sleep(0.8)
    report["shape_redo_notice"] = flow.panel_texts("Shape status")
    time.sleep(0.8)
    depth_redone = [text for text in flow.panel_texts(RAIL) if " of " in text]
    checks.append({"check": "Undo's tile and Ctrl+Z take strokes back; Ctrl+Y lays one again",
                   "ok": depth_undone == ["0 of 32"] and depth_quick == ["1 of 32"]
                   and depth_back == ["0 of 32"] and depth_redone == ["1 of 32"],
                   "depth": [depth_undone, depth_quick, depth_back, depth_redone],
                   "notice": report["shape_redo_notice"]})
    flow.snap("shape-after-redo-1280x720", SIZES[0])
    flow.key("KeyZ", hold=("ControlLeft", "ShiftLeft"))
    time.sleep(1.0)
    report["shape_ctrl_shift_z"] = [text for text in flow.panel_texts(RAIL) if " of " in text]
    flow.key("KeyZ", hold=("ControlLeft",))
    time.sleep(1.2)

    # Shape's own keys.
    seen = {}
    for code, tool in (("KeyR", "Raise"), ("KeyF", "Lower"), ("KeyT", "Flatten"), ("KeyG", "Smooth"),
                       ("KeyP", "Paint")):
        flow.key(code)
        over_ground(SIZES[0])
        seen[code] = flow.panel_texts(SHAPE_TAG)[:1]
    size_before = flow.panel_texts(BRUSH)
    flow.key("BracketLeft")
    size_after = flow.panel_texts(BRUSH)
    seen["["] = [[text for text in texts if text.endswith(" m")] for texts in (size_before, size_after)]
    flow.key("KeyV")
    over_ground(SIZES[0])
    seen["V"] = flow.panel_texts(SHAPE_TAG)[:1]
    flow.key("KeyH")
    report["keys_shape"] = seen
    checks.append({"check": "R F T G P choose the tools and [ makes the brush smaller",
                   "ok": [seen[code][0].split(" · ")[0] if seen[code] else None
                          for code in ("KeyR", "KeyF", "KeyT", "KeyG", "KeyP")]
                   == ["Raise", "Lower", "Flatten", "Smooth", "Paint"]
                   and seen["["][0] != seen["["][1], "seen": seen})

    # A refusal: the brush over a home.
    home = flow.world().homes("Harbour")[0]
    hx, hz = home["door"][0] / 1000.0, home["door"][1] / 1000.0 - 2.5
    flow.click(RAIL + " > Raise")
    flow.look(hx, hz, 30, "shape-refused")
    over_ground(SIZES[0])
    time.sleep(0.6)
    refused = flow.panel_texts(SHAPE_TAG)
    report["shape_refused_tag"] = refused
    checks.append({"check": "over a building the tag gives the simulation's reason",
                   "ok": len(refused) >= 2, "tag": refused})
    flow.both("shape-refused", over_ground, settle=0.8)
    flow.call("npc/pause")
    flow.park(SIZES[0])
    flow.click("Oversee")
    time.sleep(0.6)
    flow.close_ledger()


def live(flow, report, checks):
    flow.whole_island()
    flow.click("Live")
    flow.call("npc/resume")
    wait_for(lambda: flow.panel_texts(HOTBAR), 60)
    time.sleep(2.5)
    flow.call("npc/pause")
    report["live_goal"] = flow.panel_texts(GOAL)
    report["live_needs"] = flow.panel_texts(NEEDS)
    report["live_hotbar"] = flow.panel_texts(HOTBAR)
    report["live_prompt_idle"] = flow.panel_texts(PROMPT)
    checks.append({"check": "the Live HUD has the goal, five needs and coins, and the hotbar",
                   "ok": all(word in report["live_needs"] for word in
                             ("Health", "Food", "Water", "Energy", "Warmth", "Coins"))
                   and [text for text in report["live_hotbar"] if text in dict(WORKSPACES)]
                   == [name for name, _ in WORKSPACES] and bool(report["live_goal"])})
    flow.both("live-hud-idle", flow.park, settle=0.9)

    # The seven workspaces, by their tiles.
    spaces = {}
    for name, _ in WORKSPACES:
        flow.click(HOTBAR + " > " + name)
        time.sleep(0.9)
        spaces[name] = flow.panel_texts(WORKSPACE)
        flow.both(f"live-workspace-{name.lower()}", flow.park, settle=0.8)
    report["live_workspaces"] = spaces
    checks.append({"check": "each of the seven workspaces opens in the right column",
                   "ok": all(spaces.get(name) for name, _ in WORKSPACES)})
    flow.click(WORKSPACE + " > Close workspace")
    time.sleep(0.5)
    goal_folded = flow.panel_texts(GOAL)

    # Live's keys.
    seen = {}
    for name, code in WORKSPACES:
        flow.key("Key" + code)
        seen[code] = (flow.panel_texts(WORKSPACE) or [None])[0]
    flow.snap("keys-live-m-explore-1280x720", SIZES[0])
    flow.key("Tab")
    seen["Tab in a workspace"] = flow.focused()
    flow.snap("keys-live-tab-in-workspace-1280x720", SIZES[0])
    flow.key("Escape")
    seen["Esc on a workspace"] = {"workspace": bool(flow.panel_texts(WORKSPACE)),
                                  "menu": flow.has_widget("Resume")}
    checks.append({"check": "I C B G T N M open the workspaces and Esc closes the open one",
                   "ok": all(seen[code] for _, code in WORKSPACES)
                   and seen["Esc on a workspace"] == {"workspace": False, "menu": False}, "seen": seen})
    # Shift+Tab from the world enters the ring at its end, the hotbar, and
    # walks back over the needs and the goal to the bar, then off its start.
    ring = []
    for _ in range(30):
        flow.key("Tab", hold=("ShiftLeft",))
        ring.append(flow.focused())
        if ring[-1] is None or flow.panel_texts(WORKSPACE):
            break
        if ring[-1] == "Game: Water":
            flow.snap("keys-live-focus-on-a-need-1280x720", SIZES[0])
    seen["Shift+Tab from the world, repeated"] = ring
    checks.append({"check": "in Live, Shift+Tab walks the hotbar, the needs, the goal and the bar",
                   "ok": all(name in ring for name in ("Game: Stop", "Game: Pouch", "Game: Health",
                                                       "Game: Fold goal", "Game: Menu")),
                   "focus": ring})
    if ring and ring[-1] is not None:
        # Esc hands the keyboard back to the world.
        flow.key("Escape")
    if flow.has_widget("Resume"):
        flow.click("Resume")
        flow.call("npc/pause")
    if flow.panel_texts(WORKSPACE):
        flow.click(WORKSPACE + " > Close workspace")
    report["keys_live"] = seen
    report["live_goal_after_workspace"] = goal_folded

    # A walk under way, then something in reach: the goal's own button.
    flow.call("npc/resume")
    buttons = [text for _, text in flow.button_rows(GOAL)]
    report["live_goal_buttons"] = buttons
    working = reach = None
    if buttons:
        flow.press_row(buttons[-1].split("\n")[0], GOAL)
    end = time.monotonic() + 150
    while time.monotonic() < end and (working is None or reach is None):
        flow.check()
        time.sleep(0.4)
        prompt = flow.panel_texts(PROMPT)
        if not prompt:
            continue
        if "Stop" in prompt and working is None:
            flow.call("npc/pause")
            working = prompt
            flow.both("live-prompt-working", flow.park, settle=0.6)
            flow.call("npc/resume")
        elif "E" in prompt and "Stop" not in prompt and reach is None:
            flow.call("npc/pause")
            reach = prompt
            flow.both("live-prompt-in-reach", flow.park, settle=0.6)
            flow.call("npc/resume")
    report["live_prompt_working"] = working
    report["live_prompt_reach"] = reach
    checks.append({"check": "work under way shows as a pill with Stop", "ok": working is not None})
    checks.append({"check": "something in reach shows as a prompt with its key cap",
                   "ok": reach is not None})
    if reach is not None:
        flow.key("KeyE")
        time.sleep(1.5)
        report["live_after_e"] = {"prompt": flow.panel_texts(PROMPT),
                                  "workspace": flow.panel_texts(WORKSPACE)[:3]}
        flow.snap("keys-live-e-1280x720", SIZES[0])
    flow.key("KeyX")
    time.sleep(0.8)
    flow.call("npc/pause")
    if flow.panel_texts(WORKSPACE):
        flow.click(WORKSPACE + " > Close workspace")

    # Water from the well, then the pouch: an item is chosen before it is used.
    def idle(seconds):
        # The walk shows as the working pill a moment after the request.
        started = time.monotonic() + 8
        while time.monotonic() < started and "Stop" not in flow.panel_texts(PROMPT):
            time.sleep(0.3)
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            flow.check()
            time.sleep(0.5)
            if "Stop" not in flow.panel_texts(PROMPT):
                return True
        return False

    def pouch_and_recipes():
        flow.call("npc/resume")
        flow.click(HOTBAR + " > Craft")
        time.sleep(0.8)
        flow.press_row("Gather Water", WORKSPACE)
        time.sleep(1.5)
        idle(60)
        flow.click(HOTBAR + " > Pouch")
        time.sleep(1.0)
        slots = [(entity, text) for entity, text in flow.button_rows(WORKSPACE)
                 if len(text.splitlines()) == 2 and text.splitlines()[0].isdigit()]
        items = [text.replace("\n", " ") for _, text in slots]
        report["live_pouch_items"] = items
        if slots:
            flow.activate(slots[0][0], "a pouch slot")
            time.sleep(0.8)
            flow.call("npc/pause")
            report["live_pouch_item_chosen"] = flow.panel_texts(WORKSPACE)
            flow.both("live-pouch-item-chosen", flow.park, settle=0.8)
            flow.call("npc/resume")
        checks.append({"check": "a gathered item is chosen in the pouch and then offers what to do",
                       "ok": bool(items) and any(word in report.get("live_pouch_item_chosen", [])
                                                 for word in ("Drink", "Eat", "Offer for sale")),
                       "items": items})
        # The village workshops: recipes with what they take.
        flow.click(HOTBAR + " > Explore")
        time.sleep(0.8)
        flow.press_row("Travel to Harbour", WORKSPACE)
        time.sleep(1.5)
        idle(90)
        flow.click(HOTBAR + " > Craft")
        time.sleep(1.0)
        flow.call("npc/pause")
        craft = flow.panel_texts(WORKSPACE)
        report["live_craft_after_walk"] = craft
        recipes = [text for text in craft if text.startswith("Make ") and text != "Make something useful"]
        flow.both("live-craft-with-pouch", flow.park, settle=0.8)
        checks.append({"check": "Craft lists a known workshop's recipes with what they take",
                       "ok": bool(recipes), "recipes": recipes[:6]})

    flow.section("live pouch and recipes", pouch_and_recipes)
    flow.call("npc/pause")
    if flow.panel_texts(WORKSPACE):
        flow.click(WORKSPACE + " > Close workspace")

    # Compact: Interface size 140% in a 1280x720 window.
    def compact():
        try:
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
            flow.snap("live-compact-hud-1280x720", SIZES[0])
            flow.click(HOTBAR + " > Craft")
            time.sleep(0.8)
            flow.snap("live-compact-craft-1280x720", SIZES[0])
            flow.click(WORKSPACE + " > Close workspace")
        finally:
            flow.click("Menu")
            flow.click("Settings")
            flow.click("Display")
            for _ in range(4):
                flow.click("Interface −")
            flow.click("Back")
            flow.click("Resume")
            flow.call("npc/pause")
            time.sleep(0.8)

    flow.section("live compact", compact)

    # Night.
    report.setdefault("ticks", {})["live_night"] = flow.to_phase(0.88)
    time.sleep(1.0)
    flow.both("live-night-hud", flow.park, settle=0.9)
    flow.click(HOTBAR + " > Pouch")
    time.sleep(0.8)
    flow.both("live-night-pouch", flow.park, settle=0.8)
    flow.click(HOTBAR + " > Trade")
    time.sleep(0.8)
    flow.both("live-night-trade", flow.park, settle=0.8)
    flow.click(WORKSPACE + " > Close workspace")


def pouch(flow, report, checks):
    """Enter Live, draw water at the well, choose it in the pouch."""
    flow.whole_island()
    flow.click("Live")
    flow.call("npc/resume")
    wait_for(lambda: flow.panel_texts(HOTBAR), 60)
    time.sleep(2.5)
    flow.click(HOTBAR + " > Craft")
    time.sleep(0.8)
    flow.press_row("Gather Water", WORKSPACE)
    started = time.monotonic() + 8
    while time.monotonic() < started and "Stop" not in flow.panel_texts(PROMPT):
        time.sleep(0.3)
    end = time.monotonic() + 90
    while time.monotonic() < end and "Stop" in flow.panel_texts(PROMPT):
        flow.check()
        time.sleep(0.5)
    time.sleep(1.0)
    flow.click(HOTBAR + " > Pouch")
    time.sleep(1.0)
    slots = [(entity, text) for entity, text in flow.button_rows(WORKSPACE)
             if len(text.splitlines()) == 2 and text.splitlines()[0].isdigit()]
    report["pouch_slots"] = [text.replace("\n", " ") for _, text in slots]
    flow.call("npc/pause")
    flow.both("live-pouch-with-water", flow.park, settle=0.8)
    if slots:
        flow.activate(slots[0][0], "a pouch slot")
        time.sleep(0.8)
        report["pouch_item_chosen"] = flow.panel_texts(WORKSPACE)
        flow.both("live-pouch-item-chosen", flow.park, settle=0.8)
    checks.append({"check": "a gathered item is chosen in the pouch and then offers what to do",
                   "ok": bool(slots) and any(word in report.get("pouch_item_chosen", [])
                                             for word in ("Drink", "Eat", "Offer for sale")),
                   "slots": report["pouch_slots"]})
    # Two panes beside whatever the prompt says at the well.
    flow.click(HOTBAR + " > Craft")
    time.sleep(0.8)
    report["prompt_beside_two_panes"] = flow.panel_texts(PROMPT)
    flow.both("live-prompt-beside-two-panes", flow.park, settle=0.8)
    flow.click(WORKSPACE + " > Close workspace")


def night(flow, report, checks):
    """Oversee at night: a sleeper's inspector, and the inn with its guest beds."""
    flow.settle(SIZES[0])
    report.setdefault("ticks", {})["night"] = flow.to_phase(0.93)
    flow.whole_island()
    flow.open_page("People")
    time.sleep(0.6)
    report["night_roster"] = [text.replace("\n", " | ") for _, text in flow.rows()][:12]
    row = flow.inspect(lambda text: "asleep" in text or "sleeping" in text, "a sleeper")
    if row is None:
        row = flow.inspect(lambda text: True, "a resident at night")
    overview = flow.card()
    flow.both("night-inspector-overview", flow.park, settle=0.9)
    why = flow.card("Why")
    flow.both("night-inspector-why", flow.park, settle=0.8)
    day = flow.card("Day")
    report["night_inspector"] = {"row": row, "overview": overview, "why": why, "day": day}
    checks.append({"check": "a sleeper's inspector says they are asleep, in bed and why",
                   "ok": any("sleep" in text.lower() for text in overview)
                   and any("asleep" in text for text in overview if text.startswith("Bed "))
                   and len(why) > 4, "row": row})
    flow.click(INSPECTOR + " > Close inspector")
    time.sleep(0.4)
    # The inn, through its keeper's Work link.
    flow.open_page("People")
    time.sleep(0.6)
    keeper = flow.inspect(lambda text: "innkeeper" in text.split("\n")[0], "the innkeeper")
    inn = {}
    if keeper is not None and any(text.startswith("Work \u00b7 ")
                                  for _, text in flow.button_rows(INSPECTOR)):
        flow.press_row("Work \u00b7 ", INSPECTOR)
        time.sleep(1.2)
        inn["Overview"] = flow.panel_texts(BUILDING_CARD)
        flow.both("night-inn-card-overview", flow.park, settle=0.8)
        if flow.has_widget(BUILDING_CARD + " > People"):
            flow.click(BUILDING_CARD + " > People")
            time.sleep(0.6)
            inn["People"] = flow.panel_texts(BUILDING_CARD)
            flow.both("night-inn-card-guests", flow.park, settle=0.8)
        flow.click(BUILDING_CARD + " > Close card")
    report["night_inn_card"] = inn
    checks.append({"check": "the inn's card lists its guest beds",
                   "ok": any(text.startswith("Guest beds ") for text in inn.get("Overview", []))
                   and any(text.startswith("Bed 1 ") for text in inn.get("People", [])),
                   "keeper": keeper, "card": inn})
    flow.whole_island()


def save_notice(flow, report, checks):
    """Ctrl+S in Shape and in Live: where the host's save notice stands."""
    seen = {}
    flow.click("Shape")
    wait_for(lambda: flow.panel_texts(RAIL), 10)
    flow.park(SIZES[0])
    flow.key("KeyS", hold=("ControlLeft",))
    time.sleep(0.8)
    seen["Shape"] = [text for text in flow.texts() if "saved" in text.lower()]
    flow.both("shape-save-notice", flow.park, settle=0.2)
    time.sleep(5.0)
    flow.click("Live")
    flow.call("npc/resume")
    wait_for(lambda: flow.panel_texts(HOTBAR), 60)
    time.sleep(2.0)
    flow.call("npc/pause")
    flow.park(SIZES[0])
    flow.key("KeyS", hold=("ControlLeft",))
    time.sleep(0.8)
    seen["Live"] = [text for text in flow.texts() if "saved" in text.lower()]
    flow.both("live-save-notice", flow.park, settle=0.2)
    report["save_notice"] = seen
    checks.append({"check": "Ctrl+S saves in Shape and in Live and says so",
                   "ok": bool(seen["Shape"]) and bool(seen["Live"]), "seen": seen})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-redesign-p45")
    parser.add_argument("--only", default="oversee,keys,shape,live")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-phase45-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation by text or control name; pointer through window "
                 "cursor position and MouseButtonInput messages; keys through KeyboardInput "
                 "messages; view camera placement; the simulation clock stepped while paused",
        "keyboard_input_tested": True, "screenshots": {}, "checks": [], "sections": wanted}
    run = base / "island"
    child, url = launch(binary, run, [])
    save = run / "living-world.world.json"
    try:
        flow = Phase45Flow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 60 * 60
        flow.tick_budget = 10 * DAY
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
        flow.advance(1500)
        report["ticks"] = {"day": flow.to_phase(0.35)}
        flow.park(SIZES[0])
        flow.whole_island()
        parts = {"oversee": oversee, "keys": keys, "shape": shape, "live": live, "save": save_notice,
                 "compact": compact_economy, "pouch": pouch, "night": night}
        for name in wanted:
            flow.section(name, lambda name=name: parts[name](flow, report, report["checks"]))
            # Whatever a failed section left open is put away for the next.
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
        # A partial run keeps the full run's summary beside its own.
        partial = args.only != parser.get_default("only")
        name = "summary-" + "-".join(wanted) + ".json" if partial else "summary.json"
        (args.out / name).write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                     encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
