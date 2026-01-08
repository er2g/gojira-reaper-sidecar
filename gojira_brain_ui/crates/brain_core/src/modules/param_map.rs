pub mod global {
    pub const INPUT_GAIN: i32 = 0;
    pub const OUTPUT_GAIN: i32 = 1;
    pub const NOISE_GATE: i32 = 2;
}

pub mod selectors {
    pub const AMP_TYPE_INDEX: i32 = 29;
}

pub mod pedals {
    pub mod wow_pitch {
        pub const PEDAL_SWITCH: i32 = 3;
        pub const ACTIVE: i32 = 4;
        pub const PITCH_VAL: i32 = 6;
    }

    pub mod octaver {
        pub const ACTIVE: i32 = 8;
        pub const OCT1: i32 = 9;
        pub const OCT2: i32 = 10;
        pub const DIRECT: i32 = 11;
    }

    pub mod overdrive {
        pub const ACTIVE: i32 = 13;
        pub const DRIVE: i32 = 14;
        pub const TONE: i32 = 15;
        pub const LEVEL: i32 = 16;
    }

    pub mod distortion {
        pub const ACTIVE: i32 = 17;
        pub const DIST: i32 = 18;
        pub const FILTER: i32 = 19;
        pub const VOL: i32 = 20;
    }

    pub mod phaser {
        pub const ACTIVE: i32 = 21;
        pub const RATE: i32 = 22;
    }

    pub mod chorus {
        pub const ACTIVE: i32 = 23;
        pub const RATE: i32 = 24;
        pub const DEPTH: i32 = 25;
        pub const MIX: i32 = 27;
    }

    pub mod delay {
        pub const ACTIVE: i32 = 101;
        pub const FEEDBACK: i32 = 106;
        pub const MIX: i32 = 105;
        pub const TIME: i32 = 108;
    }

    pub mod reverb {
        pub const ACTIVE: i32 = 112;
        pub const MIX: i32 = 114;
        pub const TIME: i32 = 115;
        pub const LOW_CUT: i32 = 116;
        pub const HIGH_CUT: i32 = 117;
    }
}

pub mod cab {
    pub const ACTIVE: i32 = 83;
    pub const TYPE_SELECTOR: i32 = 84;

    pub mod mic1 {
        pub const POS: i32 = 87;
        pub const DIST: i32 = 88;
        pub const LEVEL: i32 = 89;
        pub const IR_SEL: i32 = 92;
    }

    pub mod mic2 {
        pub const POS: i32 = 94;
        pub const DIST: i32 = 95;
        pub const LEVEL: i32 = 96;
        pub const IR_SEL: i32 = 99;
    }
}

