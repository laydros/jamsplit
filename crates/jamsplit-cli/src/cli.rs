use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use jamsplit_core::audio::{probe_audio, AudioInfo};
use jamsplit_core::ffmpeg::FfmpegPaths;
use jamsplit_core::markers::{parse_markers, MarkerFormat, ParsedMarkers};
use jamsplit_core::plan::{plan, SplitPlan};
use jamsplit_core::report::render_table;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "jamsplit", version, about = "Split one long jam recording into per-song MP3s")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Export one MP3 per song
    Split(SplitArgs),
    /// Check that the audio + marker pair is usable; writes nothing
    Validate(CommonArgs),
    /// Show the split plan as a table; writes nothing
    Inspect(CommonArgs),
}

fn parse_format(s: &str) -> Result<MarkerFormat, String> {
    s.parse()
}

#[derive(Args)]
pub struct CommonArgs {
    /// Input audio file (WAV expected; anything ffprobe reads is accepted)
    #[arg(long)]
    pub audio: PathBuf,
    /// Marker file (audacity, plain, or reaper format)
    #[arg(long)]
    pub markers: PathBuf,
    /// Force the marker format instead of auto-detecting
    #[arg(long, value_parser = parse_format)]
    pub format: Option<MarkerFormat>,
    /// Path to an ffmpeg binary (ffprobe must sit next to it)
    #[arg(long)]
    pub ffmpeg_path: Option<PathBuf>,
}

#[derive(Args)]
pub struct SplitArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Output directory (default: ./<audio-file-stem>/)
    #[arg(long)]
    pub outdir: Option<PathBuf>,
    /// MP3 album tag (e.g. the session name)
    #[arg(long)]
    pub album: Option<String>,
    /// MP3 artist tag
    #[arg(long)]
    pub artist: Option<String>,
    /// Replace existing output files instead of refusing
    #[arg(long)]
    pub overwrite: bool,
    /// Show what would be exported without writing anything
    #[arg(long)]
    pub dry_run: bool,
}

pub struct Loaded {
    pub ffmpeg: FfmpegPaths,
    pub parsed: ParsedMarkers,
    pub audio: AudioInfo,
    pub plan: SplitPlan,
}

/// The shared pipeline: locate ffmpeg, parse markers, probe audio, plan.
/// Prints warnings and the format announcement to stderr; returns Err with
/// everything already formatted for display.
pub fn load(common: &CommonArgs) -> Result<Loaded> {
    let ffmpeg = FfmpegPaths::locate(common.ffmpeg_path.as_deref())?;

    let content = std::fs::read_to_string(&common.markers)
        .with_context(|| format!("could not read marker file {}", common.markers.display()))?;
    let parsed = parse_markers(&content, common.format).map_err(|errs| {
        let lines: Vec<String> = errs
            .iter()
            .map(|e| format!("{}: {e}", common.markers.display()))
            .collect();
        anyhow!("{}", lines.join("\n"))
    })?;
    let how = if common.format.is_some() { "forced" } else { "auto-detected" };
    eprintln!("marker format: {} ({how})", parsed.format);

    let audio = probe_audio(&ffmpeg, &common.audio)?;

    let split_plan = plan(&parsed, &audio).map_err(|failure| {
        for w in &failure.warnings {
            eprintln!("warning: {w}");
        }
        anyhow!(
            "{}",
            failure.errors.iter().map(|e| format!("error: {e}")).collect::<Vec<_>>().join("\n")
        )
    })?;
    for w in &split_plan.warnings {
        eprintln!("warning: {w}");
    }

    Ok(Loaded { ffmpeg, parsed, audio, plan: split_plan })
}

pub fn validate(args: &CommonArgs) -> Result<()> {
    let loaded = load(args)?;
    let total = loaded.plan.audio.duration_seconds;
    println!(
        "OK: {} songs over {}",
        loaded.plan.songs.len(),
        jamsplit_core::plan::fmt_time(total)
    );
    Ok(())
}

pub fn inspect(args: &CommonArgs) -> Result<()> {
    let loaded = load(args)?;
    print!("{}", render_table(&loaded.plan));
    Ok(())
}
