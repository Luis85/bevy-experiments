use crate::HistoryError;

/// All time spans are half-open: start is inclusive and end is exclusive.
pub fn valid_at(epoch: i64, from: i64, until: Option<i64>) -> bool {
    epoch >= from && until.is_none_or(|end| epoch < end)
}

pub(crate) fn valid_range(from: i64, until: Option<i64>) -> Result<(), HistoryError> {
    if until.is_some_and(|end| end <= from) {
        return Err(HistoryError::Invalid(
            "time interval must have end > start".into(),
        ));
    }
    // f64 clocks are exact to the second throughout this documented year range.
    if !(-62_135_596_800..=253_402_300_799).contains(&from)
        || until.is_some_and(|v| !(-62_135_596_800..=253_402_300_800).contains(&v))
    {
        return Err(HistoryError::Invalid(
            "supported dates are years 0001 through 9999 UTC".into(),
        ));
    }
    Ok(())
}

/// Parse an explicit UTC date (`YYYY-MM-DD`) or signed Unix-seconds integer.
/// This parser does not guess locale, time zone, or approximate historical dates.
pub fn parse_date(value: &str) -> Result<i64, HistoryError> {
    if let Ok(epoch) = value.parse::<i64>() {
        valid_range(epoch, None)?;
        return Ok(epoch);
    }
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return Err(HistoryError::Invalid(
            "date must be Unix seconds or YYYY-MM-DD (UTC midnight)".into(),
        ));
    }
    let year = value[..4]
        .parse::<i64>()
        .map_err(|e| HistoryError::Invalid(e.to_string()))?;
    let month = value[5..7]
        .parse::<i64>()
        .map_err(|e| HistoryError::Invalid(e.to_string()))?;
    let day = value[8..]
        .parse::<i64>()
        .map_err(|e| HistoryError::Invalid(e.to_string()))?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year < 1 || day < 1 || day > max_day {
        return Err(HistoryError::Invalid("invalid calendar date".into()));
    }
    let adjusted = year - i64::from(month <= 2);
    let era = adjusted.div_euclid(400);
    let year_of_era = adjusted - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Ok((era * 146_097 + day_of_era - 719_468) * 86_400)
}

pub(crate) fn json_date(
    value: Option<&serde_json::Value>,
    fallback: Option<i64>,
) -> Result<Option<i64>, HistoryError> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(fallback),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| HistoryError::Invalid("timestamp must be an integer".into())),
        Some(serde_json::Value::String(text)) => parse_date(text).map(Some),
        _ => Err(HistoryError::Invalid(
            "timestamp must be an integer or explicit UTC date".into(),
        )),
    }
}
