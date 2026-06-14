//! LunaBet native shell entry point (spec 12).
//!
//! Deliberately thin: the window points straight at the deployed LunaBet site
//! (`tauri.conf.json` -> app.windows[0].url), so this is a "remote shell" and a
//! product change ships server-side without a store resubmission. The native
//! parts (push token registration via APNs/FCM, deep-link handling) are wired
//! in spec 12 phase C and plug in here as plugins / setup hooks.
//!
//! `mobile_entry_point` is the symbol the generated iOS/Android projects call.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // .plugin(tauri_plugin_push::init())  // spec 12 phase C: APNs / FCM
        .run(tauri::generate_context!())
        .expect("error while running the LunaBet shell");
}
