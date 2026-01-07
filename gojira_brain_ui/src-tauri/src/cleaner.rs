use crate::param_map;
use gojira_protocol::{MergeMode, ParamChange};
use std::collections::HashSet;

#[derive(Clone, Copy)]
struct ModuleDef {
    bypass: &'static [i32],
    params: &'static [i32],
}

const MODULES: &[ModuleDef] = &[
    ModuleDef {
        bypass: &[param_map::pedals::wow_pitch::PEDAL_SWITCH, param_map::pedals::wow_pitch::ACTIVE],
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
            param_map::pedals::delay::TIME,
        ],
    },
    ModuleDef {
        bypass: &[param_map::pedals::reverb::ACTIVE],
        params: &[
            param_map::pedals::reverb::ACTIVE,
            param_map::pedals::reverb::TIME,
            param_map::pedals::reverb::LOW_CUT,
            param_map::pedals::reverb::HIGH_CUT,
        ],
    },
    ModuleDef {
        bypass: &[param_map::cab::ACTIVE],
        params: &[
            param_map::cab::ACTIVE,
            param_map::cab::TYPE_SELECTOR,
            param_map::cab::mic1::POS,
            param_map::cab::mic1::DIST,
            param_map::cab::mic1::LEVEL,
            param_map::cab::mic1::IR_SEL,
            param_map::cab::mic2::POS,
            param_map::cab::mic2::DIST,
            param_map::cab::mic2::LEVEL,
            param_map::cab::mic2::IR_SEL,
        ],
    },
];

pub fn apply_replace_active_cleaner(mode: MergeMode, params: Vec<ParamChange>) -> Vec<ParamChange> {
    if !matches!(mode, MergeMode::ReplaceActive) {
        return params;
    }

    let touched_modules: HashSet<usize> = MODULES
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            params.iter().any(|p| {
                m.params.contains(&p.index) && !m.bypass.contains(&p.index)
            })
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
