use crate::app_state::UiCommand;
use futures_util::{SinkExt, StreamExt};
use gojira_protocol::{ClientCommand, ServerMessage};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

const WS_URL: &str = "ws://127.0.0.1:9001";

#[derive(Serialize)]
struct StatusEvent {
    status: &'static str,
    retry_in: Option<u64>,
}

pub async fn run(mut rx: mpsc::Receiver<UiCommand>, app: AppHandle) {
    let mut desired_connected = true;
    let mut backoff = Backoff::default();

    let mut ws: Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> = None;
    let mut session_token: Option<String> = None;

    emit_status(&app, "connecting", None);

    loop {
        if !desired_connected {
            ws = None;
            session_token = None;
            emit_status(&app, "disconnected", None);
            match rx.recv().await {
                Some(UiCommand::Connect) => desired_connected = true,
                Some(UiCommand::Disconnect) => {}
                Some(UiCommand::SendToDll(_)) => {}
                None => return,
            }
            continue;
        }

        if ws.is_none() {
            emit_status(&app, "connecting", None);
            match tokio_tungstenite::connect_async(WS_URL).await {
                Ok((socket, _)) => {
                    ws = Some(socket);
                    session_token = None;
                    backoff.reset();
                    emit_status(&app, "connected", None);
                }
                Err(_) => {
                    let retry = backoff.next_delay();
                    emit_status(&app, "disconnected", Some(retry.as_secs()));
                    tokio::time::sleep(retry).await;
                    continue;
                }
            }
        }

        let Some(mut socket) = ws.take() else { continue };

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(UiCommand::Connect) => desired_connected = true,
                        Some(UiCommand::Disconnect) => { desired_connected = false; break; }
                        Some(UiCommand::SendToDll(cmd)) => {
                            if let Some(token) = session_token.as_deref() {
                                let msg = inject_token(cmd, token);
                                if send_json(&mut socket, &msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                        None => return,
                    }
                }
                incoming = socket.next() => {
                    match incoming {
                        Some(Ok(msg)) => {
                            if let Ok(text) = msg.into_text() {
                                if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                                    match server_msg.clone() {
                                        ServerMessage::Handshake { session_token: t, instances, .. } => {
                                            session_token = Some(t.clone());
                                            let _ = app.emit("reaper://handshake", instances);
                                            let _ = send_json(&mut socket, &ClientCommand::HandshakeAck { session_token: t }).await;
                                        }
                                        ServerMessage::ProjectChanged => {
                                            let _ = app.emit("reaper://project_changed", ());
                                        }
                                        ServerMessage::Ack { .. } => {
                                            let _ = app.emit("reaper://ack", server_msg);
                                        }
                                        ServerMessage::Error { .. } => {
                                            let _ = app.emit("reaper://error", server_msg);
                                        }
                                    }
                                }
                            }
                        }
                        _ => break,
                    }
                }
            }
        }

        ws = None;
        session_token = None;
    }
}

fn emit_status(app: &AppHandle, status: &'static str, retry_in: Option<u64>) {
    let _ = app.emit("reaper://status", StatusEvent { status, retry_in });
}

fn inject_token(cmd: ClientCommand, token: &str) -> ClientCommand {
    match cmd {
        ClientCommand::HandshakeAck { .. } => ClientCommand::HandshakeAck {
            session_token: token.to_string(),
        },
        ClientCommand::RefreshInstances { .. } => ClientCommand::RefreshInstances {
            session_token: token.to_string(),
        },
        ClientCommand::SetTone {
            session_token: _,
            command_id,
            target_fx_guid,
            mode,
            params,
        } => ClientCommand::SetTone {
            session_token: token.to_string(),
            command_id,
            target_fx_guid,
            mode,
            params,
        },
    }
}

async fn send_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    msg: &ClientCommand,
) -> Result<(), ()> {
    let payload = serde_json::to_string(msg).map_err(|_| ())?;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(payload))
        .await
        .map_err(|_| ())
}

#[derive(Default)]
struct Backoff {
    idx: usize,
}

impl Backoff {
    fn reset(&mut self) {
        self.idx = 0;
    }

    fn next_delay(&mut self) -> Duration {
        let delays = [1, 2, 5, 10];
        let secs = delays.get(self.idx).copied().unwrap_or(10);
        self.idx = (self.idx + 1).min(delays.len());
        Duration::from_secs(secs)
    }
}
