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
    slice,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
};

const TSDR_OK: c_int = 0;

#[repr(C)]
struct TsdrLib {
    _private: [u8; 0],
}

type ValueChangedCallback = unsafe extern "C" fn(c_int, c_double, c_double, *mut c_void);
type PlotReadyCallback = unsafe extern "C" fn(c_int, c_int, *mut c_double, c_int, u32, *mut c_void);
type ReadAsyncCallback = unsafe extern "C" fn(*mut c_float, c_int, c_int, *mut c_void);

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
    fn tsdr_readasync(
        tsdr: *mut TsdrLib,
        callback: Option<ReadAsyncCallback>,
        context: *mut c_void,
    ) -> c_int;
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

    fn read_async(&mut self, sender: SyncSender<CaptureEvent>) -> Result<(), TsdrError> {
        let context = FrameCallbackContext { sender };
        let status = unsafe {
            tsdr_readasync(
                self.raw.as_ptr(),
                Some(frame_callback),
                (&context as *const FrameCallbackContext).cast_mut().cast(),
            )
        };
        self.status(status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGB888 pixels.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub plugin_path: String,
    pub plugin_parameters: String,
    pub frequency_hz: u32,
    pub gain: f32,
    pub display_timing: DisplayTiming,
}

#[derive(Debug)]
pub enum CaptureEvent {
    Starting,
    Running,
    Frame(RgbFrame),
    Stopped(Result<(), TsdrError>),
}

struct FrameCallbackContext {
    sender: SyncSender<CaptureEvent>,
}

unsafe extern "C" fn frame_callback(
    buffer: *mut c_float,
    width: c_int,
    height: c_int,
    context: *mut c_void,
) {
    if buffer.is_null() || context.is_null() || width <= 0 || height <= 0 {
        return;
    }

    let Ok(width) = u32::try_from(width) else {
        return;
    };
    let Ok(height) = u32::try_from(height) else {
        return;
    };
    let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
        return;
    };
    let Some(byte_count) = pixel_count.checked_mul(3) else {
        return;
    };

    // A corrupt plugin or timing calculation must not request an unbounded
    // allocation from a native callback.
    if byte_count > 256 * 1024 * 1024 {
        return;
    }

    let samples = unsafe { slice::from_raw_parts(buffer, pixel_count) };
    let frame = frame_from_samples(width, height, samples);
    let context = unsafe { &*(context.cast::<FrameCallbackContext>()) };

    // Rendering is intentionally lossy: if the UI is behind, retain native
    // capture throughput and discard the stale frame.
    match context.sender.try_send(CaptureEvent::Frame(frame)) {
        Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

fn frame_from_samples(width: u32, height: u32, samples: &[f32]) -> RgbFrame {
    let mut pixels = Vec::with_capacity(samples.len() * 3);
    for &value in samples {
        let color = if value > 0.0 && value <= 1.0 {
            let gray = (value * 255.0) as u8;
            [gray, gray, gray]
        } else if value <= 0.0 {
            [0, 0, 0]
        } else if value == 256.0 {
            [255, 0, 0]
        } else if value == 512.0 {
            [0, 255, 0]
        } else if value == 1024.0 {
            [0, 0, 255]
        } else {
            [255, 255, 255]
        };
        pixels.extend_from_slice(&color);
    }

    RgbFrame {
        width,
        height,
        pixels,
    }
}

struct StopState {
    native: Mutex<*mut TsdrLib>,
    requested: AtomicBool,
}

// The pointer is only read while holding `native`. Its pointee remains owned
// by the capture worker until the pointer is cleared under the same lock.
unsafe impl Send for StopState {}
unsafe impl Sync for StopState {}

impl StopState {
    fn request(&self) {
        self.requested.store(true, Ordering::Release);
        let native = self.native.lock().expect("capture stop mutex poisoned");
        if !native.is_null() {
            unsafe {
                tsdr_stop(*native);
            }
        }
    }
}

pub struct CaptureSession {
    stop: Arc<StopState>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureSession {
    pub fn start(config: CaptureConfig) -> (Self, Receiver<CaptureEvent>) {
        // Two slots provide one frame under presentation and one pending frame.
        // The callback drops additional frames instead of growing memory.
        let (sender, receiver) = sync_channel(2);
        let stop = Arc::new(StopState {
            native: Mutex::new(ptr::null_mut()),
            requested: AtomicBool::new(false),
        });
        let worker_stop = Arc::clone(&stop);

        let worker = thread::spawn(move || {
            let _ = sender.send(CaptureEvent::Starting);
            let result = run_capture(config, &sender, &worker_stop);
            let _ = sender.send(CaptureEvent::Stopped(result));
        });

        (
            Self {
                stop,
                worker: Some(worker),
            },
            receiver,
        )
    }

    pub fn request_stop(&self) {
        self.stop.request();
    }

    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn join(mut self) -> thread::Result<()> {
        self.request_stop();
        match self.worker.take() {
            Some(worker) => worker.join(),
            None => Ok(()),
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn run_capture(
    config: CaptureConfig,
    sender: &SyncSender<CaptureEvent>,
    stop: &StopState,
) -> Result<(), TsdrError> {
    let mut engine = TsdrEngine::new()?;
    {
        let mut native = stop.native.lock().expect("capture stop mutex poisoned");
        *native = engine.raw.as_ptr();
    }

    let result = (|| {
        engine.set_display_mode(
            config.display_timing.active_height,
            config.display_timing.refresh_rate_hz,
        )?;
        engine.set_frequency(config.frequency_hz)?;
        engine.set_gain(config.gain)?;
        engine.load_plugin(&config.plugin_path, &config.plugin_parameters)?;

        if stop.requested.load(Ordering::Acquire) {
            return Ok(());
        }

        let _ = sender.send(CaptureEvent::Running);
        engine.read_async(sender.clone())
    })();

    {
        let mut native = stop.native.lock().expect("capture stop mutex poisoned");
        *native = ptr::null_mut();
    }
    result
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

    #[test]
    fn native_frame_values_are_converted_to_rgb() {
        let frame = frame_from_samples(3, 2, &[-1.0, 0.5, 2.0, 256.0, 512.0, 1024.0]);
        assert_eq!(frame.width, 3);
        assert_eq!(frame.height, 2);
        assert_eq!(
            frame.pixels,
            [
                0, 0, 0, 127, 127, 127, 255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255,
            ]
        );
    }

    #[test]
    fn capture_reports_plugin_load_failure_without_hardware() {
        let config = CaptureConfig {
            plugin_path: "/definitely/not/a/tempest/plugin.so".into(),
            plugin_parameters: String::new(),
            frequency_hz: 154_000_000,
            gain: 0.5,
            display_timing: DisplayTiming::DELL_2407WFP_1920X1200,
        };
        let (session, events) = CaptureSession::start(config);
        assert!(matches!(events.recv().unwrap(), CaptureEvent::Starting));
        let stopped = events.recv().unwrap();
        assert!(matches!(stopped, CaptureEvent::Stopped(Err(_))));
        session.join().unwrap();
    }
}
