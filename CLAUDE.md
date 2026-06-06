# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

jamsplit splits one long jam-session recording (usually a Zoom-recorder WAV) into per-song MP3s using a human-made marker file. Rust workspace; ffmpeg/ffprobe do all audio work as external subprocesses.

## Status (2026-06-06)

M1 (engine + CLI) implemented. M2 implementation plan is written and ready
to execute: `docs/superpowers/plans/2026-06-06-m2-egui-gui.md` (read its
header — it mandates the superpowers plan-execution skills).

- M1 - engine + CLI: done
- M2 - egui GUI: plan ready, implementation not started; nothing ships to users before M2 is done
- M3 - release packaging CI (out of v1 scope)

## Source of truth

Read these before any work, in order:

1. `jam-wav-splitter.md` - v1 requirements spec
2. `docs/superpowers/specs/2026-06-05-jam-wav-splitter-design.md` - validated design

The design doc records decided trade-offs (Rust over Go, per-song ffmpeg invocations over the segment muxer, workspace over single crate, egui over slint/Tauri, MP3 V0 hardcoded). Read the rationale before proposing changes that touch them; don't re-litigate them without Jason.

## Architecture invariants

Three-crate workspace: `jamsplit-core` (lib, all logic), `jamsplit-cli` (clap bin `jamsplit`), `jamsplit-gui` (egui bin, M2). Full detail is in the design doc; the load-bearing rules are:

- Core never prints and never depends on clap or egui. `export()` reports progress through a callback - the CLI prints from it, the GUI drives a progress bar from it.
- Parsers (audacity/plain/reaper) are dumb: bytes in, `(start_seconds, title)` out. Every business rule (sorting, duplicates, bounds, untitled naming, filename sanitization, boundary math) lives in `plan()`, which is where unit tests concentrate.
- ffmpeg/ffprobe resolve in order: `--ffmpeg-path` flag, then adjacent to our executable, then PATH. The adjacent step exists so M3 can ship batteries-included zips with zero code changes - do not remove it.
- Parse and validation problems are collected and reported all at once, never die-on-first.
- Exports write `name.mp3.part` and rename on success; a per-song failure doesn't stop the run (exit 2 at the end).

## Commands

- `cargo test` — full suite. ffmpeg-dependent integration tests skip (with a
  notice) when ffmpeg is absent; `JAMSPLIT_TEST_REQUIRE_FFMPEG=1` makes
  skips fail (CI mode).
- `cargo test -p jamsplit-core <name>` — one test.
- `cargo run -p jamsplit-cli -- split --audio x.wav --markers m.txt` — run the CLI.
- `cargo fmt --all` and `cargo clippy --workspace` before finishing a task.
