# Delivery review and changes — 0.18

Reviewed 29 October 2026 against the attached 0.17 source. Scope: release scripts and desktop extraction integrity; no Windows execution claim.

## Concrete findings and fix

The desktop packager rejected traversal and symlinks, but accepted case-sensitive source names that collide during extraction to the normal Windows case-insensitive filesystem. It also accepted DOS device names, trailing dots/spaces and Windows-invalid characters. A successful ZIP CRC/hash check does not ensure those paths survive extraction. These conditions could produce a nominally accepted archive with missing or overwritten files.

`portable_parts` now rejects noncanonical/traversal names, DOS device components (including extensions and documented superscript port digits), trailing dot/space components, control characters and Windows-invalid punctuation. `validate_namespace` checks case aliases and file/directory conflicts in the full assembled desktop namespace before publication. Existing symlink containment and exact-byte verification remain.

An independent review identified another insertion boundary: gate logs and evidence artifacts were inserted into a dictionary before namespace comparison, allowing duplicate exact identities to disappear. Combined raw reserved/report/log/artifact names are now checked before any evidence insertion. The exact `source-hashes.json` evidence artifact is the single intentional overlap, already hash-checked against its accepted source manifest. Differently cased spellings, duplicate logs and report/log shadows are rejected.

The release runner now includes `desktop-packaging-tests` as a gate. Its output uses Python unittest's separate test count; Rust test totals remain Rust-only.

## Executed evidence

From `/tmp/terra-v018-integration/terra-world`:

- `python3 -m unittest discover -s scripts -p test_package_desktop.py -v`: **11 passed**, retained in `desktop-packaging-tests.log`.
- `python3 -m py_compile scripts/package_desktop.py scripts/test_package_desktop.py scripts/verify.py`: exit 0.
- Current 152-entry desktop allowlist plus generated Linux filenames: portable namespace passed.
- Current 152-entry desktop allowlist plus generated Windows filenames: portable namespace passed.

The suite tests valid extraction bytes and acceptance-record hash from an actually written ZIP, exact/case alias payload rejection without publication, duplicate logs, artifact/report/log shadows, devices, forbidden/trimming characters, traversal, file/directory conflicts and symlink traversal. Full packaging tests use an isolated temporary project, mocked acceptance/source bindings and a minimal synthetic ELF header; they verify packaging behavior and do not substitute for release gates or execution of a real native binary.

The real symlink case is explicitly POSIX-only so future Windows runs do not require elevated symlink-creation privileges; Windows will report that platform skip. The Linux run above has zero skipped tests.

The initial added regression run against the pre-fix packager reproduced acceptance of the case-alias payload and duplicate gate logs. The existing valid publication path passed before and after the correction. Workspace file synchronization restored baseline scripts during that first run; final implementation and accepted tests were moved to the stable integration tree before the evidence above was collected.

## Windows and trusted-delivery boundary

G02 remains open. No Windows compiler, executable, clean-machine extraction, create/play/save/reopen/recovery journey, physical GPU/input devices, signing, SmartScreen reputation, installer or update/rollback qualification was executed here. Portable-name checks reduce one deterministic packaging failure but cannot certify Windows support.

The next feasible Windows increment should run the repository's pinned toolchain and full gates on the intended Windows host, package its exact accepted PE executable, extract to a fresh ordinary user folder, then execute the documented first-session, save/reopen and recovery journeys. Keep the resulting OS/GPU/input/runtime profile and actual captures/receipts. Signing and authenticated release distribution remain separate trust decisions (G11); file hashes establish local integrity only.
