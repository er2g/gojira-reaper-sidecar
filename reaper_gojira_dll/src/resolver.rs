use crate::protocol::{Confidence, GojiraInstance};
use crate::reaper_api::ReaperApi;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;

pub type FxLookup = HashMap<String, (String, i32)>;

fn trace_enabled() -> bool {
    matches!(
        std::env::var("GOJIRA_DLL_TRACE_SCAN").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn trace_line(msg: &str) {
    if !trace_enabled() {
        return;
    }
    let path = std::env::temp_dir().join("reaper_gojira_dll_scan.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg);
        let _ = f.flush();
    }
}

pub fn scan_project_instances(api: &dyn ReaperApi) -> (Vec<GojiraInstance>, FxLookup) {
    let mut instances = Vec::new();
    let mut lookup: FxLookup = HashMap::new();

    let track_count = api.count_tracks();
    trace_line(&format!("scan: track_count={track_count}"));
    for ti in 0..track_count {
        let Some(track) = api.get_track(ti) else { continue };
        let Some(track_guid) = api.track_guid(track) else { continue };
        let track_name = api.track_name(track);

        let fx_count = api.track_fx_count(track);
        trace_line(&format!(
            "scan: track[{ti}] name='{}' guid='{}' fx_count={}",
            track_name, track_guid, fx_count
        ));
        for fxi in 0..fx_count {
            let fx_name = api.track_fx_name(track, fxi);
            trace_line(&format!("scan: track[{ti}] fx[{fxi}] name='{}'", fx_name));
            let Some(confidence) = gojira_confidence(&fx_name) else {
                continue;
            };
            let Some(fx_guid) = api.track_fx_guid(track, fxi) else { continue };
            trace_line(&format!(
                "scan: MATCH track[{ti}] fx[{fxi}] guid='{}' confidence={:?}",
                fx_guid, confidence
            ));

            lookup.insert(fx_guid.clone(), (track_guid.clone(), fxi));
            instances.push(GojiraInstance {
                track_guid: track_guid.clone(),
                track_name: track_name.clone(),
                fx_guid,
                fx_name,
                last_known_fx_index: fxi,
                confidence,
            });
        }
    }

    (instances, lookup)
}

pub fn resolve_fx(
    api: &dyn ReaperApi,
    cache: &mut FxLookup,
    target_fx_guid: &str,
) -> Result<(usize, i32), ResolveError> {
    if let Some((track_guid, fx_index)) = cache.get(target_fx_guid).cloned() {
        if let Some(track) = find_track_by_guid(api, &track_guid) {
            if verify_fx_guid(api, track, fx_index, target_fx_guid) {
                return Ok((track, fx_index));
            }
        }
    }

    let (_instances, fresh) = scan_project_instances(api);
    *cache = fresh;

    if let Some((track_guid, fx_index)) = cache.get(target_fx_guid).cloned() {
        let Some(track) = find_track_by_guid(api, &track_guid) else {
            return Err(ResolveError::TargetNotFound);
        };
        if verify_fx_guid(api, track, fx_index, target_fx_guid) {
            return Ok((track, fx_index));
        }
    }

    Err(ResolveError::TargetNotFound)
}

pub fn find_track_by_guid(api: &dyn ReaperApi, track_guid: &str) -> Option<usize> {
    let track_count = api.count_tracks();
    for ti in 0..track_count {
        let Some(track) = api.get_track(ti) else { continue };
        if api.track_guid(track).as_deref() == Some(track_guid) {
            return Some(track);
        }
    }
    None
}

fn verify_fx_guid(api: &dyn ReaperApi, track: usize, fx_index: i32, target_fx_guid: &str) -> bool {
    api.track_fx_guid(track, fx_index)
        .as_deref()
        .is_some_and(|g| g == target_fx_guid)
}

fn gojira_confidence(fx_name: &str) -> Option<Confidence> {
    let n = normalize(fx_name);
    if n.contains("archetype") && n.contains("gojira") {
        return Some(Confidence::High);
    }
    if n.contains("gojira") {
        return Some(Confidence::Low);
    }
    None
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub enum ResolveError {
    TargetNotFound,
}
