# Reliability and delivery review — 0.24

## Scope and material findings

Reviewed the delivered 0.23 README, current review/validation/gap register,
architecture, compatibility, recovery, security and desktop delivery contracts;
then followed persistence/launcher output admission, canonical subprocess
execution, receipt admission, packaging and CI to their actual implementation.
The selected changes implement WP06–08 in the 0.24 plan. They do not replace
Windows execution, native-window or physical-device acceptance.

| Priority | Delivered behavior and evidence | Implemented improvement |
|---|---|---|
| P1 | Protected-path hard-link comparison returned false outside Unix. Existing path normalization did not reliably resolve Windows directory junctions for a new output. | Use retained portable file-identity handles and resolve existing components before comparing the database, sidecars and managed metadata outputs. |
| P1 | A successful gate leader could return while an ordinary child still held its output and later wrote. Output spooling and final readback had no byte limit. | Bound capture and the shared leader/EOF deadline, require owned POSIX group completion for success, and retain typed failure/cleanup observations. |
| P2 | CI produced and uploaded desktop ZIPs without checking a fresh extraction of the delivered artifact. | Bound and validate ZIP metadata/namespace/types/modes, extract exclusively into a private temporary root, passively verify all payload bytes, then optionally perform bounded startup and recheck the tree/archive. |
| P1, integration review | The first new receipt validator checked execution evidence only when it was present. Removing the entire object could bypass that evidence requirement. | Require complete gate and startup-probe execution evidence for numeric releases from 0.24.0 onward, including host/output policy and actual retained-log byte counts. |

## Primary research and resulting decisions

Research used primary documentation, read on 30 October 2026:

- [Microsoft: hard links and junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions)
  establishes that another pathname or directory junction can refer to the same
  storage. Path spelling alone therefore cannot admit an output safely.
- [Microsoft: FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)
  describes file identity in conjunction with volume identity. The already
  locked [same-file handle API](https://docs.rs/same-file/latest/same_file/struct.Handle.html)
  supplies portable retained handles; `same-file = "=1.0.6"` was promoted to a
  direct persistence dependency without changing its existing package pin.
- [Rust: `std::path::absolute`](https://doc.rust-lang.org/std/path/fn.absolute.html)
  and [Microsoft: file naming](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)
  informed drive-relative, rooted and trailing-dot/space handling. Existing
  components are still resolved afterward, because making a path absolute does
  not establish filesystem identity.
- [Python: subprocess](https://docs.python.org/3.11/library/subprocess.html)
  documents pipe completion, timeout and buffering behavior. The runner now
  treats leader exit, stream closure, output budget and owned-group observation
  as separate facts. No unlimited spool or unbounded final wait remains in the
  canonical runner.
- [Python: zipfile](https://docs.python.org/3.13/library/zipfile.html)
  informed explicit path inspection and bounded extraction. The new verifier
  validates every namespace member before writing any member and never delegates
  archive path interpretation to `extractall`.

The platform restrictions, output budget and public delivery guarantees below
are Terra's explicit implementation decisions. The cited APIs support those
decisions; they do not independently certify this release or its publisher.

## Protected output identity

`terra-persistence::validate_output_path` now uses `std::path::absolute`, resolves
each existing component (including junctions), compares existing file handles
from `same-file`, and rejects inspection errors other than a missing path. The
database, WAL, shared-memory and rollback-journal names remain reserved even
before creation. Callers already use this guard for managed profile, witness and
lease output protection as well as the database itself.

Independent world review found two additional Windows cases and both were
corrected before focused acceptance. Drive-relative and rooted inputs now follow
the operating system's actual absolute-path rules. For unresolved Unicode names,
portable text casing cannot establish Windows filesystem equivalence. Ambiguous
unresolved components beneath the same actual existing ancestor therefore fail
closed. A definite ASCII difference or a distinct resolved directory remains
usable. The diagnostic directs the user to a distinct existing output directory.
Trailing spaces/dots are treated conservatively, including verbatim-path inputs.

Fixtures cover database/sidecar/profile/witness/lease hard links without changing
their bytes, distinct files with identical contents, missing destinations through
directory aliases, and uninspectable ancestors. Windows-only fixtures use a real
junction and cover drive-relative/rooted/trailing-dot paths and Unicode aliases.
Their path values are passed through environment variables to a fixed PowerShell
script rather than interpolated into executable shell text. Pure ambiguity
classification cases also run on Linux.

This remains a guard for cooperative, unchanged paths. It does not create a
filesystem-race sandbox against another process replacing directories between
admission and publication. Windows-specific fixtures are present but have not
been executed on a Windows host in this environment.

## Bounded canonical execution and receipt admission

The canonical roster remains **44 gates**. Each gate and both startup probes now
record execution observations. The default raw output limit is **64 MiB**;
`--gate-output-limit-bytes` accepts a positive integer up to **512 MiB**. Capture
retains at most one overflow sentinel beyond that allowance internally, and a
failure log may add at most 2,048 bytes of diagnostic trailer.

The deadline covers both leader lifetime and output EOF. Cleanup requests owned
POSIX-group termination or a bounded Windows tree termination, then uses bounded
leader/reader waits. A request to terminate is never reported as proof that the
tree stopped. A successful POSIX gate requires an absent owned process group;
Windows records leader exit and closed output with descendant status explicitly
unconfirmed. Deliberately detached descendants are outside the group check.

The retained execution object records status, process platform, failure reason,
output policy, observed raw and retained byte counts, overflow, stream completion,
leader observation, descendant observation and cleanup. Current receipt admission
requires integer zero exit codes, finite positive timeouts, complete successful
execution evidence, the recorded host/output policy, and raw/retained counts that
match the actual hashed log bytes. Linux/POSIX evidence cannot claim success with
an unconfirmed descendant status. Ordered `metadata` and `toolchain` startup
probes, their canonical log names and hash bindings are mandatory. Removing or
partially forging these records cannot authorize packaging.

The legacy path is explicit: numeric releases older than 0.24.0 did not record
this evidence. Optional execution objects in such receipts are still validated
when present. Current/future releases cannot omit them. Structural package
fixtures use real numeric legacy/current versions and do not claim actual game
or canonical execution.

For integrators, `release_contract.validate_gate_results` accepts
`require_execution=True`, the receipt's `environment.os`, and its
`environment.gate_output_limit_bytes`. Full receipt admission additionally calls
`verify_desktop.validate_receipt_execution(receipt, log_sizes)` with sizes from
the evidence bytes actually read. Canonical verification, desktop packaging and
extracted package inspection use these checks.

### Actual delivered-source regression

`reproduce_gate_lifetime.py` loads the exact 0.23 runner from the delivered source
archive and compares its behavior with the revised runner using a real bounded
child. The child waits until the runner has returned before it may write a marker.
The delivered runner returned exit 0 and the child subsequently wrote. The new
runner failed at its 0.5-second shared deadline, retained partial output, and the
child did not write after return.

`gate-lifetime-baseline.json` retains the archive/runner hashes and observations.
Its failed new-run cleanup observed the group still present at that instant
(for example, a killed child can still require reaping), so the receipt does not
claim group absence. It records the termination request, observed leader exit,
closed output and absence of the late write separately. This is a point-in-time
regression, not final release or Windows acceptance.

## Fresh desktop archive verification

`scripts/verify_desktop_archive.py` is standard-library-only and imports the
already shipped `verify_desktop.py`. Parent integration added it to the desktop
payload manifest. The new CI step runs after packaging and before artifact
upload on both configured host jobs; failed reports remain among the always
retained verification artifacts.

Before `ZipFile` allocates member objects, a bounded terminal-directory scan
limits member count and metadata size. The verifier then checks one portable
root, no traversal/case/parent-file aliases, declared regular files, exact
0644/0755 archive modes, supported compression, no encryption, and individual and
aggregate byte budgets. Split archives and unsupported ZIP64 directory layouts
are rejected. Extraction uses exclusive writes and bounded reads into a fresh
temporary directory. Unix access is explicitly 0700 at the parent; Windows ACL
qualification is not claimed.

Passive manifest, binary, source/evidence and exact-tree checks must pass before
`--smoke-help` can run the executable. The extracted tree, modes and original
archive identity are checked afterward. Reports use exclusive publication and
will not replace an earlier result. A corrupt archive receives a failed report.

An actual fresh extraction and `--help` run on the prior delivered Linux desktop
ZIP passed: 266 entries, 39,669,704 compressed bytes, 172,715,399 extracted bytes;
help exited 0 with 3,227 bytes and an absent owned group. The unchanged prior ZIP
SHA-256 is `e13a2b09dee1180ed65f2bfddccdb25cf9c144dba82250f26e73e74da3f8122f`.
`../v0.24-preflight/baseline-desktop-archive-check.json` records this compatibility
check. It is not a 0.24 desktop acceptance result; final 0.24 packaging/extraction
belongs to the integrator's delivered-artifact acceptance.

## Focused execution and independent review

| Retained receipt under `../v0.24-preflight/` | Actual result | Scope |
|---|---|---|
| `reliability-python-initial.json` | 69 Python tests passed; 9.229 s outer duration | Initial implementation before the stricter omitted-evidence review correction. Preserved unchanged. |
| `reliability-python-final.json` | **72 Python tests passed**, no failures/skips; 9.443 s unittest, 9.581 s outer duration | Final tooling fixtures, including complete current receipt admission and startup probes. |
| `reliability-persistence-final.json` | **53 Rust tests passed**, no failures/ignored; 40.341 s including compilation | `cargo test -p terra-persistence --locked --offline` on Linux using the pinned toolchain/cache. |

Each final focused receipt records the exact command, timestamps, exit/deadline,
execution observations and retained log SHA-256. These focused runs do not claim
the final source/executable release binding; canonical integration supplies that
separate acceptance record. Windows-only fixtures were not compiled or run on a
Windows target here.

The 72 Python tests include ordinary success/nonzero exits, exact-limit output,
overflow, inherited pipe and redirected-child behavior, finite cleanup,
startup-timeout/overflow terminal receipts, receipt omission/partial/forged-zero
rejection, archive namespace/type/mode/budget/CRC failures, passive-before-startup
ordering, post-startup changes and failed-report preservation.

The world specialist independently reviewed identity/normalization, runner and
archive boundaries in `world-review-of-reliability.md`. The player specialist
independently reviewed the archive path and reran its 11 tests, recorded in
`player-cross-review.md`. No remaining blocking source finding was reported.

## Remaining delivery requirements

Actual Windows execution and physical GPU/input/accessibility qualification are
still open. Archive hashes prove consistency and association, not publisher
authentication; signed distribution, trusted updates and representative user
devices remain separate acceptance. Owned process groups do not enumerate
deliberately detached descendants. These limits remain explicit in code,
receipts and the carried product gap register rather than being inferred from
passing Linux unit fixtures or CI configuration.
