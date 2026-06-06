// Embeds the exe icon on Windows. Host and target are always the same
// platform in this project (no cross-compilation), so cfg(windows) on the
// build script is equivalent to targeting Windows.
#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/jamsplit.ico");
    winresource::WindowsResource::new()
        .set_icon("../../assets/icons/jamsplit.ico")
        .compile()
        .expect("embedding the Windows icon resource failed");
}

#[cfg(not(windows))]
fn main() {}
