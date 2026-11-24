//! Fusuma on ESP-IDF boards. See the README.

mod hw;

use std::time::{Duration, Instant};

use fusuma::App;
use fusuma_platform_embedded::{Driver, DriverError};
pub use fusuma::platform::{Backend, BackendSetting, RenderLayer};
use fusuma::platform::{Hardware, Machine, Render};


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
    /// The text of the project's settings file (`fusuma.toml`), usually `Some(include_str!("../fusuma.toml"))`:
    /// the middle layer of the renderer choice. `None` leaves it to the automatic rule and the build.
    pub settings: Option<&'static str>,
    /// The build's layer, the strongest: what a cargo feature or the program itself insists on.
    pub build: RenderLayer,
    /// Backlight, 0 to 100.
    pub backlight: i32,
    /// Stack of the thread the app runs on, in kilobytes. Text layout and drawing are deep calls: the main task's
    /// 32 KB overflowed once Parley was in, so the app gets a stack of its own.
    pub stack_kb: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            settings: None,
            build: RenderLayer::EMPTY,
            backlight: 80,
            stack_kb: 128,
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

/// Free internal RAM, the lowest it has been since boot (the number `min_free_kb` should be set from), and free
/// PSRAM, in bytes.
fn heap() -> (usize, usize, usize) {
    use esp_idf_svc::sys::{
        MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_free_size, heap_caps_get_minimum_free_size,
    };
    // SAFETY: plain getters with no preconditions.
    unsafe {
        (
            heap_caps_get_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_free_size(MALLOC_CAP_SPIRAM),
        )
    }
}

/// Runs `make(width, height)` on this board, for ever.
///
/// # Errors
/// When the display cannot be brought up or no renderer can be made.
pub fn run<A: App>(
    make: impl FnOnce(u32, u32) -> A + Send + 'static,
    options: Options,
) -> anyhow::Result<()> {
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("heap at start: internal free {} KB", heap().0 / 1024);
    // The app is made and run on a thread with the stack it asked for; this one only waits for it.
    std::thread::Builder::new()
        .name("fusuma".into())
        .stack_size(options.stack_kb.max(16) * 1024)
        .spawn(move || run_here(make, options))
        .map_err(|e| anyhow::anyhow!("cannot start the app thread: {e}"))?
        .join()
        .map_err(|_| anyhow::anyhow!("the app thread panicked"))?
}

fn run_here<A: App>(
    make: impl FnOnce(u32, u32) -> A,
    options: Options,
) -> anyhow::Result<()> {

    let board = hw::P4Board::new()?;
    hw::set_backlight(options.backlight);
    let (w, h) = (hw::WIDTH as u32, hw::HEIGHT as u32);
    // The layers, weakest first: the automatic rule needs no layer, then the project's file, then the build.
    let file = match options.settings {
        Some(text) => RenderLayer::from_toml(text)
            .map_err(|e| anyhow::anyhow!("fusuma.toml is not valid: {e}"))?,
        None => RenderLayer::EMPTY,
    };
    let render = Render::layered(&[file, options.build]);
    let machine = Machine {
        screen: Some((w, h)),
        hardware: Some(hardware()),
        render,
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
    let plan = fusuma::platform::plan(&machine, None, LINKED)
        .ok_or_else(|| anyhow::anyhow!("no renderer is linked"))?;
    log::info!(
        "renderer settings: fallback {}, keep {} KB free{}",
        render.fallback,
        render.min_free_kb,
        match (options.build.backend, file.backend) {
            (Some(BackendSetting::Fixed(_)), _) => ", backend from the build",
            (_, Some(BackendSetting::Fixed(_))) => ", backend from fusuma.toml",
            _ => "",
        }
    );
    log::info!("renderer: {plan}");
    let mut driver = Driver::new(make(w, h), board, plan.uses)
        .ok_or_else(|| anyhow::anyhow!("cannot make a {} surface", plan.uses.name()))?;
    let started_at = Instant::now();
    let mut report = Instant::now();

    loop {
        let now = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let begun = Instant::now();
        match driver.step(now) {
            Ok(step) if step.shown.is_some() => {
                if driver.shell().stats.frames == 0 {
                    log::info!("heap after the first frame: internal free {} KB", heap().0 / 1024);
                }
                driver
                    .shell_mut()
                    .stats
                    .record(u64::try_from(begun.elapsed().as_millis()).unwrap_or(u64::MAX));
            }
            Ok(_) => {}
            Err(DriverError::Draw(e)) => log::error!("draw failed: {}", e.0),
            Err(DriverError::Board(never)) => match never {},
        }
        if report.elapsed() > Duration::from_secs(10) {
            report = Instant::now();
            let (free, low, psram) = heap();
            let stats = driver.shell().stats;
            log::info!(
                "alive: {} frames, {} ms each on average; internal RAM free {} KB (lowest ever {} KB), PSRAM free {} KB",
                stats.frames,
                stats.average_ms(),
                free / 1024,
                low / 1024,
                psram / 1024
            );
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}
