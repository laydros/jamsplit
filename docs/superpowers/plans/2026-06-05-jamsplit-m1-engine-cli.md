# jamsplit M1 (Engine + CLI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build jamsplit-core (all splitting logic) and the jamsplit CLI so one long jam recording plus a marker file becomes per-song MP3s — every acceptance criterion of the v1 spec.

**Architecture:** Rust workspace. `jamsplit-core` is a lib that never prints and never sees clap: dumb parsers normalize three marker formats into `(start_seconds, title)` pairs, `plan()` applies every business rule to produce a `SplitPlan`, and `export()` runs one ffmpeg invocation per song with progress callback + cancel token. `jamsplit-cli` maps `split`/`validate`/`inspect` onto that pipeline. ffmpeg/ffprobe are external subprocesses resolved flag → adjacent-to-executable → PATH.

**Tech Stack:** Rust 2021. Core deps: serde, serde_json, thiserror, csv. CLI deps: clap (derive), anyhow. Dev deps: tempfile, assert_cmd, predicates.

**Source of truth:** `docs/superpowers/specs/2026-06-05-jamsplit-design.md` (design, binding) and `docs/spec.md` (requirements). If this plan and the design doc conflict, the design doc wins — stop and flag it.

---

## File map (what gets created)

```text
Cargo.toml                                  # workspace root
.gitignore
crates/jamsplit-core/Cargo.toml
crates/jamsplit-core/src/lib.rs             # pub mod lines only
crates/jamsplit-core/src/markers/mod.rs     # RawMarker, MarkerFormat, ParseError, parse_timestamp, detect_format, parse_markers
crates/jamsplit-core/src/markers/plain.rs   # flexible hand-written format
crates/jamsplit-core/src/markers/audacity.rs
crates/jamsplit-core/src/markers/reaper.rs
crates/jamsplit-core/src/plan.rs            # plan(), SplitPlan, Song, sanitize_title, check_collisions, fmt_time
crates/jamsplit-core/src/audio.rs           # AudioInfo, parse_ffprobe_output, probe_audio
crates/jamsplit-core/src/ffmpeg.rs          # FfmpegPaths::locate, CancelToken, ExportOptions, build_song_args, export
crates/jamsplit-core/src/report.rs          # Summary (serde), build_summary, write_summary, render_table
crates/jamsplit-core/tests/common/mod.rs    # ffmpeg_or_skip(), make_wav()
crates/jamsplit-core/tests/export_integration.rs
crates/jamsplit-cli/Cargo.toml
crates/jamsplit-cli/src/main.rs             # entry; exit-code mapping
crates/jamsplit-cli/src/cli.rs              # clap types + subcommand driver functions
crates/jamsplit-cli/tests/common/mod.rs     # copy of ffmpeg_or_skip()/make_wav() (10 lines; a test-util crate is not worth it)
crates/jamsplit-cli/tests/cli_integration.rs
README.md
```

Unit tests live in `#[cfg(test)] mod tests` blocks inside each source file. Integration tests (anything spawning real ffmpeg/ffprobe) live in `tests/`.

## Conventions for every task

- TDD: write the failing test, watch it fail, implement minimally, watch it pass, commit. No step reordering.
- Run tests from the repo root: `cargo test -p jamsplit-core` (or `-p jamsplit-cli`). A single test: `cargo test -p jamsplit-core test_name`.
- Commit messages: plain imperative, matching repo history ("Add plain marker parser"). Never mention Claude.
- ffmpeg-dependent tests must call `ffmpeg_or_skip()` (Task 9) and early-return when it yields `None`. They are hard-required only when `JAMSPLIT_TEST_REQUIRE_FFMPEG=1` (CI).
- Times are `f64` seconds everywhere. Core never prints, never exits, never reads CLI args.

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `crates/jamsplit-core/Cargo.toml`, `crates/jamsplit-core/src/lib.rs`

- [ ] **Step 1: Create the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/jamsplit-core"]

[workspace.package]
version = "0.1.0"
edition = "2021"
```

- [ ] **Step 2: Create `.gitignore`**

```text
/target
```

- [ ] **Step 3: Create `crates/jamsplit-core/Cargo.toml`**

```toml
[package]
name = "jamsplit-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
csv = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create `crates/jamsplit-core/src/lib.rs`**

```rust
//! Core engine for jamsplit: marker parsing, split planning, ffmpeg-driven export.
```

(Module lines are added by later tasks as the modules come into existence.)

- [ ] **Step 5: Verify the workspace builds and the empty test suite passes**

Run: `cargo test -p jamsplit-core`
Expected: compiles, `running 0 tests ... test result: ok`

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml .gitignore crates/
git commit -m "Add workspace scaffold with jamsplit-core crate"
```

---

### Task 2: Timestamp parser

The shared time parser used by the plain and Reaper parsers. Colon count decides the form: none = raw seconds, one = `M:SS`, two = `H:MM:SS`. Leading component unbounded; every later component must be `< 60`; fractional seconds allowed; negatives rejected.

**Files:**
- Create: `crates/jamsplit-core/src/markers/mod.rs`
- Modify: `crates/jamsplit-core/src/lib.rs`

- [ ] **Step 1: Create `markers/mod.rs` with the failing tests**

```rust
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
    todo!()
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
```

- [ ] **Step 2: Add the module to `lib.rs` and create an empty `markers/plain.rs`**

`lib.rs` becomes:

```rust
//! Core engine for jamsplit: marker parsing, split planning, ffmpeg-driven export.
pub mod markers;
```

`markers/plain.rs` is created empty (the `pub mod plain;` line needs the file to exist; Task 3 fills it).

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core parse_timestamp`
Expected: FAIL — `todo!()` panics (or compile error if signatures drifted; fix until the failure is the panic).

- [ ] **Step 4: Implement `parse_timestamp`**

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add shared timestamp parser"
```

---

### Task 3: Plain-text marker parser

Hand-written format: time, optional separator (`- `, tab, or whitespace), title = trimmed rest of line (may be empty). `#` comments and blank lines ignored. Errors carry 1-based line numbers and are collected, not die-on-first.

**Files:**
- Modify: `crates/jamsplit-core/src/markers/plain.rs`

- [ ] **Step 1: Write the failing tests in `markers/plain.rs`**

```rust
use super::{ParseError, RawMarker};

/// Parse the hand-written plain format. Collects all errors instead of
/// stopping at the first.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(start: f64, title: &str) -> RawMarker {
        RawMarker { start_seconds: start, title: title.to_string() }
    }

    #[test]
    fn parses_all_separator_styles() {
        let input = "0:00 Opening Jam\n05:23 - Slow Blues\n1:02:11\tCloser\n3722.5 Encore Noodle\n";
        let got = parse(input).unwrap();
        assert_eq!(got, vec![
            marker(0.0, "Opening Jam"),
            marker(323.0, "Slow Blues"),
            marker(3731.0, "Closer"),
            marker(3722.5, "Encore Noodle"),
        ]);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core plain`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `parse`**

```rust
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
                markers.push(RawMarker { start_seconds, title: title.to_string() });
            }
            Err(message) => errors.push(ParseError { line: line_no, message }),
        }
    }
    if errors.is_empty() { Ok(markers) } else { Err(errors) }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add plain-text marker parser"
```

---

### Task 4: Audacity label parser

Tab-separated export: `start<TAB>end<TAB>label`, times in decimal seconds. Only `start` is used (range ends ignored). Lines starting with `\` are spectral-selection frequency data — skipped silently. Label may be empty or absent.

**Files:**
- Create: `crates/jamsplit-core/src/markers/audacity.rs`
- Modify: `crates/jamsplit-core/src/markers/mod.rs` (add `pub mod audacity;`)

- [ ] **Step 1: Add `pub mod audacity;` to `markers/mod.rs`, create `markers/audacity.rs` with failing tests**

```rust
use super::{ParseError, RawMarker};

/// Parse an Audacity label export (File -> Export Labels). Tab-separated
/// `start end label`; spectral lines starting with `\` are skipped.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    todo!()
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
    fn collects_errors_with_line_numbers() {
        let input = "5.0\t5.0\tFine\nnot\tnumbers\there\n9.0\n";
        let errs = parse(input).unwrap_err();
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].line, 2); // non-numeric start/end
        assert_eq!(errs[1].line, 3); // only one field
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core audacity`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `parse`**

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add Audacity label parser"
```

---

### Task 5: Reaper CSV parser

Region/Marker Manager export: header `#,Name,Start,End,Length`, parsed by column name (extra columns tolerated). Rows `M*` (markers) and `R*` (regions) both accepted; regions contribute Start, End ignored. Start values go through `parse_timestamp`; a `N.N.NN` shape means the project was in bars/beats — that gets a specific re-export message.

**Files:**
- Create: `crates/jamsplit-core/src/markers/reaper.rs`
- Modify: `crates/jamsplit-core/src/markers/mod.rs` (add `pub mod reaper;`)

- [ ] **Step 1: Add `pub mod reaper;` to `markers/mod.rs`, create `markers/reaper.rs` with failing tests**

```rust
use super::{ParseError, RawMarker};

/// Parse a Reaper Region/Marker Manager CSV export.
pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    todo!()
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
        assert!(errs[0].message.contains("Minutes:Seconds"), "got: {}", errs[0].message);
    }

    #[test]
    fn missing_required_columns_is_one_error() {
        let errs = parse("Name,Position\nIntro,0:00\n").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core reaper`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `parse`**

```rust
fn looks_like_bars_beats(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn parse(content: &str) -> Result<Vec<RawMarker>, Vec<ParseError>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(content.as_bytes());
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => return Err(vec![ParseError { line: 1, message: format!("not a CSV file: {e}") }]),
    };
    let col = |name: &str| headers.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
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
                errors.push(ParseError { line: line_no, message: format!("bad CSV row: {e}") });
                continue;
            }
        };
        let id = record.get(id_col).unwrap_or("").trim();
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
                markers.push(RawMarker { start_seconds, title });
            }
            Err(message) => errors.push(ParseError { line: line_no, message }),
        }
    }
    if errors.is_empty() { Ok(markers) } else { Err(errors) }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add Reaper CSV marker parser"
```

---

### Task 6: Format auto-detection and parse dispatch

Detection order is a deliberate tiebreak (see design doc): Audacity's strict shape first (every non-blank, non-`\` line is `float TAB float ...`), then the Reaper header signature, then plain as fallback. `parse_markers` is the single entry point frontends call.

**Files:**
- Modify: `crates/jamsplit-core/src/markers/mod.rs`

- [ ] **Step 1: Add the failing tests and stubs to `markers/mod.rs`**

Add above the existing tests module:

```rust
/// Which marker format a file is in. `FromStr` accepts the CLI names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerFormat {
    Audacity,
    Plain,
    Reaper,
}

impl std::str::FromStr for MarkerFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "audacity" => Ok(Self::Audacity),
            "plain" => Ok(Self::Plain),
            "reaper" => Ok(Self::Reaper),
            other => Err(format!("unknown format '{other}' (expected audacity, plain, or reaper)")),
        }
    }
}

impl std::fmt::Display for MarkerFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Audacity => "audacity",
            Self::Plain => "plain",
            Self::Reaper => "reaper",
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
    todo!()
}

/// Parse markers, auto-detecting the format unless one is forced.
pub fn parse_markers(
    content: &str,
    format: Option<MarkerFormat>,
) -> Result<ParsedMarkers, Vec<ParseError>> {
    todo!()
}
```

Add inside the tests module:

```rust
    #[test]
    fn detects_audacity_shape() {
        assert_eq!(detect_format("1.0\t1.0\tIntro\n2.0\t3.0\n"), MarkerFormat::Audacity);
    }

    #[test]
    fn detection_skips_spectral_lines_like_the_parser_does() {
        let input = "1.0\t1.0\tChorus\n\\\t440.0\t880.0\n2.0\t2.0\tOutro\n";
        assert_eq!(detect_format(input), MarkerFormat::Audacity);
    }

    #[test]
    fn detects_reaper_header() {
        assert_eq!(detect_format("#,Name,Start,End,Length\nM1,Song,0:00,,\n"), MarkerFormat::Reaper);
    }

    #[test]
    fn falls_back_to_plain() {
        assert_eq!(detect_format("0:00 Opening Jam\n5:23 Slow Blues\n"), MarkerFormat::Plain);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core markers`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `detect_format` and `parse_markers`**

```rust
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
        if first.trim().to_ascii_lowercase().starts_with("#,name,start") {
            return MarkerFormat::Reaper;
        }
    }
    MarkerFormat::Plain
}

pub fn parse_markers(
    content: &str,
    format: Option<MarkerFormat>,
) -> Result<ParsedMarkers, Vec<ParseError>> {
    let format = format.unwrap_or_else(|| detect_format(content));
    let markers = match format {
        MarkerFormat::Audacity => audacity::parse(content)?,
        MarkerFormat::Plain => plain::parse(content)?,
        MarkerFormat::Reaper => reaper::parse(content)?,
    };
    Ok(ParsedMarkers { markers, format })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add marker format auto-detection and parse dispatch"
```

---

### Task 7: Title resolution and filename rules

Pure string functions, built bottom-up so `plan()` (Task 8) composes them. Three rules from the design doc: blank titles resolve to `Untitled Song N` (used in tag *and* filename); filenames sanitize the union of all-OS forbidden characters; a non-blank title that sanitizes to nothing falls back to `Untitled Song N` for the filename only.

**Files:**
- Create: `crates/jamsplit-core/src/plan.rs`
- Modify: `crates/jamsplit-core/src/lib.rs` (add `pub mod plan;`)

- [ ] **Step 1: Create `plan.rs` with failing tests, add `pub mod plan;` to `lib.rs`**

```rust
/// Resolve a marker title: blank/whitespace becomes `Untitled Song {track}`.
/// The resolved title is used everywhere — MP3 title tag and filename.
pub fn resolve_title(raw: &str, track: usize) -> String {
    todo!()
}

/// Make a title safe as a filename on every OS we ship to: replace
/// `/ \ : * ? " < > |` and control chars with `_`, collapse runs of `_`,
/// trim leading dots and trailing dots/spaces.
pub fn sanitize_title(title: &str) -> String {
    todo!()
}

/// Build `NN - Title.mp3`. `NN` is zero-padded to max(2, digits(total)).
/// A title that sanitizes to nothing falls back to `Untitled Song {track}`.
pub fn filename_for(track: usize, total: usize, resolved_title: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_titles_resolve_to_untitled() {
        assert_eq!(resolve_title("", 3), "Untitled Song 3");
        assert_eq!(resolve_title("   ", 12), "Untitled Song 12");
        assert_eq!(resolve_title("Slow Blues", 1), "Slow Blues");
    }

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(sanitize_title("AC/DC Jam"), "AC_DC Jam");
        assert_eq!(sanitize_title("a\\b:c*d?e\"f<g>h|i"), "a_b_c_d_e_f_g_h_i");
        assert_eq!(sanitize_title("tab\there"), "tab_here");
    }

    #[test]
    fn collapses_runs_and_trims_dots_and_spaces() {
        assert_eq!(sanitize_title("a//b"), "a_b");
        assert_eq!(sanitize_title("ends with dots..."), "ends with dots");
        assert_eq!(sanitize_title(".hidden"), "hidden");
        assert_eq!(sanitize_title("trailing space "), "trailing space");
    }

    #[test]
    fn filenames_are_padded_and_numbered() {
        assert_eq!(filename_for(1, 12, "Opening Jam"), "01 - Opening Jam.mp3");
        assert_eq!(filename_for(7, 120, "X"), "007 - X.mp3");
    }

    #[test]
    fn title_that_sanitizes_to_nothing_falls_back_for_filename() {
        assert_eq!(filename_for(2, 9, "..."), "02 - Untitled Song 2.mp3");
    }

    #[test]
    fn identical_titles_still_make_distinct_filenames() {
        // the track prefix guarantees within-run uniqueness by construction
        assert_ne!(filename_for(3, 10, "A_B"), filename_for(7, 10, "A_B"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core plan`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement the three functions**

```rust
pub fn resolve_title(raw: &str, track: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("Untitled Song {track}")
    } else {
        trimmed.to_string()
    }
}

pub fn sanitize_title(title: &str) -> String {
    const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut out = String::with_capacity(title.len());
    let mut last_was_underscore = false;
    for c in title.chars() {
        if FORBIDDEN.contains(&c) || c.is_control() {
            if !last_was_underscore {
                out.push('_');
                last_was_underscore = true;
            }
        } else {
            out.push(c);
            last_was_underscore = c == '_';
        }
    }
    out.trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string()
}

pub fn filename_for(track: usize, total: usize, resolved_title: &str) -> String {
    let width = std::cmp::max(2, total.to_string().len());
    let safe = sanitize_title(resolved_title);
    let name = if safe.is_empty() {
        format!("Untitled Song {track}")
    } else {
        safe
    };
    format!("{track:0width$} - {name}.mp3")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add title resolution and filename sanitization"
```

---

### Task 8: plan() — boundaries, validation, warnings

The business-rule core. Takes parsed markers + probed audio, returns a `SplitPlan` or all errors at once. Also: `fmt_time` for human display and `check_collisions` for the pre-export filesystem check.

**Files:**
- Create: `crates/jamsplit-core/src/audio.rs` (just `AudioInfo` for now; probing is Task 10)
- Modify: `crates/jamsplit-core/src/plan.rs`, `crates/jamsplit-core/src/lib.rs` (add `pub mod audio;`)

- [ ] **Step 1: Create `audio.rs` with the `AudioInfo` type, add `pub mod audio;` to `lib.rs`**

```rust
use std::path::PathBuf;

/// What we learned about the input audio from ffprobe.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub path: PathBuf,
    pub duration_seconds: f64,
    pub codec_name: String,
    /// pcm_*, flac, alac. Lossy (or unknown) inputs get an accuracy warning.
    pub lossless: bool,
}
```

- [ ] **Step 2: Add failing tests and stubs to `plan.rs`**

Add above the tests module:

```rust
use crate::audio::AudioInfo;
use crate::markers::ParsedMarkers;
use std::path::Path;

/// One song to export. `end_seconds` is always concrete (the last song's is
/// the audio duration); `to_eof` tells export() to omit `-t`.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub track: usize,
    pub title: String,
    pub filename: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub to_eof: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplitPlan {
    pub songs: Vec<Song>,
    pub audio: AudioInfo,
    pub warnings: Vec<String>,
}

/// Every validation error at once, plus any warnings gathered before failing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}", errors.join("\n"))]
pub struct PlanFailure {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Seconds -> `M:SS.s` or `H:MM:SS.s` for tables and warnings.
pub fn fmt_time(seconds: f64) -> String {
    todo!()
}

/// Apply every business rule: sort (warn), reject duplicates and
/// out-of-bounds markers, resolve titles, build filenames, compute
/// boundaries (song N ends at marker N+1; last runs to EOF).
pub fn plan(parsed: &ParsedMarkers, audio: &AudioInfo) -> Result<SplitPlan, PlanFailure> {
    todo!()
}

/// Pre-export check: which target files already exist in `outdir`?
/// Stale `.part` files are not collisions (never finished output).
pub fn check_collisions(plan: &SplitPlan, outdir: &Path, overwrite: bool) -> Result<(), Vec<String>> {
    todo!()
}
```

Add inside the tests module (these use small helpers — include them):

```rust
    use crate::markers::{MarkerFormat, RawMarker};

    fn audio(duration: f64) -> AudioInfo {
        AudioInfo {
            path: "/tmp/jam.wav".into(),
            duration_seconds: duration,
            codec_name: "pcm_s16le".to_string(),
            lossless: true,
        }
    }

    fn parsed(markers: &[(f64, &str)]) -> ParsedMarkers {
        ParsedMarkers {
            markers: markers
                .iter()
                .map(|(s, t)| RawMarker { start_seconds: *s, title: t.to_string() })
                .collect(),
            format: MarkerFormat::Plain,
        }
    }

    #[test]
    fn boundaries_song_n_ends_at_marker_n_plus_1_last_at_eof() {
        let p = plan(&parsed(&[(0.0, "One"), (100.0, "Two")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].start_seconds, 0.0);
        assert_eq!(p.songs[0].end_seconds, 100.0);
        assert!(!p.songs[0].to_eof);
        assert_eq!(p.songs[1].end_seconds, 250.0);
        assert!(p.songs[1].to_eof);
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn unsorted_markers_are_sorted_with_warning() {
        let p = plan(&parsed(&[(100.0, "Two"), (0.0, "One")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].title, "One");
        assert_eq!(p.songs[0].track, 1);
        assert!(p.warnings.iter().any(|w| w.contains("sort")));
    }

    #[test]
    fn duplicates_and_out_of_bounds_are_collected_errors() {
        let err = plan(
            &parsed(&[(0.0, "A"), (0.0, "Dup"), (999.0, "Past End")]),
            &audio(250.0),
        )
        .unwrap_err();
        assert_eq!(err.errors.len(), 2);
        assert!(err.errors.iter().any(|e| e.contains("duplicate")));
        assert!(err.errors.iter().any(|e| e.contains("Past End") || e.contains("999")));
    }

    #[test]
    fn marker_exactly_at_duration_is_an_error() {
        assert!(plan(&parsed(&[(0.0, "A"), (250.0, "Empty")]), &audio(250.0)).is_err());
    }

    #[test]
    fn zero_markers_is_an_error() {
        assert!(plan(&parsed(&[]), &audio(250.0)).is_err());
    }

    #[test]
    fn warns_about_skipped_intro_short_songs_and_lossy_input() {
        let mut lossy = audio(250.0);
        lossy.codec_name = "mp3".to_string();
        lossy.lossless = false;
        let p = plan(&parsed(&[(10.0, "A"), (11.0, "Tiny"), (50.0, "B")]), &lossy).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("0:10.0"))); // skipped intro
        assert!(p.warnings.iter().any(|w| w.contains("Tiny")));   // 1s song
        assert!(p.warnings.iter().any(|w| w.contains("approximate"))); // lossy
    }

    #[test]
    fn blank_titles_get_untitled_with_final_track_numbers() {
        let p = plan(&parsed(&[(0.0, ""), (100.0, "")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].title, "Untitled Song 1");
        assert_eq!(p.songs[1].title, "Untitled Song 2");
        assert_eq!(p.songs[1].filename, "02 - Untitled Song 2.mp3");
        assert!(p.warnings.iter().any(|w| w.contains("Untitled")));
    }

    #[test]
    fn fmt_time_formats() {
        assert_eq!(fmt_time(0.0), "0:00.0");
        assert_eq!(fmt_time(323.5), "5:23.5");
        assert_eq!(fmt_time(3731.0), "1:02:11.0");
    }

    #[test]
    fn collisions_respect_overwrite_and_ignore_part_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&parsed(&[(0.0, "One"), (100.0, "Two")]), &audio(250.0)).unwrap();
        // nothing exists yet: fine either way
        assert!(check_collisions(&p, dir.path(), false).is_ok());
        // a stale .part is not a collision
        std::fs::write(dir.path().join("01 - One.mp3.part"), b"junk").unwrap();
        assert!(check_collisions(&p, dir.path(), false).is_ok());
        // a real target file is
        std::fs::write(dir.path().join("01 - One.mp3"), b"old").unwrap();
        let errs = check_collisions(&p, dir.path(), false).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("01 - One.mp3"));
        // unless overwrite
        assert!(check_collisions(&p, dir.path(), true).is_ok());
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core plan`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 4: Implement `fmt_time`, `plan`, `check_collisions`**

```rust
pub fn fmt_time(seconds: f64) -> String {
    let h = (seconds / 3600.0).floor() as u64;
    let m = ((seconds % 3600.0) / 60.0).floor() as u64;
    let s = seconds % 60.0;
    if h > 0 {
        format!("{h}:{m:02}:{s:04.1}")
    } else {
        format!("{m}:{s:04.1}")
    }
}

pub fn plan(parsed: &ParsedMarkers, audio: &AudioInfo) -> Result<SplitPlan, PlanFailure> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if parsed.markers.is_empty() {
        errors.push("no markers found in the marker file".to_string());
        return Err(PlanFailure { errors, warnings });
    }

    let mut markers = parsed.markers.clone();
    let already_sorted = markers.windows(2).all(|w| w[0].start_seconds <= w[1].start_seconds);
    if !already_sorted {
        markers.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
        warnings.push("markers were out of order — auto-sorted by start time".to_string());
    }

    for w in markers.windows(2) {
        if w[0].start_seconds == w[1].start_seconds {
            errors.push(format!(
                "duplicate marker timestamp at {}",
                fmt_time(w[0].start_seconds)
            ));
        }
    }
    for m in &markers {
        if m.start_seconds >= audio.duration_seconds {
            errors.push(format!(
                "marker '{}' at {} is at or past the end of the audio ({})",
                m.title,
                fmt_time(m.start_seconds),
                fmt_time(audio.duration_seconds)
            ));
        }
    }
    if !errors.is_empty() {
        return Err(PlanFailure { errors, warnings });
    }

    if markers[0].start_seconds > 0.0 {
        warnings.push(format!(
            "first marker is at {} — the first {} of audio will not be exported (add a 0:00 marker to keep it)",
            fmt_time(markers[0].start_seconds),
            fmt_time(markers[0].start_seconds)
        ));
    }
    if !audio.lossless {
        warnings.push(format!(
            "input codec '{}' is lossy — split points may be approximate",
            audio.codec_name
        ));
    }

    let total = markers.len();
    let songs: Vec<Song> = markers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let track = i + 1;
            let to_eof = i == total - 1;
            let end_seconds = if to_eof {
                audio.duration_seconds
            } else {
                markers[i + 1].start_seconds
            };
            let title = resolve_title(&m.title, track);
            if m.title.trim().is_empty() {
                warnings.push(format!("marker {track} has no title — using '{title}'"));
            }
            let filename = filename_for(track, total, &title);
            Song { track, title, filename, start_seconds: m.start_seconds, end_seconds, to_eof }
        })
        .collect();

    for song in &songs {
        let len = song.end_seconds - song.start_seconds;
        if len < 2.0 {
            warnings.push(format!(
                "song {} '{}' is only {len:.1}s long — stray marker?",
                song.track, song.title
            ));
        }
    }

    Ok(SplitPlan { songs, audio: audio.clone(), warnings })
}

pub fn check_collisions(plan: &SplitPlan, outdir: &Path, overwrite: bool) -> Result<(), Vec<String>> {
    if overwrite {
        return Ok(());
    }
    let collisions: Vec<String> = plan
        .songs
        .iter()
        .filter(|s| outdir.join(&s.filename).exists())
        .map(|s| format!("would overwrite existing file: {}", outdir.join(&s.filename).display()))
        .collect();
    if collisions.is_empty() { Ok(()) } else { Err(collisions) }
}
```

Note the closure mutating `warnings` while mapping — if the borrow checker objects, build songs with a plain `for` loop pushing into a `Vec` instead; behavior must match the tests either way.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add split planning with validation rules"
```

---

### Task 9: ffmpeg/ffprobe location

Resolution order from the design doc: explicit `--ffmpeg-path` → binaries adjacent to our own executable → PATH. The adjacent step is what makes future "batteries included" zips a pure packaging change — do not remove it. Location checks file existence only; a non-executable file will fail loudly at spawn time, which is fine.

**Files:**
- Create: `crates/jamsplit-core/src/ffmpeg.rs`
- Modify: `crates/jamsplit-core/src/lib.rs` (add `pub mod ffmpeg;`)

- [ ] **Step 1: Create `ffmpeg.rs` with failing tests, add `pub mod ffmpeg;` to `lib.rs`**

```rust
use std::path::{Path, PathBuf};

/// Resolved locations of the two binaries we drive.
#[derive(Debug, Clone, PartialEq)]
pub struct FfmpegPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LocateError {
    #[error("--ffmpeg-path {0} does not exist")]
    ExplicitNotFound(PathBuf),
    #[error("found ffmpeg at {0}, but no ffprobe next to it — both are required")]
    FfprobeMissingNextToExplicit(PathBuf),
    #[error(
        "ffmpeg/ffprobe not found (tried --ffmpeg-path, next to this executable, and PATH).\n\
         Install ffmpeg:\n\
         \x20 macOS:   brew install ffmpeg\n\
         \x20 Windows: winget install Gyan.FFmpeg\n\
         \x20 Linux:   your package manager (apt/dnf/pacman) install ffmpeg\n\
         or point at a binary with --ffmpeg-path, or place ffmpeg and ffprobe next to jamsplit."
    )]
    NotFound,
}

/// Platform-correct executable name.
fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

impl FfmpegPaths {
    /// Resolve with real process context (current_exe dir, PATH).
    pub fn locate(explicit: Option<&Path>) -> Result<Self, LocateError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self::locate_with(explicit, exe_dir.as_deref(), std::env::var_os("PATH").as_deref())
    }

    /// Injectable core, unit-testable without touching the real environment.
    pub fn locate_with(
        explicit: Option<&Path>,
        exe_dir: Option<&Path>,
        path_var: Option<&std::ffi::OsStr>,
    ) -> Result<Self, LocateError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(exe(name));
        File::create(&p).unwrap();
        p
    }

    #[test]
    fn explicit_path_with_sibling_ffprobe() {
        let dir = tempfile::tempdir().unwrap();
        let ffmpeg = touch(dir.path(), "ffmpeg");
        let ffprobe = touch(dir.path(), "ffprobe");
        let got = FfmpegPaths::locate_with(Some(&ffmpeg), None, None).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg, ffprobe });
    }

    #[test]
    fn explicit_path_missing_ffprobe_is_a_specific_error() {
        let dir = tempfile::tempdir().unwrap();
        let ffmpeg = touch(dir.path(), "ffmpeg");
        let err = FfmpegPaths::locate_with(Some(&ffmpeg), None, None).unwrap_err();
        assert!(matches!(err, LocateError::FfprobeMissingNextToExplicit(_)));
    }

    #[test]
    fn explicit_path_that_does_not_exist() {
        let err =
            FfmpegPaths::locate_with(Some(Path::new("/nope/ffmpeg")), None, None).unwrap_err();
        assert!(matches!(err, LocateError::ExplicitNotFound(_)));
    }

    #[test]
    fn adjacent_dir_wins_over_path() {
        let adjacent = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        let adj_ffmpeg = touch(adjacent.path(), "ffmpeg");
        let adj_ffprobe = touch(adjacent.path(), "ffprobe");
        touch(on_path.path(), "ffmpeg");
        touch(on_path.path(), "ffprobe");
        let path_var = std::env::join_paths([on_path.path()]).unwrap();
        let got = FfmpegPaths::locate_with(None, Some(adjacent.path()), Some(&path_var)).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg: adj_ffmpeg, ffprobe: adj_ffprobe });
    }

    #[test]
    fn path_search_allows_split_directories() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ffmpeg = touch(a.path(), "ffmpeg");
        let ffprobe = touch(b.path(), "ffprobe");
        let path_var = std::env::join_paths([a.path(), b.path()]).unwrap();
        let got = FfmpegPaths::locate_with(None, None, Some(&path_var)).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg, ffprobe });
    }

    #[test]
    fn nothing_found_mentions_the_flag() {
        let err = FfmpegPaths::locate_with(None, None, None).unwrap_err();
        assert!(err.to_string().contains("--ffmpeg-path"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core ffmpeg`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `locate_with`**

```rust
    pub fn locate_with(
        explicit: Option<&Path>,
        exe_dir: Option<&Path>,
        path_var: Option<&std::ffi::OsStr>,
    ) -> Result<Self, LocateError> {
        if let Some(ffmpeg) = explicit {
            if !ffmpeg.is_file() {
                return Err(LocateError::ExplicitNotFound(ffmpeg.to_path_buf()));
            }
            let ffprobe = ffmpeg
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(exe("ffprobe"));
            if !ffprobe.is_file() {
                return Err(LocateError::FfprobeMissingNextToExplicit(ffmpeg.to_path_buf()));
            }
            return Ok(Self { ffmpeg: ffmpeg.to_path_buf(), ffprobe });
        }

        if let Some(dir) = exe_dir {
            let ffmpeg = dir.join(exe("ffmpeg"));
            let ffprobe = dir.join(exe("ffprobe"));
            if ffmpeg.is_file() && ffprobe.is_file() {
                return Ok(Self { ffmpeg, ffprobe });
            }
        }

        if let Some(path_var) = path_var {
            let find = |name: &str| {
                std::env::split_paths(path_var)
                    .map(|d| d.join(exe(name)))
                    .find(|p| p.is_file())
            };
            if let (Some(ffmpeg), Some(ffprobe)) = (find("ffmpeg"), find("ffprobe")) {
                return Ok(Self { ffmpeg, ffprobe });
            }
        }

        Err(LocateError::NotFound)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add ffmpeg/ffprobe location with resolution order"
```

---

### Task 10: Audio probing and the shared test helpers

`probe_audio` runs ffprobe and fills `AudioInfo`. The JSON parsing is a pure function so the interesting cases are unit-testable without ffprobe. This task also creates the integration-test scaffolding every later ffmpeg-touching test uses: `ffmpeg_or_skip()` and `make_wav()`.

**Files:**
- Modify: `crates/jamsplit-core/src/audio.rs`
- Create: `crates/jamsplit-core/tests/common/mod.rs`
- Create: `crates/jamsplit-core/tests/export_integration.rs` (hosts ALL ffmpeg-dependent core tests, probe included)

- [ ] **Step 1: Add failing unit tests and stubs to `audio.rs`**

```rust
use crate::ffmpeg::FfmpegPaths;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("audio file not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("ffprobe failed on {path}: {stderr}")]
    ProbeFailed { path: std::path::PathBuf, stderr: String },
    #[error("{0} has no audio stream")]
    NoAudioStream(std::path::PathBuf),
    #[error("could not read a duration from {0}")]
    NoDuration(std::path::PathBuf),
}

/// Lossless codecs get sample-accurate seeking; everything else warns.
fn is_lossless(codec: &str) -> bool {
    codec.starts_with("pcm_") || matches!(codec, "flac" | "alac")
}

/// Parse `ffprobe -of json` output (pure, unit-testable).
pub fn parse_ffprobe_output(json: &str, path: &Path) -> Result<AudioInfo, AudioError> {
    todo!()
}

/// Run ffprobe on `path` and build an `AudioInfo`.
pub fn probe_audio(ffmpeg: &FfmpegPaths, path: &Path) -> Result<AudioInfo, AudioError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wav_probe_output() {
        let json = r#"{"streams":[{"codec_name":"pcm_s16le"}],"format":{"duration":"10.005333"}}"#;
        let info = parse_ffprobe_output(json, Path::new("/tmp/jam.wav")).unwrap();
        assert_eq!(info.codec_name, "pcm_s16le");
        assert!(info.lossless);
        assert!((info.duration_seconds - 10.005333).abs() < 1e-9);
    }

    #[test]
    fn mp3_is_lossy_flac_is_not() {
        let mp3 = r#"{"streams":[{"codec_name":"mp3"}],"format":{"duration":"5.0"}}"#;
        assert!(!parse_ffprobe_output(mp3, Path::new("a.mp3")).unwrap().lossless);
        let flac = r#"{"streams":[{"codec_name":"flac"}],"format":{"duration":"5.0"}}"#;
        assert!(parse_ffprobe_output(flac, Path::new("a.flac")).unwrap().lossless);
    }

    #[test]
    fn no_streams_and_no_duration_are_specific_errors() {
        let none = r#"{"streams":[],"format":{"duration":"5.0"}}"#;
        assert!(matches!(
            parse_ffprobe_output(none, Path::new("x")).unwrap_err(),
            AudioError::NoAudioStream(_)
        ));
        let nodur = r#"{"streams":[{"codec_name":"pcm_s16le"}],"format":{}}"#;
        assert!(matches!(
            parse_ffprobe_output(nodur, Path::new("x")).unwrap_err(),
            AudioError::NoDuration(_)
        ));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core audio`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement `parse_ffprobe_output` and `probe_audio`**

```rust
pub fn parse_ffprobe_output(json: &str, path: &Path) -> Result<AudioInfo, AudioError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| AudioError::ProbeFailed {
        path: path.to_path_buf(),
        stderr: format!("unparseable ffprobe output: {e}"),
    })?;
    let codec_name = v["streams"][0]["codec_name"]
        .as_str()
        .ok_or_else(|| AudioError::NoAudioStream(path.to_path_buf()))?
        .to_string();
    let duration_seconds: f64 = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .filter(|d: &f64| *d > 0.0)
        .ok_or_else(|| AudioError::NoDuration(path.to_path_buf()))?;
    let lossless = is_lossless(&codec_name);
    Ok(AudioInfo { path: path.to_path_buf(), duration_seconds, codec_name, lossless })
}

pub fn probe_audio(ffmpeg: &FfmpegPaths, path: &Path) -> Result<AudioInfo, AudioError> {
    if !path.is_file() {
        return Err(AudioError::NotFound(path.to_path_buf()));
    }
    let output = std::process::Command::new(&ffmpeg.ffprobe)
        .args([
            "-v", "error",
            "-select_streams", "a:0",
            "-show_entries", "stream=codec_name",
            "-show_entries", "format=duration",
            "-of", "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| AudioError::ProbeFailed {
            path: path.to_path_buf(),
            stderr: format!("could not run ffprobe: {e}"),
        })?;
    if !output.status.success() {
        return Err(AudioError::ProbeFailed {
            path: path.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    parse_ffprobe_output(&String::from_utf8_lossy(&output.stdout), path)
}
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Create `tests/common/mod.rs` (the gating + fixture helpers)**

```rust
use jamsplit_core::ffmpeg::FfmpegPaths;
use std::path::{Path, PathBuf};

/// Locate ffmpeg or skip the calling test with a notice. CI sets
/// JAMSPLIT_TEST_REQUIRE_FFMPEG=1 to turn a skip into a failure.
pub fn ffmpeg_or_skip() -> Option<FfmpegPaths> {
    match FfmpegPaths::locate(None) {
        Ok(paths) => Some(paths),
        Err(_) => {
            if std::env::var_os("JAMSPLIT_TEST_REQUIRE_FFMPEG").is_some() {
                panic!("ffmpeg is required (JAMSPLIT_TEST_REQUIRE_FFMPEG is set) but was not found");
            }
            eprintln!("skipping: ffmpeg not available on this machine");
            None
        }
    }
}

/// Generate a small sine-wave WAV fixture for end-to-end tests.
pub fn make_wav(ff: &FfmpegPaths, dir: &Path, seconds: f64) -> PathBuf {
    let path = dir.join("fixture.wav");
    let status = std::process::Command::new(&ff.ffmpeg)
        .args(["-y", "-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={seconds}"))
        .args(["-ar", "44100", "-ac", "1"])
        .arg(&path)
        .status()
        .expect("could not run ffmpeg to build fixture");
    assert!(status.success(), "fixture generation failed");
    path
}
```

- [ ] **Step 6: Create `tests/export_integration.rs` with the probe round-trip tests**

```rust
mod common;

use common::{ffmpeg_or_skip, make_wav};
use jamsplit_core::audio::{probe_audio, AudioError};

#[test]
fn probes_a_real_wav() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let info = probe_audio(&ff, &wav).unwrap();
    assert!((info.duration_seconds - 10.0).abs() < 0.1, "duration: {}", info.duration_seconds);
    assert!(info.lossless);
    assert!(info.codec_name.starts_with("pcm_"));
}

#[test]
fn probe_missing_file_is_not_found() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let err = probe_audio(&ff, std::path::Path::new("/no/such/file.wav")).unwrap_err();
    assert!(matches!(err, AudioError::NotFound(_)));
}

#[test]
fn probe_non_audio_file_fails() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.wav");
    std::fs::write(&junk, b"this is not audio").unwrap();
    assert!(probe_audio(&ff, &junk).is_err());
}
```

- [ ] **Step 7: Run the full suite including integration**

Run: `cargo test -p jamsplit-core`
Expected: all PASS (integration tests run for real — ffmpeg is installed on this machine; if it weren't, they'd print the skip notice and pass vacuously).

- [ ] **Step 8: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add ffprobe audio probing and integration test helpers"
```

---

### Task 11: ffmpeg argument construction (pure)

The exact per-song invocation from the design doc, as a pure function so the argv is unit-testable: input-seek before `-i`, `-map_metadata -1` to strip Zoom BWF/iXML junk, libmp3lame V0, metadata flags, explicit `-f mp3` because the output goes to a `.part` path.

**Files:**
- Modify: `crates/jamsplit-core/src/ffmpeg.rs`

- [ ] **Step 1: Add types and failing tests to `ffmpeg.rs`**

Add above the tests module:

```rust
use crate::plan::Song;
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared cancel flag. The GUI's Cancel button sets it; the CLI passes one
/// that is never set.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub outdir: std::path::PathBuf,
    pub album: Option<String>,
    pub artist: Option<String>,
    /// Callers must run `plan::check_collisions` first; export itself
    /// renames over existing targets without asking.
    pub overwrite: bool,
    pub cancel: CancelToken,
}

/// The `.part` path a song is encoded into before the success-rename.
pub fn part_path(opts: &ExportOptions, song: &Song) -> std::path::PathBuf {
    opts.outdir.join(format!("{}.part", song.filename))
}

/// Build the exact ffmpeg argv for one song (everything after the program
/// name). Pure — unit-tested against the design doc's invocation.
pub fn build_song_args(
    audio: &Path,
    song: &Song,
    total: usize,
    opts: &ExportOptions,
) -> Vec<OsString> {
    todo!()
}
```

Add inside the tests module:

```rust
    use crate::plan::Song;

    fn song(track: usize, title: &str, start: f64, end: f64, to_eof: bool) -> Song {
        Song {
            track,
            title: title.to_string(),
            filename: crate::plan::filename_for(track, 3, title),
            start_seconds: start,
            end_seconds: end,
            to_eof,
        }
    }

    fn opts(album: Option<&str>, artist: Option<&str>) -> ExportOptions {
        ExportOptions {
            outdir: "/out".into(),
            album: album.map(String::from),
            artist: artist.map(String::from),
            overwrite: false,
            cancel: CancelToken::new(),
        }
    }

    #[test]
    fn middle_song_args_match_the_design() {
        let s = song(2, "AC/DC Jam", 323.5, 410.0, false);
        let got = build_song_args(Path::new("/in/jam.wav"), &s, 3, &opts(Some("Practice"), None));
        let want: Vec<OsString> = [
            "-hide_banner", "-nostdin", "-v", "error", "-y",
            "-ss", "323.5", "-t", "86.5",
            "-i", "/in/jam.wav",
            "-map_metadata", "-1",
            "-c:a", "libmp3lame", "-q:a", "0",
            "-metadata", "title=AC/DC Jam", // tag keeps the slash — only filenames sanitize
            "-metadata", "track=2/3",
            "-metadata", "album=Practice",
            "-f", "mp3",
            "/out/02 - AC_DC Jam.mp3.part",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn last_song_omits_duration() {
        let s = song(3, "Closer", 410.0, 600.0, true);
        let got = build_song_args(Path::new("/in/jam.wav"), &s, 3, &opts(None, Some("The Band")));
        let joined: Vec<String> = got.iter().map(|o| o.to_string_lossy().into_owned()).collect();
        assert!(!joined.contains(&"-t".to_string()));
        assert!(joined.contains(&"artist=The Band".to_string()));
        assert!(!joined.iter().any(|a| a.starts_with("album=")));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core song_args`
Expected: FAIL — `todo!()` panics. (`cargo test -p jamsplit-core last_song` covers the second.)

- [ ] **Step 3: Implement `build_song_args`**

```rust
pub fn build_song_args(
    audio: &Path,
    song: &Song,
    total: usize,
    opts: &ExportOptions,
) -> Vec<OsString> {
    let mut args: Vec<OsString> =
        ["-hide_banner", "-nostdin", "-v", "error", "-y", "-ss"]
            .iter()
            .map(OsString::from)
            .collect();
    args.push(song.start_seconds.to_string().into());
    if !song.to_eof {
        args.push("-t".into());
        args.push((song.end_seconds - song.start_seconds).to_string().into());
    }
    args.push("-i".into());
    args.push(audio.as_os_str().to_os_string());
    for s in ["-map_metadata", "-1", "-c:a", "libmp3lame", "-q:a", "0"] {
        args.push(s.into());
    }
    args.push("-metadata".into());
    args.push(format!("title={}", song.title).into());
    args.push("-metadata".into());
    args.push(format!("track={}/{total}", song.track).into());
    if let Some(album) = &opts.album {
        args.push("-metadata".into());
        args.push(format!("album={album}").into());
    }
    if let Some(artist) = &opts.artist {
        args.push("-metadata".into());
        args.push(format!("artist={artist}").into());
    }
    args.push("-f".into());
    args.push("mp3".into());
    args.push(part_path(opts, song).into_os_string());
    args
}
```

The argv in the test is the contract — if anything drifts, fix the implementation to match the test.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add per-song ffmpeg argument construction"
```

---

### Task 12: export() — run, rename, continue-on-error, cancel

The executor: one ffmpeg child per song, `.part` then rename, last ~15 stderr lines kept on failure, keep going after failures, cancel kills the current child and skips the rest. All tests here are integration (real ffmpeg).

**Files:**
- Modify: `crates/jamsplit-core/src/ffmpeg.rs`
- Modify: `crates/jamsplit-core/tests/export_integration.rs`

- [ ] **Step 1: Add the result types and `export` stub to `ffmpeg.rs`**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SongStatus {
    Ok,
    Failed { stderr_tail: String },
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongResult {
    pub track: usize,
    pub title: String,
    /// Final (post-rename) path the song was or would have been written to.
    pub file: std::path::PathBuf,
    pub status: SongStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportReport {
    pub results: Vec<SongResult>,
    pub canceled: bool,
}

impl ExportReport {
    pub fn any_failed(&self) -> bool {
        self.results.iter().any(|r| matches!(r.status, SongStatus::Failed { .. }))
    }
}

/// Export every song in the plan. Creates `opts.outdir` if needed. Calls
/// `on_progress` after each song settles (ok, failed, or skipped).
pub fn export(
    plan: &crate::plan::SplitPlan,
    ffmpeg: &FfmpegPaths,
    opts: &ExportOptions,
    on_progress: &mut dyn FnMut(&SongResult),
) -> std::io::Result<ExportReport> {
    todo!()
}
```

- [ ] **Step 2: Add the failing integration tests to `tests/export_integration.rs`**

```rust
use jamsplit_core::ffmpeg::{export, CancelToken, ExportOptions, SongStatus};
use jamsplit_core::markers::parse_markers;
use jamsplit_core::plan::plan;
use std::path::Path;

fn read_tags(ff: &jamsplit_core::ffmpeg::FfmpegPaths, path: &Path) -> serde_json::Value {
    let out = std::process::Command::new(&ff.ffprobe)
        .args(["-v", "error", "-show_entries", "format_tags=title,track,album,artist", "-of", "json"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["format"]["tags"].clone()
}

fn opts(outdir: &Path, overwrite: bool) -> ExportOptions {
    ExportOptions {
        outdir: outdir.to_path_buf(),
        album: Some("Practice 2026-06-05".to_string()),
        artist: Some("The Band".to_string()),
        overwrite,
        cancel: CancelToken::new(),
    }
}

#[test]
fn full_split_files_durations_tags_progress() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 AC/DC Jam\n3.0 Slow Blues\n6.5\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let mut seen = Vec::new();
    let report = export(&p, &ff, &opts(&outdir, false), &mut |r| seen.push(r.track)).unwrap();

    assert_eq!(seen, vec![1, 2, 3]);
    assert!(!report.any_failed() && !report.canceled);

    let f1 = outdir.join("01 - AC_DC Jam.mp3"); // filename sanitized
    let f3 = outdir.join("03 - Untitled Song 3.mp3"); // blank title resolved
    assert!(f1.is_file() && f3.is_file());
    assert!(!outdir.join("01 - AC_DC Jam.mp3.part").exists());

    let d1 = probe_audio(&ff, &f1).unwrap().duration_seconds;
    let d3 = probe_audio(&ff, &f3).unwrap().duration_seconds;
    assert!((d1 - 3.0).abs() < 0.1, "song 1 duration {d1}");
    assert!((d3 - 3.5).abs() < 0.1, "song 3 duration {d3}");

    let tags = read_tags(&ff, &f1);
    assert_eq!(tags["title"], "AC/DC Jam"); // tag keeps the slash
    assert_eq!(tags["track"], "1/3");
    assert_eq!(tags["album"], "Practice 2026-06-05");
    assert_eq!(tags["artist"], "The Band");
}

#[test]
fn one_failure_does_not_stop_the_rest() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n3.0 Two\n6.5 Three\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    // make song 2's .part path unwritable by occupying it with a directory
    std::fs::create_dir_all(outdir.join("02 - Two.mp3.part")).unwrap();

    let report = export(&p, &ff, &opts(&outdir, false), &mut |_| {}).unwrap();
    assert!(matches!(&report.results[1].status, SongStatus::Failed { stderr_tail } if !stderr_tail.is_empty()));
    assert!(matches!(report.results[0].status, SongStatus::Ok));
    assert!(matches!(report.results[2].status, SongStatus::Ok));
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(outdir.join("03 - Three.mp3").is_file());
    assert!(report.any_failed());
}

#[test]
fn cancel_after_first_song_skips_the_rest() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n3.0 Two\n6.5 Three\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let o = opts(&outdir, false);
    let cancel = o.cancel.clone();
    let report = export(&p, &ff, &o, &mut |r| {
        if r.track == 1 {
            cancel.cancel();
        }
    })
    .unwrap();

    assert!(report.canceled);
    assert!(matches!(report.results[0].status, SongStatus::Ok));
    assert!(matches!(report.results[1].status, SongStatus::Skipped));
    assert!(matches!(report.results[2].status, SongStatus::Skipped));
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(!outdir.join("02 - Two.mp3").exists());
}

#[test]
fn overwrite_true_replaces_existing_outputs() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n5.0 Two\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let first = export(&p, &ff, &opts(&outdir, false), &mut |_| {}).unwrap();
    assert!(!first.any_failed());
    let second = export(&p, &ff, &opts(&outdir, true), &mut |_| {}).unwrap();
    assert!(!second.any_failed());
    assert!(outdir.join("01 - One.mp3").is_file());
}
```

Also extend the imports at the top of the file from Task 10 so both test groups share them — final import block:

```rust
mod common;

use common::{ffmpeg_or_skip, make_wav};
use jamsplit_core::audio::{probe_audio, AudioError};
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core --test export_integration`
Expected: FAIL — `todo!()` panics in `export`.

- [ ] **Step 4: Implement `export`**

```rust
pub fn export(
    plan: &crate::plan::SplitPlan,
    ffmpeg: &FfmpegPaths,
    opts: &ExportOptions,
    on_progress: &mut dyn FnMut(&SongResult),
) -> std::io::Result<ExportReport> {
    use std::io::Read;

    std::fs::create_dir_all(&opts.outdir)?;
    let total = plan.songs.len();
    let mut results = Vec::with_capacity(total);
    let mut canceled = false;

    for song in &plan.songs {
        let target = opts.outdir.join(&song.filename);
        if canceled || opts.cancel.is_canceled() {
            canceled = true;
            let result = SongResult {
                track: song.track,
                title: song.title.clone(),
                file: target,
                status: SongStatus::Skipped,
            };
            on_progress(&result);
            results.push(result);
            continue;
        }

        let part = part_path(opts, song);
        let status = match run_one(ffmpeg, &plan.audio.path, song, total, opts, &part) {
            RunOutcome::Done => {
                // Windows cannot rename over an existing file
                if opts.overwrite && target.exists() {
                    let _ = std::fs::remove_file(&target);
                }
                match std::fs::rename(&part, &target) {
                    Ok(()) => SongStatus::Ok,
                    Err(e) => SongStatus::Failed { stderr_tail: format!("rename failed: {e}") },
                }
            }
            RunOutcome::Failed(stderr_tail) => {
                let _ = std::fs::remove_file(&part);
                SongStatus::Failed { stderr_tail }
            }
            RunOutcome::Canceled => {
                let _ = std::fs::remove_file(&part);
                canceled = true;
                SongStatus::Skipped
            }
        };

        let result = SongResult {
            track: song.track,
            title: song.title.clone(),
            file: target,
            status,
        };
        on_progress(&result);
        results.push(result);
    }

    Ok(ExportReport { results, canceled })
}

enum RunOutcome {
    Done,
    Failed(String),
    Canceled,
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn run_one(
    ffmpeg: &FfmpegPaths,
    audio: &Path,
    song: &crate::plan::Song,
    total: usize,
    opts: &ExportOptions,
    part: &Path,
) -> RunOutcome {
    use std::io::Read;

    let mut child = match std::process::Command::new(&ffmpeg.ffmpeg)
        .args(build_song_args(audio, song, total, opts))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return RunOutcome::Failed(format!("could not start ffmpeg: {e}")),
    };

    let exit = loop {
        if opts.cancel.is_canceled() {
            let _ = child.kill();
            let _ = child.wait();
            return RunOutcome::Canceled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return RunOutcome::Failed(format!("waiting on ffmpeg failed: {e}")),
        }
    };

    // -v error keeps stderr tiny, so reading after exit cannot deadlock on a
    // full pipe (it fits in the OS pipe buffer)
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }

    if exit.success() {
        RunOutcome::Done
    } else {
        RunOutcome::Failed(last_lines(stderr.trim(), 15))
    }
}
```

(`use std::io::Read;` appears in both functions — keep one at module scope instead if clippy complains.)

- [ ] **Step 5: Run the integration suite to verify it passes**

Run: `cargo test -p jamsplit-core --test export_integration`
Expected: all PASS (4 new tests + the 3 probe tests).

- [ ] **Step 6: Run the full suite**

Run: `cargo test -p jamsplit-core`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add per-song export with rename, failure isolation, and cancel"
```

---

### Task 13: Summary and table rendering

`report.rs`: the serde `Summary` written as `jamsplit-summary.json` (after real splits only — including partially failed and canceled ones) and the human track table. Core returns strings/structs; frontends decide where they go.

**Files:**
- Create: `crates/jamsplit-core/src/report.rs`
- Modify: `crates/jamsplit-core/src/lib.rs` (add `pub mod report;`)

- [ ] **Step 1: Create `report.rs` with failing tests, add `pub mod report;` to `lib.rs`**

```rust
use crate::ffmpeg::{ExportReport, SongStatus};
use crate::plan::{fmt_time, SplitPlan};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Summary {
    pub source_audio: PathBuf,
    pub markers_file: PathBuf,
    pub format: String,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub tool_version: String,
    pub warnings: Vec<String>,
    pub songs: Vec<SongSummary>,
}

#[derive(Debug, Serialize)]
pub struct SongSummary {
    pub track: usize,
    pub title: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub file: PathBuf,
    /// "ok" | "failed" | "skipped"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Combine the plan and the export outcomes into the summary structure.
pub fn build_summary(
    plan: &SplitPlan,
    report: &ExportReport,
    markers_file: &Path,
    format: &str,
    album: Option<&str>,
    artist: Option<&str>,
) -> Summary {
    todo!()
}

/// Write `jamsplit-summary.json` into the outdir, pretty-printed.
pub fn write_summary(summary: &Summary, outdir: &Path) -> std::io::Result<PathBuf> {
    todo!()
}

/// Human track table for inspect/dry-run/post-split output.
pub fn render_table(plan: &SplitPlan) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioInfo;
    use crate::ffmpeg::{SongResult, SongStatus};
    use crate::plan::Song;

    fn test_plan() -> SplitPlan {
        let audio = AudioInfo {
            path: "/in/jam.wav".into(),
            duration_seconds: 600.0,
            codec_name: "pcm_s16le".to_string(),
            lossless: true,
        };
        let songs = vec![
            Song { track: 1, title: "One".into(), filename: "01 - One.mp3".into(),
                   start_seconds: 0.0, end_seconds: 323.5, to_eof: false },
            Song { track: 2, title: "Two".into(), filename: "02 - Two.mp3".into(),
                   start_seconds: 323.5, end_seconds: 600.0, to_eof: true },
        ];
        SplitPlan { songs, audio, warnings: vec!["a warning".to_string()] }
    }

    fn test_report() -> ExportReport {
        ExportReport {
            results: vec![
                SongResult { track: 1, title: "One".into(), file: "/out/01 - One.mp3".into(),
                             status: SongStatus::Ok },
                SongResult { track: 2, title: "Two".into(), file: "/out/02 - Two.mp3".into(),
                             status: SongStatus::Failed { stderr_tail: "boom".into() } },
            ],
            canceled: false,
        }
    }

    #[test]
    fn summary_carries_statuses_and_errors() {
        let s = build_summary(&test_plan(), &test_report(), Path::new("/in/markers.txt"),
                              "plain", Some("Album"), None);
        assert_eq!(s.tool_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(s.songs[0].status, "ok");
        assert_eq!(s.songs[0].error, None);
        assert_eq!(s.songs[1].status, "failed");
        assert_eq!(s.songs[1].error.as_deref(), Some("boom"));
        assert_eq!(s.warnings, vec!["a warning".to_string()]);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"track\":1"));
        assert!(!json.contains("\"error\":null")); // skipped when None
    }

    #[test]
    fn write_summary_creates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = build_summary(&test_plan(), &test_report(), Path::new("/in/markers.txt"),
                              "plain", None, None);
        let path = write_summary(&s, dir.path()).unwrap();
        assert_eq!(path.file_name().unwrap(), "jamsplit-summary.json");
        let read: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read["songs"][1]["status"], "failed");
    }

    #[test]
    fn table_lists_every_song_with_times() {
        let t = render_table(&test_plan());
        let expected = "\
track  start      end        length     title
   1   0:00.0     5:23.5     5:23.5     One
   2   5:23.5     10:00.0    4:36.5     Two
";
        assert_eq!(t, expected);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p jamsplit-core report`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement the three functions**

```rust
pub fn build_summary(
    plan: &SplitPlan,
    report: &ExportReport,
    markers_file: &Path,
    format: &str,
    album: Option<&str>,
    artist: Option<&str>,
) -> Summary {
    let songs = plan
        .songs
        .iter()
        .zip(&report.results)
        .map(|(song, result)| {
            let (status, error) = match &result.status {
                SongStatus::Ok => ("ok", None),
                SongStatus::Failed { stderr_tail } => ("failed", Some(stderr_tail.clone())),
                SongStatus::Skipped => ("skipped", None),
            };
            SongSummary {
                track: song.track,
                title: song.title.clone(),
                start_seconds: song.start_seconds,
                end_seconds: song.end_seconds,
                duration_seconds: song.end_seconds - song.start_seconds,
                file: result.file.clone(),
                status: status.to_string(),
                error,
            }
        })
        .collect();
    Summary {
        source_audio: plan.audio.path.clone(),
        markers_file: markers_file.to_path_buf(),
        format: format.to_string(),
        album: album.map(String::from),
        artist: artist.map(String::from),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        warnings: plan.warnings.clone(),
        songs,
    }
}

pub fn write_summary(summary: &Summary, outdir: &Path) -> std::io::Result<PathBuf> {
    let path = outdir.join("jamsplit-summary.json");
    let json = serde_json::to_string_pretty(summary).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

pub fn render_table(plan: &SplitPlan) -> String {
    let mut out = String::from("track  start      end        length     title\n");
    for song in &plan.songs {
        out.push_str(&format!(
            "{:>4}   {:<10} {:<10} {:<10} {}\n",
            song.track,
            fmt_time(song.start_seconds),
            fmt_time(song.end_seconds),
            fmt_time(song.end_seconds - song.start_seconds),
            song.title
        ));
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p jamsplit-core`
Expected: all PASS. If the table test fails on spacing, fix the *implementation* until it matches the expected string in the test, not the other way around — the column layout in the test is the contract.

- [ ] **Step 5: Commit**

```bash
git add crates/jamsplit-core
git commit -m "Add summary JSON and track table rendering"
```

---

### Task 14: CLI crate — validate and inspect

`jamsplit-cli` is a thin shell: clap types, a shared `load()` pipeline (read markers → locate ffmpeg → probe → plan), and per-subcommand drivers. Output discipline: results on stdout, warnings/errors/announcements on stderr. Exit codes: 0 success, 1 invalid input.

**Files:**
- Create: `crates/jamsplit-cli/Cargo.toml`, `crates/jamsplit-cli/src/main.rs`, `crates/jamsplit-cli/src/cli.rs`
- Create: `crates/jamsplit-cli/tests/common/mod.rs`, `crates/jamsplit-cli/tests/cli_integration.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add the crate to the workspace and create `Cargo.toml`**

Workspace `members` becomes `["crates/jamsplit-core", "crates/jamsplit-cli"]`.

`crates/jamsplit-cli/Cargo.toml`:

```toml
[package]
name = "jamsplit-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "jamsplit"
path = "src/main.rs"

[dependencies]
jamsplit-core = { path = "../jamsplit-core" }
clap = { version = "4", features = ["derive"] }
anyhow = "1"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
serde_json = "1"
```

- [ ] **Step 2: Write the failing CLI integration tests**

`crates/jamsplit-cli/tests/common/mod.rs` — copy of the core helpers (consciously duplicated; a test-util crate is not worth it for ~30 lines):

```rust
use jamsplit_core::ffmpeg::FfmpegPaths;
use std::path::{Path, PathBuf};

pub fn ffmpeg_or_skip() -> Option<FfmpegPaths> {
    match FfmpegPaths::locate(None) {
        Ok(paths) => Some(paths),
        Err(_) => {
            if std::env::var_os("JAMSPLIT_TEST_REQUIRE_FFMPEG").is_some() {
                panic!("ffmpeg is required (JAMSPLIT_TEST_REQUIRE_FFMPEG is set) but was not found");
            }
            eprintln!("skipping: ffmpeg not available on this machine");
            None
        }
    }
}

pub fn make_wav(ff: &FfmpegPaths, dir: &Path, seconds: f64) -> PathBuf {
    let path = dir.join("fixture.wav");
    let status = std::process::Command::new(&ff.ffmpeg)
        .args(["-y", "-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={seconds}"))
        .args(["-ar", "44100", "-ac", "1"])
        .arg(&path)
        .status()
        .expect("could not run ffmpeg to build fixture");
    assert!(status.success(), "fixture generation failed");
    path
}
```

`crates/jamsplit-cli/tests/cli_integration.rs`:

```rust
mod common;

use assert_cmd::Command;
use common::{ffmpeg_or_skip, make_wav};
use predicates::prelude::*; // for .or() on predicates
use std::path::Path;

fn write_markers(dir: &Path, content: &str) -> std::path::PathBuf {
    let p = dir.join("markers.txt");
    std::fs::write(&p, content).unwrap();
    p
}

fn jamsplit() -> Command {
    Command::cargo_bin("jamsplit").unwrap()
}

#[test]
fn validate_ok_announces_format_and_exits_zero() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    jamsplit()
        .args(["validate", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .success()
        .stdout(predicates::str::contains("OK"))
        .stderr(predicates::str::contains("plain"));
}

#[test]
fn validate_duplicate_markers_exits_one() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n0:00 Dup\n");
    jamsplit()
        .args(["validate", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("duplicate"));
}

#[test]
fn validate_missing_audio_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let markers = write_markers(dir.path(), "0:00 One\n");
    jamsplit()
        .args(["validate", "--audio", "/no/such.wav", "--markers"]).arg(&markers)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("not found").or(predicates::str::contains("ffmpeg")));
}

#[test]
fn inspect_prints_the_track_table() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 Opening Jam\n5.0 Closer\n");
    jamsplit()
        .args(["inspect", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .success()
        .stdout(predicates::str::contains("track"))
        .stdout(predicates::str::contains("Opening Jam"));
}

#[test]
fn forced_format_is_respected() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    // looks like Audacity, forced to plain
    let markers = write_markers(dir.path(), "1.5\t2.5\tA\n");
    jamsplit()
        .args(["inspect", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .args(["--format", "plain"])
        .assert()
        .success()
        .stderr(predicates::str::contains("plain"));
}
```

- [ ] **Step 3: Create `src/cli.rs` (clap types + load pipeline + validate/inspect drivers)**

```rust
use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use jamsplit_core::audio::{probe_audio, AudioInfo};
use jamsplit_core::ffmpeg::FfmpegPaths;
use jamsplit_core::markers::{parse_markers, MarkerFormat, ParsedMarkers};
use jamsplit_core::plan::{plan, SplitPlan};
use jamsplit_core::report::render_table;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "jamsplit", version, about = "Split one long jam recording into per-song MP3s")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Export one MP3 per song
    Split(SplitArgs),
    /// Check that the audio + marker pair is usable; writes nothing
    Validate(CommonArgs),
    /// Show the split plan as a table; writes nothing
    Inspect(CommonArgs),
}

fn parse_format(s: &str) -> Result<MarkerFormat, String> {
    s.parse()
}

#[derive(Args)]
pub struct CommonArgs {
    /// Input audio file (WAV expected; anything ffprobe reads is accepted)
    #[arg(long)]
    pub audio: PathBuf,
    /// Marker file (audacity, plain, or reaper format)
    #[arg(long)]
    pub markers: PathBuf,
    /// Force the marker format instead of auto-detecting
    #[arg(long, value_parser = parse_format)]
    pub format: Option<MarkerFormat>,
    /// Path to an ffmpeg binary (ffprobe must sit next to it)
    #[arg(long)]
    pub ffmpeg_path: Option<PathBuf>,
}

#[derive(Args)]
pub struct SplitArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Output directory (default: ./<audio-file-stem>/)
    #[arg(long)]
    pub outdir: Option<PathBuf>,
    /// MP3 album tag (e.g. the session name)
    #[arg(long)]
    pub album: Option<String>,
    /// MP3 artist tag
    #[arg(long)]
    pub artist: Option<String>,
    /// Replace existing output files instead of refusing
    #[arg(long)]
    pub overwrite: bool,
    /// Show what would be exported without writing anything
    #[arg(long)]
    pub dry_run: bool,
}

pub struct Loaded {
    pub ffmpeg: FfmpegPaths,
    pub parsed: ParsedMarkers,
    pub audio: AudioInfo,
    pub plan: SplitPlan,
}

/// The shared pipeline: locate ffmpeg, parse markers, probe audio, plan.
/// Prints warnings and the format announcement to stderr; returns Err with
/// everything already formatted for display.
pub fn load(common: &CommonArgs) -> Result<Loaded> {
    let ffmpeg = FfmpegPaths::locate(common.ffmpeg_path.as_deref())?;

    let content = std::fs::read_to_string(&common.markers)
        .with_context(|| format!("could not read marker file {}", common.markers.display()))?;
    let parsed = parse_markers(&content, common.format).map_err(|errs| {
        let lines: Vec<String> = errs
            .iter()
            .map(|e| format!("{}: {e}", common.markers.display()))
            .collect();
        anyhow!("{}", lines.join("\n"))
    })?;
    let how = if common.format.is_some() { "forced" } else { "auto-detected" };
    eprintln!("marker format: {} ({how})", parsed.format);

    let audio = probe_audio(&ffmpeg, &common.audio)?;

    let split_plan = plan(&parsed, &audio).map_err(|failure| {
        for w in &failure.warnings {
            eprintln!("warning: {w}");
        }
        anyhow!(
            "{}",
            failure.errors.iter().map(|e| format!("error: {e}")).collect::<Vec<_>>().join("\n")
        )
    })?;
    for w in &split_plan.warnings {
        eprintln!("warning: {w}");
    }

    Ok(Loaded { ffmpeg, parsed, audio, plan: split_plan })
}

pub fn validate(args: &CommonArgs) -> Result<()> {
    let loaded = load(args)?;
    let total = loaded.plan.audio.duration_seconds;
    println!(
        "OK: {} songs over {}",
        loaded.plan.songs.len(),
        jamsplit_core::plan::fmt_time(total)
    );
    Ok(())
}

pub fn inspect(args: &CommonArgs) -> Result<()> {
    let loaded = load(args)?;
    print!("{}", render_table(&loaded.plan));
    Ok(())
}
```

- [ ] **Step 4: Create `src/main.rs`**

```rust
mod cli;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let parsed = cli::Cli::parse();
    let result = match &parsed.command {
        cli::Command::Validate(args) => cli::validate(args),
        cli::Command::Inspect(args) => cli::inspect(args),
        cli::Command::Split(_) => unimplemented!("Task 15"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 5: Run the CLI tests (skip the split ones — they don't exist yet)**

Run: `cargo test -p jamsplit-cli`
Expected: the five tests PASS (they exercise validate/inspect only).

- [ ] **Step 6: Run the whole workspace**

Run: `cargo test`
Expected: all PASS across both crates.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/jamsplit-cli
git commit -m "Add jamsplit CLI with validate and inspect"
```

---

### Task 15: CLI split — dry-run, collisions, summary, exit codes

The split driver: default outdir `./<audio-stem>/`, collision gate before anything is written, per-song progress lines, summary written even on partial failure, exit 2 when any song failed.

**Files:**
- Modify: `crates/jamsplit-cli/src/cli.rs`, `crates/jamsplit-cli/src/main.rs`
- Modify: `crates/jamsplit-cli/tests/cli_integration.rs`

- [ ] **Step 1: Add the failing split tests to `cli_integration.rs`**

```rust
#[test]
fn split_dry_run_writes_nothing() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    jamsplit()
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .arg("--outdir").arg(&outdir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicates::str::contains("would"));
    assert!(!outdir.exists(), "dry-run must not create the outdir");
}

#[test]
fn split_produces_files_and_summary() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    jamsplit()
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .arg("--outdir").arg(&outdir)
        .args(["--album", "Practice"])
        .assert()
        .success();
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(outdir.join("02 - Two.mp3").is_file());
    let summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outdir.join("jamsplit-summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(summary["album"], "Practice");
    assert_eq!(summary["songs"][0]["status"], "ok");
}

#[test]
fn split_default_outdir_is_audio_stem() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n");
    jamsplit()
        .current_dir(dir.path())
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .success();
    assert!(dir.path().join("fixture").join("01 - One.mp3").is_file());
}

#[test]
fn split_refuses_collisions_without_overwrite() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::write(outdir.join("01 - One.mp3"), b"old").unwrap();
    jamsplit()
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .arg("--outdir").arg(&outdir)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("01 - One.mp3"))
        .stderr(predicates::str::contains("--overwrite"));
    // the collision gate fires before ANY export happens
    assert!(!outdir.join("02 - Two.mp3").exists());

    jamsplit()
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .arg("--outdir").arg(&outdir)
        .arg("--overwrite")
        .assert()
        .success();
    assert!(outdir.join("02 - Two.mp3").is_file());
}

#[test]
fn split_partial_failure_exits_two_and_still_writes_summary() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n3.0 Two\n6.5 Three\n");
    let outdir = dir.path().join("out");
    // occupy song 2's .part path with a directory to force a failure
    std::fs::create_dir_all(outdir.join("02 - Two.mp3.part")).unwrap();
    jamsplit()
        .args(["split", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .arg("--outdir").arg(&outdir)
        .assert()
        .code(2);
    let summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outdir.join("jamsplit-summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(summary["songs"][1]["status"], "failed");
    assert_eq!(summary["songs"][0]["status"], "ok");
}
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `cargo test -p jamsplit-cli`
Expected: the five split tests FAIL (`unimplemented!("Task 15")` aborts); earlier tests still pass.

- [ ] **Step 3: Implement the split driver in `cli.rs`**

Add to the imports: `use jamsplit_core::ffmpeg::{export, CancelToken, ExportOptions, SongStatus};`, `use jamsplit_core::plan::check_collisions;`, `use jamsplit_core::report::{build_summary, write_summary};`

```rust
/// Exit meaning: Ok(true) = all exports fine, Ok(false) = some failed (exit 2).
pub fn split(args: &SplitArgs) -> Result<bool> {
    let loaded = load(&args.common)?;
    let outdir = args.outdir.clone().unwrap_or_else(|| {
        PathBuf::from(
            args.common.audio.file_stem().map(|s| s.to_os_string()).unwrap_or_else(|| "songs".into()),
        )
    });

    if args.dry_run {
        print!("{}", render_table(&loaded.plan));
        if !outdir.exists() {
            println!("would create directory: {}", outdir.display());
        }
        for song in &loaded.plan.songs {
            let target = outdir.join(&song.filename);
            let collides = if target.exists() { "  (would overwrite)" } else { "" };
            println!("would write: {}{collides}", target.display());
        }
        return Ok(true);
    }

    if let Err(collisions) = check_collisions(&loaded.plan, &outdir, args.overwrite) {
        for c in &collisions {
            eprintln!("error: {c}");
        }
        eprintln!("pass --overwrite to replace existing files");
        anyhow::bail!("refusing to overwrite {} existing file(s)", collisions.len());
    }

    let opts = ExportOptions {
        outdir: outdir.clone(),
        album: args.album.clone(),
        artist: args.artist.clone(),
        overwrite: args.overwrite,
        cancel: CancelToken::new(),
    };
    let total = loaded.plan.songs.len();
    let report = export(&loaded.plan, &loaded.ffmpeg, &opts, &mut |r| {
        let outcome = match &r.status {
            SongStatus::Ok => "ok".to_string(),
            SongStatus::Failed { .. } => "FAILED".to_string(),
            SongStatus::Skipped => "skipped".to_string(),
        };
        println!("[{}/{total}] {} ... {outcome}", r.track, r.file.display());
    })?;

    let summary = build_summary(
        &loaded.plan,
        &report,
        &args.common.markers,
        &loaded.parsed.format.to_string(),
        args.album.as_deref(),
        args.artist.as_deref(),
    );
    let summary_path = write_summary(&summary, &outdir)?;
    println!("summary: {}", summary_path.display());

    if report.any_failed() {
        for r in &report.results {
            if let SongStatus::Failed { stderr_tail } = &r.status {
                eprintln!("error: song {} failed:\n{stderr_tail}", r.track);
            }
        }
        return Ok(false);
    }
    Ok(true)
}
```

- [ ] **Step 4: Wire it in `main.rs`**

Replace the `Command::Split` arm and result handling:

```rust
fn main() -> ExitCode {
    let parsed = cli::Cli::parse();
    match &parsed.command {
        cli::Command::Validate(args) => to_exit(cli::validate(args).map(|()| true), 1),
        cli::Command::Inspect(args) => to_exit(cli::inspect(args).map(|()| true), 1),
        cli::Command::Split(args) => to_exit(cli::split(args), 2),
    }
}

/// Err -> exit 1 (invalid input); Ok(false) -> `partial_failure_code`.
fn to_exit(result: anyhow::Result<bool>, partial_failure_code: u8) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(partial_failure_code),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 5: Run the full workspace suite**

Run: `cargo test`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/jamsplit-cli
git commit -m "Add split command with dry-run, collision gate, and summary"
```

---

### Task 16: README and CLAUDE.md

Documentation lands with the implementation, not after. No test steps here — these are docs.

**Files:**
- Create: `README.md`
- Modify: `CLAUDE.md` (Status and Commands sections)

- [ ] **Step 1: Create `README.md`**

````markdown
# jamsplit

Split one long jam-session recording (usually a WAV from a Zoom recorder) into
one MP3 per song, using a marker file with song start times and titles.

## Requirements

- ffmpeg and ffprobe. jamsplit looks for them in this order: the
  `--ffmpeg-path` flag, next to the jamsplit executable, then PATH.
  - macOS: `brew install ffmpeg`
  - Windows: `winget install Gyan.FFmpeg`
  - Linux: install `ffmpeg` with your package manager

## Usage

```bash
# preview the plan (writes nothing)
jamsplit inspect --audio jam.wav --markers songs.txt

# check a marker file against the audio (exit 0/1, for scripts)
jamsplit validate --audio jam.wav --markers songs.txt

# see exactly what split would do, without doing it
jamsplit split --audio jam.wav --markers songs.txt --dry-run

# export MP3s (default output dir: ./jam/ — the audio file's stem)
jamsplit split --audio jam.wav --markers songs.txt --album "Practice 2026-06-05" --artist "The Band"
```

Songs are numbered in marker order: `01 - Song Title.mp3`, with `title`,
`track`, and optional `album`/`artist` MP3 tags. A `jamsplit-summary.json`
with per-song results lands in the output directory. Existing files are
never overwritten unless you pass `--overwrite`.

Exit codes: `0` success, `1` invalid input (bad markers, missing files),
`2` one or more songs failed to export.

## Marker formats

The format is auto-detected (announced on stderr); force one with
`--format audacity|plain|reaper`.

**Plain text** — hand-written, one song start per line. Times are
`H:MM:SS`, `M:SS`, or raw seconds; fractions allowed; `#` comments and
blank lines ignored; a missing title becomes `Untitled Song N`:

```text
0:00 Opening Jam
05:23 - Slow Blues
1:02:11    Closer
3722.5 Encore Noodle
```

**Audacity labels** — Tracks > Edit Labels > Export Labels (tab-separated).
Only label *start* times are used; range ends are ignored.

**Reaper** — Region/Marker Manager > export. Set the project time unit to
Minutes:Seconds first; bars/beats exports are rejected with a hint. Both
markers (M) and regions (R) are used; region ends are ignored.

Markers mark **song starts only**: song N ends where song N+1 begins, the
last song runs to the end of the file, and audio before the first marker is
not exported (jamsplit warns; add a `0:00` marker to keep it).

## Zoom recorders and split files

Zoom recorders split long sessions into multiple WAVs at 2/4 GB. jamsplit
takes one input file, so concatenate first:

```bash
printf "file '%s'\n" REC0000*.WAV > list.txt
ffmpeg -f concat -safe 0 -i list.txt -c copy session.wav
```

## Development

```bash
cargo test                 # everything; ffmpeg-dependent tests skip if ffmpeg is absent
cargo test -p jamsplit-core
JAMSPLIT_TEST_REQUIRE_FFMPEG=1 cargo test   # what CI runs — skips become failures
```

Design and plans live in `docs/superpowers/`.
````

- [ ] **Step 2: Update `CLAUDE.md`**

Replace the Status section body with:

```markdown
M1 (engine + CLI) implemented. Next: M2 (egui GUI) — needs its own
implementation plan written against the real core API.

- M1 - engine + CLI: done
- M2 - egui GUI; nothing ships to users before M2 is done
- M3 - release packaging CI (out of v1 scope)
```

(Keep the heading but refresh its date to the day this task executes.)

Replace the Commands section body with:

```markdown
- `cargo test` — full suite. ffmpeg-dependent integration tests skip (with a
  notice) when ffmpeg is absent; `JAMSPLIT_TEST_REQUIRE_FFMPEG=1` makes
  skips fail (CI mode).
- `cargo test -p jamsplit-core <name>` — one test.
- `cargo run -p jamsplit-cli -- split --audio x.wav --markers m.txt` — run the CLI.
- `cargo fmt --all` and `cargo clippy --workspace` before finishing a task.
```

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "Add README and update CLAUDE.md for implemented M1"
```

---

### Task 17: Acceptance sweep

Verify the v1 spec's acceptance criteria against reality before calling M1 done. Evidence, not vibes — run everything and check each criterion off against an actual test or command output.

- [ ] **Step 1: Format, lint, full suite**

Run, in order, fixing anything that surfaces:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test
```

Expected: fmt makes no changes (or only trivial ones — commit them), clippy is clean, every test passes with ffmpeg present.

- [ ] **Step 2: Walk the spec's acceptance criteria**

Map each criterion from `docs/spec.md` to its evidence:

- "Works with Audacity label export" → `audacity` parser unit tests + `detection_skips_spectral_lines_like_the_parser_does`
- "Works with plain timestamp text files" → `plain` parser unit tests + `validate_ok_announces_format_and_exits_zero`
- "Specs and supports Reaper marker export" → `reaper` parser unit tests (format documented in README + design doc)
- "Exports numbered MP3s correctly" → `full_split_files_durations_tags_progress`, `split_produces_files_and_summary`
- "Uses marker titles as filenames and MP3 title tags" → `full_split_files_durations_tags_progress` (tag keeps `AC/DC`, filename sanitizes)
- "Produces a dry-run preview and a simple log" → `split_dry_run_writes_nothing`, `split_partial_failure_exits_two_and_still_writes_summary`

For any criterion without green evidence: stop and fix before proceeding.

- [ ] **Step 3: One manual end-to-end smoke test**

```bash
cd "$(mktemp -d)"
ffmpeg -f lavfi -i "sine=frequency=440:duration=30" -ar 44100 -ac 1 jam.wav
printf '0:00 Opening Jam\n0:10 - Slow Blues\n22.5\n' > songs.txt
cargo run --manifest-path <REPO>/Cargo.toml -p jamsplit-cli -- split --audio jam.wav --markers songs.txt --album "Smoke Test"
ls jam/
cat jam/jamsplit-summary.json
```

(Substitute `<REPO>` with the repo path.) Expected: three MP3s including `03 - Untitled Song 3.mp3`, a summary with three `"ok"` songs, format announcement and any warnings on stderr.

- [ ] **Step 4: Commit anything the sweep changed**

```bash
git add -A
git commit -m "Apply formatting and lint fixes from acceptance sweep"
```

(Skip the commit if the sweep changed nothing.)

---

## Out of scope for this plan

- **M2 (egui GUI):** gets its own plan once this one is executed, written against the real core API. The bar for handing the tool to anyone is M2 done.
- **M3 (release CI, ffmpeg-bundled zips, mac .app):** explicitly out of v1.
- A `--mp3-quality` flag, multi-file input, ffmpeg auto-download: all designed out (see the design doc's non-goals).
