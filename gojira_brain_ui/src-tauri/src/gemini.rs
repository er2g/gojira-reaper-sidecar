use crate::system_prompt::SYSTEM_PROMPT;
use gojira_protocol::ParamChange;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct ToneRequest {
    pub user_prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[error("gemini response parse failed: {0}")]
    Parse(String),
}

pub async fn generate_tone(api_key: &str, model: &str, req: ToneRequest) -> Result<ToneResponse, GeminiError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let prompt = format!("{SYSTEM_PROMPT}\n\nUSER:\n{}", req.user_prompt);

    let payload = json!({
        "contents": [
            { "parts": [ { "text": prompt } ] }
        ],
        "generationConfig": {
            "response_mime_type": "application/json",
            "response_schema": {
                "type": "OBJECT",
                "properties": {
                    "reasoning": { "type": "STRING" },
                    "params": {
                        "type": "ARRAY",
                        "items": {
                            "type": "OBJECT",
                            "properties": {
                                "index": { "type": "INTEGER" },
                                "value": { "type": "NUMBER" }
                            },
                            "required": ["index", "value"]
                        }
                    }
                },
                "required": ["reasoning", "params"]
            }
        }
    });

    let mut backoff = Duration::from_millis(500);
    for attempt in 1..=3 {
        let resp = client.post(&url).json(&payload).send().await?;
        if resp.status().is_success() {
            let body = resp.text().await?;
            return parse_tone_response(&body).map_err(GeminiError::Parse);
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

fn parse_tone_response(body: &str) -> Result<ToneResponse, String> {
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
    let text = env
        .candidates
        .and_then(|mut c| c.pop())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|mut p| p.pop())
        .and_then(|p| p.text)
        .ok_or_else(|| format!("missing candidates.content.parts.text: {body}"))?;

    // If Gemini respects response_schema, `text` should be valid JSON.
    serde_json::from_str::<ToneResponse>(&text)
        .or_else(|_| serde_json::from_str::<ToneResponse>(body))
        .map_err(|e| format!("{e}: {text}"))
}

