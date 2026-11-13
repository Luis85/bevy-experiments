"""Native captures of the resident inspector, the Residents roster and the
calendar, at 1280x720 and 1024x768.

Build first, then run from npc-framework:
  python scripts/playtest/capture_inspector.py [--binary target/debug/game_menu.exe]

Uses a disposable spectator world and native widget activation over BRP, like
`capture_view_wiring.py`. Roster rows, the card's controls and the clock are
activated as widgets; one check also opens the card through the real pointer
path (window cursor position and `MouseButtonInput` messages, so the view's own
picking runs). The operator clock is stepped to a working morning and to the
night, so the hour on the bar is the hour the card describes. Which resident is
working, asleep or in need is read from the roster the player sees; operator
state reads (`world/state`) only cross-check it. No keyboard input is sent: Esc
and the Tab ring are covered by unit tests. The text of every card is copied
into the summary; the PNGs in target/npc-out/inspector/ still need to be read.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import uuid

import capture_player_flow
from capture_native import ROOT, query
from capture_native_homestead import TRANSFORM, file_hash, translation
from capture_player_flow import INSPECTOR, LEDGER, PRIMARY, wait_for
from capture_view_checks import DAY, CheckFlow
from capture_view_followups import Terrain

capture_player_flow.SIZES = ((1280, 720), (1024, 768))

WINDOW = "bevy_window::window::Window"
BUTTON = "bevy_ui_widgets::button::Button"
NAME = "bevy_ecs::name::Name"
# The inspector's header has two icon controls, found by name.
CLOSE = INSPECTOR + " > Close inspector"
FOLLOW = INSPECTOR + " > Follow"
TABS = ("Overview", "Day", "Why", "Home and job")
SORTS = ("Name", "Place", "Job", "Mood", "Wealth")


class InspectorFlow(CheckFlow):
    def top_down(self, texts):
        """Text rows in reading order: by the vertical position layout gave them."""
        at = {int(item["entity"]): translation(item["components"][TRANSFORM])[1]
              for item in query(self.url, [TRANSFORM])}
        return sorted(texts, key=lambda row: at.get(row["entity"], 0.0))

    def rows(self):
        """Roster rows as the player sees them on the ledger's People page, top
        to bottom: (control entity, the row's two lines). The right column holds
        one panel, so the page is opened if the inspector or nothing is there."""
        self.open_page("People")
        return [(entity, text) for entity, text in self.button_rows(LEDGER)
                if "\n" in text and " · mood " in text]

    def resident_at(self, name):
        """Operator read of one resident's position, to tell whether they moved."""
        for _, parts in self.world().having("island_villager"):
            if parts["island_villager"]["name"] == name and "pos" in parts:
                return [parts["pos"]["x"] / 1000.0, parts["pos"]["z"] / 1000.0]
        return None

    def activate(self, entity, label):
        self.record("widget", label=label, entity=entity)
        self.call("world.trigger_event", {
            "event": "bevy_ui_widgets::Activate", "value": {"entity": entity}})
        time.sleep(0.3)
        self.assert_window()

    def named(self, name):
        found = [int(row["entity"]) for row in query(self.url, [NAME])
                 if str(row["components"][NAME]) == name
                 or (isinstance(row["components"][NAME], dict)
                     and row["components"][NAME].get("name") == name)]
        if len(found) != 1:
            raise RuntimeError(f"Expected one entity named {name!r}, found {len(found)}")
        return found[0]

    def card(self, tab=None):
        """The inspector's text as shown, in reading order, without the names
        of its tabs; empty when closed. With `tab`, that tab is shown first."""
        if tab is not None and self.has_widget(INSPECTOR + " > " + tab):
            self.click(INSPECTOR + " > " + tab)
            time.sleep(0.5)
        return [text for text in self.panel_texts(INSPECTOR) if text not in TABS]

    def open_row(self, pick, label):
        """Activate the first roster row `pick` accepts; return its text and the card."""
        chosen = next(((entity, text) for entity, text in self.rows() if pick(text)), None)
        if chosen is None:
            return None
        self.activate(chosen[0], label)
        wait_for(lambda: self.card(), 6)
        time.sleep(0.8)  # Two frames to frame the camera, one refresh of the card.
        entry = {"state": label, "row": chosen[1], "tick": self.state()["tick"],
                 "hud_clock": self.hud_clock(), "card": self.card(),
                 "why": self.card("Why"), "day": self.card("Day")}
        self.card("Overview")
        self.report.setdefault("cards", []).append(entry)
        return entry

    def scale(self):
        resolution = next(row["components"][WINDOW]["resolution"] for row in query(self.url, [WINDOW])
                          if int(row["entity"]) == self.window)
        return resolution.get("scale_factor_override") or resolution["scale_factor"]

    def pointer_click(self, x, y):
        """A real left click at a logical window position."""
        factor = self.scale()
        self.call("world.mutate_components", {"entity": self.window, "component": WINDOW,
                  "path": "internal.physical_cursor_position", "value": [x * factor, y * factor]})
        self.call("world.write_message", {"message": "bevy_window::event::WindowEvent",
                  "value": {"CursorMoved": {"window": self.window, "position": [x, y], "delta": None}}})
        time.sleep(0.25)
        for state in ("Pressed", "Released"):
            self.call("world.write_message", {"message": "bevy_input::mouse::MouseButtonInput",
                      "value": {"button": "Left", "state": state, "window": self.window}})
            time.sleep(0.12)

    def sorted_by(self, key):
        self.click(key)
        time.sleep(0.5)
        return [text for _, text in self.rows()]


def mood_of(text):
    return int(text.split(" · mood ")[1].split("%")[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/inspector")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    run = ROOT / "target" / ("playtest-inspector-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    settings = run / "settings.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "worlds_dir": str(run / "rules"), "fog_of_war": True}), encoding="utf-8")
    save = run / "living-world.world.json"
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation; bounded operator clock steps; one pointer click "
                 "through window cursor position and MouseButtonInput messages",
        "keyboard_input_tested": False, "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--settings", str(settings),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = InspectorFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 40 * 60
            flow.tick_budget = 6 * DAY
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            flow.click("Menu")
            flow.click("Save game")
            flow.terrain = Terrain(json.loads(save.read_text(encoding="utf-8")))
            flow.click("Resume")
            flow.call("npc/pause")

            # A working morning: an hour and a half of decisions behind everyone.
            flow.advance(1500)
            wait_for(lambda: flow.rows(), 8)  # Opens the People page.
            report["roster_rows"] = len(flow.rows())

            # 1. Sorting and filtering, read back from the rows themselves.
            orders = {key: flow.sorted_by(key) for key in SORTS}
            report["sort"] = {key: [text.split("\n")[0] for text in rows[:4]] for key, rows in orders.items()}
            moods = [mood_of(text) for text in orders["Mood"]]
            names = [text.split(" · ")[0].lower() for text in orders["Name"]]
            report["checks"].append({"check": "roster sorted by mood, lowest first",
                                     "ok": moods == sorted(moods), "moods": moods[:8]})
            report["checks"].append({"check": "roster sorted by name",
                                     "ok": names == sorted(names)})
            flow.click("Mood")
            flow.capture("residents-by-mood")
            flow.click("Harbour")
            time.sleep(0.5)
            harbour = [text for _, text in flow.rows()]
            report["checks"].append({"check": "settlement filter keeps Harbour only",
                                     "ok": bool(harbour) and all(" · Harbour\n" in text for text in harbour),
                                     "rows": len(harbour)})
            flow.click("All")
            flow.click("Name")

            # 2. A resident at work, from the roster.
            working = flow.open_row(
                lambda text: any(word in text.split("\n")[1] for word in
                                 ("gathering", "on the way to", "buying work", "mill", "bake", "saw",
                                  "smelt", "forge", "spin", "tan", "sew", "cook", "carve")),
                "inspector-working")
            if working is None:
                raise RuntimeError("No resident was at work in the morning")
            flow.capture("inspector-working")
            report["checks"].append({"check": "the inspector's Why tab says what was chosen",
                                     "ok": "Chose" in working["why"] and len(working["why"]) > 3,
                                     "why": working["why"][:8]})

            # 3. Follow: the camera keeps a walking resident as the clock runs.
            # The resident with the longest walk ahead (an operator read; the
            # roster's own words do not say how far somebody still has to go).
            def walk_left(parts):
                intent = parts.get("move_intent")
                if not intent or "pos" not in parts:
                    return 0.0
                return ((intent["goal"]["x"] - parts["pos"]["x"]) ** 2 +
                        (intent["goal"]["z"] - parts["pos"]["z"]) ** 2) ** 0.5
            flow.call("npc/resume")
            time.sleep(1.5)
            flow.call("npc/pause")
            far = max(flow.world().having("island_villager"), key=lambda found: walk_left(found[1]))
            far_name = far[1]["island_villager"]["name"]
            report["follow_subject"] = {"name": far_name, "walk_left_m": round(walk_left(far[1]) / 1000.0, 1)}
            walker = flow.open_row(lambda text: text.startswith(far_name + " · "),
                                   "inspector-following") or working
            walker_name = walker["row"].split(" · ")[0]
            # Follow is an icon control; while following it stays lit. Whether it
            # holds is read from the camera below.
            flow.click(FOLLOW)
            time.sleep(1.5)
            _, before = flow.camera_entity()
            stood = flow.resident_at(walker_name)
            flow.call("npc/resume")
            time.sleep(5.0)
            flow.call("npc/pause")
            time.sleep(0.6)
            _, after = flow.camera_entity()
            went = flow.resident_at(walker_name)
            moved = ((went[0] - stood[0]) ** 2 + (went[1] - stood[1]) ** 2) ** 0.5
            panned = ((after["focus"][0] - before["focus"][0]) ** 2 +
                      (after["focus"][2] - before["focus"][2]) ** 2) ** 0.5
            report["follow"] = {"resident": walker_name, "resident_moved_m": round(moved, 2),
                                "camera_moved_m": round(panned, 2)}
            report["checks"].append({"check": "Follow moves the camera with a walking resident",
                                     "ok": moved > 1.0 and abs(panned - moved) < max(1.5, moved * 0.35),
                                     "resident_moved_m": round(moved, 2), "camera_moved_m": round(panned, 2)})
            flow.capture("inspector-following")
            flow.click(FOLLOW)

            # 4. The same card through the pointer: close it, then click the
            # resident where the camera framed them (the middle of the free area).
            name = working["row"].split(" · ")[0]
            flow.click(CLOSE)
            wait_for(lambda: not flow.card(), 6)
            report["checks"].append({"check": "Close closes the card", "ok": not flow.card()})
            flow.activate(next(entity for entity, text in flow.rows() if text.startswith(name + " · ")),
                          "reselect for pointer check")
            wait_for(lambda: flow.card(), 6)
            time.sleep(1.0)
            flow.click(CLOSE)
            time.sleep(1.0)
            flow.resize((1280, 720))
            # The camera framed the resident in the middle of what the 344 px
            # inspector left free; with the column empty they stay where they were.
            pointer = {"check": "a pointer click on the resident opens the card", "ok": False}
            for flow.pointer_x in ((1280 - 8 - 344) / 2, 640):
                for dy in (0, -14, 14, -28):
                    flow.pointer_click(flow.pointer_x, 341 + dy)
                    time.sleep(0.6)
                    if flow.card():
                        pointer.update(ok=flow.card()[0] == name, at=[flow.pointer_x, 341 + dy],
                                       opened=flow.card()[0])
                        break
                if flow.card():
                    break
            report["checks"].append(pointer)
            if flow.card():
                # A click on empty ground closes it again.
                flow.pointer_click(640, 660)
                time.sleep(0.6)
                report["checks"].append({"check": "a click on empty ground closes the card",
                                         "ok": not flow.card()})
            if flow.card():
                flow.click(CLOSE)

            # 5. The calendar under the clock.
            calendar = flow.named("Game: Calendar")
            flow.activate(calendar, "Calendar")
            time.sleep(0.5)
            texts = flow.texts()
            report["calendar"] = [text for text in texts
                                  if "day " in text and " of " in text or " begins " in text]
            flow.capture("calendar")
            flow.activate(calendar, "Calendar")

            # 6. Night: someone asleep. Residents sleep until rested, so look
            # from nightfall onwards and take the first hour that has a sleeper.
            def activities():
                doing = {}
                for _, text in flow.rows():
                    activity = text.split("\n")[1].split(" · mood ")[0]
                    doing[activity] = doing.get(activity, 0) + 1
                return doing

            def resting():
                world = flow.world()
                return sum(1 for _, parts in world.having("island_villager")
                           if parts["island_villager"].get("resting_until", 0) > world.tick)

            flow.open_page("People")
            flow.click("Name")
            asleep, nights = None, []
            for phase in (0.705, 0.73, 0.76, 0.80, 0.86, 0.93):
                flow.to_phase(phase)
                time.sleep(1.0)
                nights.append({"tick": flow.state()["tick"], "hud_clock": flow.hud_clock(),
                               "resting": resting(), "roster_activities": activities()})
                asleep = flow.open_row(lambda text: text.split("\n")[1].startswith(("sleeping", "resting")),
                                       "inspector-sleeping-night")
                if asleep is not None:
                    break
            report["night"] = nights
            report["checks"].append({"check": "a sleeping resident was found and inspected at night",
                                     "ok": asleep is not None})
            if asleep is not None:
                flow.capture("inspector-sleeping-night")
                flow.click(CLOSE)

            # 7. Someone in need, wherever the roster shows one: through the
            # next two days and the first day of winter, sampled every hour.
            need, critical, samples = None, None, 0
            for samples in range(1, 61):
                flow.advance(1000)
                rows = [text for _, text in flow.rows()]
                danger = [text for text in rows if text.endswith("in danger")]
                low = [text for text in rows if text.endswith("in need")]
                if low and need is None:
                    need = flow.open_row(lambda text: text == low[0], "inspector-need")
                    if need is not None:
                        flow.capture("inspector-need")
                        flow.click(CLOSE)
                if danger:
                    critical = flow.open_row(lambda text: text == danger[0], "inspector-critical")
                    if critical is not None:
                        flow.capture("inspector-critical")
                        flow.click(CLOSE)
                        break
            report["need_search"] = {"samples": samples, "ticks_between": 1000,
                                     "ended_at": flow.hud_clock(), "tick": flow.state()["tick"]}
            report["checks"].append({"check": "a resident in need was found and inspected",
                                     "ok": need is not None or critical is not None,
                                     "low": need is not None, "critical": critical is not None})

            flow.click("Menu")
            flow.click("Quit game")
            child.wait(timeout=40)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            # The pointer checks depend on where a moving resident stands, and a
            # resident in need on what the village does: reported, not required.
            report["ok"] = all(check["ok"] for check in report["checks"]
                               if "click on" not in check["check"] and "in need" not in check["check"])
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("inspector_failure", error=str(error))
            except Exception as diagnostic:
                report["diagnostic_error"] = str(diagnostic)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace[-80:]
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        log_path = run / "native.log"
        if log_path.exists():
            lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
            report["native_errors"] = [line for line in lines if "ERROR" in line or "panicked at" in line][-30:]
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for image in sorted((run / "shots").glob("*.png")) if (run / "shots").is_dir() else []:
            shutil.copy2(image, args.out / image.name)
            copied.append(str(args.out / image.name))
        report["copied"] = copied
        (run / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
        shutil.copy2(run / "summary.json", args.out / "summary.json")
        print(run, flush=True)


if __name__ == "__main__":
    main()
