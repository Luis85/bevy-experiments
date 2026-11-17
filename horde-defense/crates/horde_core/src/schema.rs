//! Serde skip predicates and neutral defaults for saved fields (schema v7
//! C1/C2). A new saved field defaults to its legacy-neutral value and is
//! skipped at exactly that value, so old saves and `state_digest` are
//! unchanged. Float predicates compare bits, never with a tolerance.

pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// Bit-exact: `-0.0` is not zero here, so it is still written.
pub(crate) fn is_zero_f32(value: &f32) -> bool {
    value.to_bits() == 0
}

/// Bit-exact: only the literal `1.0` is skipped; `next_up(1.0)` is written.
pub(crate) fn is_one_f32(value: &f32) -> bool {
    value.to_bits() == 1.0f32.to_bits()
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "every saved 1.0 field so far takes it from a container Default"
    )
)]
pub(crate) fn one_f32() -> f32 {
    1.0
}

pub(crate) fn one_u8() -> u8 {
    1
}

/// The skip predicate paired with [`one_u8`].
pub(crate) fn is_one_u8(value: &u8) -> bool {
    *value == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Neutral {
        #[serde(default = "one_f32", skip_serializing_if = "is_one_f32")]
        multiplier: f32,
        #[serde(default, skip_serializing_if = "is_zero_f32")]
        reach: f32,
        #[serde(default, skip_serializing_if = "is_false")]
        flag: bool,
        #[serde(default, skip_serializing_if = "is_default")]
        name: String,
    }
    fn neutral() -> Neutral {
        Neutral {
            multiplier: 1.0,
            ..Neutral::default()
        }
    }

    #[derive(Debug, Deserialize)]
    struct Group {
        #[serde(default = "one_u8")]
        size: u8,
    }

    #[test]
    fn neutral_one_multipliers_are_absent_and_round_trip_bit_exactly() {
        assert_eq!(serde_json::to_string(&neutral()).unwrap(), "{}");
        let legacy: Neutral = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.multiplier.to_bits(), 1.0f32.to_bits());
        assert_eq!(legacy, neutral());
        assert_eq!(serde_json::from_str::<Group>("{}").unwrap().size, 1);
        for value in [1.0f32.next_up(), 1.0f32.next_down(), 0.0, -1.0, 2.5] {
            let saved = Neutral {
                multiplier: value,
                ..neutral()
            };
            let json = serde_json::to_string(&saved).unwrap();
            assert!(json.contains("multiplier"), "{value:e} skipped: {json}");
            let loaded: Neutral = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded.multiplier.to_bits(), value.to_bits(), "{json}");
        }
        let marked = Neutral {
            flag: true,
            name: "x".into(),
            ..neutral()
        };
        let json = serde_json::to_string(&marked).unwrap();
        assert_eq!(json, r#"{"flag":true,"name":"x"}"#);
        assert_eq!(serde_json::from_str::<Neutral>(&json).unwrap(), marked);
    }

    #[test]
    fn negative_zero_is_serialized_by_is_zero_f32() {
        assert!(is_zero_f32(&0.0));
        assert!(!is_zero_f32(&-0.0));
        let saved = Neutral {
            reach: -0.0,
            ..neutral()
        };
        let json = serde_json::to_string(&saved).unwrap();
        assert_eq!(json, r#"{"reach":-0.0}"#);
        let loaded: Neutral = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.reach.to_bits(), (-0.0f32).to_bits());
    }
}
