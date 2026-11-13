"""Native captures of the island's world: fitted workplaces, an open door,
crop fields, the merchant boat, the weather and sleepers of both rigs.

Build first, then run from npc-framework:
  python scripts/playtest/capture_world.py [--binary target/debug/island_game.exe]
      [--only main|boat|rain|storm|snow] [--field-days 2]

Five disposable runs of the real game, driven over BRP like
`capture_houses.py` (controls by name, the camera by writing `RtsCamera`, a
home selected by a real left click, the clock stepped while paused):

1. `main`, on a copy of the shipped rules whose weather is always clear, so
   the scenes compare: the whole island at noon and at night, each settlement's
   workplaces, a resident beside a workplace, a door somebody has just stepped
   through (`ISLAND_VIEW_DOOR_HOLD=6` keeps it open long enough to frame; in
   play it shuts after 0.6 s), a field early and later, and a sleeper of
   each rig in a cut-away home. In this run crops
   may also be sown in autumn and grow in half a day, so the fields change
   within the run; in the shipped rules a new island starts in autumn and
   its fields lie fallow until spring.
2. `boat`: the merchant boat sailing in, moored and sailing out, framed on
   the berth the view logs for it.
3. `rain`, `storm`, `snow`: the same rules with only that weather, by day, at
   night for rain, and from the island overview.

Residents, fields, the boat and who sleeps are found with operator state reads
(`world/state`) and the save the pause menu writes. `npc/perf` is sampled for
the `island_world` systems in clear weather and in each kind of weather, and
`ISLAND_VIEW_STATS=1` logs the mean frame of each run.

Second pass (`--only scale|live|harbour|rain|storm|snow|harsh`, written to
target/npc-out/world2/ by default):

- `scale`: the island at noon and at night, each settlement's workplaces from
  the front, every workplace model from its entrance side with the resident
  standing nearest its door, a door caught open at the shipped hold (no
  `ISLAND_VIEW_DOOR_HOLD`), and each work pose close up and at settlement zoom.
- `live`: Live with the played character's adventurer rig outdoors, a home and
  a tree between the camera and the character (Oversee at the same camera is
  the "before"; the host logs `island unblock:` lines), and the character's
  homestead cottage cut away if the world has one.
- `harbour`: the jetty and quay empty, then with the boat moored and goods on
  the quay.
- `harsh`: the shipped rules with a harsh winter certain from the first
  winter: the deep-snow ground tint without falling snow.

Third pass (`--only bodies|cast|precip|flash|shocks|poses|cottage`, written to
target/npc-out/world3/ by default; `--only third` runs them all):

- `bodies`: one run per adventurer body forced on the played character
  (`ISLAND_VIEW_PLAYER_BODY`), the same tick and spot from the Live camera at
  9 and 18 m beside the nearest resident.
- `cast`: before (`ISLAND_VIEW_CAST=mixed`) and after (one family): a market,
  a smithy, the street, a resident at a door and a home cut away at night.
- `precip`: rain, storm and snow at 22, 45 and 90 m and over the island.
- `flash`: a storm with the lightning flash held (`ISLAND_VIEW_FLASH=hold`).
- `shocks`: drought and blight tints forced in the view (`ISLAND_VIEW_SHOCK`).
- `poses`: chop and fish at 8 m from the clearest side, tools at 20 m, and a
  selected resident behind a house at a close Oversee camera.
- `cottage`: the UI agent's prepared saves in target/npc-out/fixtures/, when
  they exist.

Captures are 1280x720 and still need to be read.
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
from capture_houses import HOME, INDOORS, PLACE, STATS, VILLAGER, HousesFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import PRIMARY, wait_for
from capture_view_checks import DAY, SETTLEMENTS, xz
from capture_view_followups import CAMERA, Terrain

capture_player_flow.SIZES = ((1280, 720),)

FIELD = "regional_field_v1"
BUILT = "regional_building_v1"
PROFILE = "regional_resident_profile_v1"
MERCHANT = "regional_merchant_v1"
SHIPPED = ROOT / "content/island_content/worlds/living_world.json"


def rules_with_weather(run, kind, quick_crops=False):
    """A copy of the shipped rules in which every day has `kind` of weather
    and no shock: authored rules given to the host, not a simulation write.
    With `quick_crops`, crops may also be sown in autumn and grow in
    half a day (two days in the shipped rules, sown in spring and summer
    only): a new island starts in autumn, and its fields would lie fallow
    for the whole run."""
    preset = json.loads(SHIPPED.read_text(encoding="utf-8"))
    changed = []

    def retune(node):
        if isinstance(node, dict):
            if "forecast_days" in node and "spring" in node:
                for season in ("spring", "summer", "autumn", "winter"):
                    node[season] = {name: (1 if name == kind else 0)
                                    for name in ("clear", "rain", "storm", "snow")}
                for shock in ("drought_permille", "blight_permille", "harsh_winter_permille"):
                    if shock in node:
                        node[shock] = 0
                changed.append(kind)
            if quick_crops and "sow_seasons" in node and "growth_ticks" in node:
                node["sow_seasons"] = ["spring", "summer", "autumn"]
                node["growth_ticks"] = 12000
            for value in node.values():
                retune(value)
        elif isinstance(node, list):
            for value in node:
                retune(value)

    retune(preset)
    if not changed:
        raise RuntimeError("The shipped rules have no weather table to change")
    edited = run / f"living_world.{kind}.rules.json"
    edited.write_text(json.dumps(preset), encoding="utf-8")
    return edited


class WorldFlow(HousesFlow):
    save_path = None

    def saved(self):
        """The world as the pause menu saves it: resources the state read
        does not carry (the merchant's round)."""
        self.press("Menu")
        self.click("Save game")
        snapshot = json.loads(self.save_path.read_text(encoding="utf-8"))
        self.click("Resume")
        self.call("npc/pause")
        return snapshot

    def hold(self, code, seconds):
        """A key held down for `seconds` with the clock running."""
        logical = {"Character": code[3:].lower()} if code.startswith("Key") else code
        event = {"key_code": code, "logical_key": logical, "state": "Pressed", "text": None,
                 "repeat": False, "window": self.window}
        self.call("npc/resume")
        self.call("world.write_message", {"message": "bevy_input::keyboard::KeyboardInput", "value": event})
        time.sleep(seconds)
        event["state"] = "Released"
        self.call("world.write_message", {"message": "bevy_input::keyboard::KeyboardInput", "value": event})
        time.sleep(0.5)
        self.call("npc/pause")

    def key(self, code):
        """One key press and release as the window reports it."""
        logical = {"Character": code[3:].lower()} if code.startswith("Key") else code
        for state in ("Pressed", "Released"):
            self.call("world.write_message", {"message": "bevy_input::keyboard::KeyboardInput", "value": {
                "key_code": code, "logical_key": logical, "state": state, "text": None,
                "repeat": False, "window": self.window}})
            time.sleep(0.12)
        time.sleep(0.45)

    def merchant(self):
        return self.saved().get("resources", {}).get(MERCHANT, {})

    def snap_now(self, label):
        """A screenshot at once, without settling the window: for something
        that lasts a fraction of a second."""
        self.snap(f"{label}-1280x720", (1280, 720))

    def perf(self, label, seconds=4.0, match=("island_world", "island_homes", "island_poses")):
        """Per-system cost of the world, homes and pose passes."""
        try:
            self.call("npc/perf/reset")
            began = time.monotonic()
            time.sleep(seconds)
            report = self.call("npc/perf")
        except RuntimeError as error:
            self.report.setdefault("perf", {})[label] = {"unavailable": str(error)}
            return
        sampled = time.monotonic() - began
        rows = {row["name"].split("island_view::")[-1]: [row["p50_us"], row["p95_us"]]
                for row in report.get("systems", []) if any(part in row.get("name", "") for part in match)}
        self.report.setdefault("perf", {})[label] = {
            "frames": report.get("frames"),
            "mean_frame_ms": round(1000.0 * sampled / report["frames"], 2) if report.get("frames") else None,
            "p50_p95_us": rows, "sum_p50_us": round(sum(row[0] for row in rows.values()), 1)}

    def today(self):
        """The calendar day the journal's header names (first day is 1)."""
        for text in self.texts():
            if text.startswith("Day ") and text[4:].split(" ")[0].isdigit():
                return int(text[4:].split(" ")[0])
        return None

    def fields(self):
        """Fields: label, plot and how many plots are at each stage."""
        world, rows = self.world(), []
        for sim_id, parts in world.having(FIELD):
            plot = parts.get(BUILT, {}).get("plot")
            stages = {}
            growth = []
            for one in parts[FIELD].get("plots", []):
                stage = str(one.get("stage"))
                stages[stage] = stages.get(stage, 0) + 1
                growth.append(one.get("growth", 0))
            rows.append({"id": sim_id, "label": parts.get(BUILT, {}).get("label"), "plot": plot,
                         "stages": stages, "growth": growth, "tick": world.tick})
        return rows

    def field_shot(self, label, field):
        plot = field["plot"]
        mid = ((plot["lo_x"] + plot["hi_x"] + 1) / 2.0, (plot["lo_z"] + plot["hi_z"] + 1) / 2.0)
        self.frame(label + "-24m", mid[0], mid[1], 24, yaw=math.pi * 0.8)
        self.frame(label + "-50m", mid[0], mid[1], 50, yaw=math.pi * 0.8)


def launch(binary, run, rules, extra_env):
    run.mkdir(parents=True)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_STATS"] = "1"
    # Every first-run tip counted as seen, so none covers a capture.
    environment["ISLAND_TIPS_SEEN"] = "all"
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    environment.update(extra_env)
    log = (run / "native.log").open("wb")
    child = subprocess.Popen([str(binary), "--front-end", "--island-save",
        str(run / "living-world.world.json"), "--island-rules", str(rules), "--remote",
        "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run)], cwd=ROOT,
        env=environment, stdout=log, stderr=subprocess.STDOUT,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    return child, f"http://127.0.0.1:{port}"


def enter(binary, run, rules, report, extra_env, days):
    child, url = launch(binary, run, rules, extra_env)
    flow = WorldFlow(run, child, url, report, time.monotonic())
    flow.log_path = run / "native.log"
    flow.save_path = run / "living-world.world.json"
    flow.deadline = time.monotonic() + 70 * 60
    flow.tick_budget = int((days + 4) * DAY)
    flow.bootstrap()
    flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
    flow.click("New game")
    flow.click("Oversee the village")
    wait_for(lambda: flow.widget("Oversee"))
    flow.call("npc/pause")
    time.sleep(1.0)
    flow.press("Menu")
    flow.click("Save game")
    flow.terrain = Terrain(json.loads(flow.save_path.read_text(encoding="utf-8")))
    flow.click("Resume")
    flow.call("npc/pause")
    flow.park_pointer()
    # The first-run tips would cover the top of every capture.
    for _ in range(6):
        try:
            flow.click("Got it")
        except RuntimeError:
            break
    return child, flow


def leave(child, flow, run, report, label):
    try:
        flow.press("Menu")
        flow.click("Quit game")
        child.wait(timeout=40)
    finally:
        if child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        lines = (run / "native.log").read_text(encoding="utf-8", errors="replace").splitlines()
        report.setdefault("native_errors", {})[label] = [
            line for line in lines if "ERROR" in line or "panicked at" in line][-20:]
        report.setdefault("scene_stats", {})[label] = [
            line[line.index(STATS):] for line in lines if STATS in line]


def street(flow):
    world = flow.world()
    row = sorted((home for home in world.homes("Harbour") if home["door"][1] < -30000),
                 key=lambda home: home["door"][0])
    return world, row, sum(home["door"][0] for home in row) / len(row) / 1000.0, row[0]["door"][1] / 1000.0


def main_run(binary, base, report, field_days):
    run = base / "main"
    child, flow = enter(binary, run, rules_with_weather(base, "clear", quick_crops=True), report,
                        {"ISLAND_VIEW_DOOR_HOLD": "6"}, field_days + 2)
    try:
        world, row, street_x, street_z = street(flow)

        # 1. A door: somebody of the street steps out. The street is framed
        # first; the moment a resident leaves, one capture is taken at once,
        # then the home is framed while its door is held open.
        flow.settle((1280, 720))
        flow.look(street_x, street_z - 1.0, 30, "door-street", 0.0)
        inside = {home["id"]: True for home in row}
        stepped = None
        for _ in range(600):
            flow.call("npc/step", {"ticks": 2})
            world = flow.world()
            for home in row:
                people = [sim_id for sim_id, parts in world.having(VILLAGER)
                          if (parts.get(INDOORS) or {}).get("building") == home["id"]]
                if inside[home["id"]] and not people:
                    stepped = home
                inside[home["id"]] = bool(people)
            if stepped:
                break
        report["door"] = None if stepped is None else {"home": stepped["label"], "tick": world.tick}
        if stepped:
            flow.snap_now("door-step-street-30m")
            door = stepped["door"]
            flow.look(door[0] / 1000.0, door[1] / 1000.0 - 1.0, 17, "door-open", 0.35)
            flow.snap_now("door-open-17m")
            # The resident on the doorstep, selected with a click, then seen
            # from behind their home: the home is taken out of the way.
            x, z = door[0] / 1000.0, door[1] / 1000.0
            flow.look(x, z, 17, "unblock-select", 0.0)
            before = flow.entities()
            flow.pointer_click(640, 325)
            time.sleep(0.8)
            report["unblock"] = {"home": stepped["label"],
                                 "entities_after_click": [before, flow.entities()]}
            flow.snap_now("unblock-front-17m")
            flow.look(x, z, 20, "unblock-behind", math.pi)
            time.sleep(0.6)
            flow.snap_now("unblock-behind-20m")
            flow.look(x, z, 20, "unblock-corner", math.pi * 0.75)
            time.sleep(0.6)
            flow.snap_now("unblock-corner-20m")
            flow.close_card()
            # A click on open ground lets go of the selection.
            flow.look(x, z - 9.0, 17, "unblock-clear", 0.0)
            flow.pointer_click(640, 360)
            time.sleep(0.4)

        # 2. Noon: the whole island, each settlement's workplaces, a resident
        # beside a workplace.
        flow.to_phase(0.35)
        flow.whole_island()
        flow.capture("island-noon")
        flow.perf("clear")
        world = flow.world()
        places = {sim_id: parts for sim_id, parts in world.having(PLACE)}
        for name in SETTLEMENTS:
            sites = [xz(parts["pos"]) for parts in places.values()
                     if parts[PLACE]["label"].startswith(name)]
            if not sites:
                continue
            cx = sum(site[0] for site in sites) / len(sites) / 1000.0
            cz = sum(site[1] for site in sites) / len(sites) / 1000.0
            slug = name.lower().replace(" ", "-")
            flow.frame(f"workplaces-{slug}-front-66m", cx, cz + 2.0, 66, yaw=math.pi)
            flow.frame(f"workplaces-{slug}-back-66m", cx, cz + 2.0, 66, yaw=0.0)
        # One workplace of every model with whoever stands nearest to it,
        # looked for again through the working day for the models nobody
        # was near.
        kinds = {"smith": "smithy", "miller": "mill", "woodcutter": "timber-yard",
                 "carpenter": "timber-yard", "miner": "mine", "baker": "bakery",
                 "farmer": "market", "tailor": "market", "tanner": "market",
                 "hunter": "lodge", "innkeeper": "inn", "water_keeper": "well"}
        order = ["smithy", "mill", "timber-yard", "mine", "bakery", "market", "lodge", "inn", "well"]
        scale = {}
        for attempt in range(6):
            world = flow.world()
            people = [(parts[VILLAGER]["name"], xz(parts["pos"]))
                      for _, parts in world.having(VILLAGER) if "pos" in parts]
            best = {}
            for _, parts in world.having(PLACE):
                kind = kinds.get(parts[PLACE].get("role"))
                if not kind or kind in scale or "pos" not in parts:
                    continue
                site = xz(parts["pos"])
                name, away = min(((name, math.hypot(at[0] - site[0], at[1] - site[1]))
                                  for name, at in people), key=lambda pair: pair[1])
                if kind not in best or away < best[kind]["metres"] * 1000.0:
                    best[kind] = {"kind": kind, "role": parts[PLACE].get("role"), "site": site,
                                  "label": parts[PLACE]["label"], "resident": name,
                                  "metres": round(away / 1000.0, 1), "tick": flow.state()["tick"]}
            last = attempt == 5
            for kind in order:
                entry = best.get(kind)
                if entry and (entry["metres"] <= 4.0 or last):
                    scale[kind] = entry
                    x, z = entry["site"][0] / 1000.0, entry["site"][1] / 1000.0
                    flow.frame(f"scale-{kind}-18m", x, z + 1.0, 18, yaw=math.pi * 0.88)
            if len(scale) == len(order):
                break
            flow.to_tick(flow.state()["tick"] + DAY // 24)
        report["beside_workplaces"] = [scale[kind] for kind in order if kind in scale]

        # 3. Fields now; again later in the run.
        fields = flow.fields()
        report["fields"] = {"day": fields}
        harbour = next((field for field in fields if str(field["label"]).startswith("Harbour")),
                       fields[0] if fields else None)
        if harbour:
            flow.field_shot("field-early", harbour)

        # 5. The field again, once it has changed (two game days grow a crop).
        if harbour:
            seen = [harbour["stages"]]
            for index in range(int(field_days * 2)):
                flow.to_tick(flow.state()["tick"] + DAY // 4)
                now = next((field for field in flow.fields() if field["id"] == harbour["id"]), None)
                if now is None:
                    break
                grown = max(now["growth"] or [0])
                if now["stages"] not in seen or (index % 2 == 1 and grown):
                    if now["stages"] not in seen:
                        seen.append(now["stages"])
                    report["fields"].setdefault("later", []).append(now)
                    # At once: a ripe plot is harvested within the hour. Only
                    # a field found changed in the dark waits for daylight.
                    if now["tick"] % DAY > int(0.68 * DAY):
                        flow.to_phase(0.05)
                    flow.field_shot(f"field-later-{len(report['fields']['later'])}", now)
                if len(report["fields"].get("later", [])) >= 3:
                    break

        # 6. Night: the island, and a sleeper of each rig.
        flow.to_phase(0.85)
        flow.whole_island()
        flow.capture("island-night")
        census, world, homes = flow.census()
        report["night"] = {key: census[key] for key in
                           ("tick", "indoors", "outdoors", "resting_indoors", "housed_resting_outdoors")}
        sleepers = {}
        for building, people in census["by_building"].items():
            home = homes.get(int(building))
            if home is None:
                continue
            for person in people:
                parts = world.parts(person)
                if parts[VILLAGER].get("resting_until", 0) - world.tick < 200:
                    continue
                rig = "civilian" if parts.get(PROFILE, {}).get("appearance", 0) % 8 < 4 else "adventurer"
                sleepers.setdefault(rig, {"name": parts[VILLAGER]["name"], "home": home["label"],
                                          "door": xz(home["door"])})
        report["sleepers"] = sleepers
        for rig, entry in sleepers.items():
            body = (entry["door"][0] / 1000.0, entry["door"][1] / 1000.0 - 3.5)
            flow.look(body[0], body[1], 22, f"sleeper-{rig}", 0.0)
            entry["opened_by_click"] = flow.click_centre()
            flow.park_pointer()
            flow.frame(f"asleep-{rig}-corner-11m", body[0], body[1], 11, yaw=math.pi / 4, settle=1.5)
            flow.frame(f"asleep-{rig}-side-11m", body[0], body[1], 11, yaw=-math.pi / 2, settle=1.0)
            flow.close_card()
    finally:
        leave(child, flow, run, report, "main")
    return run


def boat_run(binary, base, report):
    """The merchant boat: sailing in, moored, sailing out."""
    run = base / "boat"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 6)
    try:
        flow.to_phase(0.35)
        merchant = flow.merchant()
        report["merchant_at_noon"] = {key: merchant.get(key) for key in
                                      ("next_visit_day", "docked_until", "visits", "post")}
        report["trading_post_plot"] = flow.world().parts(merchant.get("post")).get(BUILT, {}).get("plot")
        # Where the view moors the boat: its own line in the host's log.
        berth = [line.split("berth: ")[1] for line in
                 (run / "native.log").read_text(encoding="utf-8", errors="replace").splitlines()
                 if "merchant boat berth: " in line]
        report["boat_berth"] = berth[-1] if berth else None
        today = flow.today()
        report["today"] = today
        if not berth or merchant.get("next_visit_day") is None or today is None:
            raise RuntimeError("No berth, visit day or calendar day to find the boat by")
        where = berth[-1].split(";")[0].split(",")
        mid = (float(where[0]), float(where[1]))
        # The landing is the first tick past `arrive_permille` (150 in the
        # shipped rules) of the visit's day. `next_visit_day` counts days from
        # 0; the journal's header counts today from 1.
        now = flow.state()["tick"]
        days = int(merchant["next_visit_day"]) - (today - 1)
        landing = now - now % DAY + days * DAY + int(0.15 * DAY)
        report["boat_landing_tick"] = landing
        if not 0 < landing - now < 6 * DAY:
            raise RuntimeError(f"The boat's landing at tick {landing} is not within six days of {now}")
        flow.to_tick(landing - 240)
        flow.frame("boat-sailing-in-60m", mid[0], mid[1], 60, yaw=0.0)
        flow.frame("boat-sailing-in-30m", mid[0], mid[1], 30, yaw=math.pi / 2)
        docked = None
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            docked = flow.merchant().get("docked_until")
            if docked:
                break
        report["boat_docked_until"] = docked
        report["boat_tick"] = flow.state()["tick"]
        time.sleep(1.0)
        for yaw, side in ((0.0, "a"), (math.pi, "b"), (math.pi / 2, "c")):
            flow.frame(f"boat-moored-{side}-30m", mid[0], mid[1], 30, yaw=yaw)
        flow.frame("boat-moored-90m", mid[0], mid[1], 90, yaw=0.0)
        if docked:
            flow.to_tick(int(docked) + 180)
            time.sleep(1.0)
            flow.frame("boat-sailing-out-60m", mid[0], mid[1], 60, yaw=0.0)
    finally:
        leave(child, flow, run, report, "boat")
    return run


def weather_run(binary, base, report, kind):
    run = base / kind
    child, flow = enter(binary, run, rules_with_weather(base, kind), report, {}, 2)
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.3)
        flow.frame(f"weather-{kind}-day-45m", street_x, street_z + 6.0, 45, running=True, settle=4.5)
        flow.frame(f"weather-{kind}-day-22m", street_x, street_z + 2.0, 22, running=True, settle=1.5)
        flow.perf(kind)
        if kind == "storm":
            # Lightning is a double flash every 7.3 s: a burst of captures.
            flow.look(street_x, street_z + 6.0, 45, "storm-flash", 0.0)
            flow.call("npc/resume")
            for index in range(10):
                flow.snap_now(f"weather-storm-flash-{index}")
                time.sleep(0.25)
            flow.call("npc/pause")
        flow.whole_island()
        flow.capture(f"weather-{kind}-island")
        if kind in ("rain", "snow"):
            flow.to_phase(0.85)
            flow.frame(f"weather-{kind}-night-45m", street_x, street_z + 6.0, 45, running=True, settle=3.0)
        report.setdefault("weather", {})[kind] = {
            "tick": flow.state()["tick"],
            "texts": [text for text in flow.texts() if kind in text.lower()][:4]}
    finally:
        leave(child, flow, run, report, kind)
    return run


def rules_with_shock(run, shock):
    """Clear weather every day and `shock` certain from the first season
    that can have it, with no grace: authored rules, not a simulation write."""
    path = rules_with_weather(run, "clear")
    preset = json.loads(path.read_text(encoding="utf-8"))

    def retune(node):
        if isinstance(node, dict):
            if "forecast_days" in node and "spring" in node:
                node["shock_grace_seasons"] = 0
                for name in ("drought_permille", "blight_permille", "harsh_winter_permille"):
                    node[name] = 1000 if name.startswith(shock) else 0
            for value in node.values():
                retune(value)
        elif isinstance(node, list):
            for value in node:
                retune(value)

    retune(preset)
    edited = run / f"living_world.{shock}.rules.json"
    edited.write_text(json.dumps(preset), encoding="utf-8")
    return edited


def log_lines(run, marker):
    return [line[line.index(marker):] for line in
            (run / "native.log").read_text(encoding="utf-8", errors="replace").splitlines()
            if marker in line]


FISH = "village_fish"


def workers(world):
    """`World.workers` plus fishers."""
    found = world.workers()
    found["fish"] = []
    for sim_id, parts in world.having(VILLAGER):
        gathering = parts.get("island_gathering")
        if not gathering or "pos" not in parts or "move_intent" in parts:
            continue
        kind = world.parts(gathering["patch"]).get("island_resource_patch", {}).get("kind")
        if kind == FISH:
            found["fish"].append({"id": sim_id, "name": parts[VILLAGER]["name"], "at": xz(parts["pos"]),
                                  "left": gathering["ends"] - world.tick})
    found["fish"].sort(key=lambda row: -row["left"])
    return found


def tree_unblock(flow, run, x, z):
    """A woodcutter selected at a close camera, the camera turned until a
    tree stands between it and them: after (selected), then before at the
    same camera with nobody selected."""
    flow.look(x, z, 12, "tree-select", math.pi * 0.85)
    flow.pointer_click(640, 340)
    time.sleep(0.8)
    flow.park_pointer()
    for step in range(16):
        yaw = step * math.pi / 8
        lines = log_lines(run, "island unblock:")
        flow.look(x, z, 14, "unblock-tree-sweep", yaw)
        time.sleep(0.5)
        now = log_lines(run, "island unblock:")
        if len(now) > len(lines) and "(0 trees)" not in now[-1]:
            flow.snap_now("unblock-tree-selected-after")
            flow.close_card()
            flow.pointer_click(1180, 420)
            time.sleep(0.6)
            flow.park_pointer()
            flow.look(x, z, 14, "unblock-tree", yaw)
            time.sleep(1.0)
            flow.snap_now("unblock-tree-unselected-before")
            return {"yaw": yaw, "log": now[-2:]}
    flow.close_card()
    return {"found": False, "log": log_lines(run, "island unblock:")[-3:]}


def scale_run(binary, base, report):
    """Workplaces beside a resident, the settlements, a door at 1x, poses."""
    run = base / "scale"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 3)
    try:
        world, row, street_x, street_z = street(flow)
        # 1. A door at the shipped hold: one home, framed, stepped two ticks
        # at a time until somebody leaves it; one capture at once.
        flow.to_phase(0.04)
        world = flow.world()
        homes = {sim_id: parts[HOME] for sim_id, parts in world.having(HOME)}
        inside = lambda w, home: [sim_id for sim_id, parts in w.having(VILLAGER)
                                  if (parts.get(INDOORS) or {}).get("building") == home]
        # The middle of the street, framed at 20 m: any of the five homes
        # nearest the middle that somebody leaves.
        report["door_1x"] = None
        flow.settle((1280, 720))
        middle = sorted(row, key=lambda home: abs(home["door"][0] / 1000.0 - street_x))[:5]
        flow.look(street_x, street_z - 0.5, 20, "door-1x", 0.0)
        was = {home["id"]: len(inside(world, home["id"])) for home in middle}
        for _ in range(1500):
            flow.call("npc/step", {"ticks": 2})
            now = flow.world()
            left = [home for home in middle if len(inside(now, home["id"])) < was[home["id"]]]
            was = {home["id"]: len(inside(now, home["id"])) for home in middle}
            if left:
                flow.snap_now("door-open-1x-20m")
                time.sleep(0.6)
                flow.snap_now("door-open-1x-20m-later")
                report["door_1x"] = {"home": left[0]["label"], "tick": now.tick,
                                     "door": left[0]["door"]}
                break

        # 2. Noon: the island and each settlement's workplaces.
        flow.to_phase(0.35)
        flow.whole_island()
        flow.capture("island-noon")
        flow.perf("clear")
        world = flow.world()
        places = {sim_id: parts for sim_id, parts in world.having(PLACE)}
        for name in SETTLEMENTS:
            sites = [xz(parts["pos"]) for parts in places.values()
                     if parts[PLACE]["label"].startswith(name)]
            if not sites:
                continue
            cx = sum(site[0] for site in sites) / len(sites) / 1000.0
            cz = sum(site[1] for site in sites) / len(sites) / 1000.0
            slug = name.lower().replace(" ", "-")
            flow.frame(f"workplaces-{slug}-front-66m", cx, cz + 2.0, 66, yaw=math.pi)
            flow.frame(f"workplaces-{slug}-front-40m", cx, cz + 2.0, 40, yaw=math.pi * 0.9)

        # 3. Every workplace model from its entrance side, the resident
        # nearest its door beside it; looked for again through the day.
        kinds = {"smith": "smithy", "miller": "mill", "woodcutter": "timber-yard",
                 "carpenter": "timber-yard", "miner": "mine", "baker": "bakery",
                 "farmer": "market", "tailor": "market", "tanner": "market",
                 "hunter": "lodge", "innkeeper": "inn", "water_keeper": "well"}
        order = ["smithy", "mill", "timber-yard", "mine", "bakery", "market", "lodge", "inn"]
        scale = {}
        for attempt in range(10):
            world = flow.world()
            people = [(parts[VILLAGER]["name"], xz(parts["pos"]))
                      for _, parts in world.having(VILLAGER)
                      if "pos" in parts and (parts.get(INDOORS) or {}).get("building") is None]
            best = {}
            for _, parts in world.having(PLACE):
                kind = kinds.get(parts[PLACE].get("role"))
                if not kind or kind in scale or "pos" not in parts:
                    continue
                site = xz(parts["pos"])
                name, away = min(((name, math.hypot(at[0] - site[0], at[1] - site[1]))
                                  for name, at in people), key=lambda pair: pair[1])
                if kind not in best or away < best[kind]["metres"] * 1000.0:
                    best[kind] = {"kind": kind, "role": parts[PLACE].get("role"), "site": site,
                                  "label": parts[PLACE]["label"], "resident": name,
                                  "metres": round(away / 1000.0, 1), "tick": world.tick}
            last = attempt == 9
            for kind in order:
                entry = best.get(kind)
                if entry and (entry["metres"] <= 2.5 or last):
                    scale[kind] = entry
                    x, z = entry["site"][0] / 1000.0, entry["site"][1] / 1000.0
                    # The body lies 3.5 m behind the site (+z); the camera
                    # stands on the entrance side.
                    flow.frame(f"scale-{kind}-front-16m", x, z + 1.8, 16, yaw=math.pi * 0.92)
            if len(scale) == len(order):
                break
            flow.to_tick(flow.state()["tick"] + DAY // 48)
        report["beside_workplaces"] = [scale[kind] for kind in order if kind in scale]

        # 4. Work poses, close and at settlement zoom.
        flow.to_phase(0.3)
        poses = {}
        for attempt in range(12):
            found = workers(flow.world())
            for kind in ("chop", "hammer", "farm", "fish"):
                rows = [row for row in found[kind] if row["left"] >= 40]
                if kind in poses or not rows:
                    continue
                row = rows[0]
                poses[kind] = row
                x, z = row["at"][0] / 1000.0, row["at"][1] / 1000.0
                flow.frame(f"pose-{kind}-8m", x, z, 8, yaw=math.pi * 0.85, running=True, settle=0.8)
                flow.frame(f"pose-{kind}-40m", x, z, 40, yaw=math.pi * 0.85, running=True, settle=0.4)
                if kind == "chop" and "unblock_tree" not in report:
                    report["unblock_tree"] = tree_unblock(flow, base / "scale", x, z)
            if len(poses) == 4:
                break
            # A fisher is rarer: after the first tries, step an hour at a time.
            flow.to_tick(flow.state()["tick"] + (DAY // 48 if attempt < 16 else DAY // 24))
        report["poses"] = poses

        # 5. Night.
        flow.to_phase(0.85)
        flow.whole_island()
        flow.capture("island-night")
    finally:
        leave(child, flow, run, report, "scale")
    return run


def played_id(flow):
    village = flow.saved().get("resources", {}).get("island_village", {})
    return village.get("played")


def live_run(binary, base, report):
    """Live: the player's rig, unblocking, the homestead cottage."""
    run = base / "live"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 2)
    try:
        flow.to_phase(0.3)
        flow.whole_island()
        flow.click("Live")
        flow.call("npc/resume")
        time.sleep(4.0)
        flow.call("npc/pause")
        for _ in range(3):
            try:
                flow.click("Got it")
            except RuntimeError:
                break
        played = played_id(flow)
        world = flow.world()
        me = world.parts(played)
        report["live"] = {"played": played, "components": sorted(me.keys())[:40]}
        if "pos" not in me:
            raise RuntimeError(f"No position for the played character {played}")
        at = xz(me["pos"])
        x, z = at[0] / 1000.0, at[1] / 1000.0
        report["live"]["at"] = [x, z]
        flow.frame("live-player-outdoors-9m", x, z, 9, yaw=math.pi * 0.85, running=True, settle=1.2)
        flow.frame("live-player-outdoors-18m", x, z, 18, yaw=0.3, running=True, settle=1.0)

        # A home, then a tree, between the camera and the character: the
        # camera is turned to look across the nearest one at 22 m. After in
        # Live; before at the same camera in Oversee, where nobody is followed.
        def nearest(points, low, high):
            rows = sorted((math.hypot(px - x, pz - z), (px, pz)) for px, pz in points)
            return next((point for away, point in rows if low <= away <= high), None)

        def low(label, distance, yaw, pitch=0.42):
            """The camera lower than it rests, so a roof or a crown between
            it and the character hides them."""
            flow.look(x, z, distance, label, yaw)
            entity, _ = flow.camera_entity()
            flow.call("world.mutate_components", {"entity": entity, "component": CAMERA,
                                                  "path": "pitch", "value": pitch})

        def across(label, target, distance=22.0):
            yaw = math.atan2(target[0] - x, target[1] - z)
            low(label, distance, yaw)
            time.sleep(1.2)
            flow.snap_now(f"{label}-live-after")
            cut = log_lines(run, "island unblock:")
            flow.click("Oversee")
            time.sleep(0.8)
            low(label, distance, yaw)
            time.sleep(1.0)
            flow.snap_now(f"{label}-oversee-before")
            flow.click("Live")
            time.sleep(0.8)
            return {"target": target, "yaw": yaw, "unblock_log": cut[-3:]}

        bodies = []
        for sim_id, parts in world.having(HOME):
            door = xz(parts[HOME]["door"])
            # The body stands behind its door; which side is in the save.
            bodies.append((door[0] / 1000.0, door[1] / 1000.0 + (3.0 if door[1] > at[1] else -3.0)))
        home = nearest(bodies, 4.0, 14.0)
        report["live"]["unblock_home"] = across("unblock-home", home, 20.0) if home else None
        flow.call("npc/resume")
        time.sleep(2.0)
        flow.perf("live_unblock")
        flow.call("npc/pause")
        # Off to the well among the trees ("Go and do it"), then look for a
        # tree between the camera and the character.
        try:
            flow.click("Go and do it")
        except RuntimeError as error:
            report["live"]["go_and_do_it"] = str(error)
        flow.call("npc/resume")
        time.sleep(12.0)
        flow.call("npc/pause")
        moved = flow.world().parts(played)
        if "pos" in moved:
            at = xz(moved["pos"])
            x, z = at[0] / 1000.0, at[1] / 1000.0
        report["live"]["at_well"] = [x, z]
        flow.frame("live-player-at-work-9m", x, z, 9, yaw=math.pi * 0.85, running=True, settle=1.2)
        report["live"]["unblock_tree"] = None
        if True:
            # Trees are view dressing, not in the save: sweep the camera round
            # the character until the host logs a tree out of the way.
            for step in range(16):
                yaw = step * math.pi / 8
                before = len(log_lines(run, "trees)"))
                low("unblock-tree-sweep", 20, yaw, 0.3)
                time.sleep(0.9)
                lines = log_lines(run, "island unblock:")
                if len(log_lines(run, "trees)")) > before and lines and "(0 trees)" not in lines[-1]:
                    flow.snap_now("unblock-tree-live-after")
                    flow.click("Oversee")
                    time.sleep(0.8)
                    low("unblock-tree", 20, yaw, 0.3)
                    time.sleep(1.0)
                    flow.snap_now("unblock-tree-oversee-before")
                    flow.click("Live")
                    time.sleep(0.8)
                    report["live"]["unblock_tree"] = {"yaw": yaw, "log": lines[-2:]}
                    break

        # The homestead cottage, if the world gives the character one.
        cottages = [(sim_id, parts) for sim_id, parts in world.having(HOME)
                    if parts[HOME].get("label", "").lower().startswith(("homestead", "your"))
                    or played in parts[HOME].get("residents", [])]
        report["live"]["cottages"] = [[sim_id, parts[HOME].get("label")] for sim_id, parts in cottages]
        for sim_id, parts in cottages[:1]:
            door = xz(parts[HOME]["door"])
            flow.look(door[0] / 1000.0, door[1] / 1000.0, 14, "cottage", math.pi * 0.8)
            flow.frame("live-cottage-14m", door[0] / 1000.0, door[1] / 1000.0, 14, yaw=math.pi * 0.8)
        report["live"]["log"] = log_lines(run, "island unblock:")[-12:]
    finally:
        leave(child, flow, run, report, "live")
    return run


def harbour_run(binary, base, report):
    """The jetty and quay empty, then with the boat in."""
    run = base / "harbour"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 6)
    try:
        flow.to_phase(0.35)
        merchant = flow.merchant()
        jetty = log_lines(run, "harbour jetty:")
        berth = log_lines(run, "merchant boat berth:")
        report["harbour"] = {"jetty": jetty[-1:] if jetty else None, "berth": berth[-1:] if berth else None,
                             "post": merchant.get("post"),
                             "post_plot": flow.world().parts(merchant.get("post")).get(BUILT, {}).get("plot")}
        if not jetty:
            raise RuntimeError("The host logged no harbour jetty")
        foot = jetty[-1].split("foot ")[1].split(";")[0].split(",")
        head = jetty[-1].split("head ")[1].split(";")[0].split(",")
        foot = (float(foot[0]), float(foot[1]))
        head = (float(head[0]), float(head[1]))
        mid = ((foot[0] + head[0]) / 2.0, (foot[1] + head[1]) / 2.0)
        out = math.atan2(head[0] - foot[0], head[1] - foot[1])
        flow.frame("harbour-empty-24m", mid[0], mid[1], 24, yaw=out + math.pi / 2)
        flow.frame("harbour-empty-45m", mid[0], mid[1], 45, yaw=out + math.pi * 0.75)
        flow.frame("harbour-quay-12m", foot[0], foot[1], 12, yaw=out + math.pi * 0.7)
        today = flow.today()
        now = flow.state()["tick"]
        if merchant.get("next_visit_day") is None or today is None:
            raise RuntimeError("No visit day to wait for the boat by")
        days = int(merchant["next_visit_day"]) - (today - 1)
        landing = now - now % DAY + days * DAY + int(0.15 * DAY)
        report["harbour"]["landing_tick"] = landing
        flow.to_tick(landing - 120)
        docked = None
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            docked = flow.merchant().get("docked_until")
            if docked:
                break
        report["harbour"]["docked_until"] = docked
        time.sleep(1.0)
        flow.frame("harbour-moored-24m", mid[0], mid[1], 24, yaw=out + math.pi / 2)
        flow.frame("harbour-moored-45m", mid[0], mid[1], 45, yaw=out + math.pi * 0.75)
        flow.frame("harbour-moored-quay-12m", foot[0], foot[1], 12, yaw=out + math.pi * 0.7)
        if docked:
            flow.to_tick(int(docked) + 600)
            time.sleep(1.0)
            flow.frame("harbour-after-sailing-24m", mid[0], mid[1], 24, yaw=out + math.pi / 2)
    finally:
        leave(child, flow, run, report, "harbour")
    return run


def harsh_run(binary, base, report):
    """Clear days and a harsh winter certain: deep snow lies without falling."""
    run = base / "harsh"
    child, flow = enter(binary, run, rules_with_shock(base, "harsh_winter"), report, {}, 12)
    try:
        world, row, street_x, street_z = street(flow)
        for _ in range(10):
            if any("winter" in text.lower() for text in flow.texts()):
                break
            flow.to_tick(flow.state()["tick"] + DAY)
        flow.to_phase(0.3)
        flow.call("npc/resume")
        time.sleep(30.0)
        flow.call("npc/pause")
        report["harsh"] = {"tick": flow.state()["tick"],
                           "texts": [text for text in flow.texts() if "inter" in text][:4]}
        flow.frame("shock-harsh-winter-day-45m", street_x, street_z + 6.0, 45, running=True, settle=8.0)
        flow.whole_island()
        flow.capture("shock-harsh-winter-island")
    finally:
        leave(child, flow, run, report, "harsh")
    return run


# Third pass (`--only bodies|cast|precip|flash|shocks|poses|cottage`, written
# to target/npc-out/world3/ by default). Each override below is a view-only
# environment variable read once by the host (docs/island-atmosphere.md).

BODIES = ("rogue", "unhooded_rogue", "knight", "barbarian", "mage")
BLOCKER = "grid_blocker"


def blockers(world):
    """Solid building bodies on the ground: (lo_x, lo_z, hi_x, hi_z) metres."""
    boxes = []
    for _, parts in world.having(BLOCKER):
        if "pos" not in parts:
            continue
        x, z = xz(parts["pos"])
        hx, hz = parts[BLOCKER]["half_x_mm"], parts[BLOCKER]["half_z_mm"]
        boxes.append(((x - hx) / 1000.0, (z - hz) / 1000.0, (x + hx) / 1000.0, (z + hz) / 1000.0))
    return boxes


def box_distance(box, x, z):
    dx = max(box[0] - x, 0.0, x - box[2])
    dz = max(box[1] - z, 0.0, z - box[3])
    return math.hypot(dx, dz)


def clear_side(boxes, x, z, reach=9.0):
    """The camera yaw whose side of the subject is clearest of buildings: the
    camera stands at (sin yaw, cos yaw) from its focus."""
    best = None
    for step in range(16):
        yaw = step * math.pi / 8
        clearance = min([box_distance(box, x + math.sin(yaw) * t, z + math.cos(yaw) * t)
                         for box in boxes for t in (1.0, 3.0, 5.0, reach)] or [99.0])
        if best is None or clearance > best[1]:
            best = (yaw, clearance)
    return best


def enter_live(flow):
    flow.click("Live")
    time.sleep(1.5)
    for _ in range(3):
        try:
            flow.click("Got it")
        except RuntimeError:
            break
    flow.park_pointer()


def bodies_run(binary, base, report, body, cast=None, prefix=None):
    """The played character in `body` at the start, from the Live camera, at
    9 and 18 m beside the resident nearest them: same tick and spot every run.
    With `cast` (`ISLAND_VIEW_CAST`) and `prefix`, the same group is captured
    as a before/after of the residents' family."""
    prefix = prefix or f"player-body-{body}"
    run = base / prefix
    env = {"ISLAND_VIEW_PLAYER_BODY": body}
    if cast:
        env["ISLAND_VIEW_CAST"] = cast
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, env, 1)
    try:
        flow.to_phase(0.3)
        enter_live(flow)
        played = played_id(flow)
        # Step (the same ticks every run) until a resident outdoors passes
        # within 3 m of the waiting character; frame the two together.
        for _ in range(240):
            world = flow.world()
            at = xz(world.parts(played)["pos"])
            near = sorted((math.hypot(xz(parts["pos"])[0] - at[0], xz(parts["pos"])[1] - at[1]) / 1000.0,
                           parts[VILLAGER]["name"], xz(parts["pos"]))
                          for sim_id, parts in world.having(VILLAGER)
                          if sim_id != played and "pos" in parts
                          and (parts.get(INDOORS) or {}).get("building") is None)
            if near and near[0][0] <= 3.0:
                break
            flow.call("npc/step", {"ticks": 10})
        x, z = at[0] / 1000.0, at[1] / 1000.0
        if near and near[0][0] <= 3.0:
            x, z = (at[0] + near[0][2][0]) / 2000.0, (at[1] + near[0][2][1]) / 2000.0
        report.setdefault("bodies", {})[prefix] = {"tick": world.tick, "player": [at[0] / 1000.0, at[1] / 1000.0],
                                                 "nearest_resident": near[0][:2] if near else None}
        time.sleep(4.0)
        flow.frame(f"{prefix}-9m", x, z, 9, yaw=math.pi * 0.85, settle=1.5)
        flow.frame(f"{prefix}-18m", x, z, 18, yaw=0.3, settle=1.0)
        if cast:
            flow.frame(f"{prefix}-front-12m", x, z, 12, yaw=math.pi * 1.85, settle=1.0)
    finally:
        leave(child, flow, run, report, prefix)
    return run


def cast_run(binary, base, report, cast):
    """Before (`mixed`, the roster before this pass) or after (the default):
    a market and a smithy with residents, a resident at a door, and a home cut
    away with somebody asleep in it."""
    label = "before" if cast == "mixed" else "after"
    run = base / f"cast-{label}"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report,
                        {"ISLAND_VIEW_CAST": cast} if cast != "default" else {}, 3)
    entry = report.setdefault("cast", {}).setdefault(label, {})
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.35)
        world = flow.world()
        sites = {}
        for _, parts in world.having(PLACE):
            role = parts[PLACE].get("role")
            kind = {"farmer": "market", "tailor": "market", "tanner": "market", "smith": "smithy"}.get(role)
            if kind and kind not in sites and "pos" in parts:
                sites[kind] = (xz(parts["pos"]), parts[PLACE]["label"])
        entry["sites"] = {kind: label_ for kind, (_, label_) in sites.items()}

        def outdoors(world):
            return [xz(parts["pos"]) for _, parts in world.having(VILLAGER)
                    if "pos" in parts and (parts.get(INDOORS) or {}).get("building") is None]

        # Each workplace once two residents stand within 9 m of it (the same
        # ticks in both runs: the clock is stepped, never run).
        for kind, (site, _) in sorted(sites.items()):
            for _ in range(90):
                near = [at for at in outdoors(flow.world())
                        if math.hypot(at[0] - site[0], at[1] - site[1]) < 9000]
                if len(near) >= 2:
                    break
                flow.call("npc/step", {"ticks": 20})
            entry.setdefault("near", {})[kind] = [len(near), flow.state()["tick"]]
            x, z = site[0] / 1000.0, site[1] / 1000.0
            flow.frame(f"cast-{label}-{kind}-16m", x, z + 1.8, 16, yaw=math.pi * 0.92)
        # The thickest group of residents outdoors, at 20 m.
        people = outdoors(flow.world())
        if people:
            centre = max(people, key=lambda at: sum(
                math.hypot(at[0] - other[0], at[1] - other[1]) < 8000 for other in people))
            entry["crowd"] = [centre[0] / 1000.0, centre[1] / 1000.0]
            flow.frame(f"cast-{label}-crowd-20m", centre[0] / 1000.0, centre[1] / 1000.0, 20,
                       yaw=math.pi * 0.85)
        door = flow.at_a_door(budget=1200)
        entry["door"] = door
        if door:
            flow.frame(f"cast-{label}-at-door-8m", door["door"][0] / 1000.0, door["door"][1] / 1000.0 - 1.0,
                       8, yaw=math.pi * 0.85, settle=1.0)
        flow.sleeper(row, f"cast-{label}-asleep")
        entry["sleeper"] = report.get("sleeper")
    finally:
        leave(child, flow, run, report, f"cast-{label}")
    return run


def precip_run(binary, base, report, kind):
    """`kind` of weather at 22, 45 and 90 m and over the whole island."""
    run = base / f"precip-{kind}"
    child, flow = enter(binary, run, rules_with_weather(base, kind), report, {}, 1)
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.3)
        for distance in (22, 45, 90):
            flow.frame(f"precip-{kind}-day-{distance}m", street_x, street_z + 4.0, distance, running=True,
                       settle=4.0 if distance == 22 else 1.5)
            flow.perf(f"{kind}-{distance}m")
        flow.whole_island()
        flow.call("npc/resume")
        time.sleep(2.0)
        flow.capture(f"precip-{kind}-island")
        flow.perf(f"{kind}-island")
        flow.call("npc/pause")
        entry = report.setdefault("precip", {})
        entry[kind] = {"tick": flow.state()["tick"], "camera": flow.camera_entity()[1]}
    finally:
        leave(child, flow, run, report, f"precip-{kind}")
    return run


def flash_run(binary, base, report):
    """A storm with the lightning flash held (`ISLAND_VIEW_FLASH=hold`)."""
    run = base / "flash"
    child, flow = enter(binary, run, rules_with_weather(base, "storm"), report,
                        {"ISLAND_VIEW_FLASH": "hold"}, 1)
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.3)
        flow.frame("lightning-flash-day-45m", street_x, street_z + 4.0, 45, running=True, settle=4.0)
        flow.frame("lightning-flash-day-22m", street_x, street_z + 2.0, 22, running=True, settle=1.0)
        flow.to_phase(0.85)
        flow.frame("lightning-flash-night-45m", street_x, street_z + 4.0, 45, running=True, settle=2.0)
        flow.whole_island()
        flow.capture("lightning-flash-island")
    finally:
        leave(child, flow, run, report, "flash")
    return run


def shock_run(binary, base, report, shock):
    """A shock's tints on the ground and the fields, forced in the view
    (`ISLAND_VIEW_SHOCK`) on clear quick-crop rules: the simulation has no
    drought or blight then; the fields are as they grow."""
    run = base / f"shock-{shock}"
    child, flow = enter(binary, run, rules_with_weather(base, "clear", quick_crops=True), report,
                        {"ISLAND_VIEW_SHOCK": shock}, 2)
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.3)
        flow.to_tick(flow.state()["tick"] + DAY // 3)
        fields = flow.fields()
        report.setdefault("shocks", {})[shock] = {"fields": fields}
        harbour = next((field for field in fields if str(field["label"]).startswith("Harbour")),
                       fields[0] if fields else None)
        if harbour:
            flow.field_shot(f"shock-{shock}-field", harbour)
        flow.frame(f"shock-{shock}-street-45m", street_x, street_z + 4.0, 45, running=True, settle=6.0)
        flow.whole_island()
        flow.capture(f"shock-{shock}-island")
    finally:
        leave(child, flow, run, report, f"shock-{shock}")
    return run


def select_resident(flow, name):
    """A real click on the resident under the middle of the window; True once
    their name shows in the window's texts."""
    for y in (338, 350, 325, 360, 312):
        flow.pointer_click(640, y)
        time.sleep(0.8)
        if any(name in text for text in flow.texts()):
            flow.park_pointer()
            return True
    flow.park_pointer()
    return False


def poses_run(binary, base, report):
    """Chop with the axe and fish with the rod at 8 m from their clearest
    side, every work tool at 20 m; then a selected resident behind a house at
    a close Oversee camera, before and after."""
    run = base / "poses"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 3)
    try:
        flow.to_phase(0.3)
        poses = {}
        for attempt in range(40):
            if attempt >= 16 and "fish" in poses:
                break
            world = flow.world()
            boxes = blockers(world)
            found = workers(world)
            for kind in ("chop", "fish", "hammer", "farm"):
                if kind in poses:
                    continue
                rows = [row for row in found[kind] if row["left"] >= 40]
                ranked = []
                for row in rows:
                    x, z = row["at"][0] / 1000.0, row["at"][1] / 1000.0
                    room = min([box_distance(box, x, z) for box in boxes] or [99.0])
                    # Other people stand in the way too: a box of 0.6 m each.
                    people = [(px / 1000.0 - 0.3, pz / 1000.0 - 0.3, px / 1000.0 + 0.3, pz / 1000.0 + 0.3)
                              for (px, pz) in (xz(parts["pos"]) for sim_id, parts in world.having(VILLAGER)
                                               if sim_id != row["id"] and "pos" in parts)
                              if math.hypot(px / 1000.0 - x, pz / 1000.0 - z) < 10.0]
                    yaw, clearance = clear_side(boxes + people, x, z)
                    ranked.append((min(room, clearance), yaw, row))
                ranked.sort(key=lambda entry: -entry[0])
                if not ranked or (ranked[0][0] < 3.0 and attempt < 12 and kind in ("chop", "fish")):
                    continue
                clearance, yaw, row = ranked[0]
                x, z = row["at"][0] / 1000.0, row["at"][1] / 1000.0
                poses[kind] = {**row, "yaw": round(yaw, 3), "clearance_m": round(clearance, 1)}
                if kind in ("chop", "fish"):
                    flow.frame(f"pose-{kind}-8m", x, z, 8, yaw=yaw, running=True, settle=0.8)
                flow.frame(f"tool-{kind}-20m", x, z, 20, yaw=yaw, running=True, settle=0.5)
            if len(poses) == 4:
                break
            # A fisher is rarer: after the first tries, step an hour at a time.
            flow.to_tick(flow.state()["tick"] + (DAY // 48 if attempt < 16 else DAY // 24))
        report["poses"] = poses
        flow.perf("poses")

        # A resident just behind a house from the camera, at 14 m in Oversee.
        world = flow.world()
        boxes = blockers(world)
        homes = [box for box in boxes if (box[2] - box[0]) * (box[3] - box[1]) <= 32.0]
        chosen = None
        for sim_id, parts in world.having(VILLAGER):
            if "pos" not in parts or "move_intent" in parts or (parts.get(INDOORS) or {}).get("building"):
                continue
            x, z = xz(parts["pos"])[0] / 1000.0, xz(parts["pos"])[1] / 1000.0
            for box in homes:
                away = box_distance(box, x, z)
                if 1.0 <= away <= 3.0:
                    cx, cz = (box[0] + box[2]) / 2.0, (box[1] + box[3]) / 2.0
                    chosen = (sim_id, parts[VILLAGER]["name"], x, z, math.atan2(cx - x, cz - z))
                    break
            if chosen:
                break
        entry = report.setdefault("unblock_oversee", {"found": bool(chosen)})
        if chosen:
            sim_id, name, x, z, yaw = chosen
            entry.update({"resident": name, "at": [x, z], "yaw": round(yaw, 3)})
            # Selected from the open side, then the camera goes behind the house.
            flow.look(x, z, 12, "unblock-oversee-select", yaw + math.pi)
            entry["selected"] = select_resident(flow, name)
            before = log_lines(run, "island unblock:")
            flow.look(x, z, 14, "unblock-oversee", yaw)
            time.sleep(1.2)
            flow.snap_now("unblock-oversee-selected-after-14m")
            entry["log"] = log_lines(run, "island unblock:")[len(before):][-3:]
            flow.close_card()
            flow.look(x + math.sin(yaw + math.pi) * 6.0, z + math.cos(yaw + math.pi) * 6.0, 14,
                      "unblock-oversee-clear", yaw)
            flow.pointer_click(640, 360)
            time.sleep(0.6)
            flow.park_pointer()
            flow.look(x, z, 14, "unblock-oversee", yaw)
            time.sleep(1.2)
            flow.snap_now("unblock-oversee-unselected-before-14m")
    finally:
        leave(child, flow, run, report, "poses")
    return run


def storm_night_run(binary, base, report):
    """A storm at night at 45 m and over the island, after the near-lens
    streak fix."""
    run = base / "storm-night"
    child, flow = enter(binary, run, rules_with_weather(base, "storm"), report, {}, 2)
    try:
        world, row, street_x, street_z = street(flow)
        flow.to_phase(0.85)
        flow.frame("precip-storm-night-45m", street_x, street_z + 4.0, 45, running=True, settle=4.0)
        flow.frame("precip-storm-night-22m", street_x, street_z + 2.0, 22, running=True, settle=1.0)
        flow.to_phase(0.3)
        flow.frame("precip-storm-day-45m", street_x, street_z + 4.0, 45, running=True, settle=2.0)
        flow.whole_island()
        flow.call("npc/resume")
        time.sleep(2.0)
        flow.capture("precip-storm-island")
        flow.perf("storm-island")
        flow.call("npc/pause")
    finally:
        leave(child, flow, run, report, "storm-night")
    return run


FIXTURES = ROOT / "target/npc-out/fixtures"


def fixture_save(name):
    for candidate in (FIXTURES / name, FIXTURES / f"{name}.world.json", FIXTURES / f"{name}.json",
                      FIXTURES / name / "living-world.world.json"):
        if candidate.is_file():
            return candidate
    return None


def knight_run(binary, base, report):
    """The played character as the Knight (the island's rule, no override)
    in the `cottage-evening` fixture: inside the cut-away cottage, then
    outside after a few seconds of "Go and do it"."""
    return cottage_run(binary, base, report, "cottage-evening", knight=True)


def cottage_run(binary, base, report, name, knight=False):
    """A prepared save (the UI agent's fixture): the played character's
    cottage outside and cut away, the upgrade site, and the character asleep
    in their own bed at night. The fixture is copied, never written."""
    source = fixture_save(name)
    entry = report.setdefault("cottage", {})
    if source is None:
        entry[name] = {"missing": str(FIXTURES / name)}
        return None
    run = base / (f"knight-{name}" if knight else f"cottage-{name}")
    run.mkdir(parents=True)
    shutil.copy2(source, run / "living-world.world.json")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_STATS"] = "1"
    log = (run / "native.log").open("wb")
    child = subprocess.Popen([str(binary), "--front-end", "--island-save", str(run / "living-world.world.json"),
        "--remote", "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run)], cwd=ROOT,
        env=environment, stdout=log, stderr=subprocess.STDOUT,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    url = f"http://127.0.0.1:{port}"
    flow = WorldFlow(run, child, url, report, time.monotonic())
    flow.log_path = run / "native.log"
    flow.save_path = run / "living-world.world.json"
    flow.deadline = time.monotonic() + 40 * 60
    flow.tick_budget = int(3 * DAY)
    try:
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        flow.click("Continue")
        time.sleep(3.0)
        flow.call("npc/pause")
        flow.terrain = Terrain(json.loads(flow.save_path.read_text(encoding="utf-8")))
        for _ in range(4):
            try:
                flow.click("Got it")
            except RuntimeError:
                break
        try:
            enter_live(flow)
        except RuntimeError as error:
            entry.setdefault(name, {})["live"] = str(error)
        played = played_id(flow)
        world = flow.world()
        homes = [(sim_id, parts[HOME]) for sim_id, parts in world.having(HOME)
                 if played in parts[HOME].get("residents", [])]
        me = world.parts(played)
        if knight:
            # Indoors the character is drawn in the cut-away room, not on
            # the entrance point the simulation keeps: frame the body.
            house = next((xz(parts["pos"]) for sim_id, parts in world.having(HOME)
                          if sim_id == (homes[0][0] if homes else None) and "pos" in parts), None)
            at = house or xz(me["pos"])
            for where in ("inside", "outside"):
                if where == "outside":
                    # Walk out with the movement keys (WASD), trying each
                    # until the character is outdoors.
                    for code in ("KeyS", "KeyW", "KeyA", "KeyD"):
                        flow.hold(code, 1.6)
                        me = flow.world().parts(played)
                        if (me.get(INDOORS) or {}).get("building") is None:
                            flow.hold(code, 1.2)
                            me = flow.world().parts(played)
                            break
                    at = xz(me["pos"])
                report.setdefault("knight", {})[where] = {
                    "at": [at[0] / 1000.0, at[1] / 1000.0],
                    "indoors": (me.get(INDOORS) or {}).get("building")}
                flow.frame(f"live-player-knight-{where}-9m", at[0] / 1000.0, at[1] / 1000.0, 9,
                           yaw=math.pi * 0.75, settle=1.5)
                flow.frame(f"live-player-knight-{where}-18m", at[0] / 1000.0, at[1] / 1000.0, 18,
                           yaw=math.pi * 0.75, settle=1.0)
            return run
        entry[name] = {"played": played, "tick": world.tick, "homes": [[h[0], h[1].get("label")] for h in homes],
                       "indoors": (me.get(INDOORS) or {}).get("building"),
                       "texts": [text for text in flow.texts() if "ottage" in text or "chest" in text.lower()][:8]}
        if homes:
            door = xz(homes[0][1]["door"])
            x, z = door[0] / 1000.0, door[1] / 1000.0
            flow.frame(f"cottage-{name}-outside-16m", x, z, 16, yaw=math.pi * 0.8)
            flow.frame(f"cottage-{name}-outside-front-12m", x, z, 12, yaw=0.0)
            # Cut away: the cottage selected by a click on it (its card
            # opens its inside), unless the character is in it already.
            flow.look(x, z - 3.0, 16, "cottage-select", math.pi * 0.75)
            entry[name]["opened_by_click"] = flow.click_centre()
            flow.park_pointer()
            flow.frame(f"cottage-{name}-inside-11m", x, z - 3.0, 11, yaw=math.pi * 0.75, settle=1.5)
            flow.frame(f"cottage-{name}-inside-side-11m", x, z - 3.0, 11, yaw=-math.pi / 2, settle=1.0)
        # The homestead upgrade has no place of its own in the view: it is
        # the cottage itself, framed wider.
        if homes:
            door = xz(homes[0][1]["door"])
            flow.frame(f"cottage-{name}-corner-22m", door[0] / 1000.0, door[1] / 1000.0 - 2.5, 22,
                       yaw=math.pi * 0.25)
        if "pos" in me:
            at = xz(me["pos"])
            flow.frame(f"cottage-{name}-player-12m", at[0] / 1000.0, at[1] / 1000.0, 12, yaw=math.pi * 0.85)
        # Night: the player asks to sleep (E, as the prompt says), and the
        # clock is stepped until the character lies in their bed.
        flow.to_phase(0.85)
        entry[name]["prompt"] = [text for text in flow.texts() if "Sleep" in text][:2]
        flow.key("KeyE")
        flow.call("npc/resume")
        time.sleep(3.0)
        flow.call("npc/pause")
        for _ in range(24):
            me = flow.world().parts(played)
            if (me.get(INDOORS) or {}).get("building") is not None and \
                    me.get(VILLAGER, {}).get("resting_until", 0) > flow.state()["tick"]:
                break
            flow.call("npc/step", {"ticks": 60})
        entry[name]["night"] = {"indoors": (me.get(INDOORS) or {}).get("building"),
                                "resting_until": me.get(VILLAGER, {}).get("resting_until"),
                                "tick": flow.state()["tick"]}
        if homes:
            door = xz(homes[0][1]["door"])
            x, z = door[0] / 1000.0, door[1] / 1000.0
            flow.frame(f"cottage-{name}-asleep-11m", x, z - 3.0, 11, yaw=math.pi * 0.75, settle=1.5)
    finally:
        leave(child, flow, run, report, f"cottage-{name}")
    return run


# Fourth pass (`--only block|upgrade5x6`, target/npc-out/world4/).

def fixture_flow(binary, base, report, name, label, extra_env=None):
    """A disposable host on a copy of fixture `name`, continued into Live.
    Returns (child, flow, run, played); the fixture is never written."""
    source = fixture_save(name)
    if source is None:
        raise RuntimeError(f"No fixture {name} in {FIXTURES}")
    run = base / label
    run.mkdir(parents=True)
    shutil.copy2(source, run / "living-world.world.json")
    # Settings of the run's own, as `capture_ui_redesign.launch` writes them:
    # the title then offers Continue for the copied save.
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_STATS"] = "1"
    environment["ISLAND_TIPS_SEEN"] = "all"
    environment.update(extra_env or {})
    log = (run / "native.log").open("wb")
    child = subprocess.Popen([str(binary), "--front-end", "--island-save",
        str(run / "living-world.world.json"), "--remote", "--remote-port", str(port), "--no-render-remote",
        "--out-dir", str(run)], cwd=ROOT,
        env=environment, stdout=log, stderr=subprocess.STDOUT,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    url = f"http://127.0.0.1:{port}"
    flow = WorldFlow(run, child, url, report, time.monotonic())
    flow.log_path = run / "native.log"
    flow.save_path = run / "living-world.world.json"
    flow.deadline = time.monotonic() + 50 * 60
    flow.tick_budget = int(4 * DAY)
    flow.bootstrap()
    flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
    flow.click("Continue")
    time.sleep(3.0)
    flow.call("npc/pause")
    flow.terrain = Terrain(json.loads(flow.save_path.read_text(encoding="utf-8")))
    for _ in range(4):
        try:
            flow.click("Got it")
        except RuntimeError:
            break
    enter_live(flow)
    return child, flow, run, played_id(flow)


def walk_out(flow, played):
    """Out of the door with the movement keys, trying each until outdoors."""
    me = flow.world().parts(played)
    for code in ("KeyS", "KeyW", "KeyA", "KeyD"):
        flow.hold(code, 1.6)
        me = flow.world().parts(played)
        if (me.get(INDOORS) or {}).get("building") is None:
            flow.hold(code, 1.2)
            return flow.world().parts(played)
    return me


def block_run(binary, base, report, prefix):
    """A home between the camera and whoever is followed, with this build
    (`prefix` names it: before or after): leaving the own cottage in Live,
    then a selected resident behind a house at a close Oversee camera."""
    child, flow, run, played = fixture_flow(binary, base, report, "cottage-evening", f"block-{prefix}")
    entry = report.setdefault("block", {}).setdefault(prefix, {})
    try:
        # Inside first: where the character stands in the cut-away room.
        house = next((xz(parts["pos"]) for sim_id, parts in flow.world().having(HOME)
                      if played in parts[HOME].get("residents", []) and "pos" in parts), None)
        if house:
            for distance, yaw in ((9, math.pi * 0.75), (9, -math.pi * 0.25)):
                flow.frame(f"inside-standing-{distance}m-{'a' if yaw > 0 else 'b'}-{prefix}",
                           house[0] / 1000.0, house[1] / 1000.0, distance, yaw=yaw, settle=1.2)
        me = walk_out(flow, played)
        at = xz(me["pos"])
        x, z = at[0] / 1000.0, at[1] / 1000.0
        entry["leaving"] = {"at": [x, z], "indoors": (me.get(INDOORS) or {}).get("building")}
        for distance in (9, 18):
            flow.frame(f"block-live-leaving-{distance}m-{prefix}", x, z, distance, yaw=math.pi * 0.75,
                       settle=1.2)
        # The cottage between a low camera and the character: across it.
        house = next((xz(parts["pos"]) for sim_id, parts in flow.world().having(HOME)
                      if played in parts[HOME].get("residents", []) and "pos" in parts), None)
        if house:
            yaw = math.atan2(house[0] / 1000.0 - x, house[1] / 1000.0 - z)
            entry["across_yaw"] = round(yaw, 3)
            for distance, label in ((14, "across-14m"), (20, "across-20m")):
                flow.look(x, z, distance, "block-across", yaw)
                entity, _ = flow.camera_entity()
                flow.call("world.mutate_components", {"entity": entity, "component": CAMERA,
                                                      "path": "pitch", "value": 0.45})
                time.sleep(0.1)
                flow.snap_now(f"block-live-{label}-early-{prefix}")
                time.sleep(1.2)
                flow.snap_now(f"block-live-{label}-{prefix}")
        entry["live_log"] = log_lines(run, "island unblock:")[-3:]
        # Oversee: a resident just behind a house from the camera.
        flow.click("Oversee")
        time.sleep(1.0)
        for _ in range(4):
            try:
                flow.click("Got it")
            except RuntimeError:
                break
        world = flow.world()
        homes = [box for box in blockers(world) if (box[2] - box[0]) * (box[3] - box[1]) <= 32.0]
        chosen = None
        for sim_id, parts in world.having(VILLAGER):
            if sim_id == played or "pos" not in parts or "move_intent" in parts \
                    or (parts.get(INDOORS) or {}).get("building"):
                continue
            rx, rz = xz(parts["pos"])[0] / 1000.0, xz(parts["pos"])[1] / 1000.0
            for box in homes:
                if 1.0 <= box_distance(box, rx, rz) <= 3.0:
                    cx, cz = (box[0] + box[2]) / 2.0, (box[1] + box[3]) / 2.0
                    chosen = (parts[VILLAGER]["name"], rx, rz, math.atan2(cx - rx, cz - rz))
                    break
            if chosen:
                break
        entry["oversee"] = {"found": bool(chosen)}
        if chosen:
            name, rx, rz, yaw = chosen
            entry["oversee"].update({"resident": name, "at": [rx, rz], "yaw": round(yaw, 3)})
            flow.look(rx, rz, 12, "block-oversee-select", yaw + math.pi)
            entry["oversee"]["selected"] = select_resident(flow, name)
            flow.look(rx, rz, 14, "block-oversee", yaw)
            time.sleep(1.2)
            flow.snap_now(f"block-oversee-selected-14m-{prefix}")
            entry["oversee"]["log"] = log_lines(run, "island unblock:")[-2:]
            flow.close_card()
    finally:
        leave(child, flow, run, report, f"block-{prefix}")
    return run


def upgrade_run(binary, base, report):
    """The larger cottage finished from the `upgrade-underway` fixture: a
    builder hired and the character sent home through the Build workspace
    (recorded commands), the clock stepped until the upgrade is done; then
    the 5 by 6 m cottage outside and cut away, at 1280x720 and 1024x768."""
    child, flow, run, played = fixture_flow(binary, base, report, "upgrade-underway", "upgrade-5x6")
    entry = report.setdefault("upgrade", {})
    try:
        flow.key("KeyB")
        time.sleep(0.6)
        flow.call("npc/resume")
        try:
            flow.click("Hire a builder")
            entry["hired"] = True
        except RuntimeError as error:
            entry["hired"] = str(error)
        time.sleep(2.0)
        flow.call("npc/pause")
        flow.key("KeyB")
        flow.key("KeyB")
        try:
            flow.click("Go home and rest")
            entry["home"] = True
        except RuntimeError as error:
            entry["home"] = str(error)
        flow.key("KeyB")
        # The missing materials ordered from the residents (recorded
        # `LiveAct` commands, as the Build workspace's Order rows send them).
        site = (flow.saved().get("resources", {}).get("regional_live_v1", {})
                .get("upgrades", {}).get(str(played), {}))
        receipts = []
        for kind, units in site.get("cost", {}).items():
            missing = units - site.get("delivered", {}).get(kind, 0) - site.get("ordered", {}).get(kind, 0)
            if missing > 0:
                command = {"type": "LiveAct", "unit": played, "verb": "order", "kind": kind,
                           "quantity": missing}
                try:
                    receipts.append([command, flow.call("npc/command", {"cmd": command})])
                except RuntimeError as error:
                    receipts.append([command, str(error)])
        entry["orders"] = receipts
        stopped = False
        upgrades = None
        for _ in range(60):
            flow.to_tick(flow.state()["tick"] + 300)
            upgrades = flow.saved().get("resources", {}).get("regional_live_v1", {}).get("upgrades", {})
            mine = upgrades.get(str(played)) or upgrades.get(played)
            if not mine:
                break
            delivered = mine.get("delivered", {})
            cost = mine.get("cost", {})
            if not stopped and all(delivered.get(kind, 0) >= units for kind, units in cost.items()):
                # Stand at the door so the work runs (as the player would).
                flow.key("KeyX")
                stopped = True
        entry["left"] = upgrades
        entry["tick"] = flow.state()["tick"]
        world = flow.world()
        homes = [(sim_id, parts) for sim_id, parts in world.having(HOME)
                 if played in parts[HOME].get("residents", [])]
        entry["homes"] = [{"id": sim_id, "capacity": parts[HOME].get("capacity"),
                           "blocker": parts.get("grid_blocker"), "pos": parts.get("pos")}
                          for sim_id, parts in homes]
        if not homes:
            return run
        door = xz(homes[0][1][HOME]["door"])
        centre = xz(homes[0][1]["pos"]) if "pos" in homes[0][1] else door
        x, z = centre[0] / 1000.0, centre[1] / 1000.0
        old_sizes = capture_player_flow.SIZES
        capture_player_flow.SIZES = ((1280, 720), (1024, 768))
        try:
            # Outside: the character walks out first if the work left them in.
            me = flow.world().parts(played)
            if (me.get(INDOORS) or {}).get("building") is not None:
                walk_out(flow, played)
            flow.frame("upgrade-5x6-outside-22m", x, z, 22, yaw=math.pi * 0.8, settle=1.5)
            flow.frame("upgrade-5x6-outside-front-16m", door[0] / 1000.0, door[1] / 1000.0, 16, yaw=0.0)
            # Inside: the character is sent to sleep (a recorded `LiveAct`),
            # walks home and lies down: the played character's home is cut
            # away, seen from three sides.
            command = {"type": "LiveAct", "unit": played, "verb": "sleep"}
            try:
                entry["sleep"] = flow.call("npc/command", {"cmd": command})
            except RuntimeError as error:
                entry["sleep"] = str(error)
            for _ in range(20):
                flow.to_tick(flow.state()["tick"] + 60)
                me = flow.world().parts(played)
                if (me.get(INDOORS) or {}).get("building") is not None:
                    break
            entry["inside"] = (me.get(INDOORS) or {}).get("building")
            flow.frame("upgrade-5x6-inside-12m", x, z, 12, yaw=math.pi * 0.75, settle=1.5)
            flow.frame("upgrade-5x6-inside-side-12m", x, z, 12, yaw=-math.pi / 2, settle=1.0)
            flow.frame("upgrade-5x6-inside-18m", x, z, 18, yaw=math.pi * 0.25, settle=1.0)
        finally:
            capture_player_flow.SIZES = old_sizes
    finally:
        leave(child, flow, run, report, "upgrade-5x6")
    return run


# Fifth pass (`--only harbour5|homework|fisher|larger`, target/npc-out/world5/).

def harbour5_run(binary, base, report):
    """The berth the simulation gives, the boat in, the trading post at the
    waterfront, and a resident carrying goods to the post if one is caught."""
    run = base / "harbour5"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 6)
    entry = report.setdefault("harbour5", {})
    try:
        flow.to_phase(0.35)
        merchant = flow.merchant()
        berth = log_lines(run, "merchant boat berth:")
        entry["berth_log"] = berth[-1:]
        today = flow.today()
        now = flow.state()["tick"]
        days = int(merchant["next_visit_day"]) - (today - 1)
        landing = now - now % DAY + days * DAY + int(0.15 * DAY)
        flow.to_tick(landing - 120)
        docked = None
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            docked = flow.merchant().get("docked_until")
            if docked:
                break
        entry["docked_until"] = docked
        post_id = merchant.get("post")
        world = flow.world()
        post = xz(world.parts(post_id)["pos"]) if post_id and "pos" in world.parts(post_id) else None
        line = berth[-1] if berth else ""
        at = [float(v) for v in line.split("berth: ")[1].split(";")[0].split(",")] if "berth: " in line else None
        entry["post"] = post
        entry["berth"] = at
        if not at or not post:
            raise RuntimeError("No berth or post to frame")
        px, pz = post[0] / 1000.0, post[1] / 1000.0
        mid = ((at[0] + px) / 2.0, (at[1] + pz) / 2.0)
        out = math.atan2(at[0] - px, at[1] - pz)
        flow.frame("harbour5-moored-24m", mid[0], mid[1], 24, yaw=out + math.pi / 2, settle=1.5)
        flow.frame("harbour5-moored-40m", mid[0], mid[1], 40, yaw=out + math.pi * 0.75, settle=1.0)
        flow.frame("harbour5-quay-12m", mid[0], mid[1], 12, yaw=out + math.pi * 0.6, settle=1.0)
        # Somebody walking goods to the post: stepped until one is near.
        caught = None
        for _ in range(80):
            world = flow.world()
            for sim_id, parts in world.having(VILLAGER):
                if "pos" not in parts or "move_intent" not in parts:
                    continue
                if not world.bulk(parts):
                    continue
                rx, rz = xz(parts["pos"])
                if math.hypot(rx - post[0], rz - post[1]) < 14000:
                    caught = (sim_id, parts[VILLAGER]["name"], rx / 1000.0, rz / 1000.0, world.bulk(parts))
                    break
            if caught:
                break
            flow.call("npc/step", {"ticks": 15})
        entry["carrier"] = caught and {"name": caught[1], "at": [caught[2], caught[3]], "load": caught[4]}
        if caught:
            flow.frame("harbour5-carrier-12m", caught[2], caught[3], 12, yaw=out + math.pi / 2,
                       running=True, settle=0.4)
            flow.frame("harbour5-carrier-20m", caught[2], caught[3], 20, yaw=out + math.pi / 2, settle=0.6)
    finally:
        leave(child, flow, run, report, "harbour5")
    return run


def homework_run(binary, base, report):
    """Physical homestead work from `upgrade-underway`: a builder hired (or
    the fixture's own) and the missing materials ordered (recorded
    `LiveAct`s); haulers caught walking materials to the cottage door; then
    the clock stepped in small bounded steps until the builder is working a
    stage at the door (`UpgradeWork` on the builder, the site's `working`
    state), shot from 9 m and 20 m; and the house from one camera at that
    stage and at the next."""
    child, flow, run, played = fixture_flow(binary, base, report, "upgrade-underway", "homework")
    entry = report.setdefault("homework", {})
    work_key = "regional_upgrade_work_v1"

    def upgrade():
        return (flow.saved().get("resources", {}).get("regional_live_v1", {})
                .get("upgrades", {}).get(str(played)))

    try:
        receipts = []
        command = {"type": "LiveAct", "unit": played, "verb": "hire"}
        try:
            receipts.append([command, flow.call("npc/command", {"cmd": command})])
        except RuntimeError as error:
            receipts.append([command, str(error)])
        site = upgrade() or {}
        for kind, units in site.get("cost", {}).items():
            missing = units - site.get("delivered", {}).get(kind, 0) - site.get("ordered", {}).get(kind, 0)
            if missing > 0:
                command = {"type": "LiveAct", "unit": played, "verb": "order", "kind": kind,
                           "quantity": missing}
                try:
                    receipts.append([command, flow.call("npc/command", {"cmd": command})])
                except RuntimeError as error:
                    receipts.append([command, str(error)])
        entry["commands"] = receipts
        site = upgrade() or {}
        entry["site_at_start"] = {key: site.get(key) for key in
                                  ("builder", "builder_until", "work_ticks", "remaining_ticks", "tier")}
        builder_id = site.get("builder")
        world = flow.world()
        home = next(((sim_id, parts) for sim_id, parts in world.having(HOME)
                     if played in parts[HOME].get("residents", [])), None)
        door = xz(home[1][HOME]["door"]) if home else None
        centre = xz(home[1]["pos"]) if home and "pos" in home[1] else door
        entry["door"] = door
        entry["house"] = centre

        def away_from_house(px, pz, turn=0.0):
            # The camera on the far side of the worker from the cottage.
            return math.atan2(px - centre[0], pz - centre[1]) + turn

        # The operator's world read does not carry `UpgradeWork` (it is the
        # simulation's own component, not in the state export): read it from
        # the ECS by its type, with each holder's SimId.
        work_type = "island_content::village::live::UpgradeWork"
        sim_type = "npc_sim::ids::SimId"
        entry["work_source"] = "world.query " + work_type

        def at_work_now():
            try:
                rows = query(flow.url, [work_type, sim_type])
            except Exception as error:
                entry["work_query_error"] = str(error)
                return None
            found = {}
            for row in rows:
                sim = row["components"].get(sim_type)
                sim = sim[0] if isinstance(sim, list) else sim
                found[int(sim)] = row["components"].get(work_type)
            return found

        hauler, builder, states, stage_shots = None, None, [], []
        start = world.tick
        budget = int(2 * DAY)
        last = None
        finished = False
        while world.tick - start < budget and not finished:
            world = flow.world()
            working_now = at_work_now()
            if working_now is None:
                # Without the component read: the builder standing still at
                # the door, said so in the summary.
                entry["work_source"] = "inferred: the builder still within 2.5 m of the door"
                working_now = {sim: {"inferred": True} for sim, parts in world.having(VILLAGER)
                               if sim == builder_id and "pos" in parts and "move_intent" not in parts
                               and door and math.hypot(xz(parts["pos"])[0] - door[0],
                                                       xz(parts["pos"])[1] - door[1]) < 2500}
            for sim_id, parts in world.having(VILLAGER):
                if sim_id == played or "pos" not in parts:
                    continue
                rx, rz = xz(parts["pos"])
                near = door and math.hypot(rx - door[0], rz - door[1]) < 9000
                if not hauler and near and "move_intent" in parts and world.bulk(parts):
                    hauler = {"name": parts[VILLAGER]["name"], "at": [rx / 1000.0, rz / 1000.0],
                              "load": world.bulk(parts), "tick": world.tick}
                    flow.frame("homework-hauler-side-9m", rx / 1000.0, rz / 1000.0, 9,
                               yaw=away_from_house(rx, rz, math.pi / 2), settle=0.4)
                    flow.frame("homework-hauler-20m", rx / 1000.0, rz / 1000.0, 20,
                               yaw=away_from_house(rx, rz, math.pi / 3), settle=0.4)
                if builder_id is None and (working_now.get(sim_id) or {}).get("hired"):
                    builder_id = sim_id
                if sim_id != builder_id:
                    continue
                state = ("working" if sim_id in working_now
                         else ("walking" if "move_intent" in parts else "away"))
                if state != last:
                    states.append({"tick": world.tick, "state": state, "at": [rx / 1000.0, rz / 1000.0]})
                    last = state
                work = working_now.get(sim_id) or {}
                if state == "working" and not builder:
                    builder = {"id": sim_id, "name": parts[VILLAGER]["name"], "state": state,
                               "at": [rx / 1000.0, rz / 1000.0], "work": work, "tick": world.tick}
                    flow.frame("homework-builder-9m", rx / 1000.0, rz / 1000.0, 9,
                               yaw=away_from_house(rx, rz, 0.5), running=True, settle=0.6)
                    flow.frame("homework-builder-side-9m", rx / 1000.0, rz / 1000.0, 9,
                               yaw=away_from_house(rx, rz, 1.4), running=True, settle=0.5)
                    flow.frame("homework-builder-20m", rx / 1000.0, rz / 1000.0, 20,
                               yaw=away_from_house(rx, rz, 0.5), running=True, settle=0.6)
                    flow.call("npc/pause")
                    stage_shots.append({"label": "homework-stage-a-14m", "tick": flow.state()["tick"],
                                        "work": work})
                    flow.frame("homework-stage-a-14m", door[0] / 1000.0, door[1] / 1000.0, 14,
                               yaw=math.pi * 0.25, settle=1.0)
                elif (builder and state == "working" and len(stage_shots) == 1
                        and work.get("started") != (builder["work"] or {}).get("started")):
                    # The next stage: the first credited and a new one begun.
                    flow.call("npc/pause")
                    stage_shots.append({"label": "homework-stage-b-14m", "tick": world.tick, "work": work})
                    flow.frame("homework-stage-b-14m", door[0] / 1000.0, door[1] / 1000.0, 14,
                               yaw=math.pi * 0.25, settle=1.0)
                    finished = True
            # Now and then: is the upgrade done (no next stage to wait for)?
            if builder and not finished and world.tick % 1200 < 10 and upgrade() is None:
                stage_shots.append({"label": "homework-stage-b-14m", "tick": world.tick,
                                    "upgrade_finished": True})
                flow.frame("homework-stage-b-14m", door[0] / 1000.0, door[1] / 1000.0, 14,
                           yaw=math.pi * 0.25, settle=1.0)
                finished = True
            if not finished:
                flow.call("npc/step", {"ticks": 10})
        entry["hauler"] = hauler
        entry["builder"] = builder
        entry["builder_id"] = builder_id
        entry["builder_states"] = states[:40]
        entry["stage_shots"] = stage_shots
        entry["ticks_stepped"] = world.tick - start
        shots = run / "shots"
        if len(stage_shots) >= 2:
            entry["stage_change"] = pixel_change(shots / "homework-stage-a-14m-1280x720.png",
                                                 shots / "homework-stage-b-14m-1280x720.png")
        entry["upgrade"] = upgrade()
        if door:
            flow.frame("homework-site-22m", door[0] / 1000.0, door[1] / 1000.0 - 2.5, 22,
                       yaw=math.pi * 0.25, settle=1.0)
    finally:
        leave(child, flow, run, report, "homework")
    return run


def fisher_run(binary, base, report):
    """A new game run to day 3, when fishers have started: a fisher at the
    water with the rod, close and at mid distance."""
    run = base / "fisher"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 5)
    entry = report.setdefault("fisher", {})
    try:
        flow.to_tick(flow.state()["tick"] + int(2.6 * DAY))
        flow.to_phase(0.3)
        found = None
        for attempt in range(48):
            world = flow.world()
            rows = [row for row in workers(world)["fish"] if row["left"] >= 30]
            if rows:
                found = rows[0]
                break
            flow.to_tick(flow.state()["tick"] + DAY // 48)
        entry["found"] = found
        if found:
            boxes = blockers(flow.world())
            x, z = found["at"][0] / 1000.0, found["at"][1] / 1000.0
            yaw, clearance = clear_side(boxes, x, z)
            entry["yaw"] = yaw
            flow.frame("fisher-8m", x, z, 8, yaw=yaw, running=True, settle=1.0)
            flow.frame("fisher-8m-side", x, z, 8, yaw=yaw + math.pi / 2, running=True, settle=0.6)
            flow.frame("fisher-20m", x, z, 20, yaw=yaw, running=True, settle=0.6)
    finally:
        leave(child, flow, run, report, "fisher")
    return run


def larger_run(binary, base, report):
    """The finished larger cottage (fixture `larger-cottage-done`): outside
    and cut away, at 1280x720 and 1024x768."""
    child, flow, run, played = fixture_flow(binary, base, report, "larger-cottage-done", "larger")
    entry = report.setdefault("larger", {})
    old_sizes = capture_player_flow.SIZES
    capture_player_flow.SIZES = ((1280, 720), (1024, 768))
    try:
        world = flow.world()
        home = next(((sim_id, parts) for sim_id, parts in world.having(HOME)
                     if played in parts[HOME].get("residents", [])), None)
        me = world.parts(played)
        entry["home"] = home and {"id": home[0], "capacity": home[1][HOME].get("capacity"),
                                  "blocker": home[1].get("grid_blocker"), "pos": home[1].get("pos")}
        entry["indoors"] = (me.get(INDOORS) or {}).get("building")
        if not home:
            return run
        centre = xz(home[1]["pos"])
        door = xz(home[1][HOME]["door"])
        x, z = centre[0] / 1000.0, centre[1] / 1000.0
        if entry["indoors"] is not None:
            # Inside first (the played character's home is cut away).
            flow.frame("larger-inside-12m", x, z, 12, yaw=math.pi * 0.75, settle=1.5)
            flow.frame("larger-inside-side-12m", x, z, 12, yaw=-math.pi / 2, settle=1.0)
            flow.frame("larger-inside-18m", x, z, 18, yaw=math.pi * 0.25, settle=1.0)
            walk_out(flow, played)
        me = flow.world().parts(played)
        entry["outdoors_after"] = (me.get(INDOORS) or {}).get("building") is None
        # Outside, the camera turned away from the character so nothing
        # takes the cottage out of the way.
        flow.frame("larger-outside-22m", x, z, 22, yaw=math.pi * 0.8, settle=1.5)
        flow.frame("larger-outside-front-16m", door[0] / 1000.0, door[1] / 1000.0, 16, yaw=0.0)
        if entry["indoors"] is None:
            # Cut away by a close Oversee camera on the body.
            flow.click("Oversee")
            time.sleep(0.8)
            # The first-run tips would cover the room.
            for _ in range(6):
                try:
                    flow.click("Got it")
                except RuntimeError:
                    break
            flow.frame("larger-inside-12m", x, z, 12, yaw=math.pi * 0.75, settle=1.5)
            flow.frame("larger-inside-side-12m", x, z, 12, yaw=-math.pi / 2, settle=1.0)
    finally:
        capture_player_flow.SIZES = old_sizes
        leave(child, flow, run, report, "larger")
    return run



# Ambient motion (`--only motion|budget`, target/npc-out/world5/motion/).

def pixel_change(first, second, grid=(3, 3)):
    """Mean absolute pixel change (0..255) of two PNGs, over the frame
    without the HUD bands and per region of a grid (row-major)."""
    from PIL import Image, ImageChops, ImageStat
    a = Image.open(first).convert("RGB")
    b = Image.open(second).convert("RGB")
    width, height = a.size
    top, bottom = 50, height - 70
    diff = ImageChops.difference(a, b)
    mean = lambda box: round(sum(ImageStat.Stat(diff.crop(box)).mean) / 3.0, 3)
    regions = {}
    columns, rows = grid
    for row in range(rows):
        for column in range(columns):
            box = (column * width // columns, top + row * (bottom - top) // rows,
                   (column + 1) * width // columns, top + (row + 1) * (bottom - top) // rows)
            regions[f"r{row}c{column}"] = mean(box)
    return {"whole": mean((0, top, width, bottom)), "regions": regions,
            "changed_share": round(sum(1 for v in regions.values() if v > 0.05) / len(regions), 2)}


def contact_sheet(paths, out):
    """Six frames, half size, three across."""
    from PIL import Image
    frames = [Image.open(path).convert("RGB") for path in paths]
    w, h = frames[0].size[0] // 2, frames[0].size[1] // 2
    sheet = Image.new("RGB", (w * 3, h * ((len(frames) + 2) // 3)))
    for i, frame in enumerate(frames):
        sheet.paste(frame.resize((w, h)), ((i % 3) * w, (i // 3) * h))
    sheet.save(out)


def motion_pair(flow, run, entry, label, x, z, distance, yaw=0.0, running=False, gap=2.0):
    """Two frames `gap` seconds apart from one fixed camera; paused unless
    `running`."""
    flow.look(x, z, distance, label, yaw)
    time.sleep(1.2)
    if running:
        flow.call("npc/resume")
    flow.snap_now(label + "-a")
    time.sleep(gap)
    if running:
        # The camera can be drawn after somebody while the clock runs: put
        # it back exactly before the second frame.
        flow.look(x, z, distance, label, yaw)
    flow.snap_now(label + "-b")
    if running:
        flow.call("npc/pause")
    shots = run / "shots"
    entry[label] = {"camera": [x, z, distance, yaw], "paused": not running, "seconds_apart": gap,
                    "change": pixel_change(shots / f"{label}-a-1280x720.png",
                                           shots / f"{label}-b-1280x720.png")}


def motion_sheet(flow, run, entry, label):
    paths = []
    for i in range(6):
        flow.snap_now(f"{label}-{i}")
        paths.append(run / "shots" / f"{label}-{i}-1280x720.png")
        if i < 5:
            time.sleep(1.0)
    out = run / "shots" / f"{label}-sheet.png"
    contact_sheet(paths, out)
    entry[label] = {"frames": 6, "seconds": 5, "sheet": out.name,
                    "first_to_last": pixel_change(paths[0], paths[-1])}


def motion_run(binary, base, report):
    """Ambient motion from fixed cameras: atmosphere paused, sim-driven
    machines running; contact sheets from Oversee and Live; the title."""
    run = base / "motion"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("motion", {})
    try:
        flow.to_phase(0.3)
        # The boat in: stepped to the merchant's next landing.
        merchant = flow.merchant()
        today = flow.today()
        now = flow.state()["tick"]
        days = int(merchant["next_visit_day"]) - (today - 1)
        flow.to_tick(now - now % DAY + days * DAY + int(0.15 * DAY) - 120)
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            if flow.merchant().get("docked_until"):
                break
        entry["docked_until"] = flow.merchant().get("docked_until")
        entry["berth_log"] = log_lines(run, "merchant boat berth:")[-1:]
        world, row, street_x, street_z = street(flow)
        merchant = flow.merchant()
        post = xz(flow.world().parts(merchant["post"])["pos"]) if merchant.get("post") else None
        px, pz = (post[0] / 1000.0, post[1] / 1000.0) if post else (street_x, street_z)
        flow.call("npc/pause")
        flow.resize((1280, 720))
        motion_pair(flow, run, entry, "motion-clouds-island", street_x, street_z, 150, yaw=0.3)
        motion_pair(flow, run, entry, "motion-trees-14m", px + 6.0, pz + 8.0, 14, yaw=math.pi * 0.8)
        line = entry["berth_log"][0] if entry["berth_log"] else ""
        bx, bz = ([float(v) for v in line.split("berth: ")[1].split(";")[0].split(",")]
                  if "berth: " in line else (px - 6.0, pz))
        entry["moored_at"] = [bx, bz]
        motion_pair(flow, run, entry, "motion-sea-shore-30m", bx, bz, 30, yaw=math.pi / 2)
        motion_pair(flow, run, entry, "motion-boat-14m", bx, bz, 14, yaw=math.pi / 2)
        motion_pair(flow, run, entry, "motion-smoke-20m", street_x, street_z, 20, yaw=math.pi)
        # Machines: a resident at work on a station.
        station = None
        for _ in range(30):
            world = flow.world()
            for sim_id, parts in world.having(VILLAGER):
                craft = parts.get("crafting")
                if craft and craft.get("workstation") is not None:
                    place = world.parts(craft["workstation"])
                    if "pos" in place:
                        station = (craft["workstation"], xz(place["pos"]), place.get(PLACE))
                        break
            if station:
                break
            flow.call("npc/step", {"ticks": 60})
        entry["machine_station"] = station and {"id": station[0], "place": station[2]}
        if station:
            sx, sz = station[1][0] / 1000.0, station[1][1] / 1000.0
            motion_pair(flow, run, entry, "motion-machine-running-16m", sx, sz, 16, yaw=0.6, running=True)
            motion_pair(flow, run, entry, "motion-machine-paused-16m", sx, sz, 16, yaw=0.6)
        flow.look(street_x, street_z, 60, "motion-oversee", 0.5)
        time.sleep(1.0)
        motion_sheet(flow, run, entry, "motion-oversee")
        enter_live(flow)
        flow.call("npc/pause")
        time.sleep(1.0)
        motion_sheet(flow, run, entry, "motion-live")
        # The title's backdrop.
        try:
            flow.press("Menu")
            flow.click("Save and go to title")
            time.sleep(2.0)
            motion_sheet(flow, run, entry, "motion-title")
        except RuntimeError as error:
            entry["title"] = str(error)
    finally:
        leave(child, flow, run, report, "motion")
    return run


def budget_run(binary, base, report, kind="clear"):
    """Frame time at island zoom and close zoom, paused, 1280x720: at dawn,
    noon and night in `kind` weather (only noon in a storm)."""
    run = base / f"budget-{kind}"
    child, flow = enter(binary, run, rules_with_weather(base, kind), report, {}, 3)
    entry = report.setdefault("budget", {})
    try:
        world, row, street_x, street_z = street(flow)
        phases = {"dawn": 0.03, "noon": 0.35, "night": 0.85} if kind == "clear" else {"noon": 0.35}
        for name, phase in phases.items():
            flow.to_phase(phase)
            flow.call("npc/pause")
            # The window can be slow to settle on a busy machine: try again.
            for attempt in range(4):
                try:
                    flow.resize((1280, 720))
                    break
                except RuntimeError:
                    time.sleep(2.0)
            for zoom, distance in (("island-150m", 150), ("close-14m", 14)):
                label = f"{kind}-{name}-{zoom}"
                flow.look(street_x, street_z, distance, label, 0.3)
                time.sleep(2.0)
                flow.perf(label, seconds=6.0, match=("island_world", "island_homes", "island_poses",
                                                     "living_scene", "island_atmosphere", "village_world"))
                entry[label] = report.get("perf", {}).get(label)
    finally:
        leave(child, flow, run, report, f"budget-{kind}")
    return run



# Sixth pass (`--only look6`, target/npc-out/world6/): the ship arriving,
# moored and leaving, deer, woods, water and cloud shadows, from cameras
# that do not depend on the build, so two builds give same-camera pairs.

WILDLIFE = "island_wildlife"


def look6_run(binary, base, report):
    run = base / "look6"
    season = os.environ.get("ISLAND_VIEW_SEASON", "")
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("look6", {"view_season": season or None})
    try:
        flow.resize((1280, 720))
        flow.to_phase(0.3)
        merchant = flow.merchant()
        today = flow.today()
        now = flow.state()["tick"]
        days = int(merchant["next_visit_day"]) - (today - 1)
        landing = now - now % DAY + days * DAY + int(0.15 * DAY)
        post = xz(flow.world().parts(merchant["post"])["pos"]) if merchant.get("post") else None
        px, pz = (post[0] / 1000.0, post[1] / 1000.0) if post else (0.0, 0.0)
        # Arriving: some 250 ticks out.
        flow.to_tick(landing - 260)
        flow.call("npc/pause")
        flow.look(px - 12.0, pz - 14.0, 60, "ship-arriving-60m", math.pi / 2)
        time.sleep(1.5)
        flow.snap_now("ship-arriving-60m")
        docked = None
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            docked = flow.merchant().get("docked_until")
            if docked:
                break
        entry["docked_until"] = docked
        line = (log_lines(run, "merchant boat berth:") or [""])[-1]
        bx, bz = ([float(v) for v in line.split("berth: ")[1].split(";")[0].split(",")]
                  if "berth: " in line else (px - 6.0, pz))
        entry["moored_at"] = [bx, bz]
        flow.call("npc/pause")
        for label, distance, yaw in (("ship-moored-24m", 24, math.pi / 2),
                                     ("ship-moored-14m", 14, math.pi * 0.7),
                                     ("ship-moored-40m", 40, math.pi * 0.35)):
            flow.look(bx, bz, distance, label, yaw)
            time.sleep(1.5)
            flow.snap_now(label)
        motion_pair(flow, run, entry, "motion6-ship-14m", bx, bz, 14, yaw=math.pi * 0.7, gap=3.0)
        motion_pair(flow, run, entry, "motion6-shore-20m", (bx + px) / 2, (bz + pz) / 2, 20,
                    yaw=math.pi / 2, gap=3.0)
        motion_pair(flow, run, entry, "motion6-sea-40m", bx - 10.0, bz - 10.0, 40, yaw=math.pi / 2, gap=3.0)
        world, row, street_x, street_z = street(flow)
        motion_pair(flow, run, entry, "motion6-clouds-island", street_x, street_z, 150, yaw=0.3, gap=3.0)
        motion_pair(flow, run, entry, "motion6-clouds-80m", street_x, street_z, 80, yaw=0.3, gap=3.0)
        motion_pair(flow, run, entry, "motion6-trees-20m", px + 6.0, pz + 10.0, 20, yaw=math.pi * 0.8,
                    gap=2.0)
        motion_pair(flow, run, entry, "motion6-trees-40m", px + 6.0, pz + 14.0, 40, yaw=math.pi * 0.8,
                    gap=2.0)
        motion_pair(flow, run, entry, "motion6-smoke-25m", street_x, street_z, 25, yaw=math.pi, gap=2.0)
        # Deer.
        deer = [(sim_id, xz(parts["pos"])) for sim_id, parts in flow.world().having(WILDLIFE)
                if "pos" in parts]
        entry["deer"] = [[sim_id, at] for sim_id, at in deer[:4]]
        for i, (sim_id, at) in enumerate(deer[:2]):
            x, z = at[0] / 1000.0, at[1] / 1000.0
            for label, distance, yaw in ((f"deer{i}-7m", 7, 0.4), (f"deer{i}-14m", 14, 2.4)):
                flow.look(x, z, distance, label, yaw)
                time.sleep(1.2)
                flow.call("npc/resume")
                time.sleep(1.0)
                flow.call("npc/pause")
                flow.snap_now(label)
        # Leaving: after the boat's stay.
        if docked:
            flow.to_tick(int(docked) + 220)
            flow.call("npc/pause")
            flow.look(bx - 10.0, bz - 10.0, 60, "ship-leaving-60m", math.pi / 2)
            time.sleep(1.5)
            flow.snap_now("ship-leaving-60m")
        flow.look(street_x, street_z, 150, "island-150m", 0.3)
        time.sleep(1.0)
        motion_sheet(flow, run, entry, "sheet6-oversee-island")
        # Small life in its season (the view's season pinned for review):
        # butterflies and leaves by day close up, fireflies at dusk.
        if season in ("summer", "autumn"):
            motion_pair(flow, run, entry, f"motion6-{season}-life-14m", px + 6.0, pz + 10.0, 14,
                        yaw=math.pi * 0.8, gap=1.0)
            flow.look(px + 6.0, pz + 10.0, 14, "life-sheet", math.pi * 0.8)
            time.sleep(1.0)
            motion_sheet(flow, run, entry, f"sheet6-{season}-life-14m")
        if season == "summer":
            flow.to_phase(0.8)
            flow.call("npc/pause")
            motion_pair(flow, run, entry, "motion6-summer-dusk-14m", px + 6.0, pz + 10.0, 14,
                        yaw=math.pi * 0.8, gap=1.0)
    finally:
        leave(child, flow, run, report, "look6")
    return run



# Seventh pass (`--only village7`): the village centre at noon, dusk and
# night, a farm, smoke from homes the simulation heats, and a worked mill.

def village7_run(binary, base, report):
    run = base / "village7"
    season = os.environ.get("ISLAND_VIEW_SEASON", "")
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 6)
    entry = report.setdefault("village7", {"view_season": season or None})
    tag = season or "calendar"
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        world, row, street_x, street_z = street(flow)
        entry["street"] = [street_x, street_z]
        for name, phase in (("noon", 0.35), ("dusk", 0.68), ("night", 0.85)):
            flow.to_phase(phase)
            flow.call("npc/pause")
            for label, distance, yaw in ((f"village-{tag}-{name}-30m", 30, 0.8),
                                         (f"village-{tag}-{name}-70m", 70, 0.5)):
                flow.look(street_x, street_z, distance, label, yaw)
                time.sleep(1.5)
                flow.snap_now(label)
        flow.to_phase(0.35)
        flow.call("npc/pause")
        # A farm: the first field with a plot.
        fields = [field for field in flow.fields() if field.get("plot")]
        entry["field"] = fields[0] if fields else None
        if fields:
            plot = fields[0]["plot"]
            fx = (plot["lo_x"] + plot["hi_x"] + 1) / 2.0
            fz = (plot["lo_z"] + plot["hi_z"] + 1) / 2.0
            for label, distance in ((f"farm-{tag}-20m", 20), (f"farm-{tag}-40m", 40)):
                flow.look(fx, fz, distance, label, math.pi * 0.8)
                time.sleep(1.5)
                flow.snap_now(label)
            motion_pair(flow, run, entry, f"motion7-crops-{tag}-16m", fx, fz, 16, yaw=math.pi * 0.8, gap=2.0)
        # Smoke from homes the simulation reports heated.
        world = flow.world()
        heated = [(sim_id, parts) for sim_id, parts in world.having(HOME)
                  if (parts[HOME].get("heated_until") or 0) > world.tick and "door" in parts[HOME]]
        entry["heated_homes"] = [sim_id for sim_id, _ in heated][:8]
        if heated:
            door = xz(heated[0][1][HOME]["door"])
            motion_pair(flow, run, entry, f"motion7-smoke-heated-{tag}-20m", door[0] / 1000.0,
                        door[1] / 1000.0, 20, yaw=math.pi, gap=2.0)
        # A worked mill: step until someone crafts at a mill or a sawmill.
        found = None
        for _ in range(60):
            world = flow.world()
            for sim_id, parts in world.having(VILLAGER):
                craft = parts.get("crafting")
                if not craft or craft.get("workstation") is None:
                    continue
                place = world.parts(craft["workstation"])
                role = str((place.get(PLACE) or {}).get("role", "")).lower()
                if role in ("miller", "woodcutter", "carpenter") and "pos" in place:
                    found = (craft["workstation"], xz(place["pos"]), place.get(PLACE))
                    break
            if found:
                break
            flow.call("npc/step", {"ticks": 120})
        entry["machine"] = found and {"id": found[0], "place": found[2]}
        if found:
            mx, mz = found[1][0] / 1000.0, found[1][1] / 1000.0
            motion_pair(flow, run, entry, "motion7-mill-running-18m", mx, mz, 18, yaw=0.6, running=True)
            motion_pair(flow, run, entry, "motion7-mill-paused-18m", mx, mz, 18, yaw=0.6)
    finally:
        leave(child, flow, run, report, "village7")
    return run


# Eighth pass (`--only look8`, target/npc-out/world8/): the hull's colour on
# screen, same-camera views for a before/after pair, the deer, the grain
# mill's wheel close up.

def fence_sample(path, box=(440, 250, 840, 470), lean=35):
    """Warm pixels of a field fence filling the middle of a frame: the
    sunlit (brightest tenth) and shaded means in 8-bit sRGB, and whether the
    sunlit timber is a mid brown (R 150 to 175, R - B 50 to 80)."""
    from PIL import Image
    image = Image.open(path).convert("RGB").crop(box)
    # Timber leans red; the pale plot and the snow do not.
    warm = [(r, g, b) for r, g, b in image.getdata() if r - b >= lean and r >= g and r > 25]
    if len(warm) < 200:
        return {"warm_pixels": len(warm), "ok": None}
    warm.sort(key=lambda c: 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2])
    tenth = max(1, len(warm) // 10)
    mean = lambda part: [round(sum(c[i] for c in part) / len(part), 1) for i in range(3)]
    sunlit, shaded = mean(warm[-tenth:]), mean(warm[:tenth])
    return {"box": list(box), "warm_pixels": len(warm), "sunlit": sunlit, "shaded": shaded,
            "median": list(warm[len(warm) // 2]),
            "ok": 150 <= sunlit[0] <= 175 and 50 <= sunlit[0] - sunlit[2] <= 80}


def hull_sample(path, box=(560, 250, 780, 480)):
    """Warm pixels (wood, not water, sail or sky) in the middle of a frame
    centred on the ship (the box keeps the pier's deck out): the sunlit (brightest tenth) and shaded (darkest
    tenth) means in 8-bit sRGB, and whether the sunlit hull meets the
    pass-8 limit (R <= 150, R - B <= 75)."""
    from PIL import Image
    image = Image.open(path).convert("RGB").crop(box)
    # Warm and saturated enough to be wood: the edges of the white sail and
    # a gull passing over the hull blend to pale warm greys, left out.
    warm = [(r, g, b) for r, g, b in image.getdata()
            if r > b + 12 and r >= g and r > 25 and (r - b) >= 0.15 * r]
    if len(warm) < 200:
        return {"warm_pixels": len(warm), "ok": None}
    warm.sort(key=lambda c: 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2])
    tenth = max(1, len(warm) // 10)
    mean = lambda part: [round(sum(c[i] for c in part) / len(part), 1) for i in range(3)]
    sunlit, shaded = mean(warm[-tenth:]), mean(warm[:tenth])
    return {"box": list(box), "warm_pixels": len(warm), "sunlit": sunlit, "shaded": shaded,
            "median": list(warm[len(warm) // 2]),
            "ok": sunlit[0] <= 150 and sunlit[0] - sunlit[2] <= 75 and sum(shaded) < sum(sunlit) * 0.8}


def white_share(path, box=(320, 120, 960, 600)):
    from PIL import Image
    image = Image.open(path).convert("RGB").crop(box)
    pixels = list(image.getdata())
    return sum(1 for r, g, b in pixels if min(r, g, b) > 200) / len(pixels)


def translation_of(value):
    """The translation of a reflected GlobalTransform, whatever its shape."""
    if isinstance(value, dict):
        for key in ("translation", "w_axis"):
            if key in value:
                return translation_of(value[key])
        numbers = []
        for item in value.values():
            numbers.extend(flatten(item))
        return numbers[-3:] if len(numbers) >= 3 else None
    numbers = flatten(value)
    return numbers[-3:] if len(numbers) >= 3 else None


def flatten(value):
    if isinstance(value, (int, float)):
        return [float(value)]
    if isinstance(value, dict):
        out = []
        for key in sorted(value, key=lambda k: {"x": 0, "y": 1, "z": 2, "w": 3}.get(k, 9)):
            out.extend(flatten(value[key]))
        return out
    if isinstance(value, list):
        out = []
        for item in value:
            out.extend(flatten(item))
        return out
    return []


def mill_hub(flow, x, z, part="windmill_top_fan"):
    name = "bevy_ecs::name::Name"
    transform = "bevy_transform::components::global_transform::GlobalTransform"
    try:
        rows = query(flow.url, [name, transform])
    except Exception as error:
        return {"error": str(error)}
    best = None
    for row in rows:
        label = row["components"].get(name)
        label = label.get("name", "") if isinstance(label, dict) else str(label)
        if part not in label:
            continue
        at = translation_of(row["components"].get(transform))
        if not at:
            continue
        reach = math.hypot(at[0] - x, at[2] - z)
        if best is None or reach < best[0]:
            best = (reach, at)
    return best and [round(v, 2) for v in best[1]]


def hub_look(flow, hub, distance, yaw, pitch):
    entity, _ = flow.camera_entity()
    for path, value in (("focus", [float(hub[0]), float(hub[1]), float(hub[2])]),
                        ("distance", float(distance)), ("pitch", float(pitch)), ("yaw", float(yaw))):
        flow.call("world.mutate_components", {"entity": entity, "component": CAMERA,
                                              "path": path, "value": value})
    time.sleep(0.6)


def look8_run(binary, base, report):
    run = base / "look8"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("look8", {})
    shots = run / "shots"
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        flow.to_phase(0.3)
        merchant = flow.merchant()
        today = flow.today()
        now = flow.state()["tick"]
        days = int(merchant["next_visit_day"]) - (today - 1)
        landing = now - now % DAY + days * DAY + int(0.15 * DAY)
        post = xz(flow.world().parts(merchant["post"])["pos"]) if merchant.get("post") else None
        px, pz = (post[0] / 1000.0, post[1] / 1000.0) if post else (0.0, 0.0)
        flow.to_tick(landing - 260)
        docked = None
        for _ in range(20):
            flow.to_tick(flow.state()["tick"] + 300)
            docked = flow.merchant().get("docked_until")
            if docked:
                break
        entry["docked_until"] = docked
        line = (log_lines(run, "merchant boat berth:") or [""])[-1]
        entry["berth_log"] = line
        bx, bz = ([float(v) for v in line.split("berth: ")[1].split(";")[0].split(",")]
                  if "berth: " in line else (px - 6.0, pz))
        entry["moored_at"] = [bx, bz]
        # Noon for the colour check.
        flow.to_phase(0.35)
        flow.call("npc/pause")
        cameras = [("harbour-24m", bx, bz, 24, math.pi / 2),
                   ("harbour-pier-40m", bx, bz, 40, math.pi * 0.35),
                   ("harbour-pier-14m", bx, bz, 14, math.pi * 0.7),
                   ("woods-40m", px + 6.0, pz + 14.0, 40, math.pi * 0.8)]
        world, row, street_x, street_z = street(flow)
        entry["street"] = [street_x, street_z]
        cameras.append(("village-30m", street_x, street_z, 30, 0.8))
        fields = [field for field in flow.fields() if field.get("plot")]
        if fields:
            plot = fields[0]["plot"]
            cameras.append(("farm-20m", (plot["lo_x"] + plot["hi_x"] + 1) / 2.0,
                            (plot["lo_z"] + plot["hi_z"] + 1) / 2.0, 20, math.pi * 0.8))
        # Same cameras as another run (its summary), for a before/after pair
        # when the two builds moor the ship a little differently.
        same = os.environ.get("LOOK8_CAMERAS")
        if same:
            fixed = json.loads(Path(same).read_text(encoding="utf-8"))["look8"]["cameras"]
            cameras = [(label, *fixed[label]) for label, *_ in cameras if label in fixed]
        entry["cameras"] = {label: [x, z, d, yaw] for label, x, z, d, yaw in cameras}
        for label, x, z, distance, yaw in cameras:
            flow.look(x, z, distance, label, yaw)
            time.sleep(1.5)
            flow.snap_now(label)
        # The field fence close up: the camera on its near side.
        if fields:
            plot = fields[0]["plot"]
            fx, fz = plot["lo_x"] + 0.0, (plot["lo_z"] + plot["hi_z"] + 1) / 2.0
            for label, distance, yaw in (("fence-7m", 7, math.pi * 0.8), ("fence-7m-side", 7, math.pi * 0.3)):
                flow.look(fx, fz, distance, label, yaw)
                time.sleep(1.5)
                flow.snap_now(label)
                entry[f"fence_{label}"] = fence_sample(shots / f"{label}-1280x720.png")
            report["checks"].append({"check": "sunlit fence R 150-175, R - B 50-80",
                                     "value": entry["fence_fence-7m"], "ok": entry["fence_fence-7m"].get("ok")})
        entry["hull_noon_24m"] = hull_sample(shots / "harbour-24m-1280x720.png")
        entry["hull_noon_14m"] = hull_sample(shots / "harbour-pier-14m-1280x720.png")
        report["checks"].append({"check": "sunlit hull R <= 150 and R - B <= 75, shaded darker",
                                 "value": entry["hull_noon_24m"], "ok": entry["hull_noon_24m"].get("ok")})
        # Deer.
        deer = [(sim_id, xz(parts["pos"])) for sim_id, parts in flow.world().having(WILDLIFE)
                if "pos" in parts]
        entry["deer"] = [[sim_id, at] for sim_id, at in deer[:4]]
        for i, (sim_id, at) in enumerate(deer[:2]):
            x, z = at[0] / 1000.0, at[1] / 1000.0
            for label, distance, yaw in ((f"deer{i}-7m", 7, 0.4), (f"deer{i}-14m", 14, 2.4)):
                flow.look(x, z, distance, label, yaw)
                time.sleep(1.5)
                flow.snap_now(label)
        # The grain mill: a miller at work, its wheel close up.
        found = None
        for _ in range(80):
            world = flow.world()
            for sim_id, parts in world.having(VILLAGER):
                craft = parts.get("crafting")
                if not craft or craft.get("workstation") is None:
                    continue
                place = world.parts(craft["workstation"])
                role = str((place.get(PLACE) or {}).get("role", "")).lower()
                if role == "miller" and "pos" in place:
                    found = (craft["workstation"], xz(place["pos"]), place.get(PLACE))
                    break
            if found:
                break
            flow.call("npc/step", {"ticks": 120})
        entry["mill"] = found and {"id": found[0], "place": found[2], "pos": found[1]}
        if found:
            mx, mz = found[1][0] / 1000.0, found[1][1] / 1000.0
            # The grain mill is drawn as a windmill (`living_scene::building`
            # for the miller): its turning wheel is the sail rotor
            # (`windmill_top_fan`). Find the rotor nearest the station and
            # frame its hub from the side.
            hub = mill_hub(flow, mx, mz)
            entry["mill_hub"] = hub
            if hub:
                trials = {}
                for yaw in (0.0, math.pi / 2, math.pi, -math.pi / 2):
                    label = f"mill-wheel-yaw{yaw:+.1f}"
                    hub_look(flow, hub, 11.0, yaw, 0.3)
                    time.sleep(1.2)
                    flow.snap_now(label)
                    trials[yaw] = label
                entry["mill_views"] = list(trials.values())
                yaw = float(os.environ.get("LOOK8_MILL_YAW", "nan"))
                if math.isnan(yaw):
                    # The side the sails face: the view with the most sail
                    # (white) in its middle.
                    yaw = max(trials, key=lambda y: white_share(shots / f"{trials[y]}-1280x720.png"))
                entry["mill_yaw"] = yaw
                near = float(os.environ.get("LOOK8_MILL_M", "8"))
                entry["mill_distance"] = near
                for running, label in ((True, "mill-wheel-running"), (False, "mill-wheel-paused")):
                    hub_look(flow, hub, near, yaw, 0.3)
                    time.sleep(1.2)
                    if running:
                        flow.call("npc/resume")
                    flow.snap_now(label + "-a")
                    time.sleep(2.0)
                    if running:
                        hub_look(flow, hub, near, yaw, 0.3)
                    flow.snap_now(label + "-b")
                    if running:
                        flow.call("npc/pause")
                    entry[label] = {"hub": hub, "yaw": yaw, "paused": not running, "seconds_apart": 2.0,
                                    "change": pixel_change(shots / f"{label}-a-1280x720.png",
                                                           shots / f"{label}-b-1280x720.png")}
    finally:
        leave(child, flow, run, report, "look8")
    return run


# Tenth pass (`--only look10`, target/npc-out/world10/): the herd's spacing
# measured on the drawn models, and the field gate close up.

def drawn_herd(flow, x, z, reach=12.0):
    """The drawn deer and stag models near (x, z): their names and ground
    positions, and the least distance between any two (overall and between
    two stags)."""
    name = "bevy_ecs::name::Name"
    transform = "bevy_transform::components::global_transform::GlobalTransform"
    rows = query(flow.url, [name, transform])
    animals = []
    for row in rows:
        label = row["components"].get(name)
        label = label.get("name", "") if isinstance(label, dict) else str(label)
        if label not in ("deer model", "stag model"):
            continue
        at = translation_of(row["components"].get(transform))
        if at and math.hypot(at[0] - x, at[2] - z) < reach:
            animals.append((label, round(at[0], 2), round(at[2], 2)))
    pairs = [(math.hypot(a[1] - b[1], a[2] - b[2]), a[0], b[0])
             for i, a in enumerate(animals) for b in animals[i + 1:]]
    least = min(pairs) if pairs else None
    stags = [d for d, a, b in pairs if a == b == "stag model"]
    return {"animals": animals, "least": least and round(least[0], 2),
            "least_pair": least and [least[1], least[2]],
            "least_stags": round(min(stags), 2) if stags else None}


def look10_run(binary, base, report):
    run = base / "look10"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("look10", {})
    shots = run / "shots"
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        flow.to_phase(0.35)
        flow.call("npc/pause")
        deer = [(sim_id, xz(parts["pos"])) for sim_id, parts in flow.world().having(WILDLIFE)
                if "pos" in parts]
        entry["deer"] = [[sim_id, at] for sim_id, at in deer[:8]]
        if deer:
            x, z = deer[0][1][0] / 1000.0, deer[0][1][1] / 1000.0
            # Twice: the herd is spaced once a second, after it is drawn.
            time.sleep(2.5)
            entry["herd"] = drawn_herd(flow, x, z)
            for label, distance, yaw in (("deer0-7m", 7, 0.4), ("deer0-14m", 14, 2.4)):
                flow.look(x, z, distance, label, yaw)
                time.sleep(1.5)
                flow.snap_now(label)
        fields = [field for field in flow.fields() if field.get("plot")]
        if fields:
            plot = fields[0]["plot"]
            fx, fz = plot["lo_x"] + 0.0, (plot["lo_z"] + plot["hi_z"] + 1) / 2.0
            for label, distance, yaw in (("fence-7m", 7, math.pi * 0.8), ("fence-7m-side", 7, math.pi * 0.3)):
                flow.look(fx, fz, distance, label, yaw)
                time.sleep(1.5)
                flow.snap_now(label)
                entry[f"fence_{label}"] = fence_sample(shots / f"{label}-1280x720.png")
            # The gate stands at the left of `fence-7m`: its pale greys.
            entry["gate_greys"] = grey_share(shots / "fence-7m-1280x720.png", (0, 180, 300, 420))
    finally:
        leave(child, flow, run, report, "look10")
    return run


def grey_share(path, box):
    """The share of pale neutral grey pixels (spread under 18, R over 140)
    in a box: the gate's crossbars, before they took the timber."""
    from PIL import Image
    pixels = list(Image.open(path).convert("RGB").crop(box).getdata())
    grey = [p for p in pixels if max(p) - min(p) < 18 and p[0] > 140 and p[0] < 235]
    return {"box": list(box), "grey_share": round(len(grey) / len(pixels), 4)}


# Eleventh pass (`--only deer11`, target/npc-out/world11/): the deer moving
# as the simulation moves them: a herd grazing and walking, a flight, a
# dead animal if a hunt ends in one; and the frame cost of the herds.

MOTION_TYPE = "island_content::village::wildlife::WildlifeMotion"
SIM_ID_TYPE = "npc_sim::ids::SimId"


def deer_states(flow):
    """Each animal's motion from the ECS by type (state, target, speed),
    by SimId; and from the operator read when that is not available."""
    out = {}
    try:
        for row in query(flow.url, [MOTION_TYPE, SIM_ID_TYPE]):
            sim = row["components"].get(SIM_ID_TYPE)
            sim = sim[0] if isinstance(sim, list) else sim
            out[int(sim)] = row["components"].get(MOTION_TYPE)
    except Exception as error:
        out["error"] = str(error)
    return out


def herd_read(flow, ids):
    """Sim position, motion and vitality of the animals `ids`."""
    world = flow.world()
    motion = deer_states(flow)
    rows = {}
    for sim_id in ids:
        parts = world.parts(sim_id)
        if not parts:
            rows[sim_id] = {"gone": True}
            continue
        vital = next((value for key, value in parts.items() if "vital" in key), None)
        move = motion.get(sim_id) or parts.get("regional_wildlife_motion") or {}
        state = move.get("state") if isinstance(move, dict) else move
        rows[sim_id] = {"pos": [round(v / 1000.0, 3) for v in xz(parts["pos"])] if "pos" in parts else None,
                        "state": state, "speed_mm_per_tick": (move or {}).get("speed_mm_per_tick"),
                        "target": (move or {}).get("target"), "vitality": vital}
    return {"tick": world.tick, "animals": rows}


def moved_between(first, second):
    out = {}
    for sim_id, a in first["animals"].items():
        b = second["animals"].get(sim_id, {})
        if a.get("pos") and b.get("pos"):
            out[sim_id] = round(math.hypot(b["pos"][0] - a["pos"][0], b["pos"][1] - a["pos"][1]), 3)
    return out


def herds(world):
    """The animals grouped by nearness (within 12 m of the group's first)."""
    animals = [(sim_id, xz(parts["pos"])) for sim_id, parts in world.having(WILDLIFE) if "pos" in parts]
    groups = []
    for sim_id, at in animals:
        for group in groups:
            if math.hypot(at[0] - group["at"][0], at[1] - group["at"][1]) < 12000:
                group["ids"].append(sim_id)
                break
        else:
            groups.append({"at": at, "ids": [sim_id]})
    return groups


def centre_of(read):
    points = [row["pos"] for row in read["animals"].values() if row.get("pos")]
    if not points:
        return None
    return (sum(p[0] for p in points) / len(points), sum(p[1] for p in points) / len(points))


def deer11_run(binary, base, report):
    run = base / "deer11"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("deer11", {})
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        flow.to_phase(0.3)
        flow.call("npc/pause")
        world = flow.world()
        groups = herds(world)
        entry["herds"] = [{"at": g["at"], "ids": g["ids"]} for g in groups]
        sample = groups[0]["ids"][0] if groups else None
        entry["deer_parts_keys"] = sorted(world.parts(sample)) if sample else None
        entry["motion_read"] = {k: v for k, v in list(deer_states(flow).items())[:3]}
        if not groups:
            raise RuntimeError("no herd on the island")
        herd = max(groups, key=lambda g: len(g["ids"]))
        ids = herd["ids"]
        # Grazing and walking: one camera, two shots 3 s apart with the
        # clock running; per-animal positions before and after.
        first = herd_read(flow, ids)
        cx, cz = centre_of(first)
        flow.look(cx, cz, 16, "herd-a", 0.6)
        time.sleep(1.5)
        flow.snap_now("herd-16m-a")
        flow.call("npc/resume")
        time.sleep(3.0)
        flow.call("npc/pause")
        flow.look(cx, cz, 16, "herd-b", 0.6)
        time.sleep(0.6)
        flow.snap_now("herd-16m-b")
        second = herd_read(flow, ids)
        entry["herd_pair"] = {"camera": [cx, cz, 16, 0.6], "first": first, "second": second,
                              "moved_m": moved_between(first, second),
                              "drawn_first_to_second": None}
        entry["herd_pair"]["drawn"] = drawn_herd(flow, cx, cz, 20.0)
        # A flight: step the clock and watch every herd for a resident
        # close by and then a fleeing animal.
        all_ids = [i for g in groups for i in g["ids"]]
        before = during = after = None
        dead_seen = None
        start = flow.state()["tick"]
        while flow.state()["tick"] - start < DAY and not after:
            read = herd_read(flow, all_ids)
            states = {i: r.get("state") for i, r in read["animals"].items()}
            fleeing = [i for i, s in states.items() if str(s).lower() == "fleeing"]
            dead = [i for i, r in read["animals"].items()
                    if isinstance(r.get("vitality"), dict) and (r["vitality"].get("died_at") is not None
                                                                or r["vitality"].get("health", 1) <= 0)]
            if dead and not dead_seen:
                at = read["animals"][dead[0]]["pos"]
                dead_seen = {"id": dead[0], "tick": read["tick"], "at": at}
                flow.look(at[0], at[1], 8, "dead", 0.6)
                time.sleep(1.2)
                flow.snap_now("deer-dead-8m")
            if not before:
                residents = [xz(parts["pos"]) for _, parts in flow.world().having(VILLAGER) if "pos" in parts]
                for group in groups:
                    near = [r for r in residents
                            if math.hypot(r[0] - group["at"][0], r[1] - group["at"][1]) < 16000]
                    if near:
                        g = herd_read(flow, group["ids"])
                        c = centre_of(g)
                        if c:
                            before = {"tick": g["tick"], "herd": group["ids"], "camera": [c[0], c[1], 24, 0.6],
                                      "read": g, "resident_at": [near[0][0] / 1000.0, near[0][1] / 1000.0]}
                            flow.look(c[0], c[1], 24, "flee-before", 0.6)
                            time.sleep(1.2)
                            flow.snap_now("flee-before-24m")
                        break
            if fleeing and not during:
                group = next((g for g in groups if fleeing[0] in g["ids"]), {"ids": fleeing})
                g = herd_read(flow, group["ids"])
                cam = before["camera"] if before and fleeing[0] in before["herd"] else None
                c = (cam[0], cam[1]) if cam else centre_of(g)
                during = {"tick": g["tick"], "herd": group["ids"], "camera": [c[0], c[1], 24, 0.6], "read": g}
                flow.look(c[0], c[1], 24, "flee-during", 0.6)
                flow.call("npc/resume")
                time.sleep(0.8)
                flow.call("npc/pause")
                flow.snap_now("flee-during-24m")
                flow.look(c[0], c[1], 40, "flee-during-40m", 0.6)
                time.sleep(0.6)
                flow.snap_now("flee-during-40m")
                after_at = g["tick"] + 400
            if during and not after and flow.state()["tick"] >= after_at:
                g = herd_read(flow, during["herd"])
                c = during["camera"]
                after = {"tick": g["tick"], "read": g, "moved_m": moved_between(during["read"], g)}
                flow.look(c[0], c[1], 24, "flee-after", 0.6)
                time.sleep(1.2)
                flow.snap_now("flee-after-24m")
                flow.look(c[0], c[1], 40, "flee-after-40m", 0.6)
                time.sleep(0.6)
                flow.snap_now("flee-after-40m")
                break
            flow.call("npc/step", {"ticks": 40})
        entry["flee"] = {"before": before, "during": during, "after": after}
        entry["dead"] = dead_seen or "no animal died in the run"
        # Frame cost in a herd view, the herd animated.
        flow.look(cx, cz, 20, "herd-cost", 0.6)
        time.sleep(1.0)
        flow.call("npc/resume")
        time.sleep(2.0)
        flow.perf("herd-20m-running", seconds=6.0,
                  match=("island_world", "island_homes", "island_poses", "living_scene",
                         "island_atmosphere", "village_world"))
        flow.call("npc/pause")
        entry["perf"] = report.get("perf", {}).get("herd-20m-running")
        entry["ambient_off"] = os.environ.get("ISLAND_VIEW_NO_AMBIENT")
    finally:
        leave(child, flow, run, report, "deer11")
    return run


# Sound (`--only sound11`): the island heard from the coast, the woods and
# the village by day and by night, with the clock running; the native log's
# `island_audio:` lines counted (a layer started, a one-shot, music, a UI
# sound). Needs the sound on: not muted, ISLAND_AUDIO_OFF unset.

def sound_lines(log_path):
    """Counts of the `island_audio:` lines in a native log."""
    counts = {"layers": {}, "one_shots": {}, "music": [], "ui": 0, "errors": []}
    try:
        text = log_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return counts
    for line in text.splitlines():
        if "island_audio:" in line:
            tail = line.split("island_audio:", 1)[1].strip()
            if tail.startswith("layer "):
                name = tail.split()[1]
                counts["layers"][name] = counts["layers"].get(name, 0) + 1
            elif tail.startswith("one-shot "):
                name = tail.split()[1]
                counts["one_shots"][name] = counts["one_shots"].get(name, 0) + 1
            elif tail.startswith("music "):
                counts["music"].append(tail)
            elif tail.startswith("ui "):
                counts["ui"] += 1
        lowered = line.lower()
        if ("audio" in lowered or ".ogg" in lowered) and ("error" in lowered or "panick" in lowered):
            counts["errors"].append(line[-200:])
    return counts


def sound11_run(binary, base, report):
    run = base / "sound11"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 8)
    entry = report.setdefault("sound11", {})
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        merchant = flow.merchant()
        post = xz(flow.world().parts(merchant["post"])["pos"]) if merchant.get("post") else None
        px, pz = (post[0] / 1000.0, post[1] / 1000.0) if post else (0.0, 0.0)
        world, row, street_x, street_z = street(flow)
        spots = [("coast", px - 6.0, pz, 18), ("woods", px + 6.0, pz + 14.0, 20),
                 ("village", street_x, street_z, 22), ("oversee", street_x, street_z, 150)]
        for phase_name, phase in (("day", 0.35), ("night", 0.85)):
            flow.to_phase(phase)
            for label, x, z, distance in spots:
                flow.look(x, z, distance, f"sound-{phase_name}-{label}", 0.6)
                flow.call("npc/resume")
                time.sleep(6.0)
                flow.call("npc/pause")
                flow.snap_now(f"sound-{phase_name}-{label}")
                entry[f"{phase_name}-{label}"] = sound_lines(flow.log_path)
        entry["log"] = sound_lines(flow.log_path)
    finally:
        leave(child, flow, run, report, "sound11")
    return run


# Shape placements (`--only shape12`): one of each shipped prop, a field and
# a garden placed by recorded commands near the village street, shot from
# close and from above; then one prop removed and the spot shot again.

SHIPPED_PROPS = ["barrel", "crate_stack", "fence", "flower_bed", "signpost", "flag", "wheelbarrow"]


def place_near(flow, command, x0, z0, tries=40):
    """Place `command` at the first free cell in a spiral round (x0, z0);
    the receipt and the cell, or the last refusal."""
    last = None
    for step in range(tries):
        ring = 1 + step // 8
        angle = (step % 8) * math.pi / 4
        x = round(x0 + ring * 2 * math.cos(angle))
        z = round(z0 + ring * 2 * math.sin(angle))
        try:
            receipt = flow.call("npc/command", {"cmd": {**command, "x": x, "z": z}})
            flow.call("npc/step", {"ticks": 2})
            return {"cell": [x, z], "receipt": receipt}
        except RuntimeError as error:
            last = str(error)[:200]
    return {"refused": last}


def shape12_run(binary, base, report):
    run = base / "shape12"
    child, flow = enter(binary, run, rules_with_weather(base, "clear"), report, {}, 3)
    entry = report.setdefault("shape12", {})
    try:
        for attempt in range(4):
            try:
                flow.resize((1280, 720))
                break
            except RuntimeError:
                time.sleep(2.0)
        flow.to_phase(0.35)
        flow.call("npc/pause")
        world, row, street_x, street_z = street(flow)
        placed = {}
        for index, prop in enumerate(SHIPPED_PROPS):
            placed[prop] = place_near(flow, {"type": "PlaceProp", "prop": prop, "rotation": index % 4},
                                      street_x + 6 + index * 2, street_z + 8)
        placed["field"] = place_near(flow, {"type": "PlacePlot", "plot": "field", "rotation": 0},
                                     street_x - 14, street_z + 12)
        placed["garden"] = place_near(flow, {"type": "PlacePlot", "plot": "garden", "rotation": 1},
                                      street_x + 12, street_z + 16)
        entry["placed"] = placed
        # The view reads placed props twice a second.
        flow.call("npc/resume")
        time.sleep(2.0)
        flow.call("npc/pause")
        cx, cz = street_x + 12, street_z + 10
        for label, x, z, distance, yaw in (("shape-props-14m", cx, cz - 2, 14, 0.5),
                                           ("shape-props-30m", cx, cz, 30, 0.5),
                                           ("shape-field-20m", street_x - 14, street_z + 12, 20, 0.8),
                                           ("shape-garden-16m", street_x + 12, street_z + 16, 16, 0.8)):
            flow.look(x, z, distance, label, yaw)
            time.sleep(1.5)
            flow.snap_now(label)
        # A placed prop removed: its visual goes with it.
        props = [(sim_id, parts) for sim_id, parts in flow.world().having("living_prop_v1")]
        entry["props_in_world"] = [[sim_id, parts["living_prop_v1"].get("prop")] for sim_id, parts in props]
        if props:
            removed = props[0][0]
            try:
                entry["removed"] = [removed, flow.call("npc/command", {"cmd": {"type": "RemoveProp", "prop": removed}})]
            except RuntimeError as error:
                entry["removed"] = [removed, str(error)[:200]]
            flow.call("npc/resume")
            time.sleep(1.5)
            flow.call("npc/pause")
            flow.look(cx, cz - 2, 14, "shape-props-after-removal-14m", 0.5)
            time.sleep(1.5)
            flow.snap_now("shape-props-after-removal-14m")
    finally:
        leave(child, flow, run, report, "shape12")
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/world3")
    parser.add_argument("--only", choices=("main", "boat", "rain", "storm", "snow",
                                           "scale", "live", "harbour", "harsh", "third", "bodies",
                                           "cast", "precip", "flash", "shocks", "poses", "cottage",
                                           "knight", "stormnight", "block", "upgrade5x6",
                                           "harbour5", "homework", "fisher", "larger", "motion", "budget", "look6", "village7", "look8", "look10", "deer11", "sound11", "shape12"))
    parser.add_argument("--prefix", default="after", help="--only block: before or after")
    parser.add_argument("--group-only", action="store_true",
                        help="--only cast: the group at the start only")
    parser.add_argument("--bodies", default=",".join(BODIES),
                        help="comma-separated bodies for --only bodies")
    parser.add_argument("--field-days", type=float, default=2.0,
                        help="game days to wait for the field to change")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    base = ROOT / "target" / ("playtest-world-" + uuid.uuid4().hex[:10])
    base.mkdir(parents=True)
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation by control name; view camera placement; a left "
                 "click through the window cursor; bounded operator clock steps",
        "rules": "copies of the shipped rules with one kind of weather every day and no shock",
        "physical_input_tested": False, "screenshots": {}, "checks": []}
    runs = []
    third = {"bodies", "cast", "precip", "flash", "shocks", "poses", "cottage", "knight", "stormnight",
             "block", "upgrade5x6", "harbour5", "homework", "fisher", "larger", "motion", "budget", "look6", "village7", "look8", "look10", "deer11", "sound11", "shape12"}
    try:
        if args.only == "knight":
            runs.append(knight_run(binary, base, report))
        if args.only == "stormnight":
            runs.append(storm_night_run(binary, base, report))
        if args.only == "block":
            runs.append(block_run(binary, base, report, args.prefix))
        if args.only == "upgrade5x6":
            runs.append(upgrade_run(binary, base, report))
        fifth = {"harbour5": harbour5_run, "homework": homework_run, "fisher": fisher_run,
                 "larger": larger_run, "motion": motion_run, "look6": look6_run, "village7": village7_run,
                 "look8": look8_run, "look10": look10_run,
                 "deer11": deer11_run, "sound11": sound11_run, "shape12": shape12_run}
        if args.only in fifth:
            runs.append(fifth[args.only](binary, base, report))
        if args.only == "budget":
            runs.append(budget_run(binary, base, report, "clear"))
            runs.append(budget_run(binary, base, report, "storm"))
        for kind in ("bodies", "cast", "precip", "flash", "shocks", "poses", "cottage"):
            if args.only not in (kind, "third"):
                continue
            if kind == "bodies":
                for body in args.bodies.split(","):
                    runs.append(bodies_run(binary, base, report, body.strip()))
            elif kind == "cast":
                runs.append(bodies_run(binary, base, report, "knight", "mixed", "cast-before-group"))
                runs.append(bodies_run(binary, base, report, "knight", "adventurer", "cast-after-group"))
                if not args.group_only:
                    runs.append(cast_run(binary, base, report, "mixed"))
                    runs.append(cast_run(binary, base, report, "default"))
            elif kind == "precip":
                for weather in ("rain", "storm", "snow"):
                    runs.append(precip_run(binary, base, report, weather))
            elif kind == "flash":
                runs.append(flash_run(binary, base, report))
            elif kind == "shocks":
                for shock in ("drought", "blight"):
                    runs.append(shock_run(binary, base, report, shock))
            elif kind == "poses":
                runs.append(poses_run(binary, base, report))
            elif kind == "cottage":
                for name in ("cottage-evening", "upgrade-underway"):
                    runs.append(cottage_run(binary, base, report, name))
        for kind in ("scale", "live", "harbour", "rain", "storm", "snow", "harsh", "main", "boat"):
            if args.only in third or args.only == "third":
                break
            if args.only not in (None, kind) or (args.only is None and kind in ("main", "boat")):
                continue
            if kind == "main":
                runs.append(main_run(binary, base, report, args.field_days))
            elif kind == "boat":
                runs.append(boat_run(binary, base, report))
            elif kind == "scale":
                runs.append(scale_run(binary, base, report))
            elif kind == "live":
                runs.append(live_run(binary, base, report))
            elif kind == "harbour":
                runs.append(harbour_run(binary, base, report))
            elif kind == "harsh":
                runs.append(harsh_run(binary, base, report))
            else:
                runs.append(weather_run(binary, base, report, kind))
        report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for run in base.iterdir():
            if run.is_dir() and (run / "shots").is_dir():
                for image in sorted((run / "shots").glob("*.png")):
                    shutil.copy2(image, args.out / image.name)
                    copied.append(image.name)
        report["copied"] = copied
        name = "summary.json" if args.only is None else f"summary-{args.only}.json"
        if args.only == "block":
            name = f"summary-block-{args.prefix}.json"
        (args.out / name).write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
