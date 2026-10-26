# Task 2 report: `fl_math`

**Status:** DONE_WITH_CONCERNS (minor; see Concerns)
**Commit:** `0d4f8eb feat(fl_math): Fx fixed-point, vectors, binary angles, trig tables, PCG32` (branch m0/phase-a, only `crates/fl_math/**` staged)

## What was implemented

- **`fx.rs`**: `Fx(I32F32)`, `#[repr(transparent)]`, with the derives from the brief.
  - Consts: `ZERO ONE HALF MAX MIN EPSILON`.
  - `const fn`s: `from_int`, `from_ratio` (i128, truncates toward zero, panics on den 0 or overflow), `from_raw`, `raw`, `floor_to_i64` (arithmetic shift), `abs`, `min`, `max`, `clamp`, `sqrt` (`u128::isqrt((raw as u128) << 32)`, ≤ 0 gives ZERO), `mul_int`.
  - `from_f64` rejects NaN, ±inf and |v| ≥ 2^31, then uses `I32F32::checked_from_num` (round to nearest).
  - `to_f32` and `to_f64`.
  - Ops plus `*Assign` delegate to `I32F32`, so overflow panics in debug and wraps in release.
  - `Display` delegates to `I32F32` (decimal).
  - `FxRangeError { value: f64 }` implements Display and Error, and its message names the value.
  - serde: `Serialize` writes `f64`. `Deserialize` calls `deserialize_f64`, which works with RON and non-self-describing formats such as postcard. The visitor handles f64, i64, u64, i128 and u128, and every path goes through `from_f64`.
- **`trig.rs`** (crate-private):
  - `PI_Q62` integer constant, computed offline with Python `decimal`; a test checks it against `f64::consts::PI`.
  - `QUARTER_SINE: LazyLock<Vec<i64>>` has 16 385 entries, built by an i128 Q62 Taylor series with no float. Entries are rounded to Q32, and the other quadrants mirror them.
  - `ATAN_TABLE` uses the brief's 16 values verbatim.
  - `atan2_raw` handles the axes exactly. Otherwise it normalises magnitude so the larger component's top bit is bit 59 (no overflow even for `Fx::MIN`, and full precision for `EPSILON` inputs), reflects into the right half-plane (+32768), then runs 16 CORDIC vectoring iterations on i64.
- **`angle.rs`**: `Angle(u16)`.
  - Consts: `ZERO QUARTER HALF`.
  - `from_raw` and `raw`.
  - `from_degrees` uses exact integer rounding (nearest, ties toward +∞) and wraps any value.
  - `to_degrees` is exact. `to_radians_f32` is for presentation.
  - Wrapping `Add Sub Neg`.
  - `signed_diff` (`wrapping_sub as i16`), `rotate_towards` (snaps when within `max_step`), `sin`, `cos` (= sin + QUARTER) and `atan2`.
  - serde as degrees through `Fx`.
- **`vec.rs`**: `FxVec2` with every item listed in the brief, and `FxVec3` (`new ZERO Add Sub Mul<Fx> xy`). Both serialize as tuples.
  - `length` is computed exactly as `isqrt(x_raw² + y_raw²)` in u128 and saturates at `Fx::MAX`, rather than as `length_sq().sqrt()`. This is more precise, works for any magnitude, and does not overflow.
  - `normalize_or_zero` uses the same exact length and an i128 division.
  - `length_sq` and `dot` use plain Fx multiplication. They overflow for lengths ≥ ~46 341, which is documented.
- **`rng.rs`**:
  - `Pcg32` is PCG-XSH-RR 64/32, seeded exactly as `pcg32_srandom_r`.
  - `next_u64` returns the first draw in the high half.
  - `range_u32` uses Lemire multiply-and-reject (unbiased) and panics on an empty range.
  - `chance` is `range_u32(0, den) < num`.
  - `unit_fx` is `from_raw(next_u32)`.
  - `range_fx` computes `lo + floor((hi-lo)·u32 / 2^32)` in i128, giving `[lo, hi)` without overflow. It panics if lo > hi.
  - `state()`.
  - `derive_seed` is `splitmix64(splitmix64(splitmix64(seed) ^ (tick<<32 | sim_id)) ^ stream)`, as a `const fn`.

## TDD evidence

**RED.** I wrote all test modules plus `tests/fx_props.rs` first, with `lib.rs` re-exporting the not-yet-defined items.
```
$ cargo test -p fl_math
error[E0432]: unresolved import `angle::Angle`
error[E0432]: unresolved imports `fx::Fx`, `fx::FxRangeError`
error[E0432]: unresolved imports `rng::Pcg32`, `rng::derive_seed`
error[E0432]: unresolved imports `vec::FxVec2`, `vec::FxVec3`
error: could not compile `fl_math` (lib) due to 4 previous errors
error: could not compile `fl_math` (lib test) due to 295 previous errors; 5 warnings emitted
```

**First GREEN run.** 38/39 passed. `from_f64_range_edges` failed because my own test was wrong: it assumed the largest f64 below 2^31 rounds up to 2^31. It is actually 2^31 − 2^-22, which Q32 holds exactly. I fixed the test to assert the exact raw value (`i64::MAX - 1023`) and corrected a misleading comment in `from_f64`.

**GREEN.**
```
$ cargo test -p fl_math
running 40 tests ... test result: ok. 40 passed; 0 failed
running 4 tests (fx_props) ... test result: ok. 4 passed; 0 failed
```

**Gate** (from C:\fl\a, re-run after the last edit):
- `cargo fmt --check`: OK. I ran `cargo fmt -p fl_math` once to apply formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: all pass (fl_math 40 + 4, fl_protocol 4).
- `bash scripts/check_deps.sh`: `deps OK`.

**Measured accuracy** (temporary measurement test, deleted before commit):
- sin: worst error against f64 is 1.16e-10, i.e. 0.4999 raw units, so the table is correctly rounded to Q32.
- atan2 over the −50..=50 grid: worst error 3 units (limit 4).
- `from_angle(a).angle()` over all 65 536 angles: worst error 3 units. A unit test now checks this exhaustively too.

## Files changed
- `crates/fl_math/src/lib.rs` (modified)
- `crates/fl_math/src/fx.rs`, `angle.rs`, `vec.rs`, `trig.rs`, `rng.rs` (new)
- `crates/fl_math/tests/fx_props.rs` (new)

No Cargo.toml or Cargo.lock changes.

## Tests beyond the brief
- Fx: range edges (±inf, ±2^31, just below 2^31), the error message names the value, RON ints and floats round-trip, consts/raw, `from_ratio` sign and floor, ops and assign-ops, helpers (sqrt exact raw values including `MAX`), Display.
- Angle: cardinal sin/cos exact, cos everywhere, atan2 axes plus extreme magnitudes (`MIN`, `MAX`, `EPSILON`), `rotate_towards` snapping, wrapping ops and `signed_diff` bounds, degrees round-trip (every 7th raw value), serde as degrees.
- Vec: ops, length extremes (EPSILON, saturating MAX), normalize (tiny, `MIN`), angles and rotate, clamp, tuple serde (2D and 3D), FxVec3, exhaustive from_angle round-trip.
- Rng: each derive_seed input matters (tick and sim_id not interchangeable), `next_u64` composition, `range_u32` bounds, coverage and wide range, `chance` edges and rate, `range_fx` bounds, serde round-trip of state.
- Trig: π constant, table endpoints and monotonicity, CORDIC table equals rounded `atan(2^-i)`.

## Self-review findings
- f64 arithmetic in non-test code appears only in `from_f64` (finite/abs check) and `to_radians_f32`. `to_f32` and `to_f64` are conversions. Two serde paths also touch f64 only by passing values along, which the brief requires:
  - `Serialize` writes an f64.
  - The integer visitors cast `as f64` before `from_f64`.
- The trig tables are built with pure integers.
- No `unsafe`.
- No HashMap or Instant.

## Concerns / interpretations
1. **`mul_matches_f64_within_2eps` tolerance.** With operands up to ±40 000, the f64 reference product (up to 1.6e9) is itself rounded by up to ~1e-7, which is far more than 2 eps (4.7e-10). A literal "within 2 eps of f64" test is impossible at that range. The test therefore checks two things:
   - (a) `|got − f64 product| ≤ 2 eps + 2·|expected|·f64::EPSILON`;
   - (b) an exact i128 check that the result is within 2 raw units of the true product.

   (b) is the real 2-eps guarantee.
2. **`vec_length_matches_f64`** uses `prop_assume!(expected > 1e-3)`. Below that length, the 2^-32 resolution alone exceeds 1e-6 relative error. Raw components are drawn uniformly over ±2000·2^32, so the assumption almost never triggers.
3. **Fx serde round-trip is lossy.** Fx serialization through f64 (as specified) is exact only for |v| < 2^21. Larger values with fine fractions lose their low bits, so do not use serde to snapshot or hash sim state containing such values.
4. **`Fx` overflow follows integer semantics** (debug panic, release wrap), per spec §5.4 "checked in debug builds". `Fx::MIN.abs()` overflows.
5. **Additions the brief did not list:** `FxVec3` got tuple serde for consistency with `FxVec2`. `FxVec2`/`FxVec3` also derive `Default, PartialEq, Eq, Hash`.

---

# Fix round 1

**Status:** NEEDS_CONTEXT for item 1. Items 2 and 3 are done in the working tree but **not committed**, because the requested commit covers all three items.

## Item 1 (R5, lossless binary serde): NEEDS_CONTEXT
`crates/fl_math/Cargo.toml` dev-deps are only `proptest` and `ron`. There is no `postcard`, and I am not allowed to edit Cargo.toml.

I have not implemented R5. The change itself is small: branch on `is_human_readable()` in the Fx and Angle `Serialize`/`Deserialize`, raw `i64`/`u16` when false. It cannot be tested without a non-human-readable format, because serde's `de::value` deserializers report `is_human_readable() == true`. Options:
- (a) add `postcard.workspace = true` to fl_math `[dev-dependencies]`;
- (b) let me write a tiny test-only non-human-readable serializer/deserializer pair;
- (c) implement without a direct test and rely on a later fl_protocol postcard round-trip test.

## Item 2 (pinned table bits): done
- `trig::tests::quarter_table_pinned_bits`:
  - `QUARTER_SINE[1] = 411_775`
  - `[8192] = 3_037_000_500`
  - `[16383] = 4_294_967_276`

  Python f64 reference: 411774.83, 3037000499.976, 4294967276.26, so all three are correctly rounded.
- `angle::tests::atan2_pinned_bits` pins seven current CORDIC outputs as (y, x) → raw (true value in parentheses):
  - (1,1)→8194 (8192.0)
  - (1,2)→4834 (4836.02)
  - (3,4)→6712 (6711.96)
  - (-5,12)→61418 (61418.18)
  - (7,-3)→20608 (20607.17)
  - (-1,-1)→40962 (40960.0)
  - (50,-49)→24468 (24470.65)

  All are within 3 units.

## Item 3 (rotate_towards tie): done
- The doc comment now says that when the target is exactly opposite (`signed_diff` = -32768), it always turns clockwise (decreasing raw).
- Test `angle::tests::rotate_towards_opposite_turns_clockwise`: 1000 toward 33768 with step 10 gives 990, and 0 toward HALF gives 65526.

## Commands and output
```
$ cargo fmt -p fl_math && cargo test -p fl_math
test result: ok. 43 passed; 0 failed   (unit)
test result: ok. 4 passed; 0 failed    (fx_props)
$ cargo clippy -p fl_math --all-targets -- -D warnings
Finished `dev` profile [optimized + debuginfo] target(s) in 1m 12s   (no warnings)
```
Uncommitted: `crates/fl_math/src/angle.rs`, `crates/fl_math/src/trig.rs`.

## Fix round 1, continued (ruling: option a)

**Status:** DONE. **Commit:** `da0fa1c fix(fl_math): lossless binary serde, pinned table bits`. It covers items 1–3. I staged only `crates/fl_math` and `Cargo.lock`.

- **Dependencies:** `crates/fl_math/Cargo.toml` gets `postcard.workspace = true` in `[dev-dependencies]`. Cargo.lock changed by one line: `"postcard"` added to fl_math's dependency list. postcard 1.1.3 was already locked.
- **`Fx` serde:**
  - `Serialize`: `is_human_readable()` gives `f64` (unchanged); otherwise `serialize_i64(raw)`.
  - `Deserialize`: `is_human_readable()` gives the existing f64 visitor through `from_f64`; otherwise `i64::deserialize` then `from_raw`.
- **`Angle` serde:** human-readable formats use degrees through `Fx` (unchanged); otherwise `serialize_u16(raw)` / `u16::deserialize`.
- `FxVec2`/`FxVec3` delegate to `Fx` through tuples, so they are lossless in binary formats too.
- **Covering tests (new):**
  - `fx::tests::postcard_roundtrip_is_bit_exact`: raw values 0x7FFF_FFFF_FFFF_FFF1, -0x1234_5678_9ABC_DEF1, i64::MIN, i64::MAX, ±1 and 0 round-trip exactly. It also checks that the binary bytes equal postcard's encoding of the raw i64.
  - `angle::tests::postcard_roundtrip_is_bit_exact`: all 65 536 angles round-trip, and the bytes equal the raw u16's encoding.
  - `vec::tests::postcard_roundtrip_is_bit_exact`: FxVec2 and FxVec3 with odd raw values.
- RON behaviour is unchanged; the existing RON tests still pass.

```
$ cargo fmt -p fl_math && cargo test -p fl_math
test result: ok. 46 passed; 0 failed   (unit)
test result: ok. 4 passed; 0 failed    (fx_props)
$ cargo clippy -p fl_math --all-targets -- -D warnings
Finished `dev` profile [optimized + debuginfo] target(s) in 5.64s   (no warnings)
```
This resolves the original concern 3 (lossy serde) for binary formats. It remains true only for human-readable formats, as intended.
