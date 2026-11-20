# Independent review — extracted desktop verifier

Reviewed `scripts/verify_desktop.py`, its tests and package integration during
0.22 implementation. This is code review; the integrator owns executed Python
and final extracted-package evidence.

## Good boundaries retained

Passive inspection does not run a binary. Explicit smoke mode checks host OS and
architecture, rechecks the binary hash, uses a finite timeout, and requires the
expected bounded help output. The verifier checks portable path spelling,
duplicate JSON keys, exact file hashes/inventory, terminal receipt alias identity,
accepted binary association and all referenced gate/report hashes. Source-free
standard-library execution is appropriate for a packaged Windows/Linux handoff.
The scope explicitly distinguishes consistency from publisher authentication and
physical-device qualification.

## Findings sent to the integrator

1. **Hard-linked files were admitted.** Symlink/reparse rejection alone does not
   reject another regular file alias or an external hard link. Recommend checking
   regular-file link counts where provided and real `os.link` rejection tests.
2. **One terminal alias was read without its JSON bound.**
   `evidence/gates.json` used `read_bytes()` after a generic file limit of 4 GiB,
   while the canonical terminal receipt used an 8 MiB bounded reader. Recommend
   bounded reading for both aliases before comparing their exact bytes.
3. **Gate-counter validation was permissive.** Equality to zero with defaults
   admits missing values, `false` and `0.0`. Recommend strict integer counter
   fields consistent with release admission. Do not imply that this lightweight
   association check independently executes or authenticates the acceptance run.

These are integration corrections, not a request to broaden into a signing or
update subsystem. The final review status belongs to the integrator's recorded
resolution and fresh tests.

## Confirmation

Reinspected the corrected implementation and tests. Both regular-file inventory
and direct file resolution now reject link counts above one; the test creates a
real external `os.link` alias. Both terminal receipt names use the bounded JSON
reader before exact-byte comparison. Failed/ignored counters must be present
integer zeros, with missing, boolean and floating-point regressions.
The integrator's `v0.22-preflight/python-tooling-confirmation.log` records 46
passing Python tests. These three findings are closed for the inspected code;
fresh canonical release acceptance and final packaged execution remain the
integrator's separate gates.
