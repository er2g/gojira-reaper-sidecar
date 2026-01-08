use brain_core::cleaner::{apply_replace_active_cleaner, sanitize_params};
use brain_core::gemini::{generate_tone_auto as gemini_generate_tone, ToneRequest};
use brain_core::protocol::{
    ClientCommand, MergeMode, ParamChange, ParamEnumOption, ParamFormatSample, ParamFormatTriplet,
};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, State};

use crate::tauri_utils::app_state::{AppState, UiCommand};
use crate::tauri_utils::diff::{diff_params, DiffItem};
use crate::tauri_utils::vault;
use serde::Deserialize;

#[derive(Serialize, Clone)]
pub struct HandshakePayload {
    pub session_token: String,
    pub instances: Vec<brain_core::protocol::GojiraInstance>,
    pub validation_report: HashMap<String, String>,
    pub param_enums: HashMap<i32, Vec<ParamEnumOption>>,
    pub param_formats: HashMap<i32, ParamFormatTriplet>,
    pub param_format_samples: HashMap<i32, Vec<ParamFormatSample>>,
}

#[derive(Serialize)]
pub struct PreviewResult {
    pub reasoning: String,
    pub params: Vec<ParamChange>,
    pub diff: Vec<DiffItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRemapEntry {
    pub from: i32,
    pub to: i32,
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
pub fn get_index_remap(state: State<'_, AppState>) -> Result<HashMap<i32, i32>, String> {
    state
        .index_remap
        .lock()
        .map(|m| m.clone())
        .map_err(|_| "index remap lock poisoned".to_string())
}

#[tauri::command]
pub fn set_index_remap(state: State<'_, AppState>, entries: Vec<IndexRemapEntry>) -> Result<(), String> {
    let mut map = state
        .index_remap
        .lock()
        .map_err(|_| "index remap lock poisoned".to_string())?;
    map.clear();
    for e in entries {
        if e.from != e.to {
            map.insert(e.from, e.to);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn reset_index_remap(state: State<'_, AppState>) -> Result<(), String> {
    state
        .index_remap
        .lock()
        .map_err(|_| "index remap lock poisoned".to_string())?
        .clear();
    Ok(())
}

#[tauri::command]
pub async fn generate_tone(
    app: AppHandle,
    state: State<'_, AppState>,
    target_fx_guid: String,
    prompt: String,
    preview_only: bool,
) -> Result<PreviewResult, String> {
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro".to_string());

    let backend_env = std::env::var("GEMINI_BACKEND")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase());
    let vertex_model = model.contains("2.5") || model.starts_with("gemini-2");
    let skip_api_key = matches!(
        backend_env.as_deref(),
        Some("vertex")
            | Some("vertexai")
            | Some("vertex_ai")
            | Some("oauth")
            | Some("google-oauth")
            | Some("google_oauth")
            | Some("googleai-oauth")
    ) || (backend_env.is_none() && vertex_model);

    let api_key = if skip_api_key {
        None
    } else {
        let pass = state
            .vault
            .lock()
            .map_err(|_| "vault lock poisoned")?
            .passphrase
            .clone()
            .ok_or_else(|| "vault passphrase not set".to_string())?;
        Some(
            vault::load_api_key(&app, &pass)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "api key not set".to_string())?,
        )
    };

    let prompt = augment_prompt_with_param_meta(&state, &prompt);

    let tone = gemini_generate_tone(&model, ToneRequest { user_prompt: prompt }, api_key.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    let index_remap = state
        .index_remap
        .lock()
        .map_err(|_| "index remap lock poisoned".to_string())?
        .clone();

    let mut params = sanitize_params(tone.params).map_err(|e| e.to_string())?;
    params = apply_replace_active_cleaner(MergeMode::ReplaceActive, params);
    params = apply_index_remap(params, &index_remap);
    params = sanitize_params(params).map_err(|e| e.to_string())?;

    let old = state
        .param_cache
        .lock()
        .map_err(|_| "cache lock poisoned")?
        .get(&target_fx_guid)
        .cloned()
        .unwrap_or_default();
    let d = diff_params(&old, &params, &index_remap);

    if !preview_only {
        apply_tone_inner(&state, &target_fx_guid, MergeMode::ReplaceActive, params.clone()).await?;
    }

    Ok(PreviewResult {
        reasoning: tone.reasoning,
        params,
        diff: d,
    })
}

fn augment_prompt_with_param_meta(state: &AppState, prompt: &str) -> String {
    let enums = state
        .param_enums
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    let formats = state
        .param_formats
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    let samples = state
        .param_format_samples
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();

    if enums.is_empty() && formats.is_empty() && samples.is_empty() {
        return prompt.to_string();
    }

    // Keep this compact; model gets the full list but in a machine-friendly shape.
    let mut meta = String::new();
    meta.push_str("\n\nPLUGIN PARAM META (from the current REAPER instance):\n");

    // Only include the relevant cab/IR + a couple of mode selectors by default.
    let include_enum = [84, 92, 99, 113, 5];
    let mut enum_obj: HashMap<i32, Vec<(f32, String)>> = HashMap::new();
    for idx in include_enum {
        if let Some(opts) = enums.get(&idx) {
            let mapped: Vec<(f32, String)> = opts
                .iter()
                .map(|o| (o.value, o.label.clone()))
                .collect();
            enum_obj.insert(idx, mapped);
        }
    }

    let mut include_fmt: Vec<i32> = Vec::new();
    include_fmt.extend([0, 1, 2]); // input/output gain + gate
    include_fmt.extend(30..=51); // amp knobs
    include_fmt.extend(54..=82); // graphic EQ bands
    include_fmt.extend([87, 88, 89, 94, 95, 96]); // cab mic position/distance/level
    include_fmt.extend([105, 106, 108]); // delay
    include_fmt.extend([114, 115, 116, 117]); // reverb
    include_fmt.sort_unstable();
    include_fmt.dedup();

    let mut fmt_obj: HashMap<i32, (String, String, String)> = HashMap::new();
    for idx in include_fmt {
        if let Some(t) = formats.get(&idx) {
            fmt_obj.insert(idx, (t.min.clone(), t.mid.clone(), t.max.clone()));
        }
    }

    // JSON keeps token count lower than prose for large IR lists.
    if !enum_obj.is_empty() {
        if let Ok(j) = serde_json::to_string(&enum_obj) {
            meta.push_str("ENUM_OPTIONS_JSON=");
            meta.push_str(&j);
            meta.push('\n');
        }
    }
    if !fmt_obj.is_empty() {
        if let Ok(j) = serde_json::to_string(&fmt_obj) {
            meta.push_str("FORMATTED_VALUE_TRIPLETS_JSON=");
            meta.push_str(&j);
            meta.push('\n');
        }
    }

    // Optionally include formatted samples (norm->formatted) so the Rust resolver can convert
    // human units (like dB) into normalized 0..1 values without the model having to guess.
    // Keep this limited to the most tone-relevant parameters to avoid prompt bloat.
    if !samples.is_empty() {
        let mut sample_obj: HashMap<i32, Vec<(f32, String)>> = HashMap::new();
        let mut include: Vec<i32> = Vec::new();
        include.push(2); // Gate
        include.extend(54..=82); // Graphic EQ bands
        include.extend([29, 30, 31, 32, 33, 34, 35]); // Clean amp
        include.extend(36..=43); // Rust amp
        include.extend(44..=51); // Hot amp
        include.extend([101, 105, 106, 108, 112, 113, 114, 115, 116, 117]); // Time FX
        include.extend([83, 84, 85, 89, 92, 96, 99]); // Cab selectors (+ mic levels)

        include.sort_unstable();
        include.dedup();

        for idx in include {
            if let Some(v) = samples.get(&idx) {
                let mapped: Vec<(f32, String)> = v
                    .iter()
                    .map(|s| (s.norm, s.formatted.clone()))
                    .collect();
                if !mapped.is_empty() {
                    sample_obj.insert(idx, mapped);
                }
            }
        }

        if !sample_obj.is_empty() {
            if let Ok(j) = serde_json::to_string(&sample_obj) {
                meta.push_str("PARAM_FORMAT_SAMPLES_JSON=");
                meta.push_str(&j);
                meta.push('\n');
            }
        }
    }

    meta.push_str("Use these option labels when choosing Cab Type (84) and Mic IR (92/99). Set the parameter value close to the provided float for the desired label.\n");
    meta.push_str("For continuous cab mic controls (Position/Distance), the formatted triplets can hint at units/direction; use them to pick sensible normalized values.\n");
    meta.push_str("You may specify some values in human units (like dB) if PARAM_FORMAT_SAMPLES_JSON is present; the backend will translate them to 0..1.\n");

    // Hard cap to prevent runaway prompts if IR lists are enormous.
    const MAX_EXTRA_CHARS: usize = 25_000;
    if meta.len() > MAX_EXTRA_CHARS {
        meta.truncate(MAX_EXTRA_CHARS);
        meta.push_str("\n...(meta truncated)\n");
    }

    format!("{prompt}{meta}")
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
    let index_remap = state
        .index_remap
        .lock()
        .map_err(|_| "index remap lock poisoned".to_string())?
        .clone();

    let mut params = sanitize_params(params).map_err(|e| e.to_string())?;
    params = apply_replace_active_cleaner(mode, params);
    params = apply_index_remap(params, &index_remap);
    params = sanitize_params(params).map_err(|e| e.to_string())?;

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

fn apply_index_remap(params: Vec<ParamChange>, index_remap: &HashMap<i32, i32>) -> Vec<ParamChange> {
    if index_remap.is_empty() {
        return params;
    }
    params
        .into_iter()
        .map(|mut p| {
            if let Some(to) = index_remap.get(&p.index) {
                p.index = *to;
            }
            p
        })
        .collect()
}

fn chrono_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
