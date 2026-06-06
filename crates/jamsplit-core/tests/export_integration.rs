mod common;

use common::{ffmpeg_or_skip, make_wav};
use jamsplit_core::audio::{probe_audio, AudioError};

#[test]
fn probes_a_real_wav() {
    let Some(ff) = ffmpeg_or_skip() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = make_wav(&ff, dir.path(), 10.0);
    let info = probe_audio(&ff, &wav).unwrap();
    assert!((info.duration_seconds - 10.0).abs() < 0.1, "duration: {}", info.duration_seconds);
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
