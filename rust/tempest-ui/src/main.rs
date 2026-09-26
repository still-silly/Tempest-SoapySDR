use slint::{ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer};
use tempest_tsdr::{DisplayTiming, TsdrEngine};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let weak = ui.as_weak();
    ui.on_start_requested(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_status_text(
                "Prototype ready; RTL-SDR streaming controller is the next milestone".into(),
            );
        }
    });

    let weak = ui.as_weak();
    ui.on_stop_requested(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_status_text("Stopped".into());
        }
    });

    ui.run()?;
    drop(engine);
    Ok(())
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
