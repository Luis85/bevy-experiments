### Task 16: `fl_cli` data-check and hash-dump (rev-1 T10)

**Track:** tools · **Lane 2** (cut after Task 14)

**Files:**
- Create: `crates/fl_cli/src/{lib,main,data_check,hash_dump}.rs`, `crates/fl_cli/tests/data_check.rs`
- Create fixture `crates/fl_cli/tests/fixtures/bad_mod/**` (a missing dependency, an unknown component, an unknown file kind)

**Interfaces:**
- Consumes: `fl_catalog::{features, base_mod_dir}`, `load_content`, `SimRunner`, `TestDriver`, `MatchSetup::for_scenario`.
- Produces:
```rust
pub struct DataCheckReport { pub mods: Vec<String>, pub units: usize, pub kinds: Vec<(&'static str, usize)>, pub warnings: Vec<String>, pub rules_hash: u64 }
pub fn data_check(dirs: &[PathBuf]) -> Result<DataCheckReport, BuildError>;
pub fn hash_dump(mods: &[PathBuf], scenario: &str, ticks: u32, interval: u32) -> Result<Vec<(Tick, u64)>, CliError>;
#[derive(Debug, thiserror::Error)] pub enum CliError { Build(String), Setup(String), Io(String), Usage(String) }
```
- CLI (`clap` derive):
  - `fl_cli data-check <DIRS>...` prints the report, including `rules hash: 0x{:016x}`. On failure it prints every error and exits 1.
  - `fl_cli hash-dump --mods <DIR>... --scenario <ID> --ticks <N> [--interval <N>]` prints one `tick hash` line per hash.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn base_mod_passes()               // data_check(&[base_mod_dir()]) is Ok with units == 10
#[test] fn bad_mod_lists_all_errors()      // ≥ 3 errors, and each one's Display contains "bad_mod"
#[test] fn data_check_bad_mod_exit_code()  // the binary (env!("CARGO_BIN_EXE_fl_cli")) on the bad fixture → exit code 1, output contains every error
#[test] fn hash_dump_is_repeatable()       // base.testbed, 100 ticks, interval 10 → two calls equal, 10 entries
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo run -p fl_cli -- data-check assets/base` prints the report.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: `PASS data-check` (the row is now active) and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_cli): data-check and hash-dump`.

