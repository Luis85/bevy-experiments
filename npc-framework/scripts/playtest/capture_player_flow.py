"""One-window Island Village flow through actual native widgets.

Build separately, then run:
  python scripts/playtest/capture_player_flow.py --binary target/debug/game_menu.exe
  python scripts/playtest/capture_player_flow.py --build-cottage
  python scripts/playtest/capture_player_flow.py --life-loop --skip-trade
  python scripts/playtest/capture_player_flow.py --trade-only
  python scripts/playtest/capture_player_flow.py --life-loop --recorded-exploration

Uses a disposable settings/rules/save folder. No provider, grants, teleports or
raw simulation writes. Ordinary material work uses actor observations and
enabled widgets, with bounded operator clock steps while normal 1x stays active.
An explicit --recorded-exploration flag permits ordinary actor-relative Move
orders only when the directional widgets are unavailable; life actions remain
on visible widgets and physical WASD/right-click is not claimed.
--spectator verifies unarrived inspection before joining. --death-check is a
separate operator-induced mortality phase using recorded SetNeed commands.
--resume-save copies a prior snapshot and never writes the original.
Screenshots need visual review; BRP activation is not physical-input testing.
"""
import argparse
import json
import math
import os
import re
from pathlib import Path
import socket
import shutil
import struct
import subprocess
import time
import uuid

from capture_native import ROOT, WINDOW, query
from capture_native_homestead import Rehearsal, file_hash

BUTTON = "bevy_ui_widgets::button::Button"
PRIMARY = "bevy_window::window::PrimaryWindow"
SIZES = ((1280, 720), (1024, 768))
MAX_TICKS = 18000


def wait_for(predicate, seconds=30):
    end, last = time.monotonic() + seconds, None
    while time.monotonic() < end:
        try:
            result = predicate()
            if result:
                return result
        except (OSError, RuntimeError, KeyError, StopIteration) as error:
            last = error
        time.sleep(0.15)
    raise RuntimeError(f"Native flow readiness timed out: {last}")


def friendly(kind):
    name = kind.removeprefix("village_").replace("_", " ")
    return name[:1].upper() + name[1:]


class PlayerFlow(Rehearsal):
    tick_budget = MAX_TICKS
    recorded_exploration = False

    def assert_window(self):
        windows = query(self.url, with_types=[PRIMARY])
        if len(windows) != 1 or len(query(self.url, [WINDOW])) != 1 or int(windows[0]["entity"]) != self.window:
            raise RuntimeError("Title and play did not retain exactly one primary window")

    def widget(self, label, enabled=True):
        rows, _, disabled = self.ui()
        widgets = {int(row["entity"]) for row in query(self.url, with_types=[BUTTON])}
        matches = [row for row in rows if row["text"] == label and row["parent"] in widgets]
        if len(matches) != 1:
            raise RuntimeError(f"Expected one visible widget {label!r}, found {len(matches)}")
        row = matches[0]
        if (row["parent"] not in disabled) != enabled:
            raise RuntimeError(f"Widget {label!r} is not enabled={enabled}")
        return row

    def has_widget(self, label, enabled=None):
        rows, _, disabled = self.ui()
        widgets = {int(row["entity"]) for row in query(self.url, with_types=[BUTTON])}
        matches = [row for row in rows if row["text"] == label and row["parent"] in widgets]
        return len(matches) == 1 and (enabled is None or (matches[0]["parent"] not in disabled) == enabled)

    def select_site(self, chosen, tab):
        self.open_tab(tab)
        if self.has_widget("Change site"):
            self.click("Change site")
        self.click(chosen["name"])

    def select_person(self, person):
        self.open_tab("People · N")
        if self.has_widget("Change neighbour"):
            self.click("Change neighbour")
        self.click(person["name"])

    def text(self, label, prefix=False):
        rows, _, _ = self.ui()
        widgets = {int(row["entity"]) for row in query(self.url, with_types=[BUTTON])}
        matches = [row for row in rows if row["parent"] in widgets
                   and (row["text"].startswith(label) if prefix else row["text"] == label)]
        if len(matches) == 1:
            return matches[0]
        return super().text(label, prefix)

    def click(self, label, prefix=False):
        if prefix:
            raise RuntimeError("Player flow requires exact widget labels")
        try:
            row = wait_for(lambda: self.widget(label), 6)
            # The shared helper checks clipping and scrolls only view nodes. The
            # text override disambiguates a selected plot's matching heading.
            self.reveal(label)
            row = self.widget(label)
        except (RuntimeError, KeyError) as error:
            self.failure_context("widget_unavailable", label=label, error=str(error))
            raise
        self.record("widget", label=label, entity=row["parent"])
        self.call("world.trigger_event", {
            "event": "bevy_ui_widgets::Activate", "value": {"entity": row["parent"]}})
        time.sleep(0.2)
        if label != "Quit game":
            self.assert_window()
        # Quit is verified by the process exit code and durable save below.
        # Its expected window/BRP shutdown cannot satisfy a live-window query.

    def raw_observe(self, unit=None):
        observed = self.call("agent/observe", {"unit": self.actor if unit is None else unit, "events": 96})
        if observed.get("omniscient") is not False:
            raise RuntimeError("Actor observation unexpectedly became omniscient")
        if observed["tick"] > self.tick_budget:
            raise RuntimeError(f"Player flow exceeded its {self.tick_budget}-tick budget")
        self.tick = observed["tick"]
        return observed

    def observe(self):
        observed = self.raw_observe()
        village = observed["sections"]["village"]
        if village["alive"] is not True:
            raise RuntimeError("Natural-play hero died; stop rather than hide survival failure")
        needs = village["needs"]
        if min(needs.get("hunger", 100000), needs.get("thirst", 100000)) < 5000:
            self.record("critical_survival_stop", observation=self.public_receipt(observed))
            raise RuntimeError("Food/water became critical; stop instead of hiding a survival failure")
        if not hasattr(self, "known_workplaces"):
            self.known_workplaces = {}
        for place in village["environment"]["workstations"]:
            position = self.observed_position(observed, place["id"])
            if position is not None:
                self.known_workplaces[place["id"]] = {"id": place["id"], "name": place["name"],
                    "position": position, "accepts": place.get("accepts", []), "observed_at": observed["tick"]}
        return observed

    def feedback_strings(self):
        """Read the same visible feedback a player sees, never private UI state."""
        rows, parents, _ = self.ui()
        name_type = "bevy_ecs::name::Name"

        def contains(value, text):
            if isinstance(value, str):
                return value == text
            if isinstance(value, dict):
                return any(contains(item, text) for item in value.values())
            if isinstance(value, list):
                return any(contains(item, text) for item in value)
            return False

        try:
            roots = {int(row["entity"]) for row in query(self.url, [name_type])
                     if contains(row["components"][name_type], "Player context")}
        except RuntimeError:
            roots = set()
        selected = []
        for row in rows:
            entity, seen = row["entity"], set()
            while entity not in seen:
                seen.add(entity)
                if entity in roots:
                    selected.append(str(row["text"]))
                    break
                if entity not in parents:
                    break
                entity = parents[entity]
        # If Name reflection is unavailable, retain visible text as explicitly
        # labelled diagnostic context instead of losing the original failure.
        return list(dict.fromkeys(selected or [str(row["text"]) for row in rows]))[:48]

    def failure_context(self, operation, **fields):
        """Diagnostic evidence stays within visible UI and actor observation."""
        try:
            rows, _, disabled = self.ui()
            fields["visible_text"] = [{"text": str(row["text"])[:500],
                                       "parent_disabled": row["parent"] in disabled} for row in rows[:120]]
        except (OSError, RuntimeError, KeyError) as error:
            fields["visible_text_error"] = str(error)
        if self.actor is not None:
            try:
                observed = self.raw_observe()
                village = observed["sections"]["village"]
                environment = village["environment"]
                fields["actor_observation"] = self.public_receipt(observed)
                fields["observed_sections"] = {
                    "resources": environment["resources"],
                    "workstations": [{"id": station["id"], "name": station["name"],
                        "position_in_same_observation": self.observed_position(observed, station["id"]),
                        "recipes": station["recipes"]} for station in environment["workstations"]],
                    "plots": observed["sections"].get("homestead", {}).get("plots", []),
                    "nearby_market": village.get("nearby_market", []),
                    "craft_affordances": [offer for offer in observed.get("affordances", [])
                        if str(offer.get("verb", "")).startswith("craft:")],
                }
            except (OSError, RuntimeError, KeyError) as error:
                fields["actor_observation_error"] = str(error)
        self.record(operation, **fields)

    def until(self, predicate, budget=1800, chunk=40):
        initial = self.observe()["tick"]
        while True:
            observed = self.observe()
            if predicate(observed):
                return observed
            if observed["tick"] - initial >= budget:
                self.record("condition_failed", observation=self.public_receipt(observed),
                            visible_feedback=self.feedback_strings())
                raise RuntimeError("Native work did not finish within the bounded tick budget")
            count = min(chunk, 200, budget - (observed["tick"] - initial))
            self.call("npc/step", {"ticks": count})
            time.sleep(0.18)  # Let native approach/action projection run between steps.

    def capture(self, label, anchor=None, prefix=False):
        self.assert_window()
        for size in SIZES:
            try:
                geometry = self.resize(size)
            except RuntimeError:
                # Winit can retain the old render target after an identical
                # reflected size request. Force one distinct size, then require
                # the original exact window/camera/PNG geometry again.
                self.record("capture_resize_retry", requested_pixels=list(size))
                self.resize((size[0] + 1, size[1]))
                geometry = self.resize(size)
            time.sleep(0.35)
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
            if pixels != list(size) or not path.resolve().is_relative_to(self.run.resolve()):
                raise RuntimeError("Screenshot geometry/path differs from the requested owned capture")
            self.report["screenshots"][name] = str(path.resolve())
            self.report.setdefault("image_geometry", {})[name] = {"png_pixels": pixels, **geometry}
            print(f"captured {name}", flush=True)
        self.resize(SIZES[0])

    def open_tab(self, label):
        self.click(label)

    def spectator_entry(self):
        self.click("Oversee the village")
        wait_for(lambda: self.widget("Play character"))
        before = self.state()  # Operator verification, never navigation input.
        arrivals = [entity for entity in before["entities"]
                    if "island_awaiting_arrival_v1" in entity.get("components", {})]
        if len(arrivals) != 1:
            raise RuntimeError("Spectator entry did not preserve exactly one reserved arrival")
        visitor = arrivals[0]["sim_id"]
        for component in ["island_villager", "island_vitality", "needs"]:
            if component in arrivals[0]["components"]:
                raise RuntimeError("Spectator entry started a character life")
        resident_needs = {entity["sim_id"]: entity.get("components", {}).get("needs")
                          for entity in before["entities"] if "island_villager" in entity.get("components", {})}
        self.capture("spectator-before-arrival")
        self.call("npc/step", {"ticks": 200})
        after = self.state()
        pending = next(entity for entity in after["entities"] if entity["sim_id"] == visitor)
        if "island_awaiting_arrival_v1" not in pending["components"] or "island_villager" in pending["components"]:
            raise RuntimeError("Watching the village consumed the reserved arrival")
        changed = [entity["sim_id"] for entity in after["entities"]
                   if entity["sim_id"] in resident_needs and entity.get("components", {}).get("needs") != resident_needs[entity["sim_id"]]]
        if after["tick"] <= before["tick"] or not changed:
            raise RuntimeError("NPC needs and simulation time did not progress during spectator play")
        self.report["spectator_entry"] = {"reserved_visitor": visitor, "before_tick": before["tick"],
            "after_tick": after["tick"], "npc_needs_changed": len(changed), "arrival_preserved": True}
        self.capture("spectator-running-village")
        self.click("Play character")
        return visitor

    def death_check(self):
        """A separate, explicitly induced QA phase after ordinary play evidence."""
        before = self.observe()
        old = self.actor
        start = before["tick"]
        if self.tick_budget - start < 2200:
            raise RuntimeError("Not enough remaining bounded ticks for the isolated mortality check; run --death-check separately")
        state = self.state()  # Identity/legacy verification only; never travel.
        estates = {entity["sim_id"]: entity["components"]["island_homestead_plot_v1"]
                   for entity in state["entities"] if entity.get("components", {}).get("island_homestead_plot_v1", {}).get("owner") == old}
        self.click("Stop · X")
        interventions = []
        for need in ["Hunger", "Thirst"]:
            command = {"type": "SetNeed", "unit": old, "need": need, "value": 0}
            receipt = self.call("npc/command", {"cmd": command})
            interventions.append({"command": command, "receipt": receipt})
        self.report["death_check"] = {"completed": False, "operator_induced": True,
            "scope": "isolated final QA phase in the disposable harness island; not natural survival evidence",
            "interventions": interventions, "old_actor": old, "natural_play_ended_at": start,
            "maximum_mortality_ticks": 1800}
        while True:
            observed = self.raw_observe(old)
            if observed["sections"]["village"]["alive"] is False:
                break
            if observed["tick"] - start >= 1800:
                raise RuntimeError("Recorded depleted needs did not reach actual mortality within 1800 ticks")
            self.call("npc/step", {"ticks": 100})
            time.sleep(0.18)
        died_at = observed["sections"]["village"]["died_at"]
        wait_for(lambda: self.widget("Keep watching the village"))
        self.capture("operator-induced-mortality")
        self.click("Keep watching the village")
        wait_for(lambda: self.widget("Life choices · L"))
        watching_tick = self.raw_observe(old)["tick"]
        self.capture("watching-after-death")
        self.call("npc/step", {"ticks": 100})
        corpse = self.raw_observe(old)["sections"]["village"]
        if corpse["alive"] is not False or corpse["died_at"] != died_at:
            raise RuntimeError("Watching the village revived or replaced the old life")
        if self.tick <= watching_tick:
            raise RuntimeError("The village did not run while watching after death")
        self.click("Life choices · L")
        self.capture("successor-life-choices")
        self.click("Begin a successor life")

        def successor():
            living = [entity for entity in self.state()["entities"]
                      if entity["sim_id"] != old
                      and entity.get("components", {}).get("island_villager", {}).get("role") in ("Visitor", "visitor")
                      and entity.get("components", {}).get("island_vitality", {}).get("died_at") is None]
            return living[0]["sim_id"] if len(living) == 1 else None

        self.actor = wait_for(successor)
        wait_for(lambda: self.widget("Menu · Esc"))
        self.call("npc/step", {"ticks": 20})
        living = self.observe()
        if living["sections"]["village"]["life"] != before["sections"]["village"]["life"] + 1:
            raise RuntimeError("The successor did not begin exactly one new life")
        corpse = self.raw_observe(old)["sections"]["village"]
        if corpse["alive"] is not False or corpse["died_at"] != died_at:
            raise RuntimeError("The old actor was revived during succession")
        after = {entity["sim_id"]: entity for entity in self.state()["entities"]}
        for identity, estate in estates.items():
            current = after[identity]["components"]["island_homestead_plot_v1"]
            if current["cottage_built"] != estate["cottage_built"] or current.get("cottage_entity") != estate.get("cottage_entity"):
                raise RuntimeError("Succession discarded a physical legacy cottage")
        self.report["death_check"].update(completed=True, died_at=died_at, successor=self.actor,
            old_actor_still_dead=True, legacy_estates_preserved=len(estates), successor_life=living["sections"]["village"]["life"])
        self.capture("successor-on-island")

    @staticmethod
    def position(value):
        return [value["x"], value["z"]] if isinstance(value, dict) else value

    @staticmethod
    def observed_position(observed, target):
        for group in ["sensed", "remembered"]:
            for thing in observed["sections"].get("perception", {}).get(group, []):
                if thing.get("id") == target and thing.get("pos") is not None:
                    return list(PlayerFlow.position(thing["pos"]))
        for offer in observed.get("affordances", []):
            if offer.get("target") == target and offer.get("pos") is not None:
                return list(PlayerFlow.position(offer["pos"]))
        return None

    @staticmethod
    def observed_plank_workshop(observed):
        for station in observed["sections"]["village"]["environment"]["workstations"]:
            position = PlayerFlow.observed_position(observed, station["id"])
            if position is None:
                continue
            for recipe in station["recipes"]:
                if any(output["kind"] == "plank" for output in recipe["outputs"]):
                    return {"station_id": station["id"], "name": station["name"],
                            "position": position, "observed_at": observed["tick"], "recipe": recipe}
        return None

    @staticmethod
    def refreshed_recipe(observed, remembered):
        for station in observed["sections"]["village"]["environment"]["workstations"]:
            if station["id"] != remembered["station_id"]:
                continue
            for recipe in station["recipes"]:
                if recipe["name"] == remembered["recipe"]["name"]:
                    if recipe != remembered["recipe"]:
                        raise RuntimeError("The observed workshop recipe changed; stop instead of assuming old material costs")
                    return recipe
        return None

    def return_to_workshop(self, remembered, max_moves=12):
        target = remembered["position"]
        self.record("return_to_observed_workshop", station_id=remembered["station_id"],
                    remembered_position=target, observed_at=remembered["observed_at"], maximum_moves=max_moves)
        blocked_direction = None
        for move in range(max_moves + 1):
            observed = self.observe()
            recipe = self.refreshed_recipe(observed, remembered)
            if recipe is not None:
                self.record("workshop_recipe_reobserved", station_id=remembered["station_id"],
                            recipe=recipe["name"], moves=move)
                return recipe
            if move == max_moves:
                break
            here = self.position(observed["sections"]["self"]["pos"])
            dx, dz = target[0] - here[0], target[1] - here[1]
            direction = ("east" if dx > 0 else "west") if abs(dx) > abs(dz) else ("north" if dz > 0 else "south")
            if direction == blocked_direction:
                direction = ("north" if dz >= 0 else "south") if direction in ("east", "west") else ("east" if dx >= 0 else "west")
            arrived = self.exploration_step(direction, remembered["name"],
                "return toward workshop position retained from the actor's earlier observation")
            if arrived is None:
                break
            blocked_direction = None if arrived else direction
        self.failure_context("workshop_return_failed", remembered_workshop=remembered)
        raise RuntimeError("Harness could not reobserve the remembered workshop recipe within twelve validated relative moves")

    @staticmethod
    def craft_affordance(observed, remembered):
        return next((offer for offer in observed.get("affordances", [])
                     if offer.get("verb") == "craft:" + remembered["recipe"]["name"]
                     and offer.get("target") == remembered["station_id"]), None)

    @staticmethod
    def occupancy_reason(text):
        return any(word in text.lower() for word in ["in use", "occupied", "reserved"])

    def wait_for_workshop(self, remembered, budget=600):
        initial = self.observe()["tick"]
        while True:
            observed = self.observe()
            offer = self.craft_affordance(observed, remembered)
            if offer is None or not self.occupancy_reason(str(offer.get("why_not", ""))):
                return
            if observed["tick"] - initial >= budget:
                self.failure_context("workshop_occupancy_timeout", station_id=remembered["station_id"], affordance=offer)
                raise RuntimeError("Observed workstation remained occupied through the bounded wait; reservations were not bypassed")
            self.call("npc/step", {"ticks": 40})
            time.sleep(0.18)

    def craft_batch(self, remembered, label, output):
        for attempt in range(1, 4):  # One initial action, at most two busy retries.
            self.care()
            self.return_to_workshop(remembered)
            self.wait_for_workshop(remembered)
            self.open_tab("Craft · C")
            before = self.observe()
            quantity = self.inventory(before).get("plank", 0)
            previous_feedback = self.feedback_strings()
            self.click(label)
            while True:
                observed = self.observe()
                if self.inventory(observed).get("plank", 0) >= quantity + output:
                    self.record("craft_batch_completed", station_id=remembered["station_id"], attempt=attempt,
                                recipe=remembered["recipe"]["name"], output=output)
                    return
                outcomes = self.own_outcomes(observed, before.get("seq", 0))
                refusal = [event for event in outcomes if event["kind"] in ("ActionRejected", "PathFailed", "OrderFailed")
                           or (event["kind"] == "ExternalAction" and event.get("status") in ("failed", "rejected", "cancelled"))]
                feedback = self.feedback_strings()
                idle = observed["sections"]["village"]["activity"]["busy"] is False
                terminal_text = [text for text in feedback if text.startswith((
                    "At destination:", "Action refused:", "Approach failed", "Approach rejected",
                    "Approach cancelled", "Approach interrupted", "Approach made no progress", "Movement unavailable:"))
                    and (text not in previous_feedback or refusal)]
                if idle and (refusal or terminal_text):
                    offer = self.craft_affordance(observed, remembered)
                    reason = " ".join([*terminal_text, *[str(event.get("reason", "")) for event in refusal],
                                       str((offer or {}).get("why_not", ""))])
                    retry = self.occupancy_reason(reason) and attempt < 3
                    self.record("craft_batch_refused", station_id=remembered["station_id"], attempt=attempt,
                                retrying_occupancy=retry, reason=reason, own_outcomes=refusal, visible_feedback=feedback)
                    if retry:
                        self.wait_for_workshop(remembered)
                        break
                    self.failure_context("craft_terminal_refusal", station_id=remembered["station_id"], attempt=attempt)
                    raise RuntimeError("Native crafting was refused; inspect observed availability and visible feedback")
                if observed["tick"] - before["tick"] >= 1800:
                    self.failure_context("craft_batch_timeout", station_id=remembered["station_id"], attempt=attempt)
                    raise RuntimeError("Craft did not finish within its bounded tick budget")
                self.call("npc/step", {"ticks": 40})
                time.sleep(0.18)

    @staticmethod
    def discovery_route(resource_kind=None, southern_hint=False):
        # Compass sweeps express public regional directions, not hidden-source
        # coordinates. Each leg is one normal six-metre move from observed self.
        if southern_hint:
            return ["south"] * 5 + ["west", "east", "south"], "authored southern house lane hint"
        if resource_kind == "log":
            return ["west", "west", "south", "west", "west", "north", "north", "west"], "authored western timber yard hint; bounded western sweep"
        if resource_kind == "village_berries":
            return ["east", "south", "east", "south", "east", "east", "north", "east"], "authored southeastern berry bushes hint; bounded southeastern sweep"
        if resource_kind == "village_fiber":
            # There is no precise public fibre location. Expand an honest
            # compass spiral around the actor's observed arrival history;
            # every waypoint is still a validated relative six-metre walk.
            directions = ["west", "north", "east", "south"]
            route = [directions[leg % 4] for leg in range(10) for _ in range(leg // 2 + 1)]
            return route, "bounded expanding compass search from actor-observed history; fibre coordinates unknown"
        if resource_kind == "village_grain":
            return ["north", "west", "west", "west", "north", "east", "east", "east", "east", "east", "east", "north", "west", "west", "west"], "public North fields landmark; bounded northern fan, then northeast"
        return ["east", "north", "west", "west", "south", "south", "east", "east", "east", "north", "north", "north"], "bounded expanding search from an observed central anchor"

    def exploration(self, predicate, description, southern_hint=False, max_moves=None, resource_kind=None):
        observed = self.observe()
        found = predicate(observed)
        if found:
            return found
        anchor = getattr(self, "arrival_anchor", None)
        if anchor is None:
            raise RuntimeError("Exploration has no actor-observed arrival anchor")
        route, basis = self.discovery_route(resource_kind, southern_hint)
        budget = len(route) if max_moves is None else min(max_moves, len(route))
        self.record("discovery_phase", wanted=description, known_anchor=anchor,
                    anchor_source="actor-owned arrival observation", return_moves=8,
                    search_moves=budget, basis=basis)
        # Return towards the remembered arrival area instead of searching a tiny
        # loop around whichever distant plot or person was visited most recently.
        # A failed destination gets one perpendicular detour; the nav solver still
        # owns the route. Neither this loop nor the search reads world positions.
        blocked_direction = None
        for _ in range(8):
            observed = self.observe()
            found = predicate(observed)
            if found:
                return found
            here = self.position(observed["sections"]["self"]["pos"])
            dx, dz = anchor[0] - here[0], anchor[1] - here[1]
            if max(abs(dx), abs(dz)) <= 3500:
                break
            direction = ("east" if dx > 0 else "west") if abs(dx) > abs(dz) else ("north" if dz > 0 else "south")
            if direction == blocked_direction:
                direction = ("north" if dz >= 0 else "south") if direction in ("east", "west") else ("east" if dx >= 0 else "west")
            arrived = self.exploration_step(direction, description, "return toward actor-observed arrival history", southern_hint)
            if arrived is None:
                break
            blocked_direction = None if arrived else direction
        for direction in route[:budget]:
            observed = self.observe()
            found = predicate(observed)
            if found:
                return found
            if self.exploration_step(direction, description, basis, southern_hint) is None:
                break
        found = predicate(self.observe())
        if found:
            return found
        movement = "widget/recorded actor-relative" if self.recorded_exploration else "widget-only"
        raise RuntimeError(f"Harness limitation: bounded {movement} exploration did not discover {description}; physical WASD/right-click is not exercised")

    def exploration_step(self, direction, description, basis, southern_hint=False):
        label = f"Walk 6 m {direction}"
        available = False
        self.open_tab("Explore · M")
        if self.has_widget(label):
            wait_for(lambda: self.widget(label), 6)
            available = True
        if not available and not self.recorded_exploration:
            self.record("harness_discovery_limited", wanted=description, relative_direction=direction,
                        reason="No directional widget exists in Explore; physical players can still use WASD/right-click")
            return None
        before = self.observe()
        here = self.position(before["sections"]["self"]["pos"])
        dx, dz = {"west": (-6000, 0), "east": (6000, 0), "south": (0, -6000), "north": (0, 6000)}[direction]
        target = [here[0] + dx, here[1] + dz]
        if available:
            self.click(label)
        else:
            # Match the native adapter: a separate recorded stop, followed by
            # one order/execute batch. Ordinary validation and nav remain active.
            stopped = self.call("agent/act", {"unit": self.actor, "controller": "native-player", "verb": "stop_work"})
            if stopped.get("accepted") is not True:
                raise RuntimeError(f"Recorded exploration stop refused: {stopped}")
            receipt = self.call("agent/act", {"unit": self.actor, "controller": "native-player",
                "steps": [{"verb": "order", "args": {"order": {"type": "Move", "pos": target}}},
                          {"verb": "execute_orders"}]})
            self.record("recorded_actor_relative_exploration", direction=direction,
                        observed_position=here, target=target, receipt=receipt)
            if receipt.get("accepted") is not True:
                self.record("exploration_refused", reason=receipt.get("message", receipt.get("reason")))
                return False
        arrived = self.wait_walk(before, target)
        self.record("relative_exploration", wanted=description, direction=direction, arrived=arrived,
                    input="visible widget" if available else "recorded actor-relative agent/act", basis=basis,
                    observed_start=here, observed_end=self.position(self.observe()["sections"]["self"]["pos"]))
        return arrived

    def wait_walk(self, before, target, budget=700):
        while True:
            observed = self.observe()
            position = self.position(observed["sections"]["self"]["pos"])
            outcomes = self.own_outcomes(observed, before.get("seq", 0))
            if sum((a-b)**2 for a, b in zip(position, target)) <= 1000**2 and observed["sections"]["village"]["activity"]["busy"] is False:
                return True
            refused = [event for event in outcomes if event["kind"] in ("PathFailed", "OrderFailed", "ActionRejected")
                       or (event["kind"] == "ExternalAction" and event.get("status") in ("failed", "rejected", "cancelled"))]
            if refused and observed["sections"]["village"]["activity"]["busy"] is False:
                self.record("exploration_terminal_refusal", outcomes=refused,
                            observation=self.public_receipt(observed), visible_feedback=self.feedback_strings())
                return False
            if observed["tick"] - before["tick"] >= budget:
                self.record("exploration_budget_exhausted", observation=self.public_receipt(observed),
                            target=target, visible_feedback=self.feedback_strings())
                self.click("Stop · X")
                self.until(lambda obs: obs["sections"]["village"]["activity"]["busy"] is False, budget=200)
                return False
            self.call("npc/step", {"ticks": 20})
            time.sleep(0.18)

    def gather_to(self, kind, wanted):
        while self.inventory(self.observe()).get(kind, 0) < wanted:
            self.exploration(lambda obs: next((source for source in obs["sections"]["village"]["environment"]["resources"]
                if source["kind"] == kind and source.get("remaining") != 0), None), friendly(kind), resource_kind=kind)
            self.open_tab("Craft · C")
            previous = self.inventory(self.observe()).get(kind, 0)
            self.click("Gather " + friendly(kind))
            self.until(lambda obs: self.inventory(obs).get(kind, 0) > previous)

    def care(self):
        """Use real supplies only between committed activities."""
        observed = self.observe()
        if observed["sections"]["village"]["activity"]["busy"]:
            return
        for need, kind in [("thirst", "village_water"), ("hunger", "village_berries")]:
            observed = self.observe()
            if observed["sections"]["village"]["needs"][need] >= 45000:
                continue
            self.gather_to(kind, 1)
            self.open_tab("Pouch · I")
            before = self.inventory(self.observe()).get(kind, 0)
            self.click(friendly(kind))
            self.until(lambda obs: self.inventory(obs).get(kind, 0) < before, budget=300)
            self.record("self_care", need=need, item=kind)
        if self.observe()["sections"]["village"]["needs"]["energy"] < 40000:
            self.open_tab("Pouch · I")
            self.click("Go home and rest")
            self.until(lambda obs: obs["sections"]["village"]["needs"]["energy"] >= 70000, budget=2400)
            self.click("Stop · X")
            self.until(lambda obs: obs["sections"]["village"]["activity"]["busy"] is False, budget=200)

    def known_people(self, observed=None):
        observed = observed or self.observe()
        here = self.position(observed["sections"]["self"]["pos"])
        relations = {person["id"]: person for person in observed["sections"]["village"]["social"]["relationships"]}
        seen = {}
        for group in ["sensed", "remembered"]:
            for person in observed["sections"]["perception"].get(group, []):
                if person.get("kind") != "actor" or person["id"] == self.actor or person["id"] not in relations or person.get("pos") is None:
                    continue
                name = relations.get(person["id"], {}).get("name", person.get("name", "")).rsplit(" #", 1)[0]
                if not name:
                    continue
                point = self.position(person["pos"])
                seen.setdefault(person["id"], {"id": person["id"], "name": name,
                    "currently_sensed": group == "sensed", "position": list(point), "observed_at": observed["tick"],
                    "distance": sum((a-b)**2 for a, b in zip(here, point))})
        return sorted(seen.values(), key=lambda person: (person["distance"], person["id"]))

    def conversation(self):
        attempts, tried = [], set()
        report = {"attempted": False, "completed": False, "attempts": attempts}
        self.report["conversation"] = report
        for number in range(1, 4):
            observed = self.observe()
            available = {offer.get("target") for offer in observed.get("affordances", [])
                         if offer.get("verb") == "talk" and offer.get("ok") is True and offer.get("target") is not None}
            people = [person for person in self.known_people(observed) if person["id"] not in tried]
            people.sort(key=lambda person: (person["id"] not in available, not person["currently_sensed"],
                                           person["distance"], person["id"]))
            if not people:
                break
            person = people[0]
            tried.add(person["id"])
            attempt = {"neighbour": person["name"], "id": person["id"],
                       "talk_available_when_selected": person["id"] in available,
                       "currently_sensed": person["currently_sensed"], "completed": False}
            attempts.append(attempt)
            report["attempted"] = True
            try:
                self.select_person(person)
                before = self.observe()
                previous_feedback = self.feedback_strings()
                self.click("Building a home")
                result = self.wait_conversation(person, before, previous_feedback)
                attempt.update(result)
            except RuntimeError as error:
                # A vanished/disabled visible neighbour is a social limitation.
                # Death, process exit and exhausted global budgets still abort.
                self.check()
                self.observe()
                attempt.update(reason=str(error), visible_feedback=self.feedback_strings())
            self.record("conversation_attempt", **attempt)
            if attempt["completed"]:
                report.update(completed=True, neighbour=person["name"], heard=attempt["heard"])
                self.capture("neighbour-help")
                return
            self.capture(f"conversation-attempt-{number}-unavailable")
        report["limitation"] = "No reachable conversation succeeded within three distinct observed-neighbour attempts; cottage/farm testing continues."
        self.record("social_limitation", reason=report["limitation"], attempts=len(attempts))

    def wait_conversation(self, person, before, previous_feedback, budget=600):
        end = time.monotonic() + 45
        while True:
            observed = self.observe()
            heard = [line for line in observed["sections"]["village"].get("heard_speech", [])
                     if line.get("speaker") == person["id"] and line.get("tick", 0) >= before["tick"]
                     and "southern house lane" in line.get("text", "")]
            if heard:
                return {"completed": True, "heard": heard, "after": self.public_receipt(observed)}
            feedback = self.feedback_strings()
            outcomes = self.own_outcomes(observed, before.get("seq", 0))
            refusal = [event for event in outcomes if event["kind"] in ("PathFailed", "OrderFailed")
                       or (event["kind"] == "ActionRejected" and event.get("verb") == "talk")
                       or (event["kind"] == "ExternalAction" and event.get("status") in ("failed", "rejected", "cancelled"))]
            moving = any(text.startswith(("Walking, then", "Your neighbour moved. Walking closer")) for text in feedback)
            terminal_text = [text for text in feedback if text.startswith((
                "Your neighbour is out of sight", "Your neighbour keeps moving", "Could not approach your neighbour",
                "At destination:", "Approach failed", "Approach rejected", "Approach cancelled", "Approach interrupted",
                "Approach made no progress", "Movement unavailable:")) and (text not in previous_feedback or refusal)]
            if terminal_text or (refusal and not moving and observed["sections"]["village"]["activity"]["busy"] is False):
                return {"completed": False, "reason": "Conversation ended with a visible or actor-owned terminal refusal",
                        "visible_feedback": feedback, "own_outcomes": refusal, "after": self.public_receipt(observed)}
            if observed["tick"] - before["tick"] >= budget or time.monotonic() >= end:
                result = {"completed": False, "reason": "Conversation reached its 600-tick/45-second attempt budget",
                          "visible_feedback": feedback, "own_outcomes": outcomes, "after": self.public_receipt(observed)}
                self.click("Stop · X")
                self.until(lambda obs: obs["sections"]["village"]["activity"]["busy"] is False, budget=200, chunk=1)
                return result
            self.call("npc/step", {"ticks": 1})
            time.sleep(0.18)

    def material_cottage(self):
        self.conversation()
        chosen = self.exploration(lambda obs: next((plot for plot in obs["sections"]["homestead"]["plots"]
            if plot["use_kind"] == "housing" and plot.get("state_known") is True
            and (plot.get("owner") is None or plot.get("own") is True)), None),
            "a vacant house site", southern_hint=True)
        self.select_site(chosen, "Build · B")
        if chosen.get("own") is not True:
            self.click("Claim this site")
            self.until(lambda obs: self.plot(chosen["id"], obs).get("own") is True)
        self.widget("Build this cottage", enabled=False)
        self.capture("claimed-missing-materials")
        observed = self.observe()
        cottage = observed["sections"]["homestead"]["cottage"]
        required = {item["kind"]: item["count"] for item in cottage["inputs"]}
        workshop = self.exploration(self.observed_plank_workshop, "a plank workshop with an observed position")
        self.report["remembered_workshop"] = workshop
        self.record("workshop_observed", **workshop)
        recipe = workshop["recipe"]
        if any(item["kind"] != "log" for item in recipe["inputs"]):
            raise RuntimeError("Observed recipe requires more than logs; no materials will be fabricated")
        output = next(item["count"] for item in recipe["outputs"] if item["kind"] == "plank")
        runs = math.ceil(required["plank"] / output)
        needed_logs = required["log"] + runs * sum(item["count"] for item in recipe["inputs"])
        self.gather_to("log", needed_logs)
        recipe_label = "Make " + " + ".join(f"{item['count']} {friendly(item['kind'])}" for item in recipe["outputs"])
        for _ in range(runs):
            self.craft_batch(workshop, recipe_label, output)
        self.capture("crafted-materials")
        self.select_site(chosen, "Build · B")
        before = self.inventory(self.observe())
        self.click("Build this cottage")
        built = self.until(lambda obs: self.plot(chosen["id"], obs).get("cottage_built") is True, budget=2400)
        after = self.inventory(built)
        for kind, count in required.items():
            if before.get(kind, 0) - after.get(kind, 0) != count:
                raise RuntimeError("The cottage did not spend its advertised real materials exactly once")
        self.report["material_cottage"] = {"plot": chosen["id"], "required": required,
            "gathered_logs": needed_logs, "craft_runs": runs, "grants": 0, "teleports": 0,
            "after": self.public_receipt(built)}
        self.capture("completed-cottage")
        return chosen

    def garden_loop(self, chosen, skip_trade=False):
        crops = self.observe()["sections"]["homestead"]["crops"]
        crop = next((crop for crop in crops if crop["kind"] == "village_berries"),
                    min(crops, key=lambda crop: crop["growth_ticks"]))
        self.gather_to(crop["seed_kind"], crop["seed_count"])
        self.gather_to(crop["water_kind"], crop["water_count"] + 2)
        self.gather_to("village_berries", 2)
        self.care()
        self.select_site(chosen, "Garden · G")
        bed = next(bed for bed in self.plot(chosen["id"])["beds"] if bed["stage"] == "empty")
        index = bed["index"]
        self.click(crop["label"])
        self.click(f"Bed {index + 1} · Empty soil")
        before = self.inventory(self.observe()).get(crop["seed_kind"], 0)
        self.click("Plant " + friendly(crop["kind"]) + " here")
        self.until(lambda obs: self.plot(chosen["id"], obs)["beds"][index]["stage"] == "needs_water")
        if before - self.inventory(self.observe()).get(crop["seed_kind"], 0) != crop["seed_count"]:
            raise RuntimeError("Planting did not consume the advertised real seed")
        before = self.inventory(self.observe()).get(crop["water_kind"], 0)
        self.click("Water this bed")
        self.until(lambda obs: self.plot(chosen["id"], obs)["beds"][index]["stage"] == "growing")
        if before - self.inventory(self.observe()).get(crop["water_kind"], 0) != crop["water_count"]:
            raise RuntimeError("Watering did not consume the advertised real water")
        self.capture("growing-garden")
        deadline = self.plot(chosen["id"])["beds"][index]["ready_at"]
        while self.observe()["tick"] <= deadline:
            self.care()
            self.call("npc/step", {"ticks": min(120, max(1, deadline - self.tick + 1))})
            time.sleep(0.18)
        self.select_site(chosen, "Garden · G")
        self.click(f"Bed {index + 1} · Ready to harvest")
        before = self.inventory(self.observe()).get(crop["yield_kind"], 0)
        self.click("Harvest this bed")
        harvested = self.until(lambda obs: self.plot(chosen["id"], obs)["beds"][index]["stage"] == "empty")
        if self.inventory(harvested).get(crop["yield_kind"], 0) - before != crop["yield_count"]:
            raise RuntimeError("Harvest did not produce exactly its advertised yield")
        self.report["garden_loop"] = {"crop": crop, "plot": chosen["id"], "bed": index,
            "completed": True, "after": self.public_receipt(harvested)}
        self.capture("harvested-garden")
        if skip_trade:
            self.report["quoted_trade"] = {"completed": False, "attempted": False, "skipped": True,
                "reason": "--skip-trade keeps the life-loop lifecycle separate; --trade-only must verify the market independently"}
        else:
            self.quoted_trade()

    def return_to_known_area(self, target, label, max_moves=10, stop_at_market=False):
        blocked = None
        for _ in range(max_moves):
            observed = self.observe()
            if stop_at_market and observed["sections"]["village"]["nearby_market"]:
                return True
            here = self.position(observed["sections"]["self"]["pos"])
            dx, dz = target[0] - here[0], target[1] - here[1]
            if max(abs(dx), abs(dz)) <= 3500:
                return True
            direction = ("east" if dx > 0 else "west") if abs(dx) > abs(dz) else ("north" if dz > 0 else "south")
            if direction == blocked:
                direction = ("north" if dz >= 0 else "south") if direction in ("east", "west") else ("east" if dx >= 0 else "west")
            arrived = self.exploration_step(direction, label, "return toward actor-observed location history")
            if arrived is None:
                return False
            blocked = None if arrived else direction
        return False

    def approach_merchant(self, person):
        self.select_person(person)
        before = self.observe()
        self.click("Say hello")
        end = time.monotonic() + 35
        while True:
            observed = self.observe()
            if observed["sections"]["village"]["activity"]["busy"] is False and any(
                merchant["id"] == person["id"] for merchant in observed["sections"]["village"]["nearby_market"]):
                return True
            feedback = self.feedback_strings()
            refusals = [event for event in self.own_outcomes(observed, before.get("seq", 0))
                        if event["kind"] in ("ActionRejected", "OrderFailed", "PathFailed")]
            pending = any(text.startswith(("Walking, then", "Your neighbour moved. Walking closer")) for text in feedback)
            if refusals and not pending and observed["sections"]["village"]["activity"]["busy"] is False:
                self.record("merchant_approach_refused", merchant=person["name"], outcomes=refusals, visible_feedback=feedback)
                return False
            if observed["tick"] - before["tick"] >= 700 or time.monotonic() >= end:
                self.record("merchant_approach_bounded", merchant=person["name"], visible_feedback=feedback)
                self.click("Stop · X")
                self.until(lambda obs: obs["sections"]["village"]["activity"]["busy"] is False, budget=200, chunk=1)
                return False
            self.call("npc/step", {"ticks": 1})
            time.sleep(0.18)

    def sale(self, merchant_id, kind):
        self.open_tab("Trade · T")
        label = "Sell " + friendly(kind)
        if not self.has_widget(label, enabled=True):
            self.record("trade_widget_unavailable", merchant=merchant_id, kind=kind,
                        reason="No unique enabled sale widget; no hidden action target chosen")
            return False
        self.reveal(label)
        self.capture("quoted-trade")
        # Re-quote after the screenshot, immediately before activating the real
        # enabled widget. A moving merchant/changed offer is a real refusal.
        for _ in range(4):
            before = self.observe()
            merchant = next((person for person in before["sections"]["village"]["nearby_market"] if person["id"] == merchant_id), None)
            offer = next((offer for offer in (merchant or {}).get("offers", [])
                          if offer["direction"] == "sell" and offer["kind"] == kind and offer.get("available") is True), None)
            if offer is None or not self.has_widget(label, enabled=True):
                self.record("trade_quote_expired", merchant=merchant_id, kind=kind)
                return False
            row = self.widget(label)
            texts, _, _ = self.ui()
            quotes = [int(match.group(1)) for text in texts if text["parent"] == row["parent"]
                      and (match := re.match(r"^(\d+) coins each", str(text["text"])))]
            if quotes == [offer["unit_price"]]:
                break
            time.sleep(0.22)
        else:
            self.record("trade_quote_unsettled", merchant=merchant_id, kind=kind)
            return False
        amount, coins = self.inventory(before).get(kind, 0), before["sections"]["village"]["coins"]
        self.record("widget", label=label, entity=row["parent"], displayed_quote=offer["unit_price"])
        self.call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": row["parent"]}})
        time.sleep(0.2)
        while True:
            after = self.observe()
            quantity = self.inventory(after).get(kind, 0)
            paid = after["sections"]["village"]["coins"] - coins
            if quantity == amount - 1 and paid > 0:
                if paid < offer["unit_price"]:
                    raise RuntimeError("Native sale paid less than the displayed quote limit")
                self.report["quoted_trade"].update(completed=True, merchant=merchant["name"], kind=kind,
                    minimum_quote=offer["unit_price"], actual_coins=paid,
                    stock_before=amount, stock_after=quantity, coins_before=coins,
                    coins_after=after["sections"]["village"]["coins"])
                self.capture("trade-complete")
                return True
            refusals = [event for event in self.own_outcomes(after, before.get("seq", 0)) if event["kind"] == "ActionRejected"]
            if refusals or after["tick"] - before["tick"] >= 300:
                self.record("sale_not_completed", merchant=merchant_id, kind=kind, outcomes=refusals,
                            visible_feedback=self.feedback_strings(), stock_before=amount, stock_after=quantity,
                            coins_before=coins, coins_after=after["sections"]["village"]["coins"])
                return False
            self.call("npc/step", {"ticks": 5})
            time.sleep(0.18)

    def own_sale_receipt(self, observed, kind, since):
        receipts = []
        for entry in observed.get("recent_events", []):
            effect = entry.get("event", {}).get("effect", {})
            if entry.get("unit") == self.actor and entry.get("seq", 0) > since \
                and effect.get("type") == "VillageSaleCompleted" and effect.get("kind") == kind \
                and effect.get("quantity") == 1 and effect.get("coins", 0) > 0:
                receipts.append({"seller": self.actor, "tick": entry["tick"], "seq": entry["seq"], **effect})
        return receipts[-1] if receipts else None

    def offered_sale(self, kind, budget=12000):
        # A self-sustaining village need not buy surplus immediately. Observe a
        # bounded consumption/resupply window; the overall tick/wall budgets and
        # exact inventory, payment and receipt assertions still apply.
        self.open_tab("Trade · T")
        label = "Offer " + friendly(kind) + " for sale"
        if not self.has_widget(label, enabled=True):
            self.record("posted_offer_widget_unavailable", kind=kind)
            return False
        before = self.observe()
        amount = self.inventory(before).get(kind, 0)
        coins = before["sections"]["village"]["coins"]
        if amount < 1:
            raise RuntimeError("An offered sale requires actual carried goods")
        self.click(label)
        posted = None
        observed_prices = []
        limit = min(budget, self.tick_budget - before["tick"] - 150)
        if limit <= 0:
            return False
        while True:
            observed = self.observe()
            offer = next((offer for offer in observed["sections"]["village"].get("economy", {}).get("own_offers", [])
                          if offer["kind"] == kind and offer.get("quantity") == 1), None)
            if offer is not None:
                price = offer.get("unit_price")
                if not isinstance(price, int) or price <= 0:
                    raise RuntimeError("Posted offer has no positive observed price")
                if posted is None:
                    posted = dict(offer)
                    self.record("own_offer_published_not_sold", offer=posted, stock=amount, coins=coins)
                if not observed_prices or observed_prices[-1]["unit_price"] != price:
                    observed_prices.append({"tick": observed["tick"], "unit_price": price})
            receipt = self.own_sale_receipt(observed, kind, before.get("seq", 0))
            quantity = self.inventory(observed).get(kind, 0)
            paid = observed["sections"]["village"]["coins"] - coins
            if quantity == amount - 1 and paid > 0:
                if posted is None:
                    raise RuntimeError("Goods sold before an own-offer price observation; insufficient evidence to certify this offered-sale check")
                if receipt is None:
                    raise RuntimeError("Inventory/coins changed without the actor-attributed completed-sale receipt")
                if receipt["coins"] != paid or receipt.get("unit_price") != paid:
                    raise RuntimeError("Completed sale receipt does not match exact one-good and coin deltas")
                self.report["quoted_trade"].update(completed=True, path="native public offer purchased by an autonomous resident",
                    kind=kind, posted_offer=posted, observed_offer_prices=observed_prices,
                    actual_unit_price=receipt["unit_price"], actual_coins=paid, sale_receipt=receipt,
                    stock_before=amount, stock_after=quantity, coins_before=coins,
                    coins_after=observed["sections"]["village"]["coins"])
                self.capture("posted-offer-sale-complete")
                return True
            if observed["tick"] - before["tick"] >= limit:
                self.record("posted_offer_unfilled", kind=kind, posted_offer=posted, observed_offer_prices=observed_prices,
                            stock=quantity, coins=observed["sections"]["village"]["coins"],
                            reason="Posting is not a completed sale; no attributed settlement within the bounded wait")
                return False
            self.call("npc/step", {"ticks": 5 if posted is None else 40})
            time.sleep(0.18)

    def quoted_trade(self):
        self.report["quoted_trade"] = {"completed": False, "attempts": [], "fund_grants": 0, "stock_grants": 0}
        reserve = {"village_berries": 1, "village_grain": 1, "village_water": 1}
        observed = self.observe()
        stock = self.inventory(observed)
        places = [place for place in getattr(self, "known_workplaces", {}).values()
                  if any(stock.get(kind, 0) > reserve.get(kind, 0) for kind in place["accepts"])]
        here = self.position(observed["sections"]["self"]["pos"])
        places.sort(key=lambda place: sum((a-b)**2 for a,b in zip(here, place["position"])))
        destination = places[0]["position"] if places else self.arrival_anchor
        self.return_to_known_area(destination, "a previously observed market area", stop_at_market=True)
        tried = set()
        supplied_bid = False
        for attempt in range(6):
            if self.tick_budget - self.observe()["tick"] < 600:
                break
            self.care()
            observed = self.observe()
            stock = self.inventory(observed)
            markets = observed["sections"]["village"]["nearby_market"]
            available = [(merchant, offer) for merchant in markets for offer in merchant["offers"]
                         if offer.get("available") is True and offer["direction"] == "sell"
                         and stock.get(offer["kind"], 0) > reserve.get(offer["kind"], 0)]
            if available:
                merchant, offer = available[0]
                self.report["quoted_trade"]["attempts"].append({"merchant":merchant["id"],"kind":offer["kind"],"quote":offer["unit_price"]})
                if self.sale(merchant["id"], offer["kind"]):
                    return
            # A public bid can justify one additional real gathering action.
            # Its demand/price are observed; no merchant stock or coins are edited.
            if not supplied_bid:
                bids = [(merchant, offer) for merchant in markets for offer in merchant["offers"]
                        if offer["direction"] == "sell" and (offer.get("demand") or 0) > 0
                        and offer["kind"] in ("log", "village_fiber", "ore", "village_grain", "village_berries")
                        and stock.get(offer["kind"], 0) <= reserve.get(offer["kind"], 0)
                        and "cannot afford" not in str(offer.get("reason", ""))]
                bids.sort(key=lambda pair: (pair[1]["kind"] != "log", pair[0]["id"], pair[1]["kind"]))
                if bids:
                    merchant, bid = bids[0]
                    known = next((person for person in self.known_people(observed) if person["id"] == merchant["id"]), None)
                    if known:
                        self.record("gather_for_observed_bid", merchant=merchant["id"], kind=bid["kind"], bid=bid)
                        supplied_bid = True
                        self.gather_to(bid["kind"], max(stock.get(bid["kind"], 0) + 1,
                                                       reserve.get(bid["kind"], 0) + 1))
                        self.return_to_known_area(known["position"], "the buyer's previously observed location", stop_at_market=True)
                        fresh = next((person for person in self.known_people() if person["id"] == merchant["id"]), None)
                        if fresh:
                            self.approach_merchant(fresh)
                        continue
            observed = self.observe()
            people = [person for person in self.known_people(observed) if person["id"] not in tried]
            if not people:
                break
            # Prefer already available sales/talk in the actor's current scope.
            ready = {offer.get("target") for offer in observed.get("affordances", [])
                     if offer.get("ok") is True and offer.get("verb") in ("sell", "talk")}
            people.sort(key=lambda person: (person["id"] not in ready, not person["currently_sensed"], person["distance"], person["id"]))
            person = people[0]
            tried.add(person["id"])
            self.approach_merchant(person)
        # If mobile merchants keep leaving reach, let the public market bring
        # an autonomous buyer to real carried surplus. Never count publishing as
        # settlement, and do not force an NPC to buy or change its funds/stock.
        stock = self.inventory(self.observe())
        surplus = [kind for kind, count in stock.items() if count > reserve.get(kind, 0)]
        for kind in surplus:
            self.open_tab("Trade · T")
            if self.has_widget("Offer " + friendly(kind) + " for sale", enabled=True):
                if self.offered_sale(kind):
                    return
                break  # Leave an unfilled listing as an explicit failed check.
        self.report["quoted_trade"]["reason"] = "No verified stock-and-coin sale within bounded merchant attempts, one real bid-supply gathering phase and an optional public-offer wait; publishing alone is not success."
        self.record("trade_gap", **self.report["quoted_trade"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/game_menu.exe")
    parser.add_argument("--build-cottage", action="store_true", help="also run actor-observed material acquisition and construction")
    parser.add_argument("--life-loop", action="store_true", help="also build, plant, water, harvest, and seek one observed surplus sale")
    parser.add_argument("--trade-only", action="store_true", help="fresh empty-handed market rehearsal; a verified actual stock/coin sale is mandatory")
    parser.add_argument("--skip-trade", action="store_true", help="finish --life-loop harvest/save/title/quit; test the market separately with --trade-only")
    parser.add_argument("--recorded-exploration", action="store_true", help="allow validated actor-relative Move orders only when no directional widget remains")
    parser.add_argument("--spectator", action="store_true", help="start by overseeing NPCs with an unarrived visitor, then join through Play character")
    parser.add_argument("--death-check", action="store_true", help="isolated final QA: recorded depleted needs, actual mortality, watch and successor; not natural survival evidence")
    parser.add_argument("--resume-save", type=Path, help="copy an existing snapshot into this disposable run and exercise ordinary Continue")
    parser.add_argument("--modes", action="store_true", help="also check colony/character/editor round trips and a recorded terrain brush")
    parser.add_argument("--backend", choices=("vulkan", "default"), default="vulkan",
                        help="default inherits the ordinary renderer selection without a harness override")
    args = parser.parse_args()
    if args.trade_only and (args.life_loop or args.build_cottage or args.skip_trade or args.resume_save or args.spectator or args.death_check or args.modes):
        parser.error("--trade-only requires a separate fresh run; combine only renderer/binary/exploration options")
    if args.skip_trade and not args.life_loop:
        parser.error("--skip-trade requires --life-loop")
    if args.resume_save and (args.life_loop or args.build_cottage or args.spectator or args.death_check or args.modes):
        parser.error("--resume-save exercises ordinary continuation only; use other QA flags in a separate disposable run")
    if args.resume_save and not args.resume_save.is_file():
        parser.error("--resume-save must name an existing snapshot; it is copied and never edited")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("--binary must name a built game_menu executable")
    run = ROOT / "target" / ("playtest-player-flow-" + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    settings = run / "settings.json"
    settings.write_text(json.dumps({"agents_dir": str(run / "agents"),
        "worlds_dir": str(run / "rules"), "fog_of_war": True}), encoding="utf-8")
    save = run / "agents" / "local-island.world.json"
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
        "input": "enabled native widgets; view-only resize; optional bounded clock steps",
        "physical_input_tested": False, "provider": False, "screenshots": {}, "checks": [],
        "material_cottage_requested": args.build_cottage or args.life_loop,
        "life_loop_requested": args.life_loop, "trade_only_requested": args.trade_only,
        "life_loop_trade_skipped": args.skip_trade, "recorded_exploration_allowed": args.recorded_exploration,
        "movement_input": "visible widgets, with opt-in recorded actor-relative exploration fallback" if args.recorded_exploration else "visible widgets only",
        "grants": 0, "teleports": 0}
    original_save = args.resume_save.resolve() if args.resume_save else None
    if original_save:
        save.parent.mkdir(parents=True)
        shutil.copyfile(original_save, save)
        report["compatibility"] = {"source": str(original_save), "source_sha256": file_hash(original_save),
            "copied_sha256": file_hash(save), "copied_to": str(save), "source_written": False}
    report["spectator_requested"] = args.spectator
    report["death_check_requested"] = args.death_check
    child = flow = None
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    environment = os.environ.copy()
    if args.backend != "default":
        environment["WGPU_BACKEND"] = args.backend
    report["requested_backend"] = args.backend
    report["renderer_override"] = environment.get("WGPU_BACKEND")
    try:
        with (run / "native.log").open("wb") as log:
            child = subprocess.Popen([str(binary), "--front-end", "--settings", str(settings),
                "--remote", "--remote-port", str(port), "--no-render-remote", "--out-dir", str(run)],
                cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            flow = PlayerFlow(run, child, url, report, time.monotonic())
            flow.recorded_exploration = args.recorded_exploration
            flow.tick_budget = 24000 if args.life_loop else MAX_TICKS
            report["maximum_simulation_tick"] = flow.tick_budget
            wait_for(lambda: flow.widget("New game"), 120)
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.assert_window()
            flow.widget("Continue", enabled=original_save is not None)
            initial_tick = flow.state()["tick"]
            if original_save:
                flow.tick_budget = initial_tick + MAX_TICKS
                report["maximum_simulation_tick"] = flow.tick_budget
                report["compatibility"]["additional_tick_budget"] = MAX_TICKS
            copied_hash = file_hash(save) if original_save else None
            time.sleep(0.4)
            if flow.state()["tick"] != initial_tick or (save.exists() if original_save is None else file_hash(save) != copied_hash):
                raise RuntimeError("Opening the title advanced or rewrote the island")
            report["checks"].append("title is paused with one window and preserves its unstarted or copied save state")
            flow.capture("title")
            reserved_visitor = None
            if original_save:
                if file_hash(save) != copied_hash:
                    raise RuntimeError("Title capture rewrote the copied saved island")
                flow.click("Continue")
                report["compatibility"]["continued_saved_tick"] = initial_tick
            else:
                flow.click("New game")
                flow.capture("new-character")
                if args.spectator:
                    reserved_visitor = flow.spectator_entry()
                else:
                    flow.click("Start on the island")
            wait_for(lambda: flow.widget("Menu · Esc"))
            def arrived_visitors():
                visitors = [row for row in flow.state()["entities"]
                            if row.get("components", {}).get("island_villager", {}).get("role") in ("visitor", "Visitor")
                            and row.get("components", {}).get("island_vitality", {}).get("died_at") is None]
                return visitors if len(visitors) == 1 else None
            visitors = wait_for(arrived_visitors)
            if len(visitors) != 1:
                raise RuntimeError("New game did not create exactly one visiting character")
            # Global operator projection is used only to identify the owned actor.
            # All subsequent gameplay choices use agent/observe, never world/state.
            flow.actor = visitors[0]["sim_id"]
            report["actor"] = flow.actor
            if reserved_visitor is not None:
                if flow.actor != reserved_visitor:
                    raise RuntimeError("Joining replaced the reserved visitor instead of arriving once")
                arrivals = flow.call("npc/journal", {"kinds": ["Effect"], "grep": "VillageArrival", "limit": 128})
                count = sum(1 for event in arrivals if event.get("kind", {}).get("effect", {}).get("type") == "VillageArrival")
                if count != 1:
                    raise RuntimeError("Play character did not record exactly one visitor arrival")
                flow.report["spectator_entry"].update(joined_actor=flow.actor, recorded_arrivals=count)
            report["arrival"] = flow.public_receipt(flow.observe())
            flow.arrival_anchor = list(report["arrival"]["position"])
            report["known_arrival_anchor"] = {"position": flow.arrival_anchor, "source": "actor-owned arrival observation"}
            flow.capture("arrival")
            for label, shot in ([] if args.modes or args.trade_only else [("Pouch · I", "pouch"), ("Craft · C", "craft"),
                                ("Build · B", "build"), ("Garden · G", "garden"),
                                ("Trade · T", "trade"), ("People · N", "people"), ("Explore · M", "explore")]):
                flow.open_tab(label)
                flow.capture(shot)
            if args.build_cottage or args.life_loop:
                chosen = flow.material_cottage()
                if args.life_loop:
                    flow.garden_loop(chosen, skip_trade=args.skip_trade)
            if args.trade_only:
                initial = flow.observe()
                stock = flow.inventory(initial)
                coins = initial["sections"]["village"]["coins"]
                if stock or coins != 0:
                    raise RuntimeError("Trade-only requires the ordinary fresh empty pouch and zero coins; no initial goods or money are allowed")
                report["trade_only_initial"] = {"pack": stock, "coins": coins, "tick": initial["tick"]}
                flow.quoted_trade()
                if report.get("quoted_trade", {}).get("completed") is not True:
                    raise RuntimeError("Trade-only verification failed: no actual validated stock debit and coin credit were completed")
                report["checks"].append("Fresh empty-handed actor gathered real goods for an observed bid and completed a sale with verified stock and coin settlement")
            flow.click("Menu · Esc")
            wait_for(lambda: flow.widget("Resume"))
            paused_tick = flow.observe()["tick"]
            flow.capture("pause")
            if flow.observe()["tick"] != paused_tick:
                raise RuntimeError("Pause did not hold the simulation during responsive captures")
            flow.click("Save game")
            if json.loads(save.read_text(encoding="utf-8"))["tick"] != paused_tick:
                raise RuntimeError("Pause save did not persist the exact paused world")
            flow.click("Save and title")
            wait_for(lambda: flow.widget("Continue"))
            flow.capture("saved-title")
            saved_tick = json.loads(save.read_text(encoding="utf-8"))["tick"]
            if flow.state()["tick"] != saved_tick:
                raise RuntimeError("Saved title advanced its world")
            flow.click("Continue")
            wait_for(lambda: flow.widget("Menu · Esc"))
            resumed = flow.observe()
            if resumed["unit"] != flow.actor or resumed["tick"] < saved_tick:
                raise RuntimeError("Continue replaced or rewound the saved character")
            flow.capture("continued")
            report["continued"] = flow.public_receipt(flow.observe())
            report["checks"].append("New, Pause, Save, Title and Continue retained one window and one life")
            if not args.modes and not args.trade_only:
                report["checks"].append("All seven player workspaces opened and captured at both sizes")
            if args.modes:
                flow.click("Menu · Esc")
                flow.click("Inspect village")
                flow.capture("inspector")
                flow.click("3x")
                flow.capture("inspector-3x")
                flow.click("Play character")
                wait_for(lambda: flow.widget("Menu · Esc"))
                flow.capture("character-after-fast-inspection")
                flow.click("Menu · Esc")
                flow.click("Landscape tools")
                flow.capture("landscape-tools")
                flow.click("1x")
                # Explicit inspector/editor input: this is a recorded brush,
                # not physical mouse-painting evidence or actor navigation.
                flow.call("npc/command", {"cmd": {"type": "Terraform", "x": 12, "z": 16,
                    "radius": 1, "height_mm": 1000, "surface": "stone"}})
                flow.call("npc/step", {"ticks": 2})
                flow.capture("edited-landscape")
                flow.click("Menu")
                flow.click("Save game")
                terrain = json.loads(save.read_text(encoding="utf-8"))["resources"]["island_colony_terrain_v1"]
                if terrain["revision"] != 1:
                    raise RuntimeError("The recorded editor brush was not applied and saved exactly once")
                report["mode_roundtrip"] = {"same_window": True, "terrain_revision": terrain["revision"],
                    "terrain_input": "recorded Terraform command; physical brush input not tested"}
                flow.click("Resume")
                flow.click("Play character")
                wait_for(lambda: flow.widget("Menu · Esc"))
            if args.death_check:
                flow.death_check()
            flow.click("Menu · Esc")
            flow.click("Quit game")
            child.wait(timeout=30)
            if child.returncode != 0:
                raise RuntimeError(f"Game exited with {child.returncode}")
            report["saved_tick"] = json.loads(save.read_text(encoding="utf-8"))["tick"]
            report["ok"] = True
    except Exception as error:
        report["error"] = str(error)
        if flow is not None:
            try:
                flow.failure_context("unhandled_harness_failure", error=str(error))
            except Exception as diagnostic_error:
                report["diagnostic_error"] = str(diagnostic_error)
        raise
    finally:
        if flow is not None:
            report["trace"] = flow.trace
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
        report["owned_native_stopped"] = child is None or child.poll() is not None
        if original_save:
            report["compatibility"]["source_unchanged"] = file_hash(original_save) == report["compatibility"]["source_sha256"]
            if not report["compatibility"]["source_unchanged"]:
                report["ok"] = False
                report["error"] = "Original compatibility source changed during the run"
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(run, flush=True)


if __name__ == "__main__":
    main()
