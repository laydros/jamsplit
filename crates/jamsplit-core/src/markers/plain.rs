use super::{ParseError, RawMarker};

/// Parse the hand-written plain format. Collects all errors instead of
/// stopping at the first.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let mut markers = Vec::new();
    let mut errors = Vec::new();
    for (i, raw_line) in content.lines().enumerate() {
        let line_no = i + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // first whitespace-delimited token is the time; the rest is the title
        let (time_str, rest) = match line.split_once(|c: char| c.is_whitespace()) {
            Some((t, r)) => (t, r),
            None => (line, ""),
        };
        match super::parse_timestamp(time_str) {
            Ok(start_seconds) => {
                let mut title = rest.trim();
                // a leading "- " is a separator, stripped exactly once
                if let Some(stripped) = title.strip_prefix("- ") {
                    title = stripped.trim_start();
                } else if title == "-" {
                    title = "";
                }
                markers.push(RawMarker {
                    start_seconds,
                    title: title.to_string(),
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

    fn marker(start: f64, title: &str) -> RawMarker {
        RawMarker {
            start_seconds: start,
            title: title.to_string(),
        }
    }

    #[test]
    fn parses_all_separator_styles() {
        let input = "0:00 Opening Jam\n05:23 - Slow Blues\n1:02:11\tCloser\n3722.5 Encore Noodle\n";
        let got = parse(input).unwrap();
        assert_eq!(
            got,
            vec![
                marker(0.0, "Opening Jam"),
                marker(323.0, "Slow Blues"),
                marker(3731.0, "Closer"),
                marker(3722.5, "Encore Noodle"),
            ]
        );
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let input = "# session 3\n\n0:00 One\n\n# mid comment\n1:00 Two\n";
        let got = parse(input).unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn title_may_be_empty() {
        let got = parse("0:00\n1:00 Two\n").unwrap();
        assert_eq!(got[0], marker(0.0, ""));
    }

    #[test]
    fn dash_separator_is_stripped_only_once() {
        let got = parse("0:00 - - Dashes\n").unwrap();
        assert_eq!(got[0].title, "- Dashes");
    }

    #[test]
    fn collects_all_errors_with_line_numbers() {
        let input = "0:00 Fine\nnot-a-time Song\n2:00 Fine\n5:75 Bad Seconds\n";
        let errs = parse(input).unwrap_err();
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].line, 2);
        assert_eq!(errs[1].line, 4);
    }

    #[test]
    fn titles_keep_internal_punctuation() {
        let got = parse("0:00 AC/DC Jam (take 2)\n").unwrap();
        assert_eq!(got[0].title, "AC/DC Jam (take 2)");
    }
}
