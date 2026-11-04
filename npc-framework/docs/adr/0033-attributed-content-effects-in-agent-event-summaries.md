# ADR 0033: Attributed content effects in agent event summaries

- Status: Accepted
- Date: 2026-11-02

Long registered gameplay can complete more than the default thirty action, order and craft outcomes. The bounded `recent_events` summary prefers these outcomes over other significant events. Because an applied content `Effect` is in the lower-priority group, thirty old outcomes can exclude a newer actor-attributed effect indefinitely. An actual inventory/payment change can therefore appear in the current state while its receipt never appears in repeated default observations.

Treat an applied `JournalKind::Effect` attributed to the requested actor as an own outcome for this generic summary policy. Keep the existing newest-first selection within the outcome group, bounded result count, chronological output, actor filter, perception-roll filtering and read-only behavior. Do not inspect content names or payload fields, increase buffers, change journal/replay formats or add a game-specific dependency. Failed effects retain their previous priority.

This is a generic tooling bugfix, separate from island mechanics. Its regression uses arbitrary actor effects after a full old-outcome window, foreign actor effects, failed effects and perception noise. The same event-query/response schemas remain valid. Detailed clients should still consume the authenticated incremental `agent/events` stream with the last observed cursor; a bounded observation summary cannot replace a complete event history.

Simulation state, event harvesting, event sequence numbers, notification classifications and acknowledgment rules remain unchanged. Existing outcome-priority behavior against perception chatter remains covered.
