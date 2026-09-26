use std::{
    env,
    error::Error,
    io,
    time::{Duration, Instant},
};

use tempest_tsdr::{CaptureConfig, CaptureEvent, CaptureSession, DisplayTiming};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let plugin_path = arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: capture_smoke <plugin-path> [device-arguments]",
        )
    })?;
    let plugin_parameters = arguments.next().unwrap_or_else(|| "driver=sdrplay".into());

    let config = CaptureConfig {
        plugin_path,
        plugin_parameters,
        frequency_hz: 154_000_000,
        gain: 0.5,
        display_timing: DisplayTiming::DELL_2407WFP_1920X1200,
    };
    let (session, events) = CaptureSession::start(config);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut frames = 0;
    let mut last_size = None;
    let mut terminal_result = None;

    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(500)) {
            Ok(CaptureEvent::Starting) => println!("opening source"),
            Ok(CaptureEvent::Running) => println!("capture running"),
            Ok(CaptureEvent::Frame(frame)) => {
                frames += 1;
                last_size = Some((frame.width, frame.height));
                if frames >= 3 {
                    session.request_stop();
                }
            }
            Ok(CaptureEvent::Stopped(result)) => {
                terminal_result = Some(result);
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    session.request_stop();
    session
        .join()
        .map_err(|_| io::Error::other("capture worker panicked"))?;

    if let Some(Err(error)) = terminal_result {
        return Err(error.into());
    }
    if frames == 0 {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "capture produced no TempestSDR frames within 15 seconds",
        )
        .into());
    }

    let (width, height) = last_size.expect("a counted frame has dimensions");
    println!("received {frames} frames; last frame was {width}x{height}");
    Ok(())
}
