use super::{ParseError, RawMarker};

/// Parse an Audacity label export (File -> Export Labels). Tab-separated
/// `start end label`; spectral lines starting with `\` are skipped.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let mut markers = Vec::new();
    let mut errors = Vec::new();
    for (i, raw_line) in content.lines().enumerate() {
        let line_no = i + 1;
        if raw_line.trim().is_empty() || raw_line.starts_with('\\') {
            continue;
        }
        let fields: Vec<&str> = raw_line.split('\t').collect();
        if fields.len() < 2 {
            errors.push(ParseError {
                line: line_no,
                message: "expected at least 'start<TAB>end'".to_string(),
            });
            continue;
        }
        let start: Result<f64, _> = fields[0].trim().parse();
        let end: Result<f64, _> = fields[1].trim().parse();
        match (start, end) {
            (Ok(start_seconds), Ok(_end_ignored)) => {
                // labels cannot contain tabs, but join defensively
                let title = fields.get(2..).map(|f| f.join("\t")).unwrap_or_default();
                markers.push(RawMarker { start_seconds, title: title.trim().to_string() });
            }
            _ => errors.push(ParseError {
                line: line_no,
                message: format!("'{}' is not 'seconds<TAB>seconds[<TAB>label]'", raw_line),
            }),
        }
    }
    if errors.is_empty() { Ok(markers) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_labels_using_start_only() {
        let input = "0.000000\t0.000000\tOpening Jam\n323.500000\t410.000000\tSlow Blues\n";
        let got = parse(input).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].start_seconds, 0.0);
        assert_eq!(got[0].title, "Opening Jam");
        // range label: end (410.0) ignored, start used
        assert_eq!(got[1].start_seconds, 323.5);
    }

    #[test]
    fn skips_spectral_frequency_lines() {
        let input = "10.0\t10.0\tChorus\n\\\t440.000000\t880.000000\n20.0\t20.0\tOutro\n";
        let got = parse(input).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].title, "Outro");
    }

    #[test]
    fn label_may_be_empty_or_missing() {
        let got = parse("5.0\t5.0\t\n7.0\t7.0\n").unwrap();
        assert_eq!(got[0].title, "");
        assert_eq!(got[1].title, "");
    }

    #[test]
    fn label_containing_tab_is_preserved() {
        let got = parse("5.0\t5.0\tA\tB\n").unwrap();
        assert_eq!(got[0].title, "A\tB");
    }

    #[test]
    fn collects_errors_with_line_numbers() {
        let input = "5.0\t5.0\tFine\nnot\tnumbers\there\n9.0\n";
        let errs = parse(input).unwrap_err();
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].line, 2); // non-numeric start/end
        assert_eq!(errs[1].line, 3); // only one field
    }
}
