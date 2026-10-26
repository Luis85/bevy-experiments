use fl_math::{Fx, Rolls, roll, stream_key};

#[test]
fn stream_key_reference_vectors() {
    assert_eq!(stream_key(""), 0xc381_7c01_6ba4_ff30);
    assert_eq!(stream_key("combat.dispersion"), 0xddc2_43e5_0cb8_47f4);
    assert_eq!(stream_key("debug.wander"), 0xa1cc_dba2_15ca_7a60);
    assert_eq!(stream_key("setup.scale"), 0xd9f9_6374_b859_3636);
}

#[test]
fn roll_reference_vectors() {
    assert_eq!(roll(0, 0, 0, 0, 0), 0x2130_748a_aac8_0268);
    assert_eq!(
        roll(1, 2, 3, stream_key("debug.wander"), 4),
        0xe5b4_bc46_9f17_780b
    );
}

#[test]
fn rolls_range_reference() {
    let mut r = Rolls::new(1, 2, 3, stream_key("debug.wander"));
    let got: Vec<u32> = (0..5).map(|_| r.range_u32(0, 10)).collect();
    assert_eq!(got, [1, 0, 7, 3, 8]);
    assert_eq!(r.draws(), 5);
}

#[test]
fn each_input_changes_roll() {
    let vals = [
        roll(9, 3, 4, 5, 6),
        roll(10, 3, 4, 5, 6),
        roll(9, 4, 4, 5, 6),
        roll(9, 3, 5, 5, 6),
        roll(9, 3, 4, 6, 6),
        roll(9, 3, 4, 5, 7),
    ];
    for i in 0..vals.len() {
        for j in (i + 1)..vals.len() {
            assert_ne!(vals[i], vals[j], "{i} vs {j}");
        }
    }
}

#[test]
fn tick_and_sim_id_not_interchangeable() {
    assert_ne!(roll(9, 3, 4, 5, 0), roll(9, 4, 3, 5, 0));
}

#[test]
fn next_u32_is_high_half() {
    let mut r = Rolls::new(0, 0, 0, 0);
    assert_eq!(r.next_u32(), (roll(0, 0, 0, 0, 0) >> 32) as u32);
}

#[test]
fn unit_fx_and_range_fx_bounds() {
    let mut r = Rolls::new(4, 5, 6, stream_key("t"));
    let (lo, hi) = (Fx::from_int(-5), Fx::from_int(5));
    for _ in 0..10_000 {
        let u = r.unit_fx();
        assert!(u >= Fx::ZERO && u < Fx::ONE);
        let v = r.range_fx(lo, hi);
        assert!(v >= lo && v < hi);
    }
}

#[test]
fn range_u32_covers_all_values() {
    let mut r = Rolls::new(7, 1, 1, stream_key("cover"));
    let mut seen = [false; 7];
    for _ in 0..10_000 {
        seen[r.range_u32(0, 7) as usize] = true;
    }
    assert!(seen.iter().all(|&s| s));
}

#[test]
fn chance_extremes() {
    let mut r = Rolls::new(2, 2, 2, stream_key("chance"));
    for _ in 0..100 {
        assert!(!r.chance(0, 5));
        assert!(r.chance(5, 5));
    }
}

#[test]
fn stream_key_is_const() {
    const K: u64 = stream_key("x");
    assert_eq!(K, stream_key("x"));
}

#[test]
fn lemire_rejection_consumes_extra_draws() {
    let mut r = Rolls::new(1, 2, 3, stream_key("debug.wander"));
    assert_eq!(r.range_u32(0, 3_000_000_000), 4_212_446);
    for _ in 0..19 {
        r.range_u32(0, 3_000_000_000);
    }
    assert_eq!(r.draws(), 27);
}
