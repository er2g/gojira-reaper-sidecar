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

fn parse_percent(s: &str) -> Option<f32> {
    let t = s.trim().trim_end_matches('%').trim();
    let v = t.parse::<f32>().ok()?;
    Some((v / 100.0).clamp(0.0, 1.0))
}

fn parse_db(s: &str) -> Option<f32> {
    // Accept "+3.2 dB", "-10db", "3db"
    let t = s.trim().to_ascii_lowercase();
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

fn resolve_value_for_index(
    prompt: &str,
    enums: Option<&std::collections::HashMap<i32, Vec<EnumOption>>>,
    samples: Option<&std::collections::HashMap<i32, Vec<(f32, String)>>>,
    index: i32,
    value: &serde_json::Value,
) -> Result<f32, ResolveError> {
    // Numbers still work (and numeric strings).
    if let Some(v) = parse_numeric_value(value) {
        if v > 1.0 {
            // Allow 0..100 for percent-like values.
            if v <= 100.0 {
                return Ok((v / 100.0).clamp(0.0, 1.0));
            }
        }
        return Ok(v.clamp(0.0, 1.0));
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
        }
    }

    // Time units (ms/s) - without calibration we can't map reliably, so accept normalized fallback.
    if let Some((_n, _u)) = parse_ms_or_s(s_trim) {
        return Err(ResolveError(format!(
            "time unit provided for idx {index} but no calibration is available; use 0..1 for now"
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
    let enums = parse_enum_options(original_prompt);
    let samples = parse_format_samples(original_prompt);

    let mut out: Vec<ParamChange> = Vec::with_capacity(ai_params.len());
    for p in ai_params {
        let v = resolve_value_for_index(
            original_prompt,
            enums.as_ref(),
            samples.as_ref(),
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
