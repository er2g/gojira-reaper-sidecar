use crossbeam_channel::bounded;
use gojira_protocol::{ClientCommand, ErrorCode, MergeMode, ParamChange, ServerMessage};
use reaper_gojira_dll::{MainLoop, NetworkThread, ReaperApi};
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tungstenite::Message;

struct MockReaperApi {
    params: Mutex<HashMap<i32, f32>>,
}

impl MockReaperApi {
    fn new() -> Self {
        Self {
            params: Mutex::new(HashMap::new()),
        }
    }
}

impl ReaperApi for MockReaperApi {
    fn project_state_change_count(&self) -> i32 {
        0
    }
    fn count_tracks(&self) -> i32 {
        1
    }
    fn get_track(&self, index: i32) -> Option<usize> {
        if index == 0 { Some(100) } else { None }
    }
    fn enum_project(&self, _index: i32) -> Option<(usize, String)> {
        None
    }
    fn current_project(&self) -> Option<(usize, String)> {
        Some((1, "mock_project.rpp".to_string()))
    }
    fn count_tracks_in(&self, project: usize) -> i32 {
        if project == 1 { 1 } else { 0 }
    }
    fn get_track_in(&self, project: usize, index: i32) -> Option<usize> {
        if project == 1 && index == 0 {
            Some(100)
        } else {
            None
        }
    }
    fn track_guid(&self, track: usize) -> Option<String> {
        if track == 100 {
            Some("{MOCK-TRACK-GUID}".to_string())
        } else {
            None
        }
    }
    fn track_name(&self, _track: usize) -> String {
        "Mock Track".to_string()
    }
    fn track_fx_count(&self, track: usize) -> i32 {
        if track == 100 { 1 } else { 0 }
    }
    fn track_fx_num_params(&self, _track: usize, _fx_index: i32) -> Option<i32> {
        Some(256)
    }
    fn track_fx_guid(&self, track: usize, fx_index: i32) -> Option<String> {
        if track == 100 && fx_index == 0 {
            Some("{MOCK-FX-GUID}".to_string())
        } else {
            None
        }
    }
    fn track_fx_name(&self, _track: usize, _fx_index: i32) -> String {
        "Neural DSP: Archetype Gojira (Mock)".to_string()
    }
    fn track_fx_param_name(
        &self,
        _track: usize,
        _fx_index: i32,
        param_index: i32,
    ) -> Option<String> {
        const KNOWN: &[i32] = &[
            0, 1, 2, 3, 4, 5, 29, 30, 84, 92, 99, 101, 105, 106, 108, 112, 113, 114, 115, 116,
            117,
        ];
        if KNOWN.contains(&param_index) {
            Some(format!("param_{param_index}"))
        } else {
            None
        }
    }
    fn track_fx_format_param_value(
        &self,
        _track: usize,
        _fx_index: i32,
        _param_index: i32,
        value: f32,
    ) -> Option<String> {
        Some(format!("{:.3}", value))
    }
    fn track_fx_get_param(&self, _track: usize, _fx_index: i32, param_index: i32) -> Option<f32> {
        let guard = self.params.lock().ok()?;
        guard.get(&param_index).copied()
    }
    fn track_fx_set_param(
        &self,
        _track: usize,
        _fx_index: i32,
        param_index: i32,
        value: f32,
    ) -> Result<(), String> {
        let Ok(mut guard) = self.params.lock() else {
            return Err("mock lock poisoned".to_string());
        };
        guard.insert(param_index, value);
        Ok(())
    }
}

fn read_server_message(
    ws: &mut tungstenite::WebSocket<TcpStream>,
    timeout: Duration,
) -> ServerMessage {
    let deadline = Instant::now() + timeout;
    loop {
        match ws.read() {
            Ok(Message::Text(s)) => return serde_json::from_str(&s).expect("valid server json"),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if Instant::now() >= deadline {
                    panic!("timeout waiting for server message");
                }
            }
            Err(e) => panic!("ws read failed: {e:?}"),
        }
    }
}

#[test]
fn ws_handshake_set_tone_and_unauthorized() {
    std::env::set_var("GOJIRA_SEND_PARAM_SAMPLES", "0");
    std::env::set_var("GOJIRA_SEND_VALIDATION_REPORT", "0");

    let (in_tx, in_rx) = bounded(reaper_gojira_dll::INBOUND_CAP);
    let (out_tx, out_rx) = bounded(reaper_gojira_dll::OUTBOUND_CAP);

    let net = NetworkThread::spawn_with_addr("127.0.0.1:0", in_tx, out_rx).expect("spawn net");
    let addr = net.listen_addr();

    let api = MockReaperApi::new();
    let mut main_loop = MainLoop::new(in_rx, out_tx);

    let stream = TcpStream::connect(addr).expect("tcp connect");
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let (mut ws, _) = tungstenite::client(format!("ws://{addr}"), stream).expect("ws connect");

    let deadline = Instant::now() + Duration::from_secs(2);
    let handshake = loop {
        main_loop.tick(&api);
        match ws.read() {
            Ok(Message::Text(s)) => break serde_json::from_str(&s).expect("valid server json"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("ws read failed: {e:?}"),
        }
        if Instant::now() >= deadline {
            panic!("timeout waiting for handshake");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (session_token, fx_guid) = match handshake {
        ServerMessage::Handshake {
            session_token,
            instances,
            ..
        } => {
            assert!(!instances.is_empty(), "mock scan should produce an instance");
            (session_token, instances[0].fx_guid.clone())
        }
        other => panic!("expected handshake, got: {other:?}"),
    };

    let cmd = ClientCommand::SetTone {
        session_token: session_token.clone(),
        command_id: "test-1".to_string(),
        target_fx_guid: fx_guid,
        mode: MergeMode::Merge,
        params: vec![ParamChange {
            index: 30,
            value: 0.42,
        }],
    };
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into()))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    let ack = loop {
        main_loop.tick(&api);
        match ws.read() {
            Ok(Message::Text(s)) => break serde_json::from_str(&s).expect("valid server json"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("ws read failed: {e:?}"),
        }
        if Instant::now() >= deadline {
            panic!("timeout waiting for ack");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    match ack {
        ServerMessage::Ack {
            command_id,
            applied_params,
        } => {
            assert_eq!(command_id, "test-1");
            assert_eq!(applied_params.len(), 1);
            assert_eq!(applied_params[0].index, 30);
            assert!((applied_params[0].requested - 0.42).abs() < 0.0001);
        }
        other => panic!("expected ack, got: {other:?}"),
    }

    let bad = ClientCommand::RefreshInstances {
        session_token: "WRONG".to_string(),
    };
    ws.send(Message::Text(serde_json::to_string(&bad).unwrap().into()))
        .unwrap();

    let err = read_server_message(&mut ws, Duration::from_secs(2));
    match err {
        ServerMessage::Error { code, .. } => assert!(matches!(code, ErrorCode::Unauthorized)),
        other => panic!("expected unauthorized error, got: {other:?}"),
    }

    net.shutdown();
}
