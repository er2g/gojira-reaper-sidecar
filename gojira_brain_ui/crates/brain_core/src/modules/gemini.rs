use crate::modules::cleaner::{apply_replace_active_cleaner, sanitize_params};
use crate::modules::protocol::MergeMode;
use crate::modules::protocol::ParamChange;
use crate::modules::system_prompt::SYSTEM_PROMPT;
use crate::modules::value_resolver::{resolve_ai_params, AiToneResponse};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;
use std::time::Duration;
use thiserror::Error;

const RESEARCH_PROMPT: &str = r#"You are an expert guitar tone researcher and tone designer.
Write a careful, practical tone brief for the user's request (band/era/style).

Output format (plain text, concise, 10-18 bullets total):
1) Tone DNA: key adjectives + what makes it recognizable
2) Gain & dynamics: how tight/loose, how much saturation, how pick attack should feel
3) EQ regions: low end / low-mids / high-mids / presence / fizz (use regions, not exact Hz numbers)
4) Space & modulation: chorus/delay/reverb expectations for the era (if any)
5) Cab/mic vibe: general IR/mic flavor, stereo/width if relevant
6) Pitfalls: what would make it sound wrong
7) Translation notes: how to approximate these ideas using OD boost + gate + amp EQ + graphic EQ + chorus/delay/reverb + cab choices (no plugin indices here)
"#;

#[derive(Debug, Clone, Serialize)]
pub struct ToneRequest {
    pub user_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToneResponse {
    pub reasoning: String,
    pub params: Vec<ParamChange>,
}

#[derive(Debug, Error)]
pub enum GeminiError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("gemini request failed: status={status} body={body}")]
    BadStatus { status: StatusCode, body: String },
    #[error("gemini auth error: {0}")]
    Auth(String),
    #[error("gemini response parse failed: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiBackend {
    AiStudioApiKey,
    GoogleAiOauth,
    VertexAi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TonePipeline {
    SingleStage,
    TwoStage,
}

pub fn decide_backend(_model: &str, api_key_present: bool) -> GeminiBackend {
    let env = std::env::var("GEMINI_BACKEND")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase());

    match env.as_deref() {
        Some("vertex") | Some("vertexai") | Some("vertex_ai") => return GeminiBackend::VertexAi,
        Some("oauth") | Some("google-oauth") | Some("google_oauth") | Some("googleai-oauth") => {
            return GeminiBackend::GoogleAiOauth
        }
        Some("ai") | Some("aistudio") | Some("ai-studio") | Some("api_key") | Some("apikey") => {
            return GeminiBackend::AiStudioApiKey
        }
        Some("auto") | None => {}
        Some(other) => {
            eprintln!("warning: unknown GEMINI_BACKEND={other:?}, falling back to auto");
        }
    }

    // Auto mode: if an API key is available, prefer AI Studio API-key auth regardless of model.
    // (Some environments expose Gemini 2.x models via AI Studio; users can override via GEMINI_BACKEND.)
    if api_key_present {
        return GeminiBackend::AiStudioApiKey;
    }

    let has_vertex_project = std::env::var("VERTEX_PROJECT")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_PROJECT"))
        .or_else(|_| std::env::var("GCLOUD_PROJECT"))
        .is_ok();
    if has_vertex_project {
        GeminiBackend::VertexAi
    } else {
        GeminiBackend::GoogleAiOauth
    }
}

fn decide_pipeline() -> TonePipeline {
    let env = std::env::var("TONE_PIPELINE")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "two_stage".to_string());

    match env.as_str() {
        "1" | "single" | "single_stage" | "one" | "one_stage" => TonePipeline::SingleStage,
        "2" | "two" | "two_stage" | "dual" | "dual_stage" => TonePipeline::TwoStage,
        other => {
            eprintln!("warning: unknown TONE_PIPELINE={other:?}, defaulting to two_stage");
            TonePipeline::TwoStage
        }
    }
}

fn research_model_for(main_model: &str) -> String {
    if let Ok(m) = std::env::var("TONE_RESEARCH_MODEL") {
        let m = m.trim().to_string();
        if !m.is_empty() {
            return m;
        }
    }

    // Keep stage-1 responsive; default to Flash when the main model is Pro.
    let m = main_model.trim();
    if m.contains("2.5-pro") {
        "gemini-2.5-flash".to_string()
    } else {
        m.to_string()
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push_str("\n…(truncated)\n");
    out
}

fn http_timeout_for_model(model: &str) -> Duration {
    let env = std::env::var("GEMINI_HTTP_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());

    let default_secs = if model.to_ascii_lowercase().contains("pro") {
        120
    } else {
        60
    };

    let secs = env.unwrap_or(default_secs).clamp(15, 300);
    Duration::from_secs(secs)
}

#[derive(Debug, Clone, Deserialize)]
struct EnumOption {
    value: f32,
    label: String,
}

fn extract_prompt_json_line<'a>(prompt: &'a str, key: &str) -> Option<&'a str> {
    for line in prompt.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix(key) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn extract_enum_options(prompt: &str) -> Option<std::collections::HashMap<i32, Vec<EnumOption>>> {
    let raw = extract_prompt_json_line(prompt, "ENUM_OPTIONS_JSON=")?;
    let parsed: std::collections::HashMap<String, Vec<EnumOption>> =
        serde_json::from_str(raw).ok()?;

    let mut out: std::collections::HashMap<i32, Vec<EnumOption>> = std::collections::HashMap::new();
    for (k, v) in parsed {
        if let Ok(idx) = k.parse::<i32>() {
            out.insert(idx, v);
        }
    }
    Some(out)
}

fn upsert_param(params: &mut Vec<ParamChange>, index: i32, value: f32) {
    if let Some(p) = params.iter_mut().find(|p| p.index == index) {
        p.value = value;
        return;
    }
    params.push(ParamChange { index, value });
}

fn get_param(params: &[ParamChange], index: i32) -> Option<f32> {
    params.iter().find(|p| p.index == index).map(|p| p.value)
}

fn apply_prompt_autofixes(prompt: &str, params: &mut Vec<ParamChange>) {
    let plow = prompt.to_ascii_lowercase();
    let enums = match extract_enum_options(prompt) {
        Some(e) => e,
        None => return,
    };

    // If the user asked for shimmer and reverb is on, ensure we set REV Mode (113) to Shimmer.
    // (The model sometimes forgets to set 113 even when using reverb.)
    if plow.contains("shimmer") {
        let reverb_on = get_param(params, 112).unwrap_or(0.0) >= 0.5;
        if reverb_on {
            if let Some(opts) = enums.get(&113) {
                if let Some(sh) = opts
                    .iter()
                    .find(|o| o.label.trim().eq_ignore_ascii_case("shimmer"))
                {
                    upsert_param(params, 113, sh.value);
                }
            }
        }
    }
}

fn derive_plan(params: &[ParamChange]) -> String {
    use std::collections::BTreeMap;

    let mut m: BTreeMap<i32, f32> = BTreeMap::new();
    for p in params {
        m.insert(p.index, p.value);
    }

    let mut lines: Vec<String> = Vec::new();

    let amp_v = m.get(&29).copied();
    enum AmpSel {
        Clean,
        Rust,
        Hot,
        Other(f32),
        Unset,
    }
    let amp_sel = match amp_v {
        Some(v) if (v - 0.0).abs() < 0.2 => AmpSel::Clean,
        Some(v) if (v - 0.5).abs() < 0.2 => AmpSel::Rust,
        Some(v) if (v - 1.0).abs() < 0.2 => AmpSel::Hot,
        Some(v) => AmpSel::Other(v),
        None => AmpSel::Unset,
    };
    let amp_label = match amp_sel {
        AmpSel::Clean => "Clean".to_string(),
        AmpSel::Rust => "Rust".to_string(),
        AmpSel::Hot => "Hot".to_string(),
        AmpSel::Other(v) => format!("Custom({v:.3})"),
        AmpSel::Unset => "Unset".to_string(),
    };

    let amp_line = match amp_sel {
        AmpSel::Clean => {
            let parts = [
                (30, "Gain"),
                (31, "Bright"),
                (32, "Bass"),
                (33, "Mid"),
                (34, "Treble"),
                (35, "Level"),
            ];
            let mut s = String::from("Amp: Clean (29)");
            for (idx, name) in parts {
                if let Some(v) = m.get(&idx) {
                    s.push_str(&format!(", {name} {idx}={v:.2}"));
                }
            }
            s
        }
        AmpSel::Rust => {
            let parts = [
                (36, "Gain"),
                (37, "Low"),
                (38, "Mid"),
                (39, "High"),
                (40, "Master"),
                (41, "Presence"),
                (42, "Depth"),
                (43, "Level"),
            ];
            let mut s = String::from("Amp: Rust (29)");
            for (idx, name) in parts {
                if let Some(v) = m.get(&idx) {
                    s.push_str(&format!(", {name} {idx}={v:.2}"));
                }
            }
            s
        }
        AmpSel::Hot => {
            let parts = [
                (44, "Gain"),
                (45, "Low"),
                (46, "Mid"),
                (47, "High"),
                (48, "Master"),
                (49, "Presence"),
                (50, "Depth"),
                (51, "Level"),
            ];
            let mut s = String::from("Amp: Hot (29)");
            for (idx, name) in parts {
                if let Some(v) = m.get(&idx) {
                    s.push_str(&format!(", {name} {idx}={v:.2}"));
                }
            }
            s
        }
        AmpSel::Other(_) | AmpSel::Unset => format!("Amp: {amp_label} (29)"),
    };
    lines.push(format!("- {amp_line}"));

    if let Some(v) = m.get(&2) {
        lines.push(format!("- Gate: Amount 2={v:.2}"));
    }

    // Pedals (skip if it's only an auto-added inactive toggle)
    let od_active = m.get(&13).copied();
    let od_has_params = [14, 15, 16].iter().any(|i| m.contains_key(i));
    if od_active.is_some_and(|v| v >= 0.5) || od_has_params {
        let v = od_active.unwrap_or(0.0);
        let mut s = format!("- OD: Active 13={v:.0}");
        for (idx, name) in [(14, "Drive"), (15, "Tone"), (16, "Level")] {
            if let Some(v) = m.get(&idx) {
                s.push_str(&format!(", {name} {idx}={v:.2}"));
            }
        }
        lines.push(s);
    }
    let drt_active = m.get(&17).copied();
    let drt_has_params = [18, 19, 20].iter().any(|i| m.contains_key(i));
    if drt_active.is_some_and(|v| v >= 0.5) || drt_has_params {
        let v = drt_active.unwrap_or(0.0);
        let mut s = format!("- DRT: Active 17={v:.0}");
        for (idx, name) in [(18, "Dist"), (19, "Filter"), (20, "Vol")] {
            if let Some(v) = m.get(&idx) {
                s.push_str(&format!(", {name} {idx}={v:.2}"));
            }
        }
        lines.push(s);
    }
    let chr_active = m.get(&23).copied();
    let chr_has_params = [24, 25, 26, 27].iter().any(|i| m.contains_key(i));
    if chr_active.is_some_and(|v| v >= 0.5) || chr_has_params {
        let v = chr_active.unwrap_or(0.0);
        let mut s = format!("- Chorus: Active 23={v:.0}");
        for (idx, name) in [(24, "Rate"), (25, "Depth"), (26, "Feedback"), (27, "Mix")] {
            if let Some(v) = m.get(&idx) {
                s.push_str(&format!(", {name} {idx}={v:.2}"));
            }
        }
        lines.push(s);
    }

    // EQ
    let has_any_eq = (52..=82).any(|i| m.contains_key(&i)) && (53..=82).any(|i| m.contains_key(&i));
    if has_any_eq {
        if let Some(v) = m.get(&52) {
            lines.push(format!("- EQ Section: Active 52={v:.0}"));
        } else {
            lines.push("- EQ Section: (52 not set)".to_string());
        }

        let eq_kind = match amp_sel {
            AmpSel::Clean => Some((53, 54..=62, "Clean EQ")),
            AmpSel::Rust => Some((63, 64..=72, "Rust EQ")),
            AmpSel::Hot => Some((73, 74..=82, "Hot EQ")),
            _ => {
                if (63..=72).any(|i| m.contains_key(&i)) {
                    Some((63, 64..=72, "Rust EQ"))
                } else if (73..=82).any(|i| m.contains_key(&i)) {
                    Some((73, 74..=82, "Hot EQ"))
                } else if (53..=62).any(|i| m.contains_key(&i)) {
                    Some((53, 54..=62, "Clean EQ"))
                } else {
                    None
                }
            }
        };

        if let Some((eq_active, band_range, eq_name)) = eq_kind {
            if let Some(v) = m.get(&eq_active).copied() {
                let mut s = format!("- {eq_name}: Active {eq_active}={v:.0}");
                let band_start = *band_range.start();
                let mut deltas: Vec<(i32, f32)> = band_range
                    .clone()
                    .filter_map(|i| m.get(&i).copied().map(|v| (i, v - 0.5)))
                    .collect();
                deltas.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
                deltas.truncate(5);
                if !deltas.is_empty() {
                    s.push_str("; top moves:");
                    for (idx, d) in deltas {
                        let sign = if d >= 0.0 { "+" } else { "" };
                        let band_no = idx - band_start + 1;
                        s.push_str(&format!(" B{band_no}({idx}) {sign}{d:.2}"));
                    }
                }
                lines.push(s);
            }
        }
    }

    // Cab
    let cab_active = m.get(&83).copied();
    let cab_has_params = (84..=99).any(|i| m.contains_key(&i));
    if cab_active.is_some_and(|v| v >= 0.5) || cab_has_params {
        let v = cab_active.unwrap_or(0.0);
        let mut s = format!("- Cab: Active 83={v:.0}");
        if let Some(v) = m.get(&86) {
            s.push_str(&format!(", Cab1 86={v:.0}"));
        }
        if let Some(v) = m.get(&90) {
            s.push_str(&format!(", Pan1 90={v:.2}"));
        }
        if let Some(v) = m.get(&93) {
            s.push_str(&format!(", Cab2 93={v:.0}"));
        }
        if let Some(v) = m.get(&97) {
            s.push_str(&format!(", Pan2 97={v:.2}"));
        }
        lines.push(s);
    }

    // Time FX
    let dly_active = m.get(&101).copied();
    let dly_has_params = [105, 106, 108].iter().any(|i| m.contains_key(i));
    if dly_active.is_some_and(|v| v >= 0.5) || dly_has_params {
        let v = dly_active.unwrap_or(0.0);
        let mut s = format!("- Delay: Active 101={v:.0}");
        for (idx, name) in [(105, "Mix"), (106, "Feedback"), (108, "Tempo")] {
            if let Some(v) = m.get(&idx) {
                s.push_str(&format!(", {name} {idx}={v:.2}"));
            }
        }
        lines.push(s);
    }
    let rev_active = m.get(&112).copied();
    let rev_has_params = [114, 115, 116, 117].iter().any(|i| m.contains_key(i));
    if rev_active.is_some_and(|v| v >= 0.5) || rev_has_params {
        let v = rev_active.unwrap_or(0.0);
        let mut s = format!("- Reverb: Active 112={v:.0}");
        for (idx, name) in [(114, "Mix"), (115, "Time"), (116, "LowCut"), (117, "HighCut")] {
            if let Some(v) = m.get(&idx) {
                s.push_str(&format!(", {name} {idx}={v:.2}"));
            }
        }
        lines.push(s);
    }

    lines.join("\n")
}

pub async fn generate_tone_auto(
    model: &str,
    req: ToneRequest,
    api_key: Option<&str>,
) -> Result<ToneResponse, GeminiError> {
    if decide_pipeline() == TonePipeline::TwoStage {
        let research_model = research_model_for(model);
        let research = generate_research_auto(&research_model, &req.user_prompt, api_key).await;

        let (combined_prompt, research_for_reasoning) = match research {
            Ok(text) => {
                let max_chars = std::env::var("TONE_RESEARCH_MAX_CHARS")
                    .ok()
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(1500);
                let trimmed = text.trim();
                let brief = truncate_chars(trimmed, max_chars);
                (
                    format!(
                        "{}\n\n---\nTONE RESEARCH BRIEF:\n{}\n---\nNow translate this into the Archetype Gojira parameters using the indices and rules in the system prompt.\nIn your reasoning, include a short \"Plan\" section (3-7 bullets) that explicitly maps the brief into concrete module choices (amp + EQ + cab + time FX), and reference key indices you set.",
                        req.user_prompt, brief.trim()
                    ),
                    Some(brief),
                )
            }
            Err(e) => {
                eprintln!("warning: research stage failed, continuing single-stage: {e}");
                (req.user_prompt.clone(), None)
            }
        };

        let mut out =
            generate_tone_single_stage(model, ToneRequest { user_prompt: combined_prompt }, api_key)
                .await?;

        apply_prompt_autofixes(&req.user_prompt, &mut out.params);

        // Build a plan off the same post-processing the UI/CLI will apply.
        let sanitized = sanitize_params(out.params.clone()).map_err(GeminiError::Parse)?;
        let cleaned_for_plan = apply_replace_active_cleaner(MergeMode::ReplaceActive, sanitized);
        let plan = derive_plan(&cleaned_for_plan);
        out.reasoning = if let Some(brief) = research_for_reasoning {
            format!(
                "Research brief (stage 1):\n{}\n\nPlan (derived from params):\n{}\n\n{}",
                brief.trim(),
                plan,
                out.reasoning
            )
        } else {
            format!("Plan (derived from params):\n{}\n\n{}", plan, out.reasoning)
        };
        return Ok(out);
    }

    let mut out = generate_tone_single_stage(model, req.clone(), api_key).await?;
    apply_prompt_autofixes(&req.user_prompt, &mut out.params);
    Ok(out)
}

async fn generate_tone_single_stage(
    model: &str,
    req: ToneRequest,
    api_key: Option<&str>,
) -> Result<ToneResponse, GeminiError> {
    match decide_backend(model, api_key.is_some()) {
        GeminiBackend::AiStudioApiKey => {
            let api_key =
                api_key.ok_or_else(|| GeminiError::Auth("missing GEMINI_API_KEY".to_string()))?;
            match generate_tone_aistudio(api_key, model, req.clone()).await {
                Ok(ok) => Ok(ok),
                Err(GeminiError::Auth(msg))
                    if msg.to_ascii_lowercase().contains("oauth2 is required") =>
                {
                    generate_tone_google_oauth(model, req).await
                }
                Err(GeminiError::BadStatus { status, body })
                    if status == StatusCode::UNAUTHORIZED
                        && body.to_ascii_lowercase().contains("api keys are not supported") =>
                {
                    generate_tone_google_oauth(model, req).await
                }
                Err(e) => Err(e),
            }
        }
        GeminiBackend::GoogleAiOauth => {
            match generate_tone_google_oauth(model, req.clone()).await {
                Ok(ok) => Ok(ok),
                Err(GeminiError::BadStatus { status, body })
                    if status == StatusCode::FORBIDDEN
                        && body
                            .to_ascii_lowercase()
                            .contains("insufficient authentication scopes") =>
                {
                    // If the token doesn't have Generative Language API scopes, try Vertex (which
                    // typically works with cloud-platform scoped tokens) when configured.
                    if std::env::var("VERTEX_PROJECT").is_ok()
                        || std::env::var("GOOGLE_CLOUD_PROJECT").is_ok()
                        || std::env::var("GCLOUD_PROJECT").is_ok()
                    {
                        generate_tone_vertex(model, req).await
                    } else {
                        Err(GeminiError::BadStatus {
                            status: StatusCode::FORBIDDEN,
                            body,
                        })
                    }
                }
                Err(e) => Err(e),
            }
        }
        GeminiBackend::VertexAi => generate_tone_vertex(model, req).await,
    }
}

async fn generate_research_auto(
    model: &str,
    user_prompt: &str,
    api_key: Option<&str>,
) -> Result<String, GeminiError> {
    let full_prompt = format!("{RESEARCH_PROMPT}\n\nUSER:\n{user_prompt}");
    match decide_backend(model, api_key.is_some()) {
        GeminiBackend::AiStudioApiKey => {
            let api_key =
                api_key.ok_or_else(|| GeminiError::Auth("missing GEMINI_API_KEY".to_string()))?;
            match generate_text_aistudio(api_key, model, &full_prompt).await {
                Ok(ok) => Ok(ok),
                Err(GeminiError::Auth(msg))
                    if msg.to_ascii_lowercase().contains("oauth2 is required") =>
                {
                    generate_text_google_oauth(model, &full_prompt).await
                }
                Err(e) => Err(e),
            }
        }
        GeminiBackend::GoogleAiOauth => generate_text_google_oauth(model, &full_prompt).await,
        GeminiBackend::VertexAi => generate_text_vertex(model, &full_prompt).await,
    }
}

pub async fn generate_tone_aistudio(
    api_key: &str,
    model: &str,
    req: ToneRequest,
) -> Result<ToneResponse, GeminiError> {
    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let full_prompt = format!("{SYSTEM_PROMPT}\n\nUSER:\n{}", req.user_prompt);

    let payload_with_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": {
                "type": "OBJECT",
                "properties": {
                    "reasoning": { "type": "STRING" },
                    "params": {
                        "type": "ARRAY",
                        "items": {
                            "type": "OBJECT",
                            "properties": {
                                "index": { "type": "INTEGER" },
                                "value": { "type": "STRING" }
                            },
                            "required": ["index", "value"]
                        }
                    }
                },
                "required": ["reasoning", "params"]
            }
        }
    });

    let payload_no_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let mut backoff = Duration::from_millis(500);
    for attempt in 1..=3 {
        let resp = client
            .post(&url)
            .json(if attempt == 1 {
                &payload_with_schema
            } else {
                &payload_no_schema
            })
            .send()
            .await?;
        if resp.status().is_success() {
            let body = resp.text().await?;
            return parse_tone_response(&body, &req.user_prompt).map_err(GeminiError::Parse);
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status == StatusCode::UNAUTHORIZED
            && body
                .to_ascii_lowercase()
                .contains("api keys are not supported by this api")
        {
            return Err(GeminiError::Auth(
                "AI Studio API key auth was rejected by the Generative Language API; OAuth2 is required in this environment"
                    .to_string(),
            ));
        }

        // Some endpoints reject schema fields; retry once without schema.      
        if attempt == 1
            && status == StatusCode::BAD_REQUEST
            && body.to_ascii_lowercase().contains("unknown")
        {
            continue;
        }

        let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
        if !retryable || attempt == 3 {
            return Err(GeminiError::BadStatus { status, body });
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }

    Err(GeminiError::Parse("exhausted retries".to_string()))
}

async fn generate_text_aistudio(
    api_key: &str,
    model: &str,
    full_prompt: &str,
) -> Result<String, GeminiError> {
    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let payload = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ]
    });

    let mut backoff = Duration::from_millis(500);
    for attempt in 1..=3 {
        let resp = client.post(&url).json(&payload).send().await?;
        if resp.status().is_success() {
            let body = resp.text().await?;
            return extract_candidate_text(&body).map_err(GeminiError::Parse);
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status == StatusCode::UNAUTHORIZED
            && body
                .to_ascii_lowercase()
                .contains("api keys are not supported by this api")
        {
            return Err(GeminiError::Auth(
                "AI Studio API key auth was rejected by the Generative Language API; OAuth2 is required in this environment"
                    .to_string(),
            ));
        }
        let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
        if !retryable || attempt == 3 {
            return Err(GeminiError::BadStatus { status, body });
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }

    Err(GeminiError::Parse("exhausted retries".to_string()))
}

async fn generate_tone_google_oauth(
    model: &str,
    req: ToneRequest,
) -> Result<ToneResponse, GeminiError> {
    let access_token = std::env::var("GEMINI_ACCESS_TOKEN")
        .or_else(|_| std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN"))
        .unwrap_or_else(|_| String::new());

    let access_token = if !access_token.trim().is_empty() {
        access_token
    } else {
        gcloud_print_access_token()?
    };

    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );

    let full_prompt = format!("{SYSTEM_PROMPT}\n\nUSER:\n{}", req.user_prompt);

    let payload_with_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": {
                "type": "OBJECT",
                "properties": {
                    "reasoning": { "type": "STRING" },
                    "params": {
                        "type": "ARRAY",
                        "items": {
                            "type": "OBJECT",
                            "properties": {
                                "index": { "type": "INTEGER" },
                                "value": { "type": "STRING" }
                            },
                            "required": ["index", "value"]
                        }
                    }
                },
                "required": ["reasoning", "params"]
            }
        }
    });

    let payload_no_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ]
    });

    let mut backoff = Duration::from_millis(500);
    for attempt in 1..=3 {
        let resp = client
            .post(&url)
            .bearer_auth(&access_token)
            .json(if attempt == 1 {
                &payload_with_schema
            } else {
                &payload_no_schema
            })
            .send()
            .await?;
        if resp.status().is_success() {
            let body = resp.text().await?;
            return parse_tone_response(&body, &req.user_prompt).map_err(GeminiError::Parse);
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        // Retry once without schema if field name differs for this endpoint/version.
        if attempt == 1
            && status == StatusCode::BAD_REQUEST
            && body.to_ascii_lowercase().contains("unknown")
        {
            continue;
        }

        let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
        if !retryable || attempt == 3 {
            return Err(GeminiError::BadStatus { status, body });
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }

    Err(GeminiError::Parse("exhausted retries".to_string()))
}

async fn generate_text_google_oauth(model: &str, full_prompt: &str) -> Result<String, GeminiError> {
    let access_token = std::env::var("GEMINI_ACCESS_TOKEN")
        .or_else(|_| std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN"))
        .unwrap_or_else(|_| String::new());

    let access_token = if !access_token.trim().is_empty() {
        access_token
    } else {
        gcloud_print_access_token()?
    };

    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );

    let payload = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ]
    });

    let mut backoff = Duration::from_millis(500);
    for attempt in 1..=3 {
        let resp = client
            .post(&url)
            .bearer_auth(&access_token)
            .json(&payload)
            .send()
            .await?;
        if resp.status().is_success() {
            let body = resp.text().await?;
            return extract_candidate_text(&body).map_err(GeminiError::Parse);
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
        if !retryable || attempt == 3 {
            return Err(GeminiError::BadStatus { status, body });
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }

    Err(GeminiError::Parse("exhausted retries".to_string()))
}

async fn generate_tone_vertex(model: &str, req: ToneRequest) -> Result<ToneResponse, GeminiError> {
    let project = std::env::var("VERTEX_PROJECT")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_PROJECT"))
        .or_else(|_| std::env::var("GCLOUD_PROJECT"))
        .map_err(|_| {
            GeminiError::Auth(
                "missing VERTEX_PROJECT/GOOGLE_CLOUD_PROJECT (required for Vertex AI)".to_string(),
            )
        })?;

    let location = std::env::var("VERTEX_LOCATION")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_LOCATION"))
        .unwrap_or_else(|_| "us-central1".to_string());

    let access_token = std::env::var("VERTEX_ACCESS_TOKEN")
        .or_else(|_| std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN"))
        .unwrap_or_else(|_| String::new());

    let access_token = if !access_token.trim().is_empty() {
        access_token
    } else {
        gcloud_print_access_token()?
    };

    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let full_prompt = format!("{SYSTEM_PROMPT}\n\nUSER:\n{}", req.user_prompt);

    let payload_with_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "OBJECT",
                "properties": {
                    "reasoning": { "type": "STRING" },
                    "params": {
                        "type": "ARRAY",
                        "items": {
                            "type": "OBJECT",
                            "properties": {
                                "index": { "type": "INTEGER" },
                                "value": { "type": "STRING" }
                            },
                            "required": ["index", "value"]
                        }
                    }
                },
                "required": ["reasoning", "params"]
            }
        }
    });

    let payload_no_schema = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ]
    });

    let models_to_try = vertex_model_candidates(model);
    let mut last_err: Option<GeminiError> = None;

    for candidate_model in models_to_try {
        let url = format!(
            "https://{loc}-aiplatform.googleapis.com/v1/projects/{proj}/locations/{loc}/publishers/google/models/{model}:generateContent",
            loc = location,
            proj = project,
            model = candidate_model
        );

        let mut backoff = Duration::from_millis(500);
        for attempt in 1..=3 {
            let resp = client
                .post(&url)
                .bearer_auth(&access_token)
                .json(if attempt == 1 {
                    &payload_with_schema
                } else {
                    &payload_no_schema
                })
                .send()
                .await?;

            if resp.status().is_success() {
                let body = resp.text().await?;
                return parse_tone_response(&body, &req.user_prompt).map_err(GeminiError::Parse);
            }

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            // Retry once without schema if field name differs for this endpoint/version.
            if attempt == 1
                && status == StatusCode::BAD_REQUEST
                && body.to_ascii_lowercase().contains("unknown")
            {
                continue;
            }

            // If the model alias isn't valid (or not available in this project/region), try the
            // next candidate model name.
            if status == StatusCode::NOT_FOUND
                && body
                    .to_ascii_lowercase()
                    .contains("publisher model")
            {
                last_err = Some(GeminiError::BadStatus { status, body });
                break;
            }

            let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if !retryable || attempt == 3 {
                return Err(GeminiError::BadStatus { status, body });
            }

            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    }

    Err(last_err.unwrap_or_else(|| {
        GeminiError::Parse("exhausted retries".to_string())
    }))
}

async fn generate_text_vertex(model: &str, full_prompt: &str) -> Result<String, GeminiError> {
    let project = std::env::var("VERTEX_PROJECT")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_PROJECT"))
        .or_else(|_| std::env::var("GCLOUD_PROJECT"))
        .map_err(|_| {
            GeminiError::Auth(
                "missing VERTEX_PROJECT/GOOGLE_CLOUD_PROJECT (required for Vertex AI)".to_string(),
            )
        })?;

    let location = std::env::var("VERTEX_LOCATION")
        .or_else(|_| std::env::var("GOOGLE_CLOUD_LOCATION"))
        .unwrap_or_else(|_| "us-central1".to_string());

    let access_token = std::env::var("VERTEX_ACCESS_TOKEN")
        .or_else(|_| std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN"))
        .unwrap_or_else(|_| String::new());

    let access_token = if !access_token.trim().is_empty() {
        access_token
    } else {
        gcloud_print_access_token()?
    };

    let client = reqwest::Client::builder()
        .timeout(http_timeout_for_model(model))
        .build()?;

    let payload = json!({
        "contents": [
            { "role": "user", "parts": [ { "text": full_prompt } ] }
        ]
    });

    let models_to_try = vertex_model_candidates(model);
    let mut last_err: Option<GeminiError> = None;

    for candidate_model in models_to_try {
        let url = format!(
            "https://{loc}-aiplatform.googleapis.com/v1/projects/{proj}/locations/{loc}/publishers/google/models/{model}:generateContent",
            loc = location,
            proj = project,
            model = candidate_model
        );

        let mut backoff = Duration::from_millis(500);
        for attempt in 1..=3 {
            let resp = client
                .post(&url)
                .bearer_auth(&access_token)
                .json(&payload)
                .send()
                .await?;

            if resp.status().is_success() {
                let body = resp.text().await?;
                return extract_candidate_text(&body).map_err(GeminiError::Parse);
            }

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            // If the model alias isn't valid (or not available in this project/region), try the
            // next candidate model name.
            if status == StatusCode::NOT_FOUND
                && body
                    .to_ascii_lowercase()
                    .contains("publisher model")
            {
                last_err = Some(GeminiError::BadStatus { status, body });
                break;
            }

            let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if !retryable || attempt == 3 {
                return Err(GeminiError::BadStatus { status, body });
            }

            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    }

    Err(last_err.unwrap_or_else(|| GeminiError::Parse("exhausted retries".to_string())))
}

fn gcloud_print_access_token() -> Result<String, GeminiError> {
    fn run(args: &[&str]) -> std::io::Result<std::process::Output> {
        if cfg!(windows) {
            let mut cmd_args: Vec<&str> = vec!["/C", "gcloud"];
            cmd_args.extend_from_slice(args);
            Command::new("cmd").args(cmd_args).output()
        } else {
            Command::new("gcloud").args(args).output()
        }
    }

    // Prefer ADC tokens (they can be minted with explicit scopes via:
    // `gcloud auth application-default login --scopes=...`).
    let candidates: [&[&str]; 2] = [
        &["auth", "application-default", "print-access-token"],
        &["auth", "print-access-token"],
    ];

    let mut last_err: Option<String> = None;
    for args in candidates {
        let out = run(args).map_err(|e| GeminiError::Auth(format!("failed to run gcloud: {e}")))?;
        if out.status.success() {
            let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if token.is_empty() {
                last_err = Some("gcloud returned empty access token".to_string());
                continue;
            }
            return Ok(token);
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let msg = stderr.trim();
        last_err = Some(if msg.is_empty() {
            format!("gcloud {} failed: {}", args.join(" "), out.status)
        } else {
            format!("gcloud {} failed: {msg}", args.join(" "))
        });
    }

    Err(GeminiError::Auth(
        last_err.unwrap_or_else(|| "failed to obtain access token via gcloud".to_string()),
    ))
}

fn vertex_model_candidates(model: &str) -> Vec<String> {
    let m = model.trim();
    if m.is_empty() {
        return vec![];
    }

    // If the caller already provided an explicit version, don't guess.
    if m.contains('@') || m.ends_with("-001") || m.ends_with("-002") || m.ends_with("-003") {
        return vec![m.to_string()];
    }

    // Vertex AI often exposes versioned names (e.g. gemini-1.5-flash-002 or @002).
    vec![
        m.to_string(),
        format!("{m}-002"),
        format!("{m}-001"),
        format!("{m}@002"),
        format!("{m}@001"),
    ]
}

fn parse_tone_response(body: &str, original_prompt: &str) -> Result<ToneResponse, String> {
    let text = extract_candidate_text(body)?;

    // If Gemini respects structured output, `text` should be valid JSON.
    let extracted = extract_json_like(&text).unwrap_or(text.as_str());

    if let Ok(path) = std::env::var("DUMP_AI_JSON_PATH") {
        let path = path.trim();
        if !path.is_empty() {
            // Best-effort debugging hook (avoid failing the request if the file
            // can't be written).
            let mut dump = extracted.as_bytes();
            const MAX: usize = 250_000;
            if dump.len() > MAX {
                dump = &dump[..MAX];
            }
            let _ = std::fs::write(path, dump);
        }
    }

    let parsed = serde_json::from_str::<AiToneResponse>(extracted)
        .or_else(|_| serde_json::from_str::<AiToneResponse>(body))
        .map_err(|e| format!("{e}: {extracted}"))?;

    let resolved = resolve_ai_params(original_prompt, parsed.params)
        .map_err(|e| e.to_string())?;

    Ok(ToneResponse {
        reasoning: parsed.reasoning,
        params: resolved,
    })
}

fn extract_candidate_text(body: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Envelope {
        candidates: Option<Vec<Candidate>>,
    }
    #[derive(Deserialize)]
    struct Candidate {
        content: Option<Content>,
    }
    #[derive(Deserialize)]
    struct Content {
        parts: Option<Vec<Part>>,
    }
    #[derive(Deserialize)]
    struct Part {
        text: Option<String>,
    }

    let env: Envelope = serde_json::from_str(body).map_err(|e| format!("{e}: {body}"))?;
    env.candidates
        .and_then(|mut c| c.pop())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|mut p| p.pop())
        .and_then(|p| p.text)
        .ok_or_else(|| format!("missing candidates.content.parts.text: {body}"))
}

fn extract_json_like(text: &str) -> Option<&str> {
    let t = text.trim();
    if t.starts_with('{') && t.ends_with('}') {
        return Some(t);
    }

    if let Some(stripped) = t.strip_prefix("```") {
        // Common model output: ```json\n{...}\n```
        let stripped = stripped.trim_start();
        let stripped = stripped.strip_prefix("json").unwrap_or(stripped).trim_start();
        let stripped = stripped.trim_start_matches(|c| c == '\r' || c == '\n').trim();
        let stripped = stripped.strip_suffix("```").unwrap_or(stripped).trim();
        if stripped.starts_with('{') && stripped.ends_with('}') {
            return Some(stripped);
        }
    }

    let start = t.find('{')?;
    let end = t.rfind('}')?;
    if end > start {
        Some(&t[start..=end])
    } else {
        None
    }
}
