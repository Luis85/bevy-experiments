### Task 5: `fl_math` — keyed counter-based RNG

**Track:** contract · **Refs:** ADR-0003

**Files:**
- Modify: `crates/fl_math/src/{rng,lib,fx,angle}.rs`, `crates/fl_math/tests/fx_props.rs`
- Create: `crates/fl_math/tests/rolls.rs`

**Interfaces:**
- Consumes: the existing private `splitmix64` in `rng.rs`.
- Produces (the output values are a contract: changing them moves every draw):
```rust
pub const fn stream_key(name: &str) -> u64;   // splitmix64(fnv1a64(name bytes)); FNV-1a 64: offset 0xcbf29ce484222325, prime 0x100000001b3
pub fn roll(seed: u64, tick: u32, sim_id: u32, stream: u64, n: u32) -> u64;
//   h = splitmix64(seed); h = splitmix64(h ^ ((tick as u64) << 32 | sim_id as u64)); h = splitmix64(h ^ stream); splitmix64(h ^ n as u64)
#[derive(Clone, Debug)] pub struct Rolls { /* seed, tick, sim_id, stream, n */ }
impl Rolls {
    pub const fn new(seed: u64, tick: u32, sim_id: u32, stream: u64) -> Rolls;   // n = 0
    pub fn next_u64(&mut self) -> u64;                   // roll(.., n), then n += 1
    pub fn next_u32(&mut self) -> u32;                   // high 32 bits of next_u64
    pub fn range_u32(&mut self, lo: u32, hi_excl: u32) -> u32;   // Lemire, the same algorithm as Pcg32::range_u32; each rejection consumes a draw
    pub fn chance(&mut self, num: u32, den: u32) -> bool;        // range_u32(0, den) < num
    pub fn unit_fx(&mut self) -> Fx;                     // Fx::from_raw(next_u32() as i64)
    pub fn range_fx(&mut self, lo: Fx, hi: Fx) -> Fx;    // the same formula as Pcg32::range_fx
    pub fn draws(&self) -> u32;                          // the current n
}
```
- Removes `derive_seed`, which `roll` supersedes. `Pcg32` stays for tools, content time and the client.
- The crate docs change from "the Pcg32 random number generator" to "keyed counter-based RNG (`roll`, `Rolls`), plus `Pcg32` for tools".

- [ ] **Step 1: Write the failing tests** in `tests/rolls.rs`. The reference vectors were computed with an independent Python implementation of the definitions above, so they are an oracle, not a capture:
```rust
#[test] fn stream_key_reference_vectors() {
    assert_eq!(stream_key(""), 0xc381_7c01_6ba4_ff30);
    assert_eq!(stream_key("combat.dispersion"), 0xddc2_43e5_0cb8_47f4);
    assert_eq!(stream_key("debug.wander"), 0xa1cc_dba2_15ca_7a60);
    assert_eq!(stream_key("setup.scale"), 0xd9f9_6374_b859_3636);
}
#[test] fn roll_reference_vectors() {
    assert_eq!(roll(0, 0, 0, 0, 0), 0x2130_748a_aac8_0268);
    assert_eq!(roll(1, 2, 3, stream_key("debug.wander"), 4), 0xe5b4_bc46_9f17_780b);
}
#[test] fn rolls_range_reference() {
    let mut r = Rolls::new(1, 2, 3, stream_key("debug.wander"));
    let got: Vec<u32> = (0..5).map(|_| r.range_u32(0, 10)).collect();
    assert_eq!(got, [1, 0, 7, 3, 8]);
    assert_eq!(r.draws(), 5);
}
#[test] fn each_input_changes_roll()            // bump each of the 5 inputs of roll(9,3,4,5,6) by 1 → 5 values, pairwise distinct and != the base
#[test] fn tick_and_sim_id_not_interchangeable() { assert_ne!(roll(9, 3, 4, 5, 0), roll(9, 4, 3, 5, 0)); }
#[test] fn next_u32_is_high_half() { let mut r = Rolls::new(0, 0, 0, 0); assert_eq!(r.next_u32(), (roll(0, 0, 0, 0, 0) >> 32) as u32); }
#[test] fn unit_fx_and_range_fx_bounds()        // 10_000 draws: 0 ≤ unit < 1; range_fx(-5, 5) in [-5, 5)
#[test] fn range_u32_covers_all_values()        // range_u32(0, 7) over 10_000 draws hits all 7 values
#[test] fn chance_extremes()                    // chance(0, 5) never true; chance(5, 5) always true (100 draws each)
#[test] fn stream_key_is_const() { const K: u64 = stream_key("x"); assert_eq!(K, stream_key("x")); }
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_math`. Expected: FAIL (`roll` not defined).
- [ ] **Step 3: Implement.**
  - Delete `derive_seed` and its tests, and update the `lib.rs` re-exports and docs.
  - Add `#![deny(clippy::float_arithmetic)]` to `lib.rs`.
  - Put a scoped `#[expect(clippy::float_arithmetic, reason = "load-time or presentation conversion")]` only on `Fx::from_f64`, `Fx::to_f32`, `Fx::to_f64`, `Angle::to_radians_f32` and the human-readable serde visitors.
  - Test modules and `tests/*.rs` files that compare against f64 get `#![expect(clippy::float_arithmetic, reason = "f64 oracle")]`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract`. Expected: `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_math)!: keyed counter-based RNG (roll, Rolls, stream_key); drop derive_seed` with trailer `Refs: ADR-0003`.

