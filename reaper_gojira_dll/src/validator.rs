use crate::reaper_api::ReaperApi;
use gojira_protocol::{ParamEnumOption, ParamFormatSample, ParamFormatTriplet};
use std::collections::HashMap;

const DELAY_ACTIVE_ANCHOR: i32 = 101;
const REVERB_ACTIVE_ANCHOR: i32 = 112;

// Probes for common "mix" (dry/wet) controls in known plugin builds.
const DELAY_MIX_PROBE: i32 = 105;
const REVERB_MIX_PROBE: i32 = 114;

fn validation_report_enabled() -> bool {
    matches!(
        std::env::var("GOJIRA_SEND_VALIDATION_REPORT").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

pub fn validate_parameter_map(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
) -> HashMap<String, String> {
    if !validation_report_enabled() {
        return HashMap::new();
    }
    let mut report = HashMap::new();

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

    // Continuous controls where formatted values can reveal units/direction/scales.
    // Keep this reasonably broad so the backend can do robust unit->0..1 conversions
    // without requiring full sample telemetry.
    let mut format_indices: Vec<i32> = Vec::new();
    format_indices.extend([0, 1, 2]); // input/output gain + gate
    format_indices.extend(30..=51); // amp knobs
    format_indices.extend(54..=82); // graphic EQ bands
    format_indices.extend([87, 88, 89, 94, 95, 96]); // cab mic position/distance/level
    format_indices.extend([105, 106, 108]); // delay
    format_indices.extend([114, 115, 116, 117]); // reverb
    format_indices.sort_unstable();
    format_indices.dedup();

    for idx in format_indices {
        if api.track_fx_param_name(track, fx_index, idx).is_none() {
            continue;
        }
        if let Some(t) = probe_format_triplet(api, track, fx_index, idx) {
            formats.insert(idx, t);
        }
    }

    // Optional: attach formatted samples (norm->formatted) for unit conversion.
    // Enabled via env var to avoid bloating handshake by default.
    let enable_samples = std::env::var("GOJIRA_SEND_PARAM_SAMPLES")
        .ok()
        .map(|s| s.trim().eq_ignore_ascii_case("1") || s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    if enable_samples {
        let steps = std::env::var("GOJIRA_PARAM_SAMPLE_STEPS")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(11)
            .clamp(3, 201);

        let mode = std::env::var("GOJIRA_PARAM_SAMPLE_MODE")
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_else(|| "tone".to_string());

        let indices: Vec<i32> = if mode == "all" {
            match api.track_fx_num_params(track, fx_index) {
                Some(n) => (0..n).map(|i| i as i32).collect(),
                None => Vec::new(),
            }
        } else {
            // "tone" mode: keep it reasonably small but useful for conversions.
            let mut v: Vec<i32> = Vec::new();
            v.extend([0, 1]); // input/output gain
            v.push(2); // gate
            v.push(29); // amp selector
            v.extend(30..=51); // amp knobs
            v.extend(54..=82); // EQ bands
            v.extend([83, 84, 85, 87, 88, 89, 92, 94, 95, 96, 99]); // cab selectors (+ mic pos/dist/levels)
            v.extend([101, 105, 106, 108]); // delay
            v.extend([112, 113, 114, 115, 116, 117]); // reverb
            v.sort_unstable();
            v.dedup();
            v
        };

        let mut norms: Vec<f32> = Vec::with_capacity(steps);
        for i in 0..steps {
            norms.push(i as f32 / (steps - 1) as f32);
        }

        for idx in indices {
            if api.track_fx_param_name(track, fx_index, idx).is_none() {
                continue;
            }
            let mut v: Vec<ParamFormatSample> = Vec::new();
            for &norm in &norms {
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
