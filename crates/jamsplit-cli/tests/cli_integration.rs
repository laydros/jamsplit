mod common;

use assert_cmd::Command;
use common::{ffmpeg_or_skip, make_wav};
use predicates::prelude::*; // for .or() on predicates
use std::path::Path;

fn write_markers(dir: &Path, content: &str) -> std::path::PathBuf {
    let p = dir.join("markers.txt");
    std::fs::write(&p, content).unwrap();
    p
}

fn jamsplit() -> Command {
    Command::cargo_bin("jamsplit").unwrap()
}

#[test]
fn validate_ok_announces_format_and_exits_zero() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    jamsplit()
        .args(["validate", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .success()
        .stdout(predicates::str::contains("OK"))
        .stderr(predicates::str::contains("plain"));
}

#[test]
fn validate_duplicate_markers_exits_one() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n0:00 Dup\n");
    jamsplit()
        .args(["validate", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("duplicate"));
}

#[test]
fn validate_missing_audio_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let markers = write_markers(dir.path(), "0:00 One\n");
    jamsplit()
        .args(["validate", "--audio", "/no/such.wav", "--markers"]).arg(&markers)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("not found").or(predicates::str::contains("ffmpeg")));
}

#[test]
fn inspect_prints_the_track_table() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 Opening Jam\n5.0 Closer\n");
    jamsplit()
        .args(["inspect", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .assert()
        .success()
        .stdout(predicates::str::contains("track"))
        .stdout(predicates::str::contains("Opening Jam"));
}

#[test]
fn forced_format_is_respected() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    // looks like Audacity, forced to plain
    let markers = write_markers(dir.path(), "1.5\t2.5\tA\n");
    jamsplit()
        .args(["inspect", "--audio"]).arg(&wav)
        .arg("--markers").arg(&markers)
        .args(["--format", "plain"])
        .assert()
        .success()
        .stderr(predicates::str::contains("plain"));
}
