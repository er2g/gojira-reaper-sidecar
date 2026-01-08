use crate::protocol::{ClientCommand, ErrorCode, InboundMsg, OutboundMsg, ServerMessage};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tungstenite::protocol::Message;

const WS_ADDR: &str = "127.0.0.1:9001";

struct ActiveClient {
    ws: tungstenite::WebSocket<TcpStream>,
    session_token: String,
    socket_addr: SocketAddr,
}

pub struct NetworkThread {
    shutdown: Arc<AtomicBool>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

impl NetworkThread {
    pub fn spawn(in_tx: Sender<InboundMsg>, out_rx: Receiver<OutboundMsg>) -> Result<Self, String> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = Arc::clone(&shutdown);

        let join_handle = thread::spawn(move || run_server(in_tx, out_rx, shutdown_for_thread));

        Ok(Self {
            shutdown,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
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

fn run_server(in_tx: Sender<InboundMsg>, out_rx: Receiver<OutboundMsg>, shutdown: Arc<AtomicBool>) {
    let listener = match TcpListener::bind(WS_ADDR) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ws bind failed on {WS_ADDR}: {e}");
            return;
        }
    };
    let _ = listener.set_nonblocking(true);

    let mut active: Option<ActiveClient> = None;

    while !shutdown.load(Ordering::Relaxed) {
        // Accept new connections (single-client policy).
        loop {
            match listener.accept() {
                Ok((stream, socket_addr)) => {
                    let _ = stream.set_nodelay(true);
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(30)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));

                    let ws = match tungstenite::accept(stream) {
                        Ok(ws) => ws,
                        Err(e) => {
                            eprintln!("ws handshake failed: {e}");
                            continue;
                        }
                    };

                    let session_token: String = thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(32)
                        .map(char::from)
                        .collect();

                    // Close previous active client.
                    if let Some(mut prev) = active.take() {
                        let _ = prev.ws.close(None);
                        let _ = in_tx.try_send(InboundMsg::ClientDisconnected);
                    }

                    // Notify main loop.
                    if in_tx
                        .try_send(InboundMsg::ClientConnected {
                            socket_addr,
                            session_token: session_token.clone(),
                        })
                        .is_err()
                    {
                        // Busy: try to tell the client then drop the socket.
                        let mut ws = ws;
                        let _ = send_server_message(
                            &mut ws,
                            &ServerMessage::Error {
                                msg: "server busy".to_string(),
                                code: ErrorCode::Busy,
                            },
                        );
                        let _ = ws.close(None);
                        continue;
                    }

                    active = Some(ActiveClient {
                        ws,
                        session_token,
                        socket_addr,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("ws accept failed: {e}");
                    break;
                }
            }
        }

        // Outbound: drain queued messages.
        if let Some(client) = active.as_mut() {
            loop {
                match out_rx.try_recv() {
                    Ok(OutboundMsg::Send { msg }) => {
                        if send_server_message(&mut client.ws, &msg).is_err() {
                            let _ = client.ws.close(None);
                            active = None;
                            let _ = in_tx.try_send(InboundMsg::ClientDisconnected);
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        }

        // Inbound: read at most one message per loop (timeouts keep the loop moving).
        if let Some(client) = active.as_mut() {
            match client.ws.read() {
                Ok(msg) => {
                    if handle_inbound(&in_tx, client, msg).is_err() {
                        let _ = client.ws.close(None);
                        active = None;
                        let _ = in_tx.try_send(InboundMsg::ClientDisconnected);
                    }
                }
                Err(tungstenite::Error::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(tungstenite::Error::ConnectionClosed) => {
                    active = None;
                    let _ = in_tx.try_send(InboundMsg::ClientDisconnected);
                }
                Err(_) => {
                    active = None;
                    let _ = in_tx.try_send(InboundMsg::ClientDisconnected);
                }
            }
        } else {
            // If no active client, avoid busy-looping.
            thread::sleep(Duration::from_millis(25));
        }
    }

    if let Some(mut client) = active {
        let _ = client.ws.close(None);
    }
}

fn handle_inbound(
    in_tx: &Sender<InboundMsg>,
    client: &mut ActiveClient,
    msg: Message,
) -> Result<(), ()> {
    let text = match msg {
        Message::Text(s) => s,
        Message::Binary(_) => return Ok(()),
        Message::Ping(payload) => {
            let _ = client.ws.send(Message::Pong(payload));
            return Ok(());
        }
        Message::Pong(_) => return Ok(()),
        Message::Close(_) => return Err(()),
        Message::Frame(_) => return Ok(()),
    };

    let cmd: ClientCommand = match serde_json::from_str(&text) {
        Ok(c) => c,
        Err(_) => {
            let _ = send_server_message(
                &mut client.ws,
                &ServerMessage::Error {
                    msg: "invalid json".to_string(),
                    code: ErrorCode::InvalidCommand,
                },
            );
            return Ok(());
        }
    };

    if client.session_token != cmd.session_token() {
        let _ = send_server_message(
            &mut client.ws,
            &ServerMessage::Error {
                msg: "unauthorized".to_string(),
                code: ErrorCode::Unauthorized,
            },
        );
        return Ok(());
    }

    if in_tx.try_send(InboundMsg::Command { cmd: cmd.clone() }).is_err() {
        if matches!(cmd, ClientCommand::RefreshInstances { .. }) {
            return Ok(());
        }
        let _ = send_server_message(
            &mut client.ws,
            &ServerMessage::Error {
                msg: "server busy".to_string(),
                code: ErrorCode::Busy,
            },
        );
    }

    Ok(())
}

fn send_server_message(
    ws: &mut tungstenite::WebSocket<TcpStream>,
    msg: &ServerMessage,
) -> Result<(), ()> {
    let payload = serde_json::to_string(msg).map_err(|_| ())?;
    ws.send(Message::Text(payload.into())).map_err(|_| ())
}

