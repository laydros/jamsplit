mod common;

use jamsplit_gui::state::PreviewRequest;
use jamsplit_gui::worker::run_preview;

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
