use crate::modules::param_map;
use crate::modules::protocol::ParamChange;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AiToneResponse {
    pub reasoning: String,
    pub params: Vec<AiParamChange>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiParamChange {
    pub index: i32,
    pub value: serde_json::Value,
}

#[derive(Debug)]
pub struct ResolveError(pub String);

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResolveError {}

#[derive(Debug, Clone, Deserialize)]
struct EnumOption {
    value: f32,
    label: String,
}

fn extract_prompt_json_line<'a>(prompt: &'a str, key: &str) -> Option<&'a str> {
    for line in prompt.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix(key) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn parse_enum_options(
    prompt: &str,
) -> Option<std::collections::HashMap<i32, Vec<EnumOption>>> {
    let raw = extract_prompt_json_line(prompt, "ENUM_OPTIONS_JSON=")?;
    let parsed: std::collections::HashMap<String, Vec<EnumOption>> =
        serde_json::from_str(raw).ok()?;

    let mut out: std::collections::HashMap<i32, Vec<EnumOption>> =
        std::collections::HashMap::new();
    for (k, v) in parsed {
        if let Ok(idx) = k.parse::<i32>() {
            out.insert(idx, v);
        }
    }
    Some(out)
}

fn default_enum_options() -> std::collections::HashMap<i32, Vec<EnumOption>> {
    use std::collections::HashMap;
    let mut out: HashMap<i32, Vec<EnumOption>> = HashMap::new();

    // These values have been observed in the current Gojira build via TrackFX_FormatParamValue
    // sampling (and are stable across many plugin builds). If ENUM_OPTIONS_JSON is present, it
    // takes precedence.
    out.insert(
        param_map::cab::TYPE_SELECTOR,
        vec![
            EnumOption {
                value: 0.125_976_56,
                label: "Cab 1".to_string(),
            },
            EnumOption {
                value: 0.500_976_56,
                label: "Cab 2".to_string(),
            },
            EnumOption {
                value: 0.875,
                label: "Cab 3".to_string(),
            },
        ],
    );

    let mic = vec![
        EnumOption {
            value: 0.041_748_047,
            label: "Dynamic 57".to_string(),
        },
        EnumOption {
            value: 0.166_748_05,
            label: "Dynamic 421".to_string(),
        },
        EnumOption {
            value: 0.333_496_1,
            label: "Condenser 414".to_string(),
        },
        EnumOption {
            value: 0.500_244_14,
            label: "Condenser 184".to_string(),
        },
        EnumOption {
            value: 0.666_992_2,
            label: "Ribbon 160".to_string(),
        },
        EnumOption {
            value: 0.833_740_23,
            label: "Ribbon 121".to_string(),
        },
        EnumOption {
            value: 0.958_496_1,
            label: "Custom IR".to_string(),
        },
    ];
    out.insert(param_map::cab::mic1::IR_SEL, mic.clone());
    out.insert(param_map::cab::mic2::IR_SEL, mic);

    out.insert(
        param_map::pedals::reverb::MODE,
        vec![
            EnumOption {
                value: 0.251_953_12,
                label: "Reverb".to_string(),
            },
            EnumOption {
                value: 0.751_953_1,
                label: "Shimmer".to_string(),
            },
        ],
    );
    out.insert(
        5,
        vec![
            EnumOption {
                value: 0.128_906_25,
                label: "FATSO".to_string(),
            },
            EnumOption {
                value: 0.503_906_25,
                label: "BLADE 1".to_string(),
            },
            EnumOption {
                value: 0.875,
                label: "BLADE 2".to_string(),
            },
        ],
    );

    out
}

fn default_formatted_value_triplets() -> std::collections::HashMap<i32, (String, String, String)> {
    use std::collections::HashMap;
    let mut out: HashMap<i32, (String, String, String)> = HashMap::new();

    // These have been observed via TrackFX_FormatParamValue on a recent Archetype Gojira build.
    // They are used as a fallback when FORMATTED_VALUE_TRIPLETS_JSON isn't present.
    out.insert(0, ("-24.0".to_string(), "0.0".to_string(), "24.0".to_string())); // Input Gain
    out.insert(1, ("-24.0".to_string(), "0.0".to_string(), "24.0".to_string())); // Output Gain
    out.insert(2, ("-96.0".to_string(), "-48.0".to_string(), "0.0".to_string())); // Gate Amount

    // Tempo/time (no explicit unit in formatted strings, but these map linearly in practice).
    out.insert(108, ("40.0".to_string(), "140.0".to_string(), "240.0".to_string())); // DLY Tempo (bpm)
    out.insert(115, ("250.00".to_string(), "5125.00".to_string(), "10000.00".to_string())); // REV Time (ms)
    out.insert(116, ("50".to_string(), "375".to_string(), "700".to_string())); // REV Low Cut (Hz)
    out.insert(117, ("1000".to_string(), "5500".to_string(), "10000".to_string())); // REV High Cut (Hz)

    out
}

fn parse_format_samples(
    prompt: &str,
) -> Option<std::collections::HashMap<i32, Vec<(f32, String)>>> {
    let raw = extract_prompt_json_line(prompt, "PARAM_FORMAT_SAMPLES_JSON=")?;
    let parsed: std::collections::HashMap<String, Vec<(f32, String)>> =
        serde_json::from_str(raw).ok()?;

    let mut out: std::collections::HashMap<i32, Vec<(f32, String)>> =
        std::collections::HashMap::new();
    for (k, v) in parsed {
        if let Ok(idx) = k.parse::<i32>() {
            out.insert(idx, v);
        }
    }
    Some(out)
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn parse_numeric_value(value: &serde_json::Value) -> Option<f32> {
    match value {
        serde_json::Value::Number(n) => n.as_f64().map(|v| v as f32),
        serde_json::Value::String(s) => s.trim().parse::<f32>().ok(),
        _ => None,
    }
}

fn parse_bool_like(s: &str) -> Option<f32> {
    match s.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "enabled" => Some(1.0),
        "off" | "false" | "no" | "disabled" => Some(0.0),
        _ => None,
    }
}

fn parse_percent(s: &str) -> Option<f32> {
    let t = s.trim().trim_end_matches('%').trim();
    let v = t.parse::<f32>().ok()?;
    Some((v / 100.0).clamp(0.0, 1.0))
}

fn parse_db(s: &str) -> Option<f32> {
    // Accept "+3.2 dB", "-10db", "3db"
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    let t = t.replace("db", "").trim().to_string();
    t.parse::<f32>().ok()
}

fn parse_db_from_formatted(s: &str) -> Option<f32> {
    // Accept "-6.0 dB", "-6dB", "+3,2 dB"
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    if !t.contains("db") {
        return None;
    }
    // Keep digits, sign, dot, and spaces, then parse first float-like token.
    let cleaned: String = t
        .chars()
        .map(|c| if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == ' ' { c } else { ' ' })
        .collect();
    for tok in cleaned.split_whitespace() {
        if let Ok(v) = tok.parse::<f32>() {
            return Some(v);
        }
    }
    None
}

fn invert_piecewise(points: &[(f32, f32)], target: f32) -> Option<f32> {
    // points: (physical, norm). We assume physical is monotonic after sorting.
    if points.is_empty() {
        return None;
    }
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);

    let min = pts.first()?.0;
    let max = pts.last()?.0;
    if target <= min {
        return Some(pts.first()?.1);
    }
    if target >= max {
        return Some(pts.last()?.1);
    }

    for w in pts.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if (x0..=x1).contains(&target) || (x1..=x0).contains(&target) {
            if (x1 - x0).abs() < 1e-6 {
                return Some(y0);
            }
            let t = (target - x0) / (x1 - x0);
            return Some((y0 + t * (y1 - y0)).clamp(0.0, 1.0));
        }
    }
    None
}

fn parse_ms_or_s(s: &str) -> Option<(f32, &'static str)> {
    // Returns numeric value and canonical unit "ms" or "s"
    let t = s.trim().to_ascii_lowercase().replace(' ', "");
    if let Some(v) = t.strip_suffix("ms") {
        return v.parse::<f32>().ok().map(|n| (n, "ms"));
    }
    if let Some(v) = t.strip_suffix('s') {
        return v.parse::<f32>().ok().map(|n| (n, "s"));
    }
    None
}

fn parse_ms_value(s: &str) -> Option<f32> {
    let (n, unit) = parse_ms_or_s(s)?;
    Some(if unit == "s" { n * 1000.0 } else { n })
}

fn parse_ms_from_formatted(s: &str) -> Option<f32> {
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    if !(t.contains("ms") || t.ends_with('s')) {
        return None;
    }
    // Pull first float-like token.
    let cleaned: String = t
        .chars()
        .map(|c| if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == ' ' { c } else { ' ' })
        .collect();
    let first = cleaned.split_whitespace().next()?;
    let v = first.parse::<f32>().ok()?;
    if t.contains("ms") {
        Some(v)
    } else if t.ends_with('s') {
        Some(v * 1000.0)
    } else {
        None
    }
}

fn parse_hz_value(s: &str) -> Option<f32> {
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    let t = t.replace(' ', "");
    if let Some(v) = t.strip_suffix("khz") {
        return v.parse::<f32>().ok().map(|n| n * 1000.0);
    }
    if let Some(v) = t.strip_suffix("hz") {
        return v.parse::<f32>().ok();
    }
    None
}

fn parse_bpm_value(s: &str) -> Option<f32> {
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    if !t.contains("bpm") {
        return None;
    }
    // Pull first float-like token.
    let cleaned: String = t
        .chars()
        .map(|c| {
            if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let first = cleaned.split_whitespace().next()?;
    first.parse::<f32>().ok()
}

fn resolve_amp_type(value: &serde_json::Value) -> Option<f32> {
    let s = value.as_str()?.trim();
    let s = normalize_ws(s);
    match s.to_ascii_lowercase().as_str() {
        "clean" | "the clean" => Some(0.0),
        "crunch" | "the crunch" | "rust" => Some(0.5),
        "lead" | "the lead" | "hot" => Some(1.0),
        _ => None,
    }
}

fn resolve_from_enum_label(
    enums: &std::collections::HashMap<i32, Vec<EnumOption>>,
    index: i32,
    value: &serde_json::Value,
) -> Option<f32> {
    let s = value.as_str()?.trim();
    let s = normalize_ws(s);
    let opts = enums.get(&index)?;
    // Exact label match (case-insensitive)
    if let Some(opt) = opts.iter().find(|o| eq_ignore_case(&o.label, &s)) {
        return Some(opt.value);
    }
    // Common abbreviations like "cab3", "cab 3"
    if index == param_map::cab::TYPE_SELECTOR {
        let l = s.to_ascii_lowercase().replace(' ', "");
        if l == "cab1" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 1")).map(|o| o.value);
        }
        if l == "cab2" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 2")).map(|o| o.value);
        }
        if l == "cab3" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 3")).map(|o| o.value);
        }
        if l == "cleancab" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 1")).map(|o| o.value);
        }
        if l == "crunchcab" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 2")).map(|o| o.value);
        }
        if l == "leadcab" {
            return opts.iter().find(|o| o.label.eq_ignore_ascii_case("Cab 3")).map(|o| o.value);
        }
    }
    None
}

fn resolve_eq_band_db(index: i32, s: &str) -> Option<f32> {
    // Heuristic fallback: Graphic EQ bands are typically -12..+12 dB, with 0 dB at 0.5.
    // Map desired dB into normalized 0..1.
    let db = parse_db(s)?;
    let (min_db, max_db) = (-12.0_f32, 12.0_f32);
    // Only apply to known band indices.
    let is_band = (54..=82).contains(&index);
    if !is_band {
        return None;
    }
    Some(((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0))
}

fn parse_formatted_value_triplets(
    prompt: &str,
) -> Option<std::collections::HashMap<i32, (String, String, String)>> {
    let raw = extract_prompt_json_line(prompt, "FORMATTED_VALUE_TRIPLETS_JSON=")?;
    let parsed: std::collections::HashMap<String, (String, String, String)> =
        serde_json::from_str(raw).ok()?;

    let mut out: std::collections::HashMap<i32, (String, String, String)> =
        std::collections::HashMap::new();
    for (k, v) in parsed {
        if let Ok(idx) = k.parse::<i32>() {
            out.insert(idx, v);
        }
    }
    Some(out)
}

fn parse_first_float(s: &str) -> Option<f32> {
    let t = s.trim().to_ascii_lowercase().replace(',', ".");
    let cleaned: String = t
        .chars()
        .map(|c| {
            if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    for tok in cleaned.split_whitespace() {
        if let Ok(v) = tok.parse::<f32>() {
            return Some(v);
        }
    }
    None
}

fn invert_from_triplet_physical(
    triplets: &std::collections::HashMap<i32, (String, String, String)>,
    index: i32,
    physical: f32,
) -> Option<f32> {
    let (min_s, _mid_s, max_s) = triplets.get(&index)?.clone();
    let min = parse_first_float(&min_s)?;
    let max = parse_first_float(&max_s)?;
    if (max - min).abs() < 1e-6 {
        return None;
    }

    // Only apply if the triplet clearly represents a physical range (not just "0..1").
    if max <= 1.5 && min >= -0.5 {
        return None;
    }

    Some(((physical - min) / (max - min)).clamp(0.0, 1.0))
}

fn invert_from_samples_physical(
    samples: &std::collections::HashMap<i32, Vec<(f32, String)>>,
    index: i32,
    physical: f32,
) -> Option<f32> {
    let raw = samples.get(&index)?;
    if raw.is_empty() {
        return None;
    }

    // Pick a physical parser based on sample formatted strings.
    let mut has_db = false;
    let mut has_ms = false;
    for (_norm, formatted) in raw {
        let f = formatted.to_ascii_lowercase();
        if f.contains("db") {
            has_db = true;
        }
        if f.contains("ms") || f.trim_end().ends_with('s') {
            has_ms = true;
        }
    }

    let mut pts: Vec<(f32, f32)> = Vec::new(); // (physical, norm)
    for (norm, formatted) in raw {
        let p = if has_db {
            parse_db_from_formatted(formatted)
        } else if has_ms {
            parse_ms_from_formatted(formatted)
        } else {
            parse_first_float(formatted)
        }?;
        pts.push((p, *norm));
    }

    invert_piecewise(&pts, physical)
}

fn resolve_value_for_index(
    prompt: &str,
    enums: Option<&std::collections::HashMap<i32, Vec<EnumOption>>>,
    samples: Option<&std::collections::HashMap<i32, Vec<(f32, String)>>>,
    triplets: Option<&std::collections::HashMap<i32, (String, String, String)>>,
    index: i32,
    value: &serde_json::Value,
) -> Result<f32, ResolveError> {
    // Numbers still work when they are truly normalized 0..1.
    if let Some(v) = parse_numeric_value(value) {
        if (0.0..=1.0).contains(&v) {
            return Ok(v);
        }

        // Pan controls: allow -1..1 (center=0.0) and map to 0..1 (center=0.5).
        if (index == 90 || index == 97) && (-1.0..=1.0).contains(&v) {
            return Ok(((v + 1.0) * 0.5).clamp(0.0, 1.0));
        }

        // For non-normalized numeric values, only accept them if we can invert a known physical
        // mapping (samples or formatted triplets). This prevents nonsense like "650" from being
        // silently clamped to 1.0.
        if let Some(samples) = samples {
            if let Some(norm) = invert_from_samples_physical(samples, index, v) {
                return Ok(norm);
            }
        }
        if let Some(triplets) = triplets {
            if let Some(norm) = invert_from_triplet_physical(triplets, index, v) {
                return Ok(norm);
            }
        }

        return Err(ResolveError(format!(
            "numeric value {v} for idx {index} is not a normalized 0..1 value, and no calibration mapping was available (try \"%\", \"dB\", \"ms\", \"bpm\", or enable PARAM_FORMAT_SAMPLES_JSON)"
        )));
    }

    let Some(s) = value.as_str() else {
        return Err(ResolveError(format!(
            "unsupported value type for idx {index}: {value:?}"
        )));
    };
    let s_trim = s.trim();
    if s_trim.is_empty() {
        return Err(ResolveError(format!("empty string value for idx {index}")));
    }

    if let Some(v) = parse_bool_like(s_trim) {
        return Ok(v);
    }

    if index == param_map::selectors::AMP_TYPE_INDEX {
        if let Some(v) = resolve_amp_type(value) {
            return Ok(v);
        }
    }

    if let Some(enums) = enums {
        if let Some(v) = resolve_from_enum_label(enums, index, value) {
            return Ok(v.clamp(0.0, 1.0));
        }
    }

    // Percent values like "25%" for mixes.
    if s_trim.contains('%') {
        if let Some(v) = parse_percent(s_trim) {
            return Ok(v);
        }
    }

    // Allow dB specs when we have formatted samples for this param (best), else fallback EQ mapping.
    if s_trim.to_ascii_lowercase().contains("db") {
        // Common shorthand: "flat" / "0 dB"
        if s_trim.trim().eq_ignore_ascii_case("flat") {
            if (54..=82).contains(&index) {
                return Ok(0.5);
            }
        }
        if let Some(db) = parse_db(s_trim) {
            if let Some(samples) = samples.and_then(|m| m.get(&index)) {
                let mut pts: Vec<(f32, f32)> = Vec::new(); // (db, norm)
                for (norm, formatted) in samples {
                    if let Some(v) = parse_db_from_formatted(formatted) {
                        pts.push((v, *norm));
                    }
                }
                if let Some(norm) = invert_piecewise(&pts, db) {
                    return Ok(norm);
                }
            }
            if let Some(v) = resolve_eq_band_db(index, s_trim) {
                return Ok(v);
            }
            if let Some(triplets) = triplets {
                if let Some(norm) = invert_from_triplet_physical(triplets, index, db) {
                    return Ok(norm);
                }
            }
        }
    }

    // Time units (ms/s) - without calibration we can't map reliably, so accept normalized fallback.
    if let Some(ms) = parse_ms_value(s_trim) {
        if let Some(samples) = samples.and_then(|m| m.get(&index)) {
            let mut pts: Vec<(f32, f32)> = Vec::new(); // (ms, norm)
            for (norm, formatted) in samples {
                if let Some(v) = parse_ms_from_formatted(formatted) {
                    pts.push((v, *norm));
                }
            }
            if let Some(norm) = invert_piecewise(&pts, ms) {
                return Ok(norm);
            }
        }
        if let Some(triplets) = triplets {
            if let Some(norm) = invert_from_triplet_physical(triplets, index, ms) {
                return Ok(norm);
            }
        }
        return Err(ResolveError(format!(
            "time unit provided for idx {index} but no matching PARAM_FORMAT_SAMPLES_JSON mapping was found"
        )));
    }

    // Tempo units (bpm).
    if let Some(bpm) = parse_bpm_value(s_trim) {
        if let Some(samples) = samples {
            if let Some(norm) = invert_from_samples_physical(samples, index, bpm) {
                return Ok(norm);
            }
        }
        if let Some(triplets) = triplets {
            if let Some(norm) = invert_from_triplet_physical(triplets, index, bpm) {
                return Ok(norm);
            }
        }
        return Err(ResolveError(format!(
            "bpm unit provided for idx {index} but no calibration mapping was available"
        )));
    }

    // Frequency units (Hz/kHz), e.g. "150 Hz", "6.5 kHz" (commonly used for reverb cuts).
    if let Some(hz) = parse_hz_value(s_trim) {
        if let Some(samples) = samples {
            if let Some(norm) = invert_from_samples_physical(samples, index, hz) {
                return Ok(norm);
            }
        }
        if let Some(triplets) = triplets {
            if let Some(norm) = invert_from_triplet_physical(triplets, index, hz) {
                return Ok(norm);
            }
        }
        return Err(ResolveError(format!(
            "hz unit provided for idx {index} but no calibration mapping was available"
        )));
    }

    // If prompt included enums, suggest it in error.
    let has_enums = extract_prompt_json_line(prompt, "ENUM_OPTIONS_JSON=").is_some();
    if has_enums {
        Err(ResolveError(format!(
            "could not resolve string value for idx {index}: {s_trim:?} (try a known enum label or a 0..1 number)"
        )))
    } else {
        Err(ResolveError(format!(
            "could not resolve string value for idx {index}: {s_trim:?}"
        )))
    }
}

pub fn resolve_ai_params(
    original_prompt: &str,
    ai_params: Vec<AiParamChange>,
) -> Result<Vec<ParamChange>, ResolveError> {
    let enums = {
        let mut e = default_enum_options();
        if let Some(from_prompt) = parse_enum_options(original_prompt) {
            for (k, v) in from_prompt {
                e.insert(k, v);
            }
        }
        Some(e)
    };
    let samples = parse_format_samples(original_prompt);

    let triplets = {
        let mut t = default_formatted_value_triplets();
        if let Some(from_prompt) = parse_formatted_value_triplets(original_prompt) {
            for (k, v) in from_prompt {
                t.insert(k, v);
            }
        }
        Some(t)
    };

    let mut out: Vec<ParamChange> = Vec::with_capacity(ai_params.len());
    for p in ai_params {
        let v = resolve_value_for_index(
            original_prompt,
            enums.as_ref(),
            samples.as_ref(),
            triplets.as_ref(),
            p.index,
            &p.value,
        )?;
        out.push(ParamChange {
            index: p.index,
            value: v,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_db_uses_default_triplet() {
        let params = vec![AiParamChange {
            index: 2,
            value: serde_json::Value::String("-30 dB".to_string()),
        }];
        let out = resolve_ai_params("hi", params).unwrap();
        let v = out[0].value;
        // (-30 - -96) / (0 - -96) = 66/96 = 0.6875
        assert!((v - 0.6875).abs() < 1e-4, "got {v}");
    }

    #[test]
    fn tempo_bpm_uses_default_triplet() {
        let params = vec![AiParamChange {
            index: 108,
            value: serde_json::Value::String("120 bpm".to_string()),
        }];
        let out = resolve_ai_params("hi", params).unwrap();
        let v = out[0].value;
        // (120-40)/(240-40)=0.4
        assert!((v - 0.4).abs() < 1e-4, "got {v}");
    }

    #[test]
    fn numeric_physical_requires_mapping() {
        let params = vec![AiParamChange {
            index: 54,
            value: serde_json::Value::Number(650.into()),
        }];
        let err = resolve_ai_params("hi", params).unwrap_err();
        assert!(
            err.0.contains("not a normalized 0..1"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn pan_accepts_minus_one_to_one() {
        let n = serde_json::Number::from_f64(-0.5).unwrap();
        let params = vec![AiParamChange {
            index: 90,
            value: serde_json::Value::Number(n),
        }];
        let out = resolve_ai_params("hi", params).unwrap();
        let v = out[0].value;
        assert!((v - 0.25).abs() < 1e-6, "got {v}");
    }

    #[test]
    fn hz_strings_use_default_triplet() {
        let params = vec![AiParamChange {
            index: 116,
            value: serde_json::Value::String("150 Hz".to_string()),
        }];
        let out = resolve_ai_params("hi", params).unwrap();
        let v = out[0].value;
        // (150-50)/(700-50)=100/650
        assert!((v - (100.0 / 650.0)).abs() < 1e-4, "got {v}");
    }
}
