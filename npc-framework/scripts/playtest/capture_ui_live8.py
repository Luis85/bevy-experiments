"""Native captures of the eighth UI pass: the hired builder's states in
order (hired, walking, working; "waiting" between stages), the builder at
work beside the owner at the door, and the merchant boat's next order in
Oversee (the calendar) and in Live (the Trade tab); plus every check of the
seventh pass. Each at 1280x720 and 1024x768. Also a village's Jobs table
(wanted headcount and its reason), its priorities, and Shape placing,
undoing, redoing and removing a garden plot and a barrel.

Make the saves and build first, then run from npc-framework:
  cargo test --locked -p island_content --test island_capture_fixtures -- --ignored --test-threads 7
  cargo build --locked -p island_game
  python scripts/playtest/capture_ui_live8.py
      [--only settings,worlds,summary,upgrade,builder,builder-door,alerts,boat,economy,strength,
              shift-tab,older-save,jobs,priorities,plots,props]

PNGs and summary.json go to target/npc-out/ui-live8/.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import time
import uuid

os.environ["ISLAND_TIPS_SEEN"] = "all"

import capture_player_flow  # noqa: E402
from capture_native import ROOT  # noqa: E402
from capture_native_homestead import file_hash  # noqa: E402
from capture_player_flow import LEDGER, wait_for  # noqa: E402
from capture_ui_live3 import FIXTURES  # noqa: E402
from capture_ui_live6 import settings, start, summary, upgrade, worlds  # noqa: E402
from capture_ui_live7 import Live7Flow, alerts  # noqa: E402
from capture_ui_phase45 import WORKSPACE  # noqa: E402
from capture_ui_redesign import SIZES, stop  # noqa: E402

capture_player_flow.SIZES = SIZES
CALENDAR = "Island calendar"
# The card's words for each of the builder's states.
STATES = (("working", "is building at your door"),
          ("waiting", "is between stages of work"),
          ("walking", "is on the way to work"),
          ("hired", "is hired and will come"))
WALKING_LIMIT = 1_500


class Live8Flow(Live7Flow):
    def snap(self, name, size):
        for old in ("live7-", "live6-", "live5-"):
            if name.startswith(old):
                name = "live8-" + name[len(old):]
                break
        return super(Live7Flow, self).snap(name, size)


def builder_state(texts):
    """The builder's state as the card says it (the hired builder's line),
    or "none" when the card names no builder."""
    lines = [line for text in texts for line in text.splitlines()]
    for state, words in STATES:
        if any(words in line for line in lines):
            return state
    return "none"


def watch_builder(flow, report, key):
    """Fast until the materials are in, then 20-tick steps until the card
    says the builder is at work (or the work is done). Returns the state
    changes with their ticks and the line seen at work."""
    flow.call("npc/pause")
    flow.workspace("KeyB")
    card = lambda: flow.panel_texts(WORKSPACE)  # noqa: E731
    flow.call("npc/resume")
    for _ in range(3):
        flow.key("Equal")
        time.sleep(0.2)
    try:
        wait_for(lambda: any("The materials are in" in text for text in card()) or None, 300)
    except RuntimeError:
        pass
    flow.call("npc/pause")
    states, found = [], None
    for _ in range(2 * 24_000 // 20):
        flow.call("npc/step", {"ticks": 20})
        time.sleep(0.35)
        texts = card()
        state = builder_state(texts)
        tick = flow.clock_tick()
        if not states or states[-1]["state"] != state:
            states.append({"tick": tick, "state": state})
        if state == "working":
            found = {"tick": tick, "lines": [line for text in texts for line in text.splitlines()
                                             if "builder" in line or "is " in line][:5]}
            break
        if any("Built: a larger cottage" in text for text in texts):
            break
    report[key] = {"states": states, "at_work": found}
    return states, found


def walking_ticks(states):
    total = 0
    for here, after in zip(states, states[1:]):
        if here["state"] == "walking":
            total += after["tick"] - here["tick"]
    return total


def builder(flow, report, checks):
    states, found = watch_builder(flow, report, "builder")
    order = [entry["state"] for entry in states]
    seen_walking = "walking" in order
    back_to_hired = seen_walking and "hired" in order[order.index("walking"):]
    walked = walking_ticks(states)
    checks.append({"check": "the card names the hired builder at work",
                   "ok": found is not None, "seen": found, "states": states})
    checks.append({"check": "the builder's states run hired, walking, working without going back to hired",
                   "ok": found is not None and not back_to_hired, "states": states})
    checks.append({"check": f"the builder is shown walking for at most {WALKING_LIMIT} ticks",
                   "ok": found is not None and walked <= WALKING_LIMIT, "walking_ticks": walked})
    if found:
        flow.shots("live8-builder-at-work", settle=0.3)


def builder_door(flow, report, checks):
    states, found = watch_builder(flow, report, "builder_at_door")
    checks.append({"check": "with the owner at the door the card shows the builder working, not walking",
                   "ok": found is not None, "seen": found, "states": states})
    if found:
        flow.shots("live8-builder-with-owner", settle=0.3)


def boat_lines(texts):
    return [line for text in texts for line in text.splitlines()
            if "wants" in line and ("Next call" in line or "In port" in line)]


def boat(flow, report, checks):
    """The boat's next order: in the calendar (Oversee) and in the Trade tab
    (Live). Time runs until the boat has announced one."""
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.0)
    flow.click("Calendar")
    time.sleep(0.8)
    lines = boat_lines(flow.panel_texts(CALENDAR))
    report["boat_waited_seconds"] = 0
    # The boat announces its next order as it leaves: time runs at 10x, in
    # bounded spells, until the calendar has one.
    for spell in range(36):
        if lines:
            break
        flow.click("Calendar")
        flow.call("npc/resume")
        if spell == 0:
            for _ in range(3):
                flow.key("Equal")
                time.sleep(0.2)
        time.sleep(10)
        flow.call("npc/pause")
        report["boat_waited_seconds"] += 10
        flow.click("Calendar")
        time.sleep(0.8)
        lines = boat_lines(flow.panel_texts(CALENDAR))
    report["boat_calendar"] = flow.panel_texts(CALENDAR)
    checks.append({"check": "Oversee: the calendar says what the boat wants at its next call",
                   "ok": bool(lines), "lines": lines})
    flow.shots("live8-boat-calendar", settle=0.3)
    flow.key("Escape")
    time.sleep(0.4)
    flow.click("Live")
    wait_for(lambda: flow.has_widget("Menu") or None, 30)
    flow.call("npc/pause")
    trade = flow.workspace("KeyT")
    live = boat_lines(trade)
    report["boat_trade"] = trade[:20]
    checks.append({"check": "Live: the Trade tab says what the boat wants",
                   "ok": bool(live) and "The merchant boat" in trade, "lines": live})
    flow.shots("live8-boat-trade", settle=0.3)


def economy_day(flow, report, checks):
    """Oversee, Economy page: the day's account (fund, boat) above the table."""
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.0)
    flow.open_page("Economy")
    time.sleep(0.8)
    texts = flow.panel_texts(LEDGER) or flow.texts()
    lines = [text for text in texts if text.startswith("Civic fund:") or "merchant boat" in text
             or text.startswith(("This call so far", "Its last call", "Next call wants", "In port"))]
    report["economy_day"] = lines
    checks.append({"check": "the Economy page opens with the day's account: the fund and the boat",
                   "ok": "Today on the island" in texts and any(text.startswith("Civic fund:") for text in lines),
                   "lines": lines})
    flow.shots("live8-economy-day", settle=0.3)
    flow.close_ledger()


def shape_strength(flow, report, checks):
    """Shape's brush bar has a Strength switch; choosing one lights it."""
    flow.click("Shape")
    time.sleep(1.2)
    texts = flow.texts()
    names = [name for name in ("Gentle", "Normal", "Strong") if name in texts]
    if "Strong" in names:
        flow.click("Strong")
        time.sleep(0.4)
    report["brush_strengths"] = names
    checks.append({"check": "Shape's brush bar offers Gentle, Normal and Strong",
                   "ok": names == ["Gentle", "Normal", "Strong"], "names": names})
    flow.shots("live8-shape-strength", settle=0.3)
    flow.click("Live")
    wait_for(lambda: flow.has_widget("Menu") or None, 30)


def live_shift_tab(flow, report, checks):
    """Live: Shift+Tab from the world lands on the top bar, not the hotbar."""
    flow.call("npc/pause")
    flow.key("Tab", hold=("ShiftLeft",))
    time.sleep(0.4)
    focused = flow.call("world.get_resources", {"resource": "bevy_input_focus::InputFocus"})
    report["shift_tab_focus"] = focused
    names = flow.names()
    entity = None
    try:
        value = focused.get("value", focused)
        entity = value[0] if isinstance(value, list) else value
    except (AttributeError, TypeError):
        entity = None
    owner = [name for name, entities in names.items() if entity in entities]
    report["shift_tab_owner"] = owner
    in_bar = any(word in " ".join(owner) for word in ("Menu", "Live", "Oversee", "Shape", "1x", "3x", "10x",
                                                      "Pause", "Calendar", "Weather"))
    checks.append({"check": "Live: Shift+Tab from the world enters the top bar",
                   "ok": in_bar, "focused": owner})
    flow.key("Escape")


def older_save(flow, report, checks):
    """A save made by an older version: the title says so, offers New game,
    and the file is left as it was."""
    time.sleep(1.0)
    texts = flow.texts()
    message = [text for text in texts if "older version" in text]
    report["older_save_title"] = texts[:12]
    checks.append({"check": "an older save is named on the title and New game is offered, not Continue",
                   "ok": bool(message) and "New game" in texts and "Continue" not in texts,
                   "message": message})
    flow.shots("live8-older-save-title", settle=0.3)
    saved = flow.run / "living-world.world.json"
    report["older_save_kept"] = saved.is_file() and saved.read_bytes() == OLDER.read_bytes()
    checks.append({"check": "the older save is kept untouched",
                   "ok": report["older_save_kept"]})


PLACING = "Shape plots and props"
PLACING_TAG = "Placing cursor tag"


def activate_first(flow, label):
    """Press the first visible control called `label` (the Jobs table has a
    + per trade, the priorities a High per topic). Returns its entity."""
    matches, disabled = flow.matches(label)
    enabled = [row for row in matches if row["parent"] not in disabled]
    if not enabled:
        raise RuntimeError(f"no enabled {label!r}")
    entity = enabled[0]["parent"]
    flow.call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": entity}})
    time.sleep(0.4)
    return entity


def first_village(flow):
    """The ledger's first village: the choice after "All" in the scope row."""
    texts = flow.panel_texts(LEDGER)
    if "All" not in texts:
        return None
    after = texts[texts.index("All") + 1:]
    return after[0] if after else None


def oversee_economy(flow, page):
    flow.call("npc/pause")
    flow.click("Oversee")
    time.sleep(1.0)
    flow.open_page("Economy")
    time.sleep(0.8)
    village = first_village(flow)
    if village:
        flow.click(village)
        time.sleep(0.6)
    flow.click(page)
    time.sleep(0.8)
    return village


def apply_ticks(flow, ticks=4):
    flow.call("npc/step", {"ticks": ticks})
    time.sleep(0.8)


def oversee_jobs(flow, report, checks):
    """Oversee, Economy, Jobs for one village: Now, Wanted and Open per trade,
    + and - and "Village decides" under each; + sends the wanted headcount,
    the column shows it and the simulation's reason appears under the row."""
    village = oversee_economy(flow, "Jobs")
    texts = flow.panel_texts(LEDGER)
    report["jobs_before"] = texts[:60]
    checks.append({"check": "a village's Jobs table has Now, Wanted and Open and a control row per trade",
                   "ok": bool(village) and all(word in texts for word in ("Now", "Wanted", "Open"))
                   and "Village decides" in texts and "+" in texts,
                   "village": village})
    flow.shots("live8-oversee-jobs", settle=0.3)
    try:
        activate_first(flow, "+")
        apply_ticks(flow)
    except RuntimeError as error:
        report["jobs_press"] = str(error)
    texts = flow.panel_texts(LEDGER)
    report["jobs_after"] = texts[:80]
    message = [text for text in texts if "wanted" in text and ("Residents change trade" in text)]
    reasons = [text for text in texts if " now, " in text and " wanted." in text]
    checks.append({"check": "+ sends the wanted headcount and the ledger says it applied",
                   "ok": bool(message), "message": message})
    checks.append({"check": "now and wanted differ: the simulation's reason is under the row",
                   "ok": bool(reasons), "reasons": reasons[:3]})
    flow.shots("live8-oversee-jobs-wanted", settle=0.3)
    try:
        activate_first(flow, "Village decides")
        apply_ticks(flow)
    except RuntimeError as error:
        report["jobs_clear"] = str(error)
    texts = flow.panel_texts(LEDGER)
    checks.append({"check": "Village decides hands the trade back",
                   "ok": any("the village decides again" in text for text in texts)})
    flow.close_ledger()


def oversee_priorities(flow, report, checks):
    """Oversee, Economy, Priorities for one village: Low, Normal and High per
    topic with the simulation's sentence; High on Housing changes it."""
    village = oversee_economy(flow, "Priorities")
    texts = flow.panel_texts(LEDGER)
    report["priorities_before"] = texts[:60]
    topics = [topic for topic in ("Housing", "Storage", "Workplaces", "Food reserve", "Fuel reserve",
                                  "Coats", "Boat orders") if topic in texts]
    checks.append({"check": "a village's priorities list the seven topics with Low, Normal and High",
                   "ok": bool(village) and len(topics) == 7 and all(word in texts for word in ("Low", "Normal", "High")),
                   "topics": topics})
    before = texts[texts.index("Housing") + 4] if "Housing" in texts and len(texts) > texts.index("Housing") + 4 else None
    flow.shots("live8-oversee-priorities", settle=0.3)
    try:
        activate_first(flow, "High")
        apply_ticks(flow)
    except RuntimeError as error:
        report["priorities_press"] = str(error)
    texts = flow.panel_texts(LEDGER)
    after = texts[texts.index("Housing") + 4] if "Housing" in texts and len(texts) > texts.index("Housing") + 4 else None
    report["priorities_effect"] = {"before": before, "after": after}
    checks.append({"check": "High on Housing changes the line that says what it does",
                   "ok": before is not None and after is not None and before != after,
                   "before": before, "after": after})
    flow.shots("live8-oversee-priorities-high", settle=0.3)
    flow.close_ledger()


def placing_tag(flow):
    texts = flow.panel_texts(PLACING_TAG)
    return texts[0] if texts else None, texts[1] if len(texts) > 1 else None


def aim_free(flow, size=SIZES[0]):
    """Move the pointer over the land until the simulation accepts the item
    in hand there (no reason under the tag). Returns the accepted position."""
    for fx in (0.5, 0.42, 0.58, 0.35, 0.65):
        for fy in (0.55, 0.45, 0.62, 0.38):
            position = (size[0] * fx, size[1] * fy)
            flow.cursor(*position)
            time.sleep(0.7)
            line, reason = placing_tag(flow)
            if line and not reason:
                return position
    return None


def shape_placing(flow, report, checks, item, kind):
    """Shape, Place (L): the catalog's plots and props; the ghost and the
    tag; a click places the item, Ctrl+Z takes it back, Ctrl+Y lays it again;
    Remove placed takes it away."""
    flow.call("npc/pause")
    flow.click("Shape")
    time.sleep(1.2)
    flow.key("KeyL")
    time.sleep(0.6)
    texts = flow.panel_texts(PLACING)
    report[f"placing_{kind}_panel"] = texts
    checks.append({"check": "Shape's Place panel lists plots and props from the catalog",
                   "ok": "Plots" in texts and "Props" in texts and "Garden" in texts and "Barrel" in texts,
                   "texts": texts[:24]})
    flow.click(item)
    time.sleep(0.5)
    # A refused spot first: the reason in the tag, red ghost.
    flow.cursor(SIZES[0][0] * 0.02, SIZES[0][1] * 0.5)
    time.sleep(0.4)
    position = aim_free(flow)
    line, reason = placing_tag(flow)
    report[f"placing_{kind}_tag"] = {"line": line, "reason": reason, "position": position}
    checks.append({"check": f"the tag says what a click places ({item.lower()}) where the simulation accepts it",
                   "ok": position is not None and bool(line) and item.lower() in line.lower()})
    flow.shoot(f"live8-shape-{kind}-ghost", pointer=True)
    if position is None:
        return
    flow.cursor(*position)
    time.sleep(0.6)
    flow.press("Left")
    apply_ticks(flow)
    status = " ".join(flow.texts())
    checks.append({"check": f"a click places the {item.lower()}",
                   "ok": f"Placed a {item.lower()}." in status})
    flow.shots(f"live8-shape-{kind}-placed", settle=0.3)
    flow.key("KeyZ", hold=("ControlLeft",))
    apply_ticks(flow)
    status = " ".join(flow.texts())
    checks.append({"check": f"Ctrl+Z takes the {item.lower()} back",
                   "ok": f"Removed the {item.lower()}." in status})
    flow.key("KeyY", hold=("ControlLeft",))
    apply_ticks(flow)
    status = " ".join(flow.texts())
    checks.append({"check": f"Ctrl+Y lays the {item.lower()} again",
                   "ok": f"Placed the {item.lower()} again." in status})
    flow.click("Remove placed")
    time.sleep(0.4)
    flow.cursor(*position)
    time.sleep(0.7)
    line, reason = placing_tag(flow)
    report[f"placing_{kind}_remove_tag"] = {"line": line, "reason": reason}
    flow.shots(f"live8-shape-{kind}-remove", settle=0.3)
    flow.press("Left")
    apply_ticks(flow)
    status = " ".join(flow.texts())
    checks.append({"check": f"Remove placed takes the {item.lower()} away",
                   "ok": f"Removed the {item.lower()}." in status, "tag": line})
    flow.key("Escape")
    flow.click("Live")
    wait_for(lambda: flow.has_widget("Menu") or None, 30)


def shape_plots(flow, report, checks):
    shape_placing(flow, report, checks, "Garden", "plot")


def shape_props(flow, report, checks):
    shape_placing(flow, report, checks, "Barrel", "prop")


OLDER = FIXTURES / "older-save.json"


def make_older_save():
    """A copy of a fixture as an older version of the island would have left
    it, as the unit test builds one: without the island's save stamp and
    made by other content (another content hash)."""
    data = json.loads((FIXTURES / "cottage-evening.json").read_text(encoding="utf-8"))
    header = data.setdefault("header", {})
    # Stamped with an earlier island version (a save with no stamp is
    # unreadable, not older), as the unit test builds one.
    header["island_save_version"] = int(header.get("island_save_version", 3)) - 1
    OLDER.write_text(json.dumps(data), encoding="utf-8")


PARTS = {"settings": ("cottage-evening", [settings]),
         "worlds": ("cottage-evening", [worlds]),
         "summary": ("cottage-evening", [summary]),
         "upgrade": ("upgrade-underway", [upgrade]),
         "builder": ("upgrade-underway", [builder]),
         "builder-door": ("upgrade-underway-at-door", [builder_door]),
         "alerts": ("village-alerts", [alerts]),
         "boat": ("larger-cottage-done", [boat]),
         "economy": ("larger-cottage-done", [economy_day]),
         "strength": ("cottage-evening", [shape_strength]),
         "shift-tab": ("cottage-evening", [live_shift_tab]),
         "older-save": ("older-save", [older_save]),
         "jobs": ("larger-cottage-done", [oversee_jobs]),
         "priorities": ("larger-cottage-done", [oversee_priorities]),
         "plots": ("cottage-evening", [shape_plots]),
         "props": ("cottage-evening", [shape_props])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/island_game.exe")
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live8")
    parser.add_argument("--only", default=",".join(PARTS))
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build island_game before running the capture")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live8-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "input": "native BRP widget activation by text or control name; keys through "
                       "KeyboardInput messages; pointer through the window cursor and "
                       "MouseButtonInput; the simulation clock run, paused, sped up or stepped "
                       "20 ticks at a time; prepared saves copied in; ISLAND_TIPS_SEEN=all",
              "fixtures": {}, "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    if "older-save" in wanted:
        make_older_save()
    try:
        for name in wanted:
            fixture, steps = PARTS[name]
            report["fixtures"][name] = str(FIXTURES / f"{fixture}.json")
            try:
                run, child, flow = start(binary, base, name, report, fixture)
            except RuntimeError as error:
                report["checks"].append({"check": f"section {name} started", "ok": False,
                                         "reason": str(error)})
                continue
            flow.__class__ = Live8Flow
            try:
                if name not in ("settings", "older-save"):
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
