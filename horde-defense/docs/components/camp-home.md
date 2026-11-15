---
type: View
id: camp-home
title: "Camp home"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/home.rs::home, home_primary, home_footer, liberation_pending; crates/horde_game/src/ui/camp.rs::breadcrumb"
---

# Camp home

The default camp tab (`Tab::Home`): the next defense, who can defend it and what needs the player now.

- **Entry, exit and actions:** The first entry of the camp row ("Camp"), Upgrades' Back to camp, the Recruit modal's Back to camp, Escape from every other camp tab and `resume()` without a live field or a settled report all open it. Before the journey has a starting country there is no Home: a Home request draws the world map. The footer's amber primary is `home_primary`: Return to active defense while a main defense is live, Choose next country when the settled report liberated its country and the journey is still active (it dispatches `ContinueJourney`), Go to briefing while a country is active, otherwise Choose a country. Escape goes to Results when a settled report exists, the journey is active and no defense is live; otherwise it opens the return-to-menu confirmation.
- **Content:** Two columns from a content width of 720 points, one below it. Left: **Next defense** ("<Country> · Defense N · <defender>", or "Active defense · …" while one is live) with the defender picker, then **Defenders**: one row per `Game::available_defenders` entry with its status label, readiness ("HP m · Lv L": the maximum HP it would deploy with at the next threat, and its level), "Power P · threat T · Weak|Even|Strong" from core `defender_power` (`PowerBand::label`: Weak below 85, Even 85-115, Strong above 115; text as well as colour, red, amber or green) and "Defend next" (`SetNextDefender`, disabled with core's reason). Mara's row adds "Training ›", which opens her drill-down. Right: **Alerts** (up to three outbreaks with "Plan response", which dispatches `SelectCountry` and opens the Briefing, then "+N more in Operations"), **Expeditions** ("Open operations ›"), **Offers** (up to two "Recruit ›" offers that open the Recruit modal, and "Upgrade: <node> · N coins" when one is affordable) and **Recent reports** (up to two expedition outcomes, then the last three runs).
- **World clock:** Only Home shows the clock control (Resume world clock / Pause world clock), once operations are available; the `world_clock` key (default P) works on Home only.
- **Drill-downs:** Mara (`Tab::Survivor`) and Operations (`Tab::Operations`) are pages under Home, not row entries: they draw the breadcrumb "‹ Camp home · Mara" or "‹ Camp home · Operations" and the row keeps Camp selected. The header's plague alert still deep-links to Operations from Camp, Results and Upgrades; on Settings and run history it is a red notice, not a leave route.
- **State boundary:** Every row is a core read; the page only renders and queues typed commands. Nothing here is saved; the selected tab is navigation state.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
