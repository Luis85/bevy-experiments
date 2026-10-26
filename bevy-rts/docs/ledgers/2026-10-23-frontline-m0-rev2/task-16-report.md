# Task 16 report: fl_cli data-check and hash-dump

Implemented crates/fl_cli/src/{lib,main,data_check,hash_dump}.rs, tests/data_check.rs, fixture tests/fixtures/bad_mod
(missing dependency, unknown component, unknown file kind `.widget.ron`). `data_check` returns `Result<_, BuildError>`;
the binary prints all `ContentErrors` and exits 1. `hash_dump` uses `step_tick` directly with `RunConfig.hash_interval`.
Extra: `--json` flag on both subcommands. Interval 0 is a Usage error.
Tests: 4/4 passing (cargo nextest run --profile agent -p fl_cli). TDD red step skipped (tests and impl written together).
Gate: `cargo xtask gate task --track tools` -> all PASS incl. data-check, "gate task: PASS in 212s".
