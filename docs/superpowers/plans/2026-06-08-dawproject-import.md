# DAWproject Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let jamsplit read song-start markers from a Bitwig `.dawproject` file, so a Bitwig user can split a jam recording without hand-writing a marker file.

**Architecture:** A new dumb parser `markers/dawproject.rs` in `jamsplit-core` opens the `.dawproject` zip in memory, reads `project.xml`, and emits the same `Vec<RawMarker>` the other parsers do. A new bytes-aware entry point `parse_markers_bytes` auto-routes `.dawproject` files (by zip magic) to that parser and everything else to the existing text parsers. The CLI and GUI switch from reading the marker file as a UTF-8 string to reading raw bytes and calling `parse_markers_bytes`. All business rules (sorting, bounds, untitled naming) stay in `plan()`, unchanged.

**Tech Stack:** Rust 2021 workspace. New deps in `jamsplit-core`: `zip` (deflate-only, pure Rust) and `roxmltree`. ffmpeg/ffprobe are external subprocesses (unchanged).

**Source of truth:** `docs/superpowers/specs/2026-06-08-dawproject-import-design.md` (design, binding). If this plan and the design doc conflict, the design doc wins — stop and flag it.

**Decided behavior (from the design doc):**
- Convert marker times to seconds. `timeUnit="seconds"` → use directly. `timeUnit="beats"` → `seconds = time * 60 / bpm` using the single project tempo.
- Refuse (clear error, never silent) when: tempo automation is present; `timeUnit` is absent; markers are in beats but there is no `Transport/Tempo`; the tempo unit is not `bpm`; the tempo value is not finite-and-positive.
- Markers only — audio embedded in / referenced by the project is ignored.
- All problems are collected and reported together (never die-on-first), like the existing parsers.

---

## File Structure

- Create: `crates/jamsplit-core/src/markers/dawproject.rs` — the DAWproject reader (`parse(&[u8]) -> Result<Vec<RawMarker>, Vec<ParseError>>`). One responsibility: turn `.dawproject` bytes into normalized markers.
- Modify: `crates/jamsplit-core/src/markers/mod.rs` — add `pub mod dawproject;`, the `MarkerFormat::Dawproject` variant (FromStr/Display), and the `parse_markers_bytes` router.
- Modify: `crates/jamsplit-core/Cargo.toml` — add `zip` + `roxmltree`.
- Modify: `crates/jamsplit-cli/src/cli.rs` — `FormatArg::Dawproject`, read bytes + call `parse_markers_bytes`, help text.
- Modify: `crates/jamsplit-cli/Cargo.toml` + `crates/jamsplit-cli/tests/common/mod.rs` + `crates/jamsplit-cli/tests/cli_integration.rs` — dawproject fixture helper + end-to-end test; update the help-lists-formats test.
- Modify: `crates/jamsplit-gui/src/state.rs` — `FormatChoice::Dawproject` (+ `ALL`, `label`, `into_marker_format`).
- Modify: `crates/jamsplit-gui/src/worker.rs` — read bytes + call `parse_markers_bytes`.
- Modify: `crates/jamsplit-gui/Cargo.toml` + `crates/jamsplit-gui/tests/common/mod.rs` + `crates/jamsplit-gui/tests/worker_integration.rs` — dawproject fixture helper + end-to-end `run_preview` test.
- Modify (docs): `MARKERS.md`, `README.md`, `index.html`, `docs/superpowers/specs/2026-06-05-jamsplit-design.md`, `CLAUDE.md`, `AGENTS.md`.

`crates/jamsplit-gui/src/app.rs` needs **no** change — its format dropdown iterates `FormatChoice::ALL` and uses `.label()` (app.rs:113-114).

---

## Task 1: Add dependencies to jamsplit-core

**Files:**
- Modify: `crates/jamsplit-core/Cargo.toml`

- [ ] **Step 1: Add the crates with cargo add (trims zip to pure-Rust deflate)**

Run:

```bash
cargo add zip@8 --no-default-features --features deflate -p jamsplit-core
cargo add roxmltree@0.21 -p jamsplit-core
```

Why `--no-default-features --features deflate`: zip 8's default features pull in C-backed bzip2/lzma/zstd, which complicate the cross-platform release builds. `deflate` alone uses flate2/miniz_oxide (pure Rust) and is enough to read `project.xml` (stored entries always read; deflate covers compressed ones).

- [ ] **Step 2: Confirm the resulting `[dependencies]` block**

`crates/jamsplit-core/Cargo.toml` should now contain (versions may differ in patch):

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
csv = "1"
zip = { version = "8", default-features = false, features = ["deflate"] }
roxmltree = "0.21"
```

- [ ] **Step 3: Build to verify resolution (no C toolchain errors)**

Run: `cargo build -p jamsplit-core`
Expected: builds clean. If you see errors mentioning `bzip2-sys`, `zstd-sys`, or `lzma`, a non-deflate default feature leaked in — re-check `default-features = false`.

- [ ] **Step 4: Commit**

```bash
git add crates/jamsplit-core/Cargo.toml Cargo.lock
git commit -m "Add zip and roxmltree deps for DAWproject import"
```

---

## Task 2: Add the `MarkerFormat::Dawproject` variant

**Files:**
- Modify: `crates/jamsplit-core/src/markers/mod.rs` (the `MarkerFormat` enum, its `FromStr`, its `Display`)

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `crates/jamsplit-core/src/markers/mod.rs`:

```rust
    #[test]
    fn dawproject_format_roundtrips_name() {
        assert_eq!(
            "dawproject".parse::<MarkerFormat>(),
            Ok(MarkerFormat::Dawproject)
        );
        assert_eq!(MarkerFormat::Dawproject.to_string(), "dawproject");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p jamsplit-core dawproject_format_roundtrips_name`
Expected: FAIL — `no variant named Dawproject`.

- [ ] **Step 3: Add the variant and its mappings**

In `crates/jamsplit-core/src/markers/mod.rs`, change the enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerFormat {
    Audacity,
    Plain,
    Reaper,
    Dawproject,
}
```

In its `FromStr` impl, add the `dawproject` arm and update the error text:

```rust
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
```

In its `Display` impl, add the arm:

```rust
        f.write_str(match self {
            Self::Audacity => "audacity",
            Self::Plain => "plain",
            Self::Reaper => "reaper",
            Self::Dawproject => "dawproject",
        })
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p jamsplit-core dawproject_format_roundtrips_name`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core/src/markers/mod.rs
git commit -m "Add MarkerFormat::Dawproject variant"
```

---

## Task 3: DAWproject reader — seconds happy path

**Files:**
- Create: `crates/jamsplit-core/src/markers/dawproject.rs`
- Modify: `crates/jamsplit-core/src/markers/mod.rs` (add `pub mod dawproject;`)

- [ ] **Step 1: Register the module**

At the top of `crates/jamsplit-core/src/markers/mod.rs`, add `dawproject` to the module list:

```rust
pub mod audacity;
pub mod dawproject;
pub mod plain;
pub mod reaper;
```

- [ ] **Step 2: Write the reader with the seconds happy path**

Create `crates/jamsplit-core/src/markers/dawproject.rs`:

```rust
use super::{ParseError, RawMarker};
use std::io::{Cursor, Read};

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
    let row_of = |node: &roxmltree::Node| doc.text_pos_at(node.range().start).row as usize;

    let project = doc.root_element();
    let arrangement = match project.children().find(|n| n.has_tag_name("Arrangement")) {
        Some(a) => a,
        None => {
            return Err(vec![ParseError {
                line: row_of(&project),
                message: "no <Arrangement> element in project.xml".to_string(),
            }])
        }
    };

    let markers_el = match arrangement.children().find(|n| n.has_tag_name("Markers")) {
        Some(m) => m,
        None => {
            return Err(vec![ParseError {
                line: row_of(&arrangement),
                message: "no <Markers> in the arrangement (nothing to split on)".to_string(),
            }])
        }
    };

    let time_unit = match markers_el.attribute("timeUnit") {
        Some(u) => u,
        None => {
            return Err(vec![ParseError {
                line: row_of(&markers_el),
                message: "<Markers> has no timeUnit attribute; cannot tell beats from seconds"
                    .to_string(),
            }])
        }
    };

    // None => seconds; Some(bpm) => beats converted with this constant tempo.
    let beats_bpm: Option<f64> = match time_unit {
        "seconds" => None,
        "beats" => Some(resolve_bpm(doc, &project, &markers_el)?),
        other => {
            return Err(vec![ParseError {
                line: row_of(&markers_el),
                message: format!("unknown timeUnit {other:?} (expected \"seconds\" or \"beats\")"),
            }])
        }
    };

    let mut markers = Vec::new();
    let mut errors = Vec::new();
    for m in markers_el.children().filter(|n| n.has_tag_name("Marker")) {
        let line = row_of(&m);
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
            line: row_of(&markers_el),
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
/// Used only when markers are in beats. Defined fully in Task 4.
fn resolve_bpm(
    doc: &roxmltree::Document,
    project: &roxmltree::Node,
    markers_el: &roxmltree::Node,
) -> Result<f64, Vec<ParseError>> {
    let _ = (doc, project, markers_el);
    Ok(120.0)
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
            zw.start_file(entry_name, SimpleFileOptions::default()).unwrap();
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
}
```

Note: `resolve_bpm` is a stub here (returns 120.0) so the seconds path compiles and tests pass; Task 4 replaces its body and adds the beats tests. The seconds tests never call it.

- [ ] **Step 3: Run the seconds tests**

Run: `cargo test -p jamsplit-core markers::dawproject`
Expected: PASS (`reads_seconds_markers`, `missing_name_becomes_empty_title`).

- [ ] **Step 4: Commit**

```bash
git add crates/jamsplit-core/src/markers/mod.rs crates/jamsplit-core/src/markers/dawproject.rs
git commit -m "Add DAWproject reader with seconds-marker support"
```

---

## Task 4: Beats → seconds with tempo validation

**Files:**
- Modify: `crates/jamsplit-core/src/markers/dawproject.rs` (replace the `resolve_bpm` stub; add tests)

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `dawproject.rs`:

```rust
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
        assert!(errs[0].message.contains("bpm"));
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
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p jamsplit-core markers::dawproject`
Expected: FAIL — the beats test gets 0.0 (stub bpm 120 gives the right number by luck for beat 4 = 2.0? No: stub returns 120.0, so `converts_beats_to_seconds_with_tempo` would actually PASS by coincidence, but `beats_without_tempo_is_refused`, `non_bpm_tempo_unit_is_refused`, and `non_positive_or_nonfinite_bpm_is_refused` FAIL because the stub never refuses).

- [ ] **Step 3: Replace the `resolve_bpm` stub with the real implementation**

In `dawproject.rs`, replace the entire `resolve_bpm` function with:

```rust
/// Read and validate `Project > Transport > Tempo`, returning a positive bpm.
/// Used only when markers are in beats. Refuses a missing tempo, a non-`bpm`
/// unit, or a value that is not finite and strictly positive.
fn resolve_bpm(
    doc: &roxmltree::Document,
    project: &roxmltree::Node,
    markers_el: &roxmltree::Node,
) -> Result<f64, Vec<ParseError>> {
    let row_of = |node: &roxmltree::Node| doc.text_pos_at(node.range().start).row as usize;

    let tempo = project
        .children()
        .find(|n| n.has_tag_name("Transport"))
        .and_then(|t| t.children().find(|n| n.has_tag_name("Tempo")));
    let tempo = match tempo {
        Some(t) => t,
        None => {
            return Err(vec![ParseError {
                line: row_of(markers_el),
                message: "markers are in beats but the project has no <Transport><Tempo>"
                    .to_string(),
            }])
        }
    };

    match tempo.attribute("unit") {
        Some("bpm") => {}
        other => {
            return Err(vec![ParseError {
                line: row_of(&tempo),
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
            line: row_of(&tempo),
            message: "tempo value is missing or not a positive number".to_string(),
        }]),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p jamsplit-core markers::dawproject`
Expected: PASS (all seconds + beats tests).

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core/src/markers/dawproject.rs
git commit -m "Convert beats markers to seconds with validated tempo"
```

---

## Task 5: Refusals, structural errors, and multi-error collection

**Files:**
- Modify: `crates/jamsplit-core/src/markers/dawproject.rs` (tests only — the code already handles these)

- [ ] **Step 1: Write the remaining failing tests**

Add to the `mod tests` block in `dawproject.rs`:

```rust
    #[test]
    fn tempo_automation_is_refused() {
        let xml = r#"<Project>
          <Transport><Tempo unit="bpm" value="120.0"/></Transport>
          <Arrangement>
            <TempoAutomation/>
            <Markers timeUnit="seconds"><Marker time="0.0" name="X"/></Markers>
          </Arrangement></Project>"#;
        // NOTE: this requires Task 5 Step 2's refusal in parse_document.
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
        let errs = parse(&dawproject("<Project><Arrangement></Arrangement></Project>")).unwrap_err();
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
    fn multiple_bad_markers_are_all_reported() {
        let xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
          <Marker name="no time"/>
          <Marker time="abc" name="bad"/>
          <Marker time="5.0" name="fine"/>
        </Markers></Arrangement></Project>"#;
        let errs = parse(&dawproject(xml)).unwrap_err();
        assert_eq!(errs.len(), 2);
    }
```

- [ ] **Step 2: Add the tempo-automation refusal to `parse_document`**

Everything except tempo automation already works from Task 3. In `dawproject.rs`, in `parse_document`, insert the tempo-automation check immediately after the `arrangement` is resolved and before the `markers_el` lookup:

```rust
    if let Some(ta) = arrangement.children().find(|n| n.has_tag_name("TempoAutomation")) {
        return Err(vec![ParseError {
            line: row_of(&ta),
            message: "tempo automation is not supported; jamsplit needs a single constant tempo"
                .to_string(),
        }]);
    }
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p jamsplit-core markers::dawproject`
Expected: PASS (all dawproject tests).

- [ ] **Step 4: Commit**

```bash
git add crates/jamsplit-core/src/markers/dawproject.rs
git commit -m "Refuse tempo automation and cover DAWproject error paths"
```

---

## Task 6: `parse_markers_bytes` router

**Files:**
- Modify: `crates/jamsplit-core/src/markers/mod.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `mod.rs`:

```rust
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
            zw.start_file("project.xml", SimpleFileOptions::default()).unwrap();
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
        assert!(!errs.is_empty());
    }

    #[test]
    fn bytes_router_rejects_non_utf8_text() {
        // 0xFF 0xFE is neither zip magic nor valid UTF-8.
        let errs = parse_markers_bytes(&[0xFF, 0xFE, 0x00], None).unwrap_err();
        assert!(errs[0].message.contains("UTF-8"));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p jamsplit-core bytes_router`
Expected: FAIL — `parse_markers_bytes` not found.

- [ ] **Step 3: Implement `parse_markers_bytes`**

In `crates/jamsplit-core/src/markers/mod.rs`, add this function next to `parse_markers`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p jamsplit-core bytes_router`
Expected: PASS.

- [ ] **Step 5: Run the whole core suite for regressions**

Run: `cargo test -p jamsplit-core`
Expected: PASS (all existing + new tests).

- [ ] **Step 6: Commit**

```bash
git add crates/jamsplit-core/src/markers/mod.rs
git commit -m "Add parse_markers_bytes router for DAWproject and text formats"
```

---

## Task 7: CLI integration

**Files:**
- Modify: `crates/jamsplit-cli/src/cli.rs`
- Modify: `crates/jamsplit-cli/Cargo.toml` (dev-dep `zip`)
- Modify: `crates/jamsplit-cli/tests/common/mod.rs` (dawproject fixture helper)
- Modify: `crates/jamsplit-cli/tests/cli_integration.rs` (help test + end-to-end test)

- [ ] **Step 1: Add `Dawproject` to `FormatArg` and update help text**

In `crates/jamsplit-cli/src/cli.rs`:

Change the import line:

```rust
use jamsplit_core::markers::{parse_markers_bytes, MarkerFormat, ParsedMarkers};
```

Add the variant to `FormatArg`:

```rust
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum FormatArg {
    Auto,
    Audacity,
    Plain,
    Reaper,
    Dawproject,
}
```

Add its mapping in `into_marker_format`:

```rust
            FormatArg::Dawproject => Some(MarkerFormat::Dawproject),
```

Update the `CommonArgs` doc comments for `markers` and `format`:

```rust
    /// Marker file (audacity, plain, reaper, or a Bitwig .dawproject)
    #[arg(long)]
    pub markers: PathBuf,
    /// Force the marker format instead of auto-detecting [auto|audacity|plain|reaper|dawproject]
    #[arg(long)]
    pub format: Option<FormatArg>,
```

- [ ] **Step 2: Switch `load` to read bytes and route through `parse_markers_bytes`**

In `cli.rs`, in `load`, replace the marker-reading block:

```rust
    let bytes = std::fs::read(&common.markers)
        .with_context(|| format!("could not read marker file {}", common.markers.display()))?;
    let marker_format = common.format.and_then(|f| f.into_marker_format());
    let parsed = parse_markers_bytes(&bytes, marker_format).map_err(|errs| {
        let lines: Vec<String> = errs
            .iter()
            .map(|e| format!("{}: {e}", common.markers.display()))
            .collect();
        anyhow!("{}", lines.join("\n"))
    })?;
```

(Only the first two statements changed: `read_to_string` → `read`, `parse_markers` → `parse_markers_bytes`. The `marker_format`, announcement, and the rest of `load` are unchanged.)

- [ ] **Step 3: Build and run existing CLI tests (regression)**

Run: `cargo test -p jamsplit-cli`
Expected: PASS for everything except `split_help_lists_all_format_values`, which still passes (it only asserts the four older names are present) — `dawproject` is now also listed but not yet asserted.

- [ ] **Step 4: Update the help-lists-formats test to assert dawproject**

In `crates/jamsplit-cli/tests/cli_integration.rs`, add one line to `split_help_lists_all_format_values`:

```rust
        .stdout(predicates::str::contains("reaper"))
        .stdout(predicates::str::contains("dawproject"));
```

(Append the `dawproject` assertion after the existing `reaper` one; move the `;` onto the new last line.)

- [ ] **Step 5: Add the dawproject fixture helper to the CLI test common module**

First add the dev-dependency:

```bash
cargo add zip@8 --dev --no-default-features --features deflate -p jamsplit-cli
```

Then add to `crates/jamsplit-cli/tests/common/mod.rs`:

```rust
/// Write a minimal `.dawproject` (a zip holding `project.xml`) into `dir` and
/// return its path. `project_xml` is the file body.
pub fn make_dawproject(dir: &Path, project_xml: &str) -> PathBuf {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let path = dir.join("session.dawproject");
    let file = std::fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    zw.start_file("project.xml", SimpleFileOptions::default()).unwrap();
    zw.write_all(project_xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    path
}
```

- [ ] **Step 6: Write the failing end-to-end CLI test**

Add to `crates/jamsplit-cli/tests/cli_integration.rs` (it already has `use common::{ffmpeg_or_skip, make_wav};` — extend it to also import `make_dawproject`):

```rust
use common::{ffmpeg_or_skip, make_dawproject, make_wav};
```

```rust
#[test]
fn validate_reads_a_dawproject_file() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let project_xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
        <Marker time="0.0" name="Opener"/>
        <Marker time="5.0" name="Closer"/>
    </Markers></Arrangement></Project>"#;
    let markers = make_dawproject(dir.path(), project_xml);
    jamsplit()
        .args(["validate", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .assert()
        .success()
        .stdout(predicates::str::contains("OK: 2 songs"))
        .stderr(predicates::str::contains("dawproject"));
}
```

- [ ] **Step 7: Run the CLI tests**

Run: `cargo test -p jamsplit-cli`
Expected: PASS. `validate_reads_a_dawproject_file` runs if ffmpeg is present, else prints a skip notice.

- [ ] **Step 8: Commit**

```bash
git add crates/jamsplit-cli/src/cli.rs crates/jamsplit-cli/Cargo.toml \
  crates/jamsplit-cli/tests/common/mod.rs crates/jamsplit-cli/tests/cli_integration.rs Cargo.lock
git commit -m "Wire DAWproject import into the CLI"
```

---

## Task 8: GUI integration

**Files:**
- Modify: `crates/jamsplit-gui/src/state.rs`
- Modify: `crates/jamsplit-gui/src/worker.rs`
- Modify: `crates/jamsplit-gui/Cargo.toml` (dev-dep `zip`)
- Modify: `crates/jamsplit-gui/tests/common/mod.rs` (dawproject fixture helper)
- Modify: `crates/jamsplit-gui/tests/worker_integration.rs` (end-to-end `run_preview` test)

- [ ] **Step 1: Add `Dawproject` to `FormatChoice`**

In `crates/jamsplit-gui/src/state.rs`, add the variant:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatChoice {
    #[default]
    Auto,
    Audacity,
    Plain,
    Reaper,
    Dawproject,
}
```

Extend `ALL` to five entries:

```rust
    pub const ALL: [FormatChoice; 5] = [
        FormatChoice::Auto,
        FormatChoice::Audacity,
        FormatChoice::Plain,
        FormatChoice::Reaper,
        FormatChoice::Dawproject,
    ];
```

Add the label arm:

```rust
            FormatChoice::Dawproject => "dawproject",
```

Add the `into_marker_format` arm:

```rust
            FormatChoice::Dawproject => Some(MarkerFormat::Dawproject),
```

- [ ] **Step 2: Switch `run_preview` to read bytes and route through `parse_markers_bytes`**

In `crates/jamsplit-gui/src/worker.rs`:

Change the import:

```rust
use jamsplit_core::markers::parse_markers_bytes;
```

Replace the marker-reading match in `run_preview`:

```rust
    let parsed = match std::fs::read(&request.markers) {
        Ok(bytes) => match parse_markers_bytes(&bytes, request.format) {
            Ok(parsed) => {
                format = Some((parsed.format.to_string(), request.format.is_some()));
                Some(parsed)
            }
            Err(parse_errors) => {
                errors.extend(
                    parse_errors
                        .iter()
                        .map(|e| format!("{}: {e}", request.markers.display())),
                );
                None
            }
        },
        Err(e) => {
            errors.push(format!(
                "could not read marker file {}: {e}",
                request.markers.display()
            ));
            None
        }
    };
```

(Only `read_to_string` → `read` and `parse_markers(&content, …)` → `parse_markers_bytes(&bytes, …)` changed.)

- [ ] **Step 3: Build and run existing GUI tests (regression)**

Run: `cargo test -p jamsplit-gui`
Expected: PASS (the state-machine and worker tests are unaffected).

- [ ] **Step 4: Add the dawproject fixture helper to the GUI test common module**

Add the dev-dependency:

```bash
cargo add zip@8 --dev --no-default-features --features deflate -p jamsplit-gui
```

Add to `crates/jamsplit-gui/tests/common/mod.rs`:

```rust
/// Write a minimal `.dawproject` (a zip holding `project.xml`) into `dir` and
/// return its path. `project_xml` is the file body.
pub fn make_dawproject(dir: &Path, project_xml: &str) -> PathBuf {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let path = dir.join("session.dawproject");
    let file = std::fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    zw.start_file("project.xml", SimpleFileOptions::default()).unwrap();
    zw.write_all(project_xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    path
}
```

(If `Path`/`PathBuf` are not already imported in this file, add `use std::path::{Path, PathBuf};` at the top — it mirrors the CLI common module.)

- [ ] **Step 5: Write the failing end-to-end GUI test**

In `crates/jamsplit-gui/tests/worker_integration.rs`, add:

```rust
#[test]
fn preview_reads_a_dawproject_file() {
    let Some(ff) = common::ffmpeg_or_skip() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let audio = common::make_wav(&ff, dir.path(), 10.0);
    let project_xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
        <Marker time="0.0" name="Opener"/>
        <Marker time="5.0" name="Closer"/>
    </Markers></Arrangement></Project>"#;
    let markers = common::make_dawproject(dir.path(), project_xml);
    let outdir = dir.path().join("out");

    let outcome = run_preview(&PreviewRequest {
        gen: 1,
        audio,
        markers,
        format: None,
        outdir,
        overwrite: false,
        ffmpeg: ff,
    });
    assert_eq!(outcome.errors, Vec::<String>::new());
    assert_eq!(outcome.format, Some(("dawproject".to_string(), false)));
    let plan = outcome.plan.expect("plan should build");
    assert_eq!(plan.songs.len(), 2);
    assert_eq!(plan.songs[0].filename, "01 - Opener.mp3");
}
```

- [ ] **Step 6: Run the GUI tests**

Run: `cargo test -p jamsplit-gui`
Expected: PASS. `preview_reads_a_dawproject_file` runs if ffmpeg is present, else skips.

- [ ] **Step 7: Commit**

```bash
git add crates/jamsplit-gui/src/state.rs crates/jamsplit-gui/src/worker.rs \
  crates/jamsplit-gui/Cargo.toml crates/jamsplit-gui/tests/common/mod.rs \
  crates/jamsplit-gui/tests/worker_integration.rs Cargo.lock
git commit -m "Wire DAWproject import into the GUI"
```

---

## Task 9: User-facing docs (MARKERS.md, README.md, index.html)

**Files:**
- Modify: `MARKERS.md`
- Modify: `README.md`
- Modify: `index.html`

- [ ] **Step 1: Add a Bitwig / DAWproject section to MARKERS.md**

In `MARKERS.md`, insert this new section between the REAPER section and the `## Plain text` section:

```markdown
## Bitwig (DAWproject)

Use a Bitwig `.dawproject` export when the session is laid out in Bitwig Studio
and you have placed cue markers at the song starts.

1. Open the session in Bitwig Studio.
2. Add a cue marker at each song start and name it with the song title.
3. Keep the project at a single, constant tempo — do not add tempo changes.
4. Choose `File > Export DAWproject`.
5. Use the exported `.dawproject` file as the jamsplit marker file. Leave the
   format on auto; jamsplit recognizes `.dawproject` automatically.

jamsplit reads only the arrangement cue markers (and the project tempo, to
convert them to seconds). It does not import the audio from the `.dawproject` —
choose your recording as the audio file as usual.

Requirements and limits:

- The project must have a single constant tempo. If it contains tempo changes
  (tempo automation), jamsplit refuses the file rather than guess wrong split
  points — flatten the tempo and re-export.
- Markers must carry a time unit jamsplit understands (`seconds` or `beats`);
  Bitwig sets this on export.
```

- [ ] **Step 2: Update the README marker-formats section**

In `README.md`, update the intro paragraph (around line 65):

```markdown
For step-by-step instructions on creating marker files in Audacity, REAPER,
Bitwig, or by hand, see [MARKERS.md](MARKERS.md).
```

Update the force-format line (around line 68):

```markdown
The format is auto-detected (announced on stderr); force one with
`--format audacity|plain|reaper|dawproject`.
```

Add a new format bullet after the **Reaper** bullet (around line 87), before the "Markers mark **song starts only**" paragraph:

```markdown
**Bitwig (DAWproject)** — `File > Export DAWproject`. jamsplit reads the
arrangement cue markers; the project must use a single constant tempo (tempo
changes are rejected). `.dawproject` files are recognized automatically. Only
markers are imported, not the audio.
```

- [ ] **Step 3: Update the landing page (index.html)**

In `index.html`, update the lede (line 332):

```html
          <p class="lede">jamsplit takes a long WAV or other audio file plus a marker file from Audacity, REAPER, Bitwig, or a plain text list, then exports numbered MP3s like <code>01 - Opening Jam.mp3</code>.</p>
```

Update step 2 of "How It Works" (line 371):

```html
          <p>In Audacity, REAPER, Bitwig, or a plain text file, put a marker where each song begins. The next marker ends the current song.</p>
```

Add a Bitwig card in the "Make Markers" grid, immediately after the REAPER card's closing `</div>` (after line 408) and before the Plain Text card:

```html
        <div class="card">
          <h3>Bitwig</h3>
          <ol>
            <li>Open the session in Bitwig Studio.</li>
            <li>Add a cue marker at each song start and name it.</li>
            <li>Keep one constant tempo — no tempo changes.</li>
            <li>Use <strong>File > Export DAWproject</strong>.</li>
          </ol>
          <p class="note">Use the exported <code>.dawproject</code> file as the jamsplit marker file. Only markers are imported, not the audio.</p>
        </div>
```

Update the "Use The App" steps (lines 426-427):

```html
        <li>Choose the marker file from Audacity, REAPER, Bitwig (.dawproject), or your plain text list.</li>
        <li>Leave the format on <strong>auto</strong> unless you need to force <strong>audacity</strong>, <strong>reaper</strong>, <strong>plain</strong>, or <strong>dawproject</strong>.</li>
```

- [ ] **Step 4: Commit**

```bash
git add MARKERS.md README.md index.html
git commit -m "Document DAWproject (Bitwig) marker import"
```

---

## Task 10: Source-of-truth docs (design doc + CLAUDE.md + AGENTS.md)

**Files:**
- Modify: `docs/superpowers/specs/2026-06-05-jamsplit-design.md`
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: Update the living design doc**

In `docs/superpowers/specs/2026-06-05-jamsplit-design.md`:

Update the dependency sentence (around line 71) — add the new deps to the core list:

```markdown
Times are `f64` seconds throughout. Core dependencies kept minimal: `serde`/`serde_json` (summary, ffprobe output), `thiserror`, `csv` (Reaper quoting only), `zip` + `roxmltree` (DAWproject import, post-v1). CLI adds `clap` + `anyhow`; GUI adds `eframe`/`egui` + `rfd`. No async runtime, no regex, no audio crates.
```

Update the CLI synopsis (around line 78) — add `dawproject`:

```markdown
                  [--format auto|audacity|plain|reaper|dawproject] [--ffmpeg-path PATH]
```

Add a subsection to "## Marker formats" immediately after the Reaper subsection (after the line ending "...supported boundary for Reaper's settings-dependent export.", around line 137):

```markdown
### Bitwig DAWproject (`.dawproject`, post-v1)

A `.dawproject` is a ZIP holding `project.xml`. The reader (`markers/dawproject.rs`) reads `Project > Arrangement > Markers > Marker` and normalizes each `time` to seconds: `timeUnit="seconds"` is used directly; `timeUnit="beats"` is converted with the single `Project > Transport > Tempo` value (`time * 60 / bpm`). It refuses, with a clear collected error, when tempo automation is present, when `timeUnit` is absent, when beats markers have no tempo, when the tempo unit is not `bpm`, or when the tempo value is not finite-and-positive. Detection is by ZIP magic (`PK\x03\x04`) at the bytes layer (`parse_markers_bytes`), since a ZIP is not valid UTF-8 text. Markers only — embedded/referenced audio is ignored. See `docs/superpowers/specs/2026-06-08-dawproject-import-design.md`.
```

- [ ] **Step 2: Update the architecture-invariant parser bullet in CLAUDE.md and AGENTS.md**

In both `CLAUDE.md` and `AGENTS.md`, the architecture-invariant bullet currently reads:

```markdown
- Parsers (audacity/plain/reaper) are dumb: bytes in, `(start_seconds, title)` out. Every business rule (sorting, duplicates, bounds, untitled naming, filename sanitization, boundary math) lives in `plan()`, which is where unit tests concentrate.
```

Change `(audacity/plain/reaper)` to `(audacity/plain/reaper/dawproject)` in both files:

```markdown
- Parsers (audacity/plain/reaper/dawproject) are dumb: bytes in, `(start_seconds, title)` out. Every business rule (sorting, duplicates, bounds, untitled naming, filename sanitization, boundary math) lives in `plan()`, which is where unit tests concentrate.
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-06-05-jamsplit-design.md CLAUDE.md AGENTS.md
git commit -m "Record DAWproject import in source-of-truth docs"
```

---

## Task 11: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Format**

Run: `cargo fmt --all`
Then: `git diff --stat` — if anything reformatted, review and `git commit -am "cargo fmt"`.

- [ ] **Step 2: Clippy (workspace, warnings as errors)**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. Fix any in the new code (e.g. needless borrows) and re-run.

- [ ] **Step 3: Full test suite**

Run: `cargo test --workspace`
Expected: PASS. ffmpeg-dependent integration tests print a skip notice if ffmpeg is absent.

- [ ] **Step 4: Full suite in CI mode (only if ffmpeg is installed)**

Run: `JAMSPLIT_TEST_REQUIRE_FFMPEG=1 cargo test --workspace`
Expected: PASS with no skips. If ffmpeg is not installed locally, note that this step is deferred to CI.

- [ ] **Step 5: Manual GUI smoke check of the new format**

Build and run the GUI (`cargo run -p jamsplit-gui`), confirm the Format dropdown now lists `dawproject`, and load a real or fixture `.dawproject` against an audio file to confirm the preview shows songs. Follow `docs/gui-manual-test-checklist.md` for the rest. (No commit; this is a gate before declaring done.)

- [ ] **Step 6: Final commit if any fixups were made**

```bash
git status
# commit any remaining fmt/clippy fixups with a clear message
```

---

## Notes for the implementer

- **The reader is a "dumb" parser by the project's invariant.** It must not print, must not sort/dedup/bounds-check (that is `plan()`'s job), and must collect every problem rather than stop at the first. Beats→seconds conversion is parsing (normalizing the format's time representation), not a business rule — the same category as `parse_timestamp` turning `5:23` into `323.0`.
- **No real Bitwig export has been verified.** The reader is built to the published schema. The refusal rules (unknown timeUnit, tempo automation, non-bpm tempo) exist so a surprising file fails loudly with a clear message instead of mis-splitting. If a real `.dawproject` later shows a different `timeUnit` placement (e.g. inherited from an ancestor rather than on `<Markers>`), that is the one place the reader may need to also consult an ancestor element — flag it to Jason rather than guessing.
- **roxmltree borrows the XML string.** Keep the `xml: String` alive for as long as the `Document` is used (the code above does — both live in `parse`).
- **Do not remove or weaken** the ffmpeg resolution order or the collect-and-report error handling elsewhere; this feature only adds a parser and a router.
