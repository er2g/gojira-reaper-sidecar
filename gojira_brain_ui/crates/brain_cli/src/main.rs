use brain_core::cleaner::{apply_replace_active_cleaner, sanitize_params};
use brain_core::gemini::{generate_tone_auto, ToneRequest};
use brain_core::protocol::{ClientCommand, MergeMode, ServerMessage};
use brain_core::{param_map, protocol::ParamChange};
use clap::Parser;
use std::collections::BTreeMap;
use std::net::TcpStream;
use std::path::PathBuf;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

#[derive(Parser, Debug)]
#[command(name = "brain_cli")]
struct Args {
    #[arg(long, required_unless_present = "prompt_file")]
    prompt: Option<String>,

    /// Read prompt content from a file (useful for long prompts / JSON blocks).
    #[arg(long, value_name = "PATH", conflicts_with = "prompt")]
    prompt_file: Option<PathBuf>,

    #[arg(long)]
    target_guid: Option<String>,

    #[arg(long, default_value = "auto")]
    backend: String,

    #[arg(long, default_value = "gemini-1.5-pro")]
    gemini_model: String,

    #[arg(long, default_value = "ws://127.0.0.1:9001")]
    ws_url: String,

    #[arg(long)]
    api_key_file: Option<String>,

    #[arg(long)]
    vertex_project: Option<String>,

    #[arg(long)]
    vertex_location: Option<String>,

    #[arg(long, default_value_t = false)]
    preview_only: bool,

    /// Skip REAPER websocket connection and only run AI + local QC (implies preview-only).
    #[arg(long, default_value_t = false)]
    no_ws: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let prompt = if let Some(p) = args.prompt.clone() {
        p
    } else {
        let path = args
            .prompt_file
            .clone()
            .ok_or_else(|| anyhow::anyhow!("missing --prompt or --prompt-file"))?;
        std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read prompt file {}: {e}", path.display()))?
    };

    std::env::set_var("GEMINI_BACKEND", args.backend.trim());
    if let Some(p) = args.vertex_project.as_deref() {
        std::env::set_var("VERTEX_PROJECT", p.trim());
    }
    if let Some(loc) = args.vertex_location.as_deref() {
        std::env::set_var("VERTEX_LOCATION", loc.trim());
    }

    let api_key = if let Some(path) = args.api_key_file.as_deref() {
        Some(std::fs::read_to_string(path)?.trim().to_string())
    } else {
        std::env::var("GEMINI_API_KEY").ok()
    };

    let (mut ws, session_token, target) = if args.no_ws {
        (None, String::new(), None)
    } else {
        let (mut ws, _resp) = connect(args.ws_url.as_str())?;
        let (session_token, instances, validation_report) = wait_handshake(&mut ws)?;

        eprintln!("handshake ok: {} instance(s)", instances.len());
        if !validation_report.is_empty() {
            eprintln!("validator:");
            for (k, v) in validation_report.iter() {
                eprintln!("  {k}: {v}");
            }
        }

        let target = if let Some(g) = args.target_guid.clone() {
            g
        } else {
            instances
                .iter()
                .find(|i| matches!(i.confidence, brain_core::protocol::Confidence::High))
                .or_else(|| instances.first())
                .ok_or_else(|| anyhow::anyhow!("no instances found (is the Gojira FX loaded?)"))?
                .fx_guid
                .clone()
        };

        (Some(ws), session_token, Some(target))
    };

    let tone = generate_tone_auto(
        &args.gemini_model,
        ToneRequest {
            user_prompt: prompt,
        },
        api_key.as_deref(),
    )
    .await?;

    eprintln!("\nreasoning:\n{}\n", tone.reasoning);

    let raw_params = tone.params.clone();
    let raw_sanitized = sanitize_params(raw_params.clone()).map_err(|e| anyhow::anyhow!(e))?;
    let cleaned = apply_replace_active_cleaner(MergeMode::ReplaceActive, raw_sanitized.clone());

    eprintln!("qc:");
    print_qc(&raw_params, &raw_sanitized, &cleaned);

    if args.preview_only || args.no_ws {
        eprintln!("preview_only=true (not applying to REAPER)");
        return Ok(());
    }

    let Some(ws) = ws.as_mut() else {
        return Err(anyhow::anyhow!("internal error: ws missing (this should be unreachable)"));
    };
    let target = target.ok_or_else(|| anyhow::anyhow!("internal error: target missing"))?;

    let cmd = ClientCommand::SetTone {
        session_token,
        command_id: format!("cli-{}", chrono_nanos()),
        target_fx_guid: target,
        mode: MergeMode::ReplaceActive,
        params: cleaned,
    };

    ws.send(Message::Text(serde_json::to_string(&cmd)?))?;
    wait_ack(ws)?;

    Ok(())
}

fn wait_handshake(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> anyhow::Result<(
    String,
    Vec<brain_core::protocol::GojiraInstance>,
    std::collections::HashMap<String, String>,
)> {
    loop {
        let msg = ws.read()?;
        let Message::Text(text) = msg else { continue };
        let server: ServerMessage = serde_json::from_str(&text)?;
        if let ServerMessage::Handshake {
            session_token,
            instances,
            validation_report,
            ..
        } = server
        {
            return Ok((session_token, instances, validation_report));
        }
    }
}

fn wait_ack(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> anyhow::Result<()> {
    loop {
        let msg = ws.read()?;
        let Message::Text(text) = msg else { continue };
        let server: ServerMessage = serde_json::from_str(&text)?;
        match server {
            ServerMessage::Ack { command_id } => {
                eprintln!("ack: {command_id}");
                return Ok(());
            }
            ServerMessage::Error { msg, code } => {
                return Err(anyhow::anyhow!("server error {code:?}: {msg}"));
            }
            _ => {}
        }
    }
}

fn chrono_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn label_for_index(index: i32) -> &'static str {
    match index {
        // Global
        0 => "Input Gain",
        1 => "Output Gain",
        2 => "Gate Amount",

        // Pitch / WOW
        3 => "Pitch Section Active",
        4 => "WOW Active",
        5 => "WOW Type",
        6 => "WOW Position",
        7 => "WOW Dry/Wet",

        // Octaver
        8 => "OCT Active",
        9 => "OCT Oct 1 Level",
        10 => "OCT Oct 2 Level",
        11 => "OCT Direct Level",

        param_map::selectors::AMP_TYPE_INDEX => "Amp Type",

        // Amp section + per-amp controls
        28 => "Amp Section Active",
        30 => "CLN Amp Gain",
        31 => "CLN Amp Bright",
        32 => "CLN Amp Bass",
        33 => "CLN Amp Mid",
        34 => "CLN Amp Treble",
        35 => "CLN Amp Level",
        36 => "RUST Amp Gain",
        37 => "RUST Amp Low",
        38 => "RUST Amp Mid",
        39 => "RUST Amp High",
        40 => "RUST Amp Master",
        41 => "RUST Amp Presence",
        42 => "RUST Amp Depth",
        43 => "RUST Amp Level",
        44 => "HOT Amp Gain",
        45 => "HOT Amp Low",
        46 => "HOT Amp Mid",
        47 => "HOT Amp High",
        48 => "HOT Amp Master",
        49 => "HOT Amp Presence",
        50 => "HOT Amp Depth",
        51 => "HOT Amp Level",

        // Graphic EQ
        52 => "EQ Section Active",
        53 => "CLN EQ Active",
        54 => "CLN EQ Band 1",
        55 => "CLN EQ Band 2",
        56 => "CLN EQ Band 3",
        57 => "CLN EQ Band 4",
        58 => "CLN EQ Band 5",
        59 => "CLN EQ Band 6",
        60 => "CLN EQ Band 7",
        61 => "CLN EQ Band 8",
        62 => "CLN EQ Band 9",
        63 => "RUST EQ Active",
        64 => "RUST EQ Band 1",
        65 => "RUST EQ Band 2",
        66 => "RUST EQ Band 3",
        67 => "RUST EQ Band 4",
        68 => "RUST EQ Band 5",
        69 => "RUST EQ Band 6",
        70 => "RUST EQ Band 7",
        71 => "RUST EQ Band 8",
        72 => "RUST EQ Band 9",
        73 => "HOT EQ Active",
        74 => "HOT EQ Band 1",
        75 => "HOT EQ Band 2",
        76 => "HOT EQ Band 3",
        77 => "HOT EQ Band 4",
        78 => "HOT EQ Band 5",
        79 => "HOT EQ Band 6",
        80 => "HOT EQ Band 7",
        81 => "HOT EQ Band 8",
        82 => "HOT EQ Band 9",

        param_map::pedals::overdrive::ACTIVE => "Overdrive Active",
        param_map::pedals::overdrive::DRIVE => "Overdrive Drive",
        param_map::pedals::overdrive::TONE => "Overdrive Tone",
        param_map::pedals::overdrive::LEVEL => "Overdrive Level",

        // Distortion
        17 => "DRT Active",
        18 => "DRT Dist",
        19 => "DRT Filter",
        20 => "DRT Vol",

        // Phaser
        21 => "PHSR Active",
        22 => "PHSR Rate",

        // Chorus
        23 => "CHR Active",
        24 => "CHR Rate",
        25 => "CHR Depth",
        26 => "CHR Feedback",
        27 => "CHR Mix",

        param_map::pedals::delay::ACTIVE => "Delay Active",
        param_map::pedals::delay::MIX => "Delay Dry/Wet",
        param_map::pedals::delay::FEEDBACK => "Delay Feedback",
        param_map::pedals::delay::TIME => "Delay Tempo",

        param_map::pedals::reverb::ACTIVE => "Reverb Active",
        param_map::pedals::reverb::MODE => "Reverb Mode",
        param_map::pedals::reverb::MIX => "Reverb Dry/Wet",
        param_map::pedals::reverb::TIME => "Reverb Time",
        param_map::pedals::reverb::LOW_CUT => "Reverb Low Cut",
        param_map::pedals::reverb::HIGH_CUT => "Reverb High Cut",

        // Cab
        83 => "Cab Section Active",
        84 => "Cab Type",
        85 => "Cab/Amp Linked",
        86 => "Cab 1 Active",
        87 => "Cab 1 Position",
        88 => "Cab 1 Distance",
        89 => "Cab 1 Level",
        90 => "Cab 1 Pan",
        91 => "Cab 1 Phase",
        92 => "Cab 1 Mic IR",
        93 => "Cab 2 Active",
        94 => "Cab 2 Position",
        95 => "Cab 2 Distance",
        96 => "Cab 2 Level",
        97 => "Cab 2 Pan",
        98 => "Cab 2 Phase",
        99 => "Cab 2 Mic IR",
        100 => "FX Section Active",
        _ => "Param",
    }
}

fn print_qc(raw: &[ParamChange], raw_sanitized: &[ParamChange], final_params: &[ParamChange]) {
    let mut warnings: Vec<String> = Vec::new();

    if raw.len() != raw_sanitized.len() {
        warnings.push(format!(
            "sanitize changed model param count: raw={} sanitized={}",
            raw.len(),
            raw_sanitized.len()
        ));
    }
    if raw_sanitized.len() != final_params.len() {
        warnings.push(format!(
            "replace_active added params: sanitized={} final={}",
            raw_sanitized.len(),
            final_params.len()
        ));
    }

    let mut has_bypass_or_midi = false;
    for p in final_params {
        if p.index == 118 || p.index >= 119 {
            has_bypass_or_midi = true;
        }
        if !(0.0..=1.0).contains(&p.value) || !p.value.is_finite() {
            warnings.push(format!("bad value at idx {} => {}", p.index, p.value));
        }
        if p.index < 0 || p.index > 4096 {
            warnings.push(format!("bad index {}", p.index));
        }
    }
    if has_bypass_or_midi {
        warnings.push("contains BYPASS (118) and/or MIDI CC (>=119) indices".to_string());
    }

    warnings.extend(module_consistency_warnings(final_params));

    let model_map = to_map(raw_sanitized);
    let final_map = to_map(final_params);

    let added_by_cleaner: Vec<ParamChange> = final_params
        .iter()
        .filter(|p| !model_map.contains_key(&p.index))
        .cloned()
        .collect();

    eprintln!("  model (sanitized):");
    print_grouped(raw_sanitized);

    if !added_by_cleaner.is_empty() {
        eprintln!("  added_by_replace_active:");
        print_grouped(&added_by_cleaner);
    }

    // Detect "changed by sanitizer" values (clamp/non-finite shouldn't happen, but keep it explicit).
    let raw_map = to_map(raw);
    let mut changed_by_sanitize: Vec<ParamChange> = Vec::new();
    for p in raw_sanitized {
        if let Some(orig) = raw_map.get(&p.index) {
            if (orig - p.value).abs() > 1e-6 {
                changed_by_sanitize.push(p.clone());
            }
        }
    }
    if !changed_by_sanitize.is_empty() {
        eprintln!("  changed_by_sanitize:");
        print_grouped(&changed_by_sanitize);
    }

    // Sanity: ensure no index value mismatches (shouldn't happen).
    for (idx, v) in model_map.iter() {
        if let Some(final_v) = final_map.get(idx) {
            // replace_active shouldn't overwrite model values.
            if (v - final_v).abs() > 1e-6 {
                warnings.push(format!(
                    "value changed for idx {} (model {:.3} -> final {:.3})",
                    idx, v, final_v
                ));
            }
        }
    }

    if warnings.is_empty() {
        eprintln!("  warnings: none");
    } else {
        eprintln!("  warnings:");
        for w in warnings {
            eprintln!("    - {w}");
        }
    }
}

fn module_consistency_warnings(params: &[ParamChange]) -> Vec<String> {
    let mut w = Vec::new();
    let set: std::collections::BTreeMap<i32, f32> = to_map(params);

    // If any non-toggle params are present, ensure the module toggle is explicitly present too.
    // We don't auto-fix here; we warn so the prompt/system can be improved.
    let checks: &[(&str, i32, &[i32])] = &[
        ("wow", 4, &[5, 6, 7]),
        ("oct", 8, &[9, 10, 11]),
        ("overdrive", 13, &[14, 15, 16]),
        ("distortion", 17, &[18, 19, 20]),
        ("phaser", 21, &[22]),
        ("chorus", 23, &[24, 25, 26, 27]),
        ("delay", 101, &[105, 106, 108]),
        ("reverb", 112, &[114, 115, 116, 117]),
        // Cab section active is 83; FX section active is 100 (separate toggle).
        ("cab", 83, &[84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99]),
    ];

    for (name, toggle, deps) in checks {
        let dep_present = deps.iter().any(|i| set.contains_key(i));
        if dep_present && !set.contains_key(toggle) {
            w.push(format!(
                "module '{name}' has params set ({:?}) but missing toggle idx {toggle}",
                deps.iter().copied().filter(|i| set.contains_key(i)).collect::<Vec<_>>()
            ));
        }
    }

    // Amp: if Amp Type is set, warn if it adjusts other amp's controls heavily.
    if let Some(&amp_type) = set.get(&29) {
        let clean = [30, 31, 32, 33, 34, 35, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62];
        let rust = [36, 37, 38, 39, 40, 41, 42, 43, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72];
        let hot = [44, 45, 46, 47, 48, 49, 50, 51, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82];

        let clean_touched = clean.iter().any(|i| set.contains_key(i));
        let rust_touched = rust.iter().any(|i| set.contains_key(i));
        let hot_touched = hot.iter().any(|i| set.contains_key(i));

        // interpret selection by nearest canonical value
        let sel = if (amp_type - 0.0).abs() < 0.2 {
            "clean"
        } else if (amp_type - 0.5).abs() < 0.2 {
            "rust"
        } else if (amp_type - 1.0).abs() < 0.2 {
            "hot"
        } else {
            "unknown"
        };

        match sel {
            "clean" => {
                if (rust_touched || hot_touched) && clean_touched {
                    w.push("amp type is clean but rust/hot controls are also modified".to_string());
                }
                if !clean_touched {
                    w.push("amp type is clean but no clean amp/EQ controls were modified".to_string());
                }
            }
            "rust" => {
                if (clean_touched || hot_touched) && rust_touched {
                    w.push("amp type is rust but clean/hot controls are also modified".to_string());
                }
                if !rust_touched {
                    w.push("amp type is rust but no rust amp/EQ controls were modified".to_string());
                }
            }
            "hot" => {
                if (clean_touched || rust_touched) && hot_touched {
                    w.push("amp type is hot but clean/rust controls are also modified".to_string());
                }
                if !hot_touched {
                    w.push("amp type is hot but no hot amp/EQ controls were modified".to_string());
                }
            }
            _ => {}
        }
    }

    w
}

fn to_map(params: &[ParamChange]) -> BTreeMap<i32, f32> {
    let mut out = BTreeMap::new();
    for p in params {
        out.insert(p.index, p.value);
    }
    out
}

fn group_key(index: i32) -> &'static str {
    match index {
        0..=2 => "global",
        3..=27 => "pedals_pre",
        28..=82 => "amp_eq",
        83..=100 => "cab",
        101..=111 => "delay",
        112..=118 => "reverb",
        119..=4096 => "midi_or_other",
        _ => "other",
    }
}

fn print_grouped(params: &[ParamChange]) {
    let mut groups: BTreeMap<&'static str, Vec<&ParamChange>> = BTreeMap::new();
    for p in params {
        groups.entry(group_key(p.index)).or_default().push(p);
    }

    for (g, mut items) in groups {
        items.sort_by_key(|p| p.index);
        eprintln!("    [{g}]");
        for p in items {
            eprintln!(
                "      {:>4} {:<18} = {:.3}",
                p.index,
                label_for_index(p.index),
                p.value
            );
        }
    }
}
