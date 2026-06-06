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

/// Result of one preview pipeline run (parse → probe → plan → collisions).
/// Stages that failed leave their slot empty and add to `errors`.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewOutcome {
    /// (format name, forced?) — None when parsing failed.
    pub format: Option<(String, bool)>,
    pub plan: Option<jamsplit_core::plan::SplitPlan>,
    pub errors: Vec<String>,
    /// Existing-output collisions. Shown like errors but with an
    /// "enable Overwrite" hint; gate Split alongside `errors`.
    pub collisions: Vec<String>,
    pub warnings: Vec<String>,
}

impl PreviewOutcome {
    pub fn format_label(&self) -> Option<String> {
        self.format.as_ref().map(|(name, forced)| {
            let how = if *forced { "forced" } else { "auto-detected" };
            format!("marker format: {name} ({how})")
        })
    }
}

/// One preview job for the worker thread. `gen` ties the result back to
/// the request so stale results can be discarded.
#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub gen: u64,
    pub audio: PathBuf,
    pub markers: PathBuf,
    pub format: Option<MarkerFormat>,
    pub outdir: PathBuf,
    pub overwrite: bool,
    pub ffmpeg: FfmpegPaths,
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
    /// Most recent completed preview result.
    pub preview: Option<PreviewOutcome>,
    /// True while a preview job is outstanding.
    pub preview_pending: bool,
    next_gen: u64,
}

impl AppState {
    pub fn new(ffmpeg: Result<FfmpegPaths, String>) -> Self {
        Self {
            inputs: Inputs::default(),
            ffmpeg,
            preview: None,
            preview_pending: false,
            next_gen: 0,
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

    /// Call after audio/markers/format (or ffmpeg) change. Returns the job
    /// to hand to the worker, or None while inputs are incomplete. Bumping
    /// the generation makes any in-flight result stale.
    pub fn request_preview(&mut self) -> Option<PreviewRequest> {
        let ffmpeg = self.ffmpeg.as_ref().ok()?.clone();
        let audio = self.inputs.audio.clone()?;
        let markers = self.inputs.markers.clone()?;
        let outdir = self.effective_outdir()?;
        self.next_gen += 1;
        self.preview_pending = true;
        Some(PreviewRequest {
            gen: self.next_gen,
            audio,
            markers,
            format: self.inputs.format.into_marker_format(),
            outdir,
            overwrite: self.inputs.overwrite,
            ffmpeg,
        })
    }

    /// Worker result arrived. Discarded unless it answers the latest request.
    pub fn on_preview(&mut self, gen: u64, outcome: PreviewOutcome) {
        if gen != self.next_gen {
            return;
        }
        self.preview_pending = false;
        self.preview = Some(outcome);
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

    fn ok_outcome() -> PreviewOutcome {
        PreviewOutcome {
            format: Some(("plain".to_string(), false)),
            plan: None,
            errors: vec![],
            collisions: vec![],
            warnings: vec![],
        }
    }

    fn ready_state() -> AppState {
        let mut state = AppState::new(Ok(fake_ffmpeg()));
        state.inputs.audio = Some(PathBuf::from("/recordings/jam.wav"));
        state.inputs.markers = Some(PathBuf::from("/recordings/songs.txt"));
        state
    }

    #[test]
    fn no_preview_until_both_files_picked() {
        let mut state = AppState::new(Ok(fake_ffmpeg()));
        assert!(state.request_preview().is_none());
        state.inputs.audio = Some(PathBuf::from("/recordings/jam.wav"));
        assert!(state.request_preview().is_none());
        state.inputs.markers = Some(PathBuf::from("/recordings/songs.txt"));
        assert!(state.request_preview().is_some());
    }

    #[test]
    fn no_preview_without_ffmpeg() {
        let mut state = AppState::new(Err("not found".to_string()));
        state.inputs.audio = Some(PathBuf::from("/recordings/jam.wav"));
        state.inputs.markers = Some(PathBuf::from("/recordings/songs.txt"));
        assert!(state.request_preview().is_none());
    }

    #[test]
    fn request_carries_inputs_and_bumps_generation() {
        let mut state = ready_state();
        let first = state.request_preview().unwrap();
        assert_eq!(first.audio, PathBuf::from("/recordings/jam.wav"));
        assert_eq!(first.outdir, PathBuf::from("/recordings/jam"));
        assert!(state.preview_pending);
        let second = state.request_preview().unwrap();
        assert!(second.gen > first.gen);
    }

    #[test]
    fn stale_preview_results_are_discarded() {
        let mut state = ready_state();
        let first = state.request_preview().unwrap();
        let second = state.request_preview().unwrap();
        state.on_preview(first.gen, ok_outcome()); // stale: ignored
        assert!(state.preview.is_none());
        assert!(state.preview_pending);
        state.on_preview(second.gen, ok_outcome()); // current: accepted
        assert!(state.preview.is_some());
        assert!(!state.preview_pending);
    }
}
