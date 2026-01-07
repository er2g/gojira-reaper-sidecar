use brain_core::protocol::{ClientCommand, ParamChange};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc;

pub struct AppState {
    pub tx: mpsc::Sender<UiCommand>,
    pub param_cache: Mutex<HashMap<String, Vec<ParamChange>>>,
    pub vault: Mutex<VaultState>,
    /// Index translation (canonical -> actual) for plugin version drift.
    pub index_remap: Mutex<HashMap<i32, i32>>,
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
