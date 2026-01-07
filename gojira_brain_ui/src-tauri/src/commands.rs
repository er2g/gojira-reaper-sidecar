use brain_core::cleaner::{apply_replace_active_cleaner, sanitize_params};
use brain_core::gemini::{generate_tone, ToneRequest};
use brain_core::protocol::{ClientCommand, MergeMode, ParamChange};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, State};

use crate::tauri_utils::app_state::{AppState, UiCommand};
use crate::tauri_utils::diff::{diff_params, DiffItem};
use crate::tauri_utils::vault;

#[derive(Serialize)]
pub struct HandshakePayload {
    pub session_token: String,
    pub instances: Vec<brain_core::protocol::GojiraInstance>,
    pub validation_report: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct PreviewResult {
    pub reasoning: String,
    pub params: Vec<ParamChange>,
    pub diff: Vec<DiffItem>,
}

#[tauri::command]
pub async fn connect_ws(state: State<'_, AppState>) -> Result<(), String> {
    state
        .tx
        .send(UiCommand::Connect)
        .await
        .map_err(|_| "ws actor unavailable".to_string())
}

#[tauri::command]
pub async fn disconnect_ws(state: State<'_, AppState>) -> Result<(), String> {
    state
        .tx
        .send(UiCommand::Disconnect)
        .await
        .map_err(|_| "ws actor unavailable".to_string())
}

#[tauri::command]
pub fn set_vault_passphrase(state: State<'_, AppState>, passphrase: String) -> Result<(), String> {
    let mut guard = state.vault.lock().map_err(|_| "vault lock poisoned")?;
    guard.passphrase = Some(passphrase);
    Ok(())
}

#[tauri::command]
pub fn has_api_key(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| "vault passphrase not set".to_string())?;
    Ok(vault::load_api_key(&app, &pass)
        .map_err(|e| e.to_string())?
        .is_some())
}

#[tauri::command]
pub fn save_api_key(app: AppHandle, state: State<'_, AppState>, api_key: String) -> Result<(), String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| "vault passphrase not set".to_string())?;
    vault::save_api_key(&app, &pass, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_api_key(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let pass = state
        .vault
        .lock()
        .map_err(|_| "vault lock poisoned")?
        .passphrase
        .clone()
        .ok_or_else(|| "vault passphrase not set".to_string())?;
    vault::clear_api_key(&app, &pass).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn generate_tone(
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
        .ok_or_else(|| "vault passphrase not set".to_string())?;
    let api_key = vault::load_api_key(&app, &pass)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "api key not set".to_string())?;

    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-1.5-pro".to_string());
    let tone = generate_tone(
        &api_key,
        &model,
        ToneRequest {
            user_prompt: prompt,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut params = sanitize_params(tone.params).map_err(|e| e.to_string())?;
    params = apply_replace_active_cleaner(MergeMode::ReplaceActive, params);

    let old = state
        .param_cache
        .lock()
        .map_err(|_| "cache lock poisoned")?
        .get(&target_fx_guid)
        .cloned()
        .unwrap_or_default();
    let d = diff_params(&old, &params);

    if !preview_only {
        apply_tone_inner(&state, &target_fx_guid, MergeMode::ReplaceActive, params.clone()).await?;
    }

    Ok(PreviewResult {
        reasoning: tone.reasoning,
        params,
        diff: d,
    })
}

#[tauri::command]
pub async fn apply_tone(
    state: State<'_, AppState>,
    target_fx_guid: String,
    mode: MergeMode,
    params: Vec<ParamChange>,
) -> Result<(), String> {
    apply_tone_inner(&state, &target_fx_guid, mode, params).await
}

async fn apply_tone_inner(
    state: &AppState,
    target_fx_guid: &str,
    mode: MergeMode,
    params: Vec<ParamChange>,
) -> Result<(), String> {
    let mut params = sanitize_params(params).map_err(|e| e.to_string())?;
    params = apply_replace_active_cleaner(mode, params);

    let cmd = ClientCommand::SetTone {
        session_token: String::new(),
        command_id: format!("cmd-{}", chrono_nanos()),
        target_fx_guid: target_fx_guid.to_string(),
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
        .insert(target_fx_guid.to_string(), params);

    Ok(())
}

fn chrono_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

