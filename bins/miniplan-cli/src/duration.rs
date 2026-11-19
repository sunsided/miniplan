//! Parsing of human-written durations such as `30s`, `2m` or `1h30m`.
//!
//! Replaces the unmaintained `humantime` crate for the CLI `--timeout` flag and the TUI
//! timeout field.

use std::time::Duration;

/// Parses a duration made of one or more `<integer><unit>` parts, e.g. `500ms`, `1h 30m`.
///
/// Units are `ms`, `s`, `m` and `h`. Whitespace between parts is allowed.
///
/// # Errors
///
/// Returns a message when the input is empty, a part lacks its number or unit, the unit is
/// unknown, or the total overflows [`Duration`].
pub fn parse_duration(input: &str) -> Result<Duration, String> {
    let mut rest = input.trim();
    if rest.is_empty() {
        return Err("empty duration".to_owned());
    }

    let mut total = Duration::ZERO;
    while !rest.is_empty() {
        let (part, tail) = parse_part(rest)?;
        total = total
            .checked_add(part)
            .ok_or_else(|| format!("duration too large: {input}"))?;
        rest = tail.trim_start();
    }

    Ok(total)
}

/// Parses one leading `<integer><unit>` part and returns it with the unparsed remainder.
fn parse_part(input: &str) -> Result<(Duration, &str), String> {
    let digits_end = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    let (digits, after_digits) = input.split_at(digits_end);
    if digits.is_empty() {
        return Err(format!("expected a number at '{input}'"));
    }

    let unit_end = after_digits
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_digits.len());
    let (unit, tail) = after_digits.split_at(unit_end);
    let value: u64 = digits
        .parse()
        .map_err(|_| format!("number too large: {digits}"))?;

    Ok((unit_duration(value, unit)?, tail))
}

/// Converts `value` in the given `unit` (`ms`, `s`, `m`, `h`) into a [`Duration`].
fn unit_duration(value: u64, unit: &str) -> Result<Duration, String> {
    let seconds_per_unit = match unit {
        "ms" => return Ok(Duration::from_millis(value)),
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "" => return Err(format!("missing unit after '{value}' (use ms, s, m or h)")),
        other => return Err(format!("unknown unit '{other}' (use ms, s, m or h)")),
    };

    value
        .checked_mul(seconds_per_unit)
        .map(Duration::from_secs)
        .ok_or_else(|| format!("duration too large: {value}{unit}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Single parts in every supported unit.
    #[test]
    fn parses_single_units() {
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("2m"), Ok(Duration::from_secs(120)));
        assert_eq!(parse_duration("1h"), Ok(Duration::from_secs(3600)));
    }

    /// Several parts add up, with or without whitespace between them.
    #[test]
    fn parses_combined_parts() {
        assert_eq!(parse_duration("1h30m"), Ok(Duration::from_secs(5400)));
        assert_eq!(
            parse_duration(" 1m 5s 10ms "),
            Ok(Duration::from_millis(65_010))
        );
    }

    /// Malformed input is rejected instead of guessed.
    #[test]
    fn rejects_malformed_input() {
        for bad in ["", "  ", "30", "s", "5x", "1.5s", "-1s", "99999999999999999999s"] {
            assert!(parse_duration(bad).is_err(), "{bad:?} should fail");
        }
    }
}
