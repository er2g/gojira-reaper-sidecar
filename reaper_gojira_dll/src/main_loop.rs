use crate::protocol::{
    ClientCommand, ErrorCode, InboundMsg, MergeMode, OutboundMsg, ParamChange, ServerMessage,
};
use crate::reaper_api::ReaperApi;
use crate::resolver::{self, FxLookup};
use crate::validator;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const PROJECT_CHANGED_DEBOUNCE: Duration = Duration::from_millis(500);
const MAX_PARAM_INDEX: i32 = 4096;

pub struct MainLoop {
    inbound_rx: Receiver<InboundMsg>,
    outbound_tx: Sender<OutboundMsg>,
    cache: GojiraCache,

    active_session_token: Option<String>,
    validation_ready: bool,
    last_validation_report: HashMap<String, String>,
}

pub struct GojiraCache {
    pub lookup: FxLookup,

    pub last_project_change_count: i32,
    pub last_broadcast_time: Instant,
    pub last_track_count: i32,
    pub last_total_fx_count: i32,
}

impl MainLoop {
    pub fn new(inbound_rx: Receiver<InboundMsg>, outbound_tx: Sender<OutboundMsg>) -> Self {
        Self {
            inbound_rx,
            outbound_tx,
            cache: GojiraCache {
                lookup: HashMap::new(),
                last_project_change_count: 0,
                last_broadcast_time: Instant::now().checked_sub(PROJECT_CHANGED_DEBOUNCE).unwrap_or_else(Instant::now),
                last_track_count: -1,
                last_total_fx_count: -1,
            },
            active_session_token: None,
            validation_ready: false,
            last_validation_report: HashMap::new(),
        }
    }

    pub fn tick(&mut self, api: &dyn ReaperApi) {
        let mut connected_token: Option<String> = None;
        let mut refresh_instances = false;
        let mut last_set_tone: Option<ClientCommand> = None;

        loop {
            match self.inbound_rx.try_recv() {
                Ok(msg) => match msg {
                    InboundMsg::ClientConnected {
                        session_token, ..
                    } => {
                        connected_token = Some(session_token);
                    }
                    InboundMsg::ClientDisconnected => {
                        self.active_session_token = None;
                        self.validation_ready = false;
                        self.cache.lookup.clear();
                    }
                    InboundMsg::Command { cmd } => match cmd {
                        ClientCommand::RefreshInstances { .. } => refresh_instances = true,
                        ClientCommand::SetTone { .. } => last_set_tone = Some(cmd),
                        ClientCommand::HandshakeAck { .. } => {}
                    },
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if let Some(token) = connected_token {
            self.active_session_token = Some(token.clone());
            self.validation_ready = false;
            self.last_validation_report.clear();
            self.refresh_and_handshake(api, &token);
        } else if refresh_instances {
            if let Some(token) = self.active_session_token.clone() {
                self.refresh_and_handshake(api, &token);
            }
        }

        self.watchdog(api);

        if let Some(cmd) = last_set_tone {
            self.apply_set_tone(api, cmd);
        }
    }

    pub fn try_send(&mut self, msg: OutboundMsg) {
        let _ = self.outbound_tx.try_send(msg);
    }

    fn refresh_and_handshake(&mut self, api: &dyn ReaperApi, session_token: &str) {
        let (instances, lookup) = resolver::scan_project_instances(api);
        self.cache.lookup = lookup;

        let mut validation_report = HashMap::new();
        let mut param_enums = HashMap::new();
        let mut param_formats = HashMap::new();
        if let Some(first) = instances.first() {
            if let Ok((track, fx_index)) =
                resolver::resolve_fx(api, &mut self.cache.lookup, &first.fx_guid)
            {
                validation_report = validator::validate_parameter_map(api, track, fx_index);
                let (enums, formats) = validator::probe_param_meta(api, track, fx_index);
                param_enums = enums;
                param_formats = formats;
            }
        }
        self.last_validation_report = validation_report.clone();
        self.validation_ready = !validation_report.is_empty();

        self.send(ServerMessage::Handshake {
            session_token: session_token.to_string(),
            instances,
            validation_report,
            param_enums,
            param_formats,
        });
    }

    fn watchdog(&mut self, api: &dyn ReaperApi) {
        let state = api.project_state_change_count();
        if state == self.cache.last_project_change_count {
            return;
        }
        self.cache.last_project_change_count = state;

        let track_count = api.count_tracks();
        let total_fx_count = total_fx_count(api);
        let instances_affected =
            track_count != self.cache.last_track_count || total_fx_count != self.cache.last_total_fx_count;

        self.cache.last_track_count = track_count;
        self.cache.last_total_fx_count = total_fx_count;

        if !instances_affected {
            return;
        }

        let now = Instant::now();
        if now.duration_since(self.cache.last_broadcast_time) < PROJECT_CHANGED_DEBOUNCE {
            return;
        }
        self.cache.last_broadcast_time = now;
        self.cache.lookup.clear();
        self.validation_ready = false;
        self.send(ServerMessage::ProjectChanged);
    }

    fn apply_set_tone(&mut self, api: &dyn ReaperApi, cmd: ClientCommand) {
        let ClientCommand::SetTone {
            command_id,
            target_fx_guid,
            mode,
            params,
            ..
        } = cmd
        else {
            return;
        };

        if !self.validation_ready {
            self.send(ServerMessage::Error {
                msg: "not ready (handshake/validation required)".to_string(),
                code: ErrorCode::NotReady,
            });
            return;
        }

        let (track, fx_index) = match resolver::resolve_fx(api, &mut self.cache.lookup, &target_fx_guid)
        {
            Ok(r) => r,
            Err(_) => {
                self.send(ServerMessage::Error {
                    msg: "target fx guid not found".to_string(),
                    code: ErrorCode::TargetNotFound,
                });
                return;
            }
        };

        let mut params = match sanitize_params(params) {
            Ok(p) => p,
            Err(msg) => {
                self.send(ServerMessage::Error {
                    msg,
                    code: ErrorCode::InvalidValue,
                });
                return;
            }
        };

        if matches!(mode, MergeMode::ReplaceActive) {
            params = apply_replace_active_cleaner(params);
        }

        for p in &params {
            if let Err(e) = api.track_fx_set_param(track, fx_index, p.index, p.value) {
                self.send(ServerMessage::Error {
                    msg: format!("apply failed at param {}: {e}", p.index),
                    code: ErrorCode::InternalError,
                });
                return;
            }
        }

        self.send(ServerMessage::Ack { command_id });
    }

    fn send(&mut self, msg: ServerMessage) {
        // Non-blocking best-effort. If outbound is full, ProjectChanged is acceptable to drop.
        let _ = self
            .outbound_tx
            .try_send(OutboundMsg::Send { msg });
    }
}

fn total_fx_count(api: &dyn ReaperApi) -> i32 {
    let mut sum = 0;
    let track_count = api.count_tracks();
    for ti in 0..track_count {
        let Some(track) = api.get_track(ti) else { continue };
        sum += api.track_fx_count(track);
    }
    sum
}

fn sanitize_params(params: Vec<ParamChange>) -> Result<Vec<ParamChange>, String> {
    let mut last_by_index: HashMap<i32, f32> = HashMap::new();
    for p in &params {
        if p.index < 0 || p.index > MAX_PARAM_INDEX {
            return Err(format!("invalid param index: {}", p.index));
        }
        if !p.value.is_finite() {
            return Err(format!("non-finite value at index {}", p.index));
        }
        last_by_index.insert(p.index, p.value.clamp(0.0, 1.0));
    }

    // Preserve the original order of last occurrences ("last-wins").
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in params.into_iter().rev() {
        if !seen.insert(p.index) {
            continue;
        }
        let Some(v) = last_by_index.get(&p.index) else { continue };
        out.push(ParamChange {
            index: p.index,
            value: *v,
        });
    }
    out.reverse();
    Ok(out)
}

#[derive(Clone, Copy)]
struct ModuleDef {
    bypass: &'static [i32],
    params: &'static [i32],
}

const MODULES: &[ModuleDef] = &[
    // wow/pitch: both pedal_switch (3) and active (4) are treated as bypass controls.
    ModuleDef {
        bypass: &[3, 4],
        params: &[3, 4, 6],
    },
    ModuleDef {
        bypass: &[8],
        params: &[8, 9, 10, 11],
    },
    ModuleDef {
        bypass: &[13],
        params: &[13, 14, 15, 16],
    },
    ModuleDef {
        bypass: &[17],
        params: &[17, 18, 19, 20],
    },
    ModuleDef {
        bypass: &[21],
        params: &[21, 22],
    },
    ModuleDef {
        bypass: &[23],
        params: &[23, 24, 25, 27],
    },
    ModuleDef {
        bypass: &[101],
        params: &[101, 105, 106, 108],
    },
    ModuleDef {
        bypass: &[112],
        params: &[112, 114, 115, 116, 117],
    },
];

fn apply_replace_active_cleaner(params: Vec<ParamChange>) -> Vec<ParamChange> {
    let touched_modules: HashSet<usize> = MODULES
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            params.iter().any(|p| m.params.contains(&p.index) && !m.bypass.contains(&p.index))
        })
        .map(|(i, _)| i)
        .collect();

    let mut already_set: HashSet<i32> = params.iter().map(|p| p.index).collect();
    let mut out = params;

    for (i, module) in MODULES.iter().enumerate() {
        if touched_modules.contains(&i) {
            continue;
        }
        for &bypass_idx in module.bypass {
            if already_set.insert(bypass_idx) {
                out.push(ParamChange {
                    index: bypass_idx,
                    value: 0.0,
                });
            }
        }
    }
    out
}

