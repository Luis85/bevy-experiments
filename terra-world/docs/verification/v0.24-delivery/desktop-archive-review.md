# Independent final desktop archive review — 0.24

**The actual delivered Linux ZIP passed fresh extraction and bounded startup
verification** on 30 October 2026 at `13:40:17.117112+00:00`. The command ran on
the normal host environment, without a toolchain wrapper or dependency override:

```sh
python3 -B scripts/verify_desktop_archive.py \
  --archive /workspace/scratch/96f1d170dd13/deliverables/terra-world-0.24.0-linux-x86_64.zip \
  --smoke-help \
  --report docs/verification/v0.24-delivery/desktop-archive-verification.json
```

The command exited **0**. The independently reread result is retained in
[desktop-archive-verification.json](desktop-archive-verification.json), SHA-256
`bdaaef4675cf1e67bd4dcf4549aa7f7d39c3b2dbaf716203f63e48b4db1704e6`.

## Actual artifact and acceptance binding

| Property | Verified result |
|---|---|
| ZIP | `terra-world-0.24.0-linux-x86_64.zip` |
| Compressed bytes | 39,734,961 |
| ZIP SHA-256 | `8ec346e51b065d8779bb62bb59535ef14f3ebcd216164c2ce1066ad170c2c81c` |
| Single package root | `terra-world-0.24.0-linux-x86_64` |
| Inventory | 272 regular ZIP entries: **271 manifest-listed payload files plus the manifest** |
| Extracted bytes | 172,925,602 |
| Native executable SHA-256 | `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6` |
| Build profile | `dev`, the exact accepted review executable |
| Manifest SHA-256 | `b23df490224a08c75e8a9d0e815c2001af89af37d245693fd0bb56ec018e256d` |
| Included terminal receipt SHA-256 | `8c386ac3ef5f1bead91f808db90b1301a6b6b834096a36c2cc654abd8f776d81` |

Independently read the actual ZIP's manifest and terminal receipt after the
verifier completed. The bundled `evidence/completed-gates.json` exactly equals
the current completed canonical receipt bytes: **44 gates, 1,099 distinct passing
Rust tests and 1,200 passing Rust test executions**. Its native executable hash
matches both the package manifest and the freshly extracted executable result.
The ZIP's actual byte count and SHA-256 also matched the retained verifier result
on a separate readback.

## Extraction and startup observations

The executed verifier validates the complete ZIP namespace, declared types,
archive modes and byte budgets before writing members into a fresh temporary
directory. The Unix parent was owner-only **0700**. Passive exact-inventory,
manifest, executable and current receipt checks completed before the verifier
could invoke startup. It repeated package/tree and mode checks afterward and
rechecked the original archive identity/hash. The receipt records
`fresh_extraction`, `extracted_tree_unchanged` and `archive_unchanged` as true;
every extracted regular-file mode matched the archive.

The extracted binary's `--help` exited **0** in **0.010926 seconds**, producing
**3,227 bytes** within the 65,536-byte limit. Its owned POSIX process group was
absent afterward. Help output SHA-256:
`d1546eb9baeeff823232dee6a7f83a1331e73a8adf87c7bd194ce683bb87e979`.

## Package root and launch instructions

Inspected the actual archived `START_HERE.txt`, `start-terra.sh` and
`docs/DESKTOP_DELIVERY.md`. They consistently tell the Linux user to extract the
entire folder, keep runtime data beside the executable, and run `./start-terra.sh`
from a graphical X11 session. The shell launcher changes to its own package
directory, executes the local `terra-demo --menu`, forwards supplied arguments,
and places user data under
`${XDG_DATA_HOME:-$HOME/.local/share}/terra-world`.

The included instructions distinguish passive verification from optional
`--help`, put reports outside the package, explain explicit direct-play save/data
paths, and describe system-library/driver and recovery limitations. The Linux
ZIP includes its Linux launcher; Windows instructions are explicitly for a
separately built and qualified Windows package. No actual Windows qualification
is inferred from those instructions or CI configuration.

## Scope

No blocking archive or startup issue was found. This check qualifies exact
artifact consistency, canonical association, bounded extraction and command-line
startup on this host. It does not authenticate a publisher, open a game window,
exercise a saved world, qualify a GPU/input device, establish universal Linux
portability or provide Windows acceptance. Recorded native visual/process and
domain-test evidence retain their separate scopes. No source or payload
documentation was modified during this independent archive review.
