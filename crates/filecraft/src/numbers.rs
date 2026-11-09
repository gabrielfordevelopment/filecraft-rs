//! Number formats used in messages.

/// Formats with a space as group separator: `12 345`.
pub fn format_grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    grouped
}

/// Formats with at most two decimals and without trailing zeros.
pub fn format_percent(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands_with_spaces() {
        assert_eq!(format_grouped(0), "0");
        assert_eq!(format_grouped(999), "999");
        assert_eq!(format_grouped(1000), "1 000");
        assert_eq!(format_grouped(1234567), "1 234 567");
    }

    #[test]
    fn formats_percent() {
        assert_eq!(format_percent(0.0), "0");
        assert_eq!(format_percent(12.5), "12.5");
        assert_eq!(format_percent(33.3333), "33.33");
        assert_eq!(format_percent(100.0), "100");
    }
}
