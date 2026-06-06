pub mod audacity;
pub mod plain;

/// One normalized marker: where a song starts and what it is called.
#[derive(Debug, Clone, PartialEq)]
pub struct RawMarker {
    pub start_seconds: f64,
    pub title: String,
}

/// A parse problem tied to a 1-based line number in the marker file.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Validate that a timestamp component contains only ASCII digits, with the
/// last component optionally having the form `digits.digits` (one dot, digits
/// on both sides). Rejects empty strings, leading `+`/`-`, exponent notation,
/// bare-dot forms like `.5` or `5.`, and non-finite spellings like `nan`/`inf`.
fn valid_component_shape(part: &str, is_last: bool) -> bool {
    if part.is_empty() {
        return false;
    }
    if is_last {
        // Allow either all-digits or digits.digits (one dot, digits both sides)
        match part.split_once('.') {
            None => part.chars().all(|c| c.is_ascii_digit()),
            Some((int, frac)) => {
                !int.is_empty()
                    && !frac.is_empty()
                    && int.chars().all(|c| c.is_ascii_digit())
                    && frac.chars().all(|c| c.is_ascii_digit())
            }
        }
    } else {
        part.chars().all(|c| c.is_ascii_digit())
    }
}

/// Parse a timestamp in one of three forms decided by colon count:
/// `3722.5` (raw seconds), `62:11` (M:SS, leading component unbounded),
/// `1:02:11.5` (H:MM:SS). Components after the first must be < 60.
pub fn parse_timestamp(s: &str) -> Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty timestamp".to_string());
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 {
        return Err(format!("'{s}' has too many ':' components"));
    }
    let mut total = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;
        if !valid_component_shape(part, is_last) {
            return Err(format!("'{part}' is not a valid component in '{s}'"));
        }
        // only the final component may carry a fraction
        let value: f64 = if is_last {
            part.parse().map_err(|_| format!("'{part}' is not a number in '{s}'"))?
        } else {
            part.parse::<u64>()
                .map_err(|_| format!("'{part}' is not a whole number in '{s}'"))? as f64
        };
        // components after the first must be < 60
        if i > 0 && value >= 60.0 {
            return Err(format!("'{part}' must be below 60 in '{s}'"));
        }
        total = total * 60.0 + value;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_seconds() {
        assert_eq!(parse_timestamp("90"), Ok(90.0));
        assert_eq!(parse_timestamp("3722.5"), Ok(3722.5));
        assert_eq!(parse_timestamp("0"), Ok(0.0));
    }

    #[test]
    fn minutes_seconds() {
        assert_eq!(parse_timestamp("5:23"), Ok(323.0));
        assert_eq!(parse_timestamp("0:00"), Ok(0.0));
        assert_eq!(parse_timestamp("05:23.5"), Ok(323.5));
        // leading component unbounded — YouTube-style long times
        assert_eq!(parse_timestamp("62:11"), Ok(3731.0));
    }

    #[test]
    fn hours_minutes_seconds() {
        assert_eq!(parse_timestamp("1:02:11"), Ok(3731.0));
        assert_eq!(parse_timestamp("1:02:11.25"), Ok(3731.25));
        assert_eq!(parse_timestamp("10:00:00"), Ok(36000.0));
    }

    #[test]
    fn rejects_components_of_60_or_more_after_the_first() {
        assert!(parse_timestamp("5:75").is_err());
        assert!(parse_timestamp("1:75:00").is_err());
        assert!(parse_timestamp("1:02:60").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_timestamp("").is_err());
        assert!(parse_timestamp("abc").is_err());
        assert!(parse_timestamp("-5").is_err());
        assert!(parse_timestamp("1:2:3:4").is_err());
        assert!(parse_timestamp("9.1.00").is_err()); // bars.beats shape, not a time
        assert!(parse_timestamp("5:").is_err());
        assert!(parse_timestamp(":30").is_err());
    }

    #[test]
    fn rejects_nonfinite_and_exponent_forms() {
        // non-finite float spellings
        assert!(parse_timestamp("nan").is_err());
        assert!(parse_timestamp("NaN").is_err());
        assert!(parse_timestamp("inf").is_err());
        assert!(parse_timestamp("infinity").is_err());
        assert!(parse_timestamp("INFINITY").is_err());
        // leading '+' on any component
        assert!(parse_timestamp("+5").is_err());
        assert!(parse_timestamp("+1:30").is_err());
        assert!(parse_timestamp("1:+2:30").is_err());
        // exponent notation
        assert!(parse_timestamp("1e3").is_err());
        // bare-dot fraction forms
        assert!(parse_timestamp(".5").is_err());
        assert!(parse_timestamp("5.").is_err());
    }
}
