use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod int_key_map {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer};
    use std::collections::HashMap;

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<HashMap<i32, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let raw: HashMap<String, V> = HashMap::deserialize(deserializer)?;
        let mut out: HashMap<i32, V> = HashMap::with_capacity(raw.len());
        for (k, v) in raw {
            let idx = k
                .parse::<i32>()
                .map_err(|e| D::Error::custom(format!("invalid param index key {k:?}: {e}")))?;
            out.insert(idx, v);
        }
        Ok(out)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamEnumOption {
    pub value: f32,
    pub label: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamFormatTriplet {
    pub min: String,
    pub mid: String,
    pub max: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamFormatSample {
    pub norm: f32,
    pub formatted: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Busy,
    TargetNotFound,
    InvalidValue,
    InvalidCommand,
    NotReady,
    InternalError,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ServerMessage {
    Handshake {
        session_token: String,
        instances: Vec<GojiraInstance>,
        validation_report: HashMap<String, String>,
        #[serde(default, deserialize_with = "int_key_map::deserialize")]
        param_enums: HashMap<i32, Vec<ParamEnumOption>>,
        #[serde(default, deserialize_with = "int_key_map::deserialize")]
        param_formats: HashMap<i32, ParamFormatTriplet>,
        #[serde(default, deserialize_with = "int_key_map::deserialize")]
        param_format_samples: HashMap<i32, Vec<ParamFormatSample>>,     
    },
    ProjectChanged,
    Ack {
        command_id: String,
        #[serde(default)]
        applied_params: Vec<AppliedParam>,
    },
    Error { msg: String, code: ErrorCode },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClientCommand {
    HandshakeAck { session_token: String },
    RefreshInstances { session_token: String },
    SetTone {
        session_token: String,
        command_id: String,
        target_fx_guid: String,
        mode: MergeMode,
        params: Vec<ParamChange>,
    },
}

impl ClientCommand {
    pub fn session_token(&self) -> &str {
        match self {
            ClientCommand::HandshakeAck { session_token } => session_token,
            ClientCommand::RefreshInstances { session_token } => session_token,
            ClientCommand::SetTone { session_token, .. } => session_token,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    Merge,
    ReplaceActive,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamChange {
    pub index: i32,
    pub value: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppliedParam {
    pub index: i32,
    pub requested: f32,
    pub applied: f32,
    #[serde(default)]
    pub formatted: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Low,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GojiraInstance {
    pub track_guid: String,
    pub track_name: String,
    pub fx_guid: String,
    pub fx_name: String,
    pub last_known_fx_index: i32,
    pub confidence: Confidence,
}

