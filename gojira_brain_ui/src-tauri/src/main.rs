#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod tauri_utils;

#[cfg(windows)]
fn main() {
    use crate::tauri_utils::app_state::{AppState, VaultState};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = app.get_webview_window("main").map(|w| w.set_focus());
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let salt_path = app
                .path()
                .app_local_data_dir()
                .expect("could not resolve app local data path")
                .join("stronghold_salt.bin");
            app.handle()
                .plugin(tauri_plugin_stronghold::Builder::with_argon2(&salt_path).build())?;

            let (tx, rx) = mpsc::channel(32);
            app.manage(AppState {
                tx,
                param_cache: Mutex::new(HashMap::new()),
                vault: Mutex::new(VaultState::default()),
                index_remap: Mutex::new(HashMap::new()),
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                crate::tauri_utils::ws_actor::run(rx, handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connect_ws,
            commands::disconnect_ws,
            commands::set_vault_passphrase,
            commands::has_api_key,
            commands::save_api_key,
            commands::clear_api_key,
            commands::get_index_remap,
            commands::set_index_remap,
            commands::reset_index_remap,
            commands::generate_tone,
            commands::apply_tone
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This Tauri UI host crate is intended to be built on Windows (REAPER target platform).");
}
