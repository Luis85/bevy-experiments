# Source archive helper cross-review

Reviewed 29 October 2026 by the reliability owner. Scope:
`package_source_021.py`, its imported namespace/admission contracts and the
current source-tree inventory. This is a source review, not an executed archive
acceptance result. No tests, Cargo commands or archive publication were run by
this reviewer.

## Disposition

The integrator corrected all three material findings. No remaining material
issue was found in the corrected helper for the intended frozen-tree handoff.

- **Evidence binding:** the helper requires the passed receipt's unchanged-input,
  report and log flags; exact terminal/progress-alias bytes; the canonical gate
  results and execution totals; hashed evidence and logs; unique structured test
  identities and reconciled counts; the accepted native executable digest; and
  the accepted source-input map. It rebinds accepted evidence hashes to the
  actual archive inventory before writing.
- **Original modes:** the initial inventory found 669 original entries recorded
  as Unix 0600 in the supplied ZIP but extracted as 0644. The corrected helper
  restores original ZIP modes in archive metadata, separately records differing
  `source_tree_mode` values and verifies the intended modes during ZIP readback.
  New entries retain their source-tree modes. No original executable-bit
  differences were observed.
- **External destinations:** both output and report paths are resolved and
  checked for containment, existing destinations and equality. Direct symlink
  destinations are rejected; resolved paths prevent a symlinked parent from
  redirecting a purportedly external publication into the source tree.

The helper rejects source symlinks, checks portable namespace collisions,
retains every original file, limits original-byte changes to the recorded
allowlist and preserves the two archived current documents exactly. It hashes
inventory bytes, verifies each packaged file and mode, checks the generated
manifest and full member roster, runs CRC readback and checks frozen inputs again
before emitting success.

The read-only inventory at initial review contained 98 additions, all expected
source/documents or review/execution evidence. It found no new saved worlds,
session identities, database files, binaries, nested archives, symlinks or
nonregular files. This is a point-in-time observation; the final archive manifest
is the definitive inventory after the remaining evidence is written.

## Limits

Final archive execution must still complete its byte/mode readback and publish
the external verification result. A source review does not establish that it
has done so. The checks establish local preservation and consistency, not
publisher authentication, arbitrary concurrent filesystem-mutation resistance,
power-loss durability or target hardware qualification.
