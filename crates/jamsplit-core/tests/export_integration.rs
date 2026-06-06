mod common;

use common::{ffmpeg_or_skip, make_wav};
use jamsplit_core::audio::{probe_audio, AudioError};
use jamsplit_core::ffmpeg::{export, CancelToken, ExportOptions, SongStatus};
use jamsplit_core::markers::parse_markers;
use jamsplit_core::plan::plan;
use std::path::Path;

#[test]
fn probes_a_real_wav() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let info = probe_audio(&ff, &wav).unwrap();
    assert!(
        (info.duration_seconds - 10.0).abs() < 0.1,
        "duration: {}",
        info.duration_seconds
    );
    assert!(info.lossless);
    assert!(info.codec_name.starts_with("pcm_"));
}

#[test]
fn probe_missing_file_is_not_found() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let err = probe_audio(&ff, std::path::Path::new("/no/such/file.wav")).unwrap_err();
    assert!(matches!(err, AudioError::NotFound(_)));
}

#[test]
fn probe_non_audio_file_fails() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.wav");
    std::fs::write(&junk, b"this is not audio").unwrap();
    assert!(probe_audio(&ff, &junk).is_err());
}

fn read_tags(ff: &jamsplit_core::ffmpeg::FfmpegPaths, path: &Path) -> serde_json::Value {
    let out = std::process::Command::new(&ff.ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title,track,album,artist",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["format"]["tags"].clone()
}

fn opts(outdir: &Path, overwrite: bool) -> ExportOptions {
    ExportOptions {
        outdir: outdir.to_path_buf(),
        album: Some("Practice 2026-06-05".to_string()),
        artist: Some("The Band".to_string()),
        overwrite,
        cancel: CancelToken::new(),
    }
}

#[test]
fn full_split_files_durations_tags_progress() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 AC/DC Jam\n3.0 Slow Blues\n6.5\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let mut seen = Vec::new();
    let report = export(&p, &ff, &opts(&outdir, false), &mut |r| seen.push(r.track)).unwrap();

    assert_eq!(seen, vec![1, 2, 3]);
    assert!(!report.any_failed() && !report.canceled);

    let f1 = outdir.join("01 - AC_DC Jam.mp3"); // filename sanitized
    let f3 = outdir.join("03 - Untitled Song 3.mp3"); // blank title resolved
    assert!(f1.is_file() && f3.is_file());
    assert!(!outdir.join("01 - AC_DC Jam.mp3.part").exists());

    let d1 = probe_audio(&ff, &f1).unwrap().duration_seconds;
    let d3 = probe_audio(&ff, &f3).unwrap().duration_seconds;
    assert!((d1 - 3.0).abs() < 0.1, "song 1 duration {d1}");
    assert!((d3 - 3.5).abs() < 0.1, "song 3 duration {d3}");

    let tags = read_tags(&ff, &f1);
    assert_eq!(tags["title"], "AC/DC Jam"); // tag keeps the slash
    assert_eq!(tags["track"], "1/3");
    assert_eq!(tags["album"], "Practice 2026-06-05");
    assert_eq!(tags["artist"], "The Band");
}

#[test]
fn one_failure_does_not_stop_the_rest() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n3.0 Two\n6.5 Three\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    // make song 2's .part path unwritable by occupying it with a directory
    std::fs::create_dir_all(outdir.join("02 - Two.mp3.part")).unwrap();

    let report = export(&p, &ff, &opts(&outdir, false), &mut |_| {}).unwrap();
    assert!(
        matches!(&report.results[1].status, SongStatus::Failed { stderr_tail } if !stderr_tail.is_empty())
    );
    assert!(matches!(report.results[0].status, SongStatus::Ok));
    assert!(matches!(report.results[2].status, SongStatus::Ok));
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(outdir.join("03 - Three.mp3").is_file());
    assert!(report.any_failed());
}

#[test]
fn cancel_after_first_song_skips_the_rest() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n3.0 Two\n6.5 Three\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let o = opts(&outdir, false);
    let cancel = o.cancel.clone();
    let report = export(&p, &ff, &o, &mut |r| {
        if r.track == 1 {
            cancel.cancel();
        }
    })
    .unwrap();

    assert!(report.canceled);
    assert!(matches!(report.results[0].status, SongStatus::Ok));
    assert!(matches!(report.results[1].status, SongStatus::Skipped));
    assert!(matches!(report.results[2].status, SongStatus::Skipped));
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(!outdir.join("02 - Two.mp3").exists());
}

#[test]
fn overwrite_true_replaces_existing_outputs() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 One\n5.0 Two\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");

    let first = export(&p, &ff, &opts(&outdir, false), &mut |_| {}).unwrap();
    assert!(!first.any_failed());
    let second = export(&p, &ff, &opts(&outdir, true), &mut |_| {}).unwrap();
    assert!(!second.any_failed());
    assert!(outdir.join("01 - One.mp3").is_file());
}

#[test]
fn cancel_mid_song_kills_ffmpeg_and_removes_part() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    // 240-second WAV so song 1 takes several seconds to encode, giving the
    // polling loop time to observe the cancel flag mid-encode.
    let wav = make_wav(&ff, dir.path(), 240.0);
    let audio = probe_audio(&ff, &wav).unwrap();
    let parsed = parse_markers("0:00 First\n120.0 Second\n", None).unwrap();
    let p = plan(&parsed, &audio).unwrap();
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();

    let o = ExportOptions {
        outdir: outdir.clone(),
        album: None,
        artist: None,
        overwrite: false,
        cancel: CancelToken::new(),
    };
    let cancel = o.cancel.clone();
    let outdir_watch = outdir.clone();

    // Watcher thread: polls every 10ms until a .part file appears, then
    // cancels. Falls back to canceling after a 15s deadline so the test
    // cannot hang if the .part never appears.
    let watcher = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let found = std::fs::read_dir(&outdir_watch)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .any(|e| e.file_name().to_string_lossy().ends_with(".part"))
                })
                .unwrap_or(false);
            if found || std::time::Instant::now() >= deadline {
                cancel.cancel();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    let report = export(&p, &ff, &o, &mut |_| {}).unwrap();
    watcher.join().unwrap();

    // Both the killed song and the never-started song are reported Skipped.
    for result in &report.results {
        assert!(
            matches!(result.status, SongStatus::Skipped),
            "expected Skipped for track {}, got {:?}",
            result.track,
            result.status
        );
    }

    // No .mp3 and no .part files remain after cancellation.
    let leftover: Vec<_> = std::fs::read_dir(&outdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".mp3") || name.ends_with(".part"))
        .collect();
    assert!(
        leftover.is_empty(),
        "unexpected files left after cancel: {leftover:?}"
    );
}
