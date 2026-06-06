---
date: 2026-06-06
summary: "Manual test checklist for jamsplit-gui — run before calling an M2 build done"
---

# jamsplit-gui manual test checklist

Run with `cargo run -p jamsplit-gui` unless noted. Fixture audio:
`ffmpeg -f lavfi -i "sine=frequency=440:duration=30" -ar 44100 /tmp/jam.wav`,
markers `/tmp/songs.txt`: `0:00 One` / `0:12 Two` / `0:21 Three`.

## ffmpeg resolution

- [ ] With ffmpeg on PATH: no ffmpeg error or picker is visible.
- [ ] `cargo build -p jamsplit-gui && env PATH="" ./target/debug/jamsplit-gui`:
  the install-hint error and "Locate ffmpeg…" button appear; picking a real
  ffmpeg binary clears the error and preview works.
- [ ] Picking a binary with no adjacent ffprobe shows the "no ffprobe next
  to it" error instead of clearing.

## Preview

- [ ] Picking audio + markers shows the track table and the format
  announcement ("auto-detected"); forcing a format in the dropdown changes
  it to "(forced)".
- [ ] A marker line like `5:75 x` shows a red error with the line number;
  Split is disabled.
- [ ] A marker past the audio's end shows the out-of-bounds error.
- [ ] Markers with blank titles show "Untitled Song N" rows.
- [ ] An MP3 chosen as *input* audio shows the yellow lossy-accuracy warning.
- [ ] The default outdir label tracks the audio file; picking an outdir
  overrides it.

## Collisions and overwrite

- [ ] With existing output files in the outdir: red collision errors,
  Split disabled, hint about the Overwrite checkbox shown.
- [ ] Checking Overwrite clears the collision errors and enables Split
  (no re-probe flicker — the table does not rebuild).

## Export

- [ ] Split exports all songs; the progress bar advances once per song
  ("2 / 3"); inputs are greyed out while exporting.
- [ ] Done screen lists each song with "ok", shows the summary path; the
  MP3s play and carry title/track/album/artist tags
  (`ffprobe -show_format -of json <file>`).
- [ ] "Open output folder" opens the outdir in the file manager.
- [ ] "Back" returns to the preview, which now reports collisions for the
  files just written.
- [ ] Cancel mid-export (use a longer fixture, e.g. duration=600 with many
  markers): remaining songs show "skipped", heading says Canceled,
  completed MP3s are kept, no `.part` files remain, and
  jamsplit-summary.json exists with skipped statuses.

## Window behavior

- [ ] Resizing small: the track table scrolls rather than overflowing.
- [ ] Windows only: launching the exe opens no console window.
