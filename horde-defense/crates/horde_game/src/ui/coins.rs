//! Coin amounts as every screen prints them: the digits grouped by
//! thousands, so a price, a wallet, a reward and a sale read alike. The one
//! table is core's (`horde_core::coins::grouped`), so the reasons and
//! requirement rows core writes group the same way.

/// 1077 as "1,077".
pub(super) use horde_core::coins::grouped;

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
