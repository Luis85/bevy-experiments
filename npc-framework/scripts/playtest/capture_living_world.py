"""Batched native living-world QA, using widgets and recorded inputs.

Build separately, then run this script. It owns a disposable save and process.
Player travel uses the published chart's visible buttons and actor observations.
Inspector camera choices, terrain selection and clock stepping are explicitly
operator QA. No grants, teleports, raw sim writes or physical-input claims.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import uuid

from capture_native import ROOT, query
from capture_native_homestead import file_hash
from capture_player_flow import PlayerFlow, PRIMARY, wait_for


class LivingFlow(PlayerFlow):
    def check(self):
        if time.monotonic() >= self.deadline:
            raise RuntimeError("Living-world QA exceeded its thirty-minute wall-time budget")
        if self.child.poll() is not None:
            raise RuntimeError("Owned native game exited; inspect native.log")

    def capture(self, label, **kwargs):
        super().capture(label, **kwargs)

    def bootstrap(self):
        end, last = time.monotonic() + 150, None
        while time.monotonic() < end:
            self.check()  # A crashed host is an immediate failure, not a readiness retry.
            try:
                return self.widget("New game")
            except (OSError, RuntimeError, KeyError) as error:
                last = error
                time.sleep(0.2)
        raise RuntimeError(f"Native startup did not become ready: {last}")

    def advance(self, ticks):
        while ticks:
            batch = min(200, ticks)
            self.call("npc/step", {"ticks": batch})
            ticks -= batch
            time.sleep(0.06)

    def travel(self, name):
        self.call("npc/resume")
        if not any(row["text"] == "Explore the island" for row in self.ui()[0]):
            self.open_tab("Explore · M")
        before = self.observe()
        chart = before["sections"]["living_world"]["chart"]
        destination = next(row for row in chart if row["name"] == name)
        target = self.position(destination["position"])
        self.record("chart_travel", destination=name, target=target,
                    source="published settlement chart in actor observation")
        self.click(name if self.has_widget(name, enabled=True) else "Travel to " + name)
        arrived = self.until(lambda observed:
            sum((a-b)**2 for a, b in zip(self.position(observed["sections"]["self"]["pos"]), target)) < 1800**2
            and not observed["sections"]["village"]["activity"]["busy"], budget=4500, chunk=100)
        self.report.setdefault("journeys", []).append({"destination": name,
            "before": self.public_receipt(before), "after": self.public_receipt(arrived)})
        self.click("Close")
        self.capture("character-" + name.lower().replace(" ", "-"))
        self.open_tab("Explore · M")
        self.capture("chart-" + name.lower().replace(" ", "-"))

    def region(self, name):
        # The settlements are rows of the ledger's Village page.
        self.open_page("Village")
        rows, _, _ = self.ui()
        label = next(row["text"] for row in rows if row["text"].startswith(name + " · ")
                     and row["text"].endswith(" residents"))
        self.click(label)

    def save_from_inspector(self, save):
        self.click("Menu")
        self.click("Save game")
        snapshot = json.loads(save.read_text(encoding="utf-8"))
        self.click("Resume")
        return snapshot

    def retained_homes(self, phase):
        state = self.state()  # Explicit inspector verification, not player knowledge.
        expected = {entity["components"]["island_household_v1"]["label"]
            for entity in state["entities"]
            if "island_household_v1" in entity.get("components", {})
            and "island_homestead_cottage_v1" not in entity.get("components", {})}
        def strings(value):
            if isinstance(value, str):
                yield value
            elif isinstance(value, dict):
                for item in value.values():
                    yield from strings(item)
            elif isinstance(value, list):
                for item in value:
                    yield from strings(item)
        names = {name for row in query(self.url, ["bevy_ecs::name::Name"])
                 for name in strings(row["components"]["bevy_ecs::name::Name"])}
        missing = sorted(expected - names)
        self.report.setdefault("home_visual_checks", []).append({"phase": phase,
            "expected": len(expected), "retained": len(expected & names), "missing": missing})
        if missing or len(expected) != 42:
            raise RuntimeError(f"Settlement home visuals were lost: {missing}")


def terrain_candidate(snapshot, operator_state):
    terrain = snapshot["resources"]["living_world_terrain_v1"]
    width, height = terrain["width"], terrain["height"]
    protected = []
    for entity in operator_state["entities"]:
        components = entity.get("components", {})
        if any(key in components for key in ["island_place", "island_resource_patch",
                                             "island_villager", "island_household_v1",
                                             "island_homestead_plot_v1"]):
            pos = components.get("pos")
            if pos is not None:
                protected.append(LivingFlow.position(pos))
    choices = []
    for index, cell in enumerate(terrain["cells"]):
        x, z = index % width - width // 2, index // width - height // 2
        if str(cell["surface"]).lower() != "grass" or not (-18 <= x <= 18 and -48 <= z <= -12):
            continue
        if any((x*1000+500-p[0])**2 + (z*1000+500-p[1])**2 < 8000**2 for p in protected):
            continue
        choices.append(((x*x+(z+30)**2), x, z, cell["height_mm"]))
    if not choices:
        raise RuntimeError("No safe unoccupied grass patch found for the explicitly labelled editor phase")
    _, x, z, elevation = min(choices)
    return {"type": "Terraform", "x": x, "z": z, "radius": 1,
            "height_mm": elevation, "surface": "stone"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--skip-travel", action="store_true", help="capture UI and inspector regions only; no player journey claim")
    parser.add_argument("--confirm", action="store_true", help="single bounded confirmation of the same critical states")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build game_menu before running native QA")
    run = ROOT / "target" / ("playtest-living-world-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    save = run / "living-world.world.json"
    settings = run / "settings.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "worlds_dir": str(run / "rules"), "fog_of_war": True}), encoding="utf-8")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "native BRP widget activation; view-only sizing; recorded operator clock/editor commands",
        "physical_input_tested": False, "provider": False, "grants": 0, "teleports": 0,
        "screenshots": {}, "checks": [], "confirmation": args.confirm,
        "player_travel_requested": not args.skip_travel}
    child = flow = None
    environment = os.environ.copy()
    environment["BEVY_ASSET_ROOT"] = str(ROOT)
    report["asset_root"] = str(ROOT / "assets")
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--settings", str(settings),
                "--island-save", str(save), "--remote", "--remote-port", str(port),
                "--no-render-remote", "--out-dir", str(run)], cwd=ROOT, env=environment,
                stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = LivingFlow(run, child, url, report, time.monotonic())
            flow.deadline = time.monotonic() + 30*60
            flow.tick_budget = 60000
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            try:
                query(url, with_types=["bevy_animation::AnimationPlayer"])
            except RuntimeError as error:
                report["character_readiness_query_unavailable"] = str(error)
            else:
                wait_for(lambda: len(query(url, with_types=["bevy_animation::AnimationPlayer"])) >= 42, 45)
                report["loaded_character_animation_players"] = len(query(url, with_types=["bevy_animation::AnimationPlayer"]))
            flow.capture("title")
            flow.click("New game")
            flow.capture("new-character")
            flow.click("Start on the island")
            wait_for(lambda: flow.widget("Menu"))
            flow.advance(4)
            visitors = [entity for entity in flow.state()["entities"]
                        if entity.get("components", {}).get("island_villager", {}).get("role") in ("visitor", "Visitor")]
            if len(visitors) != 1:
                raise RuntimeError("Expected exactly one arrived player")
            flow.actor = visitors[0]["sim_id"]
            report["actor"] = flow.actor
            report["arrival"] = flow.public_receipt(flow.observe())
            flow.capture("arrival-harbour")
            for tab, name in [("Explore · M", "explore"), ("People · N", "people"),
                              ("Craft · C", "craft"), ("Build · B", "build"), ("Garden · G", "garden")]:
                flow.open_tab(tab)
                flow.capture(name)
            if not args.skip_travel:
                for name in ["Alderwood", "Stone Ridge", "Harbour"]:
                    flow.travel(name)
                report["checks"].append("Three chart destinations reached by actual player movement through native widgets")
            flow.click("Menu")
            flow.click("Save game")
            flow.click("Save and title")
            flow.capture("saved-title")
            flow.click("Continue")
            wait_for(lambda: flow.widget("Menu"))
            flow.capture("continued-player")
            flow.click("Menu")
            flow.click("Oversee the village")
            flow.advance(4)
            flow.capture("inspector-overview")
            if args.confirm:
                flow.retained_homes("continued-player-inspector")
            for name in ["Harbour", "Alderwood", "Stone Ridge"]:
                flow.region(name)
                flow.capture("inspector-" + name.lower().replace(" ", "-"))
            try:
                flow.call("npc/perf/reset")
                flow.call("npc/resume")
                time.sleep(8)
                flow.call("npc/pause")
                profile = flow.call("npc/perf", {"tree": True, "capture": True})
                (run / "native-cpu-profile.json").write_text(json.dumps(profile, indent=2), encoding="utf-8")
                report["performance"] = {"capture": str(run / "native-cpu-profile.json"),
                    "scope": "eight seconds of running inspector CPU/system profiling; GPU frame rate not measured"}
            except RuntimeError as error:
                flow.call("npc/pause")
                report["performance"] = {"unavailable": str(error)}
            operator_state = flow.state()
            residents = [entity for entity in operator_state["entities"]
                         if "regional_resident_profile_v1" in entity.get("components", {})]
            report["resident_count"] = len(residents)
            if len(residents) < 3:
                raise RuntimeError("Expected regional resident identities for the model inspection")
            for appearance in [0, 1]:
                person = next((entity for entity in residents
                    if entity["components"]["regional_resident_profile_v1"]["appearance"] % 8 == appearance), None)
                if person is None:
                    continue
                # The roster is the People page; a row hands the right column
                # to that resident's inspector.
                flow.open_page("People")
                name = person["components"]["island_villager"]["name"]
                flow.press_row(name + " · ")
                time.sleep(0.8)
                flow.capture("resident-model-" + str(appearance))
            flow.region("Harbour")
            before = flow.save_from_inspector(save)
            flow.click("Shape")
            flow.capture("landscape-tools")
            command = terrain_candidate(before, flow.state())
            receipt = flow.call("npc/command", {"cmd": command})
            flow.advance(2)
            flow.capture("landscape-edited")
            after = flow.save_from_inspector(save)
            old_revision = before["resources"]["living_world_terrain_v1"]["revision"]
            new_revision = after["resources"]["living_world_terrain_v1"]["revision"]
            if new_revision != old_revision + 1:
                raise RuntimeError("Recorded regional editor brush did not apply exactly once")
            report["terrain_edit"] = {"command": command, "receipt": receipt,
                "revision": new_revision, "scope": "operator recorded command, not physical brush input"}
            flow.click("Menu")
            flow.click("Save and title")
            flow.click("New game")
            flow.click("Oversee the village")
            wait_for(lambda: flow.widget("Live"))
            flow.call("npc/pause")
            flow.advance(200)
            flow.actor = None
            flow.capture("new-spectator-world")
            if args.confirm:
                flow.retained_homes("second-new-spectator")
            state = flow.state()
            arrivals = [e for e in state["entities"] if "island_awaiting_arrival_v1" in e.get("components", {})]
            if len(arrivals) != 1:
                raise RuntimeError("New spectator world did not preserve its reserved arrival")
            flow.region("Alderwood")
            flow.advance(17500)
            flow.capture("alderwood-night")
            flow.region("Harbour")
            flow.capture("harbour-night")
            state = flow.state()
            if not any("island_awaiting_arrival_v1" in e.get("components", {}) for e in state["entities"]):
                raise RuntimeError("Night inspection consumed the reserved arrival")
            report["spectator_night"] = {"tick": state["tick"], "arrival_preserved": True,
                "clock": "bounded operator stepping while unarrived NPC world runs"}
            flow.click("Menu")
            flow.click("Quit game")
            child.wait(timeout=30)
            if child.returncode != 0:
                raise RuntimeError(f"Game exited with {child.returncode}")
            report["checks"].extend(["Title, New, play, Save, Title and Continue retain one native window",
                "Inspector regions and terrain edit captured at both sizes",
                "Second New replaces simulation while retaining regional scenery and an unarrived spectator"])
            report["saved_tick"] = json.loads(save.read_text(encoding="utf-8"))["tick"]
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("living_world_qa_failure", error=str(error))
            except Exception as diagnostic:
                report["diagnostic_error"] = str(diagnostic)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        log_path = run / "native.log"
        if log_path.exists():
            lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
            report["native_errors"] = [line for line in lines if "ERROR" in line or "panicked at" in line][-30:]
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(run, flush=True)


if __name__ == "__main__":
    main()
