"""Capture the Island Village launcher and Resident AI editor in disposable homes.

Requires a newly built game_menu executable; this script never runs Cargo, starts
an AI provider, changes personal settings, or opens an existing save. Outputs
stay below target/. These captures verify native rendering, not physical input.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[2]


def png_size(path):
    with path.open("rb") as image:
        header = image.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("capture is not a complete PNG header")
    return struct.unpack(">II", header[16:24])


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True,
                        help="newly built game_menu executable")
    parser.add_argument("--after-secs", type=int, default=8)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if not 3 <= args.after_secs <= 30:
        parser.error("--after-secs must be 3..30")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("--binary must name a built executable")
    run = (args.out or ROOT / "target" / ("colony-menu-" + uuid.uuid4().hex[:10])).resolve()
    if not run.is_relative_to((ROOT / "target").resolve()):
        parser.error("--out must be below this workspace's target directory")
    run.mkdir(parents=True, exist_ok=False)
    worlds = run / "worlds"
    worlds.mkdir()
    shutil.copyfile(ROOT / "content/testbed_content/worlds/island_colony.json",
                    worlds / "island_colony.json")
    (run / "agents").mkdir()
    settings = run / "settings.json"
    settings.write_text(json.dumps({
        "agents_dir": str(run / "agents"),
        "actors_dir": str(run / "actors"),
        "worlds_dir": str(worlds),
        "fog_of_war": False,
    }, indent=2), encoding="utf-8")
    report = {
        "ok": False,
        "binary": str(binary),
        "binary_sha256": file_hash(binary),
        "provider_started": False,
        "captures": [],
        "run": str(run),
    }
    owned = None
    try:
        for width, height in [(1280, 720), (1024, 768)]:
            for screen in ["main", "resident-ai"]:
                name = f"{screen}-{width}x{height}"
                path = run / f"{name}.png"
                command = [str(binary), "--settings", str(settings),
                           "--screen", "main" if screen == "main" else "worlds",
                           "--capture-size", f"{width}x{height}",
                           "--screenshot", str(path), "--after-secs", str(args.after_secs)]
                if screen != "main":
                    command += ["--worlds-page", "resident_ai"]
                capture = {"name": name, "path": str(path),
                           "logical_size": [width, height], "ok": False}
                report["captures"].append(capture)
                with (run / f"{name}.log").open("wb") as log:
                    owned = subprocess.Popen(command, cwd=ROOT, stdout=log,
                        stderr=subprocess.STDOUT,
                        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                    capture["owned_pid"] = owned.pid
                    owned.wait(timeout=args.after_secs + 50)
                capture["exit_code"] = owned.returncode
                if owned.returncode != 0:
                    raise RuntimeError(f"{name} exited {owned.returncode}; inspect its log")
                capture["physical_size"] = list(png_size(path))
                capture["ok"] = all(size > 0 for size in capture["physical_size"])
                print(json.dumps(capture), flush=True)
        report["ok"] = all(capture["ok"] for capture in report["captures"])
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        report["error"] = str(error)
    finally:
        if owned is not None and owned.poll() is None:
            owned.kill()
            owned.wait(timeout=10)
        report["owned_process_stopped"] = owned is None or owned.poll() is not None
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2), flush=True)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
