# Gameplay lifecycle implementation — 0.19 candidate

Implemented the dispatched active-session boundary fixes:

- `SandboxState::apply_settlement` now rejects dormant principals before any dispatch or mutation, including when called directly through the public SDK. Its API comment makes the active-session requirement explicit.
- `SandboxState::set_controlled_motion` now rejects both moving input and zero-speed repositioning for dormant owners before changing position, velocity, heading or queued work. Explicit host placement remains a separate API; hydration/session lifecycle is unchanged.

The settlement regression exercises all seven command families through both direct and wrapped entry points. Each fixture is actually feasible: founding, contribution, funding, a constructed eligible village upgrade, and supply acceptance/cancellation/fulfillment. Each rejected call checks the specific resume guidance, complete serialized-state equality and full catalog validation. Explicit resume then permits that same call through both paths and leaves valid state. The upgrade uses ordinary claim/build/contribution commands; only available fixture resources and experience are trusted test setup.

The controlled-motion regression rejects nonzero movement and stationary repositioning, compares the complete serialized state and validates the dormant invariant. Explicit resume permits ordinary motion and updates both the principal and controlled body. Existing price, payment, ownership and command schemas are unchanged.

`rustfmt --edition 2024` completed for all four changed Rust files. Rust execution is centrally coordinated by the integrator; no test-pass claim is made here until the integrator's commands complete. Proposed focused checks:

```
cargo test --offline --locked -p terra-sandbox --test settlements dormant_settlement_actions_reject_direct_and_wrapped_calls_without_mutation
cargo test --offline --locked -p terra-sandbox --test living dormant_controlled_motion_cannot_move_or_reposition_bodies_until_resume
cargo test --offline --locked -p terra-sandbox --test settlements --test living
```

This is an SDK invariant correction, not evidence of a reproduced network exploit; ordinary network intent already passes through the guarded wrapper. It narrows G03/G18 session consistency. Long finite careers, human goal comprehension and community succession/dissolution remain open product work.

## Cross-review documentation polish

The public `set_player_position` API comment now explicitly identifies trusted host movement/provisioning/restore placement, including dormant owners, and distinguishes it from active-session manual motion admission. This documents the intentional host lifecycle escape hatch without changing behavior. Rustfmt completed for `state.rs`; no additional Cargo run was performed by this owner.
