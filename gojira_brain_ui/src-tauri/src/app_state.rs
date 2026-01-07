use gojira_protocol::ClientCommand;
use gojira_protocol::ParamChange;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc;

pub struct AppState {
    pub tx: mpsc::Sender<UiCommand>,
    pub param_cache: Mutex<HashMap<String, Vec<ParamChange>>>,
    pub vault: Mutex<VaultState>,
}

#[derive(Default)]
pub struct VaultState {
    pub passphrase: Option<String>,
}

pub enum UiCommand {
    Connect,
    Disconnect,
    SendToDll(ClientCommand),
}

