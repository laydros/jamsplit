use std::path::{Path, PathBuf};

/// Resolved locations of the two binaries we drive.
#[derive(Debug, Clone, PartialEq)]
pub struct FfmpegPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LocateError {
    #[error("--ffmpeg-path {0} does not exist")]
    ExplicitNotFound(PathBuf),
    #[error("found ffmpeg at {0}, but no ffprobe next to it — both are required")]
    FfprobeMissingNextToExplicit(PathBuf),
    #[error(
        "ffmpeg/ffprobe not found (tried --ffmpeg-path, next to this executable, and PATH).\n\
         Install ffmpeg:\n\
         \x20 macOS:   brew install ffmpeg\n\
         \x20 Windows: winget install Gyan.FFmpeg\n\
         \x20 Linux:   your package manager (apt/dnf/pacman) install ffmpeg\n\
         or point at a binary with --ffmpeg-path, or place ffmpeg and ffprobe next to jamsplit."
    )]
    NotFound,
}

/// Platform-correct executable name.
fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

impl FfmpegPaths {
    /// Resolve with real process context (current_exe dir, PATH).
    pub fn locate(explicit: Option<&Path>) -> Result<Self, LocateError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self::locate_with(explicit, exe_dir.as_deref(), std::env::var_os("PATH").as_deref())
    }

    /// Injectable core, unit-testable without touching the real environment.
    pub fn locate_with(
        explicit: Option<&Path>,
        exe_dir: Option<&Path>,
        path_var: Option<&std::ffi::OsStr>,
    ) -> Result<Self, LocateError> {
        if let Some(ffmpeg) = explicit {
            if !ffmpeg.is_file() {
                return Err(LocateError::ExplicitNotFound(ffmpeg.to_path_buf()));
            }
            let ffprobe = ffmpeg
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(exe("ffprobe"));
            if !ffprobe.is_file() {
                return Err(LocateError::FfprobeMissingNextToExplicit(ffmpeg.to_path_buf()));
            }
            return Ok(Self { ffmpeg: ffmpeg.to_path_buf(), ffprobe });
        }

        if let Some(dir) = exe_dir {
            let ffmpeg = dir.join(exe("ffmpeg"));
            let ffprobe = dir.join(exe("ffprobe"));
            if ffmpeg.is_file() && ffprobe.is_file() {
                return Ok(Self { ffmpeg, ffprobe });
            }
        }

        if let Some(path_var) = path_var {
            let find = |name: &str| {
                std::env::split_paths(path_var)
                    .map(|d| d.join(exe(name)))
                    .find(|p| p.is_file())
            };
            if let (Some(ffmpeg), Some(ffprobe)) = (find("ffmpeg"), find("ffprobe")) {
                return Ok(Self { ffmpeg, ffprobe });
            }
        }

        Err(LocateError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(exe(name));
        File::create(&p).unwrap();
        p
    }

    #[test]
    fn explicit_path_with_sibling_ffprobe() {
        let dir = tempfile::tempdir().unwrap();
        let ffmpeg = touch(dir.path(), "ffmpeg");
        let ffprobe = touch(dir.path(), "ffprobe");
        let got = FfmpegPaths::locate_with(Some(&ffmpeg), None, None).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg, ffprobe });
    }

    #[test]
    fn explicit_path_missing_ffprobe_is_a_specific_error() {
        let dir = tempfile::tempdir().unwrap();
        let ffmpeg = touch(dir.path(), "ffmpeg");
        let err = FfmpegPaths::locate_with(Some(&ffmpeg), None, None).unwrap_err();
        assert!(matches!(err, LocateError::FfprobeMissingNextToExplicit(_)));
    }

    #[test]
    fn explicit_path_that_does_not_exist() {
        let err =
            FfmpegPaths::locate_with(Some(Path::new("/nope/ffmpeg")), None, None).unwrap_err();
        assert!(matches!(err, LocateError::ExplicitNotFound(_)));
    }

    #[test]
    fn adjacent_dir_wins_over_path() {
        let adjacent = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        let adj_ffmpeg = touch(adjacent.path(), "ffmpeg");
        let adj_ffprobe = touch(adjacent.path(), "ffprobe");
        touch(on_path.path(), "ffmpeg");
        touch(on_path.path(), "ffprobe");
        let path_var = std::env::join_paths([on_path.path()]).unwrap();
        let got = FfmpegPaths::locate_with(None, Some(adjacent.path()), Some(&path_var)).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg: adj_ffmpeg, ffprobe: adj_ffprobe });
    }

    #[test]
    fn path_search_allows_split_directories() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ffmpeg = touch(a.path(), "ffmpeg");
        let ffprobe = touch(b.path(), "ffprobe");
        let path_var = std::env::join_paths([a.path(), b.path()]).unwrap();
        let got = FfmpegPaths::locate_with(None, None, Some(&path_var)).unwrap();
        assert_eq!(got, FfmpegPaths { ffmpeg, ffprobe });
    }

    #[test]
    fn explicit_beats_adjacent_and_path() {
        let explicit_dir = tempfile::tempdir().unwrap();
        let adjacent_dir = tempfile::tempdir().unwrap();
        let path_dir = tempfile::tempdir().unwrap();

        // The pair we asked for explicitly.
        let exp_ffmpeg = touch(explicit_dir.path(), "ffmpeg");
        let exp_ffprobe = touch(explicit_dir.path(), "ffprobe");

        // Full decoy pair in the adjacent (exe_dir) directory.
        touch(adjacent_dir.path(), "ffmpeg");
        touch(adjacent_dir.path(), "ffprobe");

        // Full decoy pair in the PATH directory.
        touch(path_dir.path(), "ffmpeg");
        touch(path_dir.path(), "ffprobe");

        let path_var = std::env::join_paths([path_dir.path()]).unwrap();
        let got = FfmpegPaths::locate_with(
            Some(&exp_ffmpeg),
            Some(adjacent_dir.path()),
            Some(&path_var),
        )
        .unwrap();

        assert_eq!(got, FfmpegPaths { ffmpeg: exp_ffmpeg, ffprobe: exp_ffprobe });
    }

    #[test]
    fn nothing_found_mentions_the_flag() {
        let err = FfmpegPaths::locate_with(None, None, None).unwrap_err();
        assert!(err.to_string().contains("--ffmpeg-path"));
    }
}
