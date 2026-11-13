"""Unpaused native captures for the view checks: chimney smoke and forge embers
at three real times of day, carried goods and work poses at settlement zoom,
the one save notice and the Live tip.

Build first, then run from npc-framework:
  python scripts/playtest/capture_view_checks.py [--binary target/debug/island_game.exe]

Uses a disposable spectator world and native widget activation over BRP. The
light is never pinned: the operator clock is stepped to the real morning, dusk
and night, so the HUD's hour is the hour the scene shows. Every scene capture
is taken with the clock running. The camera is placed by writing the view's
own `RtsCamera`; residents and workplaces are found with operator state reads
(`world/state`), which a player does not have. No physical keyboard or mouse
input is exercised, so hovering the Live button is covered by unit tests only.
Captures are 1280x720 in target/npc-out/view-checks/ and still need to be read.
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
from capture_native_homestead import file_hash
from capture_player_flow import ALERTS, PRIMARY, wait_for
from capture_view_followups import FollowupFlow, Terrain

capture_player_flow.SIZES = ((1280, 720),)

DAY = 24000  # Ticks in an island day; checked against the host's own clock below.
LIVE_TIP = "Live begins your character's arrival on the island."
SAVED = "Island saved · Ctrl+S"
MENU_SAVED = "Your island is saved."
BULK = {"log", "plank", "ore", "ingot", "village_grain", "village_flour", "village_fiber",
        "village_hide", "village_leather", "village_raw_meat", "village_fish", "village_berries"}
HAMMER_ROLES = {"smith", "carpenter", "woodcutter", "miner"}
FARM_KINDS = {"village_grain", "village_fiber", "village_berries"}
AT_HOME_MM = 5000
SETTLEMENTS = ("Stone Ridge", "Harbour", "Alderwood")


def xz(value):
    return float(value["x"]), float(value["z"])


class World:
    """One operator read of the world, indexed for the questions asked here."""

    def __init__(self, state):
        self.tick = state["tick"]
        self.entities = {entity["sim_id"]: entity for entity in state["entities"]}

    def parts(self, sim_id):
        return self.entities.get(sim_id, {}).get("components", {})

    def having(self, key):
        return [(sim_id, entity["components"]) for sim_id, entity in sorted(self.entities.items())
                if key in entity.get("components", {})]

    def bulk(self, parts):
        """Bulk units carried, by kind (the view shows a prop from two units)."""
        load = {}
        for item in parts.get("inventory", {}).get("items", []):
            held = self.parts(item).get("item")
            if held and held["kind"] in BULK:
                load[held["kind"]] = load.get(held["kind"], 0) + max(1, held.get("stack", 1))
        return load

    def homes(self, settlement):
        rows = []
        for sim_id, parts in self.having("island_household_v1"):
            home = parts["island_household_v1"]
            if not home["label"].startswith(settlement):
                continue
            door = xz(home["door"])
            occupied = any("pos" in self.parts(resident) and
                           (xz(self.parts(resident)["pos"])[0] - door[0]) ** 2 +
                           (xz(self.parts(resident)["pos"])[1] - door[1]) ** 2 <= AT_HOME_MM ** 2
                           for resident in home["residents"])
            rows.append({"id": sim_id, "label": home["label"], "door": door, "occupied": occupied,
                         "heated": home["heated_until"] > self.tick})
        return rows

    def workers(self):
        """Residents by what the view should show them doing."""
        found = {"chop": [], "hammer": [], "farm": [], "carry": [], "idle": []}
        for sim_id, parts in self.having("island_villager"):
            person = parts["island_villager"]
            if "pos" not in parts or person.get("resting_until", 0) > self.tick:
                continue
            row = {"id": sim_id, "name": person["name"], "role": person["role"],
                   "at": xz(parts["pos"]), "bulk": self.bulk(parts), "left": 0}
            moving = "move_intent" in parts
            gathering, crafting = parts.get("island_gathering"), parts.get("crafting")
            if moving:
                # Ticks of walking left, so the walker is still walking when captured.
                intent = parts["move_intent"]
                goal = xz(intent["goal"])
                reach = ((goal[0] - row["at"][0]) ** 2 + (goal[1] - row["at"][1]) ** 2) ** 0.5
                row["left"] = int(reach / max(1, intent.get("speed_mm_per_tick", 70)))
                if sum(row["bulk"].values()) >= 2:
                    found["carry"].append(row)
            elif gathering:
                kind = self.parts(gathering["patch"]).get("island_resource_patch", {}).get("kind")
                row.update(left=gathering["ends"] - self.tick, work="gathering " + str(kind))
                if kind == "log":
                    found["chop"].append(row)
                elif kind in FARM_KINDS:
                    # Grain and berries grow in the open; fibre is cut under trees.
                    row["open"] = kind != "village_fiber"
                    found["farm"].append(row)
            elif crafting:
                station = self.parts(crafting.get("workstation")).get("island_place", {})
                row.update(left=crafting["ends"] - self.tick, station=station.get("label"),
                           work="crafting " + crafting["recipe"])
                if station.get("role") in HAMMER_ROLES:
                    found["hammer"].append(row)
            else:
                found["idle"].append(row)
        for rows in found.values():
            rows.sort(key=lambda row: (not row.get("open", True), -row["left"]))
        return found

    def working_smithies(self):
        busy = {parts["crafting"].get("workstation"): parts["crafting"]["ends"] - self.tick
                for _, parts in self.having("crafting")}
        return [{"id": sim_id, "label": parts["island_place"]["label"], "at": xz(parts["pos"]),
                 "left": busy[sim_id]}
                for sim_id, parts in self.having("island_place")
                if parts["island_place"].get("role") == "smith" and sim_id in busy]


class CheckFlow(FollowupFlow):
    def world(self):
        return World(self.state())

    def texts(self):
        return [str(row["text"]) for row in self.ui()[0]]

    def to_tick(self, target):
        now = self.state()["tick"]
        if target > now:
            self.advance(target - now)
        return self.state()["tick"]

    def to_phase(self, phase):
        """Step to the next time the day reaches `phase` (0 sunrise, 0.7 sunset)."""
        now = self.state()["tick"]
        target = now - now % DAY + int(phase * DAY)
        return self.to_tick(target if target >= now else target + DAY)

    def hud_clock(self):
        return [text for text in self.texts() if len(text) > 5 and text[2] == ":" and " · " in text][:1]

    def live(self, label, x, z, distance, settle=1.5, follow=None):
        """Frame a place, let the clock run, capture, and stop the clock again.

        `follow` names a resident on the move: the view is aimed where two
        position reads say they will be when the capture is taken."""
        self.look(x / 1000.0, z / 1000.0, distance, label)
        self.call("npc/resume")
        time.sleep(settle)
        if follow is not None and "pos" in World(self.state()).parts(follow):
            first, began = xz(World(self.state()).parts(follow)["pos"]), time.monotonic()
            time.sleep(0.25)
            second, now = xz(World(self.state()).parts(follow)["pos"]), time.monotonic()
            lead = 1.3 / max(now - began, 0.05)  # Placing the view and capturing take about 1.3 s.
            self.look((second[0] + (second[0] - first[0]) * lead) / 1000.0,
                      (second[1] + (second[1] - first[1]) * lead) / 1000.0, distance, label)
        self.capture(label)
        self.call("npc/pause")
        return self.state()["tick"]

    def smoke(self, moment, settlement):
        world = self.world()
        # The asked-for settlement, or whichever has a fire going right now.
        for name in [settlement] + [other for other in SETTLEMENTS if other != settlement]:
            homes = world.homes(name)
            smoking = [home for home in homes if home["occupied"] or home["heated"]]
            if smoking:
                settlement = name
                break
        entry = {"moment": moment, "settlement": settlement, "tick": world.tick,
                 "phase": round(world.tick % DAY / DAY, 3),
                 "hud_clock": self.hud_clock(),
                 "homes": len(homes), "occupied": sum(home["occupied"] for home in homes),
                 "heated": sum(home["heated"] for home in homes),
                 "expected_smoking": [home["label"] for home in smoking]}
        if smoking:
            x = sum(home["door"][0] for home in smoking) / len(smoking)
            z = sum(home["door"][1] for home in smoking) / len(smoking)
            near = min(smoking, key=lambda home: (home["door"][0] - x) ** 2 + (home["door"][1] - z) ** 2)
            # The fade-in takes three seconds; give the column time to stand.
            self.live(f"smoke-{moment}-62m", x, z, 62, settle=5.0)
            self.live(f"smoke-{moment}-30m", near["door"][0], near["door"][1] + 3000, 30, settle=5.0)
            entry["framed_home"] = near["label"]
        self.report.setdefault("smoke", []).append(entry)

    def embers(self, moment, budget=1500):
        """A smithy with a craft running, waiting a bounded time for one."""
        spent = 0
        while True:
            busy = [row for row in self.world().working_smithies() if row["left"] >= 60]
            if busy or spent >= budget:
                break
            self.advance(40)
            spent += 40
        entry = {"moment": moment, "waited_ticks": spent, "working": busy}
        if busy:
            smithy = max(busy, key=lambda row: row["left"])
            entry["tick"] = self.live(f"embers-{moment}-30m", smithy["at"][0], smithy["at"][1], 30, settle=2.0)
            self.live(f"embers-{moment}-14m", smithy["at"][0], smithy["at"][1], 14, settle=1.0)
        self.report.setdefault("embers", []).append(entry)
        return bool(busy)

    def poses(self, budget=9000):
        wanted, done, spent = ["chop", "hammer", "farm", "carry", "idle"], {}, 0
        while len(done) < len(wanted) and spent <= budget:
            workers = self.world().workers()
            for kind in wanted:
                if kind in done:
                    continue
                rows = [row for row in workers[kind]
                        if kind == "idle" or row["left"] >= (160 if kind == "carry" else 50)]
                # Someone hoeing in the open is worth a wait; fibre is cut under trees.
                if kind == "farm" and spent < 2400:
                    rows = [row for row in rows if row.get("open")]
                if not rows:
                    continue
                row = rows[0]
                # Two frames of the same motion at settlement zoom, then a closer one.
                follow = row["id"] if kind == "carry" else None
                row["tick"] = self.live(f"pose-{kind}-30m-a", row["at"][0], row["at"][1], 30,
                                        settle=1.0, follow=follow)
                self.live(f"pose-{kind}-30m-b", row["at"][0], row["at"][1], 30, settle=0.45, follow=follow)
                self.live(f"pose-{kind}-14m", row["at"][0], row["at"][1], 14, settle=0.6, follow=follow)
                done[kind] = row
                break  # The clock ran: read the world again before the next kind.
            else:
                self.advance(40)
                spent += 40
        self.report["poses"] = done
        self.report["poses_missing"] = [kind for kind in wanted if kind not in done]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/view-checks")
    parser.add_argument("--settlement", default="Stone Ridge")
    parser.add_argument("--only-poses", action="store_true",
                        help="skip the notice, tip, smoke and ember captures")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    run = ROOT / "target" / ("playtest-view-checks-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    save = run / "living-world.world.json"
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "scope": "smoke, embers, carried goods, work poses, save notice, Live tip",
        "input": "native BRP widget activation, view camera placement, bounded operator clock steps",
        "light": "not pinned: the sim clock is stepped to each time of day",
        "physical_input_tested": False, "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = CheckFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 30 * 60
            flow.tick_budget = 3 * DAY
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            entered = time.monotonic()
            flow.call("npc/pause")

            # The Live tip: once, briefly, then only from the Live button.
            if not args.only_poses:
                report["live_tip_at_first_sight"] = LIVE_TIP in flow.texts()
                flow.capture("hint-first-sight")
                report["live_tip_seconds_into_oversee_at_capture"] = round(time.monotonic() - entered, 1)
                time.sleep(max(0.0, 11.0 - (time.monotonic() - entered)))
                report["live_tip_after_introduction"] = LIVE_TIP in flow.texts()
                flow.capture("hint-after-introduction")
                flow.click("Shape")
                report["live_tip_in_shape"] = LIVE_TIP in flow.texts()
                flow.click("Oversee")

            # One save notice, and it fades.
            flow.click("Menu")
            flow.click("Save game")
            flow.terrain = Terrain(json.loads(save.read_text(encoding="utf-8")))
            flow.click("Resume")
            flow.call("npc/pause")
            if not args.only_poses:
                shown = flow.texts()
                report["save_notices_after_resume"] = [text for text in shown if text in (SAVED, MENU_SAVED)]
                flow.capture("save-notice")
                time.sleep(5.5)
                report["save_notices_six_seconds_later"] = [text for text in flow.texts()
                                                            if text in (SAVED, MENU_SAVED)]
                flow.capture("save-notice-faded")
            # The scene captures want the island: fold the alert stack. The
            # ledger is closed until a dock tile opens it.
            flow.click(ALERTS + " > Hide")
            flow.close_ledger()
            state = flow.state()
            if state["time_permille"] != state["tick"] % DAY * 1000 // DAY:
                raise RuntimeError("The island day is not 24,000 ticks; fix DAY")

            # Morning: 08:30 on the real clock, as in the earlier captures.
            flow.to_phase(2500 / DAY)
            if not args.only_poses:
                flow.smoke("morning", args.settlement)
            # Working day: poses, goods and a forge at work.
            flow.poses()
            if not args.only_poses:
                flow.embers("day")
                # Dusk: half an hour after the 22:48 sunset, when residents come home.
                # (Before sunset the homes are empty: everyone is still at work.)
                flow.to_phase(0.72)
                flow.smoke("dusk", args.settlement)
                flow.embers("dusk", budget=0)
                # Night: 02:24.
                flow.to_phase(0.85)
                flow.smoke("night", args.settlement)
                flow.embers("night", budget=600)

            flow.click("Menu")
            flow.click("Quit game")
            child.wait(timeout=40)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("view_checks_failure", error=str(error))
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
            report["asset_log"] = [line for line in lines if "assets: " in line][-2:]
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for image in sorted((run / "shots").glob("*.png")) if (run / "shots").is_dir() else []:
            shutil.copy2(image, args.out / image.name)
            copied.append(str(args.out / image.name))
        report["copied"] = copied
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        shutil.copy2(run / "summary.json",
                     args.out / ("summary-poses.json" if args.only_poses else "summary.json"))
        print(run, flush=True)


if __name__ == "__main__":
    main()
