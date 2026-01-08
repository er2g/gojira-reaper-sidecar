use crate::modules::param_map;
use crate::modules::protocol::{MergeMode, ParamChange};
use std::collections::HashSet;

#[derive(Clone, Copy)]
struct ModuleDef {
    bypass: &'static [i32],
    params: &'static [i32],
}

const MODULES: &[ModuleDef] = &[
    ModuleDef {
        bypass: &[
            param_map::pedals::wow_pitch::PEDAL_SWITCH,
            param_map::pedals::wow_pitch::ACTIVE,
        ],
        params: &[
            param_map::pedals::wow_pitch::PEDAL_SWITCH,
            param_map::pedals::wow_pitch::ACTIVE,
            param_map::pedals::wow_pitch::PITCH_VAL,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::octaver::ACTIVE],
        params: &[
            param_map::pedals::octaver::ACTIVE,
            param_map::pedals::octaver::OCT1,
            param_map::pedals::octaver::OCT2,
            param_map::pedals::octaver::DIRECT,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::overdrive::ACTIVE],
        params: &[
            param_map::pedals::overdrive::ACTIVE,
            param_map::pedals::overdrive::DRIVE,
            param_map::pedals::overdrive::TONE,
            param_map::pedals::overdrive::LEVEL,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::distortion::ACTIVE],
        params: &[
            param_map::pedals::distortion::ACTIVE,
            param_map::pedals::distortion::DIST,
            param_map::pedals::distortion::FILTER,
            param_map::pedals::distortion::VOL,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::phaser::ACTIVE],
        params: &[param_map::pedals::phaser::ACTIVE, param_map::pedals::phaser::RATE],
    },
    ModuleDef {
        bypass: &[param_map::pedals::chorus::ACTIVE],
        params: &[
            param_map::pedals::chorus::ACTIVE,
            param_map::pedals::chorus::RATE,
            param_map::pedals::chorus::DEPTH,
            param_map::pedals::chorus::MIX,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::delay::ACTIVE],
        params: &[
            param_map::pedals::delay::ACTIVE,
            param_map::pedals::delay::MIX,
            param_map::pedals::delay::FEEDBACK,
            param_map::pedals::delay::TIME,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::reverb::ACTIVE],
        params: &[
            param_map::pedals::reverb::ACTIVE,
            param_map::pedals::reverb::MIX,
            param_map::pedals::reverb::TIME,
            param_map::pedals::reverb::LOW_CUT,
            param_map::pedals::reverb::HIGH_CUT,
        ],
    },
];

pub fn sanitize_params(params: Vec<ParamChange>) -> Result<Vec<ParamChange>, String> {
    const MAX_PARAM_INDEX: i32 = 4096;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in params.into_iter().rev() {
        if !seen.insert(p.index) {
            continue;
        }
        if p.index < 0 || p.index > MAX_PARAM_INDEX {
            return Err(format!("invalid param index: {}", p.index));
        }
        if !p.value.is_finite() {
            return Err(format!("non-finite value at index {}", p.index));
        }
        out.push(ParamChange {
            index: p.index,
            value: p.value.clamp(0.0, 1.0),
        });
    }
    out.reverse();
    Ok(out)
}

pub fn apply_replace_active_cleaner(mode: MergeMode, params: Vec<ParamChange>) -> Vec<ParamChange> {
    if !matches!(mode, MergeMode::ReplaceActive) {
        return params;
    }

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

    // Dependency inference: if the model adjusts a section's parameters, ensure the section toggle
    // is present too. This doesn't override explicit user/model choices (only adds when missing).
    fn ensure(out: &mut Vec<ParamChange>, already_set: &mut HashSet<i32>, index: i32, value: f32) {
        if already_set.insert(index) {
            out.push(ParamChange { index, value });
        }
    }

    let touches_any = |v: &[ParamChange], start: i32, end: i32| {
        v.iter().any(|p| (start..=end).contains(&p.index))
    };

    let has_any_eq = touches_any(&out, 53, 82);
    let has_clean_eq = touches_any(&out, 54, 62) || already_set.contains(&53);
    let has_rust_eq = touches_any(&out, 64, 72) || already_set.contains(&63);
    let has_hot_eq = touches_any(&out, 74, 82) || already_set.contains(&73);

    let has_any_cab = touches_any(&out, 84, 99);
    let has_cab1 = touches_any(&out, 87, 92) || already_set.contains(&86);
    let has_cab2 = touches_any(&out, 94, 99) || already_set.contains(&93);

    if has_any_eq {
        ensure(&mut out, &mut already_set, 52, 1.0);
    }
    if has_clean_eq {
        ensure(&mut out, &mut already_set, 53, 1.0);
    }
    if has_rust_eq {
        ensure(&mut out, &mut already_set, 63, 1.0);
    }
    if has_hot_eq {
        ensure(&mut out, &mut already_set, 73, 1.0);
    }

    if has_any_cab {
        ensure(&mut out, &mut already_set, 83, 1.0);
    }
    if has_cab1 {
        ensure(&mut out, &mut already_set, 86, 1.0);
    }
    if has_cab2 {
        ensure(&mut out, &mut already_set, 93, 1.0);
    }

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

