use crate::reaper_api::ReaperApi;
use gojira_protocol::{ParamEnumOption, ParamFormatSample, ParamFormatTriplet};
use std::collections::HashMap;

const DELAY_ACTIVE_ANCHOR: i32 = 101;
const REVERB_ACTIVE_ANCHOR: i32 = 112;

// Probes for common "mix" (dry/wet) controls in known plugin builds.
const DELAY_MIX_PROBE: i32 = 105;
const REVERB_MIX_PROBE: i32 = 114;

pub fn validate_parameter_map(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
) -> HashMap<String, String> {
    let mut report = HashMap::new();

    // Backward-compatible keys (original anchors).
    report.insert(
        "delay_active".to_string(),
        anchor_report(api, track, fx_index, DELAY_ACTIVE_ANCHOR),
    );
    report.insert(
        "reverb_active".to_string(),
        anchor_report(api, track, fx_index, REVERB_ACTIVE_ANCHOR),
    );

    report.insert(
        "delay_active_101".to_string(),
        anchor_report(api, track, fx_index, DELAY_ACTIVE_ANCHOR),
    );
    report.insert(
        "delay_mix_105".to_string(),
        mix_report(api, track, fx_index, DELAY_MIX_PROBE),
    );
    report.insert(
        "reverb_active_112".to_string(),
        anchor_report(api, track, fx_index, REVERB_ACTIVE_ANCHOR),
    );
    report.insert(
        "reverb_mix_114".to_string(),
        mix_report(api, track, fx_index, REVERB_MIX_PROBE),
    );

    report.extend(validate_mix(api, track, fx_index));

    // Small windows for quick forensic debugging (kept intentionally short).
    report.insert(
        "delay_window_98_110".to_string(),
        window_dump(api, track, fx_index, 98..=110),
    );
    report.insert(
        "reverb_window_110_122".to_string(),
        window_dump(api, track, fx_index, 110..=122),
    );
    report.insert(
        "pre_window_0_40".to_string(),
        window_dump(api, track, fx_index, 0..=40),
    );
    report.insert(
        "amp_eq_window_28_60".to_string(),
        window_dump(api, track, fx_index, 28..=60),
    );
    report.insert(
        "amp_eq_window_60_90".to_string(),
        window_dump(api, track, fx_index, 60..=90),
    );
    report.insert(
        "cab_window_83_100".to_string(),
        window_dump(api, track, fx_index, 83..=100),
    );
    report
}

pub fn probe_param_meta(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
) -> (
    HashMap<i32, Vec<ParamEnumOption>>,
    HashMap<i32, ParamFormatTriplet>,
    HashMap<i32, Vec<ParamFormatSample>>,
) {
    let mut enums: HashMap<i32, Vec<ParamEnumOption>> = HashMap::new();
    let mut formats: HashMap<i32, ParamFormatTriplet> = HashMap::new();
    let mut samples: HashMap<i32, Vec<ParamFormatSample>> = HashMap::new();

    // Enumerated selectors we care about for cab/IR + a couple of FX modes.
    for (idx, samples, max_options) in [
        (84, 512, 64),  // Cab Type
        (92, 2048, 512), // Cab 1 Mic IR
        (99, 2048, 512), // Cab 2 Mic IR
        (113, 256, 32), // Reverb Mode
        (5, 128, 32),   // WOW Type
    ] {
        if api.track_fx_param_name(track, fx_index, idx).is_none() {
            continue;
        }
        let opts = probe_enum(api, track, fx_index, idx, samples, max_options);
        if !opts.is_empty() {
            enums.insert(idx, opts);
        }
    }

    // Continuous controls where formatted values can reveal units/direction.
    for idx in [87, 88, 94, 95, 105, 106, 108, 114, 115] {
        if api.track_fx_param_name(track, fx_index, idx).is_none() {
            continue;
        }
        if let Some(t) = probe_format_triplet(api, track, fx_index, idx) {
            formats.insert(idx, t);
        }
    }

    // Optional: attach a small set of formatted samples (norm->formatted) for unit conversion.
    // Enabled via env var to avoid bloating handshake by default.
    let enable_samples = std::env::var("GOJIRA_SEND_PARAM_SAMPLES")
        .ok()
        .map(|s| s.trim().eq_ignore_ascii_case("1") || s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if enable_samples {
        let steps: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
        if let Some(n) = api.track_fx_num_params(track, fx_index) {
            for idx in 0..n {
                let idx = idx as i32;
                if api.track_fx_param_name(track, fx_index, idx).is_none() {
                    continue;
                }
                let mut v: Vec<ParamFormatSample> = Vec::new();
                for &norm in &steps {
                    let formatted = api
                        .track_fx_format_param_value(track, fx_index, idx, norm)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if formatted.is_empty() {
                        continue;
                    }
                    v.push(ParamFormatSample { norm, formatted });
                }
                if !v.is_empty() {
                    samples.insert(idx, v);
                }
            }
        }
    }

    (enums, formats, samples)
}

fn probe_format_triplet(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
    idx: i32,
) -> Option<ParamFormatTriplet> {
    let min = api
        .track_fx_format_param_value(track, fx_index, idx, 0.0)
        .unwrap_or_default()
        .trim()
        .to_string();
    let mid = api
        .track_fx_format_param_value(track, fx_index, idx, 0.5)
        .unwrap_or_default()
        .trim()
        .to_string();
    let max = api
        .track_fx_format_param_value(track, fx_index, idx, 1.0)
        .unwrap_or_default()
        .trim()
        .to_string();

    if min.is_empty() && mid.is_empty() && max.is_empty() {
        None
    } else {
        Some(ParamFormatTriplet { min, mid, max })
    }
}

fn probe_enum(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
    idx: i32,
    samples: i32,
    max_options: usize,
) -> Vec<ParamEnumOption> {
    let samples = samples.max(16);

    let mut segments: Vec<(String, f32, f32)> = Vec::new();
    let mut last_label: Option<String> = None;
    let mut seg_start: f32 = 0.0;

    for s in 0..=samples {
        let v = (s as f32) / (samples as f32);
        let label = api
            .track_fx_format_param_value(track, fx_index, idx, v)
            .unwrap_or_default()
            .trim()
            .to_string();
        if label.is_empty() {
            continue;
        }

        match &last_label {
            None => {
                last_label = Some(label);
                seg_start = v;
            }
            Some(prev) if prev == &label => {}
            Some(prev) => {
                segments.push((prev.clone(), seg_start, v));
                last_label = Some(label);
                seg_start = v;
            }
        }
    }

    if let Some(prev) = last_label {
        segments.push((prev, seg_start, 1.0));
    }

    // Convert segments to unique options (midpoint value per label).
    let mut out: Vec<ParamEnumOption> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for (label, start, end) in segments {
        let label = label.trim().to_string();
        if label.is_empty() {
            continue;
        }
        if seen.contains_key(&label) {
            continue;
        }
        let mid = ((start + end) * 0.5).clamp(0.0, 1.0);
        out.push(ParamEnumOption { value: mid, label: label.clone() });
        seen.insert(label, ());
        if out.len() >= max_options {
            break;
        }
    }

    out
}

fn anchor_report(api: &dyn ReaperApi, track: usize, fx_index: i32, idx: i32) -> String {
    let Some(name) = api.track_fx_param_name(track, fx_index, idx) else {
        return format!("missing param name at {idx}");
    };
    let n = normalize(&name);
    if n.contains("active") || n.contains("on") || n.contains("enable") {
        format!("present at {idx} ({name})")
    } else {
        format!("present but suspicious at {idx} ({name})")
    }
}

fn mix_report(api: &dyn ReaperApi, track: usize, fx_index: i32, idx: i32) -> String {
    let Some(name) = api.track_fx_param_name(track, fx_index, idx) else {
        return format!("missing param name at {idx}");
    };
    let n = normalize(&name);
    if n.contains("mix") || n.contains("drywet") || (n.contains("dry") && n.contains("wet")) {
        format!("present at {idx} ({name})")
    } else {
        format!("present but suspicious at {idx} ({name})")
    }
}

fn validate_mix(api: &dyn ReaperApi, track: usize, fx_index: i32) -> HashMap<String, String> {
    let mut report = HashMap::new();
    let delay_anchor = pick_active_near(api, track, fx_index, DELAY_ACTIVE_ANCHOR, 96..=110)
        .unwrap_or(DELAY_ACTIVE_ANCHOR);
    let reverb_anchor = pick_active_near(api, track, fx_index, REVERB_ACTIVE_ANCHOR, 108..=120)
        .unwrap_or(REVERB_ACTIVE_ANCHOR);

    report.insert("delay_active_best_guess".to_string(), delay_anchor.to_string());
    report.insert(
        "reverb_active_best_guess".to_string(),
        reverb_anchor.to_string(),
    );

    let delay_mix = pick_mix_near(api, track, fx_index, delay_anchor, 100..=115);
    let reverb_mix = pick_mix_near(api, track, fx_index, reverb_anchor, 110..=125);

    report.insert(
        "delay_mix".to_string(),
        delay_mix
            .map(|(idx, name)| {
                format!(
                    "confirmed at {idx} (neighbor of {delay_anchor}): {name}"
                )
            })
            .unwrap_or_else(|| "not found".to_string()),
    );
    report.insert(
        "reverb_mix".to_string(),
        reverb_mix
            .map(|(idx, name)| {
                format!(
                    "confirmed at {idx} (neighbor of {reverb_anchor}): {name}"
                )
            })
            .unwrap_or_else(|| "not found".to_string()),
    );
    report
}

fn pick_active_near(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
    expected: i32,
    range: std::ops::RangeInclusive<i32>,
) -> Option<i32> {
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    for idx in range {
        let Some(name) = api.track_fx_param_name(track, fx_index, idx) else {
            continue;
        };
        let n = normalize(&name);
        if n.contains("active") || n.contains("on") || n.contains("enable") {
            candidates.push((idx, (idx - expected).abs()));
        }
    }
    candidates.sort_by_key(|(_, dist)| *dist);
    candidates.first().map(|(idx, _)| *idx)
}

fn pick_mix_near(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
    anchor: i32,
    range: std::ops::RangeInclusive<i32>,
) -> Option<(i32, String)> {
    let mut candidates: Vec<(i32, String, i32)> = Vec::new();
    for idx in range {
        let Some(name) = api.track_fx_param_name(track, fx_index, idx) else {
            continue;
        };
        let n = normalize(&name);
        let looks_like_mix = n.contains("mix") || n.contains("drywet") || (n.contains("dry") && n.contains("wet"));
        if looks_like_mix {
            candidates.push((idx, name, (idx - anchor).abs()));
        }
    }
    candidates.sort_by_key(|(_, _, dist)| *dist);

    if candidates.is_empty() {
        return None;
    }

    let best_dist = candidates[0].2;
    let tied: Vec<(i32, String)> = candidates
        .into_iter()
        .filter(|(_, _, dist)| *dist == best_dist)
        .map(|(i, name, _)| (i, name))
        .collect();

    if tied.len() == 1 {
        return tied.into_iter().next();
    }

    // Tie-break: prefer the candidate whose neighborhood includes "feedback" or "time".
    for (idx, name) in &tied {
        let neigh = [*idx - 1, *idx + 1];
        let ok = neigh.iter().any(|n| {
            api.track_fx_param_name(track, fx_index, *n)
                .map(|s| {
                    let ns = normalize(&s);
                    ns.contains("feedback") || ns.contains("time")
                })
                .unwrap_or(false)
        });
        if ok {
            return Some((*idx, name.clone()));
        }
    }

    tied.into_iter().next()
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn window_dump(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
    range: std::ops::RangeInclusive<i32>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for idx in range {
        if let Some(name) = api.track_fx_param_name(track, fx_index, idx) {
            parts.push(format!("{idx}:{name}"));
        }
    }
    if parts.is_empty() {
        return "no params in window".to_string();
    }
    let joined = parts.join(" | ");
    const MAX_CHARS: usize = 900;
    if joined.len() <= MAX_CHARS {
        joined
    } else {
        format!("{}…", &joined[..MAX_CHARS])
    }
}
