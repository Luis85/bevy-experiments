# NPC rules coverage — version 1

Pinned reference: **GURPS Fourth Edition Lite, August 2004**, published by
Steve Jackson Games. The implementation identifier is `lastlight-lite-2004-v1`,
correction revision 2 (`rules::RULESET_CORRECTION`).
The [publisher's reference page](https://www.sjgames.com/gurps/lite/) returned
HTTP 403 during implementation. The original publisher-authored August 2004
PDF was inspected through the [GDD's hosted copy](https://bsg-75.net/resources/rpg-systems/gurps/GURPS%20Lite%20(Fourth%20Edition).pdf).
No PDF, artwork, rulebook prose, or proprietary equipment table is bundled.

`rules.rs` supplies deterministic procedures and reference cases for attributes,
skill-point costs, critical rolls, active Dodge, Aim and range, damage types,
armor, shock, major wounds, stun, knockdown, consciousness, mortality, and fatigue.
Reference locations: attributes/derived values pp. 4–6; skills pp. 12–13;
combat pp. 24–29; injury/fatigue pp. 29–31. Tests include exact 216-outcome
probabilities, critical boundaries, the eight-yard range example, protection
before wounding, zero-HP survival, temporary stun, mortality, and save continuity.

The simulation owns legal action opportunities, readiness, ammunition, target
commitment, and turn ordering; a successful pure attack procedure alone does
not establish that an action was legal. UI probability labels must identify
attack-roll success, rather than a final chance to injure.

## Scenario adapters

The GDD's stationary defender, shared close engagement, renewable reserve,
recovery between attempts, limited advancement, authored enemy/weapon/structure
profiles, and terminal hostile removal are game scenario rules. Effective melee
engagement is one yard regardless of the contact lane's screen coordinates.
Dodge never includes a retreat bonus. Critical attack failure requires a Ready
maneuver before the weapon can attack again; this is an authored consequence
under Lite's GM-directed critical-failure handling. Armor divisors above one,
if authored, are explicit scenario modifiers, not a claim of additional coverage.

Automatic fire, pellets, grappling, called hit locations, shields, parries,
retreat steps, movement maneuvers, bleeding, long-term medical recovery,
supplements, and a complete tabletop character builder are unsupported.
Content must stay within the declared subset. This implementation is a
supported-scope adaptation, not a claim to implement the complete GURPS system.

The GDD records publisher permission as a production/distribution dependency.
This repository does not resolve that dependency or claim a digital-game license.

## Explicit implementation boundaries

The equipment catalogue authors its weapon modifiers at ST 10. Human ST 1–20
uses the pinned reference's swing progression, expressed arithmetically; the
catalogue's original modifiers and wound types remain intact. For example,
the starter 1d+2 crowbar becomes 2d+1 at ST 13. Low FP halves effective ST
(round up) for melee damage. ST advancement improves HP and swung damage;
it does not modify firearm damage. A shared trained melee skill follows the
Broadsword point-cost progression across supported implements. That shared
specialization remains an explicit game adapter. Thrusting weapons, weapon
specializations and human ST above 20 are outside the supported catalogue.
Independent HP/FP purchases are excluded from the pinned Lite scope.

The battle schedule gives the defender its action before hostiles, then uses
stable hostile IDs. This declared scenario ordering, eleven-lane geometry,
renewable ammunition and scripted enemy profiles remain distinct from a
complete tabletop encounter. Legal one-second action opportunities, attack
resolution, magazine readiness and injury recovery are still enforced.
Strategic garrisons use explicit guard-versus-hostile-group encounters with
the same resolver; they are not additional eleven-lane battlefields.

Ordinary melee attacks do not charge fatigue per strike. Strategic guard
encounters assess the supported ordinary combat exertion after the fight,
while campaign recovery restores the survivor on recall. Tests and logs
support checking implemented procedures; they do not establish blanket
compatibility with every Fourth Edition rule or supplement.

## Correction revision 2

The combat review rechecked the original reference's Aim, Increased Defense,
critical-hit and strength rules. A cautious NPC may select All-Out Defense
instead of an impossible shot: its +2 Dodge lasts until its next maneuver,
and it cannot simultaneously Aim or Attack. Offensive structures now apply
the target's injured/stunned Dodge and use their authored skill; the turret's
skill is 15 rather than an accidentally stacked 29. Walls use the same attack,
damage and protection procedure without biological shock, knockdown or HT
checks. Those durability units remain an authored scenario-object adapter.

Saved actions, random states and already-paid rewards remain serialized as one
generation. Additive phase and telemetry fields have explicit legacy defaults;
these corrections do not silently regenerate snapshots or reward ledgers.
