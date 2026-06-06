# jamsplit

Split one long jam-session recording (usually a WAV from a Zoom recorder) into
one MP3 per song, using a marker file with song start times and titles.

## Download

Prebuilt bundles for macOS (Apple Silicon), Linux x86_64, and Windows
x86_64 — ffmpeg included — are on the
[releases page](https://github.com/laydros/jamsplit/releases/latest), or
via the [project site](https://laydros.github.io/jamsplit/).

## Requirements

- ffmpeg and ffprobe. jamsplit looks for them in this order: the
  `--ffmpeg-path` flag (CLI) or the Locate button (GUI), next to the
  executable, then PATH.
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

## GUI

`jamsplit-gui` is the same splitter as a window: pick the recording and
the marker file, check the track table, click Split.

```bash
cargo run -p jamsplit-gui
```

The plan preview updates live as you change inputs and shows the same
warnings and errors as `jamsplit validate`. Existing output files block
Split until you check "Overwrite existing files". If ffmpeg isn't found
(install hints are shown), the "Locate ffmpeg…" button lets you point at
a binary directly — ffprobe must sit next to it.

## Marker formats

For step-by-step instructions on creating marker files in Audacity, REAPER, or
by hand, see [MARKERS.md](MARKERS.md).

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

## License

GPL-3.0. See [LICENSE](LICENSE).
