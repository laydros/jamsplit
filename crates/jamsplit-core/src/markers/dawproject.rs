use super::{ParseError, RawMarker};
use std::io::{Cursor, Read};

/// 1-based row of `node` within the parsed `project.xml`, for error messages.
fn row_of(doc: &roxmltree::Document, node: &roxmltree::Node) -> usize {
    doc.text_pos_at(node.range().start).row as usize
}

/// Parse a DAWproject (`.dawproject`) file: a ZIP containing `project.xml`.
/// Reads arrangement cue markers and normalizes their times to seconds.
/// Collects every problem instead of stopping at the first, like the other
/// marker parsers. `ParseError.line` is the row in `project.xml` where the
/// problem element sits (line 1 for container/IO problems).
pub fn parse(bytes: &[u8]) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let xml = match read_project_xml(bytes) {
        Ok(s) => s,
        Err(message) => return Err(vec![ParseError { line: 1, message }]),
    };
    let doc = match roxmltree::Document::parse(&xml) {
        Ok(d) => d,
        Err(e) => {
            return Err(vec![ParseError {
                line: 1,
                message: format!("project.xml is not valid XML: {e}"),
            }])
        }
    };
    parse_document(&doc)
}

/// Open the zip in memory and read the `project.xml` entry to a string.
fn read_project_xml(bytes: &[u8]) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not a readable .dawproject (zip) file: {e}"))?;
    let mut file = archive
        .by_name("project.xml")
        .map_err(|_| "project.xml not found in the .dawproject archive".to_string())?;
    let mut s = String::new();
    file.read_to_string(&mut s)
        .map_err(|e| format!("could not read project.xml: {e}"))?;
    Ok(s)
}

/// Walk Project > Arrangement > Markers, converting each Marker to seconds.
fn parse_document(doc: &roxmltree::Document) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let project = doc.root_element();
    let arrangement = match project.children().find(|n| n.has_tag_name("Arrangement")) {
        Some(a) => a,
        None => {
            return Err(vec![ParseError {
                line: row_of(doc, &project),
                message: "no <Arrangement> element in project.xml".to_string(),
            }])
        }
    };

    // jamsplit requires a single constant tempo. We refuse tempo automation for
    // BOTH time units (not just beats): a project with a varying tempo is out of
    // scope by design, even though seconds-unit marker times are absolute. This
    // is a deliberate blanket refusal, not an oversight.
    if let Some(ta) = arrangement
        .children()
        .find(|n| n.has_tag_name("TempoAutomation"))
    {
        return Err(vec![ParseError {
            line: row_of(doc, &ta),
            message: "tempo automation is not supported; jamsplit needs a single constant tempo"
                .to_string(),
        }]);
    }

    let markers_el = match arrangement.children().find(|n| n.has_tag_name("Markers")) {
        Some(m) => m,
        None => {
            return Err(vec![ParseError {
                line: row_of(doc, &arrangement),
                message: "no <Markers> in the arrangement (nothing to split on)".to_string(),
            }])
        }
    };

    let time_unit = match markers_el.attribute("timeUnit") {
        Some(u) => u,
        None => {
            return Err(vec![ParseError {
                line: row_of(doc, &markers_el),
                message: "<Markers> has no timeUnit attribute; cannot tell beats from seconds"
                    .to_string(),
            }])
        }
    };

    // None => seconds; Some(bpm) => beats converted with this constant tempo.
    let beats_bpm: Option<f64> = match time_unit {
        "seconds" => None,
        "beats" => Some(resolve_bpm(doc, &markers_el)?),
        other => {
            return Err(vec![ParseError {
                line: row_of(doc, &markers_el),
                message: format!("unknown timeUnit {other:?} (expected \"seconds\" or \"beats\")"),
            }])
        }
    };

    let mut markers = Vec::new();
    let mut errors = Vec::new();
    for m in markers_el.children().filter(|n| n.has_tag_name("Marker")) {
        let line = row_of(doc, &m);
        let time = match m.attribute("time").map(str::parse::<f64>) {
            Some(Ok(t)) if t.is_finite() && t >= 0.0 => t,
            Some(Ok(_)) => {
                errors.push(ParseError {
                    line,
                    message: "marker time must be a finite, non-negative number".to_string(),
                });
                continue;
            }
            Some(Err(_)) => {
                errors.push(ParseError {
                    line,
                    message: "marker time is not a number".to_string(),
                });
                continue;
            }
            None => {
                errors.push(ParseError {
                    line,
                    message: "marker has no time attribute".to_string(),
                });
                continue;
            }
        };
        let start_seconds = match beats_bpm {
            None => time,
            Some(bpm) => time * 60.0 / bpm,
        };
        let title = m.attribute("name").unwrap_or("").trim().to_string();
        markers.push(RawMarker {
            start_seconds,
            title,
        });
    }

    if markers.is_empty() && errors.is_empty() {
        errors.push(ParseError {
            line: row_of(doc, &markers_el),
            message: "no <Marker> elements found (nothing to split on)".to_string(),
        });
    }

    if errors.is_empty() {
        Ok(markers)
    } else {
        Err(errors)
    }
}

/// Read and validate `Project > Transport > Tempo`, returning a positive bpm.
/// Used only when markers are in beats. Refuses a missing tempo, a non-`bpm`
/// unit, or a value that is not finite and strictly positive.
fn resolve_bpm(
    doc: &roxmltree::Document,
    markers_el: &roxmltree::Node,
) -> Result<f64, Vec<ParseError>> {
    let project = doc.root_element();
    let tempo = project
        .children()
        .find(|n| n.has_tag_name("Transport"))
        .and_then(|t| t.children().find(|n| n.has_tag_name("Tempo")));
    let tempo = match tempo {
        Some(t) => t,
        None => {
            return Err(vec![ParseError {
                line: row_of(doc, markers_el),
                message: "markers are in beats but the project has no <Transport><Tempo>"
                    .to_string(),
            }])
        }
    };

    match tempo.attribute("unit") {
        Some("bpm") => {}
        other => {
            return Err(vec![ParseError {
                line: row_of(doc, &tempo),
                message: format!(
                    "tempo unit is {:?}, expected \"bpm\"",
                    other.unwrap_or("missing")
                ),
            }])
        }
    }

    match tempo.attribute("value").map(str::parse::<f64>) {
        Some(Ok(v)) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(vec![ParseError {
            line: row_of(doc, &tempo),
            message: "tempo value is missing or not a positive number".to_string(),
        }]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.dawproject` zip in memory holding one entry.
    fn zip_bytes(entry_name: &str, body: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            zw.start_file(entry_name, SimpleFileOptions::default())
                .unwrap();
            zw.write_all(body.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    /// A `.dawproject` whose only entry is `project.xml` with the given body.
    fn dawproject(project_xml: &str) -> Vec<u8> {
        zip_bytes("project.xml", project_xml)
    }

    const SECONDS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project version="1.0">
  <Transport>
    <Tempo unit="bpm" value="120.0"/>
  </Transport>
  <Arrangement>
    <Markers timeUnit="seconds">
      <Marker time="0.0" name="Opening Jam"/>
      <Marker time="323.5" name="Slow Blues"/>
    </Markers>
  </Arrangement>
</Project>"#;

    #[test]
    fn reads_seconds_markers() {
        let got = parse(&dawproject(SECONDS_XML)).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].start_seconds, 0.0);
        assert_eq!(got[0].title, "Opening Jam");
        assert_eq!(got[1].start_seconds, 323.5);
        assert_eq!(got[1].title, "Slow Blues");
    }

    #[test]
    fn missing_name_becomes_empty_title() {
        let xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
          <Marker time="1.0"/>
        </Markers></Arrangement></Project>"#;
        let got = parse(&dawproject(xml)).unwrap();
        assert_eq!(got[0].title, "");
    }

    const BEATS_XML: &str = r#"<Project>
  <Transport><Tempo unit="bpm" value="120.0"/></Transport>
  <Arrangement><Markers timeUnit="beats">
    <Marker time="0" name="One"/>
    <Marker time="4" name="Two"/>
  </Markers></Arrangement>
</Project>"#;

    #[test]
    fn converts_beats_to_seconds_with_tempo() {
        // 120 bpm => 1 beat = 0.5s; beat 4 => 2.0s.
        let got = parse(&dawproject(BEATS_XML)).unwrap();
        assert_eq!(got[0].start_seconds, 0.0);
        assert_eq!(got[1].start_seconds, 2.0);
        assert_eq!(got[1].title, "Two");
    }

    #[test]
    fn beats_without_tempo_is_refused() {
        let xml = r#"<Project><Arrangement><Markers timeUnit="beats">
          <Marker time="4" name="X"/>
        </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("no <Transport><Tempo>"));
    }

    #[test]
    fn non_bpm_tempo_unit_is_refused() {
        let xml = r#"<Project>
          <Transport><Tempo unit="linear" value="0.5"/></Transport>
          <Arrangement><Markers timeUnit="beats">
            <Marker time="4" name="X"/>
          </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert!(
            errs[0].message.contains("tempo unit") && errs[0].message.contains("bpm"),
            "got: {}",
            errs[0].message
        );
    }

    #[test]
    fn non_positive_or_nonfinite_bpm_is_refused() {
        for bad in ["0", "-120", "nan", "inf"] {
            let xml = format!(
                r#"<Project>
                  <Transport><Tempo unit="bpm" value="{bad}"/></Transport>
                  <Arrangement><Markers timeUnit="beats">
                    <Marker time="4" name="X"/>
                  </Markers></Arrangement></Project>"#
            );
            let errs = parse(&dawproject(&xml)).unwrap_err();
            assert!(
                errs[0].message.contains("positive"),
                "value {bad:?} should be refused, got: {}",
                errs[0].message
            );
        }
    }

    #[test]
    fn tempo_automation_is_refused() {
        let xml = r#"<Project>
          <Transport><Tempo unit="bpm" value="120.0"/></Transport>
          <Arrangement>
            <TempoAutomation/>
            <Markers timeUnit="seconds"><Marker time="0.0" name="X"/></Markers>
          </Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert!(errs[0].message.contains("tempo automation"));
    }

    #[test]
    fn missing_time_unit_is_refused() {
        let xml = r#"<Project><Arrangement><Markers>
          <Marker time="0.0" name="X"/>
        </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert!(errs[0].message.contains("timeUnit"));
    }

    #[test]
    fn no_markers_is_an_error() {
        let xml = r#"<Project><Arrangement>
          <Markers timeUnit="seconds"></Markers>
        </Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert!(errs[0].message.contains("no <Marker>"));
    }

    #[test]
    fn no_arrangement_is_an_error() {
        let errs = parse(&dawproject("<Project></Project>")).unwrap_err();
        assert!(errs[0].message.contains("Arrangement"));
    }

    #[test]
    fn missing_markers_element_is_an_error() {
        let errs = parse(&dawproject(
            "<Project><Arrangement></Arrangement></Project>",
        ))
        .unwrap_err();
        assert!(errs[0].message.contains("no <Markers>"));
    }

    #[test]
    fn not_a_zip_is_an_error() {
        let errs = parse(b"this is not a zip").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("zip"));
    }

    #[test]
    fn missing_project_xml_is_an_error() {
        let bytes = zip_bytes("metadata.xml", "<MetaData/>");
        let errs = parse(&bytes).unwrap_err();
        assert!(errs[0].message.contains("project.xml not found"));
    }

    #[test]
    fn malformed_xml_is_an_error() {
        let errs = parse(&dawproject("<Project><oops")).unwrap_err();
        assert!(errs[0].message.contains("valid XML"));
    }

    #[test]
    fn negative_marker_time_is_refused() {
        let xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
          <Marker time="-1.0" name="X"/>
        </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert!(errs[0].message.contains("non-negative"));
    }

    #[test]
    fn multiple_bad_markers_are_all_reported() {
        let xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
          <Marker name="no time"/>
          <Marker time="abc" name="bad"/>
          <Marker time="5.0" name="fine"/>
        </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert_eq!(errs.len(), 2);
    }
}
