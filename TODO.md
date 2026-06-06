# TODO

Deferred items from the M1 final review (2026-06-06). None block M2.

## Design question

- Duplicate-marker detection uses exact f64 equality. Near-duplicates (e.g. `10.0` vs `10.001`) pass validation and produce a ~1ms overlapping song instead of an error. Decide whether `plan()` should reject (or warn about) markers closer than some epsilon, and what the threshold should be.

## Test coverage gaps

- No CLI-level test exercises `--artist` or `--ffmpeg-path`.
- The end-to-end split test verifies the first and last songs but never the middle song's file or duration.
- `last_lines()` is never tested with input longer than n lines (the truncation path).
- `validate_missing_audio_exits_one` passes for the wrong reason when ffmpeg is absent (exit 1 comes from missing ffmpeg, not the missing audio file).

## Next milestone

- M2 (egui GUI) needs its own implementation plan written against the real core API before any GUI work starts (see CLAUDE.md).
