# ADR 0025: Event-aware AI continuation

- Status: Accepted for the user-requested stream increment
- Date: 2026-10-29

The operator needs connected external AI agents to react to game events as well
as inactivity. Extend ADR 0023's host supervisor using the existing agent event
ring. Event delivery remains outside the simulation schedule, hashes, snapshots
and replay inputs; only the AI's resulting ordinary commands affect gameplay.

Notifications belong to the currently authorized controller and actor. Only
selected own-actor outcomes/needs/failures and public calendar transitions are
eligible by default. Content-specific summaries use a generic host adapter,
never content names or dependencies in the capability crate. Do not copy raw
arbitrary journal payloads or private host data into provider prompts or status.

Maintain bounded pending batches with stable event sequence/tick identities.
Deduplicate harvested events, coalesce bursts and report delivery/drop counts.
A pending notification can bypass the inactivity threshold but never the
existing pause, busy, original-client, owned-runner, exact-session lock, control,
cooldown, backoff, hourly or concurrency safeguards. Notifications are historical
signals: the resumed agent observes current state before deciding what to do.

Continue the exact linked CLI conversation through structured argv and prompt
stdin. Event-aware scheduling must not interrupt a provider turn, choose actor
actions, advance time, infer a private conversation, or change models/permissions.
Link/identity changes and revocation must not deliver one actor's pending events
to another. Failed launches retain pending work subject to bounded retry policy.
Read-only status never harvests into delivery queues or starts a provider.

Expose an event-aware policy switch through the existing world preset editor;
enabled supervision remains the master switch. Saved presets remain immutable,
and optional serialization defaults preserve ordinary legacy descriptors.
Verification uses an owned fake CLI for event payload delivery and isolation;
authenticated provider smoke is separately bounded and explicitly authorized.
