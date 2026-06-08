use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use jamsplit_core::audio::probe_audio;
use jamsplit_core::ffmpeg::{export, CancelToken, ExportOptions, FfmpegPaths, SongStatus};
use jamsplit_core::markers::{parse_markers_bytes, MarkerFormat, ParsedMarkers};
use jamsplit_core::plan::{check_collisions, plan, SplitPlan};
use jamsplit_core::report::{build_summary, render_table, write_summary};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "jamsplit",
    version,
    about = "Split one long jam recording into per-song MP3s"
)]
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

/// CLI-side format selector. `Auto` maps to `None` (auto-detect);
/// the named variants map to the corresponding `MarkerFormat`.
/// `clap::ValueEnum` gives lowercase names and lists them in `--help`.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum FormatArg {
    Auto,
    Audacity,
    Plain,
    Reaper,
    Dawproject,
}

impl FormatArg {
    /// Convert to the `Option<MarkerFormat>` that `parse_markers_bytes` expects.
    pub fn into_marker_format(self) -> Option<MarkerFormat> {
        match self {
            FormatArg::Auto => None,
            FormatArg::Audacity => Some(MarkerFormat::Audacity),
            FormatArg::Plain => Some(MarkerFormat::Plain),
            FormatArg::Reaper => Some(MarkerFormat::Reaper),
            FormatArg::Dawproject => Some(MarkerFormat::Dawproject),
        }
    }
}

#[derive(Args)]
pub struct CommonArgs {
    /// Input audio file (WAV expected; anything ffprobe reads is accepted)
    #[arg(long)]
    pub audio: PathBuf,
    /// Marker file (audacity, plain, reaper, or a Bitwig .dawproject)
    #[arg(long)]
    pub markers: PathBuf,
    /// Force the marker format instead of auto-detecting [auto|audacity|plain|reaper|dawproject]
    #[arg(long)]
    pub format: Option<FormatArg>,
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
    pub plan: SplitPlan,
}

/// The shared pipeline: locate ffmpeg, parse markers, probe audio, plan.
/// Prints warnings and the format announcement to stderr; returns Err with
/// everything already formatted for display.
pub fn load(common: &CommonArgs) -> Result<Loaded> {
    let ffmpeg = FfmpegPaths::locate(common.ffmpeg_path.as_deref())?;

    let bytes = std::fs::read(&common.markers)
        .with_context(|| format!("could not read marker file {}", common.markers.display()))?;
    let marker_format = common.format.and_then(|f| f.into_marker_format());
    let parsed = parse_markers_bytes(&bytes, marker_format).map_err(|errs| {
        let lines: Vec<String> = errs
            .iter()
            .map(|e| format!("{}: {e}", common.markers.display()))
            .collect();
        anyhow!("{}", lines.join("\n"))
    })?;
    let how = if marker_format.is_some() {
        "forced"
    } else {
        "auto-detected"
    };
    eprintln!("marker format: {} ({how})", parsed.format);

    let audio = probe_audio(&ffmpeg, &common.audio)?;

    let split_plan = plan(&parsed, &audio).map_err(|failure| {
        for w in &failure.warnings {
            eprintln!("warning: {w}");
        }
        anyhow!(
            "{}",
            failure
                .errors
                .iter()
                .map(|e| format!("error: {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;
    for w in &split_plan.warnings {
        eprintln!("warning: {w}");
    }

    Ok(Loaded {
        ffmpeg,
        parsed,
        plan: split_plan,
    })
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

/// Check for file collisions, print diagnostics, and bail if any are found.
/// Used by both the dry-run and real export paths so they cannot drift.
fn collision_gate(
    plan: &jamsplit_core::plan::SplitPlan,
    outdir: &std::path::Path,
    overwrite: bool,
) -> Result<()> {
    if let Err(collisions) = check_collisions(plan, outdir, overwrite) {
        for c in &collisions {
            eprintln!("error: {c}");
        }
        eprintln!("pass --overwrite to replace existing files");
        anyhow::bail!(
            "refusing to overwrite {} existing file(s)",
            collisions.len()
        );
    }
    Ok(())
}

/// Exit meaning: Ok(true) = all exports fine, Ok(false) = some failed (exit 2).
pub fn split(args: &SplitArgs) -> Result<bool> {
    let loaded = load(&args.common)?;
    let outdir = args.outdir.clone().unwrap_or_else(|| {
        PathBuf::from(
            args.common
                .audio
                .file_stem()
                .map(|s| s.to_os_string())
                .unwrap_or_else(|| "songs".into()),
        )
    });

    if args.dry_run {
        print!("{}", render_table(&loaded.plan));
        if !outdir.exists() {
            println!("would create directory: {}", outdir.display());
        }
        collision_gate(&loaded.plan, &outdir, args.overwrite)?;
        for song in &loaded.plan.songs {
            let target = outdir.join(&song.filename);
            let collides = if target.exists() {
                "  (would overwrite)"
            } else {
                ""
            };
            println!("would write: {}{collides}", target.display());
        }
        return Ok(true);
    }

    collision_gate(&loaded.plan, &outdir, args.overwrite)?;

    let opts = ExportOptions {
        outdir: outdir.clone(),
        album: args.album.clone(),
        artist: args.artist.clone(),
        overwrite: args.overwrite,
        cancel: CancelToken::new(),
    };
    let total = loaded.plan.songs.len();
    let report = export(&loaded.plan, &loaded.ffmpeg, &opts, &mut |r| {
        let outcome = match &r.status {
            SongStatus::Ok => "ok".to_string(),
            SongStatus::Failed { .. } => "FAILED".to_string(),
            SongStatus::Skipped => "skipped".to_string(),
        };
        println!("[{}/{total}] {} ... {outcome}", r.track, r.file.display());
    })?;

    // Emit per-song failure details first — before anything that can fail —
    // so the user always sees which songs failed even if summary write errors.
    if report.any_failed() {
        for r in &report.results {
            if let SongStatus::Failed { stderr_tail } = &r.status {
                eprintln!("error: song {} failed:\n{stderr_tail}", r.track);
            }
        }
    }

    let summary = build_summary(
        &loaded.plan,
        &report,
        &args.common.markers,
        &loaded.parsed.format.to_string(),
        args.album.as_deref(),
        args.artist.as_deref(),
    );
    match write_summary(&summary, &outdir) {
        Ok(summary_path) => println!("summary: {}", summary_path.display()),
        Err(e) if report.any_failed() => {
            // Export already failed: report summary error but still exit 2.
            eprintln!("error: could not write summary: {e}");
            return Ok(false);
        }
        Err(e) => {
            // All exports succeeded; summary write is the only failure.
            return Err(e).context("could not write summary");
        }
    }

    Ok(!report.any_failed())
}
