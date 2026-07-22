// Prevent additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    starlane_notes_desktop::run();
}
