use crate::state::{ExportEnd, ExportRequest, PreviewOutcome, PreviewRequest};
use jamsplit_core::audio::probe_audio;
use jamsplit_core::ffmpeg::{export, ExportOptions, SongResult};
use jamsplit_core::markers::parse_markers;
use jamsplit_core::plan::{check_collisions, plan};
use jamsplit_core::report::{build_summary, write_summary};
use std::sync::mpsc::Sender;

/// Everything the workers send back to the UI thread.
pub enum Msg {
    Preview { gen: u64, outcome: PreviewOutcome },
    Song(SongResult),
    ExportDone(ExportEnd),
}

/// parse → probe → plan → check_collisions, collecting problems instead of
/// stopping at the first stage (parse and probe failures are independent
/// and should be reported together). Never prints — core's rule.
pub fn run_preview(request: &PreviewRequest) -> PreviewOutcome {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut collisions = Vec::new();
    let mut format = None;

    let parsed = match std::fs::read_to_string(&request.markers) {
        Ok(content) => match parse_markers(&content, request.format) {
            Ok(parsed) => {
                format = Some((parsed.format.to_string(), request.format.is_some()));
                Some(parsed)
            }
            Err(parse_errors) => {
                errors.extend(
                    parse_errors
                        .iter()
                        .map(|e| format!("{}: {e}", request.markers.display())),
                );
                None
            }
        },
        Err(e) => {
            errors.push(format!(
                "could not read marker file {}: {e}",
                request.markers.display()
            ));
            None
        }
    };

    let audio = match probe_audio(&request.ffmpeg, &request.audio) {
        Ok(audio) => Some(audio),
        Err(e) => {
            errors.push(e.to_string());
            None
        }
    };

    let mut split_plan = None;
    if let (Some(parsed), Some(audio)) = (parsed.as_ref(), audio.as_ref()) {
        match plan(parsed, audio) {
            Ok(p) => {
                warnings.extend(p.warnings.iter().cloned());
                if let Err(c) = check_collisions(&p, &request.outdir, request.overwrite) {
                    collisions = c;
                }
                split_plan = Some(p);
            }
            Err(failure) => {
                warnings.extend(failure.warnings);
                errors.extend(failure.errors);
            }
        }
    }

    PreviewOutcome {
        format,
        plan: split_plan,
        errors,
        collisions,
        warnings,
    }
}

/// Run the preview pipeline on its own thread. `notify` is called after the
/// send so the UI repaints (the app passes `ctx.request_repaint`).
pub fn spawn_preview(request: PreviewRequest, tx: Sender<Msg>, notify: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let outcome = run_preview(&request);
        let _ = tx.send(Msg::Preview {
            gen: request.gen,
            outcome,
        });
        notify();
    });
}

/// Run export() on its own thread, streaming per-song results, then write
/// the summary — always, including canceled and partially-failed runs.
pub fn spawn_export(request: ExportRequest, tx: Sender<Msg>, notify: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let opts = ExportOptions {
            outdir: request.outdir.clone(),
            album: request.album.clone(),
            artist: request.artist.clone(),
            overwrite: request.overwrite,
            cancel: request.cancel.clone(),
        };
        let progress_tx = tx.clone();
        let notify_ref = &notify;
        let result = export(&request.plan, &request.ffmpeg, &opts, &mut |song| {
            let _ = progress_tx.send(Msg::Song(song.clone()));
            notify_ref();
        });
        let end = match result {
            Ok(report) => {
                let summary = build_summary(
                    &request.plan,
                    &report,
                    &request.markers,
                    &request.format_name,
                    request.album.as_deref(),
                    request.artist.as_deref(),
                );
                let summary = write_summary(&summary, &request.outdir)
                    .map_err(|e| format!("could not write summary: {e}"));
                ExportEnd::Finished { report, summary }
            }
            Err(e) => ExportEnd::Failed(e.to_string()),
        };
        let _ = tx.send(Msg::ExportDone(end));
        notify();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PreviewRequest;
    use jamsplit_core::ffmpeg::FfmpegPaths;
    use jamsplit_core::markers::MarkerFormat;

    #[test]
    fn preview_collects_parse_and_probe_errors_together() {
        let dir = tempfile::tempdir().unwrap();
        let markers = dir.path().join("markers.txt");
        std::fs::write(&markers, "5:75 Bad Minute\n").unwrap();
        let request = PreviewRequest {
            gen: 1,
            audio: dir.path().join("missing.wav"),
            markers,
            format: Some(MarkerFormat::Plain),
            outdir: dir.path().join("out"),
            overwrite: false,
            ffmpeg: FfmpegPaths {
                ffmpeg: "/nonexistent/ffmpeg".into(),
                ffprobe: "/nonexistent/ffprobe".into(),
            },
        };
        let outcome = run_preview(&request);
        assert!(outcome.plan.is_none());
        assert_eq!(outcome.format, None); // parsing failed, no format to announce
                                          // both stages reported, not just the first
        assert!(outcome.errors.iter().any(|e| e.contains("markers.txt")));
        assert!(outcome.errors.iter().any(|e| e.contains("missing.wav")));
    }

    #[test]
    fn unreadable_marker_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let request = PreviewRequest {
            gen: 1,
            audio: dir.path().join("missing.wav"),
            markers: dir.path().join("no-such-markers.txt"),
            format: None,
            outdir: dir.path().join("out"),
            overwrite: false,
            ffmpeg: FfmpegPaths {
                ffmpeg: "/nonexistent/ffmpeg".into(),
                ffprobe: "/nonexistent/ffprobe".into(),
            },
        };
        let outcome = run_preview(&request);
        assert!(outcome
            .errors
            .iter()
            .any(|e| e.contains("no-such-markers.txt")));
    }
}
