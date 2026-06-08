# DAWproject import — design

Date: 2026-06-08
Status: validated, ready for implementation plan

## Goal

Let someone who uses Bitwig Studio split a jam recording with jamsplit by
pointing it at a `.dawproject` file they exported from Bitwig, instead of
hand-writing a marker file. The `.dawproject` carries the song-start cue markers
the user placed in Bitwig; jamsplit reads them as another marker source.

Scope for this iteration: **markers only**. The user still selects their audio
file (the same WAV they worked with in Bitwig) separately. Importing the audio
embedded in or referenced by the `.dawproject` is explicitly out of scope here;
the design leaves room to add it later but does not build it.

## Background: the DAWproject format

DAWproject is an open DAW-interchange format (originated by Bitwig, also read by
Studio One and Cubase). Verified facts used by this design, from the published
schema (`Project.xsd`) and README at github.com/bitwig/dawproject:

- Extension is `.dawproject` (not `.daw`). The container is a **ZIP** holding
  `project.xml` (root element `<Project>`), `metadata.xml`, and optional media.
- Cue markers live at `Project > Arrangement > Markers > Marker`. A `<Marker>`
  has a required `time` (xs:double) and optional `name`, `color`, `comment`.
- Marker times are expressed in the unit named by a `timeUnit` attribute on the
  `<Markers>` element, whose value is `beats` or `seconds`. The schema defines
  no default when the attribute is absent.
- Tempo lives at `Project > Transport > Tempo`, a value with `unit="bpm"`.
- `Arrangement` may contain `TempoAutomation` (a tempo curve). When present, a
  single bpm value is not sufficient to convert beats to seconds.
- Bitwig anchors cue markers musically (they follow tempo), so a Bitwig export
  is expected to use `timeUnit="beats"`. This was inferred, not confirmed
  against a real export — see "Open questions / risks".

## Decided behavior

- **Convert marker times to seconds.** `timeUnit="seconds"` → use the value
  directly. `timeUnit="beats"` → read the single project tempo and compute
  `seconds = time * 60 / bpm`.
- **Require a constant tempo.** If the arrangement contains `TempoAutomation`,
  refuse with a clear message rather than emit silently-wrong cut points. A
  single-tempo conversion is the only conversion supported.
- **Refuse rather than guess when the time unit is unknown.** If `timeUnit` is
  absent from the `<Markers>` element, refuse with a clear message. We do not
  assume a default. (Can be relaxed later if a real Bitwig export proves a
  reliable default.)
- **Markers only.** Audio embedded in or referenced by the project is ignored.

These rules mirror jamsplit's existing posture: the REAPER parser likewise
refuses bars/beats input and tells the user how to fix it.

## Architecture

Approach A: a small, targeted reader in `jamsplit-core` that reads only what it
needs from `project.xml`. No full-format-modeling dependency.

This honors the existing invariant that marker parsers are "dumb: bytes in,
`(start_seconds, title)` out, every business rule lives in `plan()`."
Beats→seconds conversion is part of normalizing the format's own time
representation into seconds — the same kind of work `parse_timestamp` already
does turning `5:23` into `323.0`. It is not a business rule; sorting, dedup,
bounds, untitled naming, and filename sanitization all still happen in `plan()`,
unchanged.

### New dependencies (jamsplit-core)

- `zip` — open the `.dawproject` container in memory and read `project.xml`.
- `roxmltree` — read-only DOM XML parsing with source position info (for error
  line numbers). Pure Rust, light.

Trim `zip` to the compression features actually needed (deflate + store) to keep
the dependency small; `project.xml` may be deflated inside the container.

### New module: `crates/jamsplit-core/src/markers/dawproject.rs`

```rust
pub fn parse(bytes: &[u8]) -> Result<Vec<RawMarker>, Vec<ParseError>>
```

Same signature shape as `audacity::parse` / `plain::parse` / `reaper::parse`,
except it takes `&[u8]` (binary container) rather than `&str`.

Steps:

1. Open `bytes` as a zip from an in-memory cursor. Failure (not a zip) → one
   `ParseError` at line 1 with a clear message.
2. Read the `project.xml` entry to a string. Missing entry → one `ParseError`.
3. Parse the XML with `roxmltree`. Malformed XML → one `ParseError` (use the
   parser's reported position if available).
4. Find `Project > Arrangement`. Missing → `ParseError`.
5. Detect `TempoAutomation` under the arrangement. Present → `ParseError`
   ("tempo automation/changes not supported; jamsplit needs a constant tempo").
6. Find `Arrangement > Markers`. Missing or no `<Marker>` children → an error
   (no markers to split on). Read `timeUnit` from the `<Markers>` element.
   Absent → `ParseError` ("could not determine marker time unit").
7. If `timeUnit="beats"`, read `Project > Transport > Tempo`. Require
   `unit="bpm"` and a finite, strictly-positive `value`. `Tempo` is a
   `realParameter` in the schema, whose `unit` enum permits values other than
   `bpm`, and roxmltree does not schema-validate — so a missing/non-`bpm` unit
   or a non-finite/non-positive value is refused with a clear `ParseError`
   rather than blindly multiplied. Missing `Tempo` element → `ParseError`
   ("markers are in beats but the project has no tempo").
8. For each `<Marker>`: read required `time` (reject missing/non-numeric/negative
   with a per-marker `ParseError` carrying that element's source line), read
   optional `name` (default empty), convert to seconds per the unit, push a
   `RawMarker { start_seconds, title }`.
9. Collect every problem; return `Err(errors)` if any, else `Ok(markers)`. Never
   die on the first bad marker — consistent with the other parsers.

`ParseError.line` is reused as the row in `project.xml` (via roxmltree position
info) where meaningful, falling back to line 1 for container/structural errors,
matching the REAPER parser's file-level convention.

### Routing: `crates/jamsplit-core/src/markers/mod.rs`

- Add `MarkerFormat::Dawproject` (FromStr accepts `dawproject`; Display emits
  `dawproject`).
- Add a bytes-aware entry point:

  ```rust
  pub fn parse_markers_bytes(
      bytes: &[u8],
      format: Option<MarkerFormat>,
  ) -> Result<ParsedMarkers, Vec<ParseError>>
  ```

  Logic: if `format == Some(Dawproject)`, or (auto-detect, i.e. `format` is
  `None`, and) `bytes` begin with the zip magic `PK\x03\x04`, route to
  `dawproject::parse` and report format `Dawproject`. Otherwise decode the bytes
  as UTF-8 (surfacing a clear error if a text format was forced on non-UTF-8
  bytes) and delegate to the existing `parse_markers(content, format)` — the
  text detection and the audacity/plain/reaper paths are untouched.

The existing `parse_markers(&str, ...)`, `detect_format`, and the three text
parsers do not change. `detect_format` stays text-only; binary detection (zip
magic) is handled one layer up in `parse_markers_bytes`, because a zip is not
valid UTF-8 and cannot flow through the string path.

### Frontends

Both frontends do the same two-line change: read the marker file as **bytes**
instead of a UTF-8 string, and call `parse_markers_bytes`.

- CLI `crates/jamsplit-cli/src/cli.rs` (`load`): `std::fs::read` instead of
  `read_to_string`; call `parse_markers_bytes`. Add `Dawproject` to `FormatArg`
  and `into_marker_format`. Update `--format` and `--markers` help text.
- GUI `crates/jamsplit-gui/src/worker.rs` (`run_preview`): same read+call swap.
  Add `Dawproject` to `state.rs` `FormatChoice`, its `ALL` array, and `label`.

Because zip-magic auto-detection routes `.dawproject` files automatically, the
user simply picks the `.dawproject` file and leaves the format on `auto`. No new
button or import wizard is needed for the markers-only flow; the existing
audio-picker + marker-picker UI already covers it. The summary JSON's format
field will read `dawproject` via the normal path.

## Error handling

Every problem is collected and reported together (never die-on-first), matching
the rest of jamsplit. Cases with their own clear message:

- Not a zip / unreadable container.
- `project.xml` missing from the container.
- Malformed `project.xml`.
- No `Arrangement`, or no `Markers`/`<Marker>` elements (nothing to split on).
- `TempoAutomation` present (tempo changes unsupported).
- `timeUnit` absent (unknown unit — refuse, don't guess).
- `timeUnit="beats"` with no `Transport/Tempo`, a non-`bpm` tempo unit, or a
  non-finite/non-positive bpm value.
- Per-marker: missing/non-numeric/negative `time`.

Downstream validation (duplicate times, out-of-bounds vs. audio duration,
zero-length songs, untitled naming) is unchanged — it remains in `plan()`, which
sees the normalized `RawMarker`s exactly as it does for every other format.

## Testing

Unit tests in `dawproject.rs` build small `.dawproject` zips in memory (write a
`project.xml` string into a zip via the `zip` crate — no committed binary
fixtures), covering:

- Seconds markers parse to the right times and titles.
- Beats markers convert correctly with a known bpm (e.g. 120 bpm, beat 4 → 2.0s).
- `TempoAutomation` present → refused with a tempo-changes message.
- `timeUnit="beats"` with no tempo → refused.
- `timeUnit` absent → refused (unknown unit).
- Empty/missing `name` → empty title (so `plan()` names it `Untitled Song N`).
- No markers / no arrangement → error.
- Not a zip → error.
- `project.xml` missing → error.
- Malformed XML → error.
- Multiple bad markers → all reported together, not just the first.

Routing tests in `markers/mod.rs`:

- `parse_markers_bytes` on zip-magic bytes auto-routes to `Dawproject`.
- Forcing `Dawproject` on non-zip bytes errors cleanly.
- Forcing a text format / auto-detecting on text bytes still works (regression).

No-ffmpeg coverage stays at the core level: `dawproject::parse` and
`parse_markers_bytes` are pure parsing/routing and need no ffmpeg, so the whole
branch/refusal matrix above runs unconditionally. The CLI and GUI end-to-end
paths cannot be ffmpeg-free — `cli::load` calls `FfmpegPaths::locate` and
`probe_audio`, and `worker::run_preview` calls `probe_audio`. So the
temp-`.dawproject` end-to-end test added to each frontend's integration suite
gates on the existing `ffmpeg_or_skip()` helper (the convention the other
integration tests already use; `JAMSPLIT_TEST_REQUIRE_FFMPEG=1` turns a skip
into a CI failure). Those end-to-end tests prove the bytes-read + routing wiring
reaches `dawproject::parse`; the parsing behavior itself is covered by the
no-ffmpeg core tests.

## Documentation and web page

Treated as part of the change, not a follow-up.

User-facing docs and web page:

- `MARKERS.md` — new "Bitwig / DAWproject" section: how to export
  (`File > Export DAWproject` in Bitwig), and the requirements (constant tempo,
  no tempo changes; markers placed at song starts). Note markers-only.
- `README.md` — add DAWproject/Bitwig to the "Marker formats" section
  (lines ~63–85) and the `--format` list.
- `index.html` (landing page) — add Bitwig/DAWproject to the format mentions
  (lede ~line 332, the "where each song begins" intro ~371, the per-format
  list with Audacity/REAPER/Plain Text ~386–411, and the format note ~427).

Source-of-truth docs (read before any work, per `CLAUDE.md` and `AGENTS.md` —
leaving them stale would make the repo's own guidance contradict the code):

- `docs/superpowers/specs/2026-06-05-jamsplit-design.md` — this is the *living*
  validated design. Update it for real: add a DAWproject subsection to "Marker
  formats", add `dawproject` to the `--format auto|audacity|plain|reaper`
  synopsis, and add `zip` + `roxmltree` to the core dependency list.
- `docs/spec.md` — this is the **v1 requirements** spec, and DAWproject was not a
  v1 requirement. Do **not** rewrite its required-formats list (that would
  falsely retcon v1 scope). Instead add a short "Post-v1 additions" note
  pointing to this design doc, so a reader is not misled by the v1 format list.
- `CLAUDE.md` and `AGENTS.md` — add `dawproject` to their marker-format
  references / command lists.

## Open questions / risks

- **Unverified Bitwig output.** No real Bitwig `.dawproject` has been inspected.
  The design follows the published schema, which is authoritative for structure,
  but the exact `timeUnit` Bitwig writes, whether it sets it on `<Markers>`, and
  whether it embeds audio are not confirmed. The refuse-when-unknown and
  refuse-on-tempo-automation rules make an unexpected file fail loudly with a
  clear message rather than silently mis-split. Obtaining a real export from the
  end user is the recommended first validation step after implementation.
- **`timeUnit` placement.** The schema allows `timeUnit` on the `timeline` base
  type, so it appears as a literal attribute on whatever timeline element
  carries it — expected to be `<Markers>`. If a real file places it elsewhere
  (e.g. an inherited/document-level convention), step 6 may need to also consult
  an ancestor. Deferred until a real file is available.

## Out of scope (possible future work)

- Importing the audio embedded in or referenced by the `.dawproject` (the
  "one-button, pick one file" flow). Markers-only is deliberate for this
  iteration.
- Tempo-automation-aware beats→seconds conversion (integrating the tempo curve).
- Writing/exporting `.dawproject` files.
