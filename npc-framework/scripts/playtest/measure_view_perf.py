"""Per-system view cost on the authored living world, read from BRP `npc/perf`.

Build first, then run from npc-framework:
  python scripts/playtest/measure_view_perf.py [--binary target/debug/island_game.exe]
      [--match living_scene] [--seconds 6] [--running]
      [--camera 0,0,330] [--no-dressing] [--size 2560x1440]

Launches a disposable developer scenario (no save, no player session), frames
Stone Ridge at 62 m, waits for the scene, resets the profiler and samples for
the given time. Paused by default, as in docs/island-atmosphere.md. Prints the
matching systems, the ten largest and the frame summary. `--camera` frames
another view (`0,0,330` is the overview) and `--no-dressing` leaves out the
outcrop rocks and edge bushes (`ISLAND_VIEW_DRESSING=off`) for a comparison on
the same build. `--size` resizes the window (physical pixels) before sampling.
The mean frame time is the sampling time over the frames counted; with vertical
sync it cannot fall below the display's refresh interval. Operator measurement
only, and only as steady as the machine it runs on.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import uuid

from capture_native import ROOT, WINDOW, query, rpc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--match", default="living_scene")
    parser.add_argument("--seconds", type=float, default=6.0)
    parser.add_argument("--running", action="store_true", help="sample with the world running at 1x")
    parser.add_argument("--camera", default="65,40,62", help="X,Z,DIST of the view to measure")
    parser.add_argument("--no-dressing", action="store_true", help="leave out rocks and bushes")
    parser.add_argument("--size", help="WIDTHxHEIGHT window size in physical pixels")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before measuring")
    run = ROOT / "target" / ("playtest-view-perf-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment["ISLAND_VIEW_DAY_PHASE"] = "0.35"
    if args.no_dressing:
        environment["ISLAND_VIEW_DRESSING"] = "off"
    child = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--scenario", "living_world",
                "--island-mode", "inspector", "--camera", args.camera, "--mute",
                "--remote", "--remote-port", str(port), "--no-render-remote",
                "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            end = time.monotonic() + 150
            while True:
                if child.poll() is not None:
                    raise RuntimeError("Native host exited; inspect " + str(run / "native.log"))
                try:
                    rpc(url, "npc/pause")
                    break
                except (OSError, RuntimeError):
                    if time.monotonic() > end:
                        raise
                    time.sleep(0.3)
            time.sleep(14)  # Scene assets, chunks and the first framing settle.
            if args.size:
                width, height = (int(part) for part in args.size.lower().split("x"))
                row = query(url, [WINDOW], ["bevy_window::window::PrimaryWindow"])[0]
                resolution = dict(row["components"][WINDOW]["resolution"])
                resolution.update(scale_factor_override=1.0, physical_width=width,
                                  physical_height=height)
                rpc(url, "world.mutate_components", {"entity": int(row["entity"]),
                    "component": WINDOW, "path": "resolution", "value": resolution})
                time.sleep(3)
                row = query(url, [WINDOW], ["bevy_window::window::PrimaryWindow"])[0]
                now = row["components"][WINDOW]["resolution"]
                print("window", now["physical_width"], "x", now["physical_height"])
            if args.running:
                rpc(url, "npc/resume")
            rpc(url, "npc/perf/reset")
            began = time.monotonic()
            time.sleep(args.seconds)
            report = rpc(url, "npc/perf")
            sampled = time.monotonic() - began
            rpc(url, "npc/pause")
        (run / "perf.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        systems = report.get("systems", [])
        print("frames", report.get("frames"), "systems", len(systems))
        if report.get("frames"):
            print("mean_frame_ms", round(1000.0 * sampled / report["frames"], 2))
        print("SUMMARY", json.dumps({key: value for key, value in report.items()
                                     if not isinstance(value, (list, dict))}))
        for key, value in report.items():
            if isinstance(value, dict):
                print("GROUP", key, json.dumps(value)[:600])
        if systems:
            print("fields", sorted(systems[0].keys()))
        for row in systems:
            if args.match in row.get("name", ""):
                print("MATCH", json.dumps(row))
        for row in systems[:10]:
            print("TOP", json.dumps(row))
        print(run)
    finally:
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)


if __name__ == "__main__":
    main()
