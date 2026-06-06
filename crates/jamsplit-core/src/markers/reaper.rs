use super::{ParseError, RawMarker};

fn looks_like_bars_beats(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Parse a Reaper Region/Marker Manager CSV export.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(content.as_bytes());
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            return Err(vec![ParseError {
                line: 1,
                message: format!("not a CSV file: {e}"),
            }])
        }
    };
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let (id_col, name_col, start_col) = match (col("#"), col("Name"), col("Start")) {
        (Some(i), Some(n), Some(s)) => (i, n, s),
        _ => {
            return Err(vec![ParseError {
                line: 1,
                message: "missing required columns '#', 'Name', 'Start' — is this a Reaper Region/Marker Manager export?".to_string(),
            }])
        }
    };

    let mut markers = Vec::new();
    let mut errors = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line_no = i + 2; // 1-based, after the header line
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors.push(ParseError {
                    line: line_no,
                    message: format!("bad CSV row: {e}"),
                });
                continue;
            }
        };
        let id = record.get(id_col).unwrap_or("").trim();
        // case-sensitive: Reaper always emits uppercase M/R ids
        if !(id.starts_with('M') || id.starts_with('R')) {
            continue; // not a marker/region row
        }
        let start_str = record.get(start_col).unwrap_or("").trim();
        let start = if looks_like_bars_beats(start_str) {
            Err(format!(
                "'{start_str}' looks like bars.beats — set Reaper's time unit to Minutes:Seconds and re-export"
            ))
        } else {
            super::parse_timestamp(start_str)
        };
        match start {
            Ok(start_seconds) => {
                let title = record.get(name_col).unwrap_or("").trim().to_string();
                markers.push(RawMarker {
                    start_seconds,
                    title,
                });
            }
            Err(message) => errors.push(ParseError {
                line: line_no,
                message,
            }),
        }
    }
    if errors.is_empty() {
        Ok(markers)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markers_and_regions() {
        let input = "\
#,Name,Start,End,Length
M1,Opening Jam,0:00.000,,
R1,Slow Blues,5:23.500,9:00.000,3:36.500
M2,Closer,1:02:11.000,,
";
        let got = parse(input).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].title, "Opening Jam");
        assert_eq!(got[1].start_seconds, 323.5); // region start used, end ignored
        assert_eq!(got[2].start_seconds, 3731.0);
    }

    #[test]
    fn handles_quoted_names_with_commas_and_extra_columns() {
        let input = "\
#,Name,Start,End,Length,Color
M1,\"Slow, Heavy Jam\",1:00.000,,,#FF0000
";
        let got = parse(input).unwrap();
        assert_eq!(got[0].title, "Slow, Heavy Jam");
    }

    #[test]
    fn empty_name_is_allowed() {
        let got = parse("#,Name,Start,End,Length\nM1,,2:00.000,,\n").unwrap();
        assert_eq!(got[0].title, "");
    }

    #[test]
    fn bars_beats_start_gets_reexport_message() {
        let errs = parse("#,Name,Start,End,Length\nM1,Song,9.1.00,,\n").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].message.contains("Minutes:Seconds"),
            "got: {}",
            errs[0].message
        );
    }

    #[test]
    fn missing_required_columns_is_one_error() {
        let errs = parse("Name,Position\nIntro,0:00\n").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 1);
    }

    #[test]
    fn non_marker_rows_are_silently_skipped() {
        let input = "#,Name,Start,End,Length\nT1,Tempo,1:00.000,,\nM1,Song,2:00.000,,\n";
        let got = parse(input).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].title, "Song");
    }

    #[test]
    fn malformed_csv_row_is_a_collected_error() {
        // The csv crate treats an unterminated quote by consuming the rest of the
        // input into a single oversized record.  The Start field comes up empty,
        // so the error surfaces as "empty timestamp" rather than a CSV-level error.
        // Either way the call must return Err with at least one ParseError.
        let input = "#,Name,Start,End,Length\nM1,\"unterminated,1:00.000\nM2,Song,2:00.000,,\n";
        let errs = parse(input).unwrap_err();
        assert!(!errs.is_empty(), "expected at least one error, got none");
    }
}
