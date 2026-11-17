---
type: View
id: defender-picker
title: "Defender picker"
parent: results
status: Implemented
source: "crates/horde_game/src/ui/chrome.rs::defender_picker, defender_picker_of, picker_entries_of, PICKER_CHIPS; crates/horde_game/src/ui/home.rs::next_defense"
---

# Defender picker

Choose who defends the next stage: you, Mara or a recruit.

- **Entry, exit and actions:** A "Defender" row inline on Results (between defenses of a running journey), on the Briefing and in Camp home's Next defense card. Up to three defenders are selectable chips; more become a combo box. Choosing dispatches `Command::SetNextDefender`, which only stores the choice; the Briefing primary then reads "Deploy personally" or "Deploy <name>".
- **State boundary:** Options and their order come from `Game::available_defenders`; Mara is listed once the Companion feature is owned. A defender core would refuse (locked, guarding, on an expedition, already defending, or carrying an item another deployment holds) is disabled and shows core's `DefenderBlocker` label beside its name and on hover. Nothing is drawn while a defense is live or while only one defender can be chosen.
- **Responsiveness and accessibility:** The label and its options share one centred row wherever the picker is placed; the blocked reason is text, never colour alone.

This is not the camp pages' "You / Mara" editor switch (`mode_picker`), which only chooses whose equipment and training a page edits.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
