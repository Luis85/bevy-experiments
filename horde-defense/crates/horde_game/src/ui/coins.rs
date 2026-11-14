//! Coin amounts as every screen prints them: the digits grouped by
//! thousands, so a price, a wallet, a reward and a sale read alike.

/// 1077 as "1,077".
pub(super) fn grouped(amount: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::grouped;

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
        ] {
            assert_eq!(grouped(amount), text, "{amount}");
        }
    }
}
