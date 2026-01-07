use crate::reaper_api::ReaperApi;
use std::collections::HashMap;

const DELAY_ACTIVE_ANCHOR: i32 = 101;
const REVERB_ACTIVE_ANCHOR: i32 = 112;

pub fn validate_parameter_map(
    api: &dyn ReaperApi,
    track: usize,
    fx_index: i32,
) -> HashMap<String, String> {
    let mut report = HashMap::new();

    report.insert(
        "delay_active".to_string(),
        anchor_report(api, track, fx_index, DELAY_ACTIVE_ANCHOR),
    );
    report.insert(
        "reverb_active".to_string(),
        anchor_report(api, track, fx_index, REVERB_ACTIVE_ANCHOR),
    );

    report.extend(validate_mix(api, track, fx_index));
    report
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

fn validate_mix(api: &dyn ReaperApi, track: usize, fx_index: i32) -> HashMap<String, String> {
    let mut report = HashMap::new();
    let delay_mix = pick_mix_near(api, track, fx_index, DELAY_ACTIVE_ANCHOR, 100..=115);
    let reverb_mix = pick_mix_near(api, track, fx_index, REVERB_ACTIVE_ANCHOR, 110..=125);

    report.insert(
        "delay_mix".to_string(),
        delay_mix
            .map(|(idx, name)| format!("confirmed at {idx} (neighbor of {DELAY_ACTIVE_ANCHOR}): {name}"))
            .unwrap_or_else(|| "not found".to_string()),
    );
    report.insert(
        "reverb_mix".to_string(),
        reverb_mix
            .map(|(idx, name)| format!("confirmed at {idx} (neighbor of {REVERB_ACTIVE_ANCHOR}): {name}"))
            .unwrap_or_else(|| "not found".to_string()),
    );
    report
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
        if normalize(&name).contains("mix") {
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
