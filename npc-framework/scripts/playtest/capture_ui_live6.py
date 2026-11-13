"""Native captures of the sixth UI pass (polish): the Reduce motion setting,
captures without first-run tips (`ISLAND_TIPS_SEEN=all`), the upgrade card
in short rows, an alert's tooltip beside the list, the world picker's plain
reason for a bad file, the export toast clear of the Resources panel at
1024x768, and the morning account's Bought and Sold rows. Each at 1280x720
and 1024x768.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p testbed_content --test island_capture_fixtures -- --ignored --test-threads 5
  cargo build --locked -p game_menu
  python scripts/playtest/capture_ui_live6.py [--only settings,worlds,summary,upgrade,alerts]

PNGs and summary.json go to target/npc-out/ui-live6/.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import time
import uuid

# Every first-run tip counts as seen for these runs: no tip covers a scene.
os.environ["ISLAND_TIPS_SEEN"] = "all"

import capture_player_flow  # noqa: E402
from capture_native import ROOT, query  # noqa: E402
from capture_native_homestead import file_hash  # noqa: E402
from capture_player_flow import ALERTS, PRIMARY, wait_for  # noqa: E402
from capture_ui_live3 import FIXTURES  # noqa: E402
from capture_ui_live5 import (Live5Flow, summary as live5_summary,  # noqa: E402
                              upgrade as live5_upgrade, worlds as live5_worlds)
from capture_ui_phase45 import WORKSPACE  # noqa: E402
from capture_ui_redesign import SIZES, launch, stop  # noqa: E402
from capture_view_checks import DAY  # noqa: E402

capture_player_flow.SIZES = SIZES


class Live6Flow(Live5Flow):
    def snap(self, name, size):
        # The fifth pass's sections are reused; their shots carry this pass's name.
        return super().snap(name.replace("live5-", "live6-", 1), size)

    def resize(self, size):
        # The desktop's scale has been seen to switch between 1 and 2 during
        # these runs (the camera target then reports twice the pixels); a
        # resize is tried again a few times before the section gives up.
        for attempt in range(4):
            try:
                return super().resize(size)
            except RuntimeError:
                failed = [entry for entry in self.trace if entry.get("operation") == "resize_failed"]
                self.report.setdefault("resize_failures", []).append(failed[-1:] if failed else None)
                if attempt == 3:
                    raise
                time.sleep(2.0)

    def tips_shown(self):
        return [row for row in self.ui()[0] if str(row.get("text", "")) == "Got it"]


def start(binary, base, name, report, fixture, attempts=5):
    """Launch on a copy of `fixture` and settle the window; a window that
    does not settle (another agent's game or a build loading the machine) is
    stopped and launched again, never left running."""
    for attempt in range(attempts):
        run = base / (name if attempt == 0 else f"{name}-retry{attempt}")
        child, url = launch(binary, run, [], save=FIXTURES / f"{fixture}.json")
        try:
            flow = Live6Flow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 40 * 60
            flow.tick_budget = 8 * DAY
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.settle(SIZES[0])
            return run, child, flow
        except RuntimeError as error:
            report.setdefault("start_retries", []).append(f"{name} attempt {attempt}: {error}")
            stop(child, run, report, name)
    raise RuntimeError(f"{name}: the window did not settle in {attempts} launches")


def overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def settings(flow, report, checks):
    time.sleep(1.0)
    flow.click("Settings")
    time.sleep(0.8)
    flow.click("Accessibility")
    time.sleep(0.8)
    texts = flow.texts()
    report["accessibility"] = [text for text in texts if "motion" in text.lower() or "calmer" in text.lower()]
    checks.append({"check": "Settings has a Reduce motion toggle with a plain sentence",
                   "ok": "Reduce motion" in texts and any("hold still" in text for text in texts),
                   "texts": report["accessibility"]})
    flow.shots("live6-settings-accessibility", settle=0.4)
    flow.key("Escape")
    time.sleep(0.5)


def worlds(flow, report, checks):
    live5_worlds(flow, report, checks)
    # In the new game, Shape with the Resources panel open: an export's
    # toast stands beside the panel, at both sizes.
    rules = flow.run / "rules"
    flow.key("KeyN")
    time.sleep(0.6)
    clear = {}
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.4)
        flow.park(size)
        flow.export_now(rules)
        time.sleep(0.3)
        panel = [row for row in flow.ui()[0] if str(row.get("text", "")) == "Resources"]
        toast = [row for row in flow.ui()[0] if str(row.get("text", "")).startswith("World exported as")]
        panel_rect = flow.rect(int(panel[0]["entity"])) if panel else None
        toast_rect = flow.rect(int(toast[0]["entity"])) if toast else None
        # The panel's own box: from its title down the window's left side.
        box = None if panel_rect is None else [panel_rect[0] - 12, panel_rect[1] - 12,
                                               panel_rect[0] - 12 + 248, panel_rect[1] + 340]
        clear[f"{size[0]}x{size[1]}"] = {"panel": box, "toast_text": toast_rect,
                                         "clear": box is not None and toast_rect is not None
                                         and not overlaps(box, toast_rect)}
        flow.snap(f"live6-export-beside-panel-{size[0]}x{size[1]}", size)
    flow.settle(SIZES[0])
    report["export_beside_panel"] = clear
    checks.append({"check": "with the Resources panel open the export toast stands beside it",
                   "ok": all(item["clear"] for item in clear.values()), "seen": clear})
    picker = report.get("picker") or []
    bad = [text for text in picker if text.startswith("Cannot be used")]
    checks.append({"check": "a bad world file says so plainly, without the parser's words",
                   "ok": bad == ["Cannot be used: it is not a world file."], "texts": bad})


def summary(flow, report, checks):
    live5_summary(flow, report, checks)
    lines = report.get("summary") or []
    checks.append({"check": "the morning's account has a Sold row",
                   "ok": "Sold" in lines, "texts": lines})


def upgrade(flow, report, checks):
    live5_upgrade(flow, report, checks)
    # The builder at work is captured by capture_ui_live7.py, stepping the
    # clock: the stage at the door lasts about 180 ticks.
    card = report.get("card_unanswered") or []
    status = [text for text in card if "No one has taken" in text]
    rows = status[0].split("\n") if status else []
    report["card_rows"] = rows
    checks.append({"check": "the upgrade card's status is short rows, one sentence each",
                   "ok": len(rows) >= 3 and all(". " not in row for row in rows), "rows": rows})


def alerts(flow, report, checks):
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.2)
    if flow.has_widget("Show"):
        flow.click("Show")
        time.sleep(0.6)
    report["tips_in_oversee"] = len(flow.tips_shown())
    checks.append({"check": "with ISLAND_TIPS_SEEN=all no first-run tip shows",
                   "ok": report["tips_in_oversee"] == 0})
    # Alerts come as the village runs: time runs at 10x until two show.
    def alert_rows():
        return [entity for entity, text in flow.button_rows(ALERTS)
                if text.splitlines()[:1] != [] and text.splitlines()[0] not in ("Show", "Hide", "")
                and "journal" not in text]
    if len(alert_rows()) < 2:
        flow.call("npc/resume")
        for _ in range(3):
            flow.key("Equal")
            time.sleep(0.2)
        try:
            wait_for(lambda: len(alert_rows()) >= 2 or None, 240)
        except RuntimeError:
            pass
        flow.call("npc/pause")
        if flow.has_widget("Show"):
            flow.click("Show")
            time.sleep(0.6)
    seen = {}
    for size in SIZES:
        flow.settle(size)
        time.sleep(0.6)
        flow.park(size)
        # The alert rows: the list's buttons other than Show / Hide.
        rows = alert_rows()
        report.setdefault("alert_rows", {})[f"{size[0]}x{size[1]}"] = [
            text for _, text in flow.button_rows(ALERTS)]
        if len(rows) < 2:
            seen[f"{size[0]}x{size[1]}"] = {"rows": len(rows)}
            continue
        first, second = flow.rect(rows[0]), flow.rect(rows[1])
        flow.cursor((first[0] + first[2]) / 2, (first[1] + first[3]) / 2)
        cards = wait_for(lambda: flow.tooltips_inside(size) or None, 6) or []
        time.sleep(0.5)
        flow.snap(f"live6-alert-tooltip-{size[0]}x{size[1]}", size)
        clear = bool(cards) and all(not overlaps(card["rect"], second) for card in cards)
        seen[f"{size[0]}x{size[1]}"] = {"cards": cards, "next_row": second, "clear": clear}
        flow.park(size)
    flow.settle(SIZES[0])
    report["alert_tooltips"] = seen
    checks.append({"check": "an alert's tooltip stands beside the list, not over the next alert",
                   "ok": bool(seen) and all(item.get("clear") for item in seen.values()),
                   "seen": seen})
    flow.shots("live6-oversee-no-tips")


PARTS = {"settings": ("cottage-evening", [settings]),
         "worlds": ("cottage-evening", [worlds]),
         "summary": ("cottage-evening", [summary]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "alerts": ("upgrade-underway", [alerts]),
         "alerts-larger": ("larger-cottage-done", [alerts]),
         "alerts-cottage": ("cottage-evening", [alerts])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live6")
    parser.add_argument("--only", default="settings,worlds,summary,upgrade,alerts")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live6-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor and "
                       "MouseButtonInput; the simulation clock run, paused or sped up by its keys; "
                       "prepared saves copied in; ISLAND_TIPS_SEEN=all",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            run, child, flow = start(binary, base, name, report, fixture)
            flow.__class__ = Live6Flow
            try:
                if name != "settings":
                    flow.enter_live()
                for step in steps:
                    flow.section(step.__name__, lambda step=step: step(flow, report, report["checks"]))
                    flow.section(step.__name__ + " cleanup", lambda: flow.settle(SIZES[0]))
            finally:
                stop(child, run, report, name)
                if (run / "shots").is_dir():
                    for image in sorted((run / "shots").glob("*.png")):
                        shutil.copy2(image, args.out / image.name)
        report["ok"] = all(check["ok"] for check in report["checks"])
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        report["copied"] = sorted(str(path) for path in args.out.glob("*.png"))
        (args.out / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                               encoding="utf-8")
        print(base, flush=True)


if __name__ == "__main__":
    main()
