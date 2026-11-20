# Independent network specialist cross-review of gameplay changes

Scope: `settlement/commands.rs`, `living/commands.rs`, new settlement/living regressions, and adjacent public SandboxState mutation APIs. Source review only; root owns execution.

## Conclusion

No blocking correctness or compatibility finding in the dispatched changes. Both new dormant checks occur before mutation. `apply_settlement` now mirrors the existing wrapper admission, and `set_controlled_motion` rejects moving intent and stationary coordinate replacement alike. They do not wake an owner, clear cargo or modify economy on failure.

The seven settlement cases are meaningful: the same exact command succeeds after explicit resume through each tested entry point. The tests cover Found, Contribute, Fund, Upgrade, AcceptContract, CancelContract and FulfillContract; compare complete serialized domain state on rejection and run catalog validation before and after. The upgrade has actual eligible structures/materials/experience rather than relying on an otherwise-invalid request. The motion test checks both principal and body positions, unchanged serialized state and successful movement after resume. Existing broader integration gates still need to pass.

## Adjacent API audit and intended exceptions

| API boundary | Review |
|---|---|
| `apply` | Existing early dormant gate covers ordinary crafting/building/faction/living/transport commands. |
| `apply_living` / `apply_transport` / transaction helpers | Nonpublic to downstream callers; reached through guarded command wrappers or checked host adapters. |
| `deliver_vehicle_with_ground` | `garage_vehicle_position` checks dormancy before placement/transaction. |
| `exit_vehicle_with_ground` | `vehicle_exit_position` checks dormancy before planning/transaction. |
| `move_units_with_environment` | Calls guarded `apply` before changing paths; partial-state/contact preflight is read-only. |
| `set_controlled_motion` | New early gate covers the previously exposed host-motion mutation. |
| `set_player_position` | Deliberately trusted placement API, separate from player intent. Native/server callers supply accepted host coordinates. Its exception is called out in the new controlled-motion documentation. Do not add a blanket gate here without reviewing restore/provisioning semantics. |
| `set_character_profile`, registration, seed, lifecycle, regional load/assembly | Trusted bootstrap/maintenance APIs, not independent user-intent authorization boundaries. SetCharacterProfile wire intent still goes through guarded `apply`. |
| `record_vehicle_motion`, simulation advancement | Trusted physical accounting/simulation callbacks. They are not wire commands; ordinary simulation already excludes dormant owned bodies. The current change does not claim to sandbox a malicious embedding host with mutable public domain state. |

## Nonblocking clarity follow-up

`state.rs` documentation for `set_player_position` currently says trusted simulation/server after movement validation. Add one explicit sentence that this host placement hook may update dormant placement during provisioning/restore, whereas ordinary manual movement uses `set_controlled_motion` after session admission. This would make the intentionally retained exception visible at its own API definition rather than only in the other method's comment. No behavior change is needed.

Do not describe this as closing every public SDK mutation path or as a network exploit fix: the embedding host remains trusted and ordinary network commands already had wrapper protection. It closes the identified direct-settlement and controlled-motion admission defects.
