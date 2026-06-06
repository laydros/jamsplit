use crate::plan::Song;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

impl FfmpegPaths {
    /// Resolve with real process context (current_exe dir, PATH).
    pub fn locate(explicit: Option<&Path>) -> Result<Self, LocateError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self::locate_with(
            explicit,
            exe_dir.as_deref(),
            std::env::var_os("PATH").as_deref(),
        )
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
                return Err(LocateError::FfprobeMissingNextToExplicit(
                    ffmpeg.to_path_buf(),
                ));
            }
            return Ok(Self {
                ffmpeg: ffmpeg.to_path_buf(),
                ffprobe,
            });
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

/// Shared cancel flag. The GUI's Cancel button sets it; the CLI passes one
/// that is never set.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub outdir: std::path::PathBuf,
    pub album: Option<String>,
    pub artist: Option<String>,
    /// Callers must run `plan::check_collisions` first; export itself
    /// renames over existing targets without asking.
    pub overwrite: bool,
    pub cancel: CancelToken,
}

/// The `.part` path a song is encoded into before the success-rename.
pub fn part_path(opts: &ExportOptions, song: &Song) -> std::path::PathBuf {
    opts.outdir.join(format!("{}.part", song.filename))
}

/// Build the exact ffmpeg argv for one song (everything after the program
/// name). Pure — unit-tested against the design doc's invocation.
pub fn build_song_args(
    audio: &Path,
    song: &Song,
    total: usize,
    opts: &ExportOptions,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-v", "error", "-y", "-ss"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(song.start_seconds.to_string().into());
    if !song.to_eof {
        args.push("-t".into());
        args.push((song.end_seconds - song.start_seconds).to_string().into());
    }
    args.push("-i".into());
    args.push(audio.as_os_str().to_os_string());
    for s in ["-map_metadata", "-1", "-c:a", "libmp3lame", "-q:a", "0"] {
        args.push(s.into());
    }
    args.push("-metadata".into());
    args.push(format!("title={}", song.title).into());
    args.push("-metadata".into());
    args.push(format!("track={}/{total}", song.track).into());
    if let Some(album) = &opts.album {
        args.push("-metadata".into());
        args.push(format!("album={album}").into());
    }
    if let Some(artist) = &opts.artist {
        args.push("-metadata".into());
        args.push(format!("artist={artist}").into());
    }
    args.push("-f".into());
    args.push("mp3".into());
    args.push(part_path(opts, song).into_os_string());
    args
}

#[derive(Debug, Clone, PartialEq)]
pub enum SongStatus {
    Ok,
    Failed { stderr_tail: String },
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongResult {
    pub track: usize,
    pub title: String,
    /// Final (post-rename) path the song was or would have been written to.
    pub file: std::path::PathBuf,
    pub status: SongStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportReport {
    pub results: Vec<SongResult>,
    pub canceled: bool,
}

impl ExportReport {
    pub fn any_failed(&self) -> bool {
        self.results
            .iter()
            .any(|r| matches!(r.status, SongStatus::Failed { .. }))
    }
}

/// Export every song in the plan. Creates `opts.outdir` if needed. Calls
/// `on_progress` after each song settles (ok, failed, or skipped).
pub fn export(
    plan: &crate::plan::SplitPlan,
    ffmpeg: &FfmpegPaths,
    opts: &ExportOptions,
    on_progress: &mut dyn FnMut(&SongResult),
) -> std::io::Result<ExportReport> {
    std::fs::create_dir_all(&opts.outdir).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "could not create output directory {}: {e}",
                opts.outdir.display()
            ),
        )
    })?;
    let total = plan.songs.len();
    let mut results = Vec::with_capacity(total);
    let mut canceled = false;

    for song in &plan.songs {
        let target = opts.outdir.join(&song.filename);
        if canceled || opts.cancel.is_canceled() {
            canceled = true;
            let result = SongResult {
                track: song.track,
                title: song.title.clone(),
                file: target,
                status: SongStatus::Skipped,
            };
            on_progress(&result);
            results.push(result);
            continue;
        }

        let part = part_path(opts, song);
        let status = match run_one(ffmpeg, &plan.audio.path, song, total, opts) {
            RunOutcome::Done => {
                // Windows cannot rename over an existing file
                if opts.overwrite && target.exists() {
                    let _ = std::fs::remove_file(&target);
                }
                match std::fs::rename(&part, &target) {
                    Ok(()) => SongStatus::Ok,
                    Err(e) => {
                        let _ = std::fs::remove_file(&part);
                        SongStatus::Failed {
                            stderr_tail: format!("rename failed: {e}"),
                        }
                    }
                }
            }
            RunOutcome::Failed(stderr_tail) => {
                let _ = std::fs::remove_file(&part);
                SongStatus::Failed { stderr_tail }
            }
            RunOutcome::Canceled => {
                let _ = std::fs::remove_file(&part);
                canceled = true;
                SongStatus::Skipped
            }
        };

        let result = SongResult {
            track: song.track,
            title: song.title.clone(),
            file: target,
            status,
        };
        on_progress(&result);
        results.push(result);
    }

    Ok(ExportReport { results, canceled })
}

enum RunOutcome {
    Done,
    Failed(String),
    Canceled,
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn run_one(
    ffmpeg: &FfmpegPaths,
    audio: &Path,
    song: &crate::plan::Song,
    total: usize,
    opts: &ExportOptions,
) -> RunOutcome {
    use std::io::Read;

    let mut child = match std::process::Command::new(&ffmpeg.ffmpeg)
        .args(build_song_args(audio, song, total, opts))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return RunOutcome::Failed(format!("could not start ffmpeg: {e}")),
    };

    // Drain stderr on a background thread while we poll: a child that writes
    // more than the OS pipe buffer would otherwise block mid-write and never
    // exit. The thread sees EOF once the child dies, so joining cannot hang.
    // read_to_end + from_utf8_lossy so non-UTF8 bytes (e.g. Windows paths
    // with non-ASCII characters) never cause a silent empty capture.
    let stderr_pipe = child.stderr.take();
    let drain = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let exit = loop {
        if opts.cancel.is_canceled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = drain.join();
            return RunOutcome::Canceled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = drain.join();
                return RunOutcome::Failed(format!("waiting on ffmpeg failed: {e}"));
            }
        }
    };

    let stderr_bytes = drain.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr_bytes);

    if exit.success() {
        RunOutcome::Done
    } else {
        RunOutcome::Failed(last_lines(stderr.trim(), 15))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn song(track: usize, title: &str, start: f64, end: f64, to_eof: bool) -> Song {
        Song {
            track,
            title: title.to_string(),
            filename: crate::plan::filename_for(track, 3, title),
            start_seconds: start,
            end_seconds: end,
            to_eof,
        }
    }

    fn opts(album: Option<&str>, artist: Option<&str>) -> ExportOptions {
        ExportOptions {
            outdir: "/out".into(),
            album: album.map(String::from),
            artist: artist.map(String::from),
            overwrite: false,
            cancel: CancelToken::new(),
        }
    }

    #[test]
    fn middle_song_args_match_the_design() {
        let s = song(2, "AC/DC Jam", 323.5, 410.0, false);
        let got = build_song_args(
            Path::new("/in/jam.wav"),
            &s,
            3,
            &opts(Some("Practice"), None),
        );
        let mut want: Vec<OsString> = [
            "-hide_banner",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-ss",
            "323.5",
            "-t",
            "86.5",
            "-i",
            "/in/jam.wav",
            "-map_metadata",
            "-1",
            "-c:a",
            "libmp3lame",
            "-q:a",
            "0",
            "-metadata",
            "title=AC/DC Jam", // tag keeps the slash — only filenames sanitize
            "-metadata",
            "track=2/3",
            "-metadata",
            "album=Practice",
            "-f",
            "mp3",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        // The output path is joined by part_path(), so its separator is
        // platform-native; build the expectation the same way.
        want.push(
            Path::new("/out")
                .join("02 - AC_DC Jam.mp3.part")
                .into_os_string(),
        );
        assert_eq!(got, want);
    }

    #[test]
    fn last_song_omits_duration() {
        let s = song(3, "Closer", 410.0, 600.0, true);
        let got = build_song_args(
            Path::new("/in/jam.wav"),
            &s,
            3,
            &opts(None, Some("The Band")),
        );
        let joined: Vec<String> = got
            .iter()
            .map(|o| o.to_string_lossy().into_owned())
            .collect();
        assert!(!joined.contains(&"-t".to_string()));
        assert!(joined.contains(&"artist=The Band".to_string()));
        assert!(!joined.iter().any(|a| a.starts_with("album=")));
    }

    #[test]
    fn last_lines_keeps_only_the_final_n() {
        let stderr = "one\ntwo\nthree\nfour\nfive";
        assert_eq!(last_lines(stderr, 3), "three\nfour\nfive");
    }

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
        assert_eq!(
            got,
            FfmpegPaths {
                ffmpeg: adj_ffmpeg,
                ffprobe: adj_ffprobe
            }
        );
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

        assert_eq!(
            got,
            FfmpegPaths {
                ffmpeg: exp_ffmpeg,
                ffprobe: exp_ffprobe
            }
        );
    }

    #[test]
    fn nothing_found_mentions_the_flag() {
        let err = FfmpegPaths::locate_with(None, None, None).unwrap_err();
        assert!(err.to_string().contains("--ffmpeg-path"));
    }
}
