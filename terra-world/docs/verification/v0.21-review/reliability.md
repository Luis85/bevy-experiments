# Terra World 0.21 reliability review and implementation evidence

Reviewed 29 October 2026 against the supplied 0.20 source. The initial review
was read-only and preceded implementation dispatch. Scope: persistence, native
save/recovery integration, diagnostics, operational failure boundaries and QA.
Product, current review/plan/validation, regional persistence, recovery bundles,
diagnostics, operations and compatibility documentation were read with the
corresponding store/worker/native implementation.

## Confirmed findings

1. **P2 / WP07 — native full-state activation bypass.**
   `RegionalPersistence::hydrate` returned success for any complete live state,
   bypassing the committed-store validation added to `RegionStore::hydrate` in
   0.20. Native commands/source registration could mutate before a later
   checkpoint detected same-generation payload/schema corruption.
2. **P2 / WP08 — JSON replacement permissions.** `sandbox::atomic_write` created
   its temporary with default Unix permissions, allowing replacement of a 0600
   snapshot with a 0644 file under umask 0022. Complete local saves and the
   advertised export API contain private world state.
3. **P2 / R3 — first-session checkpoint witness.** Managed profiles initially
   record `checkpoint_present=false`; ordinary checkpoint success does not
   persist that flag. An abrupt first-session exit followed by missing progress
   can therefore admit a fresh world. A fix needs a failure protocol, not a
   nonempty-file inference or a post-commit marker with another crash window.
4. **P2 / R4 — publication durability result.** Several JSON/profile/diagnostic
   publishers ignore parent-directory synchronization errors. Propagating a
   post-rename error requires an explicit uncertain-publication result and must
   not promise rollback. The broad correction remains separate follow-up work.

## Implemented boundaries

WP07 submits the existing bounded asynchronous hydrate operation for each new
activation, whether the live view is complete or partial. Only an already
admitted `activation_open` batch takes the fast path, and it must still carry a
complete state. Pending work retains its single request; a matching completion
opens the batch; source replay/commands consume it; checkpoint closes it.
Explicit restore already validates and opens its batch. Ordinary frames do not
perform synchronous storage reads for this admission.

WP08 creates snapshot publication temporaries with Unix mode 0600 before any
bytes are written. Exclusive creation, same-directory replacement, existing
file synchronization and cleanup remain. This policy does not qualify Windows
ACL behavior or solve the separate directory-sync result issue.

New regression tests exercise complete-state live-edit preservation, one request
per batch, generation-zero and dormant-state hydration, explicit restore,
same-generation payload/schema corruption before queued commands, sixteen-intent
queue progress and actual native source replay. The replay test injects newly
completed source content without an eviction checkpoint that could mask the
activation defect. Snapshot tests run the real write/read implementation under
umask 000 in an isolated subprocess and exercise failed rename cleanup.

## R3 approved protocol and implementation

The integrator approved the failure protocol before native edits. An immutable
private `world-started.json` is published exclusively and synchronized before
the managed runtime can write progress. It states that initialization was
admitted, never that saving succeeded. Only its first successful publisher may
initialize a missing checkpoint; subsequent processes must restore a valid
committed snapshot. The witness binds the profile path, progress path/mode and
world/source identity. Renaming and ordinary presentation/clock settings do not
change that identity. Its publication does not change `checkpoint_present` or
the profile's played dates.

The launch preflight validates existing legacy progress before adding a witness.
Regional startup repeats full read-only archive/local-host/catalog admission
before its normal writable open, so missing, corrupt, empty or generation-zero
input is not initialized as a side effect. JSON startup separately rejects a
required snapshot that disappears after preflight. No flow/UI hook is needed.
Interrupted initialization without a valid checkpoint fails closed with
verified-backup/create-separate-world guidance; it is not automatically reset.

Witness publication uses a private complete temporary, exclusive hard-link
publication and explicit Unix parent synchronization. Before publication,
failure preserves the unstarted profile and destination. An error after
publication retains the witness and declines startup, without claiming rollback.
The general directory-sync issue in older publishers remains open.

Added tests cover abrupt child-process termination after the first JSON save
and a real committed SQLite WAL transaction, exact legacy/profile continuity,
missing/corrupt/generation-zero input, rename/settings compatibility and
source/path rejection, concurrent first starts, failed exclusive publication,
injected post-publication sync failure, and both native startup barriers.

Independent cross-review also found that output reservation needed to include
the new witness. CLI preflight now protects the managed profile and witness,
including recursively created output parents and diagnostic lock paths. The
same authoritative witness-path helper feeds runtime screenshot/political-export
checks and every diagnostic worker publication. New tests exercise absent
metadata paths, screenshot-directory collisions, hard-link and symbolic-link
aliases introduced after preflight, asynchronous refusal before directory/file
creation and report/lock aliases introduced between diagnostic publications.
Existing metadata and earlier reports are preserved on rejection.

This cannot retroactively identify a legacy `checkpoint_present=false` world
whose progress was already lost before it ever gained a witness: its metadata
is indistinguishable from an unstarted legacy world. Valid legacy progress is
protected on its next admitted launch. Keep the witness with a manually retained
managed profile. Recovered managed copies already declare checkpoint presence
and obtain their own new bound witness. These process-interruption checks do not
qualify physical power loss, arbitrary filesystem replacement, Windows ACLs or
every filesystem's hard-link/durability behavior.

## Independent technical interface assessment B

Assessment B was held until the separate player/design Assessment A completed.
The required fallback command was attempted once from the source root:

```text
.codex/skills/impeccable/scripts/impeccable detect --json crates/terra-demo/src
```

It exited 127 because the script is absent. There is no detector score or
coverage. Source inspection verified launcher word-or-character wrapping,
scroll-contained errors/status, tab-indexed controls, controller field labels,
cancel-first serial-bound restore review, inspector focus borders and compact
navigation. Generic in-game text lacked the launcher's explicit long-string
wrapping. Sibling visible field text and a controller label do not establish
native assistive naming. These source observations were supplied to the player
owner after the independent assessment completed.

No native/browser pixels, rendered contrast, actual keyboard traversal,
assistive-device behavior or physical input qualification were observed by this
reviewer. Historical captures and archived tests were not counted as fresh.

## Execution status

Owned Rust files were formatted with the pinned toolchain's rustfmt, and diff
whitespace checks passed. Fresh compilation/tests are coordinated by the
integrator against the shared target. The initial WP07 run passed its five
adapter tests (real worker, command and source-replay paths). Both new private
snapshot publication tests passed. An overly broad `snapshot` filter also
selected a separate loopback-network test whose bind was denied by the default
sandbox; that command was not a fully passing run and the integrator is rerunning
under the applicable loopback execution permission. No production admission was
weakened for that environmental failure.

R3 added twelve further test identities: five model protocol tests, two startup
barrier tests, three managed-output preflight/runtime tests and two diagnostic
worker tests. Their execution and the final shared regression outcomes are
pending. New source tests alone do not establish a passing result. The final
canonical acceptance receipt remains the authoritative full-run evidence.
