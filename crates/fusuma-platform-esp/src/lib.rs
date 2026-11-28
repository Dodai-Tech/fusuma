//! Fusuma on ESP-IDF boards. See the README.

#[cfg(not(fusuma_eink))]
mod backlight;
mod board;
#[cfg(all(feature = "demo", not(fusuma_eink)))]
mod demo;
#[cfg(not(fusuma_eink))]
mod display;
#[cfg(fusuma_eink)]
mod eink;
#[cfg(not(fusuma_eink))]
mod panel;
mod power;
mod touch;

#[cfg(not(fusuma_eink))]
use std::time::{Duration, Instant};

use fusuma::App;
use fusuma::platform::Hardware;
pub use fusuma::platform::{Backend, BackendSetting, RenderLayer};
#[cfg(not(fusuma_eink))]
use fusuma::platform::{Machine, Render, Settings};
#[cfg(not(fusuma_eink))]
use fusuma_platform_embedded::Board as _;
#[cfg(not(fusuma_eink))]
use fusuma_platform_embedded::{Driver, DriverError};

/// The backends linked into this program: the integer one always, the others by cargo feature.
pub const LINKED: &[Backend] = &[
    Backend::Integer,
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
        fpu: board::fpu(),
        // Vello has no SIMD code for Xtensa or RISC-V: it runs its scalar fallback here.
        simd: fusuma_platform_embedded::vello_has_simd(),
        gpu: false,
    }
}

/// The font families `[fonts]` asks for, among those the build linked; warns when its atlas sizes are not the
/// ones the build made (the build reads them from the file `FUSUMA_SETTINGS` names).
#[cfg(feature = "fonts")]
fn font_set(choice: &fusuma::platform::FontChoice) -> anyhow::Result<fusuma::fonts::FontSet> {
    let set = fusuma::fonts::FontSet::choose(
        choice.preset.as_deref(),
        choice.sans.as_deref(),
        choice.mono.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!("fusuma.toml [fonts]: {e}"))?;
    log::info!(
        "fonts: sans {}, mono {}, atlases at {:?} px",
        set.sans.map_or("the bitmap font", |f| f.name),
        set.mono.map_or("the bitmap font", |f| f.name),
        fusuma::fonts::ATLAS_SIZES
    );
    if let Some(sizes) = &choice.sizes {
        let mut want = sizes.clone();
        want.sort_unstable();
        want.dedup();
        if want != fusuma::fonts::ATLAS_SIZES {
            log::warn!(
                "fusuma.toml asks for atlases at {want:?} px, the build made {:?}: build with FUSUMA_SETTINGS naming this file",
                fusuma::fonts::ATLAS_SIZES
            );
        }
    }
    Ok(set)
}

/// Free internal RAM, the lowest it has been since boot (the number `min_free_kb` should be set from), and free
/// PSRAM, in bytes.
fn heap() -> (usize, usize, usize) {
    use esp_idf_svc::sys::{
        MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_free_size,
        heap_caps_get_minimum_free_size,
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

#[cfg(fusuma_eink)]
fn run_here<A: App>(make: impl FnOnce(u32, u32) -> A, options: Options) -> anyhow::Result<()> {
    eink::run_here(make, options)
}

#[cfg(not(fusuma_eink))]
fn run_here<A: App>(make: impl FnOnce(u32, u32) -> A, options: Options) -> anyhow::Result<()> {
    log::info!("board: {} ({}, {})", board::NAME, board::ABOUT, board::CHIP);
    let panel = panel::EspBoard::open(options.backlight)?;
    let (w, h) = panel.size();
    // The layers, weakest first: the automatic rule needs no layer, then the project's file, then the build.
    let settings = match options.settings {
        Some(text) => Settings::from_toml(text)
            .map_err(|e| anyhow::anyhow!("fusuma.toml is not valid: {e}"))?,
        None => Settings::default(),
    };
    let file = settings.render;
    let render = Render::layered(&[file, options.build]);
    let machine = Machine {
        screen: Some((w, h)),
        hardware: Some(hardware()),
        render,
    };
    let facts = hardware();
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
    let mut driver = Driver::new(make(w, h), panel, plan.uses)
        .ok_or_else(|| anyhow::anyhow!("cannot make a {} surface", plan.uses.name()))?;
    #[cfg(feature = "fonts")]
    driver.set_fonts(font_set(&settings.fonts)?);
    let palette = fusuma::Palette::choose(
        settings.theme.palette.as_deref(),
        settings.theme.accent.as_deref(),
    )
    .map_err(|e| anyhow::anyhow!("fusuma.toml [theme]: {e}"))?;
    log::info!("palette: {}", palette.name);
    driver.input(fusuma::Input::Palette(palette));
    let started_at = Instant::now();
    let mut report = Instant::now();
    // (count, total ms, worst ms) of the updates after the first: small ones (under a tenth of the screen, a tap or a
    // hover) and big ones (page changes, scrolling).
    let (mut small, mut big) = ((0u32, 0u64, 0u64), (0u32, 0u64, 0u64));
    // Where the time of the updates went, summed (input, draw, raster, present, in us), and the slowest update
    // since the last report with its stages and region.
    let mut stages = [0u64; 4];
    let mut slowest: Option<(u32, fusuma::render::Rect, fusuma_platform_embedded::Timing)> = None;

    #[cfg(feature = "demo")]
    let mut demo = demo::Demo::new(w, h);
    loop {
        let now = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        #[cfg(feature = "demo")]
        demo.drive(&mut driver, now);
        let begun = Instant::now();
        match driver.step(now) {
            Ok(step) if step.shown.is_some() => {
                let took = u64::try_from(begun.elapsed().as_millis()).unwrap_or(u64::MAX);
                if driver.shell().stats.frames == 0 {
                    log::info!(
                        "first frame {took} ms; internal RAM free {} KB",
                        heap().0 / 1024
                    );
                } else if let Some(r) = step.shown {
                    let tenth = u64::from(r.w) * u64::from(r.h) * 10 < u64::from(w) * u64::from(h);
                    let slot = if tenth { &mut small } else { &mut big };
                    slot.0 += 1;
                    slot.1 += took;
                    slot.2 = slot.2.max(took);
                    if let Some(t) = step.timing {
                        for (sum, us) in stages.iter_mut().zip([
                            t.input_us,
                            t.draw_us,
                            t.raster_us,
                            t.present_us,
                        ]) {
                            *sum += u64::from(us);
                        }
                        let total = t.input_us + t.draw_us + t.raster_us + t.present_us;
                        if slowest.is_none_or(|(worst, ..)| total > worst) {
                            slowest = Some((total, r, t));
                        }
                    }
                }
                driver.shell_mut().stats.record(took);
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
            log::info!(
                "updates: small {} (avg {} ms, worst {} ms), big {} (avg {} ms, worst {} ms)",
                small.0,
                small.1.checked_div(u64::from(small.0)).unwrap_or(0),
                small.2,
                big.0,
                big.1.checked_div(u64::from(big.0)).unwrap_or(0),
                big.2
            );
            let n = u64::from(small.0 + big.0).max(1);
            log::info!(
                "per update on average: input {} us, draw {} us, raster {} us, present {} us",
                stages[0] / n,
                stages[1] / n,
                stages[2] / n,
                stages[3] / n
            );
            if let Some((total, r, t)) = slowest.take() {
                log::info!(
                    "slowest since the last report: {} ms for {}x{} at ({}, {}): input {} us, draw {} us, raster {} us, present {} us",
                    total / 1000,
                    r.w,
                    r.h,
                    r.x,
                    r.y,
                    t.input_us,
                    t.draw_us,
                    t.raster_us,
                    t.present_us
                );
            }
            if let Some(line) = driver.board_mut().report() {
                log::info!("{line}");
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}
