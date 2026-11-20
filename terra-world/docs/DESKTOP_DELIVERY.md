# Desktop builds, packages and qualification

The complete project ZIP is source, documentation, original content and retained
evidence. It is not a Windows executable distribution. The project provides a
repeatable desktop packager and Linux/Windows CI jobs. The [validation record](VALIDATION.md)
states which jobs and native journeys actually ran; adding the Windows job does
not establish Windows acceptance. macOS packaging remains outside this pass.

## Build and verify from source

Use Python 3.11 or later and the pinned Rust toolchain from
`rust-toolchain.toml`. Linux build dependencies are listed in the root README.
For the Windows x86-64 MSVC target, install Rust and Visual Studio C++ build
tools with the Windows SDK. The Rust project's
[MSVC target documentation](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html)
describes supported host tooling; this is a dependency reference, not Terra
device qualification.

From the extracted project root:

```sh
python scripts/verify.py --jobs 3 --report-dir verification-desktop
python scripts/package_desktop.py --gates verification-desktop/completed-gates.json --output terra-desktop.zip
python scripts/verify_desktop_archive.py --archive terra-desktop.zip --smoke-help --report verification-desktop/desktop-extracted.json
```

Use `python3` where that is the Python 3 executable. Use a new report directory
and output ZIP each time. The runner executes the shared Rust, source-content,
asset, recovery, loopback TLS and native-build gates. It does not open a window.
CI runs the same commands on `ubuntu-latest` and `windows-latest`, retaining
each platform's evidence even on failure. A desktop artifact is uploaded only
after its own platform's acceptance, packaging, fresh extraction and bounded
startup succeed. The final step's report is retained in verification evidence,
including a failure; CI configuration alone does not show that a job ran.

The packager checks the complete ordered canonical gate roster, strict integer
success/test counters, positive execution counts for every Rust-test gate, and
hashed unique Rust test identities reconciled against per-crate and total counts.
It also checks source-input hashes, captured logs, actual compiler version,
executable digest and executable architecture. It rejects changed
sources, another platform's record and an existing output. It copies the exact
accepted binary and the original runtime-data paths used by presets, preserves
licenses, and verifies every ZIP entry before publishing. It never includes
saved worlds or resume identities. The package manifest records every payload
hash and its build profile. A curated payload allowlist prevents an unrelated
JSON or identity file placed near fixtures from entering a package automatically.
Accepted logs and report artifacts accompany the gate record for inspection.
These checks establish local structural consistency and byte integrity; they do
not authenticate a publisher or prove that a coordinated fabricated receipt came
from a trustworthy machine. Implementation input hashes exclude Markdown and
licenses; the final package manifest records those delivered bytes separately.
The standard gate currently builds the optimized
**dev profile** (`opt-level = 1`); this package is a reproducible review build.
Release-profile performance, signing, an installer and auto-update remain
separate delivery work.

The current player-guide set is included so routes, controls, transport,
communities and living-world help remain connected. Developer source links,
historical screenshots and earlier evidence archives refer to the companion
complete source project. The packager currently accepts x86-64 Linux ELF and
x86-64 Windows PE executables; other architectures require their own packaging
and execution evidence.

## Start an extracted desktop package

### Inspect before launching

The current package includes the standalone verifier introduced in 0.22 that needs only Python 3.11 or later; it
does not need Cargo or the source project. From a clean extracted package run:

```sh
python scripts/verify_desktop.py
python scripts/verify_desktop.py --smoke-help --timeout-seconds 20
```

Use `python3` on systems where that is the Python 3 command. The first command is
read-only and does not execute the game. It checks the exact bounded inventory,
portable names, symbolic/reparse/hard links, file hashes, executable architecture
and the terminal acceptance-record association. The second explicitly runs the
accepted executable's `--help` on a matching x86-64 operating system with a
finite deadline. A help success does not test a window, controller, saved world
or GPU. Both commands print a JSON result and return a nonzero status on failure.

Startup output is captured through a pipe with a 64 KiB limit, plus one byte to
detect overflow; it is never spooled to an unlimited temporary file. Overflow,
nonzero exit, unreadable output or a deadline produces failure. The deadline
includes output closure, so a child that exits while a descendant keeps the pipe
open cannot turn into an indefinitely waiting reader. Cleanup is bounded
separately: up to 10 seconds for Windows `taskkill`, two seconds to observe the
leader's exit and one second for output closure. POSIX cleanup targets the owned
process group; descendants that deliberately detach are outside that group.
Windows descendant cleanup after the leader has already exited can remain
unconfirmed, and the failure says so. This is a startup diagnostic, not process
isolation for an untrusted executable.

On POSIX, successful leader exit and closed output are followed by a check that
the owned process group no longer exists. A redirected background descendant
therefore causes failure and a group-termination request; the request itself is
never recorded as verified exit. The successful report names
`descendant_check.status=owned_process_group_absent`. The current Windows path
cannot enumerate redirected descendants reliably after the leader exits; its
successful help report instead names `descendant_check.status=unconfirmed` and
establishes only the leader's exit and output closure. Target-platform process
and native-window qualification remain separate.

Keep the folder unchanged during checking. Redirect any report **outside** the
package so it is not an unlisted file. The tool expects a clean extraction;
local caches, added mods or other files can make an otherwise intentional working
folder differ. Inspect a fresh extraction to establish the shipped baseline;
do not delete saved worlds or unknown files merely to make a check pass.

These are consistency checks, not a signature or publisher-authentication
scheme. The verifier cannot independently establish that recorded tests were
performed by a trusted machine. Keep its exact report with the named archive
when diagnosing missing/corrupt payloads versus runtime/driver startup failures.

### Start the game

Use [Playtesting](PLAYTESTING.md) for the current first-session route, optional
local diagnostic reports, save/reopen checks and issue template. It identifies
the pending Windows and physical-device qualification separately from the
checks completed on the supplied review build.

Extract the entire archive to a writable folder. Keep `configs` and `crates`
beside the executable: these contain the prepared terrain, historical presets,
political geometry and original content that the menu resolves locally.

- **Windows:** run `Start-Terra.cmd`. It opens the native menu and uses
  `%LOCALAPPDATA%\TerraWorld` for worlds, settings and private multiplayer
  identities. If Windows reports a missing Microsoft runtime DLL, use the
  [official Visual C++ redistributable guidance](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/)
  for the x64 build. Do not obtain individual DLLs from unrelated download sites.
- **Linux:** run `./start-terra.sh` from a graphical X11 session with a compatible
  graphics driver. If an extractor discarded the executable flag, run
  `chmod +x start-terra.sh terra-demo`. User data goes under
  `${XDG_DATA_HOME:-$HOME/.local/share}/terra-world`.

The package does not bundle system graphics, input or C runtime libraries. On
Linux, inspect the accepted executable with `ldd ./terra-demo` and resolve any
missing libraries through the distribution's normal packages; portability across
distributions and older system libraries needs its own execution evidence. The
release check includes an extracted-folder command-line smoke test on the
recorded host. Windowed create/play/save/reopen acceptance is recorded separately
and can remain unavailable in a socket-restricted executor. Neither check makes
a universal Linux binary compatibility promise.

The launcher scripts set the working directory to the extracted package. For
direct command-line play, run the executable from that directory and choose
`--user-data`, `--region-store` or `--save` explicitly. Default direct-play
paths and caches otherwise belong to the working directory. For example:

```sh
./terra-demo --play --region-store /absolute/worlds/frontier.db --quality performance
```

On Windows use `terra-demo.exe` and a Windows absolute path. `--menu` options
and direct-play options are different contexts; the generated launcher script
always opens the menu. Start without custom content to establish the bundled
baseline before adding a game-specific asset or source pack.

## Upgrade and recovery

Keep the previous executable, source/data configuration and a verified backup
before upgrading a world. Menu-created profiles retain absolute source paths;
moving their original extracted source folder can make a prepared world
unavailable. For supported local prepared or procedural worlds, create a
[self-contained recovery bundle](RECOVERY_BUNDLES.md) before removing those
dependencies. It retains the database, resolved configuration, terrain and
optional political input together, and restores to a new location with rebased
paths. Historical/live worlds and custom catalogs need their own dependency
policy. Copying files into a new package does not authorize changing a saved
world's source identity.

**Your worlds → Make backup** performs this supported local operation
without a separate operator executable. **Backups and recovery → Verify** opens
a named review; **Recover as new world** creates a separately selectable managed
world. Keep the complete backup wrapper, including its `world-backup.json` and
`bundle` directory, when copying it elsewhere. Removing the only local disk is
not protected by another folder on that disk. Choose retention and offsite
copies for the deployment; the launcher does not provide those services.

In 0.16, edit **Recovered world name** in the verified review before starting
the restore. The progress message names that chosen copy; opening the completed
world remains a separate action. **Your worlds → Rename** changes an existing
managed world's displayed title while retaining its progress, source paths,
character and dates. Creation timestamps help distinguish copies, but names and
timestamps are not world identity: each recovered copy has its own directory.

Protocol 11 clients require protocol 11 hosts. Domain save and regional schema
remain 1; consult [Compatibility](COMPATIBILITY.md). The desktop package does
not include server/operator or asset-authoring executables; build those from the
complete source project when hosting, producing packs or maintaining worlds.
[Operations](OPERATIONS.md) covers verified regional
backup and recovery to a new destination. Private capability files should stay
with the account, not in a distributable or a bug attachment.

### Managed starts and interrupted first sessions

Starting a managed world in 0.21 publishes a private `world-started.json` beside
its `world.json` before the runtime can write progress. Keep this record with
the profile. It means initialization was admitted; it does **not** certify a
successful checkpoint. The profile's `checkpoint_present` setting can still be
false after an abrupt first-session exit, so it is not sufficient evidence that
an empty world is safe to initialize.

A genuinely unstarted world receives one exclusive fresh-world admission.
Subsequent starts must restore a valid checkpoint. If the first launch crashes
before its first successful save, or required progress later becomes missing,
empty or corrupt, reopening stops with recovery guidance. A regional database
with no committed generation also stops. This conservative behavior prevents a
later start from silently replacing lost progress with a fresh world. Keep the
original directory and record; recover a verified backup through **Backups and
recovery**, or use **New world** to create a separate world. Do not delete the
witness or clear checkpoint metadata to force the old profile to start again.

Valid legacy progress gains a witness after read-only validation on its next
admitted launch. No new record can establish whether an older
`checkpoint_present=false` profile already lost its progress before this
upgrade; without a witness those bytes are indistinguishable from an unstarted
world. Older launchers ignore this protection despite matching save/schema
numbers. Keep the executable version with your evidence and use a current
launcher for the managed-start guarantee.

For a raw archival copy, close the world's processes and retain its **complete
managed folder**: `world.json`, `world-started.json`, `world-session.lock`, progress and any SQLite
sidecars. Include separately located progress and source dependencies too.
Copying an active database's main file alone is not a complete checkpoint
backup. The witness binds the original profile/progress paths, so a copied folder
cannot be treated as a relocated playable world. The supported recovery flow
rebases a verified bundle into a new location and gives the recovered profile
its own witness on its first admitted launch. Keep the complete
`world-backup.json` plus `bundle/` wrapper for that workflow.

### Private saves and protected export paths

Shared publication creates same-directory temporaries as owner-only Unix files;
first writes and replacements retain that restriction even with a permissive
umask. It checks file synchronization, new parent-directory entries and, on
Unix, the publication directory. A failure before publication preserves the old
destination. A failure after publication retains the new visible file and
reports that durability was not confirmed. The application must not acknowledge
a successful save or promise rollback in that case. Recovery retains completed
inner outputs even if a later wrapper cannot be acknowledged. See
[Compatibility](COMPATIBILITY.md) and [Recovery bundles](RECOVERY_BUNDLES.md)
for the API and final-marker boundaries. These checks do not establish Windows
ACL behavior or qualify physical power loss.

A managed session holds a nonblocking OS lease on the stable
`world-session.lock` file before initialization and through final metadata
publication. A second participating process fails without changing progress.
Closing or crashing the holder releases the OS lock; the marker file remains.
Do not delete or replace it to force another writer. Older executables, direct
SDK writers and arbitrary external filesystem replacement do not participate
in this cooperative protocol.

From 0.23, **Rename** and **Make backup** acquire the same lease for their complete
metadata operation. Finish the other session, or wait for its maintenance to
finish, then refresh **Your worlds** and retry. Busy backup attempts create no
destination. Native backup holds ownership while checking the selected profile,
copying the committed regional world and publishing its final wrapper; the
operator's explicit online SQLite backup API retains its existing behavior.
Metadata maintenance can create the stable coordination marker on an older or
unstarted world, but it never creates a launch witness, initializes gameplay,
sets a checkpoint flag or changes its last-played time. JSON worlds support the
same coordinated rename; native backup remains regional-only.

Managed profile, witness and lease names are reserved output boundaries even before
they exist. Screenshot, political-export and diagnostic report/lock destinations
are checked before output directories are created and again by their runtime
writers. A destination such as `world-started.json/capture.png`, or an existing
hard-link/symbolic-link alias to the metadata, is rejected. Choose a separate
output path; retain profile, witness and lease files as part of the world's recovery
evidence. These checks do not defend against arbitrary filesystem replacement
racing an operation.

## Optional original model packs

### Original animated Comet example

The original wildlife pack remains the default. To review the optional Comet
compact-car model together with all five original wildlife models, launch from
the extracted package directory:

```sh
./terra-demo --play --region-store /absolute/worlds/comet-review.db --asset-pack crates/terra-bevy/assets/transport-demo.pack.json
```

Choose a new absolute regional-store path for that example. To use an existing
menu-created world, pass `--world-profile /absolute/path/world.json` and the same
`--asset-pack` argument directly to the executable. The menu itself does not
accept `--asset-pack`. Rent or buy **Comet** through Garage; other models keep
their existing presentation. Wheels follow admitted forward/reverse travel and
steering, and pause freezes presentation. The cabin has empty seats and a fixed
livery. Garage explains the fixed livery instead of charging for a native paint
change it cannot show. First-person retains the procedural cockpit. The example
has no driver/boarding animation or audio and requires the 0.15 reader; see
[Asset packs](ASSET_PACKS.md) and [Authoring](ASSET_AUTHORING.md).

### Fieldhand with a Comet cabin variant

The optional 0.16 composed pack contains the original **Fieldhand** human, a
separate **Comet cabin variant** and the five unchanged wildlife models. The
variant makes room for the seated human; the earlier Comet model and manifests
remain intact. Fieldhand retains the existing body, skin, hair and outfit
choices, with idle, walking, running and seated clips. The woodland-only pack
remains the default.

For a separate review world, run from the extracted package directory with a
new absolute regional-store path:

```sh
./terra-demo --play --region-store /absolute/worlds/fieldhand-review.db --asset-pack crates/terra-bevy/assets/character-demo.pack.json
```

To see the character choices already saved through native creation, select the
managed profile directly instead:

```sh
./terra-demo --world-profile /absolute/path/world.json --asset-pack crates/terra-bevy/assets/character-demo.pack.json
```

[Game flow](GAME_FLOW.md) explains profile locations. On Windows use
`terra-demo.exe` and Windows absolute paths. The menu and its launcher scripts
do not accept `--asset-pack`; these are direct executable commands. The separate
`crates/terra-bevy/assets/character-pack/pack.json` is the human-only example;
use the composed pack above to review the human and declared vehicle seat
together. Its GLBs and manifests are included in the desktop package; no
authoring executable or regeneration is needed to run it.

Rent or buy **Comet** through Garage, then use Driving to enter it manually.
For a known manually controlling owner, the seated visual follows the vehicle;
the saved authoritative avatar, ownership and ordinary Enter/Exit rules retain
their meanings. First-person hides the rider and exterior and keeps the
procedural cockpit. Releasing manual control to RTS/autopilot returns the
standing avatar under the existing rules. Other players' public appearance
does not establish vehicle occupancy. This is a visual seat handoff, with no
physical/timed boarding, remote passenger system or audio.

The optional human policy and driver-seat declaration require a **0.16 reader**
despite retaining asset schema 1. Packs omitting those fields keep their older
normalized identities. Procedural presentation remains available during loading
or failure. Pause freezes ongoing animation; a change in actual occupancy still
selects the standing or seated posture directly. The
[character example](../crates/terra-bevy/assets/character-pack/README.md) and
[human authoring handoff](ASSET_AUTHORING.md#preview-the-original-customized-human-and-driver-handoff)
describe the five-output generator, seven-model recipe and remaining production
limits. Target hardware and clean-machine Windows acceptance remain separate.

## Troubleshooting

| Symptom | Next useful action |
|---|---|
| A preset cannot find its manifest or political file | Extract the complete package and use its launcher so relative paths have the correct root. For an existing profile, retain its original absolute source directory. |
| Renderer initialization fails | Record the OS, GPU and driver; try `--quality performance --no-atmosphere` from direct play. A software-renderer result is not physical-GPU qualification. |
| An imported pack fails validation | Keep the error and pack identity; repair the authoring input or use `--procedural-models` for a direct-play fallback. |
| A host rejects admission | Check the exact executable/wire contract, source identity, TLS CA/name and private session file. Do not silently create a new principal to hide a failed resume. |
| A regional save reports blocked storage | Retain the database and error. For a recoverable failure, follow the displayed retry/restore instructions; Restore discards progress after the last successful checkpoint. If a local session says the storage worker stopped and requires reopening, use **Close failed world**, then reopen the same world to validate its committed generation. See [Playtesting recovery](PLAYTESTING.md#when-a-session-goes-wrong). |
| Session ended without a final diagnostic export | Add `--telemetry-interval-seconds N` with `--telemetry FILE` for retained periodic captures. Check the last timestamp and running/exit status; stale or missing data is unknown health. See [Diagnostics](DIAGNOSTICS.md). |

## Remaining platform acceptance

Qualify Windows on actual supported hardware before claiming a ready-to-play
Windows release: clean-machine startup, native menu and character creation,
each bundled source preset, 1100×700 and enlarged text, mouse/keyboard and
physical controller/wheel paths, save/close/reopen, TLS join/resume, renderer
fallback and runtime dependency handling. Record the exact package/executable
hash, OS, GPU/driver, devices and outcome. A CI build and a simulated input test
answer different questions from those journeys.

The broader [baseline gaps report](OPEN_GAPS_REPORT_v0.12.md) and
[current follow-up](REVIEW_GAPS_v0.24.md) keep platform delivery, physical-device
acceptance and deployment capacity separate.

## Terminal receipt publication

The runner publishes completed-gates.json exclusively, flushes it and verifies
its exact bytes before returning success. gates.json is a progress/compatibility
alias. A completion_record-bearing receipt can only package with the matching
safe terminal sidecar. This does not prevent later external modification or
authenticate a publisher; ordinary recorded hashes remain necessary.

## Bounded verification commands (0.24)

Each acceptance command has a 3,600-second default deadline. Use
`--gate-timeout-seconds N` for a positive finite limit suited to the build machine.
Metadata/compiler probes use the smaller of this value and 120 seconds. Startup,
leader execution and output EOF participate in the observed execution lifetime.

Gate output is streamed into a bounded buffer: 64 MiB by default. The explicit
`--gate-output-limit-bytes N` accepts 1 through 512 MiB in bytes. One extra byte
detects overflow; the retained log contains at most the chosen raw allowance plus
a 2,048-byte diagnostic trailer. Overflow, output read failure, nonzero status,
deadline or an unconfirmed owned POSIX group fails the command. Successful output
is retained exactly for Rust test identity parsing.

Cleanup uses finite waits: Windows taskkill up to 10 seconds, leader exit up to 2
seconds, and reader completion up to 1 second. A termination request is recorded
separately from observed completion. Deliberately detached descendants remain
outside POSIX group ownership. Windows success establishes leader exit and output
closure, with redirected-descendant inspection explicitly unconfirmed.

Current 0.24 receipts require complete execution metadata for every gate and both
ordered startup probes. Admission validates platform, limits, observed/retained
byte counts against actual logs, successful output completion, exit and applicable
descendant proof. Omitting these fields or setting exit 0 alongside failed or
partial evidence cannot authorize a 0.24 package. Older numeric receipt versions
retain an explicitly version-scoped compatibility path.

Failed logs and terminal evidence remain in their new run directory. Fix the
cause or choose a justified budget, then rerun into a new directory. These checks
qualify command evidence, not physical durability or target-device behavior.

## Verify the delivered ZIP

`verify_desktop_archive.py` accepts one new report path and retains either the
successful or failed result. It bounds compressed bytes, central-directory size,
member count and expanded content before extraction. It rejects unsafe portable
paths, duplicate/case/file-directory aliases, multiple roots, symlink/special
members, encryption, unsupported compression and unexpected file modes.

Extraction occurs in a private temporary directory. Passive manifest, inventory,
content, executable architecture and current receipt validation must succeed
before optional `--smoke-help` executes the binary. The tree/modes and archive
identity are rechecked afterward. Temporary extraction is removed; the report
retains exact ZIP identity and scope. This establishes consistency, not publisher
signing, a native window, GPU support or a clean-machine/Windows qualification.

The package contains this checker and its standalone verifier dependency, so the
same command is available from either source or an extracted desktop folder.
