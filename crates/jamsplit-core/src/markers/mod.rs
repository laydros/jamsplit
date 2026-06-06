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
        // only the final component may carry a fraction
        let value: f64 = if is_last {
            part.parse().map_err(|_| format!("'{part}' is not a number in '{s}'"))?
        } else {
            part.parse::<u64>()
                .map_err(|_| format!("'{part}' is not a whole number in '{s}'"))? as f64
        };
        if value < 0.0 || part.starts_with('-') {
            return Err(format!("negative time in '{s}'"));
        }
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
}
