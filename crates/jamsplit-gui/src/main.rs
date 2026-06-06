#![windows_subsystem = "windows"]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "jamsplit",
        options,
        Box::new(|cc| {
            // One tick larger than egui's defaults for legibility.
            cc.egui_ctx.all_styles_mut(|style| {
                for font_id in style.text_styles.values_mut() {
                    font_id.size += 1.0;
                }
            });
            Ok(Box::new(jamsplit_gui::app::JamsplitApp::new()))
        }),
    )
}
