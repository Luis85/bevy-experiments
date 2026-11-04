"""One bounded confirmation of the three menu layouts changed after review."""
import json
from pathlib import Path
import subprocess
import sys
import uuid

from capture_native import ROOT


def main():
    fixture = Path(sys.argv[1]).resolve()
    if fixture.parent != (ROOT / "target").resolve() or not fixture.name.startswith("native-sessions-"):
        raise RuntimeError("Use the owned synthetic session capture fixture")
    evidence = json.loads((fixture / "report.json").read_text(encoding="utf-8"))
    if not evidence.get("ok") or evidence.get("provider_started") or evidence.get("client_state_created"):
        raise RuntimeError("The source fixture must be a completed no-provider capture")
    out = ROOT / "target" / ("native-sessions-confirm-" + uuid.uuid4().hex[:12])
    out.mkdir()
    report = {"ok": False, "provider_started": False, "private_history_read": False,
        "source_fixture": str(fixture), "screenshots": {}, "scope": "three changed menu layouts only"}
    child = None
    try:
        for label, tab, width in [("menu-metrics", "metrics", 960), ("menu-logs", "logs", 960), ("menu-logs-480", "logs", 480)]:
            path = out / (label + ".png")
            command = [str(ROOT / "target/release/game_menu.exe"), "--settings", str(fixture / "settings.json"),
                "--sessions", "Preview", "--session-view", tab, "--capture-size", f"{width}x720",
                "--screenshot", str(path), "--after-secs", "4"]
            with (out / (label + ".log")).open("wb") as output:
                child = subprocess.Popen(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                child.wait(timeout=60)
            if child.returncode or not path.is_file():
                raise RuntimeError("Owned menu confirmation failed")
            report["screenshots"][label] = str(path)
        report["ok"] = True
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait(timeout=10)
        report["owned_processes_stopped"] = child is None or child.poll() is not None
        (out / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(dict(report, run=str(out)), indent=2), flush=True)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
