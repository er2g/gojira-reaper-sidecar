use reaper_low::raw::MediaTrack;
use reaper_low::Reaper;
use std::ffi::CStr;
use std::os::raw::c_char;

pub trait ReaperApi {
    fn project_state_change_count(&self) -> i32;
    fn count_tracks(&self) -> i32;
    fn get_track(&self, index: i32) -> Option<usize>;
    fn track_guid(&self, track: usize) -> Option<String>;
    fn track_name(&self, track: usize) -> String;

    fn track_fx_count(&self, track: usize) -> i32;
    fn track_fx_num_params(&self, track: usize, fx_index: i32) -> Option<i32>;
    fn track_fx_guid(&self, track: usize, fx_index: i32) -> Option<String>;
    fn track_fx_name(&self, track: usize, fx_index: i32) -> String;
    fn track_fx_param_name(&self, track: usize, fx_index: i32, param_index: i32)
        -> Option<String>;
    fn track_fx_format_param_value(
        &self,
        track: usize,
        fx_index: i32,
        param_index: i32,
        value: f32,
    ) -> Option<String>;
    fn track_fx_set_param(
        &self,
        track: usize,
        fx_index: i32,
        param_index: i32,
        value: f32,
    ) -> Result<(), String>;
}

#[derive(Clone, Copy)]
pub struct ReaperApiImpl {
    reaper: Reaper,
}

impl ReaperApiImpl {
    pub fn new(reaper: Reaper) -> Self {
        Self { reaper }
    }

    fn to_track_ptr(track: usize) -> *mut MediaTrack {
        track as *mut MediaTrack
    }

    fn c_buf_to_string(buf: &[c_char]) -> String {
        // `buf` is expected to be NUL-terminated on success.
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .to_string()
    }
}

impl ReaperApi for ReaperApiImpl {
    fn project_state_change_count(&self) -> i32 {
        unsafe { self.reaper.GetProjectStateChangeCount(std::ptr::null_mut()) }
    }

    fn count_tracks(&self) -> i32 {
        unsafe { self.reaper.CountTracks(std::ptr::null_mut()) }
    }

    fn get_track(&self, index: i32) -> Option<usize> {
        let track = unsafe { self.reaper.GetTrack(std::ptr::null_mut(), index) };
        if track.is_null() {
            None
        } else {
            Some(track as usize)
        }
    }

    fn track_guid(&self, track: usize) -> Option<String> {
        let guid = unsafe { self.reaper.GetTrackGUID(Self::to_track_ptr(track)) };
        if guid.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 64];
        unsafe {
            self.reaper.guidToString(guid, buf.as_mut_ptr());
        }
        Some(Self::c_buf_to_string(&buf))
    }

    fn track_name(&self, track: usize) -> String {
        let mut buf = [0 as c_char; 256];
        let ok = unsafe {
            self.reaper
                .GetTrackName(Self::to_track_ptr(track), buf.as_mut_ptr(), buf.len() as i32)
        };
        if ok {
            Self::c_buf_to_string(&buf)
        } else {
            String::new()
        }
    }

    fn track_fx_count(&self, track: usize) -> i32 {
        unsafe { self.reaper.TrackFX_GetCount(Self::to_track_ptr(track)) }
    }

    fn track_fx_num_params(&self, track: usize, fx_index: i32) -> Option<i32> {
        let n = unsafe { self.reaper.TrackFX_GetNumParams(Self::to_track_ptr(track), fx_index) };
        if n <= 0 {
            None
        } else {
            Some(n)
        }
    }

    fn track_fx_guid(&self, track: usize, fx_index: i32) -> Option<String> {
        let guid =
            unsafe { self.reaper.TrackFX_GetFXGUID(Self::to_track_ptr(track), fx_index) };
        if guid.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 64];
        unsafe {
            self.reaper.guidToString(guid, buf.as_mut_ptr());
        }
        Some(Self::c_buf_to_string(&buf))
    }

    fn track_fx_name(&self, track: usize, fx_index: i32) -> String {
        let mut buf = [0 as c_char; 512];
        let ok = unsafe {
            self.reaper.TrackFX_GetFXName(
                Self::to_track_ptr(track),
                fx_index,
                buf.as_mut_ptr(),
                buf.len() as i32,
            )
        };
        if ok {
            Self::c_buf_to_string(&buf)
        } else {
            String::new()
        }
    }

    fn track_fx_param_name(
        &self,
        track: usize,
        fx_index: i32,
        param_index: i32,
    ) -> Option<String> {
        let mut buf = [0 as c_char; 256];
        let ok = unsafe {
            self.reaper.TrackFX_GetParamName(
                Self::to_track_ptr(track),
                fx_index,
                param_index,
                buf.as_mut_ptr(),
                buf.len() as i32,
            )
        };
        if ok {
            Some(Self::c_buf_to_string(&buf))
        } else {
            None
        }
    }

    fn track_fx_format_param_value(
        &self,
        track: usize,
        fx_index: i32,
        param_index: i32,
        value: f32,
    ) -> Option<String> {
        let mut buf = [0 as c_char; 256];
        let ok = unsafe {
            self.reaper.TrackFX_FormatParamValue(
                Self::to_track_ptr(track),
                fx_index,
                param_index,
                value as f64,
                buf.as_mut_ptr(),
                buf.len() as i32,
            )
        };
        if ok {
            Some(Self::c_buf_to_string(&buf))
        } else {
            None
        }
    }

    fn track_fx_set_param(
        &self,
        track: usize,
        fx_index: i32,
        param_index: i32,
        value: f32,
    ) -> Result<(), String> {
        let ok = unsafe {
            self.reaper.TrackFX_SetParam(
                Self::to_track_ptr(track),
                fx_index,
                param_index,
                value as f64,
            )
        };
        if ok {
            Ok(())
        } else {
            Err("TrackFX_SetParam returned false".to_string())
        }
    }
}
