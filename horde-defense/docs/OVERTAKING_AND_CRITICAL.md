# Overtaking and critical-hostile state

The user's later instruction permits fast enemies to overtake slower approaching enemies. This supersedes the old rule that every moving body blocks the next body in its lane. The battlefield retains its deployment-frozen lane count (3-17 with shipped settings), and no enemy changes its assigned lane.

Approaching enemies advance at their own effective movement speed. A faster peer within 1.4 yards begins a deterministic side step. Interior-lane peers move to opposite sides; outer-lane passing moves inward. The saved `Enemy.pass_offset` is bounded to ±.32 yard and eases at up to two yards/second, including existing injury/fatigue movement reductions. It introduces no RNG draws. The displacement stays inside its assigned lane, and outer lanes retain the valid corpse/feedback vertical bounds.

`Enemy.y()` includes this displacement. Rendering, mouse hit geometry, turret range, area attacks, corpse positions and loot positions must use that same method. Hit stop, stun and prone conditions hold lateral movement as well as forward movement. Stable actor IDs remain attached to the same enemy throughout the maneuver.

Walls and the defense line still stop movement. A genuinely stalled front establishes a queue; an approaching moving body does not. Queued actors do not gain additional contact-attacker slots or bypass walls. Traps still cover their complete assigned-lane trigger strip, including the passing displacement, and retain their authored charges/cooldowns. No enemy is pushed rightward to create spacing.

Passing and queue geometry are serializable authoritative state. Old enemies without `pass_offset` default to zero. Validation rejects non-finite/out-of-lane offsets. Tests cover fast passing while the slow enemy continues, smooth separation in middle/outer lanes, intact walls, trap coverage, pause/checkpoint preservation and identical fixed-tick outcomes under 1×/2×/4× observation batches.

## Zero HP in NPC combat

The user explicitly chose to retain the injury rules and clearly label critical enemies. NPC hostiles can remain conscious at zero or negative HP. They move with the existing injury reductions and face consciousness checks at their next legal one-second turn. Zero HP alone grants no kill, corpse or reward.

`Enemy.condition(mode)` returns `Active`, `Critical` or `Defeated`; `critically_wounded(mode)` identifies a still-active NPC hostile at zero/negative authoritative injury HP. Terminal means dead, unconscious or mortally wounded in NPC mode; Personal mode still uses zero HP as defeat. Other temporary conditions remain available through the existing injury flags.

A zero-bar actor can therefore be legally critical while still moving. Presentation should show **Critical / still fighting** and the real signed injury HP, instead of an empty dead-looking bar. No GURPS death check, attack cadence or zero-HP rule changed.

Actual terminal actors are excluded from movement and attacks and reaped in the resolving fixed tick. Regressions exercise Personal firing/melee, traps/turrets in both modes, NPC catastrophic hits and legal HT-turn unconsciousness under 1×/2×/4× batches. Corpse position/tick and reward identity commit once; subsequent steps cannot move or pay that actor again.
