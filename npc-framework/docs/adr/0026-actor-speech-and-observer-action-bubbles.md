# ADR 0026: Actor speech and observer action bubbles

- Status: Accepted for the user-requested stream increment
- Date: 2026-10-29

Actors need public dialogue, including speech addressed to another actor, and
the audience needs to understand an external AI's issued actions. Keep the
recorded-input boundary: Island Village speech is an ordinary `VillageAct`
`talk` command. The generic MCP `talk` tool is a convenience wrapper for a
registered `agent/act` verb, with no content dependency or control bypass.

Utterances are bounded public single-line text, at most 240 Unicode characters
and 1024 UTF-8 bytes, without control characters. Addressed speech requires a
living nearby actor; aloud speech reaches living listeners within 6000mm.
Process listeners in stable SimId order. Preserve the old resident-offer query
when `talk` has no text. Journals record speaker speech and recipient-scoped
heard copies; only heard dialogue appears in an actor's bounded recent speech
observation. No private notebook or unseen/distant speech is copied.

Speech history is ephemeral journal information, not a new authoritative
component or persistent chat archive. The host scheduler classifies incoming
speech into a closed notification; it never interpolates dialogue into its
continuation instructions. Agents observe events as game content before acting.

Generic host-only accepted-action notices contain only epoch, sequence, tick,
actor and bounded public verb. They cover both queued actions and command
plans, with no arguments, arbitrary labels, credentials or private reasoning.
Views use separate entities to show transient action/thought and speech bubbles
near actors. Acceptance is an intention, not proof an action completed. Bubbles
respect fog, viewport/HUD collision rules, bounded population and expiry; none
of their timing or layout feeds simulation state. Replay/reset clears stale
view state. Gameplay records text; replay does not invoke an AI provider.
