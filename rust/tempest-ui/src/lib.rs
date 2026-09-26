use std::{cell::RefCell, path::PathBuf, rc::Rc, thread};

use slint::{ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer};
use tempest_tsdr::{CaptureConfig, CaptureEvent, CaptureSession, DisplayTiming, TsdrEngine};

slint::include_modules!();

#[cfg(target_os = "android")]
mod android_usb;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let timing = DisplayTiming::DELL_2407WFP_1920X1200;
    let mut engine = TsdrEngine::new()?;
    engine.set_display_mode(timing.active_height, timing.refresh_rate_hz)?;

    let ui = AppWindow::new()?;
    ui.set_video_frame(make_test_frame(960, 600));
    ui.set_display_name(timing.name.into());
    ui.set_timing_summary(
        format!(
            "{}×{} active · {}×{} total · {:.6} Hz · {:.3} kHz line rate · {:.3} MHz pixel clock",
            timing.active_width,
            timing.active_height,
            timing.horizontal_total,
            timing.vertical_total,
            timing.refresh_rate_hz,
            timing.line_rate_hz() / 1_000.0,
            timing.pixel_clock_hz as f64 / 1_000_000.0,
        )
        .into(),
    );
    ui.set_status_text("Native library initialized · test frame active".into());
    ui.set_plugin_path(default_plugin_path().to_string_lossy().into_owned().into());

    let session = Rc::new(RefCell::new(None::<CaptureSession>));

    let weak = ui.as_weak();
    let start_session = Rc::clone(&session);
    ui.on_start_requested(move || {
        if let Some(ui) = weak.upgrade() {
            let mut current = start_session.borrow_mut();
            if current
                .as_ref()
                .is_some_and(|session| !session.is_finished())
            {
                ui.set_status_text("Capture is already active".into());
                return;
            }
            if let Some(finished) = current.take() {
                let _ = finished.join();
            }

            let frequency_mhz = match ui.get_frequency_mhz().trim().parse::<f64>() {
                Ok(value) if value.is_finite() && value > 0.0 => value,
                _ => {
                    ui.set_status_text("Frequency must be a positive number in MHz".into());
                    return;
                }
            };
            let frequency_hz = (frequency_mhz * 1_000_000.0).round();
            if frequency_hz > u32::MAX as f64 {
                ui.set_status_text("Frequency is outside the native API range".into());
                return;
            }

            let gain = match ui.get_gain_percent().trim().parse::<f32>() {
                Ok(value) if (0.0..=100.0).contains(&value) => value / 100.0,
                _ => {
                    ui.set_status_text("Gain must be between 0 and 100 percent".into());
                    return;
                }
            };

            #[cfg(not(target_os = "android"))]
            let plugin_parameters = ui.get_source_arguments().to_string();
            #[cfg(target_os = "android")]
            let plugin_parameters = match android_usb::prepare_rtlsdr() {
                Ok(fd) => {
                    let parameters = format!("fd={fd}");
                    ui.set_source_arguments(parameters.clone().into());
                    parameters
                }
                Err(error) => {
                    ui.set_status_text(error.into());
                    return;
                }
            };

            let config = CaptureConfig {
                plugin_path: ui.get_plugin_path().to_string(),
                plugin_parameters,
                frequency_hz: frequency_hz as u32,
                gain,
                display_timing: DisplayTiming::DELL_2407WFP_1920X1200,
            };
            let (capture, events) = CaptureSession::start(config);
            *current = Some(capture);
            ui.set_capture_active(true);

            let weak = ui.as_weak();
            thread::spawn(move || {
                while let Ok(event) = events.recv() {
                    match event {
                        CaptureEvent::Starting => update_status(&weak, "Opening SDR source…"),
                        CaptureEvent::Running => update_status(&weak, "Capture running"),
                        CaptureEvent::Frame(frame) => {
                            let mut pixels =
                                SharedPixelBuffer::<Rgb8Pixel>::new(frame.width, frame.height);
                            pixels.make_mut_bytes().copy_from_slice(&frame.pixels);
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = weak.upgrade() {
                                    ui.set_video_frame(Image::from_rgb8(pixels));
                                }
                            });
                        }
                        CaptureEvent::Stopped(result) => {
                            #[cfg(target_os = "android")]
                            android_usb::release_rtlsdr();
                            let message = match result {
                                Ok(()) => "Capture stopped".to_owned(),
                                Err(error) => format!("Capture failed: {error}"),
                            };
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = weak.upgrade() {
                                    ui.set_capture_active(false);
                                    ui.set_status_text(message.into());
                                }
                            });
                            break;
                        }
                    }
                }
            });
        }
    });

    let weak = ui.as_weak();
    let stop_session = Rc::clone(&session);
    ui.on_stop_requested(move || {
        if let Some(ui) = weak.upgrade() {
            if let Some(session) = stop_session.borrow().as_ref() {
                ui.set_status_text("Stopping capture…".into());
                session.request_stop();
            } else {
                ui.set_status_text("Capture is not running".into());
            }
        }
    });

    ui.run()?;
    if let Some(session) = session.borrow_mut().take() {
        let _ = session.join();
    }
    #[cfg(target_os = "android")]
    android_usb::release_rtlsdr();
    drop(engine);
    Ok(())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    if let Err(error) = android_usb::initialize(&app) {
        eprintln!("Android USB initialization failed: {error}");
    }
    slint::android::init(app).expect("failed to initialize Slint's Android backend");
    if let Err(error) = run() {
        eprintln!("TempestSDR startup failed: {error}");
    }
}

fn update_status(weak: &slint::Weak<AppWindow>, message: &str) {
    let weak = weak.clone();
    let message = message.to_owned();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_status_text(message.into());
        }
    });
}

#[cfg(not(target_os = "android"))]
fn default_plugin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("TSDRPlugin_Soapy")
        .join("bin")
        .join("LINUX")
        .join("X64")
        .join("libTSDRPlugin_Soapy.so")
}

#[cfg(target_os = "android")]
fn default_plugin_path() -> PathBuf {
    PathBuf::from("libTSDRPlugin_RTLSDR.so")
}

fn make_test_frame(width: u32, height: u32) -> Image {
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let stride = width as usize;

    for (index, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
        let x = index % stride;
        let y = index / stride;
        let horizontal = (255 * x / stride.max(1)) as u8;
        let vertical = (255 * y / height.max(1) as usize) as u8;
        let grid = if x % 80 < 2 || y % 60 < 2 { 55 } else { 0 };
        let scanline = if y.is_multiple_of(2) { 18 } else { 0 };
        let value = horizontal
            .saturating_add(vertical / 4)
            .saturating_add(grid)
            .saturating_sub(scanline);
        *pixel = Rgb8Pixel::new(value, value, value);
    }

    Image::from_rgb8(pixels)
}
