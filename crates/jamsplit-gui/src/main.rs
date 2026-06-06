#![windows_subsystem = "windows"]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "jamsplit",
        options,
        Box::new(|_cc| Ok(Box::new(Placeholder))),
    )
}

struct Placeholder;

impl eframe::App for Placeholder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("jamsplit");
        });
    }
}
