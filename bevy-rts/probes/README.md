# Probes

Throwaway crates that answered one question about a dependency before the project relied on
it. They are workspace members but not `default-members`, and the gate excludes them
(`--exclude 'probe_*'`), so they never slow a normal build.

**Probes are historical evidence, not maintained code** (ruling R43). Each was built and run
against the commit named below; later API changes in the workspace crates may stop it from
compiling. To re-run one, check out its commit first. The results live in
`docs/research/bevy020-api-reference.md`.

| Probe | Question | Built against | Result |
|---|---|---|---|
| `dynlink` (`probe_dynlink`) | Does `bevy/dynamic_linking` link with sim crates that depend only on `bevy_app`/`bevy_ecs`? | `a28e79b` | Works headless; `fl_game --features dynamic` builds; a windowed run is untested ("Probe: dynamic_linking"). |
| `remote` (`probe_remote`) | Custom `bevy_remote` methods, a round trip to a sim runner thread, an offscreen screenshot | `80bbd57` | Works; instant handlers run on the main thread, watching methods need a `+watch` suffix, late replies need request ids. |
