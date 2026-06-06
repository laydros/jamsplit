use std::path::{Path, PathBuf};

use crate::ffmpeg::FfmpegPaths;

/// What we learned about the input audio from ffprobe.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub path: PathBuf,
    pub duration_seconds: f64,
    pub codec_name: String,
    /// pcm_*, flac, alac. Lossy (or unknown) inputs get an accuracy warning.
    pub lossless: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("audio file not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("ffprobe failed on {path}: {stderr}")]
    ProbeFailed { path: std::path::PathBuf, stderr: String },
    #[error("{0} has no audio stream")]
    NoAudioStream(std::path::PathBuf),
    #[error("could not read a duration from {0}")]
    NoDuration(std::path::PathBuf),
}

/// Lossless codecs get sample-accurate seeking; everything else warns.
fn is_lossless(codec: &str) -> bool {
    codec.starts_with("pcm_") || matches!(codec, "flac" | "alac")
}

/// Parse `ffprobe -of json` output (pure, unit-testable).
pub fn parse_ffprobe_output(json: &str, path: &Path) -> Result<AudioInfo, AudioError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| AudioError::ProbeFailed {
        path: path.to_path_buf(),
        stderr: format!("unparseable ffprobe output: {e}"),
    })?;
    let codec_name = v["streams"][0]["codec_name"]
        .as_str()
        .ok_or_else(|| AudioError::NoAudioStream(path.to_path_buf()))?
        .to_string();
    let duration_seconds: f64 = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .filter(|d: &f64| d.is_finite() && *d > 0.0)
        .ok_or_else(|| AudioError::NoDuration(path.to_path_buf()))?;
    let lossless = is_lossless(&codec_name);
    Ok(AudioInfo { path: path.to_path_buf(), duration_seconds, codec_name, lossless })
}

/// Run ffprobe on `path` and build an `AudioInfo`.
pub fn probe_audio(ffmpeg: &FfmpegPaths, path: &Path) -> Result<AudioInfo, AudioError> {
    if !path.is_file() {
        return Err(AudioError::NotFound(path.to_path_buf()));
    }
    let output = std::process::Command::new(&ffmpeg.ffprobe)
        .args([
            "-v", "error",
            "-select_streams", "a:0",
            "-show_entries", "stream=codec_name",
            "-show_entries", "format=duration",
            "-of", "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| AudioError::ProbeFailed {
            path: path.to_path_buf(),
            stderr: format!("could not run ffprobe: {e}"),
        })?;
    if !output.status.success() {
        return Err(AudioError::ProbeFailed {
            path: path.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    parse_ffprobe_output(&String::from_utf8_lossy(&output.stdout), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wav_probe_output() {
        let json = r#"{"streams":[{"codec_name":"pcm_s16le"}],"format":{"duration":"10.005333"}}"#;
        let info = parse_ffprobe_output(json, Path::new("/tmp/jam.wav")).unwrap();
        assert_eq!(info.codec_name, "pcm_s16le");
        assert!(info.lossless);
        assert!((info.duration_seconds - 10.005333).abs() < 1e-9);
    }

    #[test]
    fn mp3_is_lossy_flac_is_not() {
        let mp3 = r#"{"streams":[{"codec_name":"mp3"}],"format":{"duration":"5.0"}}"#;
        assert!(!parse_ffprobe_output(mp3, Path::new("a.mp3")).unwrap().lossless);
        let flac = r#"{"streams":[{"codec_name":"flac"}],"format":{"duration":"5.0"}}"#;
        assert!(parse_ffprobe_output(flac, Path::new("a.flac")).unwrap().lossless);
    }

    #[test]
    fn no_streams_and_no_duration_are_specific_errors() {
        let none = r#"{"streams":[],"format":{"duration":"5.0"}}"#;
        assert!(matches!(
            parse_ffprobe_output(none, Path::new("x")).unwrap_err(),
            AudioError::NoAudioStream(_)
        ));
        let nodur = r#"{"streams":[{"codec_name":"pcm_s16le"}],"format":{}}"#;
        assert!(matches!(
            parse_ffprobe_output(nodur, Path::new("x")).unwrap_err(),
            AudioError::NoDuration(_)
        ));
        let infdur = r#"{"streams":[{"codec_name":"pcm_s16le"}],"format":{"duration":"inf"}}"#;
        assert!(matches!(
            parse_ffprobe_output(infdur, Path::new("x")).unwrap_err(),
            AudioError::NoDuration(_)
        ));
    }
}
