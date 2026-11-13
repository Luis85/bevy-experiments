"""Native captures of the Oversee build tray, the Projects page and building
cards, at 1280x720 and 1024x768, and of one site from placement to completion.

Build first, then run from npc-framework:
  python scripts/playtest/capture_build_menu.py [--building cottage]
      [--days 6] [--binary target/debug/island_game.exe]

Uses a disposable spectator world and native widget activation over BRP, like
`capture_inspector.py`. The ghost and the placement go through the real
pointer path: the window's cursor position and `MouseButtonInput` messages, so
the view's own picking, placement check and command recording run. The script
never sends `PlaceBuilding` itself. The camera is placed by writing the view's
own `RtsCamera`, with the wanted cell in the middle of the window; whether a
spot is buildable is read from the tag beside the cursor, as a player would.
Operator state reads (`world/state`) find the settlement, tell when materials
are in and when the site is finished, and cross-check what the panels say.
After placing, the clock runs at 10x (the bar's own button) until the site has
its materials and until it is complete, or until `--days` game days have
passed: then the summary says where the site stands. No keyboard input is
sent: B, R and Esc are covered by unit tests, so the ghost is never turned
here (turning is the R key alone). PNGs go to
target/npc-out/build-menu/ and still need to be read.
"""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import time
import uuid

import capture_player_flow
from capture_inspector import BUTTON, WINDOW, InspectorFlow
from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import (BUILD_DOCK, BUILDING_CARD, LEDGER, PRIMARY, TRAY,
                                 wait_for)
from capture_view_checks import DAY
from capture_view_followups import Terrain

SIZES = ((1280, 720), (1024, 768))
capture_player_flow.SIZES = SIZES

SITE = "regional_construction_site_v1"
BUILT = "regional_building_v1"
DEPOT = "regional_depot_v1"
# The tag beside the cursor: its verdict is one of these two sentences.
TAG = "Build cursor tag"
VALID = "You can build here"
INVALID = "Cannot build here"


class BuildFlow(InspectorFlow):
    def cursor(self, x, y):
        """Put the pointer at a logical window position (see capture_view_wiring)."""
        factor = self.scale()
        self.call("world.mutate_components", {"entity": self.window, "component": WINDOW,
                  "path": "internal.physical_cursor_position", "value": [x * factor, y * factor]})
        self.call("world.write_message", {"message": "bevy_window::event::WindowEvent",
                  "value": {"CursorMoved": {"window": self.window, "position": [x, y], "delta": None}}})

    def press(self, button):
        for state in ("Pressed", "Released"):
            self.call("world.write_message", {"message": "bevy_input::mouse::MouseButtonInput",
                      "value": {"button": button, "state": state, "window": self.window}})
            time.sleep(0.12)

    def note(self):
        """The tag beside the cursor: (its verdict, the simulation's reason for
        a refusal). `(None, None)` while no building is in hand over the island."""
        texts = self.panel_texts(TAG)
        verdict = next((text for text in texts if text in (VALID, INVALID)), None)
        reason = None
        if verdict == INVALID:
            # The reason is the sentence under the verdict.
            after = texts[texts.index(INVALID) + 1:]
            reason = after[0] if after else None
        return verdict, reason

    def cards(self):
        """The build tray's cards, as read: (entity, name and lines)."""
        return self.button_rows(TRAY)

    def open_group(self, group):
        """Show a category of the build dock in the tray. The tile of the shown
        category closes the tray, so it is asked for only when needed."""
        if getattr(self, "group", None) != group or not self.cards():
            self.click(BUILD_DOCK + " > " + group)
            self.group = group
            wait_for(lambda: self.cards(), 6)
            time.sleep(0.4)

    def close_tray(self):
        if self.panel_texts(TRAY):
            self.click(TRAY + " > Close build menu")
            time.sleep(0.4)
        self.group = None

    def groups(self):
        """The categories the build dock offers, left to right."""
        return [text.splitlines()[0] for _, text in self.button_rows(BUILD_DOCK)]

    def aim(self, x, z, distance, label, size=SIZES[0]):
        """Frame a cell in the middle of the window and point at it."""
        self.look(x + 0.5, z + 0.5, distance, label)
        note = (None, None)
        # The machine's real mouse may cross the window and take the pointer
        # away; put it back until the view has a cell under it.
        for _ in range(6):
            self.cursor(size[0] / 2, size[1] / 2)
            time.sleep(0.9)  # One placement check: on a new cell, or within half a second.
            note = self.note()
            if note[0] is not None:
                break
        return note

    def settle(self, size):
        """Resize, nudging through a distinct size when Winit keeps the old
        render target (see PlayerFlow.capture); a busy machine may need more
        than one nudge."""
        last = None
        for attempt in range(4):
            try:
                if attempt:
                    self.resize((size[0] + attempt, size[1]))
                return self.resize(size)
            except RuntimeError as error:
                last = error
                self.record("capture_resize_retry", requested_pixels=list(size), attempt=attempt)
                self.deadline = max(self.deadline, time.monotonic() + 60)
                # A window somebody minimised during a long run has no
                # surface to draw on: ask the window to come back.
                self.call("world.mutate_components", {"entity": self.window, "component": WINDOW,
                          "path": "internal.minimize_request", "value": False})
                time.sleep(1.0)
        raise last

    def shoot(self, label, pointer=False):
        """Capture at both sizes. With `pointer`, the cursor is put back in the
        middle of each size first, so the ghost stays on the framed cell."""
        self.assert_window()
        for size in SIZES:
            geometry = self.settle(size)
            if pointer:
                self.cursor(size[0] / 2, size[1] / 2)
                time.sleep(0.9)
            else:
                time.sleep(0.45)
            name = f"{label}-{size[0]}x{size[1]}"
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
            self.report.setdefault("image_geometry", {})[name] = {"png_pixels": pixels, **geometry}
            if pointer:
                self.report.setdefault("notes", {})[name] = list(self.note())
            print(f"captured {name}", flush=True)
        self.settle(SIZES[0])
        if pointer:
            self.cursor(SIZES[0][0] / 2, SIZES[0][1] / 2)

    def sites(self):
        return {sim_id: parts[SITE] for sim_id, parts in self.world().having(SITE)}

    def built(self):
        world = self.world()
        return {sim_id: parts for sim_id, parts in world.having(BUILT)}

    def button_starting(self, prefix):
        """The one visible button whose label starts with `prefix`."""
        rows, _, _ = self.ui()
        widgets = {int(row["entity"]) for row in query(self.url, with_types=[BUTTON])}
        found = [row for row in rows if row["parent"] in widgets and str(row["text"]).startswith(prefix)]
        if len(found) != 1:
            raise RuntimeError(f"Expected one button starting {prefix!r}, found {len(found)}")
        return found[0]

    def panel_lines(self, *needles):
        return [text for text in self.texts() if any(needle in text for needle in needles)]

    def stamp(self, moment, **fields):
        entry = {"moment": moment, "tick": self.state()["tick"], "hud_clock": self.hud_clock(), **fields}
        self.report.setdefault("timeline", []).append(entry)
        print(json.dumps(entry, ensure_ascii=False), flush=True)
        return entry

    def run_until(self, predicate, budget_ticks, poll=0.5):
        """Let the clock run at the bar's 10x until `predicate()` holds."""
        began = self.state()["tick"]
        self.click("10×")
        self.call("npc/resume")
        try:
            while True:
                self.check()
                time.sleep(poll)
                value = predicate()
                tick = self.state()["tick"]
                if value or tick - began >= budget_ticks:
                    return value, tick - began
        finally:
            self.call("npc/pause")
            time.sleep(0.4)


def site_state(site):
    cost, delivered = site["cost"], site["delivered"]
    return {"label": site["label"], "cost": cost, "delivered": delivered,
            "supplied": all(delivered.get(kind, 0) >= count for kind, count in cost.items()),
            "work_done": site["work_done"], "work_ticks": site["work_ticks"],
            "incoming": site.get("incoming", {}), "builder": site.get("builder"),
            "paid": site.get("paid")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/build-menu")
    parser.add_argument("--building", default="cottage", help="catalogue key of the building to place")
    parser.add_argument("--days", type=float, default=6.0, help="game days to wait for completion")
    parser.add_argument("--reserve", type=int, default=None,
                        help="start the island from a copy of the shipped rules whose civic reserve is "
                             "this many coins (tuning.civic_reserve, with civic_reserve_grant_days 0)")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    run = ROOT / "target" / ("playtest-build-menu-" + uuid.uuid4().hex[:10])
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
    environment.pop("ISLAND_VIEW_DAY_PHASE", None)
    rules = []
    if args.reserve is not None:
        # Authored rules, not a sim write: a copy of the shipped preset with a
        # different reserve, given to the host the way a player would.
        preset = json.loads((ROOT / "content/island_content/worlds/living_world.json")
                            .read_text(encoding="utf-8"))

        def retune(node):
            if isinstance(node, dict):
                if "civic_reserve" in node:
                    node["civic_reserve"] = args.reserve
                    if "civic_reserve_grant_days" in node:
                        node["civic_reserve_grant_days"] = 0
                    return True
                return any(retune(value) for value in node.values())
            if isinstance(node, list):
                return any(retune(value) for value in node)
            return False

        if not retune(preset):
            raise RuntimeError("The shipped rules have no civic_reserve to change")
        edited = run / "living_world.rules.json"
        edited.write_text(json.dumps(preset), encoding="utf-8")
        rules = ["--island-rules", str(edited)]
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation; pointer through window cursor position and "
                 "MouseButtonInput messages; view camera placement; the bar's own 10x clock",
        "keyboard_input_tested": False, "building": args.building,
        "rules": "shipped" if args.reserve is None else
                 f"copy of the shipped rules with civic_reserve {args.reserve} and civic_reserve_grant_days 0",
        "days_budget": args.days,
        "screenshots": {}, "checks": [], "timeline": []}
    child = flow = None
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--island-rules", str(run / "rules" / "living_world.json"),
                "--island-save", str(save), *rules, "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = BuildFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 75 * 60
            flow.tick_budget = int((args.days + 3) * DAY)
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            flow.click("Menu")
            flow.click("Save game")
            flow.terrain = Terrain(json.loads(save.read_text(encoding="utf-8")))
            flow.click("Resume")
            flow.call("npc/pause")
            # A working morning, so residents hold stock and the planner has reviewed.
            flow.advance(1500)
            flow.stamp("start")

            # 1. The build tray, from each category of the build dock.
            groups = {}
            for group in flow.groups():
                flow.open_group(group)
                groups[group] = [text for _, text in flow.cards()]
                if group == "Work":
                    flow.shoot("palette-workplaces")
            report["palette"] = groups
            report["checks"].append({"check": "every category lists buildings with cost and purpose",
                                     "ok": bool(groups) and all(
                                         cards and all(card.count("\n") >= 2 for card in cards)
                                         for cards in groups.values()),
                                     "counts": {k: len(v) for k, v in groups.items()}})
            flow.open_group("Homes")
            flow.shoot("palette-open")

            # 2. Take the building in hand. A card's first line is its name.
            wanted = None
            for group, cards in groups.items():
                for card in cards:
                    if card.split("\n")[0].lower() == args.building.replace("_", " "):
                        wanted = (group, card.split("\n")[0])
            if wanted is None:
                raise RuntimeError(f"The build tray does not list {args.building!r}: {groups}")
            flow.open_group(wanted[0])
            flow.press_row(wanted[1], TRAY)
            name = wanted[1]

            # 3. A buildable spot near the Harbour: read from the cursor note.
            homes = flow.world().homes("Harbour")
            cx = round(sum(home["door"][0] for home in homes) / len(homes) / 1000.0)
            cz = round(sum(home["door"][1] for home in homes) / len(homes) / 1000.0)
            valid = tried = None
            ring = [(0, 0)] + [(round(radius * math.cos(turn)), round(radius * math.sin(turn)))
                               for radius in (8, 13, 18, 24, 30) for turn in
                               [index * math.pi / 6 for index in range(12)]]
            tried = []
            for dx, dz in ring:
                x, z = cx + dx, cz + dz
                line, reason = flow.aim(x, z, 34, "search")
                tried.append({"cell": [x, z], "note": line, "reason": reason})
                if line == VALID and reason is None:
                    valid = (x, z)
                    break
            report["placement_search"] = tried
            if valid is None:
                raise RuntimeError("No buildable spot was found near the Harbour")
            line, reason = flow.aim(valid[0], valid[1], 30, "ghost-valid")
            report["checks"].append({"check": "a green ghost says a click places it",
                                     "ok": line == VALID and reason is None, "note": line,
                                     "tag": flow.panel_texts(TAG)})
            flow.shoot("ghost-valid", pointer=True)

            # 4. A refusal: the entrance on top of a founding home.
            home = homes[0]
            hx, hz = round(home["door"][0] / 1000.0), round(home["door"][1] / 1000.0) - 3
            line, reason = flow.aim(hx, hz, 30, "ghost-invalid")
            report["checks"].append({"check": "a red ghost gives the simulation's reason",
                                     "ok": line == INVALID and bool(reason), "reason": reason})
            flow.shoot("ghost-invalid", pointer=True)
            before = set(flow.sites())
            flow.press("Left")
            flow.call("npc/step", {"ticks": 2})
            time.sleep(0.6)
            report["checks"].append({"check": "a click on a red ghost opens no site",
                                     "ok": set(flow.sites()) == before})

            # 5. Place it: a real click on the green ghost.
            opened = []
            for attempt in range(3):
                line, reason = flow.aim(valid[0], valid[1], 30, "place")
                if line != VALID:
                    raise RuntimeError(f"The chosen spot is no longer buildable: {line} / {reason}")
                flow.press("Left")
                time.sleep(0.5)
                flow.call("npc/step", {"ticks": 2})
                time.sleep(0.8)
                opened = [sim_id for sim_id in flow.sites() if sim_id not in before]
                if opened:
                    break
                # The click was lost to the real mouse; the ghost is still green.
                flow.record("place_retry", attempt=attempt)
            report["checks"].append({"check": "a click on a green ghost opens one site",
                                     "ok": len(opened) == 1, "sites": opened,
                                     "status": flow.panel_lines("site opened", "not placed", "requested")})
            if len(opened) != 1:
                raise RuntimeError("The click did not open exactly one site")
            site_id = opened[0]
            site = flow.sites()[site_id]
            label = site["label"]
            plot = site["plot"]
            centre = ((plot["lo_x"] + plot["hi_x"] + 1) / 2.0, (plot["lo_z"] + plot["hi_z"] + 1) / 2.0)
            flow.stamp("placed", site=site_id, **site_state(site))
            # The ghost over the new site is refused now.
            time.sleep(0.9)
            report["ghost_over_own_site"] = list(flow.note())
            flow.press("Right")
            time.sleep(0.4)
            report["checks"].append({"check": "right-click puts the building down",
                                     "ok": flow.note()[0] is None})
            flow.close_tray()

            def frame_site(tag, distance=24):
                flow.look(centre[0], centre[1], distance, tag)
                # Open ground: the bottom-left corner holds the Overlays and Places
                # dock now, and a pointer resting there shows its tooltip.
                flow.cursor(420, 130)

            frame_site("site-placed")
            flow.shoot("site-placed")

            # 6. The Projects page, then the site's card from its row: the card
            # takes the right column from the ledger.
            flow.open_page("Projects")
            time.sleep(0.6)
            report["projects_panel"] = flow.panel_texts(LEDGER)
            flow.shoot("projects-panel")
            flow.press_row(label, LEDGER)
            time.sleep(1.2)
            report["site_card"] = flow.panel_texts(BUILDING_CARD)
            report["checks"].append({"check": "choosing a project row opens the site card",
                                     "ok": any(text.startswith("Building site") for text in report["site_card"])})
            flow.shoot("site-card")
            # Abandoning asks first; Keep building leaves the site alone.
            flow.open_page("Projects")
            flow.click("Abandon " + label)
            time.sleep(0.5)
            asked = flow.has_widget("Abandon it") and flow.has_widget("Keep building")
            flow.shoot("projects-cancel-confirm")
            flow.click("Keep building")
            flow.call("npc/step", {"ticks": 2})
            time.sleep(0.5)
            report["checks"].append({"check": "Abandon asks first and Keep building keeps the site",
                                     "ok": asked and site_id in flow.sites()})
            flow.close_ledger()
            frame_site("site-run")

            # 7. Run at 10x: materials in, then complete.
            budget = int(args.days * DAY)

            def supplied():
                current = flow.sites().get(site_id)
                return current is None or site_state(current)["supplied"]

            done, spent = flow.run_until(supplied, budget)
            current = flow.sites().get(site_id)
            if current is not None:
                flow.stamp("materials delivered" if done else "materials still missing",
                           ticks_waited=spent, **site_state(current))
                frame_site("site-materials")
                flow.shoot("site-materials")
            report["checks"].append({"check": "residents delivered every material", "ok": bool(done),
                                     "ticks": spent})

            def finished():
                return site_id not in flow.sites()

            complete, more = flow.run_until(finished, max(0, budget - spent)) if done else (False, 0)
            built = flow.built().get(site_id)
            if complete and built is not None:
                flow.stamp("complete", ticks_waited=spent + more, label=built[BUILT]["label"],
                           completed_at=built[BUILT]["completed_at"],
                           household=built.get("island_household_v1"), depot=built.get(DEPOT))
                frame_site("site-complete")
                flow.shoot("site-complete")
                # The finished building's card, through a real click on it.
                flow.settle(SIZES[0])
                flow.cursor(SIZES[0][0] / 2, SIZES[0][1] / 2)
                time.sleep(0.3)
                flow.press("Left")
                time.sleep(1.0)
                report["building_card"] = flow.panel_lines("Home", "Beds ", "Lives here", "hearth", "Storage",
                                                           "Stock ", "Workplace", "Works here", "Market stall")
                report["checks"].append({"check": "a click on the finished building opens its card",
                                         "ok": bool(report["building_card"])})
                flow.shoot("building-card")
                if flow.has_widget(BUILDING_CARD + " > Close card"):
                    flow.click(BUILDING_CARD + " > Close card")
                # The same building by daylight and at night, where the hour differs.
                flow.to_phase(0.30)
                frame_site("site-complete-day")
                flow.shoot("site-complete-day")
                flow.to_phase(0.86)
                frame_site("site-complete-night")
                flow.shoot("site-complete-night")
                flow.stamp("night", household=flow.built().get(site_id, {}).get("island_household_v1"))
            else:
                current = flow.sites().get(site_id)
                state = site_state(current) if current is not None else {"gone": True}
                flow.stamp("not complete within the run", ticks_waited=spent + more, **state)
                flow.open_page("Projects")
                time.sleep(0.6)
                report["stalled_panel"] = flow.panel_texts(LEDGER)
                frame_site("site-stalled")
                flow.shoot("site-stalled")
            report["checks"].append({"check": "the site was completed by residents", "ok": bool(complete),
                                     "ticks": spent + more})
            report["alert_lines"] = flow.lines()
            report["all_buildings"] = {sim_id: parts[BUILT]["label"] for sim_id, parts in flow.built().items()}
            report["open_sites"] = {sim_id: site_state(site) for sim_id, site in flow.sites().items()}

            flow.click("Menu")
            flow.click("Quit game")
            child.wait(timeout=40)
            if child.returncode != 0:
                raise RuntimeError(f"Native game exited with {child.returncode}")
            # Whether residents finish in time is the simulation's business:
            # reported, not required of the view.
            report["ok"] = all(check["ok"] for check in report["checks"]
                               if "residents" not in check["check"] and "finished building" not in check["check"])
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("build_menu_failure", error=str(error))
            except Exception as diagnostic:
                report["diagnostic_error"] = str(diagnostic)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace[-80:]
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        log_path = run / "native.log"
        if log_path.exists():
            lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
            report["native_errors"] = [line for line in lines if "ERROR" in line or "panicked at" in line][-30:]
        args.out.mkdir(parents=True, exist_ok=True)
        copied = []
        for image in sorted((run / "shots").glob("*.png")) if (run / "shots").is_dir() else []:
            shutil.copy2(image, args.out / image.name)
            copied.append(str(args.out / image.name))
        report["copied"] = copied
        (run / "summary.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
        shutil.copy2(run / "summary.json", args.out / "summary.json")
        print(run, flush=True)


if __name__ == "__main__":
    main()
