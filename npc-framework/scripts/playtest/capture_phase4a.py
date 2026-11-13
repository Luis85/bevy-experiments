"""Native captures of the Live / Oversee / Shape bar, alerts, economy and settings.

Build first, then run from npc-framework:
  python scripts/playtest/capture_phase4a.py [--binary target/debug/game_menu.exe]

Uses a disposable spectator world, native widget activation over BRP and bounded
operator clock steps. Every state is captured at 1280x720 and 1024x768 and copied
to target/npc-out/phase4a/. Screenshots still need a human or agent to read them.
No physical keyboard or mouse input is exercised: hotkeys and drag painting are
covered by unit tests only, and this script makes no claim about them.
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

from capture_living_world import LivingFlow, terrain_candidate
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import PRIMARY, wait_for


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/phase4a")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    run = ROOT / "target" / ("playtest-phase4a-" + uuid.uuid4().hex[:10])
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
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "scope": "session bar, alerts, economy table, shape palette, live HUD, settings",
        "input": "native BRP widget activation and bounded operator clock steps",
        "physical_input_tested": False, "provider": False, "grants": 0, "teleports": 0,
        "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--settings", str(settings),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = LivingFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 30*60
            flow.tick_budget = 24000
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])

            flow.click("Settings")
            flow.click("Master −")
            flow.click("Display")
            flow.click("Interface +")
            flow.click("Interface −")
            flow.capture("settings")
            stored = json.loads((run / "island-settings.json").read_text(encoding="utf-8"))
            if stored.get("master_volume") != 50 or stored.get("ui_scale") != 100:
                raise RuntimeError(f"Settings file does not hold the chosen values: {stored}")
            report["settings_file"] = stored
            flow.click("Back")

            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            # A little over one game day, so levy day and daily life reach the feed.
            flow.advance(2600)
            flow.capture("oversee-village")
            flow.open_page("Economy")
            flow.capture("oversee-economy")
            rows, _, _ = flow.ui()
            texts = [str(row["text"]) for row in rows]
            report["alert_lines"] = [text for text in texts if text.startswith("Day ")][:12]
            report["economy_header_visible"] = all(word in texts for word in ("Good", "Stock", "Price", "Since yesterday"))

            # Operator measurement, not player input: how long one recorded
            # single-cell edit takes to validate on this build.
            before = flow.save_from_inspector(save)
            command = terrain_candidate(before, flow.state())
            command.update(radius=0, surface="grass")
            timings = []
            for _ in range(3):
                command["height_mm"] += 250
                started = time.perf_counter()
                flow.call("npc/command", {"cmd": command})
                timings.append(round(time.perf_counter() - started, 3))
                flow.advance(2)
            report["single_cell_terraform_submit_seconds"] = timings

            flow.click("Shape")
            flow.click("Flatten · T")
            flow.click("Stone")
            flow.click("Larger · ]")
            flow.capture("shape-palette")

            flow.click("Live")
            flow.call("npc/resume")
            wait_for(lambda: flow.has_widget("Pouch · I"), 40)
            time.sleep(1.5)
            flow.capture("live-hud")
            # Pause is one control on the bar; paused is its lit state and the
            # "Paused" marker under the bar. The same control resumes.
            flow.click("Pause")
            wait_for(lambda: "Paused" in [str(row["text"]) for row in flow.ui()[0]])
            flow.capture("live-paused")
            flow.click("Pause")
            flow.click("3×")
            time.sleep(0.5)

            flow.click("Menu")
            flow.capture("pause-menu")
            flow.click("Settings")
            flow.capture("pause-settings")
            flow.click("Back")
            flow.click("Quit game")
            child.wait(timeout=40)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("phase4a_failure", error=str(error))
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
