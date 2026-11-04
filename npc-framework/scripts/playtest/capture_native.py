"""Disposable native evidence, using BRP widget events and recorded operator commands.

Run from npc-framework: python scripts/playtest/capture_native.py [--baseline | --confirm-polish]
Requires built release actor_testbed and Python 3 standard library. Never opens a
private save. This is protocol/widget QA, not physical-input or screen-reader QA.
"""
import argparse
import json
import math
import os
from pathlib import Path
import platform
import shutil
import socket
import subprocess
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]
TEXT = "bevy_ui::widget::text::Text"
PARENT = "bevy_ecs::hierarchy::ChildOf"
WINDOW = "bevy_window::window::Window"
NODE = "bevy_ui::ui_node::Node"
POLISH_CAPTURES = frozenset({
    "normal-unlinked", "notebook-diary", "herp-speech",
    "herp-resident-dialogue", "herp-resident-dialogue-480", "recovered-diary",
})


def rpc(url, method, params=None):
    request = urllib.request.Request(url, json.dumps({
        "jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}
    }).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=30) as response:
        value = json.load(response)
    if "error" in value:
        raise RuntimeError(f"{method}: {value['error']}")
    return value["result"]


def query(url, components=(), with_types=()):
    return rpc(url, "world.query", {"data": {"components": list(components)},
        "filter": {"with": list(with_types)}, "strict": True})


def scalar(value):
    while isinstance(value, list) and len(value) == 1:
        value = value[0]
    return value


def click(url, label):
    parents = [int(scalar(row["components"][PARENT]))
        for row in query(url, [TEXT, PARENT])
        if scalar(row["components"][TEXT]) == label]
    if len(parents) != 1:
        raise RuntimeError(f"Expected one {label} widget, found {len(parents)}")
    rpc(url, "world.trigger_event", {"event": "bevy_ui_widgets::Activate",
        "value": {"entity": parents[0]}})
    time.sleep(0.6)


def require_visible_text(url, label):
    """Check inherited UI display, not just an always-present hidden text entity."""
    nodes = {int(row["entity"]): row["components"][NODE]
        for row in query(url, [NODE])}
    parents = {int(row["entity"]): int(scalar(row["components"][PARENT]))
        for row in query(url, [PARENT])}
    candidates = [int(row["entity"]) for row in query(url, [TEXT])
        if scalar(row["components"][TEXT]) == label]
    for entity in candidates:
        seen = set()
        while entity not in seen:
            seen.add(entity)
            if nodes.get(entity, {}).get("display") == "None":
                break
            if entity not in parents:
                return
            entity = parents[entity]
    raise RuntimeError(f"Expected visible {label} UI; hidden text is not capture evidence")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", action="store_true", help="measure pre-change binary only")
    parser.add_argument("--confirm-polish", action="store_true", help="run the complete fixture but capture only six changed states; not a full visual matrix")
    parser.add_argument("--browser-check", action="store_true", help="hold a live dual-view fixture for up to ten minutes; create browser-done in its run folder to continue")
    args = parser.parse_args()
    if args.baseline and args.confirm_polish:
        parser.error("--confirm-polish cannot be combined with --baseline")
    run = ROOT / "target" / ("playtest-native-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    report = {"ok": False, "baseline": args.baseline,
        "polish_confirmation": args.confirm_polish,
        "capture_scope": "baseline" if args.baseline else "selective_polish_confirmation" if args.confirm_polish else "full_matrix",
        "screenshots": {}, "skipped_capture_states": [],
        "input": "BRP widgets, view-only window sizing, recorded operator commands",
        "machine": {"os": platform.platform(), "processor": platform.processor()},
        "fixture": "new disposable explorer; forced death only after economic/perf captures"}
    child = browser_server = None
    projection = run / "world-state.json"
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(ROOT / "target/release/actor_testbed.exe"),
                "--explorer-home", str(run / "agents"), "--actors-dir", str(run / "actors"),
                "--worlds-dir", str(run / "worlds"), "--agent", "Herp",
                "--world", "Island", "--preset", "human", "--world-preset", "island_village",
                "--paused", "--fog", "on", "--remote-port", str(port),
                "--world-state-out", str(projection), "--world-state-interval-ms", "100",
                "--no-render-remote", "--out-dir", str(run / "npc-out")],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            deadline = time.monotonic() + 120
            last_ready_error = "no UI text entities yet"
            while time.monotonic() < deadline:
                if child.poll() is not None:
                    raise RuntimeError("Owned native host exited during startup; inspect native.log")
                try:
                    startup_text = {scalar(row["components"][TEXT]) for row in query(url, [TEXT])}
                    if startup_text.intersection({"Return to world", "OK"}):
                        break
                except (OSError, RuntimeError) as error:
                    last_ready_error = str(error)
                time.sleep(0.3)
            else:
                raise RuntimeError("Native BRP startup timeout: " + last_ready_error)
            labels = {scalar(row["components"][TEXT]) for row in query(url, [TEXT])}
            dismiss = next((label for label in ["Return to world", "OK"] if label in labels), None)
            if dismiss is None:
                raise RuntimeError("No expected explorer onboarding dismiss control")
            click(url, dismiss)  # Never select a provider during an offline fixture.
            report["onboarding_dismissed_with"] = dismiss
            rpc(url, "npc/pause")  # The operator owns the fixture clock.
            saved = json.loads((run / "agents/herp/island.world.json").read_text(encoding="utf-8-sig"))
            report["seed"] = saved["header"]["seed"]
            report["world_preset"] = "island_village"
            report["projection_interval_ms"] = 100
            window = int(query(url, with_types=["bevy_window::window::PrimaryWindow"])[0]["entity"])
            resolution = next(row["components"][WINDOW]["resolution"] for row in query(url, [WINDOW])
                if int(row["entity"]) == window)
            original = dict(resolution)
            scale = resolution.get("scale_factor_override") or resolution["scale_factor"]
            report["original_resolution"] = resolution

            def mutate(path, value):
                rpc(url, "world.mutate_components", {"entity": window,
                    "component": WINDOW, "path": "resolution." + path, "value": value})
                time.sleep(0.7)

            def capture(label):
                if args.confirm_polish and label not in POLISH_CAPTURES:
                    report["skipped_capture_states"].append(label)
                    return
                time.sleep(0.7)
                path = Path(rpc(url, "npc/screenshot", {"path": "shots/" + label})["path"])
                end = time.monotonic() + 30
                while not path.is_file() and time.monotonic() < end:
                    time.sleep(0.1)
                if not path.is_file():
                    raise RuntimeError("Screenshot did not arrive")
                report["screenshots"][label] = str(path.relative_to(ROOT))

            def key(code):
                event = {"key_code": code, "logical_key": code, "state": "Pressed",
                    "text": None, "repeat": False, "window": window}
                rpc(url, "world.write_message", {"message": "bevy_input::keyboard::KeyboardInput", "value": event})
                time.sleep(0.2)
                event["state"] = "Released"
                rpc(url, "world.write_message", {"message": "bevy_input::keyboard::KeyboardInput", "value": event})
                time.sleep(0.5)

            capture("actual-viewport-paused")
            mutate("physical_width", round(1280 * scale))
            mutate("physical_height", round(720 * scale))
            capture("desktop-paused-unlinked")
            credentials = json.loads((run / "agents/herp/agent.json").read_text(encoding="utf-8-sig"))
            registration = rpc(url, "agent/register", {"agent_id": credentials["agent_id"],
                "token": credentials["token"]})
            auth = {"session": registration["session"]}
            rpc(url, "agent/enter_world", auth)
            queued = rpc(url, "agent/take_control", auth)
            report["queued_control"] = {key: queued.get(key) for key in ("pending", "in_control", "tick")}
            if queued.get("in_control") is True:
                raise RuntimeError("Paused control unexpectedly applied")
            capture("queued-control")
            rpc(url, "npc/resume")  # Operator, not actor, owns the clock.
            time.sleep(1)
            assert rpc(url, "agent/take_control", auth)["in_control"]
            rpc(url, "npc/perf/reset")
            begin = time.monotonic()
            tick = rpc(url, "world/state")["tick"]
            time.sleep(12)
            end_tick = rpc(url, "world/state")["tick"]
            elapsed = time.monotonic() - begin
            report["performance"] = {"elapsed_seconds": elapsed,
                "ticks": end_tick - tick, "achieved_tps": (end_tick - tick) / elapsed,
                "speed": "1x, configured 20 TPS"}
            perf = rpc(url, "npc/perf", {"tree": True})
            report["performance"]["runtime"] = perf.get("runtime")
            # Profiler data is operator evidence, not passed to the actor.
            (run / "perf.json").write_text(json.dumps(perf, indent=2), encoding="utf-8")
            capture("normal-unlinked")
            rpc(url, "npc/pause")
            if args.baseline:
                report["ok"] = True
                return
            key("F8")
            capture("fog-off")
            key("F8")
            key("F7")
            try:
                require_visible_text(url, "F7 NOTES")
            except RuntimeError:
                # Preserve the failed assertion while collecting only view state.
                # Do not resend a key or silently turn a later frame into a pass.
                diagnostic = {"window_focused": next(
                    row["components"][WINDOW].get("focused") for row in query(url, [WINDOW])
                    if int(row["entity"]) == window)}
                try:
                    require_visible_text(url, "F7 NOTES")
                    diagnostic["visible_on_read_only_recheck"] = True
                except RuntimeError:
                    diagnostic["visible_on_read_only_recheck"] = False
                report["notebook_failure_view_state"] = diagnostic
                raise
            capture("notebook-diary")
            key("F7")
            click(url, "AI scheduler")
            capture("scheduler")
            mutate("physical_width", round(480 * scale))
            capture("scheduler-480")
            click(url, "Close")
            capture("normal-480")
            mutate("physical_width", 1600)
            mutate("physical_height", 900)
            mutate("scale_factor_override", 1.25)
            capture("dpi-125")
            mutate("scale_factor_override", original.get("scale_factor_override"))
            mutate("physical_width", round(1280 * scale))
            mutate("physical_height", round(720 * scale))
            observed = rpc(url, "agent/observe", auth)
            actor = observed["unit"]
            landing_pos = list(observed["sections"]["self"]["pos"])
            rejected = rpc(url, "agent/act", {**auth, "verb": "eat"})
            assert rejected["accepted"] is False
            capture("action-refusal")
            # A perceived worksite supplies a destination; operator advances the
            # fixture after the registered actor chooses a normal movement order.
            sensed = observed["sections"]["perception"]["sensed"]
            destination = next(p for p in sensed if p["kind"] == "workstation")
            moved = rpc(url, "agent/act", {**auth, "steps": [
                {"verb": "order", "args": {"order": {"type": "Move", "pos": destination["pos"]}}},
                {"verb": "execute_orders"}]})
            assert moved["accepted"] is True
            rpc(url, "npc/step", {"ticks": 2})
            capture("busy-walking")
            rpc(url, "npc/step", {"ticks": 700})
            observed = rpc(url, "agent/observe", auth)
            sensed = {p["id"]: p for p in observed["sections"]["perception"]["sensed"]}
            patch = next((a["target"] for a in observed["affordances"]
                if a["verb"] == "gather" and a.get("target") in sensed), None)
            if patch is not None:
                moved = rpc(url, "agent/act", {**auth, "steps": [
                    {"verb": "order", "args": {"order": {"type": "Move", "pos": sensed[patch]["pos"]}}},
                    {"verb": "execute_orders"}]})
                assert moved["accepted"] is True
                rpc(url, "npc/step", {"ticks": 900})
                gathering = rpc(url, "agent/act", {**auth, "verb": "gather", "target": patch})
                assert gathering["accepted"] is True
                rpc(url, "npc/step", {"ticks": 1})
                capture("busy-gathering")
                rpc(url, "npc/step", {"ticks": 700})
                capture("gather-result")
            spoken = rpc(url, "agent/act", {**auth, "verb": "talk",
                "args": {"text": "Hello Island. I am Herp. What shall we make today?"}})
            assert spoken["accepted"] is True
            rpc(url, "npc/step", {"ticks": 1})
            capture("herp-speech")
            # Registered, perception-scoped dialogue: approach a remembered or
            # currently sensed resident by ordinary movement, re-observing a
            # moving target. No position/component edits or world-truth lookup.
            dialogue_text = (
                "Bonjour, île! Grüß dich, village. I am Herp; I am gathering wood, "
                "learning our daily trades, and looking for useful work. Shall we "
                "make bread, clean water, tools, and a good home together?"
            )
            assert 0 < len(dialogue_text) <= 240
            assert len(dialogue_text.encode("utf-8")) <= 1024
            addressed_target = None
            dialogue_attempts = 0
            dialogue_returned = False
            dialogue_scan_steps = 0
            dialogue_ticks = 0
            for dialogue_attempts in range(1, 25):
                observed = rpc(url, "agent/observe", auth)
                perception = observed["sections"]["perception"]
                known = {p["id"]: p for field in ("remembered", "sensed")
                    for p in perception.get(field, [])}
                talks = [a for a in observed["affordances"]
                    if a["verb"] == "talk" and a.get("target") in known]
                if not talks:
                    if dialogue_returned:
                        raise RuntimeError("No perceived resident after the bounded landing return and local scan")
                    # The actor remembers its own earlier position even after
                    # resource work outlasts the world's perception memory.
                    returned = rpc(url, "agent/act", {**auth, "steps": [
                        {"verb": "order", "args": {"order": {"type": "Move", "pos": landing_pos}}},
                        {"verb": "execute_orders"}]})
                    if returned.get("accepted") is not True:
                        raise RuntimeError("Registered return to the actor's own remembered landing was rejected")
                    rpc(url, "npc/step", {"ticks": 900})
                    dialogue_ticks += 900
                    dialogue_returned = True
                    # Small relative walks turn the actor through the cardinal
                    # directions; sensing still comes only from normal game tools.
                    for dx, dz in [(0, 1000), (1000, 0), (0, -1000), (-1000, 0)]:
                        scanned = rpc(url, "agent/observe", auth)
                        perceived = {p["id"] for field in ("remembered", "sensed")
                            for p in scanned["sections"]["perception"].get(field, [])}
                        if any(a["verb"] == "talk" and a.get("target") in perceived
                            for a in scanned["affordances"]):
                            break
                        here = scanned["sections"]["self"]["pos"]
                        turned = rpc(url, "agent/act", {**auth, "steps": [
                            {"verb": "order", "args": {"order": {"type": "Move", "pos": [here[0] + dx, here[1] + dz]}}},
                            {"verb": "execute_orders"}]})
                        if turned.get("accepted") is not True:
                            raise RuntimeError("Registered local perception scan movement was rejected")
                        rpc(url, "npc/step", {"ticks": 40})
                        dialogue_ticks += 40
                        dialogue_scan_steps += 1
                    continue
                own_pos = observed["sections"]["self"]["pos"]
                # Retain the chosen resident while remembered, avoiding target
                # switching whenever two workers cross at similar distances.
                selected = next((a for a in talks if a["target"] == addressed_target), None)
                if selected is None:
                    selected = min(talks, key=lambda opportunity: (
                        sum((a - b) ** 2 for a, b in zip(own_pos, known[opportunity["target"]]["pos"])),
                        opportunity["target"]))
                    addressed_target = selected["target"]
                if selected.get("ok") is True and addressed_target in perception.get("within_reach", []):
                    dialogue = rpc(url, "agent/act", {**auth, "verb": "talk", "target": addressed_target,
                        "args": {"text": dialogue_text}})
                    if dialogue.get("accepted") is True:
                        rpc(url, "npc/step", {"ticks": 1})
                        dialogue_ticks += 1
                        answered = rpc(url, "agent/observe", auth)
                        replies = answered["sections"]["village"].get("heard_speech", [])
                        if not any(line.get("speaker") == addressed_target
                            and line.get("tick", -1) >= observed["tick"] for line in replies):
                            raise RuntimeError("Addressed resident speech produced no recipient-scoped reply")
                        break
                position = known[addressed_target]["pos"]
                approach = rpc(url, "agent/act", {**auth, "steps": [
                    {"verb": "order", "args": {"order": {"type": "Move", "pos": position}}},
                    {"verb": "execute_orders"}]})
                if approach.get("accepted") is not True:
                    raise RuntimeError("Observed resident approach was rejected")
                # Re-observe more often near a moving person instead of waiting
                # a fixed interval after reaching their now-stale destination.
                distance2 = sum((a - b) ** 2 for a, b in zip(own_pos, position))
                approach_ticks = max(4, min(80, math.isqrt(distance2) // 140))
                rpc(url, "npc/step", {"ticks": approach_ticks})
                dialogue_ticks += approach_ticks
            else:
                raise RuntimeError("Resident dialogue approach exceeded the 24-attempt / 3000-tick fixture budget")
            assert dialogue_ticks <= 3000
            report["dialogue_fixture"] = {"addressed": True, "reply_heard": True,
                "approach_attempts": dialogue_attempts, "public_resident_id": addressed_target,
                "speech_characters": len(dialogue_text), "speech_utf8_bytes": len(dialogue_text.encode("utf-8")),
                "returned_to_own_landing": dialogue_returned, "local_scan_steps": dialogue_scan_steps,
                "sim_ticks_advanced": dialogue_ticks,
                "movement": "registered orders toward perceived/remembered positions; operator advances paused clock"}
            capture("herp-resident-dialogue")
            mutate("physical_width", round(480 * scale))
            capture("herp-resident-dialogue-480")
            mutate("physical_width", round(1280 * scale))
            if args.browser_check:
                viewer = ROOT.parent / "npc-world-viewer"
                program = shutil.which("node")
                if not program or not (viewer / "dist/index.html").is_file():
                    raise RuntimeError("browser-check requires Node and the built sibling viewer")
                with socket.socket() as listener:
                    listener.bind(("127.0.0.1", 0))
                    browser_port = listener.getsockname()[1]
                browser_url = f"http://127.0.0.1:{browser_port}"
                browser_log = (run / "browser.log").open("wb")
                browser_server = subprocess.Popen([program, "--import", "tsx", "server/main.ts", "--production",
                    "--state", str(projection), "--port", str(browser_port)], cwd=viewer,
                    stdout=browser_log, stderr=subprocess.STDOUT,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                browser_log.close()
                ready = {"native_url": url, "browser_url": browser_url, "actor": actor,
                    "projection": str(projection), "run": str(run), "clock": "paused operator fixture"}
                (run / "browser-ready.json").write_text(json.dumps(ready), encoding="utf-8")
                print("DUAL_VIEW_READY " + json.dumps(ready), flush=True)
                end = time.monotonic() + 600
                while not (run / "browser-done").exists() and time.monotonic() < end:
                    if browser_server.poll() is not None:
                        raise RuntimeError("owned browser server exited; inspect browser.log")
                    time.sleep(0.25)
                report["browser_fixture"] = ready
                if not (run / "browser-done").exists():
                    raise RuntimeError("bounded browser check timed out")
            page = rpc(url, "agent/act", {**auth, "verb": "write_diary",
                "args": {"title": "First island day", "body": "Public diary survives; private books remain private."}})
            assert page["accepted"] is True
            rpc(url, "npc/step", {"ticks": 1})
            # Separate explicitly labeled forced-death fixture, never live actor.
            rpc(url, "npc/command", {"cmd": {"type": "SetNeed", "unit": actor,
                "need": "Thirst", "value": 0}})
            rpc(url, "npc/step", {"ticks": 2000})
            capture("permanent-death")
            click(url, "New life")
            rpc(url, "npc/step", {"ticks": 1})
            capture("successor-reconnect")
            state = rpc(url, "world/state")  # Operator diagnostic for recovery fixture only.
            diary = next(e["sim_id"] for e in state["entities"]
                if "island_dropped_diary" in e.get("components", {}))
            successor = next(e["sim_id"] for e in state["entities"]
                if e.get("components", {}).get("island_villager", {}).get("life") == 2)
            credentials = json.loads((run / "agents/herp/agent.json").read_text(encoding="utf-8-sig"))
            registration = rpc(url, "agent/register", {"agent_id": credentials["agent_id"], "token": credentials["token"]})
            auth = {"session": registration["session"]}
            rpc(url, "agent/enter_world", auth)
            rpc(url, "agent/take_control", auth)
            rpc(url, "npc/step", {"ticks": 1})
            rpc(url, "agent/take_control", auth)
            rpc(url, "npc/step", {"ticks": 40})
            # The previous life walked away from the landing point. The successor
            # must walk to its public diary too; direct interaction never teleports.
            for _ in range(6):
                state = rpc(url, "world/state")
                entities = {e["sim_id"]: e for e in state["entities"]}
                holder = entities[diary]["components"].get("carried_by", {}).get("holder")
                target = holder or diary
                diary_pos = entities[target]["components"]["pos"]
                if isinstance(diary_pos, dict):
                    diary_pos = [diary_pos["x"], diary_pos["z"]]
                approach = rpc(url, "agent/act", {**auth, "steps": [
                    {"verb": "order", "args": {"order": {"type": "Move", "pos": diary_pos}}},
                    {"verb": "execute_orders"}]})
                assert approach["accepted"] is True
                rpc(url, "npc/step", {"ticks": 900})
                request = {**auth, "verb": "ask_diary", "target": holder} if holder else {
                    **auth, "verb": "interact", "target": diary, "args": {"verb": "pick_up"}}
                pickup = rpc(url, "agent/act", request)
                if pickup["accepted"] is True:
                    rpc(url, "npc/step", {"ticks": 40})
                    break
            else:
                raise RuntimeError("Successor could not approach/recover its public diary within the fixture budget")
            click(url, "Found diaries")
            capture("recovered-diary")
            if args.confirm_polish and set(report["screenshots"]) != POLISH_CAPTURES:
                raise RuntimeError("Selective polish confirmation did not capture all six required states")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        if browser_server is not None:
            try:
                if browser_server.poll() is None:
                    browser_server.terminate()
                browser_server.wait(timeout=10)
            except (OSError, subprocess.TimeoutExpired):
                if browser_server.poll() is None:
                    browser_server.kill()
                browser_server.wait(timeout=10)
            report["owned_browser_server_exited"] = browser_server.poll() is not None
        if child is not None:
            try:
                rpc(url, "app/quit")
                child.wait(timeout=20)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                child.kill()
                child.wait(timeout=10)
            report["owned_process_exited"] = child.poll() is not None
        report["run"] = str(run.relative_to(ROOT))
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2))
        if not report["ok"]:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
