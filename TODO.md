# TODO

Deferred items from the M1 final review (2026-06-06). None block M2.

## Design question

- Duplicate-marker detection uses exact f64 equality. Near-duplicates (e.g. `10.0` vs `10.001`) pass validation and produce a ~1ms overlapping song instead of an error. Decide whether `plan()` should reject (or warn about) markers closer than some epsilon, and what the threshold should be.

## Deferred from the M2 final review (2026-06-06)

None block shipping; the manual checklist run is the only gate left for handing the tool out.

### Tests (worker/core level — the design doc's no-UI-tests rule doesn't apply)

- Hermetic `ExportEnd::Failed` test: `export()` refusing to start (outdir creation fails when its parent is a regular file). ~10 lines in `worker_integration.rs`; covers the GUI's "Export failed" branch end to end.
- Partial-failure path: a run where `export()` succeeds but one song fails (`SongStatus::Failed`, `any_failed() == true`) — exercises the "Done — N song(s) failed" heading and the summary-still-written behavior.
- End-to-end tag-trim assertion: `export_writes_mp3s_and_summary` writes `album: Some("Practice")` but never reads the resulting MP3 tags; add an ffprobe check so the deliberate GUI trimming is verified through ffmpeg metadata, not just at the request boundary.
- `format_label()` (forced vs auto-detected wording) and the wrong-phase guards (`on_song`/`cancel_export`/`recheck_collisions` outside their phases) have no coverage.

### Polish

- `on_exit` cancel hook (~5 lines): closing the window mid-export currently lets the detached ffmpeg child finish its current song (orphaned `.part` at worst — `.part` discipline protects finals).
- "Open output folder" reads `effective_outdir()` live in the Done phase: changing the outdir after an export opens the new (empty) dir, and the button silently no-ops on `ExportEnd::Failed` when the dir was never created. Consider capturing the export's actual outdir and hiding the button on Failed.
- `ui_done` clones the full `ExportEnd` (incl. KB-scale stderr tails) every repaint of the resting Done screen — fix via a deferred `back_clicked` flag or `Rc`.
- `Msg::Song`/`Msg::ExportDone` carry no generation tag; cross-run safety rests on an undocumented ExportDone-is-terminal + Done-barrier invariant. Document it (or tag the messages) before any change to the export flow.
- Superseded preview threads accumulate under rapid input edits (each edit spawns a full parse+probe pipeline; only the newest result is kept). Wasteful, not incorrect.
- README's GUI section doesn't state the default output location (next to the audio file — differs from the CLI's cwd-relative default).
- Checklist fixture command lacks `-y`, so it fails on re-run if `/tmp/jam.wav` exists.
- `(ui.available_height() - 40.0)` can go negative in tiny windows (non-panicking, degenerate scroll area) — clamp with `.max(0.0)` if it ever bothers anyone.
- `gen` field in `PreviewRequest` becomes a reserved keyword in edition 2024 (`r#gen` needed on edition migration).
- eframe is pinned to 0.33.x (0.34 needs rustc 1.92 > toolchain 1.90); consider loosening `0.33.3` → `0.33` before M3 packaging.
- `probe_audio` runs ffprobe with no subprocess timeout (core-owned, predates M2; same exposure as the CLI).

## Next milestone

- M3 — release packaging CI (out of v1 scope; see design doc).
