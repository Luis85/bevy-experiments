"""Native captures and one measurement for the view wiring pass: chimney smoke
and forge embers, a settlement at night from the front and from behind, and a
Shape stroke driven through the real pointer path with its plan-to-mesh time.

Build first, then run from npc-framework:
  python scripts/playtest/capture_view_wiring.py [--mode full|shape|night]
      [--per-cell] [--pin-night] [--binary target/debug/island_game.exe]

Uses a disposable spectator world and native widget activation over BRP, like
`capture_view_checks.py`. The Shape drag is not a recorded command sent by the
script: it writes the window's cursor position and `MouseButtonInput` messages
(the same state and messages the OS delivers), so the view's own picking, dab
planning, command recording and mesh rebuild run. `ISLAND_VIEW_STATS=1` makes
the view log one `shape stroke:` line per stroke with the time from the frame
that planned an edit to the frame whose terrain mesh shows it; those lines are
copied into the summary. `--per-cell` sets `ISLAND_VIEW_SHAPE_BATCH=off`, which
records uneven edits one cell at a time as before, for a comparison on one
build. `--pin-night` pins the light to midnight (`ISLAND_VIEW_DAY_PHASE=0.85`)
instead of stepping the clock, for a quick look at the night scene; the HUD
then still shows the real hour. Captures are 1280x720 in
target/npc-out/view-wiring/ and still need to be read.
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
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import ALERTS, PRIMARY, wait_for
from capture_view_checks import DAY, SETTLEMENTS, CheckFlow, World, xz
from capture_view_followups import CAMERA, Terrain, island_pitch

capture_player_flow.SIZES = ((1280, 720),)

WINDOW = "bevy_window::window::Window"
STROKE = "shape stroke: "


class WiringFlow(CheckFlow):
    log_path = None

    def look(self, x, z, distance, label, yaw=math.pi):
        """Frame a place. Yaw 0 is the player's default view (house fronts);
        the other capture scripts look from behind, at yaw pi."""
        entity, _ = self.camera_entity()
        focus = [float(x), self.terrain.ground(x, z), float(z)]
        for path, value in (("focus", focus), ("distance", float(distance)),
                            ("pitch", island_pitch(distance)), ("yaw", float(yaw))):
            self.call("world.mutate_components", {"entity": entity, "component": CAMERA,
                                                  "path": path, "value": value})
        time.sleep(0.6)
        _, pose = self.camera_entity()
        self.record("camera", label=label, focus=focus, distance=distance, yaw=yaw)
        self.report.setdefault("camera_poses", {})[label] = pose

    def shot(self, label, x, z, distance, yaw=math.pi, settle=1.5):
        self.look(x / 1000.0, z / 1000.0, distance, label, yaw)
        self.call("npc/resume")
        time.sleep(settle)
        self.capture(label)
        self.call("npc/pause")

    def settlement(self, name):
        homes = self.world().homes(name)
        x = sum(home["door"][0] for home in homes) / len(homes)
        z = sum(home["door"][1] for home in homes) / len(homes)
        return homes, x, z

    # Shape: the real pointer path.

    def scale(self):
        resolution = next(row["components"][WINDOW]["resolution"] for row in query(self.url, [WINDOW])
                          if int(row["entity"]) == self.window)
        return resolution.get("scale_factor_override") or resolution["scale_factor"]

    def cursor(self, x, y):
        """Put the pointer at a logical window position: the window's own
        cursor state, which the Shape tools read, and the picking pointer,
        so interface hover follows it and not the machine's real mouse."""
        factor = self.pointer_scale
        self.call("world.mutate_components", {"entity": self.window, "component": WINDOW,
                  "path": "internal.physical_cursor_position", "value": [x * factor, y * factor]})
        self.call("world.write_message", {"message": "bevy_window::event::WindowEvent",
                  "value": {"CursorMoved": {"window": self.window, "position": [x, y], "delta": None}}})

    def button(self, pressed):
        self.call("world.write_message", {"message": "bevy_input::mouse::MouseButtonInput",
                  "value": {"button": "Left", "state": "Pressed" if pressed else "Released",
                            "window": self.window}})

    def stroke_lines(self):
        text = self.log_path.read_text(encoding="utf-8", errors="replace")
        return [line[line.index(STROKE) + len(STROKE):].strip()
                for line in text.splitlines() if STROKE in line]

    def drag(self, label, start, end, seconds=2.4):
        """Press, sweep the cursor across the view and release; then wait for
        the view's own summary of the stroke."""
        before = len(self.stroke_lines())
        steps = max(8, int(seconds / 0.04))
        # A capture sets the window's size and scale; read them as they are now.
        self.pointer_scale = self.scale()
        self.cursor(*start)
        # The cursor note names the cell under the pointer once the view has it.
        wait_for(lambda: [text for text in self.texts() if " · cell " in text], 8)
        began = time.monotonic()
        self.button(True)
        for step in range(1, steps + 1):
            t = step / steps
            self.cursor(start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t)
            due = began + seconds * t
            if due > time.monotonic():
                time.sleep(due - time.monotonic())
        held = time.monotonic() - began
        self.button(False)
        lines = wait_for(lambda: self.stroke_lines()[before:], 90)
        entry = {"stroke": label, "held_seconds": round(held, 2), "cursor_steps": steps,
                 "view_log": lines}
        self.report.setdefault("shape", []).append(entry)
        self.record("drag", **entry)
        return entry

    def history(self, label, widget):
        before = len(self.stroke_lines())
        self.click(widget)
        lines = wait_for(lambda: self.stroke_lines()[before:], 60)
        self.report.setdefault("shape", []).append({"stroke": label, "view_log": lines})

    def open_ground(self):
        """The dry, fairly level spot farthest from every home, site, patch
        and resident: the brush is refused within five metres of any of them."""
        taken = [xz(entity["components"]["pos"]) for entity in self.state()["entities"]
                 if "pos" in entity.get("components", {})]
        best = None
        for z in range(-80, 81, 4):
            for x in range(-100, 101, 4):
                cells = [self.terrain.cell(x + dx, z + dz) for dx in range(-9, 10, 3) for dz in range(-9, 10, 3)]
                if any(cell is None or str(cell["surface"]).lower() == "water" for cell in cells):
                    continue
                heights = [cell["height_mm"] for cell in cells]
                if max(heights) - min(heights) > 900 or min(heights) < 250:
                    continue
                near = min(math.hypot(px / 1000.0 - x, pz / 1000.0 - z) for px, pz in taken)
                if best is None or near > best[0]:
                    best = (near, x, z)
        if best is None:
            raise RuntimeError("No open ground for the Shape stroke")
        return best[1], best[2], round(best[0], 1)

    def shape(self):
        x, z, clearance = self.open_ground()
        # A radius-4 brush must stay five metres clear of everything, so the
        # sweep is as long as the open ground allows, up to 13 m.
        reach = max(1.5, min(6.5, clearance - 9.5))
        half = int(reach / 0.046)
        self.report["shape_ground"] = {"cell": [x, z], "clearance_m": clearance,
                                       "sweep_m": round(2 * reach, 1)}
        self.click("Shape")
        wait_for(lambda: self.widget("Raise · R"))
        # 40 m out, looking straight at the spot: the window centre is the
        # cell, and 100 px across is about 4.6 m of ground.
        self.look(x, z, 40, "shape", yaw=0.0)
        self.pointer_scale = self.scale()
        self.click("Raise · R")
        self.click("Edge: hard · H")
        wait_for(lambda: self.widget("Edge: soft · H"))
        for _ in range(2):
            self.click("Larger · ]")
        self.report["shape_brush"] = [text for text in self.texts() if text.startswith("Radius ")][:1]
        self.call("npc/resume")
        time.sleep(0.5)
        self.capture("shape-before-drag")
        self.drag("soft Raise, radius 4", (640 - half, 380), (640 + half, 380))
        time.sleep(0.4)
        self.capture("shape-after-raise-drag")
        self.click("Smooth · G")
        self.drag("soft Smooth, radius 4", (640 + half, 392), (640 - half, 392))
        time.sleep(0.4)
        self.capture("shape-after-smooth-drag")
        self.report["shape_status_after_strokes"] = [text for text in self.texts()
                                                     if "edit" in text.lower() or "applied" in text.lower()][:3]
        self.history("undo Smooth", "Undo · Ctrl+Z")
        self.history("undo Raise", "Undo · Ctrl+Z")
        time.sleep(0.4)
        self.capture("shape-after-undo")
        self.call("npc/pause")
        self.click("Oversee")
        # The scene captures want the island; Oversee leaves the ledger closed.
        self.close_ledger()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/view-wiring")
    parser.add_argument("--mode", choices=("full", "shape", "night"), default="full")
    parser.add_argument("--per-cell", action="store_true",
                        help="record uneven Shape edits cell by cell (ISLAND_VIEW_SHAPE_BATCH=off)")
    parser.add_argument("--pin-night", action="store_true",
                        help="pin the light to midnight instead of stepping the clock")
    parser.add_argument("--tag", default="", help="suffix for the summary file name")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    run = ROOT / "target" / ("playtest-view-wiring-" + uuid.uuid4().hex[:10])
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
    environment["ISLAND_VIEW_STATS"] = "1"
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    environment.pop("ISLAND_VIEW_SHAPE_BATCH", None)
    if args.per_cell:
        environment["ISLAND_VIEW_SHAPE_BATCH"] = "off"
    if args.pin_night:
        environment["ISLAND_VIEW_DAY_PHASE"] = "0.85"
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "mode": args.mode, "per_cell": args.per_cell, "pinned_night": args.pin_night,
        "input": "native BRP widget activation; view camera placement; bounded operator clock "
                 "steps; for Shape, window cursor position and MouseButtonInput messages",
        "physical_input_tested": False, "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = WiringFlow(run, child, url, report, time.monotonic())
            flow.log_path = run / "native.log"
            flow.deadline = time.monotonic() + 30 * 60
            flow.tick_budget = 3 * DAY
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
            # The scene captures want the island: fold the alert stack. The
            # ledger is closed until a dock tile opens it.
            flow.click(ALERTS + " > Hide")
            flow.close_ledger()

            if args.mode in ("full", "shape"):
                flow.shape()
            if args.mode == "full":
                # Morning: 08:30 on the real clock.
                flow.to_phase(2500 / DAY)
                flow.smoke("morning", "Stone Ridge")
                flow.embers("day")
                busy = report.get("embers", [{}])[-1].get("working") or []
                if busy:
                    smithy = max(busy, key=lambda row: row["left"])
                    # The forge chimney stands 3 m behind the smithy's own
                    # position and its mouth is 2.4 m up: aim past it.
                    flow.shot("embers-day-10m", smithy["at"][0] + 500, smithy["at"][1] + 5600, 10,
                              settle=0.8)
            if args.mode in ("full", "night"):
                if not args.pin_night:
                    flow.to_phase(0.85)  # 02:24.
                if args.mode == "full":
                    flow.smoke("night", "Stone Ridge")
                for name in SETTLEMENTS[:2] if args.mode == "full" else ("Harbour",):
                    homes, x, z = flow.settlement(name)
                    slug = name.lower().replace(" ", "-")
                    report.setdefault("night", []).append({
                        "settlement": name, "tick": flow.state()["tick"],
                        "hud_clock": flow.hud_clock(), "homes": len(homes),
                        "occupied": sum(home["occupied"] for home in homes),
                        "heated": sum(home["heated"] for home in homes)})
                    flow.shot(f"night-{slug}-front-62m", x, z, 62, yaw=0.0, settle=3.0)
                    flow.shot(f"night-{slug}-back-62m", x, z, 62, yaw=math.pi, settle=1.0)
                    lit = [home for home in homes if home["occupied"] or home["heated"]] or homes
                    home = min(lit, key=lambda home: (home["door"][0] - x) ** 2 + (home["door"][1] - z) ** 2)
                    flow.shot(f"night-{slug}-home-front-24m", home["door"][0], home["door"][1] - 2000,
                              24, yaw=0.0, settle=1.0)
                    flow.shot(f"night-{slug}-home-back-24m", home["door"][0], home["door"][1] - 4000,
                              24, yaw=math.pi, settle=1.0)
                    flow.shot(f"night-{slug}-home-side-24m", home["door"][0], home["door"][1] - 3000,
                              24, yaw=math.pi / 2, settle=1.0)

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
                flow.failure_context("view_wiring_failure", error=str(error))
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
            report["shape_log"] = [line[line.index(STROKE):] for line in lines if STROKE in line]
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for image in sorted((run / "shots").glob("*.png")) if (run / "shots").is_dir() else []:
            name = image.name if not args.tag else image.name.replace("-1280x720", f"-{args.tag}-1280x720")
            shutil.copy2(image, args.out / name)
            copied.append(str(args.out / name))
        report["copied"] = copied
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        suffix = "-".join(part for part in (args.mode, args.tag) if part)
        shutil.copy2(run / "summary.json", args.out / f"summary-{suffix}.json")
        print(run, flush=True)


if __name__ == "__main__":
    main()
