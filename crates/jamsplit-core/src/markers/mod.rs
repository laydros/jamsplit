pub mod audacity;
pub mod dawproject;
pub mod plain;
pub mod reaper;

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
            part.parse()
                .map_err(|_| format!("'{part}' is not a number in '{s}'"))?
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

/// Which marker format a file is in. `FromStr` accepts the CLI names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerFormat {
    Audacity,
    Plain,
    Reaper,
    Dawproject,
}

impl std::str::FromStr for MarkerFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "audacity" => Ok(Self::Audacity),
            "plain" => Ok(Self::Plain),
            "reaper" => Ok(Self::Reaper),
            "dawproject" => Ok(Self::Dawproject),
            other => Err(format!(
                "unknown format '{other}' (expected audacity, plain, reaper, or dawproject)"
            )),
        }
    }
}

impl std::fmt::Display for MarkerFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Audacity => "audacity",
            Self::Plain => "plain",
            Self::Reaper => "reaper",
            Self::Dawproject => "dawproject",
        })
    }
}

/// Markers plus the format they were read as (so frontends can announce it).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMarkers {
    pub markers: Vec<RawMarker>,
    pub format: MarkerFormat,
}

/// Detect the marker format. Order is a deliberate tiebreak — every Audacity
/// file also parses as plain, so the strict shape is tested first.
pub fn detect_format(content: &str) -> MarkerFormat {
    // Audacity: every non-blank, non-spectral line is float TAB float [TAB ...]
    let mut saw_audacity_line = false;
    let all_audacity = content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('\\'))
        .all(|l| {
            let fields: Vec<&str> = l.split('\t').collect();
            let ok = fields.len() >= 2
                && fields[0].trim().parse::<f64>().is_ok()
                && fields[1].trim().parse::<f64>().is_ok();
            saw_audacity_line |= ok;
            ok
        });
    if all_audacity && saw_audacity_line {
        return MarkerFormat::Audacity;
    }
    // Reaper: header row signature
    if let Some(first) = content.lines().find(|l| !l.trim().is_empty()) {
        if first
            .trim()
            .to_ascii_lowercase()
            .starts_with("#,name,start")
        {
            return MarkerFormat::Reaper;
        }
    }
    MarkerFormat::Plain
}

/// Parse markers, auto-detecting the format unless one is forced. A leading UTF-8 BOM is stripped.
pub fn parse_markers(
    content: &str,
    format: Option<MarkerFormat>,
) -> Result<ParsedMarkers, Vec<ParseError>> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let format = format.unwrap_or_else(|| detect_format(content));
    let markers = match format {
        MarkerFormat::Audacity => audacity::parse(content)?,
        MarkerFormat::Plain => plain::parse(content)?,
        MarkerFormat::Reaper => reaper::parse(content)?,
        // DAWproject is a binary (zip) format; `parse_markers_bytes` routes it to
        // `dawproject::parse` before reaching here. Reaching this arm means a text
        // parse was requested for a binary format — report it, never panic.
        MarkerFormat::Dawproject => {
            return Err(vec![ParseError {
                line: 1,
                message: "dawproject is a binary format; read it with parse_markers_bytes"
                    .to_string(),
            }])
        }
    };
    Ok(ParsedMarkers { markers, format })
}

/// Parse markers from the raw bytes of a marker file. A `.dawproject` (ZIP)
/// is routed to the DAWproject reader; everything else is decoded as UTF-8 and
/// handled by the existing text parsers. Auto-detection uses the ZIP magic
/// number; `Some(MarkerFormat::Dawproject)` forces the DAWproject path.
pub fn parse_markers_bytes(
    bytes: &[u8],
    format: Option<MarkerFormat>,
) -> Result<ParsedMarkers, Vec<ParseError>> {
    let is_zip = bytes.starts_with(b"PK\x03\x04");
    if format == Some(MarkerFormat::Dawproject) || (format.is_none() && is_zip) {
        let markers = dawproject::parse(bytes)?;
        return Ok(ParsedMarkers {
            markers,
            format: MarkerFormat::Dawproject,
        });
    }
    let content = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return Err(vec![ParseError {
                line: 1,
                message: "marker file is not valid UTF-8 text (and not a .dawproject file)"
                    .to_string(),
            }])
        }
    };
    parse_markers(content, format)
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

    #[test]
    fn detects_audacity_shape() {
        assert_eq!(
            detect_format("1.0\t1.0\tIntro\n2.0\t3.0\n"),
            MarkerFormat::Audacity
        );
    }

    #[test]
    fn detection_skips_spectral_lines_like_the_parser_does() {
        let input = "1.0\t1.0\tChorus\n\\\t440.0\t880.0\n2.0\t2.0\tOutro\n";
        assert_eq!(detect_format(input), MarkerFormat::Audacity);
    }

    #[test]
    fn detects_reaper_header() {
        assert_eq!(
            detect_format("#,Name,Start,End,Length\nM1,Song,0:00,,\n"),
            MarkerFormat::Reaper
        );
    }

    #[test]
    fn falls_back_to_plain() {
        assert_eq!(
            detect_format("0:00 Opening Jam\n5:23 Slow Blues\n"),
            MarkerFormat::Plain
        );
        // mixed shapes are not Audacity
        assert_eq!(detect_format("1.0\t2.0\tA\n0:00 B\n"), MarkerFormat::Plain);
        assert_eq!(detect_format(""), MarkerFormat::Plain);
    }

    #[test]
    fn forced_format_skips_detection() {
        // looks like Audacity, but we force plain: first float is the time,
        // rest of line (including the tab) is the title
        let got = parse_markers("1.5\t2.5\tA\n", Some(MarkerFormat::Plain)).unwrap();
        assert_eq!(got.format, MarkerFormat::Plain);
        assert_eq!(got.markers[0].title, "2.5\tA");
    }

    #[test]
    fn parse_markers_reports_detected_format() {
        let got = parse_markers("0:00 One\n", None).unwrap();
        assert_eq!(got.format, MarkerFormat::Plain);
        assert_eq!(got.markers.len(), 1);
    }

    #[test]
    fn dawproject_format_roundtrips_name() {
        assert_eq!(
            "dawproject".parse::<MarkerFormat>(),
            Ok(MarkerFormat::Dawproject)
        );
        assert_eq!(MarkerFormat::Dawproject.to_string(), "dawproject");
    }

    #[test]
    fn utf8_bom_is_stripped_before_detection_and_parsing() {
        let got = parse_markers(
            "\u{feff}#,Name,Start,End,Length\nM1,Song,0:00.000,,\n",
            None,
        )
        .unwrap();
        assert_eq!(got.format, MarkerFormat::Reaper);
        assert_eq!(got.markers.len(), 1);

        let got = parse_markers("\u{feff}0:00 One\n", None).unwrap();
        assert_eq!(got.format, MarkerFormat::Plain);
        assert_eq!(got.markers[0].title, "One");
    }

    /// Minimal valid `.dawproject` (seconds) for routing tests.
    fn dawproject_seconds_bytes() -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
          <Marker time="0.0" name="One"/>
        </Markers></Arrangement></Project>"#;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zw.start_file("project.xml", SimpleFileOptions::default())
                .unwrap();
            zw.write_all(xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn bytes_router_autodetects_dawproject_by_zip_magic() {
        let parsed = parse_markers_bytes(&dawproject_seconds_bytes(), None).unwrap();
        assert_eq!(parsed.format, MarkerFormat::Dawproject);
        assert_eq!(parsed.markers.len(), 1);
    }

    #[test]
    fn bytes_router_handles_text_like_the_string_path() {
        let parsed = parse_markers_bytes(b"0:00 One\n", None).unwrap();
        assert_eq!(parsed.format, MarkerFormat::Plain);
        assert_eq!(parsed.markers[0].title, "One");
    }

    #[test]
    fn bytes_router_forced_dawproject_on_text_errors_cleanly() {
        let errs = parse_markers_bytes(b"0:00 One\n", Some(MarkerFormat::Dawproject)).unwrap_err();
        // text bytes forced as dawproject fail in the zip reader, not silently
        assert!(
            errs[0].message.contains("zip"),
            "got: {}",
            errs[0].message
        );
    }

    #[test]
    fn bytes_router_rejects_non_utf8_text() {
        // 0xFF 0xFE is neither zip magic nor valid UTF-8.
        let errs = parse_markers_bytes(&[0xFF, 0xFE, 0x00], None).unwrap_err();
        assert!(errs[0].message.contains("UTF-8"));
    }
}
