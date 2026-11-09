"""Bounded native colony rehearsal using widgets and recorded commands, no provider.

Run after cargo build --locked -p actor_testbed -p game_menu. Screenshots require
visual inspection; debug widget events are not physical keyboard/mouse testing.
"""
import json
import argparse
from pathlib import Path
import socket
import subprocess
import time
import uuid
from capture_native import ROOT, TEXT, PARENT, WINDOW, NODE, query, rpc, scalar
from capture_native_homestead import Rehearsal, file_hash


def wait_for(predicate, seconds=60):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        try:
            result = predicate()
            if result:
                return result
        except (OSError, RuntimeError, KeyError) as error:
            last = error
        time.sleep(0.15)
    raise RuntimeError(f"Native readiness timed out: {last}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/actor_testbed.exe")
    binary = parser.parse_args().binary.resolve()
    if not binary.is_file():
        parser.error("--binary must name a built actor_testbed executable")
    run = ROOT / "target" / ("playtest-island-colony-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    report = {"ok": False, "input": "native BRP widget activation and recorded content commands",
              "provider": False, "screenshots": {}, "checks": [],
              "binary": str(binary), "binary_sha256": file_hash(binary)}
    child = None
    resize_helper = None
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"

    def call(method, params=None):
        if child.poll() is not None:
            raise RuntimeError("Owned native window exited; inspect native.log")
        return rpc(url, method, params)

    def click(label, optional=False):
        widgets = {int(row["entity"]) for row in query(url, with_types=["bevy_ui_widgets::button::Button"])}
        disabled = {int(row["entity"]) for row in query(url, with_types=["bevy_ui::interaction_states::InteractionDisabled"])}
        nodes = {int(row["entity"]): row["components"][NODE] for row in query(url, [NODE])}
        lineage = {int(row["entity"]): int(scalar(row["components"][PARENT])) for row in query(url, [PARENT])}
        def visible(entity):
            while entity:
                if nodes.get(entity, {}).get("display") == "None":
                    return False
                entity = lineage.get(entity)
            return True
        parents = [int(scalar(row["components"][PARENT])) for row in query(url, [TEXT, PARENT])
                   if scalar(row["components"][TEXT]) == label
                   and int(scalar(row["components"][PARENT])) in widgets
                   and int(scalar(row["components"][PARENT])) not in disabled
                   and visible(int(row["entity"]))]
        if not parents and optional:
            return False
        if len(parents) != 1:
            raise RuntimeError(f"Expected one {label!r} widget, got {len(parents)}")
        call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": parents[0]}})
        time.sleep(0.3)
        return True

    def step(ticks):
        call("npc/step", {"ticks": ticks})
        time.sleep(max(0.4, ticks / 1000))

    def capture(label):
        for width, height in [(1280, 720), (1024, 768)]:
            resize_helper.resize((width, height))
            time.sleep(0.65)
            name = f"{label}-{width}x{height}"
            path = Path(call("npc/screenshot", {"path": "shots/" + name})["path"])
            def complete():
                if not path.exists() or path.stat().st_size < 5000:
                    return False
                with path.open("rb") as png:
                    png.seek(-12, 2)
                    return png.read(12)[4:8] == b"IEND"
            wait_for(complete, 30)
            report["screenshots"][name] = str(path)
            print(f"captured {name}", flush=True)
        resize_helper.resize((1280, 720))

    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary),
                "--scenario", "island_colony", "--island-mode", "inspector", "--island-save", str(run / "island.json"),
                "--speed", "0", "--remote", "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run)],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            wait_for(lambda: any(scalar(row["components"][TEXT]) == "Island Village" for row in query(url, [TEXT])), 120)
            resize_helper = Rehearsal(run, child, url, report, time.monotonic())
            resize_helper.window = int(query(url, with_types=["bevy_window::window::PrimaryWindow"])[0]["entity"])
            step(2)
            capture("inspector")
            for _ in range(3):
                if not click("Scroll down", optional=True):
                    break
            capture("inspector-lower")
            for _ in range(3):
                if not click("Scroll up", optional=True):
                    break
            step(18000)
            capture("evening")
            click("Keep 3 days of food")
            step(2)
            wait_for(lambda: any("Household food reserve: 3 days" in str(scalar(row["components"][TEXT])) for row in query(url, [TEXT])))
            report["checks"].append("native food reserve policy applied")
            click("Residents")
            names = [scalar(row["components"][TEXT]) for row in query(url, [TEXT])]
            ada = next(text for text in names if isinstance(text, str) and text.startswith("Ada · farmer ·"))
            click(ada)
            capture("resident")
            click("Terraform")
            capture("editor")
            call("npc/command", {"cmd": {"type": "Terraform", "x": 12, "z": 16, "radius": 1, "height_mm": 1000, "surface": "stone"}})
            step(2)
            capture("editor-terrain")
            report["checks"].append("recorded terrain command applied")
            click("Play character")
            step(4)
            capture("character")
            # Return through the actual ledger launcher; character life is not reset.
            click("Colony & landscape [F11]")
            click("Inspect")
            step(2)
            capture("returned-inspector")
            report["checks"].append("character arrived and released native control")
            call("app/quit", {"pid": child.pid})
            child.wait(timeout=30)
            if child.returncode != 0:
                raise RuntimeError(f"Native host exited {child.returncode}")
            saved = json.loads((run / "island.json").read_text(encoding="utf-8"))
            report["saved_tick"] = saved["tick"]
            report["terrain_revision"] = saved["resources"]["island_colony_terrain_v1"]["revision"]
            if report["terrain_revision"] != 1:
                raise RuntimeError("Edited terrain was not saved")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(run, flush=True)


if __name__ == "__main__":
    main()
