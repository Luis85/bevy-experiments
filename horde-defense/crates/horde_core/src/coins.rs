//! Coin amounts as player text prints them: the digits grouped by
//! thousands, so a price, a wallet, a reward and a sale read alike in core's
//! reasons and on every screen (`ui/coins.rs` re-exports [`grouped`]).

/// 1077 as "1,077".
pub fn grouped(amount: u64) -> String {
    let digits = amount.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

/// The shortfall reason of a purchase: "Need 1,200 more coins".
pub fn need_more(shortfall: u64) -> String {
    format!("Need {} more coins", grouped(shortfall))
}

#[cfg(test)]
mod tests {
    use super::{grouped, need_more};

    #[test]
    fn the_shortfall_reason_is_grouped() {
        assert_eq!(need_more(15), "Need 15 more coins");
        assert_eq!(need_more(1_200), "Need 1,200 more coins");
    }

    #[test]
    fn amounts_group_by_thousands() {
        for (amount, text) in [
            (0, "0"),
            (7, "7"),
            (999, "999"),
            (1_000, "1,000"),
            (1_077, "1,077"),
            (25_000, "25,000"),
            (999_999, "999,999"),
            (1_234_567, "1,234,567"),
            (u64::MAX, "18,446,744,073,709,551,615"),
        ] {
            assert_eq!(grouped(amount), text, "{amount}");
        }
    }
}
