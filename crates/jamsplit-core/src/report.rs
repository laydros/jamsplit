use crate::ffmpeg::{ExportReport, SongStatus};
use crate::plan::{fmt_time, SplitPlan};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Summary {
    pub source_audio: PathBuf,
    pub markers_file: PathBuf,
    pub format: String,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub tool_version: String,
    pub warnings: Vec<String>,
    pub songs: Vec<SongSummary>,
}

#[derive(Debug, Serialize)]
pub struct SongSummary {
    pub track: usize,
    pub title: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub file: PathBuf,
    /// "ok" | "failed" | "skipped"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Combine the plan and the export outcomes into the summary structure.
pub fn build_summary(
    plan: &SplitPlan,
    report: &ExportReport,
    markers_file: &Path,
    format: &str,
    album: Option<&str>,
    artist: Option<&str>,
) -> Summary {
    let songs = plan
        .songs
        .iter()
        .zip(&report.results)
        .map(|(song, result)| {
            let (status, error) = match &result.status {
                SongStatus::Ok => ("ok", None),
                SongStatus::Failed { stderr_tail } => ("failed", Some(stderr_tail.clone())),
                SongStatus::Skipped => ("skipped", None),
            };
            SongSummary {
                track: song.track,
                title: song.title.clone(),
                start_seconds: song.start_seconds,
                end_seconds: song.end_seconds,
                duration_seconds: song.end_seconds - song.start_seconds,
                file: result.file.clone(),
                status: status.to_string(),
                error,
            }
        })
        .collect();
    Summary {
        source_audio: plan.audio.path.clone(),
        markers_file: markers_file.to_path_buf(),
        format: format.to_string(),
        album: album.map(String::from),
        artist: artist.map(String::from),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        warnings: plan.warnings.clone(),
        songs,
    }
}

/// Write `jamsplit-summary.json` into the outdir, pretty-printed.
pub fn write_summary(summary: &Summary, outdir: &Path) -> std::io::Result<PathBuf> {
    let path = outdir.join("jamsplit-summary.json");
    let json = serde_json::to_string_pretty(summary).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Human track table for inspect/dry-run/post-split output.
pub fn render_table(plan: &SplitPlan) -> String {
    let mut out = String::from("track  start      end        length     title\n");
    for song in &plan.songs {
        out.push_str(&format!(
            "{:>4}   {:<10} {:<10} {:<10} {}\n",
            song.track,
            fmt_time(song.start_seconds),
            fmt_time(song.end_seconds),
            fmt_time(song.end_seconds - song.start_seconds),
            song.title
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioInfo;
    use crate::ffmpeg::SongResult;
    use crate::plan::Song;

    fn test_plan() -> SplitPlan {
        let audio = AudioInfo {
            path: "/in/jam.wav".into(),
            duration_seconds: 600.0,
            codec_name: "pcm_s16le".to_string(),
            lossless: true,
        };
        let songs = vec![
            Song {
                track: 1,
                title: "One".into(),
                filename: "01 - One.mp3".into(),
                start_seconds: 0.0,
                end_seconds: 323.5,
                to_eof: false,
            },
            Song {
                track: 2,
                title: "Two".into(),
                filename: "02 - Two.mp3".into(),
                start_seconds: 323.5,
                end_seconds: 600.0,
                to_eof: true,
            },
        ];
        SplitPlan {
            songs,
            audio,
            warnings: vec!["a warning".to_string()],
        }
    }

    fn test_report() -> ExportReport {
        ExportReport {
            results: vec![
                SongResult {
                    track: 1,
                    title: "One".into(),
                    file: "/out/01 - One.mp3".into(),
                    status: SongStatus::Ok,
                },
                SongResult {
                    track: 2,
                    title: "Two".into(),
                    file: "/out/02 - Two.mp3".into(),
                    status: SongStatus::Failed {
                        stderr_tail: "boom".into(),
                    },
                },
            ],
            canceled: false,
        }
    }

    #[test]
    fn summary_carries_statuses_and_errors() {
        let s = build_summary(
            &test_plan(),
            &test_report(),
            Path::new("/in/markers.txt"),
            "plain",
            Some("Album"),
            None,
        );
        assert_eq!(s.tool_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(s.songs[0].status, "ok");
        assert_eq!(s.songs[0].error, None);
        assert_eq!(s.songs[1].status, "failed");
        assert_eq!(s.songs[1].error.as_deref(), Some("boom"));
        assert_eq!(s.warnings, vec!["a warning".to_string()]);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"track\":1"));
        assert!(!json.contains("\"error\":null")); // skipped when None
    }

    #[test]
    fn write_summary_creates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = build_summary(
            &test_plan(),
            &test_report(),
            Path::new("/in/markers.txt"),
            "plain",
            None,
            None,
        );
        let path = write_summary(&s, dir.path()).unwrap();
        assert_eq!(path.file_name().unwrap(), "jamsplit-summary.json");
        let read: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read["songs"][1]["status"], "failed");
    }

    #[test]
    fn table_lists_every_song_with_times() {
        let t = render_table(&test_plan());
        let expected = "\
track  start      end        length     title
   1   0:00.0     5:23.5     5:23.5     One
   2   5:23.5     10:00.0    4:36.5     Two
";
        assert_eq!(t, expected);
    }
}
