#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This Tauri UI is intended to be built on Windows (REAPER target platform).");
}

#[cfg(windows)]
mod app;

