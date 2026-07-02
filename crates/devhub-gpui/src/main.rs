#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod platform;
mod ui;

fn main() {
    app::run();
}
