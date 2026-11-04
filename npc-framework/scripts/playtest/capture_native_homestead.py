"""Offline native Home & farm widget rehearsal; run only after the release build.

python scripts/playtest/capture_native_homestead.py [--confirm-polish]

Creates one disposable Expanded Living scenario with no foreign controller and
no provider bindings. The initial hero position is explicitly staged near a
house plot; materials are NOT granted. All gameplay input uses actual visible,
enabled native widgets. Debug stepping owns time at 0x; no OS input is sent.
Screenshots require manual inspection and do not prove physical-input/accessibility QA.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import uuid

from capture_native import ROOT, TEXT, PARENT, WINDOW, NODE, query, rpc, scalar

DISABLED = "bevy_ui::interaction_states::InteractionDisabled"
COMPUTED = "bevy_ui::ui_node::ComputedNode"
TRANSFORM = "bevy_ui::ui_transform::UiGlobalTransform"
SCROLL = "bevy_ui::ui_node::ScrollPosition"
CAMERA = "bevy_camera::camera::Camera"
INITIAL_POSITION = [-5000, -16500]  # Declared presentation fixture, not discovered knowledge.
SIZES = ((1440, 900), (1024, 768))
MAX_TICKS = 24000
MAX_SECONDS = 12 * 60
CONFIRM_STATES = frozenset({"cottage-built", "bed2-ready", "controls-released"})


def vector(value):
    value = scalar(value)
    if isinstance(value, dict) and "x" in value and "y" in value:
        return [value["x"], value["y"]]
    if isinstance(value, list) and len(value) == 2 and all(isinstance(n, (int, float)) for n in value):
        return value
    raise RuntimeError("Unexpected reflected UI vector shape")


def translation(value):
    value = scalar(value)
    if isinstance(value, dict):
        if "translation" in value:
            return vector(value["translation"])
        if len(value) == 1:
            return translation(next(iter(value.values())))
    if isinstance(value, list) and len(value) == 6 and all(isinstance(n, (int, float)) for n in value):
        return value[-2:]
    if isinstance(value, list) and len(value) in (2, 3):
        return vector(value[-1])
    raise RuntimeError("Unexpected reflected UI transform shape")


def friendly(kind):
    return kind.removeprefix("village_").replace("_", " ")


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


class Rehearsal:
    def __init__(self, run, child, url, report, began):
        self.run, self.child, self.url, self.report = run, child, url, report
        self.deadline = began + MAX_SECONDS
        self.actor = self.window = None
        self.tick = None
        self.trace = []

    def check(self):
        if time.monotonic() >= self.deadline:
            raise RuntimeError("Native homestead fixture exceeded twelve minutes")
        if self.child.poll() is not None:
            raise RuntimeError("Owned native fixture exited before completion")

    def record(self, operation, **fields):
        self.trace.append({"operation": operation, "tick": self.tick, **fields})
        if len(self.trace) > 300:
            self.trace.pop(0)

    def call(self, method, params=None):
        self.check()
        return rpc(self.url, method, params)

    def state(self):
        return self.call("world/state")

    def observe(self):
        observed = self.call("agent/observe", {"unit": self.actor, "events": 64})
        if observed.get("omniscient") is not False:
            raise RuntimeError("Native actor observation unexpectedly became omniscient")
        if observed["sections"]["village"]["alive"] is not True:
            raise RuntimeError("Fixture hero died; stop rather than hide survival failure")
        if observed["tick"] > MAX_TICKS:
            raise RuntimeError("Native homestead fixture exceeded 24000 simulation ticks")
        return observed

    def step(self, ticks):
        self.check()
        if not 0 < ticks <= 200 or self.tick + ticks > MAX_TICKS:
            raise RuntimeError("Invalid or exhausted explicit clock budget")
        expected = self.tick + ticks
        self.call("npc/step", {"ticks": ticks})
        end = min(self.deadline, time.monotonic() + 30)
        while time.monotonic() < end:
            observed = self.observe()
            if observed["tick"] > expected:
                raise RuntimeError("Clock advanced outside the explicit debug step")
            if observed["tick"] == expected:
                self.tick = expected
                time.sleep(0.07)  # Let native pending-action/paint systems consume the new state.
                return observed
            time.sleep(0.05)
        raise RuntimeError("Explicit native clock step did not finish")

    def until(self, predicate, budget=1800, chunk=20):
        initial = self.tick
        while self.tick - initial <= budget:
            observed = self.observe()
            if predicate(observed):
                return observed
            if self.tick - initial == budget:
                break
            self.step(min(chunk, budget - (self.tick - initial)))
        self.record("condition_failed", observation=self.public_receipt(self.observe()))
        raise RuntimeError("Native action did not reach its asserted result within its tick budget")

    @staticmethod
    def inventory(observed):
        items = {}
        for item in observed["sections"]["inventory"]["items"]:
            items[item["kind"]] = items.get(item["kind"], 0) + item["stack"]
        return items

    @staticmethod
    def own_outcomes(observed, since=0):
        outcomes = []
        for entry in observed.get("recent_events", []):
            event = entry.get("event", {})
            if entry.get("unit") != observed["unit"] or entry.get("seq", 0) <= since:
                continue
            if event.get("type") not in ("ExternalAction", "PathFailed", "OrderFailed", "OrderDone", "ActionRejected"):
                continue
            row = {"seq": entry.get("seq"), "tick": entry.get("tick"), "kind": event["type"]}
            for key in ("verb", "status", "reason"):
                if isinstance(event.get(key), str):
                    row[key] = event[key][:240]
            outcomes.append(row)
        return outcomes[-12:]

    @staticmethod
    def public_receipt(observed):
        village = observed["sections"]["village"]
        return {"tick": observed["tick"], "position": observed["sections"]["self"]["pos"],
            "activity": village["activity"]["kind"], "coins": village["coins"],
            "needs": village["needs"], "pack": Rehearsal.inventory(observed),
            "own_outcomes": Rehearsal.own_outcomes(observed)}

    def ui(self):
        self.check()
        nodes = {int(row["entity"]): row["components"][NODE] for row in query(self.url, [NODE])}
        parents = {int(row["entity"]): int(scalar(row["components"][PARENT])) for row in query(self.url, [PARENT])}
        disabled = {int(row["entity"]) for row in query(self.url, with_types=[DISABLED])}

        def displayed(entity):
            seen = set()
            while entity not in seen:
                seen.add(entity)
                if nodes.get(entity, {}).get("display") == "None":
                    return False
                if entity not in parents:
                    return True
                entity = parents[entity]
            return False

        texts = [{"entity": int(row["entity"]), "parent": parents.get(int(row["entity"])),
            "text": scalar(row["components"][TEXT])} for row in query(self.url, [TEXT])
            if displayed(int(row["entity"]))]
        return texts, parents, disabled

    def text(self, label, prefix=False):
        rows, _, _ = self.ui()
        matching = [row for row in rows if row["text"].startswith(label)] if prefix else [row for row in rows if row["text"] == label]
        if len(matching) != 1:
            raise RuntimeError(f"Expected one inherited-visible text {label!r}, found {len(matching)}")
        return matching[0]

    def reveal(self, label, prefix=False):
        row = self.text(label, prefix)
        _, parents, _ = self.ui()
        scrolls = {int(item["entity"]): scalar(item["components"][SCROLL]) for item in query(self.url, [SCROLL])}
        ancestor = row["entity"]
        while ancestor in parents and ancestor not in scrolls:
            ancestor = parents[ancestor]
        if ancestor not in scrolls:
            return
        # This is view-only scrolling, equivalent to bringing a keyboard-focused
        # row into view. No authoritative component is written.
        for _ in range(3):
            geometry = {int(item["entity"]): item["components"] for item in query(self.url, [COMPUTED, TRANSFORM])}
            target = geometry.get(row["entity"])
            body = geometry.get(ancestor)
            if target is None or body is None:
                raise RuntimeError("Cannot verify clipped native widget geometry")
            center = translation(target[TRANSFORM])[1]
            height = vector(target[COMPUTED]["size"])[1]
            body_center = translation(body[TRANSFORM])[1]
            body_height = vector(body[COMPUTED]["size"])[1]
            low, high = body_center - body_height / 2 + 8, body_center + body_height / 2 - 8
            delta = min(0, center - height / 2 - low) + max(0, center + height / 2 - high)
            if abs(delta) < 1:
                return
            current = vector(next(item["components"][SCROLL] for item in query(self.url, [SCROLL]) if int(item["entity"]) == ancestor))
            self.call("world.mutate_components", {"entity": ancestor, "component": SCROLL,
                "path": "0", "value": [current[0], max(0, current[1] + delta)]})
            time.sleep(0.18)
        raise RuntimeError("Native row did not become visible inside its scroll body")

    def button(self, label, prefix=False, enabled=True):
        self.reveal(label, prefix)
        end = min(self.deadline, time.monotonic() + 3)
        while True:
            row = self.text(label, prefix)
            _, _, disabled = self.ui()
            actual = row["parent"] not in disabled
            if actual == enabled:
                return row
            if time.monotonic() >= end:
                raise RuntimeError(f"Native button {label!r} enabled={actual}; expected {enabled}")
            time.sleep(0.1)

    def click(self, label, prefix=False):
        row = self.button(label, prefix, True)
        self.record("widget", label=row["text"])
        self.call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": row["parent"]}})
        time.sleep(0.16)

    def resize(self, size):
        end = min(self.deadline, time.monotonic() + 12)
        last = {}
        for attempt in range(3):
            current = next(row["components"][WINDOW]["resolution"] for row in query(self.url, [WINDOW])
                if int(row["entity"]) == self.window)
            resolution = dict(current)
            resolution.update(scale_factor_override=1.0, physical_width=size[0], physical_height=size[1])
            # One atomic component-path replacement prevents separate Winit
            # resize events from restoring stale width/height/scale fields.
            self.call("world.mutate_components", {"entity": self.window, "component": WINDOW,
                "path": "resolution", "value": resolution})
            consecutive = 0
            attempt_end = min(end, time.monotonic() + 3)
            while time.monotonic() < attempt_end:
                current = next(row["components"][WINDOW]["resolution"] for row in query(self.url, [WINDOW])
                    if int(row["entity"]) == self.window)
                cameras = [row["components"][CAMERA] for row in query(self.url, [CAMERA])
                    if row["components"][CAMERA].get("is_active") is True]
                targets = []
                for camera in cameras:
                    info = camera.get("computed", {}).get("target_info")
                    if not isinstance(info, dict):
                        continue
                    pixels = vector(info["physical_size"])
                    viewport = camera.get("viewport")
                    view_pixels = vector(viewport["physical_size"]) if isinstance(viewport, dict) else pixels
                    targets.append({"target_pixels": pixels, "viewport_pixels": view_pixels,
                        "scale_factor": info.get("scale_factor")})
                window_pixels = [current["physical_width"], current["physical_height"]]
                last = {"window_pixels": window_pixels, "camera_targets": targets}
                settled = window_pixels == list(size) and current.get("scale_factor_override") == 1.0 and bool(targets)
                settled = settled and all(target["target_pixels"] == list(size)
                    and target["viewport_pixels"] == list(size) and target["scale_factor"] == 1.0 for target in targets)
                consecutive = consecutive + 1 if settled else 0
                if consecutive >= 3:
                    return last
                time.sleep(0.15)
            if time.monotonic() >= end:
                break
        self.record("resize_failed", requested_pixels=list(size), actual=last)
        raise RuntimeError("Atomic native resolution and computed camera viewport did not settle to requested dimensions")

    def capture(self, label, anchor=None, prefix=False):
        if self.report["polish_confirmation"] and label not in CONFIRM_STATES:
            self.report["skipped_capture_states"].append(label)
            if self.observe()["tick"] != self.tick:
                raise RuntimeError("Clock advanced outside explicit debug stepping")
            return
        for size in SIZES:
            settled = self.resize(size)
            self.text("Home & farm")
            if anchor:
                self.reveal(anchor, prefix)
            time.sleep(0.25)
            name = f"{label}-{size[0]}x{size[1]}"
            path = Path(self.call("npc/screenshot", {"path": "shots/" + name})["path"])
            end = min(self.deadline, time.monotonic() + 20)
            pixels = None
            while time.monotonic() < end:
                try:
                    with path.open("rb") as image:
                        header = image.read(24)
                        image.seek(-12, 2)
                        tail = image.read(12)
                    if header[:8] == b"\x89PNG\r\n\x1a\n" and header[12:16] == b"IHDR" and tail[4:8] == b"IEND":
                        pixels = list(struct.unpack(">II", header[16:24]))
                        break
                except (OSError, ValueError):
                    pass  # Background encoder/file replacement may briefly hold the owned path.
                time.sleep(0.05)
            if pixels is None or not path.resolve().is_relative_to(self.run.resolve()):
                raise RuntimeError("Owned screenshot was missing or outside its disposable output folder")
            if pixels != list(size):
                self.report["failed_capture_geometry"] = {"capture": name,
                    "requested_pixels": list(size), "actual_png_pixels": pixels}
                raise RuntimeError(f"PNG IHDR dimensions {pixels} do not match requested {list(size)}")
            self.report["screenshots"][name] = str(path.resolve().relative_to(ROOT.resolve()))
            self.report.setdefault("image_geometry", {})[name] = {"png_pixels": pixels, **settled}
        self.resize(SIZES[0])
        if self.observe()["tick"] != self.tick:
            raise RuntimeError("Clock advanced during view-only capture")

    def plot(self, plot_id, observed=None):
        observed = observed or self.observe()
        return next(plot for plot in observed["sections"]["homestead"]["plots"] if plot["id"] == plot_id)

    def explore(self, direction):
        first = self.observe()
        before = first["sections"]["self"]["pos"]
        since = first.get("seq", 0)
        started = self.tick
        self.click(f"Walk 6 m {direction}")
        while self.tick - started <= 500:
            observed = self.observe()
            idle = observed["sections"]["village"]["activity"]["busy"] is False
            displacement = sum((a-b)**2 for a, b in zip(before, observed["sections"]["self"]["pos"]))
            outcomes = self.own_outcomes(observed, since)
            navigation_refused = any(event["kind"] == "PathFailed"
                or (event["kind"] == "OrderFailed" and event.get("reason", "").lower().startswith(("blocked", "unreachable", "no path")))
                for event in outcomes)
            if idle and navigation_refused:
                self.record("exploration_destination_refused", direction=direction,
                    displacement_mm2=displacement, own_outcomes=outcomes,
                    recovery="continue the bounded relative route in its next direction")
                return False
            if idle and displacement >= 5000**2:
                self.record("exploration_arrived", direction=direction)
                return True
            if idle and any(event["kind"] == "ExternalAction" and event.get("verb") == "execute_orders"
                and event.get("status") in ("failed", "cancelled", "rejected") for event in outcomes):
                self.record("movement_failed_without_navigation_reason", direction=direction, own_outcomes=outcomes)
                raise RuntimeError("Native movement failed without a recognized navigation refusal; inspect the bridge outcome")
            if self.tick - started == 500:
                break
            self.step(20)
        self.record("exploration_stalled", direction=direction, observation=self.public_receipt(self.observe()))
        raise RuntimeError("Native exploration stalled without arrival or an explicit navigation refusal")

    def discover(self, predicate, directions, description):
        observed = self.observe()
        for direction in [None, *directions]:
            if direction:
                self.explore(direction)
                observed = self.observe()
            value = predicate(observed)
            if value is not None:
                self.record("discovered", kind=description)
                return value
        raise RuntimeError(f"Bounded actor-perspective exploration did not discover {description}")

    def resource(self, kind, directions):
        return self.discover(lambda obs: next((r for r in obs["sections"]["village"]["environment"]["resources"]
            if r["kind"] == kind), None), directions, kind)

    def gather_to(self, kind, wanted, directions):
        self.resource(kind, directions)
        while self.inventory(self.observe()).get(kind, 0) < wanted:
            previous = self.inventory(self.observe()).get(kind, 0)
            self.click("Gather 1 " + friendly(kind))
            self.until(lambda obs: self.inventory(obs).get(kind, 0) > previous, budget=1800)
            self.record("gathered", kind=kind, quantity=self.inventory(self.observe()).get(kind, 0) - previous)

    def money(self):
        state = self.state()  # Operator verification only, never a source for player decisions.
        carried = dropped = 0
        for entity in state["entities"]:
            components = entity.get("components", {})
            carried += components.get("island_wallet", {}).get("coins", 0)
            dropped += components.get("island_coin_purse", {}).get("coins", 0)
        fund = 0  # The declared fresh village starts with zero in its community fund.
        entries = self.call("npc/journal", {"kinds": ["Effect"], "grep": "VillageCommunityFund", "limit": 32})
        balances = [entry["kind"]["effect"]["balance"] for entry in entries
            if entry.get("kind", {}).get("effect", {}).get("type") == "VillageCommunityFund"]
        if balances:
            fund = balances[-1]
        elif state["day"] != 0:
            raise RuntimeError("Community fund balance cannot be certified outside the founding day")
        return {"wallets": carried, "dropped_purses": dropped, "community_fund": fund,
            "total": carried + dropped + fund}

    def run_fixture(self):
        state = self.state()
        visitors = [entity for entity in state["entities"]
            if entity.get("components", {}).get("island_villager", {}).get("role") in ("visitor", "Visitor")]
        if len(visitors) != 1:
            raise RuntimeError("Fixture must contain exactly one visiting hero")
        self.actor = visitors[0]["sim_id"]
        self.tick = state["tick"]
        if visitors[0]["components"].get("external_controller") is not None:
            raise RuntimeError("Foreign visitor controller present before native Play")
        self.window = int(query(self.url, with_types=["bevy_window::window::PrimaryWindow"])[0]["entity"])
        self.resize(SIZES[0])
        self.step(1)  # Operator perception scan; still no player control or material grant.
        self.click("Home & farm [F2]")
        self.capture("free-home-panel", "Play as hero")
        initial = self.observe()
        self.report["initial_actor"] = self.public_receipt(initial)
        if self.inventory(initial).get("plank", 0) or self.inventory(initial).get("log", 0):
            raise RuntimeError("No-grant fixture unexpectedly has initial building materials")
        before_money = self.money()
        self.click("Play as hero")
        pending = self.call("npc/unit", {"id": self.actor})
        if pending["components"].get("external_controller") is not None:
            raise RuntimeError("Play as hero bypassed recorded control assignment")
        self.click("Resume world")  # Multiplier remains 0: only explicit debug steps advance time.
        stable = self.observe()["tick"]
        time.sleep(0.4)
        if self.observe()["tick"] != stable:
            raise RuntimeError("The zero-multiplier host did not remain stopped")
        self.step(1)
        current = self.call("npc/unit", {"id": self.actor})["components"].get("external_controller", {})
        if current.get("id") != "native-player" or current.get("mode") not in ("Direct", "direct"):
            raise RuntimeError("Native Direct control did not apply")
        # A recorded control assignment can apply before the actor's staggered
        # perception scan. Match the bounded native fixture warmup explicitly;
        # this does not reveal the map or alter positions/materials.
        self.step(64)
        self.record("perception_warmup", explicit_ticks=64)
        self.report["initial_perception_warmup_ticks"] = 64
        plots = self.observe()["sections"]["homestead"]["plots"]
        own_pos = self.observe()["sections"]["self"]["pos"]
        candidates = [plot for plot in plots if plot["use_kind"] == "housing" and plot.get("state_known") is True]
        if not candidates:
            raise RuntimeError("Staged initial location did not reveal a housing plot")
        chosen = min(candidates, key=lambda plot: sum((a-b)**2 for a, b in zip(own_pos,
            [plot["position"]["x"], plot["position"]["z"]])))
        plot_id = chosen["id"]
        self.click(chosen["name"] + " ", prefix=True)
        self.click("Claim plot")
        self.until(lambda obs: self.plot(plot_id, obs).get("own") is True, budget=1200)
        cost = self.text("Cottage:", prefix=True)["text"]
        if "Carrying 0 planks, 0 logs" not in cost:
            raise RuntimeError("Missing-material feedback does not reflect the empty real pack")
        self.button("Build cottage", enabled=False)
        self.report["missing_input_feedback"] = {"cost_text": cost, "build_disabled": True}
        self.capture("claimed-missing-inputs", "Cottage:", prefix=True)
        cottage = self.observe()["sections"]["homestead"]["cottage"]
        required = {item["kind"]: item["count"] for item in cottage["inputs"]}
        self.click("Supplies")
        def plank_recipe(obs):
            for station in obs["sections"]["village"]["environment"]["workstations"]:
                for recipe in station["recipes"]:
                    if any(output["kind"] == "plank" for output in recipe["outputs"]):
                        return recipe
            return None
        recipe = self.discover(plank_recipe, ["north", "north", "west", "east"], "plank recipe")
        if any(item["kind"] != "log" for item in recipe["inputs"]):
            raise RuntimeError("Advertised plank recipe needs additional real inputs; fixture will not invent them")
        output = next(item["count"] for item in recipe["outputs"] if item["kind"] == "plank")
        runs = math.ceil(required["plank"] / output)
        log_count = required["log"] + runs * sum(item["count"] for item in recipe["inputs"])
        self.gather_to("log", log_count, ["west", "west", "north", "west", "north", "east"])
        current_recipe = self.discover(plank_recipe, ["east", "east", "east", "north", "east"], "plank workshop after logging")
        if current_recipe != recipe:
            raise RuntimeError("Plank recipe changed between discovery and ordinary crafting")
        for _ in range(runs):
            before = self.inventory(self.observe()).get("plank", 0)
            self.click(f"Make {output} plank")
            self.until(lambda obs: self.inventory(obs).get("plank", 0) >= before + output, budget=1800)
        self.capture("supplies-crafted", f"Make {output} plank")
        self.report["material_acquisition"] = {"free_material_grants": 0, "gathered_logs": log_count,
            "actual_recipe": recipe["name"], "craft_runs": runs, "crafted_planks": runs * output,
            "proof": "native Supplies widgets and observed inventory changes"}
        crop = self.observe()["sections"]["homestead"]["crop"]
        grain = crop["seed_kind"]
        water = crop["water_kind"]
        self.gather_to(grain, max(crop["seed_count"], self.inventory(self.observe()).get(grain, 0) + 1),
            ["north", "north", "west", "north", "west", "east"])
        self.gather_to(water, max(crop["water_count"], self.inventory(self.observe()).get(water, 0) + 1),
            ["east", "east", "east", "south", "east", "south"])
        self.click("Land & crops")
        self.click(chosen["name"] + " ", prefix=True)
        self.button("Build cottage", enabled=True)
        self.capture("materials-ready", "Cottage:", prefix=True)
        before_build = self.inventory(self.observe())
        self.click("Build cottage")
        built = self.until(lambda obs: self.plot(plot_id, obs).get("cottage_built") is True, budget=2400)
        after_build = self.inventory(built)
        for kind, count in required.items():
            if before_build.get(kind, 0) - after_build.get(kind, 0) != count:
                raise RuntimeError("Cottage did not spend its advertised real materials exactly once")
        self.capture("cottage-built", "Cottage built", prefix=True)
        self.click("Bed 2 ", prefix=True)
        before_seed = self.inventory(self.observe()).get(grain, 0)
        self.click("Plant grain", prefix=True)
        planted = self.until(lambda obs: self.plot(plot_id, obs)["beds"][1]["stage"] == "needs_water", budget=1200)
        if self.inventory(planted).get(grain, 0) != before_seed - crop["seed_count"]:
            raise RuntimeError("Plant did not consume an actual seed")
        if any(bed["stage"] != "empty" for bed in self.plot(plot_id, planted)["beds"] if bed["index"] != 1):
            raise RuntimeError("Plant changed a bed other than the chosen second bed")
        self.capture("bed2-planted", "Bed 2 ", prefix=True)
        before_water = self.inventory(self.observe()).get(water, 0)
        self.click("Water crop", prefix=True)
        watered = self.until(lambda obs: self.plot(plot_id, obs)["beds"][1]["stage"] == "growing", budget=1200)
        if self.inventory(watered).get(water, 0) != before_water - crop["water_count"]:
            raise RuntimeError("Watering did not consume actual clean water")
        self.capture("bed2-watered", "Bed 2 ", prefix=True)
        ready_at = self.plot(plot_id, watered)["beds"][1]["ready_at"]
        while self.tick < ready_at:
            self.step(min(200, ready_at - self.tick))
        if self.plot(plot_id)["beds"][1]["stage"] != "ready":
            raise RuntimeError("Crop did not mature at its advertised simulation tick")
        self.capture("bed2-ready", "Harvest", prefix=True)
        before_harvest = self.inventory(self.observe()).get(grain, 0)
        self.click("Harvest", prefix=True)
        harvested = self.until(lambda obs: self.plot(plot_id, obs)["beds"][1]["stage"] == "empty", budget=1200)
        if self.inventory(harvested).get(grain, 0) != before_harvest + crop["yield_count"]:
            raise RuntimeError("Harvest did not yield the advertised actual grain")
        self.capture("bed2-harvested", "Bed 2 ", prefix=True)
        energy = harvested["sections"]["village"]["needs"]["energy"]
        self.click("Food & trade")
        self.click("Walk home and rest")
        rested = self.until(lambda obs: obs["sections"]["village"]["activity"]["busy"] is False
            and obs["sections"]["village"]["needs"]["energy"] > energy, budget=1800)
        self.capture("home-rested", "Your pack")
        self.click("Release controls")
        self.step(1)
        if self.call("npc/unit", {"id": self.actor})["components"].get("external_controller") is not None:
            raise RuntimeError("Release controls did not release native ownership")
        self.capture("controls-released", "Play as hero")
        after_money = self.money()
        if after_money["total"] != before_money["total"]:
            raise RuntimeError("Observed wallets, dropped coins and finite fund did not conserve currency")
        self.report["currency"] = {"before": before_money, "after": after_money, "conserved": True}
        self.report["final_actor"] = self.public_receipt(rested)
        self.report["selected_bed_index"] = 1
        self.report["clock_ticks"] = self.tick
        self.report["assertions"] = ["recorded native Direct control", "known housing plot claimed", "missing real inputs disable build",
            "real logs gathered", "real planks crafted", "exact cottage costs spent", "selected second bed planted/watered/harvested",
            "growth follows ticks", "free home rest restores energy", "native controls released", "currency conserved"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--confirm-polish", action="store_true",
        help="exercise the full no-grant fixture but capture only three corrected states at both sizes")
    args = parser.parse_args()
    run = ROOT / "target" / ("playtest-native-homestead-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    scenario = json.loads((ROOT / "content/testbed_content/scenarios/living_village.json").read_text(encoding="utf-8"))
    scenario["name"] = "native_homestead_widget_fixture"
    scenario["controllers"] = []
    scenario["world"]["ai_scheduler"] = {"enabled": False}
    scenario["world"]["island_geography"] = "expanded"
    scenario["world"]["items"] = []
    scenario["actors"][0]["pos"] = INITIAL_POSITION
    scenario["actors"][0]["facing"] = [0, 1]
    scenario_path = run / "fixture.scenario.json"
    scenario_path.write_text(json.dumps(scenario, indent=2), encoding="utf-8")
    bindings = run / "empty-bindings.json"
    bindings.write_text('{"bindings":[]}', encoding="utf-8")
    environment = os.environ.copy()
    environment["NPC_AI_SCHEDULER_BINDINGS"] = str(bindings)
    for name in ("OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"):
        environment.pop(name, None)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    report = {"ok": False, "screenshots": {}, "real_provider": False, "provider_command_invocations": 0,
        "polish_confirmation": args.confirm_polish, "skipped_capture_states": [],
        "capture_scope": "selective_polish_confirmation" if args.confirm_polish else "full_22_image_matrix",
        "staging": {"initial_position_mm": INITIAL_POSITION, "plot_discovery_unaided": False, "material_grants": []},
        "clock": "0x host; UI Resume enables controls, only operator npc/step advances time",
        "input": "inherited-visible enabled widget Activate; view-only Window/ScrollPosition mutations; no OS input",
        "limits": {"sim_ticks": MAX_TICKS, "wall_seconds": MAX_SECONDS},
        "owned_native_stopped": False, "manual_png_inspection_required": True}
    child = harness = None
    began = time.monotonic()
    try:
        executable = ROOT / "target/release/actor_testbed.exe"
        report["binary_sha256"] = file_hash(executable)
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(executable), "--scenario", str(scenario_path), "--seed", "7", "--paused", "--speed", "0",
                "--fog", "on", "--remote", "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run / "npc-out")],
                cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            end = time.monotonic() + 120
            while time.monotonic() < end:
                if child.poll() is not None:
                    raise RuntimeError("Owned native exited during startup")
                try:
                    if rpc(url, "world/state").get("entities") and query(url, [TEXT]):
                        break
                except (OSError, RuntimeError):
                    pass
                time.sleep(0.2)
            else:
                raise RuntimeError("Native widget fixture startup exceeded 120 seconds")
            harness = Rehearsal(run, child, url, report, began)
            harness.run_fixture()
            if args.confirm_polish:
                expected = {f"{state}-{width}x{height}" for state in CONFIRM_STATES for width, height in SIZES}
                if set(report["screenshots"]) != expected:
                    raise RuntimeError("Polish confirmation did not capture all six required images")
            report["ai_client_directories_created"] = [str(path.relative_to(run)) for path in run.rglob(".ai-client")]
            if report["ai_client_directories_created"]:
                raise RuntimeError("Offline native fixture unexpectedly created provider-client state")
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if harness is not None:
            try:
                report["failure_actor"] = harness.public_receipt(harness.observe())
                report["failure_visible_text"] = [row["text"][:300] for row in harness.ui()[0]][-40:]
            except Exception:
                report["failure_diagnostic_unavailable"] = True
    finally:
        if child is not None:
            try:
                rpc(url, "app/quit", {"pid": child.pid})
                child.wait(timeout=20)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=10)
            report["owned_native_stopped"] = child.poll() is not None
        report["elapsed_seconds"] = round(time.monotonic() - began, 3)
        report["run"] = str(run.relative_to(ROOT))
        if harness is not None:
            report["trace"] = harness.trace
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2))
        if not report["ok"] or not report["owned_native_stopped"]:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
