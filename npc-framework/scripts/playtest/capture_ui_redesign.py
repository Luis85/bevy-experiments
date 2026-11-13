"""Native captures of the redesigned interface (spec phases 1 and 2): the widget
gallery and the Oversee shell, at 1280x720 and 1024x768.

Build first, then run from npc-framework:
  python scripts/playtest/capture_ui_redesign.py [--binary target/debug/island_game.exe]
      [--skip-gallery] [--skip-oversee]

Two disposable runs of the real game, driven over BRP like
`capture_build_menu.py`:

1. `--ui-gallery controls`: every gallery page in Parchment, and the Controls
   and Data pages in Dusk.
2. A new island, entered through "Oversee the village": the default view at
   noon and at night, the alert stack once something needs attention, each
   ledger page in the right column, the Overlays and Places lists, a tooltip at
   both levels (the real pointer path: window cursor position and the tooltip's
   own timers) and, where the display allows the window, a 2560x1440 window
   for the automatic interface size.

Controls are found by their `Name` ("Game: Economy"), which every kit control
carries, icon-only ones too; a name that exists twice is told apart by the
panel it sits in. No keyboard input is sent: the keys are covered by unit
tests. PNGs go to target/npc-out/ui-redesign-p2/ and still need to be read.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import time
import uuid

import capture_player_flow
from capture_build_menu import BuildFlow
from capture_inspector import NAME
from capture_native import PARENT, ROOT, query, scalar
from capture_native_homestead import TRANSFORM, file_hash, translation
from capture_player_flow import PRIMARY, wait_for
from capture_view_checks import DAY

SIZES = ((1280, 720), (1024, 768))
capture_player_flow.SIZES = SIZES
COMPUTED = "bevy_ui::ui_node::ComputedNode"
PAGES = ("Village", "Economy", "People", "Projects", "Journal")
GALLERY = ("Tokens", "Controls", "Data", "Messages", "Icons")
LEDGER_DOCK = "Island dock: ledger"
VIEW_DOCK = "Island dock: view"


class UiFlow(BuildFlow):
    def names(self):
        found = {}
        for row in query(self.url, [NAME]):
            value = row["components"][NAME]
            name = value.get("name") if isinstance(value, dict) else str(value)
            found.setdefault(name, []).append(int(row["entity"]))
        return found

    def parents(self):
        return {int(row["entity"]): int(scalar(row["components"][PARENT]))
                for row in query(self.url, [PARENT])}

    def control(self, name, under=None):
        """The one control called `name`, inside the panel `under` if given."""
        names = self.names()
        found = names.get("Game: " + name, [])
        if under is not None:
            roots = names.get(under, [])
            if len(roots) != 1:
                raise RuntimeError(f"Expected one panel {under!r}, found {len(roots)}")
            parents = self.parents()

            def inside(entity):
                seen = set()
                while entity in parents and entity not in seen:
                    seen.add(entity)
                    entity = parents[entity]
                    if entity == roots[0]:
                        return True
                return False

            found = [entity for entity in found if inside(entity)]
        if len(found) != 1:
            raise RuntimeError(f"Expected one control {name!r} (under {under!r}), found {len(found)}")
        return found[0]

    def press(self, name, under=None):
        entity = self.control(name, under)
        self.activate(entity, name)
        return entity

    def centre(self, entity):
        """Centre of a UI node in logical window pixels."""
        factor = self.scale()
        for row in query(self.url, [TRANSFORM]):
            if int(row["entity"]) == entity:
                x, y = translation(row["components"][TRANSFORM])[:2]
                return x / factor, y / factor
        raise RuntimeError(f"Entity {entity} has no UI transform")

    def snap(self, name, size):
        """One screenshot at the current window size."""
        path = Path(self.call("npc/screenshot", {"path": "shots/" + name})["path"])

        def complete():
            try:
                with path.open("rb") as image:
                    header = image.read(24)
                    image.seek(-12, 2)
                    tail = image.read(12)
                if header[:8] == b"\x89PNG\r\n\x1a\n" and tail[4:8] == b"IEND":
                    return list(struct.unpack(">II", header[16:24]))
            except (OSError, ValueError):
                pass
            return None

        pixels = wait_for(complete)
        if pixels != list(size):
            raise RuntimeError(f"{name}: PNG is {pixels}, not {list(size)}")
        self.report["screenshots"][name] = str(path.resolve())
        print(f"captured {name}", flush=True)

    def both(self, label, prepare=None, settle=0.6):
        """Capture at both sizes; `prepare(size)` runs after each resize."""
        self.assert_window()
        for size in SIZES:
            self.settle(size)
            time.sleep(settle)
            if prepare is not None:
                prepare(size)
            self.snap(f"{label}-{size[0]}x{size[1]}", size)
        self.settle(SIZES[0])

    def park(self, size):
        """Move the pointer to open ground so nothing is hovered."""
        self.cursor(size[0] * 0.5, size[1] * 0.55)
        time.sleep(0.3)

    def to_phase(self, phase):
        now = self.state()["tick"]
        target = now - now % DAY + int(phase * DAY)
        target = target if target >= now else target + DAY
        self.advance(target - now)
        return self.state()["tick"]

    def whole_island(self):
        """Frame the whole island through Places, as a player would. The
        machine's own mouse may have rested at a window edge and panned."""
        self.press("Places", VIEW_DOCK)
        time.sleep(0.5)
        self.press("Whole island", "Island places")
        time.sleep(1.0)

    def panel_style(self, current):
        """Toggle the panel style in Settings (pause menu) and come back."""
        self.press("Menu")
        self.click("Settings")
        self.click(current)
        self.click("Back")
        self.click("Resume")
        self.call("npc/pause")
        time.sleep(0.6)

    def stack(self):
        """What the alert stack says: (header, visible row titles)."""
        texts = self.texts()
        header = next((text for text in texts if text in ("Alerts", "No alerts")), None)
        return header, [text for text in texts
                        if text.startswith(("Warning · ", "Critical · "))]


def launch(binary, run, extra, phase=None, save=None):
    """Start the game in `run`; `save` is a prepared island save (a fixture
    from `island_capture_fixtures`) copied in as the game's own save, so the
    fixture itself is never written."""
    run.mkdir(parents=True)
    if save is not None:
        shutil.copy2(save, run / "living-world.world.json")
    settings = run / "settings.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "worlds_dir": str(run / "rules"), "fog_of_war": True}), encoding="utf-8")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    if phase is not None:
        environment["ISLAND_VIEW_DAY_PHASE"] = str(phase)
    log = (run / "native.log").open("wb")
    child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
        "--island-save", str(run / "living-world.world.json"), "--remote", "--remote-port", str(port),
        "--no-render-remote", "--out-dir", str(run), *extra], cwd=ROOT, env=environment,
        stdout=log, stderr=subprocess.STDOUT,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    return child, f"http://127.0.0.1:{port}"


def gallery(binary, base, report):
    run = base / "gallery"
    child, url = launch(binary, run, ["--ui-gallery", "controls"])
    try:
        flow = UiFlow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 15 * 60
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        time.sleep(1.0)
        for page in GALLERY:
            flow.press(page)
            time.sleep(0.5)
            flow.both(f"gallery-{page.lower()}")
        flow.press("Dusk")
        for page in ("Controls", "Data", "Messages"):
            flow.press(page)
            time.sleep(0.5)
            flow.both(f"gallery-{page.lower()}-dusk")
        # Wrapped text and icons at twice the size, where the display allows the window.
        try:
            flow.settle((2560, 1440))
            time.sleep(1.5)
            flow.snap("gallery-messages-dusk-auto-scale-2560x1440", (2560, 1440))
        except RuntimeError as error:
            report["checks"].append({"check": "gallery at 2560x1440", "ok": False, "reason": str(error)})
        finally:
            flow.settle(SIZES[0])
        report["checks"].append({"check": "gallery pages captured", "ok": True, "pages": list(GALLERY)})
    finally:
        stop(child, run, report, "gallery")
    return run


def oversee(binary, base, report):
    run = base / "oversee"
    child, url = launch(binary, run, [])
    try:
        flow = UiFlow(run, child, url, report, time.monotonic())
        flow.deadline = time.monotonic() + 40 * 60
        flow.tick_budget = 12 * DAY
        flow.bootstrap()
        flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
        flow.click("New game")
        flow.click("Oversee the village")
        wait_for(lambda: flow.widget("Oversee"))
        flow.call("npc/pause")
        time.sleep(1.0)

        # 1. The default view: right column empty, at noon and at night.
        report["ticks"] = {"noon": flow.to_phase(0.35)}
        flow.park(SIZES[0])
        flow.whole_island()
        flow.both("oversee-noon", flow.park)
        report["top_bar"] = [text for text in flow.texts() if text][:40]
        report["ticks"]["night"] = flow.to_phase(0.85)
        flow.both("oversee-night", flow.park)
        # The open risk of the palette: a parchment panel over a night scene.
        flow.press("Economy", LEDGER_DOCK)
        time.sleep(0.9)
        flow.both("ledger-economy-night", flow.park, settle=0.9)
        flow.press("Economy", LEDGER_DOCK)
        time.sleep(0.4)
        # The same in the Dusk panel style, chosen in Settings as a player would.
        try:
            flow.panel_style("Panels: parchment")
            flow.press("Economy", LEDGER_DOCK)
            time.sleep(0.9)
            flow.both("ledger-economy-night-dusk", flow.park, settle=0.9)
            flow.press("Economy", LEDGER_DOCK)
            time.sleep(0.4)
            flow.panel_style("Panels: dusk")
            report["checks"].append({"check": "Dusk chosen in Settings and captured at night", "ok": True})
        except (RuntimeError, KeyError) as error:
            report["checks"].append({"check": "Dusk chosen in Settings and captured at night",
                                     "ok": False, "reason": str(error)})

        # 2. The alert stack, once something needs attention (at most four days).
        header, rows = flow.stack()
        days = 0
        while not rows and days < 4:
            flow.advance(DAY // 2)
            days += 0.5
            header, rows = flow.stack()
        flow.to_phase(0.3)
        header, rows = flow.stack()
        report["alerts"] = {"header": header, "rows": rows, "days_waited": days}
        flow.park(SIZES[0])
        flow.whole_island()
        report["checks"].append({"check": "the alert stack has rows to show", "ok": bool(rows),
                                 "header": header})
        flow.both("oversee-alerts", flow.park)
        flow.press("Hide", "Village alerts")
        time.sleep(0.4)
        flow.both("oversee-alerts-folded", flow.park)
        flow.press("Hide", "Village alerts")

        # 3. Each ledger page in the right column, from its dock tile.
        for page in PAGES:
            flow.press(page, LEDGER_DOCK)
            time.sleep(0.9)
            flow.both(f"ledger-{page.lower()}", flow.park, settle=0.9)
        flow.press("Journal", LEDGER_DOCK)  # The same tile closes the column.
        time.sleep(0.5)

        # 4. The lists above the left dock.
        for name in ("Overlays", "Places"):
            flow.press(name, VIEW_DOCK)
            time.sleep(0.5)
            flow.both(f"popover-{name.lower()}", flow.park)
            flow.press(name, VIEW_DOCK)
            time.sleep(0.3)

        # 5. A tooltip at both levels, through the pointer and the tooltip's own timers.
        chip = flow.control("Residents")
        for size in SIZES:
            flow.settle(size)
            time.sleep(0.6)
            flow.park(size)
            x, y = flow.centre(chip)
            for attempt in range(3):
                flow.cursor(x, y)
                time.sleep(0.9)
                if any(text == "Everyone has a home." or " without a home." in text for text in flow.texts()):
                    break
            flow.snap(f"tooltip-level1-{size[0]}x{size[1]}", size)
            flow.cursor(x + 1, y)
            time.sleep(0.2)
            flow.cursor(x, y)
            time.sleep(2.2)
            report.setdefault("tooltips", {})[f"{size[0]}x{size[1]}"] = [
                text for text in flow.texts() if "housed in" in text or text.startswith("Click: open")]
            flow.snap(f"tooltip-level2-{size[0]}x{size[1]}", size)
            flow.park(size)
        flow.settle(SIZES[0])

        # 6. Automatic interface size in a 1440p window, where the display allows one.
        try:
            flow.settle((2560, 1440))
            time.sleep(1.0)
            flow.park((1280, 720))
            flow.snap("oversee-auto-scale-2560x1440", (2560, 1440))
            report["checks"].append({"check": "a 2560x1440 window was captured", "ok": True})
        except RuntimeError as error:
            report["checks"].append({"check": "a 2560x1440 window was captured", "ok": False,
                                     "reason": str(error)})
        finally:
            flow.settle(SIZES[0])

        # 7. A resident chosen from the People page: the inspector takes the
        # right column (its own redesign is the next phase).
        flow.press("People", LEDGER_DOCK)
        time.sleep(0.9)
        names = flow.names()
        rows = sorted(name for name in names if name.startswith("Game: ") and " · mood " in name)
        report["checks"].append({"check": "the roster lists residents", "ok": bool(rows), "rows": len(rows)})
        if rows:
            flow.activate(names[rows[0]][0], rows[0].splitlines()[0])
            time.sleep(1.5)
            report["checks"].append({"check": "choosing a resident replaces the ledger with the inspector",
                                     "ok": "Ledger" not in flow.texts()})
            flow.both("inspector-right-column", flow.park, settle=0.9)
    finally:
        stop(child, run, report, "oversee")
    return run


def stop(child, run, report, label):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            # A busy machine can take longer to let the window go.
            child.kill()
            child.wait(timeout=30)
            report.setdefault("slow_stops", []).append(label)
    log_path = run / "native.log"
    if log_path.exists():
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        report.setdefault("native_errors", {})[label] = [
            line for line in lines if "ERROR" in line or "panicked at" in line][-30:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-redesign-p2")
    parser.add_argument("--skip-gallery", action="store_true")
    parser.add_argument("--skip-oversee", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    base = ROOT / "target" / ("playtest-ui-redesign-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation by control name; pointer through window cursor "
                 "position; the simulation clock stepped while paused",
        "keyboard_input_tested": False, "screenshots": {}, "checks": []}
    runs = []
    try:
        if not args.skip_gallery:
            runs.append(gallery(binary, base, report))
        if not args.skip_oversee:
            runs.append(oversee(binary, base, report))
        report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for run in [base / "gallery", base / "oversee"]:
            if (run / "shots").is_dir():
                for image in sorted((run / "shots").glob("*.png")):
                    shutil.copy2(image, args.out / image.name)
                    copied.append(str(args.out / image.name))
        report["copied"] = copied
        (args.out / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                               encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
