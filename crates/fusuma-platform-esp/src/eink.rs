//! A board whose screen is e-paper: power pins, the SPI bus, the panel (found by asking the chip), three keys and touch,
//! run by `fusuma_platform_embedded::eink::EinkDriver`. The emissive boards go through `panel.rs` and `Driver` instead.

use std::time::{Duration, Instant};

use esp_idf_svc::sys::fusuma_esp as ffi;
use fusuma::platform::{Machine, Render, Settings};
use fusuma::render::Point;
use fusuma::{App, Input, Key, TouchTracker};
use fusuma_platform_embedded::eink::bus::Bus;
#[cfg_attr(not(fusuma_display = "epd4g"), allow(unused_imports))]
use fusuma_platform_embedded::eink::epd4g::{Config as Epd4gConfig, Epd4g};
#[cfg_attr(not(fusuma_display = "epd7in3e"), allow(unused_imports))]
use fusuma_platform_embedded::eink::epd7in3e::{Config as Epd7in3eConfig, Epd7in3e};
#[cfg_attr(not(fusuma_display = "epd13e6"), allow(unused_imports))]
use fusuma_platform_embedded::eink::epd13e6::{Chip, Config as Epd13e6Config, Epd13e6, Link};
#[cfg_attr(
    any(
        fusuma_display = "uc8179",
        fusuma_display = "ssd1680",
        fusuma_display = "epd4g",
        fusuma_display = "epd13e6",
        fusuma_display = "epd7in3e",
        fusuma_display = "st7305"
    ),
    allow(unused_imports)
)]
use fusuma_platform_embedded::eink::ssd1677::{Config as Ssd1677Config, Ssd1677};
#[cfg_attr(not(fusuma_display = "ssd1680"), allow(unused_imports))]
use fusuma_platform_embedded::eink::ssd1680::{Config as Ssd1680Config, Ssd1680};
#[cfg_attr(not(fusuma_display = "st7305"), allow(unused_imports))]
use fusuma_platform_embedded::eink::st7305::{Config as St7305Config, St7305};
#[cfg_attr(
    any(
        fusuma_display = "uc8179",
        fusuma_display = "ssd1680",
        fusuma_display = "epd4g",
        fusuma_display = "epd13e6",
        fusuma_display = "epd7in3e",
        fusuma_display = "st7305"
    ),
    allow(unused_imports)
)]
use fusuma_platform_embedded::eink::sticky::{Sticky, Variant};
#[cfg_attr(not(fusuma_display = "uc8179"), allow(unused_imports))]
use fusuma_platform_embedded::eink::uc8179::{Config as Uc8179Config, Uc8179};
use fusuma_platform_embedded::eink::{EinkDriver, Panel, RefreshPolicy};
use fusuma_platform_embedded::{HeldTouch, orient};

use crate::board::{
    self, BUTTONS, BUTTONS_ACTIVE_LOW, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS, HEIGHT,
    TOUCH_MIRROR, TOUCH_SWAP_XY, WIDTH,
};
use crate::{LINKED, Options, hardware, heap, touch};

/// An ESP-IDF error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EspError(pub i32);

/// The panel's SPI bus and pins, through Fusuma's C component.
pub(crate) struct EspBus;

#[cfg_attr(fusuma_display = "epd13e6", allow(dead_code))]
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
            hz: u32::try_from(board::option(OPTIONS, "data_rate_mhz") * 1_000_000)
                .unwrap_or(10_000_000),
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
        check(unsafe { ffi::fusuma_epd_wait_busy(10_000, 0) })
    }

    fn wait_busy_low(&mut self) -> Result<(), EspError> {
        // The UC81xx's busy line is low while it works.
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd_wait_busy(20_000, 1) })
    }
}

/// The two-controller panel's bus and pins, through Fusuma's C component.
#[cfg_attr(not(fusuma_display = "epd13e6"), allow(dead_code))]
pub(crate) struct EspLink;

#[cfg_attr(not(fusuma_display = "epd13e6"), allow(dead_code))]
impl EspLink {
    fn open() -> anyhow::Result<Self> {
        let cfg = ffi::fusuma_epd2_config_t {
            sck: board::pin(PINS, "sck"),
            mosi: board::pin(PINS, "mosi"),
            cs_master: board::pin(PINS, "cs"),
            cs_slave: board::pin(PINS, "cs2"),
            dc: board::pin(PINS, "dc"),
            rst: board::pin(PINS, "rst"),
            busy: board::pin(PINS, "busy"),
            pwr: board::pin(PINS, "en"),
            hz: u32::try_from(board::option(OPTIONS, "data_rate_mhz") * 1_000_000)
                .unwrap_or(10_000_000),
        };
        // SAFETY: called once at startup; `cfg` lives for the call.
        let err = unsafe { ffi::fusuma_epd2_init(&cfg) };
        anyhow::ensure!(
            err == 0,
            "two-controller e-paper init failed: esp_err {err}"
        );
        Ok(Self)
    }
}

impl Link for EspLink {
    type Error = EspError;

    fn power(&mut self, on: bool) -> Result<(), EspError> {
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd2_power(i32::from(on)) })
    }

    fn reset(&mut self) -> Result<(), EspError> {
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd2_reset() })
    }

    fn begin(&mut self, chips: Chip, command: u8) -> Result<(), EspError> {
        let mask = match chips {
            Chip::Master => 1,
            Chip::Slave => 2,
            Chip::Both => 3,
        };
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd2_begin(mask, command) })
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), EspError> {
        // SAFETY: the slice is valid for the call; the C side copies it in pieces.
        check(unsafe { ffi::fusuma_epd2_data(bytes.as_ptr(), bytes.len()) })
    }

    fn end(&mut self) -> Result<(), EspError> {
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd2_end() })
    }

    fn wait_idle(&mut self) -> Result<(), EspError> {
        // A refresh of this panel takes tens of seconds.
        // SAFETY: plain call after init.
        check(unsafe { ffi::fusuma_epd2_wait_idle(120_000) })
    }

    fn delay_ms(&mut self, ms: u32) -> Result<(), EspError> {
        std::thread::sleep(Duration::from_millis(u64::from(ms)));
        Ok(())
    }
}

/// The panel this firmware drives, by what the board file names.
enum Chosen {
    #[cfg_attr(
        any(
            fusuma_display = "uc8179",
            fusuma_display = "ssd1680",
            fusuma_display = "epd4g",
            fusuma_display = "epd13e6",
            fusuma_display = "epd7in3e",
            fusuma_display = "st7305"
        ),
        allow(dead_code)
    )]
    Sticky(Sticky<EspBus>),
    #[cfg_attr(not(fusuma_display = "uc8179"), allow(dead_code))]
    Uc8179(Uc8179<EspBus>),
    #[cfg_attr(not(fusuma_display = "ssd1680"), allow(dead_code))]
    Ssd1680(Ssd1680<EspBus>),
    #[cfg_attr(not(fusuma_display = "epd4g"), allow(dead_code))]
    Epd4g(Epd4g<EspBus>),
    #[cfg_attr(not(fusuma_display = "epd13e6"), allow(dead_code))]
    Epd13e6(Epd13e6<EspLink>),
    #[cfg_attr(not(fusuma_display = "epd7in3e"), allow(dead_code))]
    Epd7in3e(Epd7in3e<EspBus>),
    #[cfg_attr(not(fusuma_display = "st7305"), allow(dead_code))]
    St7305(St7305<EspBus>),
}

impl Panel for Chosen {
    type Error = EspError;

    fn size(&self) -> (u32, u32) {
        match self {
            Self::Sticky(p) => p.size(),
            Self::Uc8179(p) => p.size(),
            Self::Ssd1680(p) => p.size(),
            Self::Epd4g(p) => p.size(),
            Self::Epd13e6(p) => p.size(),
            Self::Epd7in3e(p) => p.size(),
            Self::St7305(p) => p.size(),
        }
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), EspError> {
        match self {
            Self::Sticky(p) => p.full(frame),
            Self::Uc8179(p) => p.full(frame),
            Self::Ssd1680(p) => p.full(frame),
            Self::Epd4g(p) => p.full(frame),
            Self::Epd13e6(p) => p.full(frame),
            Self::Epd7in3e(p) => p.full(frame),
            Self::St7305(p) => p.full(frame),
        }
    }

    fn partial(&mut self, region: fusuma::render::Rect, data: &[u8]) -> Result<(), EspError> {
        match self {
            Self::Sticky(p) => p.partial(region, data),
            Self::Uc8179(p) => p.partial(region, data),
            Self::Ssd1680(p) => p.partial(region, data),
            Self::Epd4g(p) => p.partial(region, data),
            Self::Epd13e6(p) => p.partial(region, data),
            Self::Epd7in3e(p) => p.partial(region, data),
            Self::St7305(p) => p.partial(region, data),
        }
    }

    fn sleep(&mut self) -> Result<(), EspError> {
        match self {
            Self::Sticky(p) => p.sleep(),
            Self::Uc8179(p) => p.sleep(),
            Self::Ssd1680(p) => p.sleep(),
            Self::Epd4g(p) => p.sleep(),
            Self::Epd13e6(p) => p.sleep(),
            Self::Epd7in3e(p) => p.sleep(),
            Self::St7305(p) => p.sleep(),
        }
    }

    fn screen(&self) -> fusuma::Screen {
        match self {
            Self::Sticky(p) => p.screen(),
            Self::Uc8179(p) => p.screen(),
            Self::Ssd1680(p) => p.screen(),
            Self::Epd4g(p) => p.screen(),
            Self::Epd13e6(p) => p.screen(),
            Self::Epd7in3e(p) => p.screen(),
            Self::St7305(p) => p.screen(),
        }
    }

    fn palette(&self) -> Option<fusuma_platform_embedded::eink::Palette> {
        match self {
            Self::Epd4g(p) => p.palette(),
            Self::Epd13e6(p) => p.palette(),
            Self::Epd7in3e(p) => p.palette(),
            _ => None,
        }
    }

    fn full_color(&mut self, packed: &[u8]) -> Result<(), EspError> {
        match self {
            Self::Epd4g(p) => p.full_color(packed),
            Self::Epd13e6(p) => p.full_color(packed),
            Self::Epd7in3e(p) => p.full_color(packed),
            _ => Ok(()),
        }
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
    crate::power::rails()?;

    #[cfg(not(fusuma_display = "epd13e6"))]
    let bus = EspBus::open()?;
    #[cfg(fusuma_display = "epd13e6")]
    let link = EspLink::open()?;
    #[cfg_attr(fusuma_display = "epd13e6", allow(unused_variables))]
    let mirror = board::option(OPTIONS, "mirror_x") == 1;
    let y_down = board::option(OPTIONS, "y_decrement") == 1;
    // Which controller the board file names decides the panel; an 800x480 SSD1677 board may be either of the Sticky's two.
    #[cfg(fusuma_display = "uc8179")]
    let (panel, policy) = {
        let _ = mirror;
        // Waveshare's 7.5 inch: about four seconds for a full refresh, 0.4 for a partial, a clean-up now and then.
        (
            Chosen::Uc8179(Uc8179::new(
                bus,
                Uc8179Config {
                    width: WIDTH,
                    height: HEIGHT,
                },
            )),
            RefreshPolicy {
                min_interval_ms: 800,
                settle_ms: 150,
                full_every: 20,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(fusuma_display = "ssd1680")]
    let (panel, policy) = {
        let _ = mirror;
        // The small panels: about two seconds for a full refresh, 0.3 for a partial, a full one now and then to clear ghosts.
        (
            Chosen::Ssd1680(Ssd1680::new(bus, Ssd1680Config::new(WIDTH, HEIGHT))),
            RefreshPolicy {
                min_interval_ms: 1_000,
                settle_ms: 150,
                full_every: 20,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(fusuma_display = "epd13e6")]
    let (panel, policy) = {
        let _ = mirror;
        // The 13.3 inch six-colour panel: tens of seconds to a refresh and no partial one: settle for a long while, never clean up.
        (
            Chosen::Epd13e6(Epd13e6::new(
                link,
                Epd13e6Config {
                    width: WIDTH,
                    height: HEIGHT,
                },
            )),
            RefreshPolicy {
                min_interval_ms: 60_000,
                settle_ms: 2_000,
                full_every: 0,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(fusuma_display = "epd7in3e")]
    let (panel, policy) = {
        let _ = mirror;
        // The 7.3 inch six-colour panel: tens of seconds to a refresh and no partial one.
        (
            Chosen::Epd7in3e(Epd7in3e::new(
                bus,
                Epd7in3eConfig {
                    width: WIDTH,
                    height: HEIGHT,
                },
            )),
            RefreshPolicy {
                min_interval_ms: 45_000,
                settle_ms: 2_000,
                full_every: 0,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(fusuma_display = "st7305")]
    let (panel, policy) = {
        let _ = mirror;
        // A reflective LCD: it refreshes at once and does not flash, so a change is shown after a short settle.
        (
            Chosen::St7305(St7305::new(
                bus,
                St7305Config {
                    width: WIDTH,
                    height: HEIGHT,
                },
            )),
            RefreshPolicy {
                min_interval_ms: 60,
                settle_ms: 20,
                full_every: 0,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(fusuma_display = "epd4g")]
    let (panel, policy) = {
        let _ = mirror;
        // The four-colour panels: a refresh is seconds long and the only kind there is, so settle for a while and never
        // wait for a clean-up.
        (
            Chosen::Epd4g(Epd4g::new(
                bus,
                Epd4gConfig {
                    width: WIDTH,
                    height: HEIGHT,
                },
            )),
            RefreshPolicy {
                min_interval_ms: 15_000,
                settle_ms: 1_000,
                full_every: 0,
                ..RefreshPolicy::DEFAULT
            },
        )
    };
    #[cfg(not(any(
        fusuma_display = "uc8179",
        fusuma_display = "ssd1680",
        fusuma_display = "epd4g",
        fusuma_display = "epd13e6",
        fusuma_display = "epd7in3e",
        fusuma_display = "st7305"
    )))]
    let (panel, policy) = {
        let s = if (WIDTH, HEIGHT) == (800, 480) {
            Sticky::detect_with(
                bus,
                Ssd1677Config {
                    mirror_x: mirror,
                    y_decrement: y_down,
                    ..Ssd1677Config::new(WIDTH, HEIGHT)
                },
            )
            .map_err(|e| anyhow::anyhow!("probing the panel: {e:?}"))?
        } else {
            Sticky::Ssd1677(Ssd1677::new(
                bus,
                Ssd1677Config {
                    mirror_x: mirror,
                    y_decrement: y_down,
                    ..Ssd1677Config::new(WIDTH, HEIGHT)
                },
            ))
        };
        // The newer panel only does full refreshes, several seconds each: settle longer and never wait for the clean-up.
        let policy = match s.variant() {
            Variant::Ssd1677 => RefreshPolicy::DEFAULT,
            Variant::Ssd2677 => RefreshPolicy {
                min_interval_ms: 3_000,
                settle_ms: 400,
                full_every: 0,
                ..RefreshPolicy::DEFAULT
            },
        };
        log::info!("panel: {:?}", s.variant());
        (Chosen::Sticky(s), policy)
    };
    let glass = panel.screen();

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
    // The integer renderer draws straight into a one-bit frame (48 KB for 800x480, where RGB565 is 768 KB and nothing is left
    // to pack); Vello, if the settings ask for it, draws RGB565 that is quantised when it is shown.
    let app = make(w, h);
    #[cfg(fusuma_color)]
    let mut driver = EinkDriver::new_color(app, panel, plan.uses, policy).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot make a {} surface for the four-colour panel",
            plan.uses.name()
        )
    })?;
    #[cfg(not(fusuma_color))]
    let mut driver = if plan.uses == fusuma::platform::Backend::Integer {
        EinkDriver::new_mono(app, panel, policy)
            .ok_or_else(|| anyhow::anyhow!("cannot make a {w}x{h} one-bit frame"))?
    } else {
        EinkDriver::new(app, panel, plan.uses, policy)
            .ok_or_else(|| anyhow::anyhow!("cannot make a {} surface", plan.uses.name()))?
    };
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
