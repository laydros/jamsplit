# TODO

Deferred items from the M1 final review (2026-06-06). None block M2.

## Design question

- Duplicate-marker detection uses exact f64 equality. Near-duplicates (e.g. `10.0` vs `10.001`) pass validation and produce a ~1ms overlapping song instead of an error. Decide whether `plan()` should reject (or warn about) markers closer than some epsilon, and what the threshold should be.

## Deferred from the M2 final review (2026-06-06)

None block shipping. The manual checklist run happened 2026-06-06: everything passes on macOS; the Windows no-console check is the one unchecked item (needs a Windows box, fold into M3 packaging).

### Tests (worker/core level — the design doc's no-UI-tests rule doesn't apply)

- Hermetic `ExportEnd::Failed` test: `export()` refusing to start (outdir creation fails when its parent is a regular file). ~10 lines in `worker_integration.rs`; covers the GUI's "Export failed" branch end to end.
- Partial-failure path: a run where `export()` succeeds but one song fails (`SongStatus::Failed`, `any_failed() == true`) — exercises the "Done — N song(s) failed" heading and the summary-still-written behavior.
- End-to-end tag-trim assertion: `export_writes_mp3s_and_summary` writes `album: Some("Practice")` but never reads the resulting MP3 tags; add an ffprobe check so the deliberate GUI trimming is verified through ffmpeg metadata, not just at the request boundary.
- `format_label()` (forced vs auto-detected wording) and the wrong-phase guards (`on_song`/`cancel_export`/`recheck_collisions` outside their phases) have no coverage.

### Polish

- Closing the app while a native file picker (Choose… for audio or markers) is open aborts with a winit re-entrancy panic ("tried to handle event while another event is currently being handled"). `rfd`'s blocking `pick_file()` runs a nested macOS modal loop inside egui's `update()`, and a terminate event arriving during that loop is not handled cleanly. Normal use (open → select/cancel → quit) is unaffected. The fix is switching the pickers from blocking `pick_file()` to `rfd`'s async dialog so no nested modal runs inside the event loop.
- `on_exit` cancel hook (~5 lines): closing the window mid-export currently lets the detached ffmpeg child finish its current song (orphaned `.part` at worst — `.part` discipline protects finals).
- "Open output folder" reads `effective_outdir()` live in the Done phase: changing the outdir after an export opens the new (empty) dir, and the button silently no-ops on `ExportEnd::Failed` when the dir was never created. Consider capturing the export's actual outdir and hiding the button on Failed.
- `ui_done` clones the full `ExportEnd` (incl. KB-scale stderr tails) every repaint of the resting Done screen — fix via a deferred `back_clicked` flag or `Rc`.
- `Msg::Song`/`Msg::ExportDone` carry no generation tag; cross-run safety rests on an undocumented ExportDone-is-terminal + Done-barrier invariant. Document it (or tag the messages) before any change to the export flow.
- Superseded preview threads accumulate under rapid input edits (each edit spawns a full parse+probe pipeline; only the newest result is kept). Wasteful, not incorrect.
- README's GUI section doesn't state the default output location (next to the audio file — differs from the CLI's cwd-relative default).
- `(ui.available_height() - 40.0)` can go negative in tiny windows (non-panicking, degenerate scroll area) — clamp with `.max(0.0)` if it ever bothers anyone.
- `gen` field in `PreviewRequest` becomes a reserved keyword in edition 2024 (`r#gen` needed on edition migration).
- `probe_audio` runs ffprobe with no subprocess timeout (core-owned, predates M2; same exposure as the CLI).

## Docs / website (done 2026-06-09)

Both changes shipped:

- DAWproject coverage reframed from Bitwig-specific to spec-based across
  `index.html`, `README.md`, and `MARKERS.md`: jamsplit follows the open
  DAWproject spec, so it should work with Bitwig, Cubase, and Studio One
  `.dawproject` exports, but is still untested against a real export. The
  refuse-when-unknown framing (a non-conforming file fails loudly rather than
  mis-splitting) is preserved.
- Headline de-jammed: `index.html` `<h1>` now reads "Split one long recording
  into song files." The `<title>` and `og:description` were updated to match
  (confirmed with Jason 2026-06-09).

## Next milestone

M3 is done; v0.1.0 was published 2026-06-06 (plan:
`docs/superpowers/plans/2026-06-06-m3-release-packaging.md`), followed the
same day by v0.2.0 (Windows wgpu/WARP renderer fix, startup-error dialog),
then v0.3.0 (2026-06-08, Bitwig `.dawproject` marker import).
Remaining before v1.0:

- Verify a real split from the Windows and Linux bundles on real machines
  (the macOS bundle was smoke-tested end to end). The Windows GUI now
  launches (wgpu/WARP fix, verified 2026-06-06 on a VM over RDP, along with
  the no-console and Explorer/taskbar icon checks), but a real split has
  not been run from the Windows or Linux bundles yet.

## Future (post-M3 candidates)

- wav concat helper: Zoom recorders split long sessions at the 2/4 GB
  boundary; the design doc punts to a documented ffmpeg concat one-liner.
  A `jamsplit concat`-style helper (CLI subcommand and/or GUI affordance)
  would remove that manual step. Out of v1 scope per the design doc;
  discuss scope with Jason before starting.
