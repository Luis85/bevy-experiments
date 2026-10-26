### Task 2: `fl_math` — Fx, vectors, Angle, trig, PCG

**Files:** Create `crates/fl_math/src/{lib,fx,vec,angle,trig,rng}.rs` and `crates/fl_math/tests/fx_props.rs`.

**Interfaces — Produces:**
- **`pub struct Fx(I32F32)`**, `#[repr(transparent)]`, deriving `Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash`.
  - Consts: `ZERO`, `ONE`, `HALF`, `MAX`, `MIN`, `EPSILON` (raw 1).
  - Constructors and accessors: `from_int(i32)`, `from_ratio(num: i64, den: i64)`, `from_raw(i64)`, `raw() -> i64`, and `from_f64(f64) -> Result<Fx, FxRangeError>`. `from_f64` is for load time only; it rejects NaN, ±inf and |v| ≥ 2^31.
  - Presentation only: `to_f32()`, `to_f64()`.
  - Maths: `floor_to_i64()`, `abs`, `min`, `max`, `clamp`, `sqrt` (≤ 0 gives ZERO), `mul_int(i32)`.
  - Ops: `Add Sub Mul Div Neg` and the `*Assign` forms. `Display` prints a decimal.
  - serde: `Deserialize` accepts RON integers and floats through `from_f64`, and the error names the value. `Serialize` writes an `f64`.
- **`pub struct FxVec2 { pub x: Fx, pub y: Fx }`**, serialized as a 2-tuple `(x, y)`.
  - `new`, `ZERO`, `Add Sub Neg`, `Mul<Fx>`, `Div<Fx>`.
  - `dot`, `perp_dot`, `length_sq`, `length`, `distance_sq`, `normalize_or_zero`.
  - `from_angle(Angle)` gives a unit vector; `rotate(Angle)`; `angle() -> Angle` uses atan2; `clamp_to_rect(min: FxVec2, max: FxVec2)`.
- **`pub struct FxVec3 { pub x: Fx, pub y: Fx, pub z: Fx }`**: `new`, `ZERO`, `Add Sub`, `Mul<Fx>`, `xy() -> FxVec2`.
- **`pub struct Angle(u16)`**, a binary angle where 65536 is a full turn. Derives `Copy, Eq, Ord, Hash, Default, Debug`.
  - Consts: `ZERO`, `QUARTER` (16384), `HALF` (32768).
  - `from_raw(u16)`, `raw()`, `from_degrees(Fx)`, `to_degrees() -> Fx`, and `to_radians_f32()` for presentation.
  - Wrapping `Add Sub Neg`.
  - `signed_diff(self, to: Angle) -> i32`, in `[-32768, 32767]`.
  - `rotate_towards(self, target: Angle, max_step: Angle) -> Angle`.
  - `sin() -> Fx`, `cos() -> Fx`, `atan2(y: Fx, x: Fx) -> Angle` (`atan2(0,0)` = ZERO).
  - serde: serialized as degrees (`f64`), going through `Fx`.
- **`pub struct Pcg32 { state: u64, inc: u64 }`**: PCG-XSH-RR 64/32, seeded exactly like `pcg32_srandom_r`. Derives `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`.
  - `new(seed: u64, stream: u64)`, `next_u32`, `next_u64`.
  - `range_u32(lo, hi_excl)`, unbiased (Lemire or rejection).
  - `chance(num: u32, den: u32) -> bool`.
  - `unit_fx() -> Fx` returns `Fx::from_raw(next_u32() as i64)`, so the result is in `[0,1)`.
  - `range_fx(lo: Fx, hi: Fx) -> Fx`, `state() -> (u64, u64)`.
- **`pub fn derive_seed(seed: u64, tick: u32, sim_id: u32, stream: u32) -> u64`**: splitmix64 over the packed inputs, for parallel phases (§5.4).

- [ ] **Step 1: Write the failing tests.** These are unit tests in each module; f64 is allowed in tests only.
```rust
#[test] fn pcg32_reference_vector() {               // pcg32-demo, seed 42 stream 54
    let mut r = Pcg32::new(42, 54);
    let got: Vec<u32> = (0..6).map(|_| r.next_u32()).collect();
    assert_eq!(got, [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]);
}
#[test] fn sin_table_exact_points() { assert_eq!(Angle::QUARTER.sin(), Fx::ONE); assert_eq!(Angle::HALF.sin(), Fx::ZERO); assert_eq!(Angle::ZERO.cos(), Fx::ONE); }
#[test] fn sin_matches_f64_everywhere() { for r in 0..=u16::MAX { let a = Angle::from_raw(r); let e = (r as f64 / 65536.0 * std::f64::consts::TAU).sin(); assert!((a.sin().to_f64() - e).abs() < 1e-6, "{r}"); } }
#[test] fn atan2_error_within_4_units() { /* grid x,y in -50..=50 step 1, skip (0,0): |signed_diff(atan2, round(f64 atan2 * 65536/TAU))| <= 4 */ }
#[test] fn rotate_towards_takes_short_way() { let a = Angle::from_degrees(Fx::from_int(350)); let t = Angle::from_degrees(Fx::from_int(10)); assert_eq!(a.rotate_towards(t, Angle::from_degrees(Fx::from_int(5))), Angle::from_degrees(Fx::from_int(355))); }
#[test] fn fx_rejects_out_of_range() { assert!(Fx::from_f64(f64::NAN).is_err()); assert!(Fx::from_f64(1e40).is_err()); assert!(ron::from_str::<Fx>("1e40").is_err()); assert_eq!(ron::from_str::<Fx>("40").unwrap(), Fx::from_int(40)); }
#[test] fn sqrt_exact_squares() { assert_eq!(Fx::from_int(144).sqrt(), Fx::from_int(12)); assert_eq!(Fx::from_int(-4).sqrt(), Fx::ZERO); }
#[test] fn unit_fx_in_range() { let mut r = Pcg32::new(1, 1); for _ in 0..10_000 { let u = r.unit_fx(); assert!(u >= Fx::ZERO && u < Fx::ONE); } }
#[test] fn derive_seed_distinct_neighbours() { assert_ne!(derive_seed(1, 0, 1, 0), derive_seed(1, 0, 2, 0)); assert_eq!(derive_seed(9, 3, 4, 5), derive_seed(9, 3, 4, 5)); }
```
  `tests/fx_props.rs` holds the proptests:
  - `add_sub_roundtrip`
  - `mul_matches_f64_within_2eps`, with operands in ±40_000
  - `vec_length_matches_f64`, with components in ±2_000 and relative error < 1e-6
  - `angle_from_vec_roundtrip`: `FxVec2::from_angle(a).angle()` is within 4 units of `a`
- [ ] **Step 2: Run** `cargo test -p fl_math`. Expected: FAIL (items not defined).
- [ ] **Step 3: Implement.**
  - **Sine:** a quarter-wave table of 16 385 `Fx` values, built once in a `std::sync::LazyLock` by a **pure integer** routine: a Taylor series in i128 fixed point with π as an integer constant, and no f64. The other quadrants mirror it.
  - **atan2:** CORDIC in vectoring mode on i64, after reducing to the right half-plane. Use 16 iterations and a constant table of `atan(2^-i)` in binary-angle units: `8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1, 0`.
  - **sqrt:** `u128::isqrt((raw as u128) << 32)`.
- [ ] **Step 4: Run** `cargo test -p fl_math`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_math): Fx fixed-point, vectors, binary angles, trig tables, PCG32`.

