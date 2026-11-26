//! A board whose screen is e-paper: power pins, the SPI bus, the panel (found by asking the chip), three keys and touch,
//! run by `fusuma_platform_eink::EinkDriver`. The emissive boards go through `panel.rs` and `Driver` instead.

use std::time::{Duration, Instant};

use esp_idf_svc::sys::fusuma_esp as ffi;
use fusuma::platform::{Machine, Render, Settings};
use fusuma::render::Point;
use fusuma::{App, Input, Key, TouchTracker};
use fusuma_platform_eink::ssd1677::{Config as Ssd1677Config, Ssd1677};
use fusuma_platform_eink::sticky::{Sticky, Variant};
use fusuma_platform_eink::bus::Bus;
use fusuma_platform_eink::{EinkDriver, Panel, RefreshPolicy};
use fusuma_platform_embedded::{HeldTouch, orient};

use crate::board::{
    self, BUTTONS, BUTTONS_ACTIVE_LOW, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS, HEIGHT,
    POWER_HIGH, POWER_LOW, TOUCH_MIRROR, TOUCH_SWAP_XY, WIDTH,
};
use crate::{LINKED, Options, hardware, heap, touch};

/// An ESP-IDF error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EspError(pub i32);

/// The panel's SPI bus and pins, through Fusuma's C component.
pub(crate) struct EspBus;

impl EspBus {
    fn open() -> anyhow::Result<Self> {
        let cfg = ffi::fusuma_epd_config_t {
            sck: board::pin(PINS, "sck"),
            mosi: board::pin(PINS, "mosi"),
            miso: board::pin(PINS, "miso"),
            cs: board::pin(PINS, "cs"),
            dc: board::pin(PINS, "dc"),
            rst: board::pin(PINS, "rst"),
            busy: board::pin(PINS, "busy"),
            en: board::pin(PINS, "en"),
            hz: u32::try_from(board::option(OPTIONS, "data_rate_mhz") * 1_000_000).unwrap_or(10_000_000),
        };
        // SAFETY: called once at startup; `cfg` lives for the call.
        let err = unsafe { ffi::fusuma_epd_init(&cfg) };
        anyhow::ensure!(err == 0, "e-paper SPI init failed: esp_err {err}");
        Ok(Self)
    }
}

fn check(err: i32) -> Result<(), EspError> {
    if err == 0 { Ok(()) } else { Err(EspError(err)) }
}

impl Bus for EspBus {
    type Error = EspError;

    fn command(&mut self, command: u8) -> Result<(), EspError> {
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd_command(command) })
    }

    fn data(&mut self, data: &[u8]) -> Result<(), EspError> {
        // SAFETY: the slice is valid for the call, which copies it before sending.
        check(unsafe { ffi::fusuma_epd_data(data.as_ptr(), data.len()) })
    }

    fn reset(&mut self) -> Result<(), EspError> {
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd_reset() })
    }

    fn read_byte(&mut self) -> Result<Option<u8>, EspError> {
        let mut v = 0u8;
        // SAFETY: the out-pointer is valid for the call.
        check(unsafe { ffi::fusuma_epd_read_byte(&mut v) })?;
        Ok(Some(v))
    }

    fn delay_ms(&mut self, ms: u32) -> Result<(), EspError> {
        std::thread::sleep(Duration::from_millis(u64::from(ms)));
        Ok(())
    }

    fn wait_busy(&mut self) -> Result<(), EspError> {
        // A full refresh takes a few seconds; ten is a dead panel.
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd_wait_busy(10_000) })
    }
}

/// What a key does to the app.
fn key_of(name: &str) -> Option<Key> {
    Some(match name {
        "ok" => Key::Activate,
        "up" => Key::Prev,
        "down" => Key::Next,
        "left" => Key::Left,
        "right" => Key::Right,
        _ => return None,
    })
}

/// The hardware keys, with a debounce of two equal reads.
struct Keys {
    keys: Vec<(Key, i32, bool, bool)>,
}

impl Keys {
    fn open() -> Self {
        let keys = BUTTONS
            .iter()
            .filter_map(|(name, gpio)| {
                key_of(name).map(|k| {
                    // SAFETY: a plain GPIO setup at startup.
                    unsafe { ffi::fusuma_gpio_input_pullup(*gpio) };
                    (k, *gpio, false, false)
                })
            })
            .collect();
        Self { keys }
    }

    /// The keys that went down since the last call.
    fn pressed(&mut self) -> Vec<Key> {
        let mut out = Vec::new();
        for (key, gpio, last, stable) in &mut self.keys {
            // SAFETY: a plain read.
            let level = unsafe { ffi::fusuma_gpio_read(*gpio) } != 0;
            let down = level != BUTTONS_ACTIVE_LOW;
            // Two reads in a row the same: a state; a change of state is a press when it went down.
            if down == *last && down != *stable {
                *stable = down;
                if down {
                    out.push(*key);
                }
            }
            *last = down;
        }
        out
    }
}

/// Runs `make(width, height)` on an e-paper board, for ever.
pub(crate) fn run_here<A: App>(
    make: impl FnOnce(u32, u32) -> A,
    options: Options,
) -> anyhow::Result<()> {
    log::info!("board: {} ({}, {})", board::NAME, board::ABOUT, board::CHIP);
    // Rails and latches first: the board switches itself off without them, and the touch panel needs its power before
    // anything talks to it.
    for &g in POWER_HIGH {
        // SAFETY: a plain GPIO setup at startup.
        unsafe { ffi::fusuma_gpio_output(g, 1) };
    }
    for &g in POWER_LOW {
        // SAFETY: as above.
        unsafe { ffi::fusuma_gpio_output(g, 0) };
    }
    std::thread::sleep(Duration::from_millis(100));

    let bus = EspBus::open()?;
    let mirror = board::option(OPTIONS, "mirror_x") == 1;
    let panel = if (WIDTH, HEIGHT) == (800, 480) {
        Sticky::detect(bus, mirror).map_err(|e| anyhow::anyhow!("probing the panel: {e:?}"))?
    } else {
        Sticky::Ssd1677(Ssd1677::new(
            bus,
            Ssd1677Config {
                mirror_x: mirror,
                ..Ssd1677Config::new(WIDTH, HEIGHT)
            },
        ))
    };
    let variant = panel.variant();
    let glass = panel.screen();
    log::info!("panel: {variant:?}");
    // The newer panel only does full refreshes, several seconds each: settle longer and never wait for the clean-up.
    let policy = match variant {
        Variant::Ssd1677 => RefreshPolicy::DEFAULT,
        Variant::Ssd2677 => RefreshPolicy {
            min_interval_ms: 3_000,
            settle_ms: 400,
            full_every: 0,
            ..RefreshPolicy::DEFAULT
        },
    };

    let (w, h) = panel.size();
    let settings = match options.settings {
        Some(text) => Settings::from_toml(text)
            .map_err(|e| anyhow::anyhow!("fusuma.toml is not valid: {e}"))?,
        None => Settings::default(),
    };
    let render = Render::layered(&[settings.render, options.build]);
    let machine = Machine {
        screen: Some((w, h)),
        hardware: Some(hardware()),
        render,
    };
    let plan = fusuma::platform::plan(&machine, None, LINKED)
        .ok_or_else(|| anyhow::anyhow!("no renderer is linked"))?;
    log::info!("{w}x{h} e-paper, renderer: {plan}");
    let mut driver = EinkDriver::new(make(w, h), panel, plan.uses, policy)
        .ok_or_else(|| anyhow::anyhow!("cannot make a {} surface", plan.uses.name()))?;
    // What the board file says the device is (its size, form, inputs), with the refresh time of the panel found.
    driver.input(Input::Screen(board::SCREEN.with_glass(
        fusuma::DisplayKind::Reflective,
        board::SCREEN.depth_bits.max(1),
        glass.refresh_ms,
    )));
    #[cfg(feature = "fonts")]
    driver.set_fonts(crate::font_set(&settings.fonts)?);

    let have_touch = touch::init();
    let mut keys = Keys::open();
    let mut held = HeldTouch::default();
    let mut tracker = TouchTracker::new();
    let started_at = Instant::now();
    let mut report = Instant::now();
    loop {
        let now = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        for key in keys.pressed() {
            driver.input(Input::Key(key));
        }
        if have_touch {
            let read = touch::read().map(|(down, x, y)| {
                down.then(|| orient(x, y, WIDTH, HEIGHT, TOUCH_SWAP_XY, TOUCH_MIRROR))
            });
            let at: Option<Point> = held.sample(read);
            for event in tracker.step(at).into_iter().flatten() {
                driver.input(Input::Pointer(event));
            }
        }
        match driver.step(now) {
            Ok(step) => {
                if let Some(r) = step.refreshed {
                    log::info!("refresh {r:?} at {now} ms");
                }
            }
            Err(e) => log::error!("e-paper step failed: {e:?}"),
        }
        if report.elapsed() > Duration::from_secs(30) {
            report = Instant::now();
            let (free, low, psram) = heap();
            log::info!(
                "alive: internal RAM free {} KB (lowest {} KB), PSRAM free {} KB",
                free / 1024,
                low / 1024,
                psram / 1024
            );
        }
        // Keys and touch are polled, so the loop does not sleep long even when nothing is due.
        std::thread::sleep(Duration::from_millis(20));
    }
}
