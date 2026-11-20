# 0.21 integration, delivery and product-scope review

Baseline: supplied 0.20 source archive. Three independent specialists reviewed
player experience, world/simulation, and reliability. The integrator reviewed
networking, delivery and cross-module contracts and wrote the implementation
plan before dispatch. This file records integration decisions; final executed
counts belong in the current validation report and terminal gate receipt.

## Review evidence and decisions

`NetClient::poll`, `ReplicaState`, protocol admission and `SessionIdentity` were
inspected. The incumbent code retains requested-manifest validation, revisioned
replication, identity binding, bounded framed messages, explicit TLS trust and
private capability storage. No new network-protocol change was selected. Full
TCP/TLS regression remains part of canonical acceptance. This source review
does not establish Internet accounts, moderation, WAN performance or publisher
trust; those remain G13/G14/G18 work.

The native source review found remaining issues despite earlier passing checks:
Loading controller gating, draft focus boundaries, misleading direct-JSON save
copy, heterogeneous-shop pricing panic, clipped-neighborhood bootstrap, global
feature-edit remeshing, full-state adapter hydration bypass and snapshot file
modes. Each receives a boundary regression; the old receipt is not reused as
proof of those changes. A later first-launch witness design was approved after
reviewing its two-file crash window and conservative recovery behavior.

The release runner waited indefinitely for each gate. WP09 adds a configurable
positive finite deadline (default 3600 seconds), owned-process-tree cleanup,
retained partial output, exit 124 and explicit timeout fields. Startup metadata
and compiler probes also have deadlines. Desktop admission rejects a timed-out
gate even when another field incorrectly records exit zero. The canonical
44-gate roster and historic receipt compatibility remain.

## Fresh preliminary execution

- Installed the repository-pinned Rust 1.99.1 into an isolated task toolchain;
  fetched exact locked dependencies. The release versions are grounded in the
  [official Rust announcement](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/)
  and [official Bevy 0.20 announcement](https://bevy.org/news/bevy-0-20/).
- Baseline Python delivery suite: 28 passed.
- Added real subprocess tests for completed/failed output, timeout and descendant
  termination, finite-deadline parsing and timed-out receipt rejection: 33 Python
  tests passed, zero skips, before final acceptance.
- Initial native dependency warmup encountered a transient 17-parameter Bevy
  system during active UI edits. The owner grouped the query parameters; that
  warmup is not presented as a baseline or final acceptance result.
- Default sandbox display setup could not bind its local Unix socket. The
  requested native verification was then run through the normal reviewed
  execution permission mechanism; the virtual-display probe succeeded at
  1440×900. This supersedes the inherited environment limitation for this run,
  but does not itself establish rendered gameplay or physical GPU/input support.

## Documentation and release scope

The current README now leads with current changes, review/plan/gap/validation
links and actual quick-start instructions. Earlier release overviews are retained
in `docs/RELEASE_HISTORY.md`; original versioned receipts and screenshots stay
historical. PRODUCT's opening iteration is updated from 0.18 to 0.21. The previous
current product review and validation are archived byte-for-byte as 0.20.

Workspace version becomes 0.21.0; only the nine local package versions and the
explicit SQLite test dependency change in the lock. Save 1, regional schema 1,
wire 10/terra-world/10 and asset schema 1 remain. New managed-launch witness
enforcement requires the new launcher; an older reader cannot provide its
additional crash-detection guarantee.

## Remaining review obligations

Peer cross-review, complete canonical acceptance, fresh native rendered
inspection, archive preservation and final artifact verification must be recorded
before describing the deliverable as accepted. Windows execution, physical
devices, measured GPU tails, real users, production content/licensing and public
service operations retain their specific open criteria.

## Final integration completion

The final canonical run passed all 44 gates: 1,000 distinct
Rust tests, 1,097 passing Rust executions, 34 separate Python tests,
zero failed/ignored/skipped outcomes in their stated suites. All 373 frozen
implementation inputs and published logs/reports retained their hashes.
The accepted native hash is `209b5f2ee0d74d8b4776346297af6838dd9e05cf622af80645f63dedb0bc13d1`.

Fresh native candidate review found a Space-key wrap at minimum window/150%
text. One narrow correction batch widened that key and shared the actual save
copy in launcher review. Rebuilt confirmation passed with unchanged runtime
source and the accepted binary; managed create/checkpoint/exit/reopen/exit and
SQLite integrity/witness continuity were observed. See the native record for
software-renderer, first-visible-frame and input-device limits.

Packaging review added six current/historical handoff documents to the curated
desktop allowlist. The initial canonical candidate was interrupted and retained;
final acceptance is `v0.21-final-acceptance`. That allowlist is the sole frozen
input differing from native capture, with unchanged executable bytes.

The local-link audit passed 196 destinations/anchors across 13 current documents.
Independent preservation checked every original and the 1,885-file protected
union. The source-archive helper was peer-reviewed and corrected to bind accepted
evidence, preserve original Unix modes and keep both outputs outside source.
Final archives require actual byte/mode readback before delivery.

Desktop packaging and independent extraction/hash/`--help` smoke both passed.
The curated desktop ZIP contains 255 files and the exact accepted executable.
Final source packaging uses the peer-reviewed archive helper and embeds the
complete per-file manifest; its external result records the archive's own hash.
