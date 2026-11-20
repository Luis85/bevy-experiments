# World storage operations

The operations tools verify, back up, export and migrate existing worlds without
starting a renderer. All explicit maintenance outputs must be new paths. Source
identity, geographic level, catalog, private host metadata and complete dormant
state stay attached to the world; changing a filename does not migrate a source
or convert a server principal into a local character.

## Complete local recovery sets

For a reference local regional world, [Recovery bundles](RECOVERY_BUNDLES.md)
adds a checked database/catalog/configuration/prepared-source set and a restore
into a new directory. It preserves the last committed generation and validates
local host metadata. Use that workflow when the original source tree may be
unavailable. The lower-level database/archive operations below remain useful
for custom hosts; they do not independently collect provider dependencies.

[Retained native diagnostics](DIAGNOSTICS.md) can preserve bounded incident
context before a process exits. They are separate from recovery data and cannot
restore player state. Retention/offsite policy, deployment owners and measured
recovery targets remain operator decisions.

## Build and inspect

```sh
cargo build -p terra-bevy --no-default-features --bin terra-worldctl
target/debug/terra-worldctl inspect saves/frontier.db
target/debug/terra-worldctl verify saves/frontier.db
```

Both commands open an existing bounded database, validate its schema and SQLite
integrity, decode the committed pages, verify checksums, reconstruct the complete
world and validate catalog/reference invariants. They print safe counts and the
committed generation. They do not print inventories, money or resume capabilities.
They never intentionally create a missing source. Inspection does not change the
authority's restore cursor or checkpoint generation.

Game metadata is deliberately host-owned. A generic regional inspection verifies
the domain and reports `game_metadata_requires_host_validation: true`; the actual
local/server adapter checks its clock, player, source configuration and private
session references when opening or converting the world. A generic archive alone
is not permission to relabel those fields.

## Back up a running database

```sh
target/debug/terra-worldctl backup saves/frontier.db backups/frontier-001.db
```

This uses SQLite's online backup API and includes committed WAL state. A raw copy
of only the main `.db` file may omit committed transactions in its WAL. A backup
contains the latest committed checkpoint; ask the authority to save and wait for
the generation to advance if its latest gameplay changes must be included. A
consistent backup can be made while another process has the database open, but
world inspection and export remain bounded maintenance workloads.

The output must not exist. Database aliases, hard links, symlinks and SQLite
sidecars are rejected as output targets. New database, archive and explicit JSON
export files use owner-only permissions on Unix. These files contain private
world state and server resume capabilities and should be distributed only to the
world operator.

## Portable regional archives

```sh
target/debug/terra-worldctl export saves/frontier.db backups/frontier-001.region.json
target/debug/terra-worldctl import backups/frontier-001.region.json restored/frontier.db
target/debug/terra-worldctl verify restored/frontier.db
```

An archive contains the complete domain, pinned catalog, source identity, region
level, last committed generation and private host metadata. It includes dormant
depletion, removed-feature tombstones, wallets, housing, factions, transport jobs
and ownership. Export reads the last committed checkpoint and does not install
another active working set into a running host.

Import validates the complete domain before creating a temporary database,
checkpoints it, and publishes a new destination through SQLite backup. The
destination begins at generation one; `source_generation` is provenance rather
than a restored writer lease. Existing paths are never overwritten. No automatic
catalog, identity, schema or geographic-level transformation occurs. Portable
archives are bounded to 128 MiB; the lower 32 MiB document / 64 MiB checkpoint and
domain collection bounds still govern whether an import can be committed.

## Local version-one JSON saves

The local game continues to read complete v0.4/v0.5 JSON envelopes under matching
source and catalog rules. Import requires the exact `WorldConfig` JSON for that
world so the tool checks its source revision, seed, geographic level, terrain
resolution, live-data policy, historical epoch and political dataset identity.

Use the **resolved configuration** saved with that world, including its computed
`base_data_revision` and `political_dataset_revision`. For a world created in the
launcher, extract the profile's `config` object into a separate JSON file; the
whole profile is a wrapper, not a `WorldConfig`. Starter files such as
`configs/connected-heightmap.json` leave source identity resolution to game
startup and are not sufficient as conversion configurations by themselves.
Do not replace saved identity fields to make a different source pass validation.
The `backup` and `recover` commands preserve the recorded identity and do not
require a configuration argument.

```sh
target/debug/terra-worldctl import-local old-world.json saves/imported.db world-config.json
target/debug/terra-worldctl export-local saves/imported.db backups/local-world.json world-config.json
```

The tool uses `LocalWorldSave::read`, `validate_for`, and the same regional
metadata/identity code as the Bevy adapter. It checks the simulation clock,
selected character, politics and complete state before conversion. Import uses
the reference built-in catalog; custom games should call the same library
conversion with their pinned catalog. Existing reference catalogs are unchanged.
Exports are complete version-one JSON envelopes and preserve the 48 MiB local
save limit. The game still needs access to its configured provider assets or
historical pack; the gameplay save does not bundle terrain source files.

## Server version-one JSON saves

Use the actual server validator for server metadata and authoritative terrain:

```sh
cargo run -p terra-net --bin terra-server -- \
  --bind 127.0.0.1:0 --import-json old-server.json --region-store saves/imported-server.db

cargo run -p terra-net --bin terra-server -- \
  --bind 127.0.0.1:0 --region-store saves/imported-server.db --export-json backups/server.json
```

These commands perform maintenance and exit before seeding demo content or
running simulation. Supply the original `--seed`, `--lat`, `--lon`, `--level`,
`--resolution`, `--heightmap`, `--world-pack`, `--epoch` and `--politics` flags
when they differ from the defaults. Prepared worlds require the original
heightmap or collection manifest. Historical worlds require their matching pack and date;
missing coverage or a mismatched source fails without changing a destination.
The example uses a temporary loopback port because the host object owns a
listener even during maintenance; it does not admit clients before exiting.

Import checks player/session ownership, resume capability format and uniqueness,
accepted command sequences, starting inventory, clock, saved actors, catalog and
terrain coverage using `Server::load_world` and `validate_saved_world`. It then
publishes a new database with `import_region_store_new`. Export opens an existing
regional world and uses `export_world_new`, including the complete dormant state.
The existing 32 MiB server JSON bound remains. `--import-json` and `--export-json`
cannot be mixed with each other, `--load` or `--save`.

## File-publication outcomes

Terra 0.22 uses `terra_sandbox::publication` for private snapshots, host exports,
resume identities, backup/archive files, managed metadata and diagnostic reports.
Publication first validates and writes a same-directory private temporary, then
flushes its bytes, performs the requested replace or exclusive-create operation,
and checks the containing-directory synchronization on Unix. Missing save and
recovery parents are created privately and their new directory entries are
synchronized before payload publication. Relative filenames use the current
directory as that boundary.

| Outcome | What exists | Operator response |
|---|---|---|
| Success | Complete published output; required file and Unix namespace synchronization calls succeeded. | Continue the workflow, retaining normal backups and evidence. |
| Not published | Any prior destination remains; only this operation's owned staging is cleaned. | Resolve the reported cause before retrying. Existing no-clobber destinations remain protected. |
| Published, durability unconfirmed | The new complete bytes are visible and retained, although a later cleanup/synchronization failed. | Keep the output, inspect it with the appropriate verifier, and retain the prior recovery source. Do not assume rollback or silently overwrite it with a blind retry. |

Library callers can match `PublicationError::{NotPublished, Published}` and use
`published()`. `SandboxError`, `StoreError` and `NetError` preserve this typed
variant for their direct file publishers. Native UI errors include the stage;
a final managed JSON save with an uncertain result produces an error exit, and
an uncertain multiplayer identity publication prevents connected-state admission.
Its visible identity remains available for validated resume. Error messages do
not disclose bearer capabilities.

Recovery builders flush the bounded copied payload tree before its final marker.
Containing launcher workflows preserve a published backend if a later wrapper
record/profile cannot be confirmed, while distinguishing that retained backend
from a completed native backup or recovered world. An incomplete, unmarked
private builder can still be cleaned. See [recovery bundles](RECOVERY_BUNDLES.md).

This checks program behavior and reported operating-system results. The
[Linux fsync contract](https://man7.org/linux/man-pages/man2/fsync.2.html) requires
a separate directory operation for namespace persistence. Other platforms retain
checked file synchronization and namespace publication here; this code does not
assert a portable directory-fsync equivalent, tested Windows ACLs, or physical
power-loss survival. SQLite transactions retain their separate WAL/FULL policy.
Use the recorded target-platform validation when making a deployment guarantee.

## Pending work and recovery

Normal local frames and server ticks submit one background job and poll without
waiting on disk. The submitted authority revision stays visible and frozen until
the matching completion arrives. New local intents wait in the command queue. Host success responses remain buffered until their complete batch commits.
Fixed simulation does not catch up accumulated storage wait time. Explicit
startup, restore, backup/export and final exit are maintenance barriers.

For custom hosts, `RegionWorker::pending` is backpressure, not failure.
`RegionalPersistence::authority_paused` also covers activation and failed work;
all authoritative game-owned systems must respect it. A successful F5 request
means the committed generation advanced after completion. Queuing F5 alone is
not durable evidence. To exit, stop admitting mutations, finish the pending job,
commit the final current state, then close the database. The provided local and
server adapters follow this order and surface a failed final checkpoint.

A recoverable write failure retains visible state and the previous durable generation.
The local UI pauses with F5 retry/F9 restore choices; F9 discards later uncommitted
progress. A disconnected storage worker is terminal for that session: pending
backpressure clears, but polling, submissions and maintenance keep returning a
latched error until the world is reopened. The UI offers **Close failed world**
instead of retrying an unavailable worker. Live progress is unconfirmed: the last
submitted transaction may have committed before its acknowledgement was lost.
Reopen validates the database's committed state before new authority can proceed;
the adapter does not resubmit an uncertain transaction automatically. `requires_reopen()`
distinguishes this terminal state from an ordinary recoverable write error.
The reference server CLI stops on a storage error, while an embedding
host may fix a transient condition and continue polling/retrying the retained
batch. A stale writer must restart/restore from the newest generation. Repeated
retry does not repair a checksum, schema, source or catalog mismatch.

Preserve incident files unchanged. Restore a verified backup into a **new** path,
inspect it, then point the matching host configuration at that path. This tooling
does not overwrite a suspect database or perform arbitrary rollback in place.

## Current scale boundary

This is one bounded authority per database. SQLite generation checks prevent an
accidental stale writer from replacing newer state; they do not elect an owner,
provide distributed leases or transfer authority between shards. Disconnected
principals now freeze and can release owned-body/home pages. Authenticated
resume hydrates those dependencies before reactivation. Ordinary checkpoints
certify unchanged cold resource/source/tombstone groups and reuse equal encoded
pages. Current global ledgers, active state and actual cold owned or policy-bearing
records still receive complete merged-scope validation on the storage worker;
untrusted decode and explicit restore validate the complete bounded world.
Active-state copies remain a main-thread cost. Higher population caps, additional
certified dependency classes and distributed handoff require further structural
work and measurement; this increment does not raise those caps or claim production
MMORPG capacity.

## TLS transport and player capabilities

The reference server/client enable the optional `terra-net/tls` feature by
default. The native demo enables `terra-bevy/tls`; custom headless Bevy hosts can
enable it explicitly. TLS uses rustls 0.23.46 with its ring provider. A client
must provide an explicit trusted CA and the expected certificate hostname or IP
SAN. There is no accept-invalid-certificate or hostname bypass.

```sh
cargo run -p terra-net --bin terra-server -- \
  --bind 0.0.0.0:7777 --region-store saves/server.db \
  --tls-cert certificates/server-chain.pem --tls-key certificates/private-key.pem

cargo run -p terra-net --bin terra-client -- \
  --connect 192.0.2.10:7777 --tls-ca certificates/trusted-ca.pem \
  --tls-server-name world.example --session private/player-session.json

cargo run -p terra-demo -- \
  --connect 192.0.2.10:7777 --tls-ca certificates/trusted-ca.pem \
  --tls-server-name world.example
```

The address and `world.example` above are placeholders. Use the operator's actual
server address and a matching certificate. Each PEM input is bounded to 1 MiB,
certificate chains to 32 entries, and the private key must match the server
certificate. CA/key parsing runs at setup; native connection setup runs on its
connection worker. TLS record and handshake I/O then progress through the same
bounded nonblocking socket loop. Existing admission deadlines still apply.
Transport counters count encrypted wire bytes; replication counters describe the
application frames before TLS overhead.

The client buffers its Hello/resume capability until certificate validation and
Terra ALPN negotiation complete. TLS early data is disabled. A wrong CA, wrong
hostname, malformed TLS record or plaintext peer on a TLS listener fails before
player admission. `NetClient::is_encrypted()` and native `NetworkStatus.encrypted`
become true only after the TLS handshake completes.

Loopback plaintext remains available for local development. The reference CLI
requires `--allow-insecure-remote` before non-loopback plaintext binding or
connection; the native launcher exposes an explicit choice. The embedding
library leaves deployment policy to the game and offers `Server::bind_tls` and
`NetClient::connect_tls` in addition to its existing plaintext constructors.

TLS protects server identity and capabilities in transit. It does not replace an
account service, external identity provider, credential recovery, moderation,
authorization policy or a public production deployment review. Session files
remain private bearer credentials. The public fixture certificates/key under
`crates/terra-net/tests/fixtures/tls` are only test data and must never be used for
a real hosted world.


### Owner lifecycle during import and restart

A host cannot restore live sockets from disk. Loading/importing a complete host
save marks its saved principals dormant, stops stale input and preserves their
identity, money, cargo, needs, jobs and assignments. Therefore an immediate host
JSON re-export can differ in owner lifecycle flags from the input even though
all durable progress is preserved. This intentional normalization is distinct
from a schema/source/catalog migration. The next valid resume hydrates that
owner before accepting gameplay. Duplicate connected resumes, expired input
sequences and mismatched protocol handshakes cannot request storage hydration.

Company payroll and loaded commercial service freeze with their owner. Keep
fare escrow and cargo in recovery/export checks; their value must remain exact
across cold-page merge and restart. Current clients and hosts require protocol 11
and TLS `terra-world/11` ALPN for the private commercial state and movement
acknowledgements. An old peer is rejected before gameplay rather than accepting
an incompatible view.

## Exact-principal operator controls

The reference host exposes a trusted Rust administrative surface. It is
not a client command and creates no unauthenticated network admin endpoint.
The embedding game must authenticate and authorize the caller, resolve the exact
saved principal ID, and perform any real-person identity recovery before use.

| API | Result |
|---|---|
| `provision_principal()` | Atomically create one dormant principal with the configured starter policy; return a private capability and commit receipt |
| `provision_principal_for(address)` | Provision with an explicit client-facing address for a wildcard listener or mapped port |
| `principal_status(player_id)` | Existing principal ID, connected/dormant state and suspension flag; no capability or inventory |
| `set_principal_suspended(player_id, true)` | Commit suspension and dormant ownership, then disconnect active peers; future Hello rejects before regional hydration |
| `set_principal_suspended(player_id, false)` | Allow that principal to resume with its existing valid capability; ownership remains dormant until authenticated admission |
| `rotate_resume_capability(player_id)` | Replace the old bearer capability, commit and disconnect existing peers; return a new private `SessionIdentity` |
| `rotate_resume_capability_for(player_id, address)` | Explicit client-facing address when the listener is wildcard-bound or its public port is mapped |
| `operator_audit()` | The latest 128 ordered operator actions, with principal IDs, revision numbers and simulation timestamps; no bearer secrets |

Unknown IDs fail. The methods reject an already-busy regional worker. They stage
world/control changes, run domain validation and commit the metadata and pages
atomically before changing live policy or dropping peers. If SQLite is locked or
a write fails, the previous capability, connection, ownership and generation
remain valid. A successful receipt includes its committed generation when a
regional store is attached. A host without regional storage receives `None` and
must explicitly save its world for persistence.

These are synchronous maintenance operations, so call them at an administrative
barrier rather than inside a fixed simulation update. Capability rotation does
not remove a suspension. Suspension preserves the player's property, wallet,
vehicles, inventory, cargo and sequence history; it is access control, not deletion.
The audit tail is bounded diagnostic history rather than an external tamperproof
security log. It uses the simulation clock, not an independently trusted wall clock.

```rust,ignore
// `player_id` has been resolved by your authenticated host administration layer.
let receipt = server.set_principal_suspended(player_id, true)?;
assert!(receipt.committed_generation.is_some()); // this host uses regional storage

let (_receipt, recovery_identity) = server.rotate_resume_capability(player_id)?;
// SessionIdentity::save atomically writes a private file. It intentionally has
// no Debug implementation; never print the bearer capability into telemetry.
recovery_identity.save("private/recovery-session.json")?;
server.set_principal_suspended(player_id, false)?;
```

Deliver the replacement only through the game's authorized private recovery
channel. Wildcard listeners must use `rotate_resume_capability_for` with the
client-facing unicast address; unspecified addresses, multicast and port zero are
rejected before any capability changes. A published address/port can differ from
the bind endpoint, and is an explicit host administration choice. The example file is not account enrollment, password recovery or a
moderation service. The old capability cannot resume after a successful rotation.
Private server JSON and regional metadata preserve suspension and audit state
across restart; older envelopes default to no suspensions and an empty audit.

### Resume-only admission

Use `--resume-only` on the reference server, or set
`ServerConfig { allow_new_players: false, ..Default::default() }`, when a host
admits only already-provisioned saved principals. A missing capability receives
`registration_closed`; an invalid supplied capability remains a `resume` error
and never falls back to allocating another player. Suspended principals still
receive `access`. These checks precede regional hydration, identity allocation
and starter inventory grants.

```sh
cargo run -p terra-net --bin terra-server --locked -- \
  --region-store user_data/server.db --resume-only \
  --tls-cert private/server-chain.pem --tls-key private/server-key.pem
```

The default reference demo continues to allow new guests. Admission policy is a
**startup host setting**, retained in the launch configuration rather than the
world save; repeat the flag on deployment/restart. Loading a save does not
silently override the operator's current policy. The startup diagnostic states
whether new guests are accepted. Trusted provisioning is available through the
Rust maintenance API below; real-person enrollment, authorization and recovery
remain the embedding game's responsibility. Suspension of an exact principal
alone is not a ban on a person who can register another guest identity; the
resume-only policy closes that guest-registration route for private hosts.

### Provision a new principal without opening guests

A fresh resume-only host can call `provision_principal` at a maintenance barrier.
The operation validates aggregate principal/body capacity, safe resident spawn,
starter inventory, living crew and transport registration on a candidate world.
It allocates one unique bearer capability and records a secret-free `Provision`
audit entry. The new principal starts dormant with no socket or active network
actor. Guest admission remains closed throughout.

```rust,ignore
let mut server = Server::bind(ServerConfig {
    allow_new_players: false,
    ..Default::default()
})?;
// Register the host's source environment and gameplay catalog before provisioning.
server.open_region_store("private/world.sqlite")?;
let (receipt, invitation) = server.provision_principal()?;
assert!(receipt.committed_generation.is_some());
invitation.save("private/invited-player-session.json")?;
// Privately deliver this exact SessionIdentity through your authorized channel.
```

For wildcard/mapped listeners, use `provision_principal_for(client_address)`.
The address must be unicast with a nonzero port and must match the endpoint the
client will use. The returned artifact is an address-pinned private
`SessionIdentity`, not an account, email invitation, password or a public
registration endpoint. Never print or add its capability to audit records.
`SessionIdentity::save` supplies the existing atomic private-file policy; handle
file-write/delivery failure by retaining the committed principal and retrying
the authorized handoff, rather than provisioning an extra principal.

With regional storage, principal, dormant ownership, capability and operator
audit commit together before the live world changes. A locked/failed writer
leaves the prior next ID, capability map, owned state, residency and generation
intact. With an in-memory host the receipt has no committed generation; call
`save_world` successfully before delivering the artifact. Provisioned dormant
principals still consume the ordinary saved-player and living-body caps. Closing
guest registration does not add an external identity provider or remove these
finite limits.

The [0.8 admission process smoke](server-admission-v0.8.json) verifies the actual
server executable's help, policy diagnostic, ephemeral loopback listener,
three-tick clean shutdown and committed regional database. The five operator
regressions in the [final headless gate](headless-verification-v0.8.json) separately
verify authenticated readmission, unknown/invalid/suspended rejection before
hydration, exact private-state continuity, recovery address checks and write
failure without changing a live capability or connection. These are bounded
functional checks, separate from the [real-session workload](PERFORMANCE.md#08-actual-regional-session-churn).

## Atomic verified backups and recovery at regional scale

Backups now copy into a private temporary directory and verify **that copied
snapshot** before publishing the requested output name. A process interruption
during copying can leave a private temporary artifact, but cannot leave a
partially written database under the final backup name. Publication is atomic
and refuses an existing destination or its SQLite sidecars. Copying includes
committed WAL contents and the resulting file is consolidated, synced and
self-contained before publication.

The copy loop has a 30-second default deadline, using batches of 128 SQLite
pages. A continuously busy or changing source can no longer keep an operator
waiting indefinitely. `RegionBackupOptions` lets an embedding operator choose
1–4,096 pages per step and a positive copy deadline up to 300 seconds. The
bounded copy phase is followed by complete domain validation and disk sync;
the deadline is not a promise that filesystem calls or validation can be
preempted. On a copy timeout, release locks or pause writes and retry. No final
file is published by a failed copy or failed validation.

`backup_with_options` returns a receipt naming the generation **inside the
verified snapshot**, its payload bytes, region count, copied pages, copy steps
and elapsed time. A concurrent source checkpoint therefore cannot make the
success message name an older pre-copy inspection. No player inventories,
capabilities, metadata values or paths appear in that receipt. The existing
`backup_to` API uses these defaults and retains its `Result<()>` contract.

A direct recovery operation verifies a chosen backup and publishes a new world
path:

```sh
target/debug/terra-worldctl inspect backups/frontier-001.db
target/debug/terra-worldctl recover backups/frontier-001.db restored/frontier-recovered.db
target/debug/terra-worldctl verify restored/frontier-recovered.db
```

Then open the new path with the exact matching host/source configuration. The
operation preserves generation, catalog, identity, private metadata and dormant
records. It does not repair or overwrite a suspect database, fabricate missing
pages, or bypass host-specific clock/session/source validation. Keep the incident
files unchanged and select a known valid backup; a corrupt backup is rejected
without creating the recovery destination.

Maintenance `inspect`, `verify`, `backup`, `recover`, and exports now use
`RegionStore::open_read_only`. They do not change the source database's journal
mode or rewrite its content. The read-only handle is WAL-aware and enforces
SQLite query-only mode; SQLite may still access its normal shared-memory sidecar
when reading a live WAL database. `open_existing` remains the writable
embedding API for existing authorities.

Inspection also reports the 64 MiB checkpoint payload limit, remaining payload
bytes and the 4,096-region limit beside observed counts. These describe the
existing bounded storage contract. They do not increase saved-world limits or
claim distributed authority.

## Exercise recovery with real sessions before deployment

The [completed 0.12 workload](verification/v0.12-final/regional-sessions.json)
joins backup recovery with **16 actual local TLS clients**, **182 living bodies
(80 owned)**, prepared terrain and **1,024 dormant overlay regions**. Run the
exact workload in [Scaling](SCALING.md#012-combined-prepared-terrain-and-cold-world-acceptance),
then retain its JSON report with the executable and source hashes. Its private saves,
capabilities and inspection exports remain inside a temporary fixture directory;
the public report contains counts, timings and exact-continuation results.

The final run completed **4,840 fixed steps in 131.962 wall seconds**, with 25
resident source chunks and 5,248 resource records, including 4,096 cold depleted
records. It passed 64 offline-owner, 64 resumed-owner, 128 restarted-owner and
16 authenticated recovered-owner comparisons. All 128 durable craft commands
retained exact inventory and sequence state; their acknowledgement p95 was
**108.879 ms**. The 6,464 local TLS ping samples had a **58.887 ms p95**. These are
finite local observations, including fixture scheduling and storage barriers.

The report distinguishes a verified database copy from a usable recovered host.
It reopened the copied world through the host's own validator, compared full
state and private metadata, and authenticated every prior owner with the same
durable command sequence before any simulation step. Backup/recovery preserved
**generation 165** across **1,051 nonempty regions** and **1,686,295 checkpoint
payload bytes**; subsequent authenticated continuation reached **generation 169**.
Source bytes and all cold records remained unchanged.

The real competing-writer failure left authority, tick and generation unchanged;
retry preserved the logical world. Removing/restoring a source tile respectively
blocked/resumed movement. The host counts a failed storage completion in its
domain-error total. Only the exact injected SQLite lock failure, with one error
and no input, simulation or snapshot work, is accepted; additional or unrelated
errors always fail. The separate `session-probe-checks` release gate also passed
the regression for that distinction.

| Current recovery observation | Scope | Elapsed time |
| --- | --- | ---: |
| Verified recovery copy in the session workload | Generation 165; 1,051 regions; 579 pages in five copy steps | 42.378 ms |
| Storage-only backup | Generation 2; 1,027 regions; 1,708,853 payload bytes | 51.606 ms |
| Storage-only recovery | Same generation-two snapshot | 48.574 ms |
| Storage-only working-set restore | 1,023 dormant regions retained | 28.441 ms |

The [storage-only report](verification/v0.12-final/recovery.json) has 47 living
bodies and 6,144 resources, but zero connected clients, no renderer and no terrain
import. Its exact full-state, private-metadata, generation, dormant-record and
source-byte checks passed; corrupt input rejected without publishing an output.
Its generic receipt still requires host-specific metadata validation. The
combined session workload supplies that separate recovered-host check.

The [final gate report](verification/v0.12-final/gates.json) passed all 20 gates
with unchanged input/log/report hashes. Earlier build-cache exhaustion and
inconsistent synthetic-payroll fixtures are retained as separate incomplete
attempts in [0.12 validation](VALIDATION_v0.12.md#earlier-incomplete-attempts). Their partial
results are excluded, and production accounting validation was not weakened.

The workload's TLS key and principals are synthetic test fixtures. A production
rehearsal must use the deployment's own certificate, account, source and backup
policies on its declared hardware. No local timing figure here establishes an
Internet latency or availability guarantee.

### Poisoned store locks (0.18)

A poisoned regional-store mutex marks an uncertain mutation boundary. The
worker preserves its last acknowledged status, rejects further work and requires
a validated reopen. Read-only export/backup adapters expose the same unavailable
state. Never treat a default/unknown status or a cleared pending flag as an
acknowledgement of rollback or durability. Ordinary completed transaction errors
retain the established retry behavior. This does not interrupt a wedged OS write
or establish a storage latency bound.
