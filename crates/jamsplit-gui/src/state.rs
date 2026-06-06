use jamsplit_core::ffmpeg::FfmpegPaths;
use jamsplit_core::markers::MarkerFormat;
use std::path::{Path, PathBuf};

/// GUI-side format selector; `Auto` maps to `None` (auto-detect).
/// Mirrors the CLI's `FormatArg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatChoice {
    #[default]
    Auto,
    Audacity,
    Plain,
    Reaper,
}

impl FormatChoice {
    pub const ALL: [FormatChoice; 4] = [
        FormatChoice::Auto,
        FormatChoice::Audacity,
        FormatChoice::Plain,
        FormatChoice::Reaper,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FormatChoice::Auto => "auto",
            FormatChoice::Audacity => "audacity",
            FormatChoice::Plain => "plain",
            FormatChoice::Reaper => "reaper",
        }
    }

    /// Convert to the `Option<MarkerFormat>` that `parse_markers` expects.
    pub fn into_marker_format(self) -> Option<MarkerFormat> {
        match self {
            FormatChoice::Auto => None,
            FormatChoice::Audacity => Some(MarkerFormat::Audacity),
            FormatChoice::Plain => Some(MarkerFormat::Plain),
            FormatChoice::Reaper => Some(MarkerFormat::Reaper),
        }
    }
}

/// Everything the user can set. Mirrors the CLI's split arguments.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub audio: Option<PathBuf>,
    pub markers: Option<PathBuf>,
    pub format: FormatChoice,
    pub album: String,
    pub artist: String,
    /// None = default: `<audio dir>/<audio stem>/`.
    pub outdir: Option<PathBuf>,
    pub overwrite: bool,
}

/// Blank or whitespace-only tag fields are treated as unset.
pub fn none_if_blank(s: &str) -> Option<String> {
    let trimmed = s.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The whole GUI state machine, free of egui so it is unit-testable.
/// The app layer renders from it and feeds it events.
pub struct AppState {
    pub inputs: Inputs,
    /// Err holds the rendered LocateError, shown with the "Locate ffmpeg" picker.
    pub ffmpeg: Result<FfmpegPaths, String>,
}

impl AppState {
    pub fn new(ffmpeg: Result<FfmpegPaths, String>) -> Self {
        Self {
            inputs: Inputs::default(),
            ffmpeg,
        }
    }

    /// Explicit outdir if picked, else `<audio dir>/<audio stem>/`.
    /// None until an audio file is chosen.
    pub fn effective_outdir(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.inputs.outdir {
            return Some(dir.clone());
        }
        let audio = self.inputs.audio.as_ref()?;
        let stem = audio
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("songs"));
        Some(audio.parent().unwrap_or_else(|| Path::new("")).join(stem))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jamsplit_core::ffmpeg::FfmpegPaths;
    use std::path::PathBuf;

    pub fn fake_ffmpeg() -> FfmpegPaths {
        FfmpegPaths {
            ffmpeg: PathBuf::from("/nonexistent/ffmpeg"),
            ffprobe: PathBuf::from("/nonexistent/ffprobe"),
        }
    }

    #[test]
    fn default_outdir_is_stem_next_to_audio() {
        let mut state = AppState::new(Ok(fake_ffmpeg()));
        assert_eq!(state.effective_outdir(), None);
        state.inputs.audio = Some(PathBuf::from("/recordings/jam 2026-06-05.wav"));
        assert_eq!(
            state.effective_outdir(),
            Some(PathBuf::from("/recordings/jam 2026-06-05"))
        );
    }

    #[test]
    fn explicit_outdir_wins() {
        let mut state = AppState::new(Ok(fake_ffmpeg()));
        state.inputs.audio = Some(PathBuf::from("/recordings/jam.wav"));
        state.inputs.outdir = Some(PathBuf::from("/elsewhere/out"));
        assert_eq!(
            state.effective_outdir(),
            Some(PathBuf::from("/elsewhere/out"))
        );
    }

    #[test]
    fn format_choice_maps_to_core() {
        use jamsplit_core::markers::MarkerFormat;
        assert_eq!(FormatChoice::Auto.into_marker_format(), None);
        assert_eq!(
            FormatChoice::Reaper.into_marker_format(),
            Some(MarkerFormat::Reaper)
        );
    }

    #[test]
    fn blank_tags_become_none() {
        assert_eq!(none_if_blank(""), None);
        assert_eq!(none_if_blank("   "), None);
        assert_eq!(none_if_blank(" The Band "), Some("The Band".to_string()));
    }
}
