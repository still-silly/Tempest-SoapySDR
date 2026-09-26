//! Minimal, safe ownership wrapper for the TempestSDR C API.
//!
//! Streaming and plugin ownership will be added after the desktop frame path
//! has been validated. Keeping the raw ABI private prevents UI code from
//! accidentally retaining native buffers after their callback returns.

use std::{
    ffi::{CStr, CString},
    fmt,
    marker::PhantomData,
    os::raw::{c_char, c_double, c_float, c_int, c_void},
    ptr::{self, NonNull},
    rc::Rc,
};

const TSDR_OK: c_int = 0;

#[repr(C)]
struct TsdrLib {
    _private: [u8; 0],
}

type ValueChangedCallback = unsafe extern "C" fn(c_int, c_double, c_double, *mut c_void);
type PlotReadyCallback = unsafe extern "C" fn(c_int, c_int, *mut c_double, c_int, u32, *mut c_void);

unsafe extern "C" {
    fn tsdr_init(
        tsdr: *mut *mut TsdrLib,
        value_callback: Option<ValueChangedCallback>,
        plot_callback: Option<PlotReadyCallback>,
        context: *mut c_void,
    );
    fn tsdr_setbasefreq(tsdr: *mut TsdrLib, frequency: u32) -> c_int;
    fn tsdr_setgain(tsdr: *mut TsdrLib, gain: c_float) -> c_int;
    fn tsdr_setresolution(tsdr: *mut TsdrLib, height: c_int, refresh_rate: c_double) -> c_int;
    fn tsdr_loadplugin(
        tsdr: *mut TsdrLib,
        plugin_path: *const c_char,
        parameters: *const c_char,
    ) -> c_int;
    fn tsdr_unloadplugin(tsdr: *mut TsdrLib) -> c_int;
    fn tsdr_stop(tsdr: *mut TsdrLib) -> c_int;
    fn tsdr_isrunning(tsdr: *mut TsdrLib) -> c_int;
    fn tsdr_getlasterrortext(tsdr: *mut TsdrLib) -> *const c_char;
    fn tsdr_free(tsdr: *mut *mut TsdrLib);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsdrError {
    pub code: i32,
    pub message: String,
}

impl fmt::Display for TsdrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} (TempestSDR error {})",
            self.message, self.code
        )
    }
}

impl std::error::Error for TsdrError {}

/// Owns one `tsdr_lib_t` instance.
///
/// The native library has mutable process callbacks, so this first wrapper is
/// intentionally neither `Send` nor `Sync`. The future streaming controller
/// will own it on a dedicated worker thread and communicate through channels.
pub struct TsdrEngine {
    raw: NonNull<TsdrLib>,
    _thread_confined: PhantomData<Rc<()>>,
}

impl TsdrEngine {
    pub fn new() -> Result<Self, TsdrError> {
        let mut raw = ptr::null_mut();
        unsafe { tsdr_init(&mut raw, None, None, ptr::null_mut()) };

        let raw = NonNull::new(raw).ok_or_else(|| TsdrError {
            code: -1,
            message: "TempestSDR could not allocate an engine".into(),
        })?;

        Ok(Self {
            raw,
            _thread_confined: PhantomData,
        })
    }

    pub fn set_display_mode(&mut self, height: u32, refresh_rate: f64) -> Result<(), TsdrError> {
        let height = i32::try_from(height).map_err(|_| TsdrError {
            code: -1,
            message: "Display height does not fit the native API".into(),
        })?;
        let status = unsafe { tsdr_setresolution(self.raw.as_ptr(), height, refresh_rate) };
        self.status(status)
    }

    pub fn set_frequency(&mut self, frequency_hz: u32) -> Result<(), TsdrError> {
        let status = unsafe { tsdr_setbasefreq(self.raw.as_ptr(), frequency_hz) };
        self.status(status)
    }

    pub fn set_gain(&mut self, gain: f32) -> Result<(), TsdrError> {
        let status = unsafe { tsdr_setgain(self.raw.as_ptr(), gain) };
        self.status(status)
    }

    pub fn load_plugin(&mut self, path: &str, parameters: &str) -> Result<(), TsdrError> {
        let path = CString::new(path).map_err(|_| TsdrError {
            code: -1,
            message: "Plugin path contains a NUL byte".into(),
        })?;
        let parameters = CString::new(parameters).map_err(|_| TsdrError {
            code: -1,
            message: "Plugin parameters contain a NUL byte".into(),
        })?;
        let status =
            unsafe { tsdr_loadplugin(self.raw.as_ptr(), path.as_ptr(), parameters.as_ptr()) };
        self.status(status)
    }

    pub fn unload_plugin(&mut self) -> Result<(), TsdrError> {
        let status = unsafe { tsdr_unloadplugin(self.raw.as_ptr()) };
        self.status(status)
    }

    pub fn stop(&mut self) -> Result<(), TsdrError> {
        let status = unsafe { tsdr_stop(self.raw.as_ptr()) };
        self.status(status)
    }

    pub fn is_running(&self) -> bool {
        unsafe { tsdr_isrunning(self.raw.as_ptr()) != 0 }
    }

    fn status(&self, status: c_int) -> Result<(), TsdrError> {
        if status == TSDR_OK {
            return Ok(());
        }

        let message = unsafe {
            let error = tsdr_getlasterrortext(self.raw.as_ptr());
            if error.is_null() {
                "Unknown TempestSDR error".into()
            } else {
                CStr::from_ptr(error).to_string_lossy().into_owned()
            }
        };

        Err(TsdrError {
            code: status,
            message,
        })
    }
}

impl Drop for TsdrEngine {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }

        let mut raw = self.raw.as_ptr();
        unsafe { tsdr_free(&mut raw) };
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayTiming {
    pub name: &'static str,
    pub active_width: u32,
    pub active_height: u32,
    pub horizontal_total: u32,
    pub vertical_total: u32,
    pub pixel_clock_hz: u64,
    pub refresh_rate_hz: f64,
}

impl DisplayTiming {
    pub const DELL_2407WFP_1920X1200: Self = Self {
        name: "Dell 2407WFP — 1920×1200 @ 59.95 Hz",
        active_width: 1920,
        active_height: 1200,
        horizontal_total: 2080,
        vertical_total: 1235,
        pixel_clock_hz: 154_000_000,
        refresh_rate_hz: 154_000_000.0 / (2080.0 * 1235.0),
    };

    pub fn line_rate_hz(self) -> f64 {
        self.pixel_clock_hz as f64 / self.horizontal_total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dell_profile_matches_edid_timing() {
        let timing = DisplayTiming::DELL_2407WFP_1920X1200;
        assert_eq!(timing.horizontal_total, 1920 + 48 + 32 + 80);
        assert_eq!(timing.vertical_total, 1200 + 3 + 6 + 26);
        assert!((timing.refresh_rate_hz - 59.950_171).abs() < 0.000_001);
        assert!((timing.line_rate_hz() - 74_038.46).abs() < 0.1);
    }

    #[test]
    fn engine_can_be_created_configured_and_freed() {
        let mut engine = TsdrEngine::new().expect("engine should initialize");
        engine
            .set_display_mode(1200, DisplayTiming::DELL_2407WFP_1920X1200.refresh_rate_hz)
            .expect("display mode should be accepted");
        engine
            .set_frequency(154_000_000)
            .expect("frequency should be accepted");
        engine.set_gain(0.5).expect("gain should be accepted");
        assert!(!engine.is_running());
    }
}
