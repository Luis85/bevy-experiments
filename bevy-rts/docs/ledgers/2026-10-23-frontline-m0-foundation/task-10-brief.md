### Task 10: `fl_cli` data-check and hash-dump

**Files:** `crates/fl_cli/src/{lib,main,data_check,hash_dump}.rs`, `crates/fl_cli/tests/data_check.rs`, and `crates/fl_cli/tests/fixtures/bad_mod/**` (a missing dependency, an unknown component and an unknown file kind).

**Interfaces — Produces:**
```rust
pub struct DataCheckReport { pub mods: Vec<String>, pub units: usize, pub kinds: Vec<(&'static str, usize)>, pub warnings: Vec<String>, pub rules_hash: u64 }
pub fn data_check(dirs: &[PathBuf]) -> Result<DataCheckReport, ContentErrors>;    // uses fl_catalog::features()
pub fn hash_dump(mods: &[PathBuf], scenario: &str, ticks: u32, interval: u32) -> Result<Vec<(Tick, u64)>, CliError>;
```
The CLI uses `clap` derive:
- `fl_cli data-check <DIRS>...` prints the report (including `rules hash: 0x{:016x}`) and warnings. On failure it prints every error line, then exits 1.
- `fl_cli hash-dump --mods <DIR>... --scenario <ID> --ticks <N> [--interval <N>]` prints `tick hash` lines.

- [ ] **Step 1: Write the failing tests.**
  - `base_mod_passes`: `data_check(&[base_mod_dir()])` is Ok and `units == 10`.
  - `bad_mod_lists_all_errors`: at least 3 errors, and each one's Display contains `bad_mod`.
  - `data_check_bad_mod_exit_code`: run the built binary via `env!("CARGO_BIN_EXE_fl_cli")` on the bad fixture. It exits with code 1, and its output contains all the errors.
  - `hash_dump_is_repeatable`: `base.testbed`, 100 ticks, interval 10. Two calls are equal and have 10 entries.
- [ ] **Step 2: Run** `cargo test -p fl_cli`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli`. Expected: PASS, and `cargo run -p fl_cli -- data-check assets/base` prints the report.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): data-check and hash-dump`.

