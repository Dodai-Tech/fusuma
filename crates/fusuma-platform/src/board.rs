//! Board files: one TOML file describes a board, what it is made of and how its parts are wired, so that adding a
//! board with known parts is a file, not code. A firmware's build reads it to pick and configure its drivers; tools
//! read it to recognise and flash the board; Hibari OS reads the same file.
//!
//! ```toml
//! name = "s3-amoled-206"
//! title = "Waveshare ESP32-S3-Touch-AMOLED-2.06"   # the friendly name; optional, the name when absent
//! about = "Waveshare ESP32-S3-Touch-AMOLED-2.06 watch"
//! capabilities = ["display", "touch"]     # optional, free-form (Hibari matches apps against them)
//!
//! [display]
//! width = 410
//! height = 502
//! shape = "rounded"                       # rect (default), rounded (needs radius) or circle
//! radius = 70
//! offset = [22, 0]                        # where the visible area starts in the panel's memory; default [0, 0]
//! grid = 2                                # rectangles must start and end on multiples of this; default 1
//! controller = "co5300"                   # see CONTROLLERS; optional for a board file that only describes
//! bus = "qspi"                            # must be the controller's bus when given
//! pins = { sck = 11, d = [4, 5, 6, 7], cs = 12, rst = 8 }
//! options = { pclk_mhz = 80 }             # the controller's numbers, with defaults
//!
//! [touch]
//! controller = "ft3168"
//! pins = { sda = 15, scl = 14, rst = 9 }
//! mirror = [false, false]                 # flip x, flip y; default no
//! swap_xy = false
//!
//! [backlight]                             # optional; a panel like an AMOLED sets its own brightness
//! controller = "ledc"
//! pins = { pwm = 32 }
//! invert = true
//!
//! [firmware]
//! chip = "esp32s3"
//! flash_mb = 32
//! boot_limit_mb = 16                      # the bootloader boots no app past this; optional
//! target = "xtensa-esp32s3-none-elf"      # optional: a bare-metal firmware's Rust target (ESP-IDF derives it)
//!
//! [hardware]                              # optional: the renderer choice's facts (see `Hardware`)
//! ram_kb = 512
//! psram_kb = 8192
//! fpu = true
//!
//! [render]                                # optional: as in `fusuma.toml`
//! backend = "integer"
//! ```
//!
//! Every key is checked: an unknown key, controller, pin or option is an error naming the file and the key.
//! [`BOARD_SCHEMA`] is the JSON schema of the file.

use std::collections::BTreeMap;
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

use serde::Deserialize;

use crate::settings::RawRender;
use crate::{Hardware, RenderLayer};

/// The JSON schema of a board file, for editors and for tools to validate against.
pub const BOARD_SCHEMA: &str = include_str!("../board.schema.json");

/// The chips a board file may name: Espressif's, as `espflash board-info` spells them.
pub const CHIPS: [&str; 8] = [
    "esp32", "esp32s2", "esp32s3", "esp32c2", "esp32c3", "esp32c6", "esp32h2", "esp32p4",
];

/// What a part of a board is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    /// A display controller.
    Display,
    /// A touch controller.
    Touch,
    /// A backlight driver.
    Backlight,
}

/// A pin a controller is wired with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSpec {
    /// The key in `pins`.
    pub name: &'static str,
    /// How many GPIOs: 1 for one number, more for a list (a QSPI bus's four data lines).
    pub count: usize,
    /// Whether the board must say it.
    pub required: bool,
}

/// A number a controller can be configured with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptionSpec {
    /// The key in `options`.
    pub name: &'static str,
    /// Its value when the board does not say.
    pub default: i64,
    /// The smallest value allowed.
    pub min: i64,
    /// The largest value allowed.
    pub max: i64,
}

/// A controller some firmware has a driver for: what it needs from a board file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controller {
    /// The name a board file uses.
    pub name: &'static str,
    /// What it is.
    pub kind: PartKind,
    /// How it is connected.
    pub bus: &'static str,
    /// Its pins.
    pub pins: &'static [PinSpec],
    /// Its numbers.
    pub options: &'static [OptionSpec],
    /// The chips it works with; empty for any.
    pub chips: &'static [&'static str],
    /// The screen sizes its driver supports; empty for any.
    pub sizes: &'static [(u32, u32)],
    /// Whether it takes `display.offset` (a panel whose memory is larger than what is visible).
    pub offset: bool,
}

const fn pin(name: &'static str, count: usize, required: bool) -> PinSpec {
    PinSpec {
        name,
        count,
        required,
    }
}

const fn opt(name: &'static str, default: i64, min: i64, max: i64) -> OptionSpec {
    OptionSpec {
        name,
        default,
        min,
        max,
    }
}

/// The controllers Fusuma's ESP drivers know (`fusuma-platform-esp`). A board made of these is one file.
pub const CONTROLLERS: [Controller; 8] = [
    Controller {
        name: "co5300",
        kind: PartKind::Display,
        bus: "qspi",
        pins: &[
            pin("sck", 1, true),
            pin("d", 4, true),
            pin("cs", 1, true),
            pin("rst", 1, true),
        ],
        options: &[opt("pclk_mhz", 80, 1, 80)],
        chips: &[],
        sizes: &[],
        offset: true,
    },
    Controller {
        name: "ek79007",
        kind: PartKind::Display,
        bus: "dsi",
        pins: &[pin("rst", 1, true)],
        options: &[
            opt("lanes", 2, 1, 2),
            opt("lane_mbps", 1000, 80, 1500),
            opt("phy_ldo_channel", 3, 0, 4),
            opt("phy_ldo_mv", 2500, 500, 2700),
        ],
        // MIPI DSI exists on the P4 only, and the driver's timing is for its 1024x600 panel.
        chips: &["esp32p4"],
        sizes: &[(1024, 600)],
        offset: false,
    },
    Controller {
        name: "ssd1677",
        kind: PartKind::Display,
        bus: "spi",
        // An e-paper panel: a write-only SPI bus, a data/command line, a reset, a busy line (high while it works) and
        // optionally a pin that switches the panel's power.
        pins: &[
            pin("sck", 1, true),
            pin("mosi", 1, true),
            pin("miso", 1, false),
            pin("cs", 1, true),
            pin("dc", 1, true),
            pin("rst", 1, true),
            pin("busy", 1, true),
            pin("en", 1, false),
        ],
        options: &[opt("data_rate_mhz", 10, 1, 40), opt("mirror_x", 0, 0, 1)],
        chips: &[],
        sizes: &[],
        offset: false,
    },
    Controller {
        name: "ssd1680",
        kind: PartKind::Display,
        bus: "spi",
        // The small black and white panels (SSD1680, SSD1681): 1.54 inch 200x200, 2.13 inch 122x250, 2.9 inch 128x296. The
        // wiring of the SSD1677's; the busy line is high while it works. The driver follows Waveshare's V2 and V4 sequences
        // and has run against a recording bus only, not on a panel.
        pins: &[
            pin("sck", 1, true),
            pin("mosi", 1, true),
            pin("miso", 1, false),
            pin("cs", 1, true),
            pin("dc", 1, true),
            pin("rst", 1, true),
            pin("busy", 1, true),
            pin("en", 1, false),
        ],
        options: &[opt("data_rate_mhz", 4, 1, 20)],
        chips: &[],
        sizes: &[],
        offset: false,
    },
    Controller {
        name: "uc8179",
        kind: PartKind::Display,
        bus: "spi",
        // The same wiring as the SSD1677's; its busy line is low while it works (the driver knows).
        pins: &[
            pin("sck", 1, true),
            pin("mosi", 1, true),
            pin("miso", 1, false),
            pin("cs", 1, true),
            pin("dc", 1, true),
            pin("rst", 1, true),
            pin("busy", 1, true),
            pin("en", 1, false),
        ],
        options: &[opt("data_rate_mhz", 4, 1, 20)],
        chips: &[],
        sizes: &[],
        offset: false,
    },
    Controller {
        name: "gt911",
        kind: PartKind::Touch,
        bus: "i2c",
        pins: &[
            pin("sda", 1, true),
            pin("scl", 1, true),
            pin("rst", 1, false),
            pin("int", 1, false),
        ],
        options: &[],
        chips: &[],
        sizes: &[],
        offset: false,
    },
    Controller {
        name: "ft3168",
        kind: PartKind::Touch,
        bus: "i2c",
        pins: &[
            pin("sda", 1, true),
            pin("scl", 1, true),
            pin("rst", 1, false),
        ],
        options: &[],
        chips: &[],
        sizes: &[],
        offset: false,
    },
    Controller {
        name: "ledc",
        kind: PartKind::Backlight,
        bus: "pwm",
        pins: &[pin("pwm", 1, true)],
        options: &[opt("freq_hz", 5000, 100, 40_000)],
        chips: &[],
        sizes: &[],
        offset: false,
    },
];

/// The controller called `name`.
#[must_use]
pub fn controller(name: &str) -> Option<&'static Controller> {
    CONTROLLERS.iter().find(|c| c.name == name)
}

/// The outline of a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardShape {
    /// Square corners.
    Rect,
    /// Corners rounded with this radius in pixels.
    Rounded(u32),
    /// A round screen.
    Circle,
}

/// A part's wiring: its pins (each one or several GPIO numbers) and numbers, checked against its [`Controller`] and
/// with the defaults filled in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wiring {
    /// The controller, `None` when the file names none.
    pub controller: Option<&'static Controller>,
    /// Pin name to GPIO numbers.
    pub pins: BTreeMap<String, Vec<u8>>,
    /// Option name to value, every option of the controller present.
    pub options: BTreeMap<String, i64>,
}

impl Wiring {
    /// The first GPIO of pin `name`.
    #[must_use]
    pub fn pin(&self, name: &str) -> Option<u8> {
        self.pins.get(name).and_then(|p| p.first().copied())
    }

    /// The value of option `name` (its default when the file did not say).
    #[must_use]
    pub fn option(&self, name: &str) -> Option<i64> {
        self.options.get(name).copied()
    }
}

/// The `[display]` table, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    /// Visible width in pixels.
    pub width: u32,
    /// Visible height in pixels.
    pub height: u32,
    /// The outline.
    pub shape: BoardShape,
    /// Where the visible area starts in the panel's memory.
    pub offset: [u16; 2],
    /// Rectangles must start and end on multiples of this.
    pub grid: u32,
    /// The controller and its wiring.
    pub wiring: Wiring,
}

/// The `[touch]` table, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Touch {
    /// The controller and its wiring.
    pub wiring: Wiring,
    /// Flip x, flip y (after the swap).
    pub mirror: [bool; 2],
    /// Swap x and y.
    pub swap_xy: bool,
}

/// The `[backlight]` table, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlight {
    /// The controller and its wiring.
    pub wiring: Wiring,
    /// The light is on when the pin is low.
    pub invert: bool,
}

/// The `[firmware]` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Firmware {
    /// The chip, one of [`CHIPS`].
    pub chip: String,
    /// Flash size in megabytes.
    pub flash_mb: u32,
    /// The bootloader boots no app that ends past this many megabytes; `None` for no limit.
    pub boot_limit_mb: Option<u32>,
    /// A bare-metal firmware's Rust target; an ESP-IDF build derives its own from the chip.
    pub target: Option<String>,
}

/// The forms a `[screen]` may declare (the names `fusuma::FormFactor` reads).
pub const FORMS: [&str; 6] = ["micro", "panel", "watch", "phone", "tablet", "desktop"];

/// The inputs a `[screen]` may list.
pub const INPUTS: [&str; 4] = ["touch", "keys", "encoder", "pointer"];

/// What the screen is, as a board file says it: the facts the toolkit cannot measure. Every field has a default, so a
/// board declares only what it knows (or names a [`SCREEN_PRESETS`] preset and adds what differs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenSpec {
    /// The form factor (one of [`FORMS`]), or `None` to let the size decide.
    pub form: Option<String>,
    /// The glass's width in millimetres, 0 for unknown.
    pub width_mm: u32,
    /// The glass's height in millimetres, 0 for unknown.
    pub height_mm: u32,
    /// `true` if the glass reflects light (e-ink, memory LCD).
    pub reflective: bool,
    /// Bits per pixel the glass shows (1, 2, 4), 0 for any.
    pub depth_bits: u8,
    /// A refresh takes this many milliseconds; 0 for as fast as asked.
    pub refresh_ms: u32,
    /// How it is driven (each one of [`INPUTS`]), in the order written.
    pub inputs: Vec<String>,
}

/// A known piece of hardware's screen facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenPreset {
    /// What `preset = "..."` takes.
    pub name: &'static str,
    /// One line for people.
    pub about: &'static str,
    /// The form factor.
    pub form: &'static str,
    /// Glass width in millimetres.
    pub width_mm: u32,
    /// Glass height in millimetres.
    pub height_mm: u32,
    /// Whether the glass reflects light.
    pub reflective: bool,
    /// Bits per pixel (0 for any).
    pub depth_bits: u8,
    /// Refresh time in milliseconds (0 for fast).
    pub refresh_ms: u32,
    /// The inputs.
    pub inputs: &'static [&'static str],
}

/// Screens of hardware that is common on a desk: a board file names one and adds only what differs.
pub const SCREEN_PRESETS: [ScreenPreset; 11] = [
    ScreenPreset {
        name: "amoled-2in06-watch",
        about: "2.06 inch AMOLED watch, 410x502 (Waveshare ESP32-S3-Touch-AMOLED-2.06)",
        form: "watch",
        width_mm: 41,
        height_mm: 50,
        reflective: false,
        depth_bits: 0,
        refresh_ms: 0,
        inputs: &["touch", "keys"],
    },
    ScreenPreset {
        name: "round-1in28",
        about: "1.28 inch round touch board, 240x240 (GC9A01 and CST816)",
        form: "panel",
        width_mm: 33,
        height_mm: 33,
        reflective: false,
        depth_bits: 0,
        refresh_ms: 0,
        inputs: &["touch"],
    },
    ScreenPreset {
        name: "oled-0in96",
        about: "0.96 inch OLED, 128x64 (SSD1306), with buttons",
        form: "micro",
        width_mm: 22,
        height_mm: 11,
        reflective: false,
        depth_bits: 1,
        refresh_ms: 0,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "lcd-1in14",
        about: "1.14 inch LCD, 240x135 (ST7789), with buttons",
        form: "micro",
        width_mm: 25,
        height_mm: 14,
        reflective: false,
        depth_bits: 0,
        refresh_ms: 0,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-2in9",
        about: "2.9 inch e-paper (Waveshare, SSD1680), 128x296 as the panel is built, black and white, partial refresh",
        form: "micro",
        width_mm: 29,
        height_mm: 67,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 300,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-1in54",
        about: "1.54 inch e-paper (Waveshare, SSD1681), 200x200, black and white",
        form: "micro",
        width_mm: 27,
        height_mm: 27,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 200,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-2in13",
        about: "2.13 inch e-paper (Waveshare, SSD1680), 122x250, black and white",
        form: "micro",
        width_mm: 24,
        height_mm: 49,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 300,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-4in2",
        about: "4.2 inch e-paper (Waveshare, UC8176), 400x300, black and white",
        form: "panel",
        width_mm: 85,
        height_mm: 64,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 500,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-7in5",
        about: "7.5 inch e-paper (Waveshare, UC8179), 800x480, black and white",
        form: "tablet",
        width_mm: 163,
        height_mm: 98,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 1000,
        inputs: &["keys"],
    },
    ScreenPreset {
        name: "epaper-3in97",
        about: "3.97 inch e-paper, 800x480, black and white and four grays (Seeed reTerminal Sticky, SSD1677), touch and three keys",
        form: "panel",
        width_mm: 86,
        height_mm: 52,
        reflective: true,
        depth_bits: 1,
        refresh_ms: 600,
        inputs: &["touch", "keys"],
    },
    ScreenPreset {
        name: "lcd-7in-1024x600",
        about: "7 inch touch panel, 1024x600 (Waveshare ESP32-P4)",
        form: "tablet",
        width_mm: 154,
        height_mm: 90,
        reflective: false,
        depth_bits: 0,
        refresh_ms: 0,
        inputs: &["touch"],
    },
];

/// Pins the board needs driven before anything else works (a power latch, a regulator enable).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Power {
    /// GPIO numbers set high at boot, in order.
    pub high: Vec<u8>,
    /// GPIO numbers set low at boot (a rail enable that is active high and must start off, a shared bus's other device
    /// parked), after the high ones.
    pub low: Vec<u8>,
    /// Register writes to a chip on an I2C bus (an I/O expander, a power-management chip) that switches the board's rails,
    /// done after the pins above.
    pub i2c: Option<PowerI2c>,
}

/// A bus and the writes that switch rails on through a chip on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerI2c {
    /// The bus's data pin.
    pub sda: u8,
    /// The bus's clock pin.
    pub scl: u8,
    /// `[address, register, value]`, in order, each one write of the register then the value.
    pub writes: Vec<[u8; 3]>,
}

/// The keys a board can have, and what the toolkit does with each (`ok` presses, `up` and `down` move the focus).
pub const BUTTON_NAMES: [&str; 5] = ["ok", "up", "down", "left", "right"];

/// Hardware keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    /// Each key (one of [`BUTTON_NAMES`]) and its GPIO.
    pub keys: BTreeMap<String, u8>,
    /// Whether a pressed key pulls its pin low (with the pull-up on), which is the usual wiring.
    pub active_low: bool,
}

/// A board file, read and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardFile {
    /// The board's name: what `--board` takes.
    pub name: String,
    /// The friendly name people know the board by (`Waveshare ESP32-S3-Touch-AMOLED-1.64`); the name when the file has none.
    pub title: String,
    /// One line for people; the name when the file has none.
    pub about: String,
    /// Free-form capabilities (`display`, `touch`, `wifi`, ...).
    pub capabilities: Vec<String>,
    /// The screen, if any.
    pub display: Option<Display>,
    /// What the screen is (form, size, glass, inputs), if the file says.
    pub screen: Option<ScreenSpec>,
    /// Pins driven at boot, if any.
    pub power: Option<Power>,
    /// Hardware keys, if any.
    pub buttons: Option<Buttons>,
    /// The touch panel, if any.
    pub touch: Option<Touch>,
    /// The backlight, if the panel has one of its own.
    pub backlight: Option<Backlight>,
    /// What builds and flashes it; `None` for a device that is not flashed (a simulated one, a desktop preview).
    pub firmware: Option<Firmware>,
    /// The renderer choice's facts, if given.
    pub hardware: Option<Hardware>,
    /// A `[render]` layer, if given.
    pub render: RenderLayer,
}

/// A board file could not be read: the message names the file and the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardError(pub String);

impl core::fmt::Display for BoardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BoardError {}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawPin {
    One(i64),
    Many(Vec<i64>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    name: String,
    title: Option<String>,
    about: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    display: Option<RawDisplay>,
    screen: Option<RawScreen>,
    power: Option<RawPower>,
    buttons: Option<RawButtons>,
    touch: Option<RawTouch>,
    backlight: Option<RawBacklight>,
    firmware: Option<RawFirmware>,
    hardware: Option<RawHardware>,
    render: Option<RawRender>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDisplay {
    width: u32,
    height: u32,
    shape: Option<String>,
    radius: Option<u32>,
    offset: Option<[u16; 2]>,
    grid: Option<u32>,
    controller: Option<String>,
    bus: Option<String>,
    #[serde(default)]
    pins: BTreeMap<String, RawPin>,
    #[serde(default)]
    options: BTreeMap<String, i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPower {
    #[serde(default)]
    high: Vec<i64>,
    #[serde(default)]
    low: Vec<i64>,
    i2c: Option<RawPowerI2c>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPowerI2c {
    sda: i64,
    scl: i64,
    #[serde(default)]
    writes: Vec<Vec<i64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawButtons {
    #[serde(default)]
    pins: BTreeMap<String, i64>,
    active_low: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScreen {
    preset: Option<String>,
    form: Option<String>,
    width_mm: Option<u32>,
    height_mm: Option<u32>,
    kind: Option<String>,
    depth: Option<u8>,
    refresh_ms: Option<u32>,
    inputs: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTouch {
    controller: String,
    bus: Option<String>,
    #[serde(default)]
    pins: BTreeMap<String, RawPin>,
    #[serde(default)]
    options: BTreeMap<String, i64>,
    #[serde(default)]
    mirror: [bool; 2],
    #[serde(default)]
    swap_xy: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBacklight {
    controller: String,
    bus: Option<String>,
    #[serde(default)]
    pins: BTreeMap<String, RawPin>,
    #[serde(default)]
    options: BTreeMap<String, i64>,
    #[serde(default)]
    invert: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFirmware {
    chip: String,
    flash_mb: u32,
    boot_limit_mb: Option<u32>,
    target: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHardware {
    ram_kb: u32,
    #[serde(default)]
    psram_kb: u32,
    #[serde(default)]
    fpu: bool,
    #[serde(default)]
    simd: bool,
    #[serde(default)]
    gpu: bool,
}

impl BoardFile {
    /// Reads and checks a board file. `origin` (its path) is put in front of every error.
    ///
    /// # Errors
    /// [`BoardError`] for text that is not valid TOML, an unknown key, chip, controller, pin or option, a pin or
    /// option out of range, a controller on the wrong chip, bus or part, or a shape without its radius.
    pub fn from_toml(text: &str, origin: &str) -> Result<Self, BoardError> {
        let err = |msg: String| BoardError(format!("{origin}: {msg}"));
        let raw: RawFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
        if raw.name.is_empty()
            || !raw
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
        {
            return Err(err(format!(
                "name '{}' must be letters, digits, '-', '.' or '_'",
                raw.name
            )));
        }
        let firmware = raw
            .firmware
            .map(|f| {
                if !CHIPS.contains(&f.chip.as_str()) {
                    return Err(format!(
                        "firmware.chip '{}' is not one of {}",
                        f.chip,
                        CHIPS.join(", ")
                    ));
                }
                if f.flash_mb == 0 {
                    return Err("firmware.flash_mb must be above 0".into());
                }
                if f.boot_limit_mb.is_some_and(|b| b == 0 || b > f.flash_mb) {
                    return Err(
                        "firmware.boot_limit_mb must be above 0 and at most flash_mb".into(),
                    );
                }
                Ok(Firmware {
                    chip: f.chip,
                    flash_mb: f.flash_mb,
                    boot_limit_mb: f.boot_limit_mb,
                    target: f.target,
                })
            })
            .transpose()
            .map_err(err)?;
        // A controller that works on some chips only needs to know the chip.
        let chip = firmware.as_ref().map(|f| f.chip.clone());
        let chip = chip.as_deref();
        let display = raw
            .display
            .map(|d| read_display(d, chip).map_err(err))
            .transpose()?;
        let screen = raw
            .screen
            .map(|r| read_screen(r).map_err(|e| err(format!("[screen] {e}"))))
            .transpose()?;
        if screen.is_some() && display.is_none() {
            return Err(err("[screen] needs a [display]".into()));
        }
        let power = raw
            .power
            .map(|p| {
                let list = |name: &str, pins: &[i64]| -> Result<Vec<u8>, String> {
                    let gpios = pins
                        .iter()
                        .map(|&g| {
                            gpio(g)
                                .ok_or_else(|| format!("[power] {name}: GPIO {g} is not 0 to 255"))
                        })
                        .collect::<Result<Vec<u8>, String>>()?;
                    let mut seen = gpios.clone();
                    seen.sort_unstable();
                    seen.dedup();
                    if seen.len() != gpios.len() {
                        return Err(format!("[power] {name}: a GPIO twice"));
                    }
                    Ok(gpios)
                };
                let (high, low) = (list("high", &p.high)?, list("low", &p.low)?);
                if let Some(g) = high.iter().find(|g| low.contains(g)) {
                    return Err(format!("[power] GPIO {g} is both high and low"));
                }
                let i2c = p
                    .i2c
                    .map(|i| {
                        let pin = |n: &str, g: i64| {
                            gpio(g).ok_or_else(|| format!("[power.i2c] {n}: GPIO {g} is not 0 to 255"))
                        };
                        let (sda, scl) = (pin("sda", i.sda)?, pin("scl", i.scl)?);
                        if sda == scl {
                            return Err("[power.i2c] sda and scl are the same pin".to_string());
                        }
                        let writes = i
                            .writes
                            .iter()
                            .map(|w| {
                                let [a, r, v] = w[..] else {
                                    return Err(format!(
                                        "[power.i2c] a write is three numbers (address, register, value), not {w:?}"
                                    ));
                                };
                                if !(0..0x80).contains(&a) {
                                    return Err(format!("[power.i2c] address {a:#x} is not a 7-bit address"));
                                }
                                let byte = |n: &str, x: i64| {
                                    u8::try_from(x).map_err(|_| format!("[power.i2c] {n} {x} is not 0 to 255"))
                                };
                                Ok([byte("address", a)?, byte("register", r)?, byte("value", v)?])
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        Ok(PowerI2c { sda, scl, writes })
                    })
                    .transpose()?;
                Ok(Power { high, low, i2c })
            })
            .transpose()
            .map_err(err)?;
        let buttons = raw
            .buttons
            .map(|b| {
                let mut keys = BTreeMap::new();
                for (name, g) in b.pins {
                    if !BUTTON_NAMES.contains(&name.as_str()) {
                        return Err(format!(
                            "[buttons] pins.{name}: a key is one of {}",
                            BUTTON_NAMES.join(", ")
                        ));
                    }
                    let g = gpio(g).ok_or_else(|| {
                        format!("[buttons] pins.{name}: GPIO {g} is not 0 to 255")
                    })?;
                    keys.insert(name, g);
                }
                let mut gpios: Vec<u8> = keys.values().copied().collect();
                gpios.sort_unstable();
                gpios.dedup();
                if gpios.len() != keys.len() {
                    return Err("[buttons] two keys on one GPIO".to_string());
                }
                Ok(Buttons {
                    keys,
                    active_low: b.active_low.unwrap_or(true),
                })
            })
            .transpose()
            .map_err(err)?;
        let touch = raw
            .touch
            .map(|t| {
                let wiring = wire(
                    "touch",
                    PartKind::Touch,
                    Some(t.controller),
                    t.bus,
                    t.pins,
                    t.options,
                    chip,
                )?;
                Ok(Touch {
                    wiring,
                    mirror: t.mirror,
                    swap_xy: t.swap_xy,
                })
            })
            .transpose()
            .map_err(err)?;
        if touch.is_some() && display.is_none() {
            return Err(err("[touch] needs a [display]".into()));
        }
        let backlight = raw
            .backlight
            .map(|b| {
                let wiring = wire(
                    "backlight",
                    PartKind::Backlight,
                    Some(b.controller),
                    b.bus,
                    b.pins,
                    b.options,
                    chip,
                )?;
                Ok(Backlight {
                    wiring,
                    invert: b.invert,
                })
            })
            .transpose()
            .map_err(err)?;
        let hardware = raw.hardware.map(|h| Hardware {
            ram_kb: h.ram_kb,
            psram_kb: h.psram_kb,
            fpu: h.fpu,
            simd: h.simd,
            gpu: h.gpu,
        });
        let render = raw
            .render
            .map_or(Ok(RenderLayer::EMPTY), RawRender::into_layer)
            .map_err(|e| err(format!("render: {e}")))?;
        Ok(Self {
            title: raw.title.unwrap_or_else(|| raw.name.clone()),
            about: raw.about.unwrap_or_else(|| raw.name.clone()),
            name: raw.name,
            capabilities: raw.capabilities,
            display,
            screen,
            power,
            buttons,
            touch,
            backlight,
            firmware,
            hardware,
            render,
        })
    }
}

fn gpio(n: i64) -> Option<u8> {
    u8::try_from(n).ok()
}

fn read_screen(r: RawScreen) -> Result<ScreenSpec, String> {
    let mut spec = ScreenSpec::default();
    if let Some(name) = &r.preset {
        let preset = SCREEN_PRESETS
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| {
                format!(
                    "preset '{name}' is not one of {}",
                    SCREEN_PRESETS.map(|p| p.name).join(", ")
                )
            })?;
        spec = ScreenSpec {
            form: Some(preset.form.to_string()),
            width_mm: preset.width_mm,
            height_mm: preset.height_mm,
            reflective: preset.reflective,
            depth_bits: preset.depth_bits,
            refresh_ms: preset.refresh_ms,
            inputs: preset.inputs.iter().map(|i| (*i).to_string()).collect(),
        };
    }
    if let Some(form) = r.form {
        if !FORMS.contains(&form.as_str()) {
            return Err(format!("form '{form}' is not one of {}", FORMS.join(", ")));
        }
        spec.form = Some(form);
    }
    if let Some(w) = r.width_mm {
        spec.width_mm = w;
    }
    if let Some(h) = r.height_mm {
        spec.height_mm = h;
    }
    if (spec.width_mm == 0) != (spec.height_mm == 0) {
        return Err("width_mm and height_mm go together".into());
    }
    if spec.width_mm > 5000 || spec.height_mm > 5000 {
        return Err("width_mm and height_mm are at most 5000".into());
    }
    if let Some(kind) = r.kind {
        spec.reflective = match kind.as_str() {
            "emissive" => false,
            "reflective" | "eink" | "epaper" => true,
            _ => return Err(format!("kind '{kind}' is not emissive or reflective")),
        };
    }
    if let Some(d) = r.depth {
        if ![1, 2, 4, 8].contains(&d) {
            return Err(format!("depth {d} is not 1, 2, 4 or 8 bits"));
        }
        spec.depth_bits = d;
    }
    if let Some(ms) = r.refresh_ms {
        if ms > 60_000 {
            return Err("refresh_ms is at most 60000".into());
        }
        spec.refresh_ms = ms;
    }
    if let Some(inputs) = r.inputs {
        for i in &inputs {
            if !INPUTS.contains(&i.as_str()) {
                return Err(format!("input '{i}' is not one of {}", INPUTS.join(", ")));
            }
        }
        spec.inputs = inputs;
    }
    Ok(spec)
}

fn read_display(d: RawDisplay, chip: Option<&str>) -> Result<Display, String> {
    if d.width == 0 || d.height == 0 || d.width > 8192 || d.height > 8192 {
        return Err("display.width and display.height must be 1 to 8192".into());
    }
    let shape = match (d.shape.as_deref().unwrap_or("rect"), d.radius) {
        ("rect", None) => BoardShape::Rect,
        ("circle", None) => BoardShape::Circle,
        ("rounded", Some(r)) if r > 0 && r <= d.width.min(d.height) / 2 => BoardShape::Rounded(r),
        ("rounded", _) => {
            return Err(
                "display.shape 'rounded' needs a radius of 1 to half the shorter side".into(),
            );
        }
        ("rect" | "circle", Some(_)) => {
            return Err("display.radius is only for shape = \"rounded\"".into());
        }
        (other, _) => {
            return Err(format!(
                "display.shape '{other}' is not rect, rounded or circle"
            ));
        }
    };
    let grid = d.grid.unwrap_or(1);
    if !(1..=16).contains(&grid) {
        return Err("display.grid must be 1 to 16".into());
    }
    let wiring = wire(
        "display",
        PartKind::Display,
        d.controller,
        d.bus,
        d.pins,
        d.options,
        chip,
    )?;
    let offset = d.offset.unwrap_or([0, 0]);
    if let Some(c) = wiring.controller
        && !c.offset
        && offset != [0, 0]
    {
        return Err(format!("display.offset: the {} takes none", c.name));
    }
    if let Some(c) = wiring.controller
        && !c.sizes.is_empty()
        && !c.sizes.contains(&(d.width, d.height))
    {
        return Err(format!(
            "display: the {} driver supports {} only",
            c.name,
            c.sizes
                .iter()
                .map(|(w, h)| format!("{w}x{h}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(Display {
        width: d.width,
        height: d.height,
        shape,
        offset,
        grid,
        wiring,
    })
}

/// Checks a part's controller, bus, pins and options; fills in the options' defaults.
fn wire(
    part: &str,
    kind: PartKind,
    name: Option<String>,
    bus: Option<String>,
    pins: BTreeMap<String, RawPin>,
    options: BTreeMap<String, i64>,
    chip: Option<&str>,
) -> Result<Wiring, String> {
    let Some(name) = name else {
        // A display described without a controller: no wiring may be given either.
        if bus.is_some() || !pins.is_empty() || !options.is_empty() {
            return Err(format!("{part}: bus, pins and options need a controller"));
        }
        return Ok(Wiring::default());
    };
    let c = controller(&name)
        .filter(|c| c.kind == kind)
        .ok_or_else(|| {
            let known: Vec<&str> = CONTROLLERS
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| c.name)
                .collect();
            format!(
                "{part}.controller '{name}' is unknown; known: {}",
                known.join(", ")
            )
        })?;
    if let Some(b) = bus.filter(|b| b != c.bus) {
        return Err(format!(
            "{part}.bus '{b}': the {} is on '{}'",
            c.name, c.bus
        ));
    }
    if !c.chips.is_empty() {
        let Some(chip) = chip else {
            return Err(format!(
                "{part}: the {} works on {} only; say which in firmware.chip",
                c.name,
                c.chips.join(", ")
            ));
        };
        if !c.chips.contains(&chip) {
            return Err(format!(
                "{part}: the {} works on {} only, not on {chip}",
                c.name,
                c.chips.join(", ")
            ));
        }
    }
    let mut out = BTreeMap::new();
    for (key, value) in pins {
        let spec = c.pins.iter().find(|p| p.name == key).ok_or_else(|| {
            let known: Vec<&str> = c.pins.iter().map(|p| p.name).collect();
            format!(
                "{part}.pins.{key}: the {} has no such pin; it has {}",
                c.name,
                known.join(", ")
            )
        })?;
        let gpios = match value {
            RawPin::One(n) => std::vec![n],
            RawPin::Many(v) => v,
        };
        if gpios.len() != spec.count {
            return Err(format!(
                "{part}.pins.{key}: {} GPIO{} expected, {} given",
                spec.count,
                if spec.count == 1 { "" } else { "s" },
                gpios.len()
            ));
        }
        let gpios = gpios
            .into_iter()
            .map(|g| u8::try_from(g).ok().filter(|g| *g < 64))
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(|| format!("{part}.pins.{key}: a GPIO is 0 to 63"))?;
        out.insert(key, gpios);
    }
    if let Some(missing) = c
        .pins
        .iter()
        .find(|p| p.required && !out.contains_key(p.name))
    {
        return Err(format!(
            "{part}.pins.{}: the {} needs it",
            missing.name, c.name
        ));
    }
    let mut used: Vec<u8> = out.values().flatten().copied().collect();
    used.sort_unstable();
    if let Some(w) = used.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("{part}.pins: GPIO {} is used twice", w[0]));
    }
    let mut opts = BTreeMap::new();
    for (key, value) in &options {
        let spec =
            c.options.iter().find(|o| o.name == key).ok_or_else(|| {
                format!("{part}.options.{key}: the {} has no such option", c.name)
            })?;
        if !(spec.min..=spec.max).contains(value) {
            return Err(format!(
                "{part}.options.{key} = {value}: must be {} to {}",
                spec.min, spec.max
            ));
        }
    }
    for spec in c.options {
        opts.insert(
            spec.name.to_string(),
            options.get(spec.name).copied().unwrap_or(spec.default),
        );
    }
    Ok(Wiring {
        controller: Some(c),
        pins: out,
        options: opts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const S3: &str = r#"
name = "s3-amoled-206"
about = "Waveshare ESP32-S3-Touch-AMOLED-2.06 watch"
capabilities = ["display", "touch"]

[display]
width = 410
height = 502
shape = "rounded"
radius = 70
offset = [22, 0]
grid = 2
controller = "co5300"
bus = "qspi"
pins = { sck = 11, d = [4, 5, 6, 7], cs = 12, rst = 8 }

[touch]
controller = "ft3168"
pins = { sda = 15, scl = 14, rst = 9 }

[firmware]
chip = "esp32s3"
flash_mb = 32
boot_limit_mb = 16
target = "xtensa-esp32s3-none-elf"

[hardware]
ram_kb = 512
psram_kb = 8192
fpu = true
"#;

    fn read(text: &str) -> Result<BoardFile, BoardError> {
        BoardFile::from_toml(text, "board.toml")
    }

    /// `S3` with `old` replaced by `new`: one mistake at a time.
    fn with(old: &str, new: &str) -> Result<BoardFile, BoardError> {
        assert!(S3.contains(old), "{old}");
        read(&S3.replacen(old, new, 1))
    }

    fn fails(old: &str, new: &str, says: &str) {
        let e = with(old, new).expect_err(new).0;
        assert!(
            e.starts_with("board.toml: ") && e.contains(says),
            "{new}: {e}"
        );
    }

    #[test]
    fn the_sticky_is_an_e_paper_board_with_power_pins_and_three_keys() {
        let text = include_str!("../../fusuma-platform-esp/boards/s3-sticky/board.toml");
        let b = BoardFile::from_toml(text, "s3-sticky").unwrap();
        let d = b.display.unwrap();
        assert_eq!((d.width, d.height), (800, 480));
        let c = d.wiring.controller.unwrap();
        assert_eq!((c.name, c.bus), ("ssd1677", "spi"));
        assert_eq!(d.wiring.pins["busy"], [18]);
        assert_eq!(d.wiring.options["mirror_x"], 1);
        assert_eq!(d.wiring.options["data_rate_mhz"], 10);
        let s = b.screen.unwrap();
        assert!(s.reflective);
        assert_eq!(s.form.as_deref(), Some("panel"));
        let power = b.power.unwrap();
        assert_eq!((power.high, power.low), (vec![45, 46, 42, 8], vec![10]));
        let k = b.buttons.unwrap();
        assert_eq!(
            (k.keys["ok"], k.keys["up"], k.keys["down"], k.active_low),
            (4, 5, 6, true)
        );
    }

    #[test]
    fn a_rail_behind_an_i2c_chip_is_a_list_of_register_writes() {
        let with_tables = |t: &str| with("[touch]", &format!("{t}\n\n[touch]"));
        let b = with_tables(
            "[power.i2c]\nsda = 18\nscl = 8\nwrites = [[0x20, 3, 0xFE], [0x20, 1, 0x01]]",
        )
        .unwrap();
        let i = b.power.unwrap().i2c.unwrap();
        assert_eq!((i.sda, i.scl), (18, 8));
        assert_eq!(i.writes, [[0x20, 3, 0xFE], [0x20, 1, 0x01]]);
        for (text, says) in [
            ("[power.i2c]\nsda = 1\nscl = 1\nwrites = []", "same pin"),
            (
                "[power.i2c]\nsda = 1\nscl = 2\nwrites = [[0x20, 3]]",
                "three numbers",
            ),
            (
                "[power.i2c]\nsda = 1\nscl = 2\nwrites = [[0x80, 3, 1]]",
                "address 0x80",
            ),
            (
                "[power.i2c]\nsda = 1\nscl = 2\nwrites = [[0x20, 256, 1]]",
                "0 to 255",
            ),
        ] {
            let e = with_tables(text).expect_err(text).0;
            assert!(e.contains(says), "{text}: {e}");
        }
    }

    #[test]
    fn power_pins_and_keys_are_checked() {
        let with_tables = |t: &str| with("[touch]", &format!("{t}\n\n[touch]"));
        let b =
            with_tables("[power]\nhigh = [1, 2]\n[buttons]\npins = { ok = 3 }\nactive_low = false")
                .unwrap();
        assert_eq!(b.power.unwrap().high, [1, 2]);
        assert!(!b.buttons.unwrap().active_low);
        for (text, says) in [
            ("[power]\nhigh = [1, 1]", "a GPIO twice"),
            ("[power]\nhigh = [1]\nlow = [1]", "both high and low"),
            ("[power]\nlow = [256]", "[power] low: GPIO 256"),
            ("[power]\nhigh = [300]", "GPIO 300"),
            ("[buttons]\npins = { fire = 3 }", "pins.fire"),
            (
                "[buttons]\npins = { ok = 3, up = 3 }",
                "two keys on one GPIO",
            ),
            ("[buttons]\npins = { ok = -1 }", "GPIO -1"),
        ] {
            let e = with_tables(text).expect_err(text).0;
            assert!(
                e.starts_with("board.toml: ") && e.contains(says),
                "{text}: {e}"
            );
        }
    }

    #[test]
    fn a_screen_is_a_preset_and_what_differs_and_every_mistake_is_named() {
        let with_screen = |t: &str| with("[touch]", &format!("[screen]\n{t}\n\n[touch]"));
        assert!(read(S3).unwrap().screen.is_none(), "optional");
        let s = with_screen("preset = \"amoled-2in06-watch\"")
            .unwrap()
            .screen
            .unwrap();
        assert_eq!(
            (s.form.as_deref(), s.width_mm, s.height_mm, s.reflective),
            (Some("watch"), 41, 50, false)
        );
        assert_eq!(s.inputs, ["touch", "keys"]);
        // What is written beats the preset.
        let s = with_screen(
            "preset = \"epaper-2in9\"\nrefresh_ms = 900\ninputs = [\"touch\"]\nform = \"panel\"",
        )
        .unwrap()
        .screen
        .unwrap();
        assert_eq!((s.reflective, s.depth_bits, s.refresh_ms), (true, 1, 900));
        assert_eq!(
            (s.form.as_deref(), s.inputs.as_slice()),
            (Some("panel"), ["touch".to_string()].as_slice())
        );
        // No preset: only what is written, the rest default.
        let s = with_screen("kind = \"eink\"\ndepth = 2")
            .unwrap()
            .screen
            .unwrap();
        assert_eq!(
            (s.form, s.width_mm, s.reflective, s.depth_bits),
            (None, 0, true, 2)
        );
        for (text, says) in [
            ("preset = \"toaster\"", "preset 'toaster'"),
            ("form = \"fridge\"", "form 'fridge'"),
            ("kind = \"crt\"", "kind 'crt'"),
            ("depth = 3", "depth 3"),
            ("inputs = [\"voice\"]", "input 'voice'"),
            ("width_mm = 40", "go together"),
            ("width_mm = 9000\nheight_mm = 4", "at most 5000"),
            ("refresh_ms = 70000", "at most 60000"),
            ("colour = 1", "colour"),
        ] {
            let e = with_screen(text).expect_err(text).0;
            assert!(
                e.starts_with("board.toml: ") && e.contains(says),
                "{text}: {e}"
            );
        }
        let e = read("name = \"x\"\n[screen]\nform = \"micro\"")
            .unwrap_err()
            .0;
        assert!(e.contains("[screen] needs a [display]"), "{e}");
        // Every preset is a valid screen of its own.
        for p in SCREEN_PRESETS {
            let s = with_screen(&format!("preset = \"{}\"", p.name))
                .unwrap()
                .screen
                .unwrap();
            assert!(FORMS.contains(&s.form.as_deref().unwrap()), "{}", p.name);
            assert!(p.inputs.iter().all(|i| INPUTS.contains(i)), "{}", p.name);
            assert!(p.width_mm > 0 && p.height_mm > 0 && !p.about.is_empty());
        }
    }

    #[test]
    fn a_full_board_reads_with_its_wiring_and_defaults() {
        let b = read(S3).unwrap();
        assert_eq!(
            (b.name.as_str(), b.firmware.as_ref().unwrap().chip.as_str()),
            ("s3-amoled-206", "esp32s3")
        );
        let d = b.display.unwrap();
        assert_eq!(
            (d.width, d.height, d.shape, d.offset, d.grid),
            (410, 502, BoardShape::Rounded(70), [22, 0], 2)
        );
        assert_eq!(d.wiring.controller.unwrap().name, "co5300");
        assert_eq!(d.wiring.pins["d"], [4, 5, 6, 7]);
        assert_eq!(d.wiring.pin("sck"), Some(11));
        assert_eq!(
            d.wiring.option("pclk_mhz"),
            Some(80),
            "the default filled in"
        );
        let t = b.touch.unwrap();
        assert_eq!(
            (t.wiring.pin("rst"), t.mirror, t.swap_xy),
            (Some(9), [false, false], false)
        );
        assert_eq!(b.hardware.unwrap().psram_kb, 8192);
        assert_eq!(
            (b.firmware.as_ref().unwrap().boot_limit_mb, b.render),
            (Some(16), RenderLayer::EMPTY)
        );
        assert!(b.backlight.is_none());
    }

    #[test]
    fn a_board_without_parts_is_only_a_firmware_table() {
        let b =
            read("name = \"headless-c6\"\n[firmware]\nchip = \"esp32c6\"\nflash_mb = 4\n").unwrap();
        assert_eq!(
            (b.about.as_str(), b.display, b.touch),
            ("headless-c6", None, None)
        );
        // A simulated device: a screen and no firmware.
        let sim = read("name = \"desktop\"\n[display]\nwidth = 800\nheight = 600\n").unwrap();
        assert_eq!((sim.firmware, sim.display.unwrap().width), (None, 800));
        let e = read("name = \"x\"\n[display]\nwidth = 1024\nheight = 600\ncontroller = \"ek79007\"\npins = { rst = 1 }\n")
            .unwrap_err()
            .0;
        assert!(e.contains("say which in firmware.chip"), "{e}");
        // A display described but not wired (a host preview profile).
        let d = read("name = \"p\"\n[display]\nwidth = 320\nheight = 240\n[firmware]\nchip = \"esp32\"\nflash_mb = 4\n")
            .unwrap()
            .display
            .unwrap();
        assert_eq!(
            (d.wiring.controller, d.shape, d.grid),
            (None, BoardShape::Rect, 1)
        );
    }

    #[test]
    fn every_mistake_names_its_key() {
        fails(
            "chip = \"esp32s3\"",
            "chip = \"esp32s9\"",
            "firmware.chip 'esp32s9'",
        );
        fails(
            "controller = \"co5300\"",
            "controller = \"st7789\"",
            "known: co5300, ek79007",
        );
        fails(
            "controller = \"ft3168\"",
            "controller = \"co5300\"",
            "touch.controller 'co5300' is unknown",
        );
        fails("bus = \"qspi\"", "bus = \"spi\"", "the co5300 is on 'qspi'");
        fails(
            "cs = 12,",
            "cs = 12, dc = 3,",
            "display.pins.dc: the co5300 has no such pin",
        );
        fails(
            "d = [4, 5, 6, 7]",
            "d = [4, 5, 6]",
            "4 GPIOs expected, 3 given",
        );
        fails("sck = 11", "sck = 64", "a GPIO is 0 to 63");
        fails("sck = 11", "sck = 12", "GPIO 12 is used twice");
        fails("cs = 12, ", "", "display.pins.cs: the co5300 needs it");
        fails("radius = 70", "radius = 300", "needs a radius");
        fails(
            "shape = \"rounded\"\nradius = 70",
            "shape = \"rect\"\nradius = 70",
            "only for shape",
        );
        fails(
            "shape = \"rounded\"",
            "shape = \"oval\"",
            "display.shape 'oval'",
        );
        fails("grid = 2", "grid = 0", "display.grid");
        fails("width = 410", "width = 0", "display.width");
        fails("boot_limit_mb = 16", "boot_limit_mb = 64", "boot_limit_mb");
        fails("flash_mb = 32", "flash_mb = 0", "flash_mb must be above 0");
        fails("name = \"s3-amoled-206\"", "name = \"a b\"", "name 'a b'");
        fails("[hardware]", "[hardwar]", "unknown field `hardwar`");
        fails("grid = 2", "grid = 2\ncolour = 1", "unknown field `colour`");
        fails(
            "pins = { sda",
            "options = { speed = 4 }\npins = { sda",
            "touch.options.speed",
        );
        fails(
            "fpu = true",
            "fpu = true\n[render]\nbackend = \"skia\"",
            "render: unknown render backend 'skia'",
        );
    }

    #[test]
    fn a_controller_is_held_to_its_chip_size_and_option_ranges() {
        let p4 = r#"
name = "p4-7in"
[display]
width = 1024
height = 600
controller = "ek79007"
pins = { rst = 33 }
options = { lane_mbps = 1200 }
[touch]
controller = "gt911"
pins = { sda = 7, scl = 8 }
mirror = [true, true]
[backlight]
controller = "ledc"
pins = { pwm = 32 }
invert = true
[firmware]
chip = "esp32p4"
flash_mb = 32
"#;
        let b = read(p4).unwrap();
        let d = b.display.unwrap();
        assert_eq!(d.wiring.option("lane_mbps"), Some(1200));
        assert_eq!(d.wiring.option("lanes"), Some(2));
        assert_eq!(b.touch.unwrap().mirror, [true, true]);
        let bl = b.backlight.unwrap();
        assert_eq!(
            (bl.wiring.pin("pwm"), bl.invert, bl.wiring.option("freq_hz")),
            (Some(32), true, Some(5000))
        );
        let e = read(&p4.replace("esp32p4", "esp32s3")).unwrap_err().0;
        assert!(e.contains("the ek79007 works on esp32p4 only"), "{e}");
        let e = read(&p4.replace("width = 1024", "width = 800"))
            .unwrap_err()
            .0;
        assert!(e.contains("supports 1024x600 only"), "{e}");
        let e = read(&p4.replace("lane_mbps = 1200", "lane_mbps = 9000"))
            .unwrap_err()
            .0;
        assert!(e.contains("lane_mbps = 9000: must be 80 to 1500"), "{e}");
        let e = read(&p4.replace("width = 1024", "offset = [2, 0]\nwidth = 1024"))
            .unwrap_err()
            .0;
        assert!(e.contains("display.offset: the ek79007 takes none"), "{e}");
        let e = read(&p4.replace("[display]\n", "[displayy]\n"))
            .unwrap_err()
            .0;
        assert!(e.contains("displayy"), "{e}");
        let e = read("name = \"t\"\n[touch]\ncontroller = \"gt911\"\npins = { sda = 1, scl = 2 }\n[firmware]\nchip = \"esp32\"\nflash_mb = 4\n")
            .unwrap_err()
            .0;
        assert!(e.contains("[touch] needs a [display]"), "{e}");
    }

    /// Every board file Fusuma ships reads and checks.
    #[test]
    fn the_shipped_board_files_read() {
        for (path, text) in [
            (
                "p4-7in",
                include_str!("../../fusuma-platform-esp/boards/p4-7in/board.toml"),
            ),
            (
                "s3-amoled-206",
                include_str!("../../fusuma-platform-esp/boards/s3-amoled-206/board.toml"),
            ),
            (
                "c6-epaper-154",
                include_str!("../../fusuma-platform-esp/boards/c6-epaper-154/board.toml"),
            ),
            (
                "s3-epaper-154",
                include_str!("../../fusuma-platform-esp/boards/s3-epaper-154/board.toml"),
            ),
            (
                "s3-amoled-143",
                include_str!("../../fusuma-platform-esp/boards/s3-amoled-143/board.toml"),
            ),
            (
                "s3-amoled-164",
                include_str!("../../fusuma-platform-esp/boards/s3-amoled-164/board.toml"),
            ),
        ] {
            let b = read(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(b.name, path, "the file's name is its directory's");
            assert_ne!(b.title, b.name, "{path} has a friendly title");
            assert!(
                b.display.is_some_and(|d| d.wiring.controller.is_some()),
                "{path} is wired"
            );
        }
    }

    #[test]
    fn the_controller_table_is_consistent_and_the_schema_lists_every_controller() {
        for c in CONTROLLERS {
            assert_eq!(controller(c.name), Some(&c));
            let mut names: Vec<&str> = c
                .pins
                .iter()
                .map(|p| p.name)
                .chain(c.options.iter().map(|o| o.name))
                .collect();
            let n = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), n, "{}: a name twice", c.name);
            for o in c.options {
                assert!(
                    o.min <= o.default && o.default <= o.max,
                    "{}.{}",
                    c.name,
                    o.name
                );
            }
            assert!(
                BOARD_SCHEMA.contains(&format!("\"{}\"", c.name)),
                "the schema names {}",
                c.name
            );
        }
        for chip in CHIPS {
            assert!(
                BOARD_SCHEMA.contains(&format!("\"{chip}\"")),
                "the schema names {chip}"
            );
        }
        assert!(BOARD_SCHEMA.trim_start().starts_with('{'));
    }
}
