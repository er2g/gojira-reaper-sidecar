mod app_state;
mod calibration;
mod cleaner;
mod diff;
mod gemini;
mod param_map;
mod system_prompt;
mod vault;
mod ws_actor;

use crate::app_state::{AppState, UiCommand, VaultState};
use crate::diff::diff_params;
use crate::gemini::{ToneRequest, ToneResponse};
use crate::vault::VaultError;
use gojira_protocol::{ClientCommand, MergeMode, ParamChange};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[derive(serde::Serialize)]
struct PreviewResult {
    reasoning: String,
    params: Vec<ParamChange>,
    diff: Vec<diff::DiffItem>,
}

#[tauri::command]
async fn connect_ws(state: State<'_, AppState>) -> Result<(), String> {
    state
        .tx
        .send(UiCommand::Connect)
        .await
        .map_err(|_| "ws actor unavailable".to_string())
}

#[tauri::command]
async fn disconnect_ws(state: State<'_, AppState>) -> Result<(), String> {
    state
        .tx
        .send(UiCommand::Disconnect)
        .await
        .map_err(|_| "ws actor unavailable".to_string())
}

#[tauri::command]
fn set_vault_passphrase(state: State<'_, AppState>, passphrase: String) -> Result<(), String> {
    let mut guard = state.vault.lock().map_err(|_| "vault lock poisoned")?;
    guard.passphrase = Some(passphrase);
    Ok(())
}

#[tauri::command]
fn has_api_key(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| VaultError::PassphraseNotSet.to_string())?;
    let key = crate::vault::load_api_key(&app, &pass).map_err(|e| e.to_string())?;
    Ok(key.is_some())
}

#[tauri::command]
fn save_api_key(app: AppHandle, state: State<'_, AppState>, api_key: String) -> Result<(), String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| VaultError::PassphraseNotSet.to_string())?;
    crate::vault::save_api_key(&app, &pass, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_api_key(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| VaultError::PassphraseNotSet.to_string())?;
    crate::vault::clear_api_key(&app, &pass).map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_tone(
    app: AppHandle,
    state: State<'_, AppState>,
    target_fx_guid: String,
    prompt: String,
    preview_only: bool,
) -> Result<PreviewResult, String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| VaultError::PassphraseNotSet.to_string())?;
    let api_key = crate::vault::load_api_key(&app, &pass)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "api key not set".to_string())?;

    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-1.5-pro".to_string());
    let ToneResponse { reasoning, params } =
        gemini::generate_tone(&api_key, &model, ToneRequest { user_prompt: prompt })
            .await
            .map_err(|e| e.to_string())?;

    let params = cleaner::apply_replace_active_cleaner(MergeMode::ReplaceActive, params);

    let old = state
        .param_cache
        .lock()
        .map_err(|_| "cache lock poisoned")?
        .get(&target_fx_guid)
        .cloned()
        .unwrap_or_default();
    let d = diff_params(&old, &params);

    if !preview_only {
        let cmd = ClientCommand::SetTone {
            session_token: String::new(),
            command_id: format!("cmd-{}", chrono_nanos()),
            target_fx_guid: target_fx_guid.clone(),
            mode: MergeMode::ReplaceActive,
            params: params.clone(),
        };
        state
            .tx
            .send(UiCommand::SendToDll(cmd))
            .await
            .map_err(|_| "ws actor unavailable".to_string())?;

        state
            .param_cache
            .lock()
            .map_err(|_| "cache lock poisoned")?
            .insert(target_fx_guid, params.clone());
    }

    Ok(PreviewResult {
        reasoning,
        params,
        diff: d,
    })
}

#[tauri::command]
async fn apply_tone(
    state: State<'_, AppState>,
    target_fx_guid: String,
    mode: MergeMode,
    params: Vec<ParamChange>,
) -> Result<(), String> {
    let params = cleaner::apply_replace_active_cleaner(mode, params);
    let cmd = ClientCommand::SetTone {
        session_token: String::new(),
        command_id: format!("cmd-{}", chrono_nanos()),
        target_fx_guid: target_fx_guid.clone(),
        mode,
        params: params.clone(),
    };
    state
        .tx
        .send(UiCommand::SendToDll(cmd))
        .await
        .map_err(|_| "ws actor unavailable".to_string())?;
    state
        .param_cache
        .lock()
        .map_err(|_| "cache lock poisoned")?
        .insert(target_fx_guid, params);
    Ok(())
}

fn chrono_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

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
            app.handle().plugin(tauri_plugin_stronghold::Builder::with_argon2(&salt_path).build())?;

            let (tx, rx) = mpsc::channel(256);
            app.manage(AppState {
                tx,
                param_cache: Mutex::new(HashMap::new()),
                vault: Mutex::new(VaultState::default()),
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                ws_actor::run(rx, handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect_ws,
            disconnect_ws,
            set_vault_passphrase,
            has_api_key,
            save_api_key,
            clear_api_key,
            generate_tone,
            apply_tone
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
