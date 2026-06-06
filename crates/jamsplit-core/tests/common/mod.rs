use jamsplit_core::ffmpeg::FfmpegPaths;
use std::path::{Path, PathBuf};

/// Locate ffmpeg or skip the calling test with a notice. CI sets
/// JAMSPLIT_TEST_REQUIRE_FFMPEG=1 to turn a skip into a failure.
pub fn ffmpeg_or_skip() -> Option<FfmpegPaths> {
    match FfmpegPaths::locate(None) {
        Ok(paths) => Some(paths),
        Err(_) => {
            if std::env::var_os("JAMSPLIT_TEST_REQUIRE_FFMPEG").is_some() {
                panic!(
                    "ffmpeg is required (JAMSPLIT_TEST_REQUIRE_FFMPEG is set) but was not found"
                );
            }
            eprintln!("skipping: ffmpeg not available on this machine");
            None
        }
    }
}

/// Generate a small sine-wave WAV fixture for end-to-end tests.
pub fn make_wav(ff: &FfmpegPaths, dir: &Path, seconds: f64) -> PathBuf {
    let path = dir.join("fixture.wav");
    let status = std::process::Command::new(&ff.ffmpeg)
        .args(["-y", "-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={seconds}"))
        .args(["-ar", "44100", "-ac", "1"])
        .arg(&path)
        .status()
        .expect("could not run ffmpeg to build fixture");
    assert!(status.success(), "fixture generation failed");
    path
}
