use crate::param_map;
use gojira_protocol::ParamChange;

#[derive(serde::Serialize, Debug, Clone)]
pub struct DiffItem {
    pub label: String,
    pub index: i32,
    pub old_value: Option<f32>,
    pub new_value: Option<f32>,
}

pub fn diff_params(old_params: &[ParamChange], new_params: &[ParamChange]) -> Vec<DiffItem> {
    use std::collections::HashMap;
    let old: HashMap<i32, f32> = old_params.iter().map(|p| (p.index, p.value)).collect();
    let new: HashMap<i32, f32> = new_params.iter().map(|p| (p.index, p.value)).collect();

    let mut keys: Vec<i32> = old.keys().chain(new.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();

    keys.into_iter()
        .filter_map(|idx| {
            let o = old.get(&idx).copied();
            let n = new.get(&idx).copied();
            if o == n {
                return None;
            }
            Some(DiffItem {
                label: label_for_index(idx).to_string(),
                index: idx,
                old_value: o,
                new_value: n,
            })
        })
        .collect()
}

fn label_for_index(index: i32) -> &'static str {
    match index {
        param_map::global::INPUT_GAIN => "Global: Input Gain",
        param_map::global::OUTPUT_GAIN => "Global: Output Gain",
        param_map::global::NOISE_GATE => "Global: Noise Gate",
        param_map::selectors::AMP_TYPE_INDEX => "Amp: Type Select",
        param_map::pedals::overdrive::ACTIVE => "Overdrive: Active",
        param_map::pedals::overdrive::DRIVE => "Overdrive: Drive",
        param_map::pedals::overdrive::TONE => "Overdrive: Tone",
        param_map::pedals::overdrive::LEVEL => "Overdrive: Level",
        param_map::pedals::delay::ACTIVE => "Delay: Active",
        param_map::pedals::delay::MIX => "Delay: Mix",
        param_map::pedals::delay::TIME => "Delay: Time",
        param_map::pedals::reverb::ACTIVE => "Reverb: Active",
        param_map::pedals::reverb::TIME => "Reverb: Time",
        param_map::pedals::reverb::LOW_CUT => "Reverb: Low Cut",
        param_map::pedals::reverb::HIGH_CUT => "Reverb: High Cut",
        _ => "Param",
    }
}

