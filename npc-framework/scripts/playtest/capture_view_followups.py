"""Native captures for the view follow-ups: folded alerts, legend, economy,
shape palette, settings, and the ground, rock, shore, road and tree-line look.

Build first, then run from npc-framework:
  python scripts/playtest/capture_view_followups.py [--binary target/debug/island_game.exe]

Uses a disposable spectator world, native widget activation over BRP, bounded
operator clock steps and `ISLAND_VIEW_DAY_PHASE=0.35` (noon light). The camera
is placed by writing the view's own `RtsCamera` component over BRP; that is
view state and changes no simulation result. Every state is captured at
1280x720 and 1024x768 and copied to target/npc-out/view-followups/.
No physical keyboard or mouse input is exercised: Tab order, focus rings, edge
panning and middle-mouse orbit are covered by unit tests only. Screenshots
still need a human or agent to read them.
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

from capture_living_world import LivingFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import ALERTS, LEDGER, LEGEND, PAGES, PRIMARY, wait_for

CAMERA = "island_view::camera::RtsCamera"


def island_pitch(distance):
    """The resting pitch of the island camera (`camera::island_pitch`)."""
    t = min(1.0, max(0.0, math.log(max(distance, 1.0) / 30.0) / math.log(8.0)))
    t = t * t * (3.0 - 2.0 * t)
    return 0.663 + (1.012291 - 0.663) * t


class Terrain:
    def __init__(self, snapshot):
        terrain = snapshot["resources"]["living_world_terrain_v1"]
        self.width, self.height, self.cells = terrain["width"], terrain["height"], terrain["cells"]

    def cell(self, x, z):
        column, row = x + self.width // 2, z + self.height // 2
        if 0 <= column < self.width and 0 <= row < self.height:
            return self.cells[row * self.width + column]
        return None

    def is_water(self, x, z):
        cell = self.cell(x, z)
        return cell is None or str(cell["surface"]).lower() == "water"

    def biome(self, x, z):
        cell = self.cell(x, z)
        return None if cell is None else str(cell.get("biome", "")).lower()

    def ground(self, x, z):
        cell = self.cell(int(math.floor(x)), int(math.floor(z)))
        return 0.0 if cell is None else max(0, cell["height_mm"]) / 1000.0

    def south_coast(self, x, start):
        """First water cell going south (towards the camera) from `start`."""
        for z in range(start, -self.height // 2, -1):
            if self.is_water(x, z):
                return x, z
        raise RuntimeError("No coast south of the Harbour")

    def woodland_border(self):
        """A woodland cell with open ground beside it, nearest the island's middle."""
        best = None

        def open_ground(x, z):
            return self.biome(x, z) not in (None, "woodland") and not self.is_water(x, z)

        for z in range(-self.height // 2 + 8, self.height // 2 - 8):
            for x in range(-self.width // 2 + 8, self.width // 2 - 8):
                if self.biome(x, z) != "woodland" or self.is_water(x, z):
                    continue
                for side in (-1, 1):
                    # A straight stretch: the same border four cells up and down.
                    if not all(self.biome(x, z + d) == "woodland" and open_ground(x + side, z + d)
                               for d in (-4, -2, 0, 2, 4)):
                        continue
                    score = x * x + (z + 10) ** 2
                    if best is None or score < best[0]:
                        best = (score, x, z)
        if best is None:
            raise RuntimeError("No straight woodland border found")
        return best[1], best[2]


class FollowupFlow(LivingFlow):
    terrain = None

    def camera_entity(self):
        rows = query(self.url, [CAMERA])
        if len(rows) != 1:
            raise RuntimeError(f"Expected one island camera, found {len(rows)}")
        return int(rows[0]["entity"]), rows[0]["components"][CAMERA]

    def look(self, x, z, distance, label):
        entity, _ = self.camera_entity()
        focus = [float(x), self.terrain.ground(x, z), float(z)]
        for path, value in (("focus", focus), ("distance", float(distance)),
                            ("pitch", island_pitch(distance)), ("yaw", math.pi)):
            self.call("world.mutate_components", {"entity": entity, "component": CAMERA,
                                                  "path": path, "value": value})
        time.sleep(0.6)
        _, pose = self.camera_entity()
        self.record("camera", label=label, focus=focus, distance=distance)
        self.report.setdefault("camera_poses", {})[label] = pose

    def lines(self, prefix=""):
        """Journal lines, newest first. They were the Oversee feed; they are the
        rows of the ledger's Journal page now (the alert stack shows only what
        needs attention). The page is opened for the read and the right column
        put back as it was."""
        before = self.ledger_page()
        self.open_page("Journal")
        found = [text for _, text in self.button_rows(LEDGER)
                 if text not in ("Important", "Critical", "Everything") and text not in PAGES
                 and text.startswith(prefix)]
        if before is None:
            self.close_ledger()
        elif before != "Journal":
            self.open_page(before)
        return found

    def residents(self):
        """Operator read of resident positions, to compare between captures."""
        return {entity["sim_id"]: self.position(entity["components"]["pos"])
                for entity in self.state()["entities"]
                if "island_villager" in entity.get("components", {}) and "pos" in entity["components"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/view-followups")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    run = ROOT / "target" / ("playtest-view-followups-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    save = run / "living-world.world.json"
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_DAY_PHASE"] = "0.35"
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "scope": "alert severity and folding, legend, economy selector, shape palette, settings, scene look",
        "input": "native BRP widget activation, view camera placement and bounded operator clock steps",
        "physical_input_tested": False, "provider": False, "grants": 0, "teleports": 0,
        "light": "ISLAND_VIEW_DAY_PHASE=0.35 (noon); the HUD shows the real time",
        "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = FollowupFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 30*60
            flow.tick_budget = 24000
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])

            # Settings: the camera toggle and pan speed persist beside the save.
            flow.click("Settings")
            flow.click("Edge pan: on")
            flow.click("Pan +")
            flow.capture("settings")
            stored = json.loads((run / "island-settings.json").read_text(encoding="utf-8"))
            if stored.get("edge_pan") is not False or stored.get("pan_speed") != 125:
                raise RuntimeError(f"Settings file does not hold the camera choices: {stored}")
            report["settings_file"] = stored
            flow.click("Edge pan: off")
            flow.click("Pan −")
            restored = json.loads((run / "island-settings.json").read_text(encoding="utf-8"))
            if restored.get("edge_pan") is not True or restored.get("pan_speed") != 100:
                raise RuntimeError(f"Settings did not return to their defaults: {restored}")
            flow.click("Back")

            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            # A little over one game day, so job changes, trades and levy day reach the feed.
            flow.advance(2600)
            flow.capture("oversee-overview")
            folded = flow.lines()
            report["feed_default"] = folded
            folds = [line for line in folded if "· show" in line]
            report["legend_hidden_at_overview"] = not flow.has_widget(LEGEND + " > Hide")

            flow.open_page("Economy")
            flow.capture("oversee-economy")

            flow.region("Stone Ridge")
            time.sleep(0.8)
            report["legend_shown_at_settlement_zoom"] = flow.has_widget(LEGEND + " > Hide")
            flow.capture("oversee-ridge-feed-legend")
            # Folds and the three filters belong to the Journal page.
            flow.open_page("Journal")
            if folds:
                first = folds[0].split("\n")[0]
                flow.press_row(first)
                report["feed_unfolded"] = flow.lines()
                flow.capture("oversee-feed-unfolded")
                flow.press_row(first.replace("· show", "· hide"))
            flow.click("Everything")
            report["feed_everything"] = flow.lines()
            flow.click("Critical")
            report["feed_critical"] = flow.lines()
            flow.click("Important")
            flow.close_ledger()
            flow.click(ALERTS + " > Hide")
            if flow.has_widget(LEGEND + " > Hide"):
                flow.click(LEGEND + " > Hide")
            flow.capture("oversee-collapsed")
            report["collapsed_title"] = flow.panel_texts(ALERTS)

            # Scene shots: panels out of the way, noon light, autumn.
            flow.terrain = Terrain(flow.save_from_inspector(save))
            # Leaving the pause menu resumes the clock; the stills want it stopped.
            flow.call("npc/pause")
            flow.close_ledger()  # The scene captures want the island.
            ridge = (65, 40)
            flow.look(*ridge, 62, "ridge-mid")
            flow.capture("scene-ridge-mid")
            flow.look(ridge[0] + 6, ridge[1] - 4, 13, "ridge-close")
            flow.capture("scene-ridge-close")
            flow.look(0, -30, 62, "harbour")
            flow.capture("scene-harbour-noon")
            coast = flow.terrain.south_coast(0, -40)
            flow.look(coast[0], coast[1] + 5, 34, "coast")
            flow.capture("scene-coast")
            flow.look(coast[0], coast[1] + 2, 12, "coast-close")
            flow.capture("scene-coast-close")
            border = flow.terrain.woodland_border()
            report["woodland_border_cell"] = list(border)
            flow.look(border[0], border[1], 48, "woodland-edge")
            flow.capture("scene-woodland-edge")
            flow.look(-65, 40, 62, "alderwood")
            flow.capture("scene-alderwood")
            flow.look(0, 0, 330, "overview")
            flow.capture("scene-overview")

            # Unpaused: three captures a few seconds apart at one settlement.
            flow.look(ridge[0], ridge[1], 30, "ridge-live")
            before = flow.residents()
            flow.call("npc/resume")
            moved = []
            for index, name in enumerate(("a", "b", "c")):
                time.sleep(3.0 if index else 1.0)
                flow.capture("live-ridge-" + name)
                now = flow.residents()
                moved.append(sum(1 for unit, pos in now.items()
                                 if unit in before and pos != before[unit]))
                before = now
            flow.call("npc/pause")
            report["residents_moved_between_live_captures"] = moved
            report["tick_after_live_captures"] = flow.state()["tick"]

            flow.close_ledger()
            flow.click("Shape")
            flow.click("Flatten · T")
            flow.click("Stone")
            flow.click("Larger · ]")
            flow.capture("shape-palette")

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
                flow.failure_context("view_followups_failure", error=str(error))
            except Exception as diagnostic:
                report["diagnostic_error"] = str(diagnostic)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace
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
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        shutil.copy2(run / "summary.json", args.out / "summary.json")
        print(run, flush=True)


if __name__ == "__main__":
    main()
