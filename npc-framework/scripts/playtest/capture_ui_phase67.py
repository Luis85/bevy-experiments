"""Native captures of the redesigned interface, spec phases 6 and 7: the title,
the new-game start choice, the pause menu, the four Settings tabs, the Guide,
the Dusk panel style, save and load feedback on the toast lane, the first-run
tips of each mode, a second-level tooltip in each mode, and the Shape redo
after a refused dab, at 1280x720 and 1024x768.

Build first, then run from npc-framework:
  python scripts/playtest/capture_ui_phase67.py [--binary target/debug/island_game.exe]

One disposable run of the real game, driven over BRP like
`capture_ui_phase45.py`: controls by the text on them or by their `Name`, the
pointer through the window's cursor position, and keys as `KeyboardInput`
messages (Tab, Shift+Tab, Esc, Ctrl+S, Ctrl+Z, Ctrl+Y, the bracket keys). A
part that fails is recorded as a failed check and the run goes on. PNGs go to
target/npc-out/ui-redesign-p67/ and still need to be read.
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
from capture_player_flow import LEDGER, PRIMARY, wait_for
from capture_ui_phase45 import (BRUSH, HOTBAR, NEEDS, RAIL, WORKSPACE, Phase45Flow)
from capture_ui_redesign import SIZES, launch, stop
from capture_view_checks import DAY
from capture_view_followups import Terrain
from capture_view_wiring import WiringFlow

capture_player_flow.SIZES = SIZES
TIP = "Island tip"
SETTINGS_TABS = ("Sound", "Display", "Controls", "Accessibility")
GUIDE_PAGES = ("Controls", "Oversee", "Live", "Shape", "The economy")


class Phase67Flow(Phase45Flow):
    def stored(self):
        """The settings file as the game last wrote it."""
        path = self.run / "island-settings.json"
        return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}

    def ring(self, presses, shift=False):
        """Press Tab `presses` times; the names of what took the focus."""
        seen = []
        for _ in range(presses):
            self.key("Tab", hold=("ShiftLeft",) if shift else ())
            seen.append(self.focused())
        return seen

    def hover_levels(self, entity, label, expect):
        """Rest the pointer on a control: the first and the second level of
        its tooltip, by the tooltip's own timers. `expect` is a word of the
        second level."""
        self.settle(SIZES[0])
        self.park(SIZES[0])
        x, y = self.centre(entity)
        self.cursor(x, y)
        time.sleep(0.9)
        self.snap(f"tooltip-{label}-level1-1280x720", SIZES[0])
        self.cursor(x + 1, y)
        time.sleep(0.2)
        self.cursor(x, y)
        time.sleep(2.4)
        texts = self.texts()
        cards = self.tooltips_inside(SIZES[0])
        self.report.setdefault("checks", []).append({
            "check": f"the {label} tooltip stays inside the 1280x720 window",
            "ok": bool(cards) and all(card["inside"] for card in cards), "cards": cards})
        self.snap(f"tooltip-{label}-level2-1280x720", SIZES[0])
        # The same tooltip at 1024x768.
        self.settle(SIZES[1])
        time.sleep(0.6)
        x, y = self.centre(entity)
        self.cursor(x, y)
        time.sleep(2.6)
        cards = self.tooltips_inside(SIZES[1])
        self.report["checks"].append({
            "check": f"the {label} tooltip stays inside the 1024x768 window",
            "ok": bool(cards) and all(card["inside"] for card in cards), "cards": cards})
        self.snap(f"tooltip-{label}-level2-1024x768", SIZES[1])
        self.settle(SIZES[0])
        self.park(SIZES[0])
        return any(expect in text for text in texts)

    def tip(self):
        return self.panel_texts(TIP)

    def ui_scale(self):
        """The interface scale the game applies (Bevy's UiScale)."""
        names = [name for name in self.call("world.list_resources", {})
                 if str(name).endswith("::UiScale")]
        value = self.call("world.get_resources", {"resource": names[0]})
        value = value.get("value", value) if isinstance(value, dict) else value
        while isinstance(value, (list, dict)) and value:
            value = value[0] if isinstance(value, list) else next(iter(value.values()))
        return float(value)

    def tooltips_inside(self, size):
        """Every shown tooltip card's rectangle, and whether it lies inside
        the window (`size` in logical pixels)."""
        factor = self.scale()
        width, height = size[0] * factor, size[1] * factor
        names = {entity: name for name, entities in self.names().items() for entity in entities}
        cards = []
        for row in query(self.url, [COMPUTED, TRANSFORM]):
            entity = int(row["entity"])
            name = names.get(entity, "")
            if not name.startswith("Tooltip: "):
                continue
            x, y = translation(row["components"][TRANSFORM])[:2]
            w, h = vector(row["components"][COMPUTED]["size"])
            if w <= 0 or h <= 0:
                continue
            rect = [x - w / 2, y - h / 2, x + w / 2, y + h / 2]
            inside = rect[0] >= -0.5 and rect[1] >= -0.5 and rect[2] <= width + 0.5 and rect[3] <= height + 0.5
            cards.append({"name": name, "rect": [round(v / factor, 1) for v in rect], "inside": inside})
        self.report.setdefault("tooltip_rects", []).extend(cards)
        return cards


def title_and_menus(flow, report, checks):
    time.sleep(1.0)
    report["title"] = [text for text in flow.texts() if text][:20]
    checks.append({"check": "a fresh title has no Continue and offers New game first",
                   "ok": "New game" in report["title"] and "Continue" not in report["title"]})
    flow.both("title", settle=0.8)
    ring = flow.ring(5)
    back = flow.ring(1, shift=True)
    report["title_tab_ring"] = {"tab": ring, "shift_tab": back}
    checks.append({"check": "Tab walks the title's buttons and Shift+Tab walks back",
                   "ok": len({name for name in ring if name}) >= 4 and back[0] == ring[-2],
                   "ring": ring, "back": back})
    flow.snap("title-tab-focus-1280x720", SIZES[0])

    # Settings: four tabs, each captured; a step button and the file.
    flow.click("Settings")
    time.sleep(0.5)
    tabs = {}
    for tab in SETTINGS_TABS:
        flow.click(tab)
        time.sleep(0.5)
        tabs[tab] = [text for text in flow.texts() if text][:60]
        flow.both(f"settings-{tab.lower()}", settle=0.5)
    report["settings_tabs"] = tabs
    checks.append({"check": "Settings has four tabs with their own controls",
                   "ok": "Master volume" in tabs["Sound"] and "Interface size" in tabs["Display"]
                   and "Camera pan speed" in tabs["Controls"] and "Panel style" in tabs["Accessibility"]})
    flow.click("Sound")
    flow.click("Master −")
    time.sleep(0.4)
    flow.click("Display")
    flow.click("Fixed")
    time.sleep(0.4)
    stored = flow.stored()
    checks.append({"check": "a step and a choice are applied and written to the settings file",
                   "ok": stored.get("master_volume") == 50 and stored.get("ui_scale_auto") is False,
                   "file": stored})
    flow.click("Auto")
    flow.click("Sound")
    flow.click("Master +")
    # The Settings screen's own Tab ring, by real Tab presses.
    ring = flow.ring(8)
    report["settings_tab_ring"] = ring
    checks.append({"check": "Tab walks the Settings screen",
                   "ok": len({name for name in ring if name}) >= 6, "ring": ring})
    # Esc is Back on this screen.
    flow.key("Escape")
    time.sleep(0.5)
    checks.append({"check": "Esc leaves Settings for the title",
                   "ok": flow.has_widget("New game") and not flow.has_widget("Master +")})

    # The Guide: five pages.
    flow.click("How to play")
    time.sleep(0.5)
    pages = {}
    for page in GUIDE_PAGES:
        flow.click(page)
        time.sleep(0.4)
        pages[page] = [text for text in flow.texts() if text][:40]
        flow.both("guide-" + page.lower().replace(" ", "-"), settle=0.4)
    report["guide_pages"] = {page: len(texts) for page, texts in pages.items()}
    checks.append({"check": "the Guide has five pages and each says something of its own",
                   "ok": len({tuple(texts) for texts in pages.values()}) == 5})
    ring = flow.ring(7)
    report["guide_tab_ring"] = ring
    checks.append({"check": "Tab walks the Guide's tabs and Back",
                   "ok": len({name for name in ring if name}) >= 5, "ring": ring})
    flow.key("Escape")
    time.sleep(0.4)
    checks.append({"check": "Esc leaves the Guide for the title",
                   "ok": flow.has_widget("New game") and not flow.has_widget("The economy")})

    # A new beginning: the name and the three ways to begin.
    flow.click("New game")
    time.sleep(0.6)
    report["new_game"] = [text for text in flow.texts() if text][:30]
    checks.append({"check": "the new-game screen offers Live, Oversee and Shape, each with a sentence",
                   "ok": all(word in report["new_game"] for word in ("Live", "Oversee", "Shape"))
                   and all(flow.has_widget(name) for name in
                           ("Start on the island", "Oversee the village", "Shape the landscape"))})
    flow.both("new-game-start-choice", settle=0.5)
    ring = flow.ring(6)
    report["new_game_tab_ring"] = ring
    checks.append({"check": "Tab walks the new-game screen",
                   "ok": len({name for name in ring if name}) >= 4, "ring": ring})
    flow.key("Escape")
    time.sleep(0.5)
    checks.append({"check": "Esc leaves the new-game screen for the title",
                   "ok": flow.has_widget("New game") and not flow.has_widget("Start on the island")})
    # A menu button explains itself on hover.
    ok = flow.hover_levels(flow.control("Settings"), "menu", "accessibility")
    checks.append({"check": "a menu button has a tooltip", "ok": ok})


def body_at_1080(flow, report, checks):
    """Body text at 1920x1080 with the automatic interface size: the UI
    scale the game chose, the body step it gives, and a capture to measure
    the glyphs on."""
    size = (1920, 1080)
    flow.settle(size)
    time.sleep(1.2)
    factor = flow.ui_scale()
    # The glyph box of a body-sized line: the tagline is BODY+2, the
    # credits line CAPTION; the name of a button is BODY.
    factor_window = flow.scale()
    report["body_text_1080"] = {"ui_scale": factor, "body_font_px": round(14 * factor, 2),
                                "window_scale_factor": factor_window}
    flow.snap("title-1920x1080", size)
    checks.append({"check": "body text at 1920x1080 with Auto is 18 px or more",
                   "ok": 14 * factor >= 18, "seen": report["body_text_1080"]})
    flow.settle(SIZES[0])


def oversee(flow, report, checks):
    # The four first-run tips of Oversee, one at a time.
    tips = []
    for index in range(4):
        texts = wait_for(lambda: flow.tip(), 8)
        tips.append(texts)
        if index in (0, 1):
            flow.both(f"tip-oversee-{index + 1}", flow.park, settle=0.5)
        else:
            flow.park(SIZES[0])
            flow.snap(f"tip-oversee-{index + 1}-1280x720", SIZES[0])
        flow.click(TIP + " > Got it")
        time.sleep(0.6)
    report["oversee_tips"] = tips
    time.sleep(1.0)
    stored = flow.stored()
    checks.append({"check": "Oversee shows four tips in turn; each dismissed one is remembered",
                   "ok": len({texts[0] for texts in tips}) == 4 and not flow.tip()
                   and len(stored.get("tips_seen", [])) == 4, "seen": stored.get("tips_seen"),
                   "titles": [texts[0] for texts in tips]})
    # A total's tooltip on two levels.
    ok = flow.hover_levels(flow.control("Residents"), "oversee", "housed in")
    checks.append({"check": "a total's tooltip has a second level in Oversee", "ok": ok})
    # Ctrl+S: the save is said once on the toast lane.
    flow.key("KeyS", hold=("ControlLeft",))
    time.sleep(0.7)
    report["save_toast"] = [text for text in flow.texts() if "saved" in text.lower()]
    # At once, while the three-second toast stands; then at both sizes.
    flow.snap("save-toast-oversee-at-once-1280x720", SIZES[0])
    flow.both("save-toast-oversee", flow.park, settle=0.1)
    checks.append({"check": "Ctrl+S answers with the toast 'Island saved'",
                   "ok": "Island saved" in report["save_toast"], "texts": report["save_toast"]})
    time.sleep(3.0)


def shape(flow, report, checks):
    x, z, clearance = WiringFlow.open_ground(flow)
    flow.click("Shape")
    wait_for(lambda: flow.panel_texts(RAIL), 10)
    tip = wait_for(lambda: flow.tip(), 8)
    report["shape_tip"] = tip
    flow.look(x, z, 40, "shape")
    flow.both("tip-shape", flow.park, settle=0.5)
    flow.click(TIP + " > Got it")
    checks.append({"check": "Shape has its own first-run tip", "ok": tip[0] == "Shape the land"})
    ok = flow.hover_levels(flow.control("Undo", RAIL), "shape", "32 strokes")
    checks.append({"check": "Undo's tooltip has a second level in Shape", "ok": ok})

    # The redo after a refused dab: hold a soft Raise still until the
    # simulation refuses a dab, then take the stroke back and lay it again.
    flow.call("npc/resume")
    flow.click(RAIL + " > Raise")
    flow.click(BRUSH + " > Soft")
    flow.key("BracketRight")
    flow.key("BracketRight")
    middle = (SIZES[0][0] / 2, SIZES[0][1] / 2)
    reach = max(40.0, min(140.0, (clearance - 9.5) / 0.046))
    refused = []

    def hold_still():
        end = time.monotonic() + 6.0
        while time.monotonic() < end:
            notice = flow.panel_texts("Shape status")
            if any("Not applied" in text for text in notice):
                refused.extend(notice)
                break
            time.sleep(0.3)

    flow.sweep((middle[0] - reach, middle[1]), (middle[0] + reach, middle[1]), pause_at=0.6,
               then=hold_still)
    time.sleep(1.5)
    depth = lambda: [text for text in flow.panel_texts(RAIL) if " of " in text]  # noqa: E731
    after_stroke = depth()
    flow.key("KeyZ", hold=("ControlLeft",))
    time.sleep(1.5)
    undone = depth()
    flow.key("KeyY", hold=("ControlLeft",))
    time.sleep(0.8)
    notice = flow.panel_texts("Shape status")
    time.sleep(0.8)
    redone = depth()
    report["shape_redo"] = {"refused_during_stroke": refused, "after_stroke": after_stroke,
                            "undone": undone, "redo_notice": notice, "redone": redone}
    flow.snap("shape-redo-after-refused-dab-1280x720", SIZES[0])
    checks.append({"check": "a stroke with a refused dab is taken back and laid again",
                   "ok": bool(refused) and after_stroke == ["1 of 32"] and undone == ["0 of 32"]
                   and redone == ["1 of 32"] and not any("not applied" in text for text in notice),
                   "seen": report["shape_redo"]})
    flow.key("KeyZ", hold=("ControlLeft",))
    time.sleep(1.0)
    flow.call("npc/pause")
    flow.park(SIZES[0])


def live(flow, report, checks):
    flow.click("Live")
    flow.call("npc/resume")
    wait_for(lambda: flow.panel_texts(HOTBAR), 60)
    time.sleep(2.5)
    flow.call("npc/pause")
    tip = wait_for(lambda: flow.tip(), 8)
    report["live_tip"] = tip
    flow.both("tip-live", flow.park, settle=0.5)
    flow.click(TIP + " > Got it")
    time.sleep(0.5)
    checks.append({"check": "Live has its own first-run tip", "ok": tip[0] == "This is your life"})
    ok = flow.hover_levels(flow.control("Water", NEEDS), "live", "open your pouch")
    checks.append({"check": "a need's tooltip has a second level in Live", "ok": ok})
    flow.both("live-tag", flow.park, settle=0.5)
    report["tips_seen"] = flow.stored().get("tips_seen")

    # The pause menu, by Esc, and its own Tab ring.
    flow.key("Escape")
    wait_for(lambda: flow.has_widget("Resume"), 8)
    pause = [text for text in flow.texts() if text][:20]
    report["pause"] = pause
    checks.append({"check": "the pause menu has no mode buttons and says when the island was saved",
                   "ok": not any(text in pause for text in ("Oversee the village", "Shape the landscape"))
                   and any("saved" in text for text in pause)
                   and all(text in pause for text in ("Resume", "Save game", "Save and go to title",
                                                      "Save and quit"))})
    flow.both("pause", settle=0.5)
    ring = flow.ring(6)
    report["pause_tab_ring"] = ring
    checks.append({"check": "Tab walks the pause menu", "ok": len({name for name in ring if name}) >= 5,
                   "ring": ring})
    flow.click("Save game")
    time.sleep(0.6)
    flow.snap("pause-saved-1280x720", SIZES[0])

    # Dusk: Settings from the pause menu, then every panel follows.
    flow.click("Settings")
    flow.click("Accessibility")
    flow.click("Dusk")
    time.sleep(0.6)
    flow.both("settings-accessibility-dusk", settle=0.5)
    flow.click("Display")
    time.sleep(0.4)
    flow.snap("settings-display-dusk-1280x720", SIZES[0])
    flow.key("Escape")
    wait_for(lambda: flow.has_widget("Resume"), 8)
    flow.both("pause-dusk", settle=0.5)
    flow.click("How to play")
    time.sleep(0.4)
    flow.snap("guide-controls-dusk-1280x720", SIZES[0])
    flow.key("Escape")
    time.sleep(0.4)
    flow.key("Escape")  # Resume
    wait_for(lambda: flow.panel_texts(HOTBAR), 20)
    flow.call("npc/pause")
    flow.click(HOTBAR + " > Craft")
    time.sleep(0.9)
    flow.both("live-dusk-craft", flow.park, settle=0.6)
    flow.click(HOTBAR + " > Explore")
    time.sleep(0.9)
    flow.both("live-dusk-explore", flow.park, settle=0.6)
    flow.click(WORKSPACE + " > Close workspace")
    flow.click("Shape")
    wait_for(lambda: flow.panel_texts(RAIL), 10)
    flow.both("shape-dusk", flow.park, settle=0.6)
    flow.click("Oversee")
    time.sleep(0.6)
    flow.open_page("Economy")
    time.sleep(0.6)
    flow.both("oversee-dusk-economy", flow.park, settle=0.6)
    flow.close_ledger()


def title_again(flow, report, checks):
    # Back to the title with a save: Continue leads, with who and when.
    flow.click("Menu")
    wait_for(lambda: flow.has_widget("Resume"), 8)
    flow.click("Save and title")
    wait_for(lambda: flow.has_widget("New game"), 60)
    time.sleep(1.0)
    title = [text for text in flow.texts() if text][:20]
    report["title_with_save"] = title
    checks.append({"check": "with a save the title leads with Continue and who it is",
                   "ok": "Continue" in title and any("day " in text for text in title)})
    flow.both("title-continue-dusk", settle=0.6)
    flow.click("New game")
    time.sleep(0.5)
    flow.both("new-game-dusk", settle=0.5)
    flow.key("Escape")
    time.sleep(0.4)
    # Parchment again, and the tips offered once more.
    flow.click("Settings")
    flow.click("Accessibility")
    again = flow.has_widget("Show the first-run tips again", enabled=True)
    flow.click("Show the first-run tips again")
    time.sleep(0.5)
    flow.click("Parchment")
    time.sleep(0.5)
    stored = flow.stored()
    flow.snap("settings-accessibility-tips-again-1280x720", SIZES[0])
    checks.append({"check": "Settings shows the first-run tips again",
                   "ok": again and stored.get("tips_seen") == [] and stored.get("panel_style") == "parchment",
                   "file": stored})
    flow.key("Escape")
    time.sleep(0.4)
    # Continue: the island is loaded and says so.
    flow.click("Continue")
    wait_for(lambda: flow.widget("Menu"), 60)
    time.sleep(0.8)
    loaded = [text for text in flow.texts() if text.startswith("Island loaded")]
    report["load_toast"] = loaded
    flow.both("load-toast", flow.park, settle=0.1)
    checks.append({"check": "Continue answers with a toast that the island is loaded",
                   "ok": bool(loaded), "texts": loaded})
    time.sleep(1.0)
    report["tip_after_reset"] = flow.tip()
    checks.append({"check": "after the reset a first-run tip shows again",
                   "ok": bool(report["tip_after_reset"])})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-redesign-p67")
    parser.add_argument("--only", default="",
                        help="comma-separated parts: menus,oversee,shape,live,again")
    args = parser.parse_args()
    only = {part for part in args.only.split(",") if part}
    wanted = lambda part: not only or part in only  # noqa: E731
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    base = ROOT / "target" / ("playtest-ui-phase67-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation by text or control name; pointer through window "
                 "cursor position and MouseButtonInput messages; keys through KeyboardInput "
                 "messages; view camera placement; the simulation clock stepped while paused",
        "keyboard_input_tested": True, "screenshots": {}, "checks": []}
    run = base / "island"
    child, url = launch(binary, run, [])
    save = run / "living-world.world.json"
    checks = report["checks"]
    try:
        flow = Phase67Flow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 60 * 60
        flow.tick_budget = 10 * DAY
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        flow.settle(SIZES[0])
        if wanted("menus"):
            flow.section("title and menus", lambda: title_and_menus(flow, report, checks))
            flow.section("body text at 1080", lambda: body_at_1080(flow, report, checks))
        if not flow.has_widget("Oversee the village"):
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
        for name, part in (("oversee", oversee), ("shape", shape), ("live", live),
                           ("title again", title_again)):
            if not wanted(name.split()[-1]):
                continue
            flow.section(name, lambda part=part: part(flow, report, checks))
            flow.section(name + " cleanup", lambda: flow.settle(SIZES[0]))
        report["ok"] = all(check["ok"] for check in checks)
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
        (args.out / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                               encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
