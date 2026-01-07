use crate::protocol::{ClientCommand, ErrorCode, InboundMsg, OutboundMsg, ServerMessage};
use crossbeam_channel::{Receiver, Sender};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use ws::{CloseCode, Handler, Handshake, Message, Result as WsResult, Sender as WsSender};

const WS_ADDR: &str = "127.0.0.1:9001";

#[derive(Clone)]
struct ActiveClient {
    out: WsSender,
    session_token: String,
    socket_addr: SocketAddr,
}

pub struct NetworkThread {
    server_sender: WsSender,
    shutdown: Arc<AtomicBool>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

impl NetworkThread {
    pub fn spawn(in_tx: Sender<InboundMsg>, out_rx: Receiver<OutboundMsg>) -> Result<Self, String> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let active: Arc<Mutex<Option<ActiveClient>>> = Arc::new(Mutex::new(None));

        let active_for_out = Arc::clone(&active);
        let shutdown_for_out = Arc::clone(&shutdown);
        let pump_handle =
            thread::spawn(move || outbound_pump(out_rx, active_for_out, shutdown_for_out));

        let active_for_server = Arc::clone(&active);
        let in_tx_for_server = in_tx.clone();
        let shutdown_for_server = Arc::clone(&shutdown);

        let (server_sender_tx, server_sender_rx) = std::sync::mpsc::channel();
        let server_handle = thread::spawn(move || {
            let server = ws::WebSocket::new(move |out| ServerConn {
                out,
                in_tx: in_tx_for_server.clone(),
                active: Arc::clone(&active_for_server),
                shutdown: Arc::clone(&shutdown_for_server),
            });

            let server = match server {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("ws server init failed: {e}");
                    return;
                }
            };
            let broadcaster = server.broadcaster();
            let _ = server_sender_tx.send(broadcaster.clone());

            if let Err(e) = server.listen(WS_ADDR) {
                eprintln!("ws listen failed: {e}");
            }
        });

        let server_sender = server_sender_rx
            .recv()
            .map_err(|_| "ws broadcaster unavailable".to_string())?;

        // Keep both threads alive under a single handle by joining them on drop.
        let join_handle = Some(thread::spawn(move || {
            let _ = server_handle.join();
            let _ = pump_handle.join();
        }));

        Ok(Self {
            server_sender,
            shutdown,
            join_handle: Mutex::new(join_handle),
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.server_sender.shutdown();
        if let Ok(mut h) = self.join_handle.lock() {
            if let Some(h) = h.take() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for NetworkThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn outbound_pump(
    out_rx: Receiver<OutboundMsg>,
    active: Arc<Mutex<Option<ActiveClient>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        let msg = match out_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let OutboundMsg::Send { msg } = msg;
        let out = active.lock().ok().and_then(|a| a.as_ref().map(|c| c.out.clone()));
        let Some(out) = out else { continue };

        let payload = match serde_json::to_string(&msg) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to serialize outbound msg: {e}");
                continue;
            }
        };
        let _ = out.send(payload);
    }
}

struct ServerConn {
    out: WsSender,
    in_tx: Sender<InboundMsg>,
    active: Arc<Mutex<Option<ActiveClient>>>,
    shutdown: Arc<AtomicBool>,
}

impl Handler for ServerConn {
    fn on_open(&mut self, shake: Handshake) -> WsResult<()> {
        if self.shutdown.load(Ordering::Relaxed) {
            let _ = self.out.close(CloseCode::Normal);
            return Ok(());
        }
        let socket_addr = shake
            .peer_addr
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));
        let session_token: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        if let Ok(mut guard) = self.active.lock() {
            if let Some(prev) = guard.take() {
                let _ = prev.out.close(CloseCode::Away);
            }
            *guard = Some(ActiveClient {
                out: self.out.clone(),
                session_token: session_token.clone(),
                socket_addr,
            });
        }

        if self
            .in_tx
            .try_send(InboundMsg::ClientConnected {
                socket_addr,
                session_token,
            })
            .is_err()
        {
            let payload = json_payload(&ServerMessage::Error {
                msg: "server busy".to_string(),
                code: ErrorCode::Busy,
            })?;
            let _ = self.out.send(payload);
            let _ = self.out.close(CloseCode::Again);
        }

        Ok(())
    }

    fn on_close(&mut self, _: CloseCode, _: &str) {
        if let Ok(mut guard) = self.active.lock() {
            if let Some(active) = guard.as_ref() {
                if active.out.token() == self.out.token() {
                    *guard = None;
                    let _ = self.in_tx.try_send(InboundMsg::ClientDisconnected);
                }
            }
        }
    }

    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        let socket_addr = self.peer_addr().ok();
        let text = msg.as_text()?;

        let cmd: ClientCommand = match serde_json::from_str(text) {
            Ok(c) => c,
            Err(_) => {
                return self.send_error("invalid json", ErrorCode::InvalidCommand);
            }
        };

        let active_token = self
            .active
            .lock()
            .ok()
            .and_then(|a| a.as_ref().map(|c| c.session_token.clone()));

        if active_token.as_deref() != Some(cmd.session_token()) {
            return self.send_error("unauthorized", ErrorCode::Unauthorized);
        }

        let push = self.in_tx.try_send(InboundMsg::Command { cmd: cmd.clone() });
        if push.is_err() {
            // Flood policy:
            // - SetTone => reject BUSY
            // - RefreshInstances => coalesce/drop
            if matches!(cmd, ClientCommand::RefreshInstances { .. }) {
                return Ok(());
            }
            let peer = socket_addr
                .map(|p| p.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            eprintln!("inbound channel full; reject from {peer}");
            return self.send_error("server busy", ErrorCode::Busy);
        }

        Ok(())
    }
}

impl ServerConn {
    fn peer_addr(&self) -> WsResult<SocketAddr> {
        let guard = self.active.lock().map_err(|_| ws::Error::new(ws::ErrorKind::Internal, "lock poisoned"))?;
        let Some(active) = guard.as_ref() else {
            return Err(ws::Error::new(ws::ErrorKind::Internal, "no active client"));
        };
        Ok(active.socket_addr)
    }

    fn send_error(&self, msg: &str, code: ErrorCode) -> WsResult<()> {
        let payload = json_payload(&ServerMessage::Error {
            msg: msg.to_string(),
            code,
        })?;
        self.out.send(payload)
    }
}

fn json_payload(msg: &ServerMessage) -> WsResult<String> {
    serde_json::to_string(msg)
        .map_err(|e| ws::Error::new(ws::ErrorKind::Internal, e.to_string()))
}
