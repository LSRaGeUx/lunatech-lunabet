// Desktop entry point (used for `cargo tauri dev` on a laptop). On iOS/Android
// the generated projects call `lunabet_mobile_lib::run` via the mobile entry
// point in lib.rs instead.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lunabet_mobile_lib::run()
}
