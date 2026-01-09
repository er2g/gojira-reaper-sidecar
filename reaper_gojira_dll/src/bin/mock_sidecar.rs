use crossbeam_channel::bounded;
use reaper_gojira_dll::{MainLoop, NetworkThread, ReaperApi};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_ADDR: &str = "127.0.0.1:0";

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

    fn track_fx_param_name(&self, _track: usize, _fx_index: i32, param_index: i32) -> Option<String> {
        // Keep probe work small; we only "expose" a handful of indices.
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

fn parse_arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let addr = parse_arg_value(&args, "--addr")
        .or_else(|| std::env::var("GOJIRA_WS_ADDR").ok())
        .unwrap_or_else(|| DEFAULT_ADDR.to_string());

    let addr_file = parse_arg_value(&args, "--addr-file").map(PathBuf::from);
    let run_for_ms = parse_arg_value(&args, "--run-for-ms")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_millis);

    let (in_tx, in_rx) = bounded(reaper_gojira_dll::INBOUND_CAP);
    let (out_tx, out_rx) = bounded(reaper_gojira_dll::OUTBOUND_CAP);

    let net = match NetworkThread::spawn_with_addr(&addr, in_tx, out_rx) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if let Some(path) = &addr_file {
        let _ = fs::write(path, net.listen_addr().to_string());
    }

    println!("mock_sidecar listening on ws://{}", net.listen_addr());

    let api = MockReaperApi::new();
    let mut main_loop = MainLoop::new(in_rx, out_tx);

    let start = Instant::now();
    loop {
        main_loop.tick(&api);
        thread::sleep(Duration::from_millis(33));
        if let Some(max) = run_for_ms {
            if start.elapsed() >= max {
                break;
            }
        }
    }

    net.shutdown();
}
