pub fn eq_db_to_unit(db: f32) -> f32 {
    ((db / 24.0) + 0.5).clamp(0.0, 1.0)
}

pub fn gain_db_to_unit(db: f32) -> f32 {
    ((db / 48.0) + 0.5).clamp(0.0, 1.0)
}

pub fn on_off(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}
