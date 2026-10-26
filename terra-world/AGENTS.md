# Terra World assistant guide

Read `README.md`, `docs/ARCHITECTURE.md`, `docs/INTEGRATION.md` and the relevant
crate guide. Preserve the separation between pure geographic/core contracts,
streaming/providers, sandbox state, authoritative networking and the optional
Bevy renderer. The default examples must continue to work offline.

- Keep coordinates, world/epoch identity, provenance, fallback warnings and
  attribution explicit. Dated/synthetic fixtures must not become claims of
  reconstructed historical accuracy.
- Server-owned actors and commands retain ownership/reach/movement validation;
  persistence changes must validate references and document migration.
- Keep network access opt-in. Never commit live provider credentials, private
  session capabilities, saves or caches. Follow `THIRD_PARTY_NOTICES.md`.
- Run commands from this directory. Start with headless tests:
  `cargo test --locked --workspace --exclude terra-demo --exclude terra-bevy`.
- Run `bash scripts/verify.sh` for the existing complete integration sequence,
  plus `cargo clippy --locked --workspace --all-targets -- -D warnings`.
  Root `scripts/dev.ps1 verify -Project terra-world` runs the common CI baseline.
- A graphical change needs a local `cargo run --locked -p terra-demo` playtest
  and screenshot evidence. Preserve rendering as an optional adapter feature.
  Update `docs/VALIDATION.md` only with results actually observed.
