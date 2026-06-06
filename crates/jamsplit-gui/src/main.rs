#![windows_subsystem = "windows"]

/// Decode the embedded icon for the window/taskbar/dock. The `.icns`/`.ico`
/// artifacts cover Finder and Explorer; this covers the running window.
/// Deliberately the opaque web `icon-512.png` rather than the alpha master:
/// taskbars composite small icons on opaque chrome, where a filled square
/// reads cleaner than transparent corners.
fn load_icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../../assets/icons/icon-512.png"))
        .expect("embedded icon is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    eframe::egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([760.0, 560.0])
            .with_icon(load_icon()),
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
