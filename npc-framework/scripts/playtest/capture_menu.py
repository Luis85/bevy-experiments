"""Owned, no-provider menu captures. Requires built release apps and Python 3.

--mode worlds: select AI scheduler in the opened editor before the capture timer.
--mode detached: capture the dedicated-agent controls of a new paused island host.
No existing settings/saves or provider sessions are used. Built-in screenshot exit
avoids invoking Menu Quit on unrelated worlds; the guard stops only its owned host.
"""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import uuid

from capture_native import ROOT, rpc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=["worlds", "detached"], default="worlds")
    parser.add_argument("--after-secs", type=int, default=90)
    args = parser.parse_args()
    if not 3 <= args.after_secs <= 300:
        parser.error("capture delay must be 3..300 seconds")
    run = ROOT / "target" / ("stream-menu-" + uuid.uuid4().hex[:10])
    run.mkdir()
    settings = run / "settings.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "actors_dir": str(run / "actors"), "worlds_dir": str(run / "worlds"),
        "fog_of_war": True}), encoding="utf-8")
    host = menu = None
    url = None
    report = {"ok": False, "mode": args.mode, "provider_started": False,
        "run": str(run), "screenshot": str(run / (args.mode + ".png"))}
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    try:
        if args.mode == "detached":
            host = subprocess.Popen([str(ROOT / "target/release/npcctl.exe"), "serve",
                "--explorer-home", str(run / "agents"), "--actors-dir", str(run / "actors"),
                "--worlds-dir", str(run / "worlds"), "--agent", "Herp", "--world", "Island",
                "--preset", "human", "--world-preset", "island_village", "--port", "0", "--paused"],
                cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                creationflags=flags)
            ready = queue.Queue(maxsize=1)

            def collect():
                for line in host.stdout:
                    try:
                        value = json.loads(line)
                        if "listening" in value:
                            ready.put_nowait(value["listening"])
                    except (ValueError, queue.Full):
                        pass

            threading.Thread(target=collect, daemon=True).start()
            url = ready.get(timeout=60)
        command = [str(ROOT / "target/release/game_menu.exe"), "--settings", str(settings),
            "--screenshot", report["screenshot"], "--after-secs", str(args.after_secs)]
        command += ["--screen", "worlds"] if args.mode == "worlds" else ["--detached", "Herp"]
        with (run / "menu.log").open("wb") as log:
            menu = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                creationflags=flags)
            report["owned_menu_pid"] = menu.pid
            (run / "ready.json").write_text(json.dumps(report), encoding="utf-8")
            print("MENU_CAPTURE_READY " + json.dumps(report), flush=True)
            menu.wait(timeout=args.after_secs + 90)
        report["ok"] = menu.returncode == 0 and Path(report["screenshot"]).is_file()
    except (OSError, RuntimeError, ValueError, queue.Empty, subprocess.TimeoutExpired):
        report["failure"] = "owned_menu_capture_failed; inspect its local log"
    finally:
        if menu is not None and menu.poll() is None:
            menu.kill()
            menu.wait(timeout=10)
        if host is not None:
            try:
                if host.poll() is None and url:
                    rpc(url, "session/stop")
                host.wait(timeout=20)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                host.kill()
                host.wait(timeout=10)
        report["owned_processes_stopped"] = all(p is None or p.poll() is not None for p in [host, menu])
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2), flush=True)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
