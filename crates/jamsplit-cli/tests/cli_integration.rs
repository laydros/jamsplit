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
        .args(["validate", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
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
        .args(["validate", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("duplicate"));
}

#[test]
fn validate_missing_audio_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let markers = write_markers(dir.path(), "0:00 One\n");
    jamsplit()
        .args(["validate", "--audio", "/no/such.wav", "--markers"])
        .arg(&markers)
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
        .args(["inspect", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
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
        .args(["inspect", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .args(["--format", "plain"])
        .assert()
        .success()
        .stderr(predicates::str::contains("plain"));
}

#[test]
fn split_dry_run_writes_nothing() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicates::str::contains("would"));
    assert!(!outdir.exists(), "dry-run must not create the outdir");
}

#[test]
fn split_produces_files_and_summary() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .args(["--album", "Practice"])
        .assert()
        .success();
    assert!(outdir.join("01 - One.mp3").is_file());
    assert!(outdir.join("02 - Two.mp3").is_file());
    let summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outdir.join("jamsplit-summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(summary["album"], "Practice");
    assert_eq!(summary["songs"][0]["status"], "ok");
}

#[test]
fn split_default_outdir_is_audio_stem() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n");
    jamsplit()
        .current_dir(dir.path())
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .assert()
        .success();
    assert!(dir.path().join("fixture").join("01 - One.mp3").is_file());
}

#[test]
fn split_refuses_collisions_without_overwrite() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::write(outdir.join("01 - One.mp3"), b"old").unwrap();
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("01 - One.mp3"))
        .stderr(predicates::str::contains("--overwrite"));
    // the collision gate fires before ANY export happens
    assert!(!outdir.join("02 - Two.mp3").exists());

    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .arg("--overwrite")
        .assert()
        .success();
    assert!(outdir.join("02 - Two.mp3").is_file());
}

#[test]
fn split_partial_failure_exits_two_and_still_writes_summary() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n3.0 Two\n6.5 Three\n");
    let outdir = dir.path().join("out");
    // occupy song 2's .part path with a directory to force a failure
    std::fs::create_dir_all(outdir.join("02 - Two.mp3.part")).unwrap();
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .assert()
        .code(2);
    let summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outdir.join("jamsplit-summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(summary["songs"][1]["status"], "failed");
    assert_eq!(summary["songs"][0]["status"], "ok");
}

#[test]
fn dry_run_without_overwrite_exits_one_on_collision() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::write(outdir.join("01 - One.mp3"), b"old").unwrap();
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .arg("--dry-run")
        .assert()
        .code(1)
        .stderr(predicates::str::contains("would overwrite existing file"))
        .stderr(predicates::str::contains("--overwrite"));
    // dry-run must not touch any files
    assert_eq!(std::fs::read(outdir.join("01 - One.mp3")).unwrap(), b"old");
    assert!(!outdir.join("02 - Two.mp3").exists());
}

#[test]
fn dry_run_with_overwrite_shows_would_overwrite_label() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::write(outdir.join("01 - One.mp3"), b"old").unwrap();
    jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .arg("--dry-run")
        .arg("--overwrite")
        .assert()
        .success()
        .stdout(predicates::str::contains("(would overwrite)"));
    // dry-run must not touch any files
    assert_eq!(std::fs::read(outdir.join("01 - One.mp3")).unwrap(), b"old");
}

#[cfg(unix)]
#[test]
fn split_partial_failure_exits_two_even_when_summary_write_also_fails() {
    use std::os::unix::fs::PermissionsExt;
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let markers = write_markers(dir.path(), "0:00 One\n5.0 Two\n");
    // Pre-create the outdir and make it read-only so every song encode AND
    // summary write fail (can't create .part files or summary json in it).
    let outdir = dir.path().join("out");
    std::fs::create_dir_all(&outdir).unwrap();
    std::fs::set_permissions(&outdir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = jamsplit()
        .args(["split", "--audio"])
        .arg(&wav)
        .arg("--markers")
        .arg(&markers)
        .arg("--outdir")
        .arg(&outdir)
        .assert()
        .code(2)
        .stderr(predicates::str::contains("error: song"))
        .stderr(predicates::str::contains("could not write summary"));

    // Restore permissions so tempdir cleanup works.
    std::fs::set_permissions(&outdir, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(result);
}
