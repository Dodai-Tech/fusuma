//! Board files: one TOML file describes a board, what it is made of and how its parts are wired, so that adding a
//! board with known parts is a file, not code. A firmware's build reads it to pick and configure its drivers; tools
//! read it to recognise and flash the board; Hibari OS reads the same file.
//!
//! ```toml
//! name = "s3-amoled-206"
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
pub const CONTROLLERS: [Controller; 5] = [
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

/// A board file, read and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardFile {
    /// The board's name: what `--board` takes.
    pub name: String,
    /// One line for people; the name when the file has none.
    pub about: String,
    /// Free-form capabilities (`display`, `touch`, `wifi`, ...).
    pub capabilities: Vec<String>,
    /// The screen, if any.
    pub display: Option<Display>,
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
    about: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    display: Option<RawDisplay>,
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
            about: raw.about.unwrap_or_else(|| raw.name.clone()),
            name: raw.name,
            capabilities: raw.capabilities,
            display,
            touch,
            backlight,
            firmware,
            hardware,
            render,
        })
    }
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
        ] {
            let b = read(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(b.name, path, "the file's name is its directory's");
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
