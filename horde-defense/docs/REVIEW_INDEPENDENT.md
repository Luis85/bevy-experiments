# Independent review outcomes

The comprehensive pass used separate implementation agents for combat/rules, campaign/strategy and native runtime/UX, followed by fresh read-only architecture and visual reviewers.

## Architecture and correctness

The independent reviewer found no P0/P1 issues in inspected scheduling, worker settlement, reservations, lane/blocker ordering, terminal results, pause or recovery. It identified two P2 defects:

1. Short fire/melee taps could be overwritten by a released held-input snapshot before a fixed step. Production now retains pressed attack edges and their press-time aims. Five regressions cover cross-frame taps, same-frame press/release/motion, outer-lane melee, the mouse-only strike button and cadence conservation.
2. The save writer could acknowledge a generation larger than the reader accepted. Both now share an inclusive 64 MiB boundary. The writer preflights the complete envelope before directory/temp/backup mutation; boundary tests preserve both prior generations and a stale temp file.

The follow-up reviewer marked both findings **resolved**. It accepted the engine-free aggregate, typed application commands and named Bevy schedules as appropriate separation for this game. Public facade access and synchronous persistence remain documented tradeoffs, not hidden claims of stronger isolation.

## Native presentation

A fresh reviewer inspected 23 native framebuffers covering the menu, map, all preparation pages, personal/NPC combat, bosses, construction, results, strategic operations, settings, biome variants and peak pressure. It found one P2 defect: an inline NPC recall slider forced the tactical panel beneath the arena.

The label and slider are now stacked and width-bounded; NPC text and observation controls wrap inside an explicit scrolling panel. A layout regression covers both 1440×900 and 1050×700 at 135% UI scale. Normal/minimum NPC and peak captures, plus the survivor sheet, were recaptured. The follow-up reviewer marked the defect **resolved** and gave a **ship for native playtesting** verdict at that review scope.

These were independent source and framebuffer reviews. Reviewers did not rerun builders or claim an OS playthrough. The final combined gate passed 148 tests with format checking and warnings-denied Clippy. Native frame measurements, package checks, manual interaction evidence and remaining balance limits are recorded in [VALIDATION.md](VALIDATION.md).

## Animation, atmosphere and UX follow-up — 2 November

Separate agents implemented tick/event animation, layered audio and native UX, followed by fresh read-only architecture and visual reviews. The architecture reviewer confirmed presentation/authority separation, bounded temporary histories and voices, normalized environmental mixing, current gain application and explicit Bevy scheduling. It identified and confirmed two corrections: authoritative trap shots no longer fabricate turret fire, and exclusive output directories prevent stale animation frames from passing as new captures. Both have regressions; no findings remained from that scoped review.

The visual reviewer inspected 27 fresh native views and six sampled frames from each of three normal/reduced/tropical sequences. Its bounded list contained three P2 findings and no P1: a long NPC HUD action clipped, turret art could not track steep headings, and detailed ground obscured small dark enemies/approach seams. One correction batch added native one-row ellipsis with complete sidebar/tooltip text, calibrated the painted gun spindle/bore and muzzle tracer with a planted tripod, and reduced terrain micro-contrast while reinforcing full-width lane seams.

Fresh `screenshots-polished/` and `motion-*-polished/` renderer output received **ship — all three P2 findings resolved** confirmation at exactly that scope. The final workspace gate passed **185 tests** (72 app, 113 core) and strict Clippy. Active dense-combat frame samples averaged approximately 60 FPS at both tested sizes on the documented machine. No reviewer inferred a manual Windows playthrough, audible mix quality or long-session balance from this evidence. See [VALIDATION.md](VALIDATION.md) for final paths, metrics and remaining human questions.
