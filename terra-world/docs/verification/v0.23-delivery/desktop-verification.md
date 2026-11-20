# Independent desktop archive verification — Terra World 0.23.0

**Passed on 30 October 2026.** The actual Linux desktop archive was independently
inspected, extracted into a new private directory, and checked using its shipped
standalone verifier. Passive inspection and explicit bounded `--help` both
returned exit **0**. All archive bytes, extracted files and Unix modes remained
unchanged afterward.

This verification concerns the extracted package and command-line startup on
the current Linux host. It is not clean-machine installation, native-window,
physical-device, Windows or publisher-authentication certification.

## Artifact and accepted identities

| Item | Verified value |
| --- | --- |
| Archive | `terra-world-0.23.0-linux-x86_64.zip` |
| Archive size | **39,669,704 bytes** |
| Archive SHA-256 | `e13a2b09dee1180ed65f2bfddccdb25cf9c144dba82250f26e73e74da3f8122f` |
| Uncompressed files | **266 regular files**, totaling **172,715,399 bytes** |
| Native executable SHA-256 | `432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f` |
| Accepted terminal receipt SHA-256 | `12b4a3f72591c7431bbc0aeda6fe189c5650b81da29d29c7fa8ef955c903cfa0` |
| Package manifest SHA-256 | `26da9124ac8b5bad15242519094f329a045ab0d9fe540591e3892af95b471d5e` |
| Target/profile | `linux-x86_64`, recorded **dev** build profile |

The canonical receipt contains **44 gates**, **1,070 distinct Rust tests** and
**1,171 Rust test executions**. The independently accepted native confirmation
already binds this same executable to the final direct-process screenshots; the
desktop package does not substitute a separately rebuilt executable.

## Complete archive and payload checks

The independent [audit helper](audit_desktop_023.py) checked the exact ZIP member
set before extraction. Every member has the single expected
`terra-world-0.23.0-linux-x86_64/` prefix, a portable unique path and a regular Unix
file type. No directory entries, links, special files, duplicate names,
case aliases, file/directory collisions or encrypted entries were admitted.

The full ZIP CRC check passed. Each entry was then read completely during
extraction, with its byte count, streamed CRC and SHA-256 checked. Every declared
manifest hash matched. The generated manifest itself is included in the
independent inventory and hashed separately.

The **266-file** inventory consists of:

| Category | Count | Association checked |
| --- | ---: | --- |
| Declared runtime/document payload | **180** | Every file matches the current source bytes. The exact payload allowlist is itself bound to the canonical frozen implementation map. |
| Canonical evidence | **82** | Both terminal/progress receipts, 44 gate logs and 36 accepted artifacts match the actual canonical files byte for byte. |
| Native executable | **1** | Matches the accepted native binary hash. |
| Generated launch/readme files | **2** | `start-terra.sh` and `START_HERE.txt` match their package-manifest hashes. |
| Generated package manifest | **1** | Separately hashed and checked against the complete observed inventory. |

The archive records **264 files at mode 0644** and **two at mode 0755**. The
executable and `start-terra.sh` are the two executable entries. All recorded
modes were preserved on extraction.

The private extraction parent was newly created at mode **0700**:

`/tmp/terra023-desktop-independent-_sa8sdsz`

The extracted package is retained below that directory. No game save, user
profile or verification report was written into the package. All raw results
and logs were written outside it in this evidence directory.

## Shipped verifier execution

The audit invoked the **extracted** `scripts/verify_desktop.py` with Python
**3.12.14**, using the extracted package as the working directory. It did not
substitute the checkout's verifier or invoke Cargo.

| Run | Result |
| --- | --- |
| Passive inspection | **Passed**, exit 0, no timeout, empty stderr. Verified **265 manifest-listed files**; the 266th file is the manifest itself. `smoke_help.status` remained `not_requested`. |
| Explicit `--smoke-help --timeout-seconds 20` | **Passed**, exit 0, no timeout, empty stderr. The child printed **3,227 bytes**, below the **65,536-byte** limit, and completed in **0.010872 seconds**. The owned POSIX process group was absent at completion. |

The help-output SHA-256 is
`d1546eb9baeeff823232dee6a7f83a1331e73a8adf87c7bd194ce683bb87e979`.
The small observed help duration describes this invocation only; it is not a
gameplay or graphics-performance benchmark. Deliberately detached descendants
remain outside the verifier's owned-group observation, as its result states.

### Actual environment boundary

The host is **Linux x86_64**. Its `ldd` check resolved the executable's linked
dependencies, including the system `libudev`, `libgcc_s`, `libm`, `libc` and
`libcap`; no missing library was reported. **The prepared toolchain wrapper was
not used. `LD_LIBRARY_PATH` was unset, and no override was added.**

The complete dependency observation is retained in
[desktop-host-libraries.log](desktop-host-libraries.log). Resolving linked
libraries and completing `--help` does not exercise later graphics-driver loading,
a window, input devices or an otherwise unprepared machine. Native interaction
has its separate [accepted confirmation record](../v0.23-native/confirmation-provenance-accepted.json).

## Final unchanged-state check

After both runs, the audit reread every extracted file. Its exact file inventory,
SHA-256, byte length and Unix mode matched the pre-execution snapshot. No
hard-link or symbolic-link alias appeared. The original ZIP retained its exact
size and hash. All 180 declared source payloads, the 381 frozen implementation
inputs and the actual canonical terminal receipt remained unchanged during this
verification.

The complete per-entry inventory, commands, durations, output-file hashes and
environment details are in [desktop-verification.json](desktop-verification.json).

## Retained execution records

- [Passive verifier result](desktop-passive.json) — the shipped verifier's raw
  JSON stdout, preserved verbatim.
- [Passive stderr log](desktop-passive.stderr.log) — empty, as observed.
- [Bounded-help verifier result](desktop-smoke.json) — the shipped verifier's raw
  JSON stdout, including output bounds and process-group result.
- [Bounded-help stderr log](desktop-smoke.stderr.log) — empty, as observed.
- [Host linked-library inspection](desktop-host-libraries.log).
- [Complete independent audit](desktop-verification.json) and
  [reproducible audit helper](audit_desktop_023.py).

The raw result hashes are
`b3f7a384f35f61957b596b9e101cac9282c0758328922ba7110b0c62c976e129`
for passive inspection and
`952eee5c4df274c83bf007209b9b78f4d9faa64cff559de4d33c4f1779c8ba3f`
for bounded help. These establish retained local evidence consistency; they do
not authenticate a software publisher.
