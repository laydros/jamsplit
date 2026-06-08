mod common;

use jamsplit_core::audio::AudioInfo;
use jamsplit_core::ffmpeg::{CancelToken, FfmpegPaths, SongStatus};
use jamsplit_core::plan::{Song, SplitPlan};
use jamsplit_gui::state::{ExportEnd, ExportRequest, PreviewRequest};
use jamsplit_gui::worker::{run_preview, spawn_export, Msg};

fn hand_built_plan(audio: std::path::PathBuf) -> SplitPlan {
    SplitPlan {
        songs: vec![
            Song {
                track: 1,
                title: "Opener".to_string(),
                filename: "01 - Opener.mp3".to_string(),
                start_seconds: 0.0,
                end_seconds: 5.0,
                to_eof: false,
            },
            Song {
                track: 2,
                title: "Closer".to_string(),
                filename: "02 - Closer.mp3".to_string(),
                start_seconds: 5.0,
                end_seconds: 10.0,
                to_eof: true,
            },
        ],
        audio: AudioInfo {
            path: audio,
            duration_seconds: 10.0,
            codec_name: "pcm_s16le".to_string(),
            lossless: true,
        },
        warnings: vec![],
    }
}

#[test]
fn canceled_export_skips_all_songs_and_still_writes_summary() {
    let dir = tempfile::tempdir().unwrap();
    let outdir = dir.path().join("out");
    let cancel = CancelToken::new();
    cancel.cancel(); // canceled before it starts: every song must be skipped
    let request = ExportRequest {
        plan: hand_built_plan(dir.path().join("jam.wav")),
        ffmpeg: FfmpegPaths {
            ffmpeg: "/nonexistent/ffmpeg".into(),
            ffprobe: "/nonexistent/ffprobe".into(),
        },
        outdir: outdir.clone(),
        album: None,
        artist: None,
        overwrite: false,
        cancel,
        markers: dir.path().join("songs.txt"),
        format_name: "plain".to_string(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_export(request, tx, || {});

    let mut skipped = 0;
    loop {
        match rx.recv().expect("worker disappeared") {
            Msg::Song(result) => {
                assert_eq!(result.status, SongStatus::Skipped);
                skipped += 1;
            }
            Msg::ExportDone(ExportEnd::Finished { report, summary }) => {
                assert!(report.canceled);
                let path = summary.expect("summary is written even on cancel");
                let json: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                assert_eq!(json["songs"][0]["status"], "skipped");
                assert_eq!(json["format"], "plain");
                break;
            }
            _ => panic!("unexpected message from export worker"),
        }
    }
    assert_eq!(skipped, 2);
}

#[test]
fn preview_happy_path_then_collision_then_overwrite() {
    let Some(ff) = common::ffmpeg_or_skip() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let audio = common::make_wav(&ff, dir.path(), 10.0);
    let markers = dir.path().join("songs.txt");
    std::fs::write(&markers, "0:00 Opener\n0:06 Closer\n").unwrap();
    let outdir = dir.path().join("out");

    let request = PreviewRequest {
        gen: 1,
        audio,
        markers,
        format: None,
        outdir: outdir.clone(),
        overwrite: false,
        ffmpeg: ff,
    };
    let outcome = run_preview(&request);
    assert_eq!(outcome.errors, Vec::<String>::new());
    assert_eq!(outcome.format, Some(("plain".to_string(), false)));
    let plan = outcome.plan.expect("plan should build");
    assert_eq!(plan.songs.len(), 2);
    assert_eq!(plan.songs[0].filename, "01 - Opener.mp3");
    assert!(outcome.collisions.is_empty());

    // a pre-existing target is a collision...
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::write(outdir.join("01 - Opener.mp3"), b"x").unwrap();
    let outcome = run_preview(&request);
    assert_eq!(outcome.collisions.len(), 1);
    assert!(outcome.plan.is_some()); // plan still shown alongside the error

    // ...unless overwrite is on
    let request = PreviewRequest {
        overwrite: true,
        ..request
    };
    let outcome = run_preview(&request);
    assert!(outcome.collisions.is_empty());
}

#[test]
fn export_writes_mp3s_and_summary() {
    let Some(ff) = common::ffmpeg_or_skip() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let audio = common::make_wav(&ff, dir.path(), 10.0);
    let markers = dir.path().join("songs.txt");
    std::fs::write(&markers, "0:00 Opener\n0:06 Closer\n").unwrap();
    let outdir = dir.path().join("out");

    // build the plan the same way the GUI does
    let preview = run_preview(&PreviewRequest {
        gen: 1,
        audio,
        markers: markers.clone(),
        format: None,
        outdir: outdir.clone(),
        overwrite: false,
        ffmpeg: ff.clone(),
    });
    let request = ExportRequest {
        plan: preview.plan.expect("plan should build"),
        ffmpeg: ff,
        outdir: outdir.clone(),
        album: Some("Practice".to_string()),
        artist: None,
        overwrite: false,
        cancel: CancelToken::new(),
        markers,
        format_name: "plain".to_string(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_export(request, tx, || {});

    let mut ok_songs = 0;
    loop {
        match rx.recv().expect("worker disappeared") {
            Msg::Song(result) => {
                assert_eq!(result.status, SongStatus::Ok, "song should export cleanly");
                ok_songs += 1;
            }
            Msg::ExportDone(ExportEnd::Finished { report, summary }) => {
                assert!(!report.canceled);
                assert!(!report.any_failed());
                summary.expect("summary written");
                break;
            }
            _ => panic!("unexpected message from export worker"),
        }
    }
    assert_eq!(ok_songs, 2);
    assert!(outdir.join("01 - Opener.mp3").is_file());
    assert!(outdir.join("02 - Closer.mp3").is_file());
    assert!(outdir.join("jamsplit-summary.json").is_file());
}

#[test]
fn preview_reads_a_dawproject_file() {
    let Some(ff) = common::ffmpeg_or_skip() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let audio = common::make_wav(&ff, dir.path(), 10.0);
    let project_xml = r#"<Project><Arrangement><Markers timeUnit="seconds">
        <Marker time="0.0" name="Opener"/>
        <Marker time="5.0" name="Closer"/>
    </Markers></Arrangement></Project>"#;
    let markers = common::make_dawproject(dir.path(), project_xml);
    let outdir = dir.path().join("out");

    let outcome = run_preview(&PreviewRequest {
        gen: 1,
        audio,
        markers,
        format: None,
        outdir,
        overwrite: false,
        ffmpeg: ff,
    });
    assert_eq!(outcome.errors, Vec::<String>::new());
    assert_eq!(outcome.format, Some(("dawproject".to_string(), false)));
    let plan = outcome.plan.expect("plan should build");
    assert_eq!(plan.songs.len(), 2);
    assert_eq!(plan.songs[0].filename, "01 - Opener.mp3");
}
