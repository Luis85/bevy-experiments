"""Native captures of the review fix pass (closing phase): every screen the
pass built, each at 1280x720 and 1024x768. Runs a frozen build, so no
island crate is built while the simulation is being edited.

Run from npc-framework:
  python scripts/playtest/capture_ui_live9.py
      [--binary target/npc-out/closing-ui/frozen/island_game.exe]
      [--only probe,title-cards,new,oversee-start,outlook,goods,orders,alerts,season,shape,why,pause,states]
      [--fixtures DIR] [--made DIR] [--out DIR]

The binary may also come from ISLAND_GAME_EXE. PNGs, texts and summary.json
go to target/npc-out/ui-live9/<section>/.
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
from capture_native import ROOT, query  # noqa: E402
from capture_native_homestead import file_hash  # noqa: E402
from capture_player_flow import LEDGER, PRIMARY, wait_for  # noqa: E402
from capture_ui_live3 import FIXTURES  # noqa: E402
from capture_ui_live6 import Live6Flow  # noqa: E402
from capture_ui_live8 import Live8Flow, activate_first  # noqa: E402
from capture_ui_redesign import SIZES, launch, stop  # noqa: E402
from capture_view_checks import DAY  # noqa: E402

capture_player_flow.SIZES = SIZES
FROZEN = ROOT / "target/npc-out/closing-ui/frozen/island_game.exe"
SAVE = "living-world.world.json"
AUTOSAVE = "living-world.world.autosave.json"
MADE = ROOT / "target/npc-out/closing-ui/fixtures9"
AUTOSAVE_FOR = {"title-autosave": MADE / "base8.json"}
# The probe's fixtures: `--fixtures` points it at a frozen set.
PROBE_FIXTURES = FIXTURES


class Live9Flow(Live8Flow):
    def snap(self, name, size):
        for old in ("live8-", "live7-", "live6-", "live5-"):
            if name.startswith(old):
                name = "live9-" + name[len(old):]
                break
        return super(Live8Flow, self).snap(name, size)

    def dump(self, label):
        """Every text on screen, kept beside the shots for judging words."""
        texts = self.texts()
        self.report.setdefault("texts", {})[label] = texts
        return texts

    def size_shots(self, label, sizes=SIZES, prepare=None, settle=0.8):
        """Capture at `sizes`; `prepare(size)` after each resize."""
        self.assert_window()
        for size in sizes:
            self.settle(size)
            time.sleep(settle)
            if prepare is not None:
                prepare(size)
            else:
                self.park(size)
            self.snap(f"live9-{label}-{size[0]}x{size[1]}", size)
        self.settle(SIZES[0])

    def hover(self, entity, size=SIZES[0]):
        """Rest the pointer on an entity's centre until its tooltip shows."""
        x, y = self.centre(entity)
        self.cursor(x, y)
        time.sleep(1.6)


def start(binary, base, name, report, save=None, attempts=3, extra=()):
    """Launch on a copy of `save` (or a fresh title without one) and settle
    the window; never leaves a window running when it fails."""
    for attempt in range(attempts):
        run = base / (name if attempt == 0 else f"{name}-retry{attempt}")
        child, url = launch(binary, run, list(extra), save=save)
        # A newer autosave beside the save, in place before the title reads it.
        if name in AUTOSAVE_FOR:
            auto = run / AUTOSAVE
            shutil.copy2(AUTOSAVE_FOR[name], auto)
            later = time.time() + 600
            os.utime(auto, (later, later))
        try:
            flow = Live6Flow(run, child, url, report, time.monotonic())
            flow.__class__ = Live9Flow
            flow.deadline = time.monotonic() + 40 * 60
            flow.tick_budget = 40 * DAY
            flow.bootstrap()
            flow.window = int(query(url, with_types=[PRIMARY])[0]["entity"])
            flow.settle(SIZES[0])
            return run, child, flow
        except RuntimeError as error:
            report.setdefault("start_retries", []).append(f"{name} attempt {attempt}: {error}")
            stop(child, run, report, name)
    raise RuntimeError(f"{name}: the window did not settle in {attempts} launches")


def probe(flow, report, checks, fixture):
    """Which fixtures this build opens: the title's words for each."""
    time.sleep(1.5)
    texts = flow.dump(f"title-{fixture}")
    report.setdefault("fixture_titles", {})[fixture] = texts[:20]
    flow.size_shots(f"probe-{fixture}", sizes=SIZES[:1])


def step_days(flow, days):
    """Run the clock `days` game days, paused, in 200-tick steps."""
    flow.call("npc/pause")
    ticks = int(days * DAY)
    while ticks > 0:
        batch = min(200, ticks)
        flow.call("npc/step", {"ticks": batch})
        ticks -= batch
        time.sleep(0.05)
    time.sleep(0.8)


def save_as(flow, name):
    """Ctrl+S, then keep a copy of the save as a fixture of this build."""
    path = flow.run / SAVE
    before = path.stat().st_mtime if path.exists() else 0
    flow.key("KeyS", hold=("ControlLeft",))
    wait_for(lambda: path.exists() and path.stat().st_mtime > before or None, 60)
    time.sleep(1.0)
    MADE.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, MADE / f"{name}.json")
    flow.report.setdefault("made_fixtures", []).append(str(MADE / f"{name}.json"))


def new_screen(flow, report, checks):
    """The title without a save, then the New screen at both sizes."""
    time.sleep(1.5)
    flow.dump("title-fresh")
    flow.size_shots("title-fresh")
    flow.click("New game")
    time.sleep(1.2)
    texts = flow.dump("new-screen")
    checks.append({"check": "the New screen offers Live first, Oversee second and Shape as a link",
                   "ok": all(word in texts for word in ("Live", "Oversee", "Or shape the land first")),
                   "texts": texts[:30]})
    flow.size_shots("new-screen")


def make_bases(flow, report, checks):
    """From the New screen: begin in Oversee, then save at day 0, 3 and 8 as
    fixtures of this build (the shipped fixtures are refused by it)."""
    flow.click("Oversee the village")
    wait_for(lambda: flow.has_widget("Oversee") or None, 120)
    time.sleep(2.0)
    flow.call("npc/pause")
    save_as(flow, "base0")
    step_days(flow, 3)
    save_as(flow, "base3")
    step_days(flow, 5)
    save_as(flow, "base8")
    flow.dump("base8")
    flow.size_shots("base8-oversee", sizes=SIZES[:1])


# --- Title cards -------------------------------------------------------------

def edited_save(name, change):
    """A copy of base3 with its header changed by `change(data)`."""
    data = json.loads((MADE / "base3.json").read_text(encoding="utf-8"))
    change(data)
    path = MADE / f"{name}.json"
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def older(data):
    data["header"]["island_save_version"] = int(data["header"]["island_save_version"]) - 1


def newer(data):
    data["header"]["island_save_version"] = int(data["header"]["island_save_version"]) + 1


def other_build(data):
    data["header"]["island_content_hash"] = "0123456789abcdef"


def title_card(kind):
    def body(flow, report, checks):
        time.sleep(1.5)
        texts = flow.dump(f"title-{kind}")
        report.setdefault("title_cards", {})[kind] = texts[:16]
        flow.size_shots(f"title-{kind}")
    body.__name__ = f"title_{kind}"
    return body


def title_parts():
    MADE.mkdir(parents=True, exist_ok=True)
    unreadable = MADE / "unreadable.json"
    text = (MADE / "base3.json").read_text(encoding="utf-8")
    unreadable.write_text(text[: len(text) // 3], encoding="utf-8")
    return [("title-older", edited_save("older", older), [title_card("older")]),
            ("title-other-build", edited_save("other-build", other_build), [title_card("other-build")]),
            ("title-newer", edited_save("newer", newer), [title_card("newer")]),
            ("title-unreadable", unreadable, [title_card("unreadable")]),
            ("title-autosave", MADE / "base3.json", [autosave_title])]


def autosave_title(flow, report, checks):
    """A newer autosave beside the save: the title offers it. The autosave is
    placed while the title shows (the game reads it when the title opens, so
    the title is asked again through Settings and Back)."""
    time.sleep(1.5)
    texts = flow.dump("title-autosave")
    checks.append({"check": "a newer autosave is offered on the title",
                   "ok": "Continue from the autosave" in texts, "texts": texts[:16]})
    # Defect 16: the card's two choices only, both in sight (the first has
    # focus; its tooltip stands to its right, the second is below it).
    choices = [text for text in texts if text.startswith("Continue")]
    report["autosave_choices"] = choices
    checks.append({"check": "the autosave title offers the card's two choices and no third Continue",
                   "ok": sorted(set(choices)) == ["Continue from my save", "Continue from the autosave"],
                   "choices": choices})
    flow.size_shots("title-autosave")


# --- Oversee from a save of this build ---------------------------------------

def continue_oversee(flow):
    """Continue the save in Oversee and pause. "Continue" would enter Live,
    where the Traveller arrives and, left alone in Oversee afterwards, dies:
    a death of the script's own making."""
    time.sleep(1.0)
    flow.click("Oversee the village")
    wait_for(lambda: flow.has_widget("Oversee") or None, 120)
    time.sleep(2.0)
    flow.call("npc/pause")
    if not flow.panel_texts("Village alerts"):
        flow.click("Oversee")
        time.sleep(1.0)


def text_entity(flow, start, panel=None):
    """The text entity whose words start with `start` (inside `panel`)."""
    inside = flow.within(panel) if panel else None
    for row in flow.ui()[0]:
        if str(row["text"]).startswith(start) and (inside is None or inside(row["entity"])):
            return row["entity"]
    return None


def outlook(flow, report, checks):
    continue_oversee(flow)
    texts = flow.dump("outlook")
    lines = [text for text in flow.panel_texts("Village alerts")]
    report["outlook_lines"] = lines
    checks.append({"check": "the Village outlook stands in the alerts column",
                   "ok": "Village outlook" in lines, "lines": lines})
    flow.size_shots("outlook")
    # A line's tooltip.
    first = next((text for text in lines if text not in ("Village outlook", "Alerts", "No alerts", "Hide")
                  and (":" in text or text.startswith(("All is well", "Food", "Water")))), None)
    entity = text_entity(flow, first, "Village alerts") if first else None
    if entity:
        flow.hover(entity)
        flow.snap("live9-outlook-tooltip-1280x720", SIZES[0])
    # Folded.
    flow.click("Village alerts > Hide")
    time.sleep(0.8)
    report["outlook_folded"] = flow.panel_texts("Village alerts")
    flow.size_shots("outlook-folded")
    flow.click("Show")
    time.sleep(0.8)
    # A window lower than 640 px: two lines.
    low = (1024, 600)
    flow.settle(low)
    time.sleep(1.5)
    flow.park(low)
    report["outlook_low"] = flow.panel_texts("Village alerts")
    flow.snap(f"live9-outlook-low-{low[0]}x{low[1]}", low)
    # Defect 8: Compact folds the stack; Show opens it there.
    if flow.has_widget("Show"):
        flow.click("Show")
        time.sleep(1.0)
        flow.park(low)
        opened = flow.panel_texts("Village alerts")
        report["outlook_low_shown"] = opened
        checks.append({"check": "Show opens the alerts in a Compact window",
                       "ok": "Village outlook" in opened, "texts": opened})
        flow.snap(f"live9-outlook-low-shown-{low[0]}x{low[1]}", low)
    flow.settle(SIZES[0])


def goods_rows(flow):
    """The goods table's row names, in order."""
    texts = flow.panel_texts(LEDGER)
    if "Since yesterday" in texts:
        start = texts.index("Since yesterday") + 1
    elif "Change" in texts:
        start = texts.index("Change") + 1
    else:
        return []
    return texts[start:]


def press(flow, prefix):
    """Activate the ledger row whose text starts with `prefix`; False when
    there is not exactly one."""
    try:
        flow.press_row(prefix)
        time.sleep(0.8)
        return True
    except RuntimeError:
        return False


def goods(flow, report, checks):
    continue_oversee(flow)
    flow.open_page("Economy")
    time.sleep(1.0)
    texts = flow.dump("goods")
    signed = [text for text in texts if text[:1] in "+-" and text[1:].isdigit()]
    report["price_changes"] = signed
    checks.append({"check": "the Goods table shows a price change beside some price",
                   "ok": bool(signed), "changes": signed})
    flow.size_shots("goods-table")
    # The good card under its row: the first good with a price.
    for name in ("Bread", "Clean water", "Logs", "Planks", "Berries"):
        if press(flow, name):
            break
    time.sleep(1.2)
    card = flow.dump("good-card")
    report["good_card"] = [text for text in card if text.startswith(
        ("Price", "No price", "Last market", "It has not", "Used by", "Going without", "Held up",
         "Made by", "Nobody makes", "About", "Open sites", "Stock", "Last ", "More about"))]
    flow.size_shots("good-card")
    for start, label in (("Last market", "line3"), ("Held up", "line8")):
        entity = text_entity(flow, start, LEDGER)
        report.setdefault("card_tips", {})[label] = entity is not None
        if entity:
            flow.hover(entity)
            flow.snap(f"live9-good-card-tip-{label}-1280x720", SIZES[0])
    # The narrow ledger: 1024x768 is the Standard layout; the fold shows
    # where the ledger is Compact.
    flow.settle(SIZES[1])
    time.sleep(1.0)
    report["good_card_1024"] = flow.panel_texts(LEDGER)
    more = [text for text in flow.panel_texts(LEDGER) if text.startswith("More about")]
    if more:
        press(flow, more[0])
        time.sleep(0.8)
        flow.snap("live9-good-card-fold-open-1024x768", SIZES[1])
    flow.settle(SIZES[0])
    # "Show {job} in Jobs": a good that nobody makes.
    found = None
    names = [text for text in goods_rows(flow) if text[:1].isupper() and not text[:1].isdigit()]
    for name in names[:40]:
        if not press(flow, name + "\n"):
            continue
        time.sleep(0.6)
        links = [text for text in flow.panel_texts(LEDGER) if text.startswith("Show ") and text.endswith(" in Jobs")]
        if links:
            found = (name, links[0])
            break
        press(flow, name + "\n")
        time.sleep(0.3)
    report["show_job"] = found
    checks.append({"check": "a good nobody makes links to its job in Jobs", "ok": found is not None,
                   "found": found})
    if found:
        flow.size_shots("good-card-nobody-makes")
        press(flow, found[1])
        time.sleep(0.25)
        flow.snap("live9-show-job-flash-1280x720", SIZES[0])
        time.sleep(1.0)
        flow.snap("live9-show-job-after-1280x720", SIZES[0])


def economy_page(flow, page):
    continue_oversee(flow)
    flow.open_page("Economy")
    time.sleep(0.8)
    flow.click(page)
    time.sleep(1.0)


def orders(flow, report, checks):
    economy_page(flow, "Jobs")
    flow.size_shots("jobs")
    # An order while paused: the pill counts it.
    activate_first(flow, "+")
    time.sleep(0.8)
    texts = flow.dump("jobs-order-paused")
    pill = [text for text in texts if text.startswith("Paused")]
    report["pill"] = pill
    checks.append({"check": "the Paused pill counts the waiting order",
                   "ok": any("order waiting" in text for text in pill), "pill": pill})
    queued = [text for text in texts if "Queued" in text or "queued" in text]
    report["queued_lines"] = queued
    flow.size_shots("jobs-order-paused")
    activate_first(flow, "+")
    time.sleep(0.6)
    flow.dump("jobs-two-orders")
    flow.snap("live9-jobs-two-orders-paused-1280x720", SIZES[0])
    # Time runs: the orders apply; "Your orders" says how they stand.
    flow.call("npc/step", {"ticks": 4})
    time.sleep(1.0)
    texts = flow.dump("jobs-orders-applied")
    report["your_orders"] = [text for text in texts if "wanted" in text or text.startswith("Your orders")]
    checks.append({"check": "Jobs shows the Your orders block", "ok": any(text.startswith("Your orders")
                                                                          for text in texts)})
    flow.size_shots("jobs-your-orders")
    # Priorities: High on a topic, then one job's own level.
    flow.click("Priorities")
    time.sleep(0.8)
    flow.size_shots("priorities")
    activate_first(flow, "High")
    flow.call("npc/step", {"ticks": 4})
    time.sleep(0.8)
    press(flow, "Give one job its own level")
    time.sleep(0.8)
    for _ in range(3):
        flow.key("PageDown")
        time.sleep(0.3)
    texts = flow.dump("priorities-one-job")
    flow.size_shots("priorities-one-job-chips")
    rows = flow.button_rows(LEDGER)
    labels = [text for entity, text in rows]
    after = next((index for index, text in enumerate(labels) if text.startswith("Give one job")), None)
    chips = [entity for entity, text in rows[after + 1:]
             if text not in ("Low", "Normal", "High", "Same as the others")][:1] if after is not None else []
    report["job_chips"] = labels[after:][:20] if after is not None else labels[:40]
    if chips:
        flow.call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": chips[0]}})
        time.sleep(0.5)
        flow.call("npc/step", {"ticks": 4})
        time.sleep(1.0)
    texts = flow.dump("priorities-own-level")
    checks.append({"check": "a job's own workplace level shows with Same as the others",
                   "ok": "Same as the others" in texts})
    flow.size_shots("priorities-own-level")


def alerts_fold(flow, report, checks):
    """Days run until three residents of one village share a need alert's
    reason, or the budget ends."""
    continue_oversee(flow)
    found = None
    for day in range(6):
        texts = flow.panel_texts("Village alerts")
        found = next((text for text in texts if " residents in " in text), None)
        if found:
            break
        step_days(flow, 1)
        keep_watching(flow)
    report["alert_fold"] = {"row": found, "texts": flow.panel_texts("Village alerts")}
    checks.append({"check": "three residents with one reason fold into one alert row",
                   "ok": found is not None, "row": found})
    flow.size_shots("alerts")
    if found:
        flow.click(found)
        time.sleep(1.0)
        flow.dump("alerts-people")
        flow.size_shots("alerts-fold-people")


SEASONS = ("Spring", "Summer", "Autumn", "Winter")


def season_ended(texts):
    """The season card's title is on screen ("Autumn has ended")."""
    return any(text in (f"{name} has ended" for name in SEASONS) for text in texts)


def keep_watching(flow):
    """The never-arrived character of an Oversee start can die; its modal
    is put away so the capture goes on (a defect the report names)."""
    if flow.has_widget("Keep watching the village"):
        flow.report.setdefault("life_ended_modal", 0)
        flow.report["life_ended_modal"] += 1
        flow.snap("live9-life-ended-in-oversee-1280x720", SIZES[0])
        flow.click("Keep watching the village")
        time.sleep(0.8)
        return True
    return False


def season(flow, report, checks):
    """From day 0 (two days before winter): the column empty, the card opens
    by itself; then the calendar's Last season; then whether the card yields
    to the build tray and to placing."""
    continue_oversee(flow)
    flow.close_ledger()
    for _ in range(30):
        step_days(flow, 0.25)
        keep_watching(flow)
        if season_ended(flow.texts()):
            break
    texts = flow.dump("season-card")
    checks.append({"check": "the season card opens into the empty right column",
                   "ok": season_ended(texts)})
    flow.size_shots("season-card")
    # Does it yield? The build tray (B), then Shape placing (L).
    flow.key("KeyB")
    time.sleep(1.0)
    report["season_after_build"] = season_ended(flow.texts())
    flow.snap("live9-season-then-build-1280x720", SIZES[0])
    flow.key("KeyB")
    time.sleep(0.6)
    flow.click("Shape")
    time.sleep(1.2)
    flow.key("KeyL")
    time.sleep(0.8)
    report["season_after_place"] = season_ended(flow.texts())
    flow.snap("live9-season-then-place-1280x720", SIZES[0])
    flow.key("Escape")
    time.sleep(0.4)
    flow.click("Oversee")
    time.sleep(1.0)
    report["season_back_in_oversee"] = season_ended(flow.texts())
    flow.snap("live9-season-back-in-oversee-1280x720", SIZES[0])
    # The calendar's Last season.
    flow.click("Calendar")
    time.sleep(0.8)
    flow.dump("calendar")
    flow.size_shots("calendar-last-season", prepare=lambda size: None)
    if flow.has_widget("Last season"):
        flow.click("Last season")
        time.sleep(1.0)
        flow.snap("live9-season-from-calendar-1280x720", SIZES[0])


def season_toast(flow, report, checks):
    """The right column in use when the season ends: a toast offers it."""
    continue_oversee(flow)
    flow.open_page("Economy")
    time.sleep(0.6)
    toast = None
    for _ in range(30):
        step_days(flow, 0.25)
        if keep_watching(flow) or flow.ledger_page() is None:
            flow.open_page("Economy")
        toast = next((text for text in flow.texts() if text.endswith("has ended.")
                      and not text.startswith("This life")), None)
        if toast:
            break
    report["season_toast"] = toast
    checks.append({"check": "a season ending over an open page offers its card in a toast",
                   "ok": toast is not None and flow.has_widget("See the season"), "toast": toast})
    flow.size_shots("season-toast", prepare=lambda size: None)
    if flow.has_widget("See the season"):
        flow.click("See the season")
        time.sleep(1.0)
        flow.snap("live9-season-from-toast-1280x720", SIZES[0])


def shape(flow, report, checks):
    continue_oversee(flow)
    flow.click("Shape")
    time.sleep(1.5)
    flow.key("KeyL")
    time.sleep(0.8)
    flow.dump("place-panel")
    flow.size_shots("shape-place-panel")
    for item in ("Barrel", "Garden"):
        if flow.has_widget(item):
            flow.click(item)
            time.sleep(0.6)
            flow.dump(f"place-{item.lower()}")
            flow.size_shots(f"shape-place-{item.lower()}-cost")
            break
    if flow.has_widget("Place for free"):
        flow.click("Place for free")
        time.sleep(0.4)
        flow.call("npc/step", {"ticks": 4})
        time.sleep(1.0)
        texts = flow.dump("place-free")
        checks.append({"check": "Place for free lights the Free badge", "ok": "Free" in texts})
        flow.size_shots("shape-place-free")
        flow.click("Place for free")
        flow.call("npc/step", {"ticks": 4})
        time.sleep(0.6)
    flow.key("Escape")
    time.sleep(0.5)
    # X removes a patch, Ctrl+Z brings it back.
    flow.key("KeyN")
    time.sleep(0.8)
    at, tag = flow.hover_until(SIZES[0], lambda tag: len(tag) == 1 and tag[0].startswith("Place"))
    report["patch_place"] = {"at": at, "tag": tag}
    if at:
        flow.click_here()
        flow.call("npc/step", {"ticks": 4})
        time.sleep(0.8)
        flow.key("KeyX")
        time.sleep(0.6)
        flow.cursor(at[0], at[1])
        time.sleep(0.8)
        report["remove_tag"] = flow.tag()
        flow.snap("live9-shape-remove-hover-1280x720", SIZES[0])
        flow.click_here()
        flow.call("npc/step", {"ticks": 4})
        time.sleep(0.8)
        flow.dump("patch-removed")
        flow.snap("live9-shape-patch-removed-1280x720", SIZES[0])
        flow.key("KeyZ", hold=("ControlLeft",))
        flow.call("npc/step", {"ticks": 4})
        time.sleep(0.8)
        texts = flow.dump("patch-back")
        checks.append({"check": "Ctrl+Z brings the removed patch back",
                       "ok": any("back" in text.lower() or "Placed" in text for text in texts)})
        flow.snap("live9-shape-patch-back-1280x720", SIZES[0])


def why(flow, report, checks):
    continue_oversee(flow)
    flow.open_page("People")
    time.sleep(0.8)
    rows = [entity for entity, text in flow.button_rows(LEDGER) if "\n" in text]
    if not rows:
        raise RuntimeError("no resident row")
    flow.call("world.trigger_event", {"event": "bevy_ui_widgets::Activate", "value": {"entity": rows[0]}})
    time.sleep(1.2)
    flow.click("Why")
    time.sleep(1.0)
    flow.dump("why")
    flow.size_shots("why")
    behaviour = flow.run / "behaviour.json"
    if flow.has_widget("Save behaviour to file"):
        flow.click("Save behaviour to file")
        time.sleep(1.5)
        flow.dump("why-saved")
        report["behaviour_saved"] = behaviour.is_file()
        flow.snap("live9-why-saved-1280x720", SIZES[0])
        if behaviour.is_file():
            flow.click("Reload behaviour from file")
            time.sleep(1.5)
            flow.dump("why-reloaded-good")
            flow.snap("live9-why-reload-good-1280x720", SIZES[0])
        behaviour.write_text("{ not a behaviour", encoding="utf-8")
        flow.click("Reload behaviour from file")
        time.sleep(1.5)
        flow.dump("why-reloaded-bad")
        flow.size_shots("why-reload-bad", prepare=lambda size: None)


def pause_menu(flow, report, checks):
    continue_oversee(flow)
    flow.key("KeyS", hold=("ControlLeft",))
    time.sleep(2.0)
    flow.key("Escape")
    time.sleep(1.0)
    texts = flow.dump("pause")
    flow.size_shots("pause-menu")
    if flow.has_widget("Quit without saving"):
        flow.click("Quit without saving")
        time.sleep(0.8)
        flow.dump("confirm-quit")
        flow.size_shots("confirm-quit")
        flow.click("Back")
        time.sleep(0.6)
    flow.click("How to play")
    time.sleep(1.0)
    if flow.has_widget("Controls"):
        flow.click("Controls")
        time.sleep(0.8)
    flow.dump("guide-controls")
    flow.size_shots("guide-controls")


# --- The five states the last pass did not reach -----------------------------
#
# Each is made by recorded commands (`npc/command`, the same path the ledger
# uses) and clock steps on a save of this build, never by writing state:
# - Nobody makes / Show {job} in Jobs: `SetProfessionTarget` miller 0 in
#   Harbour, four days (one resident leaves a job per day): the mill has open places and nobody in them; Flour's
#   card says so and links to Jobs (then the scroll-and-flash).
# - Held up: the same order; the bakery's bread now waits for flour, so
#   Flour's card has "Held up: Bread waits for …" (line 8, its tooltip).
# - Compact fold: the Flour card at 1280x690 (below 700 high is Compact).
# - Folded alert row: `SetProfessionTarget` water_keeper 0 in Harbour, up to
#   eight days in quarter days: nobody sells water, so Harbour's residents
#   go thirsty for one reason and fold into "N residents in Harbour are …".
# - The season toast's button: `season_toast` from base0 (two days before
#   winter) with Economy open; the host no longer toasts autosaves over it.

COMPACT = (1280, 690)
SETTLEMENT = "living_world_settlement_v1"


def settlement_id(flow, key="harbour"):
    """The village's SimId from the operator read (a capture helper only;
    nothing the player sees is read this way)."""
    for entity in flow.state()["entities"]:
        settlement = entity.get("components", {}).get(SETTLEMENT)
        if settlement and settlement.get("key") == key:
            return entity["sim_id"]
    raise RuntimeError(f"no settlement {key!r}")


def want(flow, settlement, profession, wanted):
    """A recorded `SetProfessionTarget`, applied by two ticks."""
    receipt = flow.call("npc/command", {"cmd": {"type": "SetProfessionTarget", "settlement": settlement,
                                                "profession": profession, "wanted": wanted}})
    flow.report.setdefault("commands", []).append({"profession": profession, "wanted": wanted,
                                                   "receipt": receipt})
    flow.call("npc/step", {"ticks": 2})
    time.sleep(0.5)


def flour_card(flow):
    flow.open_page("Economy")
    time.sleep(0.8)
    if flow.has_widget("Goods"):
        flow.click("Goods")
        time.sleep(0.6)
    return press(flow, "Flour")


def nobody_makes_and_held_up(flow, report, checks):
    continue_oversee(flow)
    harbour = settlement_id(flow)
    want(flow, harbour, "miller", 0)
    # Residents leave a job one at a time, after the day's work: 4 days.
    for _ in range(4):
        step_days(flow, 1)
        keep_watching(flow)
    opened = flour_card(flow)
    time.sleep(1.0)
    card = flow.dump("flour-card")
    link = next((text for text in card if text.startswith("Show ") and text.endswith(" in Jobs")), None)
    held = [text for text in card if text.startswith("Held up")]
    report["state_nobody_makes"] = {"opened": opened, "nobody": [t for t in card if t.startswith("Nobody")],
                                    "link": link}
    report["state_held_up"] = held
    checks.append({"check": "Flour's card says nobody makes it and links to Jobs",
                   "ok": link is not None, "card": card[:40]})
    checks.append({"check": "Flour's card says what is held up for it", "ok": bool(held), "held": held})
    flow.size_shots("state-flour-card")
    entity = text_entity(flow, "Held up", LEDGER)
    if entity:
        flow.hover(entity)
        flow.snap("live9-state-held-up-tip-1280x720", SIZES[0])
    # The narrow ledger: Compact, where the card folds after its first lines.
    flow.settle(COMPACT)
    time.sleep(1.2)
    texts = flow.panel_texts(LEDGER)
    more = [text for text in texts if text.startswith("More about")]
    report["state_compact_fold"] = {"more": more, "texts": texts[:40]}
    checks.append({"check": "the good card folds in a Compact window", "ok": bool(more)})
    flow.snap(f"live9-state-compact-fold-{COMPACT[0]}x{COMPACT[1]}", COMPACT)
    if more:
        press(flow, more[0])
        time.sleep(0.8)
        flow.snap(f"live9-state-compact-fold-open-{COMPACT[0]}x{COMPACT[1]}", COMPACT)
    flow.settle(SIZES[0])
    time.sleep(1.0)
    if link:
        press(flow, link)
        time.sleep(0.25)
        flow.snap("live9-state-show-job-flash-1280x720", SIZES[0])
        time.sleep(1.0)
        flow.dump("show-job-after")
        flow.snap("live9-state-show-job-after-1280x720", SIZES[0])


def folded_alert_row(flow, report, checks):
    continue_oversee(flow)
    harbour = settlement_id(flow)
    want(flow, harbour, "water_keeper", 0)
    found = None
    # Up to 8 days in quarter days: the wells' keepers leave one a day.
    for _ in range(32):
        step_days(flow, 0.25)
        keep_watching(flow)
        found = next((text for text in flow.panel_texts("Village alerts") if " residents in " in text), None)
        if found:
            break
    report["state_alert_fold"] = {"row": found, "texts": flow.panel_texts("Village alerts")}
    checks.append({"check": "residents with one reason fold into one alert row", "ok": found is not None,
                   "row": found})
    flow.size_shots("state-alerts-folded")
    if found:
        flow.click(found)
        time.sleep(1.0)
        flow.dump("state-alerts-people")
        flow.size_shots("state-alerts-fold-people")


def clock(flow):
    """The HUD's "hh:mm" as minutes past midnight, or None."""
    for text in flow.texts():
        text = str(text)
        if len(text) >= 5 and text[2] == ":" and text[:2].isdigit() and text[3:5].isdigit():
            return int(text[:2]) * 60 + int(text[3:5])
    return None


def oversee_start(flow, report, checks):
    """A new game begun in Oversee, a day and a half run: the character who
    never arrived must not die nor be named in the alerts (defect 1); then
    the sea at dawn (the lattice, defect 22)."""
    time.sleep(1.5)
    flow.click("New game")
    time.sleep(1.2)
    flow.click("Oversee the village")
    wait_for(lambda: flow.has_widget("Oversee") or None, 120)
    time.sleep(2.0)
    flow.call("npc/pause")
    modal = 0
    for _ in range(6):
        step_days(flow, 0.25)
        if keep_watching(flow):
            modal += 1
    alerts = flow.panel_texts("Village alerts")
    traveller = [text for text in flow.texts() if "Traveller" in str(text)]
    report["oversee_start"] = {"life_ended_modals": modal, "alerts": alerts, "traveller": traveller}
    checks.append({"check": "an Oversee start runs a day and a half with no life-ended modal",
                   "ok": modal == 0, "modals": modal})
    checks.append({"check": "the alerts never name the character who never arrived",
                   "ok": not any("Traveller" in str(text) for text in alerts), "alerts": alerts})
    flow.size_shots("oversee-start-day-and-a-half")
    # Dawn: step in tenths of an hour until the HUD clock reads 05:00-06:30.
    seen = []
    for _ in range(240):
        minutes = clock(flow)
        seen.append(minutes)
        if minutes is not None and 300 <= minutes <= 390:
            break
        step_days(flow, 1 / 96)
    report["dawn_clock"] = seen[-1]
    flow.dump("dawn")
    flow.size_shots("sea-at-dawn")


def base(name):
    return MADE / f"{name}.json"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path,
                        default=Path(os.environ.get("ISLAND_GAME_EXE", FROZEN)))
    parser.add_argument("--out", type=Path, default=ROOT / "target/npc-out/ui-live9")
    parser.add_argument("--only", default="probe")
    parser.add_argument("--fixtures", type=Path, default=None,
                        help="the folder of fixtures the probe opens (default: target/npc-out/fixtures)")
    parser.add_argument("--made", type=Path, default=None,
                        help="where the New section keeps the base saves it makes (default: fixtures9)")
    args = parser.parse_args()
    global MADE, PROBE_FIXTURES
    if args.fixtures is not None:
        PROBE_FIXTURES = args.fixtures.resolve()
    if args.made is not None:
        MADE = args.made.resolve()
        AUTOSAVE_FOR["title-autosave"] = MADE / "base8.json"
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"no game at {binary}")
    wanted = [part.strip() for part in args.only.split(",") if part.strip()]
    base = ROOT / "target" / ("playtest-ui-live9-" + uuid.uuid4().hex[:10])
    report = {"ok": False, "binary": str(binary), "binary_sha256": file_hash(binary),
              "screenshots": {}, "checks": [], "sections": wanted}
    args.out.mkdir(parents=True, exist_ok=True)
    try:
        for name in wanted:
            for label, save, steps in SECTIONS[name]():
                try:
                    run, child, flow = start(binary, base, label, report, save)
                except RuntimeError as error:
                    report["checks"].append({"check": f"section {label} started", "ok": False,
                                             "reason": str(error)})
                    continue
                try:
                    for step in steps:
                        flow.section(f"{label}:{step.__name__}",
                                     lambda step=step: step(flow, report, report["checks"]))
                finally:
                    stop(child, run, report, label)
                    out = args.out / name
                    out.mkdir(parents=True, exist_ok=True)
                    if (run / "shots").is_dir():
                        for image in sorted((run / "shots").glob("*.png")):
                            shutil.copy2(image, out / image.name)
        report["ok"] = all(check["ok"] for check in report["checks"])
    finally:
        report["copied"] = sorted(str(path) for path in args.out.rglob("*.png"))
        name = "summary-" + "-".join(wanted)[:60] + ".json"
        (args.out / name).write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                     encoding="utf-8")
        print(base, flush=True)


def probe_parts():
    parts = []
    for path in sorted(PROBE_FIXTURES.glob("*.json")):
        if path.stem == "older-save":
            continue
        fixture = path.stem
        parts.append((f"probe-{fixture}", path,
                      [lambda flow, report, checks, fixture=fixture: probe(flow, report, checks, fixture)]))
    for part in parts:
        part[2][0].__name__ = "probe"
    return parts


SECTIONS = {"probe": probe_parts,
            "new": lambda: [("new", None, [new_screen, make_bases])],
            "oversee-start": lambda: [("oversee-start", None, [oversee_start])],
            "title-cards": title_parts,
            "outlook": lambda: [("outlook", base("base8"), [outlook])],
            "goods": lambda: [("goods", base("base8"), [goods])],
            "orders": lambda: [("orders", base("base8"), [orders])],
            "alerts": lambda: [("alerts", base("base8"), [alerts_fold])],
            "season": lambda: [("season", base("base0"), [season]),
                               ("season-toast", base("base0"), [season_toast])],
            "shape": lambda: [("shape", base("base3"), [shape])],
            "why": lambda: [("why", base("base3"), [why])],
            "pause": lambda: [("pause", base("base3"), [pause_menu])],
            # The five states the last pass did not reach (see above).
            "states": lambda: [("state-goods", base("base3"), [nobody_makes_and_held_up]),
                               ("state-alerts", base("base3"), [folded_alert_row]),
                               ("state-season-toast", base("base0"), [season_toast])]}


if __name__ == "__main__":
    main()
