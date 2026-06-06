use crate::audio::AudioInfo;
use crate::markers::ParsedMarkers;
use std::path::Path;

/// One song to export. `end_seconds` is always concrete (the last song's is
/// the audio duration); `to_eof` tells export() to omit `-t`.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub track: usize,
    pub title: String,
    pub filename: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub to_eof: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplitPlan {
    pub songs: Vec<Song>,
    pub audio: AudioInfo,
    pub warnings: Vec<String>,
}

/// Every validation error at once, plus any warnings gathered before failing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}", errors.join("\n"))]
pub struct PlanFailure {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Seconds -> `M:SS.s` or `H:MM:SS.s` for tables and warnings.
pub fn fmt_time(seconds: f64) -> String {
    let seconds = (seconds * 10.0).round() / 10.0;
    let h = (seconds / 3600.0).floor() as u64;
    let m = ((seconds % 3600.0) / 60.0).floor() as u64;
    let s = seconds % 60.0;
    if h > 0 {
        format!("{h}:{m:02}:{s:04.1}")
    } else {
        format!("{m}:{s:04.1}")
    }
}

/// Apply every business rule: sort (warn), reject duplicates and
/// out-of-bounds markers, resolve titles, build filenames, compute
/// boundaries (song N ends at marker N+1; last runs to EOF).
pub fn plan(parsed: &ParsedMarkers, audio: &AudioInfo) -> Result<SplitPlan, PlanFailure> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if parsed.markers.is_empty() {
        errors.push("no markers found in the marker file".to_string());
        return Err(PlanFailure { errors, warnings });
    }

    let mut markers = parsed.markers.clone();
    let already_sorted = markers.windows(2).all(|w| w[0].start_seconds <= w[1].start_seconds);
    if !already_sorted {
        markers.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
        warnings.push("markers were out of order — auto-sorted by start time".to_string());
    }

    for m in &markers {
        if !m.start_seconds.is_finite() || m.start_seconds < 0.0 {
            errors.push(format!(
                "marker '{}' has an invalid start time ({})",
                m.title, m.start_seconds
            ));
        }
    }
    for w in markers.windows(2) {
        if w[0].start_seconds == w[1].start_seconds {
            errors.push(format!(
                "duplicate marker timestamp at {}",
                fmt_time(w[0].start_seconds)
            ));
        }
    }
    for m in &markers {
        if m.start_seconds.is_finite() && m.start_seconds >= audio.duration_seconds {
            errors.push(format!(
                "marker '{}' at {} is at or past the end of the audio ({})",
                m.title,
                fmt_time(m.start_seconds),
                fmt_time(audio.duration_seconds)
            ));
        }
    }
    if !errors.is_empty() {
        return Err(PlanFailure { errors, warnings });
    }

    if markers[0].start_seconds > 0.0 {
        warnings.push(format!(
            "first marker is at {} — the first {} of audio will not be exported (add a 0:00 marker to keep it)",
            fmt_time(markers[0].start_seconds),
            fmt_time(markers[0].start_seconds)
        ));
    }
    if !audio.lossless {
        warnings.push(format!(
            "input codec '{}' is lossy — split points may be approximate",
            audio.codec_name
        ));
    }

    let total = markers.len();
    let mut songs = Vec::with_capacity(total);
    for (i, m) in markers.iter().enumerate() {
        let track = i + 1;
        let to_eof = i == total - 1;
        let end_seconds = if to_eof {
            audio.duration_seconds
        } else {
            markers[i + 1].start_seconds
        };
        let title = resolve_title(&m.title, track);
        if m.title.trim().is_empty() {
            warnings.push(format!("marker {track} has no title — using '{title}'"));
        }
        let filename = filename_for(track, total, &title);
        let len = end_seconds - m.start_seconds;
        if len < 2.0 {
            warnings.push(format!(
                "song {} '{}' is only {len:.1}s long — stray marker?",
                track, title
            ));
        }
        songs.push(Song { track, title, filename, start_seconds: m.start_seconds, end_seconds, to_eof });
    }

    Ok(SplitPlan { songs, audio: audio.clone(), warnings })
}

/// Pre-export check: which target files already exist in `outdir`?
/// Stale `.part` files are not collisions (never finished output).
pub fn check_collisions(plan: &SplitPlan, outdir: &Path, overwrite: bool) -> Result<(), Vec<String>> {
    if overwrite {
        return Ok(());
    }
    let collisions: Vec<String> = plan
        .songs
        .iter()
        .filter(|s| outdir.join(&s.filename).exists())
        .map(|s| format!("would overwrite existing file: {}", outdir.join(&s.filename).display()))
        .collect();
    if collisions.is_empty() { Ok(()) } else { Err(collisions) }
}

/// Resolve a marker title: blank/whitespace becomes `Untitled Song {track}`.
/// The resolved title is used everywhere — MP3 title tag and filename.
pub fn resolve_title(raw: &str, track: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("Untitled Song {track}")
    } else {
        trimmed.to_string()
    }
}

/// Make a title safe as a filename on every OS we ship to: replace
/// `/ \ : * ? " < > |` and control chars with `_`, collapse runs of `_`,
/// trim leading dots and trailing dots/spaces.
pub fn sanitize_title(title: &str) -> String {
    const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let mut out = String::with_capacity(title.len());
    let mut last_was_underscore = false;
    for c in title.chars() {
        if FORBIDDEN.contains(&c) || c.is_control() {
            if !last_was_underscore {
                out.push('_');
                last_was_underscore = true;
            }
        } else {
            out.push(c);
            last_was_underscore = c == '_';
        }
    }
    out.trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// Build `NN - Title.mp3`. `NN` is zero-padded to max(2, digits(total)).
/// A title that sanitizes to nothing falls back to `Untitled Song {track}`.
pub fn filename_for(track: usize, total: usize, resolved_title: &str) -> String {
    let width = std::cmp::max(2, total.to_string().len());
    let safe = sanitize_title(resolved_title);
    let name = if safe.is_empty() {
        format!("Untitled Song {track}")
    } else {
        safe
    };
    format!("{track:0width$} - {name}.mp3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_titles_resolve_to_untitled() {
        assert_eq!(resolve_title("", 3), "Untitled Song 3");
        assert_eq!(resolve_title("   ", 12), "Untitled Song 12");
        assert_eq!(resolve_title("Slow Blues", 1), "Slow Blues");
    }

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(sanitize_title("AC/DC Jam"), "AC_DC Jam");
        assert_eq!(sanitize_title("a\\b:c*d?e\"f<g>h|i"), "a_b_c_d_e_f_g_h_i");
        assert_eq!(sanitize_title("tab\there"), "tab_here");
    }

    #[test]
    fn collapses_runs_and_trims_dots_and_spaces() {
        assert_eq!(sanitize_title("a//b"), "a_b");
        assert_eq!(sanitize_title("ends with dots..."), "ends with dots");
        assert_eq!(sanitize_title(".hidden"), "hidden");
        assert_eq!(sanitize_title("trailing space "), "trailing space");
    }

    #[test]
    fn filenames_are_padded_and_numbered() {
        assert_eq!(filename_for(1, 12, "Opening Jam"), "01 - Opening Jam.mp3");
        assert_eq!(filename_for(7, 120, "X"), "007 - X.mp3");
    }

    #[test]
    fn title_that_sanitizes_to_nothing_falls_back_for_filename() {
        assert_eq!(filename_for(2, 9, "..."), "02 - Untitled Song 2.mp3");
    }

    #[test]
    fn identical_titles_still_make_distinct_filenames() {
        // the track prefix guarantees within-run uniqueness by construction
        assert_ne!(filename_for(3, 10, "A_B"), filename_for(7, 10, "A_B"));
    }

    use crate::markers::{MarkerFormat, RawMarker};

    fn audio(duration: f64) -> AudioInfo {
        AudioInfo {
            path: "/tmp/jam.wav".into(),
            duration_seconds: duration,
            codec_name: "pcm_s16le".to_string(),
            lossless: true,
        }
    }

    fn parsed(markers: &[(f64, &str)]) -> ParsedMarkers {
        ParsedMarkers {
            markers: markers
                .iter()
                .map(|(s, t)| RawMarker { start_seconds: *s, title: t.to_string() })
                .collect(),
            format: MarkerFormat::Plain,
        }
    }

    #[test]
    fn boundaries_song_n_ends_at_marker_n_plus_1_last_at_eof() {
        let p = plan(&parsed(&[(0.0, "One"), (100.0, "Two")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].start_seconds, 0.0);
        assert_eq!(p.songs[0].end_seconds, 100.0);
        assert!(!p.songs[0].to_eof);
        assert_eq!(p.songs[1].end_seconds, 250.0);
        assert!(p.songs[1].to_eof);
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn unsorted_markers_are_sorted_with_warning() {
        let p = plan(&parsed(&[(100.0, "Two"), (0.0, "One")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].title, "One");
        assert_eq!(p.songs[0].track, 1);
        assert!(p.warnings.iter().any(|w| w.contains("sort")));
    }

    #[test]
    fn duplicates_and_out_of_bounds_are_collected_errors() {
        let err = plan(
            &parsed(&[(0.0, "A"), (0.0, "Dup"), (999.0, "Past End")]),
            &audio(250.0),
        )
        .unwrap_err();
        assert_eq!(err.errors.len(), 2);
        assert!(err.errors.iter().any(|e| e.contains("duplicate")));
        assert!(err.errors.iter().any(|e| e.contains("Past End") || e.contains("999")));
    }

    #[test]
    fn marker_exactly_at_duration_is_an_error() {
        assert!(plan(&parsed(&[(0.0, "A"), (250.0, "Empty")]), &audio(250.0)).is_err());
    }

    #[test]
    fn zero_markers_is_an_error() {
        assert!(plan(&parsed(&[]), &audio(250.0)).is_err());
    }

    #[test]
    fn warns_about_skipped_intro_short_songs_and_lossy_input() {
        let mut lossy = audio(250.0);
        lossy.codec_name = "mp3".to_string();
        lossy.lossless = false;
        let p = plan(&parsed(&[(10.0, "Tiny"), (11.0, "A"), (50.0, "B")]), &lossy).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("0:10.0"))); // skipped intro
        assert!(p.warnings.iter().any(|w| w.contains("Tiny")));   // 1s song
        assert!(p.warnings.iter().any(|w| w.contains("approximate"))); // lossy
    }

    #[test]
    fn blank_titles_get_untitled_with_final_track_numbers() {
        let p = plan(&parsed(&[(0.0, ""), (100.0, "")]), &audio(250.0)).unwrap();
        assert_eq!(p.songs[0].title, "Untitled Song 1");
        assert_eq!(p.songs[1].title, "Untitled Song 2");
        assert_eq!(p.songs[1].filename, "02 - Untitled Song 2.mp3");
        assert!(p.warnings.iter().any(|w| w.contains("Untitled")));
    }

    #[test]
    fn fmt_time_formats() {
        assert_eq!(fmt_time(0.0), "0:00.0");
        assert_eq!(fmt_time(323.5), "5:23.5");
        assert_eq!(fmt_time(3731.0), "1:02:11.0");
    }

    #[test]
    fn fmt_time_rounds_at_field_boundaries() {
        assert_eq!(fmt_time(59.96), "1:00.0");
        assert_eq!(fmt_time(3599.96), "1:00:00.0");
        assert_eq!(fmt_time(59.94), "0:59.9");
    }

    #[test]
    fn collisions_respect_overwrite_and_ignore_part_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = plan(&parsed(&[(0.0, "One"), (100.0, "Two")]), &audio(250.0)).unwrap();
        // nothing exists yet: fine either way
        assert!(check_collisions(&p, dir.path(), false).is_ok());
        // a stale .part is not a collision
        std::fs::write(dir.path().join("01 - One.mp3.part"), b"junk").unwrap();
        assert!(check_collisions(&p, dir.path(), false).is_ok());
        // a real target file is
        std::fs::write(dir.path().join("01 - One.mp3"), b"old").unwrap();
        let errs = check_collisions(&p, dir.path(), false).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("01 - One.mp3"));
        // unless overwrite
        assert!(check_collisions(&p, dir.path(), true).is_ok());
    }

    #[test]
    fn failures_carry_warnings_gathered_before_the_error() {
        let err = plan(&parsed(&[(100.0, "Two"), (0.0, "A"), (0.0, "Dup")]), &audio(250.0)).unwrap_err();
        assert!(!err.errors.is_empty());
        assert!(err.warnings.iter().any(|w| w.contains("sort")));
    }

    #[test]
    fn last_song_short_warns_and_names_itself() {
        let p = plan(&parsed(&[(0.0, "Long"), (249.0, "ShortLast")]), &audio(250.0)).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("ShortLast")));
    }

    #[test]
    fn non_finite_and_negative_starts_are_collected_errors() {
        let err = plan(
            &parsed(&[(f64::NAN, "Bad"), (-5.0, "Negative"), (10.0, "Fine")]),
            &audio(250.0),
        )
        .unwrap_err();
        assert_eq!(err.errors.len(), 2);
        assert!(err.errors.iter().all(|e| e.contains("invalid start time")));
    }
}
