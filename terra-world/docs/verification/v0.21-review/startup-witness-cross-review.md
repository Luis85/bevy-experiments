# Independent managed-startup witness cross-review — 0.21

Reviewer: world/simulation owner, reviewing reliability WP08b and the corrected
WP09 startup-probe execution on 29 October 2026. This reviewer has not edited the
reviewed implementation and has not started Cargo while shared builds run.

## Review status

The witness protocol is sound within its stated managed-launch and filesystem
scope. The one concrete integration finding below has been corrected and
independently reviewed in source. No further source-level blocker was found.
The integrator must still take Rust execution outcomes from the coordinated
fresh runs; this source review does not certify unexecuted tests.

## Required correction: reserve managed metadata in output paths

**P2, demonstrated by the source execution path.** The initial WP08b code adds
world-started.json beside the managed profile, but output::validate and the
runtime screenshot guard protect only progress/database/session paths.

For a genuinely unstarted profile, pass a screenshot destination shaped as:

    <managed-profile-directory>/world-started.json/capture.png

The profile initially reads successfully, output preflight admits the path,
and main::run creates its parent directory before prepare_managed_launch.
That directory now occupies the reserved witness pathname. Preparation fails,
and subsequent WorldProfile::read also rejects the directory. The attempt has
stranded an otherwise unstarted profile without creating progress. An existing
PNG symlink or hard-link alias to managed metadata also needs the same alias-aware
guard at the runtime screenshot writer.

The reliability owner independently identified the metadata-output seam and
implemented the requested bounded correction. The reviewer checked that:

- Publication and export checks share model::managed_start_witness_path, so
  their reserved pathname cannot drift independently.
- Output preflight protects the profile and witness, output ancestors and
  existing filesystem aliases before main can create screenshot directories.
- Screenshot and political export use check_managed again at their runtime
  writer call sites.
- Telemetry carries the selected managed profile into its background writer and
  validates both report and lock paths on every publication, including after
  a previous successful capture.
- Every Recorder call site supplies the new argument; unmanaged tests retain
  None. Existing independent progress/session protections remain in place.

Five added regression identities cover future metadata directories, a future
profile/lock collision, hard/symbolic aliases introduced after preflight,
asynchronous recorder refusal before creation and repeated publication checks.
Their Rust execution is owned by the integrator. The corrected diff passes
git diff --check. This reviewer has not claimed native reproduction or Rust
execution of the original finding or its correction.

## Protocol assessment

### New, interrupted and legacy worlds

- A genuinely new profile with no witness and no progress can gain one exclusive
  witness and the in-memory first-initialization admission. Publishing the
  witness does not claim a successful save or change played/checkpoint metadata.
- Any existing progress is validated before a legacy profile gains a witness.
  Missing required progress, empty files, corrupt files and regional generation
  zero fail rather than becoming fresh worlds.
- An interrupted first launch that has published its witness but has no committed
  checkpoint deliberately fails closed. The user receives backup/separate-world
  guidance; the implementation does not automatically remove the witness.
- A committed first checkpoint can resume even if abrupt process exit prevented
  the later presentation-settings write. Losing that checkpoint afterward cannot
  silently turn the witnessed profile into a new world.
- Pre-existing legacy metadata with checkpoint_present=false and already-lost
  progress is inherently indistinguishable from a never-started legacy profile.
  The implementation documentation correctly retains that limitation.

### Identity and concurrent admission

The witness binds the canonical selected profile path, absolute selected progress
path, JSON/regional mode and the existing immutable WorldIdentity. Source identity
is compared again against the prepared runtime configuration before publication.
Title, quality and ordinary presentation/clock settings remain outside the
identity, so the tested name/settings changes remain valid.

Publication writes and synchronizes a complete private temporary, then uses an
exclusive hard link to claim the final pathname. Two contenders that both saw no
witness cannot both receive fresh-world admission. A contender that observes an
existing witness must restore valid progress. This first-initializer guarantee
does not add a general lock excluding every concurrent resumed session.

### Publication failure and runtime barriers

Temporary creation uses create_new and Unix mode 0600. Cleanup begins only after
this attempt successfully owns its temporary. Write/file-sync/link failures do
not authorize world initialization. A failure after publication retains the
witness and refuses this launch; Unix directory-sync failure is reported without
claiming rollback. The tests include a failed exclusive destination and injected
post-publication parent-sync failure.

The managed main path prepares admission before app.run. JSON restore rejects a
required checkpoint that vanishes before restoration. Regional startup performs
read-only full archive/catalog/host validation before its ordinary writable open;
archive extraction rejects a store with no committed generation. Validation
failure blocks persistence and requests error exit, without installing fresh
runtime progress in the checked cases. An admission mode cannot be changed after
regional startup.

These conclusions do not certify physical power loss, Windows ACLs, every
filesystem's hard-link/durability behavior, arbitrary concurrent replacement of
filesystem objects, or behavior of older launchers that do not know the witness.

## Corrected WP09 startup probes

The corrected verify.main routes both cargo metadata and rustc --version through
run_logged. Startup uses the smaller of the requested gate deadline and 120
seconds, publishes a dedicated log, records timed_out/exit/deadline fields and
binds the log hash in evidence_artifacts. A failed or timed-out probe raises
before canonical gates run and leaves a failed terminal receipt. The canonical
gate roster is unchanged.

The Windows cleanup call now uses taskkill with check=True. Failure marks
descendant cleanup unconfirmed, terminates/waits for the leader, and uses explicit
qualified wording in the timeout log instead of claiming the whole tree stopped.

Executed/inspected evidence:

- The integrator's /tmp/terra-python-021.log records 34 Python tests passed with
  zero failures or skips, including the actual startup-child timeout regression.
  This reviewer inspected that fresh log rather than rerunning the broad suite.
- This reviewer executed a focused Python check with a mocked failing Windows
  taskkill. The helper returned false, required checked exit status and attempted
  leader cleanup. This is branch verification on Linux, not Windows qualification.
- A real timed-out Python command, with the cleanup result deliberately marked
  unconfirmed, returned 124, retained exactly its published log bytes, and used
  the unconfirmed-descendant wording. Both focused checks passed.

The earlier reproduced startup-child escape and missing partial output are
addressed by the corrected common execution path and its fresh regression.
