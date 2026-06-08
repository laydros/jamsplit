use jamsplit_core::ffmpeg::FfmpegPaths;
use std::path::{Path, PathBuf};

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

/// Write a minimal `.dawproject` (a zip holding `project.xml`) into `dir` and
/// return its path. `project_xml` is the file body.
pub fn make_dawproject(dir: &Path, project_xml: &str) -> PathBuf {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let path = dir.join("session.dawproject");
    let file = std::fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    zw.start_file("project.xml", SimpleFileOptions::default())
        .unwrap();
    zw.write_all(project_xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    path
}

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
