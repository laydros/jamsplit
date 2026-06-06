use std::path::PathBuf;

/// What we learned about the input audio from ffprobe.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub path: PathBuf,
    pub duration_seconds: f64,
    pub codec_name: String,
    /// pcm_*, flac, alac. Lossy (or unknown) inputs get an accuracy warning.
    pub lossless: bool,
}
