use brain_core::protocol::{ClientCommand, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::VecDeque;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::commands::HandshakePayload;
use crate::tauri_utils::app_state::UiCommand;
use tauri::Manager;

const WS_URL: &str = "ws://127.0.0.1:9001";

#[derive(Serialize, Clone)]
struct StatusEvent {
    status: &'static str,
    retry_in: Option<u64>,
}

pub async fn run(mut rx: mpsc::Receiver<UiCommand>, app: AppHandle) {
    let mut desired_connected = true;
    let mut backoff = Backoff::default();
    let mut backlog: VecDeque<UiCommand> = VecDeque::new();

    emit_status(&app, "connecting", None);

    loop {
        if !desired_connected {
            emit_status(&app, "disconnected", None);
            match recv_or_backlog(&mut rx, &mut backlog).await {
                Some(UiCommand::Connect) => desired_connected = true,
                Some(UiCommand::Disconnect) => {}
                Some(UiCommand::SendToDll(_)) => {}
                None => return,
            }
            continue;
        }

        emit_status(&app, "connecting", None);
        let socket = match tokio_tungstenite::connect_async(WS_URL).await {
            Ok((socket, _)) => {
                backoff.reset();
                emit_status(&app, "connected", None);
                socket
            }
            Err(_) => {
                let retry = backoff.next_delay();
                emit_status(&app, "disconnected", Some(retry.as_secs()));
                tokio::time::sleep(retry).await;
                continue;
            }
        };

        let (mut write, mut read) = socket.split();
        let mut session_token: Option<String> = None;
        let mut pending_set_tone: Option<ClientCommand> = None;

        'conn: loop {
            tokio::select! {
                next = recv_or_backlog(&mut rx, &mut backlog) => {
                    let Some(cmd) = next else { return };
                    match cmd {
                        UiCommand::Connect => {}
                        UiCommand::Disconnect => { desired_connected = false; break 'conn; }
                        UiCommand::SendToDll(cmd) => {
                            let cmd = coalesce_last_set_tone(cmd, &mut rx, &mut backlog);
                            match cmd {
                                Coalesced::Other(cmd) => {
                                    if send_to_dll(&mut write, &session_token, cmd).await.is_err() {
                                        break 'conn;
                                    }
                                }
                                Coalesced::LastSetTone(cmd) => {
                                    if session_token.is_some() {
                                        if send_to_dll(&mut write, &session_token, cmd).await.is_err() {
                                            break 'conn;
                                        }
                                    } else {
                                        pending_set_tone = Some(cmd);
                                    }
                                }
                            }
                        }
                    }
                }
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(msg)) => {
                            let Ok(text) = msg.into_text() else { continue };
                            let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) else { continue };
                            match server_msg {
                                ServerMessage::Handshake { session_token: t, instances, validation_report, param_enums, param_formats, param_format_samples } => {
                                    session_token = Some(t.clone());

                                    // Keep a copy in backend state so we can inject it into AI prompts.
                                    if let Some(state) = app.try_state::<crate::tauri_utils::app_state::AppState>() {
                                        if let Ok(mut g) = state.param_enums.lock() {
                                            *g = param_enums.clone();
                                        }
                                        if let Ok(mut g) = state.param_formats.lock() {
                                            *g = param_formats.clone();
                                        }
                                        if let Ok(mut g) = state.param_format_samples.lock() {
                                            *g = param_format_samples.clone();
                                        }
                                    }

                                    let _ = app.emit("reaper://handshake", HandshakePayload {
                                        session_token: t.clone(),
                                        instances,
                                        validation_report,
                                        param_enums,
                                        param_formats,
                                        param_format_samples,
                                    });
                                    let _ = send_raw(&mut write, &ClientCommand::HandshakeAck { session_token: t }).await;
                                    if let Some(pending) = pending_set_tone.take() {
                                        let _ = send_to_dll(&mut write, &session_token, pending).await;
                                    }
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
                        _ => break 'conn,
                    }
                }
            }
        }
    }
}

enum Coalesced {
    Other(ClientCommand),
    LastSetTone(ClientCommand),
}

fn coalesce_last_set_tone(
    first: ClientCommand,
    rx: &mut mpsc::Receiver<UiCommand>,
    backlog: &mut VecDeque<UiCommand>,
) -> Coalesced {
    let mut last_set_tone = if matches!(first, ClientCommand::SetTone { .. }) {
        Some(first)
    } else {
        return Coalesced::Other(first);
    };

    while let Ok(next) = rx.try_recv() {
        match next {
            UiCommand::SendToDll(cmd) if matches!(cmd, ClientCommand::SetTone { .. }) => {
                last_set_tone = Some(cmd);
            }
            other => {
                backlog.push_back(other);
                break;
            }
        }
    }

    Coalesced::LastSetTone(last_set_tone.expect("set tone must exist"))
}

async fn send_to_dll(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
    session_token: &Option<String>,
    cmd: ClientCommand,
) -> Result<(), ()> {
    let Some(token) = session_token.as_deref() else {
        return Ok(());
    };

    let cmd = inject_token(cmd, token);
    send_raw(write, &cmd).await
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

async fn send_raw(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
    cmd: &ClientCommand,
) -> Result<(), ()> {
    let payload = serde_json::to_string(cmd).map_err(|_| ())?;
    write
        .send(tokio_tungstenite::tungstenite::Message::Text(payload.into()))
        .await
        .map_err(|_| ())
}

async fn recv_or_backlog(
    rx: &mut mpsc::Receiver<UiCommand>,
    backlog: &mut VecDeque<UiCommand>,
) -> Option<UiCommand> {
    if let Some(cmd) = backlog.pop_front() {
        return Some(cmd);
    }
    rx.recv().await
}

fn emit_status(app: &AppHandle, status: &'static str, retry_in: Option<u64>) {
    let _ = app.emit("reaper://status", StatusEvent { status, retry_in });
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

