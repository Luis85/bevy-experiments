"""Native captures of the island's homes: residents going indoors, the fitted
houses beside a resident, and the cutaway that shows a home's inside.

Build first, then run from npc-framework:
  python scripts/playtest/capture_houses.py [--binary target/debug/island_game.exe]
      [--skip-build] [--days 2] [--pin-night]

Uses a disposable spectator world and native widget activation over BRP, like
`capture_ui_redesign.py` (controls are found by name). The camera is placed by
writing the view's own
`RtsCamera`; a home is selected with a real left click on it (the window's
cursor position and `MouseButtonInput` messages), which opens its card and so
its cutaway. Homes, residents and who is indoors are found with operator state
reads (`world/state`), which a player does not have. The clock is stepped to
the real night: the light is never pinned, so sleepers sleep by the hour the
HUD shows.

Residents rest around sunset, for a few hundred ticks, so the home with a
sleeper is captured at about 23:00, when the light is still that of dusk;
`--pin-night` takes only that capture with the light pinned to midnight.

The player-built cottage is placed with one recorded `PlaceBuilding` command
on a copy of the shipped rules in which a cottage needs no materials and 40
ticks of work, so residents finish it within the run; `--skip-build` leaves
that part out and runs on the shipped rules.

Captures are 1280x720 in target/npc-out/houses/ and still need to be read.
"""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import uuid

import capture_player_flow
from capture_build_menu import BUILT, SITE, BuildFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import PRIMARY, wait_for
from capture_view_checks import DAY, World, xz
from capture_ui_redesign import UiFlow
from capture_view_followups import Terrain
from capture_view_wiring import WiringFlow

# The scripts imported above each set their own capture sizes.
capture_player_flow.SIZES = ((1280, 720),)

INDOORS = "regional_indoors_v1"
HOME = "island_household_v1"
VILLAGER = "island_villager"
PLACE = "island_place"
STATS = "island scene stats:"


class HousesFlow(WiringFlow, UiFlow):
    def cursor(self, x, y):
        BuildFlow.cursor(self, x, y)

    def close_card(self):
        """Close an open building card, if there is one."""
        try:
            self.press("Close card")
        except RuntimeError:
            return False
        time.sleep(0.6)
        return True

    def click_centre(self):
        """A real left click in the middle of the window, on a building: its
        card opens and its inside is built. The machine's own mouse can take
        a click away, so it is tried up to three times; returns whether the
        inside appeared."""
        before = self.entities()
        for attempt in range(3):
            self.pointer_click(640, 360)
            time.sleep(0.8)
            if self.entities() > before + 20:
                return True
            self.record("click_retry", attempt=attempt)
        return False

    def park_pointer(self):
        """Take the pointer out of the window so no hover highlight and no
        tooltip is captured. (The bottom-left corner, where it used to rest,
        holds the Overlays and Places dock now: a pointer resting there shows
        the Overlays tooltip, as it should.)"""
        position = {"entity": self.window, "component": "bevy_window::window::Window",
                    "path": "internal.physical_cursor_position"}
        try:
            self.call("world.mutate_components", {**position, "value": None})
        except RuntimeError:
            # The empty stretch of the top bar, between the totals and the
            # weather: chrome without a tooltip.
            factor = self.scale()
            self.call("world.mutate_components", {**position, "value": [845 * factor, 22 * factor]})

    def frame(self, label, x, z, distance, yaw=0.0, settle=1.0, running=False):
        """Frame a point (metres) and capture; the clock stays as it is unless
        `running`."""
        self.look(x, z, distance, label, yaw)
        if running:
            self.call("npc/resume")
        time.sleep(settle)
        self.capture(label)
        if running:
            self.call("npc/pause")

    def census(self):
        """Who rests, and where: nobody with a home may rest outdoors."""
        world = self.world()
        homes = {sim_id: parts[HOME] for sim_id, parts in world.having(HOME)}
        housed = {resident: home for home, data in homes.items() for resident in data["residents"]}
        inside, resting_inside, resting_outside, outdoors = {}, 0, [], 0
        for sim_id, parts in world.having(VILLAGER):
            person = parts[VILLAGER]
            building = (parts.get(INDOORS) or {}).get("building")
            resting = person.get("resting_until", 0) > world.tick
            if building is not None:
                inside.setdefault(building, []).append(sim_id)
                resting_inside += resting
            else:
                outdoors += 1
                if resting and sim_id in housed:
                    resting_outside.append({"id": sim_id, "name": person["name"]})
        return {"tick": world.tick, "hud_clock": self.hud_clock(), "residents": len(world.having(VILLAGER)),
                "indoors": sum(len(people) for people in inside.values()), "outdoors": outdoors,
                "resting_indoors": resting_inside, "housed_resting_outdoors": resting_outside,
                "by_building": {str(key): value for key, value in sorted(inside.items())}}, world, homes

    def at_a_door(self, budget=2400):
        """Step the clock a tick at a time until somebody walks across their
        own doorstep: a resident beside a closed home, for proportion."""
        spent = 0
        while spent <= budget:
            world = self.world()
            homes = {sim_id: parts[HOME] for sim_id, parts in world.having(HOME)}
            for home_id, home in homes.items():
                door = xz(home["door"])
                for resident in home["residents"]:
                    parts = world.parts(resident)
                    if "pos" not in parts or (parts.get(INDOORS) or {}).get("building") is not None:
                        continue
                    at = xz(parts["pos"])
                    reach = math.hypot(at[0] - door[0], at[1] - door[1])
                    if reach <= 900:
                        return {"home": home_id, "label": home["label"], "door": door, "resident": resident,
                                "name": parts[VILLAGER]["name"], "metres_from_door": round(reach / 1000.0, 2),
                                "tick": world.tick, "waited_ticks": spent,
                                "walking": "move_intent" in parts}
            self.call("npc/step", {"ticks": 2})
            spent += 2
            time.sleep(0.03)
        return None

    def sleeper(self, row, prefix):
        """Step from sunset until somebody of the street rests in bed with
        at least 120 ticks to go, open that home by a click and capture it
        with the clock stopped (the view still animates)."""
        report = self.report
        self.to_phase(0.705)
        timeline, sleeper, spent = [], None, 0
        while spent <= int(0.27 * DAY):
            census, world, _ = self.census()
            timeline.append([world.tick, census["indoors"], census["resting_indoors"]])
            for home_row in row:
                people = census["by_building"].get(str(home_row["id"]), [])
                left = [world.parts(person)[VILLAGER].get("resting_until", 0) - world.tick
                        for person in people]
                if any(ticks >= 120 for ticks in left):
                    sleeper = (home_row, people, max(left))
                    break
            if sleeper:
                break
            self.advance(40)
            spent += 40
        report["rest_timeline"] = {
            "columns": ["tick", "indoors", "resting indoors"], "first": timeline[:3],
            "most_resting": max(timeline, key=lambda entry: entry[2]), "last": timeline[-1],
            "samples": len(timeline)}
        report["sleeper"] = None if sleeper is None else {
            "label": sleeper[0]["label"], "inside": sleeper[1], "rest_ticks_left": sleeper[2],
            "tick": self.state()["tick"], "phase": round(self.state()["tick"] % DAY / DAY, 3),
            "hud_clock": self.hud_clock()}
        if sleeper is None:
            return
        home = sleeper[0]
        body = (home["door"][0] / 1000.0, home["door"][1] / 1000.0 - 3.5)
        self.look(body[0], body[1], 22, "sleeper-select", 0.0)
        report["sleeper"]["opened_by_click"] = self.click_centre()
        self.park_pointer()
        report["sleeper"]["card"] = [text for text in self.texts()
                                     if any(word in text for word in ("Beds", "Lives here", "hearth"))][:6]
        self.frame(prefix + "-front-22m", body[0], body[1], 22, settle=1.5)
        self.frame(prefix + "-corner-11m", body[0], body[1], 11, yaw=math.pi / 4, settle=1.2)
        self.frame(prefix + "-side-11m", body[0], body[1], 11, yaw=-math.pi / 2, settle=1.2)
        self.close_card()

    def perf(self, label, seconds=4.0, match="island_homes"):
        """Per-system cost of the homes pass, where the build has a profiler."""
        try:
            self.call("npc/perf/reset")
            began = time.monotonic()
            time.sleep(seconds)
            report = self.call("npc/perf")
        except RuntimeError as error:
            self.report.setdefault("perf", {})[label] = {"unavailable": str(error)}
            return
        sampled = time.monotonic() - began
        rows = [row for row in report.get("systems", []) if match in row.get("name", "")]
        self.report.setdefault("perf", {})[label] = {
            "frames": report.get("frames"),
            "mean_frame_ms": round(1000.0 * sampled / report["frames"], 2) if report.get("frames") else None,
            "systems": rows}

    def entities(self):
        return len(query(self.url, ["bevy_transform::components::transform::Transform"]))


def cheap_cottage(run):
    """A copy of the shipped rules whose cottage needs no materials and 40
    ticks of work: authored rules given to the host, not a simulation write."""
    preset = json.loads((ROOT / "content/island_content/worlds/living_world.json")
                        .read_text(encoding="utf-8"))
    changed = []

    def retune(node):
        if isinstance(node, dict):
            if node.get("id") == "cottage" and "work_ticks" in node:
                node["cost"] = []
                node["work_ticks"] = 40
                changed.append(node["id"])
            for value in node.values():
                retune(value)
        elif isinstance(node, list):
            for value in node:
                retune(value)

    retune(preset)
    if not changed:
        raise RuntimeError("The shipped rules have no cottage to change")
    edited = run / "living_world.rules.json"
    edited.write_text(json.dumps(preset), encoding="utf-8")
    return edited


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/houses")
    parser.add_argument("--skip-build", action="store_true",
                        help="leave out the player-built cottage and run on the shipped rules")
    parser.add_argument("--pin-night", action="store_true",
                        help="only the home with a sleeper, with the light pinned to midnight "
                             "(ISLAND_VIEW_DAY_PHASE=0.85): residents rest around sunset, when "
                             "the real light is still that of dusk")
    parser.add_argument("--days", type=float, default=2.0,
                        help="game days to wait for residents to finish the placed cottage")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    run = ROOT / "target" / ("playtest-houses-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    save = run / "living-world.world.json"
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_STATS"] = "1"
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    if args.pin_night:
        environment["ISLAND_VIEW_DAY_PHASE"] = "0.85"
    # The front end of island_game starts the coastal world from its rules file.
    shipped = ROOT / "content/island_content/worlds/living_world.json"
    rules = ["--island-rules", str(shipped if args.skip_build or args.pin_night else cheap_cottage(run))]
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "pinned_night": args.pin_night,
        "rules": "shipped" if args.skip_build or args.pin_night else
                 "copy of the shipped rules; a cottage costs nothing and takes 40 ticks of work",
        "input": "native BRP widget activation; view camera placement; a left click through the "
                 "window cursor and MouseButtonInput messages; bounded operator clock steps; one "
                 "recorded PlaceBuilding command",
        "physical_input_tested": False, "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end",
                "--island-save", str(save), *rules, "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = HousesFlow(run, child, url, report, time.monotonic())
            flow.log_path = run / "native.log"
            flow.deadline = time.monotonic() + 60 * 60
            flow.tick_budget = int((args.days + 4) * DAY)
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Oversee"))
            flow.call("npc/pause")
            time.sleep(1.0)
            # The save gives the script the ground's height under a camera focus.
            flow.press("Menu")
            flow.click("Save game")
            flow.terrain = Terrain(json.loads(save.read_text(encoding="utf-8")))
            flow.click("Resume")
            flow.call("npc/pause")
            flow.park_pointer()

            # 0. Sunrise: everybody starts at home. Who is drawn?
            census, world, homes = flow.census()
            report["sunrise"] = census
            harbour = world.homes("Harbour")
            row = sorted((home for home in harbour if home["door"][1] < -30000),
                         key=lambda home: home["door"][0])
            street_x = sum(home["door"][0] for home in row) / len(row) / 1000.0
            street_z = row[0]["door"][1] / 1000.0
            if args.pin_night:
                flow.sleeper(row, "asleep-pinned-night")
                flow.press("Menu")
                flow.click("Quit game")
                child.wait(timeout=40)
                report["ok"] = child.returncode == 0
                return
            report["entities_idle"] = flow.entities()
            flow.frame("street-sunrise-front-40m", street_x, street_z, 40, running=True, settle=2.0)
            flow.perf("idle_nothing_cut_away")

            # 1. A resident on a doorstep, beside the closed home, by day.
            found = flow.at_a_door()
            report["doorstep"] = found
            if found:
                door = found["door"]
                closed = flow.entities()
                flow.frame("doorstep-day-front-16m", door[0] / 1000.0, door[1] / 1000.0 - 1.0, 16)
                flow.frame("doorstep-day-side-16m", door[0] / 1000.0, door[1] / 1000.0 - 1.0, 16,
                           yaw=math.pi / 2)
                # An inside is some seventy entities. A door that opens or
                # shuts while somebody steps through it is four.
                after = flow.entities()
                report["checks"].append({"check": "a home under a 16 m camera keeps its roof",
                                         "ok": abs(after - closed) <= 8,
                                         "entities": [closed, after]})

            # 2. A working morning, then a home selected by a click: its cutaway.
            flow.to_tick(max(flow.state()["tick"], 2600))
            census, world, homes = flow.census()
            report["morning"] = census
            home = row[len(row) // 2]
            body = (home["door"][0] / 1000.0, home["door"][1] / 1000.0 - 3.5)
            flow.look(body[0], body[1], 22, "home-select", 0.0)
            before = flow.entities()
            clicked = flow.click_centre()
            report["home_card"] = [text for text in flow.texts()
                                   if any(word in text for word in ("Home", "Beds", "Lives here", "hearth"))][:8]
            flow.park_pointer()
            time.sleep(0.6)
            opened = flow.entities()
            report["checks"].append({"check": "a click on a home builds its inside",
                                     "ok": clicked and opened > before + 20, "entities": [before, opened],
                                     "home": home["label"]})
            flow.frame("cutaway-day-front-22m", body[0], body[1], 22)
            flow.frame("cutaway-day-corner-14m", body[0], body[1], 14, yaw=math.pi / 4)
            flow.frame("cutaway-day-back-14m", body[0], body[1], 14, yaw=math.pi)
            flow.perf("one_home_cut_away")
            report["checks"].append({"check": "the home card has a close control",
                                     "ok": flow.close_card()})
            # Out again at a distance: the inside is gone.
            flow.look(body[0], body[1], 40, "home-closed", 0.0)
            time.sleep(0.8)
            closed = flow.entities()
            report["checks"].append({"check": "closing the card removes the inside",
                                     "ok": closed <= before + 2, "entities": [before, opened, closed]})
            # A close camera alone opens the home it looks at.
            flow.frame("cutaway-day-zoom-12m", body[0], body[1], 12, yaw=math.pi / 5)
            report["checks"].append({"check": "a 12 m camera on a home opens it",
                                     "ok": flow.entities() > closed + 20})

            # 3. The inn: outside, then cut away.
            inns = [(sim_id, parts) for sim_id, parts in world.having(PLACE)
                    if parts[PLACE].get("role") == "innkeeper" and parts[PLACE]["label"].startswith("Harbour")]
            if inns:
                inn_id, parts = inns[0]
                site = xz(parts["pos"])
                inn = (site[0] / 1000.0, site[1] / 1000.0 + 3.5)
                report["inn"] = {"id": inn_id, "label": parts[PLACE]["label"], "site": site}
                flow.frame("inn-outside-front-24m", inn[0], inn[1] - 2.0, 24, yaw=math.pi)
                flow.frame("inn-outside-corner-24m", inn[0], inn[1] - 2.0, 24, yaw=math.pi * 0.75)
                flow.look(inn[0], inn[1], 22, "inn-select", math.pi)
                report["checks"].append({"check": "a click on the inn builds its inside",
                                         "ok": flow.click_centre()})
                report["inn"]["card"] = [text for text in flow.texts()
                                         if any(word in text for word in ("Workplace", "Works here", "Innkeeper"))][:6]
                flow.park_pointer()
                flow.frame("inn-cutaway-front-22m", inn[0], inn[1], 22, yaw=math.pi)
                flow.frame("inn-cutaway-corner-14m", inn[0], inn[1], 14, yaw=math.pi * 0.75)
                flow.close_card()

            # 4. A cottage the player places and residents build.
            if not args.skip_build:
                centre = (round(street_x), round(street_z) + 9)
                placed = None
                before_sites = set(sim_id for sim_id, _ in flow.world().having(SITE))
                for radius in (0, 3, 6, 9, 12):
                    for turn in range(8 if radius else 1):
                        x = centre[0] + round(radius * math.cos(turn * math.pi / 4))
                        z = centre[1] + round(radius * math.sin(turn * math.pi / 4))
                        try:
                            flow.call("npc/command", {"cmd": {
                                "type": "PlaceBuilding", "building": "cottage", "x": x, "z": z,
                                "rotation": 0}})
                        except RuntimeError as error:
                            report.setdefault("placement_refusals", []).append(
                                {"cell": [x, z], "error": str(error)[:160]})
                            continue
                        flow.call("npc/step", {"ticks": 2})
                        time.sleep(0.3)
                        new = [sim_id for sim_id, _ in flow.world().having(SITE)
                               if sim_id not in before_sites]
                        if new:
                            placed = {"site": new[0], "cell": [x, z]}
                            break
                    if placed:
                        break
                report["placed"] = placed
                if placed:
                    site_id = placed["site"]
                    plot = flow.world().parts(site_id)[SITE]["plot"]
                    mid = ((plot["lo_x"] + plot["hi_x"] + 1) / 2.0, (plot["lo_z"] + plot["hi_z"] + 1) / 2.0)
                    flow.frame("built-site-24m", mid[0], mid[1], 24, yaw=math.pi)
                    began = flow.state()["tick"]
                    flow.call("npc/speed", {"multiplier": 10})
                    flow.call("npc/resume")
                    done = False
                    try:
                        while flow.state()["tick"] - began < args.days * DAY:
                            flow.check()
                            time.sleep(1.0)
                            if BUILT in flow.world().parts(site_id):
                                done = True
                                break
                    finally:
                        flow.call("npc/pause")
                        flow.call("npc/speed", {"multiplier": 1})
                    placed.update(built=done, ticks=flow.state()["tick"] - began, plot=plot)
                    report["checks"].append({"check": "residents finished the placed cottage",
                                             "ok": done, "ticks": placed["ticks"]})
                    if done:
                        time.sleep(1.0)
                        # A plot of rotation 0 has its entrance on its -z side.
                        flow.frame("built-cottage-outside-20m", mid[0], mid[1], 20, yaw=math.pi)
                        flow.frame("built-cottage-corner-20m", mid[0], mid[1], 20, yaw=math.pi * 0.75)
                        flow.look(mid[0], mid[1], 20, "built-select", math.pi)
                        report["checks"].append({"check": "a click on the built cottage builds its inside",
                                                 "ok": flow.click_centre()})
                        flow.park_pointer()
                        report["built_card"] = [text for text in flow.texts()
                                                if any(word in text for word in ("Home", "Beds", "Lives here"))][:8]
                        flow.frame("built-cottage-cutaway-20m", mid[0], mid[1], 20, yaw=math.pi)
                        flow.frame("built-cottage-cutaway-corner-13m", mid[0], mid[1], 13, yaw=math.pi * 0.75)
                        flow.close_card()

            # 5. Night on the real clock. A rest is short (300 ticks and up), so
            # the script steps from sunset until somebody of the street is
            # asleep in bed, and captures that home with the clock stopped.
            flow.sleeper(row, "asleep-2300")
            flow.to_phase(0.85)  # 02:24.
            census, world, homes = flow.census()
            report["night"] = census
            report["checks"].append({"check": "nobody with a home rests outdoors at night",
                                     "ok": not census["housed_resting_outdoors"],
                                     "resting_indoors": census["resting_indoors"]})
            flow.frame("street-night-front-40m", street_x, street_z, 40, running=True, settle=3.0)
            flow.frame("street-night-front-24m", street_x, street_z + 1.0, 24, running=True, settle=1.5)
            flow.frame("street-night-back-62m", street_x, street_z, 62, yaw=math.pi, running=True, settle=1.5)
            asleep = None
            for home_row in sorted(row, key=lambda home: abs(home["door"][0] / 1000.0 - street_x)):
                people = census["by_building"].get(str(home_row["id"]), [])
                if len(people) >= 2 or (asleep is None and people):
                    asleep = (home_row, people)
                    if len(people) >= 2:
                        break
            report["night_home"] = None if asleep is None else {
                "label": asleep[0]["label"], "inside": asleep[1], "heated": asleep[0]["heated"]}
            if asleep is not None:
                home = asleep[0]
                body = (home["door"][0] / 1000.0, home["door"][1] / 1000.0 - 3.5)
                flow.look(body[0], body[1], 22, "night-select", 0.0)
                report["night_home"]["opened_by_click"] = flow.click_centre()
                flow.park_pointer()
                flow.frame("cutaway-night-front-22m", body[0], body[1], 22, running=True, settle=1.5)
                flow.frame("cutaway-night-corner-13m", body[0], body[1], 13, yaw=math.pi / 4,
                           running=True, settle=1.5)
                flow.frame("cutaway-night-top-13m", body[0], body[1], 13, yaw=math.pi * 0.8,
                           running=True, settle=1.5)
                flow.close_card()

            flow.press("Menu")
            flow.click("Quit game")
            child.wait(timeout=40)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("houses_failure", error=str(error))
            except Exception as diagnostic:
                report["diagnostic_error"] = str(diagnostic)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace[-60:]
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        log_path = run / "native.log"
        if log_path.exists():
            lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
            report["native_errors"] = [line for line in lines if "ERROR" in line or "panicked at" in line][-30:]
            report["scene_stats"] = [line[line.index(STATS):] for line in lines if STATS in line]
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for image in sorted((run / "shots").glob("*.png")) if (run / "shots").is_dir() else []:
            shutil.copy2(image, args.out / image.name)
            copied.append(str(args.out / image.name))
        report["copied"] = copied
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        shutil.copy2(run / "summary.json",
                     args.out / ("summary-pinned-night.json" if args.pin_night else "summary.json"))
        print(run, flush=True)


if __name__ == "__main__":
    main()
