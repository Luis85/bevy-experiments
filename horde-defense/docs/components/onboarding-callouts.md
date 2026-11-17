---
type: View
id: onboarding-callouts
title: "Unlock callouts"
parent: preparation
status: Implemented
source: "crates/horde_game/src/ui/onboarding.rs::callout_row, CALLOUTS, seed_seen_callouts, due"
---

# Unlock callouts

Announce a newly opened feature once, where the player will look next.

- **Entry, exit and actions:** One callout at a time, in a strip under the camp row on Camp and Upgrades, never in battle or on Results. Each reads a core unlock ("New: Mara - find her in Camp home", "New: Recruiting - liberation Results and Camp home", "New: Expeditions - Camp home › Operations", "New: Country operations - Camp home › Operations", "New: Build defenses - press <Build key> in battle", the grenade and Molotov lines, "New: Recruits can defend - choose on Results or Briefing" and "New: Auto-sell - set a rule in Equipment"). "Show me" opens the place it names (only when that is somewhere else) and "Got it" dismisses it; visiting the place marks it seen as well.
- **State boundary:** The seen set is `settings.json`'s `seen_callouts`, per profile and outside the save and its digest. `seed_seen_callouts` marks every already-unlocked callout seen once, so an existing journey is not flooded; New game starts an empty set and Restore defaults keeps it. Callout ids are append-only ("feature:" + `CampaignFeature::id()`, otherwise "<area>:<name>").
- **Responsiveness and accessibility:** Hidden, but not marked seen, while "Show contextual field guidance" is off. The strip is static, so reduced effects has nothing to remove; key names come from the saved bindings.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
