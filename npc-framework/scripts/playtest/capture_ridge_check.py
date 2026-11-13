"""One targeted native terrain-shading/editor check after the final rebuild.

Uses a fresh disposable spectator world, native widget events and bounded clock
steps. Six PNGs cover Ridge, Harbour's slope and editor at both supported sizes.
No player journey, material grant, teleport or physical-input claim is made.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import uuid

from capture_living_world import LivingFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import PRIMARY, wait_for


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the targeted check")
    run = ROOT / "target" / ("playtest-ridge-check-" + uuid.uuid4().hex[:10])
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
        "scope": "targeted gentle-riser shading and editor navigation-overlay confirmation",
        "input": "native BRP widgets and bounded operator clock steps",
        "physical_input_tested": False, "provider": False, "grants": 0, "teleports": 0,
        "screenshots": {}, "checks": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = LivingFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 30*60
            flow.tick_budget = 24000
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            flow.advance(6500)
            flow.region("Stone Ridge")
            flow.capture("ridge-daylight")
            flow.region("Harbour")
            flow.capture("harbour-slope")
            flow.retained_homes("targeted-spectator")
            flow.click("Shape")
            flow.capture("editor-navigation-overlay")
            state = flow.state()
            if not any("island_awaiting_arrival_v1" in e.get("components", {}) for e in state["entities"]):
                raise RuntimeError("Targeted spectator inspection consumed the unarrived visitor")
            report["tick"] = state["tick"]
            report["checks"] = ["One native window retained", "Unarrived visitor preserved",
                "All 42 authored home visual roots retained", "Six native PNGs captured at both sizes"]
            flow.click("Menu")
            flow.click("Quit game")
            child.wait(timeout=30)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("targeted_ridge_failure", error=str(error))
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
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(run, flush=True)


if __name__ == "__main__":
    main()
