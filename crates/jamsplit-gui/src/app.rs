use crate::state::{AppState, ExportEnd, FormatChoice, Phase};
use crate::worker::{self, Msg};
use eframe::egui;
use jamsplit_core::ffmpeg::{FfmpegPaths, SongResult, SongStatus};
use jamsplit_core::plan::fmt_time;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};

pub struct JamsplitApp {
    state: AppState,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Default for JamsplitApp {
    fn default() -> Self {
        Self::new()
    }
}

impl JamsplitApp {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let ffmpeg = FfmpegPaths::locate(None).map_err(|e| e.to_string());
        Self {
            state: AppState::new(ffmpeg),
            tx,
            rx,
        }
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Preview { gen, outcome } => self.state.on_preview(gen, outcome),
                Msg::Song(result) => self.state.on_song(result),
                Msg::ExportDone(end) => self.state.on_export_done(end),
            }
        }
    }

    fn kick_preview(&mut self, ctx: &egui::Context) {
        if let Some(request) = self.state.request_preview() {
            let ctx = ctx.clone();
            worker::spawn_preview(request, self.tx.clone(), move || ctx.request_repaint());
        }
    }

    /// Handles the ffmpeg failure UI only. When ffmpeg is found, the resolved
    /// path is shown in the footer. On a locate failure, this method renders
    /// the error (with install hints) and a picker — the GUI's equivalent of
    /// --ffmpeg-path.
    fn ui_ffmpeg(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Err(message) = self.state.ffmpeg.clone() else {
            return;
        };
        error_label(ui, message);
        if ui.button("Locate ffmpeg…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Select the ffmpeg binary (ffprobe must sit next to it)")
                .pick_file()
            {
                self.state.ffmpeg = FfmpegPaths::locate(Some(&path)).map_err(|e| e.to_string());
                self.kick_preview(ctx);
            }
        }
        ui.separator();
    }

    fn ui_inputs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut preview_dirty = false;
        let mut collisions_dirty = false;

        egui::Grid::new("inputs").num_columns(3).show(ui, |ui| {
            ui.strong("Audio:");
            if ui.button("Choose…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select the session recording")
                    .add_filter(
                        "audio",
                        &["wav", "flac", "aiff", "aif", "mp3", "m4a", "ogg"],
                    )
                    .add_filter("all files", &["*"])
                    .pick_file()
                {
                    self.state.inputs.audio = Some(path);
                    preview_dirty = true;
                }
            }
            ui.label(display_path(self.state.inputs.audio.as_deref()));
            ui.end_row();

            ui.strong("Markers:");
            if ui.button("Choose…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select the marker file")
                    .add_filter("markers", &["txt", "csv", "dawproject"])
                    .add_filter("all files", &["*"])
                    .pick_file()
                {
                    self.state.inputs.markers = Some(path);
                    preview_dirty = true;
                }
            }
            ui.label(display_path(self.state.inputs.markers.as_deref()));
            ui.end_row();

            ui.strong("Format:");
            let format_before = self.state.inputs.format;
            egui::ComboBox::from_id_salt("format")
                .selected_text(self.state.inputs.format.label())
                .show_ui(ui, |ui| {
                    for choice in FormatChoice::ALL {
                        ui.selectable_value(&mut self.state.inputs.format, choice, choice.label());
                    }
                });
            if self.state.inputs.format != format_before {
                preview_dirty = true;
            }
            ui.label("");
            ui.end_row();

            ui.strong("Album:");
            ui.text_edit_singleline(&mut self.state.inputs.album); // tags only
            ui.label("");
            ui.end_row();

            ui.strong("Artist:");
            ui.text_edit_singleline(&mut self.state.inputs.artist); // tags only
            ui.label("");
            ui.end_row();

            ui.strong("Output dir:");
            if ui.button("Choose…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select the output directory")
                    .pick_folder()
                {
                    self.state.inputs.outdir = Some(path);
                    collisions_dirty = true;
                }
            }
            if self.state.inputs.outdir.is_some() {
                ui.label(display_path(self.state.inputs.outdir.as_deref()));
            } else {
                match self.state.effective_outdir() {
                    Some(default) => ui.weak(format!("(default: {})", default.display())),
                    None => ui.weak("(default: next to the audio file)"),
                };
            }
            ui.end_row();

            ui.label("");
            if ui
                .checkbox(&mut self.state.inputs.overwrite, "Overwrite existing files")
                .changed()
            {
                collisions_dirty = true;
            }
            ui.label("");
            ui.end_row();
        });

        if preview_dirty {
            self.kick_preview(ctx);
        } else if collisions_dirty {
            self.state.recheck_collisions();
        }
    }

    fn ui_preview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.state.preview_pending {
            ui.spinner();
            return;
        }
        let Some(preview) = self.state.preview.clone() else {
            ui.weak("Pick an audio file and a marker file to preview the split.");
            return;
        };
        if let Some(label) = preview.format_label() {
            ui.label(label);
        }
        for warning in &preview.warnings {
            warn_label(ui, format!("warning: {warning}"));
        }
        for error in &preview.errors {
            error_label(ui, format!("error: {error}"));
        }
        for collision in &preview.collisions {
            error_label(ui, format!("error: {collision}"));
        }
        if !preview.collisions.is_empty() {
            ui.weak("Check \"Overwrite existing files\" to replace them.");
        }
        if let Some(plan) = &preview.plan {
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 40.0)
                .show(ui, |ui| {
                    egui::Grid::new("plan")
                        .striped(true)
                        .num_columns(6)
                        .show(ui, |ui| {
                            ui.strong("track");
                            ui.strong("start");
                            ui.strong("end");
                            ui.strong("length");
                            ui.strong("title");
                            ui.strong("filename");
                            ui.end_row();
                            for song in &plan.songs {
                                ui.label(song.track.to_string());
                                ui.label(fmt_time(song.start_seconds));
                                ui.label(fmt_time(song.end_seconds));
                                ui.label(fmt_time(song.end_seconds - song.start_seconds));
                                ui.label(&song.title);
                                ui.label(&song.filename);
                                ui.end_row();
                            }
                        });
                });
        }
        ui.separator();
        if ui
            .add_enabled(self.state.can_split(), egui::Button::new("Split"))
            .clicked()
        {
            if let Some(request) = self.state.start_export() {
                let ctx = ctx.clone();
                worker::spawn_export(request, self.tx.clone(), move || ctx.request_repaint());
            }
        }
    }

    fn ui_exporting(&self, ui: &mut egui::Ui) {
        let Phase::Exporting {
            results,
            total,
            cancel,
        } = &self.state.phase
        else {
            return;
        };
        ui.heading("Exporting…");
        // Cancel sits on a fixed row beside the progress bar; the song log
        // below grows without moving it.
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel.cancel();
            }
            ui.add(
                egui::ProgressBar::new(results.len() as f32 / (*total).max(1) as f32)
                    .text(format!("{} / {total}", results.len())),
            );
        });
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for result in results {
                    song_line(ui, result);
                }
            });
    }

    fn ui_done(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Phase::Done(end) = &self.state.phase else {
            return;
        };
        let end = end.clone();
        match &end {
            ExportEnd::Failed(message) => {
                ui.heading("Export failed");
                error_label(ui, message);
            }
            ExportEnd::Finished { report, summary } => {
                let failed = report
                    .results
                    .iter()
                    .filter(|r| matches!(r.status, SongStatus::Failed { .. }))
                    .count();
                let heading = match (report.canceled, failed) {
                    (true, _) => "Canceled — completed songs were kept".to_string(),
                    (false, 0) => "Done".to_string(),
                    (false, n) => format!("Done — {n} song(s) failed"),
                };
                ui.heading(heading);
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 60.0)
                    .show(ui, |ui| {
                        for result in &report.results {
                            song_line(ui, result);
                        }
                    });
                match summary {
                    Ok(path) => {
                        ui.weak(format!("summary: {}", path.display()));
                    }
                    Err(message) => {
                        error_label(ui, message);
                    }
                }
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(outdir) = self.state.effective_outdir() {
                if ui.button("Open output folder").clicked() {
                    let _ = open::that(outdir);
                }
            }
            if ui.button("Back").clicked() {
                self.kick_preview(ctx);
            }
        });
    }
}

impl eframe::App for JamsplitApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("jamsplit {}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.hyperlink_to("Website", "https://laydros.github.io/jamsplit/");
                if let Ok(paths) = &self.state.ffmpeg {
                    ui.separator();
                    let text = format!("ffmpeg: {}", paths.ffmpeg.display());
                    // A truncated label tooltips its own full text when
                    // elided — no explicit hover text, it would double up.
                    ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate());
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_ffmpeg(ui, ctx);
            let exporting = matches!(self.state.phase, Phase::Exporting { .. });
            ui.add_enabled_ui(!exporting, |ui| self.ui_inputs(ui, ctx));
            ui.separator();
            if matches!(self.state.phase, Phase::Setup) {
                self.ui_preview(ui, ctx);
            } else if matches!(self.state.phase, Phase::Exporting { .. }) {
                self.ui_exporting(ui);
            } else {
                self.ui_done(ui, ctx);
            }
        });
    }
}

fn display_path(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| "—".to_string())
}

/// Bold red error line — bold keeps it legible on both themes.
fn error_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) {
    ui.label(text.into().color(egui::Color32::LIGHT_RED).strong());
}

/// Bold warning line. Stock warn_fg_color is orange and reads as an error;
/// stay in the yellow family while keeping contrast on both themes.
fn warn_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) {
    let color = if ui.visuals().dark_mode {
        egui::Color32::YELLOW
    } else {
        egui::Color32::from_rgb(139, 109, 0)
    };
    ui.label(text.into().color(color).strong());
}

/// One settled song: "  1  /path/01 - Opener.mp3  ok". Failures expand to
/// show the ffmpeg stderr tail.
fn song_line(ui: &mut egui::Ui, result: &SongResult) {
    match &result.status {
        SongStatus::Ok => {
            ui.label(format!(
                "{:>3}  {}  ok",
                result.track,
                result.file.display()
            ));
        }
        SongStatus::Skipped => {
            ui.weak(format!(
                "{:>3}  {}  skipped",
                result.track,
                result.file.display()
            ));
        }
        SongStatus::Failed { stderr_tail } => {
            error_label(
                ui,
                format!("{:>3}  {}  FAILED", result.track, result.file.display()),
            );
            ui.collapsing(format!("ffmpeg output (track {})", result.track), |ui| {
                ui.monospace(stderr_tail);
            });
        }
    }
}
