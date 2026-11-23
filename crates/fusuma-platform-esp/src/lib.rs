//! Fusuma on ESP-IDF boards. See the README.

mod hw;
mod surface;

use std::time::{Duration, Instant};

use fusuma::{App, Pointer};
use fusuma_platform::{Backend, Hardware, Machine, Render};
use fusuma_render::Point;

use hw::{Touch, TouchTracker};
use surface::Surface;

/// The backends linked into this program: the integer one always, the others by cargo feature.
pub const LINKED: &[Backend] = &[
    Backend::Integer,
    #[cfg(feature = "skia")]
    Backend::Skia,
    #[cfg(feature = "vello-cpu")]
    Backend::VelloCpu,
];

/// What the application chooses about the platform. The default is the best experience for the hardware.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// A backend to use whatever the automatic rule would pick (the build's layer).
    pub backend: Option<Backend>,
    /// Whether an explicit choice that does not fit in memory steps down to a lighter backend.
    pub fallback: bool,
    /// Backlight, 0 to 100.
    pub backlight: i32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            backend: None,
            fallback: true,
            backlight: 80,
        }
    }
}

/// What this chip has, as ESP-IDF reports it. Internal RAM is the heap's total, which is what is really
/// there for the program, not the datasheet number.
fn hardware() -> Hardware {
    use esp_idf_svc::sys::{MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_total_size};
    // SAFETY: plain getters with no preconditions.
    let (ram, psram) = unsafe {
        (
            heap_caps_get_total_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_total_size(MALLOC_CAP_SPIRAM),
        )
    };
    Hardware {
        ram_kb: u32::try_from(ram / 1024).unwrap_or(u32::MAX),
        psram_kb: u32::try_from(psram / 1024).unwrap_or(u32::MAX),
        fpu: cfg!(target_feature = "f"),
        gpu: false,
    }
}

/// RGBA8 (premultiplied; every pixel here is opaque) to RGB565.
pub(crate) fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((u16::from(r) & 0xF8) << 8) | ((u16::from(g) & 0xFC) << 3) | (u16::from(b) >> 3)
}

/// Runs `make(width, height)` on this board, for ever.
///
/// # Errors
/// When the display cannot be brought up or no renderer can be made.
pub fn run<A: App>(make: impl FnOnce(u32, u32) -> A, options: Options) -> anyhow::Result<()> {
    esp_idf_svc::log::EspLogger::initialize_default();

    hw::init()?;
    hw::set_backlight(options.backlight);
    let (w, h) = (hw::WIDTH as u32, hw::HEIGHT as u32);
    let machine = Machine {
        screen: Some((w, h)),
        hardware: Some(hardware()),
        render: Render {
            backend: None,
            fallback: options.fallback,
        },
    };
    let facts = machine.hardware.unwrap_or(Hardware {
        ram_kb: 0,
        psram_kb: 0,
        fpu: false,
        gpu: false,
    });
    log::info!(
        "{w}x{h} screen, {} KB RAM, {} KB PSRAM, fpu {}",
        facts.ram_kb,
        facts.psram_kb,
        facts.fpu
    );
    let plan = fusuma_platform::plan(&machine, options.backend, LINKED)
        .ok_or_else(|| anyhow::anyhow!("no renderer is linked"))?;
    log::info!("renderer: {plan}");
    let mut surface = Surface::new(plan.uses, w, h)
        .ok_or_else(|| anyhow::anyhow!("cannot make a {} surface", plan.uses.name()))?;

    let fb = hw::framebuffer();
    anyhow::ensure!(!fb.is_null(), "no frame buffer");
    // SAFETY: the DSI driver owns WIDTH * HEIGHT RGB565 pixels at `fb` for the whole program, and
    // nothing else writes to it.
    let screen = unsafe { std::slice::from_raw_parts_mut(fb, hw::WIDTH * hw::HEIGHT) };

    let mut app = make(w, h);
    let mut touch = TouchTracker::default();
    let mut last = Instant::now();
    let mut report = Instant::now();
    let (mut frames, mut busy) = (0u32, Duration::ZERO);

    loop {
        let at = |x: u16, y: u16| Point::new(i32::from(x), i32::from(y));
        match touch.poll() {
            Some(Touch::Down { x, y }) => app.pointer(Pointer::Down(at(x, y))),
            Some(Touch::Move { x, y }) => app.pointer(Pointer::Move(at(x, y))),
            Some(Touch::Up { x, y }) => {
                app.pointer(Pointer::Up(at(x, y)));
                app.pointer(Pointer::Leave);
            }
            None => {}
        }
        let now = Instant::now();
        let dt = u32::try_from(now.duration_since(last).as_millis()).unwrap_or(u32::MAX);
        last = now;
        app.tick(dt);

        let started = Instant::now();
        if let Some(region) = surface.draw(&mut app) {
            surface.present(region, screen);
            hw::flush_rows(
                region.y.max(0) as u32,
                region.bottom().clamp(0, hw::HEIGHT as i32) as u32,
            );
            frames += 1;
            busy += started.elapsed();
        }
        if report.elapsed() > Duration::from_secs(10) {
            report = Instant::now();
            let average = busy.as_millis().checked_div(u128::from(frames)).unwrap_or(0);
            log::info!("alive: {frames} frames, {average} ms each on average");
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}
