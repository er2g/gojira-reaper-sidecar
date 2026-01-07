mod cleaner;
mod calibration;
mod param_map;
mod protocol;
mod system_prompt;

use crate::cleaner::apply_replace_active_cleaner;
use crate::protocol::{ClientCommand, MergeMode, ParamChange, ServerMessage};
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target_fx_guid = args
        .iter()
        .position(|a| a == "--target-fx-guid")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let requested_params: Vec<ParamChange> = vec![
        ParamChange {
            index: param_map::selectors::AMP_TYPE_INDEX,
            value: 0.5,
        },
        ParamChange {
            index: param_map::global::NOISE_GATE,
            value: 0.85,
        },
    ];

    let mode = MergeMode::ReplaceActive;
    let final_params = apply_replace_active_cleaner(mode, requested_params);

    let state = Arc::new(Mutex::new(UiState::default()));
    ws::connect("ws://127.0.0.1:9001", |out| {
        Client {
            out,
            state: Arc::clone(&state),
            target_fx_guid: target_fx_guid.clone(),
            pending_params: final_params.clone(),
        }
    })
    .expect("ws connect failed");
}

#[derive(Default)]
struct UiState {
    session_token: Option<String>,
    instances: Vec<protocol::GojiraInstance>,
}

struct Client {
    out: ws::Sender,
    state: Arc<Mutex<UiState>>,
    target_fx_guid: Option<String>,
    pending_params: Vec<ParamChange>,
}

impl ws::Handler for Client {
    fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
        let text = msg.as_text()?;
        let server_msg: ServerMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("invalid server json: {e}\n{text}");
                return Ok(());
            }
        };

        match server_msg {
            ServerMessage::Handshake {
                session_token,
                instances,
                validation_report,
            } => {
                eprintln!("handshake: {} instance(s)", instances.len());
                for (k, v) in validation_report {
                    eprintln!("validate {k}: {v}");
                }
                if let Ok(mut st) = self.state.lock() {
                    st.session_token = Some(session_token.clone());
                    st.instances = instances;
                }
                let ack_payload = serde_json::to_string(&ClientCommand::HandshakeAck {
                    session_token: session_token.clone(),
                })
                .map_err(ws_json_err)?;
                self.out.send(ack_payload)?;

                let target = self
                    .target_fx_guid
                    .clone()
                    .or_else(|| {
                        self.state
                            .lock()
                            .ok()
                            .and_then(|s| s.instances.first().map(|i| i.fx_guid.clone()))
                    });

                let Some(target_fx_guid) = target else {
                    eprintln!("no target fx guid; pass --target-fx-guid or open a project with Gojira");
                    return Ok(());
                };

                let Some(session_token) = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|s| s.session_token.clone())
                else {
                    return Ok(());
                };
                let cmd = ClientCommand::SetTone {
                    session_token,
                    command_id: uuid4(),
                    target_fx_guid,
                    mode: MergeMode::ReplaceActive,
                    params: self.pending_params.clone(),
                };
                let payload = serde_json::to_string(&cmd).map_err(ws_json_err)?;
                self.out.send(payload)?;
            }
            ServerMessage::Ack { command_id } => {
                eprintln!("ack: {command_id}");
            }
            ServerMessage::ProjectChanged => {
                eprintln!("project changed");
            }
            ServerMessage::Error { msg, code } => {
                eprintln!("error: {code:?}: {msg}");
            }
        }

        Ok(())
    }
}

fn ws_json_err(e: serde_json::Error) -> ws::Error {
    ws::Error::new(ws::ErrorKind::Internal, e.to_string())
}

fn uuid4() -> String {
    // Minimal non-crypto UUID-like id without pulling extra deps for now.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("cmd-{nanos}")
}
