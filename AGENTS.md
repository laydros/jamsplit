# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project

jamsplit splits one long jam-session recording (usually a Zoom-recorder WAV) into per-song MP3s using a human-made marker file. Rust workspace; ffmpeg/ffprobe do all audio work as external subprocesses.

## Status (2026-06-06)

All milestones are done; v0.2.0 is released (Windows wgpu/WARP renderer
fix over v0.1.0). The plan files
`docs/superpowers/plans/2026-06-06-m2-egui-gui.md` and
`docs/superpowers/plans/2026-06-06-m3-release-packaging.md` are the historical
records of the M2 and M3 design decisions. `RELEASING.md` is the release
runbook; TODO.md tracks remaining polish and post-v1 candidates.

- M1 - engine + CLI: done
- M2 - egui GUI: done (2026-06-06)
- M3 - release packaging CI: done (2026-06-06, v0.1.0 published)

## Source of truth

Read these before any work:

1. `docs/superpowers/specs/2026-06-05-jamsplit-design.md` - validated design; the binding source of truth for architecture and trade-offs. Dated feature design docs under `docs/superpowers/specs/` extend it (e.g. the DAWproject import design).
2. `docs/spec-v1.md` - the original v1 requirements, kept for historical context. Not updated for post-v1 work; current behavior lives in `README.md` / `MARKERS.md`.

The design doc records decided trade-offs (Rust over Go, per-song ffmpeg invocations over the segment muxer, workspace over single crate, egui over slint/Tauri, MP3 V0 hardcoded). Read the rationale before proposing changes that touch them; don't re-litigate them without Jason.

## Architecture invariants

Three-crate workspace: `jamsplit-core` (lib, all logic), `jamsplit-cli` (clap bin `jamsplit`), `jamsplit-gui` (egui bin). Full detail is in the design doc; the load-bearing rules are:

- Core never prints and never depends on clap or egui. `export()` reports progress through a callback - the CLI prints from it, the GUI drives a progress bar from it.
- Parsers (audacity/plain/reaper/dawproject) are dumb: bytes in, `(start_seconds, title)` out. Every business rule (sorting, duplicates, bounds, untitled naming, filename sanitization, boundary math) lives in `plan()`, which is where unit tests concentrate.
- ffmpeg/ffprobe resolve in order: `--ffmpeg-path` flag, then adjacent to our executable, then PATH. The adjacent step exists so M3 can ship batteries-included zips with zero code changes - do not remove it.
- Parse and validation problems are collected and reported all at once, never die-on-first.
- Exports write `name.mp3.part` and rename on success; a per-song failure doesn't stop the run (exit 2 at the end).

## Commands

- `cargo test` — full suite. ffmpeg-dependent integration tests skip (with a
  notice) when ffmpeg is absent; `JAMSPLIT_TEST_REQUIRE_FFMPEG=1` makes
  skips fail (CI mode).
- `cargo test -p jamsplit-core <name>` — one test.
- `cargo run -p jamsplit-cli -- split --audio x.wav --markers m.txt` — run the CLI.
- `cargo run -p jamsplit-gui` — run the GUI.
- Before calling a GUI build done, run through `docs/gui-manual-test-checklist.md`.
- `cargo fmt --all` and `cargo clippy --workspace` before finishing a task.
