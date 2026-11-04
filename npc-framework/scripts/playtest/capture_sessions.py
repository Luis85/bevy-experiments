"""Bounded native session viewer captures with synthetic records, never a provider.

Uses a disposable paused explorer, owned BRP widgets and screenshot-only menu
deep links. No physical input, private saves, provider history or inference.
"""
import json
from pathlib import Path
import socket
import subprocess
import time
import uuid

from capture_native import ROOT, TEXT, WINDOW, click, query, rpc, scalar

FLAGS = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def seed(workspace, run):
    private = workspace / ".ai-client"
    private.mkdir(parents=True, exist_ok=True)
    owner, metric_id, log_id = uuid.uuid4().hex, uuid.uuid4().hex, "1793534400-" + uuid.uuid4().hex
    logs, metrics = run / "private-session-logs", run / "session-metrics"
    logs.mkdir(exist_ok=True)
    metrics.mkdir(exist_ok=True)
    metric_path, log_path = metrics / (metric_id + ".json"), logs / (log_id + ".md")
    metric_path.write_text(json.dumps({"version": 2, "accounting_revision": 2,
        "metrics_id": metric_id, "provider": "codex", "created_unix_seconds": 1793534400,
        "turns_started": 2, "turns_completed": 2, "turns_failed": 0, "turns_interrupted": 0,
        "model": None, "effort": None, "selection_intent": {
            "requested_model": "gpt-fixture-default", "requested_effort": "high"},
        "totals": {"input_tokens": 1200, "output_tokens": 80, "cached_input_tokens": 800,
            "cost_microusd": None}, "cost_basis": "unknown", "accounting_coverage": "conversation_start"}, indent=2), encoding="utf-8")
    body = "# AI agent game client session\n\nSynthetic UI evidence. No provider session was started.\n\n"
    body += "".join(f"## Event {i}\n\n    Actor observed its surroundings and continued its assigned task.\n\n" for i in range(210))
    body += "## Turn finished — Unix 1793534400\n\n    outcome: completed\n    input_tokens: 1200\n    output_tokens: 80\n    Text sample: café\n\n## Client finished\n\n    Synthetic session preview complete.\n"
    log_path.write_text(body, encoding="utf-8")
    (private / "history-owner.json").write_text(json.dumps({"version": 1, "owner_id": owner}), encoding="utf-8")
    common = {"version": 1, "owner_id": owner, "provider": "codex", "created_unix_seconds": 1793534400}
    records = [dict(common, kind="metrics", id=metric_id, path=str(metric_path), metrics_id=None),
        dict(common, kind="log", id=log_id, path=str(log_path), metrics_id=metric_id)]
    (private / "history-index.jsonl").write_text("".join(json.dumps(record) + "\n" for record in records), encoding="utf-8")
    return logs, metrics, log_path


def wait_for_text(url, needle, seconds=90):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            if any(needle in scalar(row["components"][TEXT]) for row in query(url, [TEXT])):
                return
        except (OSError, RuntimeError):
            pass
        time.sleep(0.2)
    raise RuntimeError("Owned UI did not reach the requested state")


def main():
    run = ROOT / "target" / ("native-sessions-" + uuid.uuid4().hex[:12])
    run.mkdir()
    report = {"ok": False, "provider_started": False, "private_history_read": False,
        "evidence_kind": "synthetic records in disposable paused explorer", "screenshots": {}}
    child = menu = None
    url = None
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    try:
        url = f"http://127.0.0.1:{port}"
        with (run / "actor.log").open("wb") as output:
            child = subprocess.Popen([str(ROOT / "target/release/actor_testbed.exe"),
                "--explorer-home", str(run / "agents"), "--actors-dir", str(run / "actors"),
                "--worlds-dir", str(run / "worlds"), "--agent", "Preview", "--world", "Preview Island",
                "--preset", "human", "--world-preset", "island_village", "--paused",
                "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run / "npc-out")],
                cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, creationflags=FLAGS)
            wait_for_text(url, "Sessions")
            workspace = run / "agents/preview"
            save=workspace / "preview-island.world.json"
            before_save=json.loads(save.read_text(encoding="utf-8-sig"))
            report["saved_tick_before"]=before_save["tick"]
            logs, metrics, log_path = seed(workspace, run)
            window = int(query(url, with_types=["bevy_window::window::PrimaryWindow"])[0]["entity"])
            resolution = next(row["components"][WINDOW]["resolution"] for row in query(url, [WINDOW]) if int(row["entity"]) == window)
            scale = resolution.get("scale_factor_override") or resolution["scale_factor"]
            report["actor_ui_scale"]=scale
            report["capture_width_units"]="logical window width; PNG pixels include OS scaling"

            def size(width):
                for field, value in [("physical_width", round(width * scale)), ("physical_height", round(720 * scale))]:
                    rpc(url, "world.mutate_components", {"entity": window, "component": WINDOW,
                        "path": "resolution." + field, "value": value})
                time.sleep(0.7)

            def capture(label):
                time.sleep(0.6)
                path = Path(rpc(url, "npc/screenshot", {"path": "shots/" + label})["path"])
                deadline = time.monotonic() + 20
                while not path.is_file() and time.monotonic() < deadline:
                    time.sleep(0.1)
                if not path.is_file():
                    raise RuntimeError("Owned screenshot did not arrive")
                report["screenshots"][label] = str(path)

            click(url, "Sessions")
            wait_for_text(url, "Synthetic session preview complete")
            for width in [1280, 760, 480]:
                size(width)
                capture("actor-logs-" + str(width))
            click(url, "Metrics")
            wait_for_text(url, "Accounted input tokens")
            size(760)
            capture("actor-metrics-760")
            click(url, "Back to agent controls")
            # Hidden history controls must not start a provider or replace normal controls.
            rpc(url, "app/quit", {"pid": child.pid})
            child.wait(timeout=45)
            report["saved_tick_after"]=json.loads(save.read_text(encoding="utf-8-sig"))["tick"]
            if report["saved_tick_after"] != report["saved_tick_before"]:
                raise RuntimeError("Read-only history view advanced the paused fixture")
            settings = run / "settings.json"
            settings.write_text(json.dumps({"agents_dir": str(run / "agents"), "actors_dir": str(run / "actors"),
                "worlds_dir": str(run / "worlds"), "fog_of_war": True,
                "ai_metrics_dir": str(metrics), "ai_logs_dir": str(logs)}), encoding="utf-8")
            empty = run / "agents/empty"
            empty.mkdir()
            cases = [("menu-metrics", "Preview", "metrics", 960), ("menu-logs", "Preview", "logs", 960),
                ("menu-logs-480", "Preview", "logs", 480), ("menu-empty", "Empty", "logs", 960),
                ("settings-480", None, None, 480)]
            for label, agent, tab, width in cases:
                path = run / (label + ".png")
                command = [str(ROOT / "target/release/game_menu.exe"), "--settings", str(settings),
                    "--screenshot", str(path), "--after-secs", "4", "--capture-size", f"{width}x720"]
                command += ["--sessions", agent, "--session-view", tab] if agent else ["--screen", "settings"]
                if agent is None:
                    command += ["--settings-focus", "ai_logs"]
                with (run / (label + ".log")).open("wb") as output:
                    menu = subprocess.Popen(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, creationflags=FLAGS)
                    menu.wait(timeout=60)
                if menu.returncode or not path.is_file():
                    raise RuntimeError("Owned menu screenshot failed")
                report["screenshots"][label] = str(path)
            report["ok"] = True
            report["log_bytes"] = log_path.stat().st_size
            report["client_state_created"]=(workspace / ".ai-client/state.json").exists() or (workspace / ".ai-client/status.json").exists()
            if report["client_state_created"]:
                report["ok"]=False
                raise RuntimeError("Read-only session browsing started a client")
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired, StopIteration):
        report["failure"] = "owned native session capture failed; inspect its local app logs"
    finally:
        if child is not None and child.poll() is None:
            try:
                if url:
                    rpc(url, "app/quit", {"pid": child.pid})
                child.wait(timeout=30)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                child.kill()
                child.wait(timeout=10)
        if menu is not None and menu.poll() is None:
            menu.kill()
            menu.wait(timeout=10)
        report["owned_processes_stopped"] = all(process is None or process.poll() is not None for process in [child, menu])
        (run / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(dict(report, run=str(run)), indent=2), flush=True)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
