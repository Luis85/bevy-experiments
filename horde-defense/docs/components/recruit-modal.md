---
type: View
id: recruit-modal
title: "Recruit modal"
parent: results
status: Implemented
source: "crates/horde_game/src/ui/modal.rs::show_recruit, open_recruit, recruit_joined, recruit_as_label; crates/horde_game/src/ui/results.rs::recruit_block"
---

# Recruit modal

Recruit one survivor from a liberated country, choosing its archetype.

- **Entry, exit and actions:** Results' liberation block ("Recruit from <Country> · N coins"), Camp home's "Recruit ›" offer and Operations' "Recruit survivor" open it. Choose shows the core offer: the recruit's name and origin trait, then two archetypes, each with its tactics and starting stats and a "Recruit as <Archetype> · N coins" button (the first is the modal's amber primary), plus "Not now". `Command::RecruitAs` answering with the recruit's id moves the modal to Joined: "<Name> joined your camp." with the amber "Back to camp" (Camp home, the new roster row in focus) and "Stay here". Escape or a backdrop click closes it.
- **State boundary:** The modal reads `Game::recruit_offer` every frame, so the price and archetypes are always what `RecruitAs` charges and accepts (R10); an offer core withdraws while it is open shows the blocker's label and "Close". On Results, an unowned Survivor network node reads "Learn <node> in Upgrades to recruit from <Country> (<price> coins)" and any other blocker "Recruit from <Country>: <reason>", both muted, in place of the button.
- **Responsiveness and accessibility:** One modal slot (`StatusUi.modal`); a storage error takes it over. The phase change is text, not motion.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
