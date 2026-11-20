# Native saved-state and recovery inspection

**Passed.** The original candidate world and its native backup retain identical complete logical contents at checkpoint **23**. After the recovered world was played and saved during direct confirmation, the requested progression and money remained unchanged.

| Persisted value | Original | Backup | Recovered after confirmation |
|---|---:|---:|---:|
| Checkpoint generation | 23 | 23 | 35 |
| Completed supply contracts | 1 | 1 | 1 |
| Experience | 50 | 50 | 50 |
| Personal wallet | 181.13 cr | 181.13 cr | 181.13 cr |
| Review23 treasury | 75.00 cr | 75.00 cr | 75.00 cr |
| Founded settlements | 1 | 1 | 1 |

The recovered simulation advanced from 421.499598 to 532.949492 seconds and saved 12 further generations. Its runtime and clock are therefore expected to differ from checkpoint 23. Source identity, catalog, region level and the listed authoritative progression and money values remain identical. Full-state equality after resumed play is not claimed.

The original/backup comparison checks every schema tuple and every column of both persisted tables. All three SQLite integrity checks returned `ok`. Connections used `mode=ro&immutable=1` with query-only mode; each WAL was absent or empty. Hashes, sizes and Unix modes of all 8 inspected input files remained unchanged before and after the inspection. No full save payload was copied into this repository.

The direct recovered-world process receipt binds PID **14073**, exit **0**, source hashes unchanged across the run, and an actual matching log hash. Capture records 70, 72 and 76 identify that same window PID and their image hashes match. Earlier candidate captures establish only a stable window PID, **12147**. They do not independently establish its parentage, executable, or exit status; no parent-log PID link or standalone process receipt was retained for that window.

The initial pre-confirmation three-way equality was seen in direct tool output but lacks a separate quiescence receipt. A subsequent inspection correctly refused the recovered world while it had a nonempty WAL. The current result above comes from the fresh read after the direct confirmation process exited.

## Retained evidence

- [Untouched original/backup comparison](candidate-original-backup-state.json)
- [Nonempty-WAL refusal](candidate-state-inspection-deferred.json)
- [Post-confirmation invariant inspection](confirmation-state-inspection.json)
- [Direct recovered-world process receipt](confirmation-recovered-process.json)

This evidence verifies the persisted synthetic native journey. Rust domain/catalog validation and final canonical release acceptance remain separate.
