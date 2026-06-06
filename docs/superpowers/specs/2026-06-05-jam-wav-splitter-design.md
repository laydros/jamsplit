---
date: 2026-06-05
tags:
  - design
  - jam-wav-splitter
summary: "Validated design for jamsplit: Rust workspace with core lib, CLI, and egui GUI that split one long jam recording into per-song MP3s using a marker file and ffmpeg"
---

# Jam Splitter Design

Implements [jam-wav-splitter.md](../../../jam-wav-splitter.md) (the v1 spec), extended with a GUI milestone. The target users are bandmates who will not use a CLI; nothing ships to them until the GUI exists.

## Decisions and rationale

- **Language: Rust.** The GUI requirement removes Go's main advantage — every viable Go GUI stack (Fyne, Wails) requires cgo, which forfeits Go's easy cross-compilation and forces per-platform CI builds, exactly where Rust already is. egui is pure Rust with no webview or Docker tooling. Rust's audio ecosystem (symphonia, cpal, rodio) leaves headroom if the GUI ever grows waveform/playback features, and Jason already maintains Rust projects.
- **Splitting strategy: one ffmpeg invocation per song.** WAV is raw PCM, so `-ss` input seeking is instant and sample-accurate. Each invocation sets that song's metadata directly, and one song failing does not poison the rest. The segment muxer (one pass, then rename/retag everything) and native Rust audio (C LAME linkage, contradicts the spec's ffmpeg mandate) both lose on complexity.
- **ffmpeg is an external dependency in v1.** Resolution order: `--ffmpeg-path` flag → `ffmpeg`/`ffprobe` sitting next to our own executable → `PATH`. The adjacent-binary step makes "batteries included" zips (static ffmpeg packed next to jamsplit by release CI) a pure packaging decision with zero code changes. Auto-download-on-first-run is possible future work; embedding ffmpeg in the binary is rejected.
- **Workspace, not a single crate.** The GUI is real scope, so core/cli/gui separation is no longer speculative. It also keeps heavy GUI deps out of CLI builds.
- **MP3 encoding: libmp3lame VBR `-q:a 0` (V0, ~245 kbps), hardcoded.** A quality flag is a trivial later addition if wanted.

## Architecture

```text
Cargo.toml            # workspace
crates/
  jamsplit-core/      # lib: all logic
    src/
      lib.rs
      markers/
        mod.rs        # RawMarker, format auto-detection, dispatch
        audacity.rs   # tab-separated labels parser
        plain.rs      # flexible timestamp parser
        reaper.rs     # CSV marker/region export parser
      plan.rs         # normalize + validate -> SplitPlan (all business rules)
      audio.rs        # ffprobe wrapper: duration, readability
      ffmpeg.rs       # binary lookup, per-song command build/run
      report.rs       # human table + summary JSON
  jamsplit-cli/       # bin: jamsplit (clap lives here, never in core)
  jamsplit-gui/       # bin: jamsplit-gui (eframe/egui + rfd)
```

Every frontend runs the same pipeline:

```text
markers file ──parse──▶ Vec<RawMarker>  ┐
                                        ├──▶ plan() ──▶ SplitPlan
audio file ──ffprobe──▶ duration        ┘        │
                                                 ├─ validate: report + exit code
                                                 ├─ inspect:  track table
                                                 ├─ split:    export() per song ──▶ summary
                                                 └─ GUI:      live preview + export()
```

Parsers stay dumb: bytes in, `(start_seconds, title)` pairs out. `plan()` owns every business rule. Core API shape (signatures illustrative, firmed up during implementation planning):

```rust
parse_markers(path, format) -> Result<ParsedMarkers, Vec<ParseError>>
probe_audio(ffmpeg, path) -> Result<AudioInfo, AudioError>
plan(markers, audio, opts) -> Result<SplitPlan, ValidationReport>   // Ok still carries warnings
export(plan, opts, on_progress: impl FnMut(&SongResult)) -> ExportReport
```

`export()` reports progress through a callback — the CLI prints from it, the GUI drives a progress bar from it. Core never prints. Cancellation is also core's job: `ExportOptions` carries a `CancelToken` (shared atomic flag), checked between songs and polled (~100 ms) while waiting on the running child. On cancel, the current ffmpeg child is killed, its `.part` removed, and the remaining songs are marked skipped in the report. The GUI's Cancel button sets the token; the CLI passes one that is never set.

Times are `f64` seconds throughout. Core dependencies kept minimal: `serde`/`serde_json` (summary, ffprobe output), `thiserror`, `csv` (Reaper quoting only). CLI adds `clap` + `anyhow`; GUI adds `eframe`/`egui` + `rfd`. No async runtime, no regex, no audio crates.

## CLI

```text
jamsplit split    --audio FILE --markers FILE [--outdir DIR] [--album NAME]
                  [--artist NAME] [--overwrite] [--dry-run]
                  [--format auto|audacity|plain|reaper] [--ffmpeg-path PATH]
jamsplit validate --audio FILE --markers FILE [--format ...] [--ffmpeg-path ...]
jamsplit inspect  --audio FILE --markers FILE [--format ...] [--ffmpeg-path ...]
```

- `validate` — "is this pair usable?" Errors/warnings and an exit code. Writes nothing.
- `inspect` — "show me the plan." Track table: track, start, end, duration, title, filename. Writes nothing.
- `split --dry-run` — inspect plus write-effects preview: resolved output paths, which files would be overwritten, whether the outdir would be created. Writes nothing, not even the summary file.
- `split` — exports, then writes the summary.

Behavior:

- `--format` defaults to auto-detection; the detected format is always announced.
- `--outdir` defaults to `./<audio-file-stem>/`, created (with parents) if missing.
- Existing target files are collected during planning and reported as one error listing every collision, before anything is written, unless `--overwrite`.
- Exit codes: `0` success (warnings allowed), `1` invalid input/markers, `2` one or more exports failed. Results and tables on stdout; warnings and errors on stderr.
- Any input ffprobe can read is accepted. Lossless inputs (WAV, FLAC, ALAC) are the supported, sample-accurate path — input-side `-ss` with transcode is exact when timestamps are trustworthy. Lossy inputs (MP3, AAC) still work but emit a warning that split points may be approximate, because their seek/timestamp tables aren't always reliable (e.g. VBR MP3 without a Xing header).

### Filenames and tags

- Filename: `NN - Title.mp3`. `NN` is the 1-based track number zero-padded to `max(2, digits(song_count))`.
- Sanitization (filenames only): replace `/ \ : * ? " < > |` and ASCII control characters with `_` (union of all-OS rules — these files get shared across platforms), collapse consecutive `_`, trim leading dots and trailing dots/spaces. A title that sanitizes to nothing falls back to `Untitled Song N`.
- Blank marker titles resolve to `Untitled Song N` at plan time, and that resolved title is used **everywhere** — MP3 `title` tag and filename alike (per the v1 spec).
- Tags carry the resolved title **without filename sanitization** (`AC/DC Jam` keeps its slash in the tag): `title`, `track` = `N/total`, plus `album` and `artist` when given. Only filenames sanitize; in the rare case a non-blank title sanitizes to nothing (e.g. dots-only), the filename falls back to `Untitled Song N` while the tag keeps what the user wrote.
- Within-run path collisions are impossible by construction: the track-number prefix is unique per song and identically padded, so identical sanitized titles (or multiple `Untitled Song N` fallbacks) still produce distinct filenames, and `.part` paths inherit that uniqueness. The collision check against *existing* files covers the final `.mp3` paths; stale `.part` files left behind by an interrupted run are not collisions — they are overwritten freely, since a `.part` is never finished output.

### ffmpeg invocation (per song)

```text
ffmpeg -hide_banner -nostdin -v error
  -ss {start} -t {duration} -i {input}        # -t omitted for the last song (runs to EOF)
  -map_metadata -1                            # drop source metadata (Zoom BWF/iXML junk)
  -c:a libmp3lame -q:a 0
  -metadata title=... -metadata track=N/T [-metadata album=...] [-metadata artist=...]
  -f mp3 {outdir}/{NN - Title}.mp3.part
```

Written to `.part` (hence the explicit `-f mp3`), renamed on success — an interrupted run never leaves half-written files that look finished. Sample rate and channel count are inherited from the source. On failure: keep the last ~15 lines of ffmpeg stderr for the summary, continue with the remaining songs, exit `2` at the end.

## Marker formats

All parsers normalize to `RawMarker { start_seconds: f64, title: String }` (title may be empty). Parse errors carry file and line number and are **collected, not die-on-first** — `validate` reports everything in one pass.

### Audacity labels (tab-separated `.txt`)

- Lines: `start<TAB>end<TAB>label`, times in decimal seconds. Label may be empty.
- Only `start` is used. Range labels' ends are ignored — markers mean song starts, always.
- Spectral-selection exports insert a second line starting with `\` (frequency data); skipped silently.

### Plain text (hand-written)

- Per line: time, optional separator (`-`, tab, or whitespace), title = rest of line, trimmed (may be empty). `#` comments and blank lines ignored.
- Colon count decides the form: none = raw seconds (`3722.5`), one = `M:SS`, two = `H:MM:SS`. Fractional seconds allowed on the seconds component in every form.
- The leading component is unbounded (`62:11` = 62 minutes, valid); every later component must be < 60 — `5:75` and `1:75:00` are rejected with line numbers.

### Reaper (Region/Marker Manager CSV export)

- Header row `#,Name,Start,End,Length`; parsed by column name, extra columns (e.g. Color) tolerated. Quoted names containing commas handled by the `csv` crate.
- Rows `M*` (markers) and `R*` (regions) both accepted; regions contribute their Start, End ignored (same rule as Audacity ranges).
- Start values are parsed with the same flexible time parser. Values that look like bars/beats (e.g. `9.1.00`) fail with: "set Reaper's time unit to Minutes:Seconds and re-export." That is the supported boundary for Reaper's settings-dependent export.

### Auto-detection

Order: Audacity (strict `float TAB float` line shape on every non-blank line, excluding `\`-prefixed spectral lines — detection skips exactly what the parser skips) → Reaper (CSV header signature) → plain (fallback). The ordering is a deliberate tiebreak: every Audacity file is also *parseable* as plain (plain accepts any rest-of-line as a title), so the strict shape must be tested first. The converse edge case exists and is accepted: a hand-written plain file using tab separators whose titles parse as bare floats (`12<TAB>34`) is genuinely indistinguishable from Audacity labels and will be detected as Audacity. Mitigation: detection results are always announced, and `--format plain` overrides. An ambiguity *error* was considered and rejected — because Audacity files always also parse as plain, it would fire on every genuine Audacity export.

## Validation rules (all in `plan()`)

Errors (block split; exit `1`):

- Zero markers parsed.
- Marker at or past audio duration (would create an empty song).
- Duplicate timestamps, compared after normalization to seconds (`f64` equality — `90`, `1:30`, and `0:01:30` are the same instant).
- Audio unreadable or has no duration.

Warnings (split proceeds):

- Markers out of order → auto-sorted.
- First marker after `0:00` → states exactly how much audio will not be exported. Audio before the first marker is never exported; users who want it add a `0:00` marker.
- Song shorter than 2 seconds (almost always a stray marker).
- Blank title → `Untitled Song N` (N = final track number after sorting).

## GUI (jamsplit-gui)

Single window, eframe/egui with rfd native file dialogs.

- **Inputs:** audio picker, markers picker, format dropdown (auto + three formats), album/artist text fields, outdir picker (same `<audio-stem>/` default), overwrite checkbox.
- **Live preview:** any input change re-runs `parse → probe → plan` on a worker thread (results back over an mpsc channel + repaint request — the UI thread never blocks). Shows the track table, detected format, warnings (yellow), and errors (red). The GUI is inherently the dry-run: you see the full plan before committing.
- **States:** Idle → Preview (Split disabled while errors exist) → Exporting (per-song progress bar via `export()`'s callback; Cancel sets the core `CancelToken`, which kills the current ffmpeg child and removes its `.part`) → Done (summary + "Open output folder") / Failed / Canceled (completed songs kept, summary still written).
- The state machine lives in a plain struct, testable without egui. `#![windows_subsystem = "windows"]` keeps Windows from opening a console. Writes the same `jamsplit-summary.json`.
- egui apps look like egui, not native widgets — accepted for this tool. No waveform, no playback: file pickers and a table.

## Summary log

`jamsplit-summary.json`, written into the outdir after a real split (never on dry-run), plus a human table on stdout (CLI) or in the window (GUI). The summary is written even when the run partially fails (exit `2`) or is canceled — failed songs carry their status and stderr excerpt, skipped songs are marked skipped. Fields: source audio path, markers path, detected/forced format, album/artist, tool version, per-song entries (track, title, start, end, duration, output file, status, error excerpt if failed), and all warnings.

## Error handling

- Core: `thiserror` enums — `ParseError { file, line, message }`, validation issues with error/warning severity, `ExportError { track, stderr_tail }`. Frontends render; core never prints.
- Missing ffmpeg/ffprobe: dedicated error listing the resolution order tried, with per-OS install hints (brew / winget or gyan.dev build / apt) and a pointer to `--ffmpeg-path` and adjacent-binary support.
- CLI binary uses `anyhow` for top-level rendering.

## Testing

- **Unit (no ffmpeg):** golden fixture files per parser — blank titles, comments, `\` spectral lines, quoted CSV names, bars/beats rejection, `5:75` rejection, `62:11` acceptance — and `plan()` rules: sorting, duplicates, bounds, untitled numbering, sanitization, boundary math including last-song-to-EOF.
- **Integration (ffmpeg required):** fixture WAVs generated at test time (`ffmpeg -f lavfi -i "sine=frequency=440:duration=10"`), full split, then assert file count and names, durations via ffprobe (±50 ms), and tags via ffprobe JSON. Auto-skip with a notice when ffmpeg is absent locally; mandatory in CI.
- **GUI:** no automated UI tests in v1. Mitigations: the GUI is a thin shell over tested core code, its state machine is a plain testable struct, and M2 ships with a short manual test checklist.
- Implementation follows TDD throughout.

## Milestones

- **M1 — engine + CLI:** workspace, jamsplit-core, jamsplit-cli, full test suite, README (install including per-OS ffmpeg instructions, usage, marker format docs, Zoom concat one-liner). Every acceptance criterion in the v1 spec lands here.
- **M2 — GUI:** jamsplit-gui as specified above. The bar for handing the tool to anyone is M2 done.
- **M3 — distribution (follow-up, out of v1 scope):** GitHub Actions release matrix producing per-OS binaries, "batteries included" zips with static ffmpeg/ffprobe sidecars (BtbN for Windows/Linux, OSXExperts for macOS) plus an ffmpeg source NOTICE, and a macOS `.app` bundle for double-click launch. Requires no tool-code changes thanks to the adjacent-binary lookup.

## Non-goals for v1

From the original spec: no automatic song boundary detection, no silence-based splitting, no loudness normalization or fades.

Made explicit here:

- One input audio file per run. Zoom recorders split long sessions at 2/4 GB; users concatenate first (README documents the ffmpeg concat one-liner). Multi-file input is possible future work.
- Audio before the first marker is never exported (warned, never silent).
- Range/region ends are always ignored — starts only.
- MP3 V0 hardcoded; no quality flag.
- No GUI waveform, playback, or marker editing.
- No ffmpeg auto-download.
