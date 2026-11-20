# Independent world specialist review of reliability changes — 0.24

Reviewed `crates/terra-persistence/src/lib.rs` and the related alias fixtures,
`scripts/verify.py`, `release_contract.py`, `verify_desktop.py`,
`verify_desktop_archive.py`, and the CI extraction/verification sequence.
This is an independent source review; the reliability specialist owns execution
receipts, and final integration owns canonical and delivered-artifact acceptance.

## Findings and resolution

1. **Windows drive-relative missing paths.** The first proposed normalizer
   started from the current directory, then pushed a `C:` prefix. A missing
   drive-relative suffix could therefore remain unresolved when compared with a
   canonical database/sidecar path. The implementation now calls
   `std::path::absolute` before walking and resolving existing components.
   Windows-only fixtures exercise drive-relative, rooted, and trailing-dot/space
   aliases without changing the test process's current directory.
2. **Absent Windows Unicode case aliases.** Existing-file identity handles do
   not cover a sidecar that has not been created. ASCII-only text folding could
   not establish that arbitrary Unicode variants of those names were distinct.
   The final implementation fails closed for ambiguous unresolved Unicode
   components beneath the same actual existing ancestor. It allows a definite
   ASCII component difference or a distinct resolved output directory. The error
   names the recovery: choose a distinct existing output directory. This avoids
   claiming that a portable Unicode casing function implements Windows file
   comparison. Pure helper cases and Windows-only integration cases were added.

Both source corrections were inspected after implementation. No further blocking
source finding remained in this review. The Windows fixtures have not been run
on Windows in this environment; Linux results cannot qualify Windows behavior.

## Other reviewed boundaries

- Existing protected and output file handles remain alive during identity
  comparison. Inspection failures reject before output publication.
- Canonical gate output has a finite byte allowance plus an overflow sentinel.
  Leader completion and output EOF share a deadline. Ordinary owned POSIX
  descendants prevent success and cleanup waits are bounded. Deliberately
  detached work is outside the process-group check; Windows descendant
  completion is explicitly unconfirmed rather than asserted from leader exit.
- Desktop ZIP metadata, declared entry count, names, types, modes, compression
  and uncompressed budgets are checked before exclusive extraction into a
  private temporary directory. Extraction uses explicit member paths and bounded
  reads. Passive package validation precedes optional executable startup. The
  extracted tree, modes and original archive identity are checked afterward.
- CI places fresh extraction and startup verification before desktop artifact
  upload and retains reports on failure. Wiring a Windows job is not a Windows
  execution receipt, physical-device test or publisher-trust guarantee.

These observations concern the concrete changes reviewed on 30 October 2026.
Final executable/source/evidence identity and complete release gates remain the
integrator's acceptance boundary.
