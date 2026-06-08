# jamsplit v1 Spec

> **Historical document.** This is the original v1 requirements spec, kept for
> context. It is not updated for post-v1 features. For how jamsplit behaves now,
> see `README.md` and `MARKERS.md`; for design decisions, see the validated
> design and the dated design docs under `docs/superpowers/specs/`.

## Goal
Build a CLI tool that takes one long jam recording and a human-made marker file, then exports one MP3 per song.

## Main workflow
- Input: one long audio file (usually WAV from a Zoom recorder)
- Input: marker file with **song start times** and **song titles**
- Output: one MP3 per song
- Output: simple JSON or text summary log

## Marker sources to support
1. **Audacity labels export** (required)
2. **Plain timestamp text file** (required)
3. **Reaper marker/region export** (spec and implement if practical in v1)

Normalize all marker formats into:
- `start_seconds`
- `title`

## Core behavior
- Markers indicate **song starts only**
- Song N ends at marker N+1
- Last song ends at end of source file
- Auto-sort markers if needed, with a warning
- Reject invalid or duplicate timestamps
- If title is blank, use `Untitled Song N`

## Export behavior
- Export MP3 files
- Default filename format:
  - `01 - Song Title.mp3`
  - `02 - Song Title.mp3`
- Write MP3 metadata tags:
  - `title` = marker/region name
  - `track` = export order
  - optional `album` / session name from CLI arg

## CLI
Implement at least:
- `split --audio FILE --markers FILE`
- `validate --audio FILE --markers FILE`
- `inspect --audio FILE --markers FILE`
- `--dry-run`
- optional `--album`, `--artist`, `--outdir`, `--overwrite`

## Implementation notes
- Use `ffmpeg` / `ffprobe`
- Do not load the whole WAV into memory if avoidable
- Keep marker parsers modular so Audacity/Reaper/plain-text are separate parsers

## Non-goals for v1
- no automatic song boundary detection
- no silence-based splitting as the source of truth
- no GUI
- no loudness normalization/fades yet

## Acceptance criteria
- Works with Audacity label export
- Works with plain timestamp text files
- Specs and supports Reaper marker export if the file format is available
- Exports numbered MP3s correctly
- Uses marker titles as filenames and MP3 title tags
- Produces a dry-run preview and a simple log