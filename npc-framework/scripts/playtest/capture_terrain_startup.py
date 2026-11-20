"""Native regional readiness evidence, with an optional recorded terrain lifecycle.

Build separately. Startup captures begin as soon as BRP responds, without an
asset wait or viewport resize. SceneReadiness, widgets and completed PNGs are the
readiness evidence; wall time only bounds polling. --lifecycle enters spectator
mode through widgets and records radius-zero land/water/land operator brushes.
Owns disposable files/process. No physical-input or natural-progression claim.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import uuid

from capture_native import ROOT, TEXT, PARENT, query, rpc, scalar
from capture_native_homestead import DISABLED, file_hash
from capture_player_flow import PlayerFlow, PRIMARY

NAME = "bevy_ecs::name::Name"
TRANSFORM = "bevy_transform::components::transform::Transform"
CAMERA = "bevy_camera::camera::Camera"
VISIBILITY = "bevy_camera::visibility::ViewVisibility"
BACKGROUND = "bevy_ui::ui_node::BackgroundColor"
MESH = "bevy_mesh::components::Mesh3d"
READINESS = "island_view::scene_readiness::SceneReadiness"
ENTRY_LABELS = {"Continue", "New game", "Start on the island", "Oversee the village"}


def name(value):
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        return value.get("name", str(value))
    return str(value)


def alpha(value):
    if isinstance(value, dict):
        if "alpha" in value:
            return float(value["alpha"])
        children = value.values()
    elif isinstance(value, list):
        children = value
    else:
        return None
    return next((found for child in children if (found := alpha(child)) is not None), None)


def readiness(url):
    state = rpc(url, "world.get_resources", {"resource": READINESS})["value"]
    if state.get("error"):
        raise RuntimeError("Native scene readiness error: " + state["error"])
    if state["ready"] and not (state["expected_meshes"] > 0
            and state["prepared_meshes"] == state["expected_meshes"]
            and state["pending_pipelines"] == 0 and state["rendered_frames"] > 0):
        raise RuntimeError("Scene declared readiness without uploaded terrain and compiled pipelines")
    return state


def shell_state(url):
    disabled = {int(row["entity"]) for row in query(url, with_types=[DISABLED])}
    entries = []
    for row in query(url, [TEXT, PARENT]):
        text = scalar(row["components"].get(TEXT))
        if text in ENTRY_LABELS and PARENT in row["components"]:
            parent = int(scalar(row["components"][PARENT]))
            entries.append({"label": text, "entity": parent, "enabled": parent not in disabled})
    shells = []
    for row in query(url, [NAME, BACKGROUND]):
        if name(row["components"].get(NAME)) == "Island game front end":
            color = row["components"].get(BACKGROUND)
            shells.append({"entity": int(row["entity"]), "alpha": alpha(color), "color": color})
    return {"entries": entries, "shells": shells}


def presentation_state(url, save):
    before_ui = readiness(url)
    ui = shell_state(url)
    tick = rpc(url, "world/state")["tick"]
    after_ui = readiness(url)
    return {"readiness_before_ui": before_ui, "readiness": after_ui,
        "ui": ui, "tick": tick, "save_exists": save.exists()}


def terrain_views(url):
    # Empty chunks intentionally have no Mesh3d/ViewVisibility. Query their
    # persistent names independently so gaining a mesh cannot look like a new
    # chunk identity in this diagnostic.
    chunks = [row for row in query(url, [NAME])
        if name(row["components"].get(NAME)).startswith("regional terrain ")]
    meshes = [row for row in query(url, [NAME], with_types=[MESH])
        if name(row["components"].get(NAME)).startswith("regional terrain ")]
    return {"chunks": chunks, "mesh_names": sorted(name(row["components"][NAME]) for row in meshes)}


def check_process(child, deadline):
    if child.poll() is not None:
        raise RuntimeError("Owned native host exited; inspect native.log")
    if time.monotonic() >= deadline:
        raise RuntimeError("Native readiness/lifecycle exceeded its bounded wall-time budget")


def poll(predicate, child, deadline):
    while True:
        check_process(child, deadline)
        result = predicate()
        if result:
            return result
        time.sleep(0.04)  # Polling cadence, never a readiness condition.


def screenshot(url, run, save, label, child, deadline):
    before = presentation_state(url, save)
    path = Path(rpc(url, "npc/screenshot", {"path": "shots/" + label})["path"])
    if not path.resolve().is_relative_to(run.resolve()):
        raise RuntimeError("Screenshot path escaped the owned diagnostic folder")

    def complete():
        try:
            with path.open("rb") as source:
                header = source.read(24)
                source.seek(-12, 2)
                tail = source.read(12)
            if header[:8] == b"\x89PNG\r\n\x1a\n" and tail[4:8] == b"IEND":
                return list(struct.unpack(">II", header[16:24]))
        except (OSError, ValueError):
            pass
        return None

    size = poll(complete, child, min(deadline, time.monotonic() + 30))
    after = presentation_state(url, save)
    return {"screenshot": str(path), "pixels": size, "before": before, "after": after}


def title_frame(sample):
    before, after = sample["before"], sample["after"]
    for state in (before, after):
        if state["tick"] != 0 or state["save_exists"]:
            raise RuntimeError("Title advanced the simulation or created a save")
    stable_loading = all(not state[key]["ready"] for state in (before, after)
        for key in ("readiness_before_ui", "readiness"))
    if stable_loading:
        for state in (before, after):
            shells = state["ui"]["shells"]
            if len(shells) != 1 or shells[0]["alpha"] != 1.0:
                raise RuntimeError("Loading title exposed incomplete scenery")
            if any(row["enabled"] for row in state["ui"]["entries"]):
                raise RuntimeError("Loading title enabled a world-entry widget")
    # BRP reads straddle frames. A revealed shell requires both renderer and UI.
    settled = after["readiness"]["ready"] and len(after["ui"]["shells"]) == 1
    settled = settled and abs((after["ui"]["shells"][0]["alpha"] or 0) - 0.28) < 0.001
    settled = settled and any(row["label"] == "New game" and row["enabled"]
        for row in after["ui"]["entries"])
    return stable_loading, settled


def water_cell_in_empty_chunk(terrain, state):
    width, height = terrain["width"], terrain["height"]
    min_x, min_z = -(width // 2), -(height // 2)
    cells = terrain["cells"]

    def cell(x, z):
        if min_x <= x < min_x + width and min_z <= z < min_z + height:
            return cells[(z - min_z) * width + x - min_x]
        return None

    def water(value):
        return str(value["surface"]).lower() == "water"

    fixed = {"island_place", "island_resource_patch", "island_household_v1", "island_homestead_plot_v1"}
    occupied = {"island_villager", "island_wildlife", "island_awaiting_arrival_v1"}
    protected, blockers = [], []
    for entity in state["entities"]:
        components = entity.get("components", {})
        if "pos" not in components:
            continue
        position = PlayerFlow.position(components["pos"])
        if fixed.intersection(components):
            protected.append((position, 5000))
        elif occupied.intersection(components):
            protected.append((position, 1200))
        if "grid_blocker" in components:
            blockers.append((position, components["grid_blocker"]))

    def clear(x, z):
        if any((x * 1000 + 500 - p[0]) ** 2 + (z * 1000 + 500 - p[1]) ** 2 <= radius ** 2
                for p, radius in protected):
            return False
        return not any(abs(x * 1000 + 500 - p[0]) <= shape["half_x_mm"] + 500
            and abs(z * 1000 + 500 - p[1]) <= shape["half_z_mm"] + 500 for p, shape in blockers)

    empty_chunks, dry_chunks = set(), 0
    for chunk_z in range(min_z, min_z + height, 16):
        for chunk_x in range(min_x, min_x + width, 16):
            points = [(x, z) for z in range(chunk_z, min(chunk_z + 16, min_z + height))
                for x in range(chunk_x, min(chunk_x + 16, min_x + width))]
            if any(not water(cell(x, z)) for x, z in points):
                dry_chunks += 1
            else:
                empty_chunks.add((chunk_x, chunk_z))

    def chunk_of(x, z):
        return (min_x + (x - min_x) // 16 * 16, min_z + (z - min_z) // 16 * 16)

    choices = []
    for chunk_x, chunk_z in sorted(empty_chunks):
        for z in range(chunk_z, min(chunk_z + 16, min_z + height)):
            for x in range(chunk_x, min(chunk_x + 16, min_x + width)):
                if x not in (chunk_x, chunk_x + 15) and z not in (chunk_z, chunk_z + 15):
                    continue
                # The generated coast has no empty chunk directly adjoining
                # land. At most four explicitly recorded connector cells in
                # already nonempty chunks establish a connected test edge.
                for dx in range(-5, 6):
                    for dz in range(-5 + abs(dx), 6 - abs(dx)):
                        source = cell(x + dx, z + dz)
                        if source is None or water(source) or dx == dz == 0:
                            continue
                        height_mm = source["height_mm"]
                        for axes in ((0, 1), (1, 0)):
                            at, path = [x + dx, z + dz], []
                            for axis in axes:
                                target = (x, z)[axis]
                                while at[axis] != target:
                                    at[axis] += 1 if target > at[axis] else -1
                                    path.append(tuple(at))
                            if len(path) > 5 or any(chunk_of(a, b) in empty_chunks for a, b in path[:-1]):
                                continue
                            if any(not water(cell(a, b)) or not clear(a, b) for a, b in path):
                                continue
                            if any(abs(neighbor["height_mm"] - height_mm) > 1500
                                    for a, b in path for ox, oz in [(0, -1), (-1, 0), (1, 0), (0, 1)]
                                    if (neighbor := cell(a + ox, b + oz)) is not None and not water(neighbor)):
                                continue
                            choices.append((len(path), x * x + z * z, x, z, chunk_x, chunk_z, height_mm, path[:-1]))
    if not choices:
        raise RuntimeError("No safe water-chunk edge reachable with at most four recorded connector cells")
    _, _, x, z, chunk_x, chunk_z, dry_height, setup = min(choices)
    return {"x": x, "z": z, "chunk_name": f"regional terrain {chunk_x},{chunk_z}",
        "dry_height_mm": dry_height, "water_height_mm": cell(x, z)["height_mm"],
        "setup": [{"x": a, "z": b} for a, b in setup],
        "original_dry_chunks": dry_chunks, "index": (z - min_z) * width + x - min_x}


def lifecycle(flow, save, report):
    flow.click("New game")
    flow.click("Oversee the village")
    poll(lambda: flow.has_widget("Live"), flow.child, flow.deadline)
    flow.call("npc/pause")
    flow.click("Menu")
    # Keep the pause menu open during operator stepping/saving to hold the clock.
    flow.click("Save game")
    baseline = json.loads(save.read_text(encoding="utf-8"))
    terrain = baseline["resources"]["living_world_terrain_v1"]
    chosen = water_cell_in_empty_chunk(terrain, flow.state())
    expected_tick = flow.state()["tick"]
    initial_revision = terrain["revision"]
    rows = []
    report["lifecycle"] = {"scope": "operator terrain lifecycle; no physical brush input",
        "chosen": chosen, "initial_tick": expected_tick, "initial_revision": initial_revision, "phases": rows}

    def expected_views(present):
        views = terrain_views(flow.url)
        if len(views["mesh_names"]) != chosen["original_dry_chunks"] + int(present):
            return None
        if (chosen["chunk_name"] in views["mesh_names"]) != present:
            return None
        return views

    initial_views = poll(lambda: expected_views(False), flow.child, flow.deadline)
    report["lifecycle"]["initial_chunks"] = len(initial_views["chunks"])
    initial_identities = sorted((name(row["components"][NAME]), int(row["entity"]))
        for row in initial_views["chunks"])
    report["lifecycle"]["initial_chunk_identities"] = initial_identities
    phases = [(f"connector-{i + 1}", "recorded setup", point["x"], point["z"],
        "grass", chosen["dry_height_mm"], False) for i, point in enumerate(chosen["setup"])]
    phases.extend((label, "chunk lifecycle", chosen["x"], chosen["z"], surface, height_mm, present)
        for label, surface, height_mm, present in [
            ("land-first", "grass", chosen["dry_height_mm"], True),
            ("water-again", "water", chosen["water_height_mm"], False),
            ("land-again", "grass", chosen["dry_height_mm"], True)])
    report["lifecycle"]["recorded_setup_commands"] = len(chosen["setup"])
    for index, (label, phase, x, z, surface, height_mm, present) in enumerate(phases, start=1):
        command = {"type": "Terraform", "x": x, "z": z,
            "radius": 0, "height_mm": height_mm, "surface": surface}
        receipt = flow.call("npc/command", {"cmd": command})
        flow.call("npc/step", {"ticks": 1})
        expected_tick += 1

        def stepped():
            tick = flow.state()["tick"]
            if tick > expected_tick:
                raise RuntimeError("Operator lifecycle advanced outside its explicit one-tick steps")
            return tick == expected_tick

        poll(stepped, flow.child, flow.deadline)
        views = poll(lambda: expected_views(present), flow.child, min(flow.deadline, time.monotonic() + 20))
        identities = sorted((name(row["components"][NAME]), int(row["entity"]))
            for row in views["chunks"])
        if identities != initial_identities:
            report["lifecycle"]["identity_mismatch"] = {
                "removed": sorted(set(initial_identities) - set(identities)),
                "added": sorted(set(identities) - set(initial_identities)),
                "command": command, "receipt": receipt}
            raise RuntimeError("Terraform lost a persistent terrain chunk identity")
        flow.click("Save game")
        snapshot = json.loads(save.read_text(encoding="utf-8"))
        actual = snapshot["resources"]["living_world_terrain_v1"]
        cell_index = (z + actual["height"] // 2) * actual["width"] + x + actual["width"] // 2
        cell = actual["cells"][cell_index]
        if actual["revision"] != initial_revision + index or snapshot["tick"] != expected_tick:
            raise RuntimeError("Recorded brush did not save exactly one terrain revision at its explicit tick")
        if str(cell["surface"]).lower() != surface or cell["height_mm"] != height_mm:
            raise RuntimeError("Recorded brush did not persist the requested cell")
        sample = screenshot(flow.url, flow.run, save, label, flow.child, flow.deadline)
        sample.update(phase=phase, command=command, receipt=receipt, revision=actual["revision"],
            mesh_count=len(views["mesh_names"]), chunk_count=len(views["chunks"]), tick=expected_tick)
        rows.append(sample)
    report["lifecycle"]["ok"] = True
    flow.click("Quit game")
    flow.child.wait(timeout=30)
    if flow.child.returncode != 0:
        raise RuntimeError(f"Native lifecycle quit exited with {flow.child.returncode}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--timeout", type=float, default=120, help="startup timeout; never a readiness condition")
    parser.add_argument("--samples-after-ready", type=int, default=3)
    parser.add_argument("--lifecycle", action="store_true", help="also exercise recorded land/water/land mesh creation and removal")
    parser.add_argument("--confirm-layouts", action="store_true", help="after readiness only, capture 1280x720 and 1024x768 title layouts")
    args = parser.parse_args()
    if args.timeout <= 0 or args.samples_after_ready < 1:
        parser.error("Timeout and samples-after-ready must be positive")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before native QA")
    run = ROOT / "target" / ("terrain-startup-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    settings, save = run / "settings.json", run / "living-world.world.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "worlds_dir": str(run / "rules"), "fog_of_war": True}), encoding="utf-8")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    env["BEVY_ASSET_ROOT"] = str(ROOT)
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "operation": "native BRP reads/screenshots; startup has no viewport resize; optional post-ready layout/lifecycle checks",
        "gpu_readiness_instrumented": True, "physical_input_tested": False,
        "lifecycle_requested": args.lifecycle, "layout_confirmation_requested": args.confirm_layouts,
        "samples": []}
    child = flow = None
    start = time.monotonic()
    deadline = start + args.timeout
    print(run, flush=True)
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=env,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            last_startup_error = None
            while True:
                check_process(child, deadline)
                try:
                    initial = readiness(url)
                    break
                except OSError as error:
                    last_startup_error = str(error)
                    time.sleep(0.04)
            report["first_brp_seconds"] = time.monotonic() - start
            report["initial_readiness"] = initial
            report["last_connection_wait"] = last_startup_error
            stable_ready = stable_loading = 0
            while stable_ready < args.samples_after_ready:
                check_process(child, deadline)
                sample = screenshot(url, run, save, f"start-{len(report['samples']):02}", child, deadline)
                sample["seconds"] = time.monotonic() - start
                sample["cameras"] = query(url, [CAMERA, TRANSFORM])
                sample["terrain"] = terrain_views(url)
                report["samples"].append(sample)
                loading, settled = title_frame(sample)
                if settled and len(sample["terrain"]["mesh_names"]) != sample["after"]["readiness"]["expected_meshes"]:
                    raise RuntimeError("Ready terrain upload count does not match actual rendered chunk meshes")
                stable_loading += int(loading)
                stable_ready = stable_ready + 1 if settled else 0
            report["startup_checks"] = {"stable_loading_captures": stable_loading,
                "settled_ready_captures": stable_ready, "tick_zero": True,
                "save_absent": not save.exists(), "all_terrain_uploaded": True,
                "pipelines_pending_at_ready": 0, "loading_gate_observed": stable_loading > 0}
            if args.lifecycle or args.confirm_layouts:
                flow = PlayerFlow(run, child, url, report, start)
                flow.deadline = time.monotonic() + 180
                flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            if args.confirm_layouts:
                report["post_ready_layouts"] = []
                for size in [(1280, 720), (1024, 768)]:
                    geometry = flow.resize(size)
                    sample = screenshot(url, run, save, f"ready-layout-{size[0]}x{size[1]}", child, flow.deadline)
                    _, settled = title_frame(sample)
                    if not settled or sample["pixels"] != list(size):
                        raise RuntimeError("Post-ready title layout did not match settled requested dimensions")
                    sample["geometry"] = geometry
                    report["post_ready_layouts"].append(sample)
            if args.lifecycle:
                lifecycle(flow, save, report)
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        if flow is not None:
            report["trace"] = flow.trace
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_process_stopped"] = child is None or child.poll() is not None
        lines = (run / "native.log").read_text(encoding="utf-8", errors="replace").splitlines()
        empty_mesh = [line for line in lines if "contains no vertices" in line]
        errors = [line for line in lines if "ERROR" in line or "panicked at" in line]
        report["logs"] = {"warnings": sum(" WARN " in line for line in lines),
            "errors": sum(" ERROR " in line for line in lines),
            "panics": sum("panicked at" in line for line in lines),
            "empty_mesh_warnings": len(empty_mesh), "error_lines": errors[-20:],
            "readiness_transitions": [line for line in lines if "regional scene presentation progress" in line]}
        if empty_mesh or errors:
            report["ok"] = False
            report.setdefault("error", "Native log contains empty-mesh warnings or renderer/runtime errors")
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(run, flush=True)
    if not report["ok"]:
        raise SystemExit(report.get("error", "Native diagnostic failed"))


if __name__ == "__main__":
    main()
